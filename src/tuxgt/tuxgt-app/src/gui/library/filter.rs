use super::*;
use gpui_kit::component::button::ButtonVariants as _;
use gpui_kit::component::menu::{DropdownMenu as _, PopupMenuItem};
use gpui_kit::component::{h_flex, ActiveTheme, IconName, Selectable as _};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use tuxgt_core::FluentArgs;

use super::super::widgets;
use super::super::Shell;

impl Shell {
    pub(crate) fn filter_bar(&self, view: Entity<Self>, cx: &App) -> impl IntoElement {
        let b = cx.theme();
        h_flex()
            .id("filters")
            .flex_wrap()
            .gap_1()
            .p_2()
            .rounded(px(4.))
            .bg(b.sidebar)
            .border_1()
            .border_color(b.border)
            .child(self.drop_filter(
                view.clone(),
                "flt-store",
                self.strings.get("gui-filter-store"),
                self.filters.store.as_deref(),
                store_options(&self.games, &self.strings),
                FilterKind::Store,
                cx,
            ))
            .child(self.drop_filter(
                view.clone(),
                "flt-platform",
                self.strings.get("gui-filter-platform"),
                self.filters.platform.as_deref(),
                platform_options(&self.games, &self.strings),
                FilterKind::Platform,
                cx,
            ))
            .child(self.drop_filter(
                view.clone(),
                "flt-pdb",
                self.strings.get("gui-filter-protondb"),
                self.filters.protondb.as_deref(),
                {
                    let mut v: Vec<String> = self.tiers.values().cloned().collect();
                    v.sort();
                    v.dedup();
                    v.into_iter()
                        .map(|id| {
                            let label =
                                widgets::id_label(widgets::ValKind::Tier, &id, &self.strings);
                            (id, label)
                        })
                        .collect()
                },
                FilterKind::ProtonDb,
                cx,
            ))
            .child(self.sort_menu(view.clone(), cx))
            .child(
                widgets::btn("flt-mods", cx)
                    .when(self.filters.mods_only, |t| t.primary())
                    .when(!self.filters.mods_only, |t| t.secondary())
                    .child(widgets::blabel(
                        self.strings.get("gui-filter-active-mods"),
                        cx,
                    ))
                    .on_click({
                        let view = view.clone();
                        move |_, _, cx| {
                            view.update(cx, |this, cx| {
                                this.filters.mods_only = !this.filters.mods_only;
                                this.recompute_base();
                                tracing::debug!(action = "toggle-chip", chip = "mods-only", on = this.filters.mods_only, count = this.base_filtered.len());
                                cx.notify();
                            });
                        }
                    }),
            )
            .child(
                widgets::btn("flt-awacy", cx)
                    .when(self.filters.awacy_only, |t| t.primary())
                    .when(!self.filters.awacy_only, |t| t.secondary())
                    .child(widgets::blabel(self.strings.get("gui-filter-awacy"), cx))
                    .on_click({
                        let view = view.clone();
                        move |_, _, cx| {
                            view.update(cx, |this, cx| {
                                this.filters.awacy_only = !this.filters.awacy_only;
                                this.recompute_base();
                                tracing::debug!(action = "toggle-chip", chip = "awacy-only", on = this.filters.awacy_only, count = this.base_filtered.len());
                                cx.notify();
                            });
                        }
                    }),
            )
            .child(
                widgets::btn("flt-hidden", cx)
                    .when(self.filters.show_hidden, |t| t.primary())
                    .when(!self.filters.show_hidden, |t| t.secondary())
                    .child(widgets::blabel(
                        self.strings.get("gui-filter-show-hidden"),
                        cx,
                    ))
                    .on_click({
                        let view = view.clone();
                        move |_, _, cx| {
                            view.update(cx, |this, cx| {
                                this.filters.show_hidden = !this.filters.show_hidden;
                                tracing::debug!(action = "toggle-chip", chip = "show-hidden", on = this.filters.show_hidden);
                                cx.notify();
                            });
                        }
                    }),
            )
            .child(div().flex_1())
            .child(
                widgets::btn("view-grid", cx)
                    .ghost()
                    .child(widgets::bicon(IconName::LayoutDashboard))
                    .selected(!self.library_list)
                    .on_click({
                        let view = view.clone();
                        move |_, _, cx| {
                            view.update(cx, |this, cx| {
                                tracing::debug!(action = "set-view", view = "grid");
                                this.library_list = false;
                                this.persist_nav();
                                cx.notify();
                            });
                        }
                    }),
            )
            .child(
                widgets::btn("view-list", cx)
                    .ghost()
                    .child(widgets::bicon(IconName::Menu))
                    .selected(self.library_list)
                    .on_click({
                        let view = view.clone();
                        move |_, _, cx| {
                            view.update(cx, |this, cx| {
                                this.library_list = true;
                                tracing::debug!(action = "set-view", view = "list");
                                this.persist_nav();
                                cx.notify();
                            });
                        }
                    }),
            )
    }

