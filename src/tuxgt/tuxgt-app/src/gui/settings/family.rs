use gpui_kit::*;

use super::*;
use tuxgt_core::{
    config_dir, data_dir, family_template, list_family_assets_many, mint_recipe, snap_family_asset,
    FluentArgs, RecipeSpec,
};

use super::super::{rt_block, FamilyMint, Shell};

impl Shell {
    pub(crate) fn open_family_mint(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let templates: Vec<(String, String)> = self
            .family_templates
            .iter()
            .map(|t| (t.id.clone(), t.label.clone()))
            .collect();
        self.family_mint = Some(FamilyMint {
            loading: true,
            assets: Box::default(),
            err: None,
            selected: Vec::new(),
            game_for: std::collections::HashMap::new(),
        });
        self.family_filter_input.update(cx, |inp, cx| {
            inp.set_value(String::new(), window, cx);
        });
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    rt_block(async move {
                        // One GitHub release fetch per family, joined once:
                        // RenoDX no longer waits on Luma (or vice versa).
                        let ids: Vec<String> =
                            templates.iter().map(|(tid, _)| tid.clone()).collect();
                        let by_id: std::collections::HashMap<String, String> =
                            templates.into_iter().collect();
                        let mut rows: Vec<super::FamilyAssetRow> = Vec::new();
                        let mut errs: Vec<String> = Vec::new();
                        for (tid, out) in list_family_assets_many(&data_dir(), &ids).await {
                            let vendor = by_id.get(&tid).cloned().unwrap_or_else(|| tid.clone());
                            match out {
                                Ok(assets) => {
                                    for a in assets {
                                        rows.push(super::FamilyAssetRow {
                                            template_id: tid.clone(),
                                            vendor: vendor.clone(),
                                            name: a.name,
                                            tag: a.tag,
                                        });
                                    }
                                }
                                Err(e) => errs.push(format!("{vendor}: {e}")),
                            }
                        }
                        rows.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
                        Ok::<_, tuxgt_core::Error>((rows, errs))
                    })
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                // Snapshot library games for auto-match; pre-check only
                // exact token-set matches.
                let games: Vec<(String, String)> = this
                    .index
                    .iter()
                    .map(|g| (g.id.clone(), g.display_name().to_string()))
                    .collect();
                if let Some(m) = this.family_mint.as_mut() {
                    match result {
                        Ok((rows, errs)) => {
                            m.assets = rows.into_boxed_slice();
                            // One failed vendor still lists the other, but the
                            // card must say so — a quiet half-list reads complete.
                            m.err = if errs.is_empty() {
                                None
                            } else {
                                Some(errs.join("; "))
                            };
                            // Auto-match every row once at open.
                            for row in &m.assets {
                                let key = row.key();
                                if let Some(gid) = match_family_game(&row.name, &row.vendor, &games)
                                {
                                    m.game_for.insert(key.clone(), gid);
                                    m.selected.push(key);
                                }
                            }
                        }
                        Err(e) => m.err = Some(e.to_string()),
                    }
                    m.loading = false;
                }
                cx.notify();
            });
        })
        .detach();
        self.scroll_page_top();
        cx.notify();
    }
    /// HDR packs: checkbox toggles its key in the mint set.
    pub(crate) fn select_family_asset(&mut self, key: String, cx: &mut Context<Self>) {
        tracing::debug!(action = "select-family-asset", key = key.as_str());
        if let Some(m) = self.family_mint.as_mut() {
            if let Some(pos) = m.selected.iter().position(|s| s == &key) {
                m.selected.remove(pos);
            } else {
                m.selected.push(key);
            }
        }
        cx.notify();
    }
    /// HDR packs: Select Visible — the filtered rows only, off-filter picks
    /// kept in either direction.
    pub(crate) fn toggle_family_select_visible(
        &mut self,
        keys: Vec<String>,
        on: bool,
        cx: &mut Context<Self>,
    ) {
        tracing::debug!(action = "toggle-family-select-visible", count = keys.len(), on);
        if let Some(m) = self.family_mint.as_mut() {
            if on {
                for k in keys {
                    if !m.selected.iter().any(|s| s == &k) {
                        m.selected.push(k);
                    }
                }
            } else {
                m.selected.retain(|s| !keys.iter().any(|k| k == s));
            }
        }
        cx.notify();
    }
    /// HDR packs: per-row game dropdown correction. Picking a game also
    /// checks the row, so Add never silently skips a row the user just bound.
    pub(crate) fn set_family_game(&mut self, key: String, game_id: String, cx: &mut Context<Self>) {
        tracing::debug!(action = "set-family-game", key = key.as_str(), game = game_id.as_str());
        if let Some(m) = self.family_mint.as_mut() {
            m.game_for.insert(key.clone(), game_id);
            if !m.selected.iter().any(|s| s == &key) {
                m.selected.push(key);
            }
        }
        cx.notify();
    }

    /// HDR packs: Add mints every checked row with a game; no-game rows are
    /// skipped and named in the toast. Label is always auto. Stop on first
    /// error, retain selection for retry. Toast reuses the instance-added
    /// and err-add strings.
    pub(crate) fn mint_family_mod(&mut self, cx: &mut Context<Self>) {
        let Some(m) = self.family_mint.clone() else {
            return;
        };
        tracing::debug!(action = "mint-family", count = m.selected.len());
        if m.selected.is_empty() {
            self.status = self.strings.get("gui-family-pick-asset");
            cx.notify();
            return;
        }
        // Snapshot display names + resolved appids for the checked keys.
        let mut items: Vec<(String, String, String, String, Option<u32>)> = Vec::new();
        let mut skipped: Vec<String> = Vec::new();
        for key in m.selected.iter() {
            let Some(row) = m.assets.iter().find(|r| r.key() == *key) else {
                continue;
            };
            let Some(gid) = m.game_for.get(key).cloned() else {
                skipped.push(row.name.clone());
                continue;
            };
            let Some(g) = super::load_game_row(&gid) else {
                skipped.push(row.name.clone());
                continue;
            };
            let display = g.display_name().to_string();
            let appid = g
                .resolved_appid()
                .and_then(|s| s.parse::<u32>().ok())
                .filter(|a| *a != 0);
            let auto = suggest_family_title(&row.name, &row.vendor);
            let label = format!("{}: {}", row.vendor, auto);
            items.push((
                row.template_id.clone(),
                row.name.clone(),
                display,
                label,
                appid,
            ));
        }
        if items.is_empty() {
            let mut args = FluentArgs::new();
            args.set("id", skipped.join(", "));
            self.status = self
                .strings
                .get_args("gui-status-family-skipped", Some(&args));
            cx.notify();
            return;
        }
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    rt_block(async move {
                        let mut minted: Vec<String> = Vec::new();
                        let mut minted_keys: Vec<String> = Vec::new();
                        let mut first_err: Option<String> = None;
                        let data = data_dir();
                        let cfg = config_dir();
                        for (tpl, asset, display, label, appid) in items {
                            if first_err.is_some() {
                                break;
                            }
                            let one = async {
                                let (t, fam) = family_template(&data, &tpl)?;
                                let snapped = snap_family_asset(&data, &tpl, &asset).await;
                                let spec = RecipeSpec::family(
                                    &t, &fam, &snapped, &display, &label, appid,
                                )?;
                                mint_recipe(&cfg, &data, spec)
                            }
                            .await;
                            match one {
                                Ok(inst) => {
                                    minted.push(inst.id);
                                    minted_keys.push(format!("{tpl}:{asset}"));
                                }
                                Err(e) => {
                                    if first_err.is_none() {
                                        first_err = Some(e.to_string());
                                    }
                                }
                            }
                        }
                        Ok::<_, tuxgt_core::Error>((minted, minted_keys, first_err, skipped))
                    })
                })
                .await;
            let _ = this.update(cx, |this, cx| match result {
                Ok((minted, minted_keys, first_err, skipped)) => {
                    if !minted.is_empty() {
                        if this.instances_showing() {
                            this.instances = super::load_instances().into_boxed_slice();
                        }
                        this.refresh_selected_mods();
                    }
                    match first_err {
                        None => {
                            this.family_mint = None;
                            let ids = if minted.len() == 1 {
                                minted.into_iter().next().unwrap_or_default()
                            } else {
                                minted.join(", ")
                            };
                            let mut args = FluentArgs::new();
                            args.set("id", ids);
                            if skipped.is_empty() {
                                this.status = this
                                    .strings
                                    .get_args("gui-status-instance-added", Some(&args));
                            } else {
                                args.set("skipped", skipped.join(", "));
                                this.status = this
                                    .strings
                                    .get_args("gui-status-instance-added-skipped", Some(&args));
                            }
                        }
                        Some(error) => {
                            if let Some(fm) = this.family_mint.as_mut() {
                                fm.selected.retain(|s| !minted_keys.contains(s));
                            }
                            let mut args = FluentArgs::new();
                            args.set("error", error);
                            this.status = this.strings.get_args("gui-status-err-add", Some(&args));
                        }
                    }
                    cx.notify();
                }
                Err(e) => {
                    let mut args = FluentArgs::new();
                    args.set("error", e.to_string());
                    this.status = this.strings.get_args("gui-status-err-add", Some(&args));
                    cx.notify();
                }
            });
        })
        .detach();
    }
}
