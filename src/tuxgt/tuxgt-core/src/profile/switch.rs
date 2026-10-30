//! Park the active profile, swap in the target, journal the game-dir phase.
//! A refused confirm returns before the journal exists.

use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

use crate::{game_manifests, prewire_game, sync_session, Error, PluginHost, Result};

use super::files::{self, channel_needed};
use super::slot;
use super::{game_root_dir, journal_path, load_index, save_index};

pub const FILES_PREFIX: &str = "profile-files: ";
pub const RESTORE_PREFIX: &str = "profile-restore: ";

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SwitchPreview {
    pub file_changes: Vec<String>,
    pub drops_needs: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SwitchOutcome {
    pub active: String,
    pub restore_deferred: bool,
    /// A remembered Update Launch Options arm could not re-apply inside
    /// the switch because the store client runs; the caller finishes it
    /// through the client-stop flow (`rearm_remembered_launch`).
    pub rearm_deferred: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
struct Journal {
    phase: String,
    from: String,
    to: String,
    yes_files: bool,
    yes_restore: bool,
    drops_needs: bool,
    /// Launch Mode arm captured before parking (`hook` | `apply`), so a
    /// resumed switch still knows what its disarm must remember.
    #[serde(default)]
    arm: Option<String>,
}

pub async fn preview_switch(
    pool: &SqlitePool,
    data_dir: &Path,
    game_id: &str,
    target: &str,
) -> Result<SwitchPreview> {
    let set = super::ensure_profiles(pool, data_dir, game_id).await?;
    if set.by_id(target).is_none() {
        return Err(Error::Manifest(format!("unknown profile: {target}")));
    }
    if set.active == target {
        return Ok(SwitchPreview::default());
    }
    diff(pool, data_dir, game_id, &set.active, target).await
}

pub async fn switch_profile(
    pool: &SqlitePool,
    data_dir: &Path,
    game_id: &str,
    target: &str,
    yes_files: bool,
    yes_restore: bool,
) -> Result<SwitchOutcome> {
    let set = super::ensure_profiles(pool, data_dir, game_id).await?;
    if set.by_id(target).is_none() {
        return Err(Error::Manifest(format!("unknown profile: {target}")));
    }
    if set.active == target {
        return Ok(SwitchOutcome {
            active: target.to_string(),
            restore_deferred: false,
            rearm_deferred: false,
        });
    }
    let preview = diff(pool, data_dir, game_id, &set.active, target).await?;
    if !preview.file_changes.is_empty() && !yes_files {
        return Err(Error::NeedConfirm(format!(
            "{FILES_PREFIX}{}",
            preview.file_changes.join(", ")
        )));
    }
    if preview.drops_needs && !yes_restore {
        return Err(Error::NeedConfirm(format!(
            "{RESTORE_PREFIX}Update Launch Options will be restored."
        )));
    }
    crate::session::migrate_handle(pool).await?;
    let journal = Journal {
        phase: "parking".into(),
        from: set.active.clone(),
        to: target.to_string(),
        yes_files,
        yes_restore,
        drops_needs: preview.drops_needs,
        arm: super::arm::current_arm(pool, data_dir, game_id).await?,
    };
    let gdir = game_root_dir(data_dir, game_id)?;
    write_journal(&gdir, &journal)?;
    advance(pool, data_dir, game_id, journal).await
}

pub(crate) async fn resume_if_needed(
    pool: &SqlitePool,
    data_dir: &Path,
    game_id: &str,
) -> Result<()> {
    let gdir = game_root_dir(data_dir, game_id)?;
    let path = journal_path(&gdir);
    if !path.is_file() {
        return Ok(());
    }
    let text = fs::read_to_string(&path)?;
    let journal: Journal = toml::from_str(&text).map_err(|e| Error::Toml(e.to_string()))?;
    advance(pool, data_dir, game_id, journal).await?;
    Ok(())
}

async fn diff(
    pool: &SqlitePool,
    data_dir: &Path,
    game_id: &str,
    active: &str,
    target: &str,
) -> Result<SwitchPreview> {
    let gdir = game_root_dir(data_dir, game_id)?;
    let live_manifests = game_manifests(data_dir, game_id)?;
    let live_knobs = slot::read_live_knobs(pool, game_id).await?;
    let (incoming, incoming_knobs) =
        files::incoming_view(&gdir, active, target, &live_manifests, &live_knobs)?;
    let file_changes = files::changed_dests(&live_manifests, &incoming);
    let drops_needs = channel_needed(&live_manifests, &live_knobs)
        && !channel_needed(&incoming, &incoming_knobs)
        && crate::read_record(data_dir, game_id)?.is_some();
    Ok(SwitchPreview {
        file_changes,
        drops_needs,
    })
}

async fn advance(
    pool: &SqlitePool,
    data_dir: &Path,
    game_id: &str,
    mut journal: Journal,
) -> Result<SwitchOutcome> {
    let gdir = game_root_dir(data_dir, game_id)?;
    loop {
        match journal.phase.as_str() {
            "parking" => {
                let set = load_index(&gdir)?;
                let profile = set
                    .by_id(&journal.from)
                    .cloned()
                    .ok_or_else(|| Error::Manifest("switch source missing".into()))?;
                slot::park_live(pool, &gdir, game_id, &profile).await?;
                journal.phase = "parked".into();
                write_journal(&gdir, &journal)?;
            }
            "parked" => {
                files::remove_outgoing(pool, data_dir, game_id).await?;
                slot::refresh_parked_manifests(&gdir, &journal.from)?;
                let disabled = gdir.join("disabled");
                if disabled.exists() {
                    fs::remove_dir_all(disabled)?;
                }
                journal.phase = "removed".into();
                write_journal(&gdir, &journal)?;
            }
            "removed" => {
                slot::clear_live(&gdir)?;
                journal.phase = "cleared".into();
                write_journal(&gdir, &journal)?;
            }
            "cleared" => {
                slot::load_slot(pool, &gdir, game_id, &journal.to).await?;
                let mut set = load_index(&gdir)?;
                set.active = journal.to.clone();
                save_index(&gdir, &set)?;
                journal.phase = "loaded".into();
                write_journal(&gdir, &journal)?;
            }
            "loaded" => {
                files::apply_incoming(pool, data_dir, game_id).await?;
                journal.phase = "installed".into();
                write_journal(&gdir, &journal)?;
            }
            "installed" => {
                let host = PluginHost::load()?;
                prewire_game(data_dir, pool, game_id).await?;
                sync_session(pool, data_dir, &host, game_id).await?;
                let rearm_deferred = super::arm::settle_launch_memory(
                    pool,
                    data_dir,
                    &host,
                    game_id,
                    &gdir,
                    journal.arm.as_deref(),
                )
                .await;
                let _ = fs::remove_file(journal_path(&gdir));
                let deferred =
                    journal.drops_needs && crate::read_record(data_dir, game_id)?.is_some();
                return Ok(SwitchOutcome {
                    active: journal.to,
                    restore_deferred: deferred,
                    rearm_deferred,
                });
            }
            other => {
                return Err(Error::Manifest(format!("profile switch phase: {other}")));
            }
        }
    }
}

fn write_journal(gdir: &Path, journal: &Journal) -> Result<()> {
    let text = toml::to_string(journal).map_err(|e| Error::Toml(e.to_string()))?;
    crate::atomic_write(&super::journal_path(gdir), text.as_bytes())
}
