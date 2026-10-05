//! R37: the persisted per-game adapter choice and the conversion
//! transaction. These cover the contract the GUI depends on: the choice
//! survives a scan, every refusal lands before the first byte moves, and
//! every failure after that restores the exact pre-conversion state.
use super::adapter::{arm_fail, disarm_fail, resolve_install_adapter, FailPoint};
use super::*;
use crate::game::scan_with;
use crate::game::{game_adapter, set_game_adapter, ADAPTER_INSTALL, ADAPTER_PRELOAD};
use crate::install::{prefix_drive_c, prefix_for};
use crate::testing::{seed_game, SeedGame};
use crate::{Error, FileManifest, PlannedFile};
use sqlx::SqlitePool;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

/// A game with a game dir, one Mod recipe, and one staged payload file.
struct Fx {
    dir: PathBuf,
    data: PathBuf,
    cfg: PathBuf,
    root: PathBuf,
    pool: SqlitePool,
    gid: String,
    iid: String,
}

impl Fx {
    fn path(&self) -> &Path {
        &self.dir
    }
    fn dest(&self, name: &str) -> PathBuf {
        self.root.join(name)
    }
    fn manifest_path(&self) -> PathBuf {
        crate::manifest_path(&self.data, &self.gid, &self.iid)
    }
    fn ini_path(&self) -> PathBuf {
        let gid = crate::game::GameId::parse(&self.gid).unwrap();
        crate::prewire::managed_ini(&crate::game::game_dir(&self.data, &gid))
    }
    fn manifest(&self) -> FileManifest {
        crate::need_manifest(&self.data, &self.gid, &self.iid).unwrap()
    }
    /// Every byte the conversion could move, for exact-match assertions.
    fn snapshot(&self) -> BTreeMap<PathBuf, Option<Vec<u8>>> {
        let mut out = BTreeMap::new();
        for p in [self.dest("ReShade64.dll"), self.dest("dxgi.ini")] {
            out.insert(p.clone(), p.is_file().then(|| fs::read(&p).unwrap()));
        }
        let m = self.manifest_path();
        out.insert(m.clone(), m.is_file().then(|| fs::read(&m).unwrap()));
        let ini = self.ini_path();
        out.insert(ini.clone(), ini.is_file().then(|| fs::read(&ini).unwrap()));
        let bdir = crate::install::backups_dir(&self.data, &self.gid);
        if bdir.is_dir() {
            for e in fs::read_dir(&bdir).unwrap() {
                let p = e.unwrap().path();
                out.insert(p.clone(), Some(fs::read(&p).unwrap()));
            }
        }
        out
    }
}

fn stamp(tag: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "tuxgt-r37-{tag}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ))
}

async fn setup(tag: &str, dests: &[&str], adapter: &str) -> Fx {
    let dir = stamp(tag);
    let _ = fs::remove_dir_all(&dir);
    let data = dir.join("data");
    let cfg = dir.join("config");
    let root = dir.join("gamedir");
    fs::create_dir_all(&cfg).unwrap();
    fs::create_dir_all(&root).unwrap();
    let pool = crate::open_db(&data).await.unwrap();
    let gid = format!("manual:standalone:r37{tag}");
    let exe = root.join("game.exe");
    fs::write(&exe, b"MZ").unwrap();
    seed_game(
        &pool,
        SeedGame {
            id: &gid,
            manager: "manual",
            store: "standalone",
            game_id: &format!("r37{tag}"),
            name: Some("R37"),
            exe_path: Some(exe.to_str().unwrap()),
            install_dir: Some(root.to_str().unwrap()),
            detected_api: Some("dx12"),
            detected_bitness: Some("64"),
            ..Default::default()
        },
    )
    .await;
    // A real catalog recipe, so the conversion's recipe gate has something
    // to check (default plans allow both adapters).
    let iid = "r37mod".to_string();
    let pkg = dir.join("pkg");
    fs::create_dir_all(&pkg).unwrap();
    for dest in dests {
        fs::write(pkg.join(dest), dest.as_bytes()).unwrap();
    }
    crate::add_mod_from(
        &cfg,
        "custom",
        &iid,
        &pkg,
        None,
        None,
        &data,
        Vec::new(),
        Vec::new(),
    )
    .unwrap();
    let depot = pkg;
    let mut files = Vec::new();
    for dest in dests {
        let p = depot.join(dest);
        files.push(PlannedFile {
            source: format!("{iid}/{dest}"),
            dest: (*dest).to_string(),
            sha256: crate::sha256_file(&p).unwrap(),
            enabled: true,
            load: None,
        });
    }
    let m = FileManifest {
        game: gid.clone(),
        instance: iid.clone(),
        mod_type: "custom".into(),
        adapter: adapter.into(),
        enabled: true,
        load_order: 0,
        include: Box::default(),
        files: files.into_boxed_slice(),
        env: Box::default(),
        backups: Default::default(),
        generated_globs: Box::default(),
        harvested: Default::default(),
        provenance: crate::ModProvenance::default(),
    };
    let srcs: Vec<PathBuf> = m.files.iter().map(|f| depot.join(&f.dest)).collect();
    let staged: Vec<crate::StageInput> = m
        .files
        .iter()
        .zip(srcs.iter())
        .map(|(f, src)| crate::StageInput {
            rel: &f.dest,
            src,
            sha: &f.sha256,
        })
        .collect();
    crate::sync_staging(&data, &gid, &iid, &m.mod_type, &staged, false).unwrap();
    crate::write_manifest(&data, &m).unwrap();
    // The persisted choice agrees with the fixture manifest.
    set_game_adapter(&pool, &gid, adapter).await.unwrap();
    Fx {
        dir,
        data,
        cfg,
        root,
        pool,
        gid,
        iid,
    }
}

