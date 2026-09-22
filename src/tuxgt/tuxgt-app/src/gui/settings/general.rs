use gpui_kit::component::button::ButtonVariants as _;
use gpui_kit::component::menu::PopupMenuItem;
use gpui_kit::component::switch::Switch;
use gpui_kit::component::{h_flex, v_flex, ActiveTheme, IconName, Sizable as _};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

use tuxgt_core::{data_dir, FluentArgs};

use super::super::theme::{types, FontScale, ThemeId, TypeStyled as _};
use super::super::widgets;
use super::super::Shell;

impl Shell {
    pub(crate) fn settings_general(&self, view: Entity<Self>, cx: &App) -> impl IntoElement {
        widgets::cols(
            "two-col",
            280.,
            false,
            vec![
                (
                    1,
                    v_flex()
                        .id("set-left")
                        .gap_3()
                        .child(self.about_box(view.clone(), cx))
                        .child(self.appearance_box(view.clone(), cx))
                        .child(self.host_box(cx))
                        .child(self.host_install_box(view.clone(), cx))
                        .into_any_element(),
                ),
                (
                    1,
                    v_flex()
                        .id("set-right")
                        .gap_3()
                        .child(self.library_box(view.clone(), cx))
                        .child(self.host_tools_box(view, cx))
                        .into_any_element(),
                ),
            ],
        )
    }
    pub(crate) fn appearance_box(&self, view: Entity<Self>, cx: &App) -> impl IntoElement {
        widgets::section_card("appearance", cx)
            .child(widgets::section_title(
                self.strings.get("gui-section-appearance"),
                cx,
            ))
            .child(widgets::muted(self.strings.get("gui-note-appearance"), cx))
            .child(widgets::labeled_row(
                "appearance-theme",
                self.strings.get("gui-label-theme"),
                None,
                widgets::value_btn(
                    "theme-pick",
                    self.strings.get(self.prefs.theme_id().label_id()),
                    {
                        let current = self.prefs.theme_id();
                        let labels: Vec<(ThemeId, String)> = ThemeId::ALL
                            .iter()
                            .copied()
                            .map(|id| (id, self.strings.get(id.label_id())))
                            .collect();
                        let view = view.clone();
                        move |menu, _, _| {
                            let mut menu = menu;
                            for (id, label) in &labels {
                                let id = *id;
                                menu = menu.item(
                                    PopupMenuItem::new(label.clone())
                                        .checked(id == current)
                                        .on_click({
                                            let view = view.clone();
                                            move |_, window, cx| {
                                                view.update(cx, |this, cx| {
                                                    this.set_theme(id, window, cx)
                                                });
                                            }
                                        }),
                                );
                            }
                            menu
                        }
                    },
                    cx,
                ),
                cx,
            ))
            .child(widgets::labeled_row(
                "appearance-density",
                self.strings.get("gui-label-density"),
                None,
                h_flex()
                    .gap_1()
                    .children(FontScale::ALL.iter().copied().map(|s| {
                        let on = self.prefs.scale() == s;
                        widgets::btn(s.id(), cx)
                            .when(on, |t| t.primary())
                            .when(!on, |t| t.secondary())
                            .child(widgets::blabel(self.strings.get(s.label_id()), cx))
                            .on_click({
                                let view = view.clone();
                                move |_, window, cx| {
                                    view.update(cx, |this, cx| this.set_scale(s, window, cx));
                                }
                            })
                    })),
                cx,
            ))
    }

