//! Recipe facts for the Settings Mods Details disclosure.
//! Built once in `load_instances` from the loaded `Mod` plus payload
//! provenance. Install names and catalog-check columns stay on `CatalogMeta`
//! because they come from the game list and `mods_cache`.

use std::collections::HashMap;

use tuxgt_core::{Mod, ModProvenance, PayloadRule, SourceRef};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ModKindCode {
    Official,
    User,
    Registry(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RuleParts {
    pub arch: Option<String>,
    pub api: Option<String>,
    pub keep: String,
    pub drop: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct InstanceFacts {
    pub kind: ModKindCode,
    pub slot: Option<String>,
    pub plans: Vec<String>,
    pub fetched_at: Option<u64>,
    pub asset_bytes: Option<u64>,
    pub source_detail: String,
    pub tag: Option<String>,
    pub prerelease: bool,
    pub provenance: Option<String>,
    pub requires: Vec<String>,
    pub globs: Vec<String>,
    pub appids: Vec<u32>,
    pub include: Vec<String>,
    pub remaps: Vec<(String, String)>,
    pub rules: Vec<RuleParts>,
    pub env: Vec<(String, String)>,
}

impl InstanceFacts {
    #[cfg(test)]
    pub(crate) fn empty() -> Self {
        Self {
            kind: ModKindCode::User,
            slot: None,
            plans: Vec::new(),
            fetched_at: None,
            asset_bytes: None,
            source_detail: String::new(),
            tag: None,
            prerelease: false,
            provenance: None,
            requires: Vec::new(),
            globs: Vec::new(),
            appids: Vec::new(),
            include: Vec::new(),
            remaps: Vec::new(),
            rules: Vec::new(),
            env: Vec::new(),
        }
    }
}

pub(crate) fn instance_facts(
    inst: &Mod,
    labels: &HashMap<String, String>,
    provenance: Option<&ModProvenance>,
    provided_ready: bool,
) -> InstanceFacts {
    let (source_detail, tag, prerelease) = source_parts(&inst.source, provided_ready);
    InstanceFacts {
        kind: kind_code(inst.official, inst.registry.as_deref()),
        slot: inst.slot.clone(),
        plans: inst
            .plans_allowed
            .iter()
            .map(|p| p.as_str().to_string())
            .collect(),
        fetched_at: provenance.and_then(|p| (p.fetched_at > 0).then_some(p.fetched_at)),
        asset_bytes: provenance.and_then(|p| (p.asset_bytes > 0).then_some(p.asset_bytes)),
        source_detail,
        tag,
        prerelease,
        provenance: provenance.and_then(|p| provenance_line(&inst.source, &p.source)),
        requires: inst
            .requires
            .iter()
            .map(|id| require_text(id, labels.get(id).map(String::as_str).unwrap_or("")))
            .collect(),
        globs: inst.games.to_vec(),
        appids: inst.appids.to_vec(),
        include: inst.include.to_vec(),
        remaps: inst
            .dests
            .iter()
            .map(|(s, d)| (s.clone(), d.clone()))
            .collect(),
        rules: inst.payload.iter().filter_map(rule_parts).collect(),
        env: inst
            .env
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect(),
    }
}

fn kind_code(official: bool, registry: Option<&str>) -> ModKindCode {
    if official {
        ModKindCode::Official
    } else if let Some(slug) = registry.filter(|s| !s.is_empty()) {
        ModKindCode::Registry(slug.to_string())
    } else {
        ModKindCode::User
    }
}

fn source_parts(source: &SourceRef, provided_ready: bool) -> (String, Option<String>, bool) {
    match source {
        SourceRef::Github {
            owner,
            repo,
            asset_glob,
            tag,
            prerelease,
        } => {
            let mut parts = vec![format!("{owner}/{repo}")];
            if !asset_glob.is_empty() {
                parts.push(asset_glob.clone());
            }
            (parts.join(" · "), tag.clone(), *prerelease)
        }
        SourceRef::Local { path } => (path.clone(), None, false),
        SourceRef::ManualUrl { url } => (url.clone(), None, false),
        SourceRef::Provided { files, .. } => {
            let detail = if provided_ready {
                files.join(", ")
            } else {
                String::new()
            };
            (detail, None, false)
        }
    }
}

/// Resolved fetch URL when it is not the recipe source itself.
pub(crate) fn provenance_line(source: &SourceRef, fetched: &str) -> Option<String> {
    let fetched = fetched.trim();
    if fetched.is_empty() {
        return None;
    }
    let show = match source {
        SourceRef::ManualUrl { url } => fetched != url,
        SourceRef::Local { path } => {
            fetched != path && fetched.strip_prefix("local:") != Some(path.as_str())
        }
        // Ready provide stores the sentinel `provided`, not a fetch URL.
        SourceRef::Provided { .. } => false,
        SourceRef::Github { .. } => true,
    };
    show.then(|| fetched.to_string())
}

pub(crate) fn require_text(id: &str, label: &str) -> String {
    if label.is_empty() || label == id {
        id.to_string()
    } else {
        format!("{label} ({id})")
    }
}

fn rule_parts(rule: &PayloadRule) -> Option<RuleParts> {
    let keep = rule.keep.join(", ");
    let drop = rule.drop.join(", ");
    if rule.arch.is_none() && rule.api.is_none() && keep.is_empty() && drop.is_empty() {
        return None;
    }
    Some(RuleParts {
        arch: rule.arch.clone(),
        api: rule.api.clone(),
        keep,
        drop,
    })
}

pub(crate) fn format_bytes(n: u64) -> String {
    const K: f64 = 1024.0;
    let n = n as f64;
    let (v, unit) = if n < K {
        return format!("{n:.0} B");
    } else if n < K * K {
        (n / K, "KB")
    } else if n < K * K * K {
        (n / (K * K), "MB")
    } else {
        (n / (K * K * K), "GB")
    };
    if v < 10.0 {
        format!("{v:.1} {unit}")
    } else {
        format!("{v:.0} {unit}")
    }
}

pub(crate) fn format_date(dt: time::OffsetDateTime) -> String {
    let month = match dt.month() {
        time::Month::January => "Jan",
        time::Month::February => "Feb",
        time::Month::March => "Mar",
        time::Month::April => "Apr",
        time::Month::May => "May",
        time::Month::June => "Jun",
        time::Month::July => "Jul",
        time::Month::August => "Aug",
        time::Month::September => "Sep",
        time::Month::October => "Oct",
        time::Month::November => "Nov",
        time::Month::December => "Dec",
    };
    format!("{} {month} {}", dt.day(), dt.year())
}

pub(crate) fn format_unix_date(unix: u64) -> Option<String> {
    let ts = i64::try_from(unix).ok()?;
    let dt = time::OffsetDateTime::from_unix_timestamp(ts).ok()?;
    let offset = time::UtcOffset::local_offset_at(dt).unwrap_or(time::UtcOffset::UTC);
    Some(format_date(dt.to_offset(offset)))
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use tuxgt_core::{PayloadRule, Plan, SourceRef};

    use super::*;

    fn bare(source: SourceRef) -> Mod {
        Mod {
            id: "m".into(),
            mod_type: "custom".into(),
            label: "M".into(),
            description: String::new(),
            source,
            plans_allowed: Box::new([Plan::Preload, Plan::Install]),
            sha256: None,
            payload: Box::new([PayloadRule {
                arch: Some("64".into()),
                api: None,
                keep: Box::new(["A.dll".into()]),
                drop: Box::default(),
            }]),
            games: Box::new(["Cyberpunk*".into()]),
            appids: Box::new([1091500]),
            requires: Box::new(["d3dcompiler-47".into(), "same".into()]),
            dests: BTreeMap::from([("a.dll".into(), "b.dll".into())]),
            slot: Some("dxgi".into()),
            include: Box::new(["keep.dll".into()]),
            env: BTreeMap::from([("WINEDLLOVERRIDES".into(), "d3dcompiler_47=n".into())]),
            shader_dir: None,
            texture_dir: None,
            effect_files: Box::default(),
            official: true,
            registry: None,
            enabled: true,
        }
    }

    #[test]
    fn github_keeps_resolved_url_manual_does_not() {
        let gh = SourceRef::Github {
            owner: "optiscaler".into(),
            repo: "OptiScaler".into(),
            asset_glob: "OptiScaler_*.7z".into(),
            tag: Some("v1".into()),
            prerelease: false,
        };
        let url = "https://example.test/OptiScaler.7z";
        assert_eq!(provenance_line(&gh, url).as_deref(), Some(url));
        let manual = SourceRef::ManualUrl { url: url.into() };
        assert_eq!(provenance_line(&manual, url), None);
        let local = SourceRef::Local {
            path: "/mods/x".into(),
        };
        assert_eq!(provenance_line(&local, "local:/mods/x"), None);
        let provided = SourceRef::Provided {
            files: Box::new(["nvngx_dlssnr.dll".into()]),
            note: String::new(),
        };
        assert_eq!(provenance_line(&provided, "provided"), None);
    }

    #[test]
    fn require_text_adds_id_only_when_the_label_differs() {
        assert_eq!(require_text("same", "same"), "same");
        assert_eq!(
            require_text("d3dcompiler-47", "d3dcompiler_47"),
            "d3dcompiler_47 (d3dcompiler-47)"
        );
        assert_eq!(require_text("missing", ""), "missing");
    }

    #[test]
    fn bytes_and_date() {
        assert_eq!(format_bytes(166 * 1024 * 1024), "166 MB");
        assert_eq!(
            format_bytes(4 * 1024 * 1024 + 2 * 1024 * 1024 / 10),
            "4.2 MB"
        );
        let dt = time::OffsetDateTime::new_in_offset(
            time::Date::from_calendar_date(2026, time::Month::September, 12).unwrap(),
            time::Time::from_hms(15, 0, 0).unwrap(),
            time::UtcOffset::UTC,
        );
        assert_eq!(format_date(dt), "12 Sep 2026");
    }

    #[test]
    fn facts_from_recipe_skip_unready_provenance() {
        let mut labels = HashMap::new();
        labels.insert("d3dcompiler-47".into(), "d3dcompiler_47".into());
        labels.insert("same".into(), "same".into());
        let inst = bare(SourceRef::Github {
            owner: "optiscaler".into(),
            repo: "OptiScaler".into(),
            asset_glob: "OptiScaler_*.7z".into(),
            tag: Some("v1".into()),
            prerelease: true,
        });
        let prov = ModProvenance {
            source: "https://example.test/a.7z".into(),
            asset_sha256: "abc".into(),
            asset_bytes: 166 * 1024 * 1024,
            fetched_at: 1_757_678_400,
        };
        let facts = instance_facts(&inst, &labels, Some(&prov), true);
        assert_eq!(facts.kind, ModKindCode::Official);
        assert_eq!(
            facts.source_detail,
            "optiscaler/OptiScaler · OptiScaler_*.7z"
        );
        assert_eq!(facts.tag.as_deref(), Some("v1"));
        assert!(facts.prerelease);
        assert_eq!(
            facts.provenance.as_deref(),
            Some("https://example.test/a.7z")
        );
        assert_eq!(facts.asset_bytes, Some(166 * 1024 * 1024));
        assert_eq!(
            facts.requires,
            vec!["d3dcompiler_47 (d3dcompiler-47)".to_string(), "same".into()]
        );
        assert_eq!(facts.plans, vec!["preload".to_string(), "install".into()]);
        assert_eq!(facts.rules.len(), 1);
        assert_eq!(facts.rules[0].keep, "A.dll");

        let provided = bare(SourceRef::Provided {
            files: Box::new(["nvngx_dlssnr.dll".into()]),
            note: "note".into(),
        });
        let hidden = instance_facts(&provided, &labels, None, false);
        assert!(hidden.source_detail.is_empty());
        assert!(hidden.provenance.is_none());
        let shown = instance_facts(&provided, &labels, None, true);
        assert_eq!(shown.source_detail, "nvngx_dlssnr.dll");
    }
}