async fn convert(fx: &Fx, target: &str, yes: bool) -> crate::Result<ConversionReport> {
    convert_game_adapter(&fx.pool, &fx.data, &fx.cfg, &fx.gid, target, yes, false).await
}

/// The stored choice, the manifest, and every payload file agree.
async fn assert_consistent(fx: &Fx, adapter: &str) {
    assert_eq!(game_adapter(&fx.pool, &fx.gid).await.unwrap(), adapter);
    assert_eq!(fx.manifest().adapter, adapter);
    let tracked = crate::tracked_dests(&fx.data, &fx.gid).unwrap();
    for f in fx.manifest().files.iter() {
        let p = fx.dest(&f.dest);
        if adapter == ADAPTER_INSTALL {
            assert!(p.is_file(), "{} must be copied into the game dir", f.dest);
            assert_eq!(
                fs::read(&p).unwrap(),
                fs::read(crate::stage::stage_dir(&fx.data, &fx.gid, &fx.iid).join(&f.dest))
                    .unwrap()
            );
        } else {
            // Preload keeps nothing of ours in the game dir. A dest that is
            // still there was restored to the user's own content, so it
            // must not be the tracked payload any more.
            if p.is_file() {
                assert_ne!(
                    fs::read(&p).unwrap(),
                    fs::read(crate::stage::stage_dir(&fx.data, &fx.gid, &fx.iid).join(&f.dest))
                        .unwrap(),
                    "{} still holds the tracked payload",
                    f.dest
                );
            }
        }
    }
    assert!(!tracked.is_empty());
}

#[tokio::test]
async fn choice_defaults_to_preload_and_validates() {
    let fx = setup("def", &["ReShade64.dll"], ADAPTER_PRELOAD).await;
    // Fresh schema: the column default is preload, no write needed.
    assert_eq!(game_adapter(&fx.pool, &fx.gid).await.unwrap(), "preload");
    // Setter persists install.
    set_game_adapter(&fx.pool, &fx.gid, "install")
        .await
        .unwrap();
    assert_eq!(game_adapter(&fx.pool, &fx.gid).await.unwrap(), "install");
    // Unknown values are refused before any write, stored value intact.
    for bad in ["", "proton_env", "PRELOAD", "  "] {
        let err = set_game_adapter(&fx.pool, &fx.gid, bad).await.unwrap_err();
        assert!(matches!(err, Error::InvalidInstance(_)), "{bad:?}: {err}");
        assert_eq!(game_adapter(&fx.pool, &fx.gid).await.unwrap(), "install");
    }
    // Surrounding whitespace is trimmed, not rejected.
    set_game_adapter(&fx.pool, &fx.gid, "  preload  ")
        .await
        .unwrap();
    assert_eq!(game_adapter(&fx.pool, &fx.gid).await.unwrap(), "preload");
    // Unknown game errors on both accessors.
    assert!(matches!(
        game_adapter(&fx.pool, "manual:standalone:nope").await,
        Err(Error::UnknownGame(_))
    ));
    assert!(matches!(
        set_game_adapter(&fx.pool, "manual:standalone:nope", "install").await,
        Err(Error::UnknownGame(_))
    ));
    let _ = tokio::fs::remove_dir_all(&fx.path()).await;
}

/// A scan upsert must never reset the user's choice (the same rule
/// `override_hidden` and `steam_appid` follow).
#[tokio::test]
async fn provider_scan_preserves_the_choice() {
    use crate::provider::{GameProvider, GameRecord};
    let dir = stamp("scan");
    let _ = fs::remove_dir_all(&dir);
    let pool = crate::open_db(&dir).await.unwrap();
    let cfg = dir.join("config");
    let host = crate::PluginHost::load_with(crate::FIRST_PARTY, &cfg).unwrap();
    let gid = "steam::r37scan";
    let rec = || GameRecord::new(crate::GameId::new("steam", "", "r37scan").unwrap(), "Scan");
    struct Fake(Vec<GameRecord>);
    impl GameProvider for Fake {
        fn plugin_id(&self) -> &'static str {
            "steam"
        }
        fn scan(&self) -> crate::Result<Vec<GameRecord>> {
            Ok(self.0.clone())
        }
    }
    scan_with(&pool, &host, &[&Fake(vec![rec()])])
        .await
        .unwrap();
    assert_eq!(game_adapter(&pool, gid).await.unwrap(), "preload");
    set_game_adapter(&pool, gid, "install").await.unwrap();
    scan_with(&pool, &host, &[&Fake(vec![rec()])])
        .await
        .unwrap();
    assert_eq!(
        game_adapter(&pool, gid).await.unwrap(),
        "install",
        "a rescan must not reset the adapter choice"
    );
    // The choice also rides every row projection, search included.
    let all = crate::list_games(&pool, None, None, None).await.unwrap();
    assert_eq!(all[0].adapter, "install");
    let found = crate::list_games(&pool, None, None, Some("Scan"))
        .await
        .unwrap();
    assert_eq!(found.len(), 1, "search projection must return the row");
    assert_eq!(found[0].adapter, "install");
    let _ = tokio::fs::remove_dir_all(&dir).await;
}

