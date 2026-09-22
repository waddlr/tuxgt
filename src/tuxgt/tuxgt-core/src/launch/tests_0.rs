use super::testing::*;
use super::*;
use crate::game::{GameId, STANDALONE};
use crate::open_db;
use crate::plugin::PluginHost;
use crate::testing::{seed_game, SeedGame};
use sqlx::SqlitePool;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[test]
fn needs_nothing_hides_hook_and_apply() {
    let n = LaunchNeeds::from_manifests(&[], false, &[]);
    assert!(!n.show_radio());
    assert!(!n.hook_legal(true));
    assert!(!n.apply_legal());
    assert!(!n.channel_needed());
}

#[test]
fn needs_argv_wrappers_force_apply() {
    let n = LaunchNeeds::from_manifests(
        &[need_mf("preload", true, false)],
        false,
        &["gamescope".into()],
    );
    assert!(n.argv_wrappers);
    assert!(n.preload);
    assert!(!n.hook_legal(true));
    assert!(n.apply_legal());
    assert!(n.show_radio());
    assert!(!n.hook_preferred(true));
    let mango = LaunchNeeds::from_state(false, false, false, true);
    assert!(mango.apply_legal());
    assert!(!mango.hook_legal(true));
    assert!(is_argv_wrapper("mangohud"));
    assert!(!is_argv_wrapper("other"));
}

#[test]
fn needs_preload_or_env_allows_hook() {
    let preload = LaunchNeeds::from_manifests(&[need_mf("preload", true, false)], false, &[]);
    assert!(preload.hook_legal(true));
    assert!(!preload.hook_legal(false));
    assert!(preload.apply_legal());
    assert!(preload.hook_preferred(true));
    let env = LaunchNeeds::from_manifests(&[], true, &[]);
    assert!(env.hook_legal(true));
    assert!(env.env);
    let mod_env = LaunchNeeds::from_manifests(&[need_mf("install", true, true)], false, &[]);
    assert!(mod_env.env);
    assert!(!mod_env.install_only);
    assert!(mod_env.hook_legal(true));
}

#[test]
fn needs_install_only_collapses() {
    let n = LaunchNeeds::from_manifests(&[need_mf("install", true, false)], false, &[]);
    assert!(n.install_only);
    assert!(!n.show_radio());
    assert!(!n.hook_legal(true));
    assert!(!n.apply_legal());
    assert!(!n.channel_needed());
}

#[test]
fn needs_mixed_preload_install_is_preload() {
    let n = LaunchNeeds::from_manifests(
        &[
            need_mf("preload", true, false),
            need_mf("install", true, false),
        ],
        false,
        &[],
    );
    assert!(n.preload);
    assert!(!n.install_only);
    assert!(n.show_radio());
    assert!(n.hook_legal(true));
}

#[test]
fn needs_disabled_manifest_ignored() {
    let n = LaunchNeeds::from_manifests(&[need_mf("preload", false, false)], false, &[]);
    assert!(!n.channel_needed());
    assert!(!n.install_only);
}

fn test_host(dir: &Path) -> PluginHost {
    PluginHost::load_with(&[], dir).unwrap()
}

fn wrapper_host(dir: &Path) -> PluginHost {
    PluginHost::load_with(crate::FIRST_PARTY, dir).unwrap()
}

fn stub_paths(dir: &Path) -> LaunchPaths {
    let launcher = dir.join("tuxgt-launcher");
    std::fs::write(&launcher, b"#!/bin/sh\nexec \"$@\"\n").unwrap();
    LaunchPaths {
        launcher: Some(launcher),
        steam: None,
        heroic: None,
        umu: None,
        wine: None,
    }
}

