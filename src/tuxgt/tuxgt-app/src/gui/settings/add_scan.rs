use gpui_kit::component::button::ButtonVariants as _;
use gpui_kit::component::{h_flex, WindowExt as _};
use gpui_kit::*;
use std::path::PathBuf;

use super::*;
use tuxgt_core::ReshadePackageKind;

use super::super::widgets;
use super::super::{InstanceRow, SettingsModsTab, Shell};

pub(crate) fn extras_row_visible(
    filter: super::ExtrasKindFilter,
    p: &tuxgt_core::ReshadePackage,
    needle: &str,
) -> bool {
    (filter == super::ExtrasKindFilter::All
        || (filter == super::ExtrasKindFilter::Effects && p.kind == ReshadePackageKind::Effect)
        || (filter == super::ExtrasKindFilter::Addons && p.kind == ReshadePackageKind::Addon))
        && (needle.is_empty()
            || p.name.to_lowercase().contains(needle)
            || p.description.to_lowercase().contains(needle))
}

/// Catalog rows for one inner tab and official flag, in catalog order.
/// Official cards lead; user rows follow in their own section.
pub(crate) fn tab_rows(
    rows: &[InstanceRow],
    tab: SettingsModsTab,
    official: bool,
) -> impl Iterator<Item = &InstanceRow> {
    let types = tab.types();
    rows.iter()
        .filter(move |i| i.official == official && types.contains(&i.mod_type.as_str()))
}
/// Unified ReShade pack rows (addon/effect/texture), official cards first.
pub(crate) fn pack_rows(rows: &[InstanceRow]) -> impl Iterator<Item = &InstanceRow> {
    let packs = SettingsModsTab::pack_types();
    rows.iter()
        .filter(move |i| i.official && packs.contains(&i.mod_type.as_str()))
        .chain(
            rows.iter()
                .filter(move |i| !i.official && packs.contains(&i.mod_type.as_str())),
        )
}

/// One-scan variant of the pack prefill: infer from the already-scanned
/// `effect` rows, then re-key dests in place when the inference lands on
/// `reshade_addon`. Today that fixup is a no-op — `type_dest_for` returns
/// `src` unchanged for both `effect` and `reshade_addon` (no official
/// effect/addon row ships drop globs) — but it keeps the single-scan path
/// honest if either type ever gains a dest rewrite.
pub(crate) fn scan_once_form(
    path: PathBuf,
    mut scanned: Vec<tuxgt_core::PackageFile>,
    password: Option<String>,
) -> PickedForm {
    let addon = scanned.iter().any(|f| {
        let lower = f.src.to_ascii_lowercase();
        lower.ends_with(".addon64") || lower.ends_with(".addon")
    });
    let inferred = if addon { "reshade_addon" } else { "effect" };
    if addon {
        for f in &mut scanned {
            f.dest = f.src.clone();
        }
    }
    form_data(path, scanned, inferred, password)
}
/// Lock 8: one Add button, no extension filter. Mixed file+dir selection is
/// legal only when the platform allows it; otherwise a File | Folder popover
/// picks the kind first. Empty `mod_type` (the single pack entry) infers the
/// type from the scanned srcs; recipe TOML still honors Provides via tab jump.
pub(crate) fn pick_add_pack(view: Entity<Shell>, window: &mut Window, cx: &mut App) {
    pick_add(view, "", window, cx);
}
pub(crate) fn pick_add(
    view: Entity<Shell>,
    mod_type: &'static str,
    window: &mut Window,
    cx: &mut App,
) {
    let archive_password_input = view.read(cx).archive_password_input.clone();
    archive_password_input.update(cx, |input, cx| {
        input.set_value(String::new(), window, cx);
    });
    if cx.can_select_mixed_files_and_dirs() {
        pick_package_with(view, mod_type, true, true, "gui-prompt-add-package", cx);
        return;
    }
    let title = view.read(cx).strings.get("gui-prompt-add-package");
    let file_label = view.read(cx).strings.get("gui-action-pick-file");
    let folder_label = view.read(cx).strings.get("gui-action-pick-folder");
    window.open_dialog(cx, move |dialog, _, cx| {
        dialog.title(title.clone()).child(
            h_flex()
                .gap_2()
                .child(
                    widgets::btn("add-pick-file", cx)
                        .primary()
                        .child(widgets::blabel(file_label.clone(), cx))
                        .on_click({
                            let view = view.clone();
                            move |_, window, cx| {
                                window.close_dialog(cx);
                                pick_package_with(
                                    view.clone(),
                                    mod_type,
                                    true,
                                    false,
                                    "gui-prompt-add-instance",
                                    cx,
                                );
                            }
                        }),
                )
                .child(
                    widgets::btn("add-pick-folder", cx)
                        .secondary()
                        .child(widgets::blabel(folder_label.clone(), cx))
                        .on_click({
                            let view = view.clone();
                            move |_, window, cx| {
                                window.close_dialog(cx);
                                pick_package_with(
                                    view.clone(),
                                    mod_type,
                                    false,
                                    true,
                                    "gui-prompt-add-folder",
                                    cx,
                                );
                            }
                        }),
                ),
        )
    });
}

