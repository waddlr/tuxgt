//! Working-copy edits and Save / Discard. Save is the only store write.

use std::path::Path;

use sqlx::SqlitePool;

use super::compose::view;
use super::schema::{ArgKind, CatalogArg};
use super::store::{self, Snapshot};
use super::suppress::suppressed_for;
use crate::apply::{apply_ctx, apply_launch, read_record, restore_launch};
use crate::game::game_row_by_id;
use crate::session::set_handle;
use crate::{Error, PluginHost, Result};

pub async fn set_arg(
    pool: &SqlitePool,
    data_dir: &Path,
    host: &PluginHost,
    game_id: &str,
    arg: &str,
    value: Option<&str>,
) -> Result<()> {
    let arg = arg.to_string();
    let value = value.map(|s| s.to_string());
    crate::mutate_game(pool, data_dir, host, game_id, async {
        set_inner(pool, data_dir, game_id, &arg, value.as_deref()).await
    })
    .await
}

async fn set_inner(
    pool: &SqlitePool,
    data_dir: &Path,
    game_id: &str,
    arg: &str,
    value: Option<&str>,
) -> Result<()> {
    let (def, suppressed) = require_arg(pool, data_dir, game_id, arg).await?;
    if let Some(s) = suppressed {
        return Err(Error::ArgSuppressed(s));
    }
    let stored = match def.kind {
        ArgKind::Flag => {
            if value.is_some_and(|v| !v.is_empty()) {
                return Err(Error::InvalidArgValue(format!("{} is a flag", def.id)));
            }
            String::new()
        }
        ArgKind::Enum => {
            let v = value.unwrap_or("");
            if !def.values.iter().any(|e| e.value == v) {
                return Err(Error::InvalidArgValue(def.id.clone()));
            }
            v.to_string()
        }
        ArgKind::Freeform => {
            let v = value.unwrap_or("").trim();
            if v.is_empty() {
                return Err(Error::InvalidArgValue(def.id.clone()));
            }
            v.to_string()
        }
    };
    silence_group(pool, data_dir, game_id, &def).await?;
    store::upsert_arg(pool, game_id, &def.id, &stored, true).await
}

pub async fn enable_arg(
    pool: &SqlitePool,
    data_dir: &Path,
    host: &PluginHost,
    game_id: &str,
    arg: &str,
) -> Result<()> {
    let arg = arg.to_string();
    crate::mutate_game(pool, data_dir, host, game_id, async {
        enable_inner(pool, data_dir, game_id, &arg).await
    })
    .await
}

async fn enable_inner(pool: &SqlitePool, data_dir: &Path, game_id: &str, arg: &str) -> Result<()> {
    let (def, suppressed) = require_arg(pool, data_dir, game_id, arg).await?;
    if let Some(s) = suppressed {
        return Err(Error::ArgSuppressed(s));
    }
    let existing = store::stored_args(pool, game_id)
        .await?
        .into_iter()
        .find(|s| s.arg == def.id);
    let value = match def.kind {
        ArgKind::Flag => String::new(),
        ArgKind::Enum | ArgKind::Freeform => {
            let v = existing.map(|s| s.value).unwrap_or_default();
            if v.is_empty()
                || (def.kind == ArgKind::Enum && !def.values.iter().any(|e| e.value == v))
            {
                return Err(Error::InvalidArgValue(def.id.clone()));
            }
            v
        }
    };
    silence_group(pool, data_dir, game_id, &def).await?;
    store::upsert_arg(pool, game_id, &def.id, &value, true).await
}

pub async fn disable_arg(
    pool: &SqlitePool,
    data_dir: &Path,
    host: &PluginHost,
    game_id: &str,
    arg: &str,
) -> Result<()> {
    let arg = arg.to_string();
    crate::mutate_game(pool, data_dir, host, game_id, async {
        let (def, _) = require_arg(pool, data_dir, game_id, &arg).await?;
        let existing = store::stored_args(pool, game_id)
            .await?
            .into_iter()
            .find(|s| s.arg == def.id);
        let value = existing.map(|s| s.value).unwrap_or_default();
        store::upsert_arg(pool, game_id, &def.id, &value, false).await
    })
    .await
}

pub async fn unset_arg(
    pool: &SqlitePool,
    data_dir: &Path,
    host: &PluginHost,
    game_id: &str,
    arg: &str,
) -> Result<()> {
    let arg = arg.to_string();
    crate::mutate_game(pool, data_dir, host, game_id, async {
        let (def, _) = require_arg(pool, data_dir, game_id, &arg).await?;
        if !store::delete_arg(pool, game_id, &def.id).await? {
            return Err(Error::Args(format!("arg not set: {}", def.id)));
        }
        Ok(())
    })
    .await
}

pub async fn set_custom(
    pool: &SqlitePool,
    data_dir: &Path,
    host: &PluginHost,
    game_id: &str,
    args: &str,
) -> Result<()> {
    let args = args.to_string();
    crate::mutate_game(pool, data_dir, host, game_id, async {
        let row = game_row_by_id(pool, game_id)
            .await?
            .ok_or_else(|| Error::UnknownGame(game_id.into()))?;
        let trimmed = args.trim();
        if trimmed.is_empty() {
            return Err(Error::Args("custom arguments are empty after trim".into()));
        }
        let suppressed = suppressed_for(&row, data_dir);
        let tokens = super::compose::custom_tokens(trimmed, &[])?;
        for tok in &tokens {
            if let Some(s) = suppressed
                .iter()
                .find(|s| s.stem == super::schema::stem_of(tok))
            {
                return Err(Error::ArgSuppressed(s.appended.clone()));
            }
        }
        store::set_custom_text(pool, game_id, trimmed).await
    })
    .await
}

