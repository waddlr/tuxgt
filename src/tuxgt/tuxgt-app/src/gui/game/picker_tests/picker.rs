use std::collections::HashSet;

use super::super::{
    apply_files_toggle, files_accordion_key, files_accordion_open, files_preview,
    installed_targets, picker_rows, picker_targets, section_rows, shows_effects, FilesPreview,
    SettingsModsTab,
};
use super::row;

/// E90: the picker offers applicable Mods without an Instance on this
/// game — installed rows stay on the tab cards. The type rides the
/// section, not a per-row pill.
#[test]
fn picker_offers_only_not_installed_mods() {
    let rows = vec![
        row("reshade", "reshade", true),
        row("my-addon", "reshade_addon", false),
        row("my-proxy", "custom", false),
    ];
    let picked = picker_rows(rows);
    let ids: Vec<&str> = picked.iter().map(|r| r.instance.as_str()).collect();
    assert_eq!(ids, vec!["my-addon", "my-proxy"]);
}

/// Sections follow the Settings-Mods tab order and pin officials
/// first inside their own section, whatever `load_order` says.
#[test]
fn sections_are_official_first_and_tab_ordered() {
    let mut reshade = row("reshade", "reshade", true);
    reshade.official = true;
    reshade.load_order = 5;
    let mut fx = row("my-effect", "effect", true);
    fx.load_order = 0;
    let mut proxy = row("my-proxy", "custom", true);
    proxy.load_order = 1;
    let rows = vec![proxy.clone(), fx.clone(), reshade.clone()];
    let canon: Vec<String> = SettingsModsTab::ALL
        .iter()
        .flat_map(|t| section_rows(&rows, *t))
        .map(|r| r.instance.clone())
        .collect();
    // OptiScaler (none) | ReShade (official first, then load_order) | Custom.
    assert_eq!(canon, ["reshade", "my-effect", "my-proxy"]);
}

/// Preview buttons: `Effects` only for effect Mods with a recipe list;
/// `Files` only when there is something to list (payload files, or the
/// addon archive name before the first install).
#[test]
fn preview_buttons_follow_row_facts() {
    let mut fx = row("fx", "effect", false);
    fx.effect_files = vec!["A.fx".to_string()].into_boxed_slice();
    assert!(shows_effects(&fx.mod_type, &fx.effect_files));
    assert_eq!(files_preview(&fx.mod_type, false, None), None);
    assert_eq!(
        files_preview(&fx.mod_type, true, None),
        Some(FilesPreview::Payload)
    );
    // Names on a non-effect Mod never paint Effects.
    let mut tex = row("tex", "texture", false);
    tex.effect_files = vec!["A.fx".to_string()].into_boxed_slice();
    assert!(!shows_effects(&tex.mod_type, &tex.effect_files));
    // Addon before the first install: the archive name is the list.
    assert_eq!(
        files_preview("reshade_addon", false, Some("Pack.7z")),
        Some(FilesPreview::Asset("Pack.7z"))
    );
    // Nothing to show for other kinds, or with no derivable name.
    assert_eq!(files_preview("custom", false, Some("x.zip")), None);
    assert_eq!(files_preview("reshade_addon", false, None), None);
}

/// Archive names: a manual URL's last segment, or a github asset that
/// names one file; a glob or a trailing slash yields nothing.
#[test]
fn source_asset_from_recipe_source() {
    use tuxgt_core::SourceRef;
    let url = |u: &str| SourceRef::ManualUrl { url: u.into() };
    let gh = |glob: &str| SourceRef::Github {
        owner: "o".into(),
        repo: "r".into(),
        asset_glob: glob.into(),
        tag: None,
        prerelease: false,
    };
    assert_eq!(
        super::super::super::source_asset(&url("https://example.com/x/Pack-Addon.7z")),
        Some("Pack-Addon.7z".to_string())
    );
    assert_eq!(
        super::super::source_asset(&url("https://example.com/x/")),
        None
    );
    assert_eq!(
        super::super::super::source_asset(&gh("renodx-2077.addon64")),
        Some("renodx-2077.addon64".to_string())
    );
    assert_eq!(super::super::source_asset(&gh("OptiScaler_*.7z")), None);
    assert_eq!(
        super::super::super::source_asset(&SourceRef::Local {
            path: "/tmp/x".into()
        }),
        None
    );
}

