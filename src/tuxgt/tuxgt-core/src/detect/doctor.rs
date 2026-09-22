use std::path::PathBuf;

use sqlx::SqlitePool;

use super::*;
use crate::game::GameId;
use crate::{Error, Result};

#[derive(Clone, Debug)]
pub struct DoctorField {
    pub key: &'static str,
    pub value: String,
    pub source: Option<&'static str>,
}

#[derive(Clone, Debug)]
pub struct DoctorReport {
    pub id: String,
    pub fields: Box<[DoctorField]>,
    pub skipped: bool,
}

pub async fn detect_one(pool: &SqlitePool, id: &str, opts: DetectOpts) -> Result<bool> {
    let Some(row) = fetch_row(pool, id).await? else {
        return Err(Error::UnknownGame(id.into()));
    };
    if opts.force {
        confirm_wipe(id, &overrides_of(&row), opts.yes)?;
        clear_overrides(pool, id).await?;
    }
    let install = row.install_dir.as_deref().map(PathBuf::from);
    let store_exe = row.exe_path.as_deref().map(PathBuf::from);
    let det_exe = row.detected_exe_path.as_deref().map(PathBuf::from);
    let exe = store_exe.clone().or(det_exe);
    let prefix = row
        .prefix_path
        .as_deref()
        .or(row.detected_prefix_path.as_deref())
        .map(PathBuf::from);
    let fp = fingerprint(&fp_paths(
        install.as_deref(),
        exe.as_deref(),
        prefix.as_deref(),
    ));
    if !opts.force && row.fingerprint.as_deref() == Some(fp.as_str()) {
        return Ok(true);
    }
    let gid = GameId::new(&row.manager, &row.store, &row.game_id)?;
    let mut seed = Detected::default();
    if let Some(p) = store_exe {
        seed.exe_path = Some(p);
    }
    seed.build = row.build.clone();
    let out = run_detectors(&gid, install.as_deref(), seed);
    let new_fp = fingerprint(&fp_paths(
        install.as_deref(),
        out.exe_path.as_deref().or(exe.as_deref()),
        out.prefix_path.as_deref().or(prefix.as_deref()),
    ));
    let det_exe = if sourced(&out, "exe_path") {
        out.exe_path
            .as_ref()
            .map(|p| p.to_string_lossy().into_owned())
    } else {
        None
    };
    let det_pfx = if sourced(&out, "prefix_path") {
        out.prefix_path
            .as_ref()
            .map(|p| p.to_string_lossy().into_owned())
    } else {
        None
    };
    let det_platform = if sourced(&out, "platform") {
        out.platform.clone()
    } else {
        None
    };
    let det_bitness = if sourced(&out, "bitness") {
        out.bitness.clone()
    } else {
        None
    };
    let det_api = if sourced(&out, "api") {
        out.api.clone()
    } else {
        None
    };
    let det_extra = if sourced(&out, "extra_apis") {
        out.extra_apis.clone()
    } else {
        None
    };
    let det_engine = if sourced(&out, "engine") {
        out.engine.clone()
    } else {
        None
    };
    let det_proton = if sourced(&out, "proton") {
        out.proton.clone()
    } else {
        None
    };
    let det_build = if sourced(&out, "build") {
        out.build.clone()
    } else {
        None
    };
    let det_ver = if sourced(&out, "exe_version") {
        out.exe_version.clone()
    } else {
        None
    };
    sqlx::query(
        "UPDATE games SET
            detected_exe_path = ?, detected_platform = ?, detected_bitness = ?, detected_api = ?,
            detected_extra_apis = ?,
            detected_engine = ?, detected_prefix_path = ?, detected_proton = ?, detected_build = ?,
            detected_exe_version = ?, fingerprint = ?
         WHERE id = ?",
    )
    .bind(det_exe)
    .bind(det_platform)
    .bind(det_bitness)
    .bind(det_api)
    .bind(det_extra)
    .bind(det_engine)
    .bind(det_pfx)
    .bind(det_proton)
    .bind(det_build)
    .bind(det_ver)
    .bind(&new_fp)
    .bind(id)
    .execute(pool)
    .await?;
    Ok(false)
}

