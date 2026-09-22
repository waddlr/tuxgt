use std::path::PathBuf;

use crate::apply::ApplyCtx;
use crate::game::GameId;
use crate::{Error, Result};

pub mod heroic;
pub mod manual;
pub mod steam;

#[derive(Clone, Debug)]
pub struct GameRecord {
    pub id: GameId,
    pub name: String,
    pub install_dir: Option<PathBuf>,
    pub cover_path: Option<PathBuf>,
    pub header_path: Option<PathBuf>,
    pub exe_path: Option<PathBuf>,
    pub prefix_path: Option<PathBuf>,
    pub proton: Option<String>,
    pub build: Option<String>,
    pub launch_options: Option<String>,
    pub env: Option<String>,
    pub wrapper: Option<String>,
    /// Detected hidden flag from the store (Steam/Heroic). Default visible.
    /// Steam: `shortcuts.vdf` `IsHidden` u32 for non-Steam shortcuts;
    /// owned apps via per-app `hidden`/`tags` in `localconfig.vdf` /
    /// `sharedconfig.vdf` (Steam writes `"hidden" "1"` or a `tags` entry
    /// containing `hidden`).
    /// Heroic: `store/config.json` `games.hidden[]` (authoritative
    /// `appName` list) plus per-game `hidden`/`isHidden`/`is_hidden` in
    /// `store_cache/*_library.json`, `sideload_apps/library.json`,
    /// `*/installed.json` when present.
    /// Manual rows have no source and stay visible. Scanners populate;
    /// core persists as `detected_hidden`, GUI overrides via `override_hidden`.
    pub hidden: bool,
}

#[derive(Clone, Debug, Default)]
pub struct StoreSnap {
    pub exe_path: Option<PathBuf>,
    pub prefix_path: Option<PathBuf>,
    pub proton: Option<String>,
    pub build: Option<String>,
    pub launch_options: Option<String>,
    pub env: Option<String>,
    pub wrapper: Option<String>,
    pub wine_type: Option<String>,
}

impl GameRecord {
    pub fn new(id: GameId, name: impl Into<String>) -> Self {
        Self {
            id,
            name: name.into(),
            install_dir: None,
            cover_path: None,
            header_path: None,
            exe_path: None,
            prefix_path: None,
            proton: None,
            build: None,
            launch_options: None,
            env: None,
            wrapper: None,
            hidden: false,
        }
    }
}

pub trait GameProvider: Send + Sync {
    fn plugin_id(&self) -> &'static str;
    fn scan(&self) -> Result<Vec<GameRecord>>;
    /// Persist the wrapper so the store's own Play button injects.
    /// Atomic write + backup; idempotent; E18 bodies on Steam/Heroic.
    /// Returns a one-line report. Manual never gains a body.
    fn apply(&self, ctx: &ApplyCtx, game_id: &str) -> Result<String> {
        let _ = (ctx, game_id);
        Err(Error::ApplyUnsupported(self.plugin_id().into()))
    }
    /// Surgically restore the pre-Apply fragments (Settings page + CLI).
    /// Returns a one-line report. Manual never gains a body.
    fn restore(&self, ctx: &ApplyCtx, game_id: &str) -> Result<String> {
        let _ = (ctx, game_id);
        Err(Error::ApplyUnsupported(self.plugin_id().into()))
    }
}

pub static GAME_PROVIDERS: &[&dyn GameProvider] = &[
    &steam::SteamProvider,
    &heroic::HeroicProvider,
    &manual::ManualProvider,
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manual_apply_is_stub() {
        let ctx = ApplyCtx {
            data_dir: PathBuf::from("/tmp"),
            launcher: PathBuf::from("/tmp/tuxgt-launcher"),
            ini: PathBuf::from("/tmp/tuxgt-launcher.ini"),
            game_dir: PathBuf::from("/tmp/runtime"),
            depot: PathBuf::from("/tmp/stage"),
        };
        let manual = GAME_PROVIDERS
            .iter()
            .find(|p| p.plugin_id() == "manual")
            .unwrap();
        let err = manual.apply(&ctx, "manual:standalone:1").unwrap_err();
        assert!(matches!(err, Error::ApplyUnsupported(_)));
        let err = manual.restore(&ctx, "manual:standalone:1").unwrap_err();
        assert!(matches!(err, Error::ApplyUnsupported(_)));
    }

    #[test]
    fn apply_rejects_foreign_manager() {
        let ctx = ApplyCtx {
            data_dir: PathBuf::from("/tmp"),
            launcher: PathBuf::from("/tmp/tuxgt-launcher"),
            ini: PathBuf::from("/tmp/tuxgt-launcher.ini"),
            game_dir: PathBuf::from("/tmp/runtime"),
            depot: PathBuf::from("/tmp/stage"),
        };
        let steam = GAME_PROVIDERS
            .iter()
            .find(|p| p.plugin_id() == "steam")
            .unwrap();
        let err = steam.apply(&ctx, "heroic:gog:1").unwrap_err();
        assert!(matches!(err, Error::ApplyUnsupported(_)));
        let heroic = GAME_PROVIDERS
            .iter()
            .find(|p| p.plugin_id() == "heroic")
            .unwrap();
        let err = heroic.restore(&ctx, "steam::1").unwrap_err();
        assert!(matches!(err, Error::ApplyUnsupported(_)));
    }

    #[test]
    fn plugin_ids() {
        let ids: Vec<&str> = GAME_PROVIDERS.iter().map(|p| p.plugin_id()).collect();
        assert_eq!(ids, ["steam", "heroic", "manual"]);
    }
}
