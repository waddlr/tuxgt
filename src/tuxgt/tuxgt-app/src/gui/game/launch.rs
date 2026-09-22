use gpui_kit::component::button::ButtonVariants as _;
#[cfg(any())] // R37: gated with the `FluentArgs` import below.
use gpui_kit::component::h_flex;
use gpui_kit::component::Disableable as _;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use tuxgt_core::{
    apply_launch, data_dir, has_apply_record, heroic_running, open_db_shared, proton_ge_cachy,
    restore_launch, set_handle,
};
// R37: adapter-choice UI is hidden (MAP row 127); the row + setter below are
// kept but compiled out. Revive by deleting the four `cfg(any())` gates.
#[cfg(any())]
use tuxgt_core::FluentArgs;

use super::super::widgets;
use super::super::{rt_block, ClientStopOp, Note, Shell};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum LaunchMode {
    Hook,
    Apply,
    Vanilla,
}
impl LaunchMode {
    /// E80 precedence: handle wins, then an Apply record, else vanilla.
    /// Paint and click-time agree here so the two cannot drift.
    pub(crate) fn of(handled: bool, applied: bool) -> Self {
        if handled {
            LaunchMode::Hook
        } else if applied {
            LaunchMode::Apply
        } else {
            LaunchMode::Vanilla
        }
    }
}
impl Shell {
    pub(crate) fn select_launch_mode_ui(&mut self, mode: LaunchMode, cx: &mut Context<Self>) {
        let Some(game) = self.selected.clone() else {
            self.status = self.strings.get("gui-status-select-game");
            cx.notify();
            return;
        };
        tracing::debug!(action = "select-launch-mode", game = game.as_str(), mode = ?mode);
        // Click-time arm: exclusion reads the store while the selection can
        // move, and re-selecting the painted arm is a no-op (re-Apply would
        // only re-toast the client restart).
        let current = LaunchMode::of(
            self.handle.get(&game).copied().unwrap_or(false),
            self.applied.get(&game).copied().unwrap_or(false),
        );
        if current == mode {
            return;
        }
        let ge = proton_ge_cachy(self.selected_game().and_then(|g| g.proton.as_deref()));
        let needs = self.launch_needs;
        // E94: Not hooked is always legal — it pauses injection and leaves
        // mods/knobs untouched. Hook and Update Launch Options stay
        // constrained by needs.
        let legal = match mode {
            LaunchMode::Hook => needs.hook_legal(ge),
            LaunchMode::Apply => needs.apply_legal(),
            LaunchMode::Vanilla => true,
        };
        if !legal {
            return;
        }
        if mode == LaunchMode::Hook {
            self.adapter_choice = "preload".to_string();
        }
        let was_applied = self.applied.get(&game).copied().unwrap_or(false);
        // Running-client guard via the shared park helper: a store write
        // under a live client is discarded by its in-memory flush. Hook /
        // Vanilla only touch the store through the record when one exists.
        let (op, writes_store) = match mode {
            LaunchMode::Apply => (ClientStopOp::ApplyMode, true),
            LaunchMode::Vanilla => (ClientStopOp::VanillaMode, was_applied),
            LaunchMode::Hook => (ClientStopOp::HookMode, was_applied),
        };
        if self.park_client_stop(&game, op, writes_store, cx) {
            return;
        }
        self.launch_mode_op_ui(game, mode, false, cx);
    }

