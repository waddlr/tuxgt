use serde_json::{json, Value};

use super::{client, MetaInput, MetaLine, MetadataSource};
use crate::{Error, Result};

const SUMMARY_URL: &str = "https://www.protondb.com/api/v1/reports/summaries";
const SITE_URL: &str = "https://www.protondb.com/app";
const DAY: u64 = 24 * 60 * 60;

pub struct ProtonSource;

impl MetadataSource for ProtonSource {
    fn plugin_id(&self) -> &'static str {
        "protondb"
    }

    fn fresh_secs(&self) -> u64 {
        DAY
    }

    fn fetch(&self, input: &MetaInput) -> Result<Value> {
        let Some(appid) = &input.steam_appid else {
            return Ok(Value::Null);
        };
        let url = format!("{SUMMARY_URL}/{appid}.json");
        let res = client()?
            .get(&url)
            .send()
            .map_err(|e| Error::Fetch(format!("{url}: {e}")))?;
        if res.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(json!({ "tier": Value::Null }));
        }
        if !res.status().is_success() {
            return Err(Error::Fetch(format!("{url}: status {}", res.status())));
        }
        let body: Value = res
            .json()
            .map_err(|e| Error::Fetch(format!("{url}: {e}")))?;
        Ok(summarize(&body))
    }

    fn show(&self, input: &MetaInput, data: Option<&Value>) -> Result<Option<MetaLine>> {
        let Some(appid) = &input.steam_appid else {
            return Ok(None);
        };
        Ok(Some(MetaLine {
            key: self.plugin_id(),
            value: effective_tier(data).into(),
            extra: Some(format!("{SITE_URL}/{appid}")),
        }))
    }
}

/// Tier chips / `show` value: `tier` unless it is missing, null, empty, or
/// `pending` (low confidence), in which case the `provisionalTier` estimate
/// wins. Anything else is `none`. Old `{tier: ...}`-only cache rows work.
pub fn effective_tier(data: Option<&Value>) -> &str {
    let tier = data.and_then(|v| v.get("tier")).and_then(Value::as_str);
    if let Some(t) = tier {
        if !t.is_empty() && t != "pending" {
            return t;
        }
    }
    match data
        .and_then(|v| v.get("provisionalTier"))
        .and_then(Value::as_str)
    {
        Some(p) if !p.is_empty() => p,
        _ => "none",
    }
}

/// Whitelisted summary payload. Drops user reports and note text; keeps only
/// tier detail, confidence, score, and report count.
fn summarize(body: &Value) -> Value {
    let mut out = serde_json::Map::with_capacity(7);
    for key in [
        "tier",
        "trendingTier",
        "provisionalTier",
        "bestReportedTier",
        "confidence",
        "score",
        "total",
    ] {
        out.insert(
            key.to_string(),
            body.get(key).cloned().unwrap_or(Value::Null),
        );
    }
    Value::Object(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::GameId;

    fn input(appid: Option<&str>) -> MetaInput {
        MetaInput {
            id: GameId::new("steam", "", "814380").unwrap(),
            name: "Satisfactory".into(),
            steam_appid: appid.map(str::to_string),
        }
    }

    #[test]
    fn show_tier_and_site() {
        let src = ProtonSource;
        let data = json!({"tier": "gold"});
        let line = src
            .show(&input(Some("814380")), Some(&data))
            .unwrap()
            .unwrap();
        assert_eq!(line.key, "protondb");
        assert_eq!(line.value, "gold");
        assert_eq!(
            line.extra.as_deref(),
            Some("https://www.protondb.com/app/814380")
        );
    }

    #[test]
    fn pending_falls_back_to_provisional() {
        let data = json!({
            "tier": "pending",
            "trendingTier": "pending",
            "provisionalTier": "gold",
            "bestReportedTier": "platinum",
            "confidence": "low",
            "score": 0.6,
            "total": 42,
        });
        assert_eq!(effective_tier(Some(&data)), "gold");
        let line = ProtonSource
            .show(&input(Some("814380")), Some(&data))
            .unwrap()
            .unwrap();
        assert_eq!(line.value, "gold");
        assert_eq!(
            line.extra.as_deref(),
            Some("https://www.protondb.com/app/814380")
        );
    }

    #[test]
    fn pending_without_provisional_is_none() {
        let data = json!({"tier": "pending", "provisionalTier": null});
        assert_eq!(effective_tier(Some(&data)), "none");
        let line = ProtonSource
            .show(&input(Some("1234")), Some(&data))
            .unwrap()
            .unwrap();
        assert_eq!(line.value, "none");
    }

    #[test]
    fn no_report_is_none() {
        let data = json!({"tier": null});
        let line = ProtonSource
            .show(&input(Some("1234")), Some(&data))
            .unwrap()
            .unwrap();
        assert_eq!(line.value, "none");
        let line = ProtonSource
            .show(&input(Some("1234")), None)
            .unwrap()
            .unwrap();
        assert_eq!(line.value, "none");
    }

    #[test]
    fn summarize_keeps_whitelist_only() {
        let body = json!({
            "tier": "pending",
            "trendingTier": "silver",
            "provisionalTier": "gold",
            "bestReportedTier": "platinum",
            "confidence": "low",
            "score": 0.6,
            "total": 42,
            "reports": [{"id": 1, "notes": "do not persist"}],
        });
        let got = summarize(&body);
        assert_eq!(
            got,
            json!({
                "tier": "pending",
                "trendingTier": "silver",
                "provisionalTier": "gold",
                "bestReportedTier": "platinum",
                "confidence": "low",
                "score": 0.6,
                "total": 42,
            })
        );
    }

    #[test]
    fn no_appid_no_line() {
        assert!(ProtonSource.show(&input(None), None).unwrap().is_none());
    }
}
