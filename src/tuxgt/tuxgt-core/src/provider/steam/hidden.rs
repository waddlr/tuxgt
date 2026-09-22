use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use super::*;

pub(crate) fn steam_roots() -> Vec<PathBuf> {
    match steamlocate::locate_all() {
        Ok(dirs) => dirs.iter().map(|d| d.path().to_path_buf()).collect(),
        Err(e) => {
            tracing::warn!(error = %e, "steam not found for apply");
            Vec::new()
        }
    }
}

/// Visit every `userdata/<user>` dir under the Steam roots.
fn walk_userdata(roots: &[PathBuf], mut visit: impl FnMut(&Path)) {
    for root in roots {
        let Ok(users) = fs::read_dir(root.join("userdata")) else {
            continue;
        };
        for user in users.flatten() {
            visit(&user.path());
        }
    }
}

/// Every existing per-user `localconfig.vdf` under the Steam roots.
pub(crate) fn localconfig_files(roots: &[PathBuf]) -> Vec<PathBuf> {
    let mut out = Vec::new();
    walk_userdata(roots, |user| {
        let p = user.join("config").join("localconfig.vdf");
        if p.is_file() {
            out.push(p);
        }
    });
    out.sort();
    out
}

/// Every `sharedconfig.vdf` under the Steam roots
/// (`userdata/<user>/<appid>/remote/sharedconfig.vdf`). Owned-app hidden
/// lives here per app (`"hidden" "1"`); typically sparse — shortcuts carry
/// `IsHidden` instead.
pub(crate) fn sharedconfig_files(roots: &[PathBuf]) -> Vec<PathBuf> {
    let mut out = Vec::new();
    walk_userdata(roots, |user| {
        let Ok(entries) = fs::read_dir(user) else {
            return;
        };
        for e in entries.flatten() {
            let p = e.path().join("remote").join("sharedconfig.vdf");
            if p.is_file() {
                out.push(p);
            }
        }
    });
    out.sort();
    out
}

/// Owned-app hidden set (appid strings) from `localconfig.vdf` +
/// `sharedconfig.vdf` per-app `hidden`/`tags` sections.
pub(crate) fn load_owned_hidden(roots: &[PathBuf]) -> HashSet<String> {
    let mut out = HashSet::new();
    for p in localconfig_files(roots)
        .into_iter()
        .chain(sharedconfig_files(roots))
    {
        let Ok(text) = fs::read_to_string(&p) else {
            continue;
        };
        out.extend(collect_vdf_hidden(&text));
    }
    out
}

/// All appids in `text` whose `apps`-section block is marked hidden:
/// a `"hidden" "1"` key (any case) or a `tags` sub-block containing a
/// value exactly `hidden` (case-insensitive). Store-tag names like
/// `"Hidden Object"` never match exactly, so they stay visible.
pub(crate) fn collect_vdf_hidden(text: &str) -> HashSet<String> {
    let mut out = HashSet::new();
    let mut lines = text.lines().map(str::trim);
    if lines.by_ref().find(|l| *l == "\"apps\"").is_none()
        || lines.by_ref().find(|l| *l == "{").is_none()
    {
        return out;
    }
    // Walk state inside the `apps` block: plain scan, blank-skip after an
    // app key, or inner-line capture until the app block closes.
    enum State<'a> {
        Scan,
        SeekOpen {
            app: String,
        },
        Capture {
            app: String,
            inner: Vec<&'a str>,
            depth: i32,
            in_string: bool,
        },
    }
    // The consumed `{` scanned from depth 0 always yields depth 1.
    let mut state = State::Scan;
    let mut depth: i32 = 1;
    let mut in_string = false;
    for t in lines {
        match state {
            State::Scan => {
                if depth == 1 {
                    if let Some(app) = app_key(t) {
                        state = State::SeekOpen { app };
                    }
                }
            }
            State::SeekOpen { app } => {
                if t.is_empty() {
                    state = State::SeekOpen { app };
                } else if t == "{" {
                    state = State::Capture {
                        app,
                        inner: Vec::new(),
                        depth: 1,
                        in_string: false,
                    };
                } else {
                    // Not a block: re-test this line as a fresh key, since
                    // the scan visits every line as a key candidate.
                    state = State::Scan;
                    if depth == 1 {
                        if let Some(next) = app_key(t) {
                            state = State::SeekOpen { app: next };
                        }
                    }
                }
            }
            State::Capture {
                app,
                mut inner,
                depth: mut d,
                in_string: mut s,
            } => {
                let (dd, ss) = scan_line(t, s);
                s = ss;
                d += dd;
                if d == 0 {
                    if block_hidden(&inner) {
                        out.insert(app);
                    }
                    state = State::Scan;
                } else {
                    inner.push(t);
                    state = State::Capture {
                        app,
                        inner,
                        depth: d,
                        in_string: s,
                    };
                }
            }
        }
        let (dd, ss) = scan_line(t, in_string);
        in_string = ss;
        depth += dd;
        if depth < 0 {
            break;
        }
    }
    out
}

/// Numeric app key of a depth-1 line (`"814380"`), else `None`.
fn app_key(t: &str) -> Option<String> {
    if t.len() > 2 && t.starts_with('"') && t.ends_with('"') {
        let app = strip_quotes(t);
        if !app.is_empty() && app.chars().all(|c| c.is_ascii_digit()) {
            return Some(app);
        }
    }
    None
}

