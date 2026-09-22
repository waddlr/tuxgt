use gpui_kit::component::button::ButtonVariants as _;
use gpui_kit::component::input::Input;
use gpui_kit::component::switch::Switch;
use gpui_kit::component::{h_flex, v_flex, ActiveTheme, Disableable as _, IconName, Sizable as _};
use gpui_kit::prelude::{FluentBuilder, Styled};
use gpui_kit::*;

use tuxgt_core::{FluentArgs, PluginHost};

use super::super::theme::{types, TypeStyled as _};
use super::super::widgets;
use super::super::{PendingRegistryConfirm, RegistryRow, RemotePluginRow, Shell};

impl Shell {
    pub(crate) fn settings_core_plugins(&self, view: Entity<Self>, cx: &App) -> impl IntoElement {
        v_flex()
            .id("settings-core-plugins")
            .w_full()
            .flex_shrink_0()
            .gap_3()
            .child(widgets::muted(
                self.strings.get("gui-note-settings-core-plugins"),
                cx,
            ))
            .child(
                self.core_section(
                    "managers",
                    self.strings.get("gui-section-core-managers"),
                    ["steam", "heroic", "manual"]
                        .into_iter()
                        .filter_map(|id| self.core_plugin_row(view.clone(), id, None, cx)),
                    cx,
                ),
            )
            .child(
                self.core_section(
                    "metadata",
                    self.strings.get("gui-section-core-metadata"),
                    ["protondb", "steamgriddb", "awacy"]
                        .into_iter()
                        .filter_map(|id| {
                            let key_editor = (id == "steamgriddb").then(|| {
                                self.secret_key_editor("steamgriddb", view.clone(), cx)
                                    .into_any_element()
                            });
                            self.core_plugin_row(view.clone(), id, key_editor, cx)
                        }),
                    cx,
                ),
            )
            .child(
                self.core_section(
                    "capabilities",
                    self.strings.get("gui-section-core-capabilities"),
                    ["env", "wrapper"]
                        .into_iter()
                        .filter_map(|id| self.core_plugin_row(view.clone(), id, None, cx)),
                    cx,
                ),
            )
            .child(self.core_mods_section(view.clone(), cx))
            .child(self.core_registry_section(view.clone(), cx))
    }

    pub(crate) fn core_section<E: IntoElement>(
        &self,
        id: &str,
        title: String,
        rows: impl Iterator<Item = E>,
        cx: &App,
    ) -> impl IntoElement {
        widgets::section_card(SharedString::from(format!("core-{id}")), cx)
            .child(widgets::section_title(title, cx))
            .children(rows)
    }

    pub(crate) fn core_plugin_row(
        &self,
        view: Entity<Self>,
        id: &str,
        extra: Option<AnyElement>,
        cx: &App,
    ) -> Option<impl IntoElement> {
        let row = self.plugins.iter().find(|p| p.id == id)?.clone();
        let pid = row.id.clone();
        let b = cx.theme();
        let enabled = row.enabled;
        let is_sgdb = id == "steamgriddb";
        let switch = Switch::new(SharedString::from(format!("pe-{id}")))
            .checked(enabled)
            .xsmall()
            .on_click({
                let view = view.clone();
                move |on, _, cx| {
                    let on = *on;
                    let pid = pid.clone();
                    view.update(cx, |this, cx| {
                        this.set_plugin_enabled_ui(pid, on, cx);
                    });
                }
            });
        let enable_control = if is_sgdb && enabled {
            let gear_view = view.clone();
            h_flex()
                .gap_1()
                .items_center()
                .child(
                    widgets::btn("sgdb-settings", cx)
                        .secondary()
                        .child(widgets::bicon(IconName::Settings))
                        .on_click(move |_, _, cx| {
                            gear_view.update(cx, |this, cx| {
                                this.show_steamgriddb_settings = !this.show_steamgriddb_settings;
                                cx.notify();
                            });
                        }),
                )
                .child(switch)
                .into_any_element()
        } else {
            switch.into_any_element()
        };

        let mut card = v_flex()
            .id(SharedString::from(format!("core-pl-{id}")))
            .gap_1()
            .child(widgets::labeled_row(
                SharedString::from(format!("pl-{id}")),
                row.label.clone(),
                Some(SharedString::from(row.id.clone())),
                enable_control,
                cx,
            ));
        if is_sgdb {
            if enabled && self.show_steamgriddb_settings {
                let set = self
                    .secret_states
                    .get("steamgriddb")
                    .copied()
                    .unwrap_or(false);
                // E140: unset well is a dark surface like every pill —
                // `muted_foreground` as well reads white-on-light.
                let pill_row = widgets::labeled_row(
                    SharedString::from("steamgriddb-key-state"),
                    self.strings.get("gui-section-secret-manager"),
                    None,
                    widgets::pill(
                        self.strings.get(if set {
                            "gui-secret-set"
                        } else {
                            "gui-secret-unset"
                        }),
                        if set {
                            cx.theme().primary
                        } else {
                            b.tab_bar_segmented
                        },
                        b.border,
                        cx,
                    ),
                    cx,
                );
                let subsection = match extra {
                    Some(editor) => v_flex()
                        .gap_1()
                        .p_2()
                        .rounded(px(4.))
                        .border_1()
                        .border_color(b.border)
                        .bg(b.tab_bar_segmented)
                        .child(pill_row)
                        .child(editor),
                    None => v_flex()
                        .gap_1()
                        .p_2()
                        .rounded(px(4.))
                        .border_1()
                        .border_color(b.border)
                        .bg(b.tab_bar_segmented)
                        .child(pill_row),
                };
                card = card.child(subsection);
            }
        } else if let Some(extra) = extra {
            card = card.child(extra);
        }
        Some(card)
    }

