use std::cell::RefCell;
use std::sync::Arc;

use super::super::super::settings::AppUpdate;
use super::super::super::*;
use super::super::*;
use super::controller::Ctx;
use super::open_data::{load_open_data, OpenData};
use super::LiveWindow;

/// Build the single window from persisted prefs, like a fresh launch. Each
/// process builds it once; the reopen arms only serve an open that failed.
pub(super) fn build_window(ctx: &Ctx, cx: &mut App) -> Option<LiveWindow> {
    let data = load_open_data(&ctx.strings);
    let OpenData {
        prefs,
        nav,
        index,
        selected,
        games,
        tiers,
        proton,
        awacy,
        startup_adapter,
        plugins,
        instances,
        family_templates,
        knobs,
        mod_counts,
        mods,
        applied,
        handle,
        session_payload,
        sys,
        host_install,
        host_inventory_missing,
    } = data;
    let strings = ctx.strings.clone();
    let hide_state = Arc::clone(&ctx.hide_state);
    let tray_commands = ctx.tray_commands.clone();
    let recents = Arc::clone(&ctx.recents);
    let pending_status = Arc::clone(&ctx.pending_status);
    let tray_registered = ctx.tray_registered;
    let mut options = TitleBar::window_options();
    options.app_id = Some("tuxgt".into());
    options.window_decorations = Some(WindowDecorations::Client);
    options.window_background = WindowBackgroundAppearance::Transparent;
    options.window_min_size = Some(size(px(960.), px(640.)));
    options.window_bounds = Some(WindowBounds::Windowed(Bounds {
        origin: point(px(48.), px(48.)),
        size: size(px(1280.), px(800.)),
    }));
    // The Shell is built inside `open_window`; this carries its
    // weak handle back out so the loop can report into it without
    // holding it alive.
    let shell = RefCell::new(None);
    let window = cx.open_window(options, |window, cx| {
        window.set_window_title(&ctx.labels.title);
        theme::apply(cx, Some(window), prefs.theme_id(), prefs.scale());
        let search =
            cx.new(|cx| InputState::new(window, cx).placeholder(ctx.labels.search_ph.clone()));
        let override_input =
            cx.new(|cx| InputState::new(window, cx).placeholder(ctx.labels.override_ph.clone()));
        let custom_env_input =
            cx.new(|cx| InputState::new(window, cx).placeholder(ctx.labels.custom_ph.clone()));
        // E44: the key editor is masked; the AppID editor is plain.
        let secret_input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder(ctx.labels.key_ph.clone())
                .masked(true)
        });
        let archive_password_input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder(ctx.labels.archive_password_ph.clone())
                .masked(true)
        });
        let appid_input =
            cx.new(|cx| InputState::new(window, cx).placeholder(ctx.labels.appid_ph.clone()));
        let extra_exe_input =
            cx.new(|cx| InputState::new(window, cx).placeholder(ctx.labels.extra_exe_ph.clone()));
        let instance_id_input =
            cx.new(|cx| InputState::new(window, cx).placeholder(ctx.labels.inst_id_ph.clone()));
        let family_filter_input =
            cx.new(|cx| InputState::new(window, cx).placeholder(ctx.labels.fam_filter_ph.clone()));
        let extras_filter_input = cx
            .new(|cx| InputState::new(window, cx).placeholder(ctx.labels.extras_filter_ph.clone()));
        let packs_filter_input =
            cx.new(|cx| InputState::new(window, cx).placeholder(ctx.labels.mods_filter_ph.clone()));
        let picker_filter_input =
            cx.new(|cx| InputState::new(window, cx).placeholder(ctx.labels.mods_filter_ph.clone()));
        let installed_filter_input =
            cx.new(|cx| InputState::new(window, cx).placeholder(ctx.labels.mods_filter_ph.clone()));
        let registry_url_input = cx
            .new(|cx| InputState::new(window, cx).placeholder(ctx.labels.registry_url_ph.clone()));
        let config_input = cx.new(|cx| {
            TextareaState::new(window, cx)
                .searchable(true)
                .placeholder(ctx.labels.cfg_ph.clone())
        });
        let view = cx.new(|cx| {
            cx.subscribe(&search, |this: &mut Shell, input, ev: &InputEvent, cx| {
                if matches!(ev, InputEvent::Change) {
                    this.filters.search = input.read(cx).value().to_string();
                    cx.notify();
                }
            })
            .detach();
            cx.subscribe(
                &family_filter_input,
                |_: &mut Shell, _, ev: &InputEvent, cx| {
                    if matches!(ev, InputEvent::Change) {
                        cx.notify();
                    }
                },
            )
            .detach();
            cx.subscribe(
                &extras_filter_input,
                |_: &mut Shell, _, ev: &InputEvent, cx| {
                    if matches!(ev, InputEvent::Change) {
                        cx.notify();
                    }
                },
            )
            .detach();
            for input in [
                packs_filter_input.clone(),
                picker_filter_input.clone(),
                installed_filter_input.clone(),
            ] {
                cx.subscribe(&input, |_: &mut Shell, _, ev: &InputEvent, cx| {
                    if matches!(ev, InputEvent::Change) {
                        cx.notify();
                    }
                })
                .detach();
            }
            cx.subscribe(
                &registry_url_input,
                |_: &mut Shell, _, _: &InputEvent, cx| {
                    cx.notify();
                },
            )
            .detach();
            cx.subscribe(
                &instance_id_input,
                |_: &mut Shell, _, ev: &InputEvent, cx| {
                    if matches!(ev, InputEvent::Change) {
                        cx.notify();
                    }
                },
            )
            .detach();
            Shell {
                strings,
                nav,
                // R12: process-level tray state, shared with the
                // window controller and the tray item.
                hide_state,
                tray_commands,
                recents,
                tray_registered,
                index: index.into_boxed_slice(),
                base_filtered: Vec::new(),
                game_tab: GameTab::General,
                settings_tab: SettingsTab::from_pref(&prefs.settings_tab),
                mods_tab: SettingsModsTab::from_pref(&prefs.settings_mods_tab),
                library_list: prefs.is_list(),
                grid_metrics: None,
                grid_metrics_pending: None,
                grid_metrics_gen: 0,
                prefs,
                sys,
                host_gpu: host_gpu(),
                host_gpus: host_gpus().into_boxed_slice(),
                plugins: plugins.into_boxed_slice(),
                instances: instances.into_boxed_slice(),
                games: games.into_boxed_slice(),
                tiers,
                proton,
                awacy,
                hover_card: None,
                mod_counts,
                mods,
                applied,
                handle,
                session_payload,
                knob_ids: knob_ids_for(&knobs),
                knobs: knobs.into_boxed_slice(),
                pending_note: None,
                knob_values: HashMap::new(),
                knob_enabled: HashMap::new(),
                global_knobs: HashMap::new(),
                knob_count: 0,
                custom_count: 0,
                game_env_advance: false,
                general_advanced: false,
                settings_env_advance: false,
                custom_env: Box::default(),
                secret_states: HashMap::new(),
                family_templates: family_templates.into_boxed_slice(),
                tools: Box::default(),
                host_install: host_install.into_boxed_slice(),
                host_inventory_missing,
                secret_input,
                secret_editing: false,
                show_steamgriddb_settings: false,
                instance_id_input,
                registry_url_input,
                registries: Box::default(),
                remote_plugins: Box::default(),
                registry_busy: false,
                registry_error: None,
                registry_confirm: None,
                add_form: None,
                archive_password_input,
                pending_archive_password: None,
                family_filter_input,
                extras_filter_input,
                packs_filter_input,
                picker_filter_input,
                installed_filter_input,
                file_preview_cache: HashMap::new(),
                preview_open: HashSet::new(),
                preview_expanded: HashSet::new(),
                preview_errors: HashSet::new(),
                config_edit: None,
                config_nav_pending: None,
                config_input,
                config_external_open: false,
                config_external_level: None,
                family_mint: None,
                extras_mint: None,
                add_sync_queued: false,
                add_dest_inputs: Box::default(),
                add_dest_for: None,
                appid_input,
                wrappers: Box::default(),
                launch_needs: tuxgt_core::LaunchNeeds::default(),
                launch_cfg: None,
                appid_stored: None,
                appid_searching: false,
                appid_searched: false,
                appid_hits: Box::default(),
                extras: Box::default(),
                extra_exe_for: None,
                extra_exe_input,
                detect: Box::default(),
                detect_for: None,
                override_edit: None,
                override_input,
                custom_env_input,
                custom_env_adding: false,
                redetect_confirm: false,
                manual_remove: None,
                pending_confirm: None,
                mods_picker_open: false,
                picker_checked: Vec::new(),
                picker_collapsed: HashSet::new(),
                installed_collapsed: HashSet::new(),
                install_queue: Vec::new(),
                install_hold: None,
                install_current: None,
                install_live: HashMap::new(),
                install_live_seq: 0,
                uninstall_queue: Vec::new(),
                uninstall_current: None,
                uninstall_done: 0,
                mod_updates: HashMap::new(),
                update_last_poll: None,
                update_polling: false,
                update_poll_gen: 0,
                game_attention_settled: HashMap::new(),
                catalog_updates: std::collections::HashSet::new(),
                catalog_meta: CatalogMeta::default(),
                app_update: AppUpdate::Unknown,
                app_update_live: None,
                app_update_live_seq: 0,
                mod_update_pending: HashSet::new(),
                mod_stage: HashMap::new(),
                mod_conflicts: HashMap::new(),
                mod_extra_epoch: HashMap::new(),
                mod_files_open: HashSet::new(),
                // R37: core truth, not a literal — the
                // row is re-read on selection and after every
                // conversion.
                // Cloned per window so the builder closure
                // keeps only shared captures (it runs again when
                // an open failed).
                adapter_choice: startup_adapter.clone(),
                page_scroll: ScrollHandle::new(),
                inner_scrolls: Default::default(),
                about_env_scroll: ScrollHandle::new(),
                sidebar_scroll: VirtualListScrollHandle::new(),
                pack_list_top: std::rc::Rc::new(std::cell::Cell::new(None)),
                pack_viewport_armed: false,
                // A tray Play failure while hidden waits here.
                status: pending_status
                    .lock()
                    .ok()
                    .and_then(|mut pending| pending.take())
                    .unwrap_or_default(),
                notices: NoticeStore::default(),
                sidecar_open: false,
                focus: cx.focus_handle(),
                filters: Filters::default(),
                disabled_managers: load_disabled(),
                selected,
                search,
                hist_back: Vec::new(),
                hist_fwd: Vec::new(),
            }
        });
        // Stored base filter+sort for the boot page. Library
        // recomputes from held rows; Game/Settings rebuild from a
        // transient full read so the sidebar is populated before
        // (and without) the boot scan.
        view.update(cx, |this, _| this.rebuild_base());
        if view.read(cx).selected.is_some() {
            view.update(cx, |this, cx| this.fetch_for_tab(cx));
        }
        view.update(cx, |this, cx| {
            this.reload_secrets(cx);
            if this.nav == Nav::Settings {
                this.persist_settings_tab();
                this.reload_tools(cx);
                this.load_settings_tab(cx);
            }
            // E104: reconcile already ran at open_db; kick the
            // first catalog poll now (background, no page needed).
            this.maybe_poll_updates(cx);
        });
        // R12: the compositor's own close button answers the same
        // decision as the titlebar's Close, so the two paths can
        // never disagree about what Close means. A dropped Shell
        // (a Hide already removed this window) allows the close:
        // there is nothing left to strand.
        window.on_window_should_close(cx, {
            let view = view.downgrade();
            move |window, cx| {
                view.update(cx, |this, cx| this.request_window_close(window, cx))
                    .unwrap_or(true)
            }
        });
        Shell::spawn_boot_scan(view.clone(), cx);
        *shell.borrow_mut() = Some(view.downgrade());
        cx.new(|cx| {
            Root::new(view, window, cx)
                .bordered(false)
                .bg(transparent_black())
        })
    });
    let window = match window {
        Ok(window) => window,
        Err(e) => {
            // No window means no headless retry loop: report the
            // adapter diagnostic the platform produced and stop.
            // A stored choice that fails the open is reverted to
            // the system default so the next start opens instead
            // of failing the same way: the platform keeps the
            // failed adapter choice process-wide, so no
            // same-process retry could recover.
            tracing::error!(error = %e, gpu = ?ctx.gpu, "open TuxGT window");
            if ctx.gpu.revert_on_failure() {
                let mut stored = Prefs::load();
                stored.gpu.clear();
                stored.save();
                tracing::warn!(
                    gpu = ?ctx.gpu,
                    "stored GPU cannot present; reverted to system default"
                );
            }
            cx.quit();
            return None;
        }
    };
    Some(LiveWindow {
        window,
        shell: shell
            .into_inner()
            .expect("open_window ran the build closure"),
    })
}
