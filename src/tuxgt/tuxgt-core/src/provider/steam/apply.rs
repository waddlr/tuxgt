use std::fs;
use std::path::PathBuf;

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
    let mut rec = read_record(&ctx.data_dir, game_id)?.unwrap_or(ApplyRecord {
        game: game_id.into(),
        manager: "steam".into(),
        launcher: ctx.launcher.to_string_lossy().into_owned(),
        applied_at: now_unix(),
        files: Vec::new(),
    });
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
                rec.files.push(ApplyFile {
                    path: path.to_string_lossy().into_owned(),
                    backup,
                    previous,
                });
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
        match rec
            .files
            .iter_mut()
            .find(|f| f.path == path.to_string_lossy())
        {
            Some(entry) => {
                entry.backup = Some(backup);
            }
            None => rec.files.push(ApplyFile {
                path: path.to_string_lossy().into_owned(),
                backup: Some(backup),
                previous,
            }),
        }
        touched += 1;
    }
    write_record(&ctx.data_dir, &rec)?;
    if touched == 0 {
        Ok(format!(
            "{game_id} already applied ({} file(s))",
            files.len()
        ))
    } else {
        Ok(format!("{game_id} applied ({} file(s))", files.len()))
    }
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
