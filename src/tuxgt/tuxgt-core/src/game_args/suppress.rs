//! Read-time suppression. A present fix file decides the whole set, including empty.

use std::fs;
use std::path::{Path, PathBuf};

use super::catalog::{catalog_root, fix_key};
use super::schema::{stem_of, FixNote};
use crate::game::GameRow;
use crate::launch::{find_in_path, resolve_proton};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Suppression {
    pub stem: String,
    pub appended: String,
}

/// First present file wins. A missing file falls through. Official Proton with
/// no fix file and no umu hides nothing.
pub fn suppressed_for(row: &GameRow, data_dir: &Path) -> Vec<Suppression> {
    let Some((subdir, id)) = fix_key(row) else {
        return shipped(data_dir, "");
    };
    if let Some(set) = proton_file(row, subdir, &id) {
        return set;
    }
    if uses_umu(row) {
        if let Some(set) = umu_file(subdir, &id) {
            return set;
        }
    }
    shipped(data_dir, &id)
}

fn shipped(data_dir: &Path, id: &str) -> Vec<Suppression> {
    if id.is_empty() {
        return Vec::new();
    }
    // A present file decides the whole set, including empty. It does not merge.
    let path = catalog_root(data_dir)
        .join("protonfixes")
        .join("games")
        .join(format!("{id}.toml"));
    if !path.is_file() {
        return Vec::new();
    }
    let Ok(text) = fs::read_to_string(&path) else {
        return Vec::new();
    };
    match super::schema::parse_fixes(&text, &path.to_string_lossy()) {
        Ok(parsed) => notes_to_suppression(&parsed),
        Err(e) => {
            tracing::warn!(path = %path.display(), error = %e, "bad protonfixes annotation");
            Vec::new()
        }
    }
}

fn notes_to_suppression(notes: &[FixNote]) -> Vec<Suppression> {
    notes
        .iter()
        .map(|n| Suppression {
            stem: n.stem.clone(),
            appended: n.appended_value.clone(),
        })
        .collect()
}

fn proton_file(row: &GameRow, subdir: &str, id: &str) -> Option<Vec<Suppression>> {
    let proton = row.proton.as_deref().filter(|s| !s.is_empty())?;
    let script = resolve_proton(proton)?;
    let mut dirs = Vec::new();
    if let Some(p) = script.parent() {
        dirs.push(p.to_path_buf());
        if let Some(g) = p.parent() {
            dirs.push(g.to_path_buf());
        }
    }
    find_py(&dirs, subdir, id)
}

fn uses_umu(row: &GameRow) -> bool {
    if row.manager == "heroic" {
        return true;
    }
    // Manual proton launches go through umu-run when it is on PATH, matching
    // `proton_runner`. Steam does not.
    row.manager != "steam"
        && row.proton.as_deref().is_some_and(|s| !s.is_empty())
        && find_in_path("umu-run").is_some()
}

fn umu_file(subdir: &str, id: &str) -> Option<Vec<Suppression>> {
    let umu = find_in_path("umu-run")?;
    let parent = umu.parent()?;
    let mut roots = vec![
        parent.join("protonfixes"),
        parent.join("../share/umu/protonfixes"),
        parent.join("../share/protonfixes"),
    ];
    // These roots already end in protonfixes. Join the store dir under each.
    roots.retain(|p| p.exists());
    for root in roots {
        let path = root.join(subdir).join(format!("{id}.py"));
        if path.is_file() {
            return Some(read_py(&path));
        }
    }
    None
}

fn find_py(dirs: &[PathBuf], subdir: &str, id: &str) -> Option<Vec<Suppression>> {
    for dir in dirs {
        let path = dir
            .join("protonfixes")
            .join(subdir)
            .join(format!("{id}.py"));
        if path.is_file() {
            return Some(read_py(&path));
        }
    }
    None
}

fn read_py(path: &Path) -> Vec<Suppression> {
    let Ok(text) = fs::read_to_string(path) else {
        return Vec::new();
    };
    append_literals(&text)
        .into_iter()
        .map(|appended| Suppression {
            stem: stem_of(&appended),
            appended,
        })
        .collect()
}

/// Only a single quoted literal. f-strings and concatenation are not guessed.
pub(super) fn append_literals(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in text.lines() {
        let t = line.trim();
        if t.starts_with('#') {
            continue;
        }
        let Some(idx) = t.find("util.append_argument") else {
            continue;
        };
        let rest = t[idx + "util.append_argument".len()..].trim_start();
        let Some(rest) = rest.strip_prefix('(') else {
            continue;
        };
        let rest = rest.trim_start();
        if rest.starts_with('f') {
            continue;
        }
        let Some(lit) = quoted_literal(rest) else {
            continue;
        };
        out.push(lit);
    }
    out
}

fn quoted_literal(s: &str) -> Option<String> {
    let mut chars = s.chars();
    let q = chars.next()?;
    if q != '"' && q != '\'' {
        return None;
    }
    let mut out = String::new();
    for c in chars {
        if c == '\\' {
            return None;
        }
        if c == q {
            return Some(out);
        }
        out.push(c);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_plain_literals() {
        let src = "util.append_argument(\"-dx11\")\nutil.append_argument(f\"-dx{n}\")\nutil.append_argument('-log')\n";
        let got = append_literals(src);
        assert_eq!(got, vec!["-dx11".to_string(), "-log".to_string()]);
    }
}
