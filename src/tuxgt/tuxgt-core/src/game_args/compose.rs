//! Composable tokens, the Environment view, and the dirty tail.

use std::path::Path;

use sqlx::SqlitePool;

use super::catalog::load_catalog;
use super::catalog::{catalog_root, matching_args, GameIdentity};
use super::schema::{render_token, stem_of, ArgKind, ArgValue, CatalogArg};
use super::store::{self, StoredArg};
use super::suppress::{suppressed_for, Suppression};
use crate::apply::{quote_token, read_record};
use crate::game::{game_row_by_id, GameId, GameRow};
use crate::launch::{parse_launch_options, tokenize};
use crate::{Error, Result};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ArgMark {
    None,
    Suppressed,
    KeptUser,
}

impl ArgMark {
    pub fn label(self) -> &'static str {
        match self {
            ArgMark::None => "",
            ArgMark::Suppressed => "suppressed",
            ArgMark::KeptUser => "kept-user",
        }
    }
}

#[derive(Clone, Debug)]
pub struct ArgView {
    pub id: String,
    pub kind: ArgKind,
    pub template: String,
    pub group: Option<String>,
    pub help: String,
    pub source_url: Option<String>,
    pub values: Vec<ArgValue>,
    pub value: String,
    pub enabled: bool,
    pub source: String,
    pub mark: ArgMark,
    pub appended: Option<String>,
}

#[derive(Clone, Debug)]
pub struct GameArgsView {
    pub dirty: bool,
    pub manual: bool,
    pub custom: String,
    pub rows: Vec<ArgView>,
    pub tail: String,
    /// Unquoted composable tokens (manual `LaunchSpec` appends these).
    pub tokens: Vec<String>,
    pub count: usize,
    /// Enabled unsuppressed catalog row, or a non-empty custom token list.
    pub channel: bool,
}

#[derive(Clone, Debug)]
pub struct CatalogListRow {
    pub file: String,
    pub id: String,
    pub kind: ArgKind,
    pub help: String,
}

pub fn list_shipped(data_dir: &Path) -> Vec<CatalogListRow> {
    let root = catalog_root(data_dir);
    let mut rows = Vec::new();
    for file in load_catalog(&root) {
        let display = short_file(&root, &file.path);
        for arg in file.args {
            rows.push(CatalogListRow {
                file: display.clone(),
                id: arg.id,
                kind: arg.kind,
                help: arg.help,
            });
        }
    }
    rows
}

fn short_file(root: &Path, path: &str) -> String {
    Path::new(path)
        .strip_prefix(root)
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|_| path.to_string())
}

pub fn join_tail(tokens: &[String]) -> String {
    tokens
        .iter()
        .map(|t| quote_token(t))
        .collect::<Vec<_>>()
        .join(" ")
}

pub(crate) struct Built {
    rows: Vec<ArgView>,
    tokens: Vec<String>,
    channel: bool,
}

pub(crate) fn compose(
    args: &[CatalogArg],
    stored: &[StoredArg],
    custom: &str,
    suppressed: &[Suppression],
    baseline: &[String],
) -> Result<Built> {
    let custom_tokens = custom_tokens(custom, suppressed)?;
    let mut rows = Vec::new();
    let mut tokens = Vec::new();
    let mut used_group: Vec<String> = Vec::new();
    let mut channel = !custom_tokens.is_empty();
    for arg in args {
        let row = stored.iter().find(|s| s.arg == arg.id);
        let value = row.map(|r| r.value.clone()).unwrap_or_default();
        let enabled = row.is_some_and(|r| r.enabled);
        let supp = suppressed.iter().find(|s| s.stem == arg.stem);
        let mut mark = ArgMark::None;
        let mut appended = None;
        if let Some(s) = supp {
            mark = ArgMark::Suppressed;
            appended = Some(s.appended.clone());
        }
        let emit = enabled && supp.is_none() && group_ok(arg, &mut used_group);
        if enabled && supp.is_none() {
            channel = true;
        }
        if emit {
            let token = render_token(&arg.template, &value);
            match baseline_hit(&token, &arg.stem, baseline) {
                Hit::Same => {}
                Hit::Kept => mark = ArgMark::KeptUser,
                Hit::New => tokens.push(token),
            }
        }
        rows.push(ArgView {
            id: arg.id.clone(),
            kind: arg.kind,
            template: arg.template.clone(),
            group: arg.group.clone(),
            help: arg.help.clone(),
            source_url: arg.source_url.clone(),
            values: arg.values.clone(),
            value,
            enabled,
            source: arg.trust.label().to_string(),
            mark,
            appended,
        });
    }
    for tok in custom_tokens {
        match baseline_hit(&tok, &stem_of(&tok), baseline) {
            Hit::New => tokens.push(tok),
            Hit::Same | Hit::Kept => {}
        }
    }
    Ok(Built {
        rows,
        tokens,
        channel,
    })
}

