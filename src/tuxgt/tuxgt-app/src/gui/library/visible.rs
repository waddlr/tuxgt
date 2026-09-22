use std::collections::{HashMap, HashSet};

use super::*;
use tuxgt_core::{GameIndexRow, GameRow};

use super::super::Shell;

impl Shell {
    /// One filter+sort pass returning sorted positions and rows together.
    /// Library only: full rows are present only there. Positions index
    /// `index`, which aligns with `games` on the Library page; out-of-range
    /// positions are dropped (a failed full reload must empty the page,
    /// never panic it).
    pub fn visible(&self) -> (Vec<usize>, Vec<&GameRow>) {
        let indices: Vec<usize> = self
            .visible_indices()
            .into_iter()
            .filter(|&i| i < self.games.len())
            .collect();
        let rows = indices.iter().map(|&i| &self.games[i]).collect();
        (indices, rows)
    }

    pub fn visible_games(&self) -> Vec<&GameRow> {
        self.visible().1
    }

    /// Paint-time narrow over the stored base list. Runs on every page and
    /// needs only the minimal index: the base list already carries the
    /// Library filters and the sort, and text/hidden/disabled narrow it.
    pub fn visible_indices(&self) -> Vec<usize> {
        let query = self.filters.search.to_ascii_lowercase();
        self.base_filtered
            .iter()
            .copied()
            .filter(|&i| {
                self.index.get(i).is_some_and(|e| {
                    narrow_visible(&self.filters, &self.disabled_managers, &query, e)
                })
            })
            .collect()
    }

    /// Library-only recompute of the stored base list: every filter except
    /// text, hidden, and disabled managers, plus the active sort. Called on
    /// Library entry and whenever a base filter changes there; a no-op
    /// anywhere else (full rows are not held off-Library).
    pub(crate) fn recompute_base(&mut self) {
        if self.nav != Nav::Library {
            return;
        }
        self.base_filtered = compute_base(
            &self.filters,
            &self.tiers,
            &self.awacy,
            &self.mod_counts,
            &self.games,
        );
    }
}

/// Base filter+sort over full rows. Pure so scans can rebuild the stored
/// list from transient rows on any page.
pub(crate) fn compute_base(
    filters: &Filters,
    tiers: &HashMap<String, String>,
    awacy: &HashMap<String, AwacyFlag>,
    mod_counts: &HashMap<String, usize>,
    games: &[GameRow],
) -> Vec<usize> {
    let mut out: Vec<usize> = games
        .iter()
        .enumerate()
        .filter(|(_, g)| base_visible(filters, tiers, awacy, mod_counts, g))
        .map(|(i, _)| i)
        .collect();
    sort_indices(&filters.sort, tiers, mod_counts, games, &mut out);
    out
}

/// Base predicate: the Library filters that need full rows or Library-scoped
/// metadata (store, platform `{family}[:{api}]`, ProtonDB tier, AWACY,
/// mods-only). Pure so tests can prove base+narrow equals the old single
/// predicate.
pub(crate) fn base_visible(
    filters: &Filters,
    tiers: &HashMap<String, String>,
    awacy: &HashMap<String, AwacyFlag>,
    mod_counts: &HashMap<String, usize>,
    g: &GameRow,
) -> bool {
    if let Some(key) = filters.store.as_deref() {
        if let Some((m, s)) = parse_store_key(key) {
            if g.manager != m || g.store != s {
                return false;
            }
        }
    }
    if !platform_matches(
        filters.platform.as_deref(),
        g.platform.as_deref(),
        g.api.as_deref(),
    ) {
        return false;
    }
    if let Some(t) = filters.protondb.as_deref() {
        if tiers.get(&g.id).map(String::as_str) != Some(t) {
            return false;
        }
    }
    if filters.awacy_only && !awacy.contains_key(&g.id) {
        return false;
    }
    if filters.mods_only {
        let n = mod_counts.get(&g.id).copied().unwrap_or(0);
        if n == 0 {
            return false;
        }
    }
    true
}

