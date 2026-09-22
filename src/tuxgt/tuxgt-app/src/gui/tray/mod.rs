//! R12: one StatusNotifierItem with Show / Hide / Quit, plus the single
//! close-policy decision every close path goes through.
//!
//! The SNI client runs on its own thread (`ksni`'s blocking API over its smol
//! executor), so the app's shared Tokio `RT` stays the single Tokio runtime.
//! Callbacks therefore arrive off the main thread and only send a
//! [`AppCommand`] down a channel or set one flag mirror; the window
//! controller in `chrome/run` drains the channel and reads the mirrors, so
//! a stale callback always targets the live process, never a dropped window.
//! No SNI type crosses into `Shell` or core.

mod item;
mod play;
mod policy;
mod state;

pub(crate) use play::{refresh_recents, spawn_play_worker, TrayPlayOutcome};
pub(crate) use policy::{
    close_action, launch_action, minimize_action, store_recents, tray_play_hides, AppCommand,
    CloseAction, RecentSnapshot, RECENT_LIMIT,
};
pub(crate) use state::{HideState, InstallGuard};

use std::sync::mpsc::Sender;
use std::sync::Arc;

use item::TrayItem;
use ksni::blocking::TrayMethods;
use tuxgt_core::Strings;

/// A live tray item. Keeping the `Tray` alive is what makes the app
/// "tray-hidden" rather than dead; dropping it shuts the item down.
/// Commands flow through the caller-owned channel the window controller
/// drains, shared with every Shell's close/minimize path.
pub(crate) struct Tray {
    /// Held to keep the SNI service running.
    _handle: ksni::blocking::Handle<TrayItem>,
}

/// Register the item. `commands` is the window controller's channel: the
/// tray menu and primary activation write there, alongside the Shells.
/// `None` means no SNI host accepted the item, which is the honest "tray
/// unavailable" state — not a failure to surface, and not a reason to
/// change close behavior.
pub(crate) fn spawn(
    strings: &Strings,
    state: Arc<HideState>,
    commands: Sender<AppCommand>,
    recents: RecentSnapshot,
) -> Option<Tray> {
    let item = TrayItem {
        show_label: strings.get("gui-tray-show"),
        hide_label: strings.get("gui-tray-hide"),
        library_label: strings.get("gui-tray-show-library"),
        settings_label: strings.get("gui-tray-show-settings"),
        quit_label: strings.get("gui-tray-quit"),
        tip: strings.get("gui-tray-tooltip"),
        sender: commands,
        state,
        recents,
    };
    match item.spawn() {
        Ok(handle) => Some(Tray { _handle: handle }),
        Err(error) => {
            tracing::warn!(%error, "tray unavailable");
            None
        }
    }
}
