use super::*;

/// Stop the store client (after the same confirm the GUI ClientStop card
/// uses), run `op`, then restart. `writes` false skips the stop. A restart
/// is always attempted after a stop, including when `op` failed.
pub(crate) async fn with_store_write<T, F, Fut>(
    game: &str,
    yes: bool,
    writes: bool,
    op: F,
) -> Result<T, Box<dyn std::error::Error>>
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = Result<T, Box<dyn std::error::Error>>>,
{
    let stopped = if writes {
        confirm_and_stop(game, yes)?
    } else {
        None
    };
    let result = op().await;
    let restart = match stopped {
        Some(c) => c.restart_detached(),
        None => Ok(String::new()),
    };
    match (result, restart) {
        (Ok(v), Ok(msg)) => {
            if !msg.is_empty() {
                println!("{msg}");
            }
            Ok(v)
        }
        (Ok(v), Err(e)) => {
            tracing::warn!(game, error = %e, "client restart failed");
            eprintln!("client restart failed: {e}");
            Ok(v)
        }
        (Err(e), Ok(msg)) => {
            if !msg.is_empty() {
                println!("{msg}");
            }
            Err(e)
        }
        (Err(e), Err(re)) => {
            Err(format!("client restart failed: {re} (write also failed: {e})").into())
        }
    }
}

fn confirm_and_stop(
    game: &str,
    yes: bool,
) -> Result<Option<StoreClient>, Box<dyn std::error::Error>> {
    let Some(client) = StoreClient::for_game(game) else {
        return Ok(None);
    };
    if !client.running() {
        return Ok(None);
    }
    confirm_prompt(
        &format!(
            "{} is running and must stop before launch options can be written. Quit any running game first — this stops {}, writes, and restarts it.",
            client.name(),
            client.name()
        ),
        yes,
    )?;
    Ok(StoreClient::stop_for_write(game, true)?)
}