/// Select Visible scope: collapsed sections are excluded, the filter still
/// applies, and the ids come in the order the picker paints (section
/// order, then that section's subheads), which is the batch install order.
#[test]
fn picker_targets_skip_collapsed_sections() {
    let mut official_reshade = row("reshade", "reshade", false);
    official_reshade.official = true;
    let mut official_pack = row("official-addon", "reshade_addon", false);
    official_pack.official = true;
    let rows = vec![
        official_pack,
        official_reshade,
        row("aaa-addon", "reshade_addon", false),
        row("zzz-reshade", "reshade", false),
        row("my-proxy", "custom", false),
    ];
    // ReShade: Official (the stock mod and an official pack), then User
    // mods, then User Packs.
    assert_eq!(
        picker_targets(&rows, &HashSet::new(), ""),
        [
            "reshade",
            "official-addon",
            "zzz-reshade",
            "aaa-addon",
            "my-proxy"
        ]
    );
    let collapsed: HashSet<String> = ["reshade".to_string()].into_iter().collect();
    assert_eq!(picker_targets(&rows, &collapsed, ""), ["my-proxy"]);
    assert_eq!(
        picker_targets(&rows, &HashSet::new(), "addon"),
        ["official-addon", "aaa-addon"]
    );
}

/// A not-ready `provided` Mod is offered like any other not-installed
/// Mod and sits inside Select Visible scope: checking it is what makes
/// the install ask for its files. An installed row stays out whatever
/// its provided status says.
#[test]
fn picker_offers_not_ready_provided_mods() {
    let needs_files = super::super::super::ProvidedRow {
        present: Vec::new(),
        missing: vec!["sl.common.dll".to_string()],
        ambiguous: Vec::new(),
        ready: false,
        note: String::new(),
    };
    let mut pending = row("nvidia-streamline", "custom", false);
    pending.provided = Some(needs_files.clone());
    let mut on_game = row("nvngx-dlssnr", "custom", true);
    on_game.provided = Some(needs_files);
    let rows = vec![on_game, pending];
    let picked = picker_rows(rows);
    assert_eq!(
        picked
            .iter()
            .map(|r| r.instance.as_str())
            .collect::<Vec<_>>(),
        ["nvidia-streamline"]
    );
    assert_eq!(
        picker_targets(&picked, &HashSet::new(), ""),
        ["nvidia-streamline"]
    );
}

/// Installed Uninstall-visible scope: collapsed sections are excluded, ids
/// stay in paint (section-major) order. Collapsing everything empties the
/// scope but the tab is not empty — collapsed headers still paint.
#[test]
fn installed_targets_skip_collapsed_sections() {
    let mut official_pack = row("official-addon", "reshade_addon", true);
    official_pack.official = true;
    let rows = vec![
        official_pack,
        row("aaa-addon", "effect", true),
        row("my-proxy", "custom", true),
    ];
    let sections = SettingsModsTab::ALL
        .iter()
        .map(|tab| (*tab, section_rows(&rows, *tab)))
        .collect::<Vec<_>>();
    assert_eq!(
        installed_targets(&sections, &HashSet::new()),
        ["official-addon", "aaa-addon", "my-proxy"]
    );
    let collapsed: HashSet<String> = ["reshade".to_string()].into_iter().collect();
    assert_eq!(installed_targets(&sections, &collapsed), ["my-proxy"]);
    let custom: HashSet<String> = ["custom".to_string()].into_iter().collect();
    assert_eq!(
        installed_targets(&sections, &custom),
        ["official-addon", "aaa-addon"]
    );
    let all: HashSet<String> = ["optiscaler", "reshade", "custom"]
        .into_iter()
        .map(str::to_string)
        .collect();
    assert!(installed_targets(&sections, &all).is_empty());
}

/// E90: the dest Accordion starts collapsed and toggles per card key —
/// the kit only notifies open indices, Shell owns the set.
#[test]
fn files_accordion_toggle_is_per_card_key() {
    let mut open = HashSet::new();
    let key = files_accordion_key("steam/1", "dxgi");
    assert!(!files_accordion_open(&open, &key));
    apply_files_toggle(&mut open, key.clone(), &[0]);
    assert!(files_accordion_open(&open, &key));
    // A same-named instance on another game stays collapsed.
    let other = files_accordion_key("heroic/2", "dxgi");
    assert!(!files_accordion_open(&open, &other));
    apply_files_toggle(&mut open, key.clone(), &[]);
    assert!(!files_accordion_open(&open, &key));
}
