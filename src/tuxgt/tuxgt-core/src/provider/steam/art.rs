use std::fs;
use std::path::{Path, PathBuf};

use crate::detect::steam_store;
use crate::game::existing_path;
use crate::provider::GameRecord;

pub(crate) const COVER_NAMES: &[&str] = &[
    "library_600x900.jpg",
    "library_600x900.png",
    "library_capsule.jpg",
    "library_capsule.png",
];
pub(crate) const HEADER_NAMES: &[&str] = &[
    "library_hero.jpg",
    "library_hero.png",
    "library_header.jpg",
    "header.jpg",
    "header.png",
];
/// E107: square librarycache icon for the collapsed sidebar rail.
pub(crate) const ICON_NAMES: &[&str] = &["icon.jpg", "icon.png"];

/// Steam 2024+ stores art in `librarycache/<appid>/<hash>/filename`.
pub(crate) fn first_named(dir: &Path, names: &[&str]) -> Option<PathBuf> {
    for n in names {
        if let Some(p) = existing_path(dir.join(n)) {
            return Some(p);
        }
    }
    let rd = fs::read_dir(dir).ok()?;
    for ent in rd.flatten() {
        let p = ent.path();
        if !p.is_dir() {
            continue;
        }
        for n in names {
            if let Some(f) = existing_path(p.join(n)) {
                return Some(f);
            }
        }
    }
    None
}

pub fn steam_art(steam_root: &Path, app_id: u32) -> (Option<PathBuf>, Option<PathBuf>) {
    let cache = steam_root.join("appcache").join("librarycache");
    let id = app_id.to_string();
    let dir = cache.join(&id);
    let cover = first_named(&dir, COVER_NAMES).or_else(|| {
        [
            cache.join(format!("{id}_library_600x900.jpg")),
            cache.join(format!("{id}_library_600x900.png")),
            cache.join(format!("{id}_library_capsule.jpg")),
        ]
        .into_iter()
        .find_map(existing_path)
    });
    let header = first_named(&dir, HEADER_NAMES).or_else(|| {
        [
            cache.join(format!("{id}_library_hero.jpg")),
            cache.join(format!("{id}_header.jpg")),
        ]
        .into_iter()
        .find_map(existing_path)
    });
    (cover, header)
}

/// E107: square icon for the collapsed sidebar rail, same two shapes
/// `steam_art` uses (`librarycache/<appid>/icon.jpg`, then `<appid>_icon.jpg`).
pub fn steam_icon(steam_root: &Path, app_id: u32) -> Option<PathBuf> {
    let cache = steam_root.join("appcache").join("librarycache");
    let id = app_id.to_string();
    let dir = cache.join(&id);
    first_named(&dir, ICON_NAMES).or_else(|| {
        [
            cache.join(format!("{id}_icon.jpg")),
            cache.join(format!("{id}_icon.png")),
        ]
        .into_iter()
        .find_map(existing_path)
    })
}

/// E107: the GUI has no Steam root on `Shell`, so resolve across all
/// discovered roots.
pub fn steam_icon_for_appid(app_id: u32) -> Option<PathBuf> {
    super::steam_roots()
        .into_iter()
        .find_map(|root| steam_icon(&root, app_id))
}

pub(crate) fn apply_steam_snap(rec: &mut GameRecord, app: &str) {
    let s = steam_store(app);
    if rec.exe_path.is_none() {
        rec.exe_path = s.exe_path;
    }
    rec.prefix_path = s.prefix_path.or(rec.prefix_path.take());
    rec.proton = s.proton.or(rec.proton.take());
    rec.launch_options = s.launch_options.or(rec.launch_options.take());
}

pub(crate) fn shortcut_exe(executable: &str) -> Option<PathBuf> {
    let p = PathBuf::from(strip_quotes(executable));
    if p.is_file() {
        Some(p)
    } else {
        None
    }
}

pub(crate) fn shortcut_dir(start_dir: &str, executable: &str) -> Option<PathBuf> {
    let start = strip_quotes(start_dir);
    if !start.is_empty() {
        let p = PathBuf::from(&start);
        if p.is_dir() {
            return Some(p);
        }
    }
    let exe = strip_quotes(executable);
    let p = PathBuf::from(exe);
    p.parent().map(Path::to_path_buf).filter(|d| d.is_dir())
}

pub(crate) fn shortcut_art(steam_root: &Path, app_id: u32) -> Option<PathBuf> {
    let userdata = steam_root.join("userdata");
    let names = [
        format!("{app_id}p.png"),
        format!("{app_id}p.jpg"),
        format!("{app_id}.png"),
        format!("{app_id}.jpg"),
    ];
    let Ok(users) = std::fs::read_dir(&userdata) else {
        return None;
    };
    for user in users.flatten() {
        let grid = user.path().join("config").join("grid");
        for n in &names {
            if let Some(p) = existing_path(grid.join(n)) {
                return Some(p);
            }
        }
    }
    None
}

pub(crate) fn strip_quotes(s: &str) -> String {
    s.trim().trim_matches('"').to_string()
}

#[cfg(test)]
mod tests {
    use super::steam_icon;
    use std::path::PathBuf;

    fn scratch(tag: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("tuxgt-icon-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        root
    }

    /// E107: the 2024+ shape — `librarycache/<appid>/<hash>/icon.jpg`.
    #[test]
    fn steam_icon_reads_librarycache_dir() {
        let root = scratch("dir");
        let dir = root.join("appcache").join("librarycache").join("814380");
        std::fs::create_dir_all(dir.join("hash")).expect("mkdir");
        let icon = dir.join("hash").join("icon.jpg");
        std::fs::write(&icon, b"x").expect("write");
        assert_eq!(steam_icon(&root, 814380), Some(icon));
        assert_eq!(steam_icon(&root, 999), None);
        let _ = std::fs::remove_dir_all(&root);
    }

    /// E107: the flat fallback shape — `librarycache/<appid>_icon.png`.
    #[test]
    fn steam_icon_reads_flat_fallback() {
        let root = scratch("flat");
        let cache = root.join("appcache").join("librarycache");
        std::fs::create_dir_all(&cache).expect("mkdir");
        let icon = cache.join("814380_icon.png");
        std::fs::write(&icon, b"x").expect("write");
        assert_eq!(steam_icon(&root, 814380), Some(icon));
        let _ = std::fs::remove_dir_all(&root);
    }
}
