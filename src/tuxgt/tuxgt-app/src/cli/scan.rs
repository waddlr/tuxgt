use std::path::Path;

use super::*;

pub(crate) async fn run(
    pool: &SqlitePool,
    dir: &Path,
    strings: &Strings,
    force: bool,
    yes: bool,
) -> CliResult {
    let host = PluginHost::load()?;
    let games = scan_games_opts(pool, &host, DetectOpts { force, yes }).await?;
    print_games(strings, &games);
    sync_handle_sessions(pool, dir, &host).await?;
    for (game, files) in harvest_all(pool, dir).await? {
        for f in files {
            println!("harvest\t{game}\t{}", f.display());
        }
    }
    Ok(())
}
