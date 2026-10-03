//! Apply refuse/fail paths: no prefix, bad trees, stuck stale, host refresh.

use super::apply::apply_app_update;
use super::manifest::{load_prefix_manifest, save_prefix_manifest, PrefixFile, PrefixManifest};
use super::tests_0::{downloads_clean, pack_tarball, serve_once, tmp_root, tree_join, write_tree};

#[tokio::test]
async fn apply_refuses_before_any_fetch_on_a_bare_dir() {
    let root = tmp_root("bare");
    let prefix = root.join("prefix");
    std::fs::create_dir_all(&prefix).unwrap();
    // Unroutable URL: refusal must precede the fetch.
    let err = apply_app_update(&prefix, "v999.0.0", "http://127.0.0.1:9/x", None, None)
        .await
        .unwrap_err();
    assert!(err.to_string().contains("not an installed prefix"), "{err}");
    let _ = std::fs::remove_dir_all(&root);
}

#[tokio::test]
async fn apply_refuses_a_tree_without_bin_tuxgt() {
    let root = tmp_root("nobin");
    let prefix = root.join("prefix");
    write_tree(&prefix, &[("bin/tuxgt", "old")]);
    let tree = root.join("tree");
    write_tree(&tree.join("tuxgt"), &[("lib/only.so", "x")]);
    let url = serve_once(pack_tarball(&tree));
    let err = apply_app_update(&prefix, "v999.0.0", &url, None, None)
        .await
        .unwrap_err();
    assert!(err.to_string().contains("no bin/tuxgt"), "{err}");
    assert_eq!(
        std::fs::read_to_string(prefix.join("bin/tuxgt")).unwrap(),
        "old"
    );
    assert!(downloads_clean(&prefix));
    let _ = std::fs::remove_dir_all(&root);
}

#[tokio::test]
async fn apply_refuses_symlinks_and_unowned_paths() {
    for (case, setup) in [("symlink", "lib/link.so"), ("unowned", "games/evil")] {
        let root = tmp_root(case);
        let prefix = root.join("prefix");
        write_tree(&prefix, &[("bin/tuxgt", "old")]);
        let pkg = tree_join(&root);
        write_tree(&pkg, &[("bin/tuxgt", "new")]);
        if case == "symlink" {
            let link = pkg.join(setup);
            std::fs::create_dir_all(link.parent().unwrap()).unwrap();
            std::os::unix::fs::symlink("x", &link).unwrap();
        } else {
            write_tree(&pkg, &[(setup, "x")]);
        }
        let url = serve_once(pack_tarball(&root.join("tree")));
        let err = apply_app_update(&prefix, "v999.0.0", &url, None, None)
            .await
            .unwrap_err();
        assert!(
            err.to_string().contains("symlink") || err.to_string().contains("unowned"),
            "{case}: {err}"
        );
        assert_eq!(
            std::fs::read_to_string(prefix.join("bin/tuxgt")).unwrap(),
            "old",
            "{case}"
        );
        assert!(downloads_clean(&prefix), "{case}");
        let _ = std::fs::remove_dir_all(&root);
    }
}

