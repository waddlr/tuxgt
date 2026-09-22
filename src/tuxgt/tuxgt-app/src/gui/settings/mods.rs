use gpui_kit::component::button::ButtonVariants as _;
use gpui_kit::component::switch::Switch;
use gpui_kit::component::tab::TabBar;
use gpui_kit::component::{h_flex, v_flex, ActiveTheme, Disableable as _, IconName, Sizable as _};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

use super::*;

use super::super::theme::{types, TypeStyled as _};
use super::super::widgets;
use gpui_kit::assets::IconName as FullIconName;
use gpui_kit::component::{Icon, WindowExt as _};
use tuxgt_core::{config_dir, data_dir, FluentArgs};

use super::super::{rt_block, ConfigNavPending, InstanceRow, SettingsModsTab, Shell};
use super::authors_local_recipe;

impl Shell {
    pub(crate) fn settings_mods(&self, view: Entity<Self>, cx: &App) -> impl IntoElement {
        let tab_ix = match self.mods_tab {
            SettingsModsTab::Optiscaler => 0,
            SettingsModsTab::Reshade => 1,
            SettingsModsTab::Custom => 2,
        };
        v_flex()
            .id("settings-mods")
            .w_full()
            .gap_3()
            .flex_shrink_0()
            .child(
                TabBar::new("mods-tabs")
                    .underline()
                    .small()
                    .selected_index(tab_ix)
                    .on_click({
                        let view = view.clone();
                        move |ix, _, cx| {
                            let target = match *ix {
                                1 => SettingsModsTab::Reshade,
                                2 => SettingsModsTab::Custom,
                                _ => SettingsModsTab::Optiscaler,
                            };
                            view.update(cx, |this, cx| {
                                if target == this.mods_tab {
                                    return;
                                }
                                if !this.try_leave_config(ConfigNavPending::ModsTab(target), cx) {
                                    return;
                                }
                                this.mods_tab = target;
                                this.persist_mods_tab();
                                this.add_form = None;
                                this.scroll_page_top();
                                cx.notify();
                            });
                        }
                    })
                    .child(settings_tab(
                        IconName::File,
                        self.strings.get("gui-tab-mods-optiscaler"),
                    ))
                    .child(settings_tab(
                        IconName::File,
                        self.strings.get("gui-tab-mods-reshade"),
                    ))
                    .child(settings_tab(
                        IconName::File,
                        self.strings.get("gui-tab-mods-custom"),
                    )),
            )
            .child(widgets::muted(
                self.strings.get("gui-note-settings-mods"),
                cx,
            ))
            .child(
                h_flex()
                    .gap_2()
                    .items_center()
                    .child(self.mods_resync_button(view.clone(), cx))
                    .child({
                        let ids = self.visible_user_mod_ids(cx);
                        let empty = ids.is_empty();
                        widgets::destroy_btn(
                            "mods-remove-visible",
                            self.strings.get("gui-action-remove-visible"),
                            {
                                let view = view.clone();
                                move |_, window, cx| {
                                    view.update(cx, |this, cx| {
                                        this.confirm_remove_visible_mods(window, cx);
                                    });
                                }
                            },
                            cx,
                        )
                        .disabled(empty)
                    }),
            )
            .child(if self.pending_archive_password.is_some() {
                self.archive_password_box(view.clone(), cx)
                    .into_any_element()
            } else {
                match self.mods_tab {
                    SettingsModsTab::Optiscaler => {
                        self.mods_optiscaler_box(view, cx).into_any_element()
                    }
                    SettingsModsTab::Reshade => self.mods_reshade_box(view, cx).into_any_element(),
                    SettingsModsTab::Custom => self.mods_custom_box(view, cx).into_any_element(),
                }
            })
    }

    fn offer_switch(&self, view: Entity<Self>, i: &InstanceRow) -> impl IntoElement {
        let id = i.id.clone();
        Switch::new(i.ids.enable.clone())
            .checked(i.enabled)
            .xsmall()
            .on_click(move |on, _, cx| {
                let on = *on;
                view.update(cx, |this, cx| {
                    this.set_instance_offered_ui(id.clone(), on, cx);
                });
            })
    }

