use std::fs;
use std::path::{Path, PathBuf};

use sqlx::SqlitePool;

use crate::{game_manifests, Result};

/// Managed loader ini: `<game>/tuxgt-launcher.ini`. The app owns this file;
/// Play always exports `TUXGT_LAUNCHER_INI` pointing at it.
pub fn managed_ini(game_dir: &Path) -> PathBuf {
    game_dir.join("tuxgt-launcher.ini")
}

struct Section {
    name: Option<String>,
    lines: Vec<String>,
}

fn parse_ini(text: &str) -> Vec<Section> {
    let mut sections = vec![Section {
        name: None,
        lines: Vec::new(),
    }];
    for line in text.lines() {
        let t = line.trim();
        if t.starts_with('[') && t.ends_with(']') && t.len() > 2 {
            sections.push(Section {
                name: Some(t[1..t.len() - 1].to_string()),
                lines: Vec::new(),
            });
        } else if let Some(last) = sections.last_mut() {
            last.lines.push(line.to_string());
        } else {
            debug_assert!(false, "parse_ini sections never empty");
            sections.push(Section {
                name: None,
                lines: vec![line.to_string()],
            });
        }
    }
    sections
}

fn render_ini(sections: &[Section]) -> String {
    let mut out = String::new();
    for s in sections {
        if let Some(name) = &s.name {
            out.push_str(&format!("[{name}]\n"));
        }
        for l in &s.lines {
            out.push_str(l);
            out.push('\n');
        }
    }
    out
}

fn find_section<'a>(sections: &'a mut [Section], name: &str) -> Option<&'a mut Section> {
    sections.iter_mut().find(|s| {
        s.name
            .as_deref()
            .is_some_and(|n| n.eq_ignore_ascii_case(name))
    })
}

fn write_ini(path: &Path, sections: &[Section]) -> Result<()> {
    let rendered = render_ini(sections);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, rendered.as_bytes())?;
    fs::rename(&tmp, path)?;
    Ok(())
}

/// Rewrite `[<stem>]` (loader matches sections case-insensitively) with the
/// given body, preserving every other section byte-for-byte.
pub fn set_ini_section(ini: &Path, section: &str, body: &[String]) -> Result<()> {
    let text = fs::read_to_string(ini).unwrap_or_default();
    let mut sections = parse_ini(&text);
    match find_section(&mut sections, section) {
        Some(s) => s.lines = body.to_vec(),
        None => sections.push(Section {
            name: Some(section.to_string()),
            lines: body.to_vec(),
        }),
    }
    write_ini(ini, &sections)
}

/// Drop `[<stem>]` when it exists (e.g. last preload manifest disabled).
pub fn remove_ini_section(ini: &Path, section: &str) -> Result<()> {
    let text = fs::read_to_string(ini).unwrap_or_default();
    if text.is_empty() {
        return Ok(());
    }
    let mut sections = parse_ini(&text);
    let before = sections.len();
    sections.retain(|s| {
        s.name
            .as_deref()
            .is_none_or(|n| !n.eq_ignore_ascii_case(section))
    });
    if sections.len() == before {
        return Ok(());
    }
    write_ini(ini, &sections)
}

/// Ensure `[Init] GamesDir/DepotDir` point at the per-game dirs (relative to
/// the ini dir), preserving other `[Init]` keys and everything else.
pub fn ensure_game_init(ini: &Path) -> Result<()> {
    let text = fs::read_to_string(ini).unwrap_or_default();
    let mut sections = parse_ini(&text);
    let owned = [("GamesDir", "runtime"), ("DepotDir", "stage")];
    let init = match find_section(&mut sections, "Init") {
        Some(s) => s,
        None => {
            sections.insert(
                0,
                Section {
                    name: Some("Init".to_string()),
                    lines: Vec::new(),
                },
            );
            find_section(&mut sections, "Init")
                .ok_or_else(|| crate::Error::Manifest("ini Init section missing".into()))?
        }
    };
    for (key, val) in owned {
        let line = format!("{key}={val}");
        match init.lines.iter_mut().find(|l| {
            l.split_once('=')
                .is_some_and(|(k, _)| k.trim().eq_ignore_ascii_case(key))
        }) {
            Some(l) => *l = line,
            None => init.lines.push(line),
        }
    }
    write_ini(ini, &sections)
}

#[derive(sqlx::FromRow)]
struct Row {
    install_dir: Option<String>,
    exe_path: Option<String>,
    detected_exe_path: Option<String>,
    override_exe_path: Option<String>,
    override_api: Option<String>,
    detected_api: Option<String>,
    override_bitness: Option<String>,
    detected_bitness: Option<String>,
}

