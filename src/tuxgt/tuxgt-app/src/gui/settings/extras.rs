use gpui_kit::*;

use super::*;
use tuxgt_core::{
    config_dir, data_dir, list_reshade_packages, migrate_reshade_legacy, mint_recipe, FluentArgs,
    MintTarget, RecipeSpec,
};

use super::super::{rt_block, ReshadePackagesMint, Shell};

impl Shell {
    pub(crate) fn open_reshade_extras(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.extras_mint = Some(ReshadePackagesMint {
            loading: true,
            packages: Box::default(),
            err: None,
            selected: Vec::new(),
            kind_filter: super::ExtrasKindFilter::All,
            target_game: None,
            target_arch: None,
        });
        self.extras_filter_input.update(cx, |inp, cx| {
            inp.set_value(String::new(), window, cx);
        });
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    rt_block(async move { list_reshade_packages(&config_dir(), &data_dir()).await })
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                if let Some(m) = this.extras_mint.as_mut() {
                    match result {
                        Ok(packages) => m.packages = packages.into_boxed_slice(),
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

    /// R38: required target game for the extras mint. Resolves the full
    /// row once at pick time; the arch gates rows and the mint.
    pub(crate) fn set_extras_target(&mut self, game_id: String, cx: &mut Context<Self>) {
        tracing::debug!(action = "set-extras-target", game = game_id.as_str());
        if let Some(m) = self.extras_mint.as_mut() {
            let arch = super::load_game_row(&game_id)
                .and_then(|g| g.bitness)
                .filter(|b| b == "32" || b == "64");
            m.target_game = Some(game_id);
            m.target_arch = arch.clone();
            // Drop checked rows the new target cannot mint instead of
            // silently filtering them at Add time.
            match arch {
                Some(arch) => m.selected.retain(|s| {
                    m.packages
                        .iter()
                        .any(|p| p.key() == *s && p.mintable_for_arch(&arch))
                }),
                None => m.selected.clear(),
            }
        }
        cx.notify();
    }

    pub(crate) fn toggle_reshade_extra(&mut self, key: String, cx: &mut Context<Self>) {
        tracing::debug!(action = "toggle-reshade-extra", key = key.as_str());
        let Some(m) = self.extras_mint.as_mut() else {
            return;
        };
        let Some(arch) = m.target_arch.clone() else {
            return;
        };
        let Some(pkg) = m.packages.iter().find(|p| p.key() == key) else {
            return;
        };
        if !pkg.mintable_for_arch(&arch) {
            return;
        }
        if let Some(pos) = m.selected.iter().position(|s| s == &key) {
            m.selected.remove(pos);
        } else {
            m.selected.push(key);
        }
        cx.notify();
    }

    pub(crate) fn set_extras_filter(
        &mut self,
        filter: super::ExtrasKindFilter,
        cx: &mut Context<Self>,
    ) {
        if let Some(m) = self.extras_mint.as_mut() {
            m.kind_filter = filter;
        }
        cx.notify();
    }

    /// E96: visible + selectable extras keys under the current kind filter
    /// and text needle (installed and URL-less rows excluded). R38: no
    /// target arch → nothing is selectable.
    pub(crate) fn extras_selectable_keys(
        m: &ReshadePackagesMint,
        needle: &str,
        arch: Option<&str>,
    ) -> Vec<String> {
        let Some(arch) = arch else {
            return Vec::new();
        };
        m.packages
            .iter()
            .filter(|p| extras_row_visible(m.kind_filter, p, needle) && p.mintable_for_arch(arch))
            .map(|p| p.key())
            .collect()
    }

    pub(crate) fn toggle_select_visible_extras(&mut self, on: bool, cx: &mut Context<Self>) {
        tracing::debug!(action = "toggle-select-visible-extras", on);
        let needle = self.extras_filter_input.read(cx).value().to_string();
        let needle = needle.trim().to_lowercase();
        if let Some(m) = self.extras_mint.as_mut() {
            let keys = Self::extras_selectable_keys(m, &needle, m.target_arch.as_deref());
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

    /// R38: mint every checked row for the target arch. Each package first
    /// migrates its un-suffixed legacy recipe to `B-x64` (conflict → error,
    /// nothing written), then mints the one variant matching the target.
    /// No target, no bitness, or no arch URL fails closed, retryable.
    pub(crate) fn mint_reshade_extras(&mut self, cx: &mut Context<Self>) {
        let Some(m) = self.extras_mint.as_ref() else {
            return;
        };
        tracing::debug!(action = "mint-extras", count = m.selected.len());
        let Some(target_id) = m.target_game.clone() else {
            self.status = self.strings.get("gui-reshade-extras-need-target");
            cx.notify();
            return;
        };
        let Some(arch) = m.target_arch.clone() else {
            self.status = self.strings.get("gui-reshade-extras-need-bitness");
            cx.notify();
            return;
        };
        let target = super::load_game_row(&target_id).map(|g| {
            let appid = g
                .resolved_appid()
                .and_then(|s| s.parse::<u32>().ok())
                .filter(|a| *a != 0);
            MintTarget {
                appid,
                title: g.display_name().to_string(),
            }
        });
        let Some(target) = target else {
            self.status = self.strings.get("gui-reshade-extras-need-target");
            cx.notify();
            return;
        };
        let items: Vec<tuxgt_core::ReshadePackage> = m
            .selected
            .iter()
            .filter_map(|key| {
                m.packages
                    .iter()
                    .find(|p| p.key() == *key && p.mintable_for_arch(&arch))
                    .cloned()
            })
            .collect();
        if items.is_empty() {
            self.status = self.strings.get("gui-reshade-extras-pick");
            cx.notify();
            return;
        }
        // A mutation vetoes a Hide like a transfer: the stub handoff would
        // kill it mid-write. Held inside the future: a local would drop on
        // return, before the op starts.
        let transfer = self.hide_state.installing();
        cx.spawn(async move |this, cx| {
            let _transfer = transfer;
            let result = cx
                .background_spawn(async move {
                    rt_block(async move {
                        let mut minted: Vec<String> = Vec::new();
                        let mut minted_keys: Vec<String> = Vec::new();
                        let mut locks: Vec<(String, bool, bool)> = Vec::new();
                        let mut first_err: Option<String> = None;
                        for pkg in &items {
                            if first_err.is_some() {
                                break;
                            }
                            let mut in64 = pkg.in_catalog;
                            let mut in32 = pkg.in_catalog_32;
                            let one = migrate_reshade_legacy(&config_dir(), &data_dir(), pkg)
                                .and_then(|migrated| {
                                    if migrated.is_some() {
                                        in64 = true;
                                    }
                                    match migrated {
                                        // The migration itself delivered the
                                        // x64 variant; minting it again would
                                        // only refuse "already in catalog".
                                        Some(id) if arch != "32" => Ok(id),
                                        _ => RecipeSpec::reshade_package_for_game(
                                            pkg, &arch, &target,
                                        )
                                        .and_then(|spec| {
                                            mint_recipe(&config_dir(), &data_dir(), spec)
                                        })
                                        .map(|inst| inst.id),
                                    }
                                });
                            match one {
                                Ok(id) => {
                                    if arch == "32" {
                                        in32 = true;
                                    } else {
                                        in64 = true;
                                    }
                                    minted.push(id);
                                    minted_keys.push(pkg.key());
                                }
                                Err(e) => first_err = Some(e.to_string()),
                            }
                            locks.push((pkg.key(), in64, in32));
                        }
                        Ok::<_, tuxgt_core::Error>((minted, minted_keys, locks, first_err))
                    })
                })
                .await;
            let _ = this.update(cx, |this, cx| match result {
                Ok((minted, minted_keys, locks, first_err)) => {
                    if let Some(em) = this.extras_mint.as_mut() {
                        for (key, in64, in32) in locks {
                            if let Some(p) = em.packages.iter_mut().find(|p| p.key() == key) {
                                p.in_catalog = in64;
                                p.in_catalog_32 = in32;
                            }
                        }
                    }
                    if !minted.is_empty() {
                        if this.instances_showing() {
                            this.instances = super::load_instances().into_boxed_slice();
                        }
                        this.refresh_selected_mods();
                    }
                    match first_err {
                        None => {
                            this.extras_mint = None;
                            let mut args = FluentArgs::new();
                            if minted.len() == 1 {
                                args.set("id", minted.into_iter().next().unwrap_or_default());
                            } else {
                                args.set("id", minted.join(", "));
                            }
                            this.status = this
                                .strings
                                .get_args("gui-status-instance-added", Some(&args));
                        }
                        Some(error) => {
                            if let Some(em) = this.extras_mint.as_mut() {
                                em.selected.retain(|s| !minted_keys.contains(s));
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
