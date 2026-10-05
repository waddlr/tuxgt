//! R37: convert one game's installed instances between the two adapters as a
//! single all-or-nothing transaction.
//!
//! The persisted `games.adapter` choice and the per-instance
//! `FileManifest.adapter` must never disagree: launch, prewire, and the
//! install copies all read the manifest, so a choice change has to move
//! every manifest *and* the payload copies in the same operation, with the
//! database value written last. Every refusal (a recipe that disallows the
//! target, a foreign game-dir overwrite without consent, a missing
//! recipe, an unnamed proxy) happens before the first byte moves; every
//! failure after that restores manifests, copies, harvested-file moves,
//! backups, prewire, and the stored choice. Slot renames are the caller's:
//! they land before a `slots_chosen` replay, not inside this transaction.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use sqlx::SqlitePool;

use super::*;
use crate::game::{game_adapter, is_install, set_game_adapter, validate_adapter, GameId};
use crate::install::{backups_dir, resolve_target};
use crate::{
    apply_copies, game_root, plan_copies, prewire::managed_ini, prewire_game, remove_copies,
    tracked_dests, write_manifest, CopyOp, Error, FileManifest, Result,
};

/// File bytes, or `None` when the path did not exist.
fn read_optional(path: &Path) -> Result<Option<Vec<u8>>> {
    match fs::read(path) {
        Ok(b) => Ok(Some(b)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.into()),
    }
}

/// Pre-mutation twin of `set_staged`'s touch refusal: a staged rel whose
/// bytes differ from the recorded `staged_sha` (or have no record) is
/// user-touched, and the restage must neither drop nor overwrite it. Runs
/// in planning so the refusal lands before the first byte moves — and so
/// the rollback is never the thing that overwrites the touch.
fn refuse_touched_staging(data_dir: &Path, game: &str, instance: &str, rel: &str) -> Result<()> {
    let dest = crate::stage::stage_dir(data_dir, game, instance).join(rel);
    if !dest.is_file() {
        return Ok(());
    }
    let hash = crate::sha256_file(&dest)?;
    let recorded = crate::stage::staged_shas(data_dir, game, instance)?;
    if !recorded.get(rel).is_some_and(|s| *s == hash) {
        return Err(Error::StagedModified(format!("{game} {instance}: {rel}")));
    }
    Ok(())
}

/// What one conversion did. `instances` lists the manifests that moved;
/// instances already on the target adapter are not listed and not touched.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ConversionReport {
    pub from: String,
    pub to: String,
    pub instances: Vec<String>,
    /// True when the stored choice already matched, so nothing moved.
    pub unchanged: bool,
}

/// Injection points for the rollback tests: each arm fails after that step
/// has partially advanced, so the restore path is proven for a mid-flight
/// copy, a finished harvested-file move, a finished prewire, and a written
/// game column. Production builds compile every arm away.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum FailPoint {
    AfterFirstCopy,
    AfterHarvestedMove,
    AfterPrewrite,
    AfterGameColumn,
}

#[cfg(test)]
thread_local! {
    static ARMED: std::cell::Cell<Option<FailPoint>> = const { std::cell::Cell::new(None) };
}

/// Arm one injection point for the next conversion on this thread.
#[cfg(test)]
pub(crate) fn arm_fail(point: FailPoint) {
    ARMED.with(|a| a.set(Some(point)));
}

/// Clear the armed injection point.
#[cfg(test)]
pub(crate) fn disarm_fail() {
    ARMED.with(|a| a.set(None));
}

#[cfg(test)]
fn armed(point: FailPoint) -> bool {
    ARMED.with(|a| a.get() == Some(point))
}

#[cfg(not(test))]
fn armed(_point: FailPoint) -> bool {
    false
}

/// Everything the conversion can move, recorded before it moves: the
/// game-dir/prefix dests, the backup files, the manifests, the prewire ini,
/// and the stored choice.
struct Snapshot {
    /// Absolute dest path → pre-conversion bytes (`None` = absent).
    dests: BTreeMap<PathBuf, Option<Vec<u8>>>,
    /// Backups-dir relative path → pre-conversion bytes.
    backups: BTreeMap<PathBuf, Vec<u8>>,
    /// Manifest file → exact pre-conversion bytes (`None` = absent).
    manifests: BTreeMap<PathBuf, Option<Vec<u8>>>,
    ini: Option<Vec<u8>>,
    /// Resolved once up front, so the restore never re-parses a game id on
    /// the failure path.
    ini_path: PathBuf,
    adapter: String,
}

