use gpui_kit::component::button::ButtonVariants as _;
use gpui_kit::component::h_flex;
use gpui_kit::component::Disableable as _;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use tuxgt_core::{
    apply_launch, apply_when_hook_illegal, convert_after_unplace, data_dir, has_apply_record,
    heroic_running, open_db_shared, preflight_convert_picks, proton_ge_cachy, restore_launch,
    set_handle, validate_adapter_convert, ConversionReport, FluentArgs, GameRow, StoreClient,
};

use super::super::widgets;
use super::super::{rt_block, ClientStopOp, ConfirmOp, Note, PendingConfirm, Shell, SlotChoiceOp};
use super::confirm::confirm_dests;

/// R37: how a conversion ended when it did not land. A consent request is
/// not a refusal: core validated before any write (and before the store
/// stop) and named the dests.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum AdapterOpFail {
    NeedConfirm(Vec<String>),
    NeedSlotChoice(Vec<String>),
    Other(String),
}

/// R37: the adapter cache value for a selection: the selected row's
/// persisted choice, else the schema default when nothing is selected or
/// the row is gone. One derivation, shared by startup and every re-read, so
/// the painted row cannot drift from `games.adapter`.
pub(crate) fn adapter_cache_value(games: &[GameRow], selected: Option<&str>) -> String {
    selected
        .and_then(|id| games.iter().find(|g| g.id == id))
        .map(|g| g.adapter.clone())
        .unwrap_or_else(|| tuxgt_core::ADAPTER_PRELOAD.to_string())
}

/// R37: why an Install-adapter choice is refused before it mutates
/// anything. Pure, so the exclusivity contract is testable without a Shell.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum AdapterRefusal {
    /// Hook or Apply is armed: Install parks the one-click unhook consent.
    /// The precheck only arms this for install targets.
    ArmedChannel,
    /// A store client owns the game: the ClientStop confirm must park.
    RunningClient,
    Ok,
}

/// The Launch Mode arm the game is in right now (paint and click agree).
fn current_mode(this: &Shell, game: &str) -> LaunchMode {
    LaunchMode::of(
        this.handle.get(game).copied().unwrap_or(false),
        this.applied.get(game).copied().unwrap_or(false),
    )
}

