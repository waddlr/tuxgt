use super::binary::{apis_from_libs, parse_binary, pe_header_bitness, BinaryKind};
use super::delay::collect_delay_names;
use super::testing::{pe32_header_only, pe32_i386, pe32_iat_d3d9_delay_d3d10, write_file};
use super::{detect_one, fetch_row, fingerprint, fp_paths, DetectOpts};
use crate::open_db;
use crate::testing::{seed_game, SeedGame};
use std::collections::BTreeSet;

#[test]
fn dx10_from_d3d10() {
    let mut libs = BTreeSet::new();
    libs.insert("d3d10.dll".into());
    assert_eq!(apis_from_libs(&libs), vec!["dx10"]);
}

#[test]
fn apis_from_libs_d3d9_and_d3d10_default_dx10() {
    let mut libs = BTreeSet::new();
    libs.insert("d3d9.dll".into());
    libs.insert("d3d10.dll".into());
    assert_eq!(apis_from_libs(&libs), vec!["dx10", "dx9"]);
}

#[test]
fn delay_names_rva_and_va() {
    let mut bytes = vec![0u8; 0x80];
    bytes[0x00..0x04].copy_from_slice(&1u32.to_le_bytes());
    bytes[0x04..0x08].copy_from_slice(&0x1050u32.to_le_bytes());
    bytes[0x24..0x28].copy_from_slice(&(0x0040_0000u32 + 0x1060).to_le_bytes());
    bytes[0x50..0x5a].copy_from_slice(b"d3d10.dll\0");
    bytes[0x60..0x69].copy_from_slice(b"d3d9.dll\0");
    let mut sec = goblin::pe::section_table::SectionTable::default();
    sec.virtual_address = 0x1000;
    sec.virtual_size = 0x80;
    sec.size_of_raw_data = 0x80;
    sec.pointer_to_raw_data = 0;
    let names = collect_delay_names(&bytes, &[sec], 0x0040_0000, 0x1000, 0x60);
    assert!(names.contains("d3d10.dll"), "{names:?}");
    assert!(names.contains("d3d9.dll"), "{names:?}");
}

