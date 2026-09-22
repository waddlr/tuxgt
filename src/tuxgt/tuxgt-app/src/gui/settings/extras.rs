use gpui_kit::*;

use super::*;
use tuxgt_core::{
    config_dir, data_dir, list_reshade_packages, mint_recipe, FluentArgs, RecipeSpec,
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

    pub(crate) fn toggle_reshade_extra(&mut self, key: String, cx: &mut Context<Self>) {
        tracing::debug!(action = "toggle-reshade-extra", key = key.as_str());
        let Some(m) = self.extras_mint.as_mut() else {
            return;
        };
        let Some(pkg) = m.packages.iter().find(|p| p.key() == key) else {
            return;
        };
        if !pkg.mintable() {
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
    /// and text needle (installed and URL-less rows excluded).
    pub(crate) fn extras_selectable_keys(m: &ReshadePackagesMint, needle: &str) -> Vec<String> {
        m.packages
            .iter()
            .filter(|p| extras_row_visible(m.kind_filter, p, needle) && p.mintable())
            .map(|p| p.key())
            .collect()
    }

    pub(crate) fn toggle_select_visible_extras(&mut self, on: bool, cx: &mut Context<Self>) {
        tracing::debug!(action = "toggle-select-visible-extras", on);
        let needle = self.extras_filter_input.read(cx).value().to_string();
        let needle = needle.trim().to_lowercase();
        if let Some(m) = self.extras_mint.as_mut() {
            let keys = Self::extras_selectable_keys(m, &needle);
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

    pub(crate) fn mint_reshade_extras(&mut self, cx: &mut Context<Self>) {
        let Some(m) = self.extras_mint.as_ref() else {
            return;
        };
        tracing::debug!(action = "mint-extras", count = m.selected.len());
        let items: Vec<tuxgt_core::ReshadePackage> = m
            .selected
            .iter()
            .filter_map(|key| {
                m.packages
                    .iter()
                    .find(|p| p.key() == *key && p.mintable())
                    .cloned()
            })
            .collect();
        if items.is_empty() {
            self.status = self.strings.get("gui-reshade-extras-pick");
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
                        for pkg in items {
                            if first_err.is_some() {
                                break;
                            }
                            // Settings mint stays 64-bit (catalog is game-agnostic;
                            // per-game callers use reshade_package_for_arch)
                            let one = RecipeSpec::reshade_package(&pkg)
                                .and_then(|spec| mint_recipe(&config_dir(), &data_dir(), spec));
                            match one {
                                Ok(inst) => {
                                    minted.push(inst.id);
                                    minted_keys.push(pkg.key());
                                }
                                Err(e) => first_err = Some(e.to_string()),
                            }
                        }
                        Ok::<_, tuxgt_core::Error>((minted, minted_keys, first_err))
                    })
                })
                .await;
            let _ = this.update(cx, |this, cx| match result {
                Ok((minted, minted_keys, first_err)) => {
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
                                for p in em.packages.iter_mut() {
                                    if minted_keys.iter().any(|k| *k == p.key()) {
                                        p.in_catalog = true;
                                    }
                                }
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
