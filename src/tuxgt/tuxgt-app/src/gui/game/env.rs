use gpui_kit::component::button::ButtonVariants as _;
use gpui_kit::component::v_flex;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use tuxgt_core::{knob_is_unmanaged, proton_ge_cachy, KnobRow};

use super::super::widgets;
use super::super::{EnvPage, Shell};

impl Shell {
    pub(crate) fn env_tab(&self, game_id: &str, view: Entity<Self>, cx: &App) -> impl IntoElement {
        v_flex()
            .id("env")
            .w_full()
            .flex_shrink_0()
            .gap_3()
            .child(widgets::muted(self.strings.get("gui-note-env-applies"), cx))
            .child(self.env_groups(EnvPage::Game, view.clone(), cx))
            .child(self.custom_env_box(game_id, view, cx))
    }

    pub(crate) fn env_groups(
        &self,
        page: EnvPage,
        view: Entity<Self>,
        cx: &App,
    ) -> impl IntoElement {
        const GROUPS: &[(&str, &str)] = &[
            ("gui-knob-dxvk", "dxvk"),
            ("gui-knob-proton", "proton"),
            ("gui-knob-mesa", "mesa"),
            ("gui-knob-vkd3d", "vkd3d"),
            ("gui-knob-nvidia", "nvidia"),
            ("gui-knob-amd", "amd"),
            ("gui-knob-intel", "intel"),
            ("gui-knob-other", "other"),
        ];
        let mut main: Vec<(&'static str, Vec<&'static tuxgt_core::EnvKnob>)> = Vec::new();
        let mut advance: Vec<(&'static str, Vec<&'static tuxgt_core::EnvKnob>)> = Vec::new();
        for (title, group) in GROUPS {
            let mut m = Vec::new();
            let mut a = Vec::new();
            for k in self
                .knobs
                .iter()
                .copied()
                .filter(|k| Self::knob_in_group(k.id, group))
            {
                if self.knob_in_advance(k, page) {
                    a.push(k);
                } else {
                    m.push(k);
                }
            }
            if !m.is_empty() {
                main.push((title, m));
            }
            if !a.is_empty() {
                advance.push((title, a));
            }
        }
        // R42: mesa stays in main (it is not an NVIDIA stack); unset mesa knobs are
        // mirrored into Advance only on NVIDIA hosts.
        if self.host_gpu.nvidia && !self.host_gpu.unknown() {
            if let Some((_, mesa)) = main.iter().find(|(t, _)| *t == "gui-knob-mesa") {
                let mirror: Vec<&'static tuxgt_core::EnvKnob> = mesa
                    .iter()
                    .copied()
                    .filter(|k| !self.knob_is_set(k, page) && !self.row_unmanaged(k))
                    .collect();
                if !mirror.is_empty() {
                    advance.push(("gui-knob-mesa", mirror));
                }
            }
        }
        let mid = main.len().div_ceil(2).max(1);
        let (left, right) = main.split_at(mid.min(main.len()));
        v_flex()
            .id("env-groups")
            .gap_3()
            .child(widgets::cols(
                "two-col",
                280.,
                false,
                vec![
                    (
                        1,
                        self.env_group_column("env-main-l", left, page, view.clone(), cx)
                            .into_any_element(),
                    ),
                    (
                        1,
                        self.env_group_column("env-main-r", right, page, view.clone(), cx)
                            .into_any_element(),
                    ),
                ],
            ))
            .when(!advance.is_empty(), |this| {
                this.child(self.env_advance(advance, page, view, cx))
            })
    }

    pub(crate) fn env_group_column(
        &self,
        id: &'static str,
        groups: &[(&'static str, Vec<&'static tuxgt_core::EnvKnob>)],
        page: EnvPage,
        view: Entity<Self>,
        cx: &App,
    ) -> impl IntoElement {
        v_flex().id(id).gap_3().children(
            groups
                .iter()
                .map(|(title, knobs)| self.env_group_box(title, knobs, page, view.clone(), cx)),
        )
    }

