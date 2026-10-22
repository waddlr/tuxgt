use std::path::Path;

use super::store_write::with_store_write;
use super::*;
use crate::GameArgsCmd;
use tuxgt_core::{PluginHost, SqlitePool};

pub(crate) async fn run(
    pool: &SqlitePool,
    host: &PluginHost,
    dir: &Path,
    cmd: GameArgsCmd,
) -> Result<(), Box<dyn std::error::Error>> {
    match cmd {
        GameArgsCmd::List { id: None } => {
            for row in tuxgt_core::list_shipped(dir) {
                println!(
                    "{}\t{}\t{}\t{}",
                    row.file,
                    row.id,
                    row.kind.label(),
                    row.help
                );
            }
        }
        GameArgsCmd::List { id: Some(id) } => {
            let v = tuxgt_core::view(pool, dir, &id).await?;
            println!("{}", if v.dirty { "dirty" } else { "clean" });
            for row in v.rows {
                let en = if row.enabled { "enabled" } else { "disabled" };
                println!(
                    "{}\t{}\t{}\t{}\t{}\t{}\t{}",
                    row.id,
                    row.kind.label(),
                    row.value,
                    en,
                    row.source,
                    row.mark.label(),
                    row.help
                );
            }
        }
        GameArgsCmd::Get { id, arg } => {
            let v = tuxgt_core::view(pool, dir, &id).await?;
            let row = v
                .rows
                .into_iter()
                .find(|r| r.id == arg)
                .ok_or_else(|| tuxgt_core::Error::UnknownArg(arg))?;
            let en = if row.enabled { "enabled" } else { "disabled" };
            println!(
                "{}\t{}\t{}\t{en}\t{}\t{}\t{}",
                row.id,
                row.kind.label(),
                row.value,
                row.source,
                row.mark.label(),
                row.help
            );
        }
        GameArgsCmd::Set { id, arg, value } => {
            tuxgt_core::set_arg(pool, dir, host, &id, &arg, value.as_deref()).await?;
        }
        GameArgsCmd::Unset { id, arg } => {
            tuxgt_core::unset_arg(pool, dir, host, &id, &arg).await?;
        }
        GameArgsCmd::Enable { id, arg } => {
            tuxgt_core::enable_arg(pool, dir, host, &id, &arg).await?;
        }
        GameArgsCmd::Disable { id, arg } => {
            tuxgt_core::disable_arg(pool, dir, host, &id, &arg).await?;
        }
        GameArgsCmd::Custom { cmd } => match cmd {
            crate::ArgsCustomCmd::List { id } => {
                let v = tuxgt_core::view(pool, dir, &id).await?;
                if !v.custom.is_empty() {
                    println!("{}", v.custom);
                }
            }
            crate::ArgsCustomCmd::Set { id, args } => {
                tuxgt_core::game_args::set_custom(pool, dir, host, &id, &args).await?;
            }
            crate::ArgsCustomCmd::Clear { id } => {
                tuxgt_core::clear_custom(pool, dir, host, &id).await?;
            }
        },
        GameArgsCmd::Save { id, yes } => {
            let row = game_row(pool, &id).await?;
            if row.manager != "steam" && row.manager != "heroic" {
                return Err(tuxgt_core::Error::ArgsManualSave.into());
            }
            let v = tuxgt_core::view(pool, dir, &id).await?;
            if !v.dirty {
                println!("{id} already saved");
                return Ok(());
            }
            let line = with_store_write(&id, yes, true, || async {
                Ok(tuxgt_core::save_args(pool, dir, host, &id).await?)
            })
            .await?;
            println!("{line}");
        }
        GameArgsCmd::Discard { id } => {
            tuxgt_core::discard(pool, dir, host, &id).await?;
        }
    }
    Ok(())
}
