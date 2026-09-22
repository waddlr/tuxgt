use std::collections::{BTreeMap, BTreeSet};
use std::future::Future;
use std::path::Path;

use sqlx::SqlitePool;

use super::*;
use crate::game::{game_dir, game_exists, GameId};
use crate::prewire::managed_ini;
use crate::wrapper::game_wrappers;
use crate::{custom_env, find_enabled_knob, knob_rows, knob_values, Error, PluginHost, Result};

/// Proton built-in OptiScaler grant for the session channel (R59): the same
/// conditions as the owned `LaunchSpec` env plan (`launch.rs`: platform
/// resolves to `proton`, a CachyOS/GE flavor, plus an enabled manifest whose
/// recipe allows `proton_env`). Platform resolution is the shared
/// `launch::resolve_platform`.
pub(crate) async fn session_proton_grant(
    pool: &SqlitePool,
    data_dir: &Path,
    game_id: &str,
) -> Result<bool> {
    let row: Option<crate::launch::Row> = sqlx::query_as(
        "SELECT id, manager, store, game_id, install_dir, exe_path, prefix_path, proton,
                launch_options, env, wrapper,
                detected_platform, detected_prefix_path, detected_proton,
                override_exe_path, override_platform, override_prefix_path, override_proton
         FROM games WHERE id = ?",
    )
    .bind(game_id)
    .fetch_optional(pool)
    .await?;
    let Some(row) = row else {
        return Ok(false);
    };
    fn nonempty(v: Option<String>) -> Option<String> {
        v.filter(|s| !s.is_empty())
    }
    let proton = nonempty(row.override_proton.clone())
        .or(nonempty(row.detected_proton.clone()))
        .or(nonempty(row.proton.clone()));
    let platform = crate::launch::resolve_platform(&row);
    if platform != "proton" {
        return Ok(false);
    }
    let Some(proton) = proton else {
        return Ok(false);
    };
    if !crate::launch::proton_optiscaler_flavor(Some(&proton)) {
        return Ok(false);
    }
    crate::launch::has_proton_env_plan(data_dir, game_id)
}

/// Run a per-game mutation `op`, then rewrite that game's session files.
/// Single owner of the op-then-sync order: an op error skips the sync, a
/// sync error propagates. Global mutations fan out via `sync_handle_sessions`.
pub async fn mutate_game<Fut, T>(
    pool: &SqlitePool,
    data_dir: &Path,
    host: &PluginHost,
    game_id: &str,
    op: Fut,
) -> Result<T>
where
    Fut: Future<Output = Result<T>>,
{
    let out = op.await?;
    sync_session(pool, data_dir, host, game_id).await?;
    Ok(out)
}

/// Rewrite `games/<rel>/tux-protonfixes.conf` and `games/load-correlator.ini`.
pub async fn sync_session(
    pool: &SqlitePool,
    data_dir: &Path,
    host: &PluginHost,
    game_id: &str,
) -> Result<()> {
    migrate_handle(pool).await?;
    let gid = GameId::parse(game_id)?;
    if !game_exists(pool, game_id).await? {
        return Err(Error::UnknownGame(game_id.into()));
    }
    disarm_if_unneeded(pool, data_dir, game_id).await;
    let inject = game_handle(pool, game_id).await?;
    let mut pairs = BTreeMap::new();
    pairs.insert(
        "inject".into(),
        if inject { "1".into() } else { "0".into() },
    );
    // E80: session env is trampoline fuel, not hook-only. `inject=` gates the
    // hook alone; the trampoline self-arms from argv even at `inject=0`.
    let gdir = game_dir(data_dir, &gid);
    pairs.insert(
        "TUXGT_LAUNCHER_INI".into(),
        managed_ini(&gdir).to_string_lossy().into_owned(),
    );
    pairs.insert(
        "TUXGT_GAME_DIR".into(),
        gdir.join("runtime").to_string_lossy().into_owned(),
    );
    pairs.insert(
        "TUXGT_DEPOT".into(),
        gdir.join("stage").to_string_lossy().into_owned(),
    );
    if let Some(so) = crate::session::find_so_for_game(pool, data_dir, game_id).await {
        pairs.insert(
            "TUXGT_LAUNCHER_SO".into(),
            so.to_string_lossy().into_owned(),
        );
    }
    for (k, v) in crate::enabled_global_env_pairs(pool, Some(host)).await? {
        pairs.insert(k, v);
    }
    for row in knob_rows(pool, game_id).await? {
        if !row.enabled {
            continue;
        }
        let Some(def) = find_enabled_knob(host, &row.knob) else {
            continue;
        };
        if row.value.is_empty() {
            for var in def.env_vars() {
                pairs.insert(var.to_string(), String::new());
            }
        } else if let Some(env) = def.env_pairs(&row.value) {
            for (k, v) in env {
                pairs.insert(k, v);
            }
        }
    }
    for (k, v) in custom_env(pool, game_id).await? {
        pairs.insert(k, v);
    }
    // E74: same mod-env merge as Play (generic last-wins, overrides stem-merge).
    crate::apply_mod_env(&mut pairs, data_dir, game_id)?;
    if session_proton_grant(pool, data_dir, game_id).await? {
        pairs.insert("PROTON_USE_OPTISCALER".into(), "1".into());
    }
    let wraps = game_wrappers(pool, game_id).await?;
    if !wraps.is_empty() {
        pairs.insert("WRAPPERS".into(), wraps.join(","));
    }
    write_session_file(&protonfixes_conf(data_dir, &gid), &render_session(&pairs))?;
    rewrite_correlator(pool, data_dir).await
}

/// Rewrite sessions for every handle-on game plus every Applied game
/// (`inject=0` trampoline rows carry env too) after a global enable/disable.
pub async fn sync_handle_sessions(
    pool: &SqlitePool,
    data_dir: &Path,
    host: &PluginHost,
) -> Result<()> {
    let mut ids = BTreeSet::new();
    let handled: Vec<(String,)> =
        sqlx::query_as("SELECT game_id FROM game_handle WHERE inject != 0 ORDER BY game_id")
            .fetch_all(pool)
            .await?;
    for (id,) in handled {
        ids.insert(id);
    }
    let games: Vec<(String,)> = sqlx::query_as("SELECT id FROM games ORDER BY id")
        .fetch_all(pool)
        .await?;
    for (id,) in games {
        if crate::apply::read_record(data_dir, &id)?.is_some() {
            ids.insert(id);
        }
    }
    if ids.is_empty() {
        rewrite_correlator(pool, data_dir).await?;
        return Ok(());
    }
    for id in ids {
        sync_session(pool, data_dir, host, &id).await?;
    }
    Ok(())
}

pub async fn session_configured(pool: &SqlitePool, game_id: &str) -> Result<bool> {
    if !knob_values(pool, game_id).await?.is_empty() {
        return Ok(true);
    }
    if !custom_env(pool, game_id).await?.is_empty() {
        return Ok(true);
    }
    if !game_wrappers(pool, game_id).await?.is_empty() {
        return Ok(true);
    }
    Ok(false)
}
