use std::collections::HashMap;

use tuxgt_core::metadata::protondb::effective_tier;
use tuxgt_core::{
    cached_metadata, cached_metadata_for, data_dir, enabled_mod_count, game_handle, game_row_by_id,
    harvest_all, list_game_index, list_games, open_db_shared, scan_games, session_configured,
    sync_handle_sessions, GameIndexRow, GameRow, PluginHost,
};

use super::library::awacy_flags;

/// One runtime for every GUI data load, for the life of the process. A
/// runtime per call left no thread of its own, but its tokio blocking pool
/// (`tokio::fs` in `open_db`) and the sqlx worker of the per-call pool were
/// fresh threads every time; each one committed glibc arena pages that no
/// later `free` returned to the OS (~4MB/navigation residue from short-lived
/// per-call pool threads; fixed by sharing the pool and the runtime).
static RT: std::sync::LazyLock<tokio::runtime::Runtime> = std::sync::LazyLock::new(|| {
    // multi_thread, not current_thread: `block_on` is called from the UI
    // thread and from `background_spawn` shells, and only a multi-thread
    // runtime may be driven from two threads at once.
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(1)
        .enable_all()
        .build()
        .expect("gui runtime")
});

pub(crate) fn rt_block<T, F>(f: F) -> tuxgt_core::Result<T>
where
    F: std::future::Future<Output = tuxgt_core::Result<T>>,
{
    RT.block_on(f)
}

pub(crate) fn load_games() -> tuxgt_core::Result<Vec<GameRow>> {
    rt_block(async {
        let dir = data_dir();
        let pool = open_db_shared(&dir).await?;
        let host = PluginHost::load()?;
        if let Err(e) = sync_handle_sessions(&pool, &dir, &host).await {
            tracing::warn!(error = %e, "session correlator rewrite failed");
        }
        list_games(&pool, None, None, None).await
    })
}

/// Slim index, or `None` on a failed read. Callers that reload in place must
/// keep the list they hold rather than clobber it with an empty one.
pub(crate) fn load_index_checked() -> Option<Vec<GameIndexRow>> {
    rt_block(async {
        let pool = open_db_shared(&data_dir()).await?;
        list_game_index(&pool).await
    })
    .ok()
}

/// Always-held minimal index. Same order as `load_games` so positions align.
pub(crate) fn load_index() -> Vec<GameIndexRow> {
    load_index_checked().unwrap_or_default()
}

/// One full row for page-scoped loads. `None` on any error or unknown id.
pub(crate) fn load_game_row(id: &str) -> Option<GameRow> {
    rt_block(async {
        let pool = open_db_shared(&data_dir()).await?;
        game_row_by_id(&pool, id).await
    })
    .unwrap_or(None)
}

pub(crate) fn load_handle(id: &str) -> bool {
    rt_block(async {
        let pool = open_db_shared(&data_dir()).await?;
        game_handle(&pool, id).await
    })
    .unwrap_or(false)
}

pub(crate) fn load_payload(id: &str) -> bool {
    rt_block(async {
        let pool = open_db_shared(&data_dir()).await?;
        session_configured(&pool, id).await
    })
    .unwrap_or(false)
}

pub(crate) fn scan_library() -> tuxgt_core::Result<Vec<GameRow>> {
    rt_block(async {
        let pool = open_db_shared(&data_dir()).await?;
        let host = PluginHost::load()?;
        let dir = data_dir();
        let games = scan_games(&pool, &host).await?;
        // R31: GUI Rescan harvests like `tuxgt scan` (CLI also runs
        // `harvest_all` after scan). Per-game failures warn-and-continue
        // inside `harvest_all`; a harvest error never fails the rescan.
        if let Err(e) = harvest_all(&pool, &dir).await {
            tracing::warn!(error = %e, "gui rescan harvest failed");
        }
        if let Err(e) = sync_handle_sessions(&pool, &dir, &host).await {
            tracing::warn!(error = %e, "session correlator rewrite failed");
        }
        Ok(games)
    })
}
/// Manager ids whose provider plugin is currently disabled (steam/heroic/manual).
/// Display-layer only: callers refresh `Shell.disabled_managers` at startup and
/// after every rescan/scan; rows stay in the DB per game-provider.md E12.
pub(crate) fn load_disabled() -> std::collections::HashSet<String> {
    let disabled = PluginHost::load().map(|host| {
        ["steam", "heroic", "manual"]
            .into_iter()
            .filter(|m| !host.is_enabled(m))
            .map(str::to_string)
            .collect()
    });
    disabled.unwrap_or_default()
}

