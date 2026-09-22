use super::*;

pub(crate) async fn run(pool: &SqlitePool, cmd: InstanceCmd) -> CliResult {
    match cmd {
        InstanceCmd::Install {
            game,
            instance,
            adapter,
            redownload,
            with_requires,
            yes,
            force,
        } => {
            if adapter != "preload" && adapter != "install" {
                return Err(format!("unknown adapter: {adapter} (preload|install)").into());
            }
            let m = install_mod_cli(
                &pool,
                &game,
                &instance,
                &adapter,
                redownload,
                with_requires,
                yes,
                force,
            )
            .await?;
            println!("{}\t{}\t{}", m.game, m.instance, m.files.len());
        }
        InstanceCmd::Enable {
            game,
            instance,
            yes,
        } => {
            if !game_exists(&pool, &game).await? {
                return Err(Error::UnknownGame(game).into());
            }
            check_manifest_instance(&game, &instance)?;
            let m = enable_mod_cli(&pool, &game, &instance, true, yes).await?;
            note_staging(&data_dir(), &game);
            println!("{}\t{}\tenabled", m.game, m.instance);
        }
        InstanceCmd::Disable { game, instance } => {
            if !game_exists(&pool, &game).await? {
                return Err(Error::UnknownGame(game).into());
            }
            check_manifest_instance(&game, &instance)?;
            let m = set_instance_enabled(&pool, &data_dir(), &game, &instance, false, true).await?;
            println!("{}\t{}\tdisabled", m.game, m.instance);
        }
        InstanceCmd::Uninstall {
            game,
            instance,
            yes,
        } => {
            uninstall_mod_cli(&pool, &game, &instance, yes).await?;
            println!("{game}\t{instance}\tuninstalled");
        }
        InstanceCmd::Status { game } => {
            for line in stage_status(&data_dir(), &game)? {
                println!("{}\t{}\t{}", line.instance, line.file, line.state.as_str());
            }
        }
        InstanceCmd::Files {
            game,
            instance,
            action,
            dest,
            yes,
        } => match (action, dest) {
            (None, None) => {
                let m = read_manifest(&data_dir(), &game, &instance)?
                    .ok_or_else(|| Error::NoManifest(format!("{game} {instance}")))?;
                for f in &m.files {
                    let kind = if is_dll(&f.dest) && !include_covers(&m.include, &f.dest) {
                        "loaddll"
                    } else {
                        "include"
                    };
                    let kept = if f.enabled { "kept" } else { "omitted" };
                    let required =
                        if is_required_dest(&m.mod_type, &f.dest, &m.include, m.files.len()) {
                            "required"
                        } else {
                            "optional"
                        };
                    println!("{}\t{kind}\t{kept}\t{required}", f.dest);
                }
            }
            (Some(action), Some(dest)) => {
                if !game_exists(&pool, &game).await? {
                    return Err(Error::UnknownGame(game).into());
                }
                check_manifest_instance(&game, &instance)?;
                let on = matches!(action, FileKeep::Enable);
                set_file_keep_cli(&pool, &game, &instance, &dest, on, yes).await?;
                let word = if on { "enabled" } else { "disabled" };
                println!("{game}\t{instance}\t{dest}\t{word}");
                tracing::info!(game = game.as_str(), instance = instance.as_str(), dest = dest.as_str(), enabled = on, "file keep set");
            }
            _ => {
                return Err(
                    "usage: tuxgt instance files <game> <instance> [enable|disable <dest>]".into(),
                )
            }
        },
        InstanceCmd::Env {
            game,
            instance,
            action,
            key,
        } => match (action, key) {
            (None, None) => {
                let m = read_manifest(&data_dir(), &game, &instance)?
                    .ok_or_else(|| Error::NoManifest(format!("{game} {instance}")))?;
                for e in &m.env {
                    let kept = if e.enabled { "kept" } else { "omitted" };
                    println!("{}\t{}\t{kept}", e.key, e.value);
                }
            }
            (Some(action), Some(key)) => {
                if !game_exists(&pool, &game).await? {
                    return Err(Error::UnknownGame(game).into());
                }
                check_manifest_instance(&game, &instance)?;
                let on = matches!(action, FileKeep::Enable);
                let host = PluginHost::load()?;
                mutate_game(&pool, &data_dir(), &host, &game, async {
                    set_mod_env_enabled(&data_dir(), &game, &instance, &key, on)
                })
                .await?;
                let word = if on { "enabled" } else { "disabled" };
                println!("{game}\t{instance}\t{key}\t{word}");
                tracing::info!(game = game.as_str(), instance = instance.as_str(), key = key.as_str(), enabled = on, "mod env set");
            }
            _ => {
                return Err(
                    "usage: tuxgt instance env <game> <instance> [enable|disable <key>]".into(),
                )
            }
        },

        InstanceCmd::Slot {
            game,
            instance,
            slot,
            yes,
        } => {
            if !game_exists(&pool, &game).await? {
                return Err(Error::UnknownGame(game).into());
            }
            check_manifest_instance(&game, &instance)?;
            set_slot_cli(&pool, &game, &instance, &slot, yes).await?;
            println!("{game}\t{instance}\t{slot}");
        }
        InstanceCmd::Order { game, instances } => {
            if !game_exists(&pool, &game).await? {
                return Err(Error::UnknownGame(game).into());
            }
            let ordered = set_load_order(&pool, &data_dir(), &game, &instances).await?;
            for m in &ordered {
                println!("{game}\t{}\t{}", m.instance, m.load_order);
            }
        }
        InstanceCmd::Conflicts { game } => {
            if !game_exists(&pool, &game).await? {
                return Err(Error::UnknownGame(game).into());
            }
            for c in load_conflicts(&data_dir(), &game)? {
                println!("{}\t{}\t{}", c.dest, c.adapter, c.instances.join(","));
            }
        }
    }
    Ok(())
}

