use gpui_kit::*;

use tuxgt_core::{count_set_env, data_dir, global_knobs, knob_rows, open_db_shared, FluentArgs};

use super::super::{rt_block, EnvPage, Nav, SettingsTab, Shell};

impl Shell {
    /// Hero counts without the rows. General and Env load their own rows;
    /// other tabs use this.
    pub(crate) fn reload_knob_counts(&mut self, cx: &mut Context<Self>) {
        let Some(id) = self.selected.clone() else {
            self.knob_count = 0;
            self.custom_count = 0;
            self.arg_count = 0;
            return;
        };
        let owned = id.clone();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    rt_block(async {
                        let pool = open_db_shared(&data_dir()).await?;
                        let (knobs, custom) = count_set_env(&pool, &owned).await?;
                        let arg_count = tuxgt_core::arg_count(&pool, &data_dir(), &owned).await?;
                        Ok((knobs, custom, arg_count))
                    })
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.selected.as_deref() != Some(id.as_str()) || this.nav != Nav::Game {
                    return;
                }
                match result {
                    Ok((knobs, custom, arg_count)) => {
                        this.knob_count = knobs;
                        this.custom_count = custom;
                        this.arg_count = arg_count;
                    }
                    Err(e) => {
                        let mut args = FluentArgs::new();
                        args.set("error", e.to_string());
                        this.status = this.strings.get_args("gui-status-err-env", Some(&args));
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub(crate) fn reload_game_env(&mut self, cx: &mut Context<Self>) {
        let Some(id) = self.selected.clone() else {
            self.clear_env_maps();
            return;
        };
        let owned = id.clone();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    rt_block(async {
                        let pool = open_db_shared(&data_dir()).await?;
                        let knobs = knob_rows(&pool, &owned).await?;
                        let globals = global_knobs(&pool).await?;
                        let custom = tuxgt_core::custom_env(&pool, &owned).await?;
                        let args = tuxgt_core::view(&pool, &data_dir(), &owned).await?;
                        Ok((knobs, globals, custom, args))
                    })
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.selected.as_deref() != Some(id.as_str()) {
                    return;
                }
                match result {
                    Ok((knobs, globals, custom, args)) => {
                        this.apply_knob_rows(knobs, globals);
                        this.custom_count = custom.len();
                        this.arg_count = args.count;
                        if this.env_maps_showing() {
                            this.custom_env = custom.into_boxed_slice();
                            this.remember_env_groups(EnvPage::Game);
                        }
                        if this.args_showing() {
                            this.arg_rows = args.rows;
                            this.args_custom = args.custom;
                            this.args_dirty = args.dirty;
                        }
                    }
                    Err(e) => {
                        let mut args = FluentArgs::new();
                        args.set("error", e.to_string());
                        this.status = this.strings.get_args("gui-status-err-env", Some(&args));
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub(crate) fn reload_global_env(&mut self, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    rt_block(async {
                        let pool = open_db_shared(&data_dir()).await?;
                        global_knobs(&pool).await
                    })
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.nav != Nav::Settings || this.settings_tab != SettingsTab::GameEnv {
                    return;
                }
                match result {
                    Ok(globals) => {
                        this.global_knobs =
                            globals.into_iter().map(|r| (r.knob.clone(), r)).collect();
                        this.remember_env_groups(EnvPage::Global);
                    }
                    Err(e) => {
                        let mut args = FluentArgs::new();
                        args.set("error", e.to_string());
                        this.status = this.strings.get_args("gui-status-err-env", Some(&args));
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }
}
