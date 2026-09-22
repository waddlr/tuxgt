use gpui_kit::assets::IconName as FullIconName;
use gpui_kit::component::button::ButtonVariants as _;
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::component::{Icon, Sizable as _};
use gpui_kit::*;

use tuxgt_core::{
    config_dir, data_dir, effect_names_for, find_mod, is_config_text, payload_has_files,
    FluentArgs,
};

use super::super::*;

impl Shell {
    /// `Effects` preview toggle: the recipe's `EffectFiles` list, already
    /// drop-filtered on the row. No disk access.
    pub(crate) fn toggle_effects_preview(&mut self, id: &str, cx: &mut Context<Self>) {
        let key = format!("fx:{id}");
        if !self.preview_open.insert(key.clone()) {
            self.preview_open.remove(&key);
            cx.notify();
            return;
        }
        if !self.file_preview_cache.contains_key(&key) {
            let names = self
                .preview_source(id)
                .map(|(_, _, _, names)| names)
                .unwrap_or_default();
            let total = names.len();
            self.file_preview_cache.insert(key.clone(), (names, total));
        }
        cx.notify();
    }

    /// `(mod_type, payload_present, asset, effect names)` for a Mod id, from
    /// whichever row set is loaded: the selected game's rows, else Settings,
    /// else a transient catalog read (toggle actions only, never paint).
    pub(crate) fn preview_source(
        &self,
        id: &str,
    ) -> Option<(String, bool, Option<String>, Vec<String>)> {
        if let Some(r) = self.mods.values().flatten().find(|r| r.instance == id) {
            return Some((
                r.mod_type.clone(),
                r.payload_present,
                r.asset.clone(),
                r.effect_files.to_vec(),
            ));
        }
        if let Some(r) = self.instances.iter().find(|r| r.id == id) {
            return Some((
                r.mod_type.clone(),
                r.payload_present,
                r.asset.clone(),
                r.effect_files.to_vec(),
            ));
        }
        let m = find_mod(&config_dir(), &data_dir(), id).ok()?;
        Some((
            m.mod_type.clone(),
            payload_has_files(&data_dir(), &m),
            source_asset(&m.source),
            effect_names_for(&m),
        ))
    }

    pub(crate) fn toggle_file_preview(&mut self, id: &str, cx: &mut Context<Self>) {
        let key = format!("mod:{id}");
        if !self.preview_open.insert(key.clone()) {
            self.preview_open.remove(&key);
            cx.notify();
            return;
        }
        if !self.file_preview_cache.contains_key(&key) {
            self.fill_file_preview(id);
        }
        cx.notify();
    }

    /// (Re)run the payload walk for one mod's `mod:<id>` preview key. The
    /// toggle path fills on first expand; content-edit saves and the
    /// external-open refocus refill instead of busting — a busted key with
    /// the preview still open paints the preview-error line, while names
    /// (all the cache holds) survive a content edit untouched.
    pub(crate) fn fill_file_preview(&mut self, id: &str) {
        let key = format!("mod:{id}");
        // `Asset` rows have no payload yet: the catalog file name *is*
        // their file list.
        let mut asset = None;
        if let Some((mtype, present, name, _)) = self.preview_source(id) {
            if let Some(FilesPreview::Asset(file)) =
                files_preview(&mtype, present, name.as_deref())
            {
                asset = Some(file.to_string());
            }
        }
        if let Some(name) = asset {
            self.file_preview_cache.insert(key.clone(), (vec![name], 1));
            self.preview_errors.remove(&key);
        } else {
            match tuxgt_core::preview_payload_files(&config_dir(), &data_dir(), id) {
                Ok((names, total)) => {
                    self.file_preview_cache.insert(key.clone(), (names, total));
                    self.preview_errors.remove(&key);
                }
                Err(_) => {
                    self.preview_errors.insert(key.clone());
                }
            }
        }
    }

