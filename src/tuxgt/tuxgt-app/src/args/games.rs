use std::path::PathBuf;

use clap::Subcommand;

#[derive(Subcommand)]
pub(crate) enum GamesCmd {
    /// List indexed games
    List {
        #[arg(long)]
        manager: Option<String>,
        #[arg(long)]
        store: Option<String>,
        #[arg(long)]
        query: Option<String>,
        /// Show only hidden games
        #[arg(long)]
        hidden: bool,
        /// Include hidden games (list hides them by default)
        #[arg(long)]
        all: bool,
    },
    /// Add a manual game by executable path
    Add { exe: PathBuf },
    /// Remove a manual game by id (manual rows only)
    Remove { id: String },
    /// Search the Steam Store for a game name (prints `appid<TAB>name` lines)
    AppidSearch { name: String },
    /// Print or set a game's Steam AppID overlay
    Appid {
        id: String,
        /// Steam AppID to store (1-10 digits)
        appid: Option<String>,
        /// Clear the overlay
        #[arg(long)]
        clear: bool,
    },
    /// Per-game handle (protonfixes/trampoline inject). Default off.
    Handle {
        id: String,
        /// Turn handle on
        #[arg(long)]
        on: bool,
        /// Turn handle off
        #[arg(long)]
        off: bool,
        /// Confirm stopping Steam/Heroic to write launch options
        #[arg(long)]
        yes: bool,
    },
    /// Extra correlator exes for one game (same prefix, another exe → same session)
    ExtraExe {
        id: String,
        #[command(subcommand)]
        cmd: ExtraExeCmd,
    },
    /// Hide a game in the library list (per-game override)
    Hide {
        id: String,
        /// Drop the override and use the store's detected hidden flag
        #[arg(long)]
        clear: bool,
    },
    /// Force a game visible in the library list
    Unhide { id: String },
    /// Print or convert the persisted Install adapter (`preload` or `install`)
    Adapter {
        id: String,
        /// `preload` or `install`; omit to print the stored choice
        adapter: Option<String>,
        /// Confirm foreign game-dir overwrites without a prompt
        #[arg(long)]
        yes: bool,
        /// `<self>` or a proxy stem for every slot-configurable instance
        #[arg(long)]
        slot: Option<String>,
    },
}

#[derive(Subcommand)]
pub(crate) enum ExtraExeCmd {
    /// Add one extra exe key, then refresh the session + correlator
    Add { exe: String },
    /// Remove one extra exe key, then refresh the session + correlator
    Remove { exe: String },
    /// List stored extra exe keys for the game
    List,
}
