use clap::Subcommand;

#[derive(Subcommand)]
pub(crate) enum ProfileCmd {
    /// List profiles for a game
    List { id: String },
    /// Make a profile active
    Use {
        id: String,
        name: String,
        /// Confirm file changes and the Apply restore without a prompt
        #[arg(long, short = 'y')]
        yes: bool,
    },
    /// Add a profile without switching to it
    Add {
        id: String,
        #[arg(long)]
        name: String,
        /// Profile to copy. Requires --copy.
        #[arg(long)]
        from: Option<String>,
        /// Copy --from. Requires --from. Omit both to start empty.
        #[arg(long)]
        copy: bool,
    },
    /// Rename a profile
    Rename {
        id: String,
        name: String,
        new: String,
    },
    /// Remove a profile. The active one switches to Default first.
    Remove {
        id: String,
        name: String,
        /// Confirm file changes and the Apply restore without a prompt
        #[arg(long, short = 'y')]
        yes: bool,
    },
}
