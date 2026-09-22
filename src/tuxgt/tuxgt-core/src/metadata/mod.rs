pub mod awacy;
pub mod protondb;
pub mod steam_store;
pub mod steamgriddb;

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde_json::Value;
use sqlx::SqlitePool;

use crate::game::{steam_appid_of, GameId};
use crate::{Error, PluginHost, Result};

pub use awacy::AwacySource;
pub use protondb::ProtonSource;
pub use steam_store::{search_steam_by_name, shorten_store_query, SteamSearchHit};
pub use steamgriddb::GridSource;

const CLIENT_TIMEOUT: Duration = Duration::from_secs(15);
const SECRET_MANAGER_SERVICE: &str = "tuxgt";

pub const KEY_SOURCES: &[&str] = &["steamgriddb"];

#[derive(Clone)]
pub struct MetaInput {
    pub id: GameId,
    pub name: String,
    pub steam_appid: Option<String>,
}

pub struct MetaLine {
    pub key: &'static str,
    pub value: String,
    pub extra: Option<String>,
}

pub trait MetadataSource: Send + Sync {
    fn plugin_id(&self) -> &'static str;
    fn fresh_secs(&self) -> u64;
    fn cache_key(&self, input: &MetaInput) -> String {
        input.id.to_string()
    }
    fn fetch(&self, input: &MetaInput) -> Result<Value>;
    fn show(&self, input: &MetaInput, data: Option<&Value>) -> Result<Option<MetaLine>>;
    /// False forces a refetch of an otherwise unexpired cache row.
    fn cache_fresh(&self, _data: &Value) -> bool {
        true
    }
}

pub static METADATA_SOURCES: &[&dyn MetadataSource] = &[
    &protondb::ProtonSource,
    &steamgriddb::GridSource,
    &awacy::AwacySource,
];

