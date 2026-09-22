//! Tray menu commands, the recent-games snapshot, and the close policy.

use std::sync::{Arc, RwLock};

use tuxgt_core::GameRow;

/// What the user asked for from the tray. `Quit` is the only destructive
/// action; primary activation is Show/Hide.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum AppCommand {
    Show,
    ShowLibrary,
    ShowSettings,
    Hide,
    Play(String),
    Quit,
}

/// One recent game for the tray menu: the id a click plays, and the display
/// name the row paints (never a raw id).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RecentGame {
    pub(crate) id: String,
    pub(crate) display: String,
}

/// Controller-owned recent-games snapshot shared into the tray item (and the
/// window, so a GUI Play refreshes it). Refreshed on open and any Play —
/// each process refreshes its own, the stub re-reads after the handoff;
/// `menu()` only reads it, never the DB or the window.
pub(crate) type RecentSnapshot = Arc<RwLock<Vec<RecentGame>>>;

/// Tray Play list length: the recent-games query cap and the menu's row cap
/// in one place.
pub(crate) const RECENT_LIMIT: u32 = 5;

/// Replace the snapshot from played rows, newest first. Pure map + write, so
/// the GUI Play path (which already holds a pool) shares it with the
/// controller refresh.
pub(crate) fn store_recents(snapshot: &RecentSnapshot, rows: &[GameRow]) {
    let list: Vec<RecentGame> = rows
        .iter()
        .map(|g| RecentGame {
            id: g.id.clone(),
            display: g.display_name().to_string(),
        })
        .collect();
    if let Ok(mut guard) = snapshot.write() {
        *guard = list;
    }
}

/// What a close/minimize request resolves to. `Hide` only ever happens
/// with a ready tray, so Close can never become a no-op.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CloseAction {
    /// Let the platform close the window (and, as the last window, the app).
    Close,
    /// Drop the window and keep running with a tray icon.
    Hide,
}

/// One close request, decided once for every path (titlebar, compositor).
///
/// `tray_ready` is runtime state: with no SNI host the tray preferences
/// cannot be honored, so a close stays a real close rather than stranding the
/// process with no window and no way back.
pub(crate) fn close_action(close_to_tray: bool, tray_ready: bool) -> CloseAction {
    if close_to_tray && tray_ready {
        CloseAction::Hide
    } else {
        CloseAction::Close
    }
}

/// The minimize button's counterpart to [`close_action`]. A separate
/// preference: a user can want a tray on minimize without changing what
/// Close does.
pub(crate) fn minimize_action(minimize_to_tray: bool, tray_ready: bool) -> CloseAction {
    if minimize_to_tray && tray_ready {
        CloseAction::Hide
    } else {
        CloseAction::Close
    }
}

/// The Play counterpart: hide the window to the tray after a successful
/// launch when the preference is on. Default-on, but still fail-closed
/// without a ready tray — `Close` here means "stay visible".
pub(crate) fn launch_action(hide_on_launch: bool, tray_ready: bool) -> CloseAction {
    if hide_on_launch && tray_ready {
        CloseAction::Hide
    } else {
        CloseAction::Close
    }
}

/// Tray-Play hide decision: a successful headless Play hides the window
/// only when one is open. Pure so the controller rule stays unit-tested;
/// the veto is re-checked by the `hide()` handoff itself.
pub(crate) fn tray_play_hides(hide_on_launch: bool, window_open: bool, tray_ready: bool) -> bool {
    window_open && launch_action(hide_on_launch, tray_ready) == CloseAction::Hide
}

#[cfg(test)]
pub(super) fn recent(id: &str, display: &str) -> RecentGame {
    RecentGame {
        id: id.into(),
        display: display.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn close_is_the_default_and_never_hides() {
        // Off: a close quits even when a tray is present.
        assert_eq!(close_action(false, true), CloseAction::Close);
        // Off and no tray: the shipped behavior.
        assert_eq!(close_action(false, false), CloseAction::Close);
    }

    #[test]
    fn opt_in_close_hides_only_with_a_ready_tray() {
        assert_eq!(close_action(true, true), CloseAction::Hide);
        // No SNI host must not strand the process with no window and no
        // way to bring it back.
        assert_eq!(close_action(true, false), CloseAction::Close);
    }

    #[test]
    fn minimize_tray_is_independent_of_close_tray() {
        // Minimize-to-tray on, close-to-tray off: each answers only for
        // its own button.
        assert_eq!(minimize_action(true, true), CloseAction::Hide);
        assert_eq!(close_action(false, true), CloseAction::Close);
        // And an unavailable tray leaves the platform minimize alone.
        assert_eq!(minimize_action(true, false), CloseAction::Close);
    }

    #[test]
    fn launch_hide_needs_both_pref_and_ready_tray() {
        assert_eq!(launch_action(true, true), CloseAction::Hide);
        // Off stays visible even with a tray; on without a tray stays
        // visible rather than stranding the process.
        assert_eq!(launch_action(false, true), CloseAction::Close);
        assert_eq!(launch_action(true, false), CloseAction::Close);
        assert_eq!(launch_action(false, false), CloseAction::Close);
    }

    #[test]
    fn tray_play_hides_only_with_pref_window_and_tray() {
        assert!(tray_play_hides(true, true, true));
        // Any missing leg stays visible: pref off, no window (hidden
        // stub), or no tray to come back to.
        assert!(!tray_play_hides(false, true, true));
        assert!(!tray_play_hides(true, false, true));
        assert!(!tray_play_hides(true, true, false));
        assert!(!tray_play_hides(false, false, false));
    }

    fn game_row(id: &str, name: Option<&str>) -> GameRow {
        GameRow {
            id: id.into(),
            name: name.map(str::to_string),
            cover_path: None,
            manager: "steam".into(),
            store: String::new(),
            header_path: None,
            platform: None,
            api: None,
            install_dir: None,
            exe_path: None,
            prefix_path: None,
            proton: None,
            bitness: None,
            engine: None,
            hidden: false,
            last_played: None,
            steam_appid: None,
            adapter: "preload".into(),
        }
    }

    #[test]
    fn store_recents_paints_display_names_never_raw_ids() {
        let snapshot: RecentSnapshot = Arc::new(RwLock::new(Vec::new()));
        store_recents(
            &snapshot,
            &[
                game_row("steam::1", Some("Celeste")),
                game_row("steam::2", None),
            ],
        );
        let guard = snapshot.read().expect("snapshot");
        assert_eq!(
            *guard,
            vec![
                recent("steam::1", "Celeste"),
                recent("steam::2", "steam::2"),
            ]
        );
    }
}
