use super::*;
pub(crate) async fn run(
    pool: &SqlitePool,
    host: &PluginHost,
    strings: &Strings,
    dir: &std::path::Path,
    cmd: EnvCmd,
) -> Result<(), Box<dyn std::error::Error>> {
    match cmd {
        EnvCmd::List { id } => {
            let knobs = enabled_knobs(host);
            match id {
                None => {
                    if knobs.is_empty() {
                        println!("{}", strings.get("env-knobs-empty"));
                    } else {
                        println!("{}", strings.get("env-knobs-header"));
                        for k in knobs {
                            println!("{}\t{}\t{}", k.id, scopes_display(k.scopes), k.help);
                        }
                    }
                }
                Some(id) => {
                    let platform = effective_platform(pool, &id).await?;
                    let game_rows = knob_rows(pool, &id).await?;
                    let globals = global_knobs(pool).await?;
                    let mut lines = Vec::new();
                    for k in knobs.iter().filter(|k| scope_applies(k.scopes, &platform)) {
                        let game = game_rows.iter().find(|r| r.knob == k.id);
                        let global = globals.iter().find(|r| r.knob == k.id);
                        let unmanaged = knob_is_unmanaged(
                            k,
                            global.filter(|g| g.enabled).map(|g| g.value.as_str()),
                        );
                        let value = game.map(|r| r.value.as_str()).unwrap_or("");
                        let enabled = if game.is_some_and(|r| r.enabled) {
                            "enabled"
                        } else {
                            "disabled"
                        };
                        let source = knob_source(game, global, unmanaged).as_str();
                        lines.push((
                            k.id,
                            scopes_display(k.scopes),
                            value.to_string(),
                            enabled,
                            source,
                            k.help,
                        ));
                    }
                    if lines.is_empty() {
                        println!("{}", strings.get("env-knobs-empty"));
                    } else {
                        println!("{}", strings.get("env-knobs-header"));
                        for (knob, scopes, value, enabled, source, help) in lines {
                            println!("{knob}\t{scopes}\t{value}\t{enabled}\t{source}\t{help}");
                        }
                    }
                }
            }
        }
        EnvCmd::Get { id, knob } => {
            let k =
                find_enabled_knob(host, &knob).ok_or_else(|| Error::UnknownKnob(knob.clone()))?;
            let platform = effective_platform(pool, &id).await?;
            require_scope(k, &platform)?;
            let row = knob_rows(pool, &id)
                .await?
                .into_iter()
                .find(|r| r.knob == k.id)
                .ok_or_else(|| Error::KnobNotSet(knob.clone()))?;
            let en = if row.enabled { "enabled" } else { "disabled" };
            println!("{}\t{}\t{en}", k.id, row.value);
        }
        EnvCmd::Set { id, knob, value } => {
            let k =
                find_enabled_knob(host, &knob).ok_or_else(|| Error::UnknownKnob(knob.clone()))?;
            let platform = effective_platform(pool, &id).await?;
            require_scope(k, &platform)?;
            let value = resolve_value(k, value.as_deref())?;
            mutate_game(pool, dir, host, &id, set_knob(pool, &id, &knob, &value)).await?;
            tracing::info!(game = id.as_str(), knob = knob.as_str(), "knob set");
        }
        EnvCmd::Unset { id, knob } => {
            find_enabled_knob(host, &knob).ok_or_else(|| Error::UnknownKnob(knob.clone()))?;
            if !game_exists(pool, &id).await? {
                return Err(Error::UnknownGame(id).into());
            }
            mutate_game(pool, dir, host, &id, unset_knob(pool, &id, &knob)).await?;
            tracing::info!(game = id.as_str(), knob = knob.as_str(), "knob unset");
        }
        EnvCmd::Enable { id, knob } => {
            find_enabled_knob(host, &knob).ok_or_else(|| Error::UnknownKnob(knob.clone()))?;
            if !game_exists(pool, &id).await? {
                return Err(Error::UnknownGame(id).into());
            }
            mutate_game(pool, dir, host, &id, enable_knob(pool, &id, &knob)).await?;
            tracing::info!(game = id.as_str(), knob = knob.as_str(), "knob enabled");
        }
        EnvCmd::Disable { id, knob } => {
            find_enabled_knob(host, &knob).ok_or_else(|| Error::UnknownKnob(knob.clone()))?;
            if !game_exists(pool, &id).await? {
                return Err(Error::UnknownGame(id).into());
            }
            mutate_game(pool, dir, host, &id, disable_knob(pool, &id, &knob)).await?;
            tracing::info!(game = id.as_str(), knob = knob.as_str(), "knob disabled");
        }
        EnvCmd::Global { cmd } => {
            handle_global_env(pool, host, strings, dir, cmd).await?;
        }
        EnvCmd::Custom { cmd } => match cmd {
            CustomCmd::List { id } => {
                if !game_exists(pool, &id).await? {
                    return Err(Error::UnknownGame(id).into());
                }
                let rows = custom_env(pool, &id).await?;
                if rows.is_empty() {
                    println!("{}", strings.get("env-custom-empty"));
                } else {
                    println!("{}", strings.get("env-custom-header"));
                    for (key, value) in rows {
                        println!("{key}\t{value}");
                    }
                }
            }
            CustomCmd::Add { id, pair } => {
                if !game_exists(pool, &id).await? {
                    return Err(Error::UnknownGame(id).into());
                }
                let (key, value) = pair
                    .split_once('=')
                    .ok_or_else(|| Error::InvalidEnvKey(format!("{pair} (expected KEY=VALUE)")))?;
                validate_custom_key(key)?;
                mutate_game(pool, dir, host, &id, set_custom(pool, &id, key, value)).await?;
                tracing::info!(game = id.as_str(), key = key, "custom env set");
            }
            CustomCmd::Remove { id, key } => {
                if !game_exists(pool, &id).await? {
                    return Err(Error::UnknownGame(id).into());
                }
                mutate_game(pool, dir, host, &id, remove_custom(pool, &id, &key)).await?;
                tracing::info!(game = id.as_str(), key = key.as_str(), "custom env removed");
            }
        },
    }
    Ok(())
}