impl Snapshot {
    /// Record one instance: every dest the conversion can move and its
    /// manifest. The backup dir is walked once, on the first instance that
    /// actually moves.
    fn add_instance(
        &mut self,
        data_dir: &Path,
        game: &str,
        manifest: &FileManifest,
        root: &Path,
        prefix: Option<&Path>,
    ) -> Result<()> {
        for f in manifest.files.iter() {
            let target = resolve_target(root, prefix, &f.dest)?;
            if !self.dests.contains_key(&target) {
                self.dests.insert(target.clone(), read_optional(&target)?);
            }
        }
        let mpath = crate::download::manifest_path(data_dir, game, &manifest.instance);
        if !self.manifests.contains_key(&mpath) {
            self.manifests.insert(mpath.clone(), read_optional(&mpath)?);
        }
        Ok(())
    }

    /// Read the whole backup dir once, before the first copy can add to it.
    fn read_backups(&mut self, data_dir: &Path, game: &str) -> Result<()> {
        if !self.backups.is_empty() {
            return Ok(());
        }
        let bdir = backups_dir(data_dir, game);
        walk_files(&bdir, &bdir, &mut self.backups)
    }

    /// Undo every step the conversion made. Best effort per file, but it
    /// runs to completion and reports what it could not restore: a
    /// half-restored game dir must be visible in the log, never silently
    /// swallowed.
    async fn restore(&self, data_dir: &Path, game: &str, pool: &SqlitePool) -> Vec<String> {
        let mut problems = Vec::new();
        if let Err(e) = crate::game::restore_game_adapter(pool, game, &self.adapter).await {
            problems.push(format!("adapter restore: {e}"));
        }
        if let Err(e) = self.restore_bytes(self.ini.as_deref(), &self.ini_path) {
            problems.push(format!("prewire restore: {e}"));
        }
        let bdir = backups_dir(data_dir, game);
        for (rel, bytes) in &self.backups {
            if let Err(e) = self.restore_bytes(Some(bytes), &bdir.join(rel)) {
                problems.push(format!("backup restore {}: {e}", rel.display()));
            }
        }
        // A backup the failed run created has no snapshot entry: the restore
        // must delete it, or the dir keeps an orphan copy of user content
        // the restored manifest no longer references.
        let mut current = BTreeMap::new();
        if let Err(e) = walk_files(&bdir, &bdir, &mut current) {
            problems.push(format!("backup scan: {e}"));
        }
        for rel in current.keys() {
            if !self.backups.contains_key(rel) {
                if let Err(e) = self.restore_bytes(None, &bdir.join(rel)) {
                    problems.push(format!("backup drop {}: {e}", rel.display()));
                }
            }
        }
        for (path, bytes) in &self.dests {
            if let Err(e) = self.restore_bytes(bytes.as_deref(), path) {
                problems.push(format!("dest restore {}: {e}", path.display()));
            }
        }
        for (path, bytes) in &self.manifests {
            if let Err(e) = self.restore_bytes(bytes.as_deref(), path) {
                problems.push(format!("manifest restore {}: {e}", path.display()));
            }
        }
        problems
    }

    /// Write `bytes` back, or delete the file when it did not exist before.
    fn restore_bytes(&self, bytes: Option<&[u8]>, path: &Path) -> Result<()> {
        match bytes {
            Some(b) => crate::fs::atomic_write(path, b),
            None => match fs::remove_file(path) {
                Ok(()) => Ok(()),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
                Err(e) => Err(e.into()),
            },
        }
    }
}

/// Recursively collect regular files under `dir` as `rel → bytes`.
fn walk_files(dir: &Path, base: &Path, out: &mut BTreeMap<PathBuf, Vec<u8>>) -> Result<()> {
    if !dir.is_dir() {
        return Ok(());
    }
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            walk_files(&path, base, out)?;
        } else {
            let rel = path.strip_prefix(base).unwrap_or(&path).to_path_buf();
            out.insert(rel, fs::read(&path)?);
        }
    }
    Ok(())
}

