use std::path::Path;

use super::super::store_write::with_store_write;
use super::super::*;

/// Finish any deferred store write a completed switch left behind
/// (race/resume fallbacks; the normal path stops before the switch).
pub(super) async fn finish_deferred(
    pool: &SqlitePool,
    dir: &Path,
    id: &str,
    yes: bool,
    restore_deferred: bool,
    rearm_deferred: bool,
) -> CliResult {
    finish_restore(pool, dir, id, yes, restore_deferred).await?;
    finish_rearm(pool, dir, id, yes, rearm_deferred).await
}

async fn finish_restore(
    pool: &SqlitePool,
    dir: &Path,
    id: &str,
    yes: bool,
    restore_deferred: bool,
) -> CliResult {
    if !restore_deferred {
        return Ok(());
    }
    let line = with_store_write(id, yes, true, || async { Ok(restore_launch(dir, id)?) }).await?;
    println!("{line}");
    let host = PluginHost::load()?;
    sync_session(pool, dir, &host, id).await?;
    Ok(())
}

/// Mirror of `finish_restore` for the remembered Apply arm: the switch
/// deferred the re-apply because the store client was running.
async fn finish_rearm(
    pool: &SqlitePool,
    dir: &Path,
    id: &str,
    yes: bool,
    rearm_deferred: bool,
) -> CliResult {
    if !rearm_deferred {
        return Ok(());
    }
    let host = PluginHost::load()?;
    let line = with_store_write(id, yes, true, || async {
        Ok(rearm_remembered_launch(pool, dir, &host, id)
            .await?
            .unwrap_or_default())
    })
    .await?;
    if !line.is_empty() {
        println!("{line}");
    }
    Ok(())
}
