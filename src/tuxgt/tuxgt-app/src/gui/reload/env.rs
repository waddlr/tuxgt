use gpui_kit::*;

use tuxgt_core::{count_set_env, data_dir, global_knobs, knob_rows, open_db_shared, FluentArgs};

use super::super::{rt_block, Nav, SettingsTab, Shell};

impl Shell {
    /// Hero env counts for the selected game, without the rows. Runs on
    /// every game select; the Env tab loads full rows instead.
    pub(crate) fn reload_knob_counts(&mut self, cx: &mut Context<Self>) {
        let Some(id) = self.selected.clone() else {
            self.knob_count = 0;
            self.custom_count = 0;
            return;
        };
        let owned = id.clone();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    rt_block(async {
                        let pool = open_db_shared(&data_dir()).await?;
                        count_set_env(&pool, &owned).await
                    })
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.selected.as_deref() != Some(id.as_str()) || this.nav != Nav::Game {
                    return;
                }
                match result {
                    Ok((knobs, custom)) => {
                        this.knob_count = knobs;
                        this.custom_count = custom;
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
                        Ok((knobs, globals, custom))
                    })
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.selected.as_deref() != Some(id.as_str()) {
                    return;
                }
                match result {
                    Ok((knobs, globals, custom)) => {
                        this.apply_knob_rows(knobs, globals);
                        this.custom_count = custom.len();
                        if this.env_maps_showing() {
                            this.custom_env = custom.into_boxed_slice();
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
