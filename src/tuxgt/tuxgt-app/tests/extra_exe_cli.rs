//! R55: `games extra-exe add|remove|list` writes persistence, refreshes the
//! correlator via `sync_session`, and refuses cross-game collisions.

use std::path::PathBuf;
use std::process::Command;

struct Scratch {
    dir: PathBuf,
}

impl Scratch {
    fn new(name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("tuxgt-r55-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        Self { dir }
    }

    fn cmd(&self) -> Command {
        let mut c = Command::new(env!("CARGO_BIN_EXE_tuxgt"));
        c.env("TUXGT_DATA", self.dir.join("data"))
            .env("TUXGT_CONFIG", self.dir.join("config"));
        c
    }

    fn add_game(&self, name: &str) -> String {
        let exe = self.dir.join(name);
        std::fs::copy("/bin/true", &exe).unwrap();
        let out = self
            .cmd()
            .args(["games", "add"])
            .arg(&exe)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "games add failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8(out.stdout)
            .unwrap()
            .lines()
            .filter(|l| l.contains('\t'))
            .last()
            .expect("games add row")
            .split('\t')
            .next()
            .unwrap()
            .to_string()
    }

    fn ok(&self, args: &[&str]) -> String {
        let out = self.cmd().args(args).output().unwrap();
        assert!(
            out.status.success(),
            "{args:?} failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8(out.stdout).unwrap()
    }

    fn correlator(&self) -> String {
        std::fs::read_to_string(
            self.dir
                .join("data")
                .join("games")
                .join("load-correlator.ini"),
        )
        .unwrap()
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

#[test]
fn extra_exe_add_list_remove_roundtrip() {
    let s = Scratch::new("roundtrip");
    let id = s.add_game("FakeGame");
    let pfx = s.dir.join("pfx");
    std::fs::create_dir_all(&pfx).unwrap();
    s.ok(&["doctor", &id, "--set", &format!("prefix={}", pfx.display())]);

    // Primary exe renders under the prefix section.
    let primary = s.dir.join("FakeGame").to_string_lossy().to_lowercase();
    let rel = id
        .replace(':', "/")
        .replace("manual/standalone", "manual_standalone");
    let before = s.correlator();
    assert!(before.contains(&format!("{primary}={rel}")), "{before}");

    // Add a loader-side extra: both keys map to the same rel.
    s.ok(&["games", "extra-exe", &id, "add", "C:\\Games\\Loader.EXE"]);
    let listed = s.ok(&["games", "extra-exe", &id, "list"]);
    assert!(listed.contains("c:/games/loader.exe"), "{listed}");
    let after = s.correlator();
    assert!(after.contains(&format!("{primary}={rel}")), "{after}");
    assert!(
        after.contains(&format!("c:/games/loader.exe={rel}")),
        "{after}"
    );

    // Remove drops the key but keeps the primary.
    s.ok(&["games", "extra-exe", &id, "remove", "c:/games/loader.exe"]);
    assert!(!s
        .ok(&["games", "extra-exe", &id, "list"])
        .contains("loader"));
    let removed = s.correlator();
    assert!(removed.contains(&format!("{primary}={rel}")), "{removed}");
    assert!(!removed.contains("loader"), "{removed}");
}

#[test]
fn extra_exe_collision_refuses_naming_owner() {
    let s = Scratch::new("collide");
    let a = s.add_game("GameA");
    let b = s.add_game("GameB");
    let pfx = s.dir.join("pfx");
    std::fs::create_dir_all(&pfx).unwrap();
    s.ok(&["doctor", &a, "--set", &format!("prefix={}", pfx.display())]);
    s.ok(&["doctor", &b, "--set", &format!("prefix={}", pfx.display())]);

    // B claims A's primary exe as an extra: refused, names A, writes nothing.
    let exe_a = s.dir.join("GameA").to_string_lossy().into_owned();
    let out = s
        .cmd()
        .args(["games", "extra-exe", &b, "add", &exe_a])
        .output()
        .unwrap();
    assert!(!out.status.success(), "expected refusal, got: {out:?}");
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    assert!(stderr.contains(&a), "{stderr}");
    assert!(s.ok(&["games", "extra-exe", &b, "list"]).is_empty());

    // Same via prefix realignment: B's exe parked on A's key first, then
    // moving B's prefix onto A's prefix refuses.
    let pfx_b = s.dir.join("pfx-b");
    std::fs::create_dir_all(&pfx_b).unwrap();
    s.ok(&[
        "doctor",
        &b,
        "--set",
        &format!("prefix={}", pfx_b.display()),
    ]);
    s.ok(&["doctor", &b, "--set", &format!("exe={exe_a}")]);
    let out = s
        .cmd()
        .args(["doctor", &b, "--set", &format!("prefix={}", pfx.display())])
        .output()
        .unwrap();
    assert!(!out.status.success(), "expected refusal, got: {out:?}");
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    assert!(stderr.contains(&a), "{stderr}");
}
