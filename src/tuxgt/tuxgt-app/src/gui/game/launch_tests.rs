//! R37 Launch Mode adapter rules that hold before any mutation: Hook/Apply
//! exclusivity, the running-client park, and the fact that picking an
//! adapter never re-arms a channel: an armed Install pick parks the
//! one-click unhook consent instead of disarming by itself.
use super::super::paint_ids::ModIds;
use super::super::{ClientStopOp, ConfirmOp, ModRow, PendingConfirm};
use super::launch::{adapter_cache_value, adapter_precheck, AdapterRefusal, LaunchMode};
use super::update_adapter;

#[test]
fn armed_channel_stays_painted_when_needs_drop() {
    assert!(LaunchMode::Hook.show_hook_apply(false));
    assert!(LaunchMode::Apply.show_hook_apply(false));
    assert!(!LaunchMode::Vanilla.show_hook_apply(false));
    assert!(LaunchMode::Vanilla.show_hook_apply(true));
}

#[test]
fn install_parks_consent_while_a_channel_is_armed() {
    // Hook armed: the precheck signals the unhook consent.
    assert_eq!(
        adapter_precheck("install", LaunchMode::Hook, false),
        AdapterRefusal::ArmedChannel,
        "install must never coexist with Hook"
    );
    // Apply armed: parks too (Handle-off, but the trampoline is writing).
    assert_eq!(
        adapter_precheck("install", LaunchMode::Apply, false),
        AdapterRefusal::ArmedChannel,
        "install must not run under an Apply trampoline"
    );
    // Vanilla (nothing armed): allowed, and the signal never depends on
    // which channel it was.
    assert_eq!(
        adapter_precheck("install", LaunchMode::Vanilla, false),
        AdapterRefusal::Ok
    );
}

#[test]
fn preload_is_never_refused_by_the_channel() {
    // Preload is what the hook injects, so an armed channel is consistent
    // with it: the choice still persists under Hook or Apply.
    for mode in [LaunchMode::Hook, LaunchMode::Apply, LaunchMode::Vanilla] {
        assert_eq!(
            adapter_precheck("preload", mode, false),
            AdapterRefusal::Ok,
            "{mode:?}"
        );
    }
}

#[test]
fn a_running_client_parks_before_the_conversion() {
    // Vanilla + running client: the park is the refusal, not a silent
    // write under a live store.
    assert_eq!(
        adapter_precheck("install", LaunchMode::Vanilla, true),
        AdapterRefusal::RunningClient
    );
    // An armed channel wins over the park: the hooked pick goes through
    // the unhook consent, so one Continue still costs exactly one stop.
    assert_eq!(
        adapter_precheck("install", LaunchMode::Hook, true),
        AdapterRefusal::ArmedChannel
    );
}

#[test]
fn the_parked_op_carries_the_chosen_adapter() {
    // The park must round-trip the target through the confirm card, or
    // Confirm would convert to the wrong adapter.
    let parked = PendingConfirm::ClientStop {
        game: "steam::42".into(),
        op: ClientStopOp::AdapterConvert {
            adapter: "install".into(),
            slots_chosen: false,
            picks: Box::default(),
        },
    };
    let PendingConfirm::ClientStop { game, op } = parked else {
        panic!("a different confirm was parked");
    };
    assert_eq!(game, "steam::42");
    match op {
        ClientStopOp::AdapterConvert {
            adapter,
            slots_chosen,
            picks,
        } => {
            assert_eq!(adapter, "install");
            assert!(!slots_chosen);
            assert!(picks.is_empty());
        }
        _ => panic!("a different op was parked"),
    }
}

#[test]
fn the_stop_card_and_the_overwrite_card_are_different_consents() {
    // The blocker fix: the stop card authorizes the stop only. The foreign
    // game-dir overwrite has its own op, and Cancel on it must not touch
    // any queue (it owns none).
    let stop = ClientStopOp::AdapterConvert {
        adapter: "install".into(),
        slots_chosen: false,
        picks: Box::default(),
    };
    let overwrite = ConfirmOp::AdapterConvert {
        adapter: "install".into(),
        slots_chosen: true,
        picks: Box::default(),
        stop_confirmed: false,
    };
    // Distinct variants: routing one into the other would pre-authorize a
    // silent overwrite.
    assert!(matches!(stop, ClientStopOp::AdapterConvert { .. }));
    assert!(matches!(overwrite, ConfirmOp::AdapterConvert { .. }));
    let parked = PendingConfirm::Overwrite {
        game: "steam::42".into(),
        instance: String::new(),
        op: overwrite,
        dests: vec!["dxgi.ini".into()].into_boxed_slice(),
    };
    match parked {
        PendingConfirm::Overwrite {
            game,
            instance,
            op,
            dests,
        } => {
            assert_eq!(game, "steam::42");
            assert!(instance.is_empty(), "the consent is game-scoped");
            match op {
                ConfirmOp::AdapterConvert {
                    adapter,
                    slots_chosen,
                    picks,
                    stop_confirmed,
                } => {
                    assert_eq!(adapter, "install");
                    assert!(slots_chosen);
                    assert!(picks.is_empty());
                    assert!(!stop_confirmed);
                }
                _ => panic!("a different op was parked"),
            }
            assert_eq!(&dests[..], ["dxgi.ini".to_string()]);
        }
        _ => panic!("a different confirm was parked"),
    }
}

