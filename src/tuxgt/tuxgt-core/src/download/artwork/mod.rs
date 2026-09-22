//! Pre-rendered art thumbs: fixed-size images per game, rendered once by a
//! background task after scan and re-read at paint. The GUI never decodes
//! full-res art at runtime; sources resolve in `sources` with the same
//! priority the old paint-time fetchers used (provider cover/header files,
//! fetched originals, Steam icons, SteamGridDB URLs from the metadata cache).

mod render;
mod sources;

pub use render::{bust_game_art_for_appid, render_game_art, touch_game_art};
pub use sources::fingerprint_file as art_fingerprint_file;

use std::path::{Path, PathBuf};

use super::{art_dir, game_id_safe};

/// Thumb kinds. Fixed physical-px sizes (2x css) so paint only GPU-scales.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ArtKind {
    /// Library grid card.
    Grid,
    /// Library list row.
    List,
    /// Expanded sidebar row.
    Side,
    /// Collapsed rail well.
    Rail,
    /// Game-page wash backdrop.
    Hero,
}

impl ArtKind {
    pub const ALL: [ArtKind; 5] = [
        ArtKind::Grid,
        ArtKind::List,
        ArtKind::Side,
        ArtKind::Rail,
        ArtKind::Hero,
    ];

    pub fn file_name(self) -> &'static str {
        match self {
            ArtKind::Grid => "grid.jpg",
            ArtKind::List => "list.jpg",
            ArtKind::Side => "side.png",
            ArtKind::Rail => "rail.png",
            ArtKind::Hero => "hero.jpg",
        }
    }

    /// Max (w, h) box; aspect kept, never upscaled.
    pub fn max_box(self) -> (u32, u32) {
        match self {
            ArtKind::Grid => (400, 600),
            ArtKind::List => (96, 64),
            ArtKind::Side => (40, 56),
            ArtKind::Rail => (64, 64),
            ArtKind::Hero => (1920, 1920),
        }
    }

    /// Exact (w, h) canvas; render resizes to this up or down, so every
    /// upload is byte-identical in size and freed GPU/driver blocks stay
    /// reusable. `None` keeps the fit-in-box behavior below.
    pub fn exact_size(self) -> Option<(u32, u32)> {
        match self {
            ArtKind::Grid | ArtKind::List | ArtKind::Side | ArtKind::Rail => None,
            ArtKind::Hero => Some((1920, 640)),
        }
    }

    /// Center-crop aspect (w, h) for Cover-fit thumbs; `None` keeps the full
    /// frame.
    pub fn crop_aspect(self) -> Option<(u32, u32)> {
        match self {
            ArtKind::Grid => Some((2, 3)),
            ArtKind::List => Some((3, 2)),
            ArtKind::Side => Some((5, 7)),
            ArtKind::Rail => Some((1, 1)),
            ArtKind::Hero => Some((3, 1)),
        }
    }

    pub(crate) fn format(self) -> image::ImageFormat {
        match self {
            ArtKind::Side | ArtKind::Rail => image::ImageFormat::Png,
            ArtKind::Grid | ArtKind::List | ArtKind::Hero => image::ImageFormat::Jpeg,
        }
    }
}

/// Thumb file for one game + kind:
/// `$PREFIX/config/cache/art/<game-id-safe>/<kind file>`.
pub fn thumb_file(data_dir: &Path, game_id: &str, kind: ArtKind) -> PathBuf {
    art_dir(data_dir)
        .join(game_id_safe(game_id))
        .join(kind.file_name())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn thumb_layout_keys_by_game_and_kind() {
        let dir = Path::new("/tmp/x");
        let p = thumb_file(dir, "steam::1", ArtKind::Grid);
        assert!(p.to_string_lossy().contains("steam__1"));
        assert!(p.to_string_lossy().ends_with("grid.jpg"));
        assert_ne!(
            thumb_file(dir, "steam::1", ArtKind::Grid),
            thumb_file(dir, "steam::1", ArtKind::Rail)
        );
    }
}
