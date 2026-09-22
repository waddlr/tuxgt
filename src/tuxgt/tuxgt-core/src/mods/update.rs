use std::path::Path;

use sqlx::SqlitePool;

use super::*;
use crate::instance::Mod;
use crate::Result;

pub async fn check_update(
    data_dir: &Path,
    config_dir: &Path,
    game: &str,
    instance: &str,
) -> Result<UpdateStatus> {
    tracing::debug!(game, instance, "update check entry");
    let status = check_update_inner(data_dir, config_dir, game, instance).await?;
    tracing::info!(game, instance, status = update_kind(&status), "update checked");
    Ok(status)
}

fn update_kind(status: &UpdateStatus) -> &'static str {
    match status {
        UpdateStatus::UpToDate => "up-to-date",
        UpdateStatus::Available { .. } => "available",
        UpdateStatus::Unknown { .. } => "unknown",
    }
}

async fn check_update_inner(
    data_dir: &Path,
    config_dir: &Path,
    game: &str,
    instance: &str,
) -> Result<UpdateStatus> {
    let manifest = crate::need_manifest(data_dir, game, instance)?;
    let prov = &manifest.provenance;
    if prov.asset_sha256.is_empty() {
        return Ok(UpdateStatus::Unknown {
            reason: format!(
                "{game} {instance}: no install provenance recorded; re-run `tuxgt instance install --redownload {game} {instance}` to record it"
            ),
        });
    }
    let Ok(inst) = find_mod(config_dir, data_dir, instance) else {
        return Ok(UpdateStatus::Unknown {
            reason: format!("{game} {instance}: instance recipe not found"),
        });
    };
    if inst
        .sha256
        .as_deref()
        .is_some_and(|p| p == prov.asset_sha256)
    {
        return Ok(UpdateStatus::UpToDate);
    }
    match crate::instance::resolve_source(&inst) {
        crate::instance::SourceRef::Local { path } => {
            match crate::download::local_source_digest(Path::new(&path)) {
                Ok((sha, _)) if sha == prov.asset_sha256 => Ok(UpdateStatus::UpToDate),
                Ok((sha, _)) => Ok(UpdateStatus::Available {
                    detail: format!(
                        "{game} {instance}: local source changed (installed {}, now {})",
                        short_sha(&prov.asset_sha256),
                        short_sha(&sha)
                    ),
                }),
                Err(e) => Ok(UpdateStatus::Unknown {
                    reason: format!("{game} {instance}: cannot hash local source: {e}"),
                }),
            }
        }
        crate::instance::SourceRef::ManualUrl { url } => {
            if url != prov.source {
                return Ok(UpdateStatus::Available {
                    detail: format!("{game} {instance}: source URL moved"),
                });
            }
            cached_or_payload(data_dir, &inst, game, instance, &url, prov)
        }
        crate::instance::SourceRef::Github {
            owner,
            repo,
            asset_glob,
            tag,
            prerelease,
        } => {
            match crate::download::github_source_url(
                &owner,
                &repo,
                tag.as_deref(),
                prerelease,
                &asset_glob,
            )
            .await
            {
                Ok(url) if url != prov.source => Ok(UpdateStatus::Available {
                    detail: format!("{game} {instance}: new upstream asset {url}"),
                }),
                Ok(url) => cached_or_payload(data_dir, &inst, game, instance, &url, prov),
                Err(e) => Ok(UpdateStatus::Unknown {
                    reason: format!("{game} {instance}: cannot resolve upstream: {e}"),
                }),
            }
        }
    }
}

pub(crate) fn short_sha(sha: &str) -> &str {
    &sha[..sha.len().min(12)]
}

