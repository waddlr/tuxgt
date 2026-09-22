use super::testing::*;
use super::*;
use std::fs;
#[tokio::test]
async fn uncheck_dll_while_over_budget_ratchets_down() {
    let tag = "ratchet";
    let fx = seed(tag, 96, 0, &[]).await;
    let ini = crate::prewire::managed_ini(&crate::game::game_dir(
        &fx.data,
        &crate::game::GameId::parse(&fx.gid).unwrap(),
    ));
    // Initial prewire fails: the game is over budget with no ini section.
    crate::prewire_game(&fx.data, &fx.pool, &fx.gid)
        .await
        .unwrap_err();
    let before = fs::read(&ini).unwrap();
    // Each omit lands (Ok) while the ini stays untouched...
    for i in 0..6 {
        let d = dll(i, tag);
        set_file_keep(&fx.pool, &fx.data, &fx.gid, &fx.iid, &d, false, true)
            .await
            .unwrap();
        assert_eq!(
            fs::read(&ini).unwrap(),
            before,
            "ini written while over budget"
        );
        let m = crate::need_manifest(&fx.data, &fx.gid, &fx.iid).unwrap();
        assert!(!m.files.iter().find(|f| f.dest == d).unwrap().enabled);
        assert!(!crate::stage::stage_dir(&fx.data, &fx.gid, &fx.iid)
            .join(&d)
            .is_file());
    }
    // ...until the lists fit, at which point the ini is rewritten.
    let mut i = 6;
    loop {
        let ms = crate::game_manifests(&fx.data, &fx.gid).unwrap();
        let lens = crate::prewire::list_lens(&crate::prewire::body_for(&ms));
        if lens[0] < 8192 {
            break;
        }
        let d = dll(i, tag);
        set_file_keep(&fx.pool, &fx.data, &fx.gid, &fx.iid, &d, false, true)
            .await
            .unwrap();
        i += 1;
    }
    let text = fs::read_to_string(&ini).unwrap();
    assert!(text.contains("[game]"), "{text}");
    assert!(text.contains("LoadDLL="), "{text}");
    let _ = fs::remove_dir_all(&fx.dir);
}

#[tokio::test]
async fn nested_nondll_toggle_while_over_budget_leaves_ini_identical() {
    let tag = "inert";
    // `b.fx` stays kept so the tree line survives the `a.fx` toggle: the
    // prospective body is identical and the toggle is ini-inert.
    let fx = seed(
        tag,
        96,
        0,
        &[
            ("reshade-shaders/Shaders/pack/a.fx", true),
            ("reshade-shaders/Shaders/pack/b.fx", true),
        ],
    )
    .await;
    let ini = crate::prewire::managed_ini(&crate::game::game_dir(
        &fx.data,
        &crate::game::GameId::parse(&fx.gid).unwrap(),
    ));
    crate::prewire_game(&fx.data, &fx.pool, &fx.gid)
        .await
        .unwrap_err();
    let before = fs::read(&ini).unwrap();
    for on in [false, true] {
        set_file_keep(
            &fx.pool,
            &fx.data,
            &fx.gid,
            &fx.iid,
            "reshade-shaders/Shaders/pack/a.fx",
            on,
            true,
        )
        .await
        .unwrap();
        assert_eq!(fs::read(&ini).unwrap(), before, "toggle touched the ini");
        assert_eq!(
            crate::stage::stage_dir(&fx.data, &fx.gid, &fx.iid)
                .join("reshade-shaders/Shaders/pack/a.fx")
                .is_file(),
            on,
            "staging mismatch for on={on}"
        );
    }
    let m = crate::need_manifest(&fx.data, &fx.gid, &fx.iid).unwrap();
    assert!(
        m.files
            .iter()
            .find(|f| f.dest == "reshade-shaders/Shaders/pack/a.fx")
            .unwrap()
            .enabled
    );
    let _ = fs::remove_dir_all(&fx.dir);
}

