//! Builds the Details model: grid cells, full-width lines, and name lists.
//! Paint and row height live in `mods_details.rs`.

use std::collections::HashSet;

use tuxgt_core::FluentArgs;

use super::super::instance_facts::{format_bytes, format_unix_date, ModKindCode, RuleParts};
use super::super::{files_preview, shows_effects, InstanceRow, Shell};
use super::source_label;

pub(super) struct NameBlock {
    pub key: String,
    pub title: String,
    pub names: Vec<String>,
    pub edit_id: Option<String>,
}

pub(super) struct DetailView {
    pub cells: Vec<(String, String)>,
    pub lines: Vec<(String, String)>,
    pub inline: Vec<NameBlock>,
    pub nested: Vec<NameBlock>,
}

impl Shell {
    pub(super) fn detail_view(&self, row: &InstanceRow) -> DetailView {
        let f = &row.facts;
        let mut cells = vec![
            (self.strings.get("gui-mod-detail-id"), row.id.clone()),
            (
                self.strings.get("gui-mod-detail-kind"),
                self.kind_label(&f.kind),
            ),
            (
                self.strings.get("gui-mod-detail-type"),
                row.mod_type.clone(),
            ),
        ];
        if let Some(slot) = &f.slot {
            cells.push((self.strings.get("gui-mod-detail-slot"), slot.clone()));
        }
        if !f.plans.is_empty() {
            let plans = f
                .plans
                .iter()
                .map(|p| self.plan_label(p))
                .collect::<Vec<_>>()
                .join(", ");
            cells.push((self.strings.get("gui-mod-detail-plans"), plans));
        }
        if let Some(at) = f.fetched_at.and_then(format_unix_date) {
            cells.push((self.strings.get("gui-mod-detail-fetched"), at));
        }
        if let Some(n) = f.asset_bytes {
            cells.push((self.strings.get("gui-mod-detail-size"), format_bytes(n)));
        }
        if let Some(status) = self.status_value(&row.id) {
            cells.push((self.strings.get("gui-mod-detail-status"), status));
        }
        if let Some(checked) = self.checked_value(&row.id) {
            cells.push((self.strings.get("gui-mod-detail-checked"), checked));
        }
        let mut lines = vec![(
            self.strings.get("gui-mod-detail-source"),
            self.source_line(row),
        )];
        if let Some(url) = &f.provenance {
            lines.push((self.strings.get("gui-mod-detail-url"), url.clone()));
        }
        if !f.requires.is_empty() {
            lines.push((
                self.strings.get("gui-mod-detail-requires"),
                f.requires.join(", "),
            ));
        }
        let edit_id = (!row.official && row.payload_present).then(|| row.id.clone());
        let mut inline = Vec::new();
        let mut nested = Vec::new();
        self.push_list(&mut inline, &mut nested, self.applies_block(row));
        self.push_list(
            &mut inline,
            &mut nested,
            self.include_block(row, edit_id.clone()),
        );
        self.push_list(&mut inline, &mut nested, self.remap_block(row));
        self.push_list(&mut inline, &mut nested, self.rule_block(row));
        self.push_list(&mut inline, &mut nested, self.env_block(row));
        self.push_list(
            &mut inline,
            &mut nested,
            self.effects_block(row, edit_id.clone()),
        );
        self.push_list(&mut inline, &mut nested, self.files_block(row, edit_id));
        let installed = self
            .catalog_meta
            .installed
            .get(&row.id)
            .cloned()
            .unwrap_or_default();
        if installed.is_empty() {
            lines.push((
                self.strings.get("gui-mod-detail-installed"),
                self.strings.get("gui-mod-detail-not-installed"),
            ));
        } else {
            self.push_list(
                &mut inline,
                &mut nested,
                Some(NameBlock {
                    key: format!("dinst:{}", row.id),
                    title: self.strings.get("gui-mod-detail-installed"),
                    names: installed,
                    edit_id: None,
                }),
            );
        }
        DetailView {
            cells,
            lines,
            inline,
            nested,
        }
    }

