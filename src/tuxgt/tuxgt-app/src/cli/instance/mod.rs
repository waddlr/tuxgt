mod install;
mod mutate;
mod update;

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
            password,
            slot,
        } => {
            // Same validator the persisted setter and the GUI use, so no
            // surface can accept an adapter the others reject.
            if let Some(a) = adapter.as_deref() {
                tuxgt_core::validate_adapter(a)?;
            }
            if let Some(s) = slot.as_deref() {
                if !tuxgt_core::is_self_slot(s) {
                    tuxgt_core::slot_dll(s)?;
                }
            }
            let m = install::install_mod_cli(
                pool,
                &game,
                &instance,
                adapter.as_deref(),
                redownload,
                with_requires,
                yes,
                force,
                password,
                slot,
            )
            .await?;
            println!("{}\t{}\t{}", m.game, m.instance, m.files.len());
            note_auto_apply(pool, &game).await;
        }
        InstanceCmd::Enable {
            game,
            instance,
            yes,
        } => {
            if !game_exists(pool, &game).await? {
                return Err(Error::UnknownGame(game).into());
            }
            check_manifest_instance(&game, &instance)?;
            let m = mutate::enable_mod_cli(pool, &game, &instance, true, yes).await?;
            note_staging(&data_dir(), &game);
            println!("{}\t{}\tenabled", m.game, m.instance);
        }
        InstanceCmd::Disable { game, instance } => {
            if !game_exists(pool, &game).await? {
                return Err(Error::UnknownGame(game).into());
            }
            check_manifest_instance(&game, &instance)?;
            let m = set_instance_enabled(pool, &data_dir(), &game, &instance, false, true).await?;
            println!("{}\t{}\tdisabled", m.game, m.instance);
        }
        InstanceCmd::Uninstall {
            game,
            instance,
            yes,
        } => {
            mutate::uninstall_mod_cli(pool, &game, &instance, yes).await?;
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
                    let kind = if file_is_loaddll(&f.dest, &m.include, f.load) {
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
            (Some(FileAction::Enable) | Some(FileAction::Disable), Some(dest)) => {
                if !game_exists(pool, &game).await? {
                    return Err(Error::UnknownGame(game).into());
                }
                check_manifest_instance(&game, &instance)?;
                let on = matches!(action, Some(FileAction::Enable));
                mutate::set_file_keep_cli(pool, &game, &instance, &dest, on, yes).await?;
                let word = if on { "enabled" } else { "disabled" };
                println!("{game}\t{instance}\t{dest}\t{word}");
                tracing::info!(
                    game = game.as_str(),
                    instance = instance.as_str(),
                    dest = dest.as_str(),
                    enabled = on,
                    "file keep set"
                );
            }
            (Some(FileAction::Loaddll) | Some(FileAction::Include), Some(dest)) => {
                if !game_exists(pool, &game).await? {
                    return Err(Error::UnknownGame(game).into());
                }
                check_manifest_instance(&game, &instance)?;
                let load = matches!(action, Some(FileAction::Loaddll));
                tuxgt_core::set_file_load(pool, &data_dir(), &game, &instance, &dest, load).await?;
                let word = if load { "loaddll" } else { "include" };
                println!("{game}\t{instance}\t{dest}\t{word}");
                tracing::info!(
                    game = game.as_str(),
                    instance = instance.as_str(),
                    dest = dest.as_str(),
                    load,
                    "file load set"
                );
            }
            _ => {
                return Err(
                    "usage: tuxgt instance files <game> <instance> [enable|disable|loaddll|include <dest>]"
                        .into(),
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
                if !game_exists(pool, &game).await? {
                    return Err(Error::UnknownGame(game).into());
                }
                check_manifest_instance(&game, &instance)?;
                let on = matches!(action, FileKeep::Enable);
                let host = PluginHost::load()?;
                mutate_game(pool, &data_dir(), &host, &game, async {
                    set_mod_env_enabled(&data_dir(), &game, &instance, &key, on)
                })
                .await?;
                let word = if on { "enabled" } else { "disabled" };
                println!("{game}\t{instance}\t{key}\t{word}");
                tracing::info!(
                    game = game.as_str(),
                    instance = instance.as_str(),
                    key = key.as_str(),
                    enabled = on,
                    "mod env set"
                );
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
            if !game_exists(pool, &game).await? {
                return Err(Error::UnknownGame(game).into());
            }
            check_manifest_instance(&game, &instance)?;
            mutate::set_slot_cli(pool, &game, &instance, &slot, yes).await?;
            println!("{game}\t{instance}\t{slot}");
        }
        InstanceCmd::Order { game, instances } => {
            if !game_exists(pool, &game).await? {
                return Err(Error::UnknownGame(game).into());
            }
            let ordered = set_load_order(pool, &data_dir(), &game, &instances).await?;
            for m in &ordered {
                println!("{game}\t{}\t{}", m.instance, m.load_order);
            }
        }
        InstanceCmd::Conflicts { game } => {
            if !game_exists(pool, &game).await? {
                return Err(Error::UnknownGame(game).into());
            }
            for c in load_conflicts(&data_dir(), &game)? {
                println!("{}\t{}\t{}", c.dest, c.adapter, c.instances.join(","));
            }
        }
        InstanceCmd::Check {
            game,
            instance,
            yes,
            slot,
        } => {
            if !game_exists(pool, &game).await? {
                return Err(Error::UnknownGame(game).into());
            }
            update::check_cli(pool, &game, instance.as_deref(), yes, slot).await?;
        }
        InstanceCmd::Update {
            game,
            instance,
            yes,
            slot,
            password,
        } => {
            if !game_exists(pool, &game).await? {
                return Err(Error::UnknownGame(game).into());
            }
            update::update_cli(
                pool,
                &game,
                instance.as_deref(),
                yes,
                slot,
                password,
            )
            .await?;
        }
        InstanceCmd::Resync {
            game,
            instance,
            yes,
        } => {
            if !game_exists(pool, &game).await? {
                return Err(Error::UnknownGame(game).into());
            }
            mutate::resync_cli(&game, instance.as_deref(), yes)?;
        }
    }
    Ok(())
}
