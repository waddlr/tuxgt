use std::collections::VecDeque;
use std::time::Instant;

use super::notice::{Lane, Notice, NoticeKind, ACTIVITY_CAP};

/// Runtime store on `Shell`. E103/E104 feed it; E102 fills Live.
#[derive(Debug, Default)]
pub struct NoticeStore {
    next_id: u64,
    pub live: Vec<Notice>,
    pub activity: VecDeque<Notice>,
    pub attention: Vec<Notice>,
}

impl NoticeStore {
    fn alloc(&mut self) -> u64 {
        self.next_id += 1;
        self.next_id
    }

    /// Live card (E102 progress): uncapped, overlay + sidecar.
    pub fn emit_live(&mut self, kind: NoticeKind, text: String) -> u64 {
        let id = self.alloc();
        self.live.push(Notice::fresh(id, Lane::Live, kind, text));
        id
    }

    /// Activity toast: overlay until autohide, then last-10 sidecar.
    pub fn emit_activity(&mut self, kind: NoticeKind, text: String) -> u64 {
        let id = self.alloc();
        self.activity
            .push_back(Notice::fresh(id, Lane::Activity, kind, text));
        while self.activity.len() > ACTIVITY_CAP {
            self.activity.pop_front();
        }
        id
    }

    /// E104: drop Attention cards whose keys are not kept. Unknown keys
    /// (future families) are the caller's call — pass them through.
    pub fn retain_attention(&mut self, keep: &std::collections::HashSet<String>) {
        self.attention.retain(|n| keep.contains(&n.key));
    }

    /// Attention card: sidecar only, deduped by `key`.
    /// Same key replaces the text (no duplicate card).
    pub fn emit_attention(&mut self, key: &str, kind: NoticeKind, text: String) -> u64 {
        let id = attention_id(key);
        if let Some(n) = self.attention.iter_mut().find(|n| n.id == id) {
            n.kind = kind;
            n.text = text;
            n.created = Instant::now();
            // E104: a re-poll must not revive a session-dismiss. Only a
            // drop (retain) + new condition brings the card back.
            return id;
        }
        self.attention.push(Notice {
            id,
            lane: Lane::Attention,
            kind,
            text,
            created: Instant::now(),
            key: key.to_string(),
            progress: None,
            overlay_dismissed: false,
            snoozed_overlay: false,
            snoozed_sidecar: false,
            dismissed: false,
        });
        id
    }

    /// Overlay X: Live snoozes overlay; Activity dismisses overlay
    /// (stays in sidecar); Attention never paints overlay.
    pub fn dismiss_overlay(&mut self, id: u64) {
        if let Some(n) = self.live.iter_mut().find(|n| n.id == id) {
            n.snoozed_overlay = true;
        }
        if let Some(n) = self.activity.iter_mut().find(|n| n.id == id) {
            n.overlay_dismissed = true;
        }
    }

    /// Sidecar X: Live snoozes sidecar; Activity deletes from the 10;
    /// Attention session-dismisses.
    pub fn dismiss_sidecar(&mut self, id: u64) {
        if let Some(n) = self.live.iter_mut().find(|n| n.id == id) {
            n.snoozed_sidecar = true;
        }
        if let Some(ix) = self.activity.iter().position(|n| n.id == id) {
            self.activity.remove(ix);
        }
        if let Some(n) = self.attention.iter_mut().find(|n| n.id == id) {
            n.dismissed = true;
        }
    }

    /// Sidecar `Clear all`: empties Activity only.
    pub fn clear_activity(&mut self) {
        self.activity.clear();
    }

    /// E102: a finished Live un-snoozes as a normal Activity toast.
    pub fn finish_live(&mut self, id: u64, kind: NoticeKind, text: String) -> u64 {
        if let Some(ix) = self.live.iter().position(|n| n.id == id) {
            self.live.remove(ix);
        }
        self.emit_activity(kind, text)
    }

    /// E102: drop a Live card without a toast — the caller has its own
    /// follow-up (the E34 confirm card, or the status line). Returns whether
    /// the card was there.
    pub fn drop_live(&mut self, id: u64) -> bool {
        match self.live.iter().position(|n| n.id == id) {
            Some(ix) => {
                self.live.remove(ix);
                true
            }
            None => false,
        }
    }

    /// E102: store a Live card's percent. The value is rounded to a whole
    /// percent, so the ~100ms poll repaints at most once per 1% step.
    /// Returns whether the rendered value changed.
    pub fn set_live_progress(&mut self, id: u64, percent: Option<f32>) -> bool {
        let next = percent.map(f32::round);
        let Some(n) = self.live.iter_mut().find(|n| n.id == id) else {
            return false;
        };
        if n.progress == next {
            return false;
        }
        n.progress = next;
        true
    }

