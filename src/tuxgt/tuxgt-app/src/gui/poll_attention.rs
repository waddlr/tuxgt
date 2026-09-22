//! Per-game Attention after Update (`update-attention-stuck`).
//!
//! The 2h poll is not the only writer. A successful per-game update
//! recounts that game from manifest provenance vs the catalog cache and
//! drops `game:<id>` when nothing there is still stale. Other games and
//! catalog cards stay.

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::time::Instant;

use gpui_kit::Context;
use tuxgt_core::FluentArgs;

use super::notice::NoticeKind;
use super::Shell;

/// One game's recount, stamped when it happened.
/// `stale: None` means the recount failed.
#[derive(Clone, Copy)]
pub(crate) struct GameSettle {
    pub stale: Option<usize>,
    pub at: Instant,
}

/// Manifests whose provenance sha differs from the catalog cache sha.
/// Empty provenance, or an empty or missing cache sha, is not stale —
/// the same rule as the catalog poll.
pub(crate) fn stale_instance_count<'a>(
    manifests: impl IntoIterator<Item = (&'a str, &'a str)>,
    sha_of: &HashMap<&str, &str>,
) -> usize {
    manifests
        .into_iter()
        .filter(|(instance, prov)| {
            !prov.is_empty()
                && sha_of
                    .get(instance)
                    .is_some_and(|sha| !sha.is_empty() && *sha != *prov)
        })
        .count()
}

/// Keys to keep after one game's stale count is known. `stale == 0`
/// drops only `game:<id>`. A remaining count keeps that key so the
/// caller can refresh its text.
pub(crate) fn attention_keep_after_update<'a>(
    keys: impl IntoIterator<Item = &'a str>,
    game: &str,
    stale: usize,
) -> HashSet<String> {
    let game_key = format!("game:{game}");
    keys.into_iter()
        .filter(|k| stale > 0 || *k != game_key)
        .map(str::to_string)
        .collect()
}

/// Local stale count for one game after an update wrote its manifest.
/// The shared pool reconciles on the dirty mark `write_manifest` left,
/// so the cache sha is the payload provenance the install just recorded.
/// `None` means the recount itself failed; the caller re-polls.
pub(crate) async fn recount_game_stale(data: &Path, game: &str) -> Option<usize> {
    let pool = tuxgt_core::open_db_shared(data).await.ok()?;
    let rows = tuxgt_core::mod_cache_rows(&pool).await.ok()?;
    let sha_of: HashMap<&str, &str> = rows
        .iter()
        .map(|r| (r.id.as_str(), r.asset_sha256.as_str()))
        .collect();
    let manifests = tuxgt_core::game_manifests(data, game).ok()?;
    Some(stale_instance_count(
        manifests
            .iter()
            .map(|m| (m.instance.as_str(), m.provenance.asset_sha256.as_str())),
        &sha_of,
    ))
}

/// Per-game rows a poll may publish.
///
/// A settle stamped at or after `poll_started` wins for that game: 0 drops
/// it, a positive count replaces the poll's number, and a failed recount
/// omits it and asks for another poll. This report must not publish a
/// game whose recount failed. Settles from before this poll started are
/// ignored — that poll read the world after them.
pub(crate) fn per_game_respecting_settle(
    reported: &[(String, usize)],
    settled: &HashMap<String, GameSettle>,
    poll_started: Instant,
) -> (Vec<(String, usize)>, bool) {
    let mut again = false;
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for (game, n) in reported {
        seen.insert(game.as_str());
        let Some(s) = settled.get(game).filter(|s| s.at >= poll_started) else {
            out.push((game.clone(), *n));
            continue;
        };
        match s.stale {
            Some(0) => {}
            Some(count) => out.push((game.clone(), count)),
            None => again = true,
        }
    }
    for (game, s) in settled {
        if s.at < poll_started || seen.contains(game.as_str()) {
            continue;
        }
        match s.stale {
            Some(0) => {}
            Some(count) => out.push((game.clone(), count)),
            None => again = true,
        }
    }
    (out, again)
}

/// A recount failed at or after `poll_started`, so this poll's completion
/// must schedule another one. An older stamp does not: the retry would
/// never stop.
pub(crate) fn recount_failed_since(
    settled: &HashMap<String, GameSettle>,
    poll_started: Instant,
) -> bool {
    settled
        .values()
        .any(|s| s.stale.is_none() && s.at >= poll_started)
}

