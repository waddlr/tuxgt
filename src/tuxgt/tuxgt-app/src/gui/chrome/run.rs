use super::super::*;
use super::*;
use super::super::single_instance::{self, Acquire};

pub fn run(strings: Strings) -> Result<(), Box<dyn std::error::Error>> {
    set_trim_threshold();
    let focus_running = match single_instance::acquire(&data_dir())? {
        Acquire::Primary(primary) => Some(primary.start()),
        Acquire::Existing => return Ok(()),
    };
    let prefs = Prefs::load();
    let nav = match prefs.view.as_str() {
        "game" | "prefix" => Nav::Game,
        "settings" => Nav::Settings,
        _ => Nav::Library,
    };
    // Always-held minimal index; full rows + metadata follow the boot page
    // (Library: all, Game: selected, Settings: neither).
    let index = load_index();
    let selected = prefs
        .last_game
        .clone()
        .filter(|id| index.iter().any(|g| g.id == *id))
        .or_else(|| index.first().map(|g| g.id.clone()));
    let (games, tiers, proton, awacy) = match nav {
        Nav::Library => {
            let games = load_games().unwrap_or_else(|e| {
                tracing::error!(error = %e, "load games");
                Vec::new()
            });
            let tiers = load_tiers();
            let proton = load_proton();
            let awacy = library::load_awacy(&games);
            (games, tiers, proton, awacy)
        }
        Nav::Game => {
            let row = selected.as_deref().and_then(load_game_row);
            let (tiers, proton, awacy) = match (selected.as_ref(), row.as_ref()) {
                (Some(id), Some(g)) => game_metadata_maps(id, load_metadata_for(id, g)),
                _ => Default::default(),
            };
            (row.into_iter().collect::<Vec<_>>(), tiers, proton, awacy)
        }
        Nav::Settings => (Vec::new(), HashMap::new(), HashMap::new(), HashMap::new()),
    };
    // Library holds full rows: the index derives from the same rows so base
    // positions cannot misalign (see `enter_library_data`). A failed full
    // load keeps the slim index (empty base, populated sidebar).
    let index = if nav == Nav::Library && !games.is_empty() {
        games
            .iter()
            .map(tuxgt_core::GameIndexRow::from_row)
            .collect::<Vec<_>>()
    } else {
        index
    };

    let app = gpui_kit::application().with_assets(Assets);
    // Settings tab data loads per tab after construction (`load_settings_tab`
    // for a Settings boot, `enter_settings` otherwise), never here.
    let plugins: Vec<PluginRow> = Vec::new();
    let instances: Vec<InstanceRow> = Vec::new();
    let family_templates: Vec<tuxgt_core::ModTemplate> = Vec::new();
    let knobs = load_knobs();
    let mod_counts = load_mod_counts(&index);
    // Mods rows stay empty at boot: `game_tab` is always General here, and
    // the Mods tab loads them on entry. (The hero counts read `mod_counts`.)
    let mods: HashMap<String, Vec<ModRow>> = HashMap::new();
    let applied = selected
        .as_ref()
        .map(|id| {
            let mut m = HashMap::new();
            m.insert(id.clone(), has_apply_record(&data_dir(), id));
            m
        })
        .unwrap_or_default();
    let handle = selected
        .as_ref()
        .map(|id| {
            let mut m = HashMap::new();
            m.insert(id.clone(), load_handle(id));
            m
        })
        .unwrap_or_default();
    let session_payload = selected
        .as_ref()
        .map(|id| {
            let mut m = HashMap::new();
            m.insert(id.clone(), load_payload(id));
            m
        })
        .unwrap_or_default();
    let sys = SysInfo::gather(&strings);
    // E67: one host-install verify per process start, alongside the other
    // one-shot probes above. Missing HOME → empty report, never a failure.
    // E69: inventory presence rides along so the Settings card can say so
    // without touching the disk on paint.
    let host_install = std::env::var_os("HOME")
        .map(PathBuf::from)
        .map(|home| verify_host_install(&data_dir(), &home))
        .unwrap_or_default();
    let host_inventory_missing = load_host_inventory(&data_dir()).ok().flatten().is_none();
    // Resolved on the main thread before Shell exists: the search/override
    // inputs and the OS window identity take plain text.
    let title = strings.get("gui-title");
    let search_ph = strings.get("gui-search-placeholder");
    let override_ph = strings.get("gui-override-placeholder");
    let custom_ph = strings.get("gui-custom-placeholder");
    let key_ph = strings.get("gui-key-placeholder");
    let appid_ph = strings.get("gui-appid-placeholder");
    let inst_id_ph = strings.get("gui-placeholder-add-name");
    let fam_filter_ph = strings.get("gui-placeholder-family-filter");
    let extras_filter_ph = strings.get("gui-placeholder-reshade-extras-filter");
    let mods_filter_ph = strings.get("gui-placeholder-mods-filter");
    let extra_exe_ph = strings.get("gui-extra-exe-placeholder");
    let cfg_ph = strings.get("gui-placeholder-config");
    let archive_password_ph = strings.get("gui-placeholder-archive-password");

    app.run(move |cx| {
        gpui_kit::init(cx);
        Theme::change(cx.window_appearance(), None, cx);
        theme::apply(cx, None, prefs.theme_id(), prefs.scale());
        cx.set_app_identity("tuxgt", &title);
        let prefs = prefs.clone();
        let index = index.clone();
        let games = games.clone();
        let selected = selected.clone();
        let plugins = plugins.clone();
        let instances = instances.clone();
        let knobs = knobs.clone();
        let mod_counts = mod_counts.clone();
        let mods = mods.clone();
        let applied = applied.clone();
        let handle = handle.clone();
        let session_payload = session_payload.clone();
        let tiers = tiers.clone();
        let proton = proton.clone();
        let awacy = awacy.clone();
        let sys = sys.clone();
        let host_install = host_install.clone();
        cx.spawn(async move |cx| {
            let mut options = TitleBar::window_options();
            options.app_id = Some("tuxgt".into());
            options.window_decorations = Some(WindowDecorations::Client);
            options.window_background = WindowBackgroundAppearance::Transparent;
            options.window_min_size = Some(size(px(960.), px(640.)));
            options.window_bounds = Some(WindowBounds::Windowed(Bounds {
                origin: point(px(48.), px(48.)),
                size: size(px(1280.), px(800.)),
            }));
            let window =
            cx.open_window(options, |window, cx| {
                window.set_window_title(&title);
                theme::apply(cx, Some(window), prefs.theme_id(), prefs.scale());
                let search =
                    cx.new(|cx| InputState::new(window, cx).placeholder(search_ph.clone()));
                let override_input =
                    cx.new(|cx| InputState::new(window, cx).placeholder(override_ph.clone()));
                let custom_env_input =
                    cx.new(|cx| InputState::new(window, cx).placeholder(custom_ph.clone()));
                // E44: the key editor is masked; the AppID editor is plain.
                let secret_input = cx.new(|cx| {
                    InputState::new(window, cx)
                        .placeholder(key_ph.clone())
                        .masked(true)
                });
                let archive_password_input = cx.new(|cx| {
                    InputState::new(window, cx)
                        .placeholder(archive_password_ph.clone())
                        .masked(true)
                });
                let appid_input =
                    cx.new(|cx| InputState::new(window, cx).placeholder(appid_ph.clone()));
                let extra_exe_input =
                    cx.new(|cx| InputState::new(window, cx).placeholder(extra_exe_ph.clone()));
                let instance_id_input =
                    cx.new(|cx| InputState::new(window, cx).placeholder(inst_id_ph.clone()));
                let family_filter_input =
                    cx.new(|cx| InputState::new(window, cx).placeholder(fam_filter_ph.clone()));
                let extras_filter_input =
                    cx.new(|cx| InputState::new(window, cx).placeholder(extras_filter_ph.clone()));
                let packs_filter_input =
                    cx.new(|cx| InputState::new(window, cx).placeholder(mods_filter_ph.clone()));
                let picker_filter_input =
                    cx.new(|cx| InputState::new(window, cx).placeholder(mods_filter_ph.clone()));
                let installed_filter_input =
                    cx.new(|cx| InputState::new(window, cx).placeholder(mods_filter_ph.clone()));
                let config_input = cx.new(|cx| {
                    TextareaState::new(window, cx)
                        .searchable(true)
                        .placeholder(cfg_ph.clone())
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
                        catalog_updates: std::collections::HashSet::new(),
                        mod_update_pending: HashSet::new(),
                        mod_stage: HashMap::new(),
                        mod_conflicts: HashMap::new(),
                        mod_extra_epoch: HashMap::new(),
                        mod_files_open: HashSet::new(),
                        adapter_choice: "preload".to_string(),
                        page_scroll: ScrollHandle::new(),
                        inner_scrolls: Default::default(),
                        about_env_scroll: ScrollHandle::new(),
                        sidebar_scroll: VirtualListScrollHandle::new(),
                        status: String::new(),
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
                Shell::spawn_boot_scan(view.clone(), cx);
                cx.new(|cx| {
                    Root::new(view, window, cx)
                        .bordered(false)
                        .bg(transparent_black())
                })
            })
            .expect("open window");
            if let Some(running) = focus_running {
                cx.spawn(async move |cx| {
                    loop {
                        if let Ok(request) = running.receiver.try_recv() {
                            let activated = window
                                .update(cx, |_, window, _| window.activate_window())
                                .is_ok();
                            let _ = request.ack.send(activated);
                        }
                        cx.background_executor()
                            .timer(std::time::Duration::from_millis(50))
                            .await;
                    }
                })
                .detach();
            }
        })
        .detach();
    });
    Ok(())
}

/// The GUI holds a large, slowly turning working set (art cache, GPU staging,
/// row lists); the default trim threshold lets freed pages sit in the arenas
/// after thread exit. Trim the arena top on every free
/// above 128KB, and disable glibc's dynamic threshold ratchet. No-op off glibc.
#[cfg(target_env = "gnu")]
fn set_trim_threshold() {
    const M_TRIM_THRESHOLD: i32 = -1;
    unsafe extern "C" {
        fn mallopt(param: i32, value: i32) -> i32;
    }
    // SAFETY: glibc's public allocator entry point, no preconditions.
    unsafe {
        mallopt(M_TRIM_THRESHOLD, 128 * 1024);
    }
}

#[cfg(not(target_env = "gnu"))]
fn set_trim_threshold() {}
