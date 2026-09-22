//! E101: owned notification store — three lanes, one TopRight stack.
//!
//! The kit `NotificationList` (`window.push_notification`) cannot
//! expire-without-delete or host a sidecar, so the app owns its toast
//! stack. Runtime only: process death wipes it (no persist).
//!
//! Lanes (two filters, never paint overlay + sidecar together):
//! - Live: overlay unless overlay-snoozed, sidecar unless sidecar-snoozed.
//!   Uncapped. X snoozes (never cancels — E102 downloads keep running).
//! - Activity: overlay until autohide (info/ok 5s; warn/err until
//!   dismissed), then last 10 in the sidecar. Cap 10, oldest evicted.
//!   Overlay X dismisses the overlay (stays in sidecar); sidecar X deletes.
//! - Attention: sidecar only, never overlay. Uncapped, deduped by id.
//!   X is session-dismiss. E104 fills this lane from the catalog poll.

use std::time::{Duration, Instant};

/// info/ok autohide delay on the overlay.
pub const AUTOHIDE: Duration = Duration::from_secs(5);
/// Activity sidecar cap: oldest evicted past this.
pub const ACTIVITY_CAP: usize = 10;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum NoticeKind {
    Info,
    Ok,
    Warn,
    Err,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Lane {
    Live,
    Activity,
    Attention,
}

/// One notice in the store. `text` is the resolved Fluent string.
#[derive(Clone, Debug)]
pub struct Notice {
    pub id: u64,
    pub lane: Lane,
    pub kind: NoticeKind,
    pub text: String,
    pub created: Instant,
    /// Attention only: the emit key (`catalog:<id>` / `game:<id>`).
    /// Other lanes leave it empty; E104 navigation matches on it.
    pub key: String,
    /// Live only: 0–100 percent, rounded to whole percent so a repaint needs a
    /// visible step (E102 coalescing). `None` = indeterminate — no byte total
    /// yet, so the bar must not invent a percent.
    pub progress: Option<f32>,
    /// Overlay X on Activity (warn/err) or expiry bookkeeping.
    pub overlay_dismissed: bool,
    /// Live only: overlay X snoozes the overlay, sidecar keeps it.
    pub snoozed_overlay: bool,
    /// Live only: sidecar X snoozes the sidecar, overlay keeps it.
    pub snoozed_sidecar: bool,
    /// Attention only: X is session-dismiss.
    pub dismissed: bool,
}
impl Notice {
    pub(crate) fn fresh(id: u64, lane: Lane, kind: NoticeKind, text: String) -> Self {
        Self {
            id,
            lane,
            kind,
            text,
            key: String::new(),
            created: Instant::now(),
            progress: None,
            overlay_dismissed: false,
            snoozed_overlay: false,
            snoozed_sidecar: false,
            dismissed: false,
        }
    }

    /// info/ok leave the overlay after 5s; warn/err stay until dismissed.
    pub fn overlay_expired(&self, now: Instant) -> bool {
        match self.kind {
            NoticeKind::Info | NoticeKind::Ok => now.duration_since(self.created) >= AUTOHIDE,
            NoticeKind::Warn | NoticeKind::Err => false,
        }
    }

    /// Painted on the TopRight overlay stack.
    pub fn on_overlay(&self, now: Instant) -> bool {
        match self.lane {
            Lane::Live => !self.snoozed_overlay,
            Lane::Activity => !self.overlay_dismissed && !self.overlay_expired(now),
            Lane::Attention => false,
        }
    }

    /// Listed in the sidecar. The sidecar never paints together with the
    /// overlay, so a row that is still on the overlay is listed here too —
    /// otherwise opening the bell would hide a live warn/err with no X.
    pub fn in_sidecar(&self, _now: Instant) -> bool {
        match self.lane {
            Lane::Live => !self.snoozed_sidecar,
            Lane::Activity => true,
            Lane::Attention => !self.dismissed,
        }
    }
}
