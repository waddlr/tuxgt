use super::super::Shell;

mod card;
mod ops;
mod pending;

impl Shell {
    /// R12: consuming the parked confirm releases the veto it held — the
    /// op Confirm retries takes a guard of its own, and Cancel drops the
    /// batch. Every site that clears the stash goes through here so the
    /// veto and the work it protects cannot drift apart.
    pub(crate) fn clear_pending_confirm(&mut self) {
        self.pending_confirm = None;
        self.install_hold = None;
    }
}

/// Dests after the message's first colon (`pfx:` dests stage verbatim, so
/// everything past the first colon is the list). All current producers use
/// a colon-free prefix (`foreign game-dir dests`, `config-overwrite`,
/// `need-slot` — whose list holds instance ids, not dests).
pub(crate) fn confirm_dests(msg: &str) -> Vec<String> {
    let dests: Vec<String> = msg
        .split_once(':')
        .map(|(_, rest)| rest)
        .unwrap_or("")
        .split(", ")
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect();
    if dests.is_empty() {
        vec![msg.to_string()]
    } else {
        dests
    }
}

/// Truncated dest label for conflict rows (60 chars + `…`).
pub(crate) fn dest_short(dest: &str) -> String {
    const CAP: usize = 60;
    if dest.chars().count() <= CAP {
        dest.to_string()
    } else {
        format!("{}…", dest.chars().take(CAP).collect::<String>())
    }
}