#[tokio::test]
async fn apply_without_manifest_skips_the_stale_pass() {
    let root = tmp_root("legacy");
    let prefix = root.join("prefix");
    let sentinel = root.join("host-refresh.log");
    write_tree(&prefix, &[("bin/tuxgt", "old"), ("bin/old-tool", "stale")]);
    let tree = root.join("tree");
    let script = format!("#!/bin/sh\necho \"$@\" >> {}\nexit 0\n", sentinel.display());
    write_tree(
        &tree.join("tuxgt"),
        &[
            ("bin/tuxgt", &script),
            ("share/applications/tuxgt.desktop", "generic\n"),
        ],
    );
    let url = serve_once(pack_tarball(&tree));
    let rep = apply_app_update(&prefix, "v999.0.0", &url, None, None)
        .await
        .expect("apply");
    assert_eq!(rep.updated, 2);
    assert!(rep.removed.is_empty());
    assert!(prefix.join("bin/old-tool").is_file());
    // The desktop entry is written when missing (kept when present).
    assert_eq!(
        std::fs::read_to_string(prefix.join("share/applications/tuxgt.desktop")).unwrap(),
        "generic\n"
    );
    assert_eq!(
        load_prefix_manifest(&prefix)
            .unwrap()
            .expect("manifest")
            .tag,
        "v999.0.0"
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[tokio::test]
async fn apply_reports_a_stuck_stale_file_loudly() {
    let root = tmp_root("stuck");
    let prefix = root.join("prefix");
    let sentinel = root.join("host-refresh.log");
    write_tree(&prefix, &[("bin/tuxgt", "old")]);
    // A directory where a stale file should be: `remove_file` fails loud.
    std::fs::create_dir_all(prefix.join("bin/blocker")).unwrap();
    save_prefix_manifest(
        &prefix,
        &PrefixManifest {
            tag: "v0.0.1".into(),
            files: vec![
                PrefixFile {
                    path: "bin/tuxgt".into(),
                    sha256: "prev".into(),
                },
                PrefixFile {
                    path: "bin/blocker".into(),
                    sha256: "prev".into(),
                },
            ],
        },
    )
    .unwrap();
    let tree = root.join("tree");
    let script = format!("#!/bin/sh\necho \"$@\" >> {}\nexit 0\n", sentinel.display());
    write_tree(&tree.join("tuxgt"), &[("bin/tuxgt", &script)]);
    let url = serve_once(pack_tarball(&tree));
    let err = apply_app_update(&prefix, "v999.0.0", &url, None, None)
        .await
        .unwrap_err();
    assert!(err.to_string().contains("cannot remove stale"), "{err}");
    // The manifest is saved after the stale pass, so a re-run retries.
    assert_eq!(
        load_prefix_manifest(&prefix)
            .unwrap()
            .expect("manifest")
            .tag,
        "v0.0.1"
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[tokio::test]
async fn apply_reports_a_failed_host_refresh_loudly() {
    let root = tmp_root("hostfail");
    let prefix = root.join("prefix");
    write_tree(&prefix, &[("bin/tuxgt", "old")]);
    let tree = root.join("tree");
    write_tree(&tree.join("tuxgt"), &[("bin/tuxgt", "#!/bin/sh\nexit 3\n")]);
    let url = serve_once(pack_tarball(&tree));
    let err = apply_app_update(&prefix, "v999.0.0", &url, None, None)
        .await
        .unwrap_err();
    assert!(err.to_string().contains("host refresh failed"), "{err}");
    // The prefix itself is already new and tracked.
    assert_eq!(
        std::fs::read_to_string(prefix.join("bin/tuxgt")).unwrap(),
        "#!/bin/sh\nexit 3\n"
    );
    assert_eq!(
        load_prefix_manifest(&prefix)
            .unwrap()
            .expect("manifest")
            .tag,
        "v999.0.0"
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[tokio::test]
async fn apply_cancel_before_fetch_leaves_the_prefix() {
    let root = tmp_root("cancel-pre");
    let prefix = root.join("prefix");
    write_tree(&prefix, &[("bin/tuxgt", "old")]);
    let tree = root.join("tree");
    write_tree(&tree.join("tuxgt"), &[("bin/tuxgt", "new")]);
    let url = serve_once(pack_tarball(&tree));
    let flag = std::sync::atomic::AtomicBool::new(true);
    let err = apply_app_update(&prefix, "v999.0.0", &url, None, Some(&flag))
        .await
        .unwrap_err();
    assert!(matches!(err, crate::Error::Cancelled), "{err}");
    assert_eq!(
        std::fs::read_to_string(prefix.join("bin/tuxgt")).unwrap(),
        "old"
    );
    let _ = std::fs::remove_dir_all(&root);
}

/// Header + first byte, then a pause, then the rest — long enough for the
/// sink to raise the cancel flag before overlay.
fn serve_slow(bytes: Vec<u8>) -> String {
    use std::io::{Read, Write};
    use std::net::TcpListener;
    let leaked: &'static [u8] = Box::leak(bytes.into_boxed_slice());
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    std::thread::spawn(move || {
        let Ok((mut s, _)) = listener.accept() else {
            return;
        };
        let mut buf = [0u8; 2048];
        let _ = s.read(&mut buf);
        let hdr = format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            leaked.len()
        );
        let _ = s.write_all(hdr.as_bytes());
        let split = 1.min(leaked.len());
        let _ = s.write_all(&leaked[..split]);
        let _ = s.flush();
        std::thread::sleep(std::time::Duration::from_millis(400));
        if leaked.len() > split {
            let _ = s.write_all(&leaked[split..]);
        }
    });
    format!("http://127.0.0.1:{port}/tuxgt.tar.gz")
}

#[tokio::test]
async fn apply_cancel_mid_fetch_keeps_the_old_binary() {
    let root = tmp_root("cancel-mid");
    let prefix = root.join("prefix");
    write_tree(&prefix, &[("bin/tuxgt", "old")]);
    let tree = root.join("tree");
    write_tree(&tree.join("tuxgt"), &[("bin/tuxgt", "new")]);
    let url = serve_slow(pack_tarball(&tree));
    let flag = std::sync::atomic::AtomicBool::new(false);
    let calls = std::sync::atomic::AtomicU64::new(0);
    let sink = |_: crate::download::FetchProgress| {
        if calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst) >= 1 {
            flag.store(true, std::sync::atomic::Ordering::SeqCst);
        }
    };
    let err = apply_app_update(&prefix, "v999.0.0", &url, Some(&sink), Some(&flag))
        .await
        .unwrap_err();
    assert!(matches!(err, crate::Error::Cancelled), "{err}");
    assert_eq!(
        std::fs::read_to_string(prefix.join("bin/tuxgt")).unwrap(),
        "old"
    );
    assert!(
        part_under(&prefix.join("downloads")),
        "cancelled fetch keeps .part"
    );
    let _ = std::fs::remove_dir_all(&root);
}

fn part_under(dir: &std::path::Path) -> bool {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return false;
    };
    rd.flatten().any(|e| {
        let p = e.path();
        (p.is_dir() && part_under(&p)) || p.extension().is_some_and(|ext| ext == "part")
    })
}
