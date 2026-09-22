use percent_encoding::{utf8_percent_encode, NON_ALPHANUMERIC};
use serde_json::{json, Value};

use super::{client, secret_manager_get, MetaInput, MetaLine, MetadataSource};
use crate::{Error, Result};

const API_URL: &str = "https://www.steamgriddb.com/api/v2";
const PREFER_GRID: &str = "600x900";
const PREFER_HERO: &str = "1920x620";
/// E107: square rail icon for the collapsed sidebar rail.
const PREFER_ICON: &str = "512x512";
const DAY: u64 = 24 * 60 * 60;

pub struct GridSource;

impl MetadataSource for GridSource {
    fn plugin_id(&self) -> &'static str {
        "steamgriddb"
    }

    fn fresh_secs(&self) -> u64 {
        DAY
    }

    fn fetch(&self, input: &MetaInput) -> Result<Value> {
        let Some(key) = secret_manager_get(self.plugin_id())? else {
            return Ok(Value::Null);
        };
        let http = client()?;
        let game_id = match &input.steam_appid {
            Some(appid) => match steam_game(&http, &key, appid)? {
                Some(id) => Some(id),
                None => name_game(&http, &key, &input.name)?,
            },
            None => name_game(&http, &key, &input.name)?,
        };
        let Some(game_id) = game_id else {
            return Ok(unresolved_grid());
        };
        let art_url = best_grid(&http, &key, game_id)?;
        let hero_url = best_hero(&http, &key, game_id)?;
        let icon_url = best_icon(&http, &key, game_id)?;
        Ok(json!({ "art_url": art_url, "hero_url": hero_url, "icon_url": icon_url }))
    }

    fn cache_fresh(&self, data: &Value) -> bool {
        grid_cache_fresh(
            data,
            secret_manager_get(self.plugin_id())
                .ok()
                .flatten()
                .is_some(),
        )
    }

    fn show(&self, _input: &MetaInput, data: Option<&Value>) -> Result<Option<MetaLine>> {
        let line = match secret_manager_get(self.plugin_id())? {
            None => MetaLine {
                key: self.plugin_id(),
                value: "skipped (no key)".into(),
                extra: None,
            },
            Some(_) => MetaLine {
                key: self.plugin_id(),
                value: data
                    .and_then(|v| v.get("art_url"))
                    .and_then(Value::as_str)
                    .unwrap_or("none")
                    .into(),
                extra: data
                    .and_then(|v| v.get("hero_url"))
                    .and_then(Value::as_str)
                    .map(str::to_string),
            },
        };
        Ok(Some(line))
    }
}

fn get_json(http: &reqwest::blocking::Client, key: &str, url: &str) -> Result<Value> {
    let res = http
        .get(url)
        .bearer_auth(key)
        .send()
        .map_err(|e| Error::Fetch(format!("{url}: {e}")))?;
    if res.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(Value::Null);
    }
    if !res.status().is_success() {
        return Err(Error::Fetch(format!("{url}: status {}", res.status())));
    }
    res.json().map_err(|e| Error::Fetch(format!("{url}: {e}")))
}

fn steam_game(http: &reqwest::blocking::Client, key: &str, appid: &str) -> Result<Option<i64>> {
    let url = format!("{API_URL}/games/steam/{appid}");
    let body = get_json(http, key, &url)?;
    Ok(body
        .get("data")
        .and_then(|v| v.get("id"))
        .and_then(Value::as_i64))
}

fn name_game(http: &reqwest::blocking::Client, key: &str, name: &str) -> Result<Option<i64>> {
    if name.is_empty() {
        return Ok(None);
    }
    let term = utf8_percent_encode(name, NON_ALPHANUMERIC);
    let url = format!("{API_URL}/search/autocomplete/{term}");
    let body = get_json(http, key, &url)?;
    Ok(body
        .get("data")
        .and_then(Value::as_array)
        .and_then(|a| a.first())
        .and_then(|g| g.get("id"))
        .and_then(Value::as_i64))
}

fn best_grid(http: &reqwest::blocking::Client, key: &str, game_id: i64) -> Result<Option<String>> {
    let url = format!("{API_URL}/grids/game/{game_id}");
    let body = get_json(http, key, &url)?;
    Ok(pick_image(&body, PREFER_GRID))
}

fn best_hero(http: &reqwest::blocking::Client, key: &str, game_id: i64) -> Result<Option<String>> {
    let url = format!("{API_URL}/heroes/game/{game_id}");
    let body = get_json(http, key, &url)?;
    Ok(pick_image(&body, PREFER_HERO))
}

/// E107: square icon URL for the collapsed sidebar rail.
fn best_icon(http: &reqwest::blocking::Client, key: &str, game_id: i64) -> Result<Option<String>> {
    let url = format!("{API_URL}/icons/game/{game_id}");
    let body = get_json(http, key, &url)?;
    Ok(pick_image(&body, PREFER_ICON))
}

