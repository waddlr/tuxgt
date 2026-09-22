use super::*;

pub(crate) async fn run(cmd: CacheCmd) -> CliResult {
    match cmd {
        CacheCmd::Refresh { instance } => {
            refresh_cache(instance.as_deref()).await?;
            tracing::info!(instance = instance.as_deref().unwrap_or("all"), "cache refreshed");
            Ok(())
        }
        CacheCmd::Tools => {
            for t in all_tools() {
                let found = if t.found { "found" } else { "missing" };
                println!("{}\t{found}\t{}", t.name, t.version);
            }
            Ok(())
        }
    }
}

async fn refresh_cache(instance: Option<&str>) -> CliResult {
    let data = data_dir();
    match instance {
        Some(id) => {
            let m = find_mod(id)?;
            let (a, _) = tuxgt_core::acquire_with_source(&data, &m, true, None).await?;
            println!("{}\t{}\t{}", m.id, a.sha256, a.bytes);
        }
        None => {
            let listed = list_mods(&config_dir(), &data)?;
            for inst in &listed.mods {
                match tuxgt_core::acquire_with_source(&data, inst, true, None).await {
                    Ok((a, _)) => println!("{}\t{}\t{}", inst.id, a.sha256, a.bytes),
                    Err(e) => eprintln!("refresh\t{}\terror\t{e}", inst.id),
                }
            }
        }
    }
    Ok(())
}
