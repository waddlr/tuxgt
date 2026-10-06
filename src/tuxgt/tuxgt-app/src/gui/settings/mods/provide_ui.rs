use gpui_kit::component::button::ButtonVariants as _;
use gpui_kit::component::h_flex;
use gpui_kit::*;
use tuxgt_core::{config_dir, data_dir, FluentArgs};

use super::super::*;

use super::super::super::widgets;
use gpui_kit::component::WindowExt as _;

use super::super::super::{rt_block, Shell};

/// File | Folder choice for one provided recipe. Does not open the Add form.
/// Strings are copied before `open_dialog`: its builder runs inside `Shell::render`.
pub(crate) fn pick_provide(view: Entity<Shell>, id: String, window: &mut Window, cx: &mut App) {
    if cx.can_select_mixed_files_and_dirs() {
        let prompt = view.read(cx).strings.get("gui-prompt-provide-file");
        prompt_provide(view, id, true, true, prompt, cx);
        return;
    }
    let title = view.read(cx).strings.get("gui-action-provide-files");
    let file_label = view.read(cx).strings.get("gui-action-pick-file");
    let folder_label = view.read(cx).strings.get("gui-action-pick-folder");
    let file_prompt = view.read(cx).strings.get("gui-prompt-provide-file");
    let folder_prompt = view.read(cx).strings.get("gui-prompt-provide-folder");
    window.open_dialog(cx, move |dialog, _, cx| {
        dialog.title(title.clone()).child(
            h_flex()
                .gap_2()
                .child(
                    widgets::btn("provide-pick-file", cx)
                        .primary()
                        .child(widgets::blabel(file_label.clone(), cx))
                        .on_click({
                            let view = view.clone();
                            let id = id.clone();
                            let file_prompt = file_prompt.clone();
                            move |_, window, cx| {
                                window.close_dialog(cx);
                                prompt_provide(
                                    view.clone(),
                                    id.clone(),
                                    true,
                                    false,
                                    file_prompt.clone(),
                                    cx,
                                );
                            }
                        }),
                )
                .child(
                    widgets::btn("provide-pick-folder", cx)
                        .secondary()
                        .child(widgets::blabel(folder_label.clone(), cx))
                        .on_click({
                            let view = view.clone();
                            let id = id.clone();
                            let folder_prompt = folder_prompt.clone();
                            move |_, window, cx| {
                                window.close_dialog(cx);
                                prompt_provide(
                                    view.clone(),
                                    id.clone(),
                                    false,
                                    true,
                                    folder_prompt.clone(),
                                    cx,
                                );
                            }
                        }),
                ),
        )
    });
}

fn prompt_provide(
    view: Entity<Shell>,
    id: String,
    files: bool,
    directories: bool,
    prompt: String,
    cx: &mut App,
) {
    prompt_provide_with_password(view, id, files, directories, prompt, None, None, cx);
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn prompt_provide_with_password(
    view: Entity<Shell>,
    id: String,
    files: bool,
    directories: bool,
    prompt: String,
    password: Option<String>,
    selected_path: Option<std::path::PathBuf>,
    cx: &mut App,
) {
    // A password retry supplies the already-selected path and skips the picker.
    let rx = selected_path.is_none().then(|| {
        cx.prompt_for_paths(PathPromptOptions {
            files,
            directories,
            multiple: false,
            prompt: Some(prompt.into()),
        })
    });
    let hide = std::sync::Arc::clone(&view.read(cx).hide_state);
    cx.spawn(async move |cx| {
        let path = if let Some(path) = selected_path {
            path
        } else {
            let Some(rx) = rx else {
                return;
            };
            let picked = rx.await.ok().and_then(|r| r.ok()).flatten();
            let Some(paths) = picked else {
                return;
            };
            let Some(path) = paths.into_iter().next() else {
                return;
            };
            path
        };
        tracing::debug!(action = "provide-files", source = id.as_str());
        let id_bg = id.clone();
        let refresh_id = id.clone();
        let pending_id = id;
        let pending_path = path.clone();
        let tried_password = password.is_some();
        let password_bg = password;
        // A mutation vetoes a Hide like a transfer, taken only for the
        // write — never the picker wait above.
        let _transfer = hide.installing();
        let result = cx
            .background_spawn(async move {
                rt_block(async {
                    let report = tuxgt_core::provide_files_with_password(
                        &config_dir(),
                        &data_dir(),
                        &id_bg,
                        &path,
                        password_bg.as_deref(),
                    )?;
                    Ok::<_, tuxgt_core::Error>((report, super::super::load_instances()))
                })
            })
            .await;
        let _ = cx.update(|cx| {
            view.update(cx, |this, cx| {
                match result {
                    Ok((_report, rows)) => {
                        if this.instances_showing() {
                            this.instances = rows.into_boxed_slice();
                        }
                        // New payload bytes: installed Provided instances
                        // compare against them, so every verdict is suspect.
                        this.invalidate_all_updates();
                        this.refresh_selected_mods();
                        this.refresh_file_preview(&refresh_id);
                    }
                    Err(tuxgt_core::Error::ArchivePasswordRequired) => {
                        if tried_password {
                            this.pending_note = Some(Note::Warn(
                                this.strings.get("gui-archive-password-rejected"),
                            ));
                        }
                        this.pending_archive_password = Some(PendingArchivePassword::Provide {
                            id: pending_id,
                            path: pending_path,
                        });
                        this.scroll_page_top();
                    }
                    Err(e) => {
                        let mut args = FluentArgs::new();
                        args.set("error", e.to_string());
                        this.status = this
                            .strings
                            .get_args("gui-status-err-instance", Some(&args));
                    }
                }
                cx.notify();
            });
        });
    })
    .detach();
}

pub(crate) fn clear_provided(view: Entity<Shell>, id: String, cx: &mut App) {
    tracing::debug!(action = "clear-provided", source = id.as_str());
    let id_bg = id.clone();
    let refresh_id = id;
    let hide = std::sync::Arc::clone(&view.read(cx).hide_state);
    cx.spawn(async move |cx| {
        // A mutation vetoes a Hide like a transfer: the stub handoff would
        // kill it mid-write.
        let _transfer = hide.installing();
        let result = cx
            .background_spawn(async move {
                rt_block(async {
                    tuxgt_core::clear_provided_files(&config_dir(), &data_dir(), &id_bg)?;
                    Ok::<_, tuxgt_core::Error>(super::super::load_instances())
                })
            })
            .await;
        let _ = cx.update(|cx| {
            view.update(cx, |this, cx| {
                match result {
                    Ok(rows) => {
                        if this.instances_showing() {
                            this.instances = rows.into_boxed_slice();
                        }
                        // Payload gone: Provided verdicts flip to the
                        // not-provided note, so drop them all.
                        this.invalidate_all_updates();
                        this.refresh_selected_mods();
                        this.refresh_file_preview(&refresh_id);
                    }
                    Err(e) => {
                        let mut args = FluentArgs::new();
                        args.set("error", e.to_string());
                        this.status = this
                            .strings
                            .get_args("gui-status-err-instance", Some(&args));
                    }
                }
                cx.notify();
            });
        });
    })
    .detach();
}
