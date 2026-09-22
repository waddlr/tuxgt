mod check;
mod confirm_policy;
mod resync;

use super::*;
use gpui_kit::*;
use tuxgt_core::{
    data_dir, install_instance, open_db_shared, Error, FetchProgress, FluentArgs, InstallOpts,
};

use super::super::{notice::NoticeKind, rt_block, PendingConfirm, Shell, SlotChoiceOp};
use super::slot_picks::PickKind;

/// R37: the adapter an update/repair may install with.
///
/// `None` means "let core resolve the game's persisted choice", which is
/// the only safe answer for a Mod that is not installed yet: an
/// uninstalled catalog row carries the literal `"preload"` as a *display*
/// placeholder (`load_mods`), and letting that through would silently
/// override a game whose stored choice is `install`. An installed row's
/// manifest is genuine per-instance truth and is reused, as is an explicit
/// override from a confirm retry.
pub(crate) fn update_adapter(explicit: Option<String>, row: Option<&ModRow>) -> Option<String> {
    explicit.or_else(|| row.filter(|r| r.installed).map(|r| r.adapter.clone()))
}

impl Shell {
    /// R32 update: reinstall the instance with `--redownload` semantics so the
    /// new bytes land and provenance refreshes. Keep bits of surviving dests
    /// are preserved by core; foreign game-dir overwrites confirm first.
    /// `adapter` overrides the stored intent (confirm retry); `None` reuses it.
    /// `force_staging` (from a `ConfirmOp::UpdateForce` confirm only) lets the
    /// reinstall overwrite staged config edits. A normal Update leaves it
    /// false, so per-game touches survive the redownload. Payload drift and
    /// staged edits do not park a card.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn update_mod_ui(
        &mut self,
        game: &str,
        instance: &str,
        adapter: Option<String>,
        yes: bool,
        force_staging: bool,
        password: Option<String>,
        slot: Option<String>,
        cx: &mut Context<Self>,
    ) {
        tracing::debug!(action = "update-mod", game, instance);
        let game = game.to_string();
        let instance = instance.to_string();
        let inst_cb = instance.clone();
        let done = game.clone();
        // R37: only an installed row's manifest is per-instance truth; a
        // not-installed catalog row carries a display placeholder, so a
        // fresh install must fall through to the game's persisted choice.
        let row = self
            .mods
            .get(&game)
            .and_then(|rows| rows.iter().find(|r| r.instance == instance));
        let adapter = update_adapter(adapter, row);
        let adapter_cb = adapter.clone();
        let label = self.catalog_label(&instance);
        // E102: redownload got a Live card too (same one-per-(game, instance)
        // shape as Install).
        let cell = self.open_install_live(&game, &instance, &label, cx);
        let cell_bg = cell.clone();
        let label_done = label.clone();
        let tried_password = password.is_some();
        let password_bg = password;
        let slot_cb = slot.clone();
        let slot_bg = slot;
        // R12: a redownload is the same transfer as an install — it keeps
        // running after a Hide and vetoes one while work that lives only
        // in this window is unfinished: the download, then the overwrite
        // confirm it parks (see `park_transfer_hold`). Held inside the
        // future (see install_mod_ui): a local would drop on return,
        // before the transfer starts.
        let transfer = self.hide_state.installing();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    rt_block(async {
                        let data = data_dir();
                        // Config edits do not park a card. The redownload
                        // replaces depot bytes; `force` stays false on a
                        // normal Update so per-game staged touches survive.
                        // Foreign game-dir overwrites still come back from
                        // `install_instance` because `yes` stays false.
                        let pool = open_db_shared(&data).await?;
                        let opts = InstallOpts {
                            adapter,
                            redownload: true,
                            with_requires: None,
                            yes,
                            force: force_staging,
                            password: password_bg,
                            slot: slot_bg,
                        };
                        let sink = |p: FetchProgress| {
                            if let Ok(mut slot) = cell_bg.lock() {
                                *slot = Some(p);
                            }
                        };
                        let installed = install_instance(
                            &pool,
                            &data,
                            &tuxgt_core::config_dir(),
                            &game,
                            &instance,
                            &opts,
                            Some(&sink),
                        )
                        .await?;
                        // Recount after the manifest write so the Attention
                        // card can drop in this same callback. The shared
                        // open reconciles the cache the write just dirtied.
                        let stale =
                            super::super::poll_attention::recount_game_stale(&data, &game).await;
                        Ok((installed, stale))
                    })
                })
                .await;
            let _ = this.update(cx, move |this, cx| {
                // E102: the card belongs to (game, instance), so it finishes
                // before the page-scoped early return below.
                let outcome = match &result {
                    Ok(_) => "updated",
                    Err(Error::NeedConfirm(_)) => "need-confirm",
                    Err(Error::NeedSlotChoice(_)) => "need-slot-choice",
                    Err(Error::ArchivePasswordRequired) => "need-password",
                    Err(_) => "error",
                };
                tracing::debug!(
                    action = "update-mod",
                    game = done.as_str(),
                    instance = inst_cb.as_str(),
                    outcome
                );
                match &result {
                    Ok((_, stale)) => {
                        let mut args = FluentArgs::new();
                        args.set("label", label_done.clone());
                        let text = this.strings.get_args("gui-notice-installed", Some(&args));
                        this.finish_install_live(&done, &inst_cb, NoticeKind::Ok, text, cx);
                        // The card is global, so drop it even if the user
                        // left the page before the transfer finished.
                        // A failed recount falls back to the catalog poll.
                        this.invalidate_update(&done, &inst_cb);
                        if let Some(n) = *stale {
                            this.settle_game_attention(&done, n, cx);
                        } else {
                            // The in-flight poll, if any, must not freeze
                            // the 2h timer on a snapshot from before this.
                            this.note_game_attention_unsettled(&done);
                            this.update_last_poll = None;
                            this.maybe_poll_updates(cx);
                        }
                    }
                    Err(Error::NeedConfirm(_)) if !yes => {
                        // E34 park: the overwrite card is the follow-up.
                        this.drop_install_live(&done, &inst_cb, cx);
                    }
                    Err(Error::NeedSlotChoice(_)) => {
                        this.drop_install_live(&done, &inst_cb, cx);
                    }
                    Err(Error::ArchivePasswordRequired) => {
                        this.drop_install_live(&done, &inst_cb, cx);
                    }
                    Err(e) => {
                        this.finish_install_live(
                            &done,
                            &inst_cb,
                            NoticeKind::Err,
                            format!("{e}"),
                            cx,
                        );
                    }
                }
                if this.selected.as_deref() != Some(done.as_str()) {
                    this.park_transfer_hold(transfer);
                    return;
                }
                match result {
                    Ok((m, _)) => {
                        // Verdict already dropped above, including when the
                        // user left the page. Refresh only while it is showing.
                        this.refresh_mods(&m.game, cx);
                    }
                    Err(Error::ArchivePasswordRequired) => {
                        if tried_password {
                            this.pending_note = Some(Note::Warn(
                                this.strings.get("gui-archive-password-rejected"),
                            ));
                        }
                        this.pending_archive_password = Some(PendingArchivePassword::Install {
                            game: done.clone(),
                            instance: inst_cb.clone(),
                            adapter: adapter_cb,
                            with_requires: None,
                            yes,
                            redownload: true,
                            force: force_staging,
                            slot: slot_cb,
                        });
                        this.scroll_page_top();
                    }
                    Err(Error::NeedConfirm(msg)) if !yes => {
                        // Config-overwrite markers do not park. Foreign
                        // dests still do; a forced run keeps UpdateForce
                        // so the wipe that was already consented lands.
                        if let Some((op, dests)) = confirm_policy::confirm_op_for_update(
                            &msg,
                            force_staging,
                            adapter_cb.clone(),
                            slot_cb.clone(),
                        ) {
                            this.pending_confirm = Some(PendingConfirm::Overwrite {
                                game: done.clone(),
                                instance: inst_cb.clone(),
                                op,
                                dests: dests.into_boxed_slice(),
                            });
                        }
                    }
                    Err(Error::NeedSlotChoice(msg)) => {
                        // Forced runs keep the UpdateForce retry, but the
                        // foreign leg is never done: the slot card collects
                        // no overwrite consent, so `yes` stays false and a
                        // foreign dest still parks. Unforced runs retry plain
                        // Update. Config edits do not park.
                        let op = if force_staging {
                            SlotChoiceOp::UpdateForce {
                                adapter: adapter_cb.clone(),
                                foreign_done: false,
                            }
                        } else {
                            SlotChoiceOp::Update {
                                adapter: adapter_cb.clone(),
                            }
                        };
                        this.pending_confirm = Some(PendingConfirm::SlotChoice {
                            game: done.clone(),
                            op,
                            picks: this.picks_for(&done, confirm_dests(&msg), PickKind::Recipe),
                        });
                    }
                    Err(_) => {}
                }
                cx.notify();
                this.park_transfer_hold(transfer);
            });
        })
        .detach();
    }
}
