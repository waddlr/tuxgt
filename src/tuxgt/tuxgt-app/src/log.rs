//! App-wide logging: info default, live debug toggle, one layout for both layers.
//!
//! Level policy: `error!` = user-visible op failed; `warn!` = degraded /
//! skipped, op continues; `info!` = op completed plus one boot banner;
//! `debug!` = user-action entry with params + outcome, state transitions,
//! pipeline stage boundaries. No `trace!` anywhere.

use std::path::Path;
use std::sync::atomic::{AtomicU8, Ordering};

use tracing::Metadata;
use tracing_subscriber::layer::{Context, Filter};

/// `0` = info, `1` = debug. Written by [`set_debug`], read by [`DynamicLevel`].
static LOG_LEVEL: AtomicU8 = AtomicU8::new(0);

/// Dynamic max-level gate: info by default, debug after [`set_debug`]`(true)`.
/// Custom [`Filter`] + [`AtomicU8`] instead of `tracing_subscriber::reload`
/// (avoids a feature-gate dependency). Used only when `RUST_LOG` is unset;
/// a set `RUST_LOG` keeps today's static `EnvFilter` semantics.
pub(crate) struct DynamicLevel;

fn allows(level: &tracing::Level) -> bool {
    let max = if LOG_LEVEL.load(Ordering::Relaxed) == 1 {
        &tracing::Level::DEBUG
    } else {
        &tracing::Level::INFO
    };
    level <= max
}

impl<S> Filter<S> for DynamicLevel {
    fn enabled(&self, meta: &Metadata<'_>, _: &Context<'_, S>) -> bool {
        allows(meta.level())
    }
}

/// Flip the live level; persistence is the caller's job.
pub(crate) fn set_debug(on: bool) {
    LOG_LEVEL.store(u8::from(on), Ordering::Relaxed);
    tracing::info!(debug = on, "log level changed");
}

/// Shared fmt-layer constructor so the stderr/file layouts cannot drift.
/// Caller adds `.with_writer(...)`; `ansi` selects color per destination.
pub(crate) fn layer<S>(ansi: bool) -> tracing_subscriber::fmt::Layer<S> {
    tracing_subscriber::fmt::Layer::default()
        .with_ansi(ansi)
        .with_target(true)
        .with_file(false)
        .with_line_number(false)
        .with_thread_ids(false)
}

/// Wall-clock stamp for run-log names: `YYYYMMDD-HHMMSS` local time, UTC when
/// the local offset is unavailable. Never empty, always sortable.
fn local_stamp() -> String {
    let dt = time::OffsetDateTime::now_local().unwrap_or_else(|_| time::OffsetDateTime::now_utc());
    format!(
        "{:04}{:02}{:02}-{:02}{:02}{:02}",
        dt.year(),
        u8::from(dt.month()),
        dt.day(),
        dt.hour(),
        dt.minute(),
        dt.second()
    )
}

/// Create this run's log file: `tuxgt.log.<stamp>`, `-<pid>` on a same-second
/// collision. `create_new` keeps two boots in the same second from sharing a
/// file; pid-reuse within one second appends instead of going dark.
pub(crate) fn create_run_log(dir: &Path) -> Option<(String, std::fs::File)> {
    use std::fs::OpenOptions;
    std::fs::create_dir_all(dir).ok()?;
    let base = format!("tuxgt.log.{}", local_stamp());
    let open_new = |name: &str| {
        OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(dir.join(name))
    };
    if let Ok(file) = open_new(&base) {
        return Some((base, file));
    }
    let fallback = format!("{base}-{}", std::process::id());
    open_new(&fallback)
        .or_else(|_| OpenOptions::new().append(true).open(dir.join(&fallback)))
        .ok()
        .map(|file| (fallback, file))
}

/// Best-effort `tuxgt.log` symlink → this run's file, so the latest log has a
/// stable path. Relative target keeps the prefix movable; temp-symlink +
/// rename swaps atomically. Silent on I/O errors (no logger yet in `main`).
#[cfg(unix)]
pub(crate) fn point_current_log(dir: &Path, name: &str) {
    let tmp = dir.join(format!(".tuxgt.log.tmp-{}", std::process::id()));
    let _ = std::fs::remove_file(&tmp);
    if std::os::unix::fs::symlink(name, &tmp).is_ok() {
        let _ = std::fs::rename(&tmp, dir.join("tuxgt.log"));
    }
}

