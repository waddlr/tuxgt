mod games;
mod instance;
mod mods;

use clap::Subcommand;

pub(crate) use games::{ExtraExeCmd, GamesCmd};
pub(crate) use instance::{FileAction, FileKeep, InstanceCmd};
pub(crate) use mods::{CacheCmd, MintCmd, ModsCmd};

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
