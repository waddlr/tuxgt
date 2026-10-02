mod args;
mod cli;
mod gui;
mod log;

use std::path::PathBuf;

use clap::{Parser, Subcommand};
use tracing_subscriber::EnvFilter;
use tuxgt_core::{data_dir, debug_log_enabled, Strings};

pub(crate) use args::{
    CacheCmd, CustomCmd, EnvCmd, ExtraExeCmd, FileAction, FileKeep, GamesCmd, GlobalEnvCmd,
    InstanceCmd, MintCmd, ModsCmd, WrapperCmd,
};

#[derive(Parser)]
#[command(name = "tuxgt", about = "TuxGT — injector / runtime-mod manager")]
pub(crate) struct Cli {
    #[command(subcommand)]
    cmd: Option<Cmd>,
}

#[derive(Subcommand)]
pub(crate) enum Cmd {
    /// Open the desktop window
    Gui,
    /// Serve the tray while hidden (spawned by Hide; a manual start with a
    /// live session yields at once)
    #[command(hide = true)]
    Tray {
        /// Wait for the hiding GUI to release the prefix, then take over
        #[arg(long, hide = true)]
        handoff: bool,
    },
    /// Scan game libraries
    Scan {
        /// Re-run detectors and clear overrides
        #[arg(long)]
        force: bool,
        /// Confirm override wipe without a prompt
        #[arg(long, short = 'y')]
        yes: bool,
    },
    /// Print detection for one game (does not launch)
    Doctor {
        id: String,
        /// Set a detector override (repeatable); `FIELD=` clears it
        #[arg(long, value_name = "FIELD=VALUE")]
        set: Vec<String>,
        /// Clear a detector override (repeatable)
        #[arg(long, value_name = "FIELD")]
        unset: Vec<String>,
        /// Re-run detectors and clear overrides
        #[arg(long)]
        force: bool,
        /// Confirm override wipe without a prompt
        #[arg(long, short = 'y')]
        yes: bool,
    },
    /// Game index commands
    Games {
        #[command(subcommand)]
        cmd: GamesCmd,
    },
    /// Show metadata for one game (badge, art, warning)
    Game {
        #[command(subcommand)]
        cmd: GameCmd,
    },
    /// Metadata sources
    Metadata {
        #[command(subcommand)]
        cmd: MetadataCmd,
    },
    /// Plugin registry
    Plugins {
        #[command(subcommand)]
        cmd: PluginsCmd,
    },
    /// Environment knobs and custom per-game env
    Env {
        #[command(subcommand)]
        cmd: EnvCmd,
    },
    /// Per-game wrappers (gamescope / GameMode / MangoHud)
    Wrapper {
        #[command(subcommand)]
        cmd: WrapperCmd,
    },
    /// Play a game (does not write store config unless Enable & Play / --apply)
    Launch {
        id: String,
        /// Print the constructed command and do not exec
        #[arg(long)]
        print: bool,
        /// Persist the wrapper into Steam/Heroic, then play
        #[arg(long)]
        apply: bool,
        /// Restore the pre-Apply store config and do not play
        #[arg(long)]
        restore: bool,
        /// Confirm Enable & Play and store-client stop/restart
        #[arg(long)]
        yes: bool,
        /// Play unmodded when Enable & Play is available
        #[arg(long)]
        vanilla: bool,
    },
    /// Mod catalog commands
    Mods {
        #[command(subcommand)]
        cmd: ModsCmd,
    },
    /// Per-game instance commands
    Instance {
        #[command(subcommand)]
        cmd: InstanceCmd,
    },
    /// Download cache commands
    Cache {
        #[command(subcommand)]
        cmd: CacheCmd,
    },
    /// Write PATH + KDE desktop symlinks and ~/.config/tuxgt.conf
    Install {
        /// Install prefix (default: $HOME/tuxgt). Skips the tty prompt.
        #[arg(long)]
        prefix: Option<PathBuf>,
        /// Accept the default prefix without a prompt
        #[arg(long, short = 'y')]
        yes: bool,
        /// Verify intended host files against disk: non-ok `path\tstate`
        /// lines, then `ok\t<ok>/<total>`. Exit 0 iff all ok. Writes nothing.
        #[arg(long)]
        check: bool,
    },
    /// Remove tracked host files from a previous `tuxgt install` (PREFIX stays)
    Uninstall {
        /// Remove without a prompt
        #[arg(long, short = 'y')]
        yes: bool,
    },
}

#[derive(Subcommand)]
pub(crate) enum GameCmd {
    /// Print metadata lines (fetches what is missing or stale)
    Show {
        id: String,
        /// Re-fetch even if the cache is fresh
        #[arg(long)]
        refresh: bool,
    },
}

#[derive(Subcommand)]
pub(crate) enum MetadataCmd {
    /// Keyring commands
    Key {
        #[command(subcommand)]
        cmd: KeyCmd,
    },
}

#[derive(Subcommand)]
pub(crate) enum KeyCmd {
    /// Store a key in the system keyring (one line on stdin)
    Set { source: String },
    /// Remove a key from the system keyring
    Clear { source: String },
}

#[derive(Subcommand)]
pub(crate) enum PluginsCmd {
    /// List registered plugins
    List,
    /// Enable a plugin
    Enable { id: String },
    /// Disable a plugin
    Disable { id: String },
    /// Registry catalog: add/list/update/remove a git registry
    Registry {
        #[command(subcommand)]
        cmd: RegistryCmd,
    },
    /// Registry plugins: install/update/remove and remote enable state
    Remote {
        #[command(subcommand)]
        cmd: RemoteCmd,
    },
}

