use std::io;
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, Mutex};

use super::super::super::single_instance::{self, Running};
use super::super::super::tray::{
    self, refresh_recents, tray_play_hides, AppCommand, HideState, TrayPlayOutcome,
};
use super::super::super::*;
use super::super::*;
use super::build::build_window;
use super::{Labels, LiveWindow};

/// Bounded Play drain before the handoff exit: a click-then-Hide waits up
/// to this for the spawn, then hands off anyway. Mirrors the stub's bound.
const PLAY_DRAIN_ATTEMPTS: usize = 100;

/// Process-level GUI state shared by the window builder and the controller
/// loop: the resolved strings, tray plumbing, and the GPU preference for the
/// open-failure diagnostic.
pub(super) struct Ctx {
    pub(super) strings: Strings,
    pub(super) labels: Labels,
    pub(super) hide_state: Arc<HideState>,
    pub(super) tray_commands: Sender<AppCommand>,
    pub(super) recents: tray::RecentSnapshot,
    pub(super) pending_status: Arc<Mutex<Option<String>>>,
    pub(super) tray_registered: bool,
    pub(super) gpu: GpuPref,
}

/// The one window controller: owns the live window (if any), the tray
/// command drain, Play outcomes, and second-process focus requests.
pub(super) struct Controller {
    ctx: Ctx,
    tray_requests: Receiver<AppCommand>,
    outcome_tx: Sender<TrayPlayOutcome>,
    play_outcomes: Receiver<TrayPlayOutcome>,
    plays_outstanding: usize,
    focus_running: Option<Running>,
    window: Option<LiveWindow>,
}

impl Controller {
    /// Assemble the controller from the process-level tray plumbing the
    /// `app.run` closure owns (it holds the tray handle itself).
    pub(super) fn boot(
        strings: Strings,
        labels: Labels,
        hide_state: Arc<HideState>,
        tray_commands: Sender<AppCommand>,
        recents: tray::RecentSnapshot,
        tray_registered: bool,
        gpu: GpuPref,
        focus_running: Option<Running>,
        tray_requests: Receiver<AppCommand>,
    ) -> Self {
        // A tray Play failure with no window to toast into waits here for the
        // next Show's status line — in-process while visible, parked in the
        // prefix while hidden (the stub stashes, this boot takes).
        let pending_status: Arc<Mutex<Option<String>>> =
            Arc::new(Mutex::new(single_instance::take_tray_status(&data_dir())));
        let (outcome_tx, play_outcomes) = std::sync::mpsc::channel();
        // In-flight tray Plays, drained (bounded) before the handoff exit.
        let plays_outstanding: usize = 0;
        Self {
            ctx: Ctx {
                strings,
                labels,
                hide_state,
                tray_commands,
                recents,
                pending_status,
                tray_registered,
                gpu,
            },
            tray_requests,
            outcome_tx,
            play_outcomes,
            plays_outstanding,
            focus_running,
            window: None,
        }
    }

    /// Open the one window, then serve tray commands, Play outcomes, and
    /// focus requests until Quit or a Hide handoff.
    pub(super) async fn drive(mut self, cx: &mut AsyncApp, gpu_env: CpuEnvRestore) {
        // One window for the whole process. Show while visible focuses
        // the window it already has; it never opens a second one.
        let window = cx.update(|cx| build_window(&self.ctx, cx));
        self.window = window;
        // The first window owns its renderer now (later opens reuse the
        // platform's cached adapter choice), so the forced software vars
        // are restored: games and tools spawned from here get a clean
        // environment. Runs on the failure path too; quitting either way.
        drop(gpu_env);
        refresh_recents(&self.ctx.recents);
        loop {
            // The tray menu, its primary activation, and the Shell's
            // close/minimize paths all land in this one drain.
            while let Ok(command) = self.tray_requests.try_recv() {
                match command {
                    AppCommand::Quit => {
                        tracing::info!(action = "quit", source = "tray");
                        cx.update(|cx| cx.quit());
                        return;
                    }
                    AppCommand::Show => self.show(cx),
                    AppCommand::ShowLibrary => self.show_library(cx),
                    AppCommand::ShowSettings => self.show_settings(cx),
                    AppCommand::Play(id) => self.play(id),
                    AppCommand::Hide => {
                        if self.hide(cx).await {
                            return;
                        }
                    }
                }
            }
            if self.drain_outcomes(cx) && self.hide(cx).await {
                return;
            }
            self.drain_focus(cx);
            cx.background_executor()
                .timer(std::time::Duration::from_millis(50))
                .await;
        }
    }

