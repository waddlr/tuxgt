use super::*;
use crate::Error;
use std::fs;

#[test]
fn globs_per_type() {
    assert_eq!(
        generated_globs_for("reshade", &[]),
        vec!["ReShade.ini".to_string(), "ReShade.log".to_string()]
    );
    assert_eq!(
        generated_globs_for("optiscaler", &[]),
        vec!["OptiScaler.ini".to_string(), "OptiScaler.log".to_string()]
    );
    assert_eq!(
        generated_globs_for("reshade_addon", &["Example.addon64", "data.txt"]),
        vec![
            "ReShade.ini".to_string(),
            "ReShade.log".to_string(),
            "Example.log".to_string()
        ]
    );
    assert!(generated_globs_for("custom", &["x.dll"]).is_empty());
    assert!(generated_globs_for("effect", &[]).is_empty());
}
#[test]
fn include_dll_omitable_unless_lone_dest() {
    assert!(is_required_dest("custom", "x.dll", &[], 3));
    assert!(!is_required_dest("custom", "x.dll", &["x.dll".into()], 3));
    assert!(is_required_dest("custom", "x.dll", &["x.dll".into()], 1));
    assert!(!is_required_dest("custom", "readme.txt", &[], 2));
    assert!(is_required_dest("reshade", "ReShade64.dll", &[], 5));
    assert!(!is_required_dest(
        "reshade",
        "ReShade64.dll",
        &["ReShade64.dll".into()],
        5
    ));
}

#[test]
fn include_prefix_covers_dest_dir() {
    assert!(include_covers(&["shaders/".into()], "shaders/foo.fx"));
    assert!(include_covers(&["shaders/".into()], "shaders/sub/bar.fx"));
    assert!(!include_covers(&["shaders/".into()], "shaders2/foo.fx"));
    assert!(!include_covers(&["shaders/".into()], "other.fx"));
    assert!(include_covers(&["x.dll".into()], "x.dll"));
    assert!(!include_covers(&["x.dll".into()], "x.dll.bak"));
    assert!(!include_covers(&[], "x.dll"));
}

#[test]
fn required_dest_honors_prefix_and_slot() {
    // Trailing-`/` include entries make covered dests omit-able.
    assert!(!is_required_dest(
        "custom",
        "shaders/x.dll",
        &["shaders/".into()],
        3
    ));
    assert!(is_required_dest(
        "custom",
        "shaders/x.dll",
        &["shaders/".into()],
        1
    ));
    // OptiScaler claiming dest follows the slot basename.
    assert!(is_required_dest("optiscaler", "dxgi.dll", &[], 3));
    assert!(is_required_dest("optiscaler", "winmm.dll", &[], 3));
    assert!(is_required_dest(
        "optiscaler",
        "winmm.dll",
        &["other.dll".into()],
        3
    ));
    assert!(!is_required_dest("optiscaler", "OptiScaler.ini", &[], 3));
    // Subdir companions never claim the proxy even when slot-named.
    assert!(!is_required_dest("optiscaler", "bin/dxgi.dll", &[], 3));
}

#[test]
fn wildcard_match() {
    assert!(glob_match("ReShade.ini", "ReShade.ini"));
    assert!(!glob_match("ReShade.ini", "reshade.ini"));
    assert!(glob_match("*.log", "ReShade.log"));
    assert!(!glob_match("*.log", "ReShade.ini"));
    assert!(glob_match("Example.log", "Example.log"));
}

