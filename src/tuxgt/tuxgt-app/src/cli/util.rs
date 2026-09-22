use std::io::{self, BufRead, IsTerminal, Write};

use super::*;

pub(crate) type CliResult = Result<(), Box<dyn std::error::Error>>;

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
