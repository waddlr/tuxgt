use super::testing::*;
use super::*;
use crate::testing::{seed_game, SeedGame};
use crate::Error;
use std::fs;

async fn root_scratch(label: &str) -> (std::path::PathBuf, sqlx::SqlitePool) {
    let dir = std::env::temp_dir().join(format!(
        "tuxgt-game-root-{label}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let pool = crate::open_db(&dir.join("data")).await.unwrap();
    (dir, pool)
}

#[tokio::test]
async fn game_root_prefers_nested_store_exe_parent() {
    let (dir, pool) = root_scratch("nested").await;
    let install = dir.join("Game");
    let nested = install.join("Binaries").join("Win64");
    fs::create_dir_all(&nested).unwrap();
    let gid = "manual:standalone:rootnest1";
    seed_game(
        &pool,
        SeedGame {
            id: gid,
            name: Some("nested"),
            install_dir: Some(install.to_str().unwrap()),
            exe_path: Some(nested.join("Game.exe").to_str().unwrap()),
            ..Default::default()
        },
    )
    .await;
    assert_eq!(game_root(&pool, gid).await.unwrap(), nested);
    assert_eq!(
        game_roots(&pool, gid).await.unwrap(),
        vec![nested.clone(), install]
    );
    let _ = fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn game_root_detected_wins_over_store() {
    let (dir, pool) = root_scratch("det").await;
    let mo2 = dir.join("MO2");
    let game = dir.join("Stock Game");
    fs::create_dir_all(&mo2).unwrap();
    fs::create_dir_all(&game).unwrap();
    let gid = "steam:steam:rootmo2";
    seed_game(
        &pool,
        SeedGame {
            id: gid,
            manager: "steam",
            store: "steam",
            game_id: "1",
            name: Some("mo2"),
            install_dir: Some(mo2.to_str().unwrap()),
            exe_path: Some(mo2.join("ModOrganizer.exe").to_str().unwrap()),
            detected_exe_path: Some(game.join("SkyrimSE.exe").to_str().unwrap()),
            ..Default::default()
        },
    )
    .await;
    assert_eq!(game_root(&pool, gid).await.unwrap(), game);
    assert_eq!(game_roots(&pool, gid).await.unwrap(), vec![game, mo2]);
    let _ = fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn game_root_override_wins_and_empty_override_falls_through() {
    let (dir, pool) = root_scratch("ovr").await;
    let install = dir.join("pkg");
    let det = dir.join("detected");
    let ovr = dir.join("override");
    fs::create_dir_all(&install).unwrap();
    fs::create_dir_all(&det).unwrap();
    fs::create_dir_all(&ovr).unwrap();
    let gid = "manual:standalone:rootovr1";
    seed_game(
        &pool,
        SeedGame {
            id: gid,
            name: Some("ovr"),
            install_dir: Some(install.to_str().unwrap()),
            exe_path: Some(install.join("Store.exe").to_str().unwrap()),
            detected_exe_path: Some(det.join("Det.exe").to_str().unwrap()),
            override_exe_path: Some(ovr.join("Ovr.exe").to_str().unwrap()),
            ..Default::default()
        },
    )
    .await;
    assert_eq!(game_root(&pool, gid).await.unwrap(), ovr);
    sqlx::query("UPDATE games SET override_exe_path = '' WHERE id = ?")
        .bind(gid)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(game_root(&pool, gid).await.unwrap(), det);
    let _ = fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn game_root_falls_back_to_install_dir() {
    let (dir, pool) = root_scratch("fb").await;
    let install = dir.join("only-install");
    fs::create_dir_all(&install).unwrap();
    let gid = "manual:standalone:rootfb1";
    seed_game(
        &pool,
        SeedGame {
            id: gid,
            name: Some("fb"),
            install_dir: Some(install.to_str().unwrap()),
            ..Default::default()
        },
    )
    .await;
    assert_eq!(game_root(&pool, gid).await.unwrap(), install);
    assert_eq!(game_roots(&pool, gid).await.unwrap(), vec![install]);
    let _ = fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn game_root_joins_relative_exe_to_install_dir() {
    let (dir, pool) = root_scratch("rel").await;
    let install = dir.join("pkg");
    let nested = install.join("bin");
    fs::create_dir_all(&nested).unwrap();
    let gid = "manual:standalone:rootrel1";
    seed_game(
        &pool,
        SeedGame {
            id: gid,
            name: Some("rel"),
            install_dir: Some(install.to_str().unwrap()),
            exe_path: Some("bin/Game.exe"),
            ..Default::default()
        },
    )
    .await;
    assert_eq!(game_root(&pool, gid).await.unwrap(), nested);
    let _ = fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn game_root_missing_exe_errors() {
    let (dir, pool) = root_scratch("miss").await;
    let gid = "manual:standalone:rootmiss1";
    seed_game(
        &pool,
        SeedGame {
            id: gid,
            name: Some("miss"),
            ..Default::default()
        },
    )
    .await;
    assert!(matches!(
        game_root(&pool, gid).await.unwrap_err(),
        Error::MissingExe(_)
    ));
    let _ = fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn remove_scrubs_legacy_install_dir_copies() {
    use crate::install::remove_with_promotion;
    use crate::sha256_hex;
    use std::collections::BTreeSet;

    let (dir, pool) = root_scratch("scrub").await;
    let install = dir.join("pkg");
    let nested = install.join("bin");
    fs::create_dir_all(&nested).unwrap();
    let gid = "manual:standalone:rootscrub1";
    seed_game(
        &pool,
        SeedGame {
            id: gid,
            name: Some("scrub"),
            install_dir: Some(install.to_str().unwrap()),
            exe_path: Some(nested.join("Game.exe").to_str().unwrap()),
            ..Default::default()
        },
    )
    .await;
    let data = dir.join("data");
    let bytes = b"legacy-dll";
    let sha = sha256_hex(bytes);
    let legacy = install.join("dxgi.dll");
    fs::write(&legacy, bytes).unwrap();
    let mut m = test_manifest(gid, "reshade", "dxgi.dll", &sha);
    crate::write_manifest(&data, &m).unwrap();
    let roots = game_roots(&pool, gid).await.unwrap();
    assert_eq!(&roots[0], &nested);
    assert_eq!(&roots[1], &install);
    let excluded = [(m.instance.clone(), "dxgi.dll".into())]
        .into_iter()
        .collect::<BTreeSet<_>>();
    remove_with_promotion(
        &data,
        gid,
        m.clone(),
        &excluded,
        &mut m.backups,
        &["dxgi.dll".into()],
        &roots,
        None,
    )
    .unwrap();
    assert!(
        !legacy.exists(),
        "legacy package-root copy must be scrubbed"
    );
    let _ = fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn remove_keeps_legacy_copy_when_another_claim_owns_it() {
    use crate::install::remove_with_promotion;
    use crate::sha256_hex;
    use std::collections::BTreeSet;

    let (dir, pool) = root_scratch("scrubshare").await;
    let install = dir.join("pkg");
    let nested = install.join("bin");
    fs::create_dir_all(&nested).unwrap();
    let gid = "manual:standalone:rootscrub3";
    seed_game(
        &pool,
        SeedGame {
            id: gid,
            name: Some("scrubshare"),
            install_dir: Some(install.to_str().unwrap()),
            exe_path: Some(nested.join("Game.exe").to_str().unwrap()),
            ..Default::default()
        },
    )
    .await;
    let data = dir.join("data");
    let bytes = b"shared-dll";
    let sha = sha256_hex(bytes);
    let legacy = install.join("dxgi.dll");
    fs::write(&legacy, bytes).unwrap();
    let mut loser = test_manifest(gid, "aaa", "dxgi.dll", &sha);
    loser.load_order = 0;
    let mut winner = test_manifest(gid, "bbb", "dxgi.dll", &sha);
    winner.load_order = 1;
    crate::write_manifest(&data, &loser).unwrap();
    crate::write_manifest(&data, &winner).unwrap();
    let roots = game_roots(&pool, gid).await.unwrap();
    let excluded = [(loser.instance.clone(), "dxgi.dll".into())]
        .into_iter()
        .collect::<BTreeSet<_>>();
    remove_with_promotion(
        &data,
        gid,
        loser.clone(),
        &excluded,
        &mut loser.backups,
        &["dxgi.dll".into()],
        &roots,
        None,
    )
    .unwrap();
    assert!(
        legacy.exists(),
        "shared package-root copy must stay for the remaining claimant"
    );
    let _ = fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn remove_scrubs_legacy_when_claim_already_disabled() {
    use crate::install::remove_with_promotion;
    use crate::sha256_hex;
    use std::collections::BTreeSet;

    let (dir, pool) = root_scratch("scruboff").await;
    let install = dir.join("pkg");
    let nested = install.join("bin");
    fs::create_dir_all(&nested).unwrap();
    let gid = "manual:standalone:rootscrub2";
    seed_game(
        &pool,
        SeedGame {
            id: gid,
            name: Some("scruboff"),
            install_dir: Some(install.to_str().unwrap()),
            exe_path: Some(nested.join("Game.exe").to_str().unwrap()),
            ..Default::default()
        },
    )
    .await;
    let data = dir.join("data");
    let bytes = b"legacy-off";
    let sha = sha256_hex(bytes);
    let legacy = install.join("dxgi.dll");
    fs::write(&legacy, bytes).unwrap();
    let mut m = test_manifest(gid, "reshade", "dxgi.dll", &sha);
    m.enabled = false;
    crate::write_manifest(&data, &m).unwrap();
    let roots = game_roots(&pool, gid).await.unwrap();
    let excluded = [(m.instance.clone(), "dxgi.dll".into())]
        .into_iter()
        .collect::<BTreeSet<_>>();
    remove_with_promotion(
        &data,
        gid,
        m.clone(),
        &excluded,
        &mut m.backups,
        &["dxgi.dll".into()],
        &roots,
        None,
    )
    .unwrap();
    assert!(
        !legacy.exists(),
        "disabled claim must still scrub legacy package-root copy"
    );
    let _ = fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn remove_legacy_does_not_steal_backup_when_live_also_owned() {
    use crate::install::remove_with_promotion;
    use crate::sha256_hex;
    use std::collections::BTreeSet;

    let (dir, pool) = root_scratch("scrubbak").await;
    let install = dir.join("pkg");
    let nested = install.join("bin");
    fs::create_dir_all(&nested).unwrap();
    let gid = "manual:standalone:rootscrub4";
    seed_game(
        &pool,
        SeedGame {
            id: gid,
            name: Some("scrubbak"),
            install_dir: Some(install.to_str().unwrap()),
            exe_path: Some(nested.join("Game.exe").to_str().unwrap()),
            ..Default::default()
        },
    )
    .await;
    let data = dir.join("data");
    let ours = b"ours";
    let sha = sha256_hex(ours);
    let legacy = install.join("dxgi.dll");
    let live = nested.join("dxgi.dll");
    fs::write(&legacy, ours).unwrap();
    fs::write(&live, ours).unwrap();
    let mut m = test_manifest(gid, "reshade", "dxgi.dll", &sha);
    let bak_rel = "backups/orig.dll";
    fs::create_dir_all(data.join("backups")).unwrap();
    fs::write(data.join(bak_rel), b"foreign-orig").unwrap();
    m.backups.insert("dxgi.dll".into(), bak_rel.into());
    crate::write_manifest(&data, &m).unwrap();
    let roots = game_roots(&pool, gid).await.unwrap();
    let excluded = [(m.instance.clone(), "dxgi.dll".into())]
        .into_iter()
        .collect::<BTreeSet<_>>();
    let outcome = remove_with_promotion(
        &data,
        gid,
        m.clone(),
        &excluded,
        &mut m.backups,
        &["dxgi.dll".into()],
        &roots,
        None,
    )
    .unwrap();
    assert!(!legacy.exists(), "package leftover deleted");
    assert!(
        live.exists() || outcome.removal.restored.iter().any(|d| d == "dxgi.dll"),
        "backup must restore on the live root, not be consumed by the package scrub"
    );
    if live.exists() {
        assert_eq!(fs::read(&live).unwrap(), b"foreign-orig");
    }
    let _ = fs::remove_dir_all(&dir);
}
