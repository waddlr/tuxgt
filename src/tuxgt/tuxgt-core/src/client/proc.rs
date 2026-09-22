use std::path::Path;

use super::StoreClient;

/// Which matched processes a caller wants.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Role {
    /// Config-holding process: native binary, or the Electron browser
    /// process (no `--type=`).
    Main,
    /// Chromium/Electron helper (`--type=renderer|gpu-process|zygote|…`).
    /// The app path is in argv, so a whole-cmdline token hits these too.
    Helper,
}

/// Pids of our own processes matching `role`. Other users' processes never
/// match: we neither report on them nor signal them. (Running tuxgt as
/// another user than the client is unsupported — the scan then finds
/// nothing.)
pub(super) fn pids(client: StoreClient, role: Role) -> Vec<u32> {
    use std::os::unix::fs::MetadataExt as _;
    let (name, extra) = client.match_tokens();
    let me = unsafe { libc::getuid() };
    let Ok(entries) = std::fs::read_dir(Path::new("/proc")) else {
        return Vec::new();
    };
    entries
        .filter_map(|e| {
            let e = e.ok()?;
            let pid: u32 = e.file_name().to_str()?.parse().ok()?;
            if e.metadata().map(|m| m.uid()).unwrap_or(u32::MAX) != me {
                return None;
            }
            classify(pid, name, extra)
                .filter(|r| *r == role)
                .map(|_| pid)
        })
        .collect()
}

/// Basename match with a post-prefix boundary: exact, or the name followed
/// by `-`, `_`, or `.`.
fn head_matches(base: &str, lower: &str) -> bool {
    base == lower
        || base
            .strip_prefix(lower)
            .is_some_and(|rest| rest.starts_with(['-', '_', '.']))
}

/// Chromium/Electron helper argv. The browser process is `electron app.asar`;
/// helpers are `electron --type=renderer --app-path=…/app.asar` (and gpu,
/// zygote, utility). SIGTERM on a renderer drops the window while the main
/// process keeps the store file.
pub(super) fn is_electron_helper(cmd: &[u8]) -> bool {
    cmd.split(|b| *b == 0)
        .any(|arg| arg.starts_with(b"--type="))
}

/// True when the pid is the client main process. Exact comm covers native
/// and the Flatpak leaf; an exe/argv0 basename matching per `head_matches`
/// covers AppImage (`AppRun` backed by a `Heroic-*.AppImage` exe). The
/// `extra` tokens are distinctive install paths/ids (`heroic-games-launcher`,
/// Flatpak app ids) matched against the whole cmdline, because for Electron
/// and Flatpak layouts argv0 is the runtime (`electron`, `flatpak`) and the
/// identity lives in later args. Those tokens are specific enough that a
/// stray collision is a transient `grep`: the scan is point-in-time, every
/// signaled pid is re-checked first (the vanished are ESRCH-skipped), and
/// signaling only ever happens after the user's explicit Confirm. The
/// boundary keeps same-prefix non-clients (`steamcmd`, `steamtinkerlaunch`)
/// out; a same-name collision stops only after Confirm. Steam children
/// (`steamwebhelper`) are deliberately unmatched: the `steam -shutdown` path
/// owns them. Electron helpers are `Role::Helper`, not main.
pub(super) fn classify(pid: u32, name: &str, extra: &[&str]) -> Option<Role> {
    if is_zombie(pid) {
        return None;
    }
    let base = Path::new("/proc").join(pid.to_string());
    let cmd = std::fs::read(base.join("cmdline")).unwrap_or_default();
    if !identity_match(pid, name, extra, &base, &cmd) {
        return None;
    }
    if is_electron_helper(&cmd) {
        Some(Role::Helper)
    } else {
        Some(Role::Main)
    }
}

