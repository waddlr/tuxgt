use std::path::PathBuf;

use super::*;

pub(crate) async fn run(
    check: bool,
    yes: bool,
    from: Option<PathBuf>,
    tag: Option<String>,
    prefix: Option<PathBuf>,
) -> CliResult {
    let prefix = prefix.unwrap_or_else(data_dir);
    if let Some(asset) = from {
        // Clap `requires` guarantees the tag; local mode skips the
        // version check and the fetch entirely.
        let Some(tag) = tag else {
            return Err("--from needs --tag".into());
        };
        confirm_list("update TuxGT", std::slice::from_ref(&tag), yes)?;
        let rep = apply_local_update(&prefix, &tag, &asset)?;
        println!("updated\t{}", rep.updated);
        for r in rep.removed.iter() {
            println!("removed\t{r}");
        }
        println!("host\tok");
        println!("note\trestart the GUI to use {tag}");
        tracing::info!(tag = tag.as_str(), updated = rep.updated, "self-updated");
        return Ok(());
    }
    let current = app_version();
    println!("current\t{current}");
    match check_app_update().await {
        AppUpdateStatus::UpToDate { tag, .. } => {
            println!("latest\t{tag}");
            println!("status\tup-to-date");
        }
        AppUpdateStatus::Unknown { reason } => {
            println!("latest\t-");
            println!("status\tunknown\t{reason}");
        }
        AppUpdateStatus::Available { tag, asset_url, .. } => {
            println!("latest\t{tag}");
            if check {
                println!("status\tavailable");
                return Ok(());
            }
            confirm_list("update TuxGT", std::slice::from_ref(&tag), yes)?;
            let rep = apply_app_update(&prefix, &tag, &asset_url, None, None).await?;
            println!("updated\t{}", rep.updated);
            for r in rep.removed.iter() {
                println!("removed\t{r}");
            }
            println!("host\tok");
            println!("note\trestart the GUI to use {tag}");
            tracing::info!(tag = tag.as_str(), updated = rep.updated, "self-updated");
        }
    }
    Ok(())
}
