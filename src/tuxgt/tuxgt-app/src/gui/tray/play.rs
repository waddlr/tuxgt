//! Headless tray Play: the worker both controller loops share.

use std::sync::mpsc::Sender;

use super::super::load::rt_block;
use super::policy::{store_recents, RecentSnapshot, RECENT_LIMIT};

/// One tray-menu Play result, reported back to the controller loop. Success
/// needs nothing (the worker already touched `last_played` and refreshed the
/// snapshot); a failure surfaces in the status line.
pub(crate) struct TrayPlayOutcome {
    pub(crate) id: String,
    pub(crate) result: tuxgt_core::Result<()>,
}

/// Run one headless Play off the controller loop; the outcome returns on
/// `outcomes`. Shared by the window controller and the hidden stub.
pub(crate) fn spawn_play_worker(
    id: String,
    recents: &RecentSnapshot,
    outcomes: Sender<TrayPlayOutcome>,
) {
    let recents = RecentSnapshot::clone(recents);
    // Blocking DB + spawn work leaves the controller loop: the outcome
    // comes back through the channel.
    std::thread::spawn(move || {
        let result = tray_play_blocking(&id, &recents);
        let _ = outcomes.send(TrayPlayOutcome { id, result });
    });
}

/// Headless Play for a tray-menu click: GUI Play semantics with no arming —
/// build the spec from stored handle/apply state, detached spawn,
/// `touch_last_played`. Never `exec` (that would replace the tray process),
/// never opens a window, never quits. A stale id is an honest no-op.
fn tray_play_blocking(id: &str, recents: &RecentSnapshot) -> tuxgt_core::Result<()> {
    rt_block(async {
        let dir = tuxgt_core::data_dir();
        let pool = tuxgt_core::open_db_shared(&dir).await?;
        if !tuxgt_core::game_exists(&pool, id).await? {
            // Stale click: the snapshot still names a game that is gone;
            // drop the ghost row with this click, not the next refresh.
            match tuxgt_core::recent_games(&pool, RECENT_LIMIT).await {
                Ok(rows) => store_recents(recents, &rows),
                Err(error) => tracing::warn!(%error, "tray recents refresh failed"),
            }
            return Ok(());
        }
        let host = tuxgt_core::PluginHost::load()?;
        let paths = tuxgt_core::LaunchPaths::detect()?;
        let spec = tuxgt_core::build_launch_spec(&pool, &host, id, &paths, &dir).await?;
        // Plain Play performs no store writes, so there is nothing the
        // running-client guard must refuse — and it never stops processes.
        spec.command_detached()
            .spawn()
            .map_err(tuxgt_core::Error::Io)?;
        tuxgt_core::touch_last_played(&pool, id).await?;
        let rows = tuxgt_core::recent_games(&pool, RECENT_LIMIT).await?;
        store_recents(recents, &rows);
        Ok(())
    })
}

/// Re-read the tray's recent-games snapshot. A failed read keeps the old
/// list: the menu stays stale-but-honest, never an error row.
pub(crate) fn refresh_recents(recents: &RecentSnapshot) {
    match rt_block(async {
        let pool = tuxgt_core::open_db_shared(&tuxgt_core::data_dir()).await?;
        tuxgt_core::recent_games(&pool, RECENT_LIMIT).await
    }) {
        Ok(rows) => store_recents(recents, &rows),
        Err(error) => tracing::warn!(%error, "tray recents refresh failed"),
    }
}

#[cfg(test)]
mod tests {
    use super::super::policy::RecentGame;
    use super::*;
    use std::sync::Arc;

