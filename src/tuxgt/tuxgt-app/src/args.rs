use std::path::PathBuf;

use clap::{Subcommand, ValueEnum};

#[derive(Subcommand)]
pub(crate) enum InstanceCmd {
    /// Install a mod for a game (writes FileManifest)
    Install {
        game: String,
        instance: String,
        /// Launch adapter intent: preload (loader ini) or install (game dir)
        #[arg(long, default_value = "preload")]
        adapter: String,
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
    /// List or toggle per-dest keep for an installed instance
    Files {
        game: String,
        instance: String,
        /// enable or disable one dest
        #[arg(value_enum)]
        action: Option<FileKeep>,
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
    /// Rewrite the claiming Load dest to a proxy slot (dxgi, d3d11, d3d12, winmm, version)
    Slot {
        game: String,
        instance: String,
        /// Proxy slot stem (`winmm` or `winmm.dll`)
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
}

#[derive(Clone, Copy, ValueEnum)]
pub(crate) enum FileKeep {
    Enable,
    Disable,
}

#[derive(Subcommand)]
pub(crate) enum CacheCmd {
    /// Re-fetch cached assets without touching manifests
    Refresh {
        /// Instance id; omit for all cached assets
        instance: Option<String>,
    },
    /// List external unpack tools and availability
    Tools,
}

#[derive(Subcommand)]
pub(crate) enum ModsCmd {
    /// Print the fixture mod graph with requires/conflict diagnostics
    Graph,
    /// List official + user mods, optionally only those for one game
    List {
        /// Game id; filters to recipes applicable to that game
        id: Option<String>,
    },
    /// Add a user mod from a recipe file
    Add { file: PathBuf },
    /// Add a user mod by scanning a local directory or archive
    AddFrom {
        /// ModType name
        #[arg(long = "type")]
        mod_type: String,
        /// Mod id (slug)
        #[arg(long)]
        id: String,
        /// Directory or archive
        #[arg(long)]
        path: PathBuf,
        /// Display label (default: path stem)
        #[arg(long)]
        label: Option<String>,
        /// Required when stdout is not a TTY
        #[arg(long, short = 'y')]
        yes: bool,
    },
    /// Rescan a user mod's local package (does not rewrite manifests)
    Rescan {
        id: String,
        /// Required when stdout is not a TTY
        #[arg(long, short = 'y')]
        yes: bool,
    },
    /// Remove a user mod
    Remove { id: String },
    /// Enable a listed mod (including official)
    Enable { id: String },
    /// Disable a listed mod (including official)
    Disable { id: String },
    /// Export a user mod recipe, optionally bundled with its payload files
    Export {
        /// Mod id
        id: String,
        /// Output file (TOML recipe, or tar.gz with --files)
        #[arg(long)]
        out: PathBuf,
        /// Bundle kept payload files under payload/ in a tar.gz
        #[arg(long)]
        files: bool,
    },
}

#[derive(Subcommand)]
pub(crate) enum EnvCmd {
    /// List knobs (applicable ones with values when a game id is given)
    List { id: Option<String> },
    /// Print the value of one knob for a game
    Get { id: String, knob: String },
    /// Set a knob for a game
    Set {
        id: String,
        knob: String,
        value: Option<String>,
    },
    /// Clear a knob for a game
    Unset { id: String, knob: String },
    /// Enable a stored game knob (unset is a no-op)
    Enable { id: String, knob: String },
    /// Disable a stored game knob (keeps the value; inherit)
    Disable { id: String, knob: String },
    /// App-wide default knobs for games TuxGT injects
    Global {
        #[command(subcommand)]
        cmd: GlobalEnvCmd,
    },
    /// Custom (freeform) per-game env; core rather than a knob
    Custom {
        #[command(subcommand)]
        cmd: CustomCmd,
    },
}

#[derive(Subcommand)]
pub(crate) enum GlobalEnvCmd {
    /// List global knobs
    List,
    /// Set a global knob (defaults enabled)
    Set { knob: String, value: Option<String> },
    /// Clear a global knob
    Unset { knob: String },
    /// Enable a stored global knob
    Enable { knob: String },
    /// Disable a stored global knob (keeps the value)
    Disable { knob: String },
}

#[derive(Subcommand)]
pub(crate) enum WrapperCmd {
    /// List wrappers (all defs, or one game's on/off state)
    List { id: Option<String> },
    /// Enable a wrapper for a game
    Set { id: String, wrapper: String },
    /// Disable a wrapper for a game
    Unset { id: String, wrapper: String },
}

#[derive(Subcommand)]
pub(crate) enum CustomCmd {
    /// List custom env pairs for a game
    List { id: String },
    /// Add or update a custom env pair (KEY=VALUE)
    Add { id: String, pair: String },
    /// Remove a custom env pair
    Remove { id: String, key: String },
}
