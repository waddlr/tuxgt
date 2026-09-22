use std::fs;
use std::path::{Path, PathBuf};

use super::*;
use crate::apply::{
    drop_record, ensure_backup, now_unix, quote_fragment, read_record, write_record, ApplyCtx,
    ApplyFile, ApplyRecord,
};
use crate::{atomic_write, Error, Result};

pub(crate) fn apply_files(
    ctx: &ApplyCtx,
    game_id: &str,
    app: &str,
    files: &[PathBuf],
) -> Result<String> {
    let frag = quote_fragment(&ctx.launcher);
    let mut rec = record_or_new(ctx, game_id)?;
    let mut touched = 0;
    for path in files {
        let text = fs::read_to_string(path)?;
        let recorded = rec.files.iter().find(|f| f.path == path.to_string_lossy());
        let current = steam_get_options(&text, app);
        if current.as_deref().is_some_and(|c| c.contains(&frag)) {
            // Already applied: keep the first-wins record; reconstruct the
            // previous value when the record was lost.
            if recorded.is_none() {
                let previous = current.as_deref().and_then(|c| strip_fragment(c, &frag));
                let backup = ensure_backup(&ctx.data_dir, game_id, path, None).ok();
                record_applied(&mut rec, path, backup, previous);
            }
            continue;
        }
        let (new_text, previous) =
            steam_set_options(&text, app, &compose_options(current.as_deref(), &frag))?;
        let backup = ensure_backup(
            &ctx.data_dir,
            game_id,
            path,
            recorded.and_then(|f| f.backup.as_deref()),
        )?;
        atomic_write(path, new_text.as_bytes())?;
        record_written(&mut rec, path, backup, previous);
        touched += 1;
    }
    write_record(&ctx.data_dir, &rec)?;
    Ok(applied_report(game_id, files.len(), touched))
}

/// Fresh record, or the one a previous Apply left behind.
fn record_or_new(ctx: &ApplyCtx, game_id: &str) -> Result<ApplyRecord> {
    Ok(read_record(&ctx.data_dir, game_id)?.unwrap_or(ApplyRecord {
        game: game_id.into(),
        manager: "steam".into(),
        launcher: ctx.launcher.to_string_lossy().into_owned(),
        applied_at: now_unix(),
        files: Vec::new(),
    }))
}

/// Record a file Apply found already carrying the fragment: first-wins, so
/// a re-apply never overwrites what a first Apply recorded (that is what
/// Restore replays).
fn record_applied(
    rec: &mut ApplyRecord,
    path: &std::path::Path,
    backup: Option<String>,
    previous: Option<String>,
) {
    if rec.files.iter().any(|f| f.path == path.to_string_lossy()) {
        return;
    }
    rec.files.push(ApplyFile {
        path: path.to_string_lossy().into_owned(),
        backup,
        previous,
    });
}

/// Record a file Apply just wrote: an existing entry keeps its first-Apply
/// `previous` and only refreshes the backup path.
fn record_written(
    rec: &mut ApplyRecord,
    path: &std::path::Path,
    backup: String,
    previous: Option<String>,
) {
    match rec
        .files
        .iter_mut()
        .find(|f| f.path == path.to_string_lossy())
    {
        Some(entry) => entry.backup = Some(backup),
        None => rec.files.push(ApplyFile {
            path: path.to_string_lossy().into_owned(),
            backup: Some(backup),
            previous,
        }),
    }
}

fn applied_report(game_id: &str, files: usize, touched: usize) -> String {
    if touched == 0 {
        format!("{game_id} already applied ({files} file(s))")
    } else {
        format!("{game_id} applied ({files} file(s))")
    }
}