/// Read-only prelude shared by [`convert_game_adapter`] and
/// [`validate_adapter_convert`]: every refusal that lands before the first
/// byte moves (a missing recipe, a recipe that disallows the target, an
/// unconsented foreign overwrite, an unstaged file, a missing prefix, an
/// unnamed proxy). Side-effect free: the no-moves case reports empty
/// without persisting the choice (the conversion persists it).
struct ConvertPlan {
    from: String,
    moving: Vec<FileManifest>,
    plans: BTreeMap<String, Vec<CopyOp>>,
    root: PathBuf,
    prefix: Option<PathBuf>,
    tracked: BTreeMap<String, String>,
    harvested: Vec<HarvestedMove>,
}

async fn plan_conversion(
    pool: &SqlitePool,
    data_dir: &Path,
    config_dir: &Path,
    game: &str,
    target: &str,
    yes: bool,
    slots_chosen: bool,
) -> Result<ConvertPlan> {
    let from = game_adapter(pool, game).await?;
    // Refuse before any mutation: an enabled instance's recipe that
    // disallows the target, or one that is gone entirely (its manifest
    // could never be re-derived). A disabled instance moves no files, so
    // it never vetoes; its manifest still follows the choice.
    let mut moving: Vec<FileManifest> = Vec::new();
    let mut blocked: Vec<String> = Vec::new();
    let mut missing: Vec<String> = Vec::new();
    // Slot renames are applied by the caller before this replay. `slots_chosen`
    // skips the ask so the replay moves bytes under the dests those picks left.
    let mut need_choice: Vec<String> = Vec::new();
    for m in crate::game_manifests(data_dir, game)? {
        if m.adapter == target {
            continue;
        }
        match find_mod(config_dir, data_dir, &m.instance) {
            Ok(inst) if inst.allows_adapter(target) => {
                if !slots_chosen
                    && super::slot::conversion_needs_prompt(
                        is_install(target),
                        &m.mod_type,
                        inst.slot.as_deref(),
                        &m.files,
                        &m.include,
                    )
                {
                    if let Some(idx) = super::slot::claiming_slot_index(&m.files, &m.include) {
                        refuse_touched_staging(data_dir, game, &m.instance, &m.files[idx].dest)?;
                    }
                    need_choice.push(m.instance.clone());
                }
                moving.push(m);
            }
            // A disabled instance moves no files, so a disallowing or
            // missing recipe must not veto the conversion. The slot
            // prompt above still applies while the recipe allows the
            // target; a disabled pick stays manifest-only.
            _ if !m.enabled => {
                moving.push(m);
            }
            Ok(inst) => blocked.push(format!(
                "{} (allows {})",
                m.instance,
                inst.plans_allowed
                    .iter()
                    .map(|p| p.as_str())
                    .collect::<Vec<_>>()
                    .join("|")
            )),
            Err(_) => missing.push(m.instance.clone()),
        }
    }
    if !missing.is_empty() {
        return Err(Error::InvalidInstance(format!(
            "adapter conversion blocked, recipe missing: {}",
            missing.join(", ")
        )));
    }
    if !blocked.is_empty() {
        return Err(Error::InvalidInstance(format!(
            "adapter conversion blocked, recipe disallows {target}: {}",
            blocked.join(", ")
        )));
    }
    if !need_choice.is_empty() {
        return Err(Error::NeedSlotChoice(format!(
            "need-slot: {}",
            need_choice.join(", ")
        )));
    }

    if moving.is_empty() {
        // No payload to move: report empty without persisting. The
        // conversion persists the choice below; the validator stays
        // side-effect free so callers can run it before a store stop.
        return Ok(ConvertPlan {
            from,
            moving: Vec::new(),
            plans: BTreeMap::new(),
            root: PathBuf::new(),
            prefix: None,
            tracked: BTreeMap::new(),
            harvested: Vec::new(),
        });
    }
    let root = game_root(pool, game).await?;
    let all_dests: Vec<String> = moving
        .iter()
        .flat_map(|m| m.files.iter().map(|f| f.dest.clone()))
        .collect();
    let prefix =
        crate::install::prefix_for(pool, game, all_dests.iter().map(String::as_str)).await?;
    let tracked = tracked_dests(data_dir, game)?;

    // Pre-plan every install-side copy, so the consent check and the
    // not-staged / prefix errors all land before the first byte moves.
    let mut plans: BTreeMap<String, Vec<CopyOp>> = BTreeMap::new();
    let mut need: BTreeSet<String> = BTreeSet::new();
    if is_install(target) {
        for m in moving.iter().filter(|m| m.enabled) {
            let ops = plan_copies(
                data_dir,
                game,
                &m.instance,
                m.files.iter().filter(|f| f.enabled),
                &root,
                prefix.as_deref(),
                &tracked,
            )?;
            need.extend(
                ops.iter()
                    .filter(|o| o.needs_confirm)
                    .map(|o| o.dest.clone()),
            );
            plans.insert(m.instance.clone(), ops);
        }
    }
    // Runtime-generated files live next to the running mod: the game dir on
    // install, `<game>/runtime/` on preload. Every moving manifest shares
    // the non-target adapter, so the from-root derives from the target, not
    // the stored choice. Clashes join the same consent set as foreign
    // overwrites, so one card names every overwrite the retry authorizes.
    let (from_root, to_root) = if is_install(target) {
        (crate::stage::runtime_dir(data_dir, game), root.clone())
    } else {
        (root.clone(), crate::stage::runtime_dir(data_dir, game))
    };
    let (mut harvested, clashes) = plan_harvested_moves(&from_root, &to_root, &moving)?;
    need.extend(clashes.iter().cloned());
    if !need.is_empty() && !yes {
        return Err(Error::NeedConfirm(format!(
            "adapter-convert: {}",
            need.into_iter().collect::<Vec<_>>().join(", ")
        )));
    }
    // Consented clashes become from-wins moves: the live side overwrites.
    harvested.extend(clashes.into_iter().map(|rel| HarvestedMove {
        src: from_root.join(&rel),
        dst: to_root.join(&rel),
        identical: false,
    }));
    Ok(ConvertPlan {
        from,
        moving,
        plans,
        root,
        prefix,
        tracked,
        harvested,
    })
}

