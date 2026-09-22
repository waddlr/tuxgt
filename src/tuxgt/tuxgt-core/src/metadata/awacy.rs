use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{client, MetaInput, MetaLine, MetadataSource};
use crate::{Error, Result};

const GAMES_URL: &str =
    "https://raw.githubusercontent.com/AreWeAntiCheatYet/AreWeAntiCheatYet/master/games.json";
const WEEK: u64 = 7 * 24 * 60 * 60;

pub struct AwacySource;

#[derive(Serialize, Deserialize)]
struct AwacyEntry {
    name: String,
    #[serde(default)]
    steam_id: Option<String>,
    status: String,
    #[serde(default)]
    anticheats: Vec<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawGame {
    name: String,
    status: String,
    #[serde(default)]
    anticheats: Option<Vec<String>>,
    #[serde(default)]
    store_ids: Option<RawStores>,
}

#[derive(Deserialize)]
struct RawStores {
    steam: Option<String>,
}

impl MetadataSource for AwacySource {
    fn plugin_id(&self) -> &'static str {
        "awacy"
    }

    fn fresh_secs(&self) -> u64 {
        WEEK
    }

    fn cache_key(&self, _input: &MetaInput) -> String {
        String::new()
    }

    fn fetch(&self, _input: &MetaInput) -> Result<Value> {
        let res = client()?
            .get(GAMES_URL)
            .send()
            .map_err(|e| Error::Fetch(format!("{GAMES_URL}: {e}")))?;
        if !res.status().is_success() {
            return Err(Error::Fetch(format!(
                "{GAMES_URL}: status {}",
                res.status()
            )));
        }
        let raw: Vec<RawGame> = res
            .json()
            .map_err(|e| Error::Fetch(format!("{GAMES_URL}: {e}")))?;
        let entries: Vec<AwacyEntry> = raw
            .into_iter()
            .map(|g| AwacyEntry {
                name: g.name,
                steam_id: g.store_ids.and_then(|s| s.steam),
                status: g.status,
                anticheats: g.anticheats.unwrap_or_default(),
            })
            .collect();
        serde_json::to_value(entries).map_err(|e| Error::Fetch(e.to_string()))
    }

    fn show(&self, input: &MetaInput, data: Option<&Value>) -> Result<Option<MetaLine>> {
        let entries: Vec<AwacyEntry> = match data.map(|v| serde_json::from_value(v.clone())) {
            Some(Ok(v)) => v,
            Some(Err(e)) => {
                tracing::warn!(source = self.plugin_id(), error = %e, "unreadable awacy cache payload");
                Vec::new()
            }
            None => Vec::new(),
        };
        let line = match find(&entries, input) {
            Some(e) => MetaLine {
                key: self.plugin_id(),
                value: e.status.clone(),
                extra: (!e.anticheats.is_empty()).then(|| e.anticheats.join(",")),
            },
            None => MetaLine {
                key: self.plugin_id(),
                value: "none".into(),
                extra: None,
            },
        };
        Ok(Some(line))
    }
}

fn find<'a>(entries: &'a [AwacyEntry], input: &MetaInput) -> Option<&'a AwacyEntry> {
    if let Some(appid) = &input.steam_appid {
        if let Some(e) = entries
            .iter()
            .find(|e| e.steam_id.as_deref() == Some(appid.as_str()))
        {
            return Some(e);
        }
    }
    if input.name.is_empty() {
        return None;
    }
    entries
        .iter()
        .find(|e| e.name.eq_ignore_ascii_case(&input.name))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::GameId;
    use serde_json::json;

    fn entries() -> Vec<AwacyEntry> {
        vec![
            AwacyEntry {
                name: "Halo: The Master Chief Collection".into(),
                steam_id: Some("976730".into()),
                status: "Supported".into(),
                anticheats: vec!["Easy Anti-Cheat".into()],
            },
            AwacyEntry {
                name: "Fortnite".into(),
                steam_id: None,
                status: "Denied".into(),
                anticheats: vec!["Easy Anti-Cheat".into(), "BattlEye".into()],
            },
        ]
    }

    fn input(name: &str, appid: Option<&str>) -> MetaInput {
        MetaInput {
            id: GameId::new("steam", "", "976730").unwrap(),
            name: name.into(),
            steam_appid: appid.map(str::to_string),
        }
    }

    #[test]
    fn steam_id_match_wins() {
        let list = entries();
        let e = find(
            &list,
            &input("halo the master chief collection", Some("976730")),
        )
        .expect("matched");
        assert_eq!(e.status, "Supported");
    }

    #[test]
    fn name_match_case_insensitive() {
        let list = entries();
        let e = find(&list, &input("FORTNITE", None)).expect("matched");
        assert_eq!(e.status, "Denied");
        assert!(find(&list, &input("nope", None)).is_none());
        assert!(find(&list, &input("", None)).is_none());
    }

    #[test]
    fn line_with_anticheats_extra() {
        let src = AwacySource;
        let data = serde_json::to_value(entries()).unwrap();
        let line = src
            .show(&input("Fortnite", None), Some(&data))
            .unwrap()
            .unwrap();
        assert_eq!(line.key, "awacy");
        assert_eq!(line.value, "Denied");
        assert_eq!(line.extra.as_deref(), Some("Easy Anti-Cheat,BattlEye"));
    }

    #[test]
    fn no_data_or_no_match_is_none() {
        let src = AwacySource;
        let line = src.show(&input("Fortnite", None), None).unwrap().unwrap();
        assert_eq!(line.value, "none");
        assert_eq!(line.extra, None);
        let data = json!([]);
        let line = src
            .show(&input("Fortnite", None), Some(&data))
            .unwrap()
            .unwrap();
        assert_eq!(line.value, "none");
    }
}
