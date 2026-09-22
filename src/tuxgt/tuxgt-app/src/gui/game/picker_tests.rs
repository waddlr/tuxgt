use super::{
    apply_files_toggle, files_accordion_key, files_accordion_open, install_spawn_apply,
    installed_targets, make_win_order, merge_install_queue, move_in_order, order_install_ids,
    picker_rows, picker_targets, section_conflicts, section_rows, ModIds, ModRow, SettingsModsTab,
};
use std::collections::{HashMap, HashSet};
use tuxgt_core::LoadConflict;

use super::{files_preview, shows_effects, FilesPreview};

fn row(instance: &str, mod_type: &str, installed: bool) -> ModRow {
    ModRow {
        instance: instance.into(),
        label: instance.into(),
        mod_type: mod_type.into(),
        official: false,
        adapter: "preload".into(),
        enabled: true,
        files: 1,
        load_order: 0,
        installed,
        slot: String::new(),
        graph: String::new(),
        file_entries: Box::default(),
        env_entries: Box::default(),
        effect_files: Box::default(),
        asset: None,
        payload_present: false,
        ids: ModIds::for_instance(instance, "test-game"),
    }
}

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

/// Moves are section-local, and a non-official row never swaps past the
/// pinned official above it (nor an official past a user row).
#[test]
fn move_in_order_stays_in_section_and_kind() {
    let mut reshade = row("reshade", "reshade", true);
    reshade.official = true;
    let mut fx = row("my-effect", "effect", true);
    fx.load_order = 1;
    let rows = vec![reshade, fx];
    assert_eq!(move_in_order(&rows, "my-effect", -1), None);
    let mut a = row("a", "effect", true);
    a.load_order = 0;
    let mut b = row("b", "effect", true);
    b.load_order = 1;
    let rows = vec![a, b];
    assert_eq!(move_in_order(&rows, "b", -1).unwrap(), ["b", "a"]);
    assert_eq!(move_in_order(&rows, "a", -1), None);
    assert_eq!(move_in_order(&rows, "b", 1), None);
    assert_eq!(move_in_order(&rows, "ghost", 1), None);
}

/// Make-win moves a rival inside its own section only: the id set is
/// unchanged (core rejects anything else) and other sections keep order.
#[test]
fn make_win_orders_group_last_inside_section() {
    let mut a = row("a", "effect", true);
    a.load_order = 0;
    let mut b = row("b", "effect", true);
    b.load_order = 1;
    let mut c = row("c", "texture", true);
    c.load_order = 2;
    let rows = vec![a, b, c];
    assert_eq!(
        make_win_order(&rows, "a", &["a".to_string(), "b".to_string()]).unwrap(),
        ["b", "a", "c"]
    );
    assert_eq!(make_win_order(&rows, "ghost", &["a".to_string()]), None);
}

