use std::fs::{File, OpenOptions};
use std::io::{self, Read, Write};
use std::os::fd::AsRawFd;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, SyncSender};
use std::time::Duration;

const LOCK_NAME: &str = "gui.lock";
const SOCKET_NAME: &str = "gui.sock";
/// A Play failure while hidden waits here for the next window's status
/// line. Written by the tray stub, taken (read + removed) by the next GUI
/// boot; missing reads as no pending status.
const STATUS_NAME: &str = "tray.status";
const FOCUS_REQUEST: u8 = b'f';
const FOCUS_RESPONSE: u8 = b'o';
/// Settle budget for a transient primary (mid-handoff socket): shared with
/// the tray stub's takeover wait.
pub(crate) const CONNECT_RETRY: Duration = Duration::from_millis(20);
pub(crate) const CONNECT_ATTEMPTS: usize = 100;
const FOCUS_ACK_TIMEOUT: Duration = Duration::from_secs(2);

pub(crate) enum Acquire {
    Primary(Primary),
    Existing,
}

pub(crate) struct Primary {
    listener: UnixListener,
    path: PathBuf,
    _lock: File,
}

pub(crate) struct FocusRequest {
    pub(crate) ack: SyncSender<bool>,
}

pub(crate) struct Running {
    pub(crate) receiver: Receiver<FocusRequest>,
}

impl Primary {
    fn listen(data_dir: &Path) -> io::Result<Option<Self>> {
        std::fs::create_dir_all(data_dir)?;
        let lock_path = data_dir.join(LOCK_NAME);
        let lock = OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .open(lock_path)?;
        if unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
            let error = io::Error::last_os_error();
            if error.raw_os_error() == Some(libc::EWOULDBLOCK) {
                return Ok(None);
            }
            return Err(error);
        }

        let path = data_dir.join(SOCKET_NAME);
        let _ = std::fs::remove_file(&path);
        let listener = UnixListener::bind(&path)?;
        Ok(Some(Self {
            listener,
            path,
            _lock: lock,
        }))
    }

    pub(crate) fn start(self) -> Running {
        let Primary {
            listener,
            path,
            _lock,
        } = self;
        let (sender, receiver) = mpsc::channel();
        std::thread::spawn(move || {
            let _lock = _lock;
            loop {
                let mut stream = match listener.accept() {
                    Ok((stream, _)) => stream,
                    Err(error) => {
                        tracing::warn!(%error, "single-instance accept failed");
                        std::thread::sleep(CONNECT_RETRY);
                        continue;
                    }
                };
                let mut request = [0u8; 1];
                if stream.read_exact(&mut request).is_err() || request[0] != FOCUS_REQUEST {
                    continue;
                }
                let (ack_sender, ack_receiver) = mpsc::sync_channel(1);
                if sender.send(FocusRequest { ack: ack_sender }).is_err() {
                    return;
                }
                match ack_receiver.recv_timeout(FOCUS_ACK_TIMEOUT) {
                    Ok(true) => {
                        let _ = stream.write_all(&[FOCUS_RESPONSE]);
                        let _ = stream.flush();
                        tracing::info!(socket = %path.display(), "focus request received");
                    }
                    Ok(false) | Err(_) => {}
                }
            }
        });
        Running { receiver }
    }
}

pub(crate) fn acquire(data_dir: &Path) -> io::Result<Acquire> {
    let mut last_error = None;
    for _ in 0..CONNECT_ATTEMPTS {
        if let Some(primary) = Primary::listen(data_dir)? {
            return Ok(Acquire::Primary(primary));
        }
        match connect_existing(data_dir) {
            Ok(Acquire::Existing) => return Ok(Acquire::Existing),
            Ok(Acquire::Primary(_)) => unreachable!("connect_existing cannot be primary"),
            Err(error) if handshake_transient(error.kind()) => {
                last_error = Some(error);
                std::thread::sleep(CONNECT_RETRY);
            }
            Err(error) => return Err(error),
        }
    }
    Err(last_error.unwrap_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            "single-instance socket unavailable",
        )
    }))
}

fn connect_existing(data_dir: &Path) -> io::Result<Acquire> {
    let path = data_dir.join(SOCKET_NAME);
    let mut stream = UnixStream::connect(path)?;
    stream.set_read_timeout(Some(FOCUS_ACK_TIMEOUT + Duration::from_secs(1)))?;
    stream.set_write_timeout(Some(FOCUS_ACK_TIMEOUT + Duration::from_secs(1)))?;
    stream.write_all(&[FOCUS_REQUEST])?;
    stream.flush()?;
    let mut response = [0u8; 1];
    stream.read_exact(&mut response)?;
    if response[0] == FOCUS_RESPONSE {
        Ok(Acquire::Existing)
    } else {
        Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "invalid single-instance response",
        ))
    }
}

