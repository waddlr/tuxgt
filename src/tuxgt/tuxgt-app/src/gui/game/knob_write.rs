use gpui_kit::*;
use tuxgt_core::{
    data_dir, disable_global_knob, disable_knob, enable_global_knob, enable_knob, open_db_shared,
    set_global_knob, set_knob, sync_handle_sessions, unset_global_knob, unset_knob, FluentArgs,
    KnobRow,
};

use super::super::{rt_block, EnvPage, Shell};

impl Shell {
    pub(crate) fn write_knob(
        &mut self,
        id: &str,
        page: EnvPage,
        value: Option<&str>,
        cx: &mut Context<Self>,
    ) {
        match page {
            EnvPage::Global => self.write_global_knob(id, value, cx),
            EnvPage::Game => self.write_game_knob(id, value, false, cx),
        }
    }

    /// Empty enabled value: session writes `VAR=` so global/unmanaged does not apply.
    pub(crate) fn write_game_omit(&mut self, id: &str, cx: &mut Context<Self>) {
        self.write_game_knob(id, Some(""), true, cx);
    }

    pub(crate) fn write_game_knob(
        &mut self,
        id: &str,
        value: Option<&str>,
        omit: bool,
        cx: &mut Context<Self>,
    ) {
        let Some(game) = self.selected.clone() else {
            return;
        };
        let id = id.to_string();
        let unset = !omit && value == Some("");
        let explicit = value.filter(|v| !v.is_empty()).map(|s| s.to_string());
        let done = game.clone();
        tracing::debug!(action = "write-knob", game = game.as_str(), key = id.as_str(), value = ?explicit, omit);
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    rt_block(async {
                        let pool = open_db_shared(&data_dir()).await?;
                        let host = tuxgt_core::PluginHost::load()?;
                        tuxgt_core::mutate_game(&pool, &data_dir(), &host, &game, async {
                            if unset {
                                unset_knob(&pool, &game, &id).await
                            } else {
                                let host = tuxgt_core::PluginHost::load()?;
                                let k = tuxgt_core::find_enabled_knob(&host, &id)
                                    .ok_or_else(|| tuxgt_core::Error::UnknownKnob(id.clone()))?;
                                let v = if omit {
                                    String::new()
                                } else {
                                    tuxgt_core::resolve_value(k, explicit.as_deref())?
                                };
                                set_knob(&pool, &game, &id, &v).await
                            }
                        })
                        .await?;
                        let rows = tuxgt_core::knob_rows(&pool, &game).await?;
                        let globals = tuxgt_core::global_knobs(&pool).await?;
                        Ok((id, rows, globals))
                    })
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.selected.as_deref() != Some(done.as_str()) {
                    return;
                }
                match result {
                    Ok((id, rows, globals)) => {
                        this.apply_knob_rows(rows, globals);
                        this.session_payload
                            .insert(done.clone(), super::load_payload(&done));
                        this.reload_armed_state(&done);
                        this.reload_launch_state(cx);
                        let mut args = FluentArgs::new();
                        args.set("id", id.clone());
                        this.status = this.strings.get_args("gui-status-knob", Some(&args));
                    }
                    Err(e) => this.status = format!("{e}"),
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub(crate) fn write_game_enabled(&mut self, id: &str, on: bool, cx: &mut Context<Self>) {
        let Some(game) = self.selected.clone() else {
            return;
        };
        let id = id.to_string();
        tracing::debug!(action = "write-knob-enabled", game = game.as_str(), key = id.as_str(), on);
        let done = game.clone();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    rt_block(async {
                        let pool = open_db_shared(&data_dir()).await?;
                        let host = tuxgt_core::PluginHost::load()?;
                        tuxgt_core::mutate_game(&pool, &data_dir(), &host, &game, async {
                            if on {
                                enable_knob(&pool, &game, &id).await
                            } else {
                                disable_knob(&pool, &game, &id).await
                            }
                        })
                        .await?;
                        let rows = tuxgt_core::knob_rows(&pool, &game).await?;
                        let globals = tuxgt_core::global_knobs(&pool).await?;
                        Ok((id, rows, globals))
                    })
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.selected.as_deref() != Some(done.as_str()) {
                    return;
                }
                match result {
                    Ok((id, rows, globals)) => {
                        this.apply_knob_rows(rows, globals);
                        this.session_payload
                            .insert(done.clone(), super::load_payload(&done));
                        this.reload_armed_state(&done);
                        this.reload_launch_state(cx);
                        let mut args = FluentArgs::new();
                        args.set("id", id);
                        this.status = this.strings.get_args("gui-status-knob", Some(&args));
                    }
                    Err(e) => this.status = format!("{e}"),
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub(crate) fn write_global_knob(
        &mut self,
        id: &str,
        value: Option<&str>,
        cx: &mut Context<Self>,
    ) {
        let id = id.to_string();
        let unset = value == Some("");
        let explicit = value.filter(|v| !v.is_empty()).map(|s| s.to_string());
        tracing::debug!(action = "write-global-knob", key = id.as_str(), value = ?explicit);
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    rt_block(async {
                        let pool = open_db_shared(&data_dir()).await?;
                        if unset {
                            unset_global_knob(&pool, &id).await?;
                        } else {
                            let host = tuxgt_core::PluginHost::load()?;
                            let k = tuxgt_core::find_enabled_knob(&host, &id)
                                .ok_or_else(|| tuxgt_core::Error::UnknownKnob(id.clone()))?;
                            let v = tuxgt_core::resolve_value(k, explicit.as_deref())?;
                            set_global_knob(&pool, &id, &v).await?;
                        }
                        let host = tuxgt_core::PluginHost::load()?;
                        sync_handle_sessions(&pool, &data_dir(), &host).await?;
                        let globals = tuxgt_core::global_knobs(&pool).await?;
                        Ok((id, globals))
                    })
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                match result {
                    Ok((_id, globals)) => {
                        this.global_knobs =
                            globals.into_iter().map(|r| (r.knob.clone(), r)).collect();
                        this.status = this.strings.get("gui-status-env-restart");
                        // E94: `sync_handle_sessions` may have auto-restored
                        // the selected game's arm; repaint from core truth.
                        if let Some(game) = this.selected.clone() {
                            this.reload_armed_state(&game);
                        }
                    }
                    Err(e) => this.status = format!("{e}"),
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub(crate) fn write_global_enabled(&mut self, id: &str, on: bool, cx: &mut Context<Self>) {
        let id = id.to_string();
        tracing::debug!(action = "write-global-enabled", key = id.as_str(), on);
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    rt_block(async {
                        let pool = open_db_shared(&data_dir()).await?;
                        if on {
                            enable_global_knob(&pool, &id).await?;
                        } else {
                            disable_global_knob(&pool, &id).await?;
                        }
                        let host = tuxgt_core::PluginHost::load()?;
                        sync_handle_sessions(&pool, &data_dir(), &host).await?;
                        let globals = tuxgt_core::global_knobs(&pool).await?;
                        Ok(globals)
                    })
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                match result {
                    Ok(globals) => {
                        this.global_knobs =
                            globals.into_iter().map(|r| (r.knob.clone(), r)).collect();
                        this.status = this.strings.get("gui-status-env-restart");
                        // E94: same core-truth repaint as the value writer.
                        if let Some(game) = this.selected.clone() {
                            this.reload_armed_state(&game);
                        }
                    }
                    Err(e) => this.status = format!("{e}"),
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub(crate) fn apply_knob_rows(&mut self, rows: Vec<KnobRow>, globals: Vec<KnobRow>) {
        self.knob_count = rows
            .iter()
            .filter(|r| r.enabled && !r.value.is_empty())
            .count();
        // Counts stay fresh everywhere (hero chip); the maps refill only
        // while showing — a tab switch mid-flight must not re-inflate
        // dropped per-tab state.
        if self.env_maps_showing() {
            self.knob_values = rows
                .iter()
                .map(|r| (r.knob.clone(), r.value.clone()))
                .collect();
            self.knob_enabled = rows.iter().map(|r| (r.knob.clone(), r.enabled)).collect();
            self.global_knobs = globals.into_iter().map(|r| (r.knob.clone(), r)).collect();
        }
    }
}