    pub(crate) fn core_mods_section(&self, view: Entity<Self>, cx: &App) -> impl IntoElement {
        widgets::section_card("core-mods", cx)
            .child(widgets::section_title(
                self.strings.get("gui-section-core-mods"),
                cx,
            ))
            .child(widgets::muted(
                self.strings.get("gui-note-settings-plugins"),
                cx,
            ))
            .children(["reshade", "optiscaler"].into_iter().filter_map(|id| {
                let inst = self.instances.iter().find(|i| i.id == id)?.clone();
                let oid = inst.id.clone();
                Some(widgets::labeled_row(
                    SharedString::from(format!("core-mod-{id}")),
                    inst.label.clone(),
                    Some(SharedString::from(inst.id.clone())),
                    Switch::new(SharedString::from(format!("ce-{id}")))
                        .checked(inst.enabled)
                        .xsmall()
                        .on_click({
                            let view = view.clone();
                            move |on, _, cx| {
                                let on = *on;
                                let oid = oid.clone();
                                view.update(cx, |this, cx| {
                                    this.set_instance_offered_ui(oid, on, cx);
                                });
                            }
                        }),
                    cx,
                ))
            }))
    }

    pub(crate) fn set_plugin_enabled_ui(&mut self, id: String, on: bool, cx: &mut Context<Self>) {
        tracing::debug!(action = "set-plugin-enabled", source = id.as_str(), on);
        match PluginHost::load().and_then(|mut h| h.set_enabled(&id, on)) {
            Ok(()) => {
                if let Some(row) = self.plugins.iter_mut().find(|r| r.id == id) {
                    row.enabled = on;
                }
                self.knobs = super::load_knobs().into_boxed_slice();
                self.knob_ids = super::knob_ids_for(&self.knobs);
                self.disabled_managers = super::load_disabled();
                // Rows stay in the DB; only the display hide changes, and
                // disabled managers narrow at paint (no base recompute). A
                // failed read keeps the held index: an empty one would drop
                // the selection (and persist that) on a transient DB error.
                if let Some(index) = super::load_index_checked() {
                    self.index = index.into_boxed_slice();
                    self.revalidate_selection();
                }
                let mut args = FluentArgs::new();
                args.set("id", id.clone());
                let key = if on {
                    "gui-status-plugin-enabled"
                } else {
                    "gui-status-plugin-disabled"
                };
                self.status = self.strings.get_args(key, Some(&args));
            }
            Err(e) => self.status = format!("{e}"),
        }
        cx.notify();
    }

