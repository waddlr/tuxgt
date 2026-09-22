//! Hidden-tray stub (`tuxgt tray`): the dormant half of `gui.tray`.
//!
//! Hide spawns this mode and the GUI exits, so a hidden session is one
//! small SNI client instead of the whole window/GPU/font working set. The
//! stub re-reads prefs and recents (nothing is handed off); any Show
//! respawns the GUI to the persisted page and the stub leaves, and a
//! second `tuxgt gui` while hidden is answered the same way.

use std::path::Path;
use std::sync::mpsc::Receiver;
use std::sync::Arc;
use std::time::Duration;

use tuxgt_core::{data_dir, Strings};

use super::prefs::Prefs;
use super::single_instance;
use super::tray::{self, AppCommand, HideState};

/// Controller cadence, mirroring the window loop.
const POLL: Duration = Duration::from_millis(50);
/// Bounded Play drain before leaving: a click-then-Show waits up to this
/// for the spawn, then leaves anyway. Mirrors the window loop's bound.
const PLAY_DRAIN_ATTEMPTS: usize = 100;

/// Serve the hidden tray until a Show respawns the GUI or Quit ends the
/// session. `handoff` waits for the hiding GUI to release the prefix; a
/// manual start yields at once when a session already owns it.
pub(crate) fn run(strings: Strings, handoff: bool) -> Result<(), Box<dyn std::error::Error>> {
    let dir = data_dir();
    match acquire_or_yield(&dir, handoff)? {
        Some(primary) => serve(strings, primary),
        None if handoff => {
            tracing::warn!("tray stub handoff timed out: the prefix still has a session");
            Ok(())
        }
        None => {
            tracing::info!("tray stub yields: the prefix already has a session");
            Ok(())
        }
    }
}

/// Take the prefix, or yield it: handoff waits for the hiding GUI to
/// release it (past the settle budget the prefix must hold a healthy
/// visible session, so the stub leaves instead of waiting on it), while a
/// manual start yields at once to a live session.
fn acquire_or_yield(
    dir: &std::path::Path,
    handoff: bool,
) -> std::io::Result<Option<single_instance::Primary>> {
    let attempts = if handoff {
        single_instance::CONNECT_ATTEMPTS
    } else {
        1
    };
    for attempt in 0..attempts {
        match single_instance::try_listen(dir)? {
            Some(primary) => return Ok(Some(primary)),
            None if attempt + 1 < attempts => std::thread::sleep(single_instance::CONNECT_RETRY),
            None => break,
        }
    }
    Ok(None)
}

/// Respawn the window detached: same binary, same prefix env, stdio
/// inherited. Fire-and-forget — the caller always exits next, and the new
/// GUI's handshake retries across that exit.
fn spawn_gui() -> std::io::Result<()> {
    let exe = std::env::current_exe()?;
    std::process::Command::new(exe)
        .arg("gui")
        .stdin(std::process::Stdio::inherit())
        .stdout(std::process::Stdio::inherit())
        .stderr(std::process::Stdio::inherit())
        .spawn()
        .map(|_| ())
}

/// Boot page for a stub Show jump: the GUI opens to the persisted `view`,
/// so a jump rewrites it (Library keeps its list density; Settings lands
/// on the persisted sub-tab) before the respawn.
fn set_boot_view(library: bool) {
    let mut prefs = Prefs::load();
    prefs.view = if library {
        if prefs.is_list() {
            "library-list".into()
        } else {
            "library".into()
        }
    } else {
        "settings".into()
    };
    prefs.save();
}

