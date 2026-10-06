mod confirm;
mod rows;

pub(crate) use confirm::*;
pub(crate) use rows::*;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum EnvPage {
    Game,
    Global,
}

#[derive(Clone)]
pub enum Note {
    // T23: no live constructor; kept so the toast map stays four-kind.
    #[allow(dead_code)]
    Info(String),
    Ok(String),
    Warn(String),
    Err(String),
}

#[derive(Clone)]
pub struct PluginRow {
    pub id: String,
    pub label: String,
    pub enabled: bool,
}

/// R14 registry row for the Core Plugins Registries section: one added
/// registry plus its cached manifest state. `error` is the reason the
/// manifest could not be read; rows are then empty rather than guessed.
#[derive(Clone, Debug, Default)]
pub struct RegistryRow {
    pub url: String,
    pub manifest_path: String,
    /// Manifest path inside the registry repo (the GUI adds the default
    /// `registry.toml`), painted so the user can see what was read.
    /// The pinned commit, short. The full sha stays in `remote.toml`.
    pub pinned_short: String,
    pub plugin_count: usize,
    pub error: Option<String>,
}

/// R14 remote plugin row: one manifest entry, joined with installed and
/// enable state from `remote.toml`. Update compares the installed pin and
/// digest against the registry pin, so a moved pin with identical bytes
/// still counts as an update.
#[derive(Clone, Debug, Default)]
pub struct RemotePluginRow {
    pub id: String,
    pub label: String,
    pub description: String,
    pub author: String,
    pub registry_url: String,
    pub pinned_short: String,
    pub installed: bool,
    pub installed_pin_short: Option<String>,
    pub update_available: bool,
    pub enabled: bool,
    /// This row's registry is the one core would install from (the
    /// first registry carrying the id). False on a later registry's
    /// duplicate, which must not offer Install.
    pub install_offered: bool,
}

/// R14 Core Plugins registry consent. Install, update, and remove all
/// write `remote.toml` and the payload tree, so each asks first; the
/// enable toggle is immediate, like every other toggle on the page.
/// `None` = no dialog.
#[derive(Clone, PartialEq, Eq)]
pub enum PendingRegistryConfirm {
    Install {
        id: String,
    },
    /// `from_pin` is what the user has installed today, so the card can
    /// name the version an update would replace.
    Update {
        id: String,
        from_pin: String,
    },
    Remove {
        id: String,
    },
    /// Dropping a registry also drops every payload it installed, so it
    /// names the count it would take with it.
    RemoveRegistry {
        url: String,
        plugin_count: usize,
    },
}
