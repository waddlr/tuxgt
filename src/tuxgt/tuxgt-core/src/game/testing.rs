use super::*;
use crate::provider::{GameProvider, GameRecord};
use crate::Result;

pub(crate) fn art_row(cover: Option<&str>, header: Option<&str>) -> GameRow {
    GameRow {
        id: "heroic:gog:1".into(),
        name: None,
        cover_path: cover.map(str::to_string),
        manager: "heroic".into(),
        store: "gog".into(),
        header_path: header.map(str::to_string),
        platform: None,
        api: None,
        install_dir: None,
        exe_path: None,
        prefix_path: None,
        proton: None,
        bitness: None,
        engine: None,
        hidden: false,
        last_played: None,
        steam_appid: None,
    }
}

pub(crate) struct FakeSteam(pub(crate) Vec<GameRecord>);

impl GameProvider for FakeSteam {
    fn plugin_id(&self) -> &'static str {
        "steam"
    }
    fn scan(&self) -> Result<Vec<GameRecord>> {
        Ok(self.0.clone())
    }
}

pub(crate) fn rec(manager: &str, store: &str, game: &str, name: &str) -> GameRecord {
    GameRecord::new(GameId::new(manager, store, game).unwrap(), name)
}
