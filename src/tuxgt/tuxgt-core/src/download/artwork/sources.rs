//! Art source resolution + fingerprinting for the thumb renderer.

use std::fs;
use std::path::{Path, PathBuf};

use super::super::{art_dir, drop_download, fetch_url, game_id_safe};
use super::ArtKind;
use crate::Result;

/// Fingerprint file for one game: `src` beside its thumbs. Its content is
/// the source stamp every render compares against; its (len, mtime) stamp is
/// what the GUI settles each render on.
pub fn fingerprint_file(data_dir: &Path, game_id: &str) -> PathBuf {
    art_dir(data_dir).join(game_id_safe(game_id)).join("src")
}

fn file_stamp(path: &str) -> String {
    match fs::metadata(path) {
        Ok(m) => {
            let mtime = m
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_nanos())
                .unwrap_or(0);
            format!("{}|{}|{mtime}", path, m.len())
        }
        Err(_) => format!("{path}|-"),
    }
}

/// Inputs that bust the thumbs when they change. Local files contribute
/// path+len+mtime; remote art contributes its URL: fetched originals are
/// never revalidated, matching the old fetch-once paint kicks.
pub(crate) fn art_fingerprint(
    cover: Option<&str>,
    header: Option<&str>,
    steam_icon: Option<&Path>,
    hero_url: Option<&str>,
    icon_url: Option<&str>,
) -> String {
    let mut out = String::new();
    for (key, v) in [("cover", cover), ("header", header)] {
        match v {
            Some(s) if s.starts_with("http://") || s.starts_with("https://") => {
                out.push_str(&format!("{key}=url:{s}\n"));
            }
            Some(s) => out.push_str(&format!("{key}=file:{}\n", file_stamp(s))),
            None => out.push_str(&format!("{key}=-\n")),
        }
    }
    match steam_icon {
        Some(p) => out.push_str(&format!(
            "steamicon=file:{}\n",
            file_stamp(&p.to_string_lossy())
        )),
        None => out.push_str("steamicon=-\n"),
    }
    out.push_str(&format!("hero_url={}\n", hero_url.unwrap_or("")));
    out.push_str(&format!("icon_url={}\n", icon_url.unwrap_or("")));
    out.push_str(&render_marker());
    out
}

/// Render-version marker: the per-kind geometry itself, so a change to any
/// thumb size, crop, or format busts every cached thumb instead of relying
/// on a hand-bumped literal.
fn render_marker() -> String {
    let mut out = String::from("render=");
    for kind in ArtKind::ALL {
        let _ = std::fmt::Write::write_fmt(
            &mut out,
            format_args!(
                "{:?}/{:?}/{:?}/{:?};",
                kind.max_box(),
                kind.exact_size(),
                kind.crop_aspect(),
                kind.format(),
            ),
        );
    }
    out.push('\n');
    out
}

/// SteamGridDB hero/icon URLs from cached rows (last non-empty wins). The
/// grid `art_url` stays unused: covers were ever provider art.
pub(crate) fn grid_urls(
    rows: &[(String, String, String)],
    game_id: &str,
) -> (Option<String>, Option<String>) {
    let mut hero = None;
    let mut icon = None;
    for (gid, source, data) in rows {
        if gid != game_id || source != "steamgriddb" {
            continue;
        }
        let Ok(v) = serde_json::from_str::<serde_json::Value>(data) else {
            continue;
        };
        for (slot, key) in [(&mut hero, "hero_url"), (&mut icon, "icon_url")] {
            if let Some(url) = v
                .get(key)
                .and_then(|u| u.as_str())
                .filter(|s| !s.is_empty())
            {
                *slot = Some(url.to_string());
            }
        }
    }
    (hero, icon)
}

pub(crate) async fn fetch_original(
    data_dir: &Path,
    game_id: &str,
    url: &str,
    land: fn(&Path, &str, &Path),
    dest: PathBuf,
) -> Result<PathBuf> {
    let asset = fetch_url(data_dir, url, None, None, None, false).await?;
    land(data_dir, game_id, &asset.file);
    drop_download(data_dir, &asset.key);
    Ok(dest)
}

pub(crate) fn is_http(s: &str) -> bool {
    s.starts_with("http://") || s.starts_with("https://")
}
