use std::path::PathBuf;

use gpui_kit::*;
use tuxgt_core::data_dir;

pub(crate) const ICON_PNG: &[u8] = include_bytes!("../../../assets/icon.png");

gpui_kit::assets::icon_assets!(ExtraIcons, [Boxes, Trash, Link, Mop, FilePenLine]);

/// The kit's default icon bundle plus Lucide `boxes`, `trash`, `link`, `mop` and `file-pen-line`.
/// Embedding the whole catalog would add 7 MB.
pub(crate) struct Assets;

impl AssetSource for Assets {
    fn load(&self, path: &str) -> Result<Option<std::borrow::Cow<'static, [u8]>>> {
        match ExtraIcons.load(path)? {
            Some(bytes) => Ok(Some(bytes)),
            None => gpui_kit::assets::Assets.load(path),
        }
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        let mut out = ExtraIcons.list(path)?;
        out.extend(gpui_kit::assets::Assets.list(path)?);
        Ok(out)
    }
}
pub(crate) fn icon_file() -> PathBuf {
    let share = data_dir().join("share/icons/hicolor/64x64/apps/tuxgt.png");
    if share.is_file() {
        return share;
    }
    let legacy = data_dir().join("share").join("tuxgt").join("icon.png");
    if legacy.is_file() {
        return legacy;
    }
    let p = data_dir()
        .join("config")
        .join("cache")
        .join("tuxgt-icon.png");
    if !p.is_file() {
        let _ = std::fs::create_dir_all(p.parent().unwrap_or(std::path::Path::new(".")));
        let _ = std::fs::write(&p, ICON_PNG);
    }
    p
}
