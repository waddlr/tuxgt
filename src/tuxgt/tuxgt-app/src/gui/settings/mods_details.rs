//! Settings Mods "Details" disclosure. Short pairs sit in a fixed two-column
//! grid (the pack-window height is half the cell count). Lists of one or two
//! entries paint in place; longer lists are a nested `▸` button. Files and
//! Effects live here, not in the card header. The game picker and the
//! per-game card keep their own preview buttons.

use gpui_kit::assets::IconName as FullIconName;
use gpui_kit::component::button::ButtonVariants as _;
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::component::{h_flex, v_flex, ActiveTheme, Icon, Sizable as _};
use gpui_kit::*;

use tuxgt_core::{is_config_text, FluentArgs};

use super::super::theme::{types, TypeStyled as _};
use super::super::widgets;
use super::super::{ConfigLevel, InstanceRow, Shell};
use super::mods_detail_view::NameBlock;

const VISIBLE: usize = 10;

impl Shell {
    pub(crate) fn details_button(
        &self,
        view: Entity<Self>,
        row: &InstanceRow,
        open: bool,
        cx: &App,
    ) -> impl IntoElement {
        let arrow = if open { "▾" } else { "▸" };
        let id = row.id.clone();
        widgets::btn(row.ids.details.clone(), cx)
            .ghost()
            .child(widgets::muted(
                format!("{} {arrow}", self.strings.get("gui-mod-detail")),
                cx,
            ))
            .on_click(move |_, _, cx| {
                cx.stop_propagation();
                view.update(cx, |this, cx| this.toggle_details(&id, cx));
            })
    }

    pub(crate) fn toggle_details(&mut self, id: &str, cx: &mut Context<Self>) {
        let key = format!("det:{id}");
        if !self.preview_open.insert(key) {
            self.preview_open.remove(&format!("det:{id}"));
            cx.notify();
            return;
        }
        self.fill_file_preview(id);
        cx.notify();
    }

    pub(crate) fn details_body(
        &self,
        view: Entity<Self>,
        row: &InstanceRow,
        cx: &App,
    ) -> impl IntoElement {
        let model = self.detail_view(row);
        let mut body = v_flex()
            .id(SharedString::from(format!("details-{}", row.id)))
            .w_full()
            .gap_1();
        if !model.cells.is_empty() {
            body = body.child(self.detail_grid(&row.id, &model.cells, cx));
        }
        for (label, value) in &model.lines {
            body = body.child(self.kv(
                SharedString::from(format!("dl-{}-{label}", row.id)),
                label,
                value,
                cx,
            ));
        }
        for block in &model.inline {
            body = body.child(self.detail_names(block, true, view.clone(), cx));
        }
        if !model.nested.is_empty() {
            body = body.child(self.nested_buttons(&row.id, &model.nested, view.clone(), cx));
            for block in &model.nested {
                if self.preview_open.contains(&block.key) {
                    body = body.child(self.detail_names(block, false, view.clone(), cx));
                }
            }
        }
        body
    }

    /// Inner height of an open Details body. The card's `gap_1` above it is
    /// added by `catalog_row_h`. Closed is zero: nested lists stay armed but
    /// do not reserve space until Details is open.
    pub(crate) fn details_h(&self, id: &str, cx: &App) -> Pixels {
        if !self.preview_open.contains(&format!("det:{id}")) {
            return px(0.);
        }
        let Some(row) = self.instances.iter().find(|r| r.id == id) else {
            return px(0.);
        };
        let model = self.detail_view(row);
        let t = types(cx);
        let gap = cx.theme().font_size * 0.25;
        let text = t.label_lg.line.max(t.body_md.line);
        let mut parts: Vec<Pixels> = Vec::new();
        if !model.cells.is_empty() {
            let rows = (model.cells.len() + 1) / 2;
            let mut h = text * rows as f32;
            if rows > 1 {
                h += gap * (rows - 1) as f32;
            }
            parts.push(h);
        }
        parts.extend(model.lines.iter().map(|_| text));
        for block in &model.inline {
            parts.push(self.names_h(block, true, cx));
        }
        if !model.nested.is_empty() {
            parts.push(t.control_h);
            for block in &model.nested {
                if self.preview_open.contains(&block.key) {
                    parts.push(self.names_h(block, false, cx));
                }
            }
        }
        if parts.is_empty() {
            return px(0.);
        }
        let mut h = px(0.);
        for part in &parts {
            h += *part;
        }
        h + gap * (parts.len() - 1) as f32
    }

    fn detail_grid(&self, id: &str, cells: &[(String, String)], cx: &App) -> impl IntoElement {
        let mut grid = v_flex().w_full().gap_1();
        for (n, pair) in cells.chunks(2).enumerate() {
            let mut row = h_flex().w_full().gap_3().items_center();
            for (label, value) in pair {
                row = row.child(self.kv(
                    SharedString::from(format!("kv-{id}-{n}-{label}")),
                    label,
                    value,
                    cx,
                ));
            }
            if pair.len() == 1 {
                row = row.child(div().flex_1());
            }
            grid = grid.child(row);
        }
        grid
    }