impl Shell {
    /// Drop or refresh `game:<id>` from a just-finished per-game update.
    /// `stale == 0` removes that card only. A remaining count rewrites
    /// the text. The stamp stops a poll that started earlier from putting
    /// the card back. Catalog cards and other games stay.
    pub(crate) fn settle_game_attention(
        &mut self,
        game: &str,
        stale: usize,
        cx: &mut Context<Self>,
    ) {
        self.game_attention_settled.insert(
            game.to_string(),
            GameSettle {
                stale: Some(stale),
                at: Instant::now(),
            },
        );
        let keep = attention_keep_after_update(
            self.notices.attention.iter().map(|n| n.key.as_str()),
            game,
            stale,
        );
        self.notices.retain_attention(&keep);
        if stale == 0 {
            cx.notify();
            return;
        }
        let display = self.game_display(game);
        let mut args = FluentArgs::new();
        args.set("display", display);
        args.set("count", stale.to_string());
        let text = self.strings.get_args("gui-notice-update-game", Some(&args));
        self.emit_attention(&format!("game:{game}"), NoticeKind::Warn, text, cx);
    }

    /// The recount failed. An in-flight poll must not be the last word.
    pub(crate) fn note_game_attention_unsettled(&mut self, game: &str) {
        self.game_attention_settled.insert(
            game.to_string(),
            GameSettle {
                stale: None,
                at: Instant::now(),
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stale_count_matches_the_poll_rule() {
        let sha_of: HashMap<&str, &str> = [
            ("reshade", "bbb"),
            ("opti", "same"),
            ("empty-cache", ""),
            ("no-prov", "zzz"),
        ]
        .into_iter()
        .collect();
        let manifests = [
            ("reshade", "aaa"),
            ("opti", "same"),
            ("empty-cache", "qqq"),
            ("no-prov", ""),
            ("missing", "mmm"),
        ];
        assert_eq!(stale_instance_count(manifests, &sha_of), 1);
    }

    #[test]
    fn update_drops_only_that_games_card_when_nothing_is_stale() {
        let keys = ["catalog:reshade", "game:quake", "game:doom"];
        let keep = attention_keep_after_update(keys, "quake", 0);
        assert!(!keep.contains("game:quake"));
        assert!(keep.contains("game:doom"));
        assert!(keep.contains("catalog:reshade"));
        let still = attention_keep_after_update(keys, "quake", 2);
        assert!(still.contains("game:quake"));
        assert!(still.contains("game:doom"));
        assert!(still.contains("catalog:reshade"));
    }

    fn settle(stale: Option<usize>, at: Instant) -> GameSettle {
        GameSettle { stale, at }
    }

    #[test]
    fn poll_started_before_a_clear_does_not_put_the_card_back() {
        let started = Instant::now();
        let later = started + std::time::Duration::from_secs(1);
        let mut settled = HashMap::new();
        settled.insert("quake".into(), settle(Some(0), later));
        let (rows, again) = per_game_respecting_settle(
            &[("quake".into(), 1), ("doom".into(), 2)],
            &settled,
            started,
        );
        assert!(!again);
        assert_eq!(rows, vec![("doom".into(), 2)]);
    }

    #[test]
    fn a_later_settle_replaces_the_poll_count() {
        let started = Instant::now();
        let later = started + std::time::Duration::from_secs(1);
        let mut settled = HashMap::new();
        settled.insert("quake".into(), settle(Some(2), later));
        let (rows, again) = per_game_respecting_settle(&[("quake".into(), 5)], &settled, started);
        assert!(!again);
        assert_eq!(rows, vec![("quake".into(), 2)]);
    }

    #[test]
    fn a_settle_from_before_the_poll_leaves_the_report() {
        let started = Instant::now();
        let earlier = started
            .checked_sub(std::time::Duration::from_secs(1))
            .unwrap();
        let mut settled = HashMap::new();
        settled.insert("quake".into(), settle(Some(0), earlier));
        let (rows, again) = per_game_respecting_settle(&[("quake".into(), 1)], &settled, started);
        assert!(!again);
        assert_eq!(rows, vec![("quake".into(), 1)]);
    }

    #[test]
    fn a_failed_recount_omits_the_row_and_asks_for_another_poll() {
        let started = Instant::now();
        let later = started + std::time::Duration::from_secs(1);
        let mut settled = HashMap::new();
        settled.insert("quake".into(), settle(None, later));
        let (rows, again) = per_game_respecting_settle(
            &[("quake".into(), 1), ("doom".into(), 4)],
            &settled,
            started,
        );
        assert!(again);
        assert_eq!(rows, vec![("doom".into(), 4)]);
        assert!(recount_failed_since(&settled, started));
        let earlier = started
            .checked_sub(std::time::Duration::from_secs(1))
            .unwrap();
        settled.insert("quake".into(), settle(None, earlier));
        assert!(!recount_failed_since(&settled, started));
    }

    #[test]
    fn a_positive_settle_the_poll_missed_is_added() {
        let started = Instant::now();
        let later = started + std::time::Duration::from_secs(1);
        let mut settled = HashMap::new();
        settled.insert("quake".into(), settle(Some(3), later));
        let (rows, again) = per_game_respecting_settle(&[], &settled, started);
        assert!(!again);
        assert_eq!(rows, vec![("quake".into(), 3)]);
    }
}
