use tuxgt_core::{EnvKnob, GameIndexRow, GameRow, HostVerifyEntry};

use super::super::super::library::AwacyFlag;
use super::super::super::*;
use super::super::*;

/// Window data for one open, loaded from persisted prefs through the one
/// load path. Built fresh per open so a Show follows the persisted place
/// (view/last_game) like a fresh boot to that page — never a stale snapshot.
pub(super) struct OpenData {
    pub(super) prefs: Prefs,
    pub(super) nav: Nav,
    pub(super) index: Vec<GameIndexRow>,
    pub(super) selected: Option<String>,
    pub(super) games: Vec<GameRow>,
    pub(super) tiers: HashMap<String, String>,
    pub(super) proton: HashMap<String, ProtonSummary>,
    pub(super) awacy: HashMap<String, AwacyFlag>,
    pub(super) startup_adapter: String,
    pub(super) plugins: Vec<PluginRow>,
    pub(super) instances: Vec<InstanceRow>,
    pub(super) family_templates: Vec<tuxgt_core::ModTemplate>,
    pub(super) knobs: Vec<&'static EnvKnob>,
    pub(super) mod_counts: HashMap<String, usize>,
    pub(super) mods: HashMap<String, Vec<ModRow>>,
    pub(super) applied: HashMap<String, bool>,
    pub(super) handle: HashMap<String, bool>,
    pub(super) session_payload: HashMap<String, bool>,
    pub(super) sys: SysInfo,
    pub(super) host_install: Vec<HostVerifyEntry>,
    pub(super) host_inventory_missing: bool,
}

/// Load one window's data. Every open runs this; the controller retains none
/// of it across a Hide.
pub(super) fn load_open_data(strings: &Strings) -> OpenData {
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
    // R37: the adapter cache starts from the persisted choice through the
    // same derivation the GUI uses, never a literal.
    let startup_adapter =
        crate::gui::game::adapter_cache_value(games.as_ref(), selected.as_deref());
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
    // Settings tab data loads per tab after construction (`load_settings_tab`
    // for a Settings boot, `enter_settings` otherwise), never here.
    let plugins: Vec<PluginRow> = Vec::new();
    let instances: Vec<InstanceRow> = Vec::new();
    let family_templates: Vec<tuxgt_core::ModTemplate> = Vec::new();
    let knobs = load_knobs();
    let mod_counts = load_mod_counts(&index);
    // Mods rows stay empty at open: `game_tab` is always General here, and
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
    let sys = SysInfo::gather(strings);
    // E67: one host-install verify per window open, alongside the other
    // one-shot probes above. Missing HOME → empty report, never a failure.
    // E69: inventory presence rides along so the Settings card can say so
    // without touching the disk on paint.
    let host_install = std::env::var_os("HOME")
        .map(PathBuf::from)
        .map(|home| verify_host_install(&data_dir(), &home))
        .unwrap_or_default();
    let host_inventory_missing = load_host_inventory(&data_dir()).ok().flatten().is_none();
    OpenData {
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
    }
}
