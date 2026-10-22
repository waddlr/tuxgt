use std::io::{self, IsTerminal};
use std::path::Path;

use super::store_write::with_store_write;
use super::*;

const FILES_MARK: &str = "profile-files:";
const RESTORE_MARK: &str = "profile-restore:";

pub(crate) async fn run(pool: &SqlitePool, dir: &Path, cmd: ProfileCmd) -> CliResult {
    match cmd {
        ProfileCmd::List { id } => list(pool, dir, &id).await,
        ProfileCmd::Use { id, name, yes } => use_profile(pool, dir, &id, &name, yes).await,
        ProfileCmd::Add {
            id,
            name,
            from,
            copy,
        } => add(pool, dir, &id, &name, from.as_deref(), copy).await,
        ProfileCmd::Rename { id, name, new } => rename(pool, dir, &id, &name, &new).await,
        ProfileCmd::Remove { id, name, yes } => remove(pool, dir, &id, &name, yes).await,
    }
}

async fn list(pool: &SqlitePool, dir: &Path, id: &str) -> CliResult {
    let set = ensure_profiles(pool, dir, id).await?;
    for profile in &set.profiles {
        let mark = if profile.id == set.active {
            "active"
        } else {
            ""
        };
        println!("{}\t{mark}", profile.name);
    }
    Ok(())
}

async fn use_profile(pool: &SqlitePool, dir: &Path, id: &str, name: &str, yes: bool) -> CliResult {
    refuse_headless(yes)?;
    let profile_id = profile_id(pool, dir, id, name).await?;
    let mut yes_files = yes;
    let mut yes_restore = yes;
    loop {
        match switch_profile(pool, dir, id, &profile_id, yes_files, yes_restore).await {
            Ok(out) => {
                finish_restore(pool, dir, id, yes, out.restore_deferred).await?;
                println!("{name}");
                return Ok(());
            }
            Err(e) => accept_switch_confirm(&e, &mut yes_files, &mut yes_restore)?,
        }
    }
}

async fn add(
    pool: &SqlitePool,
    dir: &Path,
    id: &str,
    name: &str,
    from: Option<&str>,
    copy: bool,
) -> CliResult {
    if copy != from.is_some() {
        return Err("--copy and --from must be used together".into());
    }
    let profile = if let Some(from) = from {
        create_copy(pool, dir, id, name, from).await?
    } else {
        create_empty(pool, dir, id, name).await?
    };
    println!("{}", profile.name);
    Ok(())
}

async fn rename(pool: &SqlitePool, dir: &Path, id: &str, name: &str, new: &str) -> CliResult {
    let set = rename_profile(pool, dir, id, name, new).await?;
    let profile = set
        .by_name(new)
        .ok_or_else(|| Error::Manifest(format!("unknown profile: {new}")))?;
    println!("{}", profile.name);
    Ok(())
}

async fn remove(pool: &SqlitePool, dir: &Path, id: &str, name: &str, yes: bool) -> CliResult {
    refuse_headless(yes)?;
    let mut yes_files = yes;
    let mut yes_restore = yes;
    loop {
        match delete_profile(pool, dir, id, name, yes_files, yes_restore).await {
            Ok(out) => {
                finish_restore(pool, dir, id, yes, out.restore_deferred).await?;
                println!("{name}");
                return Ok(());
            }
            Err(e) => accept_switch_confirm(&e, &mut yes_files, &mut yes_restore)?,
        }
    }
}

/// Non-interactive use and remove without `--yes` error before any write.
fn refuse_headless(yes: bool) -> CliResult {
    if profile_mutate_allowed(yes, io::stdin().is_terminal()) {
        Ok(())
    } else {
        Err("re-run with --yes".into())
    }
}

fn profile_mutate_allowed(yes: bool, interactive: bool) -> bool {
    yes || interactive
}

fn accept_switch_confirm(err: &Error, yes_files: &mut bool, yes_restore: &mut bool) -> CliResult {
    match err {
        Error::NeedConfirm(msg) if msg.starts_with(FILES_MARK) && !*yes_files => {
            confirm_list("profile-files", &msg_items(msg), false)?;
            *yes_files = true;
            Ok(())
        }
        Error::NeedConfirm(msg) if msg.starts_with(RESTORE_MARK) && !*yes_restore => {
            confirm_prompt(msg, false)?;
            *yes_restore = true;
            Ok(())
        }
        other => Err(other.to_string().into()),
    }
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

async fn profile_id(pool: &SqlitePool, dir: &Path, id: &str, name: &str) -> Result<String, Error> {
    let set = ensure_profiles(pool, dir, id).await?;
    set.by_name(name)
        .map(|p| p.id.clone())
        .ok_or_else(|| Error::Manifest(format!("unknown profile: {name}")))
}

/// Gate a per-game knob, custom env, or wrapper write. `--yes` is not a
/// thing on these commands, so Add-to-Default always prompts.
pub(crate) async fn owned_mutate<F, Fut>(
    pool: &SqlitePool,
    host: &PluginHost,
    dir: &Path,
    id: &str,
    op: F,
) -> CliResult
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = Result<(), Error>>,
{
    confirm_profile_write(pool, id).await?;
    mutate_game(pool, dir, host, id, op()).await?;
    Ok(())
}

pub(crate) async fn confirm_profile_write(pool: &SqlitePool, game: &str) -> CliResult {
    let dir = data_dir();
    loop {
        match ensure_profile_write(pool, &dir, game).await {
            Ok(()) => return Ok(()),
            Err(Error::NeedProfile(msg)) => accept_default(pool, game, &msg).await?,
            Err(e) => return Err(e.into()),
        }
    }
}

pub(crate) async fn on_need_profile(
    pool: &SqlitePool,
    game: &str,
    err: &Error,
) -> Result<bool, Box<dyn std::error::Error>> {
    let Error::NeedProfile(msg) = err else {
        return Ok(false);
    };
    accept_default(pool, game, msg).await?;
    Ok(true)
}

async fn accept_default(pool: &SqlitePool, game: &str, msg: &str) -> CliResult {
    confirm_add_default(msg, io::stdin().is_terminal())?;
    let out = activate_default_for_write(pool, &data_dir(), game).await?;
    finish_restore(pool, &data_dir(), game, false, out.restore_deferred).await?;
    Ok(())
}

/// Add-to-Default always prompts. `--yes` does not accept it, so a
/// non-interactive failure must not tell the user to re-run with that flag.
fn confirm_add_default(msg: &str, interactive: bool) -> CliResult {
    if !interactive {
        return Err(format!("{msg} (needs a terminal)").into());
    }
    confirm_prompt(msg, false)
}

#[cfg(test)]
mod tests {
    use super::profile_mutate_allowed;

    #[test]
    fn profile_headless_use_needs_yes() {
        assert!(!profile_mutate_allowed(false, false));
        assert!(profile_mutate_allowed(true, false));
        assert!(profile_mutate_allowed(false, true));
    }

    #[test]
    fn add_default_headless_does_not_suggest_yes() {
        let err = super::confirm_add_default("Add this to Default?", false).unwrap_err();
        assert_eq!(err.to_string(), "Add this to Default? (needs a terminal)");
    }
}