/// Narrow predicate: disabled managers, hidden, and text over the minimal
/// index. Runs at paint on every page. `query` is the pre-lowered search text.
pub(crate) fn narrow_visible(
    filters: &Filters,
    disabled: &HashSet<String>,
    query: &str,
    e: &GameIndexRow,
) -> bool {
    if disabled.contains(&e.manager) {
        return false;
    }
    if e.hidden && !filters.show_hidden {
        return false;
    }
    if !query.is_empty() {
        let name = e.display_name().to_ascii_lowercase();
        if !name.contains(query) && !e.id.to_ascii_lowercase().contains(query) {
            return false;
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::{base_visible, compute_base, narrow_visible};
    use super::{parse_store_key, platform_matches};
    use super::{AwacyFlag, Filters};
    use std::collections::{HashMap, HashSet};
    use tuxgt_core::{GameIndexRow, GameRow};

    fn row(
        id: &str,
        manager: &str,
        store: &str,
        name: Option<&str>,
        platform: Option<&str>,
        api: Option<&str>,
        hidden: bool,
        last_played: Option<i64>,
    ) -> GameRow {
        GameRow {
            id: id.into(),
            name: name.map(str::to_string),
            cover_path: None,
            manager: manager.into(),
            store: store.into(),
            header_path: None,
            platform: platform.map(str::to_string),
            api: api.map(str::to_string),
            install_dir: None,
            exe_path: None,
            prefix_path: None,
            proton: None,
            bitness: None,
            engine: None,
            hidden,
            last_played,
            steam_appid: None,
        }
    }

    fn fixtures() -> Vec<GameRow> {
        vec![
            row(
                "steam::1",
                "steam",
                "",
                Some("Doom Eternal"),
                Some("proton"),
                Some("vulkan"),
                false,
                Some(300),
            ),
            row(
                "heroic:gog:2",
                "heroic",
                "gog",
                Some("Cyberpunk 2077"),
                Some("wine"),
                Some("dx12"),
                true,
                Some(200),
            ),
            row(
                "manual:standalone:3",
                "manual",
                "standalone",
                Some("Doom 2016"),
                Some("native"),
                Some("opengl"),
                false,
                None,
            ),
            row(
                "steam::4",
                "steam",
                "",
                None,
                Some("proton"),
                Some("dx11"),
                false,
                Some(100),
            ),
        ]
    }

    fn maps() -> (
        HashMap<String, String>,
        HashMap<String, AwacyFlag>,
        HashMap<String, usize>,
    ) {
        (
            HashMap::from([
                ("steam::1".into(), "gold".into()),
                ("heroic:gog:2".into(), "silver".into()),
            ]),
            HashMap::from([(
                "heroic:gog:2".into(),
                AwacyFlag {
                    status: "Denied".into(),
                    providers: String::new(),
                },
            )]),
            HashMap::from([("steam::1".into(), 2), ("steam::4".into(), 1)]),
        )
    }

    /// The unsplit reference predicate. Base+narrow must agree with it on
    /// every filter combination, or the split itself drops or leaks games.
    fn combined_visible(
        filters: &Filters,
        disabled: &HashSet<String>,
        tiers: &HashMap<String, String>,
        awacy: &HashMap<String, AwacyFlag>,
        mod_counts: &HashMap<String, usize>,
        g: &GameRow,
    ) -> bool {
        if disabled.contains(&g.manager) {
            return false;
        }
        if g.hidden && !filters.show_hidden {
            return false;
        }
        if let Some(key) = filters.store.as_deref() {
            if let Some((m, s)) = parse_store_key(key) {
                if g.manager != m || g.store != s {
                    return false;
                }
            }
        }
        if !platform_matches(
            filters.platform.as_deref(),
            g.platform.as_deref(),
            g.api.as_deref(),
        ) {
            return false;
        }
        if let Some(t) = filters.protondb.as_deref() {
            if tiers.get(&g.id).map(String::as_str) != Some(t) {
                return false;
            }
        }
        if filters.awacy_only && !awacy.contains_key(&g.id) {
            return false;
        }
        if filters.mods_only && mod_counts.get(&g.id).copied().unwrap_or(0) == 0 {
            return false;
        }
        if !filters.search.is_empty() {
            let q = filters.search.to_ascii_lowercase();
            let name = g.display_name().to_ascii_lowercase();
            if !name.contains(&q) && !g.id.to_ascii_lowercase().contains(&q) {
                return false;
            }
        }
        true
    }

    fn filter_sets() -> Vec<Filters> {
        let mut out = vec![Filters::default()];
        let mut store = Filters::default();
        store.store = Some("steam:".into());
        out.push(store);
        let mut native = Filters::default();
        native.platform = Some("native".into());
        out.push(native);
        let mut windows = Filters::default();
        windows.platform = Some("windows".into());
        out.push(windows);
        let mut win_api = Filters::default();
        win_api.platform = Some("windows:dx12".into());
        out.push(win_api);
        let mut tier = Filters::default();
        tier.protondb = Some("gold".into());
        out.push(tier);
        let mut awacy = Filters::default();
        awacy.awacy_only = true;
        out.push(awacy);
        let mut mods = Filters::default();
        mods.mods_only = true;
        out.push(mods);
        let mut search = Filters::default();
        search.search = "doom".into();
        out.push(search);
        let mut hidden = Filters::default();
        hidden.show_hidden = true;
        out.push(hidden);
        let mut combo = Filters::default();
        combo.platform = Some("windows".into());
        combo.mods_only = true;
        combo.search = "steam".into();
        combo.show_hidden = true;
        out.push(combo);
        out
    }

    #[test]
    fn base_plus_narrow_matches_combined() {
        let games = fixtures();
        let (tiers, awacy, mod_counts) = maps();
        for disabled in [HashSet::new(), HashSet::from(["manual".to_string()])] {
            for (fi, filters) in filter_sets().into_iter().enumerate() {
                let query = filters.search.to_ascii_lowercase();
                for g in &games {
                    let e = GameIndexRow::from_row(g);
                    let split = base_visible(&filters, &tiers, &awacy, &mod_counts, g)
                        && narrow_visible(&filters, &disabled, &query, &e);
                    assert_eq!(
                        split,
                        combined_visible(&filters, &disabled, &tiers, &awacy, &mod_counts, g),
                        "filter set {fi} game {}",
                        g.id,
                    );
                }
            }
        }
    }

    #[test]
    fn compute_base_filters_and_sorts() {
        let games = fixtures();
        let (tiers, awacy, mod_counts) = maps();
        // Mods sort: counts desc, name tiebreak.
        let mut mods = Filters::default();
        mods.sort = "mods".into();
        assert_eq!(
            compute_base(&mods, &tiers, &awacy, &mod_counts, &games),
            vec![0, 3, 1, 2]
        );
        // Tier sort: gold < silver < unranked, name tiebreak (the
        // nameless row sorts by its id).
        let mut pdb = Filters::default();
        pdb.sort = "protondb".into();
        assert_eq!(
            compute_base(&pdb, &tiers, &awacy, &mod_counts, &games),
            vec![0, 1, 2, 3]
        );
        // Base filters apply before the sort: hidden is a narrow-time flag,
        // so the base keeps the hidden wine row here.
        let mut only = Filters::default();
        only.platform = Some("windows".into());
        only.sort = "recent".into();
        assert_eq!(
            compute_base(&only, &tiers, &awacy, &mod_counts, &games),
            vec![0, 1, 3]
        );
    }
}
