//! Default stem for a slot-choice row. The error string lists instances
//! only; the default depends on which op parked the card.

use tuxgt_core::{config_dir, data_dir, list_mods};

use super::super::{Shell, SlotPick};

#[derive(Clone, Copy)]
pub(crate) enum PickKind {
    /// Fresh install: the recipe slot, else `dxgi`.
    Recipe,
    /// Preload → Install: the current proxy stem, else the recipe slot.
    ConvertInstall,
    /// Install → Preload: `<self>`.
    ConvertPreload,
    /// Re-sync: the current proxy stem, else `<self>`.
    Resync,
}

pub(crate) fn pick_slot(
    kind: PickKind,
    current: Option<&str>,
    recipe: Option<&str>,
    remembered: Option<&str>,
) -> String {
    if matches!(kind, PickKind::ConvertInstall | PickKind::ConvertPreload) {
        if let Some(token) = remembered {
            if tuxgt_core::is_self_slot(token) {
                return tuxgt_core::SELF_SLOT.to_string();
            }
            if let Ok(slot) = tuxgt_core::parse_slot(token) {
                return slot.as_str().to_string();
            }
        }
    }
    let proxy = current.filter(|s| tuxgt_core::parse_slot(s).is_ok());
    let recipe = recipe.filter(|s| tuxgt_core::parse_slot(s).is_ok());
    match kind {
        PickKind::ConvertPreload => tuxgt_core::SELF_SLOT.to_string(),
        PickKind::Resync => proxy.unwrap_or(tuxgt_core::SELF_SLOT).to_string(),
        PickKind::ConvertInstall => proxy.or(recipe).unwrap_or("dxgi").to_string(),
        PickKind::Recipe => recipe.unwrap_or("dxgi").to_string(),
    }
}

impl Shell {
    pub(crate) fn picks_for(
        &self,
        game: &str,
        instances: Vec<String>,
        kind: PickKind,
    ) -> Box<[SlotPick]> {
        let catalog = list_mods(&config_dir(), &data_dir()).ok();
        let rows = self.mods.get(game);
        let game_row = self.games.iter().find(|g| g.id == game);
        let arch = game_row.and_then(|g| g.bitness.as_deref());
        let api = game_row.and_then(|g| g.api.as_deref());
        let mut picks: Vec<SlotPick> = instances
            .into_iter()
            .map(|instance| {
                let recipe_mod = catalog
                    .as_ref()
                    .and_then(|list| list.mods.iter().find(|m| m.id == instance));
                let recipe = recipe_mod.and_then(|m| m.slot.clone());
                let row = rows.and_then(|rows| rows.iter().find(|r| r.instance == instance));
                let current = row.map(|r| r.slot.clone());
                let remembered = remembered_default(kind, game, &instance);
                let slot = pick_slot(
                    kind,
                    current.as_deref(),
                    recipe.as_deref(),
                    remembered.as_deref(),
                );
                let stock = row
                    .and_then(|r| super::super::slot_show::stock_menu_label(&r.file_entries))
                    .or_else(|| {
                        recipe_mod.and_then(|m| {
                            super::super::slot_show::recipe_stock_label(
                                &m.mod_type,
                                &m.payload,
                                arch,
                                api,
                            )
                        })
                    })
                    .unwrap_or_default();
                SlotPick {
                    instance,
                    slot,
                    stock,
                }
            })
            .collect();
        dedupe_card_defaults(kind, &mut picks);
        picks.into_boxed_slice()
    }
}

fn remembered_default(kind: PickKind, game: &str, instance: &str) -> Option<String> {
    let adapter = match kind {
        PickKind::ConvertInstall => tuxgt_core::ADAPTER_INSTALL,
        PickKind::ConvertPreload => tuxgt_core::ADAPTER_PRELOAD,
        _ => return None,
    };
    tuxgt_core::remembered_slot(&data_dir(), game, instance, adapter)
}

/// The first row may take its recipe stem. A later row in the same card
/// whose default stem is already taken stays on `<self>` (the GUI paints
/// that mod's filename). Re-sync keeps the stems the rows already show.
pub(crate) fn dedupe_card_defaults(kind: PickKind, picks: &mut [SlotPick]) {
    if !matches!(kind, PickKind::ConvertInstall | PickKind::Recipe) {
        return;
    }
    let mut used = std::collections::BTreeSet::new();
    for pick in picks.iter_mut() {
        if tuxgt_core::is_self_slot(&pick.slot) {
            continue;
        }
        if !used.insert(pick.slot.clone()) {
            pick.slot = tuxgt_core::SELF_SLOT.to_string();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_follow_the_op() {
        assert_eq!(
            pick_slot(PickKind::Recipe, None, Some("d3d12"), None),
            "d3d12"
        );
        assert_eq!(pick_slot(PickKind::Recipe, None, None, None), "dxgi");
        assert_eq!(
            pick_slot(PickKind::ConvertInstall, Some("d3d12"), Some("dxgi"), None),
            "d3d12"
        );
        assert_eq!(
            pick_slot(
                PickKind::ConvertInstall,
                Some("ReShade64.dll"),
                Some("dxgi"),
                None
            ),
            "dxgi"
        );
        assert_eq!(
            pick_slot(
                PickKind::ConvertInstall,
                Some("ReShade64.dll"),
                Some("dxgi"),
                Some("d3d12")
            ),
            "d3d12"
        );
        assert_eq!(
            pick_slot(PickKind::ConvertPreload, Some("dxgi"), Some("dxgi"), None),
            "<self>"
        );
        assert_eq!(
            pick_slot(
                PickKind::ConvertPreload,
                Some("dxgi"),
                Some("dxgi"),
                Some("winmm")
            ),
            "winmm"
        );
        assert_eq!(
            pick_slot(
                PickKind::ConvertPreload,
                Some("dxgi"),
                Some("dxgi"),
                Some("<self>")
            ),
            "<self>"
        );
        assert_eq!(
            pick_slot(PickKind::Resync, Some("winmm"), None, Some("dxgi")),
            "winmm"
        );
        assert_eq!(
            pick_slot(PickKind::Resync, Some("OptiScaler.dll"), Some("dxgi"), None),
            "<self>"
        );
    }

    #[test]
    fn two_install_rows_do_not_share_a_default_stem() {
        let mut picks = vec![
            SlotPick {
                instance: "shade".into(),
                slot: "dxgi".into(),
                stock: "ReShade64.dll".into(),
            },
            SlotPick {
                instance: "opti".into(),
                slot: "dxgi".into(),
                stock: "OptiScaler.dll".into(),
            },
        ];
        dedupe_card_defaults(PickKind::ConvertInstall, &mut picks);
        assert_eq!(picks[0].slot, "dxgi");
        assert_eq!(picks[1].slot, "<self>");
        picks[1].slot = "dxgi".into();
        dedupe_card_defaults(PickKind::Resync, &mut picks);
        assert_eq!(picks[1].slot, "dxgi");
    }
}
