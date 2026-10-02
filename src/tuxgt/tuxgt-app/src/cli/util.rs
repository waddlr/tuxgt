use std::io::{self, BufRead, IsTerminal, Write};
use std::process::{Command, Stdio};

use super::*;

pub(crate) type CliResult = Result<(), Box<dyn std::error::Error>>;

/// Restores terminal echo if this process turned it off to read a password.
struct HiddenEcho;

impl HiddenEcho {
    fn suppress() -> Option<Self> {
        let ok = Command::new("stty")
            .arg("-echo")
            .stdin(Stdio::inherit())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|status| status.success())
            .unwrap_or(false);
        ok.then_some(Self)
    }
}

impl Drop for HiddenEcho {
    fn drop(&mut self) {
        let _ = Command::new("stty")
            .arg("echo")
            .stdin(Stdio::inherit())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
}

/// Read one archive password from the terminal. An empty line cancels.
/// A non-terminal stdin reports that `--password` is required.
pub(crate) fn read_archive_password() -> Result<String, Box<dyn std::error::Error>> {
    if !io::stdin().is_terminal() {
        return Err("archive password required; re-run with --password".into());
    }
    eprint!("archive password: ");
    io::stderr().flush()?;
    let _hidden = HiddenEcho::suppress();
    let mut line = String::new();
    io::stdin().lock().read_line(&mut line)?;
    eprintln!();
    let password = line.trim_end_matches(['\r', '\n']).to_string();
    if password.is_empty() {
        return Err("archive password required".into());
    }
    Ok(password)
}

/// Run `op` with the CLI password. A `--password` that does not unlock the
/// archive fails immediately. Otherwise a terminal is prompted until the
/// archive opens or the line is empty.
pub(crate) fn with_archive_password<T>(
    password: Option<String>,
    mut op: impl FnMut(Option<&str>) -> Result<T, Error>,
) -> Result<T, Box<dyn std::error::Error>> {
    let from_flag = password.is_some();
    let mut current = password;
    loop {
        match op(current.as_deref()) {
            Ok(value) => return Ok(value),
            Err(Error::ArchivePasswordRequired) if from_flag => {
                return Err("archive password required".into());
            }
            Err(Error::ArchivePasswordRequired) => {
                current = Some(read_archive_password()?);
            }
            Err(err) => return Err(err.into()),
        }
    }
}

/// Dest / instance names after the first `:` in a NeedConfirm / NeedSlotChoice
/// message (`need-slot: a, b`).
pub(crate) fn msg_items(msg: &str) -> Vec<String> {
    msg.split_once(':')
        .map(|(_, rest)| rest)
        .unwrap_or(msg)
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

/// Format the GUI install auto-Apply outcome for stdout / stderr.
/// `None` means the helper skipped (already applied, hook-legal, client running).
pub(crate) fn auto_apply_line(
    game: &str,
    result: Result<Option<String>, Error>,
) -> Option<Result<String, String>> {
    match result {
        Ok(Some(report)) => Some(Ok(format!("{game}\tapply\t{report}"))),
        Ok(None) => None,
        Err(e) => {
            tracing::warn!(
                game,
                error = %e,
                "install adapter apply failed"
            );
            Some(Err(format!("apply failed for {game}: {e}")))
        }
    }
}

pub(crate) async fn note_auto_apply(pool: &SqlitePool, game: &str) {
    let Ok(host) = PluginHost::load() else {
        return;
    };
    match auto_apply_line(
        game,
        apply_when_hook_illegal(pool, &data_dir(), &host, game).await,
    ) {
        Some(Ok(line)) => println!("{line}"),
        Some(Err(line)) => eprintln!("{line}"),
        None => {}
    }
}

/// `true` on y/yes. `--yes` (`assume_yes`) skips the TTY. Non-TTY without
/// `--yes` errors instead of hanging.
pub(crate) fn ask_yes(prompt: &str, assume_yes: bool) -> Result<bool, Box<dyn std::error::Error>> {
    if assume_yes {
        return Ok(true);
    }
    if !io::stdin().is_terminal() {
        return Err(format!("{prompt} (re-run with --yes)").into());
    }
    eprint!("{prompt} [y/N] ");
    io::stderr().flush()?;
    let mut line = String::new();
    io::stdin().lock().read_line(&mut line)?;
    let answer = line.trim();
    Ok(answer.eq_ignore_ascii_case("y") || answer.eq_ignore_ascii_case("yes"))
}

pub(crate) fn confirm_prompt(prompt: &str, yes: bool) -> CliResult {
    if ask_yes(prompt, yes)? {
        Ok(())
    } else {
        Err("aborted".into())
    }
}

pub(crate) fn confirm_list(prompt: &str, items: &[String], yes: bool) -> CliResult {
    if items.is_empty() || yes {
        return Ok(());
    }
    if !io::stdin().is_terminal() {
        return Err(format!("{prompt}: {} (re-run with --yes)", items.join(", ")).into());
    }
    eprintln!("{prompt}: {}", items.join(", "));
    eprint!("proceed? [y/N] ");
    io::stderr().flush()?;
    let mut line = String::new();
    io::stdin().lock().read_line(&mut line)?;
    let answer = line.trim();
    if answer.eq_ignore_ascii_case("y") || answer.eq_ignore_ascii_case("yes") {
        Ok(())
    } else {
        Err("aborted".into())
    }
}

pub(crate) fn note_staging(data: &std::path::Path, game: &str) {
    let Ok(lines) = stage_status(data, game) else {
        return;
    };
    let bad = lines
        .iter()
        .filter(|l| !matches!(l.state, StageState::InSync | StageState::Unmanaged))
        .count();
    if bad > 0 {
        eprintln!("staging out of sync for {game}: {bad} file(s); see `tuxgt mods status {game}`");
    }
}

pub(crate) fn check_manifest_instance(game: &str, instance: &str) -> CliResult {
    read_manifest(&data_dir(), game, instance)?
        .ok_or_else(|| format!("no manifest for {game} {instance}"))?;
    Ok(())
}

pub(crate) async fn game_row(pool: &SqlitePool, game_id: &str) -> Result<GameRow, Error> {
    list_games(pool, None, None, None)
        .await?
        .into_iter()
        .find(|g| g.id == game_id)
        .ok_or_else(|| Error::UnknownGame(game_id.into()))
}

#[cfg(test)]
mod tests {
    use super::{ask_yes, auto_apply_line, msg_items, with_archive_password};
    use std::cell::Cell;
    use tuxgt_core::Error;

    #[test]
    fn with_archive_password_passes_value_through() {
        let calls = Cell::new(0);
        let value = with_archive_password(None, |password| {
            calls.set(calls.get() + 1);
            assert_eq!(password, None);
            Ok::<_, Error>(7)
        })
        .unwrap();
        assert_eq!((value, calls.get()), (7, 1));
    }

    #[test]
    fn with_archive_password_flag_rejection_fails_immediately() {
        let calls = Cell::new(0);
        let err = with_archive_password(Some("wrong".to_string()), |password| {
            calls.set(calls.get() + 1);
            assert_eq!(password, Some("wrong"));
            Err::<(), _>(Error::ArchivePasswordRequired)
        })
        .unwrap_err();
        assert_eq!(calls.get(), 1);
        assert_eq!(err.to_string(), "archive password required");
    }

    #[test]
    fn msg_items_splits_after_colon() {
        assert_eq!(
            msg_items("need-slot: optiscaler, reshade"),
            vec!["optiscaler", "reshade"]
        );
        assert_eq!(
            msg_items("foreign game-dir dests: dxgi.dll"),
            vec!["dxgi.dll"]
        );
        assert_eq!(msg_items("need-slot:"), Vec::<String>::new());
    }

    #[test]
    fn ask_yes_assume_skips_tty() {
        assert!(ask_yes("unused", true).unwrap());
    }

    #[test]
    fn auto_apply_line_skip_print_fail() {
        assert!(auto_apply_line("g", Ok(None)).is_none());
        assert_eq!(
            auto_apply_line("g", Ok(Some("applied".into()))),
            Some(Ok("g\tapply\tapplied".into()))
        );
        let err = auto_apply_line("g", Err(Error::UnknownGame("g".into())))
            .unwrap()
            .unwrap_err();
        assert!(err.starts_with("apply failed for g:"));
    }

    #[test]
    fn with_archive_password_other_error_propagates() {
        let err = with_archive_password(None, |_| Err::<(), _>(Error::UnknownGame("g".into())))
            .unwrap_err();
        assert!(matches!(
            err.downcast_ref::<Error>(),
            Some(Error::UnknownGame(_))
        ));
    }
}