    /// Open-or-focus shared by the Show arms: a missing window opens (and
    /// clears the hidden gate); a live one just comes forward.
    fn open_or_activate(&mut self, cx: &mut AsyncApp) {
        if self.window.is_none() {
            let window = cx.update(|cx| build_window(&self.ctx, cx));
            self.window = window;
            if self.window.is_some() {
                self.ctx.hide_state.set_hidden(false);
            }
        } else if let Some(current) = self.window.as_ref() {
            current.activate(cx);
        }
    }

    fn show(&mut self, cx: &mut AsyncApp) {
        // The flag is the gate on background work, so a
        // Show must clear it: the window it just opened
        // reads the same due-ness and polls then.
        self.open_or_activate(cx);
        refresh_recents(&self.ctx.recents);
    }

    fn show_library(&mut self, cx: &mut AsyncApp) {
        self.open_or_activate(cx);
        // Jumping while visible navigates; it never reopens.
        if let Some(current) = self.window.as_ref() {
            let _ = current.shell.update(cx, |shell, cx| shell.jump_library(cx));
        }
        refresh_recents(&self.ctx.recents);
    }

    fn show_settings(&mut self, cx: &mut AsyncApp) {
        self.open_or_activate(cx);
        // Lands on the persisted sub-tab, same as a boot
        // to Settings — never the toggle-back `enter_settings`
        // answers with when already there.
        let tab = SettingsTab::from_pref(&Prefs::load().settings_tab);
        if let Some(current) = self.window.as_ref() {
            let _ = current.shell.update(cx, |shell, cx| {
                if shell.nav == Nav::Settings {
                    if shell.settings_tab != tab {
                        shell.switch_settings_tab(tab, cx);
                    }
                } else {
                    shell.settings_tab = tab;
                    shell.enter_settings(cx);
                }
            });
        }
        refresh_recents(&self.ctx.recents);
    }

    fn play(&mut self, id: String) {
        tracing::info!(action = "tray-play", game = id.as_str());
        self.plays_outstanding += 1;
        tray::spawn_play_worker(id, &self.ctx.recents, self.outcome_tx.clone());
    }

