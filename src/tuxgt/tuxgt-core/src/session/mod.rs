mod extra;
mod handle;
mod sync;

pub(crate) use extra::*;
pub(crate) use handle::*;

pub use extra::{
    add_extra_exe, list_extra_exes, migrate_extra_exes, remove_extra_exe, rewrite_correlator,
};
pub use handle::migrate_handle;
pub use handle::{canonical_exe_key, game_handle, set_handle};
pub use sync::{mutate_game, session_configured, sync_handle_sessions, sync_session};

#[cfg(test)]
mod testing;
#[cfg(test)]
mod tests_0;
#[cfg(test)]
mod tests_1;
#[cfg(test)]
mod tests_2;
