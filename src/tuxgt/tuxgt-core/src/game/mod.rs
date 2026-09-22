mod adapter;
mod appid;
mod id;
mod index;
mod manual;
mod migrate;
mod row;

pub(crate) use id::*;
pub(crate) use index::*;
pub(crate) use migrate::*;

pub(crate) use adapter::restore_game_adapter;
pub use adapter::{game_adapter, is_install, is_preload, set_game_adapter, validate_adapter};
pub use adapter::{ADAPTER_INSTALL, ADAPTER_PRELOAD};
pub use appid::{set_steam_appid, steam_appid_of};
pub use id::{game_dir, game_rel, GameId, STANDALONE};
pub use index::{
    game_exists, game_launch_config, game_row_by_id, list_game_index, list_games, recent_games,
    scan_games, scan_games_opts, scan_with, scan_with_opts, set_hidden_override, GameLaunchConfig,
};
pub use manual::{
    add_manual, add_manual_full, existing_path, id8, id8_prefixed, remove_manual, touch_last_played,
};
pub use row::{GameIndexRow, GameRow};

#[cfg(test)]
mod testing;
#[cfg(test)]
mod tests_0;
#[cfg(test)]
mod tests_1;
#[cfg(test)]
mod tests_2;
