use std::io::{self, BufRead, IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use sha2::{Digest, Sha256};
use sqlx::SqlitePool;

use crate::game::GameId;
use crate::{Error, Result};

#[derive(Clone, Copy, Debug, Default)]
pub struct DetectOpts {
    pub force: bool,
    pub yes: bool,
}

pub(crate) fn fingerprint(paths: &[PathBuf]) -> String {
    let mut h = Sha256::new();
    for p in paths {
        h.update(p.to_string_lossy().as_bytes());
        h.update(b"\0");
        match std::fs::metadata(p) {
            Ok(m) => {
                h.update(m.len().to_le_bytes());
                let secs = m
                    .modified()
                    .ok()
                    .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                    .map(|d| d.as_secs())
                    .unwrap_or(0);
                h.update(secs.to_le_bytes());
            }
            Err(_) => h.update(0u64.to_le_bytes()),
        }
        h.update(b"\n");
    }
    hex_lower(&h.finalize())
}

pub(crate) fn hex_lower(bytes: &[u8]) -> String {
    const H: &[u8; 16] = b"0123456789abcdef";
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push(H[(b >> 4) as usize] as char);
        s.push(H[(b & 0xf) as usize] as char);
    }
    s
}

pub(crate) fn fp_paths(
    install_dir: Option<&Path>,
    exe: Option<&Path>,
    prefix: Option<&Path>,
) -> Vec<PathBuf> {
    let mut v = Vec::new();
    if let Some(p) = install_dir {
        v.push(p.to_path_buf());
    }
    if let Some(p) = exe {
        v.push(p.to_path_buf());
    }
    if let Some(p) = prefix {
        let ver = p.join("version");
        if ver.is_file() {
            v.push(ver);
        } else {
            v.push(p.to_path_buf());
        }
    }
    v
}

#[derive(Debug, sqlx::FromRow)]
pub(crate) struct Row {
    pub(crate) id: String,
    pub(crate) manager: String,
    pub(crate) store: String,
    pub(crate) game_id: String,
    pub(crate) install_dir: Option<String>,
    pub(crate) exe_path: Option<String>,
    pub(crate) prefix_path: Option<String>,
    pub(crate) proton: Option<String>,
    pub(crate) build: Option<String>,
    pub(crate) launch_options: Option<String>,
    pub(crate) env: Option<String>,
    pub(crate) wrapper: Option<String>,
    pub(crate) detected_exe_path: Option<String>,
    pub(crate) detected_platform: Option<String>,
    pub(crate) detected_bitness: Option<String>,
    pub(crate) detected_api: Option<String>,
    pub(crate) detected_extra_apis: Option<String>,
    pub(crate) detected_engine: Option<String>,
    pub(crate) detected_prefix_path: Option<String>,
    pub(crate) detected_proton: Option<String>,
    pub(crate) detected_build: Option<String>,
    pub(crate) detected_exe_version: Option<String>,
    pub(crate) override_exe_path: Option<String>,
    pub(crate) override_platform: Option<String>,
    pub(crate) override_bitness: Option<String>,
    pub(crate) override_api: Option<String>,
    pub(crate) override_extra_apis: Option<String>,
    pub(crate) override_engine: Option<String>,
    pub(crate) override_prefix_path: Option<String>,
    pub(crate) override_proton: Option<String>,
    pub(crate) override_build: Option<String>,
    pub(crate) override_exe_version: Option<String>,
    pub(crate) fingerprint: Option<String>,
}

