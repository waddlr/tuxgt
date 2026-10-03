use super::*;

pub(crate) async fn run(check: bool, yes: bool) -> CliResult {
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
            let rep = apply_app_update(&data_dir(), &tag, &asset_url, None, None).await?;
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
