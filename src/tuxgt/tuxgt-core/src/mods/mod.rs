mod adapter;
mod cache;
mod catalog;
mod enable;
mod file_mode;
mod harvested;
mod install;
pub(crate) mod keep;
mod land;
mod opts;
mod park;
mod preview;
mod provide;
mod repair;
mod slot;
mod uninstall;
mod update;

pub(crate) use cache::*;
pub(crate) use harvested::{apply_harvested_moves, plan_harvested_moves, HarvestedMove};
pub(crate) use land::*;
pub(crate) use opts::*;
pub(crate) use preview::*;
pub(crate) use repair::*;

pub use adapter::{convert_game_adapter, validate_adapter_convert, ConversionReport};
pub use cache::{
    migrate_mod_cache, mod_cache_rows, reconcile_mod_cache, CatalogStatus, ModCacheRow,
};
pub use catalog::{check_catalog_update, UpdateStatus};
pub use enable::set_instance_enabled;
pub use file_mode::set_file_load;
pub use install::install_instance;
pub use keep::set_file_keep;
pub use opts::find_mod;
pub use opts::InstallOpts;
pub use opts::{is_config_text, payload_config_path};
pub use preview::{effect_names_for, payload_has_files, payload_provenance, preview_payload_files};
pub use provide::{
    clear_provided_files, provide_files, provide_files_with_password, provided_files,
    ProvideReport, ProvidedFiles,
};
pub use slot::{
    apply_slot_picks, convert_after_unplace, ensure_repick_free, is_self_slot, payload_drift,
    preflight_convert_picks, push_global_edits, push_global_edits_all, recipe_slot_configurable,
    remembered_slot, resync_game, resync_instance, resync_repick_instances, set_instance_slot,
    set_load_order, unplace_instance_slot, PushReport, SELF_SLOT,
};
pub(crate) use slot::{claiming_slot_index, is_named_injector};
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
#[cfg(test)]
mod tests_8;
#[cfg(test)]
mod tests_9;
#[cfg(test)]
mod tests_adapter_harvested;
#[cfg(test)]
mod tests_file_mode;
#[cfg(test)]
mod tests_park;
#[cfg(test)]
mod tests_park_2;
#[cfg(test)]
mod tests_park_3;
#[cfg(test)]
mod tests_provide;
#[cfg(test)]
mod tests_slot;
#[cfg(test)]
mod tests_slot_batch;
#[cfg(test)]
mod tests_slot_modes;
#[cfg(test)]
mod tests_slot_restore;