    /// R14 Registries section: add a registry, list its plugin rows, and
    /// install / update / remove each one. Every row is a real manifest
    /// entry from the store, or a real unreadable-registry error — there
    /// are no placeholder rows. Installed payloads are inert data: the row
    /// says so, and nothing here launches a plugin.
    pub(crate) fn core_registry_section(&self, view: Entity<Self>, cx: &App) -> impl IntoElement {
        let t = types(cx);
        let add_view = view.clone();
        let mut card = widgets::section_card("core-registries", cx)
            .child(widgets::section_title(
                self.strings.get("gui-section-core-registries"),
                cx,
            ))
            .child(widgets::muted(
                self.strings.get("gui-note-settings-registry"),
                cx,
            ))
            .child(
                h_flex()
                    .gap_1()
                    .child(
                        Styled::h(
                            Input::new(&self.registry_url_input)
                                .xsmall()
                                .text_size(t.body_md.size)
                                .cleanable(true),
                            t.control_h,
                        )
                        .flex_1()
                        .min_w_0(),
                    )
                    .child({
                        let add_view = add_view.clone();
                        widgets::btn("registry-add", cx)
                            .primary()
                            .disabled(self.registry_locked())
                            .child(widgets::bicon(IconName::Plus))
                            .child(widgets::blabel(
                                self.strings.get("gui-action-add-registry"),
                                cx,
                            ))
                            .on_click(move |_, window, cx| {
                                let url = add_view
                                    .read(cx)
                                    .registry_url_input
                                    .read(cx)
                                    .value()
                                    .to_string();
                                add_view.update(cx, |this, cx| {
                                    this.add_registry_ui(&url, window, cx);
                                });
                            })
                    }),
            );

        card = card
            .when(self.registry_busy, |this| {
                this.child(widgets::muted(
                    self.strings.get("gui-status-registry-busy"),
                    cx,
                ))
            })
            .when_some(self.registry_error.clone(), |this, (id, error)| {
                let mut args = FluentArgs::new();
                args.set("error", error);
                this.child(widgets::muted(
                    self.strings
                        .get_args("gui-status-err-registry-op", Some(&args)),
                    cx,
                ))
                .when(!id.is_empty(), |this| this.child(widgets::mono(id, cx)))
            })
            .when_some(self.registry_confirm.clone(), |this, pending| {
                this.child(self.registry_confirm_card(view.clone(), pending, cx))
            });

        for registry in self.registries.iter() {
            card = card.child(self.registry_row_card(view.clone(), registry, cx));
        }

        if self.registries.is_empty() {
            card = card.child(widgets::muted(
                self.strings.get("gui-empty-registry-none"),
                cx,
            ));
        }
        card
    }

    /// One added registry: its pin and plugin count, the rows it offers,
    /// and its own Refresh/Remove controls. A registry whose cache cannot
    /// be read keeps its row and shows the error instead of an empty
    /// catalog that would read as "this registry has no plugins".
    fn registry_row_card(
        &self,
        view: Entity<Self>,
        registry: &RegistryRow,
        cx: &App,
    ) -> impl IntoElement {
        let t = types(cx);
        let mut args = FluentArgs::new();
        args.set("pin", registry.pinned_short.clone());
        args.set("manifest", registry.manifest_path.clone());
        args.set("count", registry.plugin_count.to_string());
        let mut card = widgets::section_card(
            SharedString::from(format!("core-registry-{}", registry.pinned_short)),
            cx,
        )
        .child(
            h_flex()
                .gap_1()
                .items_center()
                .justify_between()
                .child(
                    v_flex()
                        .min_w_0()
                        .child(div().tx(t.label_lg).truncate().child(registry.url.clone()))
                        .child(widgets::muted(
                            self.strings.get_args("gui-registry-summary", Some(&args)),
                            cx,
                        )),
                )
                .child(
                    h_flex()
                        .gap_1()
                        .flex_none()
                        .child({
                            let pin_view = view.clone();
                            let url = registry.url.clone();
                            widgets::btn(
                                SharedString::from(format!(
                                    "registry-refresh-{}",
                                    registry.pinned_short
                                )),
                                cx,
                            )
                            .secondary()
                            .disabled(self.registry_locked())
                            .child(widgets::blabel(
                                self.strings.get("gui-action-registry-refresh"),
                                cx,
                            ))
                            .on_click(move |_, _, cx| {
                                pin_view.update(cx, |this, cx| {
                                    this.update_registry_pin_ui(url.clone(), cx);
                                });
                            })
                        })
                        .child({
                            let drop_view = view.clone();
                            let url = registry.url.clone();
                            let count = registry.plugin_count;
                            widgets::destroy_btn(
                                SharedString::from(format!(
                                    "registry-drop-{}",
                                    registry.pinned_short
                                )),
                                self.strings.get("gui-action-registry-remove"),
                                move |_, _, cx| {
                                    drop_view.update(cx, |this, cx| {
                                        this.request_remove_registry(url.clone(), count, cx);
                                    });
                                },
                                cx,
                            )
                            .disabled(self.registry_locked())
                        }),
                ),
        );

        if let Some(error) = &registry.error {
            let mut eargs = FluentArgs::new();
            eargs.set("error", error.clone());
            card = card.child(widgets::muted(
                self.strings
                    .get_args("gui-status-err-registry-read", Some(&eargs)),
                cx,
            ));
        } else {
            for plugin in self
                .remote_plugins
                .iter()
                .filter(|p| p.registry_url == registry.url)
            {
                card = card.child(self.remote_plugin_row(view.clone(), plugin, cx));
            }
            if registry.plugin_count == 0 {
                card = card.child(widgets::muted(
                    self.strings.get("gui-empty-registry-plugins"),
                    cx,
                ));
            }
        }
        card
    }