/// A fresh database and a legacy one both read preload.
#[tokio::test]
async fn legacy_database_migrates_to_preload() {
    use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions};
    let dir = stamp("legacy");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let db = dir.join("config").join("tuxgt.sqlite");
    fs::create_dir_all(db.parent().unwrap()).unwrap();
    let opts = SqliteConnectOptions::new()
        .filename(&db)
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal)
        .foreign_keys(true)
        .busy_timeout(std::time::Duration::from_secs(5))
        .to_owned();
    let legacy = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(opts)
        .await
        .unwrap();
    // A pre-R37 schema: no adapter column, row already present.
    sqlx::query("CREATE TABLE games (id TEXT PRIMARY KEY NOT NULL, name TEXT)")
        .execute(&legacy)
        .await
        .unwrap();
    sqlx::query("INSERT INTO games (id, name) VALUES ('steam::1', 'Legacy')")
        .execute(&legacy)
        .await
        .unwrap();
    crate::game::migrate_games(&legacy, &dir).await.unwrap();
    let got: Option<(String,)> = sqlx::query_as("SELECT adapter FROM games WHERE id = 'steam::1'")
        .fetch_optional(&legacy)
        .await
        .unwrap();
    assert_eq!(
        got.map(|a| a.0),
        Some("preload".to_string()),
        "a legacy row backfills to preload"
    );
    assert_eq!(game_adapter(&legacy, "steam::1").await.unwrap(), "preload");
    // A second migrate is a no-op, not a duplicate-column error.
    crate::game::migrate_games(&legacy, &dir).await.unwrap();
    legacy.close().await;
    let _ = tokio::fs::remove_dir_all(&dir).await;
}

/// The real install path, not just the resolver: a Mod installed on a game
/// whose persisted choice is `install` lands on install, and an explicit
/// override still wins for that one call.
#[tokio::test]
async fn install_instance_uses_the_persisted_choice() {
    for (chosen, expect) in [
        (ADAPTER_PRELOAD, ADAPTER_PRELOAD),
        (ADAPTER_INSTALL, ADAPTER_INSTALL),
    ] {
        let fx = setup("realinstall", &["harmless.dll"], chosen).await;
        // A second instance, installed after the choice is set.
        let iid = "r37second";
        let pkg = fx.dir.join("pkg2");
        fs::create_dir_all(&pkg).unwrap();
        fs::write(pkg.join("second.dll"), b"second").unwrap();
        crate::add_mod_from(
            &fx.cfg,
            "custom",
            iid,
            &pkg,
            None,
            None,
            &fx.data,
            Vec::new(),
            Vec::new(),
        )
        .unwrap();
        let m = install_instance(
            &fx.pool,
            &fx.data,
            &fx.cfg,
            &fx.gid,
            iid,
            &InstallOpts::default(),
            None,
        )
        .await
        .unwrap_or_else(|e| panic!("{chosen} install: {e}"));
        assert_eq!(m.adapter, expect, "the persisted choice must win");
        assert_eq!(
            crate::need_manifest(&fx.data, &fx.gid, iid)
                .unwrap()
                .adapter,
            expect
        );
        // The other fixture instance was never touched by that install.
        assert_eq!(fx.manifest().adapter, chosen);
        // An explicit override applies to that call only.
        let other = if expect == ADAPTER_INSTALL {
            ADAPTER_PRELOAD
        } else {
            ADAPTER_INSTALL
        };
        let over = install_instance(
            &fx.pool,
            &fx.data,
            &fx.cfg,
            &fx.gid,
            iid,
            &InstallOpts {
                adapter: Some(other.into()),
                redownload: true,
                ..InstallOpts::default()
            },
            None,
        )
        .await
        .unwrap();
        assert_eq!(over.adapter, other, "an explicit override wins");
        assert_eq!(
            game_adapter(&fx.pool, &fx.gid).await.unwrap(),
            chosen,
            "an override never changes the stored choice"
        );
        let _ = tokio::fs::remove_dir_all(&fx.path()).await;
    }
}

