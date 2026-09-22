use gpui_kit::component::button::ButtonVariants as _;
use gpui_kit::component::{h_flex, WindowExt as _};
use gpui_kit::*;
use std::path::PathBuf;

use tuxgt_core::{config_dir, data_dir, FluentArgs};

use super::super::widgets;
use super::super::{rt_block, Shell};

pub(crate) fn pick_export(view: Entity<Shell>, id: String, window: &mut Window, cx: &mut App) {
    let title = view.read(cx).strings.get("gui-title-export");
    let recipe_label = view.read(cx).strings.get("gui-action-export-recipe");
    let files_label = view.read(cx).strings.get("gui-action-export-files");
    window.open_dialog(cx, move |dialog, _, cx| {
        dialog.title(title.clone()).child(
            h_flex()
                .gap_2()
                .child(
                    widgets::btn("export-pick-recipe", cx)
                        .primary()
                        .child(widgets::blabel(recipe_label.clone(), cx))
                        .on_click({
                            let view = view.clone();
                            let id = id.clone();
                            move |_, window, cx| {
                                window.close_dialog(cx);
                                run_export_with(view.clone(), id.clone(), false, cx);
                            }
                        }),
                )
                .child(
                    widgets::btn("export-pick-files", cx)
                        .secondary()
                        .child(widgets::blabel(files_label.clone(), cx))
                        .on_click({
                            let view = view.clone();
                            let id = id.clone();
                            move |_, window, cx| {
                                window.close_dialog(cx);
                                run_export_with(view.clone(), id.clone(), true, cx);
                            }
                        }),
                ),
        )
    });
}

pub(crate) fn run_export_with(view: Entity<Shell>, id: String, files: bool, cx: &mut App) {
    let suggested = if files {
        format!("{id}.tar.gz")
    } else {
        format!("{id}.toml")
    };
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    let rx = cx.prompt_for_new_path(&home, Some(&suggested));
    cx.spawn(async move |cx| {
        let picked = rx.await.ok().and_then(|r| r.ok()).flatten();
        let Some(path) = picked else {
            return;
        };
        tracing::debug!(action = "export-mod", source = id.as_str(), files);
        let result = cx
            .background_spawn(async move {
                rt_block(async {
                    tuxgt_core::export_mod(&config_dir(), &data_dir(), &id, &path, files)?;
                    Ok::<_, tuxgt_core::Error>(id)
                })
            })
            .await;
        let _ = cx.update(|cx| {
            view.update(cx, |this, _cx| match result {
                Ok(id) => {
                    let mut args = FluentArgs::new();
                    args.set("id", id);
                    this.status = this
                        .strings
                        .get_args("gui-status-instance-exported", Some(&args));
                }
                Err(e) => {
                    let mut args = FluentArgs::new();
                    args.set("error", e.to_string());
                    this.status = this.strings.get_args("gui-status-err-export", Some(&args));
                }
            });
        });
    })
    .detach();
}