    /// One remote plugin: label, its id, what is installed today, and the
    /// actions that state allows. Install and Update are mutually
    /// exclusive (an available update is an update, never a second
    /// install), Remove only appears once something is installed, and the
    /// enable toggle is the same last-right control every other row on
    /// this page uses.
    fn remote_plugin_row(
        &self,
        view: Entity<Self>,
        plugin: &RemotePluginRow,
        cx: &App,
    ) -> impl IntoElement {
        let mut args = FluentArgs::new();
        let state = if let Some(installed) = &plugin.installed_pin_short {
            args.set("pin", installed.clone());
            if plugin.update_available {
                let mut uargs = FluentArgs::new();
                uargs.set("from", installed.clone());
                uargs.set("to", plugin.pinned_short.clone());
                self.strings
                    .get_args("gui-registry-plugin-update", Some(&uargs))
            } else {
                self.strings
                    .get_args("gui-registry-plugin-installed", Some(&args))
            }
        } else if !plugin.install_offered {
            // Review P2: say why there is no Install, instead of leaving a
            // row that looks merely unavailable.
            self.strings.get("gui-registry-plugin-shadowed")
        } else {
            self.strings.get("gui-registry-plugin-available")
        };

        let errored = self
            .registry_error
            .as_ref()
            .is_some_and(|(id, _)| id == &plugin.id);

        let mut action: Option<AnyElement> = None;
        if plugin.update_available {
            let update_view = view.clone();
            let id = plugin.id.clone();
            let from = plugin.installed_pin_short.clone().unwrap_or_default();
            action = Some(
                widgets::btn(SharedString::from(format!("rp-update-{}", plugin.id)), cx)
                    .secondary()
                    .disabled(self.registry_locked())
                    .child(widgets::blabel(
                        self.strings.get("gui-mod-action-update"),
                        cx,
                    ))
                    .on_click(move |_, _, cx| {
                        update_view.update(cx, |this, cx| {
                            this.request_update_plugin(id.clone(), from.clone(), cx);
                        });
                    })
                    .into_any_element(),
            );
        } else if !plugin.installed && plugin.install_offered {
            // Review P2: only the registry core would actually install from
            // offers the button. A later registry's duplicate id keeps its
            // label and state but no Install, because clicking it would
            // fetch the EARLIER registry's payload under this row's URL.
            let install_view = view.clone();
            let id = plugin.id.clone();
            action = Some(
                widgets::btn(SharedString::from(format!("rp-install-{}", plugin.id)), cx)
                    .primary()
                    .disabled(self.registry_locked())
                    .child(widgets::blabel(self.strings.get("gui-action-install"), cx))
                    .on_click(move |_, _, cx| {
                        install_view.update(cx, |this, cx| {
                            this.request_install_plugin(id.clone(), cx);
                        });
                    })
                    .into_any_element(),
            );
        }

        let remove = plugin.installed.then(|| {
            let remove_view = view.clone();
            let id = plugin.id.clone();
            widgets::destroy_btn(
                SharedString::from(format!("rp-remove-{}", plugin.id)),
                self.strings.get("gui-action-remove"),
                move |_, _, cx| {
                    remove_view.update(cx, |this, cx| {
                        this.request_remove_plugin(id.clone(), cx);
                    });
                },
                cx,
            )
            .disabled(self.registry_locked())
        });

        let toggle_view = view.clone();
        let toggle_id = plugin.id.clone();
        // Review P1: the toggle stays disabled until a payload is installed.
        // Core refuses set_remote_enabled for an id that is not in
        // remote.toml installed, so an armed switch on a Not installed row
        // could only ever fail closed. Disabled, not omitted: the row keeps
        // its shape and the toggle lights up the moment Install lands.
        let toggle = Switch::new(SharedString::from(format!("rp-toggle-{}", plugin.id)))
            .checked(plugin.enabled)
            .xsmall()
            .disabled(self.registry_locked() || !plugin.installed)
            .on_click(move |on, _, cx| {
                let on = *on;
                let id = toggle_id.clone();
                toggle_view.update(cx, |this, cx| {
                    this.set_remote_plugin_enabled_ui(id, on, cx);
                });
            });

        let control = h_flex()
            .gap_1()
            .items_center()
            .flex_none()
            .when_some(remove, |this, btn| this.child(btn))
            .when_some(action, |this, btn| this.child(btn))
            .child(toggle)
            .into_any_element();

        let mut card = widgets::labeled_row(
            SharedString::from(format!("rp-{}", plugin.id)),
            plugin.label.clone(),
            Some(SharedString::from(plugin.id.clone())),
            control,
            cx,
        )
        .into_any_element();
        // One details block under the property row: description, author,
        // then the state line. Built by parts so a manifest that omits
        // the optional fields simply has fewer lines.
        let mut details = v_flex().px_1().gap_1();
        if !plugin.description.is_empty() {
            details = details.child(widgets::muted(plugin.description.clone(), cx));
        }
        if !plugin.author.is_empty() {
            let mut aargs = FluentArgs::new();
            aargs.set("author", plugin.author.clone());
            details = details.child(widgets::muted(
                self.strings
                    .get_args("gui-registry-plugin-author", Some(&aargs)),
                cx,
            ));
        }
        card = v_flex()
            .child(card)
            .child(details.child(widgets::muted(state, cx)))
            .into_any_element();
        if errored {
            let (id, error) = self.registry_error.clone().unwrap_or_default();
            let mut eargs = FluentArgs::new();
            eargs.set("error", error);
            eargs.set("id", id);
            card = v_flex()
                .child(card)
                .child(
                    v_flex().px_1().child(widgets::muted(
                        self.strings
                            .get_args("gui-status-err-registry-row", Some(&eargs)),
                        cx,
                    )),
                )
                .into_any_element();
        }
        card
    }

