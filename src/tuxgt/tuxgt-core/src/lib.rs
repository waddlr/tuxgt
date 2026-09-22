pub mod apply;
pub mod client;
mod db;
pub mod detect;
pub mod download;
pub mod env;
mod error;
mod fs;
pub mod game;
pub mod install;
pub mod instance;
pub mod launch;
pub mod metadata;
pub mod mods;
pub mod modtype;
mod paths;
pub mod plugin;
pub mod prewire;
pub mod provider;
pub mod session;
pub mod stage;
mod strings;
#[cfg(test)]
pub(crate) mod testing;
pub mod userland;
pub mod wrapper;

pub use db::{open_db, open_db_shared};
pub use error::{Error, Result};
pub(crate) use fs::{atomic_write, atomic_write_str};
pub(crate) use paths::dirs_home;
pub use paths::{boot_conf, config_dir, data_dir, debug_log_enabled};
pub use sqlx::SqlitePool;
pub use strings::{FluentArgs, Strings, EN_US_FTL};

pub use apply::{apply_launch, read_record, restore_launch, ApplyCtx, ApplyFile, ApplyRecord};
pub use client::StoreClient;
pub use detect::{
    detect_one, detection_snapshot, doctor, host_gpu, set_override, validate_override, DetectOpts,
    DetectSnapshot, DoctorReport, HostGpu, OVERRIDE_FIELDS,
};
pub use download::{
    acquire_with_source, all_tools, art_file, art_fingerprint_file, bust_game_art_for_appid,
    cache_local, cached_file, cap_preview, fetch_url, game_manifests, generated_globs_for,
    harvest_all, harvest_game, harvest_game_roots, harvest_roots, hero_file, icon_file,
    include_covers, is_required_dest, list_family_assets, list_family_assets_many,
    list_reshade_packages, manifest_path, preview_effect_files, read_manifest, render_game_art,
    set_file_enabled, set_manifest_enabled, set_mod_env_enabled, sha256_file, sha256_hex,
    thumb_file, tool_status, touch_game_art, unpack, write_manifest, ArtKind, CachedAsset, ExtTool,
    FamilyAsset, FetchProgress, FileManifest, ModProvenance, PlannedEnv, PlannedFile, ProgressSink,
    ReshadePackage, ReshadePackageKind, ToolStatus, EXT_TOOLS,
};
pub use env::{
    count_set_env, custom_env, disable_global_knob, disable_knob, effective_knob_value,
    effective_platform, enable_global_knob, enable_knob, enabled_global_env_pairs, enabled_knobs,
    env_keys, find_enabled_knob, find_knob, global_knobs, knob_is_unmanaged, knob_rows,
    knob_source, knob_values, live_knob_value, migrate_env, proton_ge_cachy, remove_custom,
    resolve_value, retire_environment_d, scope_applies, scopes_display, set_custom,
    set_global_knob, set_knob, unset_global_knob, unset_knob, validate_custom_key, EnvKey, EnvKnob,
    EnvKnobValue, EnvScope, Flavor, KnobRow, KnobSource, Scope, KNOBS,
};
pub use game::{
    add_manual, add_manual_full, game_dir, game_exists, game_launch_config, game_rel,
    game_row_by_id, list_game_index, list_games, remove_manual, scan_games, scan_games_opts,
    set_hidden_override, set_steam_appid, steam_appid_of, touch_last_played, GameId, GameIndexRow,
    GameLaunchConfig, GameRow,
};
pub use install::{
    apply_copies, foreign_dest, foreign_occupied, game_root, is_prefix_dest, load_conflicts,
    need_manifest, other_claims, plan_copies, plan_dest_copy, prefix_drive_c, prefix_for,
    prefix_rel, prefix_root, remove_copies, remove_dest_copy, tracked_dests, validate_prefix_dests,
    CopyOp, LoadConflict, Removal, PREFIX_DEST_PREFIX,
};
pub use instance::{
    add_mod, add_mod_from, add_mod_from_with_password, apply_package_slot, classify_package,
    classify_package_with_password, disable_mod, enable_mod, export_mod, family_template, list_mods,
    list_templates, mint_recipe, mods_for_game, remove_mod, rescan_mod, rescan_preview,
    resolve_source, scan_default_include, scan_package, set_mod_offered, snap_family_asset,
    Classification, ClassifyKind, Mod, ModList, ModProblem, ModTemplate, PackageFile, PayloadRule,
    Plan, RecipeSpec, SourceRef, TemplateFamily,
};
pub use launch::{
    apply_mod_env, build_launch_spec, game_launch_needs, is_argv_wrapper, LaunchNeeds, LaunchPaths,
    LaunchSpec,
};
pub use metadata::{
    cached_metadata, cached_metadata_for, game_show, is_key_source, search_steam_by_name,
    secret_manager_clear, secret_manager_get, secret_manager_set, secret_manager_test,
    shorten_store_query, MetaInput, MetaLine, MetadataSource, SteamSearchHit, KEY_SOURCES,
    METADATA_SOURCES,
};
pub use mods::{
    check_catalog_update, check_update, effect_names_for, enabled_mod_count,
    ensure_update_baseline, find_mod, has_apply_record, install_instance, is_config_text,
    migrate_mod_cache, mod_cache_rows, payload_config_path, payload_drift, payload_has_files,
    preview_payload_files, push_global_edits, push_global_edits_all, reconcile_mod_cache,
    resync_game, resync_instance, set_file_keep, set_instance_enabled, set_instance_slot,
    set_load_order, short_update_reason, uninstall_instance, Baseline, CatalogStatus, InstallOpts,
    ModCacheRow, PushReport, UpdateStatus,
};
pub use modtype::{
    dest_for, diagnose, fixture_packages, parse_mod_type, parse_slot, slot_dll, type_dest_for,
    Diagnosis, ModPackage, ModType, ProxySlot, MOD_TYPES,
};
pub use plugin::{PluginDesc, PluginEntry, PluginHost, PluginId, FIRST_PARTY};
pub use prewire::{ensure_game_init, is_dll, managed_ini, prewire_game};
pub use provider::heroic::heroic_running;
pub use provider::steam::steam_icon_for_appid;
pub use provider::{GameProvider, GameRecord, GAME_PROVIDERS};
pub use session::{
    add_extra_exe, canonical_exe_key, game_handle, list_extra_exes, migrate_extra_exes,
    mutate_game, remove_extra_exe, rewrite_correlator, session_configured, set_handle,
    sync_handle_sessions, sync_session,
};
pub use stage::{
    check_rel, remove_runtime_dests, remove_staging, runtime_dir, stage_dir, stage_status,
    sync_staging, StageInput, StageLine, StageState,
};
pub use userland::{
    collapse_icon_paths, expand_tilde, host_inventory_from_intended, icons_check_line,
    install_check_lines, install_check_ok, install_check_summary, install_proton_hook,
    install_userland, install_userland_with_home, intended_host_manifest, is_icon_host_path,
    is_required_host_path, load_host_inventory, packaged_prefix, required_host_failures,
    required_host_warning, save_host_inventory, uninstall_proton_hook, uninstall_userland,
    verify_host_install, HostInstallInventory, HostInventoryEntry, HostKind, HostStatus,
    HostVerifyEntry, InstallReport, IntendedHostEntry, RequiredHostWarning, UninstallReport,
    ICON_SIZES,
};
pub use wrapper::{
    find_enabled_wrapper, find_wrapper, game_wrappers, set_wrapper, unset_wrapper, wrapper_defs,
    wrapper_enabled, WrapperDef, WRAPPERS,
};

