use gpui_kit::component::button::ButtonVariants as _;
use gpui_kit::component::h_flex;
use gpui_kit::component::menu::{DropdownMenu as _, PopupMenuItem};
use gpui_kit::component::switch::Switch;
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::component::Sizable as _;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use tuxgt_core::{ArgKind, ArgMark, ArgView};

use super::super::widgets;
use super::super::Shell;

impl Shell {
    pub(crate) fn args_section(
        &self,
        game_id: &str,
        view: Entity<Self>,
        cx: &App,
    ) -> impl IntoElement {
        let rows = self.arg_rows.clone();
        let mut seen = Vec::new();
        let mut active = Vec::new();
        let mut suppressed = Vec::new();
        for row in &rows {
            if row.mark == ArgMark::Suppressed {
                suppressed.push(row.clone());
                continue;
            }
            if let Some(group) = &row.group {
                if seen.iter().any(|g| g == group) {
                    continue;
                }
                seen.push(group.clone());
                let members: Vec<ArgView> = rows
                    .iter()
                    .filter(|r| {
                        r.group.as_deref() == Some(group.as_str()) && r.mark != ArgMark::Suppressed
                    })
                    .cloned()
                    .collect();
                active.push(self.arg_group_row(game_id, &members, view.clone(), cx));
            } else {
                active.push(self.arg_single_row(game_id, row, view.clone(), cx));
            }
        }
        let custom = self.args_custom.trim();
        widgets::section_card("game-args", cx)
            .child(widgets::section_title(
                self.strings.get("gui-args-section"),
                cx,
            ))
            .when(self.args_dirty, |this| {
                let game = game_id.to_string();
                let discard_game = game.clone();
                let view_d = view.clone();
                this.child(widgets::muted(self.strings.get("gui-args-dirty"), cx))
                    .child(
                        h_flex()
                            .gap_2()
                            .child(self.args_text_btn(
                                "args-save",
                                "gui-args-save",
                                view.clone(),
                                cx,
                                move |this, cx| {
                                    this.save_args_ui(&game, cx);
                                },
                            ))
                            .child(self.args_text_btn(
                                "args-discard",
                                "gui-args-discard",
                                view_d,
                                cx,
                                move |this, cx| {
                                    this.discard_args_ui(&discard_game, cx);
                                },
                            )),
                    )
            })
            .children(active)
            .when(!suppressed.is_empty(), |this| {
                this.child(widgets::muted(self.strings.get("gui-args-applied"), cx))
                    .children(suppressed.iter().map(|row| {
                        let label = row.appended.clone().unwrap_or_else(|| row.template.clone());
                        widgets::muted(label, cx)
                    }))
            })
            .when(!custom.is_empty(), |this| {
                this.child(widgets::muted(custom.to_string(), cx))
            })
            .when(rows.is_empty() && custom.is_empty(), |this| {
                this.child(widgets::muted(self.strings.get("gui-args-none"), cx))
            })
    }

    fn args_text_btn(
        &self,
        id: &'static str,
        label: &'static str,
        view: Entity<Self>,
        cx: &App,
        op: impl Fn(&mut Self, &mut Context<Self>) + 'static,
    ) -> impl IntoElement {
        let label = self.strings.get(label);
        widgets::btn(id, cx)
            .secondary()
            .child(widgets::blabel(label, cx))
            .on_click(move |_, _, cx| {
                view.update(cx, |this, cx| op(this, cx));
            })
    }

    fn arg_single_row(
        &self,
        game_id: &str,
        row: &ArgView,
        view: Entity<Self>,
        cx: &App,
    ) -> gpui_kit::AnyElement {
        let control = match row.kind {
            ArgKind::Flag => self.arg_switch(row, view, cx).into_any_element(),
            ArgKind::Enum => self.arg_enum_menu(row, view, cx).into_any_element(),
            ArgKind::Freeform => widgets::muted(
                if row.value.is_empty() {
                    self.strings.get("gui-args-none")
                } else {
                    row.value.clone().into()
                },
                cx,
            )
            .into_any_element(),
        };
        self.arg_labeled(game_id, row, control, cx)
    }

