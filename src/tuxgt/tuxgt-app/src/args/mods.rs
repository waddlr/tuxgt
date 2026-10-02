use std::path::PathBuf;

use clap::Subcommand;

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
    Add {
        file: PathBuf,
        /// Password for an encrypted archive. Prompted on a terminal when
        /// the archive is encrypted and this is omitted.
        #[arg(long)]
        password: Option<String>,
    },
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
        /// Password for an encrypted archive. Prompted on a terminal when
        /// the archive is encrypted and this is omitted.
        #[arg(long)]
        password: Option<String>,
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
    /// Copy user-supplied files into a provided mod
    Provide {
        /// Mod id
        id: String,
        /// File, directory, or archive to copy from
        #[arg(long)]
        path: PathBuf,
        /// Password for an encrypted archive. Prompted on a terminal when
        /// the archive is encrypted and this is omitted.
        #[arg(long)]
        password: Option<String>,
    },
    /// Mint a user mod from a family template or ReShade extras list
    Mint {
        #[command(subcommand)]
        cmd: MintCmd,
    },
}

#[derive(Subcommand)]
pub(crate) enum MintCmd {
    /// List live HDR-family assets (RenoDX / Luma)
    ListFamily,
    /// List live ReShade extras packages
    ListReshade,
    /// Mint one HDR-family asset bound to a library game
    Family {
        /// Family template id (`family-renodx`, `family-luma`, …)
        #[arg(long)]
        template: String,
        /// Release asset file name
        #[arg(long)]
        asset: String,
        /// Library game id
        #[arg(long)]
        game: String,
    },
    /// Mint one ReShade extras package bound to a library game
    Reshade {
        /// PackageName from EffectPackages.ini / Addons.ini
        #[arg(long)]
        package: String,
        /// Library game id (bitness picks 32 vs 64)
        #[arg(long)]
        game: String,
        /// `effect` or `addon` when the name is listed in both
        #[arg(long)]
        kind: Option<String>,
    },
}
