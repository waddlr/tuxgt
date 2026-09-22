use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use sqlx::SqlitePool;

use super::*;
use crate::game::{GameId, STANDALONE};
use crate::prewire::is_dll;
use crate::wrapper::{game_wrappers, wrapper_defs};
use crate::PluginHost;
use crate::{
    custom_env, enabled_global_env_pairs, find_enabled_knob, game_manifests, knob_rows, Error,
    Result,
};

pub async fn build_launch_spec(
    pool: &SqlitePool,
    host: &PluginHost,
    id: &str,
    paths: &LaunchPaths,
    data_dir: &Path,
) -> Result<LaunchSpec> {
    GameId::parse(id)?;
    let row = sqlx::query_as::<_, Row>(
        "SELECT id, manager, store, game_id, install_dir, exe_path, prefix_path, proton,
                launch_options, env, wrapper,
                detected_platform, detected_prefix_path, detected_proton,
                override_exe_path, override_platform, override_prefix_path, override_proton
         FROM games WHERE id = ?",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| Error::UnknownGame(id.into()))?;

    let gid = GameId::new(&row.manager, &row.store, &row.game_id)?;
    if row.manager == "steam" {
        // Vanilla contract: Steam applies its own launch options / Proton;
        // no wrappers, no env merge on the rungameid path.
        tracing::debug!(game = id, manager = "steam", "launch spec");
        return steam_applaunch(gid, &row, paths);
    }
    if row.manager == "heroic" {
        // Same contract: Heroic owns the process (Wine/Proton, wrappers,
        // env) via its stored settings. Mods reach Heroic games through
        // Apply (GamesConfig), never through a tuxgt-owned process.
        tracing::debug!(game = id, manager = "heroic", "launch spec");
        return heroic_dispatch(gid, &row, paths);
    }

    let exe = pick(&row.override_exe_path, &None, &row.exe_path);
    if exe.is_empty() {
        return Err(Error::MissingExe(id.into()));
    }
    let mut exe = PathBuf::from(exe);
    let install = row.install_dir.as_deref().map(PathBuf::from);
    if !exe.is_absolute() {
        if let Some(dir) = &install {
            exe = dir.join(&exe);
        }
    }
    let prefix = pick(
        &row.override_prefix_path,
        &row.detected_prefix_path,
        &row.prefix_path,
    );
    let prefix = if prefix.is_empty() {
        None
    } else {
        Some(PathBuf::from(prefix))
    };
    let proton = pick(&row.override_proton, &row.detected_proton, &row.proton);
    let proton = if proton.is_empty() {
        None
    } else {
        Some(proton.to_string())
    };
    let platform = resolve_platform(&row);

    let mut env = parse_env(row.env.as_deref().unwrap_or(""))?;
    let (opt_wrap, extra_args, opt_env) = parse_launch_options(row.launch_options.as_deref());
    for (k, v) in opt_env {
        env.entry(k).or_insert(v);
    }
    // Store env JSON, then enabled global knobs, then enabled game knobs,
    // then custom env. Later wins. Skip disabled (inherit). Empty game
    // value is override-off (`VAR=`). Skip knobs whose provider is disabled
    // (E12).
    for (k, v) in enabled_global_env_pairs(pool, Some(host)).await? {
        env.insert(k, v);
    }
    for row in knob_rows(pool, id).await? {
        if !row.enabled {
            continue;
        }
        if let Some(def) = find_enabled_knob(host, &row.knob) {
            if row.value.is_empty() {
                for var in def.env_vars() {
                    env.insert(var.to_string(), String::new());
                }
            } else if let Some(pairs) = def.env_pairs(&row.value) {
                for (k, v) in pairs {
                    env.insert(k, v);
                }
            }
        }
    }
    for (k, v) in custom_env(pool, id).await? {
        env.insert(k, v);
    }
    // E74: enabled manifest [[env]] after custom env (generic last-wins,
    // WINEDLLOVERRIDES stem-merge below).
    apply_mod_env(&mut env, data_dir, id)?;

    let mut wrappers = Vec::new();
    if !opt_wrap.is_empty() {
        wrappers.push(opt_wrap);
    }
    wrappers.extend(parse_heroic_wrappers(row.wrapper.as_deref()));
    // Overlay wrappers (E42): owned path only, stored selection, table order,
    // appended before the launcher so it stays innermost (LD_PRELOAD still
    // reaches the game). A program basename already composed is not added.
    let selected = game_wrappers(pool, id).await?;
    for def in wrapper_defs(host) {
        if !selected.iter().any(|w| w == def.id) {
            continue;
        }
        if wrappers
            .iter()
            .flatten()
            .any(|t| basename_eq(t, def.program))
        {
            continue;
        }
        let mut argv = Vec::with_capacity(1 + def.args.len());
        argv.push(def.program.to_string());
        argv.extend(def.args.iter().map(|a| (*a).to_string()));
        wrappers.push(argv);
    }
    if !has_launcher(&wrappers, paths.launcher.as_deref()) {
        let launcher = paths.launcher.as_ref().ok_or(Error::MissingLauncher)?;
        wrappers.push(vec![launcher.to_string_lossy().into_owned()]);
    }

    let cwd = exe
        .parent()
        .map(Path::to_path_buf)
        .filter(|p| !p.as_os_str().is_empty())
        .or(install.clone())
        .unwrap_or_else(|| PathBuf::from("."));

    let (program, mut runner_args) = match platform.as_str() {
        "native" => (exe.clone(), Vec::new()),
        "proton" => proton_runner(
            &row,
            &exe,
            prefix.as_deref(),
            proton.as_deref(),
            paths,
            &mut env,
        )?,
        "wine" => wine_runner(
            id,
            &exe,
            prefix.as_deref(),
            proton.as_deref(),
            paths,
            &mut env,
        )?,
        other => {
            return Err(Error::MissingRunner(format!("{id} ({other})")));
        }
    };
    runner_args.extend(extra_args);
    if let Some(so) = crate::session::find_so_for_game(pool, data_dir, id).await {
        env.insert("TUXGT_LAUNCHER_SO".into(), so.to_string_lossy().into_owned());
    }
    add_managed_env(&mut env, data_dir, id)?;
    if let Some(dir) = row.install_dir.as_deref() {
        pv_rw_add(&mut env, dir);
    }
    if let Some(pfx) = prefix.as_deref() {
        pv_rw_add(&mut env, &pfx.to_string_lossy());
    }
    if platform == "proton" {
        if proton_optiscaler_flavor(proton.as_deref()) && has_proton_env_plan(data_dir, id)? {
            // Env plan: Proton's built-in OptiScaler. Its outputs stay in
            // the prefix, unmanaged.
            env.insert("PROTON_USE_OPTISCALER".into(), "1".into());
        }
    }
    if platform == "proton" || platform == "wine" {
        for m in game_manifests(data_dir, id)? {
            if !m.enabled || m.adapter != "install" {
                continue;
            }
            for f in &m.files {
                if is_dll(&f.dest) {
                    if let Some(stem) = Path::new(&f.dest).file_stem().and_then(|s| s.to_str()) {
                        wine_dll_override(&mut env, stem);
                    }
                }
            }
        }
    }
    tracing::debug!(game = id, manager = "standalone", env_entries = env.len(), "launch spec");

    Ok(LaunchSpec {
        id: gid,
        cwd,
        env,
        wrappers: wrappers.into_boxed_slice(),
        program,
        args: runner_args.into_boxed_slice(),
        owned: true,
    })
}

pub(crate) fn steam_applaunch(gid: GameId, row: &Row, paths: &LaunchPaths) -> Result<LaunchSpec> {
    let steam = paths.steam.clone().ok_or(Error::MissingSteam)?;
    if row.game_id.is_empty() {
        return Err(Error::InvalidGameId(row.id.clone()));
    }
    let args = steam_client_args(&row.store, &row.game_id)
        .ok_or_else(|| Error::InvalidGameId(row.id.clone()))?;
    let cwd = steam
        .parent()
        .map(Path::to_path_buf)
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| PathBuf::from("."));
    Ok(LaunchSpec {
        id: gid,
        cwd,
        env: BTreeMap::new(),
        wrappers: Box::default(),
        program: steam,
        args: args.into_boxed_slice(),
        owned: false,
    })
}

