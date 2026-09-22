use super::*;
use std::fs;
use std::path::{Path, PathBuf};

#[test]
fn github_release_url_pins_tag_or_latest() {
    assert_eq!(
        github_release_url("o", "r", None, "a.zip"),
        "https://github.com/o/r/releases/latest/download/a.zip"
    );
    assert_eq!(
        github_release_url("o", "r", Some("nightly-20260909"), "a.addon64"),
        "https://github.com/o/r/releases/download/nightly-20260909/a.addon64"
    );
}

#[test]
fn url_key_stable_hex() {
    let k = url_key("https://example.com/a.7z");
    assert_eq!(k.len(), 16);
    assert_eq!(k, url_key("https://example.com/a.7z"));
    assert_ne!(k, url_key("https://example.com/b.7z"));
}

#[test]
fn manifest_roundtrip() {
    let dir = std::env::temp_dir().join(format!("tuxgt-e15-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    let m = FileManifest {
        game: "steam::814380".into(),
        instance: "optiscaler".into(),
        mod_type: "optiscaler".into(),
        adapter: "preload".into(),
        enabled: true,
        load_order: 0,
        include: Box::default(),
        files: vec![PlannedFile {
            source: "cache/ab12/OptiScaler.zip#OptiScaler.dll".into(),
            dest: "dxgi.dll".into(),
            sha256: "aa".into(),
            enabled: true,
        }]
        .into_boxed_slice(),
        backups: Default::default(),
        generated_globs: vec!["OptiScaler.ini".into()].into_boxed_slice(),
        harvested: Default::default(),
        provenance: ModProvenance::default(),
        env: Box::default(),
    };
    let path = write_manifest(&dir, &m).unwrap();
    assert!(path.to_string_lossy().contains("manifests/optiscaler.toml"));
    let back = read_manifest(&dir, "steam::814380", "optiscaler")
        .unwrap()
        .unwrap();
    assert_eq!(back.instance, "optiscaler");
    assert!(back.enabled);
    assert!(read_manifest(&dir, "steam::1", "optiscaler")
        .unwrap()
        .is_none());
    assert_eq!(game_manifests(&dir, "steam::814380").unwrap().len(), 1);
    assert!(game_manifests(&dir, "steam::1").unwrap().is_empty());
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn mod_env_keep_roundtrip_and_unknown_key() {
    let dir = std::env::temp_dir().join(format!("tuxgt-e74-keep-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    let m = FileManifest {
        game: "steam::1".into(),
        instance: "d3dcompiler-47".into(),
        mod_type: "custom".into(),
        adapter: "preload".into(),
        enabled: true,
        load_order: 0,
        include: Box::default(),
        files: Box::default(),
        env: vec![PlannedEnv {
            key: "WINEDLLOVERRIDES".into(),
            value: "d3dcompiler_47=n".into(),
            enabled: true,
        }]
        .into_boxed_slice(),
        backups: Default::default(),
        generated_globs: Box::default(),
        harvested: Default::default(),
        provenance: ModProvenance::default(),
    };
    write_manifest(&dir, &m).unwrap();
    let m = set_mod_env_enabled(
        &dir,
        "steam::1",
        "d3dcompiler-47",
        "WINEDLLOVERRIDES",
        false,
    )
    .unwrap();
    assert!(
        !m.env
            .iter()
            .find(|e| e.key == "WINEDLLOVERRIDES")
            .unwrap()
            .enabled
    );
    let back = read_manifest(&dir, "steam::1", "d3dcompiler-47")
        .unwrap()
        .unwrap();
    assert!(!back.env[0].enabled);
    assert!(set_mod_env_enabled(&dir, "steam::1", "d3dcompiler-47", "NOPE", true).is_err());
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn per_dest_keep_roundtrip_and_required_refusal() {
    let dir = std::env::temp_dir().join(format!("tuxgt-e64-keep-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    let path = manifest_path(&dir, "steam::1", "optiscaler");
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    // Hand-written manifest: the missing key must read as kept.
    fs::write(
        &path,
        r#"game = "steam::1"
instance = "optiscaler"
type = "optiscaler"
adapter = "preload"
enabled = true

[[files]]
source = "s"
dest = "dxgi.dll"
sha256 = "aa"

[[files]]
source = "s"
dest = "OptiScaler.ini"
sha256 = "bb"
enabled = false
"#,
    )
    .unwrap();
    let m = read_manifest(&dir, "steam::1", "optiscaler")
        .unwrap()
        .unwrap();
    assert!(m.files[0].enabled, "missing key means kept");
    assert!(!m.files[1].enabled);
    set_file_enabled(&dir, "steam::1", "optiscaler", "OptiScaler.ini", true).unwrap();
    let back = read_manifest(&dir, "steam::1", "optiscaler")
        .unwrap()
        .unwrap();
    assert!(back.files[1].enabled);
    // The type's slot dest cannot be omitted; unknown dests error.
    assert!(set_file_enabled(&dir, "steam::1", "optiscaler", "dxgi.dll", false).is_err());
    assert!(set_file_enabled(&dir, "steam::1", "optiscaler", "nope.dll", false).is_err());
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn cached_file_lives_under_cache_with_extension() {
    let dir = PathBuf::from("/data");
    let p = cached_file(&dir, "https://cdn2.steamgriddb.com/grid/a.png?x=1");
    assert_eq!(p.file_name().unwrap(), "a.png");
    assert!(p.to_string_lossy().starts_with("/data/downloads/"));
}

#[test]
fn art_file_is_keyed_by_game_id() {
    let dir = PathBuf::from("/data");
    let p = art_file(&dir, "heroic:gog:123");
    assert_eq!(
        p,
        PathBuf::from("/data/config/cache/art/heroic_gog_123/cover")
    );
    assert_eq!(
        hero_file(&dir, "heroic:gog:123"),
        PathBuf::from("/data/config/cache/art/heroic_gog_123/hero")
    );
}

#[test]
fn filename_from_url_keeps_extension() {
    assert_eq!(filename_from_url("https://example.com/a.7z"), "a.7z");
    assert_eq!(filename_from_url("https://example.com/x.zip?dl=1"), "x.zip");
    assert_eq!(
        filename_from_url("https://example.com/a b/c.tar.gz"),
        "c.tar.gz"
    );
    assert_eq!(filename_from_url("https://example.com/"), "asset");
    assert_eq!(
        classify(Path::new(&filename_from_url(
            "https://e.com/p/OptiScaler_v1.7z"
        ))),
        Archive::SevenZ
    );
}

#[test]
fn classify_archives() {
    assert_eq!(classify(Path::new("a.ZIP")), Archive::Zip);
    assert_eq!(classify(Path::new("a.rar")), Archive::Rar);
    assert_eq!(classify(Path::new("a.7z")), Archive::SevenZ);
    assert_eq!(classify(Path::new("a.cab")), Archive::Cab);
    assert_eq!(classify(Path::new("SDK.CAB")), Archive::Cab);
    assert_eq!(classify(Path::new("a.tar.gz")), Archive::TarGz);
    assert_eq!(classify(Path::new("a.tgz")), Archive::TarGz);
    assert_eq!(classify(Path::new("a.tar.bz2")), Archive::TarBz2);
    assert_eq!(classify(Path::new("a.dll")), Archive::Plain);
    assert_eq!(
        classify(Path::new("ReShade_Setup_6.8.exe")),
        Archive::SelfExtract
    );
}

#[test]
fn strip_top_dir_only_when_single() {
    let dir = std::env::temp_dir().join(format!("tuxgt-e15-strip-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    let one = dir.join("one");
    fs::create_dir_all(one.join("pkg")).unwrap();
    fs::write(one.join("pkg").join("a.dll"), b"x").unwrap();
    strip_single_top_dir(&one).unwrap();
    assert!(one.join("a.dll").exists());
    let two = dir.join("two");
    fs::create_dir_all(&two).unwrap();
    fs::write(two.join("a.dll"), b"x").unwrap();
    fs::write(two.join("b.dll"), b"y").unwrap();
    strip_single_top_dir(&two).unwrap();
    assert!(two.join("a.dll").exists() && two.join("b.dll").exists());
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn unpack_directory_copies_tree() {
    let dir = std::env::temp_dir().join(format!("tuxgt-e49-unpack-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    let src = dir.join("src");
    fs::create_dir_all(src.join("nested")).unwrap();
    fs::write(src.join("a.dll"), b"a").unwrap();
    fs::write(src.join("nested").join("b.ini"), b"b").unwrap();
    let out = dir.join("out");
    let files = unpack(&src, &out).unwrap();
    assert!(files.iter().any(|p| p == &out.join("a.dll")));
    assert!(files.iter().any(|p| p == &out.join("nested").join("b.ini")));
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn tar_roundtrip_via_tool() {
    if !tool_status(&ExtTool {
        name: "tar",
        probe: &["tar", "--version"],
    })
    .found
    {
        return;
    }
    let dir = std::env::temp_dir().join(format!("tuxgt-e15-tar-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    let src = dir.join("src");
    fs::create_dir_all(src.join("pkg")).unwrap();
    fs::write(src.join("pkg").join("a.dll"), b"dll-bytes").unwrap();
    run("tar", &["-czf", "a.tar.gz", "-C", ".", "pkg"], &src).unwrap();
    let out = dir.join("out");
    let files = unpack(&src.join("a.tar.gz"), &out).unwrap();
    assert_eq!(files, vec![out.join("a.dll")]);
    assert_eq!(fs::read(&out.join("a.dll")).unwrap(), b"dll-bytes");
    let _ = fs::remove_dir_all(&dir);
}

/// Minimal stored (uncompressed) MS-CAB writer. `7z` cannot create cabs
/// (E_NOTIMPL) and nothing else here writes them, so the E72 fixture is
/// built by hand instead of vendored: CFHEADER + one CFFOLDER + one
/// CFFILE + one CFDATA block, no compression, unchecked checksum.
fn write_stored_cab(path: &Path, name: &str, content: &[u8]) {
    fn le16(buf: &mut Vec<u8>, v: u16) {
        buf.extend_from_slice(&v.to_le_bytes());
    }
    fn le32(buf: &mut Vec<u8>, v: u32) {
        buf.extend_from_slice(&v.to_le_bytes());
    }
    let named = format!("{name}\0");
    let cfile_len = 16 + named.len();
    let coff_files = 36u32 + 8;
    let coff_cab_start = coff_files + cfile_len as u32;
    let total = coff_cab_start + 8 + content.len() as u32;
    let mut b = Vec::with_capacity(total as usize);
    b.extend_from_slice(b"MSCF");
    le32(&mut b, 0); // reserved1
    le32(&mut b, total); // cbCabinet
    le32(&mut b, 0); // reserved2
    le32(&mut b, coff_files); // coffFiles
    le32(&mut b, 0); // reserved3
    b.push(3); // versionMinor
    b.push(1); // versionMajor
    le16(&mut b, 1); // cFolders
    le16(&mut b, 1); // cFiles
    le16(&mut b, 0); // flags
    le16(&mut b, 0); // setID
    le16(&mut b, 0); // iCabinet
    assert_eq!(b.len(), 36);
    le32(&mut b, coff_cab_start); // CFFOLDER.coffCabStart
    le16(&mut b, 1); // cCFData
    le16(&mut b, 0); // typeCompress: stored
    le32(&mut b, content.len() as u32); // CFFILE.cbFile
    le32(&mut b, 0); // uoffFolderStart
    le16(&mut b, 0); // iFolder
    le16(&mut b, 0x4A21); // date
    le16(&mut b, 0); // time
    le16(&mut b, 0x20); // attribs: archive
    b.extend_from_slice(named.as_bytes());
    le32(&mut b, 0); // CFDATA.csum (unchecked)
    le16(&mut b, content.len() as u16); // cbData
    le16(&mut b, content.len() as u16); // cbUncomp
    b.extend_from_slice(content);
    assert_eq!(b.len(), total as usize);
    fs::write(path, &b).unwrap();
}

#[test]
fn cab_roundtrip_via_7z() {
    if !tool_status(&ExtTool {
        name: "7z",
        probe: &["7z"],
    })
    .found
    {
        return; // documented skip: extraction shells `7z`
    }
    let dir = std::env::temp_dir().join(format!("tuxgt-e72-cab-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    let src = dir.join("src");
    fs::create_dir_all(&src).unwrap();
    write_stored_cab(&src.join("sdk.cab"), "inner.dll", b"cab-bytes");
    let out = dir.join("out");
    let files = unpack(&src.join("sdk.cab"), &out).unwrap();
    assert_eq!(files, vec![out.join("inner.dll")]);
    assert_eq!(fs::read(&out.join("inner.dll")).unwrap(), b"cab-bytes");
    assert!(
        !out.join("sdk.cab").exists(),
        "cab is extracted, not copied as plain"
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn cab_never_copies_as_plain() {
    // A corrupt cab errors instead of staging the cab itself as payload.
    if !tool_status(&ExtTool {
        name: "7z",
        probe: &["7z"],
    })
    .found
    {
        return; // without 7z this is a MissingTool naming error by construction
    }
    let dir = std::env::temp_dir().join(format!("tuxgt-e72-cab-bad-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let bad = dir.join("bad.cab");
    fs::write(&bad, b"not a cabinet").unwrap();
    assert!(unpack(&bad, &dir.join("out")).is_err());
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn password_protected_zip_and_7z_are_distinguishable() {
    if !tool_status(&ExtTool {
        name: "7z",
        probe: &["7z"],
    })
    .found
    {
        return;
    }
    let dir = std::env::temp_dir().join(format!("tuxgt-pw-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    let src = dir.join("src");
    fs::create_dir_all(&src).unwrap();
    fs::write(src.join("a.dll"), b"payload").unwrap();
    for ext in ["zip", "7z"] {
        let archive = dir.join(format!("protected.{ext}"));
        let kind = format!("-t{ext}");
        let created = std::process::Command::new("7z")
            .args([
                "a",
                kind.as_str(),
                "-psecret",
                archive.to_str().unwrap(),
                src.join("a.dll").to_str().unwrap(),
            ])
            .output()
            .unwrap();
        assert!(created.status.success());

        let missing = unpack_with_password(&archive, &dir.join(format!("missing-{ext}")), None)
            .unwrap_err();
        assert!(matches!(missing, crate::Error::ArchivePasswordRequired));

        let wrong = unpack_with_password(
            &archive,
            &dir.join(format!("wrong-{ext}")),
            Some("wrong"),
        )
        .unwrap_err();
        assert!(matches!(wrong, crate::Error::ArchivePasswordRequired), "{wrong:?}");

        let out = dir.join(format!("out-{ext}"));
        let files = unpack_with_password(&archive, &out, Some("secret")).unwrap();
        assert_eq!(files, vec![out.join("a.dll")]);
    }
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn rar_password_markers_are_recognized_without_running_unrar() {
    assert!(password_error(b"ERROR: Wrong password : payload.dll"));
    assert!(password_error(b"Can not open encrypted archive. Wrong password?"));
    assert!(!password_error(b"Headers Error"));
}