async fn pool_dir() -> (SqlitePool, PathBuf) {
    let dir = std::env::temp_dir().join(format!(
        "tuxgt-e06-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let _ = tokio::fs::remove_dir_all(&dir).await;
    let pool = open_db(&dir).await.unwrap();
    (pool, dir)
}

#[test]
fn maps_steam_proton_names() {
    assert!(proton_dir_matches("proton 9.0", "proton_90"));
    assert!(proton_dir_matches("proton 8.0", "proton_80"));
    assert!(proton_dir_matches("proton 10.0", "proton_10"));
    assert!(proton_dir_matches(
        "proton - experimental",
        "proton_experimental"
    ));
    assert!(proton_dir_matches("proton 9.0", "9.0-176"));
    assert!(!proton_dir_matches("proton 9.0", "proton_80"));
    assert!(!proton_dir_matches("proton 1.0", "proton_10"));
}

#[test]
fn resolve_proton_from_common() {
    let root = std::env::temp_dir().join(format!(
        "tuxgt-e06-proton-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let p90 = root.join("steamapps/common/Proton 9.0");
    std::fs::create_dir_all(&p90).unwrap();
    std::fs::write(p90.join("proton"), b"#!/bin/sh\n").unwrap();
    let exp = root.join("steamapps/common/Proton - Experimental");
    std::fs::create_dir_all(&exp).unwrap();
    std::fs::write(exp.join("proton"), b"#!/bin/sh\n").unwrap();
    assert_eq!(
        resolve_proton_in("proton_90", &[root.clone()]).as_deref(),
        Some(p90.join("proton").as_path())
    );
    assert_eq!(
        resolve_proton_in("proton_experimental", &[root.clone()]).as_deref(),
        Some(exp.join("proton").as_path())
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn steam_shortcut_rungameid() {
    assert_eq!(
        steam_client_args(STANDALONE, "4139290751"),
        Some(vec!["steam://rungameid/17778118404213833728".into()])
    );
    assert_eq!(
        steam_client_args("", "814380"),
        Some(vec!["steam://rungameid/814380".into()])
    );
}

#[test]
fn tokenize_and_percent_command() {
    let (w, extra, env) =
        parse_launch_options(Some("PROTON_LOG=1 mangohud --dlsym %command% -dx11"));
    assert_eq!(w, vec!["mangohud", "--dlsym"]);
    assert_eq!(extra, vec!["-dx11"]);
    assert_eq!(env, vec![("PROTON_LOG".to_string(), "1".to_string())]);
    let (w, extra, _) = parse_launch_options(Some("-windowed"));
    assert!(w.is_empty());
    assert_eq!(extra, vec!["-windowed"]);
}

#[test]
fn heroic_wrappers_split() {
    let w = parse_heroic_wrappers(Some("gamemoderun; mangohud --dlsym"));
    assert_eq!(
        w,
        vec![
            vec!["gamemoderun".to_string()],
            vec!["mangohud".to_string(), "--dlsym".to_string()]
        ]
    );
}

#[test]
fn argv_folds_outer_first() {
    let spec = LaunchSpec {
        id: GameId::parse("manual:standalone:abcdefgh").unwrap(),
        cwd: PathBuf::from("/tmp"),
        env: BTreeMap::new(),
        wrappers: vec![vec!["mangohud".into()], vec!["/tmp/tuxgt-launcher".into()]]
            .into_boxed_slice(),
        program: PathBuf::from("/usr/bin/true"),
        args: vec!["--x".into()].into_boxed_slice(),
        owned: true,
    };
    assert_eq!(
        spec.argv(),
        vec!["mangohud", "/tmp/tuxgt-launcher", "/usr/bin/true", "--x"]
    );
}

#[tokio::test]
async fn native_composes_and_does_not_write() {
    let true_bin = PathBuf::from("/usr/bin/true");
    if !true_bin.is_file() {
        return;
    }
    let (pool, dir) = pool_dir().await;
    let host = test_host(&dir);
    let paths = stub_paths(&dir);
    seed_game(
        &pool,
        SeedGame {
            id: "manual:standalone:aaaaaaaa",
            manager: "manual",
            store: "standalone",
            game_id: "aaaaaaaa",
            name: Some("t"),
            exe_path: Some(true_bin.to_str().unwrap()),
            launch_options: Some("mangohud %command% --foo"),
            env: Some("{\"A\":\"1\"}"),
            wrapper: Some("gamemoderun"),
            detected_platform: Some("native"),
            ..Default::default()
        },
    )
    .await;
    let spec = build_launch_spec(&pool, &host, "manual:standalone:aaaaaaaa", &paths, &dir)
        .await
        .unwrap();
    let argv = spec.argv();
    assert_eq!(argv[0], "mangohud");
    assert_eq!(argv[1], "gamemoderun");
    assert_eq!(argv[2], paths.launcher.as_ref().unwrap().to_string_lossy());
    assert_eq!(argv[3], true_bin.to_string_lossy());
    assert_eq!(argv[4], "--foo");
    assert_eq!(spec.env.get("A").map(String::as_str), Some("1"));
    let fp: (Option<String>, Option<String>) =
        sqlx::query_as("SELECT fingerprint, launch_options FROM games WHERE id = ?")
            .bind("manual:standalone:aaaaaaaa")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(fp.0.is_none());
    assert_eq!(fp.1.as_deref(), Some("mangohud %command% --foo"));
    let _ = tokio::fs::remove_dir_all(&dir).await;
}

#[tokio::test]
async fn does_not_duplicate_launcher() {
    let true_bin = PathBuf::from("/usr/bin/true");
    if !true_bin.is_file() {
        return;
    }
    let (pool, dir) = pool_dir().await;
    let host = test_host(&dir);
    let paths = stub_paths(&dir);
    let opt = format!("{} %command%", paths.launcher.as_ref().unwrap().display());
    seed_game(
        &pool,
        SeedGame {
            id: "manual:standalone:bbbbbbbb",
            manager: "manual",
            store: "standalone",
            game_id: "bbbbbbbb",
            name: Some("t"),
            exe_path: Some(true_bin.to_str().unwrap()),
            launch_options: Some(&opt),
            detected_platform: Some("native"),
            ..Default::default()
        },
    )
    .await;
    let spec = build_launch_spec(&pool, &host, "manual:standalone:bbbbbbbb", &paths, &dir)
        .await
        .unwrap();
    let n = spec
        .argv()
        .iter()
        .filter(|a| {
            Path::new(a)
                .file_name()
                .is_some_and(|n| n == "tuxgt-launcher")
        })
        .count();
    assert_eq!(n, 1);
    let _ = tokio::fs::remove_dir_all(&dir).await;
}

async fn manual_row(
    id: &str,
    exe: &Path,
    launch_options: Option<&str>,
) -> (SqlitePool, PathBuf, LaunchPaths, PluginHost) {
    let (pool, dir) = pool_dir().await;
    let host = wrapper_host(&dir);
    let paths = stub_paths(&dir);
    seed_game(
        &pool,
        SeedGame {
            id,
            manager: "manual",
            store: "standalone",
            game_id: id.rsplit(':').next().unwrap(),
            name: Some("t"),
            exe_path: Some(exe.to_str().unwrap()),
            launch_options,
            detected_platform: Some("native"),
            ..Default::default()
        },
    )
    .await;
    (pool, dir, paths, host)
}

#[tokio::test]
async fn manual_row_composes_overlay_wrappers() {
    let true_bin = PathBuf::from("/usr/bin/true");
    if !true_bin.is_file() {
        return;
    }
    let id = "manual:standalone:cccccccc";
    let (pool, dir, paths, host) = manual_row(id, &true_bin, None).await;
    // stored in reverse: composition follows the def table, not set order
    for w in ["mangohud", "gamemode", "gamescope"] {
        crate::set_wrapper(&pool, id, w).await.unwrap();
    }
    let spec = build_launch_spec(&pool, &host, id, &paths, &dir)
        .await
        .unwrap();
    assert_eq!(
        spec.argv(),
        vec![
            "gamescope".to_string(),
            "gamemoderun".to_string(),
            "mangohud".to_string(),
            paths
                .launcher
                .as_ref()
                .unwrap()
                .to_string_lossy()
                .into_owned(),
            true_bin.to_string_lossy().into_owned(),
        ]
    );
    // launch does not write the games row
    let row: (Option<String>, Option<String>) =
        sqlx::query_as("SELECT launch_options, wrapper FROM games WHERE id = ?")
            .bind(id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(row, (None, None));
    let _ = tokio::fs::remove_dir_all(&dir).await;
}
