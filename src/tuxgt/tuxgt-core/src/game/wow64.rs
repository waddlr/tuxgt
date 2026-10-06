//! 32-bit WoW64 crash warning (`gui.wow64-warning`).
//!
//! Under WoW64 a 32-bit PE runs inside a 64-bit Unix wine process: `ld.so`
//! loads only the 64-bit preload SO and the game crashes before `runtime/`.
//! This module only detects the condition; the GUI paints a warn-only row.
use sqlx::SqlitePool;

use super::GameId;
use crate::provider::heroic::heroic_live_config;
use crate::provider::steam::steam_launch_options;
use crate::{
    custom_env, enabled_global_env_pairs, find_enabled_knob, find_knob, knob_rows, PluginHost,
    Result,
};

/// Env var that switches Proton to WoW64.
pub const WOW64_VAR: &str = "PROTON_USE_WOW64";
const WOW64_KNOB: &str = "proton-wow64";

/// Which WoW64 channel is on, in warn priority order: client-owned
/// switches first, then TuxGT-owned channels, then ambient process env.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Wow64Source {
    HeroicToggle,
    HeroicEnv,
    SteamOptions,
    Knob,
    CustomEnv,
    GlobalEnv,
    ProcessEnv,
}

impl Wow64Source {
    /// Per-source warning copy (`cli.ftl`, resolved via `gui.fluent-live`).
    pub fn fluent_id(self) -> &'static str {
        match self {
            Wow64Source::HeroicToggle => "gui-wow64-heroic-toggle",
            Wow64Source::HeroicEnv => "gui-wow64-heroic-env",
            Wow64Source::SteamOptions => "gui-wow64-steam-options",
            Wow64Source::Knob => "gui-wow64-knob",
            Wow64Source::CustomEnv => "gui-wow64-custom",
            Wow64Source::GlobalEnv => "gui-wow64-global",
            Wow64Source::ProcessEnv => "gui-wow64-process",
        }
    }
}

/// One flag per WoW64 channel; gathered by `wow64_warn`, decided by
/// `should_warn_wow64`.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Wow64Sources {
    pub heroic_toggle: bool,
    pub heroic_env: bool,
    pub steam_options: bool,
    pub knob: bool,
    pub custom_env: bool,
    pub global_env: bool,
    pub process_env: bool,
}

/// Pure warn decision: only exact `"32"` warns; unknown or empty bitness
/// stays silent and 64-bit rows never warn. Channels are OR'd; the first
/// in priority order names the copy.
pub fn should_warn_wow64(bitness: Option<&str>, sources: &Wow64Sources) -> Option<Wow64Source> {
    if bitness != Some("32") {
        return None;
    }
    if sources.heroic_toggle {
        return Some(Wow64Source::HeroicToggle);
    }
    if sources.heroic_env {
        return Some(Wow64Source::HeroicEnv);
    }
    if sources.steam_options {
        return Some(Wow64Source::SteamOptions);
    }
    if sources.knob {
        return Some(Wow64Source::Knob);
    }
    if sources.custom_env {
        return Some(Wow64Source::CustomEnv);
    }
    if sources.global_env {
        return Some(Wow64Source::GlobalEnv);
    }
    if sources.process_env {
        return Some(Wow64Source::ProcessEnv);
    }
    None
}

/// An env value that switches the channel on: trimmed `1`, nothing else.
fn env_on(value: &str) -> bool {
    value.trim() == "1"
}

/// Steam LaunchOptions text carries the exact `PROTON_USE_WOW64=1` token.
pub(crate) fn options_carry_wow64(options: &str) -> bool {
    options
        .split_whitespace()
        .any(|tok| tok == "PROTON_USE_WOW64=1")
}

/// Heroic `enviromentOptions` JSON object carries `PROTON_USE_WOW64=1`.
/// Unparseable env stays silent, never warns.
pub(crate) fn heroic_env_on(env: Option<&str>) -> bool {
    let Some(raw) = env.map(str::trim).filter(|s| !s.is_empty()) else {
        return false;
    };
    let Ok(map) =
        serde_json::from_str::<std::collections::BTreeMap<String, serde_json::Value>>(raw)
    else {
        return false;
    };
    map.get(WOW64_VAR)
        .and_then(|v| v.as_str())
        .is_some_and(env_on)
}

/// Per-game proton-wow64 knob resolves to `PROTON_USE_WOW64=1`: enabled
/// row whose value maps through the knob def, mirroring the session
/// merge for that knob. `host` None skips the plugin-disabled filter.
async fn knob_on(pool: &SqlitePool, game_id: &str, host: Option<&PluginHost>) -> Result<bool> {
    let def = match host {
        Some(h) => find_enabled_knob(h, WOW64_KNOB),
        None => find_knob(WOW64_KNOB),
    };
    let Some(def) = def else {
        return Ok(false);
    };
    for row in knob_rows(pool, game_id).await? {
        if row.knob != WOW64_KNOB || !row.enabled {
            continue;
        }
        if let Some(pairs) = def.env_pairs(&row.value) {
            if pairs.iter().any(|(k, v)| k == WOW64_VAR && env_on(v)) {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

/// Warn source for one game, or `None` when silent. Bitness resolves like
/// `GameRow` (non-empty override else detected); store sources are gated
/// by manager. `host` None skips the plugin-disabled filter, same as
/// `enabled_global_env_pairs`.
pub async fn wow64_warn(
    pool: &SqlitePool,
    game_id: &str,
    host: Option<&PluginHost>,
) -> Result<Option<Wow64Source>> {
    let row: Option<(Option<String>, Option<String>)> =
        sqlx::query_as("SELECT override_bitness, detected_bitness FROM games WHERE id = ?")
            .bind(game_id)
            .fetch_optional(pool)
            .await?;
    let bitness = row.and_then(|(ob, db)| ob.filter(|s| !s.is_empty()).or(db));
    if bitness.as_deref() != Some("32") {
        return Ok(None);
    }
    let gid = GameId::parse(game_id).ok();
    let manager = gid.as_ref().map(|g| g.manager.as_str()).unwrap_or("");
    let mut sources = Wow64Sources::default();
    if manager == "heroic" {
        if let Some(app) = gid.as_ref().map(|g| g.game.as_str()) {
            if let Some(snap) = heroic_live_config(app) {
                sources.heroic_toggle = snap.enable_wow64;
                sources.heroic_env = heroic_env_on(snap.env.as_deref());
            }
        }
    }
    if manager == "steam" {
        if let Some(g) = gid.as_ref() {
            sources.steam_options =
                steam_launch_options(g).is_some_and(|o| options_carry_wow64(&o));
        }
    }
    sources.knob = knob_on(pool, game_id, host).await?;
    sources.custom_env = custom_env(pool, game_id)
        .await?
        .iter()
        .any(|(k, v)| k == WOW64_VAR && env_on(v));
    sources.global_env = enabled_global_env_pairs(pool, host)
        .await?
        .get(WOW64_VAR)
        .is_some_and(|v| env_on(v));
    sources.process_env = std::env::var(WOW64_VAR).is_ok_and(|v| env_on(&v));
    Ok(should_warn_wow64(Some("32"), &sources))
}
