use std::collections::HashSet;

use super::super::{ModRow, SettingsModsTab, Shell};

pub(crate) fn picker_rows(rows: Vec<ModRow>) -> Vec<ModRow> {
    rows.into_iter().filter(|r| !r.installed).collect()
}

/// Picker subheads, in the Settings Mods page order (Official / User mods /
/// User Packs). ReShade includes `User Packs`; the other tabs only see the
/// first two, so their third group is always empty.
pub(crate) const PICKER_SUBHEADS: [&str; 3] = [
    "gui-section-mods-official",
    "gui-section-mods-user",
    "gui-section-mods-packs",
];

/// Subhead one picker row lands under: pack kinds ride the ReShade tab's
/// `User Packs` group (officials first, like `pack_rows` on Settings).
pub(crate) fn picker_subhead(tab: SettingsModsTab, row: &ModRow) -> &'static str {
    if tab == SettingsModsTab::Reshade
        && SettingsModsTab::pack_types().contains(&row.mod_type.as_str())
    {
        "gui-section-mods-packs"
    } else if row.official {
        "gui-section-mods-official"
    } else {
        "gui-section-mods-user"
    }
}

/// One picker section's visible rows grouped by subhead, in the exact order
/// the section paints them. Select Visible walks the same groups, so a batch
/// installs in the order the user saw.
pub(crate) fn picker_groups<'a>(
    rows: &'a [ModRow],
    tab: SettingsModsTab,
    needle: &str,
) -> Vec<(&'static str, Vec<&'a ModRow>)> {
    let section: Vec<&ModRow> = rows
        .iter()
        .filter(|r| SettingsModsTab::for_type(&r.mod_type) == tab)
        .filter(|r| Shell::mod_matches_needle(&r.label, &r.instance, needle))
        .collect();
    PICKER_SUBHEADS
        .iter()
        .filter_map(|key| {
            let sub: Vec<&ModRow> = section
                .iter()
                .copied()
                .filter(|r| picker_subhead(tab, r) == *key)
                .collect();
            (!sub.is_empty()).then_some((*key, sub))
        })
        .collect()
}

/// Picker Select Visible scope: rows of the expanded sections, in paint order
/// (section-major, then the section's subheads and rows).
pub(crate) fn picker_targets(
    rows: &[ModRow],
    collapsed: &HashSet<String>,
    needle: &str,
) -> Vec<String> {
    SettingsModsTab::ALL
        .iter()
        .filter(|tab| !collapsed.contains(tab.pref_id()))
        .flat_map(|tab| picker_groups(rows, *tab, needle))
        .flat_map(|(_, sub)| sub.into_iter().map(|r| r.instance.clone()))
        .collect()
}