fn serve(
    strings: Strings,
    primary: single_instance::Primary,
) -> Result<(), Box<dyn std::error::Error>> {
    let running = primary.start();
    let dir = data_dir();
    // In-flight tray Plays, drained (bounded) before any respawn exit so a
    // click-then-Show never silently drops the spawn. Counted on this
    // thread: dispatch increments, the outcome drain decrements.
    let mut plays_outstanding: usize = 0;
    // The stub is born hidden: the menu must offer Show, never Hide.
    let hide_state = HideState::new();
    let prefs = Prefs::load();
    hide_state.set_prefs(prefs.close_to_tray, prefs.minimize_to_tray);
    hide_state.set_hidden(true);
    let (commands_tx, commands) = std::sync::mpsc::channel();
    let recents: tray::RecentSnapshot = Arc::new(std::sync::RwLock::new(Vec::new()));
    let _tray = match tray::spawn(
        &strings,
        Arc::clone(&hide_state),
        commands_tx,
        Arc::clone(&recents),
    ) {
        Some(tray) => tray,
        // No SNI host (the watcher vanished under the handoff): hand the
        // session back to a visible window instead of ending it; only a
        // failed respawn ends here as a clean Quit-equivalent.
        None => {
            tracing::warn!("tray stub has no SNI host; respawning the GUI");
            if let Err(error) = spawn_gui() {
                tracing::error!(%error, "tray stub gui respawn failed");
            }
            return Ok(());
        }
    };
    tray::refresh_recents(&recents);
    let (outcome_tx, outcomes) = std::sync::mpsc::channel();
    tracing::info!(action = "tray-stub", "serving hidden tray");
    loop {
        while let Ok(command) = commands.try_recv() {
            match command {
                AppCommand::Quit => return Ok(()),
                // Already hidden; the menu never offers it. Ignore.
                AppCommand::Hide => {}
                AppCommand::Show => {
                    drain_play_wait(&outcomes, &strings, &dir, &mut plays_outstanding);
                    match spawn_gui() {
                        Ok(()) => return Ok(()),
                        Err(error) => {
                            tracing::error!(%error, "tray stub gui respawn failed")
                        }
                    }
                }
                AppCommand::ShowLibrary => {
                    set_boot_view(true);
                    drain_play_wait(&outcomes, &strings, &dir, &mut plays_outstanding);
                    match spawn_gui() {
                        Ok(()) => return Ok(()),
                        Err(error) => {
                            tracing::error!(%error, "tray stub gui respawn failed")
                        }
                    }
                }
                AppCommand::ShowSettings => {
                    set_boot_view(false);
                    drain_play_wait(&outcomes, &strings, &dir, &mut plays_outstanding);
                    match spawn_gui() {
                        Ok(()) => return Ok(()),
                        Err(error) => {
                            tracing::error!(%error, "tray stub gui respawn failed")
                        }
                    }
                }
                AppCommand::Play(id) => {
                    tracing::info!(action = "tray-play", game = id.as_str());
                    plays_outstanding += 1;
                    tray::spawn_play_worker(id, &recents, outcome_tx.clone());
                }
            }
        }
        drain_play_outcomes(&outcomes, &strings, &dir, &mut plays_outstanding);
        // A second `tuxgt gui` while hidden is the way back in: boot it and
        // leave; its handshake retries across this exit.
        while let Ok(request) = running.receiver.try_recv() {
            drain_play_wait(&outcomes, &strings, &dir, &mut plays_outstanding);
            match spawn_gui() {
                Ok(()) => {
                    let _ = request.ack.send(true);
                    // The accept thread writes the ack after this send; a
                    // beat before exiting keeps the exit from racing it
                    // (a lost ack still heals via handshake retry).
                    std::thread::sleep(POLL);
                    return Ok(());
                }
                Err(error) => {
                    tracing::error!(%error, "tray stub gui respawn failed");
                    let _ = request.ack.send(false);
                }
            }
        }
        std::thread::sleep(POLL);
    }
}

/// Drain finished tray Plays: a failure waits in the prefix for the next
/// window's status line; success needs nothing (the worker already
/// refreshed the snapshot).
fn drain_play_outcomes(
    outcomes: &Receiver<tray::TrayPlayOutcome>,
    strings: &Strings,
    dir: &Path,
    outstanding: &mut usize,
) {
    while let Ok(outcome) = outcomes.try_recv() {
        *outstanding -= 1;
        if let Err(error) = outcome.result {
            tracing::error!(
                game = outcome.id.as_str(),
                error = %error,
                "tray play failed"
            );
            let mut args = tuxgt_core::FluentArgs::new();
            args.set("error", error.to_string());
            let text = strings.get_args("gui-status-err-play", Some(&args));
            single_instance::stash_tray_status(dir, &text);
        }
    }
}

