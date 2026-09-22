use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use sqlx::SqlitePool;

use super::*;
use crate::stage::{sync_staging, StageInput};
use crate::{
    apply_copies, generated_globs_for, other_claims, parse_mod_type, plan_copies, prewire_game,
    read_manifest, tracked_dests, write_manifest, Error, FileManifest, PlannedFile, Result,
};

pub async fn install_instance(
    pool: &SqlitePool,
    data_dir: &Path,
    config_dir: &Path,
    game: &str,
    instance_id: &str,
    opts: &InstallOpts,
    progress: crate::download::ProgressSink<'_>,
) -> Result<FileManifest> {
    tracing::debug!(game, instance = instance_id, adapter = opts.adapter.as_str(), redownload = opts.redownload, "install entry");
    let exists: Option<(String,)> = sqlx::query_as("SELECT id FROM games WHERE id = ?")
        .bind(game)
        .fetch_optional(pool)
        .await?;
    if exists.is_none() {
        return Err(Error::UnknownGame(game.into()));
    }
    let inst = find_mod(config_dir, data_dir, instance_id)?;
    if !inst.enabled {
        return Err(Error::InstanceDisabled(inst.id));
    }
    if !inst.allows_adapter(&opts.adapter) {
        return Err(Error::InvalidInstance(format!(
            "{} does not allow adapter {}",
            inst.id, opts.adapter
        )));
    }
    let mod_type = parse_mod_type(&inst.mod_type)?;
    let type_reqs = mod_type.requires().unwrap_or(&[]);
    for r in type_reqs {
        let satisfied = crate::game_manifests(data_dir, game)?
            .iter()
            .any(|m| m.mod_type == *r);
        if satisfied {
            continue;
        }
        match opts.with_requires.as_deref() {
            Some(other) => {
                let oi = find_mod(config_dir, data_dir, other)?;
                if !oi.enabled {
                    return Err(Error::InstanceDisabled(oi.id));
                }
                if oi.mod_type != *r {
                    return Err(Error::InvalidInstance(format!(
                        "{other} is {}, not required {r}",
                        oi.mod_type
                    )));
                }
                let mut nested = opts.clone();
                nested.with_requires = None;
                Box::pin(install_instance(
                    pool, data_dir, config_dir, game, other, &nested, progress,
                ))
                .await?;
            }
            None => return Err(Error::MissingRequires(r.to_string())),
        }
    }
    let mod_reqs = inst.requires.clone();
    for req in &mod_reqs {
        let satisfied = crate::game_manifests(data_dir, game)?
            .iter()
            .any(|m| m.instance == *req);
        if satisfied {
            continue;
        }
        match opts.with_requires.as_deref() {
            Some(other) => {
                let oi = find_mod(config_dir, data_dir, other)?;
                if !oi.enabled {
                    return Err(Error::InstanceDisabled(oi.id));
                }
                if oi.id != *req {
                    return Err(Error::InvalidInstance(format!(
                        "{other} is not required {req}"
                    )));
                }
                let mut nested = opts.clone();
                nested.with_requires = None;
                Box::pin(install_instance(
                    pool, data_dir, config_dir, game, other, &nested, progress,
                ))
                .await?;
            }
            None => return Err(Error::MissingRequires(req.clone())),
        }
    }
    let kind = crate::instance::mod_kind(inst.official, inst.registry.as_deref());
    let payload =
        crate::instance::payload_dir(data_dir, inst.official, inst.registry.as_deref(), &inst.id);
    let source_prefix = format!("mods/{kind}/{}", inst.id);
    tracing::debug!(game, instance = instance_id, "payload check");
    let (files, planned, provenance) = if dir_has_files(&payload) && !opts.redownload {
        let (files, planned) = land_from_payload(&payload, &source_prefix)?;
        let provenance = match crate::instance::resolve_source(&inst) {
            crate::instance::SourceRef::Local { path } => {
                let (sha, bytes) = crate::download::local_source_digest(Path::new(&path))?;
                let source = crate::download::local_key_src(Path::new(&path))?;
                let p = crate::ModProvenance {
                    source,
                    asset_sha256: sha,
                    asset_bytes: bytes,
                    fetched_at: crate::download::now_unix(),
                };
                crate::instance::write_payload_provenance(&payload, &p);
                p
            }
            _ => provenance_from_payload(&payload, &inst)?,
        };
        (files, planned, provenance)
    } else {
        let (asset, source) = crate::acquire_with_source(data_dir, &inst, opts.redownload, progress)
            .await
            .map_err(|e| {
                tracing::error!(game, instance = instance_id, stage = "acquire", error = %e, "install failed");
                e
            })?;
        let provenance = crate::ModProvenance {
            source,
            asset_sha256: asset.sha256.clone(),
            asset_bytes: asset.bytes,
            fetched_at: crate::download::now_unix(),
        };
        tracing::debug!(game, instance = instance_id, "unpack start");
        crate::instance::unpack_then_copy(&asset.file, &payload, opts.redownload).map_err(|e| {
            tracing::error!(game, instance = instance_id, stage = "unpack", error = %e, "install failed");
            e
        })?;
        crate::instance::strip_to_reshade_root_fs(&payload)?;
        let (files, planned) = land_from_payload(&payload, &source_prefix)?;
        crate::download::drop_download(data_dir, &asset.key);
        crate::instance::write_payload_provenance(&payload, &provenance);
        (files, planned, provenance)
    };
    let (arch, api) = game_arch_api(pool, game).await?;
    let dests_all: Vec<&str> = planned.iter().map(|f| f.dest.as_str()).collect();
    let keep = filter_payload(&inst.payload, &dests_all, &arch, &api);
    if !planned.is_empty() && !keep.iter().any(|k| *k) {
        return Err(Error::InvalidInstance(format!(
            "{} payload rules kept no files for {game}",
            inst.id
        )));
    }
    let (files, planned): (Vec<PathBuf>, Vec<PlannedFile>) = files
        .into_iter()
        .zip(planned)
        .enumerate()
        .filter(|(i, _)| keep[*i])
        .map(|(_, pair)| pair)
        .unzip();
    // Payload globs matched the raw archive paths above; recipe [dests], then
    // dest_root, then type dest rewrites (explicit dests win).
    let dest_root = mod_type.dest_root();
    let srcs: Vec<String> = planned.iter().map(|f| f.dest.clone()).collect();
    let remapped: Vec<bool> = srcs.iter().map(|s| inst.dests.contains_key(s)).collect();
    let mut dests: Vec<String> = srcs
        .iter()
        .map(|s| inst.dests.get(s).cloned().unwrap_or_else(|| s.clone()))
        .collect();
    for d in dests.iter_mut() {
        if !crate::install::is_prefix_dest(d) {
            *d = crate::modtype::dest_for(
                dest_root,
                d,
                inst.shader_dir.as_deref(),
                inst.texture_dir.as_deref(),
            );
        }
    }
    crate::modtype::apply_type_dests_except(&inst.mod_type, &mut dests, &remapped, &srcs)?;
    // E91: a recipe slot drives the claiming dest; without one the dxgi
    // type dest stands (explicit `[dests]` remaps still win).
    if inst.mod_type == "optiscaler" {
        if let Some(slot) = inst.slot.as_deref() {
            let want = crate::modtype::slot_dll(slot)?;
            for (i, (src, remap)) in srcs.iter().zip(remapped.iter()).enumerate() {
                if !remap && crate::modtype::is_optiscaler_dll(src) {
                    dests[i] = want.clone();
                }
            }
        }
    }
    for (i, a) in dests.iter().enumerate() {
        if dests[i + 1..].iter().any(|b| a == b) {
            return Err(Error::InvalidInstance(format!("duplicate dest: {a}")));
        }
    }
    crate::install::validate_prefix_dests(dests.iter().map(|s| s.as_str()), &opts.adapter)?;
    let prior = read_manifest(data_dir, game, &inst.id)?;
    let total_files = planned.len();
    let planned: Vec<PlannedFile> = planned
        .into_iter()
        .zip(dests)
        .map(|(mut f, dest)| {
            // Re-install matches the prior dest by source (E91): a slot
            // rename survives reinstall, a new source takes the computed
            // dest. A surviving source keeps its keep bit, and a dest the
            // type requires is never omitted.
            match prior
                .as_ref()
                .and_then(|p| p.files.iter().find(|o| o.source == f.source))
            {
                Some(p) => {
                    f.dest = p.dest.clone();
                    f.enabled = p.enabled
                        || crate::download::is_required_dest(
                            &inst.mod_type,
                            &f.dest,
                            &inst.include,
                            total_files,
                        );
                }
                None => {
                    f.dest = dest;
                }
            }
            f
        })
        .collect();
    let dests: Vec<&str> = planned.iter().map(|f| f.dest.as_str()).collect();
    let generated_globs = generated_globs_for(&inst.mod_type, &dests);
    // E74: snapshot recipe [env]; a surviving key keeps its keep bit, a new
    // key defaults on, a key gone from the recipe drops out.
    let planned_env: Vec<crate::PlannedEnv> = inst
        .env
        .iter()
        .map(|(k, v)| {
            let enabled = prior
                .as_ref()
                .and_then(|p| p.env.iter().find(|o| o.key == *k))
                .map(|o| o.enabled)
                .unwrap_or(true);
            crate::PlannedEnv {
                key: k.clone(),
                value: v.clone(),
                enabled,
            }
        })
        .collect();
    let load_order = match prior.as_ref() {
        Some(p) => p.load_order,
        None => crate::game_manifests(data_dir, game)
            .map(|ms| ms.iter().map(|m| m.load_order).max().unwrap_or(-1) + 1)
            .unwrap_or(0),
    };
    let mut manifest = FileManifest {
        game: game.into(),
        instance: inst.id.clone(),
        mod_type: inst.mod_type.clone(),
        adapter: opts.adapter.clone(),
        enabled: true,
        load_order,
        include: inst.include.clone(),
        files: planned.into_boxed_slice(),
        env: planned_env.into_boxed_slice(),
        backups: Default::default(),
        generated_globs: generated_globs.into_boxed_slice(),
        harvested: Default::default(),
        provenance,
    };
    // `gui.mod-config-edit`: per-game staged edits are first-class, so a
    // reinstall tolerates touches that predate it and continues to the
    // manifest write below. The pre-scan records dest → staged bytes: the
    // post-sync compare keys on byte identity, not on the (stale until the
    // manifest write) stage report. Fresh installs skip the scan (empty
    // map): the strict rule still catches hand-dropped files there.
    let pre_touched: BTreeMap<String, String> = if prior.is_some() {
        let sdir = crate::stage_dir(data_dir, game, &inst.id);
        let mut map = BTreeMap::new();
        for l in crate::stage_status(data_dir, game)? {
            if l.instance == inst.id && l.state == crate::StageState::UserModified {
                if let Ok(h) = crate::sha256_file(&sdir.join(&l.file)) {
                    map.insert(l.file, h);
                }
            }
        }
        map
    } else {
        BTreeMap::new()
    };
    // R35 P1: validate the prospective loader lists before staging or writing
    // the manifest — a rejected pack leaves no enabled residue behind.
    crate::prewire::ensure_ini_budget(pool, data_dir, game, &manifest).await?;
    let inputs: Vec<StageInput> = files
        .iter()
        .zip(manifest.files.iter())
        .filter(|(_, planned)| planned.enabled)
        .map(|(path, planned)| StageInput {
            rel: &planned.dest,
            src: path,
            sha: &planned.sha256,
        })
        .collect();
    tracing::debug!(game, instance = instance_id, "stage start");
    if let Err(e) = sync_staging(data_dir, game, &manifest.instance, &inputs, opts.force) {
        // Tolerate only pre-existing touches: every planned-enabled dest is
        // either synced to the fresh depot bytes or byte-identical to the
        // pre-scan. Anything else (a newly planned hand-drop, a touch that
        // changed mid-flight) still errors. The GUI re-reads stage pills
        // through `refresh_mod_extra`, so the preserved note needs no new
        // plumbing.
        let tolerable = matches!(e, Error::StagedModified(_)) && prior.is_some();
        let mut fresh = Vec::new();
        if tolerable {
            let sdir = crate::stage_dir(data_dir, game, &manifest.instance);
            for f in manifest.files.iter().filter(|f| f.enabled) {
                let staged = sdir.join(&f.dest);
                let current = staged.is_file().then(|| crate::sha256_file(&staged)).transpose()?;
                match current {
                    Some(h) if h == f.sha256 => {}
                    Some(h) if pre_touched.get(&f.dest).is_some_and(|pre| *pre == h) => {}
                    _ => fresh.push(f.dest.clone()),
                }
            }
        }
        if !tolerable || !fresh.is_empty() {
            tracing::error!(game, instance = instance_id, stage = "stage", error = %e, "install failed");
            return Err(e);
        }
        tracing::info!(game, instance = instance_id, "touched preserved, continuing");
    }
    write_manifest(data_dir, &manifest).map_err(|e| {
        tracing::error!(game, instance = instance_id, stage = "manifest", error = %e, "install failed");
        e
    })?;
    if opts.adapter == "install" {
        let root = crate::game_root(pool, game).await?;
        let prefix = crate::install::prefix_for(
            pool,
            game,
            manifest
                .files
                .iter()
                .filter(|f| f.enabled)
                .map(|f| f.dest.as_str()),
        )
        .await?;
        let tracked = tracked_dests(data_dir, game)?;
        let ops = plan_copies(
            data_dir,
            game,
            &manifest.instance,
            manifest.files.iter().filter(|f| f.enabled),
            &root,
            prefix.as_deref(),
            &tracked,
        )?;
        let need: Vec<String> = ops
            .iter()
            .filter(|o| o.needs_confirm)
            .map(|o| o.dest.clone())
            .collect();
        if !need.is_empty() && !opts.yes {
            return Err(Error::NeedConfirm(format!(
                "foreign game-dir dests: {}",
                need.join(", ")
            )));
        }
        tracing::debug!(game, instance = instance_id, "game-dir copy");
        apply_copies(
            data_dir,
            game,
            &mut manifest,
            &root,
            prefix.as_deref(),
            &ops,
            &tracked,
        )
        .map_err(|e| {
            tracing::error!(game, instance = instance_id, stage = "copy", error = %e, "install failed");
            e
        })?;
    }
    let _ = other_claims(data_dir, game, &manifest.instance)?;
    prewire_game(data_dir, pool, game).await.map_err(|e| {
        tracing::error!(game, instance = instance_id, stage = "prewire", error = %e, "install failed");
        e
    })?;
    tracing::info!(game, instance = instance_id, files = manifest.files.len(), "installed");
    Ok(manifest)
}