/// R37: dry-run of [`convert_game_adapter`]: run every pre-write refusal
/// (missing/blocked recipes, unconsented overwrites, unstaged or prefix
/// errors) without moving a byte or persisting anything. The GUI runs this
/// before stopping the store client, so a refusal never stops (and
/// restarts) Steam. The conversion re-runs the same prelude after the
/// stop, so a recipe that vanishes in between still fails closed.
pub async fn validate_adapter_convert(
    pool: &SqlitePool,
    data_dir: &Path,
    config_dir: &Path,
    game: &str,
    target: &str,
    yes: bool,
    slots_chosen: bool,
) -> Result<()> {
    let target = validate_adapter(target)?.to_string();
    plan_conversion(pool, data_dir, config_dir, game, &target, yes, slots_chosen)
        .await
        .map(|_| ())
}

/// R37: move one game's installed instances to `target` and persist the
/// choice, or change nothing at all.
///
/// `yes` authorizes the install adapter's foreign game-dir overwrites (the
/// same consent the install path collects); without it, a conversion that
/// would overwrite untracked content on a protected stem fails closed with
/// `Error::NeedConfirm` naming the dests before the first write.
pub async fn convert_game_adapter(
    pool: &SqlitePool,
    data_dir: &Path,
    config_dir: &Path,
    game: &str,
    target: &str,
    yes: bool,
    slots_chosen: bool,
) -> Result<ConversionReport> {
    let target = validate_adapter(target)?.to_string();
    tracing::debug!(game, target = target.as_str(), "adapter convert entry");
    let ConvertPlan {
        from,
        moving,
        plans,
        root,
        prefix,
        tracked,
        harvested,
    } = plan_conversion(pool, data_dir, config_dir, game, &target, yes, slots_chosen).await?;

    if moving.is_empty() {
        // Nothing installed to move: the choice still persists, so a game
        // picked before its first Mod install is honored by that install.
        if from != target {
            set_game_adapter(pool, game, &target).await?;
        }
        return Ok(ConversionReport {
            unchanged: from == target,
            from,
            to: target,
            instances: Vec::new(),
        });
    }

    let mut snap = Snapshot {
        dests: BTreeMap::new(),
        backups: BTreeMap::new(),
        manifests: BTreeMap::new(),
        ini: None,
        ini_path: managed_ini(&crate::game::game_dir(data_dir, &GameId::parse(game)?)),
        adapter: from.clone(),
    };
    for m in &moving {
        snap.add_instance(data_dir, game, m, &root, prefix.as_deref())?;
    }
    for mv in &harvested {
        for path in [mv.src.clone(), mv.dst.clone()] {
            if !snap.dests.contains_key(&path) {
                snap.dests.insert(path.clone(), read_optional(&path)?);
            }
        }
    }
    snap.read_backups(data_dir, game)?;
    snap.ini = read_optional(&snap.ini_path.clone())?;

    if let Err(e) = apply_conversion(
        pool,
        data_dir,
        game,
        &target,
        &moving,
        &plans,
        &root,
        prefix.as_deref(),
        &tracked,
        &harvested,
    )
    .await
    {
        for p in snap.restore(data_dir, game, pool).await {
            tracing::error!(game, problem = %p, "adapter conversion rollback incomplete");
        }
        return Err(e);
    }
    tracing::info!(
        game,
        from = from.as_str(),
        to = target.as_str(),
        instances = moving.len(),
        "adapter converted"
    );
    Ok(ConversionReport {
        from,
        to: target,
        instances: moving.iter().map(|m| m.instance.clone()).collect(),
        unchanged: false,
    })
}

