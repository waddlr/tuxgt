use gpui_kit::component::button::ButtonVariants as _;
use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::input::Input;
use gpui_kit::component::{h_flex, v_flex, ActiveTheme, Disableable as _, Sizable as _};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

use super::*;
use tuxgt_core::{preview_effect_files, FluentArgs, ReshadePackageKind};

use super::super::theme::{types, TypeStyled as _};
use super::super::widgets;
use super::super::{ReshadePackagesMint, Shell};

impl Shell {
    pub(crate) fn reshade_extras_box(&self, view: Entity<Self>, cx: &App) -> impl IntoElement {
        let Some(m) = self.extras_mint.as_ref() else {
            return div().into_any_element();
        };
        let t = types(cx);
        let filter = self.extras_filter_input.read(cx).value().to_string();
        let needle = filter.trim().to_lowercase();
        let visible: Vec<usize> = m
            .packages
            .iter()
            .enumerate()
            .filter(|(_, p)| extras_row_visible(m.kind_filter, p, &needle))
            .map(|(i, _)| i)
            .collect();
        let list_cap = (t.label_lg.line + t.body_md.line + px(8.)) * 12.;
        let picked = m.selected.len();
        let mut cargs = FluentArgs::new();
        cargs.set("count", picked.to_string());
        let picked_txt = self.strings.get_args("gui-chip-mods", Some(&cargs));
        widgets::section_card("reshade-extras", cx)
            .mt_2()
            .when(m.loading, |this| {
                this.child(widgets::muted(
                    self.strings.get("gui-reshade-extras-loading"),
                    cx,
                ))
            })
            .when_some(m.err.clone(), |this, err| {
                let mut eargs = FluentArgs::new();
                eargs.set("error", err);
                this.child(widgets::muted(
                    self.strings
                        .get_args("gui-reshade-extras-error-loading", Some(&eargs)),
                    cx,
                ))
            })
            .child(Styled::h(
                Input::new(&self.extras_filter_input)
                    .xsmall()
                    .text_size(t.body_md.size)
                    .cleanable(true),
                t.control_h,
            ))
            .child(
                h_flex()
                    .gap_1()
                    .children(super::ExtrasKindFilter::buttons().into_iter().map(
                        |(id, filter, label_key)| {
                            let view = view.clone();
                            let mut btn = widgets::btn(id, cx);
                            btn = if m.kind_filter == filter {
                                btn.primary()
                            } else {
                                btn.secondary()
                            };
                            btn.child(widgets::blabel(self.strings.get(label_key), cx))
                                .on_click(move |_, _, cx| {
                                    view.update(cx, |this, cx| {
                                        this.set_extras_filter(filter, cx);
                                    });
                                })
                        },
                    )),
            )
            .child({
                let keys = Self::extras_selectable_keys(m, &needle);
                let all_on =
                    !keys.is_empty() && keys.iter().all(|k| m.selected.iter().any(|s| s == k));
                let label = self.strings.get("gui-action-select-visible");
                let view_c = view.clone();
                h_flex()
                    .id("reshade-extras-select-visible")
                    .w_full()
                    .gap_2()
                    .items_center()
                    .px_1()
                    .py_1()
                    .cursor_pointer()
                    .on_click({
                        let view = view_c.clone();
                        move |_, _, cx| {
                            view.update(cx, |this, cx| {
                                this.toggle_select_visible_extras(!all_on, cx);
                            });
                        }
                    })
                    .child(
                        div().w(px(24.)).flex_none().child(
                            Checkbox::new("reshade-extras-select-visible-check")
                                .checked(all_on)
                                .accessibility_label(label.clone()),
                        ),
                    )
                    .child(div().tx(t.label_lg).child(label))
                    .into_any_element()
            })
            .child(
                v_flex()
                    .id("reshade-extras-list")
                    .w_full()
                    .max_h(list_cap)
                    .overflow_y_scroll()
                    .track_scroll(&self.inner_scrolls[2])
                    .on_scroll_wheel(widgets::chain_inner(self.inner_scrolls[2].clone()))
                    .children(self.reshade_extras_group(
                        view.clone(),
                        &visible,
                        ReshadePackageKind::Effect,
                        "gui-reshade-extras-effects",
                        m,
                        cx,
                    ))
                    .children(self.reshade_extras_group(
                        view.clone(),
                        &visible,
                        ReshadePackageKind::Addon,
                        "gui-reshade-extras-addons",
                        m,
                        cx,
                    )),
            )
            .child(widgets::muted(picked_txt, cx))
            .child(
                h_flex()
                    .gap_1()
                    .child(
                        widgets::btn("reshade-extras-save", cx)
                            .primary()
                            .child(widgets::blabel(self.strings.get("gui-action-add"), cx))
                            .on_click({
                                let view = view.clone();
                                move |_, _, cx| {
                                    view.update(cx, |this, cx| this.mint_reshade_extras(cx));
                                }
                            }),
                    )
                    .child(
                        widgets::btn("reshade-extras-cancel", cx)
                            .secondary()
                            .child(widgets::blabel(self.strings.get("gui-action-cancel"), cx))
                            .on_click({
                                let view = view.clone();
                                move |_, _, cx| {
                                    view.update(cx, |this, cx| {
                                        this.extras_mint = None;
                                        cx.notify();
                                    });
                                }
                            }),
                    ),
            )
            .into_any_element()
    }

