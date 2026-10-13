use super::*;
use crate::mods_for_game;

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
fn partial_set_is_listed_before_ready() {
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
    // Not ready is no longer hidden: the picker lists the mod and the
    // install gate (not the listing) stops it until files land.
    let listed = mods_for_game(&cfg, "Any Game", None, &data).unwrap();
    assert!(listed.mods.iter().any(|m| m.id == "nvidia-streamline"));
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

/// Not-ready no longer hides a provided mod, but disabled still does:
/// a cleared payload (which disables the mod) stays out of the picker.
#[test]
fn disabled_not_ready_stays_unlisted() {
    let (dir, data, cfg) = scratch("dis");
    crate::disable_mod(&cfg, "nvidia-streamline", &data).unwrap();
    let listed = mods_for_game(&cfg, "Any Game", None, &data).unwrap();
    assert!(listed.mods.iter().all(|m| m.id != "nvidia-streamline"));
    assert!(listed.mods.iter().any(|m| m.id == "reshade"));
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