    /// Effects/Files preview controls for one Mod row: the two buttons and
    /// their bodies. The picker and the Settings pack card paint the buttons
    /// in the row's control cluster; the installed card paints them inside
    /// its accordion body. `prefix` keeps the element ids unique per list.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn preview_controls(
        &self,
        id: &str,
        prefix: &str,
        mod_type: &str,
        effect_files: &[String],
        asset: Option<&str>,
        payload_present: bool,
        view: Entity<Self>,
        cx: &App,
        allow_edit: bool,
    ) -> (Vec<AnyElement>, Vec<AnyElement>) {
        let fx_key = format!("fx:{id}");
        let files_key = format!("mod:{id}");
        let mut buttons: Vec<AnyElement> = Vec::new();
        let mut bodies: Vec<AnyElement> = Vec::new();
        for (key, label_key, shown) in [
            (
                fx_key.as_str(),
                "gui-preview-effects",
                shows_effects(mod_type, effect_files),
            ),
            (
                files_key.as_str(),
                "gui-preview-files",
                files_preview(mod_type, payload_present, asset).is_some(),
            ),
        ] {
            if !shown {
                continue;
            }
            let open = self.preview_open.contains(key);
            let arrow = if open { "▾" } else { "▸" };
            let toggle_view = view.clone();
            let toggle_id = id.to_string();
            let effects = label_key == "gui-preview-effects";
            buttons.push(
                widgets::btn(
                    SharedString::from(format!(
                        "{prefix}-{}-{id}",
                        if effects { "fx" } else { "files" }
                    )),
                    cx,
                )
                .ghost()
                .child(widgets::muted(
                    format!("{} {arrow}", self.strings.get(label_key)),
                    cx,
                ))
                .on_click(move |_, _, cx| {
                    cx.stop_propagation();
                    toggle_view.update(cx, |this, cx| {
                        if effects {
                            this.toggle_effects_preview(&toggle_id, cx);
                        } else {
                            this.toggle_file_preview(&toggle_id, cx);
                        }
                    });
                })
                .into_any_element(),
            );
            // Edit only payload-backed lists: `Asset` rows (payload-less
            // catalog/archive names) have no payload file to rewrite, so an
            // Edit button there could only fail in `payload_config_path`.
            if let Some(body) =
                self.file_preview_list(key, view.clone(), allow_edit && payload_present, cx)
            {
                bodies.push(body);
            }
        }
        (buttons, bodies)
    }

    /// Paint a preview body cached under `key` (arrow state lives in
    /// `preview_open`). Settings callers (`allow_edit`, payload-backed lists
    /// only) paint one Edit per allowlisted text config and mount the editor
    /// card under the row that opened it; the picker and installed cards
    /// keep the plain name list.
    pub(crate) fn file_preview_list(
        &self,
        key: &str,
        view: Entity<Self>,
        allow_edit: bool,
        cx: &App,
    ) -> Option<AnyElement> {
        if !self.preview_open.contains(key) {
            return None;
        }
        // Catalog id behind a `mod:<id>` files key; `None` (e.g. `fx:` keys)
        // paints no Edit buttons.
        let edit_id: Option<String> = allow_edit
            .then(|| key.split_once(':'))
            .flatten()
            .filter(|(kind, _)| *kind == "mod")
            .map(|(_, id)| id.to_string());
        let body: AnyElement = if self.preview_errors.contains(key) {
            widgets::muted(self.strings.get("gui-preview-error"), cx).into_any_element()
        } else {
            match self.file_preview_cache.get(key) {
                None => {
                    widgets::muted(self.strings.get("gui-preview-error"), cx).into_any_element()
                }
                Some((names, _)) if names.is_empty() => {
                    widgets::muted(self.strings.get("gui-preview-needs-install"), cx)
                        .into_any_element()
                }
                Some((names, _)) => {
                    // Default 10 rows; the tail toggles the full kept list.
                    const VISIBLE: usize = 10;
                    let expanded = self.preview_expanded.contains(key);
                    let rows: Vec<AnyElement> = names
                        .iter()
                        .enumerate()
                        .take_while(|(i, _)| expanded || *i < VISIBLE)
                        .map(|(i, n)| {
                            let Some(mod_id) = edit_id.clone() else {
                                return widgets::mono(n.clone(), cx).into_any_element();
                            };
                            if !is_config_text(n) {
                                return widgets::mono(n.clone(), cx).into_any_element();
                            }
                            let edit_view = view.clone();
                            let edit_rel = n.clone();
                            h_flex()
                                .gap_2()
                                .items_center()
                                .child(
                                    div()
                                        .flex_1()
                                        .min_w_0()
                                        .child(widgets::mono(n.clone(), cx)),
                                )
                                .child(
                                    widgets::btn(
                                        SharedString::from(format!(
                                            "ec-mod-{mod_id}-{i}"
                                        )),
                                        cx,
                                    )
                                    .secondary()
                                    .child(Icon::new(FullIconName::FilePenLine).small())
                                    .tooltip(self.strings.get("gui-action-edit-config"))
                                    .on_click(move |_, window, cx| {
                                        edit_view.update(cx, |this, cx| {
                                            this.open_config_ui(
                                                ConfigLevel::Global { id: mod_id.clone() },
                                                edit_rel.clone(),
                                                window,
                                                cx,
                                            );
                                        });
                                    }),
                                )
                                .into_any_element()
                        })
                        .collect();
                    // `gui.mod-config-edit`: the editor is a full-page takeover
                    // in the Settings boxes, never an inline card here.
                    let mut list = v_flex()
                        .id(SharedString::from(format!("preview-list-{key}")))
                        .gap_0()
                        .children(rows);
                    if names.len() > VISIBLE {
                        let label = if expanded {
                            self.strings.get("gui-preview-less")
                        } else {
                            let mut args = FluentArgs::new();
                            args.set("n", names.len().saturating_sub(VISIBLE).to_string());
                            self.strings.get_args("gui-preview-more", Some(&args))
                        };
                        let toggle_view = view.clone();
                        let toggle_key = key.to_string();
                        list = list.child(
                            widgets::btn(
                                SharedString::from(format!("preview-expand-{key}")),
                                cx,
                            )
                            .ghost()
                            .child(widgets::muted(label, cx))
                            .on_click(move |_, _, cx| {
                                cx.stop_propagation();
                                toggle_view.update(cx, |this, cx| {
                                    if !this.preview_expanded.insert(toggle_key.clone()) {
                                        this.preview_expanded.remove(&toggle_key);
                                    }
                                    cx.notify();
                                });
                            }),
                        );
                    }
                    list.into_any_element()
                }
            }
        };
        Some(body)
    }

}
