use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use gpui_kit::*;

use tuxgt_core::{FetchProgress, FluentArgs};

use super::super::game::LIVE_TICK_MS;
use super::super::notice::NoticeKind;
use super::super::{Note, Shell};

/// In-flight self-update Live card. The fetch thread writes `progress` and
/// reads `cancel`; a UI pump paints the bar.
pub(crate) struct AppUpdateLive {
    pub(crate) id: u64,
    generation: u64,
    pub(crate) progress: Arc<Mutex<Option<FetchProgress>>>,
    pub(crate) cancel: Arc<AtomicBool>,
}

impl Shell {
    /// Cancel is offered while the fetch is still moving. At 100% the
    /// overlay has started and the flag is ignored.
    pub(crate) fn app_update_can_cancel(&self) -> bool {
        let Some(live) = self.app_update_live.as_ref() else {
            return false;
        };
        match live
            .progress
            .lock()
            .ok()
            .and_then(|slot| *slot)
            .and_then(|p| p.percent())
        {
            Some(p) if p >= 100.0 => false,
            _ => true,
        }
    }

    /// Abort the in-flight fetch. Overlay has already started if the flag
    /// is raised too late; that path ignores cancel.
    pub(crate) fn cancel_app_update_ui(&mut self, _cx: &mut Context<Self>) {
        tracing::debug!(action = "cancel-app-update");
        if let Some(live) = &self.app_update_live {
            live.cancel.store(true, Ordering::SeqCst);
        }
    }

    pub(crate) fn open_app_update_live(
        &mut self,
        tag: &str,
        cx: &mut Context<Self>,
    ) -> AppUpdateLive {
        let mut args = FluentArgs::new();
        args.set("tag", tag.to_string());
        let text = self
            .strings
            .get_args("gui-notice-updating-app", Some(&args));
        let id = match self.app_update_live.as_ref() {
            Some(live) => live.id,
            None => self.emit_live(NoticeKind::Info, text, cx),
        };
        self.app_update_live_seq += 1;
        let generation = self.app_update_live_seq;
        let live = AppUpdateLive {
            id,
            generation,
            progress: Arc::new(Mutex::new(None)),
            cancel: Arc::new(AtomicBool::new(false)),
        };
        self.spawn_app_update_pump(id, generation, live.progress.clone(), cx);
        self.app_update_live = Some(AppUpdateLive {
            id: live.id,
            generation: live.generation,
            progress: live.progress.clone(),
            cancel: live.cancel.clone(),
        });
        live
    }

    fn spawn_app_update_pump(
        &mut self,
        id: u64,
        generation: u64,
        cell: Arc<Mutex<Option<FetchProgress>>>,
        cx: &mut Context<Self>,
    ) {
        cx.spawn(async move |this, cx| loop {
            cx.background_executor()
                .timer(std::time::Duration::from_millis(LIVE_TICK_MS))
                .await;
            let alive = this.update(cx, |this, cx| {
                let Some(live) = this.app_update_live.as_ref() else {
                    return false;
                };
                if live.generation != generation || live.id != id {
                    return false;
                }
                let pct = cell
                    .lock()
                    .ok()
                    .and_then(|slot| *slot)
                    .and_then(|p| p.percent());
                this.set_live_progress(id, pct, cx);
                true
            });
            if !matches!(alive, Ok(true)) {
                break;
            }
        })
        .detach();
    }

    pub(crate) fn finish_app_update_live(
        &mut self,
        kind: NoticeKind,
        text: String,
        cx: &mut Context<Self>,
    ) {
        let Some(live) = self.app_update_live.take() else {
            self.pending_note = Some(match kind {
                NoticeKind::Info => Note::Info(text),
                NoticeKind::Ok => Note::Ok(text),
                NoticeKind::Warn => Note::Warn(text),
                NoticeKind::Err => Note::Err(text),
            });
            return;
        };
        self.finish_live(live.id, kind, text, cx);
    }
}
