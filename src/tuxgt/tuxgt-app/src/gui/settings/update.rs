use gpui_kit::*;

use tuxgt_core::{
    apply_app_update, check_app_update, data_dir, AppUpdateStatus, Error, FetchProgress, FluentArgs,
};

use super::super::notice::NoticeKind;
use super::super::rt_block;
use super::super::{Note, Shell};

/// About update button state. One button morphs through these; a failed
/// check toasts and falls back to `Unknown`, a failed apply back to
/// `Available` (still retryable). `Updated` is terminal for the process:
/// the running binary is old until restart.
#[derive(Clone, Debug, Default)]
pub(crate) enum AppUpdate {
    #[default]
    Unknown,
    Checking,
    Available {
        tag: String,
        url: String,
    },
    UpToDate,
    Updating,
    Updated {
        tag: String,
    },
}

impl Shell {
    /// Attention key for one available app tag.
    fn app_attention_key(tag: &str) -> String {
        format!("app:{tag}")
    }

    /// Drop `app:` Attention cards, keeping `keep` when given (a manual
    /// check that moved tags keeps the new card; an apply clears them all).
    fn prune_app_attention(&mut self, keep: Option<&str>) {
        let held: std::collections::HashSet<String> = self
            .notices
            .attention
            .iter()
            .map(|n| n.key.clone())
            .filter(|k| !k.starts_with("app:") || Some(k.as_str()) == keep)
            .collect();
        self.notices.retain_attention(&held);
    }

    /// Drop every `app:` Attention card (applied, or no longer available).
    pub(crate) fn drop_app_attention(&mut self) {
        self.prune_app_attention(None);
    }

    fn card_app_available(&mut self, tag: &str, cx: &mut Context<Self>) {
        let mut args = FluentArgs::new();
        args.set("tag", tag.to_string());
        let text = self.strings.get_args("gui-notice-update-app", Some(&args));
        let key = Self::app_attention_key(tag);
        self.emit_attention(&key, NoticeKind::Warn, text, cx);
    }

    /// E104 ride-along: fold one app check into state + Attention. `Updating`
    /// and `Updated` are terminal for this process, so the poll never
    /// overwrites them; `Unknown` preserves both the state and the card.
    pub(crate) fn apply_app_poll(
        &mut self,
        status: AppUpdateStatus,
        keep: &mut std::collections::HashSet<String>,
        cx: &mut Context<Self>,
    ) {
        if matches!(
            self.app_update,
            AppUpdate::Updating | AppUpdate::Updated { .. }
        ) {
            self.pass_app_attention(keep);
            return;
        }
        match status {
            AppUpdateStatus::Available { tag, asset_url, .. } => {
                self.card_app_available(&tag, cx);
                keep.insert(Self::app_attention_key(&tag));
                self.app_update = AppUpdate::Available {
                    tag,
                    url: asset_url,
                };
            }
            AppUpdateStatus::UpToDate { .. } => {
                // A manual check in flight still gets its say; don't let
                // the poll drop its card out from under it.
                if matches!(self.app_update, AppUpdate::Checking) {
                    self.pass_app_attention(keep);
                }
                self.app_update = AppUpdate::UpToDate;
            }
            AppUpdateStatus::Unknown { .. } => {
                self.pass_app_attention(keep);
            }
        }
    }

    /// Keep existing `app:` cards across a poll that learned nothing new.
    fn pass_app_attention(&self, keep: &mut std::collections::HashSet<String>) {
        for n in self.notices.attention.iter() {
            if n.key.starts_with("app:") {
                keep.insert(n.key.clone());
            }
        }
    }