/// Conflicts resolve per section: a group reaching into another section
/// (the engine winner is decided there, out of Make-win's reach) and a
/// mixed official/user group (the pin leaves no reachable winner) are
/// both unpainted.
#[test]
fn section_conflicts_are_same_kind_and_fully_inside() {
    let mut reshade = row("reshade", "reshade", true);
    reshade.official = true;
    let rows = vec![
        reshade,
        row("my-effect", "effect", true),
        row("renodx", "reshade_addon", true),
        row("my-proxy", "custom", true),
    ];
    let conflict = |dest: &str, ids: &[&str]| LoadConflict {
        dest: dest.into(),
        adapter: "preload".into(),
        instances: ids
            .iter()
            .map(|i| (*i).to_string())
            .collect::<Vec<_>>()
            .into_boxed_slice(),
    };
    let cross = [conflict("dxgi.dll", &["my-effect", "my-proxy"])];
    assert!(section_conflicts(&rows, SettingsModsTab::Reshade, &cross).is_empty());
    // Two ReShade contenders plus a Custom one: the section-local pair is
    // not painted either, because a later section owns the engine win.
    let reach = [conflict(
        "ReShade64.dll",
        &["my-effect", "renodx", "my-proxy"],
    )];
    assert!(section_conflicts(&rows, SettingsModsTab::Reshade, &reach).is_empty());
    let mixed = [conflict("ReShade64.dll", &["reshade", "my-effect"])];
    assert!(section_conflicts(&rows, SettingsModsTab::Reshade, &mixed).is_empty());
    let same = [conflict("ReShade64.dll", &["my-effect", "renodx"])];
    let kept = section_conflicts(&rows, SettingsModsTab::Reshade, &same);
    assert_eq!(kept.len(), 1);
    assert_eq!(kept[0].dest, "ReShade64.dll");
    assert_eq!(kept[0].instances, ["my-effect", "renodx"]);
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
        super::super::source_asset(&url("https://example.com/x/Pack-Addon.7z")),
        Some("Pack-Addon.7z".to_string())
    );
    assert_eq!(super::source_asset(&url("https://example.com/x/")), None);
    assert_eq!(
        super::super::source_asset(&gh("renodx-2077.addon64")),
        Some("renodx-2077.addon64".to_string())
    );
    assert_eq!(super::source_asset(&gh("OptiScaler_*.7z")), None);
    assert_eq!(
        super::super::source_asset(&SourceRef::Local {
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
    let mut official_pack = row("official-addon", "reshade_addon", false);
    official_pack.official = true;
    let rows = vec![
        row("reshade", "reshade", false),
        official_pack,
        row("aaa-addon", "reshade_addon", false),
        row("zzz-reshade", "reshade", false),
        row("my-proxy", "custom", false),
    ];
    // ReShade: Official, then User mods, then User Packs (an official
    // pack is a pack row, like `pack_rows` on Settings).
    assert_eq!(
        picker_targets(&rows, &HashSet::new(), ""),
        [
            "reshade",
            "zzz-reshade",
            "official-addon",
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
        .map(|tab| {
            (
                *tab,
                section_rows(&rows, *tab),
                section_conflicts(&rows, *tab, &[]),
            )
        })
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

/// R70: a picker Install while a spawn is in flight appends; it does
/// not replace the queue or ask the caller to start another spawn.
#[test]
fn merge_install_queue_appends_while_busy() {
    let info = HashMap::new();
    let mut q = vec!["old".into()];
    assert!(merge_install_queue(
        &mut q,
        None,
        &["my-effect".into(), "d3dcompiler-47".into()],
        &info
    ));
    assert_eq!(q, ["my-effect", "d3dcompiler-47"]);

    let mut q = vec!["my-addon".into()];
    assert!(!merge_install_queue(
        &mut q,
        Some("d3dcompiler-47"),
        &[
            "d3dcompiler-47".into(),
            "my-addon".into(),
            "my-effect".into()
        ],
        &info
    ));
    assert_eq!(q, ["my-addon", "my-effect"]);
}

/// R71: my-effect before ReShade in check order still installs ReShade
/// first when both are in the batch. Unchecked requires are not added.
#[test]
fn order_install_ids_selected_requires_first() {
    let mut info = HashMap::new();
    info.insert("my-effect".into(), ("effect".into(), Vec::new()));
    info.insert("reshade".into(), ("reshade".into(), Vec::new()));
    info.insert("my-addon".into(), ("reshade_addon".into(), Vec::new()));
    info.insert("d3dcompiler-47".into(), ("custom".into(), Vec::new()));
    let ids = vec![
        "my-effect".into(),
        "d3dcompiler-47".into(),
        "reshade".into(),
        "my-addon".into(),
    ];
    assert_eq!(
        order_install_ids(&ids, &info),
        ["d3dcompiler-47", "reshade", "my-effect", "my-addon"]
    );
    let only_fx = vec!["my-effect".into()];
    assert_eq!(order_install_ids(&only_fx, &info), ["my-effect"]);
    let mut info = HashMap::new();
    info.insert(
        "addon".into(),
        ("custom".into(), vec!["d3dcompiler-47".into()]),
    );
    info.insert("d3dcompiler-47".into(), ("custom".into(), Vec::new()));
    assert_eq!(
        order_install_ids(&["addon".into(), "d3dcompiler-47".into()], &info),
        ["d3dcompiler-47", "addon"]
    );
}

#[test]
fn install_spawn_apply_after_switch_back() {
    assert_eq!(install_spawn_apply(true, true, false), Some(true));
    assert_eq!(install_spawn_apply(false, true, false), Some(false));
    assert_eq!(install_spawn_apply(false, true, true), None);
    assert_eq!(install_spawn_apply(false, false, false), None);
    assert_eq!(install_spawn_apply(true, false, false), None);
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