#[derive(Subcommand)]
pub(crate) enum RegistryCmd {
    /// Add a git plugin registry (pins the ref to a commit)
    Add {
        /// Registry git URL, or a local path with --local
        url: String,
        /// Branch, tag, or commit used to discover the pinned commit
        #[arg(long, default_value = "HEAD")]
        git_ref: String,
        /// Registry manifest path inside the repo
        #[arg(long, default_value = "registry.toml")]
        manifest: String,
        /// Allow a non-https local path (development only)
        #[arg(long)]
        local: bool,
    },
    /// List registries with their plugins
    List,
    /// Update a registry pin, or every registry when no url is given
    Update { url: Option<String> },
    /// Remove a registry and the payloads it installed
    Remove { url: String },
    /// Print the payload tree digest a manifest entry needs
    Digest {
        /// Repo-relative payload path in the current repository
        path: String,
    },
}

#[derive(Subcommand)]
pub(crate) enum RemoteCmd {
    /// List installed registry plugins
    List,
    /// Install a plugin at its registry's pinned commit
    Install { id: String },
    /// Update one installed plugin, or every installed plugin
    Update { id: Option<String> },
    /// Remove an installed plugin
    Remove { id: String },
    /// Enable an installed plugin for remote use (remote.toml)
    Enable { id: String },
    /// Disable an installed plugin for remote use (remote.toml)
    Disable { id: String },
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Logs: stderr always (RUST_LOG, else TUXGT_DEBUG=1 / ui.toml debug_log,
    // else info), plus `<data>/logs/tuxgt.log.YYYYMMDD-HHMMSS` (one per run,
    // `-<pid>` on a same-second collision; `tuxgt.log` symlinks the latest,
    // newest 5 kept, non-blocking).
    let debug_on = debug_log_enabled();
    let rust_log = std::env::var("RUST_LOG")
        .ok()
        .filter(|s| !s.trim().is_empty());
    // The builder creates `<data>/logs/` eagerly, so gate it: never for
    // `--check` (verify-only, never writes), and never when the data dir
    // itself does not exist yet (first `install` into the default prefix).
    let strings = Strings::en_us()?;
    let cli = Cli::parse();
    let want_file_log =
        !matches!(cli.cmd, Some(Cmd::Install { check: true, .. })) && data_dir().is_dir();
    let log_dir = data_dir().join("logs");
    let current: Option<(String, std::fs::File)> = want_file_log
        .then(|| log::create_run_log(&log_dir))
        .flatten();
    if let Some((name, _)) = current.as_ref() {
        log::point_current_log(&log_dir, name);
        log::prune_old_logs(&log_dir, name, 5);
    }
    let (current_name, file_appender) = match current {
        Some((name, file)) => (Some(name), Some(file)),
        None => (None, None),
    };
    let (file_writer, _log_guard) = match file_appender {
        Some(appender) => {
            let (writer, guard) = tracing_appender::non_blocking(appender);
            (Some(writer), Some(guard))
        }
        None => (None, None),
    };
    use tracing_subscriber::{
        layer::{Layer, SubscriberExt},
        util::SubscriberInitExt,
    };
    match rust_log {
        Some(filter) => {
            tracing_subscriber::registry()
                .with(log::layer(true).with_writer(std::io::stderr))
                .with(file_writer.map(|w| log::layer(false).with_writer(w)))
                .with(EnvFilter::new(filter))
                .try_init()
                .ok();
        }
        None => {
            tracing_subscriber::registry()
                .with(
                    log::layer(true)
                        .with_writer(std::io::stderr)
                        .with_filter(log::DynamicLevel),
                )
                .with(file_writer.map(|w| {
                    log::layer(false)
                        .with_writer(w)
                        .with_filter(log::DynamicLevel)
                }))
                .try_init()
                .ok();
            if debug_on {
                log::set_debug(true);
            }
        }
    }
    {
        let sys = crate::gui::sys::SysInfo::gather(&strings);
        let data = data_dir();
        let data_s = data.display().to_string();
        match current_name.as_deref() {
            Some(name) => tracing::info!(
                version = env!("CARGO_PKG_VERSION"),
                built = option_env!("VERGEN_BUILD_DATE").unwrap_or("unknown"),
                os = sys.os.as_str(),
                desktop = sys.desktop.as_str(),
                gpu = sys.gpu.as_str(),
                cpu = sys.cpu.as_str(),
                ram = sys.ram.as_str(),
                data_dir = data_s.as_str(),
                log_file = log_dir.join(name).display().to_string().as_str(),
                debug = debug_on,
                "tuxgt boot"
            ),
            None => tracing::info!(
                version = env!("CARGO_PKG_VERSION"),
                built = option_env!("VERGEN_BUILD_DATE").unwrap_or("unknown"),
                os = sys.os.as_str(),
                desktop = sys.desktop.as_str(),
                gpu = sys.gpu.as_str(),
                cpu = sys.cpu.as_str(),
                ram = sys.ram.as_str(),
                data_dir = data_s.as_str(),
                file_log = false,
                debug = debug_on,
                "tuxgt boot"
            ),
        }
    }
    match cli.cmd {
        None | Some(Cmd::Gui) => gui::run(strings),
        Some(Cmd::Tray { handoff }) => gui::run_tray(strings, handoff),
        Some(cmd) => {
            let rt = tokio::runtime::Runtime::new()?;
            rt.block_on(cli::run_cli(cmd, &strings))
        }
    }
}