#[cfg(test)]
#[path = "gui_ids.rs"]
mod gui_ids;

#[cfg(test)]
mod tests {
    use super::gui_ids::GUI_IDS;
    use super::*;
    use std::path::PathBuf;
    use std::sync::Mutex;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    #[tokio::test]
    async fn open_db_and_list_empty() {
        let dir = std::env::temp_dir().join(format!("tuxgt-e02-{}", std::process::id()));
        let _ = tokio::fs::remove_dir_all(&dir).await;
        let pool = open_db(&dir).await.expect("open db");
        let games = list_games(&pool, None, None, None).await.expect("list");
        assert!(games.is_empty());
    }

    #[test]
    fn fluent_help_about() {
        let s = Strings::en_us().expect("catalog");
        assert!(!s.get("cli-about").is_empty());
        assert_ne!(s.get("cli-about"), "cli-about");
        assert_ne!(s.get("plugins-list-empty"), "plugins-list-empty");
        assert_ne!(s.get("plugin-steam-label"), "plugin-steam-label");
        assert_ne!(s.get("gui-title"), "gui-title");
        assert_ne!(s.get("gui-nav-library"), "gui-nav-library");
        assert_ne!(s.get("gui-nav-settings"), "gui-nav-settings");
        assert_ne!(s.get("gui-filter-all"), "gui-filter-all");
        assert_ne!(s.get("gui-game-stub"), "gui-game-stub");
    }

    #[test]
    fn fluent_gui_catalog_resolves() {
        let s = Strings::en_us().expect("catalog");
        // Every id the GUI resolves on the main thread. Failure means the
        // call site echoes the id to the user (APP §9).
        for id in GUI_IDS {
            assert_ne!(s.get(id), *id, "gui id echoes: {id}");
        }
        let mut args = FluentArgs::new();
        args.set("mode", s.get("gui-sort-az"));
        let cur = s.get_args("gui-sort-current", Some(&args));
        assert!(cur.starts_with("Sort: "), "sort current: {cur}");
        assert_ne!(cur, "gui-sort-current");
    }

    #[test]
    fn config_dir_tuxgt_config() {
        let _g = ENV_LOCK.lock().unwrap();
        let prev = std::env::var_os("TUXGT_CONFIG");
        std::env::set_var("TUXGT_CONFIG", "/tmp/tuxgt-e12-config");
        assert_eq!(config_dir(), PathBuf::from("/tmp/tuxgt-e12-config"));
        match prev {
            Some(v) => std::env::set_var("TUXGT_CONFIG", v),
            None => std::env::remove_var("TUXGT_CONFIG"),
        }
    }

    #[test]
    fn debug_log_enabled_env_gate() {
        let _g = ENV_LOCK.lock().unwrap();
        let prev_debug = std::env::var_os("TUXGT_DEBUG");
        let prev_config = std::env::var_os("TUXGT_CONFIG");
        // Empty config dir: no ui.toml, so only the env var decides.
        let cfg = std::env::temp_dir().join(format!("tuxgt-e70-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&cfg);
        std::fs::create_dir_all(&cfg).expect("temp config");
        std::env::set_var("TUXGT_CONFIG", &cfg);
        std::env::set_var("TUXGT_DEBUG", "1");
        assert!(debug_log_enabled());
        std::env::remove_var("TUXGT_DEBUG");
        assert!(!debug_log_enabled());
        match prev_debug {
            Some(v) => std::env::set_var("TUXGT_DEBUG", v),
            None => std::env::remove_var("TUXGT_DEBUG"),
        }
        match prev_config {
            Some(v) => std::env::set_var("TUXGT_CONFIG", v),
            None => std::env::remove_var("TUXGT_CONFIG"),
        }
        let _ = std::fs::remove_dir_all(&cfg);
    }
}