    /// Ghost bell unless Live visible (primary) or undismissed
    /// Attention (warning). Overlay + sidecar both count for Live —
    /// a snoozed-everywhere Live still owns the bell.
    pub fn bell_state(&self, now: Instant) -> BellState {
        if self
            .live
            .iter()
            .any(|n| n.on_overlay(now) || n.in_sidecar(now))
        {
            return BellState::Live;
        }
        if self.attention.iter().any(|n| !n.dismissed) {
            return BellState::Attention;
        }
        BellState::Idle
    }

    /// TopRight overlay stack, newest last.
    pub fn overlay(&self, now: Instant) -> Vec<&Notice> {
        let mut out: Vec<&Notice> = self
            .live
            .iter()
            .chain(self.activity.iter())
            .filter(|n| n.on_overlay(now))
            .collect();
        out.sort_by_key(|n| n.id);
        out
    }

    /// Sidecar rows: Live, then Activity (last 10), then Attention.
    /// A row still on the overlay is listed here too (see `in_sidecar`):
    /// the sidecar never paints together with the overlay.
    pub fn sidecar(&self, now: Instant) -> Vec<&Notice> {
        let now_live: Vec<&Notice> = self.live.iter().filter(|n| n.in_sidecar(now)).collect();
        let now_activity: Vec<&Notice> =
            self.activity.iter().filter(|n| n.in_sidecar(now)).collect();
        let now_attention: Vec<&Notice> = self
            .attention
            .iter()
            .filter(|n| n.in_sidecar(now))
            .collect();
        now_live
            .into_iter()
            .chain(now_activity)
            .chain(now_attention)
            .collect()
    }

    pub fn has_overlay(&self, now: Instant) -> bool {
        self.live
            .iter()
            .chain(self.activity.iter())
            .any(|n| n.on_overlay(now))
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BellState {
    Idle,
    Live,
    Attention,
}

/// Stable id for one Attention card key (E104 dedupes by this).
fn attention_id(key: &str) -> u64 {
    // FNV-1a: stable across runs, no new dependency.
    let mut h: u64 = 0xcbf29ce484222325;
    for b in key.bytes() {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x100000001b3);
    }
    // Avoid 0 (unused) and overlap confusion with the alloc counter start.
    if h == 0 {
        1
    } else {
        h
    }
}

#[cfg(test)]
mod tests {
    use super::super::notice::AUTOHIDE;
    use super::*;
    use std::time::Duration;

    fn now_plus(d: Duration) -> Instant {
        Instant::now() + d
    }

    #[test]
    fn info_leaves_overlay_but_stays_in_sidecar() {
        let mut s = NoticeStore::default();
        let now = Instant::now();
        let id = s.emit_activity(NoticeKind::Info, "scan done".into());
        assert_eq!(s.overlay(now).len(), 1);
        let later = now_plus(AUTOHIDE + Duration::from_secs(1));
        assert!(s.overlay(later).is_empty());
        let side = s.sidecar(later);
        assert_eq!(side.len(), 1);
        assert_eq!(side[0].id, id);
        assert_eq!(side[0].text, "scan done");
    }

    #[test]
    fn warn_err_stay_until_dismissed() {
        let mut s = NoticeStore::default();
        s.emit_activity(NoticeKind::Warn, "drift".into());
        let sticky = s.emit_activity(NoticeKind::Err, "failed".into());
        let later = now_plus(AUTOHIDE + Duration::from_secs(60));
        assert_eq!(s.overlay(later).len(), 2);
        // Overlay X clears it from the overlay; the sidecar row survives.
        s.dismiss_overlay(sticky);
        assert_eq!(s.overlay(later).len(), 1);
        assert_eq!(s.sidecar(later).len(), 2);
        // Sidecar X deletes the row outright.
        s.dismiss_sidecar(sticky);
        assert_eq!(s.sidecar(later).len(), 1);
        assert_eq!(s.sidecar(later)[0].text, "drift");
    }

    #[test]
    fn activity_caps_at_ten_oldest_evicted() {
        let mut s = NoticeStore::default();
        for i in 0..12 {
            s.emit_activity(NoticeKind::Info, format!("n{i}"));
        }
        assert_eq!(s.activity.len(), ACTIVITY_CAP);
        assert_eq!(s.activity.front().unwrap().text, "n2");
        assert_eq!(s.activity.back().unwrap().text, "n11");
    }

    #[test]
    fn clear_all_empties_activity_only() {
        let mut s = NoticeStore::default();
        s.emit_activity(NoticeKind::Info, "a".into());
        s.emit_live(NoticeKind::Info, "live".into());
        s.emit_attention("k", NoticeKind::Warn, "attn".into());
        s.clear_activity();
        assert!(s.activity.is_empty());
        assert_eq!(s.live.len(), 1);
        assert_eq!(s.attention.len(), 1);
    }

