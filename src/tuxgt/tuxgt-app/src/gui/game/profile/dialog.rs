use std::rc::Rc;

use gpui_kit::component::input::InputState;
use gpui_kit::*;
use tuxgt_core::ProfileKind;

use super::super::super::Shell;
use super::{default_new_start, NewStart};

/// The parked continuation behind the Files/Restore confirms.
pub(crate) type ProfileGo = Rc<dyn Fn(&mut Shell, &mut Context<Shell>)>;

/// A profile dialog parked as shell state and painted by
/// `profile_dialog_layer` (app-owned centered modal, client-stop style)
/// instead of the kit's top-anchored dialog. Confirm runs the parked
/// action; Cancel, click-outside, and Escape just drop it.
pub(crate) enum ProfileDialog {
    New {
        name_input: Entity<InputState>,
        start: NewStart,
        base: String,
        rows: Vec<(String, String)>,
    },
    Rename {
        old: String,
        input: Entity<InputState>,
    },
    Delete {
        name: String,
    },
    Files {
        dests: Vec<String>,
        go: ProfileGo,
    },
    Restore {
        go: ProfileGo,
    },
}

pub(crate) fn open_new_profile(view: Entity<Shell>, window: &mut Window, cx: &mut App) {
    if view.read(cx).profile_dialog.is_some() {
        return;
    }
    let Some(set) = view.read(cx).profiles.clone() else {
        return;
    };
    let strings = view.read(cx).strings.clone();
    let name_ph = strings.get("gui-profile-name");
    let name_input = cx.new(|cx| InputState::new(window, cx).placeholder(name_ph));
    let rows: Vec<(String, String)> = set
        .profiles
        .iter()
        .map(|p| (p.id.clone(), p.name.clone()))
        .collect();
    let focus = name_input.read(cx).focus_handle(cx);
    view.update(cx, |this, cx| {
        this.profile_dialog = Some(ProfileDialog::New {
            name_input,
            start: default_new_start(),
            base: set.active.clone(),
            rows,
        });
        cx.notify();
    });
    focus.focus(window, cx);
}

pub(crate) fn open_rename_profile(view: Entity<Shell>, window: &mut Window, cx: &mut App) {
    if view.read(cx).profile_dialog.is_some() {
        return;
    }
    let Some(set) = view.read(cx).profiles.clone() else {
        return;
    };
    let Some(current) = set.active_profile() else {
        return;
    };
    if current.kind == ProfileKind::Unmodded {
        return;
    }
    let old = current.name.clone();
    let input = cx.new(|cx| InputState::new(window, cx));
    input.update(cx, |inp, cx| inp.set_value(old.clone(), window, cx));
    let focus = input.read(cx).focus_handle(cx);
    view.update(cx, |this, cx| {
        this.profile_dialog = Some(ProfileDialog::Rename { old, input });
        cx.notify();
    });
    focus.focus(window, cx);
}

pub(crate) fn open_delete_profile(view: Entity<Shell>, window: &mut Window, cx: &mut App) {
    if view.read(cx).profile_dialog.is_some() {
        return;
    }
    let Some(set) = view.read(cx).profiles.clone() else {
        return;
    };
    let Some(current) = set.active_profile() else {
        return;
    };
    if current.kind != ProfileKind::User {
        return;
    }
    let name = current.name.clone();
    let focus = view.read(cx).focus.clone();
    view.update(cx, |this, cx| {
        this.profile_dialog = Some(ProfileDialog::Delete { name });
        cx.notify();
    });
    focus.focus(window, cx);
}

impl Shell {
    /// File-changes confirm, parked from inside a Shell update (the
    /// switch result handler), so it writes the state field directly
    /// instead of going through the view.
    pub(crate) fn open_files_dialog(
        &mut self,
        dests: Vec<String>,
        go: impl Fn(&mut Shell, &mut Context<Shell>) + 'static,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.can_park_profile_dialog(cx) {
            return;
        }
        self.profile_dialog = Some(ProfileDialog::Files {
            dests,
            go: Rc::new(go),
        });
        self.focus.focus(window, cx);
        cx.notify();
    }

    /// Apply-restore confirm; same parking rule as `open_files_dialog`.
    pub(crate) fn open_restore_dialog(
        &mut self,
        go: impl Fn(&mut Shell, &mut Context<Shell>) + 'static,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.can_park_profile_dialog(cx) {
            return;
        }
        self.profile_dialog = Some(ProfileDialog::Restore { go: Rc::new(go) });
        self.focus.focus(window, cx);
        cx.notify();
    }

    /// Late async preview/switch results must not replace an open dialog
    /// (typed New/Rename input, or a parked Files/Restore `go`).
    fn can_park_profile_dialog(&mut self, cx: &mut Context<Self>) -> bool {
        if self.profile_dialog.is_some() {
            self.status = self.strings.get("gui-status-resolve-confirm-first");
            cx.notify();
            return false;
        }
        true
    }

    /// Drop the parked dialog and return it. Restores shell focus so root
    /// Escape (sidecar) still works after an input-holding New/Rename closes.
    fn take_profile_dialog(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<ProfileDialog> {
        let dialog = self.profile_dialog.take()?;
        self.focus.focus(window, cx);
        cx.notify();
        Some(dialog)
    }

    /// Cancel / click-outside / Escape: drop the parked dialog and the
    /// action behind it. Nothing else is written.
    pub(crate) fn close_profile_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let _ = self.take_profile_dialog(window, cx);
    }

    pub(crate) fn profile_new_set_start(&mut self, start: NewStart, cx: &mut Context<Self>) {
        if let Some(ProfileDialog::New { start: cur, .. }) = &mut self.profile_dialog {
            *cur = start;
        }
        cx.notify();
    }

    pub(crate) fn profile_new_set_base(&mut self, base: String, cx: &mut Context<Self>) {
        if let Some(ProfileDialog::New {
            start, base: cur, ..
        }) = &mut self.profile_dialog
        {
            *start = NewStart::Copy;
            *cur = base;
        }
        cx.notify();
    }

    pub(crate) fn profile_new_create(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(ProfileDialog::New {
            name_input,
            start,
            base,
            ..
        }) = self.take_profile_dialog(window, cx)
        else {
            return;
        };
        let name = name_input.read(cx).value().to_string();
        self.create_and_use(name, start, base, cx);
    }

    pub(crate) fn profile_rename_save(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(ProfileDialog::Rename { old, input }) = self.take_profile_dialog(window, cx)
        else {
            return;
        };
        let new_name = input.read(cx).value().to_string();
        self.rename_go(old, new_name, cx);
    }

    pub(crate) fn profile_delete_confirm(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(ProfileDialog::Delete { name }) = self.take_profile_dialog(window, cx) else {
            return;
        };
        self.delete_go(name, false, false, false, cx);
    }

    pub(crate) fn profile_files_confirm(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(ProfileDialog::Files { go, .. }) = self.take_profile_dialog(window, cx) else {
            return;
        };
        go(self, cx);
    }

    pub(crate) fn profile_restore_confirm(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(ProfileDialog::Restore { go }) = self.take_profile_dialog(window, cx) else {
            return;
        };
        go(self, cx);
    }
}