    /// Tray Play with fixtures: an unknown id is an honest no-op, a manual
    /// game launches headless (`/bin/true`, exits at once) and bumps
    /// `last_played` into the snapshot, and a spec failure (no launcher
    /// anywhere) errors without a bump. `TUXGT_DATA` scopes the prefix;
    /// nothing here needs a window.
    #[cfg(unix)]
    #[test]
    fn tray_play_worker_noops_stale_launches_and_reports_failure() {
        static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let dir = std::env::temp_dir().join(format!("tuxgt-tray-play-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temp prefix");
        let prev = std::env::var_os("TUXGT_DATA");
        std::env::set_var("TUXGT_DATA", &dir);
        // Hermetic launcher: `data_dir/bin` wins the lookup, so the success
        // leg never depends on an installed tuxgt.
        let launcher = dir.join("bin/tuxgt-launcher");
        std::fs::create_dir_all(launcher.parent().expect("bin dir")).expect("bin dir");
        std::fs::write(&launcher, "#!/bin/sh\nexec \"$@\"\n").expect("fake launcher");
        std::fs::set_permissions(
            &launcher,
            std::os::unix::fs::PermissionsExt::from_mode(0o755),
        )
        .expect("chmod launcher");

        let recents: RecentSnapshot = Arc::new(std::sync::RwLock::new(Vec::new()));
        // Stale click: no row, no launch, no error — and the ghost row
        // leaves with this click instead of the next refresh.
        recents.write().expect("snapshot").push(RecentGame {
            id: "manual:standalone:deadbeef".into(),
            display: "Ghost".into(),
        });
        tray_play_blocking("manual:standalone:deadbeef", &recents).expect("stale no-op");
        assert!(recents.read().expect("snapshot").is_empty());

        // Seed a manual game on an exe that exits at once.
        let id = rt_block(async {
            let pool = tuxgt_core::open_db(&tuxgt_core::data_dir()).await?;
            let host = tuxgt_core::PluginHost::load()?;
            let row =
                tuxgt_core::add_manual(&pool, &host, std::path::Path::new("/bin/true")).await?;
            tuxgt_core::Result::Ok(row.id)
        })
        .expect("seed manual game");
        tray_play_blocking(&id, &recents).expect("headless play");
        let played = rt_block(async {
            let pool = tuxgt_core::open_db(&tuxgt_core::data_dir()).await?;
            let row = tuxgt_core::game_row_by_id(&pool, &id)
                .await?
                .expect("played row");
            tuxgt_core::Result::Ok(row.last_played)
        })
        .expect("read back");
        assert!(played.is_some(), "success bumps last_played");
        let guard = recents.read().expect("snapshot");
        assert_eq!(guard.len(), 1);
        assert_eq!(guard[0].id, id);
        assert!(!guard[0].display.is_empty());
        drop(guard);

        // No launcher anywhere: the spec fails, `last_played` keeps its value.
        // PATH keeps every entry except ones shipping tuxgt-launcher, so
        // sibling tests spawning helpers (git) keep resolving mid-suite.
        std::fs::remove_file(&launcher).expect("drop fake launcher");
        let prev_path = std::env::var_os("PATH");
        let kept: Vec<std::path::PathBuf> = prev_path
            .as_ref()
            .map(|path| {
                std::env::split_paths(path)
                    .filter(|dir| !dir.join("tuxgt-launcher").is_file())
                    .collect()
            })
            .unwrap_or_default();
        std::env::set_var("PATH", std::env::join_paths(kept).expect("join PATH"));
        let failed = tray_play_blocking(&id, &recents);
        match prev_path {
            Some(v) => std::env::set_var("PATH", v),
            None => std::env::remove_var("PATH"),
        }
        assert!(failed.is_err(), "spec failure surfaces");
        let played_after = rt_block(async {
            let pool = tuxgt_core::open_db(&tuxgt_core::data_dir()).await?;
            let row = tuxgt_core::game_row_by_id(&pool, &id)
                .await?
                .expect("played row");
            tuxgt_core::Result::Ok(row.last_played)
        })
        .expect("read back");
        assert_eq!(played_after, played, "failure never touches last_played");
        // The failed Play refreshes nothing away; the success still lists.
        refresh_recents(&recents);
        let guard = recents.read().expect("snapshot");
        assert_eq!(guard.len(), 1);
        assert_eq!(guard[0].id, id);

        match prev {
            Some(v) => std::env::set_var("TUXGT_DATA", v),
            None => std::env::remove_var("TUXGT_DATA"),
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}
