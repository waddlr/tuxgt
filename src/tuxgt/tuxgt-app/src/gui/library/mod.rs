mod add;
mod card;
mod empty;
mod filter;
mod filter_kind;
mod grid;
mod list;
mod visible;

pub(crate) use filter_kind::*;
pub(crate) use visible::compute_base;

use std::collections::HashMap;

use gpui_kit::component::v_flex;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use tuxgt_core::{FluentArgs, GameRow, Strings};

use super::widgets;
use super::Shell;
use super::*;

/// Max cover box (Steam capsule 2:3). Live cards ceil-pack under the
/// page width (shrink to fill) but never exceed these.
pub(crate) const CARD_W_MAX: f32 = 200.;
pub(crate) const CARD_H_MAX: f32 = 300.;
pub(crate) const CARD_GAP: f32 = 12.;
/// Horizontal pad on `#page-content` (`.px(14)` both sides). Subtracted
/// from `page_scroll` bounds before `card_metrics`.
pub(crate) const PAGE_CONTENT_PAD_X: f32 = 28.;

/// Column count + cover size for the current page width. Ceil-packs so
/// slack is absorbed by shrinking; exact `N×max+(N−1)×gap` stays max.
/// Height is `w * 3/2` capped at `CARD_H_MAX`. Width floored to 4px so
/// settle targets stay stable across sub-pixel noise.
pub(crate) fn card_metrics(vp_w: f32) -> (f32, f32, usize) {
    let vp_w = vp_w.max(0.);
    let cols = ((vp_w + CARD_GAP) / (CARD_W_MAX + CARD_GAP)).ceil().max(1.) as usize;
    let raw_w = ((vp_w - CARD_GAP * (cols.saturating_sub(1) as f32)) / cols as f32)
        .min(CARD_W_MAX)
        .max(40.);
    let w = (raw_w / 4.).floor() * 4.;
    let h = (w * 1.5).min(CARD_H_MAX);
    (w, h, cols)
}

/// Library sort order. The id is stored in `Filters.sort`; "" means A–Z.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum SortMode {
    Az,
    Mods,
    ProtonDb,
    Recent,
}

impl SortMode {
    fn all() -> [SortMode; 4] {
        [
            SortMode::Az,
            SortMode::Mods,
            SortMode::ProtonDb,
            SortMode::Recent,
        ]
    }

    pub(crate) fn parse(s: &str) -> SortMode {
        match s {
            "mods" => SortMode::Mods,
            "protondb" => SortMode::ProtonDb,
            "recent" => SortMode::Recent,
            _ => SortMode::Az,
        }
    }

    pub(crate) fn id(self) -> &'static str {
        match self {
            SortMode::Az => "",
            SortMode::Mods => "mods",
            SortMode::ProtonDb => "protondb",
            SortMode::Recent => "recent",
        }
    }

    pub(crate) fn label_id(self) -> &'static str {
        match self {
            SortMode::Az => "gui-sort-az",
            SortMode::Mods => "gui-sort-mods",
            SortMode::ProtonDb => "gui-sort-protondb",
            SortMode::Recent => "gui-sort-recent",
        }
    }
}

/// AWACY anti-cheat flag parsed from the cache-only metadata row.
/// The full games.json payload lives in a single ("", "awacy") cache row;
/// `load_awacy` parses it once at startup/rescan, never on paint, never on network.
#[derive(Clone, Debug)]
pub(crate) struct AwacyFlag {
    pub status: String,
    pub providers: String,
}

impl AwacyFlag {
    /// Statuses where code injection is restricted (mock: EAC warning banner).
    pub(crate) fn blocking(&self) -> bool {
        matches!(
            self.status.to_ascii_lowercase().as_str(),
            "denied" | "broken" | "borked"
        )
    }

    pub(crate) fn chip_label(&self, strings: &Strings) -> String {
        if self.providers.is_empty() {
            let mut args = FluentArgs::new();
            args.set("status", self.status.as_str());
            strings.get_args("gui-chip-awacy", Some(&args))
        } else {
            self.providers.clone()
        }
    }
}

impl Shell {
    pub(crate) fn library_chrome(&self, view: Entity<Self>, cx: &App) -> impl IntoElement {
        if self.index.is_empty() {
            div().into_any_element()
        } else {
            self.filter_bar(view, cx).into_any_element()
        }
    }

