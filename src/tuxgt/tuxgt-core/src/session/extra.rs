use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::Path;

use sqlx::SqlitePool;

use super::*;
use crate::game::{game_exists, game_rel, list_games, GameId};
use crate::{Error, Result};

/// R55: one correlator key row. `game` names the owning game for the
/// collision warn/refuse; `rel` decides the render winner (sorted-first).
#[derive(Clone, Debug)]
pub(crate) struct CorrelatorRow {
    rel: String,
    exe: String,
    prefix: String,
    game: String,
}

/// R55: per-game extra correlator exes. Sidecar keyed by game id (no FK,
/// same E12 rule as `game_wrappers`), so scan upserts (which only rewrite
/// `detected_*`) and override changes never touch extras. Keys are stored
/// canonical (`canonical_exe_key`); the prefix side always comes from the
/// row's effective `prefix_path` at render time, never per extra.
pub async fn migrate_extra_exes(pool: &SqlitePool) -> Result<()> {
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS game_extra_exes (
            game_id TEXT NOT NULL,
            exe_path TEXT NOT NULL,
            PRIMARY KEY (game_id, exe_path)
        )",
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// R55: extra exe keys for one game, in sort order. Unknown id errors.
pub async fn list_extra_exes(pool: &SqlitePool, game_id: &str) -> Result<Vec<String>> {
    GameId::parse(game_id)?;
    if !game_exists(pool, game_id).await? {
        return Err(Error::UnknownGame(game_id.into()));
    }
    migrate_extra_exes(pool).await?;
    let rows: Vec<(String,)> =
        sqlx::query_as("SELECT exe_path FROM game_extra_exes WHERE game_id = ? ORDER BY exe_path")
            .bind(game_id)
            .fetch_all(pool)
            .await?;
    Ok(rows.into_iter().map(|r| r.0).collect())
}

/// All extras by game id (re-canonicalized defensively; empties and
/// newline-bearing values dropped, exactly like the primary path).
pub(crate) async fn extra_exe_map(pool: &SqlitePool) -> Result<BTreeMap<String, Vec<String>>> {
    migrate_extra_exes(pool).await?;
    let rows: Vec<(String, String)> =
        sqlx::query_as("SELECT game_id, exe_path FROM game_extra_exes ORDER BY game_id, exe_path")
            .fetch_all(pool)
            .await?;
    let mut out: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (game, raw) in rows {
        if raw.contains('\n') {
            continue;
        }
        let key = canonical_exe_key(&raw);
        if key.is_empty() || key.contains('\n') {
            continue;
        }
        out.entry(game).or_default().push(key);
    }
    Ok(out)
}

/// R55: every key the render would contain: one primary row per game with
/// an effective exe+prefix, plus one row per extra under the same
/// prefix/rel. Games missing prefix or exe are omitted entirely, extras
/// included (no orphan keys); an extra equal to the primary after
/// normalization is skipped (no duplicate key). Sorted by rel so the
/// render net and the refuse check share one winner order.
pub(crate) async fn correlator_rows(pool: &SqlitePool) -> Result<Vec<CorrelatorRow>> {
    let extras = extra_exe_map(pool).await?;
    let mut rows: Vec<CorrelatorRow> = Vec::new();
    for g in list_games(pool, None, None, None).await? {
        let Some(exe) = g
            .exe_path
            .as_deref()
            .map(canonical_exe_key)
            .filter(|s| !s.is_empty())
        else {
            continue;
        };
        let Some(prefix) = g
            .prefix_path
            .as_deref()
            .map(canonical_exe_key)
            .filter(|s| !s.is_empty())
        else {
            continue;
        };
        if exe.contains('\n') || prefix.contains('\n') {
            continue;
        }
        let Ok(id) = GameId::parse(&g.id) else {
            continue;
        };
        let rel = game_rel(&id).to_string_lossy().replace('\\', "/");
        rows.push(CorrelatorRow {
            rel: rel.clone(),
            exe: exe.clone(),
            prefix: prefix.clone(),
            game: g.id.clone(),
        });
        if let Some(xs) = extras.get(&g.id) {
            for x in xs {
                if *x == exe {
                    continue;
                }
                rows.push(CorrelatorRow {
                    rel: rel.clone(),
                    exe: x.clone(),
                    prefix: prefix.clone(),
                    game: g.id.clone(),
                });
            }
        }
    }
    rows.sort_by(|a, b| a.rel.cmp(&b.rel));
    Ok(rows)
}

/// R55: the target game's full key list after a user-initiated write.
/// `pending` applies one exe/prefix override value (`None`/empty clears
/// back to detected/store, mirroring `GAME_ROW_COLS` coalescing) and
/// `extra_add` appends one not-yet-stored extra. Empty when the game would
/// be omitted (no orphan keys).
pub(crate) async fn target_post_keys(
    pool: &SqlitePool,
    game_id: &str,
    pending: Option<(&str, Option<&str>)>,
    extra_add: Option<&str>,
) -> Result<Vec<(String, String)>> {
    let raw: Option<(
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
    )> = sqlx::query_as(
        "SELECT override_exe_path, detected_exe_path, exe_path,
                override_prefix_path, detected_prefix_path, prefix_path
         FROM games WHERE id = ?",
    )
    .bind(game_id)
    .fetch_optional(pool)
    .await?;
    let Some((o_exe, d_exe, s_exe, o_pfx, d_pfx, s_pfx)) = raw else {
        return Ok(Vec::new());
    };
    let mut ovr_exe = o_exe;
    let mut ovr_pfx = o_pfx;
    if let Some((field, value)) = pending {
        let v = value.filter(|s| !s.is_empty()).map(str::to_string);
        if field == "exe" {
            ovr_exe = v;
        } else if field == "prefix" {
            ovr_pfx = v;
        }
    }
    // Mirrors GAME_ROW_COLS: NULLIF(override, '') else detected else store.
    let eff = |ovr: Option<String>, det: Option<String>, store: Option<String>| {
        ovr.filter(|s| !s.is_empty())
            .or(det)
            .or(store)
            .map(|s| canonical_exe_key(&s))
            .filter(|s| !s.is_empty())
    };
    let (Some(exe), Some(prefix)) = (eff(ovr_exe, d_exe, s_exe), eff(ovr_pfx, d_pfx, s_pfx)) else {
        return Ok(Vec::new());
    };
    if exe.contains('\n') || prefix.contains('\n') {
        return Ok(Vec::new());
    }
    let mut keys = vec![(prefix.clone(), exe.clone())];
    let extras = extra_exe_map(pool).await?;
    if let Some(xs) = extras.get(game_id) {
        for x in xs {
            if *x != exe && !keys.iter().any(|k| k.1 == *x) {
                keys.push((prefix.clone(), x.clone()));
            }
        }
    }
    if let Some(raw_add) = extra_add {
        let add = canonical_exe_key(raw_add);
        if !add.is_empty() && !add.contains('\n') && add != exe && !keys.iter().any(|k| k.1 == add)
        {
            keys.push((prefix, add));
        }
    }
    Ok(keys)
}

/// R55: refuse a user-initiated write whose post-write render would collide
/// with another game's key. Names the owning game and the key; writes
/// nothing (callers check before persisting, so there is no partial
/// write). Batch paths (scan, store resync, library load) never call this:
/// they keep writing and rely on the render net below.
pub(crate) async fn refuse_on_collision(
    pool: &SqlitePool,
    game_id: &str,
    keys: &[(String, String)],
) -> Result<()> {
    if keys.is_empty() {
        return Ok(());
    }
    let mut owners: BTreeMap<(String, String), String> = BTreeMap::new();
    for r in correlator_rows(pool).await? {
        if r.game == game_id {
            continue;
        }
        owners.entry((r.prefix, r.exe)).or_insert(r.game);
    }
    // Rows are rel-sorted, so the recorded owner is the render winner.
    for (prefix, exe) in keys {
        if let Some(owner) = owners.get(&(prefix.clone(), exe.clone())) {
            return Err(Error::CorrelatorCollision(format!(
                "key '[{prefix}] {exe}' is already owned by game '{owner}'"
            )));
        }
    }
    Ok(())
}

/// R55: add one extra correlator exe for a game. The key is canonicalized
/// on write; an extra equal to the primary after normalization is skipped
/// (no duplicate key), and a key colliding with another game's
/// `(prefix, exe)` is refused naming the owner, with nothing written.
/// Callers (`games extra-exe add`, the future GUI Launch-tab editor) run
/// `sync_session` after, like every other correlator writer.
pub async fn add_extra_exe(pool: &SqlitePool, game_id: &str, exe: &str) -> Result<()> {
    GameId::parse(game_id)?;
    if !game_exists(pool, game_id).await? {
        return Err(Error::UnknownGame(game_id.into()));
    }
    migrate_extra_exes(pool).await?;
    if exe.contains('\n') {
        return Err(Error::InvalidOverride(format!("bad extra exe: {exe}")));
    }
    let key = canonical_exe_key(exe);
    if key.is_empty() || key.contains('\n') {
        return Err(Error::InvalidOverride(format!("bad extra exe: {exe}")));
    }
    let current = target_post_keys(pool, game_id, None, None).await?;
    if current.iter().any(|(_, e)| e == &key) {
        return Ok(());
    }
    let keys = target_post_keys(pool, game_id, None, Some(&key)).await?;
    refuse_on_collision(pool, game_id, &keys).await?;
    sqlx::query(
        "INSERT INTO game_extra_exes (game_id, exe_path) VALUES (?, ?)
         ON CONFLICT(game_id, exe_path) DO NOTHING",
    )
    .bind(game_id)
    .bind(&key)
    .execute(pool)
    .await?;
    Ok(())
}

/// R55: remove one extra correlator exe. Unknown id errors; an unknown key
/// is a no-op. Callers run `sync_session` after.
pub async fn remove_extra_exe(pool: &SqlitePool, game_id: &str, exe: &str) -> Result<()> {
    GameId::parse(game_id)?;
    if !game_exists(pool, game_id).await? {
        return Err(Error::UnknownGame(game_id.into()));
    }
    migrate_extra_exes(pool).await?;
    let key = canonical_exe_key(exe);
    sqlx::query("DELETE FROM game_extra_exes WHERE game_id = ? AND exe_path = ?")
        .bind(game_id)
        .bind(&key)
        .execute(pool)
        .await?;
    Ok(())
}

/// R55: pre-write guard for user-initiated exe/prefix overrides (doctor
/// `--set`, the future GUI Launch-tab editors): refuse when the
/// post-write render would collide with another game's key. Other fields
/// cannot move keys and skip the check. Batch writers (scan upsert,
/// resync) write `detected_*` directly and never pass through here.
pub(crate) async fn check_override_collision(
    pool: &SqlitePool,
    game_id: &str,
    field: &str,
    value: Option<&str>,
) -> Result<()> {
    if field != "exe" && field != "prefix" {
        return Ok(());
    }
    let keys = target_post_keys(pool, game_id, Some((field, value)), None).await?;
    refuse_on_collision(pool, game_id, &keys).await
}

pub(crate) fn render_correlator(rows: &[CorrelatorRow]) -> String {
    use std::collections::btree_map::Entry;
    // Rows arrive rel-sorted: the first writer of a contested key is the
    // sorted-first rel and wins. Same-rel duplicates (a primary re-listed
    // as an extra) merge silently; cross-game losers are dropped LOUD — a
    // warn names both games, never the old silent BTreeMap last-wins.
    let mut by_prefix: BTreeMap<String, BTreeMap<String, (String, String)>> = BTreeMap::new();
    for r in rows {
        match by_prefix
            .entry(r.prefix.clone())
            .or_default()
            .entry(r.exe.clone())
        {
            Entry::Vacant(v) => {
                v.insert((r.rel.clone(), r.game.clone()));
            }
            Entry::Occupied(o) => {
                let (winner_rel, winner_game) = o.get();
                if *winner_rel != r.rel {
                    tracing::warn!(
                        prefix = %r.prefix,
                        exe = %r.exe,
                        winner = %winner_game,
                        winner_rel = %winner_rel,
                        loser = %r.game,
                        loser_rel = %r.rel,
                        "load-correlator key collision; first rel in sort order wins"
                    );
                }
            }
        }
    }
    let mut out = String::from("# tuxgt load-correlator\n# [prefix]\n# exe=rel\n");
    for (prefix, exes) in by_prefix {
        let _ = writeln!(out, "\n[{prefix}]");
        for (exe, (rel, _)) in exes {
            let _ = writeln!(out, "{exe}={rel}");
        }
    }
    out
}

pub async fn rewrite_correlator(pool: &SqlitePool, data_dir: &Path) -> Result<()> {
    let rows = correlator_rows(pool).await?;
    let path = correlator_path(data_dir);
    write_session_file(&path, &render_correlator(&rows))?;
    Ok(())
}
