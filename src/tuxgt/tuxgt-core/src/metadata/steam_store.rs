use percent_encoding::{utf8_percent_encode, NON_ALPHANUMERIC};
use serde_json::Value;

use super::client;
use crate::{Error, Result};

const SEARCH_URL: &str = "https://store.steampowered.com/api/storesearch/";
const MAX_HITS: usize = 10;

pub struct SteamSearchHit {
    pub appid: u32,
    pub name: String,
}

pub fn search_steam_by_name(name: &str) -> Result<Vec<SteamSearchHit>> {
    if name.trim().is_empty() {
        return Ok(vec![]);
    }
    let http = client()?;
    let term = utf8_percent_encode(name, NON_ALPHANUMERIC);
    let url = format!("{SEARCH_URL}?term={term}&l=en&cc=US");
    let res = http
        .get(&url)
        .send()
        .map_err(|e| Error::Fetch(format!("{url}: {e}")))?;
    if !res.status().is_success() {
        return Err(Error::Fetch(format!("{url}: status {}", res.status())));
    }
    let body: Value = res
        .json()
        .map_err(|e| Error::Fetch(format!("{url}: {e}")))?;
    Ok(parse_search(&body))
}

/// Shorten a game title for a retry when the full title finds nothing on the
/// store (edition suffixes like `Just Cause 2 - Complete Edition` match zero
/// rows while the head matches). Splits at the first ` - ` / ` : ` / ` – `;
/// `None` when there is nothing to shorten.
pub fn shorten_store_query(name: &str) -> Option<String> {
    let q = name.trim();
    for sep in [" - ", " : ", " – "] {
        if let Some((head, _)) = q.split_once(sep) {
            let head = head.trim();
            if !head.is_empty() {
                return Some(head.to_string());
            }
        }
    }
    None
}

fn parse_search(body: &Value) -> Vec<SteamSearchHit> {
    let Some(items) = body.get("items").and_then(Value::as_array) else {
        return vec![];
    };
    items
        .iter()
        .filter_map(|item| {
            let id = item
                .get("id")
                .and_then(Value::as_u64)
                .and_then(|id| u32::try_from(id).ok())?;
            let name = item.get("name")?.as_str()?.to_string();
            Some(SteamSearchHit { appid: id, name })
        })
        .take(MAX_HITS)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn skips_entry_missing_name() {
        let body = json!({
            "total": 2,
            "items": [
                {"id": 440, "name": "Team Fortress 2"},
                {"id": 570},
            ]
        });
        let hits = parse_search(&body);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].appid, 440);
        assert_eq!(hits[0].name, "Team Fortress 2");
    }

    #[test]
    fn empty_items_gives_empty_vec() {
        assert!(parse_search(&json!({"total": 0, "items": []})).is_empty());
        assert!(parse_search(&json!({})).is_empty());
    }

    #[test]
    fn shortens_edition_suffix() {
        assert_eq!(
            shorten_store_query("Just Cause 2 - Complete Edition").as_deref(),
            Some("Just Cause 2")
        );
        assert_eq!(
            shorten_store_query("Game : Definitive Edition").as_deref(),
            Some("Game")
        );
        assert_eq!(
            shorten_store_query("Game – Definitive Edition").as_deref(),
            Some("Game")
        );
    }

    #[test]
    fn no_shortening_without_separator() {
        assert_eq!(shorten_store_query("Just Cause 2"), None);
        assert_eq!(shorten_store_query("  "), None);
        assert_eq!(shorten_store_query(" - Leading Sep"), None);
    }
}