pub fn client() -> Result<reqwest::blocking::Client> {
    reqwest::blocking::Client::builder()
        .timeout(CLIENT_TIMEOUT)
        .user_agent(concat!("tuxgt/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|e| Error::Fetch(e.to_string()))
}

pub async fn migrate_metadata(pool: &SqlitePool) -> Result<()> {
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS metadata_cache (
            game_id TEXT NOT NULL,
            source TEXT NOT NULL,
            data TEXT NOT NULL,
            fetched_at INTEGER NOT NULL,
            PRIMARY KEY (game_id, source)
        )",
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// Cache-only metadata for GUI first paint. Never hits the network.
pub async fn cached_metadata(pool: &SqlitePool) -> Result<Vec<(String, String, String)>> {
    Ok(
        sqlx::query_as("SELECT game_id, source, data FROM metadata_cache")
            .fetch_all(pool)
            .await?,
    )
}

/// Cached metadata rows for one game, for page-scoped loads. Includes the
/// shared `("", "awacy")` dataset row so per-game AWACY matching still works.
pub async fn cached_metadata_for(
    pool: &SqlitePool,
    game_id: &str,
) -> Result<Vec<(String, String, String)>> {
    Ok(sqlx::query_as(
        "SELECT game_id, source, data FROM metadata_cache
         WHERE game_id = ?1 OR (game_id = '' AND source = 'awacy')",
    )
    .bind(game_id)
    .fetch_all(pool)
    .await?)
}

/// E43: metadata AppID for one row — the stored overlay wins, else the Steam
/// game segment when `manager == "steam"`, else none. Applies to every manager.
fn resolve_steam_appid(overlay: Option<String>, manager: &str, game_seg: &str) -> Option<String> {
    overlay.or_else(|| (manager == "steam").then(|| game_seg.to_string()))
}

pub async fn game_show(
    pool: &SqlitePool,
    host: &PluginHost,
    id: &str,
    refresh: bool,
) -> Result<Vec<MetaLine>> {
    let gid = GameId::parse(id)?;
    let row: Option<(String, String, Option<String>)> =
        sqlx::query_as("SELECT manager, game_id, name FROM games WHERE id = ?")
            .bind(gid.to_string())
            .fetch_optional(pool)
            .await?;
    let Some((manager, game_seg, name)) = row else {
        return Err(Error::UnknownGame(id.into()));
    };
    let input = MetaInput {
        id: gid,
        name: name.unwrap_or_default(),
        steam_appid: resolve_steam_appid(steam_appid_of(pool, id).await?, &manager, &game_seg),
    };
    let now = now_secs();
    let mut lines = Vec::new();
    for &src in METADATA_SOURCES {
        if !host.is_enabled(src.plugin_id()) {
            continue;
        }
        let key = src.cache_key(&input);
        let cached = get_cache(pool, &key, src.plugin_id()).await?;
        let fresh = cached
            .as_ref()
            .is_some_and(|(v, at)| now - *at < src.fresh_secs() as i64 && src.cache_fresh(v));
        let mut data = cached.map(|(v, _)| v);
        if refresh || !fresh {
            let owned = input.clone();
            let fetched = tokio::task::spawn_blocking(move || src.fetch(&owned))
                .await
                .unwrap_or_else(|e| Err(Error::Fetch(format!("join: {e}"))));
            match fetched {
                Ok(v) if v.is_null() => {}
                Ok(v) => {
                    put_cache(pool, &key, src.plugin_id(), &v).await?;
                    data = Some(v);
                }
                Err(e) => {
                    tracing::warn!(
                        source = src.plugin_id(),
                        game_id = %input.id,
                        error = %e,
                        "metadata fetch failed; using cached value if any"
                    );
                }
            }
        }
        if let Some(line) = src.show(&input, data.as_ref())? {
            lines.push(line);
        }
    }
    Ok(lines)
}

async fn get_cache(pool: &SqlitePool, game_id: &str, source: &str) -> Result<Option<(Value, i64)>> {
    let row: Option<(String, i64)> = sqlx::query_as(
        "SELECT data, fetched_at FROM metadata_cache WHERE game_id = ? AND source = ?",
    )
    .bind(game_id)
    .bind(source)
    .fetch_optional(pool)
    .await?;
    let Some((text, fetched_at)) = row else {
        return Ok(None);
    };
    match serde_json::from_str(&text) {
        Ok(v) => Ok(Some((v, fetched_at))),
        Err(e) => {
            tracing::warn!(
                source,
                game_id,
                error = %e,
                "unreadable metadata cache row; refetching"
            );
            Ok(None)
        }
    }
}

async fn put_cache(pool: &SqlitePool, game_id: &str, source: &str, data: &Value) -> Result<()> {
    sqlx::query(
        "INSERT INTO metadata_cache (game_id, source, data, fetched_at) VALUES (?, ?, ?, ?)
         ON CONFLICT (game_id, source) DO UPDATE SET data = excluded.data, fetched_at = excluded.fetched_at",
    )
    .bind(game_id)
    .bind(source)
    .bind(data.to_string())
    .bind(now_secs())
    .execute(pool)
    .await?;
    Ok(())
}

fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

pub fn secret_manager_get(source: &str) -> Result<Option<String>> {
    match entry(source)?.get_password() {
        Ok(v) => Ok(Some(v)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(Error::SecretManager(e.to_string())),
    }
}

pub fn secret_manager_set(source: &str, secret: &str) -> Result<()> {
    check_key_source(source)?;
    if secret.is_empty() {
        return Err(Error::SecretManager("empty key".into()));
    }
    entry(source)?
        .set_password(secret)
        .map_err(|e| Error::SecretManager(e.to_string()))
}

pub fn secret_manager_clear(source: &str) -> Result<()> {
    check_key_source(source)?;
    match entry(source)?.delete_credential() {
        Ok(()) => Ok(()),
        Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(Error::SecretManager(e.to_string())),
    }
}

/// E44: one endpoint per key source that can answer a live probe. Only
/// `steamgriddb` takes a key in v1 (`KEY_SOURCES`).
fn key_test_url(source: &str) -> &'static str {
    match source {
        "steamgriddb" => "https://www.steamgriddb.com/api/v2/search/autocomplete/portal",
        _ => "",
    }
}

/// E44: probe the stored key against the source API. Missing key, transport
/// failures, and unexpected statuses are `Err`; 2xx and 401/403 return a
/// one-line report. The key is never included in any message.
pub fn secret_manager_test(source: &str) -> Result<String> {
    check_key_source(source)?;
    let Some(key) = secret_manager_get(source)? else {
        return Err(Error::SecretManager(format!("no key set for {source}")));
    };
    let url = key_test_url(source);
    let status = client()?
        .get(url)
        .bearer_auth(&key)
        .send()
        .map_err(|e| Error::Fetch(format!("{url}: {e}")))?
        .status()
        .as_u16();
    key_test_report(source, status)
}

/// Status → report. Pure, so the outcome mapping is unit-tested without HTTP
/// and the key never reaches the message.
fn key_test_report(source: &str, status: u16) -> Result<String> {
    match status {
        200..=299 => Ok(format!("{source} key valid (HTTP {status})")),
        401 | 403 => Ok(format!("{source} key rejected (HTTP {status})")),
        _ => Err(Error::Fetch(format!(
            "{source} key test failed: HTTP {status}"
        ))),
    }
}

fn check_key_source(source: &str) -> Result<()> {
    if is_key_source(source) {
        Ok(())
    } else {
        Err(Error::UnknownMetadataSource(source.into()))
    }
}

pub fn is_key_source(source: &str) -> bool {
    KEY_SOURCES.contains(&source)
}

fn entry(source: &str) -> Result<keyring::Entry> {
    keyring::Entry::new(SECRET_MANAGER_SERVICE, source)
        .map_err(|e| Error::SecretManager(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn cache_roundtrip() {
        let dir = std::env::temp_dir().join(format!("tuxgt-e16-{}", std::process::id()));
        let _ = tokio::fs::remove_dir_all(&dir).await;
        let pool = crate::open_db(&dir).await.expect("open db");
        let data = serde_json::json!({"tier": "gold"});
        put_cache(&pool, "steam::814380", "protondb", &data)
            .await
            .expect("put");
        let (v, at) = get_cache(&pool, "steam::814380", "protondb")
            .await
            .expect("get")
            .expect("row");
        assert_eq!(v, data);
        assert!(at <= now_secs());
        assert!(get_cache(&pool, "steam::814380", "steamgriddb")
            .await
            .expect("get")
            .is_none());
    }

    #[test]
    fn steam_appid_resolution_precedence() {
        // Overlay wins for every manager.
        assert_eq!(
            resolve_steam_appid(Some("570".into()), "heroic", "gog-id"),
            Some("570".to_string())
        );
        assert_eq!(
            resolve_steam_appid(Some("570".into()), "steam", "814380"),
            Some("570".to_string())
        );
        // No overlay: Steam rows fall back to the game segment.
        assert_eq!(
            resolve_steam_appid(None, "steam", "814380"),
            Some("814380".to_string())
        );
        // No overlay and not a Steam row: none.
        assert_eq!(resolve_steam_appid(None, "heroic", "gog-id"), None);
        assert_eq!(resolve_steam_appid(None, "manual", "abcdef12"), None);
    }

    #[test]
    fn key_test_report_maps_status_without_network() {
        for ok in [200, 204, 299] {
            let report = key_test_report("steamgriddb", ok).expect("2xx is a report");
            assert!(report.contains("valid"), "2xx {ok}: {report}");
            assert!(report.starts_with("steamgriddb"), "names source: {report}");
            assert!(!report.contains('\n'), "one line: {report}");
        }
        for rejected in [401, 403] {
            let report =
                key_test_report("steamgriddb", rejected).expect("auth failure is a report");
            assert!(report.contains("rejected"), "auth {rejected}: {report}");
            assert!(!report.contains('\n'), "one line: {report}");
        }
        for err in [300, 400, 404, 429, 500, 503] {
            assert!(
                key_test_report("steamgriddb", err).is_err(),
                "status {err} must be an error"
            );
        }
    }

    #[test]
    fn source_ids() {
        let ids: Vec<&str> = METADATA_SOURCES.iter().map(|s| s.plugin_id()).collect();
        assert_eq!(ids, ["protondb", "steamgriddb", "awacy"]);
        assert!(is_key_source("steamgriddb"));
        assert!(!is_key_source("protondb"));
    }
}