pub(crate) fn cached_or_payload(
    data_dir: &Path,
    inst: &Mod,
    game: &str,
    instance: &str,
    url: &str,
    prov: &crate::ModProvenance,
) -> Result<UpdateStatus> {
    let key = crate::download::url_key(url);
    match crate::download::cached_entry_digest(data_dir, &key)? {
        Some((_, sha, _)) if sha == prov.asset_sha256 => Ok(UpdateStatus::UpToDate),
        Some((_, sha, _)) => Ok(UpdateStatus::Available {
            detail: format!(
                "{game} {instance}: cached asset changed (installed {}, now {}); reinstall to update",
                short_sha(&prov.asset_sha256),
                short_sha(&sha)
            ),
        }),
        None => {
            let payload = crate::instance::payload_dir(
                data_dir,
                inst.official,
                inst.registry.as_deref(),
                &inst.id,
            );
            if dir_has_files(&payload) {
                Ok(UpdateStatus::UpToDate)
            } else {
                Ok(UpdateStatus::Unknown {
                    reason: format!(
                        "{game} {instance}: not in cache; run `tuxgt cache refresh {instance}` then re-check"
                    ),
                })
            }
        }
    }
}

/// E76 short GUI note for an `Unknown` update reason. The GUI Unknown line
/// carries only this fixed string: never a `tuxgt …` command, never the full
/// `manager:store:game` id. `check_update` keeps its long reasons for CLI.
pub fn short_update_reason(reason: &str) -> &'static str {
    if reason.contains("no install provenance")
        || reason.contains("recipe not found")
        || reason.contains("unknown instance")
        || reason.contains("no manifest")
    {
        "gui-mod-update-unknown-record"
    } else {
        "gui-mod-update-unknown-source"
    }
}

/// Outcome of [`ensure_update_baseline`]: the status to paint, plus whether a
/// repair install ran (the GUI then reloads its stage rows).
pub struct Baseline {
    pub status: UpdateStatus,
    pub installed: bool,
}

/// E76 GUI self-heal for the Mods update line (the GUI repair path; the CLI
/// keeps the long `check_update` reasons on stdout).
///
/// `check_update` stays read-only. Before (instead of) showing Unknown, this
/// helper repairs the baseline:
/// 1. Empty provenance + cache/depot/stage bytes: records `[provenance]`
///    from those bytes (no re-unpack), then `check_update`.
/// 2. Empty provenance + nothing to hash: `install_instance` with redownload
///    semantics. Foreign dests still return `Error::NeedConfirm` for the E34
///    confirm card.
/// 3. Cache miss + resolvable source: one background `acquire_with_source` (cache fill),
///    then `check_update`.
/// 4. Recipe gone / hash failure / network error after one try: returns the
///    `Unknown` for the GUI to paint as one short muted note; Update retries
///    the redownload path.
pub async fn ensure_update_baseline(
    pool: &SqlitePool,
    data_dir: &Path,
    config_dir: &Path,
    game: &str,
    instance: &str,
) -> Result<Baseline> {
    let manifest = crate::need_manifest(data_dir, game, instance)?;
    if manifest.provenance.asset_sha256.is_empty() {
        // Path 1 (best effort: no network, no re-unpack). Failures fall
        // through to `check_update`, which reports the honest Unknown.
        let _ = repair_provenance(data_dir, config_dir, game, instance).await;
        let manifest = crate::need_manifest(data_dir, game, instance)?;
        if manifest.provenance.asset_sha256.is_empty() {
            // Path 2: nothing on disk to hash — full repair install.
            let opts = InstallOpts {
                adapter: manifest.adapter.clone(),
                redownload: true,
                with_requires: None,
                yes: false,
                force: false,
            };
            install_instance(pool, data_dir, config_dir, game, instance, &opts, None).await?;
            let status = check_update(data_dir, config_dir, game, instance).await?;
            return Ok(Baseline {
                status,
                installed: true,
            });
        }
        let status = check_update(data_dir, config_dir, game, instance).await?;
        return Ok(Baseline {
            status,
            installed: false,
        });
    }
    match check_update(data_dir, config_dir, game, instance).await? {
        UpdateStatus::Unknown { reason } if reason.contains("not in cache") => {
            // Path 3: one cache fill, then re-check. A failed fill keeps the
            // original Unknown (path 4: one short muted note).
            if fill_update_cache(data_dir, config_dir, instance)
                .await
                .is_ok()
            {
                let status = check_update(data_dir, config_dir, game, instance).await?;
                Ok(Baseline {
                    status,
                    installed: false,
                })
            } else {
                Ok(Baseline {
                    status: UpdateStatus::Unknown { reason },
                    installed: false,
                })
            }
        }
        status => Ok(Baseline {
            status,
            installed: false,
        }),
    }
}