/// The mutating half: manifests (each with its copies), then the
/// harvested-file moves, then prewire, then the stored choice last. Any
/// error returns to the caller's restore.
async fn apply_conversion(
    pool: &SqlitePool,
    data_dir: &Path,
    game: &str,
    target: &str,
    moving: &[FileManifest],
    plans: &BTreeMap<String, Vec<CopyOp>>,
    root: &Path,
    prefix: Option<&Path>,
    tracked: &BTreeMap<String, String>,
    harvested: &[HarvestedMove],
) -> Result<()> {
    let to_install = is_install(target);
    for m in moving {
        let mut m = m.clone();
        if to_install {
            if m.enabled {
                let empty = Vec::new();
                let ops = plans.get(&m.instance).unwrap_or(&empty);
                apply_copies(data_dir, game, &mut m, root, prefix, ops, tracked)?;
            }
        } else {
            // Preload keeps no game-dir copies: restore what was backed up,
            // delete what TuxGT still owns, and drop the backup map so no
            // enabled manifest claims an adapter it does not have.
            remove_copies(
                data_dir,
                &mut m.backups,
                m.files.iter().map(|f| f.dest.as_str()),
                root,
                prefix,
                tracked,
            )?;
        }
        m.adapter = target.to_string();
        write_manifest(data_dir, &m)?;
        if armed(FailPoint::AfterFirstCopy) {
            return Err(Error::Install(format!(
                "adapter conversion failed after the first copy ({})",
                m.instance
            )));
        }
    }
    let from_root = if to_install {
        crate::stage::runtime_dir(data_dir, game)
    } else {
        root.to_path_buf()
    };
    apply_harvested_moves(&from_root, harvested)?;
    if armed(FailPoint::AfterHarvestedMove) {
        return Err(Error::Install(
            "adapter conversion failed after the harvested-file move".into(),
        ));
    }
    prewire_game(data_dir, pool, game).await?;
    if armed(FailPoint::AfterPrewrite) {
        return Err(Error::Install(
            "adapter conversion failed after prewire".into(),
        ));
    }
    set_game_adapter(pool, game, target).await?;
    if armed(FailPoint::AfterGameColumn) {
        return Err(Error::Install(
            "adapter conversion failed after the game column write".into(),
        ));
    }
    Ok(())
}

/// R37: the adapter a new install for this game should use. The persisted
/// choice is the default; a caller may pass an explicit validated override
/// (the CLI `--adapter` flag), which never changes the stored choice.
pub(crate) async fn resolve_install_adapter(
    pool: &SqlitePool,
    game: &str,
    override_adapter: Option<&str>,
) -> Result<String> {
    match override_adapter {
        Some(a) => Ok(validate_adapter(a)?.to_string()),
        None => game_adapter(pool, game).await,
    }
}