async fn install_mod_cli(
    pool: &tuxgt_core::SqlitePool,
    game: &str,
    instance_id: &str,
    adapter: &str,
    redownload: bool,
    with_requires: Option<String>,
    yes: bool,
    force: bool,
) -> Result<FileManifest, Box<dyn std::error::Error>> {
    let mut opts = InstallOpts {
        adapter: adapter.into(),
        redownload,
        with_requires,
        yes,
        force,
    };
    loop {
        match install_instance(
            pool,
            &data_dir(),
            &config_dir(),
            game,
            instance_id,
            &opts,
            None,
        )
        .await
        {
            Ok(m) => {
                note_staging(&data_dir(), game);
                return Ok(m);
            }
            Err(Error::MissingRequires(t)) => {
                return Err(
                    format!("missing requires {t}; pass --with-requires <instance>").into(),
                );
            }
            Err(Error::NeedConfirm(msg)) if !opts.yes => {
                let items: Vec<String> = msg
                    .split(':')
                    .nth(1)
                    .unwrap_or(&msg)
                    .split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect();
                confirm_list("foreign game-dir dests", &items, false)?;
                opts.yes = true;
            }
            Err(e) => return Err(e.into()),
        }
    }
}

async fn enable_mod_cli(
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
                let items: Vec<String> = msg
                    .split(':')
                    .nth(1)
                    .unwrap_or(&msg)
                    .split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect();
                confirm_list("foreign game-dir dests", &items, false)?;
                yes = true;
            }
            Err(e) => return Err(e.into()),
        }
    }
}

/// Keep/omit one dest through the same foreign-overwrite confirm the
/// install/enable paths use: `--yes` answers it, otherwise a TTY prompts.
async fn set_file_keep_cli(
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
                let items: Vec<String> = msg
                    .split(':')
                    .nth(1)
                    .unwrap_or(&msg)
                    .split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect();
                confirm_list("foreign game-dir dests", &items, false)?;
                yes = true;
            }
            Err(e) => return Err(e.into()),
        }
    }
}

async fn set_slot_cli(
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
                let items: Vec<String> = msg
                    .split(':')
                    .nth(1)
                    .unwrap_or(&msg)
                    .split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect();
                confirm_list("foreign game-dir dests", &items, false)?;
                yes = true;
            }
            Err(e) => return Err(e.into()),
        }
    }
}

async fn uninstall_mod_cli(
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
                let items: Vec<String> = msg
                    .split(':')
                    .nth(1)
                    .unwrap_or(&msg)
                    .split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect();
                confirm_list("foreign game-dir dests left in place", &items, false)?;
                yes = true;
            }
            Err(e) => return Err(e.into()),
        }
    }
}
