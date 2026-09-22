use std::path::{Path, PathBuf};

use super::tests_0::vdf_int;
use super::*;
use crate::apply::{quote_fragment, ApplyCtx};

/// String node: kind `0x01` + NUL-terminated key and value.
fn bin_str(field: &str, value: &str) -> Vec<u8> {
    let mut out = vec![1u8];
    out.extend_from_slice(field.as_bytes());
    out.push(0);
    out.extend_from_slice(value.as_bytes());
    out.push(0);
    out
}

/// Object node header: kind `0x00` + NUL-terminated key.
fn bin_obj(key: &str) -> Vec<u8> {
    let mut out = vec![0u8];
    out.extend_from_slice(key.as_bytes());
    out.push(0);
    out
}

/// One realistic shortcut entry: appid, the strings Steam writes, a flag, a
/// `tags` string, and `LaunchOptions` when the game has any. Closes with the
/// entry's own `0x08`.
fn entry(key: &str, appid: u32, name: &str, options: Option<&str>) -> Vec<u8> {
    let mut out = bin_obj(key);
    out.extend(vdf_int("appid", appid));
    out.extend(bin_str("AppName", name));
    out.extend(bin_str("Exe", "/games/one/game.exe"));
    out.extend(bin_str("StartDir", "/games/one"));
    out.extend(vdf_int("IsHidden", 0));
    if let Some(o) = options {
        out.extend(bin_str("LaunchOptions", o));
    }
    out.extend(bin_str("tags", "0"));
    out.push(8);
    out
}

/// Binary blob: root `"shortcuts"` object holding both entries, closed by
/// the file's final `0x08`.
fn fixture(options: Option<&str>) -> Vec<u8> {
    let mut out = bin_obj("shortcuts");
    out.extend(entry("0", 111, "Game One", options));
    out.extend(entry("1", 222, "Game Two", None));
    out.push(8);
    out
}

fn scratch(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "tuxgt-steam-shortcuts-{}-{tag}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn ctx(data_dir: &Path, launcher: &Path) -> ApplyCtx {
    ApplyCtx {
        data_dir: data_dir.into(),
        launcher: launcher.into(),
        ini: data_dir.join("game.ini"),
        game_dir: data_dir.join("runtime"),
        depot: data_dir.join("stage"),
    }
}

#[test]
fn shortcut_options_round_trip() {
    let blob = fixture(Some("gamemoderun %command%"));
    assert_eq!(
        shortcut_get_options(&blob, 111).as_deref(),
        Some("gamemoderun %command%")
    );
    // Absent key, and an appid the file does not carry.
    assert_eq!(shortcut_get_options(&blob, 222), None);
    assert_eq!(shortcut_get_options(&blob, 999), None);

    let (set, prev) = shortcut_set_options(&blob, 111, "mangohud %command%").unwrap();
    assert_eq!(prev.as_deref(), Some("gamemoderun %command%"));
    assert_eq!(
        shortcut_get_options(&set, 111).as_deref(),
        Some("mangohud %command%")
    );

    // A second set reports the first set's value; the sibling entry is intact.
    let (again, prev2) = shortcut_set_options(&set, 111, "opt %command%").unwrap();
    assert_eq!(prev2.as_deref(), Some("mangohud %command%"));
    assert_eq!(
        shortcut_get_options(&again, 111).as_deref(),
        Some("opt %command%")
    );
    assert_eq!(shortcut_get_options(&again, 222), None);
}

#[test]
fn creating_a_missing_field_is_byte_preserving() {
    let blob = fixture(None);
    let opts = "gamemoderun %command%";
    let (set, prev) = shortcut_set_options(&blob, 222, opts).unwrap();
    assert_eq!(prev, None);
    assert_eq!(shortcut_get_options(&set, 222).as_deref(), Some(opts));

    // The new field lands last inside the entry, right before its closing
    // `0x08`; every other byte is the original blob.
    let split = blob.len() - 2;
    let field = b"\x01LaunchOptions\0gamemoderun %command%\0";
    assert_eq!(&set[..split], &blob[..split]);
    assert_eq!(&set[split + field.len()..], &blob[split..]);
    assert_eq!(&set[split..split + field.len()], field);

    // Key-absent restore returns the original blob byte-for-byte.
    assert_eq!(shortcut_remove_options(&set, 222).unwrap(), blob);
}