#[tokio::test]
async fn growing_toggle_while_over_budget_refused_before_write() {
    let tag = "refuse";
    let fx = seed(tag, 96, 0, &[]).await;
    let d = dll(0, tag);
    // One omit lands and the game is still over budget.
    set_file_keep(&fx.pool, &fx.data, &fx.gid, &fx.iid, &d, false, true)
        .await
        .unwrap();
    let man = crate::manifest_path(&fx.data, &fx.gid, &fx.iid);
    let before = fs::read(&man).unwrap();
    // Re-keeping it would grow the over-budget list: refused, nothing moves.
    let err = set_file_keep(&fx.pool, &fx.data, &fx.gid, &fx.iid, &d, true, true)
        .await
        .unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("8192-byte budget"), "{msg}");
    assert!(msg.contains("uninstall"), "{msg}");
    assert_eq!(
        fs::read(&man).unwrap(),
        before,
        "refused toggle rewrote the manifest"
    );
    assert!(
        !crate::stage::stage_dir(&fx.data, &fx.gid, &fx.iid)
            .join(&d)
            .is_file(),
        "refused toggle restaged"
    );
    let _ = fs::remove_dir_all(&fx.dir);
}

#[tokio::test]
async fn uncheck_root_file_while_includefile_over_ratchets_down() {
    let tag = "roots";
    let fx = seed(tag, 0, 96, &[]).await;
    let ini = crate::prewire::managed_ini(&crate::game::game_dir(
        &fx.data,
        &crate::game::GameId::parse(&fx.gid).unwrap(),
    ));
    crate::prewire_game(&fx.data, &fx.pool, &fx.gid)
        .await
        .unwrap_err();
    let before = fs::read(&ini).unwrap();
    for i in 0..6 {
        let d = root(i, tag);
        set_file_keep(&fx.pool, &fx.data, &fx.gid, &fx.iid, &d, false, true)
            .await
            .unwrap();
        assert_eq!(
            fs::read(&ini).unwrap(),
            before,
            "ini written while over budget"
        );
        let m = crate::need_manifest(&fx.data, &fx.gid, &fx.iid).unwrap();
        assert!(!m.files.iter().find(|f| f.dest == d).unwrap().enabled);
    }
    let mut i = 6;
    loop {
        let ms = crate::game_manifests(&fx.data, &fx.gid).unwrap();
        let lens = crate::prewire::list_lens(&crate::prewire::body_for(&ms));
        if lens[1] < 8192 {
            break;
        }
        let d = root(i, tag);
        set_file_keep(&fx.pool, &fx.data, &fx.gid, &fx.iid, &d, false, true)
            .await
            .unwrap();
        i += 1;
    }
    let text = fs::read_to_string(&ini).unwrap();
    assert!(text.contains("[game]"), "{text}");
    assert!(text.contains("IncludeFile="), "{text}");
    let _ = fs::remove_dir_all(&fx.dir);
}

#[tokio::test]
async fn omit_off_over_list_while_over_budget_refused_before_write() {
    let tag = "wronglist";
    let fx = seed(tag, 1, 96, &[]).await;
    let d = dll(0, tag);
    // The DLL omit shrinks only the under-budget LoadDLL list while
    // IncludeFile stays over: refused, nothing moves.
    let man = crate::manifest_path(&fx.data, &fx.gid, &fx.iid);
    let before = fs::read(&man).unwrap();
    let err = set_file_keep(&fx.pool, &fx.data, &fx.gid, &fx.iid, &d, false, true)
        .await
        .unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("8192-byte budget"), "{msg}");
    assert!(msg.contains("uninstall"), "{msg}");
    assert_eq!(
        fs::read(&man).unwrap(),
        before,
        "refused toggle rewrote the manifest"
    );
    assert!(
        crate::stage::stage_dir(&fx.data, &fx.gid, &fx.iid)
            .join(&d)
            .is_file(),
        "refused toggle unstaged"
    );
    let _ = fs::remove_dir_all(&fx.dir);
}