fn image_dim(g: &Value) -> Option<String> {
    if let Some(d) = g.get("dimensions").and_then(Value::as_str) {
        return Some(d.to_string());
    }
    let w = g.get("width").and_then(Value::as_u64)?;
    let h = g.get("height").and_then(Value::as_u64)?;
    Some(format!("{w}x{h}"))
}

/// E107: an icon-less payload is stale, so a pre-icon cache refetches once
/// and the collapsed rail gets its square icon.
fn grid_cache_fresh(data: &Value, has_key: bool) -> bool {
    (data.get("hero_url").is_some() && data.get("icon_url").is_some()) || !has_key
}

/// E107: a game SteamGridDB cannot resolve still writes the full key triad, so
/// `grid_cache_fresh` keeps the row fresh and it is not refetched every day.
fn unresolved_grid() -> Value {
    json!({
        "art_url": Value::Null,
        "hero_url": Value::Null,
        "icon_url": Value::Null,
    })
}

fn pick_image(body: &Value, prefer: &str) -> Option<String> {
    let images = body.get("data")?.as_array()?;
    let pick = images
        .iter()
        .find(|g| image_dim(g).as_deref() == Some(prefer))
        .or_else(|| images.first())?;
    pick.get("url").and_then(Value::as_str).map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picks_600x900_first() {
        let body = json!({
            "success": true,
            "data": [
                {"url": "https://cdn2.steamgriddb.com/grid/a.png", "dimensions": "920x430"},
                {"url": "https://cdn2.steamgriddb.com/grid/b.png", "dimensions": "600x900"},
            ]
        });
        assert_eq!(
            pick_image(&body, PREFER_GRID).as_deref(),
            Some("https://cdn2.steamgriddb.com/grid/b.png")
        );
    }

    #[test]
    fn falls_back_to_first_grid() {
        let body = json!({"data": [
            {"url": "https://cdn2.steamgriddb.com/grid/a.png", "dimensions": "512x512"},
        ]});
        assert_eq!(
            pick_image(&body, PREFER_GRID).as_deref(),
            Some("https://cdn2.steamgriddb.com/grid/a.png")
        );
        assert_eq!(pick_image(&json!({"data": []}), PREFER_GRID), None);
        assert_eq!(pick_image(&json!({}), PREFER_GRID), None);
    }

    #[test]
    fn picks_1920x620_hero() {
        let body = json!({
            "success": true,
            "data": [
                {"url": "https://cdn2.steamgriddb.com/hero/a.png", "width": 3840, "height": 1240},
                {"url": "https://cdn2.steamgriddb.com/hero/b.png", "dimensions": "1920x620"},
            ]
        });
        assert_eq!(
            pick_image(&body, PREFER_HERO).as_deref(),
            Some("https://cdn2.steamgriddb.com/hero/b.png")
        );
    }

    #[test]
    fn hero_falls_back_to_first() {
        let body = json!({"data": [
            {"url": "https://cdn2.steamgriddb.com/hero/a.png", "width": 3840, "height": 1240},
        ]});
        assert_eq!(
            pick_image(&body, PREFER_HERO).as_deref(),
            Some("https://cdn2.steamgriddb.com/hero/a.png")
        );
    }

    #[test]
    fn picks_512_icon_first() {
        let body = json!({
            "success": true,
            "data": [
                {"url": "https://cdn2.steamgriddb.com/icon/a.png", "dimensions": "256x256"},
                {"url": "https://cdn2.steamgriddb.com/icon/b.png", "dimensions": "512x512"},
            ]
        });
        assert_eq!(
            pick_image(&body, PREFER_ICON).as_deref(),
            Some("https://cdn2.steamgriddb.com/icon/b.png")
        );
    }

    #[test]
    fn icon_falls_back_to_first() {
        let body = json!({"data": [
            {"url": "https://cdn2.steamgriddb.com/icon/a.png", "width": 256, "height": 256},
        ]});
        assert_eq!(
            pick_image(&body, PREFER_ICON).as_deref(),
            Some("https://cdn2.steamgriddb.com/icon/a.png")
        );
        assert_eq!(pick_image(&json!({"data": []}), PREFER_ICON), None);
    }

    #[test]
    fn unresolved_grid_stays_fresh_when_keyed() {
        let data = unresolved_grid();
        assert!(data.get("icon_url").is_some(), "triad must be complete");
        assert!(grid_cache_fresh(&data, true));
        assert!(grid_cache_fresh(&data, false));
    }

    #[test]
    fn cache_fresh_needs_hero_and_icon_when_keyed() {
        let complete = json!({
            "art_url": "https://x/g.png",
            "hero_url": Value::Null,
            "icon_url": Value::Null,
        });
        // Pre-E107 caches carry no `icon_url`; they refetch once.
        let hero_only = json!({"art_url": "https://x/g.png", "hero_url": Value::Null});
        assert!(grid_cache_fresh(&complete, true));
        assert!(grid_cache_fresh(&complete, false));
        assert!(!grid_cache_fresh(&hero_only, true));
        assert!(grid_cache_fresh(&hero_only, false));
    }
}
