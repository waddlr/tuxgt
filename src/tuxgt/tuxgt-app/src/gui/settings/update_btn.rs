use gpui_kit::component::button::ButtonVariants as _;
use gpui_kit::component::{h_flex, Disableable as _};
use gpui_kit::*;

use tuxgt_core::FluentArgs;

use super::super::widgets;
use super::super::Shell;
use super::update::AppUpdate;

impl Shell {
    /// The single About update button, morphing by state. `Unknown` checks,
    /// `Available` applies, `Updating` cancels; every other state is a
    /// disabled honest label.
    pub(crate) fn app_update_button(&self, view: Entity<Self>, cx: &App) -> AnyElement {
        match &self.app_update {
            AppUpdate::Unknown => widgets::btn("about-check-update", cx)
                .secondary()
                .child(widgets::blabel(
                    self.strings.get("gui-action-check-updates"),
                    cx,
                ))
                .on_click({
                    let view = view.clone();
                    move |_, _, cx| {
                        view.update(cx, |this, cx| this.check_app_update_ui(cx));
                    }
                })
                .into_any_element(),
            AppUpdate::Checking => widgets::btn("about-check-update", cx)
                .secondary()
                .disabled(true)
                .child(widgets::blabel(self.strings.get("gui-action-checking"), cx))
                .into_any_element(),
            AppUpdate::Available { tag, .. } => {
                let mut args = FluentArgs::new();
                args.set("tag", tag.clone());
                widgets::btn("about-check-update", cx)
                    .primary()
                    .child(widgets::blabel(
                        self.strings.get_args("gui-action-update-to", Some(&args)),
                        cx,
                    ))
                    .on_click({
                        let view = view.clone();
                        move |_, _, cx| {
                            view.update(cx, |this, cx| this.apply_app_update_ui(cx));
                        }
                    })
                    .into_any_element()
            }
            AppUpdate::UpToDate => widgets::btn("about-check-update", cx)
                .secondary()
                .disabled(true)
                .child(widgets::blabel(
                    self.strings.get("gui-status-up-to-date"),
                    cx,
                ))
                .into_any_element(),
            AppUpdate::Updating if self.app_update_can_cancel() => {
                widgets::btn("about-check-update", cx)
                    .secondary()
                    .child(widgets::blabel(self.strings.get("gui-action-cancel"), cx))
                    .on_click({
                        let view = view.clone();
                        move |_, _, cx| {
                            view.update(cx, |this, cx| this.cancel_app_update_ui(cx));
                        }
                    })
                    .into_any_element()
            }
            AppUpdate::Updating => widgets::btn("about-check-update", cx)
                .secondary()
                .disabled(true)
                .child(widgets::blabel(self.strings.get("gui-action-updating"), cx))
                .into_any_element(),
            AppUpdate::Updated { tag } => {
                let mut args = FluentArgs::new();
                args.set("tag", tag.clone());
                widgets::btn("about-check-update", cx)
                    .secondary()
                    .disabled(true)
                    .child(widgets::blabel(
                        self.strings
                            .get_args("gui-status-restart-to-use", Some(&args)),
                        cx,
                    ))
                    .into_any_element()
            }
        }
    }

    /// About header right cluster: the Repo link plus the update button.
    pub(crate) fn about_head_actions(&self, view: Entity<Self>, cx: &App) -> impl IntoElement {
        h_flex()
            .gap_1()
            .items_center()
            .child(widgets::open_btn(
                "about-repo",
                widgets::OpenKind::Link,
                self.strings.get("gui-action-repo"),
                Some("https://github.com/waddlr/tuxgt".to_string()),
                cx,
            ))
            .child(self.app_update_button(view, cx))
    }
}
