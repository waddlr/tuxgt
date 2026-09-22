use gpui_kit::component::button::ButtonVariants as _;
use gpui_kit::component::{h_flex, v_flex, ActiveTheme};
use gpui_kit::*;
use tuxgt_core::{data_dir, list_games, open_db_shared, remove_manual, FluentArgs};

use super::super::widgets;
use super::super::{rt_block, Nav, Shell};

impl Shell {
    /// R34: remove confirm for one manual row. Steam/Heroic rows are
    /// provider-owned and never offer this path.
    pub(crate) fn manual_remove_card(&self, view: Entity<Self>, cx: &App) -> impl IntoElement {
        let b = cx.theme();
        let id = self.manual_remove.clone().unwrap_or_default();
        v_flex()
            .id("manual-remove")
            .gap_2()
            .p_3()
            .rounded(px(4.))
            .bg(b.sidebar)
            .border_1()
            .border_color(cx.theme().danger)
            .child({
                let mut args = FluentArgs::new();
                args.set("id", id.clone());
                widgets::muted(
                    self.strings.get_args("gui-note-manual-remove", Some(&args)),
                    cx,
                )
            })
            .child(
                h_flex()
                    .gap_1()
                    .child(
                        widgets::btn("manual-remove-confirm", cx)
                            .primary()
                            .child(widgets::blabel(self.strings.get("gui-action-remove"), cx))
                            .on_click({
                                let view = view.clone();
                                let id = id.clone();
                                move |_, _, cx| {
                                    view.update(cx, |this, cx| {
                                        this.remove_manual_ui(id.clone(), cx)
                                    });
                                }
                            }),
                    )
                    .child(
                        widgets::btn("manual-remove-cancel", cx)
                            .ghost()
                            .child(widgets::blabel(self.strings.get("gui-action-cancel"), cx))
                            .on_click({
                                let view = view.clone();
                                move |_, _, cx| {
                                    view.update(cx, |this, cx| {
                                        this.manual_remove = None;
                                        cx.notify();
                                    });
                                }
                            }),
                    ),
            )
    }

    pub(crate) fn remove_manual_ui(&mut self, id: String, cx: &mut Context<Self>) {
        tracing::debug!(action = "remove-manual", game = id.as_str());
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    rt_block(async {
                        let pool = open_db_shared(&data_dir()).await?;
                        remove_manual(&pool, &data_dir(), &id).await?;
                        let games = list_games(&pool, None, None, None).await?;
                        Ok::<_, tuxgt_core::Error>((id, games))
                    })
                })
                .await;
            let _ = this.update(cx, |this, cx| match result {
                Ok((gone, rows)) => {
                    this.apply_scan_rows(rows);
                    this.manual_remove = None;
                    let mut args = FluentArgs::new();
                    args.set("id", gone);
                    this.status = this
                        .strings
                        .get_args("gui-status-manual-removed", Some(&args));
                    this.revalidate_selection();
                    if this.nav == Nav::Game {
                        this.reload_selected_row();
                    }
                    cx.notify();
                }
                Err(e) => {
                    let mut args = FluentArgs::new();
                    args.set("error", e.to_string());
                    this.status = this.strings.get_args("gui-status-err-manual", Some(&args));
                    cx.notify();
                }
            });
        })
        .detach();
    }
}
