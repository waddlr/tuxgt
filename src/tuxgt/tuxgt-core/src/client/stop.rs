use std::time::{Duration, Instant};

use super::proc::{self, Role};
use super::{StoreClient, HEROIC_TERM_GRACE, STOP_POLL, STOP_TIMEOUT};
use crate::{Error, Result};

pub(super) fn stop_and_wait(client: StoreClient) -> Result<()> {
    match client {
        StoreClient::Steam => {
            let steam = crate::launch::find_steam().ok_or_else(|| {
                Error::Apply("steam binary not found; cannot stop the client".into())
            })?;
            std::process::Command::new(&steam)
                .arg("-shutdown")
                .status()
                .map_err(Error::Io)?;
        }
        StoreClient::Heroic => signal(client, Role::Main, libc::SIGTERM)?,
    }
    let escalate_after = match client {
        StoreClient::Heroic => Some(HEROIC_TERM_GRACE),
        StoreClient::Steam => None,
    };
    let gone = poll_until_stopped(
        || client.running(),
        || {
            signal(client, Role::Main, libc::SIGKILL)?;
            signal(client, Role::Helper, libc::SIGKILL)
        },
        escalate_after,
        STOP_TIMEOUT,
        STOP_POLL,
    )?;
    if gone {
        Ok(())
    } else {
        Err(Error::Apply(format!(
            "{} did not exit within {}s; launch options not written",
            client.name(),
            STOP_TIMEOUT.as_secs()
        )))
    }
}

/// Signal matched pids of `role`. Each pid is re-checked immediately before
/// the signal: it may have exited and been reused since the scan. ESRCH
/// means it already exited.
fn signal(client: StoreClient, role: Role, sig: i32) -> Result<()> {
    let (name, extra) = client.match_tokens();
    for pid in proc::pids(client, role) {
        if proc::classify(pid, name, extra) != Some(role) {
            continue;
        }
        let r = unsafe { libc::kill(pid as libc::pid_t, sig) };
        if r != 0 {
            let e = std::io::Error::last_os_error();
            if e.raw_os_error() != Some(libc::ESRCH) {
                return Err(Error::Io(e));
            }
        }
    }
    Ok(())
}

/// Poll `still_running` until it is false, the timeout, or `escalate` runs
/// once `escalate_after` has elapsed. `escalate` runs at most once, then
/// the loop keeps polling. `Ok(false)` is the timeout.
pub(super) fn poll_until_stopped(
    mut still_running: impl FnMut() -> bool,
    mut escalate: impl FnMut() -> Result<()>,
    escalate_after: Option<Duration>,
    timeout: Duration,
    poll: Duration,
) -> Result<bool> {
    let start = Instant::now();
    let mut escalated = false;
    while still_running() {
        let elapsed = start.elapsed();
        if elapsed >= timeout {
            return Ok(false);
        }
        if !escalated {
            if let Some(grace) = escalate_after {
                if elapsed >= grace {
                    escalate()?;
                    escalated = true;
                    continue;
                }
            }
        }
        let mut slice = poll.min(timeout.saturating_sub(elapsed));
        if let Some(grace) = escalate_after {
            if !escalated && elapsed < grace {
                slice = slice.min(grace.saturating_sub(elapsed));
            }
        }
        if slice.is_zero() {
            continue;
        }
        std::thread::sleep(slice);
    }
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn already_stopped_does_not_escalate() {
        let mut n = 0;
        let gone = poll_until_stopped(
            || false,
            || {
                n += 1;
                Ok(())
            },
            Some(Duration::ZERO),
            Duration::from_secs(1),
            Duration::from_millis(10),
        )
        .unwrap();
        assert!(gone);
        assert_eq!(n, 0);
    }

    #[test]
    fn grace_escalates_once_then_stops() {
        let n = std::cell::Cell::new(0);
        let alive = std::cell::Cell::new(true);
        let start = Instant::now();
        let gone = poll_until_stopped(
            || alive.get(),
            || {
                n.set(n.get() + 1);
                alive.set(false);
                Ok(())
            },
            Some(Duration::from_millis(80)),
            Duration::from_secs(2),
            Duration::from_millis(20),
        )
        .unwrap();
        assert!(gone);
        assert_eq!(n.get(), 1);
        assert!(start.elapsed() >= Duration::from_millis(80));
        assert!(start.elapsed() < Duration::from_millis(800));
    }

    #[test]
    fn no_grace_times_out_without_escalate() {
        let start = Instant::now();
        let gone = poll_until_stopped(
            || true,
            || panic!("steam path must not escalate"),
            None,
            Duration::from_millis(120),
            Duration::from_millis(30),
        )
        .unwrap();
        assert!(!gone);
        assert!(start.elapsed() >= Duration::from_millis(120));
        assert!(start.elapsed() < Duration::from_millis(600));
    }

    #[test]
    fn escalate_error_surfaces() {
        let err = poll_until_stopped(
            || true,
            || Err(Error::Apply("signal failed".into())),
            Some(Duration::ZERO),
            Duration::from_secs(1),
            Duration::from_millis(10),
        );
        assert!(matches!(err, Err(Error::Apply(_))));
    }

    #[test]
    fn term_ignoring_child_dies_when_escalation_kills_it() {
        let mut child = std::process::Command::new("bash")
            .arg("-c")
            .arg("trap '' TERM; while true; do sleep 1; done")
            .spawn()
            .expect("bash");
        let pid = child.id();
        let start = Instant::now();
        let gone = poll_until_stopped(
            || !is_zombie(pid) && Path::new(&format!("/proc/{pid}")).exists(),
            || {
                let r = unsafe { libc::kill(pid as libc::pid_t, libc::SIGKILL) };
                if r != 0 {
                    let e = std::io::Error::last_os_error();
                    if e.raw_os_error() != Some(libc::ESRCH) {
                        return Err(Error::Io(e));
                    }
                }
                Ok(())
            },
            Some(Duration::from_millis(200)),
            Duration::from_secs(3),
            Duration::from_millis(40),
        );
        let _ = child.wait();
        let gone = gone.expect("escalate");
        assert!(gone, "child still alive after SIGKILL");
        assert!(start.elapsed() < Duration::from_secs(2));
        assert!(start.elapsed() >= Duration::from_millis(200));
    }

    use std::path::Path;

    fn is_zombie(pid: u32) -> bool {
        let Ok(stat) = std::fs::read_to_string(format!("/proc/{pid}/stat")) else {
            return false;
        };
        let Some((_, rest)) = stat.rsplit_once(')') else {
            return false;
        };
        rest.split_whitespace().next() == Some("Z")
    }
}
