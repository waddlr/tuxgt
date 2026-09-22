use gpui_kit::*;
use tuxgt_core::{data_dir, open_db_shared, set_file_load, FluentArgs};

use super::super::{rt_block, Shell};

impl Shell {
    /// Switch one installed dest between LoadDLL and IncludeFile (this game).
    pub(crate) fn set_file_load_ui(
        &mut self,
        game: &str,
        instance: &str,
        dest: &str,
        load: bool,
        cx: &mut Context<Self>,
    ) {
        tracing::debug!(action = "file-load", game, instance, dest, load);
        let game = game.to_string();
        let instance = instance.to_string();
        let dest = dest.to_string();
        let inst_cb = instance.clone();
        let dest_cb = dest.clone();
        let done = game.clone();
        let transfer = self.hide_state.installing();
        cx.spawn(async move |this, cx| {
            let _transfer = transfer;
            let result = cx
                .background_spawn(async move {
                    rt_block(async {
                        let data = data_dir();
                        let pool = open_db_shared(&data).await?;
                        set_file_load(&pool, &data, &game, &instance, &dest, load).await
                    })
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.selected.as_deref() != Some(done.as_str()) {
                    return;
                }
                let outcome = match &result {
                    Ok(_) => "file-load-set",
                    Err(_) => "error",
                };
                tracing::debug!(
                    action = "file-load",
                    game = done.as_str(),
                    instance = inst_cb.as_str(),
                    dest = dest_cb.as_str(),
                    outcome
                );
                match result {
                    Ok(m) => {
                        this.refresh_mods(&m.game, cx);
                        let mut args = FluentArgs::new();
                        args.set("instance", m.instance.clone());
                        args.set("dest", dest_cb.clone());
                        let key = if load {
                            "gui-status-file-load"
                        } else {
                            "gui-status-file-include"
                        };
                        this.status = this.strings.get_args(key, Some(&args));
                    }
                    Err(e) => {
                        this.refresh_mods(&done, cx);
                        this.status = format!("{e}");
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }
}
