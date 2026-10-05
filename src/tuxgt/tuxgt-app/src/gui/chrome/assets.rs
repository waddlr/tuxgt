use std::path::PathBuf;

use gpui_kit::*;
use tuxgt_core::data_dir;

pub(crate) const ICON_PNG: &[u8] = include_bytes!("../../../assets/icon.png");

gpui_kit::assets::icon_assets!(
    ExtraIcons,
    [
        Boxes,
        Trash,
        Link,
        Mop,
        FilePenLine,
        ArrowUpToLine,
        ArrowDownToLine
    ]
);

/// The kit's default icon bundle plus Lucide `boxes`, `trash`, `link`, `mop`,
/// `file-pen-line`, `arrow-up-to-line` and `arrow-down-to-line`.
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

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::path::PathBuf;

    use gpui_kit::AssetSource;

    // Explicit imports: `use super::*` would re-import `gpui_kit::test`,
    // which breaks `#[test]` expansion ("recursion limit reached").
    use super::Assets;

    /// Every catalog icon the GUI names must resolve through `Assets`.
    /// An unregistered catalog icon compiles fine and paints nothing —
    /// no error, no log — so this test scans the crate's own source for
    /// catalog-enum variant uses and fails the build when one is unserved.
    /// Fix by registering the variant in `icon_assets!` above.
    #[test]
    fn extra_icons_cover_catalog_usages() {
        let mut sources = Vec::new();
        let mut dirs = vec![PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src")];
        while let Some(dir) = dirs.pop() {
            let entries =
                std::fs::read_dir(&dir).unwrap_or_else(|e| panic!("read {}: {e}", dir.display()));
            for entry in entries {
                let path = entry
                    .unwrap_or_else(|e| panic!("read entry in {}: {e}", dir.display()))
                    .path();
                if path.is_dir() {
                    dirs.push(path);
                } else if path.extension().and_then(|e| e.to_str()) == Some("rs") {
                    sources.push(
                        std::fs::read_to_string(&path)
                            .unwrap_or_else(|e| panic!("read {}: {e}", path.display())),
                    );
                }
            }
        }
        // Aliases first (`assets::IconName as FullIconName`), so a file
        // that renames the import is still covered; the full path itself
        // is a pseudo-alias for unaliased uses.
        let mut aliases = BTreeSet::from(["assets::IconName".to_string()]);
        for text in &sources {
            for (at, hit) in text.match_indices("assets::IconName as ") {
                let alias: String = text[at + hit.len()..]
                    .chars()
                    .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                    .collect();
                if !alias.is_empty() {
                    aliases.insert(alias);
                }
            }
        }
        let mut names = BTreeSet::new();
        for text in &sources {
            for alias in &aliases {
                let marker = format!("{alias}::");
                for (at, hit) in text.match_indices(marker.as_str()) {
                    let name: String = text[at + hit.len()..]
                        .chars()
                        .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                        .collect();
                    // The catalog const, not a variant use.
                    if !name.is_empty() && name != "ALL" {
                        names.insert(name);
                    }
                }
            }
        }
        assert!(
            !names.is_empty(),
            "scanner found no catalog icon uses — import form changed?"
        );
        let assets = Assets;
        for name in &names {
            // Full path, so this lookup is not itself scanned as a use.
            let variant = gpui_kit::assets::IconName::ALL
                .iter()
                .find(|v| format!("{v:?}") == *name)
                .unwrap_or_else(|| panic!("unknown icon variant: {name}"));
            let path = variant.path();
            let served = assets
                .load(path.as_ref())
                .unwrap_or_else(|e| panic!("asset source error for {path}: {e:?}"));
            assert!(
                served.is_some(),
                "icon {name} ({path}) is used but not served; register it in icon_assets!"
            );
        }
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
