use super::testing::*;
use super::*;
use crate::testing::{seed_game, SeedGame};
use crate::{open_db, Error};
use std::path::PathBuf;

#[test]
fn unreal_shipping_and_unity_data() {
    let root = std::env::temp_dir().join(format!("tuxgt-det-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let ue = root.join("ue");
    write_file(
        &ue.join("Binaries/Win64/Foo-Win64-Shipping.exe"),
        b"not-a-pe",
    );
    write_file(
        &ue.join("Engine/Build/Build.version"),
        br#"{"MajorVersion":5,"MinorVersion":4,"PatchVersion":2,"Changelist":1}"#,
    );
    let id = GameId::parse("manual:standalone:abcdefgh").unwrap();
    let out = run_detectors(&id, Some(&ue), Detected::default());
    assert_eq!(out.engine.as_deref(), Some("unreal"));
    assert!(out
        .exe_path
        .as_ref()
        .unwrap()
        .ends_with("Foo-Win64-Shipping.exe"));
    assert_eq!(out.build.as_deref(), Some("5.4.2-1"));

    let un = root.join("unity");
    write_file(&un.join("Bar_Data/level0"), b"x");
    write_file(&un.join("Bar.exe"), b"x");
    let out = run_detectors(&id, Some(&un), Detected::default());
    assert_eq!(out.engine.as_deref(), Some("unity"));
    assert!(out.exe_path.as_ref().unwrap().ends_with("Bar.exe"));

    let re = root.join("re");
    write_file(&re.join("re_chunk_000.pak"), b"x");
    write_file(&re.join("re8.exe"), b"x");
    let out = run_detectors(&id, Some(&re), Detected::default());
    assert_eq!(out.engine.as_deref(), Some("re_engine"));

    let ue12 = root.join("ue12");
    write_file(
        &ue12.join("Binaries/Win64/Foo-Win64-Shipping.exe"),
        b"not-a-pe",
    );
    write_file(&ue12.join("Binaries/Win64/D3D12/D3D12Core.dll"), b"x");
    write_file(
        &ue12.join("Engine/Build/Build.version"),
        br#"{"MajorVersion":5,"MinorVersion":1,"PatchVersion":0}"#,
    );
    let out = run_detectors(&id, Some(&ue12), Detected::default());
    assert_eq!(out.engine.as_deref(), Some("unreal"));
    assert_eq!(out.api.as_deref(), Some("dx12"));
    assert_eq!(out.extra_apis.as_deref(), Some("dx11"));

    let ce = root.join("fo4");
    write_file(&ce.join("Fallout4.exe"), b"x");
    write_file(&ce.join("Data/Fallout4.esm"), b"x");
    write_file(&ce.join("Fallout4Launcher.exe"), b"yyyyyyyy");
    let out = run_detectors(&id, Some(&ce), Detected::default());
    assert_eq!(out.engine.as_deref(), Some("creation"));
    assert!(out.exe_path.as_ref().unwrap().ends_with("Fallout4.exe"));

    let bs = root.join("cd");
    write_file(&bs.join("bin64/cdt.dll"), b"x");
    write_file(&bs.join("bin64/cgraph.dll"), b"x");
    write_file(&bs.join("bin64/CrimsonDesert.exe"), b"x");
    let out = run_detectors(&id, Some(&bs), Detected::default());
    assert_eq!(out.engine.as_deref(), Some("blackspace"));
    assert!(out
        .exe_path
        .as_ref()
        .unwrap()
        .ends_with("CrimsonDesert.exe"));

    let mo = root.join("mo2");
    write_file(&mo.join("ModOrganizer.exe"), b"mo2");
    let stock = mo.join("Stock Game");
    write_file(&stock.join("SkyrimSE.exe"), b"game");
    write_file(&stock.join("skse64_loader.exe"), b"skse");
    write_file(
        &mo.join("ModOrganizer.ini"),
        format!(
            "[General]\ngameName=Skyrim Special Edition\ngamePath={}\n1\\binary={}\n",
            stock.display(),
            stock.join("skse64_loader.exe").display()
        )
        .as_bytes(),
    );
    let out = run_detectors(&id, Some(&mo), Detected::default());
    assert_eq!(out.engine.as_deref(), Some("creation"));
    assert!(out.exe_path.as_ref().unwrap().ends_with("SkyrimSE.exe"));

    let elfdir = root.join("bg3");
    let true_bin = PathBuf::from("/usr/bin/true");
    if true_bin.is_file() {
        std::fs::create_dir_all(elfdir.join("bin")).unwrap();
        std::fs::copy(&true_bin, elfdir.join("bin/bg3")).unwrap();
        let out = run_detectors(&id, Some(&elfdir), Detected::default());
        assert!(out.exe_path.as_ref().unwrap().ends_with("bg3"));
        assert_eq!(out.platform.as_deref(), Some("native"));
        assert_eq!(out.bitness.as_deref(), Some("64"));
    }
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn elf_native_from_true() {
    let true_bin = PathBuf::from("/usr/bin/true");
    if !true_bin.is_file() {
        return;
    }
    let id = GameId::parse("manual:standalone:elftest1").unwrap();
    let mut seed = Detected::default();
    seed.exe_path = Some(true_bin);
    let out = run_detectors(&id, None, seed);
    assert_eq!(out.platform.as_deref(), Some("native"));
    assert_eq!(out.bitness.as_deref(), Some("64"));
}

#[test]
fn merge_engine_first_wins() {
    let mut d = Detected::default();
    d.set_engine("unreal", "unreal");
    d.set_engine("unity", "unity");
    assert_eq!(d.engine.as_deref(), Some("unreal"));
}

#[test]
fn validate_override_rules() {
    assert!(validate_override("platform", Some("wine")).is_ok());
    assert!(validate_override("bitness", Some("64")).is_ok());
    assert!(validate_override("platform", None).is_ok());
    assert!(validate_override("bitness", Some("")).is_ok());
    for (field, value) in [
        ("bitness", Some("7")),
        ("platform", Some("mac")),
        ("nope", Some("x")),
    ] {
        assert!(
            matches!(
                validate_override(field, value),
                Err(Error::InvalidOverride(_))
            ),
            "{field}={value:?}"
        );
    }
}

#[tokio::test]
async fn override_wins_and_force_clears() {
    let dir = std::env::temp_dir().join(format!("tuxgt-det-db-{}", std::process::id()));
    let _ = tokio::fs::remove_dir_all(&dir).await;
    let pool = open_db(&dir).await.unwrap();
    seed_game(
        &pool,
        SeedGame {
            id: "manual:standalone:aaaaaaaa",
            manager: "manual",
            store: "standalone",
            game_id: "aaaaaaaa",
            name: Some("t"),
            override_api: Some("dx12"),
            ..Default::default()
        },
    )
    .await;
    let rep = doctor(
        &pool,
        "manual:standalone:aaaaaaaa",
        DetectOpts {
            force: false,
            yes: true,
        },
    )
    .await
    .unwrap();
    let api = rep.fields.iter().find(|f| f.key == "api").unwrap();
    assert_eq!(api.value, "dx12");
    assert_eq!(api.source, Some("override"));
    let _ = doctor(
        &pool,
        "manual:standalone:aaaaaaaa",
        DetectOpts {
            force: true,
            yes: true,
        },
    )
    .await
    .unwrap();
    let row = fetch_row(&pool, "manual:standalone:aaaaaaaa")
        .await
        .unwrap()
        .unwrap();
    assert!(row.override_api.is_none());
    let _ = tokio::fs::remove_dir_all(&dir).await;
}

#[test]
fn vdf_compat_and_launch() {
    let dir = std::env::temp_dir().join(format!("tuxgt-vdf-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    write_file(
        &dir.join("config.vdf"),
        r#"
"InstallConfigStore"
{
  "Software"
  {
    "Valve"
    {
      "Steam"
      {
        "CompatToolMapping"
        {
          "814380"
          {
            "name" "proton_experimental"
          }
        }
      }
    }
  }
}
"#
        .as_bytes(),
    );
    write_file(
        &dir.join("localconfig.vdf"),
        r#"
"UserLocalConfigStore"
{
  "Software"
  {
    "Valve"
    {
      "Steam"
      {
        "apps"
        {
          "814380"
          {
            "LaunchOptions" "mangohud %command%"
          }
        }
      }
    }
  }
}
"#
        .as_bytes(),
    );
    assert_eq!(
        super::runtime::vdf_compat_name(dir.join("config.vdf").as_path(), "814380").as_deref(),
        Some("proton_experimental")
    );
    assert_eq!(
        super::runtime::vdf_launch_options(dir.join("localconfig.vdf").as_path(), "814380")
            .as_deref(),
        Some("mangohud %command%")
    );
    let _ = std::fs::remove_dir_all(&dir);
}
