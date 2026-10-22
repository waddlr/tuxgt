//! Publish the post-`%command%` argument tail without replacing `previous`.

use std::fs;
use std::path::Path;

use super::*;
use crate::apply::{quote_fragment, read_record, ApplyCtx};
use crate::game::{GameId, STANDALONE};
use crate::{atomic_write, Error, Result};

/// Rewrite each recorded file to `compose_options(previous, trampoline)` plus
/// `tail`. `previous` stays the pre-TuxGT fragment. `prior_tail` is the last
/// published tail, used only when `previous` is a `strip_fragment` reconstruction.
pub(crate) fn publish_tail(
    ctx: &ApplyCtx,
    game_id: &str,
    tail: &str,
    prior_tail: &str,
) -> Result<String> {
    let rec = read_record(&ctx.data_dir, game_id)?.ok_or_else(|| {
        Error::Apply(format!(
            "{game_id} has no apply record; save arms Apply first"
        ))
    })?;
    if rec.manager != "steam" {
        return Err(Error::Apply(format!(
            "{game_id} was applied via {}",
            rec.manager
        )));
    }
    if rec.files.is_empty() {
        return Err(Error::Apply(format!("{game_id} apply record has no files")));
    }
    let gid = GameId::parse(game_id)?;
    let frag = quote_fragment(&ctx.launcher);
    let standalone = gid.store == STANDALONE;
    let appid = if standalone {
        Some(shortcut_appid(&gid.game)?)
    } else {
        None
    };
    for entry in &rec.files {
        if !Path::new(&entry.path).is_file() {
            return Err(Error::Apply(format!(
                "apply target missing on arg save: {}",
                entry.path
            )));
        }
    }
    let mut wrote = 0;
    for entry in &rec.files {
        let path = Path::new(&entry.path);
        if standalone {
            let Some(id) = appid else {
                return Err(Error::Apply(format!("{game_id} has no shortcut appid")));
            };
            let bytes = fs::read(path).map_err(|e| Error::Apply(format!("{}: {e}", entry.path)))?;
            let live = shortcut_get_options(&bytes, id).unwrap_or_default();
            let options = options_for(entry.previous.as_deref(), &live, &frag, prior_tail, tail);
            let (new_bytes, _) = shortcut_set_options(&bytes, id, &options)?;
            if new_bytes != bytes {
                atomic_write(path, &new_bytes)?;
            }
        } else {
            let text = fs::read_to_string(path)
                .map_err(|e| Error::Apply(format!("{}: {e}", entry.path)))?;
            let live = steam_get_options(&text, &gid.game).unwrap_or_default();
            let options = options_for(entry.previous.as_deref(), &live, &frag, prior_tail, tail);
            let (new_text, _) = steam_set_options(&text, &gid.game, &options)?;
            if new_text != text {
                atomic_write(path, new_text.as_bytes())?;
            }
        }
        wrote += 1;
    }
    Ok(format!("{game_id} arguments saved ({wrote} file(s))"))
}

fn options_for(
    previous: Option<&str>,
    live: &str,
    frag: &str,
    prior_tail: &str,
    tail: &str,
) -> String {
    let repaired = repair_stripped(previous, live, frag, prior_tail);
    let prev = repaired.as_deref().or(previous);
    with_tail(&compose_options(prev, frag), tail)
}

/// Lost-record Apply stores `strip_fragment`, which deletes `%command%`.
/// A real previous with no `%command%` still matches the live pre-command side.
/// Anything else is that reconstruction: rebuild from the live split every Save
/// and leave `previous` itself unchanged.
fn repair_stripped(
    previous: Option<&str>,
    live: &str,
    frag: &str,
    prior_tail: &str,
) -> Option<String> {
    let previous = previous?;
    if previous.contains("%command%") || !live.contains("%command%") || !live.contains(frag) {
        return None;
    }
    let kept = live.replacen(frag, " ", 1);
    let (pre, post) = kept.split_once("%command%")?;
    let pre = pre.trim();
    let pre_norm = pre.split_whitespace().collect::<Vec<_>>().join(" ");
    let prev_norm = previous.split_whitespace().collect::<Vec<_>>().join(" ");
    if pre_norm == prev_norm {
        return None;
    }
    let post = strip_prior_tail(post.trim(), prior_tail.trim());
    Some(join_command(pre, &post))
}

fn join_command(pre: &str, post: &str) -> String {
    match (pre.is_empty(), post.is_empty()) {
        (true, true) => "%command%".to_string(),
        (true, false) => format!("%command% {post}"),
        (false, true) => format!("{pre} %command%"),
        (false, false) => format!("{pre} %command% {post}"),
    }
}

/// `prior` is the exact string `with_tail` appended. Cut that suffix, do not re-lex it.
fn strip_prior_tail(post: &str, prior: &str) -> String {
    let post = post.trim();
    let prior = prior.trim();
    if prior.is_empty() {
        return post.to_string();
    }
    if post == prior {
        return String::new();
    }
    let suffix = format!(" {prior}");
    post.strip_suffix(&suffix)
        .map(|s| s.trim_end().to_string())
        .unwrap_or_else(|| post.to_string())
}

pub(crate) fn with_tail(base: &str, tail: &str) -> String {
    let tail = tail.trim();
    if tail.is_empty() {
        base.to_string()
    } else if base.is_empty() {
        tail.to_string()
    } else {
        format!("{base} {tail}")
    }
}
