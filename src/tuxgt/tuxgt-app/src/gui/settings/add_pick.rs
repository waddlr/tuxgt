use gpui_kit::*;
use std::path::PathBuf;

use super::*;
use tuxgt_core::{
    add_mod, config_dir, data_dir, is_dll, rescan_preview, scan_default_include, scan_package,
    FluentArgs,
};

use super::super::{rt_block, SettingsModsTab, Shell};

pub(crate) fn pick_package_with(
    view: Entity<Shell>,
    mod_type: &'static str,
    files: bool,
    directories: bool,
    prompt_key: &'static str,
    cx: &mut App,
) {
    pick_package_with_password(
        view,
        mod_type,
        files,
        directories,
        prompt_key,
        None,
        None,
        cx,
    );
}

pub(crate) fn pick_package_with_password(
    view: Entity<Shell>,
    mod_type: &'static str,
    files: bool,
    directories: bool,
    prompt_key: &'static str,
    password: Option<String>,
    selected_path: Option<PathBuf>,
    cx: &mut App,
) {
    // Lock 8: pick_add passes both flags only when the platform allows mixed
    // selection. No extension filter on the prompt. A password retry supplies
    // the already-selected path and skips the picker.
    let rx = selected_path.is_none().then(|| {
        let prompt = view.read(cx).strings.get(prompt_key);
        cx.prompt_for_paths(PathPromptOptions {
            files,
            directories,
            multiple: false,
            prompt: Some(prompt.into()),
        })
    });
    cx.spawn(async move |cx| {
        let path = if let Some(path) = selected_path {
            path
        } else {
            let Some(rx) = rx else {
                return;
            };
            let picked = rx.await.ok().and_then(|r| r.ok()).flatten();
            let Some(paths) = picked else {
                return;
            };
            let Some(path) = paths.into_iter().next() else {
                return;
            };
            path
        };
        let pending_path = path.clone();
        let result = cx
            .background_spawn(async move {
                rt_block(async {
                    // Classify first (E86 tree). A recipe imports directly
                    // (honoring its type via tab jump); anything else opens
                    // the tweak form with template defaults.
                    let mut cls =
                        tuxgt_core::classify_package_with_password(&path, password.as_deref())?;
                    let temps = std::mem::take(&mut cls.temps);
                    let drop_temps = |temps: Vec<PathBuf>| {
                        for t in &temps {
                            let _ = std::fs::remove_dir_all(t);
                        }
                    };
                    match cls.kind {
                        tuxgt_core::ClassifyKind::Recipe { file } => {
                            let inst = add_mod(&config_dir(), &file, &data_dir())?;
                            drop_temps(temps);
                            let rows = super::load_instances();
                            Ok::<_, tuxgt_core::Error>(Picked::Imported {
                                id: inst.id,
                                mod_type: inst.mod_type,
                                rows,
                            })
                        }
                        tuxgt_core::ClassifyKind::Single { file, .. } => {
                            let form_path = add_form_path(&path, &file, &temps);
                            if mod_type.is_empty() {
                                let scanned = scan_package(&file, "effect", &data_dir());
                                drop_temps(temps);
                                Ok(Picked::Form(scan_once_form(
                                    form_path,
                                    scanned?,
                                    password.clone(),
                                )))
                            } else {
                                let scanned = scan_package(&file, mod_type, &data_dir());
                                drop_temps(temps);
                                let scanned = scanned?;
                                Ok(Picked::Form(form_data(
                                    form_path,
                                    scanned,
                                    mod_type,
                                    password.clone(),
                                )))
                            }
                        }
                        tuxgt_core::ClassifyKind::Folder { dir } => {
                            let form_path = add_form_path(&path, &dir, &temps);
                            if mod_type.is_empty() {
                                let scanned = scan_package(&dir, "effect", &data_dir());
                                drop_temps(temps);
                                Ok(Picked::Form(scan_once_form(
                                    form_path,
                                    scanned?,
                                    password.clone(),
                                )))
                            } else {
                                let scanned = scan_package(&dir, mod_type, &data_dir());
                                drop_temps(temps);
                                let scanned = scanned?;
                                Ok(Picked::Form(form_data(
                                    form_path,
                                    scanned,
                                    mod_type,
                                    password.clone(),
                                )))
                            }
                        }
                    }
                })
            })
            .await;
        let _ = cx.update(|cx| {
            view.update(cx, |this, cx| {
                match result {
                    Ok(Picked::Imported { id, mod_type, rows }) => {
                        this.instances = rows.into_boxed_slice();
                        // Honor a recipe's Provides: jump to the owning inner tab.
                        this.mods_tab = SettingsModsTab::for_type(&mod_type);
                        this.persist_mods_tab();
                        this.refresh_selected_mods();
                        let mut args = FluentArgs::new();
                        args.set("id", id);
                        this.status = this
                            .strings
                            .get_args("gui-status-instance-added", Some(&args));
                    }
                    Ok(Picked::Form(form)) => {
                        let stem = form
                            .path
                            .file_stem()
                            .or_else(|| form.path.file_name())
                            .map(|s| s.to_string_lossy().into_owned())
                            .unwrap_or_else(|| "package".into());
                        let PickedForm {
                            path,
                            files,
                            mode_include,
                            requires,
                            mod_type,
                            password,
                        } = form;
                        let known_reshade: Vec<String> = this
                            .instances
                            .iter()
                            .filter(|i| i.mod_type == "reshade")
                            .map(|i| i.id.clone())
                            .collect();
                        let requires: Vec<String> = requires
                            .into_iter()
                            .filter(|r| known_reshade.iter().any(|k| k == r))
                            .collect();
                        let includes = initial_includes(&mod_type, &files, mode_include);
                        let select_visible = !files.is_empty() && files.iter().all(|x| x.keep);
                        let add_form = super::AddForm {
                            rescan_id: None,
                            password,
                            mod_type: mod_type.clone(),
                            path,
                            name: stem,
                            files: files.into_boxed_slice(),
                            select_visible,
                            includes: includes.into_boxed_slice(),
                            requires,
                        };
                        this.add_form = Some(cx.new(|_| add_form));
                        this.add_sync_queued = true;
                        if SettingsModsTab::for_type(&mod_type) == this.mods_tab {
                            this.scroll_page_top();
                        }
                    }
                    Err(tuxgt_core::Error::ArchivePasswordRequired) => {
                        this.pending_archive_password = Some(PendingArchivePassword {
                            path: pending_path,
                            mod_type,
                            files,
                            directories,
                            prompt_key,
                        });
                        this.scroll_page_top();
                    }
                    Err(e) => {
                        let mut args = FluentArgs::new();
                        args.set("error", e.to_string());
                        this.status = this.strings.get_args("gui-status-err-add", Some(&args));
                    }
                }
                cx.notify();
            });
        });
    })
    .detach();
}