#[test]
fn harvests_generated_files() {
    let dir = std::env::temp_dir().join(format!("tuxgt-e18-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    let root = dir.join("game");
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("ReShade.ini"), b"ini").unwrap();
    fs::write(root.join("ReShade.log"), b"log").unwrap();
    fs::write(root.join("game.exe"), b"exe").unwrap();
    let m = FileManifest {
        game: "steam::814380".into(),
        instance: "reshade".into(),
        mod_type: "reshade".into(),
        adapter: "preload".into(),
        enabled: true,
        load_order: 0,
        include: Box::default(),
        files: Box::default(),
        backups: Default::default(),
        generated_globs: Box::default(),
        harvested: Default::default(),
        provenance: ModProvenance::default(),
        env: Box::default(),
    };
    write_manifest(&dir, &m).unwrap();
    let found = harvest_game(&dir, "steam::814380", &root).unwrap();
    assert_eq!(found.len(), 2);
    let back = read_manifest(&dir, "steam::814380", "reshade")
        .unwrap()
        .unwrap();
    assert_eq!(
        &back.generated_globs[..],
        ["ReShade.ini".to_string(), "ReShade.log".to_string()]
    );
    assert_eq!(back.harvested.len(), 2);
    assert!(back.harvested.contains_key("ReShade.ini"));
    // Second harvest is stable: no rewrite, same set.
    let again = harvest_game(&dir, "steam::814380", &root).unwrap();
    assert_eq!(again.len(), 2);
    // Removed files drop out of the manifest.
    fs::remove_file(root.join("ReShade.log")).unwrap();
    let _ = harvest_game(&dir, "steam::814380", &root).unwrap();
    let back = read_manifest(&dir, "steam::814380", "reshade")
        .unwrap()
        .unwrap();
    assert_eq!(back.harvested.len(), 1);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn harvest_merges_roots_runtime_first() {
    let dir = std::env::temp_dir().join(format!("tuxgt-r58-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    let runtime = dir.join("runtime");
    let install = dir.join("install");
    fs::create_dir_all(&runtime).unwrap();
    fs::create_dir_all(&install).unwrap();
    fs::write(runtime.join("ReShade.ini"), b"runtime-ini").unwrap();
    fs::write(install.join("ReShade.ini"), b"install-ini").unwrap();
    fs::write(install.join("ReShade.log"), b"log").unwrap();
    let m = FileManifest {
        game: "steam::814380".into(),
        instance: "reshade".into(),
        mod_type: "reshade".into(),
        adapter: "preload".into(),
        enabled: true,
        load_order: 0,
        include: Box::default(),
        files: Box::default(),
        backups: Default::default(),
        generated_globs: Box::default(),
        harvested: Default::default(),
        provenance: ModProvenance::default(),
        env: Box::default(),
    };
    write_manifest(&dir, &m).unwrap();
    let found =
        harvest_game_roots(&dir, "steam::814380", &[runtime.clone(), install.clone()]).unwrap();
    assert_eq!(found.len(), 2);
    let back = read_manifest(&dir, "steam::814380", "reshade")
        .unwrap()
        .unwrap();
    assert_eq!(back.harvested.len(), 2);
    assert!(back.harvested.contains_key("ReShade.ini"));
    assert!(back.harvested.contains_key("ReShade.log"));
    // Colliding rel resolves to the runtime (first) root bytes.
    let ini = found
        .iter()
        .find(|p| p.file_name().unwrap() == "ReShade.ini")
        .unwrap();
    assert_eq!(ini, &runtime.join("ReShade.ini"));
    assert_eq!(fs::read(ini).unwrap(), b"runtime-ini");
    let log = found
        .iter()
        .find(|p| p.file_name().unwrap() == "ReShade.log")
        .unwrap();
    assert_eq!(log, &install.join("ReShade.log"));
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn glob_match_basics() {
    assert!(glob_match("ReShade_Setup_*.exe", "ReShade_Setup_6.8.0.exe"));
    assert!(!glob_match(
        "ReShade_Setup_*.exe",
        "ReShade_Setup_6.8.0.zip"
    ));
    assert!(glob_match("OptiScaler_*.7z", "OptiScaler_v3.7z"));
    assert!(glob_match("a?c", "abc"));
    assert!(!glob_match("a?c", "ac"));
    assert!(glob_match("a*.7z", "abc.7z"));
    assert!(!glob_match("*.exe", "setup.EXE"));
    assert!(glob_match("*", "anything"));
    assert!(!glob_match("setup", "setup.exe"));
}

#[test]
fn pick_release_asset_newest_sorted() {
    let assets = vec![
        GhAsset {
            name: "ReShade_Setup_6.9.0.exe".into(),
            browser_download_url: "https://x/690".into(),
        },
        GhAsset {
            name: "ReShade_Setup_6.8.0.exe".into(),
            browser_download_url: "https://x/680".into(),
        },
        GhAsset {
            name: "checksums.txt".into(),
            browser_download_url: "https://x/sums".into(),
        },
    ];
    assert_eq!(
        pick_release_asset(&assets, "ReShade_Setup_*.exe").unwrap(),
        "https://x/690"
    );
    assert!(pick_release_asset(&assets, "Nope_*.exe").is_err());
    assert!(pick_release_asset(&[], "ReShade_Setup_*.exe").is_err());
}

#[test]
fn prerelease_release_pick_newest_nondraft() {
    let rel = |tag: &str, published: &str, created: &str, draft: bool| GhRelease {
        tag_name: tag.into(),
        published_at: published.into(),
        created_at: created.into(),
        draft,
        assets: Box::default(),
    };
    // Rolling nightlies share a created_at; published_at decides.
    let rels = vec![
        rel("v1", "2026-09-10T00:00:00Z", "2026-09-05T00:00:00Z", false),
        rel(
            "nightly-new",
            "2026-09-12T00:00:00Z",
            "2026-09-05T00:00:00Z",
            false,
        ),
        rel(
            "nightly-older-published-first",
            "2026-09-11T00:00:00Z",
            "2026-09-05T00:00:00Z",
            false,
        ),
        rel(
            "draft",
            "2026-09-13T00:00:00Z",
            "2026-09-05T00:00:00Z",
            true,
        ),
    ];
    assert_eq!(
        pick_prerelease_release(&rels).unwrap().tag_name,
        "nightly-new"
    );
    let empty: Vec<GhRelease> = Vec::new();
    assert!(pick_prerelease_release(&empty).is_none());
}

#[tokio::test]
async fn github_source_url_literal_branches_no_network() {
    // Pinned and latest-stable literal assets are direct download URLs.
    assert_eq!(
        github_source_url("o", "r", Some("nightly-20260909"), false, "a.addon64")
            .await
            .unwrap(),
        "https://github.com/o/r/releases/download/nightly-20260909/a.addon64"
    );
    assert_eq!(
        github_source_url("o", "r", None, false, "a.zip")
            .await
            .unwrap(),
        "https://github.com/o/r/releases/latest/download/a.zip"
    );
}

#[test]
fn expected_bytes_full_and_partial() {
    assert_eq!(expected_bytes(false, 100, Some(50)), Some(50));
    assert_eq!(expected_bytes(true, 100, Some(50)), Some(150));
    assert_eq!(expected_bytes(false, 0, None), None);
}

fn serve_http(body: &'static [u8], advertised_len: usize, send_len: usize, n: usize) -> u16 {
    use std::io::{Read, Write};
    use std::net::TcpListener;
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    std::thread::spawn(move || {
        for _ in 0..n {
            let Ok((mut s, _)) = listener.accept() else {
                break;
            };
            let mut buf = [0u8; 2048];
            let _ = s.read(&mut buf);
            let hdr = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {advertised_len}\r\nConnection: close\r\n\r\n"
            );
            let _ = s.write_all(hdr.as_bytes());
            let _ = s.write_all(&body[..send_len.min(body.len())]);
        }
    });
    port
}

#[tokio::test]
async fn fetch_url_incomplete_is_download_error_not_pin_mismatch() {
    let body: &'static [u8] = b"0123456789abcdef";
    let pin = sha256_hex(body);
    let port = serve_http(body, body.len(), 7, 1);
    let dir = std::env::temp_dir().join(format!("tuxgt-r69-inc-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    let url = format!("http://127.0.0.1:{port}/d3dcompiler_47.dll");
    let err = fetch_url(&dir, &url, Some(&pin), Some("d3dcompiler-47"), None, false)
        .await
        .unwrap_err();
    // hyper errors on a short Content-Length; a clean short stream
    // hits our incomplete check. Neither is a pin-mismatch.
    assert!(matches!(&err, Error::Download(_)), "{err:?}");
    assert!(!matches!(err, Error::HashMismatch { .. }));
    let part = cached_file(&dir, &url)
        .parent()
        .unwrap()
        .join("d3dcompiler_47.dll.part");
    assert!(part.exists(), "short body keeps .part for resume");
    let _ = fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn fetch_url_overlapping_same_url_one_body() {
    let body: &'static [u8] = b"one-body-not-two";
    let port = serve_http(body, body.len(), body.len(), 2);
    let dir = std::env::temp_dir().join(format!("tuxgt-r69-ovl-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    let url = format!("http://127.0.0.1:{port}/d3dcompiler_47.dll");
    let (a, b) = tokio::join!(
        fetch_url(&dir, &url, None, Some("a"), None, false),
        fetch_url(&dir, &url, None, Some("b"), None, false),
    );
    let a = a.unwrap();
    let b = b.unwrap();
    assert_eq!(a.bytes, body.len() as u64);
    assert_eq!(b.bytes, body.len() as u64);
    assert_eq!(a.sha256, sha256_hex(body));
    assert_eq!(fs::read(&a.file).unwrap(), body);
    let _ = fs::remove_dir_all(&dir);
}
