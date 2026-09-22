use std::fmt;
use std::path::{Path, PathBuf};

use crate::{Error, Result};

pub(crate) const ALPH: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";

/// Standalone-class store id (B02): one name for Steam non-Steam
/// shortcuts, Heroic sideloads, and manual rows.
pub const STANDALONE: &str = "standalone";

/// Standalone display id for a legacy standalone-class row, if it needs one.
pub(crate) fn standalone_id(manager: &str, store: &str, game: &str) -> Option<String> {
    match (manager, store) {
        ("steam", "shortcut") | ("heroic", "sideload") | ("manual", "") => {
            Some(format!("{manager}:{STANDALONE}:{game}"))
        }
        _ => None,
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct GameId {
    pub manager: String,
    pub store: String,
    pub game: String,
}

impl GameId {
    pub fn new(
        manager: impl Into<String>,
        store: impl Into<String>,
        game: impl Into<String>,
    ) -> Result<Self> {
        let manager = manager.into();
        let store = store.into();
        let game = game.into();
        if manager.is_empty() || game.is_empty() {
            return Err(Error::InvalidGameId(format!("{manager}:{store}:{game}")));
        }
        Ok(Self {
            manager,
            store,
            game,
        })
    }

    pub fn parse(s: &str) -> Result<Self> {
        let (manager, rest) = s
            .split_once(':')
            .ok_or_else(|| Error::InvalidGameId(s.into()))?;
        let (store, game) = rest
            .split_once(':')
            .ok_or_else(|| Error::InvalidGameId(s.into()))?;
        Self::new(manager, store, game)
    }
}

/// Per-game relative dir: `games/<l1>/<l2>/`. Level-1 is `manager` when
/// `store` is empty, else `{manager}_{store}`. Level-2 is the `game` field
/// with `/` and `:` replaced by `_`.
pub fn game_rel(id: &GameId) -> PathBuf {
    let l1 = if id.store.is_empty() {
        id.manager.clone()
    } else {
        format!("{}_{}", id.manager, id.store)
    };
    let l2 = id.game.replace(['/', ':'], "_");
    PathBuf::from(l1).join(l2)
}

/// Per-game dir: `data_dir/games/<rel>`.
pub fn game_dir(data_dir: &Path, id: &GameId) -> PathBuf {
    data_dir.join("games").join(game_rel(id))
}

impl fmt::Display for GameId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}:{}", self.manager, self.store, self.game)
    }
}