/// Non-unix prefixes: no symlink, per-run files still work.
#[cfg(not(unix))]
pub(crate) fn point_current_log(_dir: &Path, _name: &str) {}

/// Best-effort retention: keep the newest `keep` `tuxgt.log.*` files in `dir`
/// by name (stamps sort chronologically; `-<pid>` collision names sort after
/// their stamp), never deleting `current`. The bare `tuxgt.log` symlink does
/// not match the `tuxgt.log.` prefix, so it is never a deletion candidate.
/// Silent on I/O errors (no logger yet at prune time in `main`).
pub(crate) fn prune_old_logs(dir: &Path, current: &str, keep: usize) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut names: Vec<String> = Vec::new();
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if !name.starts_with("tuxgt.log.") {
            continue;
        }
        names.push(name);
    }
    names.sort();
    for (i, name) in names.into_iter().rev().enumerate() {
        if i < keep || name == current {
            continue;
        }
        let _ = std::fs::remove_file(dir.join(&name));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dynamic_level_gates_debug() {
        set_debug(false);
        assert!(allows(&tracing::Level::INFO));
        assert!(!allows(&tracing::Level::DEBUG));
        assert!(!allows(&tracing::Level::TRACE));
        set_debug(true);
        assert!(allows(&tracing::Level::INFO));
        assert!(allows(&tracing::Level::DEBUG));
        assert!(!allows(&tracing::Level::TRACE));
        set_debug(false);
    }

    #[test]
    fn prune_keeps_newest_five() {
        let dir = std::env::temp_dir().join(format!("tuxgt-log-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temp dir");
        // Seven fake run logs with ascending stamps; newest is the current run.
        // A bare `tuxgt.log` current-link stand-in must survive pruning.
        let mut names: Vec<String> = Vec::new();
        for i in 0..7 {
            let name = format!("tuxgt.log.2026092{i}-153020");
            std::fs::write(dir.join(&name), b"x").expect("fake log");
            names.push(name);
        }
        std::fs::write(dir.join("tuxgt.log"), b"link").expect("current stand-in");
        let current = names.last().unwrap().clone();
        prune_old_logs(&dir, &current, 5);
        let mut kept: Vec<String> = std::fs::read_dir(&dir)
            .expect("read")
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        kept.sort();
        assert_eq!(kept.len(), 6, "kept = {kept:?}");
        assert!(kept.contains(&current));
        assert!(kept.contains(&names[5]));
        assert!(kept.contains(&"tuxgt.log".to_string()));
        assert!(!kept.contains(&names[0]));
        assert!(!kept.contains(&names[1]));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn run_log_collision_stays_unique() {
        let dir = std::env::temp_dir().join(format!("tuxgt-log-collide-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let (first, _) = create_run_log(&dir).expect("first run log");
        // Immediate second boot: same stamp, so the `-<pid>` fallback fires.
        let (second, _) = create_run_log(&dir).expect("second run log");
        assert_ne!(first, second, "same-second boots must not share a file");
        assert!(first.starts_with("tuxgt.log."));
        assert!(second.starts_with("tuxgt.log."));
        if second.starts_with(first.as_str()) {
            let suffix = format!("-{}", std::process::id());
            assert!(
                second.ends_with(suffix.as_str()),
                "collision must carry pid suffix: {second}"
            );
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    #[cfg(unix)]
    fn current_symlink_survives_prune() {
        let dir = std::env::temp_dir().join(format!("tuxgt-log-link-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temp dir");
        point_current_log(&dir, "tuxgt.log.20260923-153022");
        // Re-pointing swaps the target; pruning must keep the bare link.
        point_current_log(&dir, "tuxgt.log.20260923-153023");
        prune_old_logs(&dir, "tuxgt.log.20260923-153023", 5);
        assert_eq!(
            std::fs::read_link(dir.join("tuxgt.log")).expect("link survives").to_string_lossy(),
            "tuxgt.log.20260923-153023"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
