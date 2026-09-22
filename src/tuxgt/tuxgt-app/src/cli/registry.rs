use tuxgt_core::{FluentArgs, RegistryStore};

use super::{CliResult, RegistryCmd, RemoteCmd, Strings};

/// R14 registry CLI. Same core API the GUI calls, so both surfaces write
/// one lock; unknown ids, digest/ABI mismatches, and partial writes error
/// with no mutation.
pub(crate) fn run_registry(cmd: RegistryCmd, strings: &Strings) -> CliResult {
    match cmd {
        RegistryCmd::Add {
            url,
            git_ref,
            manifest,
            local,
        } => {
            let mut store = RegistryStore::load()?;
            let entry = store.add(&url, &git_ref, &manifest, local)?;
            println!(
                "{}\t{}",
                entry.url,
                strings.get_args(
                    "plugins-registry-added",
                    Some(&args(&[("pin", &entry.pinned)]))
                )
            );
        }
        RegistryCmd::List => {
            let store = RegistryStore::load()?;
            let views = store.views();
            if views.is_empty() {
                println!("{}", strings.get("plugins-registry-empty"));
            } else {
                println!("{}", strings.get("plugins-registry-header"));
                for view in &views {
                    let pin = short(&view.entry.pinned);
                    let count = view.plugins.len().to_string();
                    println!(
                        "{}\t{}",
                        view.entry.url,
                        strings.get_args(
                            "plugins-registry-pinned",
                            Some(&args(&[("pin", &pin), ("count", &count)]))
                        )
                    );
                    if let Some(error) = &view.error {
                        println!("  !\t{error}");
                        continue;
                    }
                    for plugin in &view.plugins {
                        let state = match store.installed_entry(&plugin.id) {
                            Some(installed) => {
                                let pin = short(&installed.pinned);
                                strings.get_args(
                                    "plugins-registry-installed",
                                    Some(&args(&[("pin", &pin)])),
                                )
                            }
                            None => strings.get("plugins-registry-available"),
                        };
                        println!("  {}\t{}\t{}", plugin.id, plugin.label, state);
                    }
                }
            }
        }
        RegistryCmd::Update { url } => {
            let mut store = RegistryStore::load()?;
            let urls = match url {
                Some(url) => vec![url],
                None => store
                    .views()
                    .into_iter()
                    .map(|v| v.entry.url)
                    .collect::<Vec<String>>(),
            };
            for url in urls {
                let entry = store.update_registry(&url)?;
                println!("{}\t{}", entry.url, short(&entry.pinned));
            }
        }
        RegistryCmd::Remove { url } => {
            let mut store = RegistryStore::load()?;
            store.remove_registry(&url)?;
            tracing::info!(source = url.as_str(), "plugin registry removed");
        }
        RegistryCmd::Digest { path } => {
            let (digest, _) = tuxgt_core::plugin::registry::payload_digest_in_repo(&path)?;
            println!("{digest}");
        }
    }
    Ok(())
}

/// R14 installed-plugin CLI: install/update/remove plus the remote
/// enable toggle that lives in `remote.toml`, never `plugins.toml`.
pub(crate) fn run_remote(cmd: RemoteCmd, strings: &Strings) -> CliResult {
    let mut store = RegistryStore::load()?;
    match cmd {
        RemoteCmd::Install { id } => {
            let entry = store.install(&id)?;
            let pin = short(&entry.pinned);
            println!(
                "{}\t{}",
                entry.id,
                strings.get_args("plugins-remote-installed", Some(&args(&[("pin", &pin)])))
            );
        }
        RemoteCmd::Update { id } => match id {
            Some(id) => {
                let entry = store.update_plugin(&id)?;
                let pin = short(&entry.pinned);
                println!(
                    "{}\t{}",
                    entry.id,
                    strings.get_args("plugins-remote-updated", Some(&args(&[("pin", &pin)])))
                );
            }
            None => {
                for entry in store.installed().to_vec() {
                    let updated = store.update_plugin(&entry.id)?;
                    println!("{}\t{}", updated.id, short(&updated.pinned));
                }
            }
        },
        RemoteCmd::Remove { id } => {
            store.remove_plugin(&id)?;
            tracing::info!(source = id.as_str(), "registry plugin removed");
        }
        RemoteCmd::Enable { id } => {
            store.set_remote_enabled(&id, true)?;
            println!("{}\t{}", id, strings.get("plugin-enabled"));
        }
        RemoteCmd::Disable { id } => {
            store.set_remote_enabled(&id, false)?;
            println!("{}\t{}", id, strings.get("plugin-disabled"));
        }
        RemoteCmd::List => {
            let installed = store.installed().to_vec();
            if installed.is_empty() {
                println!("{}", strings.get("plugins-remote-empty"));
            } else {
                println!("{}", strings.get("plugins-remote-header"));
                for entry in &installed {
                    let state = if store.remote_enabled(&entry.id) {
                        strings.get("plugin-enabled")
                    } else {
                        strings.get("plugin-disabled")
                    };
                    println!("{}\t{}\t{}", entry.id, short(&entry.pinned), state);
                }
            }
        }
    }
    Ok(())
}

fn short(pin: &str) -> String {
    pin.chars().take(12).collect()
}

fn args<'a>(pairs: &[(&'a str, &'a str)]) -> FluentArgs<'a> {
    let mut args = FluentArgs::new();
    for (key, value) in pairs {
        args.set(*key, *value);
    }
    args
}