fn identity_match(pid: u32, name: &str, extra: &[&str], base: &Path, cmd: &[u8]) -> bool {
    if comm_of(pid).as_deref() == Some(name) {
        return true;
    }
    let lower = name.to_ascii_lowercase();
    if let Ok(exe) = std::fs::read_link(base.join("exe")) {
        if exe
            .file_name()
            .and_then(|s| s.to_str())
            .is_some_and(|s| head_matches(&s.to_ascii_lowercase(), &lower))
        {
            return true;
        }
    }
    if let Some(argv0) = cmd.split(|b| *b == 0).next() {
        if !argv0.is_empty()
            && Path::new(&*String::from_utf8_lossy(argv0))
                .file_name()
                .and_then(|s| s.to_str())
                .is_some_and(|s| head_matches(&s.to_ascii_lowercase(), &lower))
        {
            return true;
        }
    }
    if cmd.is_empty() {
        return false;
    }
    let text = String::from_utf8_lossy(cmd).to_ascii_lowercase();
    extra.iter().any(|t| text.contains(&t.to_ascii_lowercase()))
}

/// Zombies still have a /proc entry until reaped. They hold no config.
fn is_zombie(pid: u32) -> bool {
    let Ok(stat) = std::fs::read_to_string(Path::new("/proc").join(pid.to_string()).join("stat"))
    else {
        return false;
    };
    let Some((_, rest)) = stat.rsplit_once(')') else {
        return false;
    };
    rest.split_whitespace().next() == Some("Z")
}

/// Current comm of one pid; None when it vanished.
fn comm_of(pid: u32) -> Option<String> {
    std::fs::read_to_string(Path::new("/proc").join(pid.to_string()).join("comm"))
        .ok()
        .map(|c| c.trim_end().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn own_process_legs_match_main() {
        let me = std::process::id();
        let comm = comm_of(me).expect("own comm readable");
        assert_eq!(classify(me, &comm, &[]), Some(Role::Main));
        assert_eq!(classify(me, "tuxgt_core", &[]), Some(Role::Main));
        assert_eq!(classify(me, "tuxgt-no-such-client-xyz", &[]), None);
    }

    #[test]
    fn head_match_needs_boundary() {
        assert!(head_matches("heroic", "heroic"));
        assert!(head_matches("heroic-2.16.1.appimage", "heroic"));
        assert!(head_matches("heroic_backup", "heroic"));
        assert!(!head_matches("steamcmd", "steam"));
        assert!(!head_matches("steamtinkerlaunch", "steam"));
        assert!(!head_matches("mysteamtool", "steam"));
    }

    #[test]
    fn cmdline_token_matches_later_args() {
        let cmd = std::fs::read("/proc/self/cmdline").unwrap_or_default();
        if String::from_utf8_lossy(&cmd).contains("target") {
            assert_eq!(
                classify(std::process::id(), "tuxgt-no-such-client-xyz", &["target"]),
                Some(Role::Main)
            );
        }
    }

    #[test]
    fn electron_helper_argv_is_not_main() {
        assert!(!is_electron_helper(b"electron\0/usr/lib/heroic/app.asar\0"));
        assert!(is_electron_helper(
            b"electron\0--type=renderer\0--app-path=/usr/lib/heroic-games-launcher/app.asar\0"
        ));
        assert!(is_electron_helper(b"electron\0--type=gpu-process\0"));
        assert!(!is_electron_helper(b"--typeish\0"));
    }

    #[test]
    fn helper_cmdline_classifies_as_helper_not_main() {
        // bash -c keeps extra argv in /proc/pid/cmdline ($0, $1, …).
        let mut helper = std::process::Command::new("bash")
            .arg("-c")
            .arg("sleep 30")
            .arg("--type=renderer")
            .arg("heroic-games-launcher")
            .spawn()
            .expect("bash");
        let mut main = std::process::Command::new("bash")
            .arg("-c")
            .arg("sleep 30")
            .arg("app.asar")
            .arg("heroic-games-launcher")
            .spawn()
            .expect("bash");
        let helper_pid = helper.id();
        let main_pid = main.id();
        let role_h = classify(helper_pid, "not-a-client", &["heroic-games-launcher"]);
        let role_m = classify(main_pid, "not-a-client", &["heroic-games-launcher"]);
        let _ = helper.kill();
        let _ = helper.wait();
        let _ = main.kill();
        let _ = main.wait();
        assert_eq!(role_h, Some(Role::Helper));
        assert_eq!(role_m, Some(Role::Main));
    }
}