/// The repair/reinstall path (`InstallOpts { redownload: true }`, what
/// `mods/update.rs` issues) must resolve the same way a first install
/// does: the game's persisted choice, never a UI placeholder. This is the
/// core half of the `installed`-gated fix; the GUI half is
/// `update_adapter` in the app crate.
#[tokio::test]
async fn repair_install_follows_the_persisted_choice() {
    let fx = setup("repair", &["harmless.dll"], ADAPTER_INSTALL).await;
    let iid = "r37repair";
    let pkg = fx.dir.join("pkgr");
    fs::create_dir_all(&pkg).unwrap();
    fs::write(pkg.join("repaired.dll"), b"repaired").unwrap();
    crate::add_mod_from(
        &fx.cfg,
        "custom",
        iid,
        &pkg,
        None,
        None,
        &fx.data,
        Vec::new(),
        Vec::new(),
    )
    .unwrap();
    // First install, then the repair install: both must land on the game's
    // stored `install` choice, so the second never silently downgrades to
    // preload.
    for (label, redownload) in [("first", false), ("repair", true)] {
        let m = install_instance(
            &fx.pool,
            &fx.data,
            &fx.cfg,
            &fx.gid,
            iid,
            &InstallOpts {
                redownload,
                ..InstallOpts::default()
            },
            None,
        )
        .await
        .unwrap_or_else(|e| panic!("{label}: {e}"));
        assert_eq!(m.adapter, ADAPTER_INSTALL, "{label} must use the choice");
        assert_eq!(
            crate::need_manifest(&fx.data, &fx.gid, iid)
                .unwrap()
                .adapter,
            ADAPTER_INSTALL,
            "{label} manifest"
        );
        assert!(
            fx.dest("repaired.dll").is_file(),
            "{label} must land the payload in the game dir"
        );
        assert_eq!(game_adapter(&fx.pool, &fx.gid).await.unwrap(), "install");
    }
    // A game whose choice is preload keeps preload through a repair too.
    let other = setup("repairpre", &["harmless.dll"], ADAPTER_PRELOAD).await;
    // The recipe lives in each fixture's own config dir.
    crate::add_mod_from(
        &other.cfg,
        "custom",
        iid,
        &pkg,
        None,
        None,
        &other.data,
        Vec::new(),
        Vec::new(),
    )
    .unwrap();
    let m = install_instance(
        &other.pool,
        &other.data,
        &other.cfg,
        &other.gid,
        iid,
        &InstallOpts {
            redownload: true,
            ..InstallOpts::default()
        },
        None,
    )
    .await
    .unwrap();
    assert_eq!(m.adapter, ADAPTER_PRELOAD);
    assert!(!other.dest("repaired.dll").is_file());
    let _ = tokio::fs::remove_dir_all(&other.path()).await;
    let _ = tokio::fs::remove_dir_all(&fx.path()).await;
}

#[tokio::test]
async fn new_install_follows_the_persisted_choice() {
    let fx = setup("install", &["ReShade64.dll"], ADAPTER_PRELOAD).await;
    set_game_adapter(&fx.pool, &fx.gid, "install")
        .await
        .unwrap();
    // The catalog recipe is absent in this fixture, so resolve the same
    // default install uses, through the public API.
    let got = resolve_install_adapter(&fx.pool, &fx.gid, None)
        .await
        .unwrap();
    assert_eq!(got, "install");
    let overridden = resolve_install_adapter(&fx.pool, &fx.gid, Some("preload"))
        .await
        .unwrap();
    assert_eq!(overridden, "preload", "an explicit override still wins");
    let bad = resolve_install_adapter(&fx.pool, &fx.gid, Some("nope"))
        .await
        .unwrap_err();
    assert!(matches!(bad, Error::InvalidInstance(_)), "{bad}");
    let _ = tokio::fs::remove_dir_all(&fx.path()).await;
}

#[tokio::test]
async fn conversion_moves_copies_backups_and_prewire() {
    let fx = setup("fwd", &["ReShade64.dll", "dxgi.ini"], ADAPTER_PRELOAD).await;
    // Preload leaves no game-dir file.
    assert!(!fx.dest("ReShade64.dll").is_file());
    let report = convert(&fx, ADAPTER_INSTALL, true).await.unwrap();
    assert_eq!(report.from, "preload");
    assert_eq!(report.to, "install");
    assert_eq!(report.instances, vec![fx.iid.clone()]);
    assert!(!report.unchanged);
    assert_consistent(&fx, ADAPTER_INSTALL).await;
    // The loader ini dropped the converted instance's list.
    let ini = fs::read_to_string(fx.ini_path()).unwrap();
    assert!(
        !ini.contains("ReShade64.dll"),
        "install adapter must leave the loader list: {ini}"
    );
    // Same choice again: no-op, nothing moves.
    let again = convert(&fx, ADAPTER_INSTALL, true).await.unwrap();
    assert!(again.unchanged);
    assert!(again.instances.is_empty());
    let _ = tokio::fs::remove_dir_all(&fx.path()).await;
}

