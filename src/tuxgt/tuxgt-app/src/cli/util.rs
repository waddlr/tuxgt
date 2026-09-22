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
    use super::with_archive_password;
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
    fn with_archive_password_other_error_propagates() {
        let err = with_archive_password(None, |_| Err::<(), _>(Error::UnknownGame("g".into())))
            .unwrap_err();
        assert!(matches!(
            err.downcast_ref::<Error>(),
            Some(Error::UnknownGame(_))
        ));
    }
}
