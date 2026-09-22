use std::io::{self, IsTerminal};
use std::path::Path;

use super::*;

pub(crate) async fn run(
    pool: &SqlitePool,
    dir: &Path,
    strings: &Strings,
    cmd: ModsCmd,
) -> CliResult {
    match cmd {
        ModsCmd::Graph => {
            let pkgs = fixture_packages();
            let diag = diagnose(&pkgs)?;
            println!("{}", strings.get("mods-graph-header"));
            for p in &pkgs {
                let slot = p.slot.map(|s| s.as_str()).unwrap_or("");
                println!("{}\t{}\t{slot}", p.name, p.type_);
            }
            if diag.missing_requires.is_empty() {
                println!("requires\tok");
            } else {
                for m in &diag.missing_requires {
                    println!("requires\tmissing\t{m}");
                }
            }
            if diag.slot_conflicts.is_empty() {
                println!("conflicts\tok");
            } else {
                for c in &diag.slot_conflicts {
                    println!("conflicts\t{c}");
                }
            }
        }
        ModsCmd::List { id } => {
            let cfg = config_dir();
            let listed = match id {
                None => list_mods(&cfg, &dir)?,
                Some(game) => {
                    let row = game_row(&pool, &game).await?;
                    let appid = row
                        .resolved_appid()
                        .and_then(|s| s.parse::<u32>().ok())
                        .filter(|a| *a != 0);
                    mods_for_game(&cfg, row.name.as_deref().unwrap_or(""), appid, &dir)?
                }
            };
            println!("{}", strings.get("mods-header"));
            for m in &listed.mods {
                let plans = m
                    .plans_allowed
                    .iter()
                    .map(|p| p.as_str())
                    .collect::<Vec<_>>()
                    .join(",");
                let state = if m.enabled {
                    strings.get("plugin-enabled")
                } else {
                    strings.get("plugin-disabled")
                };
                println!(
                    "{}\t{}\t{}\t{}\t{plans}\t{state}\t{}",
                    m.id,
                    m.mod_type,
                    m.label,
                    m.source.type_str(),
                    m.requires.join(",")
                );
            }
            for p in &listed.problems {
                eprintln!("broken\t{}\t{}", p.file, p.reason);
            }
        }
        ModsCmd::Add { file } => {
            let m = add_mod(&config_dir(), &file, &data_dir())?;
            println!(
                "{}\t{}\t{}\t{}",
                m.id,
                m.mod_type,
                m.label,
                m.requires.join(",")
            );
            tracing::info!(source = m.id.as_str(), "mod added");
        }
        ModsCmd::AddFrom {
            mod_type,
            id,
            path,
            label,
            yes,
        } => {
            if !yes && !io::stdout().is_terminal() {
                return Err("mods add-from: re-run with --yes".into());
            }
            let m = add_mod_from(
                &config_dir(),
                &mod_type,
                &id,
                &path,
                label.as_deref(),
                None,
                &data_dir(),
                Vec::new(),
                Vec::new(),
            )?;
            println!(
                "{}\t{}\t{}\t{}",
                m.id,
                m.mod_type,
                m.label,
                m.requires.join(",")
            );
            tracing::info!(source = m.id.as_str(), "mod added");
        }
        ModsCmd::Rescan { id, yes } => {
            if !yes && !io::stdout().is_terminal() {
                return Err("mods rescan: re-run with --yes".into());
            }
            let m = rescan_mod(&config_dir(), &id, None, None, &data_dir())?;
            println!(
                "{}\t{}\t{}\t{}",
                m.id,
                m.mod_type,
                m.label,
                m.requires.join(",")
            );
            tracing::info!(source = m.id.as_str(), "mod rescanned");
        }
        ModsCmd::Remove { id } => {
            remove_mod(&config_dir(), &data_dir(), &id)?;
            tracing::info!(source = id.as_str(), "mod removed");
        }
        ModsCmd::Enable { id } => {
            enable_mod(&config_dir(), &id, &data_dir())?;
            tracing::info!(source = id.as_str(), "mod enabled");
        }
        ModsCmd::Disable { id } => {
            disable_mod(&config_dir(), &id, &data_dir())?;
            tracing::info!(source = id.as_str(), "mod disabled");
        }
        ModsCmd::Export { id, out, files } => {
            tuxgt_core::export_mod(&config_dir(), &data_dir(), &id, &out, files)?;
            println!("{}", out.display());
            tracing::info!(source = id.as_str(), "mod exported");
        }
    }
    Ok(())
}

pub(crate) fn find_mod(id: &str) -> Result<tuxgt_core::Mod, Box<dyn std::error::Error>> {
    let listed = list_mods(&config_dir(), &data_dir())?;
    listed
        .mods
        .into_iter()
        .find(|i| i.id == id)
        .ok_or_else(|| format!("unknown mod: {id}").into())
}