pub(crate) async fn fetch_row(pool: &SqlitePool, id: &str) -> Result<Option<Row>> {
    let row = sqlx::query_as::<_, Row>(
        "SELECT id, manager, store, game_id, install_dir, exe_path, prefix_path, proton, build,
                launch_options, env, wrapper,
                detected_exe_path, detected_platform, detected_bitness, detected_api, detected_extra_apis,
                detected_engine,
                detected_prefix_path, detected_proton, detected_build, detected_exe_version,
                override_exe_path, override_platform, override_bitness, override_api, override_extra_apis,
                override_engine,
                override_prefix_path, override_proton, override_build, override_exe_version,
                fingerprint
         FROM games WHERE id = ?",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

pub(crate) fn overrides_of(r: &Row) -> Vec<(&'static str, String)> {
    let mut v = Vec::new();
    let mut push = |k, o: &Option<String>| {
        if let Some(s) = o {
            v.push((k, s.clone()));
        }
    };
    push("exe", &r.override_exe_path);
    push("platform", &r.override_platform);
    push("bitness", &r.override_bitness);
    push("api", &r.override_api);
    push("extra_apis", &r.override_extra_apis);
    push("engine", &r.override_engine);
    push("prefix", &r.override_prefix_path);
    push("proton", &r.override_proton);
    push("build", &r.override_build);
    push("exe_version", &r.override_exe_version);
    v
}

pub(crate) fn confirm_wipe(id: &str, fields: &[(&str, String)], yes: bool) -> Result<()> {
    if fields.is_empty() {
        return Ok(());
    }
    eprintln!("force redetect will remove overrides for {id}:");
    for (k, val) in fields {
        eprintln!("  {k}\t{val}");
    }
    if yes {
        return Ok(());
    }
    if !io::stdin().is_terminal() {
        return Err(Error::NeedConfirm(
            "force redetect removes overrides; pass --yes".into(),
        ));
    }
    eprint!("type y to continue: ");
    let _ = io::stderr().flush();
    let mut line = String::new();
    io::stdin().lock().read_line(&mut line)?;
    if line.trim().eq_ignore_ascii_case("y") {
        Ok(())
    } else {
        Err(Error::NeedConfirm("aborted".into()))
    }
}

/// One overridable detector field with its three sources. `None` means unset.
#[derive(Clone, Debug)]
pub struct DetectSnapshot {
    pub key: &'static str,
    pub detected: Option<String>,
    pub override_: Option<String>,
    pub store: Option<String>,
}

impl DetectSnapshot {
    /// Effective value: override wins, then detected, then the store snapshot.
    pub fn effective(&self) -> Option<&str> {
        self.override_
            .as_deref()
            .or(self.detected.as_deref())
            .or(self.store.as_deref())
    }
}

/// Detector fields the GUI override editor (E32) may read and write.
/// Keys match `overrides_of` and the `tuxgt doctor` field names.
pub const OVERRIDE_FIELDS: &[&str] = &[
    "exe",
    "platform",
    "bitness",
    "api",
    "extra_apis",
    "engine",
    "prefix",
    "proton",
    "build",
    "exe_version",
];

/// Read detected / override / store values without running detectors.
/// Unknown id → `Error::UnknownGame`.
pub async fn detection_snapshot(pool: &SqlitePool, id: &str) -> Result<Vec<DetectSnapshot>> {
    GameId::parse(id)?;
    let row = fetch_row(pool, id)
        .await?
        .ok_or_else(|| Error::UnknownGame(id.into()))?;
    let snap = |key: &'static str,
                detected: &Option<String>,
                override_: &Option<String>,
                store: &Option<String>| {
        DetectSnapshot {
            key,
            detected: detected.clone(),
            override_: override_.clone(),
            store: store.clone(),
        }
    };
    Ok(vec![
        snap(
            "exe",
            &row.detected_exe_path,
            &row.override_exe_path,
            &row.exe_path,
        ),
        snap(
            "platform",
            &row.detected_platform,
            &row.override_platform,
            &None,
        ),
        snap(
            "bitness",
            &row.detected_bitness,
            &row.override_bitness,
            &None,
        ),
        snap("api", &row.detected_api, &row.override_api, &None),
        snap(
            "extra_apis",
            &row.detected_extra_apis,
            &row.override_extra_apis,
            &None,
        ),
        snap("engine", &row.detected_engine, &row.override_engine, &None),
        snap(
            "prefix",
            &row.detected_prefix_path,
            &row.override_prefix_path,
            &row.prefix_path,
        ),
        snap(
            "proton",
            &row.detected_proton,
            &row.override_proton,
            &row.proton,
        ),
        snap(
            "build",
            &row.detected_build,
            &row.override_build,
            &row.build,
        ),
        snap(
            "exe_version",
            &row.detected_exe_version,
            &row.override_exe_version,
            &None,
        ),
    ])
}

pub(crate) fn override_column(field: &str) -> Result<&'static str> {
    if !OVERRIDE_FIELDS.contains(&field) {
        return Err(Error::InvalidOverride(format!(
            "unknown detector field: {field}"
        )));
    }
    Ok(match field {
        "exe" => "override_exe_path",
        "platform" => "override_platform",
        "bitness" => "override_bitness",
        "api" => "override_api",
        "extra_apis" => "override_extra_apis",
        "engine" => "override_engine",
        "prefix" => "override_prefix_path",
        "proton" => "override_proton",
        "build" => "override_build",
        _ => "override_exe_version",
    })
}

/// Field-name + value rules for one detector override: the single owner of the
/// `platform` / `bitness` value checks. `None`/empty clears, so clearing never
/// fails validation. Callers that apply several overrides validate every op
/// here first, then write, so a bad op cannot leave a partial write.
pub fn validate_override(field: &str, value: Option<&str>) -> Result<()> {
    override_column(field)?;
    match (field, value.filter(|v| !v.is_empty())) {
        ("platform", Some(v)) if !matches!(v, "native" | "proton" | "wine") => {
            Err(Error::InvalidOverride(format!("bad platform: {v}")))
        }
        ("bitness", Some(v)) if !matches!(v, "32" | "64") => {
            Err(Error::InvalidOverride(format!("bad bitness: {v}")))
        }
        _ => Ok(()),
    }
}

/// Set one detector override (`None`/empty clears it back to detected).
/// Same columns the scan pipeline reads; never touches `detected_*`.
/// Unknown id → `Error::UnknownGame`.
pub async fn set_override(
    pool: &SqlitePool,
    id: &str,
    field: &str,
    value: Option<&str>,
) -> Result<()> {
    GameId::parse(id)?;
    validate_override(field, value)?;
    let column = override_column(field)?;
    let value = value.filter(|v| !v.is_empty());
    if fetch_row(pool, id).await?.is_none() {
        return Err(Error::UnknownGame(id.into()));
    }
    // Column is allowlisted above; never interpolated from raw input.
    // R55: user-initiated exe/prefix overrides refuse when the post-write
    // render would collide with another game's key (names the owner, writes
    // nothing). Batch writers bypass `set_override` (scan writes
    // `detected_*` directly), so they never refuse — the render net covers
    // them.
    crate::session::check_override_collision(pool, id, field, value).await?;
    let sql = format!("UPDATE games SET {column} = ? WHERE id = ?");
    sqlx::query(&sql).bind(value).bind(id).execute(pool).await?;
    Ok(())
}

pub(crate) async fn clear_overrides(pool: &SqlitePool, id: &str) -> Result<()> {
    sqlx::query(
         "UPDATE games SET
             override_exe_path = NULL, override_platform = NULL, override_bitness = NULL,
             override_api = NULL, override_extra_apis = NULL, override_engine = NULL, override_prefix_path = NULL,
             override_proton = NULL, override_build = NULL, override_exe_version = NULL
          WHERE id = ?",
     )
     .bind(id)
     .execute(pool)
     .await?;
    Ok(())
}