/// True when an app block hides the title. Tracks a `tags` sub-block so a
/// bare `"hidden"` value there counts, without matching store tag names.
pub(crate) fn block_hidden(block: &[&str]) -> bool {
    let mut depth: i32 = 0;
    let mut in_string = false;
    let mut in_tags = false;
    let mut tags_depth: i32 = 0;
    for line in block {
        let t = line.trim();
        if t.eq_ignore_ascii_case("\"tags\"") {
            in_tags = true;
            tags_depth = depth;
        }
        if t.to_ascii_lowercase().starts_with("\"hidden\"") {
            let rest = &t["\"hidden\"".len()..];
            if let Some(v) = first_quoted(rest) {
                if v == "1" {
                    return true;
                }
            }
        }
        if in_tags {
            for part in t.split('"').skip(1).step_by(2) {
                if part.eq_ignore_ascii_case("hidden") {
                    return true;
                }
            }
        }
        let (dd, ss) = scan_line(line, in_string);
        in_string = ss;
        depth += dd;
        if in_tags && t == "}" && depth == tags_depth {
            in_tags = false;
        }
    }
    false
}

/// Shortcut `IsHidden` map from every `shortcuts.vdf` under the roots.
/// `steamlocate` does not expose `IsHidden`, so parse the binary directly:
/// kind `2` + key `IsHidden` + LE u32 (`!= 0` hides).
pub(crate) fn load_shortcut_hidden(roots: &[PathBuf]) -> HashMap<u32, bool> {
    let mut out = HashMap::new();
    walk_userdata(roots, |user| {
        let p = user.join("config").join("shortcuts.vdf");
        let Ok(bytes) = fs::read(&p) else {
            return;
        };
        out.extend(shortcut_hidden_map(&bytes));
    });
    out
}

/// Hidden appids from every per-user `localconfig.vdf` `user-collections`
/// value (`hidden.added` minus `hidden.removed`). Covers owned + shortcut
/// appids; modern Steam records hidden here instead of legacy VDF
/// tags/`IsHidden`, which stay as fallback.
pub(crate) fn load_ucollections_hidden(roots: &[PathBuf]) -> HashSet<u32> {
    let mut out = HashSet::new();
    for p in localconfig_files(roots) {
        let Ok(text) = fs::read_to_string(&p) else {
            continue;
        };
        out.extend(collect_ucollections_hidden(&text));
    }
    out
}

/// Added-minus-removed appids from one localconfig text's
/// `"user-collections"` JSON value. Missing key or malformed JSON yields
/// empty; non-appid entries (floats, negatives, nulls) are skipped.
pub(crate) fn collect_ucollections_hidden(text: &str) -> HashSet<u32> {
    let mut out = HashSet::new();
    for line in text.lines() {
        let t = line.trim();
        if !t.starts_with("\"user-collections\"") {
            continue;
        }
        let rest = &t["\"user-collections\"".len()..];
        let Some(json) = first_quoted(rest) else {
            continue;
        };
        let Ok(doc) = serde_json::from_str::<serde_json::Value>(&json) else {
            continue;
        };
        let hidden = doc.get("hidden");
        let added = appid_list(hidden.and_then(|h| h.get("added")));
        let removed = appid_list(hidden.and_then(|h| h.get("removed")));
        out.extend(added.difference(&removed).copied());
    }
    out
}

/// Appids from a `user-collections` added/removed array: JSON numbers
/// that fit `u32` plus all-digit strings (normalized through `u32`).
fn appid_list(v: Option<&serde_json::Value>) -> HashSet<u32> {
    let mut set = HashSet::new();
    let Some(serde_json::Value::Array(items)) = v else {
        return set;
    };
    for item in items {
        let id = match item {
            serde_json::Value::Number(n) => n.as_u64().and_then(|u| u32::try_from(u).ok()),
            serde_json::Value::String(s)
                if !s.is_empty() && s.chars().all(|c| c.is_ascii_digit()) =>
            {
                s.parse::<u32>().ok()
            }
            _ => None,
        };
        if let Some(id) = id {
            set.insert(id);
        }
    }
    set
}

pub(crate) fn shortcut_hidden_map(contents: &[u8]) -> HashMap<u32, bool> {
    let mut out = HashMap::new();
    let mut current: Option<u32> = None;
    for start in 0..contents.len() {
        let kind = contents[start];
        if kind != 2 {
            continue;
        }
        let Some(null_at) = contents[start + 1..]
            .iter()
            .position(|b| *b == 0)
            .map(|i| start + 1 + i)
        else {
            continue;
        };
        let field = &contents[start + 1..null_at];
        let rest = &contents[null_at + 1..];
        if field.eq_ignore_ascii_case(b"appid") {
            if let Some(bytes) = rest.get(..4) {
                let Ok(b) = bytes.try_into() else { continue };
                let id = u32::from_le_bytes(b);
                current = Some(id);
                out.entry(id).or_insert(false);
            }
        } else if field.eq_ignore_ascii_case(b"ishidden") {
            if let Some(bytes) = rest.get(..4) {
                let Ok(b) = bytes.try_into() else { continue };
                let v = u32::from_le_bytes(b);
                if let Some(app) = current {
                    out.insert(app, v != 0);
                }
            }
        }
    }
    out
}