/// What a background Add pick resolved to: an imported recipe, or a tweak
/// form prefilled from the tab's packaged template (lock 10).
pub(crate) enum Picked {
    Imported {
        id: String,
        mod_type: String,
        rows: Vec<super::InstanceRow>,
    },
    Form(PickedForm),
}

pub(crate) struct PickedForm {
    pub(crate) path: PathBuf,
    pub(crate) files: Vec<tuxgt_core::PackageFile>,
    pub(crate) mode_include: bool,
    pub(crate) requires: Vec<String>,
    pub(crate) mod_type: String,
    pub(crate) password: Option<String>,
}

/// Which path an Add form keeps after `classify_package`: archive picks
/// (non-empty classify temps) scan the unpacked temp dir, but the form keeps
/// the picked archive — temps are dropped right after the scan, so a temp
/// path would dangle at Save and seed the Name from `tuxgt-e88-classify-…`
/// instead of the archive stem. Plain picks keep the classified path.
pub(crate) fn add_form_path(
    picked: &std::path::Path,
    classified: &std::path::Path,
    temps: &[PathBuf],
) -> PathBuf {
    if temps.is_empty() {
        classified.to_path_buf()
    } else {
        picked.to_path_buf()
    }
}

/// Prefill one Add form from the tab's packaged template, or plain
/// defaults without a deployed share tree. Template Requires pre-check the
/// dropdown (filtered to installed ReShade Mods at open); nothing is ever
/// filled into a text field.
pub(crate) fn form_data(
    path: PathBuf,
    files: Vec<tuxgt_core::PackageFile>,
    mod_type: &str,
    password: Option<String>,
) -> PickedForm {
    // E63: family templates back the family Add flow, not the per-tab
    // prefill find (family-*.toml sorts before reshade-addon.toml).
    let template = tuxgt_core::list_templates(&tuxgt_core::data_dir())
        .unwrap_or_default()
        .into_iter()
        .find(|t| t.family.is_none() && t.mod_type == mod_type);
    let mode_include = template.as_ref().is_some_and(|t| t.mode == "include");
    let requires = template.map(|t| t.requires.to_vec()).unwrap_or_default();
    PickedForm {
        path,
        files,
        mode_include,
        requires,
        mod_type: mod_type.to_string(),
        password,
    }
}