fn group_ok(arg: &CatalogArg, used: &mut Vec<String>) -> bool {
    let Some(g) = &arg.group else {
        return true;
    };
    if used.iter().any(|u| u == g) {
        return false;
    }
    used.push(g.clone());
    true
}

enum Hit {
    New,
    Same,
    Kept,
}

fn baseline_hit(token: &str, stem: &str, baseline: &[String]) -> Hit {
    let mut hit = Hit::New;
    for b in baseline {
        if stem_of(b) != stem {
            continue;
        }
        if b.eq_ignore_ascii_case(token) {
            return Hit::Same;
        }
        hit = Hit::Kept;
    }
    hit
}

pub fn custom_tokens(custom: &str, suppressed: &[Suppression]) -> Result<Vec<String>> {
    let trimmed = custom.trim();
    if trimmed.is_empty() {
        return Ok(Vec::new());
    }
    if trimmed.contains("%command%") {
        return Err(Error::Args(
            "%command% is not allowed in custom arguments".into(),
        ));
    }
    let tokens = tokenize(trimmed);
    if tokens.is_empty() {
        return Err(Error::Args("custom arguments are empty after trim".into()));
    }
    // A Proton switch re-evaluates. Drop the stem here; `set_custom` is what refuses.
    Ok(tokens
        .into_iter()
        .filter(|tok| !suppressed.iter().any(|s| s.stem == stem_of(tok)))
        .collect())
}

pub async fn view(pool: &SqlitePool, data_dir: &Path, game_id: &str) -> Result<GameArgsView> {
    let row = game_row_by_id(pool, game_id)
        .await?
        .ok_or_else(|| Error::UnknownGame(game_id.into()))?;
    let manual = row.manager != "steam" && row.manager != "heroic";
    let id = GameIdentity::from_row(&row);
    let files = load_catalog(&catalog_root(data_dir));
    let args = matching_args(&files, &id);
    let stored = store::stored_args(pool, game_id).await?;
    let custom = store::custom_text(pool, game_id).await?;
    let suppressed = suppressed_for(&row, data_dir);
    let baseline = user_baseline(pool, data_dir, &row).await?;
    let built = compose(&args, &stored, &custom, &suppressed, &baseline)?;
    let tail = join_tail(&built.tokens);
    let published = store::published(pool, game_id).await?;
    let prev = published.as_ref().map(|p| p.tail.as_str()).unwrap_or("");
    let dirty = !manual && tail != prev;
    let count = built
        .rows
        .iter()
        .filter(|r| r.enabled && r.mark != ArgMark::Suppressed)
        .count()
        + usize::from(!custom.trim().is_empty());
    Ok(GameArgsView {
        dirty,
        manual,
        custom,
        rows: built.rows,
        tail,
        tokens: built.tokens,
        count,
        channel: built.channel,
    })
}

