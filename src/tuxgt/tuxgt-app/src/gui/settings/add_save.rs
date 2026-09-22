use gpui_kit::*;

use super::*;
use tuxgt_core::{
    add_mod_from, add_mod_from_with_password, config_dir, data_dir, is_dll, rescan_mod, FluentArgs,
};

use super::super::{AddForm, Shell};

impl Shell {
    /// E91: toggle one Add-form file row: unchecking drops its dest (no
    /// destination); rechecking restores the scanned default. Keeps
    /// `select_visible` in sync (checked iff every row is checked). The row's
    /// input keeps its text; save reads live input values directly.
    pub(crate) fn toggle_add_keep(
        &mut self,
        form: Entity<AddForm>,
        i: usize,
        cx: &mut Context<Self>,
    ) {
        form.update(cx, |f, _| {
            let Some(file) = f.files.get_mut(i) else {
                return;
            };
            file.keep = !file.keep;
            if file.keep {
                let def = tuxgt_core::type_dest_for(&f.mod_type, &file.src);
                if file.dest.trim().is_empty() {
                    file.dest = def;
                }
            } else {
                file.dest = String::new();
            }
            f.select_visible = !f.files.is_empty() && f.files.iter().all(|x| x.keep);
        });
        cx.notify();
    }

    /// E91: Select Visible first row: check every row (restoring scanned dests)
    /// or uncheck every row (dropping dests).
    pub(crate) fn toggle_add_select_visible(
        &mut self,
        form: Entity<AddForm>,
        cx: &mut Context<Self>,
    ) {
        form.update(cx, |f, _| {
            let on = !(!f.files.is_empty() && f.files.iter().all(|x| x.keep));
            for file in f.files.iter_mut() {
                file.keep = on;
                if on {
                    let def = tuxgt_core::type_dest_for(&f.mod_type, &file.src);
                    if file.dest.trim().is_empty() {
                        file.dest = def;
                    }
                } else {
                    file.dest = String::new();
                }
            }
            f.select_visible = on;
        });
        cx.notify();
    }

    /// E91: toggle one ReShade Mod in the form Requires picker.
    pub(crate) fn toggle_add_require(
        &mut self,
        form: Entity<AddForm>,
        id: String,
        cx: &mut Context<Self>,
    ) {
        form.update(cx, |f, _| {
            if let Some(pos) = f.requires.iter().position(|r| r == &id) {
                f.requires.remove(pos);
            } else {
                f.requires.push(id);
            }
        });
        cx.notify();
    }

    /// E91: Save the inline Add panel: flush each checked row's dest from
    /// its live input (unchecked rows stay dest-less), then `rescan_mod`
    /// (with the rows' Include dests) for a rescan or `add_mod_from`
    /// (per-DLL includes + picked Requires) for an Add. Success closes the
    /// panel and refreshes the lists; failure keeps it open with the error
    /// in the status line.
    pub(crate) fn save_add_form(&mut self, form: Entity<AddForm>, cx: &mut Context<Self>) {
        tracing::debug!(action = "save-add-form");
        let name = self.instance_id_input.read(cx).value().to_string();
        let dests: Vec<String> = self
            .add_dest_inputs
            .iter()
            .map(|inp| inp.read(cx).value().to_string())
            .collect();
        form.update(cx, |f, _| {
            for (i, file) in f.files.iter_mut().enumerate() {
                if file.keep {
                    if let Some(d) = dests.get(i) {
                        file.dest = d.clone();
                    }
                } else {
                    file.dest = String::new();
                }
            }
        });
        let form_data = form.read(cx);
        let rescan_id = form_data.rescan_id.clone();
        let mod_type = form_data.mod_type.clone();
        let path = form_data.path.clone();
        let files = form_data.files.clone();
        let includes = form_data.includes.clone();
        let requires = form_data.requires.clone();
        let password = form_data.password.clone();
        let include: Vec<String> = files
            .iter()
            .zip(includes.iter())
            .filter(|(file, inc)| **inc && file.keep && is_dll(&file.dest))
            .map(|(file, _)| file.dest.clone())
            .collect();
        let result = if let Some(rid) = rescan_id.clone() {
            rescan_mod(
                &config_dir(),
                &rid,
                Some(&files),
                Some(&include),
                &data_dir(),
            )
        } else if let Some(password) = password.as_deref() {
            add_mod_from_with_password(
                &config_dir(),
                &mod_type,
                &suggest_id(name.trim()),
                &path,
                Some(name.trim()).filter(|s| !s.is_empty()),
                Some(&files),
                &data_dir(),
                include,
                requires,
                Some(password),
            )
        } else {
            add_mod_from(
                &config_dir(),
                &mod_type,
                &suggest_id(name.trim()),
                &path,
                Some(name.trim()).filter(|s| !s.is_empty()),
                Some(&files),
                &data_dir(),
                include,
                requires,
            )
        };
        match result {
            Ok(inst) => {
                let outcome = if rescan_id.is_some() { "rescanned" } else { "added" };
                tracing::debug!(action = "save-add-form", source = inst.id.as_str(), outcome);
                self.add_form = None;
                self.instances = super::load_instances().into_boxed_slice();
                self.refresh_selected_mods();
                let mut args = FluentArgs::new();
                args.set("id", inst.id.clone());
                let key = if rescan_id.is_some() {
                    "gui-status-instance-rescanned"
                } else {
                    "gui-status-instance-added"
                };
                self.status = self.strings.get_args(key, Some(&args));
            }
            Err(e) => {
                let mut args = FluentArgs::new();
                args.set("error", e.to_string());
                self.status = self
                    .strings
                    .get_args("gui-status-err-instance", Some(&args));
            }
        }
        cx.notify();
    }
}