#[tokio::test]
async fn conversion_restores_a_backed_up_foreign_file() {
    let fx = setup("back", &["ReShade64.dll", "dxgi.ini"], ADAPTER_INSTALL).await;
    // Seed the install state the way an install would: copy in, backing up
    // the protected foreign stem.
    let foreign = b"user-dxgi-ini".to_vec();
    fs::write(fx.dest("dxgi.ini"), &foreign).unwrap();
    // Round-trip the enable to apply the copies (which backs up the
    // protected foreign stem) exactly as a real install would.
    set_instance_enabled(&fx.pool, &fx.data, &fx.gid, &fx.iid, false, true)
        .await
        .unwrap();
    set_instance_enabled(&fx.pool, &fx.data, &fx.gid, &fx.iid, true, true)
        .await
        .unwrap();
    assert!(!fx.manifest().backups.is_empty(), "fixture backs a file up");
    assert_eq!(
        fs::read(fx.dest("ReShade64.dll")).unwrap(),
        fs::read(crate::stage::stage_dir(&fx.data, &fx.gid, &fx.iid).join("ReShade64.dll"))
            .unwrap()
    );
    assert_eq!(fs::read(fx.dest("dxgi.ini")).unwrap(), b"dxgi.ini");
    // Preload now: the tracked copy goes, the backed-up user file returns.
    convert(&fx, ADAPTER_PRELOAD, true).await.unwrap();
    assert!(!fx.dest("ReShade64.dll").is_file());
    assert_eq!(fs::read(fx.dest("dxgi.ini")).unwrap(), foreign);
    assert!(
        fx.manifest().backups.is_empty(),
        "preload keeps no backup map"
    );
    assert_consistent(&fx, ADAPTER_PRELOAD).await;
    let _ = tokio::fs::remove_dir_all(&fx.path()).await;
}

/// Each injection point must restore the exact pre-conversion state, and
/// the old adapter must stay usable afterwards.
#[tokio::test]
async fn conversion_rollback_restores_every_step() {
    for (point, label) in [
        (FailPoint::AfterFirstCopy, "after the first copy"),
        (FailPoint::AfterPrewrite, "after prewire"),
        (FailPoint::AfterGameColumn, "after the game column write"),
    ] {
        let fx = setup("rb", &["ReShade64.dll", "dxgi.ini"], ADAPTER_PRELOAD).await;
        // A foreign file the install-side conversion must not keep.
        let foreign = b"user-dxgi-ini".to_vec();
        fs::write(fx.dest("dxgi.ini"), &foreign).unwrap();
        let before = fx.snapshot();
        arm_fail(point);
        let err = convert(&fx, ADAPTER_INSTALL, true).await;
        disarm_fail();
        assert!(err.is_err(), "{label} must fail");
        let after = fx.snapshot();
        for (path, want) in &before {
            let got = after.get(path).cloned().flatten();
            assert_eq!(&got, want, "{label}: {} not restored", path.display());
        }
        // No new file appeared either.
        for path in after.keys() {
            assert!(
                before.contains_key(path),
                "{label}: {} left behind",
                path.display()
            );
        }
        // The stored choice never moved, and the old adapter is usable.
        assert_eq!(
            game_adapter(&fx.pool, &fx.gid).await.unwrap(),
            "preload",
            "{label}: stored choice moved"
        );
        assert_eq!(fx.manifest().adapter, "preload", "{label}: manifest moved");
        // Retrying after the failure succeeds cleanly.
        convert(&fx, ADAPTER_INSTALL, true).await.unwrap();
        assert_consistent(&fx, ADAPTER_INSTALL).await;
        assert_eq!(fs::read(fx.dest("dxgi.ini")).unwrap(), b"dxgi.ini");
        let _ = tokio::fs::remove_dir_all(&fx.path()).await;
    }
}

#[tokio::test]
async fn conversion_rolls_back_a_backup_it_created() {
    let fx = setup("rbk", &["ReShade64.dll", "dxgi.ini"], ADAPTER_INSTALL).await;
    let foreign = b"user-dxgi-ini".to_vec();
    fs::write(fx.dest("dxgi.ini"), &foreign).unwrap();
    // Install-adapter state: copy in, backing the foreign stem up.
    crate::mods::set_instance_enabled(&fx.pool, &fx.data, &fx.gid, &fx.iid, false, true)
        .await
        .unwrap();
    crate::mods::set_instance_enabled(&fx.pool, &fx.data, &fx.gid, &fx.iid, true, true)
        .await
        .unwrap();
    assert!(!fx.manifest().backups.is_empty(), "fixture backs a file up");
    let bdir = crate::install::backups_dir(&fx.data, &fx.gid);
    let before: BTreeMap<PathBuf, Vec<u8>> = fs::read_dir(&bdir)
        .unwrap()
        .map(|e| {
            let p = e.unwrap().path();
            (p.clone(), fs::read(&p).unwrap())
        })
        .collect();
    arm_fail(FailPoint::AfterFirstCopy);
    let err = convert(&fx, ADAPTER_PRELOAD, true).await;
    disarm_fail();
    assert!(err.is_err());
    let now: BTreeMap<PathBuf, Vec<u8>> = fs::read_dir(&bdir)
        .unwrap()
        .map(|e| {
            let p = e.unwrap().path();
            (p.clone(), fs::read(&p).unwrap())
        })
        .collect();
    assert_eq!(now, before, "rollback must restore the backup dir");
    assert_eq!(fs::read(fx.dest("dxgi.ini")).unwrap(), b"dxgi.ini");
    assert_eq!(game_adapter(&fx.pool, &fx.gid).await.unwrap(), "install");
    let _ = tokio::fs::remove_dir_all(&fx.path()).await;
}

