use super::super::*;
use tuxgt_core::{game_row_by_id, set_hidden_override};

fn hidden_word(hidden: bool) -> &'static str {
    if hidden {
        "hidden"
    } else {
        "visible"
    }
}

pub(crate) async fn run_hide(pool: &SqlitePool, id: String, clear: bool) -> CliResult {
    let value = if clear { None } else { Some(true) };
    set_hidden_override(pool, &id, value).await?;
    print_hidden(pool, &id).await
}

pub(crate) async fn run_unhide(pool: &SqlitePool, id: String) -> CliResult {
    set_hidden_override(pool, &id, Some(false)).await?;
    print_hidden(pool, &id).await
}

async fn print_hidden(pool: &SqlitePool, id: &str) -> CliResult {
    let row = game_row_by_id(pool, id)
        .await?
        .ok_or_else(|| Error::UnknownGame(id.to_string()))?;
    println!("{id}\t{}", hidden_word(row.hidden));
    tracing::info!(game = id, hidden = row.hidden, "hidden override set");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::hidden_word;

    #[test]
    fn hidden_word_matches_override() {
        assert_eq!(hidden_word(true), "hidden");
        assert_eq!(hidden_word(false), "visible");
    }
}