/// Label half: `suggest_family_title` owns the human title; the matcher takes
/// the asset NAME and strips the vendor prefix internally so the raw stem
/// (`renodx-cp2077`) never reaches token comparison.
/// HDR auto-match: every stem token must EXACT-match a token of exactly one
/// library game (case-insensitive). One concession for fused stems:
/// a leading-alpha/trailing-digit token (`cp2077`) also matches when either
/// half (len >= 3, so `cp` never counts but `2077` does) exact-matches a game
/// token. Zero or several hits stay unchecked for the user.
pub(crate) fn match_family_game(
    asset: &str,
    vendor: &str,
    games: &[(String, String)],
) -> Option<String> {
    let prefix = format!("{}-", vendor.to_lowercase());
    let stem = std::path::Path::new(asset)
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| asset.to_string())
        .to_ascii_lowercase();
    let stem = stem.strip_prefix(&prefix).unwrap_or(&stem);
    let stem_toks: Vec<&str> = stem
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|s| !s.is_empty())
        .collect();
    if stem_toks.is_empty() {
        return None;
    }
    let mut hits = Vec::new();
    for (id, name) in games {
        let lowered = name.to_ascii_lowercase();
        let name_toks: Vec<&str> = lowered
            .split(|c: char| !c.is_ascii_alphanumeric())
            .filter(|s| !s.is_empty())
            .collect();
        if name_toks.is_empty() {
            continue;
        }
        let ok = stem_toks.iter().all(|s| token_hits(s, &name_toks));
        if ok {
            hits.push(id.clone());
        }
    }
    if hits.len() == 1 {
        hits.into_iter().next()
    } else {
        None
    }
}

/// One stem token against a game's token set: whole-token equality, else the
/// fused alpha/digit halves (each len >= 3).
pub(crate) fn token_hits(stem: &str, name_toks: &[&str]) -> bool {
    if name_toks.contains(&stem) {
        return true;
    }
    let bytes = stem.as_bytes();
    let mut i = 0;
    while i < bytes.len() && bytes[i].is_ascii_alphabetic() {
        i += 1;
    }
    if i == 0 || i == bytes.len() {
        return false;
    }
    let (alpha, digits) = stem.split_at(i);
    if !digits.bytes().all(|b| b.is_ascii_digit()) {
        return false;
    }
    (alpha.len() >= 3 && name_toks.contains(&alpha))
        || (digits.len() >= 3 && name_toks.contains(&digits))
}

pub(crate) fn suggest_family_title(asset: &str, family_label: &str) -> String {
    let stem = std::path::Path::new(asset)
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| asset.to_string());
    // Stems are mixed-case (Luma-NieR_Automata.zip); fold before stripping
    // so the lowercase family prefix actually matches.
    let prefix = format!("{}-", family_label.to_lowercase());
    let stem = stem
        .to_ascii_lowercase()
        .strip_prefix(&prefix)
        .map(str::to_string)
        .unwrap_or_else(|| stem.to_ascii_lowercase());
    stem.replace(['-', '_'], " ").trim().to_string()
}

pub(crate) fn suggest_id(stem: &str) -> String {
    let mut out = String::new();
    for c in stem.to_ascii_lowercase().chars() {
        if c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' {
            out.push(c);
        } else if !out.is_empty() && !out.ends_with('-') {
            out.push('-');
        }
    }
    let out: String = out.trim_matches('-').chars().take(32).collect();
    match out.chars().next() {
        Some(c) if c.is_ascii_lowercase() => out,
        _ => format!("pkg-{out}").chars().take(32).collect(),
    }
}
#[cfg(test)]
mod tests {