    /// Manual About check (the E104 poll run covers the automatic one).
    /// Cards on `Available` like the poll does, so the button and the bell
    /// never disagree.
    pub(crate) fn check_app_update_ui(&mut self, cx: &mut Context<Self>) {
        tracing::debug!(action = "check-app-update");
        if !matches!(
            self.app_update,
            AppUpdate::Unknown | AppUpdate::UpToDate | AppUpdate::Available { .. }
        ) {
            return;
        }
        self.app_update = AppUpdate::Checking;
        cx.notify();
        cx.spawn(async move |this, cx| {
            // The check is infallible; the `Ok` wrap fits `rt_block`.
            let result = cx
                .background_spawn(async move { rt_block(async { Ok(check_app_update().await) }) })
                .await;
            let _ = this.update(cx, |this, cx| {
                match result {
                    Ok(AppUpdateStatus::Available { tag, asset_url, .. }) => {
                        let key = Self::app_attention_key(&tag);
                        this.card_app_available(&tag, cx);
                        this.prune_app_attention(Some(&key));
                        this.app_update = AppUpdate::Available {
                            tag,
                            url: asset_url,
                        };
                    }
                    Ok(AppUpdateStatus::UpToDate { .. }) => {
                        this.app_update = AppUpdate::UpToDate;
                        this.drop_app_attention();
                    }
                    Ok(AppUpdateStatus::Unknown { reason }) => {
                        this.app_update = AppUpdate::Unknown;
                        let mut args = FluentArgs::new();
                        args.set("error", reason);
                        this.pending_note = Some(Note::Warn(
                            this.strings
                                .get_args("gui-status-err-app-check", Some(&args)),
                        ));
                    }
                    // Unreachable (the check never fails); kept so a future
                    // fallible check degrades to Unknown instead of panicking.
                    Err(e) => {
                        this.app_update = AppUpdate::Unknown;
                        let mut args = FluentArgs::new();
                        args.set("error", e.to_string());
                        this.pending_note = Some(Note::Warn(
                            this.strings
                                .get_args("gui-status-err-app-check", Some(&args)),
                        ));
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Run the apply for the held `Available` state. Anything else
    /// re-checks (a stale notice click) except `Updating`/`Updated`,
    /// which ignore the click. The `app:` Attention card drops now —
    /// action taken — and a Live card tracks the fetch.
    pub(crate) fn apply_app_update_ui(&mut self, cx: &mut Context<Self>) {
        tracing::debug!(action = "apply-app-update");
        let (tag, url) = match &self.app_update {
            AppUpdate::Available { tag, url } => (tag.clone(), url.clone()),
            AppUpdate::Updating | AppUpdate::Updated { .. } => return,
            _ => {
                self.check_app_update_ui(cx);
                return;
            }
        };
        self.drop_app_attention();
        self.app_update = AppUpdate::Updating;
        let live = self.open_app_update_live(&tag, cx);
        let cell_bg = live.progress.clone();
        let cancel_bg = live.cancel.clone();
        cx.notify();
        // A mutation vetoes a Hide like a transfer: the stub handoff would
        // kill it mid-write. Held inside the future: a local would drop on
        // return, before the op starts.
        let transfer = self.hide_state.installing();
        let (tag_bg, url_bg) = (tag.clone(), url.clone());
        cx.spawn(async move |this, cx| {
            let _transfer = transfer;
            let result = cx
                .background_spawn(async move {
                    let sink = |p: FetchProgress| {
                        if let Ok(mut slot) = cell_bg.lock() {
                            *slot = Some(p);
                        }
                    };
                    rt_block(apply_app_update(
                        &data_dir(),
                        &tag_bg,
                        &url_bg,
                        Some(&sink),
                        Some(cancel_bg.as_ref()),
                    ))
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                match result {
                    Ok(rep) => {
                        this.app_update = AppUpdate::Updated {
                            tag: rep.tag.clone(),
                        };
                        this.drop_app_attention();
                        this.refresh_host_install();
                        let mut args = FluentArgs::new();
                        args.set("tag", rep.tag);
                        let text = this.strings.get_args("gui-status-app-updated", Some(&args));
                        this.finish_app_update_live(NoticeKind::Ok, text, cx);
                    }
                    Err(Error::Cancelled) => {
                        this.app_update = AppUpdate::Available {
                            tag: tag.clone(),
                            url,
                        };
                        this.card_app_available(&tag, cx);
                        let text = this.strings.get("gui-status-app-update-cancelled");
                        this.finish_app_update_live(NoticeKind::Info, text, cx);
                    }
                    Err(e) => {
                        this.app_update = AppUpdate::Available {
                            tag: tag.clone(),
                            url,
                        };
                        this.card_app_available(&tag, cx);
                        let mut args = FluentArgs::new();
                        args.set("error", e.to_string());
                        let text = this
                            .strings
                            .get_args("gui-status-err-app-update", Some(&args));
                        this.finish_app_update_live(NoticeKind::Err, text, cx);
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }
}
