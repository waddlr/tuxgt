//! Tray Hide state: the veto counter, the mirrors, and the install guard.

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;

/// Whether a Hide may proceed, and why not.
///
/// `allowed` is the ordinary case. The veto is the fail-closed answer for the
/// one state R12 cannot migrate: an in-flight install keeps its bytes moving
/// and reports into a fresh Shell, but the queue behind it (a parked E34
/// confirm, the remaining picked instances) lives only in the window being
/// dropped. Prefix, game, and store mutations hold the same veto: the stub
/// handoff would kill them mid-write. Blocking Close with a status line keeps
/// that work intact, and Quit from the tray still terminates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct HiddenState {
    pub(crate) allowed: bool,
    /// Fluent id for the "cannot hide yet" line; `None` when allowed.
    pub(crate) blocked_by: Option<&'static str>,
}

impl HiddenState {
    fn allowed() -> Self {
        Self {
            allowed: true,
            blocked_by: None,
        }
    }

    fn blocked(id: &'static str) -> Self {
        Self {
            allowed: false,
            blocked_by: Some(id),
        }
    }
}

/// R12 state the window controller owns for the whole process lifetime: the
/// two mirrors the SNI callbacks write, the counter of transfers that must
/// not be interrupted, and the hidden flag background work gates on.
///
/// It is shared, not stored in `Shell`, because the tray item outlives the
/// window: a Hide ends the GUI process, and the stub serves the tray until
/// a Show respawns it.
pub(crate) struct HideState {
    /// No window is open; background work stays minimal.
    hidden: AtomicBool,
    /// Transfers that must not be interrupted by a Hide.
    in_flight: AtomicUsize,
    /// The SNI watcher went offline: the item is registered but no host
    /// shows it, so a Hide would strand the process.
    pub(super) offline: AtomicBool,
    /// Mirrors "is a window currently shown" for the item's status and for
    /// primary activation's toggle.
    pub(super) shown: Arc<AtomicBool>,
    /// The window's persisted tray preferences, mirrored so a close decided
    /// with no window in reach (the controller) still answers for the user's
    /// choice instead of reading a stale copy.
    close_to_tray: AtomicBool,
    minimize_to_tray: AtomicBool,
}

impl HideState {
    /// One per process. The tray item, the window controller, and every
    /// `Shell` created afterwards hold this one value.
    pub(crate) fn new() -> Arc<Self> {
        Arc::new(Self {
            hidden: AtomicBool::new(false),
            in_flight: AtomicUsize::new(0),
            offline: AtomicBool::new(false),
            shown: Arc::new(AtomicBool::new(true)),
            close_to_tray: AtomicBool::new(false),
            minimize_to_tray: AtomicBool::new(true),
        })
    }

    /// Mirror the persisted preferences. Called on every Settings toggle and
    /// on every window open, so one value answers for both.
    pub(crate) fn set_prefs(&self, close_to_tray: bool, minimize_to_tray: bool) {
        self.close_to_tray.store(close_to_tray, Ordering::Release);
        self.minimize_to_tray
            .store(minimize_to_tray, Ordering::Release);
    }

    pub(crate) fn close_to_tray(&self) -> bool {
        self.close_to_tray.load(Ordering::Acquire)
    }

    pub(crate) fn minimize_to_tray(&self) -> bool {
        self.minimize_to_tray.load(Ordering::Acquire)
    }

    pub(crate) fn is_hidden(&self) -> bool {
        self.hidden.load(Ordering::Acquire)
    }

    pub(crate) fn set_hidden(&self, hidden: bool) {
        self.hidden.store(hidden, Ordering::Release);
        // The item's own mirror is the single source for primary activation
        // and the status icon; the window controller keeps it in step.
        self.shown.store(!hidden, Ordering::Relaxed);
    }

    /// False when the registered item has no host: Close must stay a real
    /// close, and the Settings row cannot promise a tray.
    pub(crate) fn is_offline(&self) -> bool {
        self.offline.load(Ordering::Acquire)
    }

    /// Park a veto for the next Hide attempt. Counted, not flagged: two
    /// blockers must not clear each other's veto.
    pub(crate) fn retain(&self) {
        self.in_flight.fetch_add(1, Ordering::AcqRel);
    }

    /// Drop one veto. A blocker that already went away (external config
    /// save, confirm resolved) leaves nothing behind.
    pub(crate) fn release(&self) {
        let previous = self.in_flight.load(Ordering::Acquire);
        if previous == 0 {
            return;
        }
        self.in_flight.store(previous - 1, Ordering::Release);
    }

    /// Track one transfer or mutation for as long as its task runs, so
    /// Hide cannot drop the window that queue state and Live progress live
    /// in, or kill the write. The guard is taken *before* the spawn, so the
    /// window never goes between the click and the veto.
    pub(crate) fn installing(self: &Arc<Self>) -> InstallGuard {
        self.retain();
        InstallGuard {
            state: Arc::clone(self),
        }
    }

    /// One Hide decision for the titlebar Close, the compositor close, and
    /// the tray's own Hide entry.
    pub(crate) fn can_hide(&self) -> HiddenState {
        if self.in_flight.load(Ordering::Acquire) == 0 {
            HiddenState::allowed()
        } else {
            HiddenState::blocked("gui-tray-busy")
        }
    }
}

/// Released on drop: an install that outlives its window must not leave the
/// tray veto armed forever.
pub(crate) struct InstallGuard {
    state: Arc<HideState>,
}

impl Drop for InstallGuard {
    fn drop(&mut self) {
        self.state.release();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hide_is_allowed_until_a_transfer_is_in_flight() {
        let state = HideState::new();
        assert!(state.can_hide().allowed);
        let first = state.installing();
        let veto = state.can_hide();
        assert!(!veto.allowed);
        assert_eq!(veto.blocked_by, Some("gui-tray-busy"));
        drop(first);
        // The veto clears with the transfer that armed it: a finished install
        // must not leave Close permanently refused.
        assert!(state.can_hide().allowed);
    }

    #[test]
    fn two_transfers_do_not_clear_each_others_veto() {
        let state = HideState::new();
        let first = state.installing();
        let second = state.installing();
        drop(first);
        assert!(
            !state.can_hide().allowed,
            "the still-running transfer owns the veto"
        );
        drop(second);
        assert!(state.can_hide().allowed);
    }

    #[test]
    fn an_extra_release_never_underflows_into_a_permanent_veto() {
        let state = HideState::new();
        state.release();
        state.retain();
        state.release();
        state.release();
        assert!(state.can_hide().allowed);
    }

    #[test]
    fn hidden_flag_gates_background_work() {
        let state = HideState::new();
        assert!(!state.is_hidden());
        state.set_hidden(true);
        assert!(state.is_hidden());
        state.set_hidden(false);
        assert!(!state.is_hidden());
    }
}
