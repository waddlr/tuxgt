use std::io::{self, BufRead};
use std::path::Path;

use super::launch::refuse_if_client_running;
use super::*;
pub(crate) fn print_games(strings: &Strings, games: &[GameRow]) {
    if games.is_empty() {
        println!("{}", strings.get("games-list-empty"));
        return;
    }
    println!("{}", strings.get("games-list-header"));
    for g in games {
        println!("{}", game_line(g));
    }
}

/// `games list` display filter: default hides hidden titles, `--hidden`
/// shows hidden only, `--all` includes them.
fn filter_hidden(games: Vec<GameRow>, hidden: bool, all: bool) -> Vec<GameRow> {
    games
        .into_iter()
        .filter(|g| if hidden { g.hidden } else { all || !g.hidden })
        .collect()
}

/// One list row: `id<TAB>name<TAB>cover_path<TAB>hidden` (marker only when
/// the effective flag is set).
fn game_line(g: &GameRow) -> String {
    let name = g.name.as_deref().unwrap_or("");
    let cover = g.cover_path.as_deref().unwrap_or("");
    let flag = if g.hidden { "hidden" } else { "" };
    format!("{id}\t{name}\t{cover}\t{flag}", id = g.id)
}

pub(crate) async fn run_games(
    pool: &SqlitePool,
    dir: &Path,
    strings: &Strings,
    cmd: GamesCmd,
) -> CliResult {
    match cmd {
        GamesCmd::List {
            manager,
            store,
            query,
            hidden,
            all,
        } => {
            let games =
                list_games(pool, manager.as_deref(), store.as_deref(), query.as_deref()).await?;
            let shown = filter_hidden(games, hidden, all);
            print_games(strings, &shown);
        }
        GamesCmd::Add { exe } => {
            let host = PluginHost::load()?;
            let row = add_manual(pool, &host, &exe).await?;
            tracing::info!(game = row.id.as_str(), "game added");
            print_games(strings, &[row]);
        }
        GamesCmd::Remove { id } => {
            remove_manual(pool, dir, &id).await?;
            println!("removed\t{id}");
            tracing::info!(game = id.as_str(), "game removed");
        }
        GamesCmd::AppidSearch { name } => {
            let hits = tokio::task::spawn_blocking(move || search_steam_by_name(&name)).await??;
            for hit in hits {
                println!("{}\t{}", hit.appid, hit.name);
            }
        }
        GamesCmd::Appid { id, appid, clear } => {
            if clear {
                if appid.is_some() {
                    return Err(Error::InvalidOverride("--clear takes no appid".into()).into());
                }
                let host = PluginHost::load()?;
                mutate_game(pool, dir, &host, &id, set_steam_appid(pool, &id, None)).await?;
                tracing::info!(game = id.as_str(), "appid cleared");
            } else if let Some(v) = appid {
                let host = PluginHost::load()?;
                mutate_game(pool, dir, &host, &id, set_steam_appid(pool, &id, Some(&v))).await?;
                tracing::info!(game = id.as_str(), appid = v.as_str(), "appid set");
            } else {
                let v = steam_appid_of(pool, &id).await?;
                println!("{id}\t{}", v.unwrap_or_default());
            }
        }
        GamesCmd::Handle { id, on, off } => {
            if on && off {
                return Err("--on and --off are exclusive".into());
            }
            let host = PluginHost::load()?;
            if on || off {
                // Hook-on auto-restores the trampoline when applied: a store
                // write, so refuse under a live client like apply/restore.
                if on && has_apply_record(dir, &id) {
                    refuse_if_client_running(&id)?;
                }
                set_handle(pool, dir, &host, &id, on).await?;
                tracing::info!(game = id.as_str(), handle = on, "handle set");
            }
            let state = if game_handle(pool, &id).await? {
                "on"
            } else {
                "off"
            };
            println!("{id}\t{state}");
        }
        GamesCmd::ExtraExe { id, cmd } => match cmd {
            ExtraExeCmd::Add { exe } => {
                let host = PluginHost::load()?;
                mutate_game(pool, dir, &host, &id, add_extra_exe(pool, &id, &exe)).await?;
                println!("{id}\t{exe}");
                tracing::info!(game = id.as_str(), exe = exe.as_str(), "extra exe added");
            }
            ExtraExeCmd::Remove { exe } => {
                let host = PluginHost::load()?;
                mutate_game(pool, dir, &host, &id, remove_extra_exe(pool, &id, &exe)).await?;
                println!("removed\t{id}\t{exe}");
                tracing::info!(game = id.as_str(), exe = exe.as_str(), "extra exe removed");
            }
            ExtraExeCmd::List => {
                for x in list_extra_exes(pool, &id).await? {
                    println!("{id}\t{x}");
                }
            }
        },
    }
    Ok(())
}