    fn arg_group_row(
        &self,
        game_id: &str,
        members: &[ArgView],
        view: Entity<Self>,
        cx: &App,
    ) -> gpui_kit::AnyElement {
        let members: Vec<ArgView> = members.to_vec();
        let chosen = members.iter().find(|r| r.enabled).cloned();
        let label: SharedString = match &chosen {
            Some(row) if row.kind == ArgKind::Enum && !row.value.is_empty() => {
                row.value.clone().into()
            }
            Some(row) => row.id.clone().into(),
            None => self.strings.get("gui-args-none").into(),
        };
        let head = chosen.unwrap_or_else(|| members[0].clone());
        let group = members[0].group.clone().unwrap_or_else(|| "g".into());
        let none = self.strings.get("gui-args-none");
        let control = widgets::btn(format!("arg-group-{group}"), cx)
            .secondary()
            .child(widgets::blabel(label, cx))
            .dropdown_menu({
                let menu_view = view;
                move |menu, _, _| {
                    let mut menu = menu.item(PopupMenuItem::new(none.clone()).on_click({
                        let view = menu_view.clone();
                        let ids: Vec<String> = members
                            .iter()
                            .filter(|r| r.enabled)
                            .map(|r| r.id.clone())
                            .collect();
                        move |_, _, cx| {
                            let ids = ids.clone();
                            view.update(cx, |this, cx| this.disable_game_args(ids, cx));
                        }
                    }));
                    for row in &members {
                        match row.kind {
                            ArgKind::Flag => {
                                let id = row.id.clone();
                                let view = menu_view.clone();
                                let help = row.help.clone();
                                menu = menu.item(PopupMenuItem::new(help).on_click(
                                    move |_, _, cx| {
                                        let id = id.clone();
                                        view.update(cx, |this, cx| {
                                            this.set_game_arg(id, None, cx);
                                        });
                                    },
                                ));
                            }
                            ArgKind::Enum => {
                                for value in &row.values {
                                    let id = row.id.clone();
                                    let picked = value.value.clone();
                                    let view = menu_view.clone();
                                    let text = format!("{} — {}", row.id, value.value);
                                    menu = menu.item(PopupMenuItem::new(text).on_click(
                                        move |_, _, cx| {
                                            let id = id.clone();
                                            let picked = picked.clone();
                                            view.update(cx, |this, cx| {
                                                this.set_game_arg(id, Some(picked), cx);
                                            });
                                        },
                                    ));
                                }
                            }
                            ArgKind::Freeform => {}
                        }
                    }
                    menu
                }
            });
        self.arg_labeled(game_id, &head, control, cx)
    }

    fn arg_switch(&self, row: &ArgView, view: Entity<Self>, _cx: &App) -> Switch {
        let id = row.id.clone();
        Switch::new(SharedString::from(format!("arg-sw-{id}")))
            .checked(row.enabled)
            .xsmall()
            .on_click(move |val, _, cx| {
                let on = *val;
                let id = id.clone();
                view.update(cx, |this, cx| {
                    if on {
                        this.set_game_arg(id, None, cx);
                    } else {
                        this.disable_game_args(vec![id], cx);
                    }
                });
            })
    }

    fn arg_enum_menu(&self, row: &ArgView, view: Entity<Self>, cx: &App) -> impl IntoElement {
        let label: SharedString = if row.enabled && !row.value.is_empty() {
            row.value.clone().into()
        } else {
            self.strings.get("gui-args-none").into()
        };
        let values = row.values.clone();
        let id = row.id.clone();
        let none = self.strings.get("gui-args-none");
        widgets::btn(format!("arg-enum-{id}"), cx)
            .secondary()
            .child(widgets::blabel(label, cx))
            .dropdown_menu(move |menu, _, _| {
                let mut menu = menu.item(PopupMenuItem::new(none.clone()).on_click({
                    let view = view.clone();
                    let id = id.clone();
                    move |_, _, cx| {
                        let id = id.clone();
                        view.update(cx, |this, cx| this.disable_game_args(vec![id], cx));
                    }
                }));
                for value in &values {
                    let picked = value.value.clone();
                    let id = id.clone();
                    let view = view.clone();
                    menu = menu.item(
                        PopupMenuItem::new(format!("{} — {}", value.value, value.help)).on_click(
                            move |_, _, cx| {
                                let id = id.clone();
                                let picked = picked.clone();
                                view.update(cx, |this, cx| {
                                    this.set_game_arg(id, Some(picked), cx);
                                });
                            },
                        ),
                    );
                }
                menu
            })
    }

    fn arg_labeled(
        &self,
        game_id: &str,
        row: &ArgView,
        control: impl IntoElement,
        cx: &App,
    ) -> gpui_kit::AnyElement {
        let mut tip = row.help.clone();
        if row.kind == ArgKind::Freeform {
            tip.push_str(&format!("\ntuxgt args set {game_id} {} <value>", row.id));
        }
        if let Some(url) = &row.source_url {
            tip.push('\n');
            tip.push_str(url);
        }
        let label = row.group.clone().unwrap_or_else(|| row.id.clone());
        let row_id = row.id.clone();
        let link = row.source_url.clone().map(|url| {
            let host = url
                .trim_start_matches("https://")
                .trim_start_matches("http://")
                .split('/')
                .next()
                .unwrap_or("source")
                .to_string();
            widgets::open_btn(
                format!("arg-src-{row_id}"),
                widgets::OpenKind::Link,
                host,
                Some(url),
                cx,
            )
        });
        let control = h_flex()
            .gap_2()
            .items_center()
            .child(control)
            .children(link);
        div()
            .id(SharedString::from(format!("arg-tip-{label}")))
            .w_full()
            .tooltip({
                let tip = tip.clone();
                move |window, cx| Tooltip::new(tip.clone()).build(window, cx)
            })
            .child(widgets::labeled_row(
                format!("arg-{label}"),
                label,
                None,
                control,
                cx,
            ))
            .into_any_element()
    }
}
