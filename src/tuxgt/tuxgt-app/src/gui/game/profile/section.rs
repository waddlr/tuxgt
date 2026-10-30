use gpui_kit::component::button::ButtonVariants as _;
use gpui_kit::component::menu::{DropdownMenu as _, PopupMenuItem};
use gpui_kit::component::{Disableable as _, IconName};
use gpui_kit::*;
use tuxgt_core::ProfileKind;

use super::super::super::widgets;
use super::super::super::Shell;
use super::dialog;

impl Shell {
    /// General Profile card: what a profile is, plus the Current
    /// dropdown. The menu is the one the hero used to carry.
    pub(crate) fn profile_section(&self, view: Entity<Self>, cx: &App) -> impl IntoElement {
        let set = self.profiles.clone();
        let label = set
            .as_ref()
            .and_then(|s| s.active_profile())
            .map(|p| p.name.clone())
            .unwrap_or_else(|| "…".to_string());
        let active = set.as_ref().map(|s| s.active.clone());
        let kind = set
            .as_ref()
            .and_then(|s| s.active_profile())
            .map(|p| p.kind);
        let can_rename = kind.is_some_and(|k| k != ProfileKind::Unmodded);
        let can_delete = kind.is_some_and(|k| k == ProfileKind::User);
        let new_label = self.strings.get("gui-profile-new");
        let rename_label = self.strings.get("gui-profile-rename");
        let delete_label = self.strings.get("gui-profile-delete");
        let menu_btn = widgets::btn("profile-menu", cx)
            .secondary()
            .disabled(set.is_none())
            .child(widgets::blabel(label, cx))
            .child(widgets::bicon(IconName::ChevronDown))
            .dropdown_menu(move |menu, _, _| {
                let Some(set) = set.clone() else {
                    return menu;
                };
                let mut menu = menu;
                for profile in &set.profiles {
                    let id = profile.id.clone();
                    let view = view.clone();
                    menu = menu.item(
                        PopupMenuItem::new(profile.name.clone())
                            .checked(active.as_deref() == Some(profile.id.as_str()))
                            .on_click(move |_, _, cx| {
                                let id = id.clone();
                                view.update(cx, |this, cx| this.profile_use_ui(id, cx));
                            }),
                    );
                }
                menu = menu
                    .separator()
                    .item(PopupMenuItem::new(new_label.clone()).on_click({
                        let view = view.clone();
                        move |_, window, cx| dialog::open_new_profile(view.clone(), window, cx)
                    }));
                if can_rename {
                    menu = menu.item(PopupMenuItem::new(rename_label.clone()).on_click({
                        let view = view.clone();
                        move |_, window, cx| dialog::open_rename_profile(view.clone(), window, cx)
                    }));
                }
                if can_delete {
                    menu = menu.item(PopupMenuItem::new(delete_label.clone()).on_click({
                        let view = view.clone();
                        move |_, window, cx| dialog::open_delete_profile(view.clone(), window, cx)
                    }));
                }
                menu
            });
        widgets::section_card("profile", cx)
            .child(widgets::section_title(
                self.strings.get("gui-section-profile"),
                cx,
            ))
            .child(widgets::muted(self.strings.get("gui-profile-note"), cx))
            .child(widgets::labeled_row(
                "profile-current",
                self.strings.get("gui-profile-current"),
                None,
                menu_btn,
                cx,
            ))
    }
}