    pub(crate) fn reshade_extras_group(
        &self,
        view: Entity<Self>,
        visible: &[usize],
        kind: ReshadePackageKind,
        title_id: &'static str,
        m: &ReshadePackagesMint,
        cx: &App,
    ) -> Vec<AnyElement> {
        let rows: Vec<usize> = visible
            .iter()
            .copied()
            .filter(|&i| m.packages[i].kind == kind)
            .collect();
        if rows.is_empty() {
            return Vec::new();
        }
        let b = cx.theme();
        let t = types(cx);
        let mut out = Vec::new();
        out.push(widgets::muted(self.strings.get(title_id), cx).into_any_element());
        for i in rows {
            let pkg = &m.packages[i];
            let key = pkg.key();
            let checked = pkg.in_catalog || m.selected.iter().any(|s| s == &key);
            let locked = !pkg.mintable();
            let clicked = key.clone();
            let view = view.clone();
            let status = if pkg.in_catalog {
                Some(self.strings.get("gui-reshade-extras-present"))
            } else if pkg.url.is_none() {
                Some(self.strings.get("gui-reshade-extras-no-url"))
            } else {
                None
            };
            let mut row = h_flex()
                .id(SharedString::from(format!("reshade-extra-{i}")))
                .w_full()
                .gap_2()
                .items_start()
                .px_1()
                .py_1()
                .when(checked, |this| this.bg(b.sidebar))
                .when(!locked, {
                    let view = view.clone();
                    let clicked = clicked.clone();
                    move |this| {
                        this.on_click(move |_, _, cx| {
                            view.update(cx, |this, cx| {
                                this.toggle_reshade_extra(clicked.clone(), cx)
                            });
                        })
                    }
                })
                .child(
                    div().w(px(24.)).flex_none().child(
                        Checkbox::new(SharedString::from(format!("reshade-extra-check-{i}")))
                            .checked(checked)
                            .disabled(locked)
                            .accessibility_label(pkg.name.clone()),
                    ),
                )
                .child(
                    v_flex()
                        .min_w_0()
                        .flex_1()
                        .child(div().tx(t.label_lg).truncate().child(pkg.name.clone()))
                        .when(!pkg.description.is_empty(), |this| {
                            this.child(widgets::muted(pkg.description.clone(), cx))
                        })
                        .when_some(status, |this, s| this.child(widgets::muted(s, cx))),
                );
            let files_label = self.strings.get("gui-preview-files");
            let extra_key = format!("extra:{i}");
            let extra_open = self.preview_open.contains(&extra_key);
            let arrow = if extra_open { "▾" } else { "▸" };
            let effect_names: Option<(Vec<String>, usize)> = (extra_open
                && pkg.kind == ReshadePackageKind::Effect
                && !pkg.effect_files.is_empty())
            .then(|| preview_effect_files(pkg));
            let addon_asset: Option<String> = (pkg.kind == ReshadePackageKind::Addon)
                .then(|| pkg.url.clone())
                .flatten()
                .as_deref()
                .and_then(super::url_file_name);
            if effect_names.is_some() || addon_asset.is_some() {
                let toggle_view = view.clone();
                let toggle_key = extra_key.clone();
                row = row.child(
                    widgets::btn(SharedString::from(format!("reshade-extra-files-{i}")), cx)
                        .ghost()
                        .child(widgets::muted(format!("{files_label} {arrow}"), cx))
                        .on_click(move |_, _, cx| {
                            cx.stop_propagation();
                            toggle_view.update(cx, |this, cx| {
                                if !this.preview_open.insert(toggle_key.clone()) {
                                    this.preview_open.remove(&toggle_key);
                                }
                                cx.notify();
                            });
                        }),
                );
            }
            if let Some(url) = pkg.repository_url.clone() {
                row = row.child(
                    div()
                        .id(SharedString::from(format!("reshade-extra-repo-stop-{i}")))
                        .on_click(|_, _, cx| cx.stop_propagation())
                        .child(
                            widgets::open_btn(
                                SharedString::from(format!("reshade-extra-repo-{i}")),
                                widgets::OpenKind::Link,
                                self.strings.get("gui-reshade-extras-repo"),
                                Some(url.clone()),
                                cx,
                            )
                            .tooltip(url),
                        )
                        .into_any_element(),
                );
            }
            out.push(row.into_any_element());
            if extra_open {
                if let Some((names, total)) = effect_names {
                    let capped = names.len() < total;
                    let mut nargs = FluentArgs::new();
                    nargs.set("n", total.saturating_sub(names.len()).to_string());
                    let more = self.strings.get_args("gui-preview-more", Some(&nargs));
                    out.push(
                        v_flex()
                            .id(SharedString::from(format!("reshade-extra-files-body-{i}")))
                            .pl_6()
                            .children(
                                names
                                    .into_iter()
                                    .map(|n| widgets::mono(n, cx).into_any_element()),
                            )
                            .when(capped, |this| this.child(widgets::muted(more, cx)))
                            .into_any_element(),
                    );
                } else if let Some(asset) = addon_asset {
                    out.push(
                        v_flex()
                            .id(SharedString::from(format!("reshade-extra-files-body-{i}")))
                            .pl_6()
                            .child(widgets::mono(asset, cx))
                            .into_any_element(),
                    );
                }
            }
        }
        out
    }
}
