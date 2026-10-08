use gpui_kit::*;

use tuxgt_core::{config_dir, data_dir, list_mods, list_templates, remove_mod, FluentArgs};

use super::super::{rt_block, SettingsModsTab, Shell};
use super::{pack_rows, tab_rows};

impl Shell {
    pub(crate) fn set_instance_offered_ui(
        &mut self,
        id: String,
        on: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        tracing::debug!(action = "set-instance-offered", source = id.as_str(), on);
        if on {
            self.confirm_restore_ui(id, window, cx);
        } else {
            self.confirm_disable_ui(id, window, cx);
        }
    }

    /// Re-read the packaged official mods from disk (lock 12): after
    /// `make deploy` with the app open, this picks up changed packaged
    /// files without a restart. There is no cache; listing is the reload.
    /// E104: Settings Update — refresh the catalog payload only
    /// (`acquire_with_source` redownload into `mods/<kind>/<id>/` + provenance).
    /// No manifest touch: game Instances stay Available until the
    /// game-card Update reinstalls them.
    pub(crate) fn refresh_catalog_payload_ui(&mut self, id: String, cx: &mut Context<Self>) {
        tracing::debug!(action = "refresh-catalog-payload", source = id.as_str());
        let id_bg = id.clone();
        // A mutation vetoes a Hide like a transfer: the stub handoff would
        // kill it mid-write. Held inside the future: a local would drop on
        // return, before the op starts.
        let transfer = self.hide_state.installing();
        cx.spawn(async move |this, cx| {
            let _transfer = transfer;
            let result = cx
                .background_spawn(async move {
                    rt_block(async {
                        let data = data_dir();
                        let cfg = config_dir();
                        let inst = tuxgt_core::find_mod(&cfg, &data, &id_bg)?;
                        let _ = tuxgt_core::acquire_with_source(&data, &inst, true, None).await?;
                        tuxgt_core::Result::Ok(inst.id.clone())
                    })
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                match result {
                    Ok(_iid) => {
                        // Re-poll: a refreshed payload usually clears the
                        // Available verdict and drops the Attention card.
                        this.update_last_poll = None;
                        this.maybe_poll_updates(cx);
                    }
                    Err(e) => {
                        this.status = format!("{e}");
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub(crate) fn resync_official_ui(&mut self, cx: &mut Context<Self>) {
        tracing::debug!(action = "resync-official");
        self.instances = super::load_instances().into_boxed_slice();
        self.family_templates = list_templates(&data_dir())
            .unwrap_or_default()
            .into_iter()
            .filter(|t| t.family.is_some())
            .collect::<Vec<_>>()
            .into_boxed_slice();
        self.refresh_selected_mods();
        self.status = self.strings.get("gui-status-official-resynced");
        cx.notify();
    }

    /// User catalog rows currently listed on this Mods inner tab.
    /// ReShade includes the filtered User Packs list. Officials never appear.
    pub(crate) fn visible_user_mod_ids(&self, cx: &App) -> Vec<String> {
        match self.mods_tab {
            SettingsModsTab::Optiscaler | SettingsModsTab::Custom => {
                tab_rows(&self.instances, self.mods_tab, false)
                    .map(|i| i.id.clone())
                    .collect()
            }
            SettingsModsTab::Reshade => {
                let needle = self
                    .packs_filter_input
                    .read(cx)
                    .value()
                    .to_string()
                    .trim()
                    .to_lowercase();
                let mut ids: Vec<String> =
                    tab_rows(&self.instances, SettingsModsTab::Reshade, false)
                        .filter(|i| i.mod_type == "reshade")
                        .map(|i| i.id.clone())
                        .collect();
                ids.extend(
                    pack_rows(&self.instances)
                        .filter(|i| {
                            !i.official && Shell::mod_matches_needle(&i.label, &i.id, &needle)
                        })
                        .map(|i| i.id.clone()),
                );
                ids
            }
        }
    }

    pub(crate) fn confirm_remove_visible_mods(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let ids = self.visible_user_mod_ids(cx);
        if ids.is_empty() {
            return;
        }
        self.confirm_remove_visible_scope(ids, window, cx);
    }

    pub(crate) fn remove_visible_mods_ui(&mut self, ids: Vec<String>, cx: &mut Context<Self>) {
        tracing::debug!(action = "remove-visible-mods", count = ids.len());
        // A mutation vetoes a Hide like a transfer: the stub handoff would
        // kill it mid-write. Held inside the future: a local would drop on
        // return, before the op starts.
        let transfer = self.hide_state.installing();
        cx.spawn(async move |this, cx| {
            let _transfer = transfer;
            let result = cx
                .background_spawn(async move {
                    rt_block(async {
                        let cfg = config_dir();
                        let data = data_dir();
                        let listed = list_mods(&cfg, &data)?;
                        let mut n = 0usize;
                        for id in ids {
                            if listed.mods.iter().any(|i| i.id == id && i.official) {
                                continue;
                            }
                            remove_mod(&cfg, &data, &id)?;
                            n += 1;
                        }
                        Ok::<_, tuxgt_core::Error>((super::load_instances(), n))
                    })
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                match result {
                    Ok((rows, n)) => {
                        if this.instances_showing() {
                            this.instances = rows.into_boxed_slice();
                        }
                        this.catalog_meta = super::load_catalog_meta();
                        this.refresh_selected_mods();
                        let mut args = FluentArgs::new();
                        args.set("n", n.to_string());
                        this.status = this.strings.get_args("gui-status-removed-n", Some(&args));
                    }
                    Err(e) => {
                        // The loop may have removed earlier mods before
                        // failing: reload so the list cannot show ghosts.
                        if this.instances_showing() {
                            this.instances = super::load_instances().into_boxed_slice();
                        }
                        this.catalog_meta = super::load_catalog_meta();
                        this.refresh_selected_mods();
                        let mut args = FluentArgs::new();
                        args.set("error", e.to_string());
                        this.status = this.strings.get_args("gui-status-err-remove", Some(&args));
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub(crate) fn remove_instance_ui(&mut self, id: String, cx: &mut Context<Self>) {
        tracing::debug!(action = "remove-instance", source = id.as_str());
        let owned = id.clone();
        let mut err_args = FluentArgs::new();
        err_args.set("id", owned.clone());
        let official_err = self
            .strings
            .get_args("gui-error-official-instance", Some(&err_args));
        // A mutation vetoes a Hide like a transfer: the stub handoff would
        // kill it mid-write. Held inside the future: a local would drop on
        // return, before the op starts.
        let transfer = self.hide_state.installing();
        cx.spawn(async move |this, cx| {
            let _transfer = transfer;
            let result = cx
                .background_spawn(async move {
                    rt_block(async {
                        let cfg = config_dir();
                        if list_mods(&cfg, &data_dir())?
                            .mods
                            .iter()
                            .any(|i| i.id == owned && i.official)
                        {
                            return Err(tuxgt_core::Error::InvalidInstance(official_err));
                        }
                        remove_mod(&cfg, &data_dir(), &owned)?;
                        Ok::<_, tuxgt_core::Error>(super::load_instances())
                    })
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                match result {
                    Ok(rows) => {
                        if this.instances_showing() {
                            this.instances = rows.into_boxed_slice();
                        }
                        this.catalog_meta = super::load_catalog_meta();
                        this.refresh_selected_mods();
                        let mut args = FluentArgs::new();
                        args.set("id", id.clone());
                        this.status = this
                            .strings
                            .get_args("gui-status-instance-removed", Some(&args));
                    }
                    Err(e) => {
                        let mut args = FluentArgs::new();
                        args.set("error", e.to_string());
                        this.status = this.strings.get_args("gui-status-err-remove", Some(&args));
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }
}