/// `heroic "heroic://launch?appName=<id>&runner=<runner>"`. The client applies
/// its stored Wine/Proton, wrappers, and env; tuxgt never manages the process.
/// `runner` is the Heroic runner name (`gog`, `sideload`, `legendary`,
/// `nile`); standalone rows dispatch with Heroic's `sideload` runner.
pub(crate) fn heroic_dispatch(gid: GameId, row: &Row, paths: &LaunchPaths) -> Result<LaunchSpec> {
    let heroic = paths.heroic.clone().ok_or(Error::MissingHeroic)?;
    if row.game_id.is_empty() {
        return Err(Error::InvalidGameId(row.id.clone()));
    }
    let mut url = format!("heroic://launch?appName={}", row.game_id);
    if matches!(row.store.as_str(), "gog" | "legendary" | "nile") || row.store == STANDALONE {
        // Wire value: Heroic still calls this runner `sideload`.
        let runner = if row.store == STANDALONE {
            "sideload"
        } else {
            row.store.as_str()
        };
        url.push_str("&runner=");
        url.push_str(runner);
    }
    let cwd = heroic
        .parent()
        .map(Path::to_path_buf)
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| PathBuf::from("."));
    Ok(LaunchSpec {
        id: gid,
        cwd,
        env: BTreeMap::new(),
        wrappers: Box::default(),
        program: heroic,
        args: vec![url].into_boxed_slice(),
        owned: false,
    })
}

/// `steam steam://rungameid/<CGameID>`. Owned: CGameID is the appid.
/// Standalone rows (non-Steam shortcuts): `(vdf_u32 << 32) | 0x02000000`.
pub(crate) fn steam_client_args(store: &str, game_id: &str) -> Option<Vec<String>> {
    let app: u32 = game_id.parse().ok()?;
    let rid = if store == STANDALONE {
        ((app as u64) << 32) | 0x0200_0000
    } else {
        app as u64
    };
    Some(vec![format!("steam://rungameid/{rid}")])
}