#[tokio::test]
async fn foreign_dests_fail_closed_then_confirm_completes() {
    let fx = setup("confirm", &["ReShade64.dll", "dxgi.ini"], ADAPTER_PRELOAD).await;
    let foreign = b"user-dxgi-ini".to_vec();
    fs::write(fx.dest("dxgi.ini"), &foreign).unwrap();
    let before = fx.snapshot();
    let err = convert(&fx, ADAPTER_INSTALL, false).await.unwrap_err();
    match &err {
        Error::NeedConfirm(msg) => assert!(msg.contains("dxgi.ini"), "{msg}"),
        other => panic!("expected NeedConfirm, got {other}"),
    }
    let after = fx.snapshot();
    for (path, want) in &before {
        assert_eq!(
            &after.get(path).cloned().flatten(),
            want,
            "{} changed before consent",
            path.display()
        );
    }
    assert_eq!(game_adapter(&fx.pool, &fx.gid).await.unwrap(), "preload");
    assert!(!fx.dest("ReShade64.dll").is_file());
    // Confirmed: the copy lands and the foreign file is backed up.
    convert(&fx, ADAPTER_INSTALL, true).await.unwrap();
    assert_consistent(&fx, ADAPTER_INSTALL).await;
    assert_eq!(fs::read(fx.dest("dxgi.ini")).unwrap(), b"dxgi.ini");
    assert!(!fx.manifest().backups.is_empty());
    // A safe (no protected dest) game converts without any consent.
    let safe = setup("safe", &["harmless.dll"], ADAPTER_PRELOAD).await;
    convert(&safe, ADAPTER_INSTALL, false).await.unwrap();
    assert_consistent(&safe, ADAPTER_INSTALL).await;
    let _ = tokio::fs::remove_dir_all(&fx.path()).await;
    let _ = tokio::fs::remove_dir_all(&safe.path()).await;
}

/// A recipe that does not allow the target adapter blocks the whole game,
/// before any mutation, naming the instance.
#[tokio::test]
async fn disallowed_recipe_blocks_before_any_write() {
    let fx = setup("blocked", &["ReShade64.dll"], ADAPTER_PRELOAD).await;
    // Narrow the recipe's plans to preload only.
    let recipe = recipe_file(&fx);
    let text = fs::read_to_string(&recipe).unwrap();
    fs::write(
        &recipe,
        text.replace(&default_plans_line(&fx), "plans_allowed = [\"preload\"]"),
    )
    .unwrap();
    let before = fx.snapshot();
    let err = convert(&fx, ADAPTER_INSTALL, true).await.unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("disallows"), "{msg}");
    assert!(msg.contains(&fx.iid), "{msg}");
    let after = fx.snapshot();
    for (path, want) in &before {
        assert_eq!(
            &after.get(path).cloned().flatten(),
            want,
            "{} changed before the refusal",
            path.display()
        );
    }
    assert_eq!(game_adapter(&fx.pool, &fx.gid).await.unwrap(), "preload");
    // Widening the plans back makes the same conversion legal.
    fs::write(
        &recipe,
        text.replace(
            &default_plans_line(&fx),
            "plans_allowed = [\"preload\", \"install\"]",
        ),
    )
    .unwrap();
    convert(&fx, ADAPTER_INSTALL, true).await.unwrap();
    assert_consistent(&fx, ADAPTER_INSTALL).await;
    let _ = tokio::fs::remove_dir_all(&fx.path()).await;
}

