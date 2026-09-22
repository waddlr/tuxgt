use std::path::Path;

use sqlx::SqlitePool;

use super::*;
use crate::Result;

pub async fn check_catalog_update(
    pool: &SqlitePool,
    data_dir: &Path,
    config_dir: &Path,
    mod_id: &str,
) -> Result<CatalogStatus> {
    tracing::debug!(source = mod_id, "catalog check entry");
    let status = check_catalog_update_inner(pool, data_dir, config_dir, mod_id).await?;
    tracing::info!(source = mod_id, status = catalog_kind(&status), "catalog checked");
    Ok(status)
}

fn catalog_kind(status: &CatalogStatus) -> &'static str {
    match status {
        CatalogStatus::UpToDate => "up-to-date",
        CatalogStatus::Available { .. } => "available",
        CatalogStatus::Unknown { .. } => "unknown",
    }
}

async fn check_catalog_update_inner(
    pool: &SqlitePool,
    data_dir: &Path,
    config_dir: &Path,
    mod_id: &str,
) -> Result<CatalogStatus> {
    let now = crate::download::now_unix() as i64;
    let Ok(inst) = find_mod(config_dir, data_dir, mod_id) else {
        let status = CatalogStatus::Unknown {
            reason: format!("{mod_id}: instance recipe not found"),
        };
        record_catalog_status(pool, mod_id, now, &status).await;
        return Ok(status);
    };
    if inst.sha256.as_deref().is_some_and(|p| !p.is_empty()) {
        let pin = inst.sha256.as_deref().unwrap_or_default();
        let payload = crate::instance::payload_dir(
            data_dir,
            inst.official,
            inst.registry.as_deref(),
            &inst.id,
        );
        let current = crate::instance::read_payload_provenance(&payload)
            .map(|p| p.asset_sha256)
            .unwrap_or_default();
        let status = if current == pin {
            CatalogStatus::UpToDate
        } else if current.is_empty() && !dir_has_files(&payload) {
            CatalogStatus::Unknown {
                reason: format!("{mod_id}: never downloaded"),
            }
        } else {
            CatalogStatus::Available {
                detail: format!("{mod_id}: pinned source differs"),
            }
        };
        record_catalog_status(pool, mod_id, now, &status).await;
        return Ok(status);
    }
    let status = match crate::instance::resolve_source(&inst) {
        crate::instance::SourceRef::Local { path } => {
            match crate::download::local_source_digest(Path::new(&path)) {
                Ok((sha, _)) => {
                    let payload = crate::instance::payload_dir(
                        data_dir,
                        inst.official,
                        inst.registry.as_deref(),
                        &inst.id,
                    );
                    let current = crate::instance::read_payload_provenance(&payload)
                        .map(|p| p.asset_sha256)
                        .unwrap_or_default();
                    if current.is_empty() {
                        CatalogStatus::Unknown {
                            reason: format!("{mod_id}: never downloaded"),
                        }
                    } else if sha == current {
                        CatalogStatus::UpToDate
                    } else {
                        CatalogStatus::Available {
                            detail: format!("{mod_id}: local source changed"),
                        }
                    }
                }
                Err(e) => CatalogStatus::Unknown {
                    reason: format!("{mod_id}: cannot hash local source: {e}"),
                },
            }
        }
        crate::instance::SourceRef::ManualUrl { url } => {
            let payload = crate::instance::payload_dir(
                data_dir,
                inst.official,
                inst.registry.as_deref(),
                &inst.id,
            );
            let prov = crate::instance::read_payload_provenance(&payload);
            let current = prov
                .as_ref()
                .map(|p| p.asset_sha256.clone())
                .unwrap_or_default();
            // R32 URL-moved signal (same as the github arm): the recipe URL
            // changed since the payload landed.
            if !current.is_empty() && prov.as_ref().is_some_and(|p| p.source != url) {
                CatalogStatus::Available {
                    detail: format!("{mod_id}: source URL moved"),
                }
            } else {
                let key = crate::download::url_key(&url);
                match crate::download::cached_entry_digest(data_dir, &key) {
                    Ok(Some((_, sha, _))) => {
                        if current.is_empty() {
                            CatalogStatus::Unknown {
                                reason: format!("{mod_id}: never downloaded"),
                            }
                        } else if sha == current {
                            CatalogStatus::UpToDate
                        } else {
                            CatalogStatus::Available {
                                detail: format!("{mod_id}: cached asset changed"),
                            }
                        }
                    }
                    Ok(None) => {
                        if dir_has_files(&payload) {
                            CatalogStatus::UpToDate
                        } else {
                            CatalogStatus::Unknown {
                                reason: format!("{mod_id}: never downloaded"),
                            }
                        }
                    }
                    Err(e) => CatalogStatus::Unknown {
                        reason: format!("{mod_id}: cannot read cache: {e}"),
                    },
                }
            }
        }
        crate::instance::SourceRef::Github {
            owner,
            repo,
            asset_glob,
            tag,
            prerelease,
        } => {
            let literal = !asset_glob.contains(['*', '?', '[']) && !prerelease;
            let url = if literal {
                crate::download::github_release_url(&owner, &repo, tag.as_deref(), &asset_glob)
            } else {
                match crate::download::github_source_url(
                    &owner,
                    &repo,
                    tag.as_deref(),
                    prerelease,
                    &asset_glob,
                )
                .await
                {
                    Ok(u) => u,
                    Err(e) => {
                        let status = CatalogStatus::Unknown {
                            reason: format!("{mod_id}: cannot resolve upstream: {e}"),
                        };
                        record_catalog_status(pool, mod_id, now, &status).await;
                        return Ok(status);
                    }
                }
            };
            let payload = crate::instance::payload_dir(
                data_dir,
                inst.official,
                inst.registry.as_deref(),
                &inst.id,
            );
            let prov = crate::instance::read_payload_provenance(&payload);
            let current = prov
                .as_ref()
                .map(|p| p.asset_sha256.clone())
                .unwrap_or_default();
            // R32 URL-moved signal: a glob that now resolves elsewhere is an
            // update even when the old bytes were never cached (steady state
            // keeps downloads/ empty).
            if !literal && !current.is_empty() && prov.as_ref().is_some_and(|p| p.source != url) {
                CatalogStatus::Available {
                    detail: format!("{mod_id}: new upstream asset {url}"),
                }
            } else {
                let key = crate::download::url_key(&url);
                match crate::download::cached_entry_digest(data_dir, &key) {
                    Ok(Some((_, sha, _))) => {
                        if current.is_empty() {
                            CatalogStatus::Unknown {
                                reason: format!("{mod_id}: never downloaded"),
                            }
                        } else if sha == current {
                            CatalogStatus::UpToDate
                        } else {
                            CatalogStatus::Available {
                                detail: format!("{mod_id}: new upstream asset {url}"),
                            }
                        }
                    }
                    Ok(None) => {
                        if dir_has_files(&payload) {
                            CatalogStatus::UpToDate
                        } else {
                            CatalogStatus::Unknown {
                                reason: format!("{mod_id}: never downloaded"),
                            }
                        }
                    }
                    Err(e) => CatalogStatus::Unknown {
                        reason: format!("{mod_id}: cannot read cache: {e}"),
                    },
                }
            }
        }
    };
    record_catalog_status(pool, mod_id, now, &status).await;
    Ok(status)
}

/// Update status of one installed instance against its current source (R32).
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum UpdateStatus {
    /// Installed bytes match the current source/cache.
    UpToDate,
    /// The source or cache moved on; reinstall to pick it up.
    Available { detail: String },
    /// No provenance recorded (pre-R32 manifest) or nothing to compare.
    Unknown { reason: String },
}