    /// Post-guard Launch Mode op: the exclusion body below runs after the
    /// running-client guard (or the ClientStop confirm it parks on).
    /// `confirmed` is the ClientStop authorization the confirm card passes
    /// back as true.
    pub(crate) fn launch_mode_op_ui(
        &mut self,
        game: String,
        mode: LaunchMode,
        confirmed: bool,
        cx: &mut Context<Self>,
    ) {
        let was_applied = self.applied.get(&game).copied().unwrap_or(false);
        // E70: arming the hook needs the protonfixes `localfixes` hook.
        // Snapshot the warning now; it lands below only when Hook applies.
        let health = if mode == LaunchMode::Hook {
            self.host_health_note()
        } else {
            None
        };
        // E70: the Apply op ran even on error, so drift still warns (never
        // blocks). Only the Apply arm warns on failure.
        let warn_on_error = mode == LaunchMode::Apply;
        let mode_bg = mode;
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    rt_block(async {
                        let pool = open_db_shared(&data_dir()).await?;
                        let host = tuxgt_core::PluginHost::load()?;
                        // The op result rides alongside, never instead of, the
                        // reloaded maps: E80 exclusion mutates the hook channel
                        // even when the store write fails (Apply clears Handle
                        // first), so the painted arm must come from a re-read.
                        // Authorized stop via `stop_for_write`: without
                        // confirm a client that started mid-op aborts the
                        // write (nothing written). Hook/Vanilla without a
                        // record touch no store file: set_handle runs bare.
                        let stopped = if mode_bg == LaunchMode::Apply || was_applied {
                            tuxgt_core::StoreClient::stop_for_write(&game, confirmed)?
                        } else {
                            None
                        };
                        let op: Result<Option<String>, String> = match mode_bg {
                            LaunchMode::Hook => set_handle(&pool, &data_dir(), &host, &game, true)
                                .await
                                .map(|_| None)
                                .map_err(|e| e.to_string()),
                            LaunchMode::Apply => apply_launch(&pool, &data_dir(), &host, &game)
                                .await
                                .map(Some)
                                .map_err(|e| e.to_string()),
                            LaunchMode::Vanilla => {
                                // Restore failure rides the inner Result like
                                // Hook/Apply: the maps below still reload, so
                                // a failed Not-hooked repaints core truth.
                                let restore: Result<Option<String>, String> = if was_applied {
                                    restore_launch(&data_dir(), &game)
                                        .map(Some)
                                        .map_err(|e| e.to_string())
                                } else {
                                    Ok(None)
                                };
                                match restore {
                                    Err(e) => Err(e),
                                    Ok(op) => set_handle(&pool, &data_dir(), &host, &game, false)
                                        .await
                                        .map(|_| op)
                                        .map_err(|e| e.to_string()),
                                }
                            }
                        };
                        let handled = tuxgt_core::game_handle(&pool, &game).await.unwrap_or(false);
                        let applied = has_apply_record(&data_dir(), &game);
                        // The client was stopped on confirm: restart it even
                        // when the op failed, and surface a restart failure
                        // over the op report.
                        let restart = match stopped {
                            Some(c) => c.restart_detached().map_err(|e| e.to_string()),
                            None => Ok(String::new()),
                        };
                        Ok((game, handled, applied, op, stopped.is_some(), restart))
                    })
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                match result {
                    Ok((id, handled, applied, op, stopped, restart)) => {
                        this.handle.insert(id.clone(), handled);
                        this.applied.insert(id.clone(), applied);
                        match op {
                            Ok(report) => {
                                // Client-restart toast on Apply/Restore
                                // transitions: both carry a report, Hook
                                // reports nothing.
                                // Fresh client after our stop/restart: the
                                // wrapper is on disk, no toast needed.
                                if report.is_some() && !stopped {
                                    this.note_heroic_restart(&id);
                                }
                                if handled {
                                    // E70: warning only; the arm already applied.
                                    if let Some(n) = health {
                                        this.pending_note = Some(n);
                                    }
                                }
                                if applied {
                                    // E70: trampoline assumes a healthy host
                                    // install. Warning only; the Apply itself
                                    // already ran.
                                    this.note_host_health();
                                }
                                if let Some(report) = report {
                                    this.status = report;
                                }
                            }
                            Err(err) => {
                                // Maps above already carry the post-failure
                                // truth (e.g. a failed Apply still cleared
                                // Handle), so the painted arm matches core and
                                // the failed arm stays re-selectable.
                                this.status = err;
                                if warn_on_error {
                                    this.note_host_health();
                                }
                            }
                        }
                        if stopped {
                            match restart {
                                Ok(msg) => {
                                    if !msg.is_empty() {
                                        this.status = format!("{} · {msg}", this.status);
                                    }
                                }
                                Err(e) => {
                                    // The client was stopped on confirm and
                                    // failed to restart: this dominates.
                                    this.status = e;
                                    this.pending_note =
                                        Some(Note::Err(this.status.clone()));
                                }
                            }
                        }
                        // The store changed under this op: re-read the About
                        // reference (launch options + wrappers + needs) so it
                        // shows disk truth. Guarded on selection inside.
                        this.refresh_launch_state(cx);
                    }
                    Err(e) => {
                        // Infra failure before any store write (db/host load,
                        // or the client stop timing out); maps are provably
                        // unchanged, status only.
                        this.status = format!("{e}");
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// After a Heroic Apply, warn while Heroic runs: it snapshots GamesConfig
    /// at startup, so its next settings save would drop the wrapper.
    pub(crate) fn note_heroic_restart(&mut self, game_id: &str) {
        let running = heroic_running();
        if game_id.starts_with("heroic:") && running {
            let s = self.strings.get("gui-status-heroic-restart");
            self.pending_note = Some(Note::Warn(s));
        }
    }

    /// Launch composition shared by plain Play and Enable & Play. `game` pins
    /// the click-time id (Enable & Play's arm is async, so the selection can
    /// move); `None` means the current selection. `apply_report` carries the
    /// arm text into the status on the Enable & Play path; plain Play passes
    /// `None` for both and keeps its exact status.
    #[cfg(any())] // R37: gated with the `FluentArgs` import above.
    pub(crate) fn adapter_choice_row(&self, view: Entity<Self>, cx: &App) -> impl IntoElement {
        let current = self.adapter_choice.clone();
        let mk = |adapter: &'static str, id: &'static str| {
            let selected = current == adapter;
            let label = widgets::id_label(widgets::ValKind::Adapter, adapter, &self.strings);
            let view = view.clone();
            let mut btn = widgets::btn(id, cx).child(widgets::blabel(label, cx));
            btn = if selected {
                btn.primary()
            } else {
                btn.secondary()
            };
            if selected {
                let help = if adapter == "install" {
                    self.strings.get("gui-adapter-help-install")
                } else {
                    self.strings.get("gui-adapter-help-preload")
                };
                btn = btn.tooltip(help);
            }
            btn.on_click(move |_, _, cx| {
                view.update(cx, |this, cx| this.set_adapter_ui(adapter, cx));
            })
        };
        h_flex()
            .gap_2()
            .items_center()
            .child(widgets::blabel(self.strings.get("gui-adapter-label"), cx))
            .child(mk("preload", "adapter-preload"))
            .child(mk("install", "adapter-install"))
    }

    /// E81 Launch Mode radio (store rows only): one exclusive arm. Hook is
    /// disabled unless the game's Proton carries umu-protonfixes (GE/Cachy);
    /// the other two arms never disable. Selecting an arm runs E80 exclusion
    /// (`select_launch_mode_ui`); Play never changes mode. Manual rows show
    /// no card: their Play wraps `tuxgt-launcher` directly.
    #[cfg(any())] // R37: gated with the `FluentArgs` import above.
    pub(crate) fn set_adapter_ui(&mut self, adapter: &str, cx: &mut Context<Self>) {
        if self.adapter_choice == adapter {
            return;
        }
        self.adapter_choice = adapter.to_string();
        let mut args = FluentArgs::new();
        args.set("name", adapter.to_string());
        self.status = self.strings.get_args("gui-adapter-status", Some(&args));
        // Install never uses Hook; Hook is preload-only.
        if adapter == "install" {
            let game = self.selected.clone().unwrap_or_default();
            if self.handle.get(&game).copied().unwrap_or(false) {
                self.select_launch_mode_ui(LaunchMode::Vanilla, cx);
                return;
            }
        }
        cx.notify();
    }

    pub(crate) fn launch_mode_section(&self, view: Entity<Self>, cx: &App) -> impl IntoElement {
        let game_id = self.selected.clone().unwrap_or_default();
        let current = LaunchMode::of(
            self.handle.get(&game_id).copied().unwrap_or(false),
            self.applied.get(&game_id).copied().unwrap_or(false),
        );
        let ge = proton_ge_cachy(self.selected_game().and_then(|g| g.proton.as_deref()));
        let needs = self.launch_needs;
        let option =
            |mode: LaunchMode, id: &'static str, title: String, desc: String, enabled: bool| {
                let selected = current == mode;
                let view = view.clone();
                let mut btn = widgets::btn(id, cx).child(widgets::blabel(title, cx));
                btn = if selected {
                    btn.primary()
                } else {
                    btn.secondary()
                };
                // Same one-selected group recipe as the adapter buttons below:
                // no handler while disabled, so the dead arm cannot arm.
                btn = btn.disabled(!enabled);
                if enabled {
                    btn = btn.on_click(move |_, _, cx| {
                        view.update(cx, |this, cx| {
                            this.select_launch_mode_ui(mode, cx);
                        });
                    });
                }
                btn.tooltip(desc)
            };
        let hook_tip = if !ge {
            self.strings.get("gui-mode-hook-disabled")
        } else if needs.argv_wrappers {
            self.strings.get("gui-mode-hook-wrappers")
        } else {
            self.strings.get("gui-mode-hook-desc")
        };
        let vanilla_tip = self.strings.get("gui-mode-vanilla-desc");
        widgets::section_card("launch-mode", cx)
            .h_full()
            .child(widgets::section_title(
                self.strings.get("gui-section-launch-mode"),
                cx,
            ))
            .when(self.is_client_game(&game_id), |this| {
                this.when(needs.install_only, |this| {
                    this.child(widgets::muted(
                        self.strings.get("gui-note-launch-install-only"),
                        cx,
                    ))
                })
                .when(needs.show_radio(), |this| {
                    this.child(option(
                        LaunchMode::Hook,
                        "launch-mode-hook",
                        self.strings.get("gui-mode-hook"),
                        hook_tip,
                        needs.hook_legal(ge),
                    ))
                    .child(option(
                        LaunchMode::Apply,
                        "launch-mode-apply",
                        self.strings.get("gui-mode-apply"),
                        self.strings.get("gui-mode-apply-desc"),
                        needs.apply_legal(),
                    ))
                })
                // E94: Not hooked is always painted and enabled; picking it
                // pauses injection without touching management state.
                .child(option(
                    LaunchMode::Vanilla,
                    "launch-mode-vanilla",
                    self.strings.get("gui-mode-vanilla"),
                    vanilla_tip,
                    true,
                ))
            })
    }
}