    fn push_list(
        &self,
        inline: &mut Vec<NameBlock>,
        nested: &mut Vec<NameBlock>,
        block: Option<NameBlock>,
    ) {
        let Some(block) = block else { return };
        if block.names.len() > 2 {
            nested.push(block);
        } else {
            inline.push(block);
        }
    }

    fn applies_block(&self, row: &InstanceRow) -> Option<NameBlock> {
        let mut names = Vec::new();
        for glob in &row.facts.globs {
            let mut args = FluentArgs::new();
            args.set("glob", glob.clone());
            names.push(
                self.strings
                    .get_args("gui-mod-detail-name-glob", Some(&args)),
            );
        }
        let mut missing = 0usize;
        for id in &row.facts.appids {
            if let Some(name) = self.catalog_meta.appids.get(id) {
                names.push(name.clone());
            } else {
                missing += 1;
            }
        }
        if missing > 0 {
            let mut args = FluentArgs::new();
            args.set("n", missing.to_string());
            names.push(
                self.strings
                    .get_args("gui-mod-detail-not-in-library", Some(&args)),
            );
        }
        (!names.is_empty()).then(|| NameBlock {
            key: format!("dapply:{}", row.id),
            title: self.strings.get("gui-mod-detail-applies"),
            names,
            edit_id: None,
        })
    }

    fn include_block(&self, row: &InstanceRow, edit_id: Option<String>) -> Option<NameBlock> {
        (!row.facts.include.is_empty()).then(|| NameBlock {
            key: format!("dinc:{}", row.id),
            title: self.strings.get("gui-mod-detail-include"),
            names: row.facts.include.clone(),
            edit_id,
        })
    }

    fn remap_block(&self, row: &InstanceRow) -> Option<NameBlock> {
        let names: Vec<String> = row
            .facts
            .remaps
            .iter()
            .map(|(src, dest)| format!("{src} → {dest}"))
            .collect();
        (!names.is_empty()).then(|| NameBlock {
            key: format!("dremap:{}", row.id),
            title: self.strings.get("gui-mod-detail-remap"),
            names,
            edit_id: None,
        })
    }

    fn rule_block(&self, row: &InstanceRow) -> Option<NameBlock> {
        let names: Vec<String> = row
            .facts
            .rules
            .iter()
            .filter_map(|r| self.rule_line(r))
            .collect();
        (!names.is_empty()).then(|| NameBlock {
            key: format!("drule:{}", row.id),
            title: self.strings.get("gui-mod-detail-payload"),
            names,
            edit_id: None,
        })
    }

    fn env_block(&self, row: &InstanceRow) -> Option<NameBlock> {
        let names: Vec<String> = row
            .facts
            .env
            .iter()
            .map(|(k, v)| format!("{k} · {v}"))
            .collect();
        (!names.is_empty()).then(|| NameBlock {
            key: format!("denv:{}", row.id),
            title: self.strings.get("gui-mod-detail-env"),
            names,
            edit_id: None,
        })
    }

    fn effects_block(&self, row: &InstanceRow, edit_id: Option<String>) -> Option<NameBlock> {
        if !shows_effects(&row.mod_type, &row.effect_files) {
            return None;
        }
        Some(NameBlock {
            key: format!("fx:{}", row.id),
            title: self.strings.get("gui-preview-effects"),
            names: row.effect_files.to_vec(),
            edit_id,
        })
    }

    fn files_block(&self, row: &InstanceRow, edit_id: Option<String>) -> Option<NameBlock> {
        if files_preview(&row.mod_type, row.payload_present, row.asset.as_deref()).is_none() {
            return None;
        }
        let key = format!("mod:{}", row.id);
        if self.preview_errors.contains(&key) {
            return Some(NameBlock {
                key: format!("dfiles:{}", row.id),
                title: self.strings.get("gui-preview-files"),
                names: vec![self.strings.get("gui-preview-error")],
                edit_id: None,
            });
        }
        let Some((names, _)) = self.file_preview_cache.get(&key) else {
            return None;
        };
        if names.is_empty() {
            return Some(NameBlock {
                key: format!("dfiles:{}", row.id),
                title: self.strings.get("gui-preview-files"),
                names: vec![self.strings.get("gui-preview-needs-install")],
                edit_id: None,
            });
        }
        let effects: HashSet<&str> = row.effect_files.iter().map(String::as_str).collect();
        let names: Vec<String> = names
            .iter()
            .filter(|n| !effect_name(n, &effects))
            .cloned()
            .collect();
        if names.len() == 1 {
            if let Some(asset) = &row.asset {
                let named = row.facts.source_detail.ends_with(asset.as_str())
                    || row
                        .facts
                        .provenance
                        .as_deref()
                        .is_some_and(|url| url.ends_with(asset.as_str()));
                if &names[0] == asset && named {
                    return None;
                }
            }
        }
        (!names.is_empty()).then(|| NameBlock {
            key: format!("dfiles:{}", row.id),
            title: self.strings.get("gui-preview-files"),
            names,
            edit_id,
        })
    }