/// Hero pill count. Skips the user baseline, and skips the catalog when nothing is set.
pub async fn arg_count(pool: &SqlitePool, data_dir: &Path, game_id: &str) -> Result<usize> {
    let stored = store::stored_args(pool, game_id).await?;
    let custom = store::custom_text(pool, game_id).await?;
    if stored.iter().all(|s| !s.enabled) && custom.trim().is_empty() {
        return Ok(0);
    }
    let row = game_row_by_id(pool, game_id)
        .await?
        .ok_or_else(|| Error::UnknownGame(game_id.into()))?;
    let suppressed = suppressed_for(&row, data_dir);
    let built = compose(
        &matching_args(
            &load_catalog(&catalog_root(data_dir)),
            &GameIdentity::from_row(&row),
        ),
        &stored,
        &custom,
        &suppressed,
        &[],
    )?;
    Ok(built
        .rows
        .iter()
        .filter(|r| r.enabled && r.mark != ArgMark::Suppressed)
        .count()
        + usize::from(!custom.trim().is_empty()))
}

pub async fn args_channel(pool: &SqlitePool, data_dir: &Path, game_id: &str) -> Result<bool> {
    let stored = store::stored_args(pool, game_id).await?;
    let custom = store::custom_text(pool, game_id).await?;
    if stored.iter().all(|s| !s.enabled) && custom.trim().is_empty() {
        return Ok(false);
    }
    Ok(view(pool, data_dir, game_id).await?.channel)
}

/// Raw argv tokens for a manual `LaunchSpec`. Store rows return an empty list.
pub async fn manual_tokens(
    pool: &SqlitePool,
    data_dir: &Path,
    game_id: &str,
) -> Result<Vec<String>> {
    let v = view(pool, data_dir, game_id).await?;
    if v.manual {
        Ok(v.tokens)
    } else {
        Ok(Vec::new())
    }
}

async fn user_baseline(pool: &SqlitePool, data_dir: &Path, row: &GameRow) -> Result<Vec<String>> {
    match row.manager.as_str() {
        "steam" => Ok(steam_baseline(data_dir, row)),
        "heroic" => Ok(heroic_baseline(data_dir, row)),
        _ => {
            let opts: Option<(Option<String>,)> =
                sqlx::query_as("SELECT launch_options FROM games WHERE id = ?")
                    .bind(&row.id)
                    .fetch_optional(pool)
                    .await?;
            let raw = opts.and_then(|r| r.0);
            let (_, extra, _) = parse_launch_options(raw.as_deref());
            Ok(extra)
        }
    }
}

fn steam_baseline(data_dir: &Path, row: &GameRow) -> Vec<String> {
    if let Some(rec) = read_record(data_dir, &row.id).ok().flatten() {
        if rec.files.is_empty() {
            return Vec::new();
        }
        let prev = rec.files.iter().find_map(|f| f.previous.as_deref());
        return post_command(prev.unwrap_or(""));
    }
    let Ok(gid) = GameId::parse(&row.id) else {
        return Vec::new();
    };
    crate::provider::steam::steam_launch_options(&gid)
        .map(|s| post_command(&s))
        .unwrap_or_default()
}

fn heroic_baseline(data_dir: &Path, row: &GameRow) -> Vec<String> {
    if let Some(rec) = read_record(data_dir, &row.id).ok().flatten() {
        for f in &rec.files {
            let Some(prev) = f.previous.as_deref() else {
                continue;
            };
            let Ok(v) = serde_json::from_str::<serde_json::Value>(prev) else {
                continue;
            };
            if v.get("launcherArgs").is_some() {
                let s = v.get("launcherArgs").and_then(|x| x.as_str()).unwrap_or("");
                return tokenize(s);
            }
        }
    }
    let Ok(gid) = GameId::parse(&row.id) else {
        return Vec::new();
    };
    crate::provider::heroic::heroic_live_config(&gid.game)
        .and_then(|s| s.launch_options)
        .map(|s| tokenize(&s))
        .unwrap_or_default()
}

fn post_command(options: &str) -> Vec<String> {
    match options.split_once("%command%") {
        Some((_, right)) => tokenize(right),
        None => Vec::new(),
    }
}