fn pick<'a>(ovr: &'a Option<String>, det: &'a Option<String>) -> &'a str {
    ovr.as_deref().or(det.as_deref()).unwrap_or("")
}

/// Effective exe: override, then detected, then the store snapshot.
fn pick_exe<'a>(row: &'a Row) -> &'a str {
    row.override_exe_path
        .as_deref()
        .or(row.detected_exe_path.as_deref())
        .or(row.exe_path.as_deref())
        .unwrap_or("")
}

pub fn is_dll(dest: &str) -> bool {
    dest.len() >= 5 && dest[dest.len() - 4..].eq_ignore_ascii_case(".dll")
}

/// Loader list budget: `ini_get_list` joins one key's values with ", " into
/// a 8192-byte buffer (`loaddll` / `includes`). `Type=` is informational and
/// not a loader list, so it is excluded.
pub(crate) const INI_LIST_BUDGET: usize = 8192;

/// Joined `LoadDLL` / `IncludeFile` list lengths for one ini body, computed
/// exactly as the loader joins them (`, `-separated). Shared by
/// `check_ini_budget` and the keep-gate ratchet so the two cannot diverge.
pub(crate) fn list_lens(body: &[String]) -> [usize; 2] {
    ["LoadDLL", "IncludeFile"].map(|key| {
        let mut lens = body.iter().filter_map(|l| {
            let (k, v) = l.split_once('=')?;
            (k == key).then_some(v.len())
        });
        let Some(first) = lens.next() else {
            return 0;
        };
        lens.fold(first, |acc, n| acc + 2 + n)
    })
}

/// Prevalidate the managed-ini list budget before any write (R35).
/// Errors name the budget when a list would overflow the loader buffer.
/// Recovery is uninstall: a rejected pack must leave no enabled residue, so
/// install/enable paths validate a prospective manifest before writing it;
/// if this fires on an existing game, drop the over-budget instance with
/// `tuxgt instance uninstall <game> <instance>` (custom LoadDLL dests are all
/// required, so omit/split cannot shrink them).
pub(crate) fn check_ini_budget(game_id: &str, stem: &str, body: &[String]) -> Result<()> {
    for (key, len) in ["LoadDLL", "IncludeFile"].iter().zip(list_lens(body)) {
        if len >= INI_LIST_BUDGET {
            let budget = INI_LIST_BUDGET;
            return Err(crate::Error::Manifest(format!(
                "{game_id}: managed ini [{stem}] {key} list exceeds {budget}-byte budget ({len} bytes); omit dests or split the pack, or uninstall the over-budget instance (`tuxgt instance uninstall {game_id} <instance>`) to recover"
            )));
        }
    }
    Ok(())
}

/// Per-manifest loader lines (LoadDLL/IncludeFile) for the managed ini.
/// Shared by `prewire_game` and the install-time prospective budget check so
/// a rejected pack never lands an enabled manifest (R35 P1).
pub(crate) fn ini_lines_for(m: &crate::FileManifest) -> Vec<String> {
    // Always-tree: nested non-DLL dests emit one tree mirror per top-level
    // dir however many siblings are omitted — omission is expressed in
    // staging (omitted dests are staging-absent), never in the ini, so
    // toggling cannot grow these lists. Root-level files keep the per-file
    // form; DLLs stay LoadDLL. Prefix dests (E73) are never listed: the
    // install adapter copies them into the prefix and the loader never
    // LoadDLLs them.
    let mut out = Vec::new();
    let mut roots: Vec<&str> = Vec::new();
    for f in m
        .files
        .iter()
        .filter(|f| f.enabled && !crate::install::is_prefix_dest(&f.dest))
    {
        if is_dll(&f.dest) && !crate::download::include_covers(&m.include, &f.dest) {
            out.push(format!(
                "LoadDLL={dest}={inst}/{dest}",
                dest = f.dest,
                inst = m.instance
            ));
        } else if let Some((dir, _)) = f.dest.split_once('/') {
            if !roots.contains(&dir) {
                roots.push(dir);
            }
        } else {
            out.push(format!(
                "IncludeFile={dest}={inst}/{dest}",
                dest = f.dest,
                inst = m.instance
            ));
        }
    }
    for dir in &roots {
        out.push(format!(
            "IncludeFile={dir}/={inst}/{dir}/",
            inst = m.instance
        ));
    }
    out
}