pub(crate) async fn run_game(pool: &SqlitePool, cmd: GameCmd) -> CliResult {
    match cmd {
        GameCmd::Show { id, refresh } => {
            let host = PluginHost::load()?;
            for line in game_show(pool, &host, &id, refresh).await? {
                match &line.extra {
                    Some(extra) => println!("{}\t{}\t{extra}", line.key, line.value),
                    None => println!("{}\t{}", line.key, line.value),
                }
            }
        }
    }
    Ok(())
}

pub(crate) fn run_metadata(cmd: MetadataCmd) -> CliResult {
    match cmd {
        MetadataCmd::Key { cmd } => match cmd {
            KeyCmd::Set { source } => {
                if !is_key_source(&source) {
                    return Err(Error::UnknownMetadataSource(source).into());
                }
                let mut secret = String::new();
                io::stdin().lock().read_line(&mut secret)?;
                secret_manager_set(&source, secret.trim())?;
                tracing::info!(source = source.as_str(), "key stored");
            }
            KeyCmd::Clear { source } => {
                secret_manager_clear(&source)?;
                tracing::info!(source = source.as_str(), "key cleared");
            }
        },
    }
    Ok(())
}

pub(crate) fn run_plugins(cmd: PluginsCmd, strings: &Strings) -> CliResult {
    let mut host = PluginHost::load()?;
    match cmd {
        PluginsCmd::List => {
            let listed = host.list();
            if listed.is_empty() {
                println!("{}", strings.get("plugins-list-empty"));
            } else {
                println!("{}", strings.get("plugins-list-header"));
                for entry in listed {
                    let id = entry.desc.id();
                    let label = strings.get(entry.desc.label_id);
                    let state = if entry.enabled {
                        strings.get("plugin-enabled")
                    } else {
                        strings.get("plugin-disabled")
                    };
                    println!("{id}\t{label}\t{state}");
                }
            }
        }
        PluginsCmd::Enable { id } => {
            host.set_enabled(&id, true)?;
            tracing::info!(source = id.as_str(), "plugin enabled");
        }
        PluginsCmd::Disable { id } => {
            host.set_enabled(&id, false)?;
            tracing::info!(source = id.as_str(), "plugin disabled");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tuxgt_core::GameRow;

    fn row(id: &str, hidden: bool) -> GameRow {
        GameRow {
            id: id.into(),
            name: Some(id.into()),
            cover_path: None,
            manager: "steam".into(),
            store: String::new(),
            header_path: None,
            platform: None,
            api: None,
            install_dir: None,
            exe_path: None,
            prefix_path: None,
            proton: None,
            bitness: None,
            engine: None,
            hidden,
            last_played: None,
            steam_appid: None,
        }
    }

    fn ids(rows: Vec<GameRow>) -> Vec<String> {
        rows.into_iter().map(|g| g.id).collect()
    }

    #[test]
    fn games_list_hidden_filter_semantics() {
        let rows = || vec![row("steam::1", false), row("steam::2", true)];
        assert_eq!(ids(filter_hidden(rows(), false, false)), vec!["steam::1"]);
        assert_eq!(ids(filter_hidden(rows(), true, false)), vec!["steam::2"]);
        assert_eq!(
            ids(filter_hidden(rows(), false, true)),
            vec!["steam::1", "steam::2"]
        );
        assert_eq!(ids(filter_hidden(rows(), true, true)), vec!["steam::2"]);
    }

    #[test]
    fn games_list_line_marks_hidden_only() {
        assert_eq!(game_line(&row("steam::1", false)), "steam::1\tsteam::1\t\t");
        assert_eq!(
            game_line(&row("steam::2", true)),
            "steam::2\tsteam::2\t\thidden"
        );
    }
}
