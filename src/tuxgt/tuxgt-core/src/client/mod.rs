mod proc;
mod stop;

use std::process::Command;
use std::time::Duration;

use crate::game::GameId;
use crate::launch::{detach_command, find_heroic, find_steam};
use crate::{Error, Result};

/// Stop-wait deadline for a client exit before Apply gives up unwritten.
const STOP_TIMEOUT: Duration = Duration::from_secs(30);
/// Poll cadence while waiting for the client procs to disappear.
const STOP_POLL: Duration = Duration::from_millis(200);
/// How long Heroic's SIGTERM may take before we SIGKILL.
///
/// Electron turns SIGTERM into `app.quit()`, which closes windows. Heroic's
/// `close` handler always `preventDefault`s, and with Exit-to-tray it hides
/// the window and returns — the main process stays up holding GamesConfig.
/// A real Quit (`handleExit`) is the tray menu / Ctrl+Q, which we cannot
/// send. Three seconds covers a tray-off `handleExit` (it hides, drops GOG
/// presence, then `app.exit()`). Past that the process is stuck in the tray
/// or on Heroic's own "pending operations" dialog.
const HEROIC_TERM_GRACE: Duration = Duration::from_secs(3);

/// Store client that owns the config Apply writes: Steam (`localconfig.vdf`,
/// plus `shortcuts.vdf` for shortcut rows) or Heroic (`GamesConfig`).
/// Both hold those files in memory and flush on exit/save, so an Apply
/// written while the client runs is discarded — stop it first.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum StoreClient {
    Steam,
    Heroic,
}

impl StoreClient {
    pub fn name(self) -> &'static str {
        match self {
            StoreClient::Steam => "Steam",
            StoreClient::Heroic => "Heroic",
        }
    }

    /// Process name plus full-cmdline tokens for installs the comm alone
    /// misses: Electron-packaged Heroic (`.../electron` with
    /// `heroic-games-launcher/app.asar` in later args), AppImage
    /// (`Heroic-*.AppImage` running as `AppRun`), and Flatpak wrappers
    /// (argv0 is `flatpak`/`bwrap`, the app id in later args).
    pub(super) fn match_tokens(self) -> (&'static str, &'static [&'static str]) {
        match self {
            StoreClient::Steam => ("steam", &["com.valvesoftware.Steam"]),
            StoreClient::Heroic => (
                "heroic",
                &["heroic-games-launcher", "com.heroicgameslauncher.hgl"],
            ),
        }
    }

    /// Owning client for a game id, if any. Manual rows have none.
    pub fn for_game(game_id: &str) -> Option<Self> {
        match GameId::parse(game_id).ok()?.manager.as_str() {
            "steam" => Some(StoreClient::Steam),
            "heroic" => Some(StoreClient::Heroic),
            _ => None,
        }
    }

    /// True when the config-holding process is alive. Electron helpers
    /// (`--type=renderer` and friends) share the app path in argv but do
    /// not hold GamesConfig; a leftover renderer is not "Heroic is running".
    pub fn running(self) -> bool {
        !proc::pids(self, proc::Role::Main).is_empty()
    }

    /// Clean stop, then wait for the config-holding process to disappear.
    ///
    /// Steam: `steam -shutdown` (flushes; that path owns `steamwebhelper`).
    /// Heroic: SIGTERM the Electron main process. If it is still alive after
    /// [`HEROIC_TERM_GRACE`], SIGKILL that main process and its Electron
    /// helpers. Never SIGKILL Steam. Never signal a process group: a game
    /// Heroic spawned can share the group.
    pub fn stop_and_wait(self) -> Result<()> {
        stop::stop_and_wait(self)
    }

    /// Stop for a store write. `confirmed` is the ClientStop
    /// authorization: without it, a client that started mid-op aborts the
    /// write instead of being stopped without consent. `Ok(Some)` stopped.
    pub fn stop_for_write(game_id: &str, confirmed: bool) -> Result<Option<StoreClient>> {
        match Self::for_game(game_id) {
            Some(c) if c.running() => {
                if !confirmed {
                    return Err(Error::Apply(format!(
                        "{} is running; nothing written — retry for the stop prompt",
                        c.name()
                    )));
                }
                c.stop_and_wait()?;
                Ok(Some(c))
            }
            _ => Ok(None),
        }
    }

    /// Restart the client detached (new session): it outlives tuxgt and no
    /// longer shares our process group.
    pub fn restart_detached(self) -> Result<String> {
        let prog = match self {
            StoreClient::Steam => find_steam(),
            StoreClient::Heroic => find_heroic(),
        }
        .ok_or_else(|| {
            Error::Apply(format!(
                "stopped {} but the {} binary was not found; start it by hand",
                self.name(),
                self.name()
            ))
        })?;
        let mut cmd = Command::new(&prog);
        detach_command(&mut cmd);
        cmd.stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());
        cmd.spawn().map_err(Error::Io)?;
        Ok(format!("{} restarted", self.name()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn client_for_game_maps_manager() {
        assert_eq!(
            StoreClient::for_game("steam:standalone:4139290751"),
            Some(StoreClient::Steam)
        );
        assert_eq!(
            StoreClient::for_game("heroic:gog:1719198803"),
            Some(StoreClient::Heroic)
        );
        assert_eq!(StoreClient::for_game("manual:standalone:abcdef12"), None);
        assert_eq!(StoreClient::for_game("bogus"), None);
    }

    #[test]
    fn client_names() {
        assert_eq!(StoreClient::Steam.name(), "Steam");
        assert_eq!(StoreClient::Heroic.name(), "Heroic");
    }

    #[test]
    fn stop_for_write_without_client_is_noop() {
        assert!(
            StoreClient::stop_for_write("manual:standalone:abcdef12", false)
                .unwrap()
                .is_none()
        );
    }
}