/// Loader lines for every enabled preload manifest of a game, in manifest
/// order. Shared by `prewire_game`, `ensure_ini_budget`, and the keep-gate
/// ratchet (which compares the current body against the prospective one).
pub(crate) fn body_for(manifests: &[crate::FileManifest]) -> Vec<String> {
    let mut body = Vec::new();
    for m in manifests {
        if !m.enabled || m.adapter != "preload" {
            continue;
        }
        body.extend(ini_lines_for(m));
    }
    body
}

/// Prospective budget check: substitute `pending` for its instance (or add it)
/// among the game's manifests and validate the resulting lists before any
/// manifest write. No-op when the game gets no ini section (missing
/// exe/stem) — `prewire_game` would write nothing either — so those installs
/// never false-fail.
pub async fn ensure_ini_budget(
    pool: &SqlitePool,
    data_dir: &Path,
    game_id: &str,
    pending: &crate::FileManifest,
) -> Result<()> {
    crate::game::GameId::parse(game_id)?;
    let row = sqlx::query_as::<_, Row>(
        "SELECT install_dir, exe_path, detected_exe_path, override_exe_path,
                override_api, detected_api, override_bitness, detected_bitness
         FROM games WHERE id = ?",
    )
    .bind(game_id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| crate::Error::UnknownGame(game_id.into()))?;
    let exe = pick_exe(&row);
    if exe.is_empty() {
        return Ok(());
    }
    let mut exe_path = PathBuf::from(exe);
    if !exe_path.is_absolute() {
        if let Some(dir) = row.install_dir.as_deref() {
            exe_path = PathBuf::from(dir).join(&exe_path);
        }
    }
    let stem = exe_path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .filter(|s| !s.is_empty());
    let Some(stem) = stem else {
        return Ok(());
    };
    let mut manifests = game_manifests(data_dir, game_id)?;
    if let Some(slot) = manifests
        .iter_mut()
        .find(|m| m.instance == pending.instance)
    {
        *slot = pending.clone();
    } else {
        manifests.push(pending.clone());
    }
    check_ini_budget(game_id, &stem, &body_for(&manifests))
}
/// Rewrite `[<stem>]` in the per-game ini from GameInfo + enabled preload
/// manifests for all managers including steam. Rows without an exe keep no
/// section. Prewire runs at install/enable/disable/uninstall time; Play never
/// writes the ini.
pub async fn prewire_game(data_dir: &Path, pool: &SqlitePool, game_id: &str) -> Result<()> {
    let gid = crate::game::GameId::parse(game_id)?;
    let gdir = crate::game::game_dir(data_dir, &gid);
    fs::create_dir_all(&gdir)?;
    let ini = managed_ini(&gdir);
    ensure_game_init(&ini)?;
    let row = sqlx::query_as::<_, Row>(
        "SELECT install_dir, exe_path, detected_exe_path, override_exe_path,
                override_api, detected_api, override_bitness, detected_bitness
         FROM games WHERE id = ?",
    )
    .bind(game_id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| crate::Error::UnknownGame(game_id.into()))?;
    let exe = pick_exe(&row);
    if exe.is_empty() {
        return Ok(());
    }
    let mut exe_path = PathBuf::from(exe);
    if !exe_path.is_absolute() {
        if let Some(dir) = row.install_dir.as_deref() {
            exe_path = PathBuf::from(dir).join(&exe_path);
        }
    }
    let stem = exe_path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .filter(|s| !s.is_empty());
    let Some(stem) = stem else {
        return Ok(());
    };
    let mut body = Vec::new();
    let api = pick(&row.override_api, &row.detected_api);
    let bitness = pick(&row.override_bitness, &row.detected_bitness);
    if matches!(api, "dx9" | "dx10" | "dx11" | "dx12") && matches!(bitness, "32" | "64") {
        body.push(format!("Type={api}_{bitness}"));
    }
    body.extend(body_for(&game_manifests(data_dir, game_id)?));
    // R35: the loader joins each list into a fixed 8192-byte buffer and
    // silently truncates overflow, so prevalidate the budget before writing.
    check_ini_budget(game_id, &stem, &body)?;
    if body.is_empty() {
        remove_ini_section(&ini, &stem)?;
    } else {
        set_ini_section(&ini, &stem, &body)?;
    }
    if let Ok(host) = crate::PluginHost::load_with(crate::FIRST_PARTY, data_dir.join("config")) {
        let _ = crate::session::mutate_game(pool, data_dir, &host, game_id, async { Ok(()) }).await;
    }
    Ok(())
}

#[cfg(test)]
mod testing;
#[cfg(test)]
mod tests_0;
#[cfg(test)]
mod tests_1;
#[cfg(test)]
mod tests_2;
