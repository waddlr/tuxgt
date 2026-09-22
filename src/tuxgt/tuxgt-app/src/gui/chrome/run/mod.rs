mod build;
mod controller;
mod open_data;

use std::sync::Arc;

use super::super::single_instance::{self, Acquire};
use super::super::tray::{self, HideState};
use super::super::*;
use super::*;

use controller::Controller;

/// R12: the one live window, plus a handle to the `Shell` inside it.
///
/// The Shell handle is weak on purpose: a Hide must let the window's
/// entities drop, so the controller may reach a live Shell (to report a
/// veto) but must never keep one alive across a hide.
struct LiveWindow {
    window: WindowHandle<Root>,
    shell: WeakEntity<Shell>,
}

impl LiveWindow {
    /// Bring the window forward, or answer `false` when it is already gone.
    fn activate<C: AppContext>(&self, cx: &mut C) -> bool {
        self.window
            .update(cx, |_, window, _| window.activate_window())
            .is_ok()
    }

    /// Say why a Hide was refused, if the Shell is still there to read it.
    fn report<C: AppContext>(&self, text: String, cx: &mut C) {
        let _ = self.shell.update(cx, |shell, cx| {
            shell.status = text;
            cx.notify();
        });
    }
}

/// Process-level resolved strings: the OS window identity plus every input
/// placeholder, resolved on the main thread before Shell exists.
struct Labels {
    title: String,
    search_ph: String,
    override_ph: String,
    custom_ph: String,
    key_ph: String,
    appid_ph: String,
    inst_id_ph: String,
    fam_filter_ph: String,
    extras_filter_ph: String,
    mods_filter_ph: String,
    extra_exe_ph: String,
    cfg_ph: String,
    archive_password_ph: String,
    registry_url_ph: String,
}

impl Labels {
    /// Resolve the search/override inputs and the OS window identity from
    /// the bundle: they take plain text.
    fn resolve(strings: &Strings) -> Self {
        Self {
            title: strings.get("gui-title"),
            search_ph: strings.get("gui-search-placeholder"),
            override_ph: strings.get("gui-override-placeholder"),
            custom_ph: strings.get("gui-custom-placeholder"),
            key_ph: strings.get("gui-key-placeholder"),
            appid_ph: strings.get("gui-appid-placeholder"),
            inst_id_ph: strings.get("gui-placeholder-add-name"),
            fam_filter_ph: strings.get("gui-placeholder-family-filter"),
            extras_filter_ph: strings.get("gui-placeholder-reshade-extras-filter"),
            mods_filter_ph: strings.get("gui-placeholder-mods-filter"),
            extra_exe_ph: strings.get("gui-extra-exe-placeholder"),
            cfg_ph: strings.get("gui-placeholder-config"),
            archive_password_ph: strings.get("gui-placeholder-archive-password"),
            registry_url_ph: strings.get("gui-action-add-registry"),
        }
    }
}

pub fn run(strings: Strings) -> Result<(), Box<dyn std::error::Error>> {
    set_trim_threshold();
    let focus_running = match single_instance::acquire(&data_dir())? {
        Acquire::Primary(primary) => Some(primary.start()),
        Acquire::Existing => return Ok(()),
    };
    let prefs = Prefs::load();
    // First-window adapter selection reads `ZED_DEVICE_ID`. A stored card is
    // applied as a preference, not a hard exclusion: the platform still walks
    // the rest of the adapter list when the named one cannot present the
    // surface, and the system default sets no filter at all. Applied here,
    // before any window (and any adapter) exists. `Cpu` instead pins
    // software rendering; its vars are restored once the first window owns
    // its renderer (see `drive`), so no child that does GL/Vulkan (games
    // above all) ever renders under them.
    let gpu = prefs.gpu_pref();
    let gpu_env = gpu.apply();
    // Window data (nav, index, rows, metadata, probes) loads per open inside
    // the controller below, so a Hide retains none of it (dormant).

    // R12: the tray owns the session's lifetime, so removing the last window
    // must not end the process — a Hide is a normal outcome. A real Quit is
    // explicit: the tray menu, or a Close with the preference off.
    let app = gpui_kit::application()
        .with_assets(Assets)
        .with_quit_mode(QuitMode::Explicit);
    // Resolved on the main thread before Shell exists: the search/override
    // inputs and the OS window identity take plain text.
    let labels = Labels::resolve(&strings);

    app.run(move |cx| {
        gpui_kit::init(cx);
        Theme::change(cx.window_appearance(), None, cx);
        theme::apply(cx, None, prefs.theme_id(), prefs.scale());
        cx.set_app_identity("tuxgt", &labels.title);
        // R12: one process-level owner for the window. The tray item and
        // every Shell's close/minimize path write to one channel, so a stale
        // callback can only target this loop — never a dropped window, never
        // a second window, and never a second DB writer.
        let hide_state = HideState::new();
        hide_state.set_prefs(prefs.close_to_tray, prefs.minimize_to_tray);
        let (tray_commands, tray_requests) = std::sync::mpsc::channel();
        // Controller-owned recent-games snapshot, shared into the tray item
        // and the window. Rebuilt per process: the stub re-reads it after
        // the handoff.
        let recents: tray::RecentSnapshot = Arc::new(std::sync::RwLock::new(Vec::new()));
        let tray = tray::spawn(
            &strings,
            Arc::clone(&hide_state),
            tray_commands.clone(),
            Arc::clone(&recents),
        );
        let tray_registered = tray.is_some();
        // Held for the process's lifetime: dropping the tray unregisters
        // the item.
        let _tray = tray;
        let controller = Controller::boot(
            strings,
            labels,
            hide_state,
            tray_commands,
            recents,
            tray_registered,
            gpu,
            focus_running,
            tray_requests,
        );
        cx.spawn(async move |cx| controller.drive(cx, gpu_env).await)
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