    /// One catalog card. Official, provided, pack, family, and user mods share
    /// it: name on the left, controls on the right, one Details disclosure.
    /// Rescan and Export stay on a local recipe the user authored.
    pub(crate) fn mod_row(
        &self,
        view: Entity<Self>,
        i: &InstanceRow,
        cx: &App,
    ) -> impl IntoElement {
        let b = cx.theme();
        let provided = i.source == "provided";
        let ready = i.provided.as_ref().is_none_or(|p| p.ready);
        let details_open = self.preview_open.contains(&format!("det:{}", i.id));
        let missing = i
            .provided
            .as_ref()
            .map(|p| p.missing.clone())
            .unwrap_or_default();
        let ambiguous = i
            .provided
            .as_ref()
            .map(|p| p.ambiguous.clone())
            .unwrap_or_default();
        let present = i
            .provided
            .as_ref()
            .map(|p| p.present.clone())
            .unwrap_or_default();
        let missing_el = pattern_list(self.strings.get("gui-provide-missing"), &missing, cx);
        let ambiguous_el = pattern_list(self.strings.get("gui-provide-ambiguous"), &ambiguous, cx);
        let present_el = pattern_list(self.strings.get("gui-provide-present"), &present, cx);
        let tools = authors_local_recipe(i);
        h_flex()
            .id(i.ids.row.clone())
            .w_full()
            .overflow_hidden()
            .rounded(px(4.))
            .border_1()
            .border_color(b.border)
            .bg(b.group_box)
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .px_2()
                    .py_1()
                    .gap_1()
                    .child(
                        h_flex()
                            .gap_2()
                            .items_center()
                            .child(
                                v_flex().min_w_0().flex_1().child(
                                    div()
                                        .min_w_0()
                                        .tx(types(cx).headline_md)
                                        .truncate()
                                        .child(i.label.clone()),
                                ),
                            )
                            .child(
                                h_flex()
                                    .flex_shrink_0()
                                    .gap_1()
                                    .items_center()
                                    .child(self.details_button(view.clone(), i, details_open, cx))
                                    .when(tools, |this| {
                                        let export_id = i.id.clone();
                                        let export_view = view.clone();
                                        let rescan_id = i.id.clone();
                                        let rescan_view = view.clone();
                                        this.child(
                                            widgets::btn(i.ids.export.clone(), cx)
                                                .secondary()
                                                .child(widgets::blabel(
                                                    self.strings.get("gui-action-export"),
                                                    cx,
                                                ))
                                                .on_click(move |_, window, cx| {
                                                    cx.stop_propagation();
                                                    pick_export(
                                                        export_view.clone(),
                                                        export_id.clone(),
                                                        window,
                                                        cx,
                                                    );
                                                }),
                                        )
                                        .child(
                                            widgets::btn(i.ids.rescan.clone(), cx)
                                                .secondary()
                                                .child(widgets::blabel(
                                                    self.strings.get("gui-action-rescan-instance"),
                                                    cx,
                                                ))
                                                .on_click(move |_, _, cx| {
                                                    cx.stop_propagation();
                                                    start_rescan(
                                                        rescan_view.clone(),
                                                        rescan_id.clone(),
                                                        cx,
                                                    );
                                                }),
                                        )
                                    })
                                    .when(!i.official, |this| {
                                        let remove_id = i.id.clone();
                                        let remove_view = view.clone();
                                        this.child(widgets::destroy_btn(
                                            i.ids.remove.clone(),
                                            self.strings.get("gui-action-remove"),
                                            move |_, _, cx| {
                                                cx.stop_propagation();
                                                remove_view.update(cx, |this, cx| {
                                                    this.remove_instance_ui(remove_id.clone(), cx);
                                                });
                                            },
                                            cx,
                                        ))
                                    })
                                    .when(provided && !ready, |this| {
                                        let provide_id = i.id.clone();
                                        let provide_view = view.clone();
                                        this.child(
                                            widgets::btn(i.ids.provide.clone(), cx)
                                                .secondary()
                                                .child(widgets::blabel(
                                                    self.strings.get("gui-action-provide-files"),
                                                    cx,
                                                ))
                                                .on_click(move |_, window, cx| {
                                                    cx.stop_propagation();
                                                    pick_provide(
                                                        provide_view.clone(),
                                                        provide_id.clone(),
                                                        window,
                                                        cx,
                                                    );
                                                }),
                                        )
                                    })
                                    .when(provided && ready, |this| {
                                        let clear_id = i.id.clone();
                                        let clear_view = view.clone();
                                        this.child(
                                            widgets::btn(i.ids.clear.clone(), cx)
                                                .secondary()
                                                .child(Icon::new(FullIconName::Mop).small())
                                                .tooltip(self.strings.get("gui-action-clear"))
                                                .on_click(move |_, _, cx| {
                                                    cx.stop_propagation();
                                                    clear_provided(
                                                        clear_view.clone(),
                                                        clear_id.clone(),
                                                        cx,
                                                    );
                                                }),
                                        )
                                    })
                                    .when(!provided || ready, |this| {
                                        this.child(self.offer_switch(view.clone(), i))
                                    }),
                            ),
                    )
                    .when(!i.description.is_empty(), |this| {
                        this.child(widgets::muted(i.description.clone(), cx))
                    })
                    .when(!i.note.is_empty(), |this| {
                        this.child(widgets::muted(i.note.clone(), cx))
                    })
                    .when(self.catalog_updates.contains(&i.id), |this| {
                        let id = i.id.clone();
                        let view = view.clone();
                        this.child(
                            h_flex()
                                .gap_2()
                                .items_center()
                                .child(
                                    div()
                                        .tx(types(cx).body_md)
                                        .text_color(cx.theme().warning)
                                        .child(self.strings.get("gui-mod-update-available-short")),
                                )
                                .child(
                                    widgets::btn(i.ids.cat_update.clone(), cx)
                                        .primary()
                                        .child(widgets::blabel(
                                            self.strings.get("gui-mod-action-update"),
                                            cx,
                                        ))
                                        .on_click(move |_, _, cx| {
                                            cx.stop_propagation();
                                            view.update(cx, |this, cx| {
                                                this.refresh_catalog_payload_ui(id.clone(), cx);
                                            });
                                        }),
                                ),
                        )
                    })
                    .when(provided && !ready, |this| {
                        this.when_some(missing_el, |this, el| this.child(el))
                            .when_some(ambiguous_el, |this, el| this.child(el))
                            .when_some(present_el, |this, el| this.child(el))
                    })
                    .when(details_open, |this| {
                        this.child(self.details_body(view.clone(), i, cx))
                    }),
            )
    }

    /// Height of one `mod_row`. The User Packs window reserves this for
    /// off-screen cards, so a short value clips the scroll and a tall one
    /// opens a gap. `py_1` and `gap_1` are 0.25rem; the card border is 1px.
    /// Rem is `theme.font_size`, the size `Root` installs.
    pub(crate) fn catalog_row_h(&self, id: &str, cx: &App) -> Pixels {
        let t = types(cx);
        let gap = cx.theme().font_size * 0.25;
        let header = t.headline_md.line.max(t.control_h);
        let mut h = header + gap * 2. + px(2.);
        let (has_description, has_note, pattern_lens) = {
            let Some(i) = self.instances.iter().find(|r| r.id == id) else {
                return h;
            };
            let lenses = if i.source == "provided" {
                i.provided
                    .as_ref()
                    .filter(|p| !p.ready)
                    .map(|p| [p.missing.len(), p.ambiguous.len(), p.present.len()])
            } else {
                None
            };
            (!i.description.is_empty(), !i.note.is_empty(), lenses)
        };
        if has_description {
            h += gap + t.body_md.line;
        }
        if has_note {
            h += gap + t.body_md.line;
        }
        if self.catalog_updates.contains(id) {
            h += gap + t.control_h;
        }
        if let Some(lenses) = pattern_lens {
            for n in lenses {
                if n == 0 {
                    continue;
                }
                let n = n as f32;
                h += gap + t.body_md.line + (gap + t.label_lg.line) * n;
            }
        }
        if self.preview_open.contains(&format!("det:{id}")) {
            h += gap + self.details_h(id, cx);
        }
        h
    }
}

/// Heading plus each pattern. `None` when the list is empty so the heading stays hidden.
fn pattern_list(heading: String, items: &[String], cx: &App) -> Option<AnyElement> {
    if items.is_empty() {
        return None;
    }
    Some(
        v_flex()
            .w_full()
            .gap_1()
            .px_1()
            .child(
                div()
                    .tx(types(cx).body_md)
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(heading),
            )
            .children(items.iter().map(|item| widgets::mono(item.clone(), cx)))
            .into_any_element(),
    )
}

/// File | Folder choice for one provided recipe. Does not open the Add form.
/// Strings are copied before `open_dialog`: its builder runs inside `Shell::render`.
fn pick_provide(view: Entity<Shell>, id: String, window: &mut Window, cx: &mut App) {
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
                    Ok::<_, tuxgt_core::Error>((report, super::load_instances()))
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

fn clear_provided(view: Entity<Shell>, id: String, cx: &mut App) {
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
                    Ok::<_, tuxgt_core::Error>(super::load_instances())
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
