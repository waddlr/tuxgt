use std::path::Path;

use super::super::store_write::with_store_write;
use super::super::*;
use tuxgt_core::{
    convert_after_unplace, game_adapter, is_install, is_self_slot, preflight_convert_picks,
    slot_dll, validate_adapter, validate_adapter_convert,
};

pub(crate) async fn run_adapter(
    pool: &SqlitePool,
    dir: &Path,
    id: String,
    adapter: Option<String>,
    yes: bool,
    slot: Option<String>,
) -> CliResult {
    match adapter.as_deref() {
        None => {
            println!("{id}\t{}", game_adapter(pool, &id).await?);
            return Ok(());
        }
        Some(a) => {
            validate_adapter(a)?;
        }
    }
    let target = validate_adapter(adapter.as_deref().unwrap())?.to_string();
    if let Some(s) = slot.as_deref() {
        if !is_self_slot(s) {
            slot_dll(s)?;
        }
    }
    let from = game_adapter(pool, &id).await?;
    if from == target {
        println!("{id}\t{from}");
        return Ok(());
    }
    let stop_yes = yes;
    let (dest_yes, picks) = consent_convert(pool, dir, &id, &target, yes, slot.as_deref()).await?;
    // GUI unhooks only after pre-stop validation. Stop/restart uses the
    // original --yes; dest confirm must not authorize ClientStop.
    with_store_write(&id, stop_yes, true, || {
        let id = id.clone();
        let target = target.clone();
        let picks = picks.clone();
        async move { convert_and_arm(pool, dir, id, target, dest_yes, picks).await }
    })
    .await
}

async fn convert_and_arm(
    pool: &SqlitePool,
    dir: &Path,
    id: String,
    target: String,
    yes: bool,
    picks: Vec<(String, String)>,
) -> CliResult {
    let host = PluginHost::load()?;
    let handled = game_handle(pool, &id).await?;
    let applied = has_apply_record(dir, &id);
    if is_install(&target) && (handled || applied) {
        if applied {
            println!("{}", restore_launch(dir, &id)?);
        }
        if handled {
            set_handle(pool, dir, &host, &id, false).await?;
        }
    }
    let pairs: Vec<(&str, &str)> = picks
        .iter()
        .map(|(i, s)| (i.as_str(), s.as_str()))
        .collect();
    let report = convert_after_unplace(
        pool,
        dir,
        &config_dir(),
        &id,
        &target,
        yes,
        !pairs.is_empty(),
        &pairs,
    )
    .await?;
    println!(
        "{id}\t{}\t{}\t{}",
        report.from,
        report.to,
        report.instances.join(",")
    );
    if is_install(&target) {
        note_auto_apply(pool, &id).await;
    } else if has_apply_record(dir, &id) {
        match restore_launch(dir, &id) {
            Ok(line) => println!("{line}"),
            Err(e) => {
                tracing::warn!(game = id.as_str(), error = %e, "preload adapter restore failed");
                eprintln!("restore failed for {id}: {e}");
            }
        }
    }
    Ok(())
}

/// Resolve NeedSlotChoice / NeedConfirm with no store writes and no convert.
async fn consent_convert(
    pool: &SqlitePool,
    dir: &Path,
    id: &str,
    target: &str,
    yes: bool,
    slot: Option<&str>,
) -> Result<(bool, Vec<(String, String)>), Box<dyn std::error::Error>> {
    let mut yes = yes;
    let mut picks: Vec<(String, String)> = Vec::new();
    let mut slots_chosen = false;
    loop {
        match validate_adapter_convert(pool, dir, &config_dir(), id, target, yes, slots_chosen)
            .await
        {
            Ok(()) => {
                if picks.is_empty() {
                    return Ok((yes, picks));
                }
                let pairs: Vec<(&str, &str)> = picks
                    .iter()
                    .map(|(i, s)| (i.as_str(), s.as_str()))
                    .collect();
                match preflight_convert_picks(pool, dir, id, target, yes, &pairs).await {
                    Ok(()) => return Ok((yes, picks)),
                    Err(Error::NeedConfirm(msg)) if !yes => {
                        confirm_list("foreign game-dir dests", &msg_items(&msg), false)?;
                        yes = true;
                    }
                    Err(e) => return Err(e.into()),
                }
            }
            Err(Error::NeedSlotChoice(msg)) if slot.is_some() && !slots_chosen => {
                let s = slot.unwrap();
                picks = msg_items(&msg)
                    .into_iter()
                    .map(|inst| (inst, s.to_string()))
                    .collect();
                slots_chosen = true;
            }
            Err(Error::NeedSlotChoice(msg)) => {
                return Err(format!(
                    "need proxy slot for {}; pass --slot <dxgi|d3d9|d3d10|d3d11|d3d12|winmm|version|<self>>",
                    msg_items(&msg).join(", ")
                )
                .into());
            }
            Err(Error::NeedConfirm(msg)) if !yes => {
                confirm_list("foreign game-dir dests", &msg_items(&msg), false)?;
                yes = true;
            }
            Err(e) => return Err(e.into()),
        }
    }
}
