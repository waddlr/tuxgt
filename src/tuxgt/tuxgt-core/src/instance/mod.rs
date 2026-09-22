mod add;
mod catalog;
mod export;
mod from;
mod migrate;
mod mint;
mod parse;
mod paths;
mod recipe;
mod scan;
mod slug;
mod templates;
mod write;

pub(crate) use add::*;
#[cfg(test)]
pub(crate) use catalog::*;
pub(crate) use export::*;
pub(crate) use migrate::*;
pub(crate) use parse::*;
pub(crate) use paths::*;
pub(crate) use recipe::*;
pub(crate) use scan::*;
pub(crate) use slug::*;
pub(crate) use templates::*;
#[cfg(test)]
pub(crate) use testing::seed_official_share_from_repo;
pub(crate) use write::*;

pub use add::{add_mod, remove_mod};
pub use catalog::{
    disable_mod, enable_mod, list_mods, mods_for_game, set_mod_offered, ModList, ModProblem,
};
pub use export::export_mod;
pub use from::{
    add_mod_from, add_mod_from_with_password, apply_package_slot, rescan_mod, rescan_preview,
    scan_default_include,
};
pub use migrate::migrate_prefix;
pub use mint::{family_template, mint_recipe, snap_family_asset, RecipeSpec};
pub use recipe::{Mod, PayloadRule, Plan, SourceRef};
pub use scan::{
    classify_package, classify_package_with_password, resolve_source, scan_package, Classification,
    ClassifyKind, PackageFile,
};
pub use templates::{list_templates, ModTemplate, TemplateFamily};

#[cfg(test)]
mod testing;
#[cfg(test)]
mod tests_catalog;
#[cfg(test)]
mod tests_classify;
#[cfg(test)]
mod tests_enable;
#[cfg(test)]
mod tests_export;
#[cfg(test)]
mod tests_from;
#[cfg(test)]
mod tests_mint;
#[cfg(test)]
mod tests_requires;
#[cfg(test)]
mod tests_scan;