/// The dry-run refuses exactly like the conversion for a disallowed
/// recipe, and persists nothing, so the GUI can run it before stopping
/// the store client.
#[tokio::test]
async fn validate_adapter_convert_agrees_without_mutating() {
    let fx = setup("validate", &["ReShade64.dll"], ADAPTER_PRELOAD).await;
    // Healthy game: the dry-run passes and changes nothing.
    validate_adapter_convert(
        &fx.pool,
        &fx.data,
        &fx.cfg,
        &fx.gid,
        ADAPTER_INSTALL,
        true,
        false,
    )
    .await
    .unwrap();
    assert_eq!(game_adapter(&fx.pool, &fx.gid).await.unwrap(), "preload");
    assert_eq!(fx.manifest().adapter, "preload");
    // Narrow the recipe to preload only: both entry points refuse with
    // the same reason, and neither persists anything.
    let recipe = recipe_file(&fx);
    let text = fs::read_to_string(&recipe).unwrap();
    fs::write(
        &recipe,
        text.replace(&default_plans_line(&fx), "plans_allowed = [\"preload\"]"),
    )
    .unwrap();
    let before = fx.snapshot();
    let verr = validate_adapter_convert(
        &fx.pool,
        &fx.data,
        &fx.cfg,
        &fx.gid,
        ADAPTER_INSTALL,
        true,
        false,
    )
    .await
    .unwrap_err();
    let cerr = convert(&fx, ADAPTER_INSTALL, true).await.unwrap_err();
    assert!(verr.to_string().contains("disallows"), "{verr}");
    assert_eq!(verr.to_string(), cerr.to_string());
    let after = fx.snapshot();
    assert_eq!(
        after.keys().collect::<Vec<_>>(),
        before.keys().collect::<Vec<_>>(),
        "validation added or removed a tracked file"
    );
    for (path, want) in &before {
        assert_eq!(
            &after.get(path).cloned().flatten(),
            want,
            "{} changed during validation",
            path.display()
        );
    }
    assert_eq!(game_adapter(&fx.pool, &fx.gid).await.unwrap(), "preload");
    let _ = tokio::fs::remove_dir_all(&fx.path()).await;
}

/// The generated recipe file for the fixture's single Mod.
fn recipe_file(fx: &Fx) -> PathBuf {
    let mut found = Vec::new();
    for dir in [fx.cfg.join("mods"), fx.data.join("mods")] {
        if dir.is_dir() {
            collect_toml(&dir, &mut found);
        }
    }
    found
        .into_iter()
        .find(|p| p.to_string_lossy().contains(&fx.iid))
        .unwrap_or_else(|| panic!("no recipe file for {}", fx.iid))
}

fn collect_toml(dir: &Path, out: &mut Vec<PathBuf>) {
    for e in fs::read_dir(dir).unwrap() {
        let p = e.unwrap().path();
        if p.is_dir() {
            collect_toml(&p, out);
        } else if p.extension().is_some_and(|x| x == "toml") {
            out.push(p);
        }
    }
}

/// The `plans_allowed` line as `add_mod_from` wrote it, so the test patches
/// the real value instead of guessing its formatting.
fn default_plans_line(fx: &Fx) -> String {
    let text = fs::read_to_string(recipe_file(fx)).unwrap();
    text.lines()
        .find(|l| l.trim_start().starts_with("plans_allowed"))
        .expect("plans_allowed line")
        .to_string()
}

/// A disabled instance converts its manifest without copying anything in.
#[tokio::test]
async fn disabled_instance_converts_without_copies() {
    let fx = setup("off", &["ReShade64.dll"], ADAPTER_PRELOAD).await;
    set_instance_enabled(&fx.pool, &fx.data, &fx.gid, &fx.iid, false, true)
        .await
        .unwrap();
    convert(&fx, ADAPTER_INSTALL, true).await.unwrap();
    assert_eq!(game_adapter(&fx.pool, &fx.gid).await.unwrap(), "install");
    assert_eq!(fx.manifest().adapter, "install");
    assert!(
        !fx.dest("ReShade64.dll").is_file(),
        "a disabled instance must not land in the game dir"
    );
    // Re-enabling now uses the persisted adapter, and needs consent for a
    // protected stem only.
    let m = set_instance_enabled(&fx.pool, &fx.data, &fx.gid, &fx.iid, true, true)
        .await
        .unwrap();
    assert_eq!(m.adapter, "install");
    assert!(fx.dest("ReShade64.dll").is_file());
    // Disabling again leaves no stale copy behind.
    set_instance_enabled(&fx.pool, &fx.data, &fx.gid, &fx.iid, false, true)
        .await
        .unwrap();
    assert!(!fx.dest("ReShade64.dll").is_file());
    let _ = tokio::fs::remove_dir_all(&fx.path()).await;
}

/// A disabled instance whose recipe disallows the target does not veto the
/// conversion: the manifest still follows the choice, and nothing lands in
/// the game dir until a re-enable.
#[tokio::test]
async fn disabled_disallowing_instance_does_not_block_convert() {
    let fx = setup("dblk", &["ReShade64.dll"], ADAPTER_PRELOAD).await;
    let recipe = recipe_file(&fx);
    let text = fs::read_to_string(&recipe).unwrap();
    fs::write(
        &recipe,
        text.replace(&default_plans_line(&fx), "plans_allowed = [\"preload\"]"),
    )
    .unwrap();
    set_instance_enabled(&fx.pool, &fx.data, &fx.gid, &fx.iid, false, true)
        .await
        .unwrap();
    convert(&fx, ADAPTER_INSTALL, true).await.unwrap();
    assert_eq!(game_adapter(&fx.pool, &fx.gid).await.unwrap(), "install");
    assert_eq!(fx.manifest().adapter, "install");
    assert!(
        !fx.dest("ReShade64.dll").is_file(),
        "a disabled instance must not land in the game dir"
    );
    let m = set_instance_enabled(&fx.pool, &fx.data, &fx.gid, &fx.iid, true, true)
        .await
        .unwrap();
    assert_eq!(m.adapter, "install");
    assert!(fx.dest("ReShade64.dll").is_file());
    let _ = tokio::fs::remove_dir_all(&fx.path()).await;
}