async fn handle_global_env(
    pool: &SqlitePool,
    host: &PluginHost,
    strings: &Strings,
    dir: &std::path::Path,
    cmd: GlobalEnvCmd,
) -> Result<(), Box<dyn std::error::Error>> {
    match cmd {
        GlobalEnvCmd::List => {
            let knobs = enabled_knobs(host);
            if knobs.is_empty() {
                println!("{}", strings.get("env-knobs-empty"));
            } else {
                let globals = global_knobs(pool).await?;
                println!("{}", strings.get("env-knobs-header"));
                for k in knobs {
                    let global = globals.iter().find(|r| r.knob == k.id);
                    let unmanaged = knob_is_unmanaged(
                        k,
                        global.filter(|g| g.enabled).map(|g| g.value.as_str()),
                    );
                    let value = if unmanaged {
                        live_knob_value(k).unwrap_or_default()
                    } else {
                        global.map(|r| r.value.clone()).unwrap_or_default()
                    };
                    let enabled = if global.is_some_and(|r| r.enabled) && !unmanaged {
                        "enabled"
                    } else {
                        "disabled"
                    };
                    let source = knob_source(None, global, unmanaged).as_str();
                    println!(
                        "{}\t{}\t{value}\t{enabled}\t{source}\t{}",
                        k.id,
                        scopes_display(k.scopes),
                        k.help
                    );
                }
            }
        }
        GlobalEnvCmd::Set { knob, value } => {
            let k =
                find_enabled_knob(host, &knob).ok_or_else(|| Error::UnknownKnob(knob.clone()))?;
            let value = resolve_value(k, value.as_deref())?;
            set_global_knob(pool, &knob, &value).await?;
            sync_handle_sessions(pool, dir, host).await?;
            tracing::info!(knob = knob.as_str(), "global knob set");
        }
        GlobalEnvCmd::Unset { knob } => {
            find_enabled_knob(host, &knob).ok_or_else(|| Error::UnknownKnob(knob.clone()))?;
            unset_global_knob(pool, &knob).await?;
            sync_handle_sessions(pool, dir, host).await?;
            tracing::info!(knob = knob.as_str(), "global knob unset");
        }
        GlobalEnvCmd::Enable { knob } => {
            find_enabled_knob(host, &knob).ok_or_else(|| Error::UnknownKnob(knob.clone()))?;
            enable_global_knob(pool, &knob).await?;
            sync_handle_sessions(pool, dir, host).await?;
            tracing::info!(knob = knob.as_str(), "global knob enabled");
        }
        GlobalEnvCmd::Disable { knob } => {
            find_enabled_knob(host, &knob).ok_or_else(|| Error::UnknownKnob(knob.clone()))?;
            disable_global_knob(pool, &knob).await?;
            sync_handle_sessions(pool, dir, host).await?;
            tracing::info!(knob = knob.as_str(), "global knob disabled");
        }
    }
    Ok(())
}

fn require_scope(k: &EnvKnob, platform: &str) -> Result<(), Error> {
    if scope_applies(k.scopes, platform) {
        Ok(())
    } else {
        Err(Error::KnobNotApplicable(format!(
            "{} on {}",
            k.id, platform
        )))
    }
}
