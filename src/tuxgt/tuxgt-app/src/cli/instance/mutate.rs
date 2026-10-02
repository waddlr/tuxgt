use super::super::*;
use tuxgt_core::{resync_game, resync_instance};

pub(crate) async fn enable_mod_cli(
    pool: &tuxgt_core::SqlitePool,
    game: &str,
    instance: &str,
    on: bool,
    yes: bool,
) -> Result<FileManifest, Box<dyn std::error::Error>> {
    let mut yes = yes;
    loop {
        match set_instance_enabled(pool, &data_dir(), game, instance, on, yes).await {
            Ok(m) => return Ok(m),
            Err(Error::NeedConfirm(msg)) if !yes => {
                confirm_list("foreign game-dir dests", &msg_items(&msg), false)?;
                yes = true;
            }
            Err(e) => return Err(e.into()),
        }
    }
}

/// Keep/omit one dest through the same foreign-overwrite confirm the
/// install/enable paths use: `--yes` answers it, otherwise a TTY prompts.
pub(crate) async fn set_file_keep_cli(
    pool: &tuxgt_core::SqlitePool,
    game: &str,
    instance: &str,
    dest: &str,
    on: bool,
    yes: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut yes = yes;
    loop {
        match set_file_keep(pool, &data_dir(), game, instance, dest, on, yes).await {
            Ok(_) => return Ok(()),
            Err(Error::NeedConfirm(msg)) if !yes => {
                confirm_list("foreign game-dir dests", &msg_items(&msg), false)?;
                yes = true;
            }
            Err(e) => return Err(e.into()),
        }
    }
}

pub(crate) async fn set_slot_cli(
    pool: &tuxgt_core::SqlitePool,
    game: &str,
    instance: &str,
    slot: &str,
    yes: bool,
) -> Result<FileManifest, Box<dyn std::error::Error>> {
    let mut yes = yes;
    loop {
        match set_instance_slot(pool, &data_dir(), game, instance, slot, yes).await {
            Ok(m) => return Ok(m),
            Err(Error::NeedConfirm(msg)) if !yes => {
                confirm_list("foreign game-dir dests", &msg_items(&msg), false)?;
                yes = true;
            }
            Err(e) => return Err(e.into()),
        }
    }
}

pub(crate) async fn uninstall_mod_cli(
    pool: &tuxgt_core::SqlitePool,
    game: &str,
    instance: &str,
    yes: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut yes = yes;
    loop {
        match uninstall_instance(pool, &data_dir(), &config_dir(), game, instance, yes).await {
            Ok(()) => return Ok(()),
            Err(Error::NeedConfirm(msg)) if !yes => {
                confirm_list(
                    "foreign game-dir dests left in place",
                    &msg_items(&msg),
                    false,
                )?;
                yes = true;
            }
            Err(e) => return Err(e.into()),
        }
    }
}

pub(crate) fn resync_cli(
    game: &str,
    instance: Option<&str>,
    yes: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let data = data_dir();
    let clash = tuxgt_core::resync_repick_instances(&data, &config_dir(), game)?;
    if !clash.is_empty() && !yes {
        return Err(format!(
            "shared proxy dests {}; pick slots with `tuxgt instance slot` or re-run with --yes",
            clash.join(", ")
        )
        .into());
    }
    let lines = match instance {
        Some(id) => {
            check_manifest_instance(game, id)?;
            resync_instance(&data, game, id, true)?
        }
        None => resync_game(&data, game, true)?,
    };
    for line in lines {
        println!("{}\t{}\t{}", line.instance, line.file, line.state.as_str());
    }
    Ok(())
}
