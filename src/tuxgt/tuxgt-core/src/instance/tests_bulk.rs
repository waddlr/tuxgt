use super::bulk::bulk_disabled_path;
use super::testing::*;
use super::*;
use std::collections::BTreeSet;
use std::fs;

#[test]
fn bulk_record_read_roundtrip() {
    let dir = temp_config();
    let cfg = dir.join("config");
    let mut games = BTreeSet::new();
    games.insert("manual:standalone:a".to_string());
    games.insert("manual:standalone:b".to_string());
    record_bulk_disabled(&cfg, "pack", &games).unwrap();
    let read = read_bulk_disabled(&cfg).unwrap();
    assert_eq!(read.get("pack"), Some(&games));
}

#[test]
fn bulk_record_merges_games() {
    let dir = temp_config();
    let cfg = dir.join("config");
    record_bulk_disabled(&cfg, "pack", &BTreeSet::from(["a".to_string()])).unwrap();
    record_bulk_disabled(&cfg, "pack", &BTreeSet::from(["b".to_string()])).unwrap();
    let read = read_bulk_disabled(&cfg).unwrap();
    assert_eq!(
        read.get("pack"),
        Some(&BTreeSet::from(["a".to_string(), "b".to_string()]))
    );
}

#[test]
fn bulk_unrecord_prunes_empty_entry_and_file() {
    let dir = temp_config();
    let cfg = dir.join("config");
    record_bulk_disabled(&cfg, "pack", &BTreeSet::from(["a".to_string()])).unwrap();
    unrecord_bulk_games(&cfg, "pack", &BTreeSet::from(["a".to_string()])).unwrap();
    assert!(read_bulk_disabled(&cfg).unwrap().is_empty());
    assert!(!bulk_disabled_path(&cfg).exists());
}

#[test]
fn bulk_clear_missing_is_noop_without_file() {
    let dir = temp_config();
    let cfg = dir.join("config");
    clear_bulk_disabled(&cfg, "pack").unwrap();
    assert!(!bulk_disabled_path(&cfg).exists());
}

#[test]
fn remove_mod_clears_bulk_and_offered_state() {
    let dir = temp_config();
    let src = dir.join("src.toml");
    fs::create_dir_all(&dir).unwrap();
    fs::write(&src, sample_recipe("fork", "\"preload\"")).unwrap();
    add_mod(&dir, &src, &dir).unwrap();
    disable_mod(&dir, "fork", &dir).unwrap();
    record_bulk_disabled(&dir, "fork", &BTreeSet::from(["g".to_string()])).unwrap();
    remove_mod(&dir, &dir, "fork").unwrap();
    assert!(!read_bulk_disabled(&dir).unwrap().contains_key("fork"));
    assert!(!read_disabled(&dir).unwrap().contains("fork"));
}
