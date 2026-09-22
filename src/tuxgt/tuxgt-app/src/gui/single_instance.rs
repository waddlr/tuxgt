use std::fs::{File, OpenOptions};
use std::io::{self, Read, Write};
use std::os::fd::AsRawFd;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, SyncSender};
use std::time::Duration;

const LOCK_NAME: &str = "gui.lock";
const SOCKET_NAME: &str = "gui.sock";
const FOCUS_REQUEST: u8 = b'f';
const FOCUS_RESPONSE: u8 = b'o';
const CONNECT_RETRY: Duration = Duration::from_millis(20);
const CONNECT_ATTEMPTS: usize = 100;
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
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::NotFound | io::ErrorKind::ConnectionRefused
                ) =>
            {
                last_error = Some(error);
                std::thread::sleep(CONNECT_RETRY);
            }
            Err(error) => return Err(error),
        }
    }
    Err(last_error.unwrap_or_else(|| io::Error::new(
        io::ErrorKind::NotFound,
        "single-instance socket unavailable",
    )))
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

#[cfg(test)]
mod tests {
    use super::{acquire, Acquire};
    use std::path::PathBuf;

    fn test_dir(name: &str) -> PathBuf {
        let path = std::env::temp_dir()
            .join(format!("tuxgt-single-instance-{name}-{}", std::process::id()));
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
}
