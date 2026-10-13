use super::rows::ProvidedRow;

/// An install parked until a `provided` Mod's files land. `target` is
/// the install call to retry; `needs` is the Mod core named not ready
/// (the target itself, or a dep the batch queued ahead of it). The
/// `provided` snapshot refreshes after every pick, so the card's
/// pattern lists stay current while the set is partial or ambiguous.
/// Ready retries the install; Cancel drops the batch (disk unchanged).
#[derive(Clone)]
pub struct PendingProvide {
    pub game: String,
    pub target: String,
    pub needs: String,
    pub label: String,
    pub provided: ProvidedRow,
    pub with_requires: Option<String>,
    pub yes: bool,
    pub slot: Option<String>,
}

/// E34 pending install/enable/uninstall confirm. `None` = no dialog.
/// Confirm retries the stashed op; Cancel drops it (disk unchanged).
#[derive(Clone)]
pub enum ConfirmOp {
    Install {
        with_requires: Option<String>,
        slot: Option<String>,
    },
    /// R32 update reinstall (`--redownload`); retried with `yes` after a
    /// foreign-dest confirm. `None` = the instance's own (or the game's
    /// persisted) adapter.
    Update {
        adapter: Option<String>,
        slot: Option<String>,
    },
    /// `gui.mod-config-edit`: Update after a config-edit confirm; retried
    /// with staging force so confirmed wipes land. `foreign_done` sequences
    /// the two consents: false = config leg (retry keeps `yes=false` so a
    /// foreign NeedConfirm still parks), true = foreign leg (retry `yes`
    /// after the foreign card consented).
    UpdateForce {
        adapter: Option<String>,
        foreign_done: bool,
        slot: Option<String>,
    },
    Enable {
        on: bool,
    },
    /// E91 proxy-slot rewrite; retried with `yes` after a foreign-dest confirm.
    Slot {
        slot: String,
    },
    Uninstall,
    /// E64 per-dest keep; retried with `yes` after a foreign-dest confirm.
    FileKeep {
        dest: String,
        on: bool,
    },
    /// E69 Settings host install; retried with `yes` after a modified-path
    /// confirm. No game or instance: the Settings card renders it.
    HostInstall,
    /// E69 Settings host uninstall; always confirmed, then retried with
    /// `yes`. No game or instance: the Settings card renders it.
    HostUninstall,
    /// R37: an Install-adapter conversion parked on foreign game-dir
    /// dests. Game-scoped (it moves every installed instance), so its
    /// `Overwrite` carry has an empty instance and Cancel only drops the
    /// stash.
    AdapterConvert {
        adapter: String,
        /// The slot card already collected picks. The replay must not ask again.
        slots_chosen: bool,
        /// Install-to-preload picks. Applied only inside the conversion that
        /// can roll them back. Empty when no rename is waiting.
        picks: Box<[SlotPick]>,
        /// The stop card already authorized `stop_for_write`. This overwrite
        /// card does not.
        stop_confirmed: bool,
    },
    /// One-click hooked Install: parked on the armed channel itself, with
    /// empty dests for the unhook consent and re-parked with dests when
    /// the pre-stop validate names foreign overwrites. Game-scoped like
    /// `AdapterConvert`, so Cancel only drops the stash.
    AdapterConvertUnhook {
        adapter: String,
        /// The slot card already collected picks. The replay must not ask again.
        slots_chosen: bool,
        /// Install-to-preload picks, same rule as `AdapterConvert`.
        picks: Box<[SlotPick]>,
    },
    /// Re-sync re-pick parked on a foreign proxy. Confirm retries the picks
    /// with overwrite consent, then resyncs. Cancel only drops the card.
    Resync {
        instance: Option<String>,
        picks: Box<[SlotPick]>,
    },
}

/// Store-client stop authorization for Apply/Restore. Confirm stops the
/// running client, writes, and restarts it; Cancel leaves disk unchanged.
#[derive(Clone)]
pub enum ClientStopOp {
    ApplyMode,
    VanillaMode,
    HookMode,
    EnablePlay {
        hook: bool,
    },
    /// R37: an Install-adapter conversion parked on a running client. The
    /// card's Confirm stops the client, runs the transaction with the
    /// consent, and restarts.
    AdapterConvert {
        adapter: String,
        /// The slot card already collected picks. The replay must not ask again.
        slots_chosen: bool,
        /// Install-to-preload picks applied after the stop, before the
        /// conversion. Empty when the dests are already recorded, or when
        /// the slot card has not been answered yet. Cancel writes nothing.
        picks: Box<[SlotPick]>,
    },
}