    /// Hide, or refuse with the reason in the status line. `true` means the
    /// handoff spawned and the caller must quit.
    async fn hide(&mut self, cx: &mut AsyncApp) -> bool {
        // The tray item is the only way back, so a Hide
        // is refused while an in-flight transfer would
        // lose its queue: the window stays and says why.
        let veto = self.ctx.hide_state.can_hide();
        if !veto.allowed {
            // The tray's own click already cleared the
            // shown mirror, and the window never went
            // away, so put the mirror back.
            self.ctx.hide_state.set_hidden(false);
            if let (Some(id), Some(current)) = (veto.blocked_by, self.window.as_ref()) {
                current.report(self.ctx.strings.get(id), cx);
            }
            return false;
        }
        // Dormant handoff: the tray stub is a fresh process
        // that never maps the window, GPU, or font state,
        // so hidden RSS is one small SNI client instead of
        // the whole GUI. The stub re-reads prefs and
        // recents; any Show respawns the GUI to the
        // persisted page.
        // In-flight tray Plays drain first (bounded): a
        // click-then-Hide must not silently drop the
        // spawn. Failures log; toasts die with the
        // window.
        for _ in 0..PLAY_DRAIN_ATTEMPTS {
            while let Ok(outcome) = self.play_outcomes.try_recv() {
                self.plays_outstanding -= 1;
                if let Err(error) = outcome.result {
                    tracing::error!(
                        game = outcome.id.as_str(),
                        error = %error,
                        "tray play failed"
                    );
                }
            }
            if self.plays_outstanding == 0 {
                break;
            }
            cx.background_executor()
                .timer(std::time::Duration::from_millis(50))
                .await;
        }
        if self.plays_outstanding != 0 {
            tracing::warn!(
                plays = self.plays_outstanding,
                "tray handoff leaving with plays in flight"
            );
        }
        match spawn_tray_stub() {
            Ok(()) => {
                tracing::info!(action = "hide", source = "tray");
                cx.update(|cx| cx.quit());
                true
            }
            Err(error) => {
                // Self-spawn failed (the binary vanished
                // mid-run): stay visible, like a veto
                // without the status line.
                tracing::error!(%error, "tray handoff spawn failed");
                false
            }
        }
    }

    /// Tray Play outcomes: a success hides the open window when the
    /// launch preference is on (true: the caller runs the `hide()`
    /// handoff, which re-checks the veto). A failure toasts into the
    /// live window, or waits for the next Show's status line while
    /// hidden; it never hides.
    fn drain_outcomes(&mut self, cx: &mut AsyncApp) -> bool {
        let mut hide = false;
        while let Ok(outcome) = self.play_outcomes.try_recv() {
            self.plays_outstanding -= 1;
            match outcome.result {
                Ok(()) => {
                    let ready = self.ctx.tray_registered && !self.ctx.hide_state.is_offline();
                    if tray_play_hides(Prefs::load().hide_on_launch, self.window.is_some(), ready) {
                        hide = true;
                    }
                }
                Err(error) => {
                    tracing::error!(
                        game = outcome.id.as_str(),
                        error = %error,
                        "tray play failed"
                    );
                    let mut args = tuxgt_core::FluentArgs::new();
                    args.set("error", error.to_string());
                    let text = self
                        .ctx
                        .strings
                        .get_args("gui-status-err-play", Some(&args));
                    match self.window.as_ref() {
                        Some(current) => current.report(text, cx),
                        None => {
                            if let Ok(mut pending) = self.ctx.pending_status.lock() {
                                *pending = Some(text);
                            }
                        }
                    }
                }
            }
        }
        hide
    }

    /// A second process asks for focus; with no window open that
    /// request is also the way back in.
    fn drain_focus(&mut self, cx: &mut AsyncApp) {
        if let Some(running) = self.focus_running.as_ref() {
            while let Ok(request) = running.receiver.try_recv() {
                if self.window.is_none() {
                    let window = cx.update(|cx| build_window(&self.ctx, cx));
                    self.window = window;
                    // Same gate as a tray Show: this window is on
                    // screen, so background work resumes.
                    if self.window.is_some() {
                        self.ctx.hide_state.set_hidden(false);
                        refresh_recents(&self.ctx.recents);
                    }
                }
                let activated = self
                    .window
                    .as_ref()
                    .is_some_and(|current| current.activate(cx));
                let _ = request.ack.send(activated);
            }
        }
    }
}

/// Spawn the hidden-tray stub (`tuxgt tray --handoff`) detached: same
/// binary, same prefix env, stdio inherited. The caller quits; the stub
/// takes single-instance primary once this process releases it.
fn spawn_tray_stub() -> io::Result<()> {
    let exe = std::env::current_exe()?;
    std::process::Command::new(exe)
        .arg("tray")
        .arg("--handoff")
        .stdin(std::process::Stdio::inherit())
        .stdout(std::process::Stdio::inherit())
        .stderr(std::process::Stdio::inherit())
        .spawn()
        .map(|_| ())
}
