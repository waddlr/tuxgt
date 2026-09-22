use gpui_kit::component::button::ButtonVariants as _;
use gpui_kit::component::input::Input;
use gpui_kit::component::{h_flex, v_flex, Sizable as _};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use tuxgt_core::{custom_env, data_dir, open_db_shared, set_custom, validate_custom_key};

use super::super::theme::types;
use super::super::widgets;
use super::super::{rt_block, Shell};

impl Shell {
    pub(crate) fn custom_env_box(
        &self,
        game_id: &str,
        view: Entity<Self>,
        cx: &App,
    ) -> impl IntoElement {
        let game_id = game_id.to_string();
        widgets::section_card("custom-env", cx)
            .child(widgets::section_header(
                "custom-env-head",
                self.strings.get("gui-section-custom-env"),
                self.page_scroll.bounds().size.width,
                vec![widgets::SectionAction::new(
                    "add-env",
                    self.strings.get("gui-action-add-keyval"),
                    {
                        let view = view.clone();
                        move |_, window, cx| {
                            view.update(cx, |this, cx| {
                                this.custom_env_adding = true;
                                cx.notify();
                            });
                            let input = view.read(cx).custom_env_input.clone();
                            input.update(cx, |inp, cx| {
                                inp.set_value(String::new(), window, cx);
                            });
                        }
                    },
                )
                .tooltip(self.strings.get("gui-tip-custom-add"))],
                cx,
            ))
            .children(self.custom_env.iter().map(|(k, v)| {
                let key = k.clone();
                let rm = widgets::destroy_btn(
                    SharedString::from(format!("rm-{k}")),
                    self.strings.get("gui-action-remove"),
                    {
                        let view = view.clone();
                        let game_id = game_id.clone();
                        move |_, _, cx| {
                            let key = key.clone();
                            let game_id = game_id.clone();
                            view.update(cx, |_this, cx| {
                                let key = key.clone();
                                let guard = game_id.clone();
                                let inner = guard.clone();
                                cx.spawn(async move |this, cx| {
                                    let result = cx
                                        .background_spawn(async move {
                                            rt_block(async {
                                                let pool = open_db_shared(&data_dir()).await?;
                                                let host = tuxgt_core::PluginHost::load()?;
                                                tuxgt_core::mutate_game(
                                                    &pool,
                                                    &data_dir(),
                                                    &host,
                                                    &inner,
                                                    tuxgt_core::remove_custom(&pool, &inner, &key),
                                                )
                                                .await?;
                                                custom_env(&pool, &inner).await
                                            })
                                        })
                                        .await;
                                    let _ = this.update(cx, |this, cx| {
                                        if this.selected.as_deref() != Some(guard.as_str()) {
                                            return;
                                        }
                                        match result {
                                            Ok(rows) => {
                                                this.custom_count = rows.len();
                                                if this.env_maps_showing() {
                                                    this.custom_env = rows.into_boxed_slice();
                                                }
                                                this.session_payload.insert(
                                                    guard.clone(),
                                                    super::load_payload(&guard),
                                                );
                                                this.reload_armed_state(&guard);
                                                this.reload_launch_state(cx);
                                            }
                                            Err(e) => this.status = format!("{e}"),
                                        }
                                        cx.notify();
                                    });
                                })
                                .detach();
                            });
                        }
                    },
                    cx,
                );
                v_flex()
                    .child(widgets::labeled_row(
                        SharedString::from(format!("ce-{k}")),
                        format!("{k}={v}"),
                        None,
                        rm,
                        cx,
                    ))
                    .child(widgets::row_hairline(cx))
            }))
            .when(self.custom_env_adding, |this| {
                let save_view = view.clone();
                let save_game = game_id.clone();
                let cancel_view = view.clone();
                this.child(
                    v_flex()
                        .gap_1()
                        .child(Styled::h(
                            Input::new(&self.custom_env_input)
                                .xsmall()
                                .text_size(types(cx).body_md.size)
                                .cleanable(true),
                            types(cx).control_h,
                        ))
                        .child(
                            h_flex()
                                .gap_1()
                                .child(
                                    widgets::btn("custom-save", cx)
                                        .child(widgets::blabel(
                                            self.strings.get("gui-action-save"),
                                            cx,
                                        ))
                                        .on_click(move |_, _, cx| {
                                            let value = {
                                                let this = save_view.read(cx);
                                                this.custom_env_input.read(cx).value().to_string()
                                            };
                                            save_view.update(cx, |this, cx| {
                                                this.add_custom_env_ui(&save_game, &value, cx);
                                            });
                                        }),
                                )
                                .child(
                                    widgets::btn("custom-cancel", cx)
                                        .ghost()
                                        .child(widgets::blabel(
                                            self.strings.get("gui-action-cancel"),
                                            cx,
                                        ))
                                        .on_click(move |_, _, cx| {
                                            cancel_view.update(cx, |this, cx| {
                                                this.custom_env_adding = false;
                                                cx.notify();
                                            });
                                        }),
                                ),
                        ),
                )
            })
            .when(self.custom_env.is_empty(), |this| {
                this.child(widgets::placeholder_note(
                    self.strings.get("gui-empty-no-custom-env"),
                    cx,
                ))
            })
    }

    /// Upsert one `KEY=VALUE` pair from the add row: split on the first
    /// `=`, `validate_custom_key` the name, `set_custom` the pair, then
    /// refresh the list and the launch preview like `reload_game_env`.
    pub(crate) fn add_custom_env_ui(&mut self, game: &str, raw: &str, cx: &mut Context<Self>) {
        let Some((k, v)) = raw.split_once('=') else {
            self.status = self.strings.get("gui-status-custom-invalid");
            cx.notify();
            return;
        };
        let key = k.trim().to_string();
        let value = v.trim().to_string();
        if let Err(e) = validate_custom_key(&key) {
            self.status = format!("{e}");
            cx.notify();
            return;
        }
        let game = game.to_string();
        tracing::debug!(action = "add-custom-env", game = game.as_str(), key = key.as_str());
        let guard = game.clone();
        self.custom_env_adding = false;
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    rt_block(async {
                        let pool = open_db_shared(&data_dir()).await?;
                        let host = tuxgt_core::PluginHost::load()?;
                        tuxgt_core::mutate_game(
                            &pool,
                            &data_dir(),
                            &host,
                            &game,
                            set_custom(&pool, &game, &key, &value),
                        )
                        .await?;
                        custom_env(&pool, &game).await
                    })
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.selected.as_deref() != Some(guard.as_str()) {
                    return;
                }
                match result {
                    Ok(rows) => {
                        this.custom_count = rows.len();
                        if this.env_maps_showing() {
                            this.custom_env = rows.into_boxed_slice();
                        }
                        this.session_payload
                            .insert(guard.clone(), super::load_payload(&guard));
                        this.reload_armed_state(&guard);
                        this.reload_launch_state(cx);
                    }
                    Err(e) => this.status = format!("{e}"),
                }
                cx.notify();
            });
        })
        .detach();
    }
}