/// Same contract as `apply_files`, byte-surgery on binary `shortcuts.vdf`
/// for `steam:standalone:*` rows: fragment composed the same way, every
/// other byte of the file copied verbatim.
pub(crate) fn apply_files_shortcut(
    ctx: &ApplyCtx,
    game_id: &str,
    appid: u32,
    files: &[PathBuf],
) -> Result<String> {
    let frag = quote_fragment(&ctx.launcher);
    // Shortcuts are per-Steam-user, so a user whose `shortcuts.vdf` has no
    // such appid is skipped rather than failed on. That decision is made
    // before the first write, so a second account can no longer abort Apply
    // after the first account's file is already rewritten. A malformed blob
    // is still refused here, and so is the case where no file has the appid
    // at all, which keeps a real miss loud instead of a silent no-op.
    let mut targets: Vec<(&Path, Vec<u8>)> = Vec::new();
    for path in files {
        let bytes = fs::read(path)?;
        if shortcut_contains(&bytes, appid)? {
            targets.push((path.as_path(), bytes));
        } else {
            tracing::info!(
                path = %path.display(),
                appid,
                "shortcuts.vdf has no such shortcut; skipping this Steam user"
            );
        }
    }
    if targets.is_empty() {
        return Err(Error::Apply(format!(
            "no shortcuts.vdf has shortcut {appid}"
        )));
    }
    let mut rec = record_or_new(ctx, game_id)?;
    let mut touched = 0;
    for (path, bytes) in &targets {
        let recorded = rec.files.iter().find(|f| f.path == path.to_string_lossy());
        let current = shortcut_get_options(bytes, appid);
        if current.as_deref().is_some_and(|c| c.contains(&frag)) {
            if recorded.is_none() {
                let previous = current.as_deref().and_then(|c| strip_fragment(c, &frag));
                let backup = ensure_backup(&ctx.data_dir, game_id, path, None).ok();
                record_applied(&mut rec, path, backup, previous);
                write_record(&ctx.data_dir, &rec)?;
            }
            continue;
        }
        let (new_bytes, previous) =
            shortcut_set_options(bytes, appid, &compose_options(current.as_deref(), &frag))?;
        let backup = ensure_backup(
            &ctx.data_dir,
            game_id,
            path,
            recorded.and_then(|f| f.backup.as_deref()),
        )?;
        atomic_write(path, &new_bytes)?;
        record_written(&mut rec, path, backup, previous);
        // Written, then recorded, before moving to the next file: a failure
        // on a later file can no longer leave this write without a restore
        // record.
        write_record(&ctx.data_dir, &rec)?;
        touched += 1;
    }
    Ok(applied_report(game_id, targets.len(), touched))
}

/// Same contract as `restore_files`, on the binary `shortcuts.vdf`.
pub(crate) fn restore_files_shortcut(ctx: &ApplyCtx, game_id: &str, appid: u32) -> Result<String> {
    let rec = read_record(&ctx.data_dir, game_id)?.ok_or_else(|| {
        Error::Apply(format!("{game_id} has no apply record; nothing to restore"))
    })?;
    if rec.manager != "steam" {
        return Err(Error::Apply(format!(
            "{game_id} was applied via {}",
            rec.manager
        )));
    }
    // A record written before standalone rows moved to shortcuts.vdf points
    // at localconfig.vdf. Refuse before any write rather than parse text as
    // binary and fail mid-loop; that line is ours to clean up by hand.
    if rec.files.iter().any(|f| {
        Path::new(&f.path)
            .file_name()
            .is_some_and(|n| n == "localconfig.vdf")
    }) {
        return Err(Error::Apply(format!(
            "{game_id} was applied to localconfig.vdf, which standalone rows \
             no longer use; remove that LaunchOptions line by hand"
        )));
    }
    let mut restored = 0;
    for entry in &rec.files {
        let path = PathBuf::from(&entry.path);
        let Ok(bytes) = fs::read(&path) else {
            tracing::warn!(path = %entry.path, "apply target missing on restore; skipping");
            continue;
        };
        let new_bytes = match &entry.previous {
            Some(prev) => shortcut_set_options(&bytes, appid, prev)?.0,
            None => shortcut_remove_options(&bytes, appid)?,
        };
        if new_bytes != bytes {
            atomic_write(&path, &new_bytes)?;
        }
        restored += 1;
    }
    drop_record(&ctx.data_dir, game_id)?;
    Ok(format!("{game_id} restored ({restored} file(s))"))
}

pub(crate) fn restore_files(ctx: &ApplyCtx, game_id: &str, app: &str) -> Result<String> {
    let rec = read_record(&ctx.data_dir, game_id)?.ok_or_else(|| {
        Error::Apply(format!("{game_id} has no apply record; nothing to restore"))
    })?;
    if rec.manager != "steam" {
        return Err(Error::Apply(format!(
            "{game_id} was applied via {}",
            rec.manager
        )));
    }
    let mut restored = 0;
    for entry in &rec.files {
        let path = PathBuf::from(&entry.path);
        let Ok(text) = fs::read_to_string(&path) else {
            tracing::warn!(path = %entry.path, "apply target missing on restore; skipping");
            continue;
        };
        let new_text = match &entry.previous {
            Some(prev) => {
                let (t, _) = steam_set_options(&text, app, prev)?;
                t
            }
            None => steam_remove_options(&text, app),
        };
        if new_text != text {
            atomic_write(&path, new_text.as_bytes())?;
        }
        restored += 1;
    }
    drop_record(&ctx.data_dir, game_id)?;
    Ok(format!("{game_id} restored ({restored} file(s))"))
}