    fn kv(&self, id: impl Into<ElementId>, label: &str, value: &str, cx: &App) -> impl IntoElement {
        let tip = SharedString::from(value.to_string());
        let label = label.to_string();
        let value = value.to_string();
        h_flex()
            .id(id)
            .flex_1()
            .min_w_0()
            .gap_2()
            .items_center()
            .child(
                div()
                    .flex_shrink_0()
                    .tx(types(cx).label_lg)
                    .text_color(cx.theme().muted_foreground)
                    .child(label),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .tx(types(cx).body_md)
                    .child(value),
            )
            .tooltip(move |window, cx| Tooltip::new(tip.clone()).build(window, cx))
    }

    fn nested_buttons(
        &self,
        row_id: &str,
        blocks: &[NameBlock],
        view: Entity<Self>,
        cx: &App,
    ) -> impl IntoElement {
        h_flex()
            .w_full()
            .gap_1()
            .flex_wrap()
            .children(blocks.iter().map(|b| {
                let open = self.preview_open.contains(&b.key);
                let arrow = if open { "▾" } else { "▸" };
                let label = format!("{} {arrow} {}", b.title, b.names.len());
                let key = b.key.clone();
                let toggle = view.clone();
                widgets::btn(
                    SharedString::from(format!("dn-{row_id}-{}", b.key.replace(':', "-"))),
                    cx,
                )
                .ghost()
                .child(widgets::muted(label, cx))
                .on_click(move |_, _, cx| {
                    cx.stop_propagation();
                    toggle.update(cx, |this, cx| {
                        if !this.preview_open.insert(key.clone()) {
                            this.preview_open.remove(&key);
                        }
                        cx.notify();
                    });
                })
            }))
    }

    fn detail_names(
        &self,
        block: &NameBlock,
        heading: bool,
        view: Entity<Self>,
        cx: &App,
    ) -> impl IntoElement {
        let expanded = self.preview_expanded.contains(&block.key);
        let shown = if expanded {
            block.names.len()
        } else {
            block.names.len().min(VISIBLE)
        };
        let mut list = v_flex()
            .id(SharedString::from(format!(
                "dlist-{}",
                block.key.replace(':', "-")
            )))
            .w_full()
            .gap_1();
        if heading {
            list = list.child(widgets::muted(block.title.clone(), cx));
        }
        for (i, name) in block.names.iter().take(shown).enumerate() {
            if block.edit_id.is_some() && is_config_text(name) {
                let mod_id = block.edit_id.clone().unwrap_or_default();
                let rel = name.clone();
                let edit_view = view.clone();
                list = list.child(
                    h_flex()
                        .gap_2()
                        .items_center()
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .child(widgets::mono(name.clone(), cx)),
                        )
                        .child(
                            widgets::btn(
                                SharedString::from(format!(
                                    "ec-{}-{i}",
                                    block.key.replace(':', "-")
                                )),
                                cx,
                            )
                            .secondary()
                            .child(Icon::new(FullIconName::FilePenLine).small())
                            .tooltip(self.strings.get("gui-action-edit-config"))
                            .on_click(move |_, window, cx| {
                                edit_view.update(cx, |this, cx| {
                                    this.open_config_ui(
                                        ConfigLevel::Global { id: mod_id.clone() },
                                        rel.clone(),
                                        window,
                                        cx,
                                    );
                                });
                            }),
                        ),
                );
            } else {
                list = list.child(widgets::mono(name.clone(), cx));
            }
        }
        if block.names.len() > VISIBLE {
            let label = if expanded {
                self.strings.get("gui-preview-less")
            } else {
                let mut args = FluentArgs::new();
                args.set("n", block.names.len().saturating_sub(VISIBLE).to_string());
                self.strings.get_args("gui-preview-more", Some(&args))
            };
            let key = block.key.clone();
            let toggle = view.clone();
            list = list.child(
                widgets::btn(
                    SharedString::from(format!("dmore-{}", block.key.replace(':', "-"))),
                    cx,
                )
                .ghost()
                .child(widgets::muted(label, cx))
                .on_click(move |_, _, cx| {
                    cx.stop_propagation();
                    toggle.update(cx, |this, cx| {
                        if !this.preview_expanded.insert(key.clone()) {
                            this.preview_expanded.remove(&key);
                        }
                        cx.notify();
                    });
                }),
            );
        }
        list
    }

    fn names_h(&self, block: &NameBlock, heading: bool, cx: &App) -> Pixels {
        let t = types(cx);
        let gap = cx.theme().font_size * 0.25;
        let expanded = self.preview_expanded.contains(&block.key);
        let shown = if expanded {
            block.names.len()
        } else {
            block.names.len().min(VISIBLE)
        };
        let mut n = if heading { 1 } else { 0 };
        let mut h = if heading { t.body_md.line } else { px(0.) };
        for name in block.names.iter().take(shown) {
            let row = if block.edit_id.is_some() && is_config_text(name) {
                t.label_lg.line.max(t.control_h)
            } else {
                t.label_lg.line
            };
            h += row;
            n += 1;
        }
        if block.names.len() > VISIBLE {
            h += t.control_h;
            n += 1;
        }
        if n > 1 {
            h += gap * (n - 1) as f32;
        }
        h
    }
}
