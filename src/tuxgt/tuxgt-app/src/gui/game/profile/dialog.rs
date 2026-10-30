use gpui_kit::component::button::ButtonVariants as _;
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::component::{h_flex, v_flex, Sizable as _, WindowExt as _};
use gpui_kit::*;
use tuxgt_core::FluentArgs;

use super::super::super::widgets;
use super::super::super::Shell;
use super::{default_new_start, NewStart};

struct NewPick {
    start: NewStart,
    base: String,
}

pub(crate) fn open_new_profile(view: Entity<Shell>, window: &mut Window, cx: &mut App) {
    let Some(set) = view.read(cx).profiles.clone() else {
        return;
    };
    let strings = view.read(cx).strings.clone();
    let active = set.active.clone();
    let rows: Vec<(String, String)> = set
        .profiles
        .iter()
        .map(|p| (p.id.clone(), p.name.clone()))
        .collect();
    let name_ph = strings.get("gui-profile-name");
    let name_input = cx.new(|cx| InputState::new(window, cx).placeholder(name_ph.clone()));
    let pick = cx.new(|_| NewPick {
        start: default_new_start(),
        base: active,
    });
    let title = strings.get("gui-profile-new");
    let empty = strings.get("gui-profile-empty");
    let empty_note = strings.get("gui-profile-empty-note");
    let copy = strings.get("gui-profile-copy");
    let create = strings.get("gui-profile-create");
    let cancel = strings.get("gui-action-cancel");
    window.open_dialog(cx, move |dialog, _, cx| {
        let chosen = pick.read(cx);
        let base_name = rows
            .iter()
            .find(|(id, _)| id == &chosen.base)
            .map(|(_, name)| name.clone())
            .unwrap_or_default();
        let mut args = FluentArgs::new();
        args.set("base", base_name);
        let copy_note = strings.get_args("gui-profile-copy-note", Some(&args));
        let start = chosen.start;
        let mut body = v_flex()
            .gap_2()
            .child(Input::new(&name_input).xsmall())
            .child(choice_row(
                "profile-new-empty",
                &empty,
                start == NewStart::Empty,
                {
                    let pick = pick.clone();
                    let view = view.clone();
                    move |cx| {
                        pick.update(cx, |state, _| state.start = NewStart::Empty);
                        view.update(cx, |_, cx| cx.notify());
                    }
                },
                cx,
            ));
        if !start.shows_base() {
            body = body.child(widgets::muted(empty_note.clone(), cx));
        }
        body = body.child(choice_row(
            "profile-new-copy",
            &copy,
            start == NewStart::Copy,
            {
                let pick = pick.clone();
                let view = view.clone();
                move |cx| {
                    pick.update(cx, |state, _| state.start = NewStart::Copy);
                    view.update(cx, |_, cx| cx.notify());
                }
            },
            cx,
        ));
        if start.shows_base() {
            body = body.child(widgets::muted(copy_note, cx));
            for (id, name) in &rows {
                let id = id.clone();
                let on = pick.read(cx).base == id;
                body = body.child(choice_row(
                    &format!("profile-base-{id}"),
                    name,
                    on,
                    {
                        let pick = pick.clone();
                        let view = view.clone();
                        let id = id.clone();
                        move |cx| {
                            pick.update(cx, |state, _| {
                                state.start = NewStart::Copy;
                                state.base = id.clone();
                            });
                            view.update(cx, |_, cx| cx.notify());
                        }
                    },
                    cx,
                ));
            }
        }
        let name_input = name_input.clone();
        let pick = pick.clone();
        let view = view.clone();
        dialog.title(title.clone()).child(
            body.child(
                h_flex()
                    .gap_2()
                    .child(action_btn(
                        "profile-create",
                        &create,
                        true,
                        {
                            let view = view.clone();
                            let name_input = name_input.clone();
                            let pick = pick.clone();
                            move |window, cx| {
                                let name = name_input.read(cx).value().to_string();
                                let start = pick.read(cx).start;
                                let base = pick.read(cx).base.clone();
                                window.close_dialog(cx);
                                view.update(cx, |this, cx| {
                                    this.create_and_use(name, start, base, cx)
                                });
                            }
                        },
                        cx,
                    ))
                    .child(action_btn(
                        "profile-create-cancel",
                        &cancel,
                        false,
                        move |window, cx| window.close_dialog(cx),
                        cx,
                    )),
            ),
        )
    });
}

pub(crate) fn open_rename_profile(view: Entity<Shell>, window: &mut Window, cx: &mut App) {
    let Some(set) = view.read(cx).profiles.clone() else {
        return;
    };
    let Some(current) = set.active_profile() else {
        return;
    };
    if current.kind == tuxgt_core::ProfileKind::Unmodded {
        return;
    }
    let old = current.name.clone();
    let strings = view.read(cx).strings.clone();
    let input = cx.new(|cx| InputState::new(window, cx));
    input.update(cx, |inp, cx| inp.set_value(old.clone(), window, cx));
    let title = strings.get("gui-profile-rename");
    let save = strings.get("gui-action-save");
    let cancel = strings.get("gui-action-cancel");
    window.open_dialog(cx, move |dialog, _, cx| {
        let input = input.clone();
        let view = view.clone();
        let old = old.clone();
        dialog.title(title.clone()).child(
            v_flex().gap_2().child(Input::new(&input).xsmall()).child(
                h_flex()
                    .gap_2()
                    .child(action_btn(
                        "profile-rename-save",
                        &save,
                        true,
                        {
                            let view = view.clone();
                            let input = input.clone();
                            let old = old.clone();
                            move |window, cx| {
                                let new_name = input.read(cx).value().to_string();
                                let old = old.clone();
                                window.close_dialog(cx);
                                view.update(cx, |this, cx| this.rename_go(old, new_name, cx));
                            }
                        },
                        cx,
                    ))
                    .child(action_btn(
                        "profile-rename-cancel",
                        &cancel,
                        false,
                        move |window, cx| window.close_dialog(cx),
                        cx,
                    )),
            ),
        )
    });
}

