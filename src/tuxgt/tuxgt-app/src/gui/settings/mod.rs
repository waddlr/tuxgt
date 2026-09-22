mod add;
mod add_password;
mod add_pick;
mod add_save;
mod add_scan;
mod env;
mod export;
mod extras;
mod extras_box;
mod family;
mod family_box;
mod general;
mod host;
mod host_card;
mod mods;
mod mods_box;
mod mods_ops;
mod plugins;
mod secret;

pub(crate) use add_pick::*;
pub(crate) use add_scan::*;
pub(crate) use export::*;

use gpui_kit::assets::IconName as FullIconName;
use gpui_kit::component::tab::{Tab, TabBar};
use gpui_kit::component::{v_flex, Icon, IconName, IconNamed, Sizable as _};
use gpui_kit::*;

use super::*;
use super::{AddForm, InstanceRow, SettingsTab, Shell};

/// Same as `game_tab`: `prefix` + `label`, never the kit icon slot (its
/// 27.5px box shifts the underline left of the visible content).
pub(crate) fn settings_tab(icon: impl IconNamed, label: String) -> Tab {
    Tab::new()
        .prefix(Icon::new(icon).small())
        .label(label.clone())
        .aria_label(label)
}
/// Display label for a stored mod source id. Stored ids stay snake_case
/// (`type_str`); only the painted text changes. Unknown ids fall back raw.
pub(crate) fn source_label(source: &str) -> &str {
    match source {
        "github" => "GitHub",
        "local" => "Local",
        // Direct-download recipe (e.g. ReShade installer): only the user-input
        // `manual_url` source paints as Manual download; anything else raw.
        "manual_url" => "Manual download",
        other => other,
    }
}

impl Shell {
    pub(crate) fn settings_chrome(&self, view: Entity<Self>, _cx: &App) -> impl IntoElement {
        let tab_ix = match self.settings_tab {
            SettingsTab::General => 0,
            SettingsTab::CorePlugins => 1,
            SettingsTab::GameEnv => 2,
            SettingsTab::Mods => 3,
        };
        v_flex()
            .id("settings-chrome")
            .w_full()
            .flex_none()
            .pb_2()
            .child(
                TabBar::new("settings-tabs")
                    .underline()
                    .small()
                    .selected_index(tab_ix)
                    .on_click({
                        let view = view.clone();
                        move |ix, _, cx| {
                            view.update(cx, |this, cx| {
                                this.switch_settings_tab(
                                    match ix {
                                        1 => SettingsTab::CorePlugins,
                                        2 => SettingsTab::GameEnv,
                                        3 => SettingsTab::Mods,
                                        _ => SettingsTab::General,
                                    },
                                    cx,
                                );
                            });
                        }
                    })
                    .child(settings_tab(
                        IconName::Settings,
                        self.strings.get("gui-tab-settings-general"),
                    ))
                    .child(settings_tab(
                        FullIconName::Boxes,
                        self.strings.get("gui-tab-settings-core-plugins"),
                    ))
                    .child(settings_tab(
                        IconName::Settings2,
                        self.strings.get("gui-tab-settings-env"),
                    ))
                    .child(settings_tab(
                        IconName::Folder,
                        self.strings.get("gui-tab-settings-mods"),
                    )),
            )
            .into_any_element()
    }

    pub(crate) fn settings_body(&self, view: Entity<Self>, cx: &App) -> impl IntoElement {
        match self.settings_tab {
            SettingsTab::General => self.settings_general(view, cx).into_any_element(),
            SettingsTab::CorePlugins => self.settings_core_plugins(view, cx).into_any_element(),
            SettingsTab::GameEnv => self.settings_env(view, cx).into_any_element(),
            SettingsTab::Mods => self.settings_mods(view, cx).into_any_element(),
        }
    }
}