fn write_temp_exe(tag: &str, bytes: &[u8]) -> (std::path::PathBuf, std::path::PathBuf) {
    let dir = std::env::temp_dir().join(format!(
        "tuxgt-pe-{tag}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let exe = dir.join("game.exe");
    write_file(&exe, bytes);
    (dir, exe)
}

#[test]
fn parse_binary_unions_iat_and_delay_import() {
    let bytes = pe32_iat_d3d9_delay_d3d10(true);
    let (dir, exe) = write_temp_exe("delay-rva", &bytes);
    assert!(goblin::Object::parse(&bytes).is_ok());
    let info = parse_binary(&exe).expect("pe");
    assert_eq!(info.bitness, 32);
    assert!(info.libs.contains("d3d9.dll"), "{:?}", info.libs);
    assert!(info.libs.contains("d3d10.dll"), "{:?}", info.libs);
    assert_eq!(apis_from_libs(&info.libs), vec!["dx10", "dx9"]);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn parse_binary_delay_import_va_name() {
    let bytes = pe32_iat_d3d9_delay_d3d10(false);
    let (dir, exe) = write_temp_exe("delay-va", &bytes);
    let info = parse_binary(&exe).expect("pe");
    assert!(info.libs.contains("d3d9.dll"), "{:?}", info.libs);
    assert!(info.libs.contains("d3d10.dll"), "{:?}", info.libs);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn live_jc2_delay_import_defaults_dx10_if_present() {
    let p = std::path::Path::new(
        "/home/ytn/games/heroic/Just Cause 2 - Complete Edition/JustCause2.exe",
    );
    if !p.is_file() {
        return;
    }
    let info = parse_binary(p).expect("jc2 pe");
    assert!(info.libs.contains("d3d10.dll"), "{:?}", info.libs);
    assert_eq!(apis_from_libs(&info.libs).first().copied(), Some("dx10"));
}

#[test]
fn pe_header_bitness_i386() {
    assert_eq!(pe_header_bitness(&pe32_header_only()), Some(32));
    assert_eq!(pe_header_bitness(&pe32_i386(true)), Some(32));
    assert_eq!(pe_header_bitness(b"not a pe"), None);
}

#[test]
fn parse_binary_recovers_cert_past_eof() {
    let dir = std::env::temp_dir().join(format!(
        "tuxgt-pe-cert-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let exe = dir.join("jc2.exe");
    let bytes = pe32_i386(true);
    write_file(&exe, &bytes);
    assert!(
        goblin::Object::parse(&bytes).is_err(),
        "fixture must fail a strict parse"
    );
    let info = parse_binary(&exe).expect("lenient or header fallback");
    assert_eq!(info.kind, BinaryKind::Pe);
    assert_eq!(info.bitness, 32);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn parse_binary_recovers_header_only_pe() {
    let dir = std::env::temp_dir().join(format!(
        "tuxgt-pe-hdr-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let exe = dir.join("old.exe");
    let bytes = pe32_header_only();
    write_file(&exe, &bytes);
    assert!(goblin::Object::parse(&bytes).is_err());
    let info = parse_binary(&exe).expect("header fallback");
    assert_eq!(info.kind, BinaryKind::Pe);
    assert_eq!(info.bitness, 32);
    assert!(info.libs.is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn detect_one_reruns_when_bitness_empty() {
    let dir = std::env::temp_dir().join(format!(
        "tuxgt-det-bit-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let _ = tokio::fs::remove_dir_all(&dir).await;
    let pool = open_db(&dir).await.unwrap();
    let install = dir.join("game");
    let exe = install.join("JustCause2.exe");
    write_file(&exe, &pe32_i386(true));
    let id = "manual:standalone:bbbbbbbb";
    seed_game(
        &pool,
        SeedGame {
            id,
            manager: "manual",
            store: "standalone",
            game_id: "bbbbbbbb",
            name: Some("jc2"),
            install_dir: Some(install.to_str().unwrap()),
            detected_exe_path: Some(exe.to_str().unwrap()),
            detected_platform: Some("proton"),
            ..Default::default()
        },
    )
    .await;
    let fp = fingerprint(&fp_paths(Some(&install), Some(&exe), None));
    sqlx::query("UPDATE games SET fingerprint = ? WHERE id = ?")
        .bind(&fp)
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
    let skipped = detect_one(&pool, id, DetectOpts::default()).await.unwrap();
    assert!(!skipped, "empty bitness must re-run");
    let row = fetch_row(&pool, id).await.unwrap().unwrap();
    assert_eq!(row.detected_bitness.as_deref(), Some("32"));
    let _ = tokio::fs::remove_dir_all(&dir).await;
}

#[tokio::test]
async fn detect_one_reruns_when_pe_api_stale() {
    let dir = std::env::temp_dir().join(format!(
        "tuxgt-det-api-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let _ = tokio::fs::remove_dir_all(&dir).await;
    let pool = open_db(&dir).await.unwrap();
    let install = dir.join("game");
    let exe = install.join("JustCause2.exe");
    write_file(&exe, &pe32_iat_d3d9_delay_d3d10(true));
    let id = "manual:standalone:cccccccc";
    seed_game(
        &pool,
        SeedGame {
            id,
            manager: "manual",
            store: "standalone",
            game_id: "cccccccc",
            name: Some("jc2"),
            install_dir: Some(install.to_str().unwrap()),
            detected_exe_path: Some(exe.to_str().unwrap()),
            detected_platform: Some("proton"),
            detected_bitness: Some("32"),
            detected_api: Some("dx9"),
            ..Default::default()
        },
    )
    .await;
    let fp = fingerprint(&fp_paths(Some(&install), Some(&exe), None));
    sqlx::query("UPDATE games SET fingerprint = ? WHERE id = ?")
        .bind(&fp)
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
    let skipped = detect_one(&pool, id, DetectOpts::default()).await.unwrap();
    assert!(!skipped, "stale pe api must re-run");
    let row = fetch_row(&pool, id).await.unwrap().unwrap();
    assert_eq!(row.detected_api.as_deref(), Some("dx10"));
    let skipped = detect_one(&pool, id, DetectOpts::default()).await.unwrap();
    assert!(skipped, "matching pe fields must skip");
    let _ = tokio::fs::remove_dir_all(&dir).await;
}