pub(crate) fn open_delete_profile(view: Entity<Shell>, window: &mut Window, cx: &mut App) {
    let Some(set) = view.read(cx).profiles.clone() else {
        return;
    };
    let Some(current) = set.active_profile() else {
        return;
    };
    if current.kind != tuxgt_core::ProfileKind::User {
        return;
    }
    let name = current.name.clone();
    let strings = view.read(cx).strings.clone();
    let mut args = FluentArgs::new();
    args.set("name", name.clone());
    let ask = strings.get_args("gui-profile-delete-ask", Some(&args));
    let delete = strings.get("gui-profile-delete");
    let cancel = strings.get("gui-action-cancel");
    window.open_dialog(cx, move |dialog, _, cx| {
        let view = view.clone();
        let name = name.clone();
        dialog.title(ask.clone()).child(
            h_flex()
                .gap_2()
                .child(action_btn(
                    "profile-delete-yes",
                    &delete,
                    true,
                    {
                        let view = view.clone();
                        let name = name.clone();
                        move |window, cx| {
                            let name = name.clone();
                            window.close_dialog(cx);
                            view.update(cx, |this, cx| this.delete_go(name, false, false, cx));
                        }
                    },
                    cx,
                ))
                .child(action_btn(
                    "profile-delete-no",
                    &cancel,
                    false,
                    |window, cx| window.close_dialog(cx),
                    cx,
                )),
        )
    });
}

pub(crate) fn open_files(
    view: Entity<Shell>,
    strings: tuxgt_core::Strings,
    window: &mut Window,
    cx: &mut App,
    dests: Vec<String>,
    go: impl Fn(&mut Shell, &mut Context<Shell>) + 'static,
) {
    let go: std::rc::Rc<dyn Fn(&mut Shell, &mut Context<Shell>)> = std::rc::Rc::new(go);
    let title = strings.get("gui-profile-files");
    let confirm = strings.get("gui-action-confirm");
    let cancel = strings.get("gui-action-cancel");
    window.open_dialog(cx, move |dialog, _, cx| {
        dialog.title(title.clone()).child(
            v_flex()
                .gap_2()
                .children(dests.iter().map(|d| widgets::mono(d.clone(), cx)))
                .child(confirm_row(view.clone(), &confirm, &cancel, go.clone(), cx)),
        )
    });
}

pub(crate) fn open_restore(
    view: Entity<Shell>,
    strings: tuxgt_core::Strings,
    window: &mut Window,
    cx: &mut App,
    go: impl Fn(&mut Shell, &mut Context<Shell>) + 'static,
) {
    let go: std::rc::Rc<dyn Fn(&mut Shell, &mut Context<Shell>)> = std::rc::Rc::new(go);
    let title = strings.get("gui-profile-restore");
    let confirm = strings.get("gui-action-confirm");
    let cancel = strings.get("gui-action-cancel");
    window.open_dialog(cx, move |dialog, _, cx| {
        dialog.title(title.clone()).child(confirm_row(
            view.clone(),
            &confirm,
            &cancel,
            go.clone(),
            cx,
        ))
    });
}

fn choice_row(
    id: &str,
    label: &str,
    on: bool,
    pick: impl Fn(&mut App) + 'static,
    cx: &App,
) -> impl IntoElement {
    let glyph = if on { "●" } else { "○" };
    h_flex()
        .id(SharedString::from(id.to_string()))
        .gap_2()
        .cursor_pointer()
        .on_click(move |_, _, cx| pick(cx))
        .child(widgets::mono(glyph, cx))
        .child(widgets::blabel(label.to_string(), cx))
}

fn action_btn(
    id: &'static str,
    label: &str,
    primary: bool,
    go: impl Fn(&mut Window, &mut App) + 'static,
    cx: &App,
) -> impl IntoElement {
    let btn = widgets::btn(id, cx).child(widgets::blabel(label.to_string(), cx));
    let btn = if primary { btn.primary() } else { btn.ghost() };
    btn.on_click(move |_, window, cx| go(window, cx))
}

fn confirm_row(
    view: Entity<Shell>,
    confirm: &str,
    cancel: &str,
    go: std::rc::Rc<dyn Fn(&mut Shell, &mut Context<Shell>)>,
    cx: &App,
) -> impl IntoElement {
    let go_yes = go.clone();
    h_flex()
        .gap_2()
        .child(action_btn(
            "profile-files-yes",
            confirm,
            true,
            {
                let view = view.clone();
                move |window, cx| {
                    window.close_dialog(cx);
                    let go_yes = go_yes.clone();
                    view.update(cx, |this, cx| go_yes(this, cx));
                }
            },
            cx,
        ))
        .child(action_btn(
            "profile-files-no",
            cancel,
            false,
            move |window, cx| window.close_dialog(cx),
            cx,
        ))
}