/// E84: cached ProtonDB tier for one game (Info tab). Parsed from the same
/// whitelisted `protondb` cache rows E83 fetches; Info never hits the
/// network. A missing cache stays an honest `none` via `effective_tier`.
#[derive(Clone)]
pub(crate) struct ProtonSummary {
    pub tier: String,
}

/// Missing cache: honest `none` tier.
impl Default for ProtonSummary {
    fn default() -> Self {
        Self {
            tier: "none".to_string(),
        }
    }
}

impl ProtonSummary {
    pub(crate) fn parse(data: &serde_json::Value) -> Self {
        Self {
            tier: effective_tier(Some(data)).to_string(),
        }
    }
}

pub(crate) fn load_tiers() -> HashMap<String, String> {
    let rows = rt_block(async {
        let pool = open_db_shared(&data_dir()).await?;
        cached_metadata(&pool).await
    })
    .unwrap_or_default();
    let mut out = HashMap::new();
    for (game_id, source, data) in rows {
        if source != "protondb" {
            continue;
        }
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&data) {
            let tier = effective_tier(Some(&v));
            if tier != "none" {
                out.insert(game_id, tier.to_string());
            }
        }
    }
    out
}

/// E84: full cached ProtonDB summaries keyed by game id (Info tab rows).
/// Same cache read as `load_tiers`; tiers stay the hero/library source.
pub(crate) fn load_proton() -> HashMap<String, ProtonSummary> {
    let rows = rt_block(async {
        let pool = open_db_shared(&data_dir()).await?;
        cached_metadata(&pool).await
    })
    .unwrap_or_default();
    let mut out = HashMap::new();
    for (game_id, source, data) in rows {
        if source != "protondb" {
            continue;
        }
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&data) {
            out.insert(game_id, ProtonSummary::parse(&v));
        }
    }
    out
}

pub(crate) fn load_mod_counts(index: &[GameIndexRow]) -> HashMap<String, usize> {
    let data = data_dir();
    index
        .iter()
        .map(|g| (g.id.clone(), enabled_mod_count(&data, &g.id)))
        .collect()
}

/// Indexed game row of a game id, for display-name and Steam-AppID matching.
pub(crate) fn game_row<'a>(games: &'a [GameRow], id: &str) -> Option<&'a GameRow> {
    games.iter().find(|g| g.id == id)
}

/// Minimal index entry of a game id. The index is always held.
pub(crate) fn index_row<'a>(index: &'a [GameIndexRow], id: &str) -> Option<&'a GameIndexRow> {
    index.iter().find(|g| g.id == id)
}

/// All cached metadata facts for one game, from a single cache read.
pub(crate) struct GameMeta {
    pub tier: Option<String>,
    pub proton: Option<ProtonSummary>,
    pub awacy: Option<super::library::AwacyFlag>,
}

/// Single-game metadata for page-scoped loads. Missing rows stay `None`
/// (honest `none` tier at paint).
pub(crate) fn load_metadata_for(id: &str, row: &GameRow) -> GameMeta {
    let rows = rt_block(async {
        let pool = open_db_shared(&data_dir()).await?;
        cached_metadata_for(&pool, id).await
    })
    .unwrap_or_default();
    let mut meta = GameMeta {
        tier: None,
        proton: None,
        awacy: None,
    };
    for (game_id, source, data) in &rows {
        if game_id != id || source != "protondb" {
            continue;
        }
        let Ok(v) = serde_json::from_str::<serde_json::Value>(data) else {
            continue;
        };
        let tier = effective_tier(Some(&v));
        if tier != "none" {
            meta.tier = Some(tier.to_string());
        }
        meta.proton = Some(ProtonSummary::parse(&v));
    }
    meta.awacy = awacy_flags(std::slice::from_ref(row), &rows).remove(id);
    meta
}

/// Single-entry metadata maps for one game. Shared by boot and page entry.
pub(crate) fn game_metadata_maps(
    id: &str,
    meta: GameMeta,
) -> (
    HashMap<String, String>,
    HashMap<String, ProtonSummary>,
    HashMap<String, super::library::AwacyFlag>,
) {
    (
        meta.tier
            .map(|t| HashMap::from([(id.to_string(), t)]))
            .unwrap_or_default(),
        meta.proton
            .map(|p| HashMap::from([(id.to_string(), p)]))
            .unwrap_or_default(),
        meta.awacy
            .map(|a| HashMap::from([(id.to_string(), a)]))
            .unwrap_or_default(),
    )
}
