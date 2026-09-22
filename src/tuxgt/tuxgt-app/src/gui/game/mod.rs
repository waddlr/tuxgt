mod about;
mod about_more;
mod appid;
mod card;
mod confirm;
mod custom_env;
mod detect;
mod detect_edit;
mod env;
mod extras;
mod hero;
mod install;
mod install_queue;
mod knob;
mod knob_write;
mod launch;
mod mods;
mod mods_update;
mod order;
mod override_ui;
mod picker;
#[cfg(test)]
mod picker_tests;
mod picker_util;
mod play;
mod rows;
mod uninstall;
mod wrappers;

pub(crate) use confirm::*;
pub(crate) use hero::*;
pub(crate) use install_queue::*;
pub(crate) use launch::*;
pub(crate) use order::*;
pub(crate) use override_ui::*;
pub(crate) use picker_util::*;
// T24: `paint_ids` builds `ModIds` from `files_accordion_key` (single owner
// of the `{game}/{instance}` shape), so the card helpers are always visible.
pub(crate) use card::*;

use gpui_kit::assets::IconName as FullIconName;
use gpui_kit::component::tab::{Tab, TabBar};
use gpui_kit::component::{v_flex, Icon, IconName, IconNamed, Sizable as _};
use gpui_kit::*;
use tuxgt_core::FetchProgress;

use super::theme::{types, TypeStyled as _};
use super::widgets;
use super::*;
use super::{
    game_row, load_handle, load_mods_for, load_payload, slot_capable, GameTab, Nav, Shell, StageRow,
};

/// E102: Live-progress poll interval. The transfer thread only writes a cell;
/// this is the UI side's repaint cadence, paired with `set_live_progress`
/// dropping sub-1% steps — the ~100ms/1% coalescing.
pub(crate) const LIVE_TICK_MS: u64 = 100;

/// E102: one in-flight install's Live card. The core transfer thread writes the
/// latest bytes into `progress`; a UI pump task reads it and repaints. `id` is
/// the notice, `generation` lets a reused card's old pump task exit instead of
/// repainting its replacement.
pub(crate) struct InstallLive {
    pub(crate) id: u64,
    pub(crate) generation: u64,
    pub(crate) progress: std::sync::Arc<std::sync::Mutex<Option<FetchProgress>>>,
}

/// Icon + text without the kit icon slot: that slot centers the glyph in a
/// 27.5px box, so the underline overhangs left of the visible content.
/// `prefix` takes the glyph's natural width and the underline hugs the tab.
pub(crate) fn game_tab(icon: impl IconNamed, label: String) -> Tab {
    Tab::new()
        .prefix(Icon::new(icon).small())
        .label(label.clone())
        .aria_label(label)
}

impl Shell {
    pub(crate) fn game_chrome(&self, view: Entity<Self>, cx: &App) -> impl IntoElement {
        let Some(g) = self.selected_game() else {
            return div().into_any_element();
        };
        let tab_ix = match self.game_tab {
            GameTab::General => 0,
            GameTab::Mods => 1,
            GameTab::Env => 2,
        };
        v_flex()
            .id("game-chrome")
            .w_full()
            .flex_none()
            .child(self.game_header(g, view.clone(), cx))
            .child(
                TabBar::new("game-tabs")
                    .underline()
                    .small()
                    .selected_index(tab_ix)
                    .on_click({
                        let view = view.clone();
                        move |ix, _, cx| {
                            view.update(cx, |this, cx| {
                                this.switch_game_tab(
                                    match ix {
                                        1 => GameTab::Mods,
                                        2 => GameTab::Env,
                                        _ => GameTab::General,
                                    },
                                    cx,
                                );
                            });
                        }
                    })
                    .child(game_tab(
                        IconName::Info,
                        self.strings.get("gui-tab-general"),
                    ))
                    .child(game_tab(
                        FullIconName::Boxes,
                        self.strings.get("gui-tab-mods"),
                    ))
                    .child(game_tab(
                        IconName::Settings2,
                        self.strings.get("gui-tab-env"),
                    )),
            )
            .into_any_element()
    }

    pub(crate) fn game_scroll(&self, view: Entity<Self>, cx: &App) -> impl IntoElement {
        let Some(g) = self.selected_game() else {
            return v_flex()
                .id("game-empty")
                .gap_2()
                .p_8()
                .items_center()
                .child(
                    div()
                        .tx(types(cx).headline_md)
                        .child(self.strings.get("gui-empty-select-game")),
                )
                .child(widgets::muted(self.strings.get("gui-empty-pick-hint"), cx))
                .into_any_element();
        };
        v_flex()
            .id("game")
            .w_full()
            .flex_shrink_0()
            .gap_3()
            .pt_2()
            .child(match self.game_tab {
                GameTab::General => self.general_tab(view.clone(), cx).into_any_element(),
                GameTab::Mods => self.mods_tab(g.id.as_str(), view.clone(), cx),
                GameTab::Env => self
                    .env_tab(g.id.as_str(), view.clone(), cx)
                    .into_any_element(),
            })
            .into_any_element()
    }
}
