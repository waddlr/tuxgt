use super::*;

#[test]
fn skips_tools() {
    assert!(skip_steam_app(228980, "Anything"));
    assert!(skip_steam_app(1, "Proton 9.0"));
    assert!(skip_steam_app(1, "Steam Linux Runtime 3.0 (sniper)"));
    assert!(skip_steam_app(1, "Steamworks Common Redistributables"));
    assert!(!skip_steam_app(814380, "Sekiro"));
    assert!(skip_steam_app(250820, "SteamVR"));
    assert!(skip_steam_app(202480, "Skyrim Creation Kit"));
}

#[test]
fn art_paths_need_files() {
    let dir = std::env::temp_dir().join(format!("tuxgt-steam-art-{}", std::process::id()));
    let cache = dir.join("appcache").join("librarycache").join("814380");
    std::fs::create_dir_all(&cache).unwrap();
    let cover = cache.join("library_600x900.jpg");
    std::fs::write(&cover, b"x").unwrap();
    let (c, h) = steam_art(&dir, 814380);
    assert_eq!(c.as_deref(), Some(cover.as_path()));
    assert!(h.is_none());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn header_prefers_library_hero() {
    let dir = std::env::temp_dir().join(format!("tuxgt-steam-hero-{}", std::process::id()));
    let cache = dir.join("appcache").join("librarycache").join("814380");
    std::fs::create_dir_all(&cache).unwrap();
    let header = cache.join("library_header.jpg");
    let hero = cache.join("library_hero.jpg");
    std::fs::write(&header, b"h").unwrap();
    std::fs::write(&hero, b"w").unwrap();
    let (_, h) = steam_art(&dir, 814380);
    assert_eq!(h.as_deref(), Some(hero.as_path()));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn art_paths_nested_hash_dir() {
    let dir = std::env::temp_dir().join(format!("tuxgt-steam-art-hash-{}", std::process::id()));
    let cache = dir.join("appcache").join("librarycache").join("1903340");
    let nested = cache.join("8b21381a43ac5a535a838f723815f8fe14ceaf7c");
    std::fs::create_dir_all(&nested).unwrap();
    let cover = nested.join("library_600x900.jpg");
    std::fs::write(&cover, b"x").unwrap();
    let (c, h) = steam_art(&dir, 1903340);
    assert_eq!(c.as_deref(), Some(cover.as_path()));
    assert!(h.is_none());
    let _ = std::fs::remove_dir_all(&dir);
}
const LOCALCONFIG: &str = "\"UserLocalConfigStore\"\n{\n\t\"Software\"\n\t{\n\t\t\"Valve\"\n\t\t{\n\t\t\t\"Steam\"\n\t\t\t{\n\t\t\t\t\"apps\"\n\t\t\t\t{\n\t\t\t\t\t\"814380\"\n\t\t\t\t\t{\n\t\t\t\t\t\t\"LaunchOptions\"\t\t\"gamemoderun %command%\"\n\t\t\t\t\t}\n\t\t\t\t\t\"123\"\n\t\t\t\t\t{\n\t\t\t\t\t}\n\t\t\t\t}\n\t\t\t}\n\t\t}\n\t}\n}\n";

#[test]
fn reads_existing_options() {
    assert_eq!(
        steam_get_options(LOCALCONFIG, "814380").as_deref(),
        Some("gamemoderun %command%")
    );
    assert_eq!(steam_get_options(LOCALCONFIG, "123"), None);
    assert_eq!(steam_get_options(LOCALCONFIG, "999"), None);
}

#[test]
fn composes_before_command() {
    let (text, prev) = steam_set_options(
        LOCALCONFIG,
        "814380",
        "gamemoderun /tmp/tuxgt-launcher %command%",
    )
    .unwrap();
    assert_eq!(prev.as_deref(), Some("gamemoderun %command%"));
    assert_eq!(
        steam_get_options(&text, "814380").as_deref(),
        Some("gamemoderun /tmp/tuxgt-launcher %command%")
    );
    // Untouched entries survive byte-for-byte.
    assert!(text.contains("\"123\"\n\t\t\t\t\t{\n\t\t\t\t\t}"));
}

#[test]
fn creates_missing_app_block() {
    let (text, prev) =
        steam_set_options(LOCALCONFIG, "999", "/tmp/tuxgt-launcher %command%").unwrap();
    assert_eq!(prev, None);
    assert_eq!(
        steam_get_options(&text, "999").as_deref(),
        Some("/tmp/tuxgt-launcher %command%")
    );
    assert!(steam_get_options(&text, "814380").is_some());
}

#[test]
fn escapes_quotes_round_trip() {
    let (text, _) = steam_set_options(
        LOCALCONFIG,
        "123",
        "FOO=\"a b\" /tmp/tuxgt-launcher %command%",
    )
    .unwrap();
    assert_eq!(
        steam_get_options(&text, "123").as_deref(),
        Some("FOO=\"a b\" /tmp/tuxgt-launcher %command%")
    );
}

#[test]
fn removes_key_absent_restore() {
    let (text, _) = steam_set_options(LOCALCONFIG, "999", "/tmp/tuxgt-launcher %command%").unwrap();
    let back = steam_remove_options(&text, "999");
    assert_eq!(steam_get_options(&back, "999"), None);
    assert!(steam_get_options(&back, "814380").is_some());
}

#[test]
fn rejects_missing_apps_section() {
    assert!(steam_set_options("\"root\"\n{\n}\n", "1", "x %command%").is_err());
}

fn vdf_int(field: &str, value: u32) -> Vec<u8> {
    let mut out = vec![2u8];
    out.extend_from_slice(field.as_bytes());
    out.push(0);
    out.extend_from_slice(&value.to_le_bytes());
    out
}

#[test]
fn shortcut_ishidden_parsed_from_userdata() {
    let root = std::env::temp_dir().join(format!("tuxgt-steam-hidden-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let cfg = root.join("userdata").join("42").join("config");
    std::fs::create_dir_all(&cfg).unwrap();
    let mut vdf = Vec::new();
    vdf.extend(vdf_int("appid", 111));
    vdf.extend(vdf_int("IsHidden", 1));
    vdf.extend(vdf_int("appid", 222));
    vdf.extend(vdf_int("IsHidden", 0));
    std::fs::write(cfg.join("shortcuts.vdf"), &vdf).unwrap();
    let map = load_shortcut_hidden(&[root.clone()]);
    assert_eq!(map.get(&111), Some(&true));
    assert_eq!(map.get(&222), Some(&false));
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn owned_hidden_from_localconfig_and_sharedconfig() {
    let root = std::env::temp_dir().join(format!("tuxgt-steam-owned-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let cfg = root.join("userdata").join("42").join("config");
    let shared = root.join("userdata").join("42").join("7").join("remote");
    std::fs::create_dir_all(&cfg).unwrap();
    std::fs::create_dir_all(&shared).unwrap();
    std::fs::write(
            cfg.join("localconfig.vdf"),
            concat!(
                "\"UserLocalConfigStore\"\n{\n",
                "\t\"Software\"\n\t{\n\t\t\"Valve\"\n\t\t{\n\t\t\t\"Steam\"\n\t\t\t{\n",
                "\t\t\t\t\"apps\"\n\t\t\t\t{\n",
                "\t\t\t\t\t\"111\"\n\t\t\t\t\t{\n\t\t\t\t\t\t\"hidden\"\t\t\"1\"\n\t\t\t\t\t}\n",
                "\t\t\t\t\t\"222\"\n\t\t\t\t\t{\n\t\t\t\t\t\t\"hidden\"\t\t\"0\"\n\t\t\t\t\t}\n",
                "\t\t\t\t\t\"333\"\n\t\t\t\t\t{\n\t\t\t\t\t\t\"tags\"\n\t\t\t\t\t\t{\n\t\t\t\t\t\t\t\"0\"\t\t\"hidden\"\n\t\t\t\t\t\t}\n\t\t\t\t\t}\n",
                "\t\t\t\t\t\"444\"\n\t\t\t\t\t{\n\t\t\t\t\t\t\"tags\"\n\t\t\t\t\t\t{\n\t\t\t\t\t\t\t\"0\"\t\t\"Hidden Object\"\n\t\t\t\t\t\t}\n\t\t\t\t\t}\n",
                "\t\t\t\t}\n\t\t\t}\n\t\t}\n\t}\n}\n",
            ),
        )
        .unwrap();
    std::fs::write(
        shared.join("sharedconfig.vdf"),
        concat!(
            "\"UserRoamingConfigStore\"\n{\n",
            "\t\"Software\"\n\t{\n\t\t\"Valve\"\n\t\t{\n\t\t\t\"Steam\"\n\t\t\t{\n",
            "\t\t\t\t\"apps\"\n\t\t\t\t{\n",
            "\t\t\t\t\t\"555\"\n\t\t\t\t\t{\n\t\t\t\t\t\t\"hidden\"\t\t\"1\"\n\t\t\t\t\t}\n",
            "\t\t\t\t}\n\t\t\t}\n\t\t}\n\t}\n}\n",
        ),
    )
    .unwrap();
    let set = load_owned_hidden(&[root.clone()]);
    assert!(set.contains("111"), "hidden key hides: {set:?}");
    assert!(!set.contains("222"), "hidden 0 stays visible: {set:?}");
    assert!(set.contains("333"), "tags entry hides: {set:?}");
    assert!(
        !set.contains("444"),
        "store tag is not a hidden marker: {set:?}"
    );
    assert!(set.contains("555"), "sharedconfig.vdf counts: {set:?}");
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn ucollections_hidden_added_minus_removed() {
    let root =
        std::env::temp_dir().join(format!("tuxgt-steam-ucollections-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let cfg = root.join("userdata").join("42").join("config");
    std::fs::create_dir_all(&cfg).unwrap();
    std::fs::write(
            cfg.join("localconfig.vdf"),
            concat!(
                "\"UserLocalConfigStore\"\n{\n",
                "\t\"user-collections\"\t\t\"{\\\"hidden\\\":{\\\"id\\\":\\\"hidden\\\",\\\"added\\\":[111, 222, 3841736097, \\\"333\\\"],\\\"removed\\\":[222]}}\"\n",
                "}\n",
            ),
        )
        .unwrap();
    let set = load_ucollections_hidden(&[root.clone()]);
    assert!(set.contains(&111), "added stays: {set:?}");
    assert!(!set.contains(&222), "removed drops: {set:?}");
    assert!(set.contains(&333), "numeric string counts: {set:?}");
    assert!(set.contains(&3841736097), "shortcut-range appid: {set:?}");
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn ucollections_hidden_absent_or_malformed_is_empty() {
    assert!(collect_ucollections_hidden("\"UserLocalConfigStore\"\n{\n}\n").is_empty());
    assert!(collect_ucollections_hidden("\"user-collections\"\t\t\"not json\"\n").is_empty());
    assert!(
        collect_ucollections_hidden("\"user-collections\"\t\t\"{\\\"other\\\":{}}\"\n").is_empty()
    );
    assert!(collect_ucollections_hidden(
        "\"user-collections\"\t\t\"{\\\"hidden\\\":{\\\"added\\\":[1.5, -2, null]}}\"\n"
    )
    .is_empty());
    assert!(collect_ucollections_hidden("\"user-collections\"\t\t\"{\\\"hidden\\\":{\\\"added\\\":[5000000000, \\\"99999999999\\\"],\\\"removed\\\":\\\"nope\\\"}}\"\n").is_empty());
    assert!(collect_ucollections_hidden(
        "\"user-collections\"\t\t\"{\\\"hidden\\\":{\\\"added\\\":{\\\"a\\\":1}}}\"\n"
    )
    .is_empty());
}