pub(crate) fn sourced(out: &Detected, field: &str) -> bool {
    out.sources.contains_key(field)
}

pub async fn detect_all(pool: &SqlitePool, opts: DetectOpts) -> Result<()> {
    let ids: Vec<(String,)> = sqlx::query_as("SELECT id FROM games ORDER BY id")
        .fetch_all(pool)
        .await?;
    if opts.force {
        let mut all = Vec::new();
        for (id,) in &ids {
            if let Some(row) = fetch_row(pool, id).await? {
                for (k, v) in overrides_of(&row) {
                    all.push((format!("{id} {k}"), v));
                }
            }
        }
        let listed: Vec<(&str, String)> =
            all.iter().map(|(k, v)| (k.as_str(), v.clone())).collect();
        confirm_wipe("*", &listed, opts.yes)?;
        for (id,) in &ids {
            clear_overrides(pool, id).await?;
        }
    }
    let per = DetectOpts {
        force: opts.force,
        yes: true,
    };
    for (id,) in ids {
        let _ = detect_one(pool, &id, per).await?;
    }
    Ok(())
}

pub(crate) fn pick<'a>(
    ovr: &'a Option<String>,
    det: &'a Option<String>,
    store: &'a Option<String>,
) -> (&'a str, Option<&'static str>) {
    if let Some(s) = ovr.as_deref() {
        return (s, Some("override"));
    }
    if let Some(s) = det.as_deref() {
        return (s, Some("detected"));
    }
    if let Some(s) = store.as_deref() {
        return (s, Some("store"));
    }
    ("", None)
}

pub(crate) fn field(key: &'static str, value: &str, source: Option<&'static str>) -> DoctorField {
    DoctorField {
        key,
        value: value.to_string(),
        source,
    }
}

pub async fn doctor(pool: &SqlitePool, id: &str, opts: DetectOpts) -> Result<DoctorReport> {
    GameId::parse(id)?;
    let skipped = detect_one(pool, id, opts).await?;
    let row = fetch_row(pool, id)
        .await?
        .ok_or_else(|| Error::UnknownGame(id.into()))?;
    let (exe, exe_s) = pick(
        &row.override_exe_path,
        &row.detected_exe_path,
        &row.exe_path,
    );
    let (plat, plat_s) = pick(&row.override_platform, &row.detected_platform, &None);
    let (bit, bit_s) = pick(&row.override_bitness, &row.detected_bitness, &None);
    let (api, api_s) = pick(&row.override_api, &row.detected_api, &None);
    let (xapi, xapi_s) = pick(&row.override_extra_apis, &row.detected_extra_apis, &None);
    let (eng, eng_s) = pick(&row.override_engine, &row.detected_engine, &None);
    let (pfx, pfx_s) = pick(
        &row.override_prefix_path,
        &row.detected_prefix_path,
        &row.prefix_path,
    );
    let (pro, pro_s) = pick(&row.override_proton, &row.detected_proton, &row.proton);
    let (bld, bld_s) = pick(&row.override_build, &row.detected_build, &row.build);
    let (ver, ver_s) = pick(&row.override_exe_version, &row.detected_exe_version, &None);
    Ok(DoctorReport {
        id: row.id,
        skipped,
        fields: vec![
            field("id", id, None),
            field("platform", plat, plat_s),
            field("bitness", bit, bit_s),
            field("api", api, api_s),
            field("extra_apis", xapi, xapi_s),
            field("engine", eng, eng_s),
            field("exe", exe, exe_s),
            field("prefix", pfx, pfx_s),
            field("proton", pro, pro_s),
            field("build", bld, bld_s),
            field("exe_version", ver, ver_s),
            field(
                "launch_options",
                row.launch_options.as_deref().unwrap_or(""),
                Some("store"),
            ),
            field("env", row.env.as_deref().unwrap_or(""), Some("store")),
            field(
                "wrapper",
                row.wrapper.as_deref().unwrap_or(""),
                Some("store"),
            ),
        ]
        .into_boxed_slice(),
    })
}
