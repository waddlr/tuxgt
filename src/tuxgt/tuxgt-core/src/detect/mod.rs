use std::path::{Path, PathBuf};

use crate::game::GameId;

mod binary;
mod blackspace;
mod creation;
mod gpu;
mod mo2;
mod pe;
mod re_engine;
mod runtime;
mod unity;
mod unreal;

mod detected;
mod doctor;
mod snapshot;

pub use gpu::{host_gpu, host_gpu_from, HostGpu};
pub use runtime::steam_store;

pub(crate) use binary::{parse_binary, BinaryInfo};
use blackspace::BlackSpace;
use creation::Creation;
use mo2::Mo2;
use pe::Pe;
use re_engine::ReEngine;
use runtime::Runtime;
use unity::Unity;
use unreal::Unreal;

pub(crate) use snapshot::*;

pub use detected::{DetectCtx, Detected, Detector};
pub use doctor::{detect_all, detect_one, doctor, DoctorReport};
pub use snapshot::{
    detection_snapshot, set_override, validate_override, DetectOpts, DetectSnapshot,
    OVERRIDE_FIELDS,
};

pub(crate) const ENGINES: &[&dyn Detector] =
    &[&Mo2, &Unreal, &Unity, &ReEngine, &Creation, &BlackSpace];

pub(crate) fn is_helper_exe(p: &Path) -> bool {
    let Some(name) = p.file_name() else {
        return false;
    };
    let n = name.to_string_lossy().to_ascii_lowercase();
    n.starts_with("unins")
        || n.contains("uninstall")
        || n.contains("crashhandler")
        || n.contains("crashreport")
        || n.contains("vcredist")
        || n.contains("vc_redist")
        || n.contains("unitycrash")
        || n.contains("easyanticheat")
        || n.contains("eosbootstrapper")
        || n.contains("launcher")
        || n.contains("social-club")
        || n.contains("socialclub")
        || n.contains("physx")
        || n.contains("modorganizer")
        || n == "xvid.exe"
        || n.starts_with("xvid-")
        || n == "touchup.exe"
        || n.contains("overlay")
}

pub(crate) fn is_exe_file(p: &Path) -> bool {
    if !p.is_file() {
        return false;
    }
    let Some(name) = p.file_name() else {
        return false;
    };
    let n = name.to_string_lossy().to_ascii_lowercase();
    if n.ends_with(".exe") || n.ends_with(".x86_64") || n.ends_with(".x86") {
        return true;
    }
    if n.contains('.') {
        return false;
    }
    elf_magic(p)
}

pub(crate) fn elf_magic(p: &Path) -> bool {
    use std::io::Read;
    let mut buf = [0u8; 4];
    let Ok(mut f) = std::fs::File::open(p) else {
        return false;
    };
    f.read_exact(&mut buf).is_ok() && buf == *b"\x7fELF"
}

pub(crate) fn fallback_exe(dir: &Path) -> Option<PathBuf> {
    let mut best: Option<(u64, PathBuf)> = None;
    walk_exes(dir, 0, 3, &mut best);
    best.map(|(_, p)| p)
}

pub(crate) fn walk_exes(dir: &Path, depth: u32, max: u32, best: &mut Option<(u64, PathBuf)>) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for ent in rd.flatten() {
        let p = ent.path();
        if p.is_dir() {
            let skip_dir = p
                .file_name()
                .map(|s| {
                    let n = s.to_string_lossy().to_ascii_lowercase();
                    n == "__installer"
                        || n == "__overlay"
                        || n.starts_with("__")
                        || n == "_commonredist"
                        || n == "directx"
                        || n.starts_with("redist")
                })
                .unwrap_or(false);
            if !skip_dir
                && depth < max
                && !p
                    .symlink_metadata()
                    .map(|m| m.file_type().is_symlink())
                    .unwrap_or(true)
            {
                walk_exes(&p, depth + 1, max, best);
            }
            continue;
        }
        if !is_exe_file(&p) || is_helper_exe(&p) {
            continue;
        }
        let mut score = std::fs::metadata(&p).map(|m| m.len()).unwrap_or(0);
        let path_l = p.to_string_lossy().to_ascii_lowercase();
        if path_l.contains("win64") || path_l.contains("/bin/") || path_l.contains("binaries") {
            score = score.saturating_add(1 << 40);
        }
        match best {
            Some((s, _)) if *s >= score => {}
            _ => *best = Some((score, p)),
        }
    }
}

pub fn run_detectors(id: &GameId, install_dir: Option<&Path>, mut out: Detected) -> Detected {
    if out
        .exe_path
        .as_deref()
        .is_some_and(|p| is_helper_exe(p) || !p.is_file())
    {
        out.exe_path = None;
    }
    let ctx0 = DetectCtx {
        id,
        install_dir,
        binary: None,
    };
    for d in ENGINES {
        d.detect(&ctx0, &mut out);
    }
    if out
        .exe_path
        .as_deref()
        .is_some_and(|p| is_helper_exe(p) || !p.is_file())
    {
        out.exe_path = None;
    }
    if out.exe_path.is_none() {
        if let Some(dir) = install_dir {
            if let Some(exe) = fallback_exe(dir) {
                out.set_exe(exe, "runtime");
            }
        }
    }
    let parsed = out.exe_path.as_deref().and_then(parse_binary);
    let ctx = DetectCtx {
        id,
        install_dir,
        binary: parsed.as_ref(),
    };
    Pe.detect(&ctx, &mut out);
    for d in ENGINES {
        d.detect(&ctx, &mut out);
    }
    Runtime.detect(&ctx, &mut out);
    out
}

#[cfg(test)]
mod testing;
#[cfg(test)]
mod tests_0;