/// A disabled instance whose recipe is gone does not veto the conversion
/// either. Enabled, the same missing recipe still blocks.
#[tokio::test]
async fn disabled_missing_recipe_does_not_block_convert() {
    let fx = setup("dmis", &["ReShade64.dll"], ADAPTER_PRELOAD).await;
    fs::remove_file(recipe_file(&fx)).unwrap();
    let err = convert(&fx, ADAPTER_INSTALL, true).await.unwrap_err();
    assert!(err.to_string().contains("recipe missing"), "{err}");
    set_instance_enabled(&fx.pool, &fx.data, &fx.gid, &fx.iid, false, true)
        .await
        .unwrap();
    convert(&fx, ADAPTER_INSTALL, true).await.unwrap();
    assert_eq!(game_adapter(&fx.pool, &fx.gid).await.unwrap(), "install");
    assert_eq!(fx.manifest().adapter, "install");
    assert!(
        !fx.dest("ReShade64.dll").is_file(),
        "a disabled instance must not land in the game dir"
    );
    // The converted recipe-less manifest is still usable: re-enable lands
    // the file under the persisted adapter.
    let m = set_instance_enabled(&fx.pool, &fx.data, &fx.gid, &fx.iid, true, true)
        .await
        .unwrap();
    assert_eq!(m.adapter, "install");
    assert!(fx.dest("ReShade64.dll").is_file());
    let _ = tokio::fs::remove_dir_all(&fx.path()).await;
}

/// Unknown game and unknown target are refused, never half-applied.
#[tokio::test]
async fn conversion_rejects_unknown_game_and_adapter() {
    let fx = setup("bad", &["ReShade64.dll"], ADAPTER_PRELOAD).await;
    let err = convert(&fx, "proton_env", true).await.unwrap_err();
    assert!(matches!(err, Error::InvalidInstance(_)), "{err}");
    let err = convert_game_adapter(
        &fx.pool,
        &fx.data,
        &fx.cfg,
        "manual:standalone:nope",
        ADAPTER_INSTALL,
        true,
        false,
    )
    .await
    .unwrap_err();
    assert!(matches!(err, Error::UnknownGame(_)), "{err}");
    assert_eq!(game_adapter(&fx.pool, &fx.gid).await.unwrap(), "preload");
    let _ = tokio::fs::remove_dir_all(&fx.path()).await;
}

/// Prefixed dests resolve under the Wine prefix and convert with it.
#[tokio::test]
async fn conversion_handles_prefix_dests() {
    let fx = setup("pfx", &["ReShade64.dll"], ADAPTER_PRELOAD).await;
    // Rewrite the fixture onto a prefix dest.
    let prefix = fx.dir.join("pfx");
    fs::create_dir_all(prefix_drive_c(&prefix).join("windows")).unwrap();
    let mut m = fx.manifest();
    m.files[0].dest = "pfx:windows/system32/other.dll".into();
    m.files[0].sha256 = {
        let p = crate::stage::stage_dir(&fx.data, &fx.gid, &fx.iid).join("ReShade64.dll");
        crate::sha256_file(&p).unwrap()
    };
    // Staging keeps the `pfx:` prefix in the path, which is what the copy
    // plan reads.
    let staged = crate::stage::stage_dir(&fx.data, &fx.gid, &fx.iid)
        .join("pfx:windows")
        .join("system32")
        .join("other.dll");
    fs::create_dir_all(staged.parent().unwrap()).unwrap();
    fs::write(&staged, b"other-dll").unwrap();
    m.files[0].sha256 = crate::sha256_file(&staged).unwrap();
    crate::write_manifest(&fx.data, &m).unwrap();
    // `pfx:` dests need a prefix on the row.
    sqlx::query("UPDATE games SET override_prefix_path = ? WHERE id = ?")
        .bind(prefix.to_str().unwrap())
        .bind(&fx.gid)
        .execute(&fx.pool)
        .await
        .unwrap();
    assert_eq!(
        prefix_for(
            &fx.pool,
            &fx.gid,
            ["pfx:windows/system32/other.dll"].into_iter()
        )
        .await
        .unwrap()
        .is_some(),
        true
    );
    convert(&fx, ADAPTER_INSTALL, true).await.unwrap();
    let target = prefix_drive_c(&prefix)
        .join("windows")
        .join("system32")
        .join("other.dll");
    assert_eq!(fs::read(&target).unwrap(), b"other-dll");
    convert(&fx, ADAPTER_PRELOAD, true).await.unwrap();
    assert!(!target.is_file(), "prefix copy must be removed again");
    let _ = tokio::fs::remove_dir_all(&fx.path()).await;
}
