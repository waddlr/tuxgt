mod actions;
mod dialog;
mod gate;
mod start;

pub(crate) use start::{default_new_start, NewStart, HERO_BODY};

use gpui_kit::component::button::ButtonVariants as _;
use gpui_kit::component::menu::{DropdownMenu as _, PopupMenuItem};
use gpui_kit::component::{h_flex, Disableable as _, IconName};
use gpui_kit::*;
use tuxgt_core::{
    data_dir, open_db_shared, preview_switch, switch_profile, FluentArgs, ProfileKind,
};

use super::super::widgets;
use super::super::{rt_block, Note, Shell};

impl Shell {
    pub(crate) fn profile_row(&self, view: Entity<Self>, cx: &App) -> impl IntoElement {
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
        let tip = self.strings.get("gui-profile-tip");
        let new_label = self.strings.get("gui-profile-new");
        let rename_label = self.strings.get("gui-profile-rename");
        let delete_label = self.strings.get("gui-profile-delete");
        h_flex().id(HERO_BODY[1]).child(
            widgets::btn("profile-menu", cx)
                .secondary()
                .disabled(set.is_none())
                .child(widgets::blabel(label, cx))
                .child(widgets::bicon(IconName::ChevronDown))
                .tooltip(tip)
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
                            move |_, window, cx| {
                                dialog::open_rename_profile(view.clone(), window, cx)
                            }
                        }));
                    }
                    if can_delete {
                        menu = menu.item(PopupMenuItem::new(delete_label.clone()).on_click({
                            let view = view.clone();
                            move |_, window, cx| {
                                dialog::open_delete_profile(view.clone(), window, cx)
                            }
                        }));
                    }
                    menu
                }),
        )
    }

    pub(crate) fn profile_use_ui(&mut self, target: String, cx: &mut Context<Self>) {
        let Some(game) = self.selected.clone() else {
            return;
        };
        let name = self
            .profiles
            .as_ref()
            .and_then(|s| s.by_id(&target))
            .map(|p| p.name.clone())
            .unwrap_or_else(|| target.clone());
        self.spawn_preview(game, target, name, cx);
    }

    pub(crate) fn spawn_preview(
        &mut self,
        game: String,
        target: String,
        name: String,
        cx: &mut Context<Self>,
    ) {
        let transfer = self.hide_state.installing();
        cx.spawn(async move |this, cx| {
            let _transfer = transfer;
            let game_bg = game.clone();
            let target_bg = target.clone();
            let result = cx
                .background_spawn(async move {
                    rt_block(async {
                        let pool = open_db_shared(&data_dir()).await?;
                        preview_switch(&pool, &data_dir(), &game_bg, &target_bg).await
                    })
                })
                .await;
            let _ = this.update_in(cx, |this, window, cx| match result {
                Ok(preview) if preview.file_changes.is_empty() && !preview.drops_needs => {
                    this.switch_go(game, target, name, true, true, cx);
                }
                Ok(preview) if !preview.file_changes.is_empty() => {
                    dialog::open_files(this_view(cx), window, cx, preview.file_changes, {
                        let game = game.clone();
                        let target = target.clone();
                        let name = name.clone();
                        move |this, cx| {
                            this.switch_go(
                                game.clone(),
                                target.clone(),
                                name.clone(),
                                true,
                                false,
                                cx,
                            )
                        }
                    });
                }
                Ok(preview) if preview.drops_needs => {
                    dialog::open_restore(this_view(cx), window, cx, {
                        let game = game.clone();
                        let target = target.clone();
                        let name = name.clone();
                        move |this, cx| {
                            this.switch_go(
                                game.clone(),
                                target.clone(),
                                name.clone(),
                                true,
                                true,
                                cx,
                            )
                        }
                    });
                }
                Ok(_) => this.switch_go(game, target, name, true, true, cx),
                Err(e) => {
                    this.status = format!("{e}");
                    cx.notify();
                }
            });
        })
        .detach();
    }

    pub(crate) fn switch_go(
        &mut self,
        game: String,
        target: String,
        name: String,
        yes_files: bool,
        yes_restore: bool,
        cx: &mut Context<Self>,
    ) {
        let transfer = self.hide_state.installing();
        cx.spawn(async move |this, cx| {
            let _transfer = transfer;
            let game_bg = game.clone();
            let target_bg = target.clone();
            let result = cx
                .background_spawn(async move {
                    rt_block(async {
                        let pool = open_db_shared(&data_dir()).await?;
                        switch_profile(
                            &pool,
                            &data_dir(),
                            &game_bg,
                            &target_bg,
                            yes_files,
                            yes_restore,
                        )
                        .await
                    })
                })
                .await;
            let _ = this.update_in(cx, |this, window, cx| match result {
                Ok(out) => this.after_profile_switch(&game, &name, out.restore_deferred, cx),
                Err(tuxgt_core::Error::NeedConfirm(msg))
                    if msg.starts_with("profile-files:") && !yes_files =>
                {
                    let dests = msg
                        .split_once(':')
                        .map(|(_, rest)| rest)
                        .unwrap_or(&msg)
                        .split(',')
                        .map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty())
                        .collect();
                    dialog::open_files(this_view(cx), window, cx, dests, {
                        let game = game.clone();
                        let target = target.clone();
                        let name = name.clone();
                        let yes_restore = yes_restore;
                        move |this, cx| {
                            this.switch_go(
                                game.clone(),
                                target.clone(),
                                name.clone(),
                                true,
                                yes_restore,
                                cx,
                            )
                        }
                    });
                }
                Err(tuxgt_core::Error::NeedConfirm(msg))
                    if msg.starts_with("profile-restore:") && !yes_restore =>
                {
                    dialog::open_restore(this_view(cx), window, cx, {
                        let game = game.clone();
                        let target = target.clone();
                        let name = name.clone();
                        move |this, cx| {
                            this.switch_go(
                                game.clone(),
                                target.clone(),
                                name.clone(),
                                true,
                                true,
                                cx,
                            )
                        }
                    });
                }
                Err(e) => {
                    this.status = format!("{e}");
                    cx.notify();
                }
            });
        })
        .detach();
    }

    pub(crate) fn after_profile_switch(
        &mut self,
        game: &str,
        name: &str,
        restore_deferred: bool,
        cx: &mut Context<Self>,
    ) {
        self.reload_profiles(cx);
        self.refresh_mods(game, cx);
        self.reload_knob_counts(cx);
        self.reload_game_env(cx);
        self.reload_armed_state(game);
        let mut args = FluentArgs::new();
        args.set("name", name.to_string());
        self.pending_note = Some(Note::Ok(
            self.strings.get_args("gui-profile-using", Some(&args)),
        ));
        if restore_deferred {
            self.queue_profile_restore(game, cx);
        }
        cx.notify();
    }
}

fn this_view(cx: &Context<Shell>) -> Entity<Shell> {
    cx.entity()
}
