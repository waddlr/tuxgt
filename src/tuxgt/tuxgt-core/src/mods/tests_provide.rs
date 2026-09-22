use super::testing::nanos;
use super::*;
use crate::instance::{parse_recipe, SourceRef};
use crate::testing::{seed_game, SeedGame};
use crate::{disable_mod, mods_for_game};
use crate::{include_covers, Error, FileManifest};
use std::fs;
use std::path::{Path, PathBuf};

const STREAM: &[&str] = &[
    "sl.interposer.dll",
    "sl.common.dll",
    "sl.pcl.dll",
    "sl.dlss.dll",
    "sl.dlss_g.dll",
    "sl.reflex.dll",
    "nvngx_dlss.dll",
    "nvngx_dlssd.dll",
    "nvngx_dlssg.dll",
];

fn official_text(id: &str) -> String {
    fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../../mods/official")
            .join(format!("{id}.toml")),
    )
    .unwrap()
}

fn scratch(tag: &str) -> (PathBuf, PathBuf, PathBuf) {
    let dir = std::env::temp_dir().join(format!(
        "tuxgt-provide-{tag}-{}-{}",
        std::process::id(),
        nanos()
    ));
    let _ = fs::remove_dir_all(&dir);
    let data = dir.join("data");
    let cfg = dir.join("config");
    fs::create_dir_all(&cfg).unwrap();
    crate::instance::seed_official_share_from_repo(&data);
    (dir, data, cfg)
}

fn payload(data: &Path, id: &str) -> PathBuf {
    data.join("mods").join("official").join(id)
}

fn write_tree(root: &Path, rel: &str, bytes: &[u8]) {
    let path = root.join(rel);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, bytes).unwrap();
}

fn reshade_manifest(game: &str) -> FileManifest {
    FileManifest {
        game: game.into(),
        instance: "reshade".into(),
        mod_type: "reshade".into(),
        adapter: "preload".into(),
        enabled: true,
        load_order: 0,
        include: Box::default(),
        files: Box::default(),
        env: Box::default(),
        backups: Default::default(),
        generated_globs: Box::default(),
        harvested: Default::default(),
        provenance: crate::ModProvenance::default(),
    }
}

fn dep_manifest(game: &str, instance: &str, mod_type: &str) -> FileManifest {
    FileManifest {
        game: game.into(),
        instance: instance.into(),
        mod_type: mod_type.into(),
        ..reshade_manifest(game)
    }
}

#[test]
fn parse_accepts_official_shapes_and_rejects_bad_keys() {
    let nv = parse_recipe(&official_text("nvngx-dlssnr"), true).unwrap();
    assert_eq!(nv.mod_type, "custom");
    assert_eq!(nv.label, "nvngx_dlssnr");
    assert_eq!(&nv.include[..], ["nvngx_dlssnr.dll"]);
    match &nv.source {
        SourceRef::Provided { files, note } => {
            assert_eq!(&files[..], ["nvngx_dlssnr.dll"]);
            assert_eq!(
                note,
                "NVIDIA DLSS neural-rendering library (nvngx_dlssnr.dll). TuxGT does not download it."
            );
        }
        other => panic!("{other:?}"),
    }
    let reno = parse_recipe(&official_text("renodx-dlss"), true).unwrap();
    assert_eq!(reno.mod_type, "reshade_addon");
    assert_eq!(
        &reno.requires[..],
        ["d3dcompiler-47", "nvidia-streamline", "nvngx-dlssnr-proxy"]
    );
    match &reno.source {
        SourceRef::Provided { files, note } => {
            assert_eq!(&files[..], ["renodx-dlss.addon64"]);
            assert!(note.contains("does not download"));
        }
        other => panic!("{other:?}"),
    }
    let sl = parse_recipe(&official_text("nvidia-streamline"), true).unwrap();
    assert_eq!(sl.mod_type, "custom");
    assert_eq!(&sl.include[..], STREAM);
    match &sl.source {
        SourceRef::Provided { files, note } => {
            assert_eq!(&files[..], STREAM);
            assert_eq!(
                note,
                "NVIDIA Streamline runtime. TuxGT does not download it."
            );
        }
        other => panic!("{other:?}"),
    }
    assert!(nv.sha256.is_none() && sl.games.is_empty() && sl.appids.is_empty());
    assert!(nv.env.is_empty() && sl.plans_allowed.iter().all(|p| p.as_str() != "proton_env"));

    let one = r#"
id = "onepin"
type = "custom"
label = "one"
sha256 = "abc"
[source]
type = "provided"
files = ["a.dll"]
"#;
    let parsed = parse_recipe(one, false).unwrap();
    assert_eq!(parsed.sha256.as_deref(), Some("abc"));
    match parsed.source {
        SourceRef::Provided { note, .. } => assert!(note.is_empty()),
        other => panic!("{other:?}"),
    }

    let bad = [
        r#"
id = "p"
type = "custom"
label = "p"
[source]
type = "provided"
files = ["a.dll"]
url = "https://example.invalid/a.dll"
"#,
        r#"
id = "p"
type = "custom"
label = "p"
[source]
type = "provided"
files = ["a.dll"]
path = "/tmp/a.dll"
"#,
        r#"
id = "p"
type = "custom"
label = "p"
[source]
type = "provided"
files = []
"#,
        r#"
id = "p"
type = "custom"
label = "p"
[source]
type = "provided"
files = ["../a.dll"]
"#,
        r#"
id = "p"
type = "custom"
label = "p"
[source]
type = "provided"
files = ["foo?.dll"]
"#,
        r#"
id = "p"
type = "custom"
label = "p"
[source]
type = "provided"
files = ["bin/a.dll"]
"#,
        r#"
id = "p"
type = "custom"
label = "p"
sha256 = "abc"
[source]
type = "provided"
files = ["a.dll", "b.dll"]
"#,
    ];
    for text in bad {
        assert!(parse_recipe(text, false).is_err(), "{text}");
    }
}