/// R37 refusal rules, in the order they apply. `running` is the caller's
/// already-resolved client state.
pub(crate) fn adapter_precheck(
    target: &str,
    mode: LaunchMode,
    client_running: bool,
) -> AdapterRefusal {
    if tuxgt_core::is_install(target) && mode != LaunchMode::Vanilla {
        // Install is the counterpart of preload; an armed channel would
        // leave the game half-converted and the channel half-armed.
        return AdapterRefusal::ArmedChannel;
    }
    if client_running {
        return AdapterRefusal::RunningClient;
    }
    AdapterRefusal::Ok
}

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

    /// Hook/Apply stay painted while that arm is live, even if needs no
    /// longer want a radio (deferred Apply restore). Vanilla + no needs
    /// still hides them.
    pub(crate) fn show_hook_apply(self, show_radio: bool) -> bool {
        show_radio || self != Self::Vanilla
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
        // A mutation vetoes a Hide like a transfer: the stub handoff would
        // kill it mid-write. Held inside the future: a local would drop on
        // return, before the op starts.
        let transfer = self.hide_state.installing();
        cx.spawn(async move |this, cx| {
            let _transfer = transfer;
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
                                    this.pending_note = Some(Note::Err(this.status.clone()));
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

    /// R37: refresh the adapter cache from the selected `GameRow`, the one
    /// place the cache is written outside startup. Called on every
    /// selection change and after every conversion result, so the painted
    /// row can never drift from `games.adapter`.
    pub(crate) fn sync_adapter_choice(&mut self) {
        self.adapter_choice = adapter_cache_value(self.games.as_ref(), self.selected.as_deref());
    }

    /// R37 Launch Mode adapter row: the persisted per-game Install adapter
    /// choice (`games.adapter`). Paints core truth — the value the setter
    /// last committed — never a pending one.
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
                let help = if tuxgt_core::is_install(adapter) {
                    self.strings.get("gui-adapter-help-install")
                } else {
                    self.strings.get("gui-adapter-help-preload")
                };
                btn = btn.tooltip(help);
            }
            btn.on_click(move |_, _, cx| {
                view.update(cx, |this, cx| this.set_adapter_ui(adapter, false, cx));
            })
        };
        h_flex()
            .gap_2()
            .items_center()
            .child(widgets::blabel(self.strings.get("gui-adapter-label"), cx))
            .child(mk(tuxgt_core::ADAPTER_PRELOAD, "adapter-preload"))
            .child(mk(tuxgt_core::ADAPTER_INSTALL, "adapter-install"))
    }

    /// R37: pick the game's Install adapter. Installs themselves run the
    /// conversion; nothing else may change this value.
    ///
    /// Order matters, and every step refuses before it mutates:
    /// 1. Hook or Apply armed -> park the one-click unhook consent
    ///    (Install only; the precheck arms it for install targets alone).
    ///    Continue validates pre-stop, then unhooks and converts under one
    ///    stop/restart; Cancel leaves the channel exactly as it was.
    /// 2. A running store client -> park the ClientStop confirm; the
    ///    conversion writes the game dir and manifests under that client.
    /// 3. Otherwise run the core transaction, then re-read core truth \u2014
    ///    the painted value follows the database, never the click.
    pub(crate) fn set_adapter_ui(
        &mut self,
        adapter: &str,
        slots_chosen: bool,
        cx: &mut Context<Self>,
    ) {
        let Some(game) = self.selected.clone() else {
            self.status = self.strings.get("gui-status-select-game");
            cx.notify();
            return;
        };
        let Ok(target) = tuxgt_core::validate_adapter(adapter) else {
            self.status = format!("unknown adapter: {adapter}");
            cx.notify();
            return;
        };
        let current = self
            .selected_game()
            .map(|g| g.adapter.as_str())
            .unwrap_or_default();
        if current == target {
            return;
        }
        tracing::debug!(action = "set-adapter", game = game.as_str(), target);
        let client_running = StoreClient::for_game(&game).is_some_and(|c| c.running());
        match adapter_precheck(target, current_mode(self, &game), client_running) {
            // One-click hooked Install: park the unhook consent instead of
            // refusing. The single Continue validates pre-stop, then unhooks
            // and converts under one stop/restart; Cancel writes nothing.
            AdapterRefusal::ArmedChannel => {
                if self.pending_confirm.is_some() {
                    self.status = self.strings.get("gui-status-resolve-confirm-first");
                    cx.notify();
                    return;
                }
                self.pending_confirm = Some(PendingConfirm::Overwrite {
                    game: game.clone(),
                    // Game-scoped like the conversion it consents to: the
                    // transaction moves every installed instance.
                    instance: String::new(),
                    op: ConfirmOp::AdapterConvertUnhook {
                        adapter: target.to_string(),
                        slots_chosen,
                        picks: Box::default(),
                    },
                    dests: Box::default(),
                });
                cx.notify();
                return;
            }
            // Running-client guard, same shape as the Launch Mode arms: the
            // conversion rewrites the game dir under a live client.
            AdapterRefusal::RunningClient => {
                if self.pending_confirm.is_some() {
                    self.status = self.strings.get("gui-status-resolve-confirm-first");
                    cx.notify();
                    return;
                }
                self.pending_confirm = Some(PendingConfirm::ClientStop {
                    game: game.clone(),
                    op: ClientStopOp::AdapterConvert {
                        adapter: target.to_string(),
                        slots_chosen,
                        picks: Box::default(),
                    },
                });
                cx.notify();
                return;
            }
            // Clear: the core transaction decides, including the no-Mods
            // case where the choice is simply persisted.
            AdapterRefusal::Ok => {}
        }
        // No consent yet: a direct click authorizes neither the stop (a
        // client that starts mid-op must fail closed, not be stopped) nor
        // a foreign game-dir overwrite (that needs its own Overwrite card).
        self.adapter_convert_op(
            game,
            target.to_string(),
            false,
            false,
            false,
            slots_chosen,
            Box::default(),
            cx,
        );
    }

    /// Post-guard conversion.
    ///
    /// `confirmed` is the ClientStop authorization the stop card passes
    /// back as true, and nothing else: it never doubles as the foreign
    /// game-dir overwrite consent. `yes` is that separate consent, set only
    /// by the Overwrite card. A client that starts between the precheck and
    /// this op therefore aborts the write (`stop_for_write` refuses
    /// unconfirmed) instead of being stopped without consent.
    ///
    /// `unhook` is the one-click hooked Install: the store setup comes out
    /// in the same stopped window the conversion runs in, so one Continue
    /// costs exactly one stop and one restart. The Continue authorizes the
    /// stop; `yes` follows the parked dests (empty on the unhook card).
    pub(crate) fn adapter_convert_op(
        &mut self,
        game: String,
        target: String,
        confirmed: bool,
        yes: bool,
        unhook: bool,
        slots_chosen: bool,
        picks: Box<[super::super::SlotPick]>,
        cx: &mut Context<Self>,
    ) {
        let name = widgets::id_label(widgets::ValKind::Adapter, &target, &self.strings);
        let mut args = FluentArgs::new();
        args.set("name", name);
        self.status = self.strings.get_args("gui-adapter-converting", Some(&args));
        let game_bg = game.clone();
        // The parked retry needs the target back on the UI thread.
        let target_bg = target.clone();
        // A mutation vetoes a Hide like a transfer: the stub handoff would
        // kill it mid-write. Held inside the future: a local would drop on
        // return, before the op starts.
        let transfer = self.hide_state.installing();
        cx.spawn(async move |this, cx| {
            let _transfer = transfer;
            // The overwrite retry needs the same picks. The background task
            // consumes the originals.
            let picks_park = picks.clone();
            let result = cx
                .background_spawn(async move {
                    rt_block(async {
                        let data = data_dir();
                        let pool = open_db_shared(&data).await?;
                        let config = tuxgt_core::config_dir();
                        let pairs: Vec<(&str, &str)> = picks
                            .iter()
                            .map(|p| (p.instance.as_str(), p.slot.as_str()))
                            .collect();
                        // Validate before stopping the store client: a
                        // refusal (missing/blocked recipe, unconsented
                        // overwrite) must surface without stopping (and
                        // restarting) Steam. The conversion re-runs the
                        // same prelude after the stop, so a recipe that
                        // vanishes in between still fails closed.
                        if let Err(e) = validate_adapter_convert(
                            &pool,
                            &data,
                            &config,
                            &game_bg,
                            &target,
                            yes,
                            slots_chosen,
                        )
                        .await
                        {
                            let op: Result<ConversionReport, AdapterOpFail> = Err(match e {
                                tuxgt_core::Error::NeedConfirm(msg) => {
                                    AdapterOpFail::NeedConfirm(confirm_dests(&msg))
                                }
                                tuxgt_core::Error::NeedSlotChoice(msg) => {
                                    AdapterOpFail::NeedSlotChoice(confirm_dests(&msg))
                                }
                                other => AdapterOpFail::Other(other.to_string()),
                            });
                            return Ok((game_bg.clone(), op, None));
                        }
                        // Prospective slot consent and a shared stem, still
                        // before the stop and before unhook. Cancel of the
                        // card therefore leaves the hook and the client alone.
                        if !pairs.is_empty() {
                            if let Err(e) = preflight_convert_picks(
                                &pool, &data, &game_bg, &target, yes, &pairs,
                            )
                            .await
                            {
                                let op: Result<ConversionReport, AdapterOpFail> = Err(match e {
                                    tuxgt_core::Error::NeedConfirm(msg) => {
                                        AdapterOpFail::NeedConfirm(confirm_dests(&msg))
                                    }
                                    tuxgt_core::Error::NeedSlotChoice(msg) => {
                                        AdapterOpFail::NeedSlotChoice(confirm_dests(&msg))
                                    }
                                    other => AdapterOpFail::Other(other.to_string()),
                                });
                                return Ok((game_bg.clone(), op, None));
                            }
                        }
                        // Same authorized stop the Launch Mode arms use: a
                        // client that started after the guard aborts the
                        // write instead of being stopped without consent.
                        let stopped = StoreClient::stop_for_write(&game_bg, confirmed)?;
                        // One-click hooked Install: the store setup comes
                        // out in the same stopped window, so the click costs
                        // exactly one stop and one restart. Unhook first: a
                        // convert failure then leaves Vanilla + the old
                        // adapter (legal), never converted + hooked. Slot
                        // unplace waits until after this, inside the convert
                        // that can put the proxy back.
                        if unhook {
                            let unhooked: Result<(), String> = async {
                                if has_apply_record(&data, &game_bg) {
                                    restore_launch(&data, &game_bg).map_err(|e| e.to_string())?;
                                }
                                let host =
                                    tuxgt_core::PluginHost::load().map_err(|e| e.to_string())?;
                                set_handle(&pool, &data, &host, &game_bg, false)
                                    .await
                                    .map_err(|e| e.to_string())
                            }
                            .await;
                            if let Err(e) = unhooked {
                                let op: Result<ConversionReport, AdapterOpFail> =
                                    Err(AdapterOpFail::Other(e));
                                return Ok((game_bg.clone(), op, stopped));
                            }
                        }
                        // The op result rides inside the Ok like every
                        // other UI op, so a refusal still repaints core
                        // truth. A foreign game-dir overwrite without
                        // consent comes back as NeedConfirm, and the Overwrite
                        // card below asks for it. Unplace of install-to-preload
                        // picks happens in here, after that validation.
                        let op: Result<ConversionReport, AdapterOpFail> = convert_after_unplace(
                            &pool,
                            &data,
                            &config,
                            &game_bg,
                            &target,
                            yes,
                            slots_chosen,
                            &pairs,
                        )
                        .await
                        .map_err(|e| match e {
                            // A consent request, not a refusal: the
                            // Overwrite card collects it and retries.
                            tuxgt_core::Error::NeedConfirm(msg) => {
                                AdapterOpFail::NeedConfirm(confirm_dests(&msg))
                            }
                            tuxgt_core::Error::NeedSlotChoice(msg) => {
                                AdapterOpFail::NeedSlotChoice(confirm_dests(&msg))
                            }
                            other => AdapterOpFail::Other(other.to_string()),
                        });
                        if let Ok(r) = &op {
                            if !r.unchanged {
                                if tuxgt_core::is_install(&target) {
                                    let host =
                                        tuxgt_core::PluginHost::load().map_err(|e| e.to_string());
                                    if let Ok(host) = host {
                                        if let Err(e) =
                                            apply_when_hook_illegal(&pool, &data, &host, &game_bg)
                                                .await
                                        {
                                            tracing::warn!(
                                                game = game_bg.as_str(),
                                                error = %e,
                                                "install adapter apply failed"
                                            );
                                        }
                                    }
                                } else if has_apply_record(&data, &game_bg) {
                                    if let Err(e) = restore_launch(&data, &game_bg) {
                                        tracing::warn!(
                                            game = game_bg.as_str(),
                                            error = %e,
                                            "preload adapter restore failed"
                                        );
                                    }
                                }
                            }
                        }
                        Ok((game_bg.clone(), op, stopped))
                    })
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                match result {
                    Ok((id, op, stopped)) => {
                        // Re-read core truth first: the row follows the
                        // database, never the click.
                        this.reload_selected_row();
                        this.reload_armed_state(&id);
                        let mut msg = match &op {
                            Ok(r) if r.unchanged => {
                                let mut a = FluentArgs::new();
                                a.set("name", r.to.clone());
                                this.strings.get_args("gui-adapter-already", Some(&a))
                            }
                            Ok(r) => {
                                let mut a = FluentArgs::new();
                                a.set("name", r.to.clone());
                                a.set("count", r.instances.len().to_string());
                                this.strings.get_args("gui-adapter-status", Some(&a))
                            }
                            // Refused or rolled back: the message is core's
                            // reason, with nothing painted as converted.
                            Err(AdapterOpFail::Other(e)) => e.clone(),
                            // Carries the restart suffix through, if a client
                            // was stopped and restarted around the refusal.
                            Err(AdapterOpFail::NeedConfirm(_)) => String::new(),
                            Err(AdapterOpFail::NeedSlotChoice(_)) => String::new(),
                        };
                        if let Some(c) = stopped {
                            match c.restart_detached() {
                                Ok(restart) if !restart.is_empty() => {
                                    msg = format!("{msg} · {restart}")
                                }
                                Ok(_) => {}
                                Err(e) => {
                                    this.status = e.to_string();
                                    this.pending_note = Some(Note::Err(this.status.clone()));
                                    this.refresh_launch_state(cx);
                                    this.refresh_mods(&id, cx);
                                    return;
                                }
                            }
                        }
                        // Foreign game-dir dests: the proxy is still in
                        // place. The retry carries the same picks and whether
                        // this call was already allowed to stop the client.
                        if let Err(AdapterOpFail::NeedConfirm(dests)) = op {
                            let mut note = this.strings.get("gui-note-overwrite-confirm");
                            if !msg.is_empty() {
                                note = format!("{msg} · {note}");
                            }
                            this.status = note.clone();
                            this.pending_confirm = Some(PendingConfirm::Overwrite {
                                game: id.clone(),
                                // The consent belongs to the game, not to one
                                // instance: the transaction moves them all.
                                instance: String::new(),
                                op: if unhook {
                                    ConfirmOp::AdapterConvertUnhook {
                                        adapter: target_bg.clone(),
                                        slots_chosen,
                                        picks: picks_park.clone(),
                                    }
                                } else {
                                    ConfirmOp::AdapterConvert {
                                        adapter: target_bg.clone(),
                                        slots_chosen,
                                        picks: picks_park,
                                        stop_confirmed: confirmed,
                                    }
                                },
                                dests: dests.into_boxed_slice(),
                            });
                            this.refresh_launch_state(cx);
                            cx.notify();
                            return;
                        }
                        // Unnamed proxies: one stem dropdown per instance,
                        // then the conversion replays with the picks.
                        if let Err(AdapterOpFail::NeedSlotChoice(instances)) = op {
                            let mut note = this.strings.get("gui-note-slot-choice");
                            if !msg.is_empty() {
                                note = format!("{msg} · {note}");
                            }
                            this.status = note.clone();
                            this.pending_confirm = Some(PendingConfirm::SlotChoice {
                                game: id.clone(),
                                op: SlotChoiceOp::AdapterConvert {
                                    adapter: target_bg.clone(),
                                    unhook,
                                },
                                picks: this.picks_for(
                                    &id,
                                    instances,
                                    if tuxgt_core::is_install(&target_bg) {
                                        super::slot_picks::PickKind::ConvertInstall
                                    } else {
                                        super::slot_picks::PickKind::ConvertPreload
                                    },
                                ),
                            });
                            this.refresh_launch_state(cx);
                            cx.notify();
                            return;
                        }
                        this.status = msg;
                        // Refused or converted: the arms and the Mods tab
                        // both re-derive from the same core read.
                        this.refresh_launch_state(cx);
                        this.refresh_mods(&id, cx);
                    }
                    Err(e) => {
                        // Infra failure before any core call (db open, or the
                        // stop timing out): nothing was written, status only.
                        this.status = format!("{e}");
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// E81 Launch Mode radio (store rows only): one exclusive arm. Hook is
    /// disabled unless the game's Proton carries umu-protonfixes (GE/Cachy);
    /// the other two arms never disable. Selecting an arm runs E80 exclusion
    /// (`select_launch_mode_ui`); Play never changes mode. Manual rows show
    /// no card: their Play wraps `tuxgt-launcher` directly.
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
        } else if !needs.hook_legal(ge) {
            self.strings.get("gui-mode-hook-install")
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
            .child(self.adapter_choice_row(view.clone(), cx))
            .when(self.is_client_game(&game_id), |this| {
                this.when(current.show_hook_apply(needs.show_radio()), |this| {
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