/// E91: initial per-DLL Include flags. Template Include mode forces every
/// kept DLL to Include; otherwise companions default Include via core
/// `scan_default_include` (the claiming Load dest stays Load).
pub(crate) fn initial_includes(
    mod_type: &str,
    files: &[tuxgt_core::PackageFile],
    mode_include: bool,
) -> Vec<bool> {
    let defaults = scan_default_include(mod_type, files);
    files
        .iter()
        .map(|x| {
            (mode_include && x.keep && is_dll(&x.dest)) || defaults.iter().any(|d| d == &x.dest)
        })
        .collect()
}

pub(crate) fn start_rescan(view: Entity<Shell>, id: String, cx: &mut App) {
    cx.spawn(async move |cx| {
        let result = cx
            .background_spawn(async move {
                rt_block(async {
                    let (inst, files) = rescan_preview(&config_dir(), &id, &data_dir())?;
                    Ok::<_, tuxgt_core::Error>((inst, files))
                })
            })
            .await;
        let _ = cx.update(|cx| {
            view.update(cx, |this, cx| {
                match result {
                    Ok((inst, files)) => {
                        let path = match &inst.source {
                            tuxgt_core::SourceRef::Local { path } => std::path::PathBuf::from(path),
                            _ => std::path::PathBuf::new(),
                        };
                        let select_visible = !files.is_empty() && files.iter().all(|x| x.keep);
                        // Rescan seeds the mode flags from the recipe's
                        // `include` list: Save writes the form's dests back.
                        let includes = files
                            .iter()
                            .map(|x| inst.include.iter().any(|d| d == &x.dest))
                            .collect::<Vec<bool>>();
                        let rescan_tab = SettingsModsTab::for_type(&inst.mod_type);
                        let add_form = super::AddForm {
                            rescan_id: Some(inst.id.clone()),
                            password: None,
                            mod_type: inst.mod_type,
                            path,
                            name: inst.id,
                            files: files.into_boxed_slice(),
                            select_visible,
                            includes: includes.into_boxed_slice(),
                            requires: Vec::new(),
                        };
                        this.add_form = Some(cx.new(|_| add_form));
                        this.add_sync_queued = true;
                        if rescan_tab == this.mods_tab {
                            this.scroll_page_top();
                        }
                    }
                    Err(e) => {
                        let mut args = FluentArgs::new();
                        args.set("error", e.to_string());
                        this.status = this
                            .strings
                            .get_args("gui-status-err-instance", Some(&args));
                    }
                }
                cx.notify();
            });
        });
    })
    .detach();
}
