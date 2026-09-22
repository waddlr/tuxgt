use super::*;
use crate::apply::ApplyCtx;
use crate::game::GameId;
use crate::provider::{GameProvider, GameRecord};
use crate::{Error, Result};

pub struct HeroicProvider;

impl GameProvider for HeroicProvider {
    fn plugin_id(&self) -> &'static str {
        "heroic"
    }

    fn scan(&self) -> Result<Vec<GameRecord>> {
        Ok(scan_roots(&default_roots()))
    }

    fn apply(&self, ctx: &ApplyCtx, game_id: &str) -> Result<String> {
        let gid = GameId::parse(game_id)?;
        if gid.manager != "heroic" {
            return Err(Error::ApplyUnsupported("heroic".into()));
        }
        heroic_apply(ctx, game_id, &gid.game)
    }

    fn restore(&self, ctx: &ApplyCtx, game_id: &str) -> Result<String> {
        let gid = GameId::parse(game_id)?;
        if gid.manager != "heroic" {
            return Err(Error::ApplyUnsupported("heroic".into()));
        }
        heroic_restore(ctx, game_id, &gid.game)
    }
}
