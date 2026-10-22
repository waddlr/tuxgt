//! Catalog and protonfixes file schema.

use serde::Deserialize;

use super::template::{check_template, ident};
use crate::{Error, Result};

pub const ENGINES: &[&str] = &["unreal", "unity", "re_engine", "creation", "blackspace"];

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum Trust {
    CuratedGame = 0,
    CuratedEngine = 1,
    Pcgw = 2,
    Protondb = 3,
    Lutris = 4,
}

impl Trust {
    pub fn label(self) -> &'static str {
        match self {
            Trust::CuratedGame => "curated",
            Trust::CuratedEngine => "engine",
            Trust::Pcgw => "pcgw",
            Trust::Protondb => "protondb",
            Trust::Lutris => "lutris",
        }
    }

    pub fn from_path(path: &str) -> Option<Self> {
        let p = path.replace('\\', "/");
        if p.contains("/protonfixes/") {
            return None;
        }
        if p.contains("/curated/games/") {
            Some(Trust::CuratedGame)
        } else if p.contains("/curated/engines/") {
            Some(Trust::CuratedEngine)
        } else if p.contains("/pcgw/") {
            Some(Trust::Pcgw)
        } else if p.contains("/protondb/") {
            Some(Trust::Protondb)
        } else if p.contains("/lutris/") {
            Some(Trust::Lutris)
        } else {
            None
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ArgKind {
    Flag,
    Enum,
    Freeform,
}

impl ArgKind {
    pub fn label(self) -> &'static str {
        match self {
            ArgKind::Flag => "flag",
            ArgKind::Enum => "enum",
            ArgKind::Freeform => "freeform",
        }
    }

    fn parse(s: &str) -> Result<Self> {
        match s {
            "flag" => Ok(ArgKind::Flag),
            "enum" => Ok(ArgKind::Enum),
            "freeform" => Ok(ArgKind::Freeform),
            other => Err(Error::Args(format!("unknown kind {other}"))),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArgValue {
    pub value: String,
    pub help: String,
}

#[derive(Clone, Debug)]
pub struct CatalogArg {
    pub id: String,
    pub kind: ArgKind,
    pub template: String,
    pub group: Option<String>,
    pub help: String,
    pub source_url: Option<String>,
    pub values: Vec<ArgValue>,
    pub stem: String,
    pub trust: Trust,
    pub file: String,
}

#[derive(Clone, Debug, Default)]
pub struct GameMatch {
    pub steam_appid: Option<String>,
    pub gog_id: Option<String>,
    pub epic_id: Option<String>,
    pub exe_names: Vec<String>,
    pub engine: Option<String>,
}

#[derive(Clone, Debug)]
pub struct CatalogFile {
    pub path: String,
    pub title: Option<String>,
    pub game: Option<GameMatch>,
    pub engine: Option<String>,
    pub trust: Trust,
    pub args: Vec<CatalogArg>,
}

/// Shipped protonfixes annotation. Never a settable row.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FixNote {
    pub stem: String,
    pub appended_value: String,
    pub fix_file: String,
}

/// Strip leading dashes, ASCII case-fold, keep the text before `=` or `{value}`.
pub fn stem_of(template: &str) -> String {
    let t = template.trim().trim_start_matches('-');
    let t = t.split("{value}").next().unwrap_or(t);
    let t = t.split('=').next().unwrap_or(t);
    t.trim_end_matches(['=', '-']).to_ascii_lowercase()
}

pub fn render_token(template: &str, value: &str) -> String {
    if template.contains("{value}") {
        template.replace("{value}", value)
    } else {
        template.to_string()
    }
}

pub fn parse_catalog(text: &str, path: &str, trust: Trust) -> Result<CatalogFile> {
    let doc: RawFile = toml::from_str(text).map_err(|e| Error::Args(format!("{path}: {e}")))?;
    let game = doc.game.map(GameMatch::from);
    let engine = doc.engine.map(|e| e.name);
    match (&game, &engine) {
        (Some(_), Some(_)) => {
            return Err(Error::Args(format!(
                "{path}: exactly one of [game] or [engine]"
            )))
        }
        (None, None) => {
            return Err(Error::Args(format!(
                "{path}: exactly one of [game] or [engine]"
            )))
        }
        _ => {}
    }
    if let Some(name) = &engine {
        if !ENGINES.contains(&name.as_str()) {
            return Err(Error::Args(format!("{path}: unknown engine {name}")));
        }
    }
    if let Some(g) = &game {
        if !g.exe_names.is_empty() && g.engine.is_none() {
            return Err(Error::Args(format!("{path}: exe_names requires engine")));
        }
        if let Some(name) = &g.engine {
            if !ENGINES.contains(&name.as_str()) {
                return Err(Error::Args(format!("{path}: unknown engine {name}")));
            }
        }
        if g.steam_appid.is_none()
            && g.gog_id.is_none()
            && g.epic_id.is_none()
            && g.exe_names.is_empty()
        {
            return Err(Error::Args(format!("{path}: [game] matches nothing")));
        }
    }
    let mut args = Vec::with_capacity(doc.arg.len());
    let mut seen_id = Vec::new();
    let mut seen_stem = Vec::new();
    for raw in doc.arg {
        let arg = raw.into_arg(path, trust)?;
        if seen_id.iter().any(|id: &String| id == &arg.id) {
            return Err(Error::Args(format!("{path}: duplicate id {}", arg.id)));
        }
        if seen_stem.iter().any(|s: &String| s == &arg.stem) {
            return Err(Error::Args(format!("{path}: duplicate stem {}", arg.stem)));
        }
        seen_id.push(arg.id.clone());
        seen_stem.push(arg.stem.clone());
        args.push(arg);
    }
    Ok(CatalogFile {
        path: path.to_string(),
        title: doc.title,
        game,
        engine,
        trust,
        args,
    })
}

pub fn parse_fixes(text: &str, path: &str) -> Result<Vec<FixNote>> {
    let doc: RawFixes = toml::from_str(text).map_err(|e| Error::Args(format!("{path}: {e}")))?;
    let mut out = Vec::with_capacity(doc.fix.len());
    for f in doc.fix {
        if f.stem.is_empty() || f.appended_value.is_empty() || f.fix_file.is_empty() {
            return Err(Error::Args(format!("{path}: fix fields must be set")));
        }
        out.push(FixNote {
            stem: stem_of(&f.stem),
            appended_value: f.appended_value,
            fix_file: f.fix_file,
        });
    }
    Ok(out)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawFile {
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    game: Option<RawGame>,
    #[serde(default)]
    engine: Option<RawEngine>,
    #[serde(default)]
    arg: Vec<RawArg>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawGame {
    #[serde(default)]
    steam_appid: Option<String>,
    #[serde(default)]
    gog_id: Option<String>,
    #[serde(default)]
    epic_id: Option<String>,
    #[serde(default)]
    exe_names: Vec<String>,
    #[serde(default)]
    engine: Option<String>,
}

impl From<RawGame> for GameMatch {
    fn from(g: RawGame) -> Self {
        Self {
            steam_appid: g.steam_appid.filter(|s| !s.is_empty()),
            gog_id: g.gog_id.filter(|s| !s.is_empty()),
            epic_id: g.epic_id.filter(|s| !s.is_empty()),
            exe_names: g.exe_names,
            engine: g.engine.filter(|s| !s.is_empty()),
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawEngine {
    name: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawArg {
    id: String,
    #[serde(default)]
    kind: Option<String>,
    template: String,
    #[serde(default)]
    group: Option<String>,
    help: String,
    #[serde(default)]
    source_url: Option<String>,
    #[serde(default)]
    value: Vec<RawValue>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawValue {
    value: String,
    help: String,
}

impl RawArg {
    fn into_arg(self, path: &str, trust: Trust) -> Result<CatalogArg> {
        if !ident(&self.id, 48) {
            return Err(Error::Args(format!("{path}: bad id {}", self.id)));
        }
        if self.help.trim().is_empty() {
            return Err(Error::Args(format!("{path}: {} missing help", self.id)));
        }
        let kind = match &self.kind {
            Some(s) => ArgKind::parse(s)?,
            None => ArgKind::Flag,
        };
        let group = match self.group {
            Some(g) if g.is_empty() => None,
            Some(g) => {
                if !ident(&g, 32) {
                    return Err(Error::Args(format!("{path}: bad group {g}")));
                }
                Some(g)
            }
            None => None,
        };
        if kind == ArgKind::Freeform && group.is_some() {
            return Err(Error::Args(format!(
                "{path}: freeform {} cannot set group",
                self.id
            )));
        }
        check_template(path, &self.template, kind)?;
        let values: Vec<ArgValue> = self
            .value
            .into_iter()
            .map(|v| ArgValue {
                value: v.value,
                help: v.help,
            })
            .collect();
        match kind {
            ArgKind::Flag if !values.is_empty() => {
                return Err(Error::Args(format!(
                    "{path}: flag {} cannot list values",
                    self.id
                )))
            }
            ArgKind::Enum if values.is_empty() => {
                return Err(Error::Args(format!(
                    "{path}: enum {} needs values",
                    self.id
                )))
            }
            ArgKind::Freeform if !values.is_empty() => {
                return Err(Error::Args(format!(
                    "{path}: freeform {} cannot list values",
                    self.id
                )))
            }
            _ => {}
        }
        for v in &values {
            if v.value.is_empty() || v.help.trim().is_empty() {
                return Err(Error::Args(format!(
                    "{path}: {} has an empty value",
                    self.id
                )));
            }
        }
        let source_url = self.source_url.filter(|s| !s.is_empty());
        if trust == Trust::CuratedGame && source_url.is_none() {
            return Err(Error::Args(format!(
                "{path}: curated {} needs source_url",
                self.id
            )));
        }
        let stem = stem_of(&self.template);
        if stem.is_empty() {
            return Err(Error::Args(format!(
                "{path}: {} has an empty stem",
                self.id
            )));
        }
        Ok(CatalogArg {
            id: self.id,
            kind,
            template: self.template,
            group,
            help: self.help,
            source_url,
            values,
            stem,
            trust,
            file: path.to_string(),
        })
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawFixes {
    #[serde(default)]
    fix: Vec<RawFix>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawFix {
    stem: String,
    appended_value: String,
    fix_file: String,
}
