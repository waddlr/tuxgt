mod appid;
mod id;
mod index;
mod manual;
mod migrate;
mod row;

pub(crate) use id::*;
pub(crate) use index::*;
pub(crate) use migrate::*;

pub use appid::{set_steam_appid, steam_appid_of};
pub use id::{game_dir, game_rel, GameId, STANDALONE};
pub use index::{
    game_exists, game_launch_config, game_row_by_id, list_game_index, list_games, scan_games,
    scan_games_opts, scan_with, scan_with_opts, set_hidden_override, GameLaunchConfig,
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
