use gpui_kit::component::menu::PopupMenuItem;
use gpui_kit::*;

use super::super::widgets;
use super::super::{card_label, GpuPref, Shell};

impl Shell {
    /// `gui.settings-gpu`: first-window GPU choice on General, under
    /// Tray. The system default, software rendering, plus every
    /// detected host GPU. Adapter selection belongs to the window that
    /// creates it, so a pick persists and applies at the next start rather
    /// than switching a live window.
    pub(crate) fn gpu_box(&self, view: Entity<Self>, cx: &App) -> impl IntoElement {
        let current = self.prefs.gpu_pref();
        let system_label = self.strings.get("gui-gpu-system");
        let cpu_label = self.strings.get("gui-gpu-cpu");
        let label = match current {
            GpuPref::SystemDefault => system_label.clone(),
            GpuPref::Cpu => cpu_label.clone(),
            GpuPref::Card(id) => card_label(id, &self.host_gpus),
        };
        let cards = self.host_gpus.to_vec();
        widgets::section_card("gpu", cx)
            .child(widgets::section_title(
                self.strings.get("gui-section-gpu"),
                cx,
            ))
            .child(widgets::muted(self.strings.get("gui-note-gpu"), cx))
            .child(widgets::labeled_row(
                "gpu-pick",
                self.strings.get("gui-label-gpu"),
                None,
                widgets::value_btn(
                    "gpu-pick-btn",
                    label,
                    {
                        let view = view.clone();
                        move |menu, _, _| {
                            let mut menu = menu.item(
                                PopupMenuItem::new(system_label.clone())
                                    .checked(current == GpuPref::SystemDefault)
                                    .on_click({
                                        let view = view.clone();
                                        move |_, _, cx| {
                                            view.update(cx, |this, cx| {
                                                this.set_gpu_pref(GpuPref::SystemDefault, cx)
                                            });
                                        }
                                    }),
                            );
                            menu = menu.item(
                                PopupMenuItem::new(cpu_label.clone())
                                    .checked(current == GpuPref::Cpu)
                                    .on_click({
                                        let view = view.clone();
                                        move |_, _, cx| {
                                            view.update(cx, |this, cx| {
                                                this.set_gpu_pref(GpuPref::Cpu, cx)
                                            });
                                        }
                                    }),
                            );
                            for card in &cards {
                                let pref = GpuPref::Card(card.device);
                                menu = menu.item(
                                    PopupMenuItem::new(card_label(card.device, &cards))
                                        .checked(current == pref)
                                        .on_click({
                                            let view = view.clone();
                                            move |_, _, cx| {
                                                view.update(cx, |this, cx| {
                                                    this.set_gpu_pref(pref, cx)
                                                })
                                            }
                                        }),
                                );
                            }
                            menu
                        }
                    },
                    cx,
                ),
                cx,
            ))
    }
}
