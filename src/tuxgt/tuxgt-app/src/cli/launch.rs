use std::path::Path;

use super::*;

pub(crate) async fn run(
    pool: &SqlitePool,
    dir: &Path,
    id: String,
    print: bool,
    apply: bool,
    restore: bool,
) -> CliResult {
    if apply && restore {
        return Err("--apply and --restore are exclusive".into());
    }
    if restore {
        refuse_if_client_running(&id)?;
        println!("{}", restore_launch(dir, &id)?);
        tracing::info!(game = id.as_str(), "launch restored");
        return Ok(());
    }
    let host = PluginHost::load()?;
    if apply {
        refuse_if_client_running(&id)?;
        println!("{}", apply_launch(pool, dir, &host, &id).await?);
    }
    let paths = LaunchPaths::detect()?;
    let spec = build_launch_spec(pool, &host, &id, &paths, dir).await?;
    if print {
        println!("{}", spec.display());
        return Ok(());
    }
    let _ = touch_last_played(pool, &id).await;
    tracing::debug!(game = id.as_str(), "spawning");
    if spec.owned && has_harvestable(dir, &id)? {
        let status = spec.command().status()?;
        tracing::info!(game = id.as_str(), "spawned");
        let roots = harvest_roots(pool, dir, &id).await;
        if roots.is_empty() {
            eprintln!("harvest skipped for {id}");
        } else {
            match harvest_game_roots(dir, &id, &roots) {
                Ok(files) => {
                    for f in files {
                        println!("harvest\t{}", f.display());
                    }
                }
                Err(e) => eprintln!("harvest failed for {id}: {e}"),
            }
        }
        std::process::exit(status.code().unwrap_or(1));
    } else {
        let err = {
            use std::os::unix::process::CommandExt;
            spec.command().exec()
        };
        return Err(err.into());
    }
}

/// A running client discards external launch-option writes on its next
/// flush, so the CLI refuses instead of silently losing the Apply. The GUI
/// offers a stop-write-restart confirm; the CLI never stops processes.
pub(crate) fn refuse_if_client_running(id: &str) -> CliResult {
    if let Some(client) = StoreClient::for_game(id) {
        if client.running() {
            return Err(format!(
                "{} is running: quit {} and retry (a running client discards external writes)",
                client.name(),
                client.name()
            )
            .into());
        }
    }
    Ok(())
}

fn has_harvestable(data: &std::path::Path, game: &str) -> Result<bool, Box<dyn std::error::Error>> {
    for m in game_manifests(data, game)? {
        if !m.generated_globs.is_empty() {
            return Ok(true);
        }
        let dests: Vec<&str> = m.files.iter().map(|f| f.dest.as_str()).collect();
        if !generated_globs_for(&m.mod_type, &dests).is_empty() {
            return Ok(true);
        }
    }
    Ok(false)
}