    pub(crate) fn library_scroll(&self, view: Entity<Self>, cx: &App) -> impl IntoElement {
        if self.index.is_empty() {
            return self.empty_library(view, cx).into_any_element();
        }
        let visible = self.visible_games();
        let recent = self.recently_played();
        v_flex()
            .id("library")
            .w_full()
            .flex_shrink_0()
            .gap_3()
            .pt_2()
            .when(!recent.is_empty(), |this| {
                this.child(
                    self.recent_strip(&recent, view.clone(), cx)
                        .into_any_element(),
                )
            })
            .when(self.manual_remove.is_some(), |this| {
                this.child(self.manual_remove_card(view.clone(), cx).into_any_element())
            })
            .child(if visible.is_empty() {
                widgets::muted(self.strings.get("gui-empty-no-match"), cx).into_any_element()
            } else if self.library_list {
                self.card_list(&visible, view.clone(), cx)
                    .into_any_element()
            } else {
                self.card_grid(&visible, view.clone(), cx)
                    .into_any_element()
            })
            .child(self.library_footer(cx))
            .into_any_element()
    }
}

/// Library sort over row positions. Pure: positions index `games`, which
/// must align with the minimal index (same order, same length).
pub(crate) fn sort_indices(
    sort: &str,
    tiers: &HashMap<String, String>,
    mod_counts: &HashMap<String, usize>,
    games: &[GameRow],
    indices: &mut Vec<usize>,
) {
    let mut keyed: Vec<(usize, (String, String), usize, u8, Option<i64>)> = indices
        .iter()
        .map(|&i| {
            let g = &games[i];
            (
                i,
                (g.display_name().to_ascii_lowercase(), g.id.clone()),
                mod_counts.get(&g.id).copied().unwrap_or(0),
                tier_rank(tiers.get(&g.id)),
                g.last_played,
            )
        })
        .collect();
    match SortMode::parse(sort) {
        SortMode::Az => {
            keyed.sort_by(|a, b| a.1.cmp(&b.1));
        }
        SortMode::Mods => {
            keyed.sort_by(|a, b| b.2.cmp(&a.2).then_with(|| a.1.cmp(&b.1)));
        }
        SortMode::ProtonDb => {
            keyed.sort_by(|a, b| a.3.cmp(&b.3).then_with(|| a.1.cmp(&b.1)));
        }
        SortMode::Recent => {
            keyed.sort_by(|a, b| match (b.4, a.4) {
                (Some(x), Some(y)) => x.cmp(&y).then_with(|| a.1.cmp(&b.1)),
                (Some(_), None) => std::cmp::Ordering::Greater,
                (None, Some(_)) => std::cmp::Ordering::Less,
                (None, None) => a.1.cmp(&b.1),
            });
        }
    }
    indices.clear();
    indices.extend(keyed.into_iter().map(|(i, _, _, _, _)| i));
}

#[cfg(test)]
mod tests {
    use super::{card_metrics, CARD_GAP, CARD_H_MAX, CARD_W_MAX};

    #[test]
    fn card_metrics_ceil_packs_wide_viewport() {
        let (w, h, cols) = card_metrics(1600.);
        assert_eq!(cols, 8);
        assert!(w <= CARD_W_MAX);
        assert!((h - w * 1.5).abs() < 0.01);
        assert!(h <= CARD_H_MAX);
        let used = cols as f32 * w + (cols - 1) as f32 * CARD_GAP;
        assert!(used <= 1600.);
        assert!(1600. - used < CARD_W_MAX + CARD_GAP);
    }

    #[test]
    fn card_metrics_exact_pack_stays_max() {
        // 4×200 + 3×12 = 836
        let (w, h, cols) = card_metrics(836.);
        assert_eq!(cols, 4);
        assert!((w - CARD_W_MAX).abs() < 0.01);
        assert!((h - CARD_H_MAX).abs() < 0.01);
    }

    #[test]
    fn card_metrics_just_below_next_max_adds_column() {
        // floor would keep 4×200; ceil packs 5 and shrinks.
        let (w, h, cols) = card_metrics(1047.);
        assert_eq!(cols, 5);
        assert!(w < CARD_W_MAX);
        assert!(w >= 196.);
        assert!((h - w * 1.5).abs() < 0.01);
    }

    #[test]
    fn card_metrics_shrinks_under_narrow_viewport() {
        let (w, h, cols) = card_metrics(150.);
        assert_eq!(cols, 1);
        assert!(w < CARD_W_MAX);
        assert!((h - w * 1.5).abs() < 0.01);
    }
}