#[test]
fn restoring_a_previous_value_is_byte_identical() {
    let blob = fixture(Some("gamemoderun %command%"));
    let (set, prev) = shortcut_set_options(&blob, 111, "wrapped-longer %command%").unwrap();
    let back = shortcut_set_options(&set, 111, prev.as_deref().unwrap())
        .unwrap()
        .0;
    assert_eq!(back, blob);
}

#[test]
fn refuses_unknown_appid_and_malformed_blobs() {
    let blob = fixture(None);
    // Steam owns shortcut ids: an unknown one is an error, never a new entry.
    assert!(shortcut_set_options(&blob, 999, "x %command%").is_err());

    // Truncated mid-object, and a mutated kind byte: both name the file.
    assert!(shortcut_set_options(&blob[..blob.len() - 4], 111, "x %command%").is_err());
    let mut bad = blob.clone();
    let at = bad.iter().position(|b| *b == 2).unwrap();
    bad[at] = 7;
    let err = shortcut_set_options(&bad, 111, "x %command%").unwrap_err();
    assert!(
        format!("{err}").contains("shortcuts.vdf"),
        "malformed input names the file: {err}"
    );
    assert!(shortcut_remove_options(&bad, 111).is_err());
}

#[test]
fn apply_composes_once_and_restore_returns_bytes() {
    let dir = scratch("apply");
    let blob = fixture(Some("gamemoderun %command%"));
    let files: Vec<PathBuf> = ["42", "7"]
        .iter()
        .map(|u| {
            let p = dir.join(format!("userdata/{u}/config/shortcuts.vdf"));
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(&p, &blob).unwrap();
            p
        })
        .collect();
    let launcher = dir.join("launch.sh");
    std::fs::write(&launcher, "#!/bin/sh\n").unwrap();
    let ctx = ctx(&dir.join("data"), &launcher);
    let frag = quote_fragment(&launcher);
    let game_id = "steam:standalone:111";

    apply_files_shortcut(&ctx, game_id, 111, &files).unwrap();
    let expected = compose_options(Some("gamemoderun %command%"), &frag);
    for f in &files {
        let bytes = std::fs::read(f).unwrap();
        assert_eq!(
            shortcut_get_options(&bytes, 111).as_deref(),
            Some(expected.as_str())
        );
        // Multi-user: every user's file written, sibling entry untouched.
        assert_eq!(shortcut_get_options(&bytes, 222), None);
    }

    // Re-apply is an idempotent no-op: one fragment, not two.
    let msg = apply_files_shortcut(&ctx, game_id, 111, &files).unwrap();
    assert!(msg.contains("already applied"), "{msg}");
    for f in &files {
        let value = shortcut_get_options(&std::fs::read(f).unwrap(), 111).unwrap();
        assert_eq!(value.matches(&frag).count(), 1, "{value}");
    }

    restore_files_shortcut(&ctx, game_id, 111).unwrap();
    for f in &files {
        assert_eq!(std::fs::read(f).unwrap(), blob, "restore is byte-identical");
    }
    // The record is dropped, so a second restore has nothing to replay.
    assert!(restore_files_shortcut(&ctx, game_id, 111).is_err());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn apply_refuses_a_shortcut_the_file_lacks() {
    let dir = scratch("missing-entry");
    let blob = fixture(None);
    let file = dir.join("userdata/42/config/shortcuts.vdf");
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    std::fs::write(&file, &blob).unwrap();
    let ctx = ctx(&dir.join("data"), &dir.join("launch.sh"));

    assert!(apply_files_shortcut(&ctx, "steam:standalone:404", 404, &[file.clone()]).is_err());
    assert_eq!(
        std::fs::read(&file).unwrap(),
        blob,
        "refused write leaves bytes"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// A user whose account never created this game: a valid blob whose entries
/// all carry other appids.
fn other_user_blob() -> Vec<u8> {
    let mut out = bin_obj("shortcuts");
    out.extend(entry("0", 999, "Some Other Game", None));
    out.push(8);
    out
}

#[test]
fn apply_skips_a_user_whose_file_lacks_the_shortcut() {
    let dir = scratch("multi-user");
    let mine = fixture(None);
    let theirs = other_user_blob();
    let first = dir.join("userdata/42/config/shortcuts.vdf");
    let second = dir.join("userdata/7/config/shortcuts.vdf");
    for (p, blob) in [(&first, &mine), (&second, &theirs)] {
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, blob).unwrap();
    }
    let launcher = dir.join("launch.sh");
    std::fs::write(&launcher, "#!/bin/sh\n").unwrap();
    let ctx = ctx(&dir.join("data"), &launcher);
    let frag = quote_fragment(&launcher);
    let game_id = "steam:standalone:111";
    let files = vec![first.clone(), second.clone()];

    // The P1 regression: a second account without the appid must not abort
    // Apply after the first account's file was already rewritten.
    let msg = apply_files_shortcut(&ctx, game_id, 111, &files).unwrap();
    assert!(msg.contains("applied"), "{msg}");

    let expected = compose_options(None, &frag);
    let got = std::fs::read(&first).unwrap();
    assert_eq!(
        shortcut_get_options(&got, 111).as_deref(),
        Some(expected.as_str())
    );
    assert_eq!(
        shortcut_get_options(&got, 222),
        None,
        "sibling entry intact"
    );
    // The other user's file is byte-identical: skipped, never rewritten.
    assert_eq!(std::fs::read(&second).unwrap(), theirs);

    // Restore replays only the file Apply actually wrote, and returns it to
    // the original bytes.
    restore_files_shortcut(&ctx, game_id, 111).unwrap();
    assert_eq!(
        std::fs::read(&first).unwrap(),
        mine,
        "restore is byte-identical"
    );
    assert_eq!(std::fs::read(&second).unwrap(), theirs);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn apply_refuses_when_no_user_has_the_shortcut() {
    let dir = scratch("missing-everywhere");
    let blob = other_user_blob();
    let files: Vec<PathBuf> = ["42", "7"]
        .iter()
        .map(|u| {
            let p = dir.join(format!("userdata/{u}/config/shortcuts.vdf"));
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(&p, &blob).unwrap();
            p
        })
        .collect();
    let ctx = ctx(&dir.join("data"), &dir.join("launch.sh"));

    // A miss on every file stays loud rather than a silent no-op.
    let err = apply_files_shortcut(&ctx, "steam:standalone:111", 111, &files).unwrap_err();
    assert!(
        format!("{err}").contains("no shortcuts.vdf has shortcut 111"),
        "{err}"
    );
    for f in &files {
        assert_eq!(
            std::fs::read(f).unwrap(),
            blob,
            "refused apply leaves bytes"
        );
    }
    // Nothing was recorded, so there is nothing to restore.
    assert!(restore_files_shortcut(&ctx, "steam:standalone:111", 111).is_err());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn apply_refuses_a_malformed_file_before_writing_anything() {
    let dir = scratch("malformed-mixed");
    let good = fixture(None);
    let first = dir.join("userdata/42/config/shortcuts.vdf");
    let second = dir.join("userdata/7/config/shortcuts.vdf");
    std::fs::create_dir_all(first.parent().unwrap()).unwrap();
    std::fs::create_dir_all(second.parent().unwrap()).unwrap();
    std::fs::write(&first, &good).unwrap();
    // Truncated mid-entry: a corrupt file is not a user to skip quietly.
    std::fs::write(&second, &good[..good.len() - 4]).unwrap();
    let ctx = ctx(&dir.join("data"), &dir.join("launch.sh"));

    assert!(apply_files_shortcut(
        &ctx,
        "steam:standalone:111",
        111,
        &[first.clone(), second.clone()]
    )
    .is_err());
    assert_eq!(
        std::fs::read(&first).unwrap(),
        good,
        "no write before the refusal"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn shortcut_files_covers_every_user() {
    let dir = scratch("files");
    let mut want = Vec::new();
    for user in ["7", "42"] {
        let p = dir.join(format!("userdata/{user}/config/shortcuts.vdf"));
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(&p, fixture(None)).unwrap();
        want.push(p);
    }
    // A user with no shortcuts file is skipped, not invented.
    std::fs::create_dir_all(dir.join("userdata/9/config")).unwrap();
    want.sort();
    assert_eq!(shortcut_files(&[dir.clone()]), want);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn non_numeric_shortcut_ids_are_refused() {
    assert!(shortcut_appid("111").is_ok());
    assert!(shortcut_appid("not-an-appid").is_err());
}