/// Wait for in-flight tray Plays (bounded) before leaving: a
/// click-then-Show must not silently drop the spawn. Outcomes drain
/// normally while waiting, so a Play failure still parks its status line.
fn drain_play_wait(
    outcomes: &Receiver<tray::TrayPlayOutcome>,
    strings: &Strings,
    dir: &Path,
    outstanding: &mut usize,
) {
    for _ in 0..PLAY_DRAIN_ATTEMPTS {
        drain_play_outcomes(outcomes, strings, dir, outstanding);
        if *outstanding == 0 {
            return;
        }
        std::thread::sleep(POLL);
    }
    tracing::warn!(
        plays = *outstanding,
        "tray stub leaving with plays in flight"
    );
}

#[cfg(test)]
mod tests {
    use super::set_boot_view;
    use super::Prefs;

    /// Stub Show jumps rewrite the boot page the GUI opens to: Library
    /// keeps its list density, Settings leaves the sub-tab alone.
    /// `TUXGT_CONFIG` scopes the prefs; nothing here needs a window.
    #[test]
    fn show_jumps_rewrite_the_boot_page() {
        static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let dir = std::env::temp_dir().join(format!("tuxgt-tray-jump-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temp config");
        let prev = std::env::var_os("TUXGT_CONFIG");
        std::env::set_var("TUXGT_CONFIG", &dir);

        let mut prefs = Prefs::load();
        prefs.view = "game".into();
        prefs.settings_tab = "mods".into();
        prefs.save();
        set_boot_view(true);
        assert_eq!(Prefs::load().view, "library");
        set_boot_view(false);
        let prefs = Prefs::load();
        assert_eq!(prefs.view, "settings");
        assert_eq!(prefs.settings_tab, "mods");

        let mut prefs = Prefs::load();
        prefs.view = "library-list".into();
        prefs.save();
        set_boot_view(true);
        assert_eq!(Prefs::load().view, "library-list");

        match prev {
            Some(v) => std::env::set_var("TUXGT_CONFIG", v),
            None => std::env::remove_var("TUXGT_CONFIG"),
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// `acquire_or_yield` takes a free prefix, yields at once to a live
    /// session without serving (serving would block forever, so returning
    /// is the assertion), and waits out a handoff release. Temp dirs only;
    /// nothing here touches env or the bus.
    #[test]
    fn acquire_takes_yields_and_waits() {
        let dir = std::env::temp_dir().join(format!("tuxgt-tray-acquire-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temp prefix");
        // Free: take it.
        let held = super::acquire_or_yield(&dir, false).expect("take free prefix");
        assert!(held.is_some());
        drop(held);
        // Live: a manual start yields at once instead of serving.
        let live = super::single_instance::take_settled(&dir);
        let yielded = super::acquire_or_yield(&dir, false).expect("yield to live session");
        assert!(yielded.is_none());
        // Handoff: a release mid-wait is taken.
        let (tx, rx) = std::sync::mpsc::channel();
        let dir_bg = dir.clone();
        std::thread::spawn(move || {
            let taken = super::acquire_or_yield(&dir_bg, true).expect("handoff wait");
            let _ = tx.send(taken.is_some());
        });
        std::thread::sleep(std::time::Duration::from_millis(100));
        drop(live);
        assert!(
            rx.recv_timeout(std::time::Duration::from_secs(10))
                .expect("handoff result"),
            "a release mid-wait is taken"
        );
        // Handoff with no release: yields after the settle budget (~2s).
        let live = super::single_instance::take_settled(&dir);
        let timed_out = super::acquire_or_yield(&dir, true).expect("handoff timeout");
        assert!(timed_out.is_none());
        drop(live);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