/// One missing dependency line on the Requires card: the require string
/// core reports (a Mod type or recipe id), its installable candidates as
/// (id, label), and the picked candidate id.
#[derive(Clone)]
pub struct ReqDep {
    pub req: String,
    pub candidates: Box<[(String, String)]>,
    pub chosen: Option<String>,
}

#[derive(Clone)]
pub enum PendingConfirm {
    /// Core returned `NeedConfirm`: retry the op with `yes=true`.
    Overwrite {
        game: String,
        instance: String,
        op: ConfirmOp,
        dests: Box<[String]>,
    },
    /// Core returned `MissingRequires`: install every chosen dep first,
    /// then the instance. One line per missing require.
    Requires {
        game: String,
        instance: String,
        deps: Box<[ReqDep]>,
    },
    /// E69 Settings TuxGT install/uninstall. No game or instance: the
    /// Settings TuxGT Install card renders it via the E34 `confirm_card`;
    /// Confirm retries the stashed op with `yes=true`, Cancel drops it.
    Host { op: ConfirmOp, paths: Box<[String]> },
    /// Running store client must stop before Apply/Restore writes.
    ClientStop { game: String, op: ClientStopOp },
    /// Core returned `NeedSlotChoice`: one proxy-stem dropdown per listed
    /// instance. Confirm applies the picks, then runs the stashed op.
    SlotChoice {
        game: String,
        op: SlotChoiceOp,
        picks: Box<[SlotPick]>,
    },
}

/// The op a slot-choice card retries after the picks land.
#[derive(Clone)]
pub enum SlotChoiceOp {
    /// R37 adapter conversion; `unhook` replays the one-click hooked
    /// Install (the stop it authorized still stands).
    AdapterConvert {
        adapter: String,
        unhook: bool,
    },
    Install {
        /// The instance the install call named; the pick may belong to a
        /// nested require instead, which installs first.
        target: String,
        with_requires: Option<String>,
    },
    Update {
        adapter: Option<String>,
    },
    UpdateForce {
        adapter: Option<String>,
        foreign_done: bool,
    },
    /// Re-sync found two enabled mods on one proxy stem. `None` is the
    /// whole game; `Some` is the instance the user clicked. The retry
    /// applies the picks, then resyncs without asking again.
    Resync {
        instance: Option<String>,
    },
}

/// One dropdown row on the slot-choice card. `slot` is the token core
/// receives (`<self>` or a proxy stem). `stock` is the filename painted
/// for `<self>` (`ReShade64.dll`). Empty when that name is not known yet.
#[derive(Clone)]
pub struct SlotPick {
    pub instance: String,
    pub slot: String,
    pub stock: String,
}

impl PendingConfirm {
    /// Requires-card radio: the clicked candidate becomes its dep line's
    /// choice for Install required. Other confirms ignore the click.
    pub(crate) fn set_requires_chosen(&mut self, req: &str, id: &str) {
        if let PendingConfirm::Requires { deps, .. } = self {
            if let Some(dep) = deps.iter_mut().find(|d| d.req == req) {
                dep.chosen = Some(id.to_string());
            }
        }
    }
    /// Slot-choice card dropdown: the picked stem becomes its instance
    /// row's choice. Other confirms ignore the click.
    pub(crate) fn set_slot_pick(&mut self, instance: &str, slot: &str) {
        if let PendingConfirm::SlotChoice { picks, .. } = self {
            if let Some(pick) = picks.iter_mut().find(|p| p.instance == instance) {
                pick.slot = slot.to_string();
            }
        }
    }
    pub fn game(&self) -> &str {
        match self {
            PendingConfirm::Overwrite { game, .. } => game,
            PendingConfirm::Requires { game, .. } => game,
            PendingConfirm::ClientStop { game, .. } => game,
            PendingConfirm::SlotChoice { game, .. } => game,
            // Host confirms never paint on a game page.
            PendingConfirm::Host { .. } => "",
        }
    }
}

#[derive(Clone, Default)]
pub struct Filters {
    pub store: Option<String>,
    pub platform: Option<String>,
    pub protondb: Option<String>,
    pub mods_only: bool,
    pub search: String,
    /// SortMode id from gui::library ("", "mods", "protondb", "recent").
    pub sort: String,
    /// Show only AWACY-flagged titles.
    pub awacy_only: bool,
    /// Show effective-hidden games (hidden by default).
    pub show_hidden: bool,
}