/// A failed handshake worth retrying: the primary is mid-handoff (Hide
/// spawned the tray stub and is exiting, or a Show respawned the GUI and
/// the stub is leaving), so the socket died between our connect and its
/// ack. A wedged primary that holds the socket but never acks still
/// surfaces as a timeout, never a silent retry.
pub(crate) fn handshake_transient(kind: io::ErrorKind) -> bool {
    use io::ErrorKind::*;
    matches!(
        kind,
        NotFound | ConnectionRefused | UnexpectedEof | ConnectionReset | BrokenPipe
    )
}

/// One non-blocking attempt at primary, without the focus handshake: the
/// tray stub's wait for the hiding GUI to release the prefix. `Ok(None)`
/// means a live primary still holds the lock.
pub(crate) fn try_listen(data_dir: &Path) -> io::Result<Option<Primary>> {
    Primary::listen(data_dir)
}

/// Take the prefix, retrying briefly. Test-only: a concurrent spawn in the
/// test process dups the lock fd across fork, so a drop-then-retake can
/// still see the lock until the child execs. Production never
/// drop-retakes — takes are monotonic per process, and every take site
/// already retries — so only tests need the settle.
#[cfg(test)]
pub(crate) fn take_settled(data_dir: &Path) -> Primary {
    for _ in 0..100 {
        match try_listen(data_dir).expect("take settled") {
            Some(primary) => return primary,
            None => std::thread::sleep(std::time::Duration::from_millis(10)),
        }
    }
    panic!("prefix never freed");
}

/// Park one status line for the next window. Best-effort: a failed write
/// logs, and the Play failure is already in the log.
pub(crate) fn stash_tray_status(data_dir: &Path, text: &str) {
    if let Err(error) = std::fs::write(data_dir.join(STATUS_NAME), text) {
        tracing::warn!(%error, "stash tray status");
    }
}

/// Take the parked status line, if any. Missing or blank reads as none;
/// the file never survives a boot.
pub(crate) fn take_tray_status(data_dir: &Path) -> Option<String> {
    let path = data_dir.join(STATUS_NAME);
    let text = std::fs::read_to_string(&path).ok()?;
    let _ = std::fs::remove_file(&path);
    let trimmed = text.trim().to_string();
    (!trimmed.is_empty()).then_some(trimmed)
}

#[cfg(test)]
mod tests {
    use super::{acquire, Acquire};
    use std::path::PathBuf;

    fn test_dir(name: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "tuxgt-single-instance-{name}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn second_launch_sends_focus_request() {
        let dir = test_dir("focus");
        let primary = match acquire(&dir).unwrap() {
            Acquire::Primary(primary) => primary,
            Acquire::Existing => panic!("first launch became secondary"),
        };
        let running = primary.start();
        let responder = std::thread::spawn(move || {
            let request = running.receiver.recv().unwrap();
            request.ack.send(true).unwrap();
        });
        assert!(matches!(acquire(&dir).unwrap(), Acquire::Existing));
        responder.join().unwrap();
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn stale_socket_is_replaced() {
        let dir = test_dir("stale");
        std::fs::write(dir.join("gui.sock"), b"stale").unwrap();
        assert!(matches!(acquire(&dir).unwrap(), Acquire::Primary(_)));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn existing_socket_without_listener_is_replaced() {
        let dir = test_dir("dead");
        let path = dir.join("gui.sock");
        let listener = std::os::unix::net::UnixListener::bind(&path).unwrap();
        drop(listener);
        assert!(matches!(acquire(&dir).unwrap(), Acquire::Primary(_)));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn only_a_vanishing_primary_is_transient() {
        use super::{handshake_transient, take_settled, try_listen};
        use std::io::ErrorKind::*;
        for kind in [
            NotFound,
            ConnectionRefused,
            UnexpectedEof,
            ConnectionReset,
            BrokenPipe,
        ] {
            assert!(handshake_transient(kind), "{kind:?} retries");
        }
        // A wedged primary (timeout) and a real refusal stay loud.
        for kind in [TimedOut, PermissionDenied, InvalidData, AddrInUse] {
            assert!(!handshake_transient(kind), "{kind:?} surfaces");
        }
        let dir = test_dir("takeover");
        // Held: no takeover. Released: the stale socket is replaced.
        let primary = try_listen(&dir).expect("first listen");
        assert!(primary.is_some());
        assert!(try_listen(&dir).expect("second listen").is_none());
        drop(primary);
        take_settled(&dir);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn tray_status_parks_one_line_for_the_next_boot() {
        use super::{stash_tray_status, take_tray_status};
        let dir = test_dir("status");
        assert_eq!(take_tray_status(&dir), None);
        stash_tray_status(&dir, "Play failed: boom");
        assert_eq!(take_tray_status(&dir).as_deref(), Some("Play failed: boom"));
        // Taken, not peeked: the next boot starts clean.
        assert_eq!(take_tray_status(&dir), None);
        stash_tray_status(&dir, "  \n");
        assert_eq!(take_tray_status(&dir), None);
        let _ = std::fs::remove_dir_all(dir);
    }
}
