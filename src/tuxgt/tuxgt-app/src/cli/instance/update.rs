use super::super::*;
use super::install::install_mod_cli;
use tuxgt_core::{check_update, ensure_update_baseline, Baseline, UpdateStatus};

pub(crate) async fn check_cli(
    pool: &SqlitePool,
    game: &str,
    instance: Option<&str>,
    yes: bool,
    slot: Option<String>,
) -> Result<(), Box<dyn std::error::Error>> {
    for id in instance_ids(game, instance)? {
        let b = baseline_cli(pool, game, &id, yes, slot.clone()).await?;
        if b.installed {
            println!("{game}\t{id}\trepaired");
        }
        print_update(game, &id, b.status);
    }
    Ok(())
}

pub(crate) async fn update_cli(
    pool: &SqlitePool,
    game: &str,
    instance: Option<&str>,
    yes: bool,
    slot: Option<String>,
    password: Option<String>,
) -> Result<(), Box<dyn std::error::Error>> {
    for id in instance_ids(game, instance)? {
        let b = baseline_cli(pool, game, &id, yes, slot.clone()).await?;
        if b.installed {
            println!("{game}\t{id}\trepaired");
        }
        print_update(game, &id, b.status.clone());
        match b.status {
            UpdateStatus::Available { detail } => {
                confirm_prompt(&format!("update {game} {id} ({detail})?"), yes)?;
                let m = install_mod_cli(
                    pool,
                    game,
                    &id,
                    installed_adapter(game, &id).as_deref(),
                    true,
                    None,
                    yes,
                    false,
                    password.clone(),
                    slot.clone(),
                )
                .await?;
                println!("{}\t{}\t{}", m.game, m.instance, m.files.len());
                note_auto_apply(pool, game).await;
            }
            UpdateStatus::UpToDate | UpdateStatus::Unknown { .. } => {}
        }
    }
    Ok(())
}

fn instance_ids(
    game: &str,
    instance: Option<&str>,
) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    match instance {
        Some(id) => {
            check_manifest_instance(game, id)?;
            Ok(vec![id.to_string()])
        }
        None => Ok(tuxgt_core::game_manifests(&data_dir(), game)?
            .into_iter()
            .map(|m| m.instance)
            .collect()),
    }
}

async fn baseline_cli(
    pool: &SqlitePool,
    game: &str,
    instance: &str,
    yes: bool,
    slot: Option<String>,
) -> Result<Baseline, Box<dyn std::error::Error>> {
    match ensure_update_baseline(pool, &data_dir(), &config_dir(), game, instance).await {
        Ok(b) => Ok(b),
        Err(Error::NeedConfirm(msg)) => {
            confirm_list("foreign game-dir dests", &msg_items(&msg), yes)?;
            repair_install(pool, game, instance, true, slot).await
        }
        Err(Error::NeedSlotChoice(_)) => repair_install(pool, game, instance, yes, slot).await,
        Err(e) => Err(e.into()),
    }
}

async fn repair_install(
    pool: &SqlitePool,
    game: &str,
    instance: &str,
    yes: bool,
    slot: Option<String>,
) -> Result<Baseline, Box<dyn std::error::Error>> {
    install_mod_cli(
        pool,
        game,
        instance,
        installed_adapter(game, instance).as_deref(),
        true,
        None,
        yes,
        false,
        None,
        slot,
    )
    .await?;
    Ok(Baseline {
        status: check_update(&data_dir(), &config_dir(), game, instance).await?,
        installed: true,
    })
}

fn installed_adapter(game: &str, instance: &str) -> Option<String> {
    read_manifest(&data_dir(), game, instance)
        .ok()
        .flatten()
        .map(|m| m.adapter)
}

fn print_update(game: &str, instance: &str, status: UpdateStatus) {
    match status {
        UpdateStatus::UpToDate => println!("{game}\t{instance}\tup-to-date"),
        UpdateStatus::Available { detail } => {
            println!("{game}\t{instance}\tavailable\t{detail}")
        }
        UpdateStatus::Unknown { reason } => println!("{game}\t{instance}\tunknown\t{reason}"),
    }
}