    #[test]
    fn attention_deduped_by_key_sidecar_only() {
        let mut s = NoticeStore::default();
        let now = Instant::now();
        s.emit_attention("game", NoticeKind::Warn, "v1".into());
        s.emit_attention("game", NoticeKind::Warn, "v2".into());
        assert_eq!(s.attention.len(), 1);
        assert_eq!(s.attention[0].text, "v2");
        assert!(s.overlay(now).is_empty());
        assert_eq!(s.sidecar(now).len(), 1);
        let id = s.attention[0].id;
        s.dismiss_sidecar(id);
        assert!(s.sidecar(now).is_empty());
        // E104: re-emit refreshes text but does NOT revive a
        // session-dismiss; only a drop + new condition brings it back.
        s.emit_attention("game", NoticeKind::Warn, "v3".into());
        assert_eq!(s.attention[0].text, "v3");
        assert!(s.sidecar(now).is_empty());
    }

    #[test]
    fn live_x_snoozes_never_cancels() {
        let mut s = NoticeStore::default();
        let now = Instant::now();
        let id = s.emit_live(NoticeKind::Info, "installing".into());
        s.dismiss_overlay(id);
        assert!(s.overlay(now).is_empty());
        assert_eq!(s.sidecar(now).len(), 1);
        s.dismiss_sidecar(id);
        assert!(s.sidecar(now).is_empty());
        // Still stored: finish_live converts it to Activity.
        let aid = s.finish_live(id, NoticeKind::Ok, "installed".into());
        assert!(s.live.is_empty());
        assert_eq!(s.overlay(now).len(), 1);
        assert_eq!(s.overlay(now)[0].id, aid);
    }

    #[test]
    fn live_progress_repaints_on_a_whole_percent_only() {
        let mut s = NoticeStore::default();
        let id = s.emit_live(NoticeKind::Info, "installing".into());
        // No byte total yet: indeterminate, and a repeat write is a no-op.
        assert!(!s.set_live_progress(id, None));
        assert_eq!(s.live[0].progress, None);
        // Byte total known: first write lands, sub-1% drift does not repaint.
        assert!(s.set_live_progress(id, Some(10.4)));
        assert_eq!(s.live[0].progress, Some(10.0));
        assert!(!s.set_live_progress(id, Some(10.2)));
        assert!(s.set_live_progress(id, Some(10.6)));
        assert_eq!(s.live[0].progress, Some(11.0));
        assert!(!s.set_live_progress(id, Some(11.4)));
        assert!(s.set_live_progress(id, Some(12.4)));
        // A dropped card is not resurrected by a late report.
        let gone = s.emit_live(NoticeKind::Info, "other".into());
        assert!(s.drop_live(gone));
        assert!(!s.drop_live(gone));
        assert!(!s.set_live_progress(gone, Some(50.0)));
    }

    #[test]
    fn drop_live_leaves_no_toast() {
        let mut s = NoticeStore::default();
        let now = Instant::now();
        let id = s.emit_live(NoticeKind::Info, "installing".into());
        s.dismiss_overlay(id);
        assert!(s.drop_live(id));
        assert!(s.live.is_empty());
        assert!(s.overlay(now).is_empty());
        assert!(s.sidecar(now).is_empty());
        // The bell stops claiming a Live card that is gone.
        assert_eq!(s.bell_state(now), BellState::Idle);
    }

    #[test]
    fn repoll_does_not_revive_session_dismiss() {
        // E104: a re-poll re-emits still-true keys; the card text refreshes
        // but a session-dismissed card stays dismissed until it drops.
        let mut s = NoticeStore::default();
        let now = Instant::now();
        let id = s.emit_attention("catalog:probe", NoticeKind::Warn, "v1".into());
        s.dismiss_sidecar(id);
        assert!(s.sidecar(now).is_empty());
        // Re-poll, same key: text updates, dismiss holds.
        s.emit_attention("catalog:probe", NoticeKind::Warn, "v2".into());
        assert_eq!(s.attention[0].text, "v2");
        assert!(s.sidecar(now).is_empty());
        assert_eq!(s.bell_state(now), BellState::Idle);
    }

    #[test]
    fn bell_live_beats_attention_beats_idle() {
        let mut s = NoticeStore::default();
        let now = Instant::now();
        assert_eq!(s.bell_state(now), BellState::Idle);
        s.emit_attention("k", NoticeKind::Warn, "a".into());
        assert_eq!(s.bell_state(now), BellState::Attention);
        s.emit_live(NoticeKind::Info, "l".into());
        assert_eq!(s.bell_state(now), BellState::Live);
    }
}
