use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use sqlx::SqlitePool;

use super::*;
use crate::game::GameId;
use crate::wrapper::game_wrappers;
use crate::{custom_env, game_manifests, knob_rows, FileManifest, Result};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LaunchSpec {
    pub id: GameId,
    pub cwd: PathBuf,
    pub env: BTreeMap<String, String>,
    pub wrappers: Box<[Vec<String>]>,
    pub program: PathBuf,
    pub args: Box<[String]>,
    /// False when a store client owns the process (Steam/Heroic dispatch):
    /// no wrappers, no env merge, no spawn-and-wait harvest. Only manual
    /// rows are owned.
    pub owned: bool,
}

impl LaunchSpec {
    pub fn argv(&self) -> Vec<String> {
        let mut inner = Vec::with_capacity(1 + self.args.len());
        inner.push(self.program.to_string_lossy().into_owned());
        inner.extend(self.args.iter().cloned());
        for w in self.wrappers.iter().rev() {
            let mut v = w.clone();
            v.extend(inner);
            inner = v;
        }
        inner
    }

    pub fn display(&self) -> String {
        self.argv()
            .iter()
            .map(|a| quote_arg(a))
            .collect::<Vec<_>>()
            .join(" ")
    }

    pub fn command(&self) -> Command {
        let argv = self.argv();
        let mut cmd = Command::new(&argv[0]);
        if argv.len() > 1 {
            cmd.args(&argv[1..]);
        }
        cmd.current_dir(&self.cwd);
        cmd.envs(&self.env);
        cmd
    }

    /// Like `command`, in a new session: the client/game outlives tuxgt
    /// and shares no process group with it. See `detach_command`.
    pub fn command_detached(&self) -> Command {
        let mut cmd = self.command();
        detach_command(&mut cmd);
        cmd
    }
}

/// gamescope / gamemoderun / mangohud-as-argv. The protonfixes hook cannot
/// wrap these; they need the Apply trampoline (`WRAPPERS=`).
pub fn is_argv_wrapper(id: &str) -> bool {
    matches!(id, "gamescope" | "gamemode" | "mangohud")
}

/// Store-row Launch Mode legality from enabled instances + env + wrappers.
/// Manual rows stay owned Play, out of the radio.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct LaunchNeeds {
    pub preload: bool,
    pub env: bool,
    pub argv_wrappers: bool,
    pub install_only: bool,
}

impl LaunchNeeds {
    pub fn from_state(preload: bool, install: bool, env: bool, argv_wrappers: bool) -> Self {
        Self {
            preload,
            env,
            argv_wrappers,
            install_only: install && !preload && !env && !argv_wrappers,
        }
    }

    pub fn from_manifests(
        manifests: &[FileManifest],
        has_game_env: bool,
        wrappers: &[String],
    ) -> Self {
        let enabled: Vec<&FileManifest> = manifests.iter().filter(|m| m.enabled).collect();
        let preload = enabled.iter().any(|m| m.adapter == "preload");
        let install = enabled.iter().any(|m| m.adapter == "install");
        let mod_env = enabled.iter().any(|m| m.env.iter().any(|e| e.enabled));
        let argv_wrappers = wrappers.iter().any(|w| is_argv_wrapper(w));
        Self::from_state(preload, install, has_game_env || mod_env, argv_wrappers)
    }

    /// Preload, env, or argv wrappers — Vanilla store Play will not inject.
    pub fn channel_needed(&self) -> bool {
        self.preload || self.env || self.argv_wrappers
    }

    pub fn show_radio(&self) -> bool {
        self.channel_needed()
    }

    pub fn hook_legal(&self, ge_cachy: bool) -> bool {
        ge_cachy && (self.preload || self.env) && !self.argv_wrappers
    }

    pub fn apply_legal(&self) -> bool {
        self.channel_needed()
    }

    /// Enable & Play prefers Hook when GE/Cachy and no argv wrappers.
    pub fn hook_preferred(&self, ge_cachy: bool) -> bool {
        ge_cachy && !self.argv_wrappers
    }
}

/// Needs from persisted state: enabled manifests, enabled+set per-game knobs,
/// custom env, wrappers. Same inputs the GUI radio computes from its loaded
/// maps. Used by `sync_session`'s auto-restore; Not hooked is always legal, so
/// no arm is gated on this.
pub async fn game_launch_needs(
    pool: &SqlitePool,
    data_dir: &Path,
    game_id: &str,
) -> Result<LaunchNeeds> {
    let manifests = game_manifests(data_dir, game_id)?;
    let has_game_env = knob_rows(pool, game_id)
        .await?
        .iter()
        .any(|r| r.enabled && !r.value.is_empty())
        || !custom_env(pool, game_id).await?.is_empty();
    let wrappers = game_wrappers(pool, game_id).await?;
    Ok(LaunchNeeds::from_manifests(
        &manifests,
        has_game_env,
        &wrappers,
    ))
}

#[derive(Clone, Debug)]
pub struct LaunchPaths {
    pub launcher: Option<PathBuf>,
    pub steam: Option<PathBuf>,
    pub heroic: Option<PathBuf>,
    pub umu: Option<PathBuf>,
    pub wine: Option<PathBuf>,
}

impl LaunchPaths {
    pub fn detect() -> Result<Self> {
        Ok(Self {
            launcher: find_tuxgt_launcher().ok(),
            steam: find_steam(),
            heroic: find_heroic(),
            umu: find_in_path("umu-run"),
            wine: find_in_path("wine"),
        })
    }
}

#[derive(sqlx::FromRow)]
pub(crate) struct Row {
    pub(crate) id: String,
    pub(crate) manager: String,
    pub(crate) store: String,
    pub(crate) game_id: String,
    pub(crate) install_dir: Option<String>,
    pub(crate) exe_path: Option<String>,
    pub(crate) prefix_path: Option<String>,
    pub(crate) proton: Option<String>,
    pub(crate) launch_options: Option<String>,
    pub(crate) env: Option<String>,
    pub(crate) wrapper: Option<String>,
    pub(crate) detected_platform: Option<String>,
    pub(crate) detected_prefix_path: Option<String>,
    pub(crate) detected_proton: Option<String>,
    pub(crate) override_exe_path: Option<String>,
    pub(crate) override_platform: Option<String>,
    pub(crate) override_prefix_path: Option<String>,
    pub(crate) override_proton: Option<String>,
}

/// Effective platform for a games row, shared by `build_launch_spec` and the
/// session proton grant: override → detected platform, else `proton` when a
/// prefix or a proton is present, else `native`.
pub(crate) fn resolve_platform(row: &Row) -> String {
    let plat = pick(&row.override_platform, &row.detected_platform, &None);
    if !plat.is_empty() {
        return plat.to_string();
    }
    let prefix = pick(
        &row.override_prefix_path,
        &row.detected_prefix_path,
        &row.prefix_path,
    );
    let proton = pick(&row.override_proton, &row.detected_proton, &row.proton);
    if !prefix.is_empty() || !proton.is_empty() {
        "proton".to_string()
    } else {
        "native".to_string()
    }
}