#[test]
fn parse_accepts_official_dfc_proxy_y4my4m() {
    let dfc64 = parse_recipe(&official_text("deep-fried-chicken-64bit"), true).unwrap();
    assert_eq!(dfc64.id, "deep-fried-chicken-64bit");
    assert_eq!(dfc64.mod_type, "reshade_addon");
    assert_eq!(dfc64.label, "Deep Fried Chicken v3 - 64bit");
    assert_eq!(
        &dfc64.include[..],
        [
            "deep-fried-chicken-nvngx.dll",
            "deep-fried-chicken-present-support.dll"
        ]
    );
    assert!(dfc64.dests.is_empty());
    assert_eq!(dfc64.slot, None);
    assert_eq!(
        &dfc64.requires[..],
        ["d3dcompiler-47", "nvidia-streamline", "nvngx-dlssnr-proxy"]
    );
    match &dfc64.source {
        SourceRef::Provided { files, note } => {
            assert_eq!(
                &files[..],
                [
                    "deep-fried-chicken.addon64",
                    "deep-fried-chicken.cfg",
                    "deep-fried-chicken-nvngx.dll",
                    "deep-fried-chicken-present-support.dll"
                ]
            );
            assert!(note.contains("64-bit"));
        }
        other => panic!("{other:?}"),
    }
    let proxy = parse_recipe(&official_text("nvngx-dlssnr-proxy"), true).unwrap();
    assert_eq!(proxy.id, "nvngx-dlssnr-proxy");
    assert_eq!(proxy.mod_type, "custom");
    assert_eq!(proxy.label, "nvngx_dlssnr - workaround proxy");
    assert_eq!(
        &proxy.include[..],
        ["nvngx_dlssnr.dll", "nvngx_dlssnr.real.dll"]
    );
    assert_eq!(proxy.slot, None);
    match &proxy.source {
        SourceRef::Provided { files, note } => {
            assert_eq!(&files[..], ["nvngx_dlssnr.dll", "nvngx_dlssnr.real.dll"]);
            assert_eq!(
                note,
                "Proxy with renamed original. Use with ReShade DLSS-NR mods. Do not also install nvngx-dlssnr: same dest, load order decides."
            );
        }
        other => panic!("{other:?}"),
    }
    let fork = parse_recipe(&official_text("optiscaler-y4my4m-v4"), true).unwrap();
    assert_eq!(fork.id, "optiscaler-y4my4m-v4");
    assert_eq!(fork.mod_type, "optiscaler");
    assert_eq!(fork.label, "OptiScaler - y4my4m - v4");
    assert_eq!(fork.slot.as_deref(), Some("dxgi"));
    assert_eq!(&fork.include[..], ["OptiScaler/", "nvngx.dll_dlssnr.dll"]);
    assert!(fork
        .plans_allowed
        .iter()
        .all(|p| p.as_str() != "proton_env"));
    assert!(fork
        .payload
        .iter()
        .flat_map(|r| r.drop.iter())
        .any(|g| g == "Licenses/*"));
    match &fork.source {
        SourceRef::Github {
            owner,
            repo,
            asset_glob,
            tag,
            prerelease,
        } => {
            assert_eq!(owner, "y4my4my4m");
            assert_eq!(repo, "OptiScaler_DLSSNR_Multipass_MFG");
            assert_eq!(
                asset_glob,
                "OptiScaler_v10.0.0-dev-fork-y4my4my4m-v4_20260905_with_DLSS.7z"
            );
            assert_eq!(tag.as_deref(), Some("v10.0.0-dev-fork-y4my4my4m-v4"));
            assert!(!prerelease);
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn official_proxy_recipes_declare_slot() {
    for id in ["reshade", "optiscaler", "optiscaler-y4my4m-v4"] {
        let m = parse_recipe(&official_text(id), true).unwrap();
        assert_eq!(m.slot.as_deref(), Some("dxgi"), "{id}");
    }
    for id in [
        "d3dcompiler-47",
        "nvngx-dlssnr",
        "nvngx-dlssnr-proxy",
        "nvidia-streamline",
        "renodx-dlss",
        "deep-fried-chicken-64bit",
    ] {
        let m = parse_recipe(&official_text(id), true).unwrap();
        assert_eq!(m.slot, None, "{id}");
    }
}

#[test]
fn x64_hit_is_stored_and_readme_is_skipped() {
    let (dir, data, cfg) = scratch("x64");
    let drop = dir.join("drop");
    write_tree(&drop, "bin/x64/sl.interposer.dll", b"from-x64");
    write_tree(&drop, "bin/x86/sl.interposer.dll", b"from-x86");
    write_tree(&drop, "README.md", b"notes");
    let report = provide_files(&cfg, &data, "nvidia-streamline", &drop).unwrap();
    assert_eq!(report.present, vec!["sl.interposer.dll".to_string()]);
    assert!(report.ambiguous.is_empty());
    assert!(!report.ready());
    let stored = payload(&data, "nvidia-streamline").join("sl.interposer.dll");
    assert_eq!(fs::read(&stored).unwrap(), b"from-x64");
    assert!(!payload(&data, "nvidia-streamline")
        .join("README.md")
        .exists());
    assert!(!payload(&data, "nvidia-streamline")
        .join(".provenance.toml")
        .exists());
    let _ = fs::remove_dir_all(&dir);
}

trait Ready {
    fn ready(&self) -> bool;
}

impl Ready for ProvideReport {
    fn ready(&self) -> bool {
        self.missing.is_empty() && self.ambiguous.is_empty()
    }
}

#[test]
fn two_unfiltered_hits_stay_ambiguous() {
    let (dir, data, cfg) = scratch("amb");
    let drop = dir.join("drop");
    write_tree(&drop, "a/sl.common.dll", b"a");
    write_tree(&drop, "b/sl.common.dll", b"b");
    let report = provide_files(&cfg, &data, "nvidia-streamline", &drop).unwrap();
    assert!(report.ambiguous.iter().any(|p| p == "sl.common.dll"));
    assert!(!report.present.iter().any(|p| p == "sl.common.dll"));
    assert!(!payload(&data, "nvidia-streamline")
        .join("sl.common.dll")
        .exists());
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn partial_set_is_omitted_until_ready() {
    let (dir, data, cfg) = scratch("part");
    let one = dir.join("one");
    write_tree(&one, "sl.pcl.dll", b"pcl");
    let report = provide_files(&cfg, &data, "nvidia-streamline", &one).unwrap();
    assert!(!report.ready());
    let inst = find_mod(&cfg, &data, "nvidia-streamline").unwrap();
    assert!(inst.enabled);
    let pf = provided_files(&data, &inst).unwrap().unwrap();
    assert!(!pf.ready);
    assert_eq!(pf.present, vec!["sl.pcl.dll".to_string()]);
    assert!(!payload(&data, "nvidia-streamline")
        .join(".provenance.toml")
        .exists());
    let listed = mods_for_game(&cfg, "Any Game", None, &data).unwrap();
    assert!(listed.mods.iter().all(|m| m.id != "nvidia-streamline"));
    assert!(listed.mods.iter().any(|m| m.id == "reshade"));

    let rest = dir.join("rest");
    for name in STREAM {
        if *name != "sl.pcl.dll" {
            write_tree(&rest, name, name.as_bytes());
        }
    }
    let report = provide_files(&cfg, &data, "nvidia-streamline", &rest).unwrap();
    assert!(report.ready());
    assert!(report.enabled);
    let inst = find_mod(&cfg, &data, "nvidia-streamline").unwrap();
    assert!(provided_files(&data, &inst).unwrap().unwrap().ready);
    assert!(payload(&data, "nvidia-streamline")
        .join(".provenance.toml")
        .exists());
    let listed = mods_for_game(&cfg, "Any Game", None, &data).unwrap();
    assert!(listed.mods.iter().any(|m| m.id == "nvidia-streamline"));
    assert_eq!(
        fs::read(payload(&data, "nvidia-streamline").join("sl.pcl.dll")).unwrap(),
        b"pcl"
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn replacing_one_file_keeps_a_disabled_complete_mod_disabled() {
    let (dir, data, cfg) = scratch("dis");
    let all = dir.join("all");
    for name in STREAM {
        write_tree(&all, name, b"v1");
    }
    let report = provide_files(&cfg, &data, "nvidia-streamline", &all).unwrap();
    assert!(report.ready());
    assert!(report.enabled);
    disable_mod(&cfg, "nvidia-streamline", &data).unwrap();
    let again = dir.join("again");
    write_tree(&again, "sl.common.dll", b"v2");
    let report = provide_files(&cfg, &data, "nvidia-streamline", &again).unwrap();
    assert!(report.ready());
    assert!(!report.enabled);
    let inst = find_mod(&cfg, &data, "nvidia-streamline").unwrap();
    assert!(!inst.enabled);
    assert!(provided_files(&data, &inst).unwrap().unwrap().ready);
    assert_eq!(
        fs::read(payload(&data, "nvidia-streamline").join("sl.common.dll")).unwrap(),
        b"v2"
    );
    assert_eq!(
        fs::read(payload(&data, "nvidia-streamline").join("sl.pcl.dll")).unwrap(),
        b"v1"
    );
    let _ = fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn install_waits_for_files_then_lands_without_download() {
    let (dir, data, cfg) = scratch("inst");
    let pool = crate::open_db(&data).await.unwrap();
    let gid = "manual:standalone:provide";
    seed_game(
        &pool,
        SeedGame {
            id: gid,
            name: Some("Provide"),
            ..Default::default()
        },
    )
    .await;
    let one = dir.join("one");
    write_tree(&one, "sl.interposer.dll", b"only");
    provide_files(&cfg, &data, "nvidia-streamline", &one).unwrap();
    let err = install_instance(
        &pool,
        &data,
        &cfg,
        gid,
        "nvidia-streamline",
        &InstallOpts::default(),
        None,
    )
    .await
    .unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("files not provided"), "{msg}");
    assert!(msg.contains("sl.common.dll"), "{msg}");
    assert!(crate::read_manifest(&data, gid, "nvidia-streamline")
        .unwrap()
        .is_none());

    let dll = dir.join("nvngx_dlssnr.dll");
    fs::write(&dll, b"nr-bytes").unwrap();
    let report = provide_files(&cfg, &data, "nvngx-dlssnr", &dll).unwrap();
    assert!(report.ready());
    let before = fs::read(payload(&data, "nvngx-dlssnr").join("nvngx_dlssnr.dll")).unwrap();
    let m = install_instance(
        &pool,
        &data,
        &cfg,
        gid,
        "nvngx-dlssnr",
        &InstallOpts::default(),
        None,
    )
    .await
    .unwrap();
    assert!(m
        .files
        .iter()
        .any(|f| { f.dest == "nvngx_dlssnr.dll" && include_covers(&m.include, &f.dest) }));
    assert_eq!(m.provenance.source, "provided");

    let mut opts = InstallOpts::default();
    opts.redownload = true;
    install_instance(&pool, &data, &cfg, gid, "nvngx-dlssnr", &opts, None)
        .await
        .unwrap();
    assert_eq!(
        fs::read(payload(&data, "nvngx-dlssnr").join("nvngx_dlssnr.dll")).unwrap(),
        before
    );

    crate::write_manifest(&data, &reshade_manifest(gid)).unwrap();
    for (id, ty) in [
        ("d3dcompiler-47", "custom"),
        ("nvidia-streamline", "custom"),
        ("nvngx-dlssnr-proxy", "custom"),
    ] {
        crate::write_manifest(&data, &dep_manifest(gid, id, ty)).unwrap();
    }
    let addon = dir.join("addon");
    write_tree(&addon, "renodx-dlss.addon64", b"addon");
    provide_files(&cfg, &data, "renodx-dlss", &addon).unwrap();
    let m = install_instance(
        &pool,
        &data,
        &cfg,
        gid,
        "renodx-dlss",
        &InstallOpts::default(),
        None,
    )
    .await
    .unwrap();
    assert!(m.files.iter().any(|f| f.dest.ends_with(".addon64")));
    let _ = fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn catalog_check_is_offline() {
    let (dir, data, cfg) = scratch("cat");
    let pool = crate::open_db(&data).await.unwrap();
    match check_catalog_update(&pool, &data, &cfg, "nvngx-dlssnr")
        .await
        .unwrap()
    {
        CatalogStatus::NeedsFiles { detail } => {
            assert!(detail.contains("files not provided"), "{detail}");
            assert!(detail.contains("nvngx_dlssnr.dll"), "{detail}");
        }
        other => panic!("{other:?}"),
    }
    let dll = dir.join("nvngx_dlssnr.dll");
    fs::write(&dll, b"nr").unwrap();
    provide_files(&cfg, &data, "nvngx-dlssnr", &dll).unwrap();
    let status = check_catalog_update(&pool, &data, &cfg, "nvngx-dlssnr")
        .await
        .unwrap();
    assert_eq!(status, CatalogStatus::UpToDate);
    let row: (String,) =
        sqlx::query_as("SELECT update_status FROM mods_cache WHERE id = 'nvngx-dlssnr'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(row.0, "uptodate");
    let _ = fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn replaced_payload_stays_catalog_current() {
    let (dir, data, cfg) = scratch("cat-replaced");
    let pool = crate::open_db(&data).await.unwrap();
    let gid = "manual:standalone:provide-replaced";
    seed_game(
        &pool,
        SeedGame {
            id: gid,
            name: Some("Provide"),
            ..Default::default()
        },
    )
    .await;
    let dll = dir.join("nvngx_dlssnr.dll");
    fs::write(&dll, b"nr-v1").unwrap();
    provide_files(&cfg, &data, "nvngx-dlssnr", &dll).unwrap();
    install_instance(
        &pool,
        &data,
        &cfg,
        gid,
        "nvngx-dlssnr",
        &InstallOpts::default(),
        None,
    )
    .await
    .unwrap();
    fs::write(&dll, b"nr-v2").unwrap();
    provide_files(&cfg, &data, "nvngx-dlssnr", &dll).unwrap();
    assert_eq!(
        check_catalog_update(&pool, &data, &cfg, "nvngx-dlssnr")
            .await
            .unwrap(),
        CatalogStatus::UpToDate
    );
    assert_eq!(
        check_update(&data, &cfg, gid, "nvngx-dlssnr")
            .await
            .unwrap(),
        UpdateStatus::Available {
            detail: "files replaced".into(),
        }
    );
    clear_provided_files(&cfg, &data, "nvngx-dlssnr").unwrap();
    let baseline = ensure_update_baseline(&pool, &data, &cfg, gid, "nvngx-dlssnr")
        .await
        .unwrap();
    assert!(!baseline.installed);
    match baseline.status {
        UpdateStatus::Unknown { reason } => {
            assert!(reason.contains("files not provided"), "{reason}");
            assert_eq!(short_update_reason(&reason), "gui-mod-update-unknown-files");
        }
        other => panic!("{other:?}"),
    }
    let _ = fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn partial_reprovide_does_not_publish_a_digest() {
    let (dir, data, cfg) = scratch("part-digest");
    let pool = crate::open_db(&data).await.unwrap();
    let gid = "manual:standalone:part-digest";
    seed_game(
        &pool,
        SeedGame {
            id: gid,
            name: Some("Provide"),
            ..Default::default()
        },
    )
    .await;
    let all = dir.join("all");
    for name in STREAM {
        write_tree(&all, name, b"v1");
    }
    provide_files(&cfg, &data, "nvidia-streamline", &all).unwrap();
    install_instance(
        &pool,
        &data,
        &cfg,
        gid,
        "nvidia-streamline",
        &InstallOpts::default(),
        None,
    )
    .await
    .unwrap();
    let installed = crate::read_manifest(&data, gid, "nvidia-streamline")
        .unwrap()
        .unwrap();
    assert!(!installed.provenance.asset_sha256.is_empty());
    clear_provided_files(&cfg, &data, "nvidia-streamline").unwrap();
    let one = dir.join("one");
    write_tree(&one, "sl.pcl.dll", b"only");
    let report = provide_files(&cfg, &data, "nvidia-streamline", &one).unwrap();
    assert!(!report.ready());
    assert!(!payload(&data, "nvidia-streamline")
        .join(".provenance.toml")
        .exists());
    reconcile_mod_cache(&pool, &data, &cfg).await.unwrap();
    let rows = mod_cache_rows(&pool).await.unwrap();
    let row = rows
        .iter()
        .find(|r| r.id == "nvidia-streamline")
        .expect("cache row");
    assert!(row.asset_sha256.is_empty(), "{}", row.asset_sha256);
    let still = crate::read_manifest(&data, gid, "nvidia-streamline")
        .unwrap()
        .unwrap();
    assert_eq!(
        still.provenance.asset_sha256,
        installed.provenance.asset_sha256
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn single_file_any_name_is_stored_as_the_dest() {
    let (dir, data, cfg) = scratch("any");
    let dll = dir.join("NVIDIA_DLSS_NR.dll");
    fs::write(&dll, b"renamed").unwrap();
    let report = provide_files(&cfg, &data, "nvngx-dlssnr", &dll).unwrap();
    assert!(report.ready(), "{report:?}");
    assert_eq!(
        fs::read(payload(&data, "nvngx-dlssnr").join("nvngx_dlssnr.dll")).unwrap(),
        b"renamed"
    );
    assert!(!payload(&data, "nvngx-dlssnr")
        .join("NVIDIA_DLSS_NR.dll")
        .exists());
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn version_suffix_matches_and_stores_the_dest_name() {
    let (dir, data, cfg) = scratch("ver");
    let addon = dir.join("renodx-dlss-v1.addon64");
    fs::write(&addon, b"addon-v1").unwrap();
    let report = provide_files(&cfg, &data, "renodx-dlss", &addon).unwrap();
    assert!(report.ready(), "{report:?}");
    assert_eq!(
        fs::read(payload(&data, "renodx-dlss").join("renodx-dlss.addon64")).unwrap(),
        b"addon-v1"
    );

    let drop = dir.join("sl");
    write_tree(&drop, "sl.interposer-2.9.0.dll", b"interposer");
    write_tree(&drop, "sl.dlss_g.dll", b"g");
    let report = provide_files(&cfg, &data, "nvidia-streamline", &drop).unwrap();
    assert!(report.present.iter().any(|p| p == "sl.interposer.dll"));
    assert!(report.present.iter().any(|p| p == "sl.dlss_g.dll"));
    assert!(!report.present.iter().any(|p| p == "sl.dlss.dll"));
    assert_eq!(
        fs::read(payload(&data, "nvidia-streamline").join("sl.interposer.dll")).unwrap(),
        b"interposer"
    );
    assert!(!payload(&data, "nvidia-streamline")
        .join("sl.dlss.dll")
        .exists());
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn renodx_dlss5_does_not_fill_renodx_dlss() {
    let (dir, data, cfg) = scratch("dlss5");
    let drop = dir.join("drop");
    write_tree(&drop, "renodx-dlss5.addon64", b"dlss5");
    write_tree(&drop, "renodx-dlss-v1.addon64", b"dlss-v1");
    let report = provide_files(&cfg, &data, "renodx-dlss", &drop).unwrap();
    assert!(report.ready(), "{report:?}");
    assert_eq!(
        fs::read(payload(&data, "renodx-dlss").join("renodx-dlss.addon64")).unwrap(),
        b"dlss-v1"
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn clear_drops_the_payload_and_disables() {
    let (dir, data, cfg) = scratch("clr");
    let dll = dir.join("nvngx_dlssnr.dll");
    fs::write(&dll, b"nr").unwrap();
    provide_files(&cfg, &data, "nvngx-dlssnr", &dll).unwrap();
    clear_provided_files(&cfg, &data, "nvngx-dlssnr").unwrap();
    let inst = find_mod(&cfg, &data, "nvngx-dlssnr").unwrap();
    assert!(!inst.enabled);
    let pf = provided_files(&data, &inst).unwrap().unwrap();
    assert!(!pf.ready);
    assert!(pf.missing.iter().any(|p| p == "nvngx_dlssnr.dll"));
    assert!(!payload(&data, "nvngx-dlssnr").exists());
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn proxy_real_dll_alone_leaves_proxy_missing() {
    let (dir, data, cfg) = scratch("proxy-one");
    let payload = payload(&data, "nvngx-dlssnr-proxy");
    let real = dir.join("nvngx_dlssnr.real.dll");
    fs::write(&real, b"real-bytes").unwrap();
    let report = provide_files(&cfg, &data, "nvngx-dlssnr-proxy", &real).unwrap();
    assert!(!report.ready(), "{report:?}");
    assert_eq!(report.present, ["nvngx_dlssnr.real.dll"]);
    assert_eq!(report.missing, ["nvngx_dlssnr.dll"]);
    // The real DLL must not also fill the proxy pattern.
    assert!(!payload.join("nvngx_dlssnr.dll").exists());
    let dll = dir.join("nvngx_dlssnr.dll");
    fs::write(&dll, b"proxy-bytes").unwrap();
    let report = provide_files(&cfg, &data, "nvngx-dlssnr-proxy", &dll).unwrap();
    assert!(report.ready(), "{report:?}");
    assert_eq!(
        fs::read(payload.join("nvngx_dlssnr.dll")).unwrap(),
        b"proxy-bytes"
    );
    let (names, total) = crate::preview_payload_files(&cfg, &data, "nvngx-dlssnr-proxy").unwrap();
    assert_eq!(total, 2);
    assert_eq!(names, ["nvngx_dlssnr.dll", "nvngx_dlssnr.real.dll"]);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn proxy_folder_with_both_files_is_ready_and_clears_clean() {
    let (dir, data, cfg) = scratch("proxy-both");
    let payload = payload(&data, "nvngx-dlssnr-proxy");
    let drop = dir.join("drop");
    write_tree(&drop, "nvngx_dlssnr.dll", b"proxy-bytes");
    write_tree(&drop, "nvngx_dlssnr.real.dll", b"real-bytes");
    let report = provide_files(&cfg, &data, "nvngx-dlssnr-proxy", &drop).unwrap();
    assert!(report.ready(), "{report:?}");
    assert!(report.missing.is_empty() && report.ambiguous.is_empty());
    assert_eq!(
        fs::read(payload.join("nvngx_dlssnr.dll")).unwrap(),
        b"proxy-bytes"
    );
    assert_eq!(
        fs::read(payload.join("nvngx_dlssnr.real.dll")).unwrap(),
        b"real-bytes"
    );
    let (names, total) = crate::preview_payload_files(&cfg, &data, "nvngx-dlssnr-proxy").unwrap();
    assert_eq!(total, 2);
    assert_eq!(names, ["nvngx_dlssnr.dll", "nvngx_dlssnr.real.dll"]);
    clear_provided_files(&cfg, &data, "nvngx-dlssnr-proxy").unwrap();
    assert!(!payload.exists());
    let inst = find_mod(&cfg, &data, "nvngx-dlssnr-proxy").unwrap();
    assert!(!inst.enabled);
    let pf = provided_files(&data, &inst).unwrap().unwrap();
    assert!(!pf.ready);
    assert_eq!(pf.missing.len(), 2);
    let _ = fs::remove_dir_all(&dir);
}

fn write_dfc_tree(root: &Path) {
    write_tree(root, "64-bit/deep-fried-chicken.addon64", b"addon64-real");
    write_tree(root, "64-bit/deep-fried-chicken.cfg", b"cfg64");
    write_tree(root, "64-bit/deep-fried-chicken-nvngx.dll", b"nvngx64");
    write_tree(
        root,
        "64-bit/deep-fried-chicken-present-support.dll",
        b"present",
    );
    write_tree(root, "32-bit/deep-fried-chicken.addon32", b"addon32");
    write_tree(root, "32-bit/deep-fried-chicken-bridge.cfg", b"bridge");
    write_tree(
        root,
        "32-bit/host64/deep-fried-chicken.addon64",
        b"addon64-host",
    );
    write_tree(root, "32-bit/host64/deep-fried-chicken.cfg", b"cfg-host");
    write_tree(
        root,
        "32-bit/host64/deep-fried-chicken-nvngx.dll",
        b"nvngx-host",
    );
    write_tree(root, "32-bit/host64/dfc-universal-host64.exe", b"exe");
    write_tree(
        root,
        "32-bit/reshade-shaders/Shaders/DFC_Universal_Feed.fx",
        b"fx",
    );
    write_tree(root, "32-bit/reshade-shaders/Shaders/ReShade.fxh", b"fxh");
    write_tree(root, "LICENSE-Deep-Fried-Chicken.md", b"lic");
    write_tree(root, "README.txt", b"readme");
}

fn assert_dfc_payloads(data: &Path) {
    let wide = payload(data, "deep-fried-chicken-64bit");
    assert_eq!(
        fs::read(wide.join("deep-fried-chicken.addon64")).unwrap(),
        b"addon64-real"
    );
    assert_eq!(
        fs::read(wide.join("deep-fried-chicken.cfg")).unwrap(),
        b"cfg64"
    );
    assert_eq!(
        fs::read(wide.join("deep-fried-chicken-nvngx.dll")).unwrap(),
        b"nvngx64"
    );
    assert_eq!(
        fs::read(wide.join("deep-fried-chicken-present-support.dll")).unwrap(),
        b"present"
    );
}

#[test]
fn deep_fried_chicken_tree_picks_the_64bit_folder() {
    let (dir, data, cfg) = scratch("dfc");
    let drop = dir.join("drop");
    write_dfc_tree(&drop);
    let wide = provide_files(&cfg, &data, "deep-fried-chicken-64bit", &drop).unwrap();
    assert!(wide.ready(), "{wide:?}");
    assert_dfc_payloads(&data);
    assert!(!payload(&data, "deep-fried-chicken-64bit")
        .join("README.txt")
        .exists());
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn two_complete_folders_stay_ambiguous() {
    let (dir, data, cfg) = scratch("twofull");
    let drop = dir.join("drop");
    for folder in ["a", "b"] {
        for name in STREAM {
            write_tree(&drop, &format!("{folder}/{name}"), b"x");
        }
    }
    let report = provide_files(&cfg, &data, "nvidia-streamline", &drop).unwrap();
    assert!(!report.ready(), "{report:?}");
    assert!(report.ambiguous.iter().any(|p| p == "sl.common.dll"));
    assert!(!payload(&data, "nvidia-streamline")
        .join("sl.common.dll")
        .exists());
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn nested_ready_prefixes_resolve_to_the_smallest() {
    let (dir, data, cfg) = scratch("nested");
    let drop = dir.join("drop");
    write_tree(&drop, "outer/unrelated.dll", b"unrelated");
    for name in STREAM {
        write_tree(&drop, &format!("outer/inner/{name}"), b"inner");
    }
    // A stray second copy outside `outer/` keeps the whole tree ambiguous,
    // while `outer` and `outer/inner` are each ready on their own.
    write_tree(&drop, "stray/sl.common.dll", b"stray");
    let report = provide_files(&cfg, &data, "nvidia-streamline", &drop).unwrap();
    assert!(report.ready(), "{report:?}");
    let payload = payload(&data, "nvidia-streamline");
    assert_eq!(fs::read(payload.join("sl.common.dll")).unwrap(), b"inner");
    assert!(!payload.join("unrelated.dll").exists());
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn encrypted_provide_requires_the_password() {
    if !crate::download::tool_status(&crate::download::ExtTool {
        name: "7z",
        probe: &["7z"],
    })
    .found
    {
        return;
    }
    let (dir, data, cfg) = scratch("dfcpw");
    let drop = dir.join("drop");
    write_dfc_tree(&drop);
    let archive = dir.join("dfc.7z");
    let created = std::process::Command::new("7z")
        .args(["a", "-t7z", "-psecret", "-y"])
        .arg(&archive)
        .arg(".")
        .current_dir(&drop)
        .output()
        .unwrap();
    assert!(
        created.status.success(),
        "{}",
        String::from_utf8_lossy(&created.stderr)
    );
    let missing = provide_files(&cfg, &data, "deep-fried-chicken-64bit", &archive).unwrap_err();
    assert!(
        matches!(missing, Error::ArchivePasswordRequired),
        "{missing:?}"
    );
    assert!(!payload(&data, "deep-fried-chicken-64bit").exists());
    let wrong = provide_files_with_password(
        &cfg,
        &data,
        "deep-fried-chicken-64bit",
        &archive,
        Some("nope"),
    )
    .unwrap_err();
    assert!(matches!(wrong, Error::ArchivePasswordRequired), "{wrong:?}");
    assert!(!payload(&data, "deep-fried-chicken-64bit").exists());
    let wide = provide_files_with_password(
        &cfg,
        &data,
        "deep-fried-chicken-64bit",
        &archive,
        Some("secret"),
    )
    .unwrap();
    assert!(wide.ready(), "{wide:?}");
    assert_dfc_payloads(&data);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn unknown_and_wrong_source_error() {
    let (dir, data, cfg) = scratch("err");
    let err = provide_files(&cfg, &data, "no-such", &dir).unwrap_err();
    assert!(matches!(err, Error::UnknownInstance(_)));
    let err = provide_files(&cfg, &data, "reshade", &dir).unwrap_err();
    assert!(matches!(err, Error::InvalidInstance(_)));
    let _ = fs::remove_dir_all(&dir);
}
