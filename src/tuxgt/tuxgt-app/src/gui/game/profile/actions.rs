use gpui_kit::*;
use tuxgt_core::{
    create_copy, create_empty, data_dir, delete_profile, open_db_shared, rename_profile, DEFAULT_ID,
};

use super::super::super::{rt_block, Shell};
use super::dialog::{open_files, open_restore};
use super::{this_view, NewStart};

impl Shell {
    pub(crate) fn create_and_use(
        &mut self,
        name: String,
        start: NewStart,
        base: String,
        cx: &mut Context<Self>,
    ) {
        let Some(game) = self.selected.clone() else {
            return;
        };
        let transfer = self.hide_state.installing();
        cx.spawn(async move |this, cx| {
            let _transfer = transfer;
            let game_bg = game.clone();
            let result = cx
                .background_spawn(async move {
                    rt_block(async {
                        let pool = open_db_shared(&data_dir()).await?;
                        if start == NewStart::Empty {
                            create_empty(&pool, &data_dir(), &game_bg, &name).await
                        } else {
                            create_copy(&pool, &data_dir(), &game_bg, &name, &base).await
                        }
                    })
                })
                .await;
            let _ = this.update(cx, |this, cx| match result {
                Ok(profile) => this.spawn_preview(game, profile.id, profile.name, cx),
                Err(e) => {
                    this.status = format!("{e}");
                    cx.notify();
                }
            });
        })
        .detach();
    }

    pub(crate) fn rename_go(&mut self, name: String, new_name: String, cx: &mut Context<Self>) {
        let Some(game) = self.selected.clone() else {
            return;
        };
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    rt_block(async {
                        let pool = open_db_shared(&data_dir()).await?;
                        rename_profile(&pool, &data_dir(), &game, &name, &new_name).await
                    })
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                match result {
                    Ok(_) => this.reload_profiles(cx),
                    Err(e) => this.status = format!("{e}"),
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub(crate) fn delete_go(
        &mut self,
        name: String,
        yes_files: bool,
        yes_restore: bool,
        cx: &mut Context<Self>,
    ) {
        let Some(game) = self.selected.clone() else {
            return;
        };
        let transfer = self.hide_state.installing();
        cx.spawn(async move |this, cx| {
            let _transfer = transfer;
            let game_bg = game.clone();
            let name_bg = name.clone();
            let result = cx
                .background_spawn(async move {
                    rt_block(async {
                        let pool = open_db_shared(&data_dir()).await?;
                        delete_profile(
                            &pool,
                            &data_dir(),
                            &game_bg,
                            &name_bg,
                            yes_files,
                            yes_restore,
                        )
                        .await
                    })
                })
                .await;
            let _ = this.update_in(cx, |this, window, cx| match result {
                Ok(out) => {
                    let active = out
                        .set
                        .by_id(DEFAULT_ID)
                        .map(|p| p.name.clone())
                        .unwrap_or_else(|| "Default".to_string());
                    this.after_profile_switch(
                        &game,
                        &active,
                        out.restore_deferred,
                        out.rearm_deferred,
                        cx,
                    );
                }
                Err(tuxgt_core::Error::NeedConfirm(msg))
                    if msg.starts_with("profile-files:") && !yes_files =>
                {
                    let dests = msg
                        .split_once(':')
                        .map(|(_, rest)| rest)
                        .unwrap_or(&msg)
                        .split(',')
                        .map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty())
                        .collect();
                    open_files(this_view(cx), this.strings.clone(), window, cx, dests, {
                        let name = name.clone();
                        move |this, cx| this.delete_go(name.clone(), true, false, cx)
                    });
                }
                Err(tuxgt_core::Error::NeedConfirm(msg))
                    if msg.starts_with("profile-restore:") && !yes_restore =>
                {
                    open_restore(this_view(cx), this.strings.clone(), window, cx, {
                        let name = name.clone();
                        move |this, cx| this.delete_go(name.clone(), true, true, cx)
                    });
                }
                Err(e) => {
                    this.status = format!("{e}");
                    cx.notify();
                }
            });
        })
        .detach();
    }
}
