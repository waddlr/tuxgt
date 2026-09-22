//! E40: `doctor --set`/`--unset` is all-or-nothing — a bad value anywhere in
//! the argv must leave existing overrides untouched.

use std::path::PathBuf;
use std::process::Command;

struct Scratch {
    dir: PathBuf,
}

impl Scratch {
    fn new(name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("tuxgt-e40-{name}-{}", std::process::id()));
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
        let exe = self.dir.join("FakeGame");
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

    fn doctor(&self, id: &str) -> String {
        let out = self.cmd().args(["doctor", id]).output().unwrap();
        assert!(
            out.status.success(),
            "doctor failed: {}",
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

fn doctor_args(id: &str, extra: &[&str]) -> Vec<String> {
    let mut v = vec!["doctor".to_string(), id.to_string()];
    v.extend(extra.iter().map(|s| s.to_string()));
    v
}

#[test]
fn bad_value_in_any_op_writes_nothing() {
    let s = Scratch::new("atomic");
    let id = s.add_game();

    // Known-good override first.
    let out = s
        .cmd()
        .args(doctor_args(&id, &["--set", "api=dx11"]))
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let before = s.doctor(&id);
    assert!(before.contains("api\tdx11\toverride"), "{before}");

    // Valid first op, invalid second: the whole argv fails.
    let out = s
        .cmd()
        .args(doctor_args(
            &id,
            &["--set", "api=dx12", "--set", "bitness=7"],
        ))
        .output()
        .unwrap();
    assert!(!out.status.success(), "expected failure, got: {out:?}");
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("bad bitness: 7"),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );

    // Nothing was written: the earlier override is unchanged.
    let after = s.doctor(&id);
    assert_eq!(after, before, "override changed despite failed argv");
    assert!(!after.contains("dx12"), "{after}");
}

#[test]
fn set_and_unset_roundtrip_through_the_cli() {
    let s = Scratch::new("roundtrip");
    let id = s.add_game();

    let out = s
        .cmd()
        .args(doctor_args(&id, &["--set", "api=dx11"]))
        .output()
        .unwrap();
    assert!(out.status.success());
    assert!(s.doctor(&id).contains("api\tdx11\toverride"));

    // Empty value clears; --unset clears too; --force with either is refused.
    let out = s
        .cmd()
        .args(doctor_args(&id, &["--set", "api="]))
        .output()
        .unwrap();
    assert!(out.status.success());
    assert!(
        !s.doctor(&id).contains("api\tdx11"),
        "empty value did not clear"
    );

    let out = s
        .cmd()
        .args(doctor_args(&id, &["--set", "platform=proton"]))
        .output()
        .unwrap();
    assert!(out.status.success());
    let out = s
        .cmd()
        .args(doctor_args(&id, &["--unset", "platform"]))
        .output()
        .unwrap();
    assert!(out.status.success());
    assert!(s.doctor(&id).contains("platform\tnative\tdetected"));

    let out = s
        .cmd()
        .args(doctor_args(&id, &["--force", "--set", "api=dx11"]))
        .output()
        .unwrap();
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("--force"));
}
