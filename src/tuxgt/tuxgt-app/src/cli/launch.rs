use std::io::IsTerminal;
use std::path::Path;

use super::store_write::with_store_write;
use super::*;
use tuxgt_core::{game_launch_needs, proton_ge_cachy};

pub(crate) async fn run(
    pool: &SqlitePool,
    dir: &Path,
    id: String,
    print: bool,
    apply: bool,
    restore: bool,
    yes: bool,
    vanilla: bool,
) -> CliResult {
    if apply && restore {
        return Err("--apply and --restore are exclusive".into());
    }
    if vanilla && (apply || restore) {
        return Err("--vanilla cannot combine with --apply or --restore".into());
    }
    if restore {
        let line =
            with_store_write(&id, yes, true, || async { Ok(restore_launch(dir, &id)?) }).await?;
        println!("{line}");
        tracing::info!(game = id.as_str(), "launch restored");
        return Ok(());
    }
    let host = PluginHost::load()?;
    if apply {
        let line = with_store_write(&id, yes, true, || async {
            Ok(apply_launch(pool, dir, &host, &id).await?)
        })
        .await?;
        println!("{line}");
        return play_game(pool, dir, &host, &id, print).await;
    }
    if print {
        return play_game(pool, dir, &host, &id, true).await;
    }
    let row = game_row(pool, &id).await?;
    let client = row.manager == "steam" || row.manager == "heroic";
    let handled = game_handle(pool, &id).await?;
    let applied = has_apply_record(dir, &id);
    let needs = game_launch_needs(pool, dir, &id).await?;
    let enable_play = client && !handled && !applied && needs.channel_needed();
    if enable_play {
        return enable_or_vanilla(pool, dir, &host, &row, needs, yes, vanilla).await;
    }
    play_game(pool, dir, &host, &id, false).await
}

async fn enable_or_vanilla(
    pool: &SqlitePool,
    dir: &Path,
    host: &PluginHost,
    row: &GameRow,
    needs: tuxgt_core::LaunchNeeds,
    yes: bool,
    vanilla: bool,
) -> CliResult {
    let id = row.id.as_str();
    if vanilla {
        return play_game(pool, dir, host, id, false).await;
    }
    if !yes && !std::io::stdin().is_terminal() {
        return Err(
            "Enable & Play available; pass --yes to arm and play, or --vanilla to play unmodded"
                .into(),
        );
    }
    if ask_yes("Enable & Play (arm Hook or Apply, then launch)?", yes)? {
        return enable_and_play(pool, dir, host, row, needs, yes).await;
    }
    if ask_yes("Play vanilla (not hooked)?", false)? {
        play_game(pool, dir, host, id, false).await
    } else {
        Ok(())
    }
}

async fn enable_and_play(
    pool: &SqlitePool,
    dir: &Path,
    host: &PluginHost,
    row: &GameRow,
    needs: tuxgt_core::LaunchNeeds,
    yes: bool,
) -> CliResult {
    let id = row.id.as_str();
    let hook = needs.hook_preferred(proton_ge_cachy(row.proton.as_deref()));
    let writes = !hook || has_apply_record(dir, id);
    let report = with_store_write(id, yes, writes, || async {
        if hook {
            set_handle(pool, dir, host, id, true).await?;
            Ok(None::<String>)
        } else {
            Ok(Some(apply_launch(pool, dir, host, id).await?))
        }
    })
    .await?;
    if let Some(line) = report {
        println!("{line}");
    }
    play_game(pool, dir, host, id, false).await
}

async fn play_game(
    pool: &SqlitePool,
    dir: &Path,
    host: &PluginHost,
    id: &str,
    print: bool,
) -> CliResult {
    let paths = LaunchPaths::detect()?;
    let spec = build_launch_spec(pool, host, id, &paths, dir).await?;
    if print {
        println!("{}", spec.display());
        return Ok(());
    }
    let _ = touch_last_played(pool, id).await;
    tracing::debug!(game = id, "spawning");
    if spec.owned && has_harvestable(dir, id)? {
        let status = spec.command().status()?;
        tracing::info!(game = id, "spawned");
        let roots = harvest_roots(pool, dir, id).await;
        if roots.is_empty() {
            eprintln!("harvest skipped for {id}");
        } else {
            match harvest_game_roots(dir, id, &roots) {
                Ok(files) => {
                    for f in files {
                        println!("harvest\t{}", f.display());
                    }
                }
                Err(e) => eprintln!("harvest failed for {id}: {e}"),
            }
        }
        std::process::exit(status.code().unwrap_or(1));
    }
    let err = {
        use std::os::unix::process::CommandExt;
        spec.command().exec()
    };
    Err(err.into())
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