    /// One Mod row (E97 grammar). User Mods are object cards: accent stripe,
    /// enable left of the name, Export / Rescan / trash right, previews in the
    /// card. Official cards are property rows (enable only, E86 lock 12).
    pub(crate) fn library_box(&self, view: Entity<Self>, cx: &App) -> impl IntoElement {
        let count = |m: &str| self.index.iter().filter(|g| g.manager == m).count();
        widgets::section_card("library-inventory", cx)
            .child(widgets::section_header(
                "library-inventory-head",
                self.strings.get("gui-section-library"),
                self.page_scroll.bounds().size.width,
                vec![widgets::SectionAction::new(
                    "rescan-all",
                    self.strings.get("gui-action-rescan-libs"),
                    {
                        let view = view.clone();
                        move |_, _, cx| {
                            view.update(cx, |this, cx| this.rescan(cx));
                        }
                    },
                )
                .icon(widgets::bicon(IconName::RotateCw))
                .primary()],
                cx,
            ))
            .child({
                let mut args = FluentArgs::new();
                args.set("count", self.index.len().to_string());
                widgets::labeled_row(
                    "lib-total",
                    self.strings.get("gui-inventory-total"),
                    None,
                    widgets::mono(self.strings.get_args("gui-chip-indexed", Some(&args)), cx),
                    cx,
                )
            })
            .children(
                [
                    ("steam", "plugin-steam-label", "gui-inventory-steam"),
                    ("heroic", "plugin-heroic-label", "gui-inventory-heroic"),
                    ("manual", "plugin-manual-label", "gui-inventory-manual"),
                ]
                .map(|(id, label_id, count_id)| {
                    let mut args = FluentArgs::new();
                    args.set("count", count(id).to_string());
                    widgets::labeled_row(
                        SharedString::from(format!("lib-{id}")),
                        self.strings.get(label_id),
                        None,
                        widgets::mono(self.strings.get_args(count_id, Some(&args)), cx),
                        cx,
                    )
                }),
            )
    }

    /// E61: unpack PATH tools the installer shells out to. Rows come from the
    /// `all_tools()` probe taken on Settings entry (`EXT_TOOLS` order) plus
    /// the retest button in the header, so paint never re-execs a tool.
    /// Missing is honest — install still errors at unpack.
    pub(crate) fn about_box(&self, view: Entity<Self>, cx: &App) -> impl IntoElement {
        let t = types(cx);
        let data = data_dir();
        let version = env!("CARGO_PKG_VERSION").to_string();
        let built = option_env!("VERGEN_BUILD_DATE").unwrap_or("unknown");
        widgets::section_card("about", cx)
            .child(widgets::section_title(
                self.strings.get("gui-section-about-app"),
                cx,
            ))
            .child(
                h_flex()
                    .gap_2()
                    .items_center()
                    .child(
                        img(super::icon_file())
                            .w(px(32.))
                            .h(px(32.))
                            .rounded(px(6.)),
                    )
                    .child(
                        v_flex()
                            .child(div().tx(t.headline_md).child(self.strings.get("gui-title")))
                            .child(widgets::muted(format!("v{version} · {built}"), cx)),
                    )
                    .child(widgets::open_btn(
                        "about-repo",
                        widgets::OpenKind::Link,
                        self.strings.get("gui-action-repo"),
                        Some("https://github.com/waddlr/tuxgt".to_string()),
                        cx,
                    )),
            )
            .child(widgets::labeled_row(
                "about-install",
                self.strings.get("gui-label-install-dir"),
                None,
                h_flex()
                    .gap_1()
                    .items_center()
                    .child(
                        div()
                            .tx(t.label_lg)
                            .text_color(cx.theme().muted_foreground)
                            .truncate()
                            .child(data.display().to_string()),
                    )
                    .child(widgets::open_btn(
                        "about-open-install",
                        widgets::OpenKind::Folder,
                        self.strings.get("gui-action-open"),
                        Some(data.display().to_string()),
                        cx,
                    )),
                cx,
            ))
            .child(widgets::labeled_row(
                "about-logs",
                self.strings.get("gui-label-logs-dir"),
                None,
                h_flex()
                    .gap_1()
                    .items_center()
                    .child(
                        div()
                            .tx(t.label_lg)
                            .text_color(cx.theme().muted_foreground)
                            .truncate()
                            .child(data.join("logs").display().to_string()),
                    )
                    .child(widgets::open_btn(
                        "about-open-logs",
                        widgets::OpenKind::Folder,
                        self.strings.get("gui-action-open"),
                        Some(data.join("logs").display().to_string()),
                        cx,
                    )),
                cx,
            ))
            .child(widgets::labeled_row(
                "about-debug",
                self.strings.get("gui-label-debug-log"),
                Some(SharedString::from(self.strings.get("gui-tip-debug-log"))),
                Switch::new("about-debug-switch")
                    .checked(self.prefs.debug_log)
                    .xsmall()
                    .on_click({
                        let view = view.clone();
                        move |on, _, cx| {
                            let on = *on;
                            view.update(cx, |this, cx| this.set_debug_log(on, cx));
                        }
                    }),
                cx,
            ))
    }
}
