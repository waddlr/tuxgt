use super::*;
use gpui_kit::component::button::ButtonVariants as _;
use gpui_kit::component::{h_flex, v_flex, ActiveTheme};
use gpui_kit::*;
use tuxgt_core::GameRow;

use super::super::widgets;
use super::super::Shell;

/// Idle delay before applying a new ceil-pack box while resizing.
const GRID_METRICS_SETTLE_MS: u64 = 120;

impl Shell {
    /// R01: top-5 by `last_played` DESC, honouring visibility filters but
    /// never the active sort. Empty when nothing has been played yet.
    pub(crate) fn recently_played(&self) -> Vec<&GameRow> {
        // Shared visible set (active sort), restored to index order so the
        // stable recent-sort below ties exactly as before.
        let (mut idx, _) = self.visible();
        idx.sort_unstable();
        let mut out: Vec<&GameRow> = idx
            .into_iter()
            .map(|i| &self.games[i])
            .filter(|g| g.last_played.is_some())
            .collect();
        out.sort_by(|a, b| {
            b.last_played
                .cmp(&a.last_played)
                .then_with(|| a.display_name().cmp(b.display_name()))
        });
        out.truncate(5);
        out
    }

    pub(crate) fn recent_strip(
        &self,
        recent: &[&GameRow],
        view: Entity<Self>,
        cx: &App,
    ) -> impl IntoElement {
        let b = cx.theme();
        v_flex()
            .id("recent")
            .gap_1()
            .p_2()
            .rounded(px(4.))
            .bg(b.sidebar)
            .border_1()
            .border_color(b.border)
            .child(widgets::section_title(
                self.strings.get("gui-section-recent"),
                cx,
            ))
            .child(
                h_flex()
                    .id("recent-row")
                    .gap_1()
                    .flex_wrap()
                    .children(recent.iter().map(|g| {
                        let id = g.id.clone();
                        let view = view.clone();
                        widgets::btn(SharedString::from(format!("recent-{id}")), cx)
                            .secondary()
                            .child(widgets::blabel(g.display_name().to_string(), cx))
                            .on_click(move |_, _, cx| {
                                let id = id.clone();
                                view.update(cx, |this, cx| this.select_game(id, cx));
                            })
                    })),
            )
    }

    /// Dynamic 2:3 covers, max 200×300. Uses frozen metrics while resizing.
    pub(crate) fn card_grid(
        &self,
        games: &[&GameRow],
        view: Entity<Self>,
        cx: &App,
    ) -> impl IntoElement {
        let vp_w = self.page_scroll.bounds().size.width.as_f32() - PAGE_CONTENT_PAD_X;
        let (card_w, card_h, cols) = self.grid_metrics.unwrap_or_else(|| card_metrics(vp_w));
        let (first, last) = self.card_window(games.len(), card_h + CARD_GAP, cols);
        h_flex()
            .id("cards")
            .w_full()
            .flex_wrap()
            .gap(px(CARD_GAP))
            .items_start()
            .children(games.iter().enumerate().map(|(i, g)| {
                self.game_card(
                    g,
                    card_w,
                    card_h,
                    (first..last).contains(&i),
                    view.clone(),
                    cx,
                )
            }))
    }

    pub(crate) fn card_window(&self, n: usize, cell: f32, cols: usize) -> (usize, usize) {
        if n == 0 {
            return (0, 0);
        }
        let vh = self.page_scroll.bounds().size.height.as_f32();
        if vh <= 0. {
            return (0, n.min(24));
        }
        let off = -self.page_scroll.offset().y.as_f32();
        let row0 = ((off - vh * 0.5) / cell).floor().max(0.) as usize;
        let row1 = ((off + vh * 1.5) / cell).ceil().max(0.) as usize;
        ((row0 * cols).min(n), ((row1 + 1) * cols).min(n))
    }

    /// Library-grid only: apply ceil-pack metrics immediately on first valid
    /// width; while resizing keep the last applied box and settle ~120ms
    /// after the target stops changing.
    pub(crate) fn tick_grid_metrics(&mut self, cx: &mut Context<Self>) {
        if self.nav != Nav::Library || self.library_list {
            // Drop in-flight settle so a later notify does not fire off-grid.
            if self.grid_metrics_pending.is_some() {
                self.grid_metrics_pending = None;
                self.grid_metrics_gen = self.grid_metrics_gen.wrapping_add(1);
            }
            return;
        }
        let vp_w = self.page_scroll.bounds().size.width.as_f32() - PAGE_CONTENT_PAD_X;
        if vp_w <= 0. {
            return;
        }
        let desired = card_metrics(vp_w);
        match self.grid_metrics {
            None => {
                self.grid_metrics = Some(desired);
                self.grid_metrics_pending = None;
            }
            Some(applied) if applied == desired => {
                if self.grid_metrics_pending.is_some() {
                    self.grid_metrics_pending = None;
                    self.grid_metrics_gen = self.grid_metrics_gen.wrapping_add(1);
                }
            }
            Some(_) => {
                if self.grid_metrics_pending == Some(desired) {
                    return;
                }
                self.grid_metrics_pending = Some(desired);
                self.grid_metrics_gen = self.grid_metrics_gen.wrapping_add(1);
                let gen = self.grid_metrics_gen;
                cx.spawn(async move |this, cx| {
                    cx.background_executor()
                        .timer(std::time::Duration::from_millis(GRID_METRICS_SETTLE_MS))
                        .await;
                    let _ = this.update(cx, |this, cx| {
                        if this.grid_metrics_gen != gen {
                            return;
                        }
                        if let Some(m) = this.grid_metrics_pending.take() {
                            this.grid_metrics = Some(m);
                            cx.notify();
                        }
                    });
                })
                .detach();
            }
        }
    }
}