    use super::super::{add_form_path, InstanceIds, InstanceRow, SettingsModsTab};
    use super::{match_family_game, pack_rows, tab_rows};

    fn row(id: &str, mod_type: &str, official: bool) -> InstanceRow {
        InstanceRow {
            id: id.into(),
            label: id.into(),
            mod_type: mod_type.into(),
            source: "local".into(),
            official,
            enabled: true,
            effect_files: Box::default(),
            asset: None,
            payload_present: false,
            ids: InstanceIds::for_id(id),
        }
    }

    fn catalog() -> Vec<InstanceRow> {
        vec![
            row("reshade", "reshade", true),
            row("my-reshade", "reshade", false),
            row("optiscaler", "optiscaler", true),
            row("opti-fork", "optiscaler", false),
            row("my-addon", "reshade_addon", false),
            row("fx", "effect", false),
            row("tex", "texture", false),
            row("d3dcompiler-47", "custom", true),
            row("my-dll", "custom", false),
        ]
    }

    #[test]
    fn tab_rows_split_official_and_user() {
        let rows = catalog();
        let ids = |tab, official: bool| {
            tab_rows(&rows, tab, official)
                .map(|r| r.id.clone())
                .collect::<Vec<_>>()
        };
        assert_eq!(ids(SettingsModsTab::Optiscaler, true), vec!["optiscaler"]);
        assert_eq!(ids(SettingsModsTab::Optiscaler, false), vec!["opti-fork"]);
        assert_eq!(ids(SettingsModsTab::Reshade, true), vec!["reshade"]);
        assert_eq!(
            ids(SettingsModsTab::Reshade, false),
            vec!["my-reshade", "my-addon", "fx", "tex"]
        );
        assert_eq!(ids(SettingsModsTab::Custom, true), vec!["d3dcompiler-47"]);
        assert_eq!(ids(SettingsModsTab::Custom, false), vec!["my-dll"]);
    }

    #[test]
    fn match_family_game_needs_vendor_strip_and_exact_tokens() {
        let games: Vec<(String, String)> = vec![
            ("steam::1".into(), "Cyberpunk 2077".into()),
            ("steam::2".into(), "Doom Eternal".into()),
            ("steam::3".into(), "Doom 2016".into()),
        ];
        assert_eq!(
            match_family_game("renodx-cp2077.addon64", "RenoDX", &games).as_deref(),
            Some("steam::1")
        );
        assert_eq!(
            match_family_game("Luma-Doom_Eternal.zip", "Luma", &games).as_deref(),
            Some("steam::2")
        );
        assert_eq!(
            match_family_game("Luma-NieR_Automata.zip", "Luma", &games),
            None
        );
        assert_eq!(match_family_game("renodx-doom.zip", "RenoDX", &games), None);
    }

    #[test]
    fn pack_rows_unify_kinds_official_first() {
        let rows = catalog();
        let ids: Vec<_> = pack_rows(&rows).map(|r| r.id.clone()).collect();
        assert_eq!(ids, vec!["my-addon", "fx", "tex"]);
    }
    #[test]
    fn add_form_path_keeps_archive_over_classify_temp() {
        use std::path::PathBuf;
        let archive = PathBuf::from("/tmp/Photorealistic Movie Graphics 1.1-1660338428.zip");
        let temp = PathBuf::from("/tmp/tuxgt-e88-classify-123-456");
        assert_eq!(add_form_path(&archive, &temp, &[temp.clone()]), archive);
        assert_eq!(add_form_path(&archive, &temp, &[]), temp);
    }
}

#[cfg(test)]
mod family_title_tests {
    use super::suggest_family_title;

    #[test]
    fn strips_lowercase_family_prefix_from_mixed_case_stem() {
        // Live Luma stems are mixed-case; the fold must happen before the
        // strip or the prefix never matches.
        assert_eq!(
            suggest_family_title("Luma-NieR_Automata.zip", "Luma"),
            "nier automata"
        );
        assert_eq!(
            suggest_family_title("renodx-cp2077.addon64", "RenoDX"),
            "cp2077"
        );
        assert_eq!(
            suggest_family_title("Custom_Build.zip", "Luma"),
            "custom build"
        );
    }
}
