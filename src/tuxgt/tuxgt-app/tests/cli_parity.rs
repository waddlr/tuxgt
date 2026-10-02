//! CLI writers that match GUI mod/game management: hide override and
//! persisted adapter convert with no installed mods.

use std::path::PathBuf;
use std::process::Command;

struct Scratch {
    dir: PathBuf,
}

impl Scratch {
    fn new(name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("tuxgt-parity-{name}-{}", std::process::id()));
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

    fn add_game(&self) -> String {
        let exe = self.dir.join("game.exe");
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
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

#[test]
fn games_hide_unhide_clear_roundtrip() {
    let fx = Scratch::new("hide");
    let id = fx.add_game();
    assert!(fx
        .ok(&["games", "hide", &id])
        .contains(&format!("{id}\thidden")));
    let listed = fx.ok(&["games", "list", "--hidden"]);
    assert!(listed.contains(&id));
    assert!(fx
        .ok(&["games", "unhide", &id])
        .contains(&format!("{id}\tvisible")));
    assert!(fx
        .ok(&["games", "hide", &id, "--clear"])
        .contains(&format!("{id}\tvisible")));
}

#[test]
fn games_adapter_print_and_convert_empty() {
    let fx = Scratch::new("adapter");
    let id = fx.add_game();
    assert!(fx
        .ok(&["games", "adapter", &id])
        .contains(&format!("{id}\tpreload")));
    let converted = fx.ok(&["games", "adapter", &id, "install"]);
    assert!(converted.contains(&format!("{id}\tpreload\tinstall")));
    assert!(fx
        .ok(&["games", "adapter", &id])
        .contains(&format!("{id}\tinstall")));
    let same = fx.ok(&["games", "adapter", &id, "install"]);
    assert!(
        same.contains(&format!("{id}\tinstall")),
        "no-op convert should print current: {same:?}"
    );
    assert!(
        !same.contains("preload"),
        "no-op convert must not report a conversion: {same:?}"
    );
}

#[test]
fn instance_check_empty_game_succeeds() {
    let fx = Scratch::new("check");
    let id = fx.add_game();
    let out = fx.ok(&["instance", "check", &id]);
    assert!(out.is_empty(), "expected no lines, got {out:?}");
}

#[test]
fn instance_update_empty_game_succeeds() {
    let fx = Scratch::new("update");
    let id = fx.add_game();
    let out = fx.ok(&["instance", "update", &id, "--yes"]);
    assert!(out.is_empty(), "expected no lines, got {out:?}");
}