    pub(crate) fn env_group_box(
        &self,
        title: &'static str,
        knobs: &[&'static tuxgt_core::EnvKnob],
        page: EnvPage,
        view: Entity<Self>,
        cx: &App,
    ) -> impl IntoElement {
        widgets::section_card(title, cx)
            .flex_shrink_0()
            .child(widgets::section_title(self.strings.get(title), cx))
            .children(
                knobs
                    .iter()
                    .copied()
                    .map(|k| self.knob_row(k, page, view.clone(), cx)),
            )
    }

    pub(crate) fn env_advance(
        &self,
        groups: Vec<(&'static str, Vec<&'static tuxgt_core::EnvKnob>)>,
        page: EnvPage,
        view: Entity<Self>,
        cx: &App,
    ) -> impl IntoElement {
        let open = match page {
            EnvPage::Game => self.game_env_advance,
            EnvPage::Global => self.settings_env_advance,
        };
        let mid = groups.len().div_ceil(2).max(1);
        let (left, right) = groups.split_at(mid.min(groups.len()));
        widgets::section_card("env-advance", cx)
            .child(
                widgets::btn("env-advance-toggle", cx)
                    .ghost()
                    .child(widgets::blabel(
                        self.strings.get("gui-section-env-advance"),
                        cx,
                    ))
                    .on_click({
                        let view = view.clone();
                        move |_, _, cx| {
                            view.update(cx, |this, cx| {
                                match page {
                                    EnvPage::Game => this.game_env_advance = !this.game_env_advance,
                                    EnvPage::Global => {
                                        this.settings_env_advance = !this.settings_env_advance
                                    }
                                }
                                cx.notify();
                            });
                        }
                    }),
            )
            .when(open, |this| {
                this.child(widgets::cols(
                    "two-col",
                    280.,
                    false,
                    vec![
                        (
                            1,
                            self.env_group_column("env-adv-l", left, page, view.clone(), cx)
                                .into_any_element(),
                        ),
                        (
                            1,
                            self.env_group_column("env-adv-r", right, page, view, cx)
                                .into_any_element(),
                        ),
                    ],
                ))
            })
    }

    pub(crate) fn knob_in_group(id: &str, group: &str) -> bool {
        match group {
            "dxvk" => id.starts_with("dxvk-"),
            "proton" => {
                id.starts_with("proton-") || id.starts_with("wine-") || id == "steamdeck-spoof"
            }
            "mesa" => id.starts_with("mesa-") || id == "vblank-mode",
            "vkd3d" => id.starts_with("vkd3d-"),
            "nvidia" => id.starts_with("nvidia-"),
            "amd" => id.starts_with("radv-") || id.starts_with("amd-") || id == "dri-prime",
            "intel" => id.starts_with("anv-") || id.starts_with("intel-"),
            "other" => !Self::knob_grouped(id),
            _ => false,
        }
    }

    pub(crate) fn knob_grouped(id: &str) -> bool {
        Self::knob_in_group(id, "dxvk")
            || Self::knob_in_group(id, "vkd3d")
            || Self::knob_in_group(id, "proton")
            || Self::knob_in_group(id, "mesa")
            || Self::knob_in_group(id, "nvidia")
            || Self::knob_in_group(id, "amd")
            || Self::knob_in_group(id, "intel")
    }
    pub(crate) fn knob_in_advance(&self, k: &tuxgt_core::EnvKnob, page: EnvPage) -> bool {
        if self.knob_is_set(k, page) || self.row_unmanaged(k) {
            return false;
        }
        // R41: a GPU-vendor group leaves main only when that vendor is absent
        // from the host (unknown hosts keep every group in main).
        let gpu_advance = match Self::knob_in_group(k.id, "nvidia") {
            true if !self.host_gpu.nvidia && !self.host_gpu.unknown() => true,
            _ if Self::knob_in_group(k.id, "amd")
                && !self.host_gpu.amd
                && !self.host_gpu.unknown() =>
            {
                true
            }
            _ if Self::knob_in_group(k.id, "intel")
                && !self.host_gpu.intel
                && !self.host_gpu.unknown() =>
            {
                true
            }
            _ => false,
        };
        if gpu_advance {
            return true;
        }
        // R43: offer DXVK vs VKD3D by effective API on game rows. DX12 keeps
        // vkd3d in main and sends dxvk to Advance; any other known API does
        // the reverse. Unknown API (or the global page) keeps both in main.
        if page == EnvPage::Game {
            let api = self.selected_game().and_then(|g| g.api.clone());
            if Self::knob_in_group(k.id, "dxvk") && api.as_deref() == Some("dx12") {
                return true;
            }
            if Self::knob_in_group(k.id, "vkd3d")
                && matches!(api.as_deref(), Some(a) if a != "dx12")
            {
                return true;
            }
        }
        if page == EnvPage::Game && k.ge_cachy_only() {
            let proton = self.selected_game().and_then(|g| g.proton.as_deref());
            if !proton_ge_cachy(proton) {
                return true;
            }
        }
        false
    }

    pub(crate) fn knob_is_set(&self, k: &tuxgt_core::EnvKnob, page: EnvPage) -> bool {
        match page {
            EnvPage::Game => self.knob_values.contains_key(k.id),
            EnvPage::Global => self.global_knobs.contains_key(k.id),
        }
    }

    pub(crate) fn row_unmanaged(&self, k: &tuxgt_core::EnvKnob) -> bool {
        let g = self.global_knobs.get(k.id);
        knob_is_unmanaged(k, g.filter(|r| r.enabled).map(|r| r.value.as_str()))
    }

    pub(crate) fn game_row(&self, id: &str) -> Option<KnobRow> {
        let value = self.knob_values.get(id)?.clone();
        Some(KnobRow {
            knob: id.to_string(),
            value,
            enabled: self.knob_enabled.get(id).copied().unwrap_or(true),
        })
    }
    /// Steam/Heroic Play is protocol dispatch. Handle on: enabled globals
    /// then per-game enabled knobs via `tux-protonfixes.conf`.
    pub(crate) fn is_client_game(&self, game_id: &str) -> bool {
        self.games
            .iter()
            .any(|g| g.id == game_id && (g.manager == "steam" || g.manager == "heroic"))
    }
}
