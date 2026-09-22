use crate::instance::PayloadRule;
use crate::testing::{seed_game, SeedGame};
use sqlx::SqlitePool;
use std::fs;
use std::path::PathBuf;

pub(crate) fn rule(
    arch: Option<&str>,
    api: Option<&str>,
    keep: &[&str],
    drop: &[&str],
) -> PayloadRule {
    PayloadRule {
        arch: arch.map(str::to_string),
        api: api.map(str::to_string),
        keep: keep.iter().map(|s| s.to_string()).collect(),
        drop: drop.iter().map(|s| s.to_string()).collect(),
    }
}

pub(crate) fn nanos() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos()
}

pub(crate) async fn local_game(
    tag: &str,
    payload: &[u8],
) -> (PathBuf, PathBuf, PathBuf, SqlitePool, String, String) {
    let dir = std::env::temp_dir().join(format!(
        "tuxgt-r32-{tag}-{}-{}",
        std::process::id(),
        nanos()
    ));
    let _ = fs::remove_dir_all(&dir);
    let data = dir.join("data");
    let cfg = dir.join("config");
    let pkg = dir.join("pkg");
    fs::create_dir_all(&cfg).unwrap();
    fs::create_dir_all(&pkg).unwrap();
    fs::write(pkg.join("plug.dll"), payload).unwrap();
    let pool = crate::open_db(&data).await.unwrap();
    let gid = format!("manual:standalone:r32{tag}");
    seed_game(
        &pool,
        SeedGame {
            id: &gid,
            name: Some("R32"),
            ..Default::default()
        },
    )
    .await;
    let iid = format!("r32plug{tag}");
    crate::add_mod_from(
        &cfg,
        "custom",
        &iid,
        &pkg,
        None,
        None,
        &data,
        Vec::new(),
        Vec::new(),
    )
    .unwrap();
    (dir, data, cfg, pool, gid, iid)
}

pub(crate) struct Fx {
    pub(crate) dir: std::path::PathBuf,
    pub(crate) data: std::path::PathBuf,
    pub(crate) pool: sqlx::SqlitePool,
    pub(crate) gid: String,
    pub(crate) iid: String,
}

/// Over-budget preload game, seeded directly (bypassing the install-time
/// prospective check): `n_dll` long-named root DLLs plus `n_root` long-named
/// root `.fx` files on a `reshade_addon` manifest (both omittable there),
/// plus `extra` dests.
pub(crate) async fn seed(tag: &str, n_dll: usize, n_root: usize, extra: &[(&str, bool)]) -> Fx {
    let dir = std::env::temp_dir().join(format!(
        "tuxgt-overbudget-{tag}-{}-{}",
        std::process::id(),
        nanos()
    ));
    let _ = fs::remove_dir_all(&dir);
    let data = dir.join("data");
    let gid = format!("manual:standalone:{tag}");
    let iid = "bigpack";
    let pool = crate::open_db(&data).await.unwrap();
    let exe = data.join("game.exe");
    fs::create_dir_all(&data).unwrap();
    fs::write(&exe, b"MZ").unwrap();
    seed_game(
        &pool,
        SeedGame {
            id: &gid,
            manager: "manual",
            store: "standalone",
            game_id: tag,
            name: Some("t"),
            exe_path: Some(exe.to_str().unwrap()),
            detected_api: Some("dx12"),
            detected_bitness: Some("64"),
            ..Default::default()
        },
    )
    .await;
    let depot = data.join("mods").join("user").join(iid);
    fs::create_dir_all(&depot).unwrap();
    let mut files = Vec::new();
    let mut rels: Vec<String> = (0..n_dll)
        .map(|i| format!("very-long-dll-name-{i:03}-{tag}-over-budget.dll"))
        .collect();
    rels.extend((0..n_root).map(|i| format!("very-long-root-name-{i:03}-{tag}-over-budget.fx")));
    rels.extend(extra.iter().map(|(d, _)| d.to_string()));
    for rel in &rels {
        let p = depot.join(rel);
        if let Some(parent) = p.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(&p, rel.as_bytes()).unwrap();
        let on = extra
            .iter()
            .find(|(d, _)| d == rel)
            .map(|(_, on)| *on)
            .unwrap_or(true);
        files.push(crate::PlannedFile {
            source: format!("mods/user/{iid}/{rel}"),
            dest: rel.clone(),
            sha256: crate::sha256_file(&p).unwrap(),
            enabled: on,
        });
    }
    let m = crate::FileManifest {
        game: gid.clone(),
        instance: iid.into(),
        mod_type: "reshade_addon".into(),
        adapter: "preload".into(),
        enabled: true,
        load_order: 0,
        include: Box::default(),
        files: files.into_boxed_slice(),
        env: Box::default(),
        backups: Default::default(),
        generated_globs: Box::default(),
        harvested: Default::default(),
        provenance: crate::ModProvenance::default(),
    };
    let srcs: Vec<std::path::PathBuf> = m
        .files
        .iter()
        .filter(|f| f.enabled)
        .map(|f| depot.join(&f.dest))
        .collect();
    let staged: Vec<crate::StageInput> = m
        .files
        .iter()
        .filter(|f| f.enabled)
        .zip(srcs.iter())
        .map(|(f, src)| crate::StageInput {
            rel: &f.dest,
            src,
            sha: &f.sha256,
        })
        .collect();
    crate::sync_staging(&data, &gid, iid, &staged, false).unwrap();
    crate::write_manifest(&data, &m).unwrap();
    // Sanity: some list is over budget, or the test proves nothing.
    let ms = crate::game_manifests(&data, &gid).unwrap();
    let lens = crate::prewire::list_lens(&crate::prewire::body_for(&ms));
    assert!(
        lens[0] >= 8192 || lens[1] >= 8192,
        "no list over budget: {lens:?}"
    );
    Fx {
        dir,
        data,
        pool,
        gid,
        iid: iid.into(),
    }
}

pub(crate) fn dll(i: usize, tag: &str) -> String {
    format!("very-long-dll-name-{i:03}-{tag}-over-budget.dll")
}

pub(crate) fn root(i: usize, tag: &str) -> String {
    format!("very-long-root-name-{i:03}-{tag}-over-budget.fx")
}
