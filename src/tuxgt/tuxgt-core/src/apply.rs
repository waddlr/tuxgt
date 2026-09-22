use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::game::GameId;
use crate::prewire::managed_ini;
use crate::{atomic_write, Error, PluginHost, Result, GAME_PROVIDERS};
use sqlx::SqlitePool;

/// Store-config Apply context: what the store's own Play button needs to
/// inject. The launcher wrapper plus the per-game managed-dir env (same
/// files Play and prewire use).
pub struct ApplyCtx {
    pub data_dir: PathBuf,
    pub launcher: PathBuf,
    pub ini: PathBuf,
    pub game_dir: PathBuf,
    pub depot: PathBuf,
}

impl ApplyCtx {
    pub fn env_pairs(&self) -> Vec<(String, String)> {
        vec![
            (
                "TUXGT_LAUNCHER_INI".into(),
                self.ini.to_string_lossy().into_owned(),
            ),
            (
                "TUXGT_GAME_DIR".into(),
                self.game_dir.to_string_lossy().into_owned(),
            ),
            (
                "TUXGT_DEPOT".into(),
                self.depot.to_string_lossy().into_owned(),
            ),
        ]
    }
}

pub fn apply_ctx(data_dir: &Path, game_id: &str) -> Result<ApplyCtx> {
    let gid = GameId::parse(game_id)?;
    let gdir = crate::game::game_dir(data_dir, &gid);
    Ok(ApplyCtx {
        launcher: find_launcher(data_dir)?,
        ini: managed_ini(&gdir),
        game_dir: gdir.join("runtime"),
        depot: gdir.join("stage"),
        data_dir: data_dir.into(),
    })
}

fn find_launcher(data_dir: &Path) -> Result<PathBuf> {
    if data_dir.join("bin/tuxgt-launcher").is_file() {
        return Ok(data_dir.join("bin/tuxgt-launcher"));
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let p = dir.join("tuxgt-launcher");
            if p.is_file() {
                return Ok(p);
            }
        }
    }
    if let Some(paths) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&paths) {
            let cand = dir.join("tuxgt-launcher");
            if cand.is_file() {
                return Ok(cand);
            }
        }
    }
    Err(Error::MissingLauncher)
}

/// Shell-quote one path for a Steam launch-options fragment.
pub fn quote_fragment(launcher: &Path) -> String {
    let s = launcher.to_string_lossy();
    if s.chars()
        .any(|c| c.is_whitespace() || c == '"' || c == '\'')
    {
        format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
    } else {
        s.into_owned()
    }
}

/// One store file Apply touched. `previous` is the fragment Apply replaced:
/// Steam `LaunchOptions` (None = the key was absent) or the Heroic
/// `wrapperOptions` snapshot (old records may also hold `enviromentOptions`).
/// `backup` is a first-wins whole-file copy (disaster recovery only;
/// restore is surgical per fragment so other games' entries survive).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ApplyFile {
    pub path: String,
    #[serde(default)]
    pub backup: Option<String>,
    pub previous: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ApplyRecord {
    pub game: String,
    pub manager: String,
    pub launcher: String,
    pub applied_at: u64,
    #[serde(default)]
    pub files: Vec<ApplyFile>,
}

pub fn apply_path(data_dir: &Path, game_id: &str) -> PathBuf {
    match GameId::parse(game_id) {
        Ok(gid) => crate::game::game_dir(data_dir, &gid).join("apply.toml"),
        Err(_) => data_dir
            .join("apply")
            .join(format!("{}.toml", crate::stage::game_safe(game_id))),
    }
}

pub fn read_record(data_dir: &Path, game_id: &str) -> Result<Option<ApplyRecord>> {
    let path = apply_path(data_dir, game_id);
    if !path.exists() {
        return Ok(None);
    }
    let text = fs::read_to_string(&path)?;
    let rec: ApplyRecord = toml::from_str(&text).map_err(|e| Error::Manifest(e.to_string()))?;
    Ok(Some(rec))
}

pub fn write_record(data_dir: &Path, rec: &ApplyRecord) -> Result<()> {
    let path = apply_path(data_dir, &rec.game);
    let text = toml::to_string(rec).map_err(|e| Error::Manifest(e.to_string()))?;
    atomic_write(&path, text.as_bytes())
}

pub fn drop_record(data_dir: &Path, game_id: &str) -> Result<()> {
    let path = apply_path(data_dir, game_id);
    if path.is_file() {
        fs::remove_file(&path)?;
    }
    Ok(())
}

pub(crate) fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// First-wins whole-file backup under `<data>/backups/<game-safe>/`.
/// Returns the backup path (absolute) or the already-recorded one.
pub(crate) fn ensure_backup(
    data_dir: &Path,
    game_id: &str,
    path: &Path,
    recorded: Option<&str>,
) -> Result<String> {
    if let Some(b) = recorded {
        return Ok(b.into());
    }
    let flat = path.to_string_lossy().replace(['/', '\\', ':'], "_");
    let dest = crate::install::backups_dir(data_dir, game_id).join(format!("{flat}.store"));
    if !dest.exists() {
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::copy(path, &dest)?;
    }
    Ok(dest.to_string_lossy().into_owned())
}

fn provider_for(game_id: &str) -> Result<(&'static dyn crate::provider::GameProvider, GameId)> {
    let gid = GameId::parse(game_id)?;
    let provider = GAME_PROVIDERS
        .iter()
        .find(|p| p.plugin_id() == gid.manager)
        .ok_or_else(|| Error::UnknownPlugin(gid.manager.clone()))?;
    Ok((*provider, gid))
}

/// Persist the wrapper into the store config, then the caller plays.
/// Exclusive arm (E80): handle off first, then write the trampoline, so
/// Apply and Hook are never both armed — even when the store write fails.
/// Optional and reversible; idempotent (re-apply is a no-op report).
pub async fn apply_launch(
    pool: &SqlitePool,
    data_dir: &Path,
    host: &PluginHost,
    game_id: &str,
) -> Result<String> {
    let (provider, gid) = provider_for(game_id)?;
    if gid.manager == "steam" || gid.manager == "heroic" {
        // Trampoline rows only: clear the hook channel before touching the
        // store. Unsupported rows (manual) keep prior behavior — provider
        // error, handle untouched.
        crate::session::set_handle(pool, data_dir, host, game_id, false).await?;
    }
    let ctx = apply_ctx(data_dir, game_id)?;
    let report = provider.apply(&ctx, game_id)?;
    tracing::info!(game = game_id, applied = !report.contains("already applied"), report = report.as_str(), "applied launch");
    Ok(report)
}

/// Surgically restore the pre-Apply store fragments. CLI and the GUI
/// header Apply/Restore share this.
pub fn restore_launch(data_dir: &Path, game_id: &str) -> Result<String> {
    let (provider, _) = provider_for(game_id)?;
    let ctx = apply_ctx(data_dir, game_id)?;
    provider.restore(&ctx, game_id)
}