pub async fn clear_custom(
    pool: &SqlitePool,
    data_dir: &Path,
    host: &PluginHost,
    game_id: &str,
) -> Result<()> {
    crate::mutate_game(pool, data_dir, host, game_id, async {
        if game_row_by_id(pool, game_id).await?.is_none() {
            return Err(Error::UnknownGame(game_id.into()));
        }
        store::clear_custom_text(pool, game_id).await
    })
    .await
}

pub async fn discard(
    pool: &SqlitePool,
    data_dir: &Path,
    host: &PluginHost,
    game_id: &str,
) -> Result<()> {
    crate::mutate_game(pool, data_dir, host, game_id, async {
        let row = game_row_by_id(pool, game_id)
            .await?
            .ok_or_else(|| Error::UnknownGame(game_id.into()))?;
        if row.manager != "steam" && row.manager != "heroic" {
            return Err(Error::ArgsManualSave);
        }
        match store::published(pool, game_id).await? {
            Some(p) => store::replace_working(pool, game_id, &p.snapshot).await,
            None => store::clear_working(pool, game_id).await,
        }
    })
    .await
}

/// Stop-write-restart stays with the caller. This writes the store, then the snapshot.
pub async fn save_args(
    pool: &SqlitePool,
    data_dir: &Path,
    host: &PluginHost,
    game_id: &str,
) -> Result<String> {
    let row = game_row_by_id(pool, game_id)
        .await?
        .ok_or_else(|| Error::UnknownGame(game_id.into()))?;
    if row.manager != "steam" && row.manager != "heroic" {
        return Err(Error::ArgsManualSave);
    }
    let current = view(pool, data_dir, game_id).await?;
    if !current.dirty {
        return Ok(format!("{game_id} already saved"));
    }
    let tail = current.tail.clone();
    let prior = store::published(pool, game_id)
        .await?
        .map(|p| p.tail)
        .unwrap_or_default();
    set_handle(pool, data_dir, host, game_id, false).await?;
    // Disarm inside set_handle restores when nothing needs a channel. Do not arm Apply after that.
    let needs = crate::launch::game_launch_needs(pool, data_dir, game_id).await?;
    let ctx = apply_ctx(data_dir, game_id)?;
    let report = if !needs.channel_needed() {
        if read_record(data_dir, game_id)?.is_some() {
            // The disarm inside set_handle left the record standing: the
            // auto-restore deferred (owner running) or failed. This
            // explicit Save finishes the teardown or errors. Publishing
            // the tail here would recompose the trampoline and then mark
            // the row clean over a live record.
            if let Some(client) = crate::StoreClient::for_game(game_id) {
                if client.running() {
                    return Err(Error::Apply(format!(
                        "{} is running; nothing written — retry for the stop prompt",
                        client.name()
                    )));
                }
            }
            restore_launch(data_dir, game_id)?;
        }
        format!("{game_id} arguments saved")
    } else {
        if read_record(data_dir, game_id)?.is_none() {
            apply_launch(pool, data_dir, host, game_id).await?;
        }
        publish_store(&ctx, &row.manager, game_id, &tail, &prior)?
    };
    let snap = Snapshot {
        args: store::stored_args(pool, game_id).await?,
        custom: store::custom_text(pool, game_id).await?,
    };
    store::upsert_published(pool, game_id, &tail, &snap).await?;
    Ok(report)
}

fn publish_store(
    ctx: &crate::apply::ApplyCtx,
    manager: &str,
    game_id: &str,
    tail: &str,
    prior_tail: &str,
) -> Result<String> {
    if manager == "steam" {
        crate::provider::steam::publish_tail(ctx, game_id, tail, prior_tail)
    } else {
        let gid = crate::game::GameId::parse(game_id)?;
        crate::provider::heroic::publish_launcher_args(ctx, game_id, &gid.game, tail)
    }
}

async fn silence_group(
    pool: &SqlitePool,
    data_dir: &Path,
    game_id: &str,
    def: &CatalogArg,
) -> Result<()> {
    let Some(group) = &def.group else {
        return Ok(());
    };
    let v = view(pool, data_dir, game_id).await?;
    for row in v.rows {
        if row.id == def.id || row.group.as_deref() != Some(group.as_str()) {
            continue;
        }
        if row.enabled
            || store::stored_args(pool, game_id)
                .await?
                .iter()
                .any(|s| s.arg == row.id)
        {
            store::upsert_arg(pool, game_id, &row.id, &row.value, false).await?;
        }
    }
    Ok(())
}

async fn require_arg(
    pool: &SqlitePool,
    data_dir: &Path,
    game_id: &str,
    arg: &str,
) -> Result<(CatalogArg, Option<String>)> {
    let v = view(pool, data_dir, game_id).await?;
    let row = v
        .rows
        .iter()
        .find(|r| r.id == arg)
        .ok_or_else(|| Error::UnknownArg(arg.into()))?;
    let files = super::catalog::load_catalog(&super::catalog::catalog_root(data_dir));
    let def = files
        .iter()
        .flat_map(|f| f.args.iter())
        .find(|a| a.id == arg && a.template == row.template)
        .cloned()
        .ok_or_else(|| Error::UnknownArg(arg.into()))?;
    let suppressed = if row.mark == super::compose::ArgMark::Suppressed {
        row.appended.clone()
    } else {
        None
    };
    Ok((def, suppressed))
}
