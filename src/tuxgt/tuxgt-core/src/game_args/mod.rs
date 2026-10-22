//! Per-game executable arguments. Not env knobs and not argv wrappers.

mod catalog;
mod compose;
mod harvest;
mod publish;
mod rank;
mod schema;
mod store;
mod suppress;
mod template;

pub use catalog::check_tree;
pub use compose::{
    arg_count, args_channel, list_shipped, manual_tokens, view, ArgMark, ArgView, GameArgsView,
};
pub use harvest::{harvest_protonfixes, protondb_has_launch_field};
pub use publish::{
    clear_custom, disable_arg, discard, enable_arg, save_args, set_arg, set_custom, unset_arg,
};
pub use rank::check_same_rank;
pub use schema::{parse_catalog, parse_fixes, ArgKind, ArgValue, CatalogFile, Trust, ENGINES};
pub use store::{clear_published_sync, migrate_game_args};

#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_edge;
#[cfg(test)]
mod tests_save;