    /// R14 consent for install / update / remove. Names the exact pin
    /// the op would write and, for an update, the one it would replace,
    /// so a failure to read the new bytes is a visible choice rather
    /// than a silent swap. Cancel makes no core call.
    fn registry_confirm_card(
        &self,
        view: Entity<Self>,
        pending: PendingRegistryConfirm,
        cx: &App,
    ) -> AnyElement {
        let confirm_view = view.clone();
        let cancel_view = view;
        let (note_key, target, detail) = match &pending {
            PendingRegistryConfirm::Install { id } => {
                ("gui-note-registry-install", id.clone(), String::new())
            }
            PendingRegistryConfirm::Update { id, from_pin } => {
                let mut args = FluentArgs::new();
                args.set("from", from_pin.clone());
                (
                    "gui-note-registry-update",
                    id.clone(),
                    self.strings
                        .get_args("gui-registry-confirm-from", Some(&args)),
                )
            }
            PendingRegistryConfirm::Remove { id } => {
                ("gui-note-registry-remove", id.clone(), String::new())
            }
            PendingRegistryConfirm::RemoveRegistry { url, plugin_count } => {
                let mut args = FluentArgs::new();
                args.set("count", plugin_count.to_string());
                (
                    "gui-note-registry-remove-registry",
                    url.clone(),
                    self.strings
                        .get_args("gui-registry-confirm-count", Some(&args)),
                )
            }
        };
        v_flex()
            .id("registry-confirm")
            .gap_1()
            .p_2()
            .rounded(px(4.))
            .border_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().tab_bar_segmented)
            .child(widgets::muted(self.strings.get(note_key), cx))
            .child(widgets::mono(target, cx))
            .when(!detail.is_empty(), |this| {
                this.child(widgets::muted(detail, cx))
            })
            .child(
                h_flex()
                    .gap_1()
                    .child(
                        widgets::btn("registry-confirm-yes", cx)
                            .primary()
                            .child(widgets::blabel(self.strings.get("gui-action-confirm"), cx))
                            .on_click(move |_, _, cx| {
                                confirm_view.update(cx, |this, cx| {
                                    this.confirm_registry_op(cx);
                                });
                            }),
                    )
                    .child(
                        widgets::btn("registry-confirm-no", cx)
                            .ghost()
                            .child(widgets::blabel(self.strings.get("gui-action-cancel"), cx))
                            .on_click(move |_, _, cx| {
                                cancel_view.update(cx, |this, cx| {
                                    this.cancel_registry_op(cx);
                                });
                            }),
                    ),
            )
            .into_any_element()
    }
}
