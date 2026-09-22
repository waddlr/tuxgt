mod cache;
mod catalog;
mod enable;
mod install;
pub(crate) mod keep;
mod land;
mod opts;
mod preview;
mod repair;
mod slot;
mod uninstall;
mod update;

pub(crate) use cache::*;
pub(crate) use land::*;
pub(crate) use opts::*;
pub(crate) use preview::*;
pub(crate) use repair::*;

pub use cache::{
    migrate_mod_cache, mod_cache_rows, reconcile_mod_cache, CatalogStatus, ModCacheRow,
};
pub use catalog::{check_catalog_update, UpdateStatus};
pub use enable::set_instance_enabled;
pub use install::install_instance;
pub use keep::set_file_keep;
pub use opts::find_mod;
pub use opts::{is_config_text, payload_config_path};
pub use opts::InstallOpts;
pub use preview::{effect_names_for, payload_has_files, preview_payload_files};
pub use slot::{
    payload_drift, push_global_edits, push_global_edits_all, resync_game, resync_instance,
    set_instance_slot, set_load_order, PushReport,
};
pub use uninstall::{enabled_mod_count, has_apply_record, read_manifest_opt, uninstall_instance};
pub use update::{check_update, ensure_update_baseline, short_update_reason, Baseline};

#[cfg(test)]
mod testing;
#[cfg(test)]
mod tests_0;
#[cfg(test)]
mod tests_1;
#[cfg(test)]
mod tests_2;
#[cfg(test)]
mod tests_3;
#[cfg(test)]
mod tests_4;
#[cfg(test)]
mod tests_5;
#[cfg(test)]
mod tests_6;
#[cfg(test)]
mod tests_7;
