mod cache;
mod doctor;
mod env;
mod games;
mod install;
mod instance;
mod launch;
mod mods;
mod scan;
mod util;
mod wrapper;

use tuxgt_core::{
    add_extra_exe, add_manual, add_mod, add_mod_from, all_tools, apply_launch, build_launch_spec,
    collapse_icon_paths, config_dir, custom_env, data_dir, diagnose, disable_global_knob,
    disable_knob, disable_mod, doctor, effective_platform, enable_global_knob, enable_knob,
    enable_mod, enabled_knobs, expand_tilde, find_enabled_knob, find_enabled_wrapper,
    fixture_packages, game_exists, game_handle, game_manifests, game_show, game_wrappers,
    generated_globs_for, global_knobs, harvest_all, harvest_game_roots, harvest_roots,
    has_apply_record,
    include_covers, install_check_lines, install_check_ok, install_check_summary, install_instance,
    install_userland, is_dll, is_key_source, is_required_dest, knob_is_unmanaged, knob_rows,
    knob_source, list_extra_exes, list_games, list_mods, live_knob_value, load_conflicts,
    load_host_inventory, mods_for_game, mutate_game, open_db, packaged_prefix, read_manifest,
    remove_custom, remove_extra_exe, remove_manual, remove_mod, rescan_mod, resolve_value,
    restore_launch, scan_games_opts, scope_applies, scopes_display, search_steam_by_name,
    secret_manager_clear, secret_manager_set, set_custom, set_file_keep, set_global_knob,
    set_handle, set_instance_enabled, set_instance_slot, set_knob, set_load_order,
    set_mod_env_enabled, set_override, set_steam_appid, set_wrapper, stage_status, steam_appid_of,
    sync_handle_sessions, touch_last_played, uninstall_instance, uninstall_userland,
    unset_global_knob, unset_knob, unset_wrapper, validate_custom_key, validate_override,
    verify_host_install, wrapper_defs, DetectOpts, EnvKnob, Error, FileManifest, GameRow,
    InstallOpts, LaunchPaths, PluginHost, SqlitePool, StageState, StoreClient, Strings,
};

use crate::{
    CacheCmd, Cmd, CustomCmd, EnvCmd, ExtraExeCmd, FileKeep, GameCmd, GamesCmd, GlobalEnvCmd,
    InstanceCmd, KeyCmd, MetadataCmd, ModsCmd, PluginsCmd, WrapperCmd,
};

pub(crate) use games::print_games;
pub(crate) use mods::find_mod;
pub(crate) use util::{check_manifest_instance, confirm_list, game_row, note_staging, CliResult};

pub async fn run_cli(cmd: Cmd, strings: &Strings) -> CliResult {
    tracing::debug!(cmd = ?std::env::args().collect::<Vec<_>>(), "cli entry");
    match cmd {
        Cmd::Gui => unreachable!("gui is handled before the CLI runtime"),
        Cmd::Install { prefix, yes, check } => install::run(prefix, yes, check),
        Cmd::Uninstall { yes } => install::uninstall(yes),
        other => {
            let dir = data_dir();
            let pool = open_db(&dir).await?;
            match other {
                Cmd::Gui | Cmd::Install { .. } | Cmd::Uninstall { .. } => unreachable!(),
                Cmd::Scan { force, yes } => scan::run(&pool, &dir, strings, force, yes).await,
                Cmd::Doctor {
                    id,
                    set,
                    unset,
                    force,
                    yes,
                } => doctor::run(&pool, &dir, id, set, unset, force, yes).await,
                Cmd::Games { cmd } => games::run_games(&pool, &dir, strings, cmd).await,
                Cmd::Game { cmd } => games::run_game(&pool, cmd).await,
                Cmd::Metadata { cmd } => games::run_metadata(cmd),
                Cmd::Plugins { cmd } => games::run_plugins(cmd, strings),
                Cmd::Launch {
                    id,
                    print,
                    apply,
                    restore,
                } => launch::run(&pool, &dir, id, print, apply, restore).await,
                Cmd::Mods { cmd } => mods::run(&pool, &dir, strings, cmd).await,
                Cmd::Cache { cmd } => cache::run(cmd).await,
                Cmd::Instance { cmd } => instance::run(&pool, cmd).await,
                Cmd::Env { cmd } => {
                    let host = PluginHost::load()?;
                    env::run(&pool, &host, strings, &dir, cmd).await
                }
                Cmd::Wrapper { cmd } => {
                    let host = PluginHost::load()?;
                    wrapper::run(&pool, &host, &dir, cmd).await
                }
            }
        }
    }
}