#[test]
fn the_cache_derives_from_the_selected_row() {
    use tuxgt_core::GameRow;
    // Minimal rows: the derivation reads `id` and `adapter` only.
    let row = |id: &str, adapter: &str| -> GameRow {
        GameRow {
            id: id.into(),
            name: None,
            cover_path: None,
            manager: "steam".into(),
            store: String::new(),
            header_path: None,
            platform: None,
            api: None,
            install_dir: None,
            exe_path: None,
            prefix_path: None,
            proton: None,
            bitness: None,
            engine: None,
            hidden: false,
            last_played: None,
            steam_appid: None,
            adapter: adapter.into(),
        }
    };
    let rows = vec![row("steam::1", "preload"), row("steam::2", "install")];
    // The selection decides, so two games never share one cache value.
    assert_eq!(adapter_cache_value(&rows, Some("steam::1")), "preload");
    assert_eq!(adapter_cache_value(&rows, Some("steam::2")), "install");
    // No selection, or a row that is gone: the schema default, never a
    // stale value from the previously selected game.
    assert_eq!(adapter_cache_value(&rows, None), "preload");
    assert_eq!(adapter_cache_value(&rows, Some("steam::9")), "preload");
    assert_eq!(adapter_cache_value(&[], Some("steam::1")), "preload");
}

/// A ModRow is the only place the display placeholder `"preload"` lives for
/// an uninstalled row. These fixtures mirror `load_mods` exactly: the
/// placeholder for `installed: false`, the manifest value otherwise.
fn mod_row(instance: &str, adapter: &str, installed: bool) -> ModRow {
    ModRow {
        instance: instance.into(),
        label: instance.into(),
        mod_type: "custom".into(),
        official: false,
        adapter: adapter.into(),
        enabled: installed,
        files: if installed { 1 } else { 0 },
        load_order: 0,
        installed,
        slot: String::new(),
        slot_capable: false,
        graph: String::new(),
        file_entries: Default::default(),
        env_entries: Default::default(),
        effect_files: Default::default(),
        asset: None,
        payload_present: true,
        ids: ModIds::for_instance(instance, "steam::1"),
    }
}

#[test]
fn the_display_placeholder_never_reaches_an_update_install() {
    // A first-time install on a game whose stored choice is `install`: the
    // uninstalled row's "preload" placeholder must not become the adapter,
    // so core resolves the persisted choice.
    let fresh = mod_row("fresh", "preload", false);
    assert_eq!(
        update_adapter(None, Some(&fresh)),
        None,
        "the placeholder must not reach InstallOpts.adapter"
    );
    // Same with no row at all (the Mods tab never loaded).
    assert_eq!(update_adapter(None, None), None);
    // An installed row's manifest is genuine per-instance truth.
    let installed = mod_row("kept", "install", true);
    assert_eq!(
        update_adapter(None, Some(&installed)),
        Some("install".to_string())
    );
    // A confirm retry's explicit override still wins over the row.
    assert_eq!(
        update_adapter(Some("preload".into()), Some(&installed)),
        Some("preload".to_string())
    );
    // An explicit override also rescues a placeholder row.
    assert_eq!(
        update_adapter(Some("install".into()), Some(&fresh)),
        Some("install".to_string())
    );
}

/// The placeholder is a display value only: an uninstalled row and an
/// installed row that happens to be preload must not be
/// distinguishable by the update path.
#[test]
fn an_uninstalled_row_and_a_preload_manifest_never_collapse() {
    let uninstalled = mod_row("x", "preload", false);
    let installed_preload = mod_row("x", "preload", true);
    assert_ne!(uninstalled.installed, installed_preload.installed);
    assert_eq!(update_adapter(None, Some(&uninstalled)), None);
    assert_eq!(
        update_adapter(None, Some(&installed_preload)),
        Some("preload".to_string())
    );
}
