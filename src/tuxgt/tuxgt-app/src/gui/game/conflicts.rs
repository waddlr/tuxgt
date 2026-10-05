use std::collections::HashMap;

use tuxgt_core::LoadConflict;

use super::super::{ModRow, SettingsModsTab};

/// Why this card cannot win `dest` by reordering: a contender sits in
/// another section (the engine winner is decided there), or the group
/// mixes official and user cards (the official pin leaves no reachable
/// winner). A `Reachable` group is won with `make_win_ui`.
pub(crate) enum Reachability {
    Reachable,
    CrossSection,
    PinnedKind,
}

/// One contested dest from one card's point of view: the group in load
/// order (winner last), plus the rivals split by whether they load
/// before (`wins_over`) or after (`loses_to`) this card. Labels are for
/// paint; `group` keeps the ids for `make_win_ui`.
pub(crate) struct DestConflict {
    pub dest: String,
    pub group: Vec<String>,
    pub wins_over: Vec<String>,
    pub loses_to: Vec<String>,
    pub winning: bool,
    pub reachable: Reachability,
}

/// Every group claiming one of `instance`'s dests, in group order.
/// Empty when the card is uncontested or unknown: conflicts only ever
/// name enabled manifests, so a disabled card marks nothing.
pub(crate) fn conflicts_for(
    rows: &[ModRow],
    conflicts: &[LoadConflict],
    instance: &str,
) -> Vec<DestConflict> {
    let Some(mine) = rows.iter().find(|r| r.instance == instance) else {
        return Vec::new();
    };
    let labels: HashMap<&str, &str> = rows
        .iter()
        .map(|r| (r.instance.as_str(), r.label.as_str()))
        .collect();
    let my_tab = SettingsModsTab::for_type(&mine.mod_type);
    let same_section = |id: &str| {
        rows.iter()
            .find(|r| r.instance == id)
            .is_some_and(|r| SettingsModsTab::for_type(&r.mod_type) == my_tab)
    };
    let same_kind = |id: &str| {
        rows.iter()
            .find(|r| r.instance == id)
            .is_some_and(|r| r.official == mine.official)
    };
    conflicts
        .iter()
        .filter(|c| c.adapter == mine.adapter)
        .filter_map(|c| {
            let pos = c.instances.iter().position(|i| i == instance)?;
            let reachable = if c.instances.iter().any(|i| !same_section(i)) {
                Reachability::CrossSection
            } else if c.instances.iter().any(|i| !same_kind(i)) {
                Reachability::PinnedKind
            } else {
                Reachability::Reachable
            };
            let label = |id: &str| labels.get(id).copied().unwrap_or(id).to_string();
            Some(DestConflict {
                dest: c.dest.clone(),
                group: c.instances.to_vec(),
                wins_over: c.instances[..pos].iter().map(|i| label(i)).collect(),
                loses_to: c.instances[pos + 1..].iter().map(|i| label(i)).collect(),
                winning: pos + 1 == c.instances.len(),
                reachable,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::super::ModIds;
    use super::*;

    fn row(instance: &str, mod_type: &str, label: &str) -> ModRow {
        ModRow {
            instance: instance.into(),
            label: label.into(),
            mod_type: mod_type.into(),
            official: false,
            adapter: "preload".into(),
            enabled: true,
            files: 1,
            load_order: 0,
            installed: true,
            slot: String::new(),
            slot_capable: false,
            graph: String::new(),
            file_entries: Box::default(),
            env_entries: Box::default(),
            effect_files: Box::default(),
            asset: None,
            payload_present: false,
            ids: ModIds::for_instance(instance, "test-game"),
        }
    }

    fn conflict(dest: &str, adapter: &str, ids: &[&str]) -> LoadConflict {
        LoadConflict {
            dest: dest.into(),
            adapter: adapter.into(),
            instances: ids
                .iter()
                .map(|i| (*i).to_string())
                .collect::<Vec<_>>()
                .into_boxed_slice(),
        }
    }

    /// Rivals split around the card's group position, in labels; the
    /// last loader wins and keeps the ids for `make_win_ui`.
    #[test]
    fn splits_rivals_around_position() {
        let rows = vec![
            row("a", "effect", "A"),
            row("b", "effect", "B"),
            row("c", "effect", "C"),
        ];
        let cs = [conflict("dxgi.dll", "preload", &["a", "b", "c"])];
        let mid = &conflicts_for(&rows, &cs, "b")[0];
        assert_eq!(mid.dest, "dxgi.dll");
        assert_eq!(mid.group, ["a", "b", "c"]);
        assert_eq!(mid.wins_over, ["A"]);
        assert_eq!(mid.loses_to, ["C"]);
        assert!(!mid.winning);
        assert!(matches!(mid.reachable, Reachability::Reachable));
        let last = &conflicts_for(&rows, &cs, "c")[0];
        assert!(last.winning);
        assert_eq!(last.wins_over, ["A", "B"]);
        assert!(last.loses_to.is_empty());
    }

    /// A group reaching into another section, or mixing official and
    /// user cards under the pin, names its reason; same-section
    /// same-kind groups (officials included) stay reachable.
    #[test]
    fn reachability_names_cross_section_and_pinned_kind() {
        let mut reshade = row("reshade", "reshade", "ReShade");
        reshade.official = true;
        let mut stock = row("stock-fx", "effect", "Stock FX");
        stock.official = true;
        let rows = vec![
            reshade,
            stock,
            row("fx", "effect", "FX"),
            row("proxy", "custom", "Proxy"),
        ];
        let cross = [conflict("dxgi.dll", "preload", &["fx", "proxy"])];
        assert!(matches!(
            conflicts_for(&rows, &cross, "fx")[0].reachable,
            Reachability::CrossSection
        ));
        let mixed = [conflict("dxgi.dll", "preload", &["reshade", "fx"])];
        assert!(matches!(
            conflicts_for(&rows, &mixed, "fx")[0].reachable,
            Reachability::PinnedKind
        ));
        let officials = [conflict("dxgi.dll", "preload", &["reshade", "stock-fx"])];
        assert!(matches!(
            conflicts_for(&rows, &officials, "reshade")[0].reachable,
            Reachability::Reachable
        ));
    }

    /// Other adapters are never this card's conflict; an unknown card
    /// marks nothing; an unknown group member falls back to its id and
    /// conservatively blocks the win.
    #[test]
    fn skips_other_adapters_and_unknown_cards() {
        let rows = vec![row("a", "effect", "A"), row("b", "effect", "B")];
        let other = [conflict("dxgi.dll", "install", &["a", "b"])];
        assert!(conflicts_for(&rows, &other, "a").is_empty());
        let cs = [conflict("dxgi.dll", "preload", &["a", "b"])];
        assert!(conflicts_for(&rows, &cs, "ghost").is_empty());
        let haunted = [conflict("dxgi.dll", "preload", &["a", "ghost"])];
        let m = &conflicts_for(&rows, &haunted, "a")[0];
        assert_eq!(m.loses_to, ["ghost"]);
        assert!(matches!(m.reachable, Reachability::CrossSection));
    }
}
