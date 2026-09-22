use super::testing::*;
use super::*;
use crate::testing::{seed_game, SeedGame};
use crate::{sha256_file, Error};
use std::fs;
use std::path::{Path, PathBuf};

#[tokio::test]
async fn healed_provenance_from_cache_reports_up_to_date() {
    // E76: empty provenance + cached asset → provenance written from
    // disk bytes (no reinstall, no re-unpack), then UpToDate.
    let (dir, data, cfg, pool, gid, iid) = local_game("d", b"plug-v1").await;
    install_instance(
        &pool,
        &data,
        &cfg,
        &gid,
        &iid,
        &InstallOpts::default(),
        None,
    )
    .await
    .unwrap();
    let payload = data.join("mods").join("user").join(&iid);
    let before = dir_bytes(&payload);
    let mut m = crate::read_manifest(&data, &gid, &iid).unwrap().unwrap();
    m.provenance = crate::ModProvenance::default();
    crate::write_manifest(&data, &m).unwrap();
    let baseline = ensure_update_baseline(&pool, &data, &cfg, &gid, &iid)
        .await
        .unwrap();
    assert!(!baseline.installed, "repair must not reinstall");
    assert_eq!(baseline.status, UpdateStatus::UpToDate);
    let m = crate::read_manifest(&data, &gid, &iid).unwrap().unwrap();
    assert!(!m.provenance.asset_sha256.is_empty());
    let (digest, _) = crate::download::local_source_digest(&dir.join("pkg")).unwrap();
    assert_eq!(m.provenance.asset_sha256, digest);
    assert_eq!(dir_bytes(&payload), before, "repair must not re-unpack");
    // GUI short notes map to fixed keys: no ids, no CLI text.
    assert_eq!(
            short_update_reason("manual:standalone:r32d r32plugd: no install provenance recorded; re-run `tuxgt instance install --redownload manual:standalone:r32d r32plugd` to record it"),
            "gui-mod-update-unknown-record"
        );
    assert_eq!(
        short_update_reason("g i: cannot resolve upstream: boom"),
        "gui-mod-update-unknown-source"
    );
    assert_eq!(
        short_update_reason("no manifest for g i"),
        "gui-mod-update-unknown-record"
    );
    let _ = fs::remove_dir_all(&dir);
}

fn dir_bytes(dir: &Path) -> Vec<(String, String)> {
    fn walk(d: &Path, base: &Path, out: &mut Vec<(String, String)>) {
        let Ok(entries) = fs::read_dir(d) else {
            return;
        };
        let mut paths: Vec<PathBuf> = entries.filter_map(|e| e.ok().map(|e| e.path())).collect();
        paths.sort();
        for p in paths {
            if p.is_dir() {
                walk(&p, base, out);
            } else if p.is_file() {
                out.push((
                    p.strip_prefix(base).unwrap().to_string_lossy().into_owned(),
                    sha256_file(&p).unwrap(),
                ));
            }
        }
    }
    let mut out = Vec::new();
    walk(dir, dir, &mut out);
    out
}

