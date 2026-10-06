use super::*;
use crate::Error;

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
