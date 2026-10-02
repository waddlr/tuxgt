use clap::{Subcommand, ValueEnum};

#[derive(Subcommand)]
pub(crate) enum InstanceCmd {
    /// Install a mod for a game (writes FileManifest)
    Install {
        game: String,
        instance: String,
        /// Launch adapter override: preload (loader ini) or install (game
        /// dir). Omitted, the game's persisted choice decides.
        #[arg(long)]
        adapter: Option<String>,
        /// Force re-fetch of the asset
        #[arg(long)]
        redownload: bool,
        /// Name the exact instance satisfying a missing requires entry (type or Mod id)
        #[arg(long)]
        with_requires: Option<String>,
        /// Confirm foreign game-dir overwrites without a prompt
        #[arg(long)]
        yes: bool,
        /// Overwrite user-touched staging from the depot
        #[arg(long)]
        force: bool,
        /// Password for an encrypted archive. Prompted on a terminal when
        /// the archive is encrypted and this is omitted.
        #[arg(long)]
        password: Option<String>,
        /// `<self>` or a proxy stem (`winmm` or `winmm.dll`) for a
        /// slot-configurable Install-adapter install. Without it the
        /// install parks with the instances that need a name.
        #[arg(long)]
        slot: Option<String>,
    },
    /// Enable an installed instance
    Enable {
        game: String,
        instance: String,
        /// Confirm foreign game-dir overwrites (install adapter) without a prompt
        #[arg(long)]
        yes: bool,
    },
    /// Disable an installed instance
    Disable { game: String, instance: String },
    /// Remove an installed instance (game-dir revert, staging + manifest drop)
    Uninstall {
        game: String,
        instance: String,
        /// Confirm game-dir removal without a prompt
        #[arg(long)]
        yes: bool,
    },
    /// Print per-file staging sync state for a game
    Status { game: String },
    /// List or toggle per-dest keep / LoadDLL for an installed instance
    Files {
        game: String,
        instance: String,
        /// enable, disable, loaddll, or include one dest
        #[arg(value_enum)]
        action: Option<FileAction>,
        /// Exact stored dest string
        dest: Option<String>,
        /// Confirm foreign game-dir overwrites (install adapter) without a prompt
        #[arg(long)]
        yes: bool,
    },
    /// List or toggle per-key env keep for an installed instance
    Env {
        game: String,
        instance: String,
        /// enable or disable one env key
        #[arg(value_enum)]
        action: Option<FileKeep>,
        /// Exact stored env key
        key: Option<String>,
    },
    /// Rewrite the claiming Load dest to `<self>` or a proxy slot
    /// (dxgi, d3d9, d3d10, d3d11, d3d12, winmm, version)
    Slot {
        game: String,
        instance: String,
        /// `<self>` or a proxy stem (`winmm` or `winmm.dll`)
        slot: String,
        /// Confirm foreign game-dir overwrites (install adapter) without a prompt
        #[arg(long)]
        yes: bool,
    },
    /// Rewrite the per-game installed-mod order (first loses, last wins)
    Order {
        game: String,
        /// Full ordered instance list, first to last
        instances: Vec<String>,
    },
    /// List contested dests with rivals in load order (winner last)
    Conflicts { game: String },
    /// Repair provenance/cache, then print update status (read-only after repair)
    Check {
        game: String,
        /// Instance id; omit to check every installed instance
        instance: Option<String>,
        /// Confirm a repair install that would overwrite a foreign dest
        #[arg(long)]
        yes: bool,
        /// Proxy slot when a repair install needs one
        #[arg(long)]
        slot: Option<String>,
    },
    /// Check, then redownload on confirmation when an update is available
    Update {
        game: String,
        /// Instance id; omit to update every installed instance
        instance: Option<String>,
        /// Confirm the redownload (and any foreign dest overwrite)
        #[arg(long)]
        yes: bool,
        /// Proxy slot when the redownload needs one
        #[arg(long)]
        slot: Option<String>,
        /// Password for an encrypted archive
        #[arg(long)]
        password: Option<String>,
    },
    /// Re-copy depot sources into staging (GUI Force re-sync)
    Resync {
        game: String,
        /// Instance id; omit to re-sync every installed instance
        instance: Option<String>,
        /// Re-sync despite a shared proxy dest, without re-picking slots
        #[arg(long)]
        yes: bool,
    },
}

#[derive(Clone, Copy, ValueEnum)]
pub(crate) enum FileKeep {
    Enable,
    Disable,
}

#[derive(Clone, Copy, ValueEnum)]
pub(crate) enum FileAction {
    Enable,
    Disable,
    Loaddll,
    Include,
}
