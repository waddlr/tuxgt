//! Launch Mode memory for profile switches. A switch whose incoming
//! profile needs no channel disarms through `sync_session`; the arm it
//! captured is remembered here so a later switch back to a profile with
//! needs can re-arm it. The file lives beside `apply.toml` on the game
//! and is never parked. See `docs/dev/app/plugin/profile.md`.

use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

use crate::{Error, PluginHost, Result};

use super::game_root_dir;

pub(crate) const ARM_HOOK: &str = "hook";
pub(crate) const ARM_APPLY: &str = "apply";

/// The arm live right now, Hook first — the radio's precedence.
pub(crate) async fn current_arm(
    pool: &SqlitePool,
    data_dir: &Path,
    game_id: &str,
) -> Result<Option<String>> {
    if crate::game_handle(pool, game_id).await? {
        return Ok(Some(ARM_HOOK.into()));
    }
    if crate::read_record(data_dir, game_id)?.is_some() {
        return Ok(Some(ARM_APPLY.into()));
    }
    Ok(None)
}

#[derive(Serialize, Deserialize)]
struct MemoryFile {
    mode: String,
}

fn memory_path(gdir: &Path) -> std::path::PathBuf {
    gdir.join("launch-memory.toml")
}

fn parse_memory(gdir: &Path) -> Option<String> {
    let path = memory_path(gdir);
    if !path.is_file() {
        return None;
    }
    fs::read_to_string(&path)
        .ok()
        .and_then(|text| toml::from_str::<MemoryFile>(&text).ok())
        .map(|file| file.mode)
        .filter(|mode| matches!(mode.as_str(), ARM_HOOK | ARM_APPLY))
}

/// Side-effect-free memory read for switch previews, which must not
/// mutate the game dir before the confirms resolve.
pub(crate) fn peek_memory(gdir: &Path) -> Option<String> {
    parse_memory(gdir)
}

/// A preference file must never be load-bearing: anything unreadable
/// or unrecognized reads as no memory and is cleared, so a bad file
/// cannot wedge every later profile open on the switch resume path.
fn read_memory(gdir: &Path) -> Option<String> {
    let parsed = parse_memory(gdir);
    if parsed.is_none() && memory_path(gdir).is_file() {
        tracing::warn!(path = %memory_path(gdir).display(), "dropping unreadable launch memory");
        clear_memory(gdir);
    }
    parsed
}

fn write_memory(gdir: &Path, mode: &str) -> Result<()> {
    let text = toml::to_string(&MemoryFile { mode: mode.into() })
        .map_err(|e| Error::Toml(e.to_string()))?;
    crate::atomic_write(&memory_path(gdir), text.as_bytes())
}

fn clear_memory(gdir: &Path) {
    let _ = fs::remove_file(memory_path(gdir));
}

/// Post-sync memory bookkeeping for one finished switch. Returns true
/// when a remembered Apply arm must wait for the caller's client-stop
/// flow. Best-effort like the auto-restore it mirrors: the incoming
/// profile is already live here, so no failure in this bookkeeping may
/// fail the switch — that would strand its journal for every resume.
pub(crate) async fn settle_launch_memory(
    pool: &SqlitePool,
    data_dir: &Path,
    host: &PluginHost,
    game_id: &str,
    gdir: &Path,
    captured: Option<&str>,
) -> bool {
    let needs = match crate::launch::game_launch_needs(pool, data_dir, game_id).await {
        Ok(needs) => needs,
        Err(e) => {
            tracing::warn!(game = game_id, error = %e, "launch needs read failed");
            return false;
        }
    };
    if !needs.channel_needed() {
        if let Some(arm) = captured {
            if let Err(e) = write_memory(gdir, arm) {
                tracing::warn!(game = game_id, error = %e, "launch memory write failed");
            }
        }
        return false;
    }
    match current_arm(pool, data_dir, game_id).await {
        Ok(Some(_)) => {
            clear_memory(gdir);
            return false;
        }
        Ok(None) => {}
        Err(e) => {
            tracing::warn!(game = game_id, error = %e, "launch arm read failed");
            return false;
        }
    }
    let mode = read_memory(gdir);
    if mode.as_deref() == Some(ARM_APPLY)
        && crate::client::StoreClient::for_game(game_id).is_some_and(|c| c.running())
    {
        return true;
    }
    match rearm_remembered_launch(pool, data_dir, host, game_id).await {
        Ok(_) => false,
        Err(e) => {
            tracing::warn!(game = game_id, error = %e, "remembered launch re-arm failed");
            // An Apply failure may just need the store stopped first;
            // the caller's client-stop flow retries it exactly once.
            mode.as_deref() == Some(ARM_APPLY)
        }
    }
}

/// Re-arm the Launch Mode a disarming profile switch remembered. The
/// memory clears only on a successful re-arm (or when an arm is already
/// live and the memory is stale); an arm the current needs make illegal
/// stays remembered. Callers run any needed store-client stop first —
/// this performs the Apply store write as-is.
pub async fn rearm_remembered_launch(
    pool: &SqlitePool,
    data_dir: &Path,
    host: &PluginHost,
    game_id: &str,
) -> Result<Option<String>> {
    let gdir = game_root_dir(data_dir, game_id)?;
    let Some(mode) = read_memory(&gdir) else {
        return Ok(None);
    };
    if current_arm(pool, data_dir, game_id).await?.is_some() {
        clear_memory(&gdir);
        return Ok(None);
    }
    let needs = crate::launch::game_launch_needs(pool, data_dir, game_id).await?;
    match mode.as_str() {
        ARM_HOOK if needs.hook_legal(true) => {
            crate::set_handle(pool, data_dir, host, game_id, true).await?;
            clear_memory(&gdir);
            // The return is a user-facing report line; Hook has none.
            Ok(Some(String::new()))
        }
        ARM_APPLY if needs.apply_legal() => {
            let report = crate::apply_launch(pool, data_dir, host, game_id).await?;
            clear_memory(&gdir);
            Ok(Some(report))
        }
        _ => Ok(None),
    }
}