    fn rule_line(&self, rule: &RuleParts) -> Option<String> {
        let mut parts = Vec::new();
        if let Some(arch) = &rule.arch {
            let mut args = FluentArgs::new();
            args.set("arch", arch.clone());
            parts.push(self.strings.get_args("gui-mod-detail-arch", Some(&args)));
        }
        if let Some(api) = &rule.api {
            parts.push(api.clone());
        }
        if !rule.keep.is_empty() {
            let mut args = FluentArgs::new();
            args.set("names", rule.keep.clone());
            parts.push(self.strings.get_args("gui-mod-detail-keep", Some(&args)));
        }
        if !rule.drop.is_empty() {
            let mut args = FluentArgs::new();
            args.set("names", rule.drop.clone());
            parts.push(self.strings.get_args("gui-mod-detail-drop", Some(&args)));
        }
        (!parts.is_empty()).then(|| parts.join(" · "))
    }

    fn kind_label(&self, kind: &ModKindCode) -> String {
        match kind {
            ModKindCode::Official => self.strings.get("gui-section-mods-official"),
            ModKindCode::User => self.strings.get("gui-mod-kind-user"),
            ModKindCode::Registry(slug) => slug.clone(),
        }
    }

    fn plan_label(&self, plan: &str) -> String {
        match plan {
            "install" => self.strings.get("gui-adapter-install"),
            "preload" => self.strings.get("gui-adapter-preload"),
            "proton_env" => self.strings.get("gui-plan-proton-env"),
            other => other.to_string(),
        }
    }

    fn source_line(&self, row: &InstanceRow) -> String {
        let mut parts = vec![source_label(&row.source).to_string()];
        if !row.facts.source_detail.is_empty() {
            parts.push(row.facts.source_detail.clone());
        }
        if let Some(tag) = &row.facts.tag {
            let mut args = FluentArgs::new();
            args.set("tag", tag.clone());
            parts.push(self.strings.get_args("gui-mod-detail-tag", Some(&args)));
        }
        if row.facts.prerelease {
            parts.push(self.strings.get("gui-mod-detail-prerelease"));
        }
        parts.join(" · ")
    }

    fn status_value(&self, id: &str) -> Option<String> {
        if self.catalog_updates.contains(id) {
            return None;
        }
        let snap = self.catalog_meta.cache.get(id)?;
        match snap.status.as_deref() {
            Some("uptodate") => Some(self.strings.get("gui-mod-detail-up-to-date")),
            Some("unknown") => Some(self.strings.get(tuxgt_core::short_update_reason(
                snap.detail.as_deref().unwrap_or(""),
            ))),
            Some("available") => Some(self.strings.get("gui-mod-update-available-short")),
            _ => None,
        }
    }

    fn checked_value(&self, id: &str) -> Option<String> {
        let at = self.catalog_meta.cache.get(id)?.last_check?;
        if at <= 0 {
            return None;
        }
        format_unix_date(u64::try_from(at).ok()?)
    }
}

fn effect_name(name: &str, effects: &HashSet<&str>) -> bool {
    let base = name.rsplit('/').next().unwrap_or(name);
    effects.iter().any(|effect| {
        let effect_base = effect.rsplit('/').next().unwrap_or(effect);
        *effect == name || *effect == base || effect_base == name || effect_base == base
    })
}
