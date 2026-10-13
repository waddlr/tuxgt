use std::collections::BTreeSet;
use std::path::Path;

use sqlx::SqlitePool;

use crate::{FileManifest, Result};

use super::uninstall::{foreign_dests, install_roots};

/// One installed game in a Settings fan-out scope scan: the manifest
/// state plus the warnings the consent dialog folds in.
#[derive(Clone, Debug)]
pub struct ScopeHit {
    pub game: String,
    pub name: String,
    pub enabled: bool,
    /// Install-adapter dests whose game-dir content an uninstall would
    /// leave in place (same rule as the uninstall pre-check). A re-enable
    /// replaces them instead (backups kept), so the restore dialog words
    /// this list as replacements.
    pub foreign: Vec<String>,
    /// Other installed instance ids in this game whose recipe requires
    /// this mod (presence-based, like the install gate).
    pub required_by: Vec<String>,
    /// Game root unresolvable (game dropped from the library): install
    /// copies cannot be uninstalled.
    pub root_error: Option<String>,
}

/// Where a mod id is installed, for the Settings remove/disable dialogs.
/// `games` is the GUI's (id, display name) index; games without a manifest
/// for the mod are skipped. Sorted by display name.
pub async fn install_scope(
    pool: &SqlitePool,
    config_dir: &Path,
    data_dir: &Path,
    games: &[(String, String)],
    mod_id: &str,
) -> Result<Vec<ScopeHit>> {
    let catalog = crate::list_mods(config_dir, data_dir)?;
    let mut out = Vec::new();
    for (game, name) in games {
        // Fail closed: a manifest read error must abort the consent,
        // never misread as not-installed (that would silently skip the
        // dialog down the catalog-only path).
        let manifests = crate::game_manifests(data_dir, game)?;
        let Some(m) = manifests.iter().find(|m| m.instance == mod_id) else {
            continue;
        };
        let mut hit = ScopeHit {
            game: game.clone(),
            name: name.clone(),
            enabled: m.enabled,
            foreign: Vec::new(),
            required_by: required_by(&catalog.mods, &manifests, mod_id),
            root_error: None,
        };
        match install_roots(pool, data_dir, game, m).await {
            Ok(Some(roots)) => match foreign_dests(data_dir, game, m, &roots) {
                Ok(foreign) => hit.foreign = foreign,
                // A mid-scan read/hash failure degrades to the same
                // warning: the run reports the real error per game.
                Err(_) => hit.root_error = Some("foreign check failed".to_string()),
            },
            Ok(None) => {}
            Err(e) => hit.root_error = Some(e.to_string()),
        }
        out.push(hit);
    }
    out.sort_by(|a, b| (&a.name, &a.game).cmp(&(&b.name, &b.game)));
    Ok(out)
}

/// Library-wide install scope: the games come from the DB index, not
/// from caller-held page state (the Settings page holds no game rows,
/// so passing those would silently scan nothing). Hidden games are
/// included: hidden is a display flag, the installs are real.
pub async fn install_scope_all(
    pool: &SqlitePool,
    config_dir: &Path,
    data_dir: &Path,
    mod_id: &str,
) -> Result<Vec<ScopeHit>> {
    let games: Vec<(String, String)> = crate::list_game_index(pool)
        .await?
        .into_iter()
        .map(|g| (g.id.clone(), g.display_name().to_string()))
        .collect();
    install_scope(pool, config_dir, data_dir, &games, mod_id).await
}

/// Installed instances in `game` whose recipe requires `mod_id`.
fn required_by(catalog: &[crate::Mod], manifests: &[FileManifest], mod_id: &str) -> Vec<String> {
    let mut out: Vec<String> = manifests
        .iter()
        .filter(|m| m.instance != mod_id)
        .filter(|m| {
            catalog
                .iter()
                .find(|i| i.id == m.instance)
                .is_some_and(|i| i.requires.iter().any(|r| r == mod_id))
        })
        .map(|m| m.instance.clone())
        .collect();
    out.sort();
    out
}

/// One game's outcome in a fan-out run. Every game reports; failures
/// never silently skip.
#[derive(Debug)]
pub struct FanoutResult {
    pub game: String,
    pub name: String,
    pub result: Result<()>,
}

/// Uninstall one mod from every listed game, sequentially. A failure on
/// one game is recorded and the run continues.
pub async fn uninstall_in_games(
    pool: &SqlitePool,
    data_dir: &Path,
    games: &[(String, String)],
    mod_id: &str,
) -> Vec<FanoutResult> {
    let mut out = Vec::with_capacity(games.len());
    for (game, name) in games {
        let result =
            super::uninstall::uninstall_instance(pool, data_dir, data_dir, game, mod_id, true)
                .await;
        out.push(FanoutResult {
            game: game.clone(),
            name: name.clone(),
            result,
        });
    }
    out
}

/// Enable or disable one mod in every listed game, sequentially. The
/// single dialog consent is the `yes`; failures are recorded per game.
pub async fn set_enabled_in_games(
    pool: &SqlitePool,
    data_dir: &Path,
    games: &[(String, String)],
    mod_id: &str,
    on: bool,
    yes: bool,
) -> Vec<FanoutResult> {
    let mut out = Vec::with_capacity(games.len());
    for (game, name) in games {
        let result = super::enable::set_instance_enabled(pool, data_dir, game, mod_id, on, yes)
            .await
            .map(|_| ());
        out.push(FanoutResult {
            game: game.clone(),
            name: name.clone(),
            result,
        });
    }
    out
}

/// Why a recorded bulk pair is not restored.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SkipReason {
    Uninstalled,
    Enabled,
}

/// Restorable games plus the recorded pairs skipped as stale.
#[derive(Clone, Debug, Default)]
pub struct RestorePlan {
    pub restore: Vec<String>,
    pub skipped: Vec<(String, SkipReason)>,
}

/// Split recorded bulk pairs into restorable games and stale skips: a
/// manifest that is gone was uninstalled since; one already enabled was
/// re-enabled by hand. Recorded order (sorted) is preserved.
pub fn plan_bulk_restore(
    data_dir: &Path,
    mod_id: &str,
    recorded: &BTreeSet<String>,
) -> RestorePlan {
    let mut plan = RestorePlan::default();
    for game in recorded {
        match super::uninstall::read_manifest_opt(data_dir, game, mod_id) {
            None => plan.skipped.push((game.clone(), SkipReason::Uninstalled)),
            Some(m) if m.enabled => plan.skipped.push((game.clone(), SkipReason::Enabled)),
            Some(_) => plan.restore.push(game.clone()),
        }
    }
    plan
}