#[tokio::test]
async fn legacy_manifest_reports_unknown_update() {
    let (dir, data, cfg, pool, gid, iid) = local_game("b", b"plug-v1").await;
    install_instance(
        &pool,
        &data,
        &cfg,
        &gid,
        &iid,
        &InstallOpts::default(),
        None,
    )
    .await
    .unwrap();
    let mut m = crate::read_manifest(&data, &gid, &iid).unwrap().unwrap();
    m.provenance = crate::ModProvenance::default();
    crate::write_manifest(&data, &m).unwrap();
    match check_update(&data, &cfg, &gid, &iid).await.unwrap() {
        UpdateStatus::Unknown { reason } => assert!(reason.contains("provenance"), "{reason}"),
        other => panic!("expected Unknown, got {other:?}"),
    }
    let _ = fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn resync_restores_user_touched_only_with_force() {
    let (dir, data, cfg, pool, gid, iid) = local_game("c", b"plug-v1").await;
    install_instance(
        &pool,
        &data,
        &cfg,
        &gid,
        &iid,
        &InstallOpts::default(),
        None,
    )
    .await
    .unwrap();
    let staged = crate::stage::stage_dir(&data, &gid, &iid).join("plug.dll");
    assert_eq!(fs::read(&staged).unwrap(), b"plug-v1");
    fs::write(&staged, b"user-edit").unwrap();
    let lines = crate::stage_status(&data, &gid).unwrap();
    assert!(lines
        .iter()
        .any(|l| l.state == crate::StageState::UserModified));
    let err = resync_instance(&data, &gid, &iid, false).unwrap_err();
    assert!(matches!(err, Error::StagedModified(_)), "{err}");
    assert_eq!(
        fs::read(&staged).unwrap(),
        b"user-edit",
        "no-force resync must not overwrite"
    );
    let lines = resync_instance(&data, &gid, &iid, true).unwrap();
    assert!(lines.iter().all(|l| l.state == crate::StageState::InSync));
    assert_eq!(fs::read(&staged).unwrap(), b"plug-v1");
    let _ = fs::remove_dir_all(&dir);
}
#[tokio::test]
async fn over_budget_pack_install_leaves_no_enabled_residue() {
    // R35 P1: a pack whose LoadDLL list exceeds the 8192-byte budget must
    // fail with no enabled manifest/staging left behind, so a later
    // install for the same game still succeeds.
    let tag = format!("p1{}", nanos() % 100000);
    let dir = std::env::temp_dir().join(format!("tuxgt-r35p1-{tag}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    let data = dir.join("data");
    let cfg = dir.join("config");
    let big = dir.join("bigpkg");
    fs::create_dir_all(&cfg).unwrap();
    fs::create_dir_all(&big).unwrap();
    let pool = crate::open_db(&data).await.unwrap();
    let gid = format!("manual:standalone:r35{tag}");
    let exe = data.join("game.exe");
    fs::create_dir_all(&data).unwrap();
    fs::write(&exe, b"MZ").unwrap();
    seed_game(
        &pool,
        SeedGame {
            id: &gid,
            manager: "manual",
            store: "standalone",
            game_id: &tag,
            name: Some("t"),
            exe_path: Some(exe.to_str().unwrap()),
            detected_api: Some("dx11"),
            detected_bitness: Some("64"),
            ..Default::default()
        },
    )
    .await;
    // 160 long-named DLLs: well over the 8192-byte LoadDLL budget, and
    // every one is required for custom so omit/split cannot help.
    for i in 0..160 {
        let name = format!("very-long-dll-name-{i:03}-p1-over-budget.dll");
        fs::write(big.join(name), b"dll").unwrap();
    }
    let big_id = format!("bigdlls{tag}");
    crate::add_mod_from(
        &cfg,
        "custom",
        &big_id,
        &big,
        None,
        None,
        &data,
        Vec::new(),
        Vec::new(),
    )
    .unwrap();
    let err = install_instance(
        &pool,
        &data,
        &cfg,
        &gid,
        &big_id,
        &InstallOpts::default(),
        None,
    )
    .await
    .unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("8192-byte budget"), "{msg}");
    assert!(msg.contains("uninstall"), "{msg}");
    assert!(
        crate::read_manifest(&data, &gid, &big_id)
            .unwrap()
            .is_none(),
        "rejected pack left an enabled manifest behind"
    );
    assert!(
        !crate::stage::stage_dir(&data, &gid, &big_id).exists(),
        "rejected pack left staging behind"
    );
    // A collapsed tree pack for the same game installs under budget: the
    // later install proves the failed one poisoned nothing.
    let shade = dir.join("shadepkg");
    let fx = dir.join("fxpkg");
    fs::create_dir_all(&shade).unwrap();
    fs::create_dir_all(fx.join("many-shaders")).unwrap();
    fs::write(shade.join("ReShade64.dll"), b"shade").unwrap();
    for i in 0..80 {
        fs::write(
            fx.join("many-shaders").join(format!("shader-{i:03}.fx")),
            b"fx",
        )
        .unwrap();
    }
    let shade_id = format!("shade{tag}");
    let fx_id = format!("fxpack{tag}");
    crate::add_mod_from(
        &cfg,
        "reshade",
        &shade_id,
        &shade,
        None,
        None,
        &data,
        Vec::new(),
        Vec::new(),
    )
    .unwrap();
    crate::add_mod_from(
        &cfg,
        "effect",
        &fx_id,
        &fx,
        None,
        None,
        &data,
        Vec::new(),
        Vec::new(),
    )
    .unwrap();
    let opts = InstallOpts {
        with_requires: Some(shade_id.clone()),
        ..InstallOpts::default()
    };
    let m = install_instance(&pool, &data, &cfg, &gid, &fx_id, &opts, None)
        .await
        .unwrap();
    assert!(m.enabled);
    let gdir = crate::game::game_dir(&data, &crate::game::GameId::parse(&gid).unwrap());
    let text = fs::read_to_string(crate::prewire::managed_ini(&gdir)).unwrap();
    assert!(text.contains("IncludeFile="), "{text}");
    assert!(!text.contains("very-long-dll-name"), "{text}");
    let _ = fs::remove_dir_all(&dir);
}
#[tokio::test]
async fn mod_id_requires_blocks_until_required_instance_present() {
    // E85: a recipe `requires = [<mod id>]` blocks install until that Mod
    // has an Instance (manifest) on the game; `--with-requires <id>`
    // installs it first. Type-level Requires still apply (custom has
    // none, so only the Mod-id gate fires here).
    let tag = format!("mr{}", nanos() % 100000);
    let dir = std::env::temp_dir().join(format!("tuxgt-modreq-{tag}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    let data = dir.join("data");
    let cfg = dir.join("config");
    let reqpkg = dir.join("reqpkg");
    let deppkg = dir.join("deppkg");
    fs::create_dir_all(&cfg).unwrap();
    fs::create_dir_all(&reqpkg).unwrap();
    fs::create_dir_all(&deppkg).unwrap();
    fs::write(reqpkg.join("req.dll"), b"dll").unwrap();
    fs::write(deppkg.join("dep.dll"), b"dll").unwrap();
    let pool = crate::open_db(&data).await.unwrap();
    let gid = format!("manual:standalone:modreq{tag}");
    let exe = data.join("game.exe");
    fs::create_dir_all(&data).unwrap();
    fs::write(&exe, b"MZ").unwrap();
    seed_game(
        &pool,
        SeedGame {
            id: &gid,
            manager: "manual",
            store: "standalone",
            game_id: &tag,
            name: Some("t"),
            exe_path: Some(exe.to_str().unwrap()),
            detected_api: Some("dx11"),
            detected_bitness: Some("64"),
            ..Default::default()
        },
    )
    .await;
    let req_id = format!("reqmod{tag}");
    let dep_id = format!("depmod{tag}");
    crate::add_mod_from(
        &cfg,
        "custom",
        &req_id,
        &reqpkg,
        None,
        None,
        &data,
        Vec::new(),
        Vec::new(),
    )
    .unwrap();
    let recipe = format!(
            "id = \"{dep_id}\"\ntype = \"custom\"\nlabel = \"dep\"\nrequires = [\"{req_id}\"]\n[source]\ntype = \"local\"\npath = \"{}\"\n",
            deppkg.to_str().unwrap()
        );
    let src = dir.join("dep.toml");
    fs::write(&src, recipe).unwrap();
    crate::add_mod(&cfg, &src, &data).unwrap();
    let err = install_instance(
        &pool,
        &data,
        &cfg,
        &gid,
        &dep_id,
        &InstallOpts::default(),
        None,
    )
    .await
    .unwrap_err();
    assert!(
        matches!(&err, Error::MissingRequires(id) if id == &req_id),
        "{err}"
    );
    let wrong = InstallOpts {
        with_requires: Some(dep_id.clone()),
        ..InstallOpts::default()
    };
    let err = install_instance(&pool, &data, &cfg, &gid, &dep_id, &wrong, None)
        .await
        .unwrap_err();
    assert!(matches!(err, Error::InvalidInstance(_)), "{err}");
    assert!(err.to_string().contains("is not required"), "{err}");
    let opts = InstallOpts {
        with_requires: Some(req_id.clone()),
        ..InstallOpts::default()
    };
    let m = install_instance(&pool, &data, &cfg, &gid, &dep_id, &opts, None)
        .await
        .unwrap();
    assert!(m.enabled);
    assert!(
        crate::read_manifest(&data, &gid, &req_id)
            .unwrap()
            .is_some(),
        "confirm must install the required Mod first"
    );
    assert!(
        crate::read_manifest(&data, &gid, &dep_id)
            .unwrap()
            .is_some(),
        "dependent installs once the require is present"
    );
    let _ = fs::remove_dir_all(&dir);
}
#[tokio::test]
async fn saved_copy_survives_original_delete() {
    let dir = std::env::temp_dir().join(format!(
        "tuxgt-pkgcopy-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let _ = fs::remove_dir_all(&dir);
    let data = dir.join("data");
    let cfg = dir.join("config");
    let orig = dir.join("orig");
    fs::create_dir_all(&cfg).unwrap();
    fs::create_dir_all(&orig).unwrap();
    fs::write(orig.join("plug.dll"), b"dll").unwrap();
    let pool = crate::open_db(&data).await.unwrap();
    let gid = "manual:standalone:pkgcopy";
    let exe = data.join("game.exe");
    fs::create_dir_all(&data).unwrap();
    fs::write(&exe, b"MZ").unwrap();
    seed_game(
        &pool,
        SeedGame {
            id: gid,
            manager: "manual",
            store: "standalone",
            game_id: "pkgcopy",
            name: Some("t"),
            exe_path: Some(exe.to_str().unwrap()),
            detected_api: Some("dx11"),
            detected_bitness: Some("64"),
            ..Default::default()
        },
    )
    .await;
    let iid = "copyplug";
    let m = crate::add_mod_from(
        &cfg,
        "custom",
        iid,
        &orig,
        None,
        None,
        &data,
        Vec::new(),
        Vec::new(),
    )
    .unwrap();
    let path = match &m.source {
        crate::SourceRef::Local { path } => path.clone(),
        _ => panic!("local source expected"),
    };
    assert!(
        path.starts_with(data.join("mods").join("user").to_str().unwrap()),
        "{path}"
    );
    // Delete the original: install still succeeds from the copy.
    fs::remove_dir_all(&orig).unwrap();
    let man = install_instance(&pool, &data, &cfg, gid, iid, &InstallOpts::default(), None)
        .await
        .unwrap();
    assert!(man.files.iter().any(|f| f.dest == "plug.dll"));
    assert!(
        man.provenance.source.starts_with("local:"),
        "{}",
        man.provenance.source
    );
    assert!(
        !man.provenance.source.contains("orig"),
        "{}",
        man.provenance.source
    );
    let _ = fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn slot_rewrite_restages_and_survives_reinstall() {
    let (dir, data, cfg, pool, gid, iid) = local_game("slot", b"plug-v1").await;
    // Remap the single DLL onto the dxgi proxy slot before install.
    let stored = data.join("mods").join("user").join(&iid);
    let mut files = crate::scan_package(&stored, "custom", &data).unwrap();
    assert_eq!(files.len(), 1);
    files[0].dest = "dxgi.dll".into();
    crate::rescan_mod(&cfg, &iid, Some(&files), None, &data).unwrap();
    let m = install_instance(
        &pool,
        &data,
        &cfg,
        &gid,
        &iid,
        &InstallOpts::default(),
        None,
    )
    .await
    .unwrap();
    assert!(m.files.iter().any(|f| f.dest == "dxgi.dll"));
    // Slot change rewrites the claiming dest, restages, and prewires.
    let m = set_instance_slot(&pool, &data, &gid, &iid, "winmm", false)
        .await
        .unwrap();
    assert!(m.files.iter().any(|f| f.dest == "winmm.dll"));
    assert!(!m.files.iter().any(|f| f.dest == "dxgi.dll"));
    let stage = crate::stage::stage_dir(&data, &gid, &iid);
    assert!(stage.join("winmm.dll").is_file());
    assert!(!stage.join("dxgi.dll").exists());
    let lines = crate::prewire::ini_lines_for(&m);
    assert!(
        lines.iter().any(|l| l.starts_with("LoadDLL=winmm.dll=")),
        "{lines:?}"
    );
    // Reinstall matches the prior dest by source: winmm survives.
    let m = install_instance(
        &pool,
        &data,
        &cfg,
        &gid,
        &iid,
        &InstallOpts::default(),
        None,
    )
    .await
    .unwrap();
    assert!(m.files.iter().any(|f| f.dest == "winmm.dll"));
    assert!(!m.files.iter().any(|f| f.dest == "dxgi.dll"));
    let _ = fs::remove_dir_all(&dir);
}
