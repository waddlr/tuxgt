use gpui_kit::component::button::ButtonVariants as _;
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::component::input::Textarea;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

use super::super::*;
use super::{config_abs_path, ConfigLevel};

impl Shell {
    /// Full-page editor (`gui.mod-config-edit`): the containing tab takes
    /// over its content for it (add-form pattern), so the page IS the
    /// editor — no buried inline card, no manual scrolling to find it.
    /// Title row = context + mono rel + Open externally; viewport-clamped
    /// Textarea body (viewport height minus title/footer reserve, 220–560px);
    /// Save / Cancel footer. Externally-only files paint the note + Open.
    /// A successful external open closes this page: the focus hook refreshes
    /// pills/previews from `config_external_level`, so no open buffer can
    /// conflict with the external save.
    pub(crate) fn config_page(&self, view: Entity<Self>, cx: &App) -> AnyElement {
        let Some(edit) = self.config_edit.as_ref() else {
            return div().into_any_element();
        };
        let context = match &edit.level {
            ConfigLevel::Global { id } => id.clone(),
            ConfigLevel::Staged { game, instance } => format!("{game} / {instance}"),
        };
        let open_view = view.clone();
        let open_level = edit.level.clone();
        let open_rel = edit.rel.clone();
        let save_view = view.clone();
        let cancel_view = view.clone();
        v_flex()
            .id("config-edit-page")
            .gap_2()
            .child(
                h_flex()
                    .gap_2()
                    .items_center()
                    .child(widgets::muted(context, cx))
                    .child(widgets::mono(edit.rel.clone(), cx))
                    .child(div().flex_1())
                    .child(
                        widgets::btn("config-open", cx)
                            .secondary()
                            .child(widgets::blabel(
                                self.strings.get("gui-action-open-external"),
                                cx,
                            ))
                            .on_click(move |_, _, cx| {
                                // Flag only a launched open: refocus must not
                                // re-hash stages for a failed resolve/launch.
                                // Closes this page: no open buffer survives to
                                // conflict with the external save.
                                if let Ok(path) = config_abs_path(&open_level, &open_rel) {
                                    if widgets::open_file_external(&path) {
                                        let level = open_level.clone();
                                        open_view.update(cx, |this, cx| {
                                            this.config_edit = None;
                                            this.config_nav_pending = None;
                                            this.config_external_open = true;
                                            this.config_external_level = Some(level);
                                            cx.notify();
                                        });
                                    }
                                }
                            }),
                    ),
            )
            .when(edit.external_only, |this| {
                this.child(widgets::muted(
                    self.strings.get("gui-note-config-external-only"),
                    cx,
                ))
            })
            .when(!edit.external_only, |this| {
                // Footer must stay in view: clamp the body to the scroll
                // viewport minus title/footer reserve (was fixed 560px,
                // which pushed Save/Cancel below short windows).
                let vp: f32 = self.page_scroll.bounds().size.height.into();
                let th = vp - 140.0;
                let body_h = px(th.clamp(220.0, 560.0));
                this.child(Textarea::new(&self.config_input).h(body_h))
                    .child(
                        h_flex()
                            .gap_1()
                            .child(
                                widgets::btn("config-save", cx)
                                    .secondary()
                                    .child(widgets::blabel(
                                        self.strings.get("gui-action-save"),
                                        cx,
                                    ))
                                    .on_click(move |_, _, cx| {
                                        save_view.update(cx, |this, cx| {
                                            this.save_config_ui(cx);
                                        });
                                    }),
                            )
                            .child(
                                widgets::btn("config-cancel", cx)
                                    .secondary()
                                    .child(widgets::blabel(
                                        self.strings.get("gui-action-cancel"),
                                        cx,
                                    ))
                                    .on_click(move |_, _, cx| {
                                        cancel_view.update(cx, |this, cx| {
                                            this.cancel_config_ui(cx);
                                        });
                                    }),
                            ),
                    )
            })
            .into_any_element()
    }
}
