use super::testing::*;
use crate::{disable_knob, set_global_knob, set_handle, set_knob, unset_global_knob};
use std::fs;

#[tokio::test]
async fn session_merges_global_then_game() {
    let _g = crate::env::TEST_ENV_LOCK.lock().unwrap();
    let prev_mango = std::env::var_os("MANGOHUD");
    let prev_hud = std::env::var_os("DXVK_HUD");
    std::env::remove_var("MANGOHUD");
    std::env::remove_var("DXVK_HUD");
    let (pool, dir, host, id) = setup().await;
    set_global_knob(&pool, "mangohud", "1").await.unwrap();
    set_handle(&pool, &dir, &host, &id, true).await.unwrap();
    crate::sync_session(&pool, &dir, &host, &id).await.unwrap();
    let text = fs::read_to_string(steam_conf(&dir)).unwrap();
    assert!(text.contains("MANGOHUD=1"), "{text}");
    set_knob(&pool, &id, "dxvk-hud", "1").await.unwrap();
    crate::sync_session(&pool, &dir, &host, &id).await.unwrap();
    let text = fs::read_to_string(steam_conf(&dir)).unwrap();
    assert!(text.contains("MANGOHUD=1"), "{text}");
    assert!(text.contains("DXVK_HUD=1"), "{text}");
    set_knob(&pool, &id, "mangohud", "1").await.unwrap();
    disable_knob(&pool, &id, "mangohud").await.unwrap();
    crate::sync_session(&pool, &dir, &host, &id).await.unwrap();
    let text = fs::read_to_string(steam_conf(&dir)).unwrap();
    assert!(text.contains("MANGOHUD=1"), "{text}");
    crate::unset_knob(&pool, &id, "mangohud").await.unwrap();
    unset_global_knob(&pool, "mangohud").await.unwrap();
    crate::sync_session(&pool, &dir, &host, &id).await.unwrap();
    let text = fs::read_to_string(steam_conf(&dir)).unwrap();
    assert!(!text.contains("MANGOHUD="), "{text}");
    match prev_mango {
        Some(v) => std::env::set_var("MANGOHUD", v),
        None => std::env::remove_var("MANGOHUD"),
    }
    match prev_hud {
        Some(v) => std::env::set_var("DXVK_HUD", v),
        None => std::env::remove_var("DXVK_HUD"),
    }
    let _ = tokio::fs::remove_dir_all(&dir).await;
}

#[tokio::test]
async fn sync_handle_sessions_updates_applied_inject0() {
    let _g = crate::env::TEST_ENV_LOCK.lock().unwrap();
    let prev_mango = std::env::var_os("MANGOHUD");
    let prev_hud = std::env::var_os("DXVK_HUD");
    std::env::remove_var("MANGOHUD");
    std::env::remove_var("DXVK_HUD");
    let (pool, dir, host, id) = setup().await;
    set_knob(&pool, &id, "dxvk-hud", "1").await.unwrap();
    set_handle(&pool, &dir, &host, &id, false).await.unwrap();
    crate::apply::write_record(
        &dir,
        &crate::apply::ApplyRecord {
            game: id.clone(),
            manager: "steam".into(),
            launcher: "/tmp/tuxgt-launcher".into(),
            applied_at: 0,
            files: vec![],
        },
    )
    .unwrap();
    set_global_knob(&pool, "mangohud", "1").await.unwrap();
    crate::sync_handle_sessions(&pool, &dir, &host)
        .await
        .unwrap();
    let text = fs::read_to_string(steam_conf(&dir)).unwrap();
    assert!(text.contains("MANGOHUD=1"), "{text}");
    assert!(text.contains("inject=0"), "{text}");
    match prev_mango {
        Some(v) => std::env::set_var("MANGOHUD", v),
        None => std::env::remove_var("MANGOHUD"),
    }
    match prev_hud {
        Some(v) => std::env::set_var("DXVK_HUD", v),
        None => std::env::remove_var("DXVK_HUD"),
    }
    let _ = tokio::fs::remove_dir_all(&dir).await;
}

#[tokio::test]
async fn so_selection_by_bitness() {
    let dir = std::env::temp_dir().join(format!("tuxgt-so32-{}-{}", std::process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("lib")).unwrap();
    std::fs::write(dir.join("lib/libtuxgt-launcher.so"), b"64").unwrap();
    std::fs::write(dir.join("lib/libtuxgt-launcher32.so"), b"32").unwrap();
    assert!(crate::session::find_so_for_arch(&dir, "64").unwrap().ends_with("libtuxgt-launcher.so"));
    assert!(crate::session::find_so_for_arch(&dir, "32").unwrap().ends_with("libtuxgt-launcher32.so"));
    std::fs::remove_file(dir.join("lib/libtuxgt-launcher32.so")).unwrap();
    // Fallback to 64 when 32 missing
    assert!(crate::session::find_so_for_arch(&dir, "32").unwrap().ends_with("libtuxgt-launcher.so"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn session_sync_writes_32bit_so_for_32bit_game() {
    let dir = std::env::temp_dir().join(format!("tuxgt-sess32-{}-{}", std::process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("lib")).unwrap();
    std::fs::write(dir.join("lib/libtuxgt-launcher.so"), b"64").unwrap();
    std::fs::write(dir.join("lib/libtuxgt-launcher32.so"), b"32").unwrap();
    let pool = crate::open_db(&dir).await.unwrap();
    let exe = dir.join("game32.exe");
    std::fs::write(&exe, b"MZ").unwrap();
    crate::testing::seed_game(
        &pool,
        crate::testing::SeedGame {
            id: "manual:standalone:sess32",
            manager: "manual",
            store: "standalone",
            game_id: "sess32",
            name: Some("t"),
            exe_path: Some(exe.to_str().unwrap()),
            detected_api: Some("dx12"),
            detected_bitness: Some("32"),
            ..Default::default()
        },
    )
    .await;
    let host = crate::PluginHost::load_with(&[], dir.join("config")).unwrap();
    // Need a manifest to make needs channel so sync doesn't disarm? Create a dummy reshade manifest
    let m = crate::FileManifest {
        game: "manual:standalone:sess32".into(),
        instance: "reshade".into(),
        mod_type: "reshade".into(),
        adapter: "preload".into(),
        enabled: true,
        load_order: 0,
        include: Box::default(),
        files: vec![crate::download::PlannedFile { source: "s".into(), dest: "ReShade32.dll".into(), sha256: "a".into(), enabled: true }].into_boxed_slice(),
        backups: Default::default(),
        generated_globs: Box::default(),
        harvested: Default::default(),
        provenance: crate::ModProvenance::default(),
        env: Box::default(),
    };
    crate::write_manifest(&dir, &m).unwrap();
    crate::session::sync_session(&pool, &dir, &host, "manual:standalone:sess32").await.unwrap();
    let gid = crate::game::GameId::parse("manual:standalone:sess32").unwrap();
    let conf = dir.join("games").join(crate::game::game_rel(&gid)).join("tux-protonfixes.conf");
    let text = std::fs::read_to_string(&conf).unwrap();
    // Exact SO filename on the TUXGT_LAUNCHER_SO line (must end with 32-bit name)
    let so_line = text
        .lines()
        .find(|l| l.starts_with("TUXGT_LAUNCHER_SO="))
        .expect("TUXGT_LAUNCHER_SO line");
    assert!(
        so_line.ends_with("libtuxgt-launcher32.so"),
        "expected 32-bit SO, got {so_line:?} text={text}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

