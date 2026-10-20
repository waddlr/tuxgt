use gpui_kit::component::button::ButtonVariants as _;
use gpui_kit::component::{h_flex, ActiveTheme};
use gpui_kit::*;
use tuxgt_core::{short_update_reason, UpdateStatus};

use super::super::super::widgets;
use super::super::super::Shell;

impl Shell {
    /// R32 per-card update affordance from the cached update baseline (E76
    /// self-heal runs before paint). UpToDate stays quiet; Available shows
    /// just an Update button (reinstall with redownload, no note); Unknown
    /// shows one short muted note plus an Update retry; no result yet stays
    /// quiet too — the background check fills in when it lands, with no
    /// in-flow placeholder to shift the card (the old checking note
    /// flickered on every toggle because `refresh_mods` re-ran the check).
    pub(crate) fn update_row(
        &self,
        game_id: &str,
        instance: &str,
        view: Entity<Self>,
        cx: &App,
    ) -> AnyElement {
        let bits = self.mods.get(game_id).and_then(|rows| {
            rows.iter().find(|r| r.instance == instance).map(|r| {
                let marked = self.mod_updates.get(&(game_id.to_string(), r.instance.clone())).is_some_and(|status| {
                    matches!(status, UpdateStatus::Unknown { reason } if reason.contains("pinned version unavailable"))
                });
                (
                    r.pin_of.is_some(),
                    r.pinnable && !r.provenance_source.is_empty(),
                    r.pin_labels.clone(),
                    r.pin_version.clone(),
                    marked || r.pin_version.is_none(),
                )
            })
        });
        if let Some((true, _, _, version, unavailable)) = bits.as_ref() {
            return self.pin_card_note(version.as_deref(), *unavailable, cx);
        }
        let update = self.update_affordance(game_id, instance, view.clone(), cx);
        let (show_pin, names) = match bits {
            Some((false, pin, names, _, _)) => (pin, names),
            _ => (false, Vec::new()),
        };
        if !show_pin && names.is_empty() {
            return update.unwrap_or_else(|| div().into_any_element());
        }
        self.pin_update_cluster(game_id, instance, show_pin, &names, update, view, cx)
    }

    /// Update control for a parent card. `None` is the quiet UpToDate /
    /// still-checking box. A pinned copy never reaches this.
    fn update_affordance(
        &self,
        game_id: &str,
        instance: &str,
        view: Entity<Self>,
        cx: &App,
    ) -> Option<AnyElement> {
        let key = (game_id.to_string(), instance.to_string());
        // E104: poll-fresh Available wins without a GitHub hit — the poll
        // already ran `check_catalog_update`; the card reuses its verdict.
        // Unknown is never poll-fed (stays on the per-card check only).
        // Available states are button-only: no note, just Update.
        if self.catalog_updates.contains(instance) && !self.mod_updates.contains_key(&key) {
            return Some(self.update_button(instance, game_id, instance, view, cx));
        }
        match self.mod_updates.get(&key) {
            Some(UpdateStatus::UpToDate) | None => None,
            Some(UpdateStatus::Available { .. }) => {
                Some(self.update_button(instance, game_id, instance, view, cx))
            }
            Some(UpdateStatus::Unknown { reason }) => {
                // E76: one short muted note — never CLI text, never the
                // game id. Update retries the redownload path, except when
                // the provided payload was cleared: there is nothing to land
                // until Provide files runs again.
                let key = short_update_reason(reason);
                let note = widgets::muted(self.strings.get(key), cx);
                if key == "gui-mod-update-unknown-files" {
                    Some(note.into_any_element())
                } else {
                    Some(self.update_cta(instance, note, game_id, instance, view, cx))
                }
            }
        }
    }

    /// Note + primary Update button row (Unknown keeps its short note).
    /// Available states use `update_button` below (button-only, no note).
    fn update_cta(
        &self,
        inst_id: &str,
        note: impl IntoElement,
        game_bg: &str,
        inst_bg: &str,
        view: Entity<Self>,
        cx: &App,
    ) -> AnyElement {
        let game = game_bg.to_string();
        let inst = inst_bg.to_string();
        h_flex()
            .gap_2()
            .items_center()
            .child(note)
            .child(
                widgets::btn(SharedString::from(format!("update-{inst_id}")), cx)
                    .primary()
                    .child(widgets::blabel(
                        self.strings.get("gui-mod-action-update"),
                        cx,
                    ))
                    .on_click(move |_, _, cx| {
                        view.update(cx, |this, cx| {
                            this.update_mod_ui(&game, &inst, None, false, false, None, None, cx);
                        });
                    }),
            )
            .into_any_element()
    }

    /// Button-only Update CTA for the Available states (no note). Button
    /// mirrors `update_cta` so the Unknown path stays byte-identical.
    fn update_button(
        &self,
        inst_id: &str,
        game_bg: &str,
        inst_bg: &str,
        view: Entity<Self>,
        cx: &App,
    ) -> AnyElement {
        let game = game_bg.to_string();
        let inst = inst_bg.to_string();
        h_flex()
            .gap_2()
            .items_center()
            .child(
                widgets::btn(SharedString::from(format!("update-{inst_id}")), cx)
                    .primary()
                    .child(widgets::blabel(
                        self.strings.get("gui-mod-action-update"),
                        cx,
                    ))
                    .on_click(move |_, _, cx| {
                        view.update(cx, |this, cx| {
                            this.update_mod_ui(&game, &inst, None, false, false, None, None, cx);
                        });
                    }),
            )
            .into_any_element()
    }

    /// E78: the Staging duplicate list is gone (sync pills live on the file
    /// rows now). The per-card force re-sync action stays (`resync_instance`
    /// with force). Always painted so re-sync works even before any stage
    /// rows exist. Idle hairline contrast (E77 rule): outline, fill not the
    /// card face.
    pub(crate) fn stage_section(
        &self,
        game_id: &str,
        instance: &str,
        view: Entity<Self>,
        cx: &App,
    ) -> AnyElement {
        let b = cx.theme();
        let resync_view = view.clone();
        let game_bg = game_id.to_string();
        let inst_bg = instance.to_string();
        let inst_id = instance.to_string();
        h_flex()
            .gap_2()
            .items_center()
            .child(
                widgets::btn(SharedString::from(format!("resync-{inst_id}")), cx)
                    .secondary()
                    .outline()
                    .border_1()
                    .border_color(b.border)
                    .child(widgets::blabel(
                        self.strings.get("gui-mod-action-resync"),
                        cx,
                    ))
                    .on_click(move |_, _, cx| {
                        resync_view.update(cx, |this, cx| {
                            this.resync_instance_ui(&game_bg, &inst_bg, cx);
                        });
                    }),
            )
            .into_any_element()
    }
}