    pub(crate) fn drop_filter(
        &self,
        view: Entity<Self>,
        id: &'static str,
        label: String,
        current: Option<&str>,
        options: Vec<(String, String)>,
        kind: FilterKind,
        cx: &App,
    ) -> impl IntoElement {
        let current = current.map(|s| s.to_string());
        // E60: menus paint the label; the value they select stays the id.
        let shown = match current.as_deref() {
            Some(cur) => options
                .iter()
                .find(|(opt, _)| opt == cur)
                .map(|(_, l)| l.clone())
                .unwrap_or_else(|| cur.to_string()),
            None => self.strings.get("gui-filter-all"),
        };
        let all = self.strings.get("gui-filter-all");
        widgets::btn(id, cx)
            .secondary()
            .child(widgets::blabel(format!("{label}: {shown}"), cx))
            .dropdown_menu(move |menu, _, _| {
                let mut menu = menu.item(
                    PopupMenuItem::new(all.clone())
                        .checked(current.is_none())
                        .on_click({
                            let view = view.clone();
                            move |_, _, cx| {
                                view.update(cx, |this, cx| {
                                    kind.apply(this, None);
                                    cx.notify();
                                });
                            }
                        }),
                );
                for (opt, opt_label) in &options {
                    let opt2 = opt.clone();
                    let view = view.clone();
                    let checked = current.as_deref() == Some(opt.as_str());
                    menu = menu.item(
                        PopupMenuItem::new(opt_label.clone())
                            .checked(checked)
                            .on_click(move |_, _, cx| {
                                let opt2 = opt2.clone();
                                view.update(cx, |this, cx| {
                                    kind.apply(this, Some(opt2.clone()));
                                    cx.notify();
                                });
                            }),
                    );
                }
                menu
            })
    }

    pub(crate) fn sort_menu(&self, view: Entity<Self>, cx: &App) -> impl IntoElement {
        let current = SortMode::parse(&self.filters.sort);
        let current_label = self.strings.get(current.label_id());
        widgets::btn("sort", cx)
            .secondary()
            .child({
                let mut args = FluentArgs::new();
                args.set("mode", current_label);
                widgets::blabel(self.strings.get_args("gui-sort-current", Some(&args)), cx)
            })
            .dropdown_menu({
                let modes: Vec<(SortMode, String)> = SortMode::all()
                    .into_iter()
                    .map(|m| (m, self.strings.get(m.label_id())))
                    .collect();
                move |menu, _, _| {
                    let mut menu = menu;
                    for (mode, label) in &modes {
                        let mode = *mode;
                        let id = mode.id().to_string();
                        let view = view.clone();
                        menu = menu.item(
                            PopupMenuItem::new(label.clone())
                                .checked(mode == current)
                                .on_click(move |_, _, cx| {
                                    let id = id.clone();
                                    view.update(cx, |this, cx| {
                                        this.filters.sort = id.clone();
                                        this.recompute_base();
                                        tracing::debug!(action = "set-sort", sort = id.as_str(), count = this.base_filtered.len());
                                        cx.notify();
                                    });
                                }),
                        );
                    }
                    menu
                }
            })
    }
}
