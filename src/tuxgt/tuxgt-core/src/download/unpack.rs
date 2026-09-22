use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use super::*;
use crate::{Error, Result};

pub struct ExtTool {
    pub name: &'static str,
    pub probe: &'static [&'static str],
}

pub static EXT_TOOLS: &[(&str, ExtTool)] = &[
    (
        "zip",
        ExtTool {
            name: "unzip",
            probe: &["unzip", "-v"],
        },
    ),
    (
        "rar",
        ExtTool {
            name: "unrar",
            probe: &["unrar"],
        },
    ),
    (
        "7z",
        ExtTool {
            name: "7z",
            probe: &["7z"],
        },
    ),
    (
        "tar",
        ExtTool {
            name: "tar",
            probe: &["tar", "--version"],
        },
    ),
];

#[derive(Debug)]
pub struct ToolStatus {
    pub name: String,
    pub found: bool,
    pub version: String,
}

pub(crate) fn first_line(out: &[u8]) -> String {
    out.split(|b| *b == b'\n')
        .next()
        .map(|l| String::from_utf8_lossy(l).trim().to_string())
        .unwrap_or_default()
}

pub fn tool_status(tool: &ExtTool) -> ToolStatus {
    let mut cmd = Command::new(tool.probe[0]);
    cmd.args(&tool.probe[1..]);
    match cmd.output() {
        Ok(o)
            if o.status.success()
                || !first_line(&o.stdout).is_empty()
                || !first_line(&o.stderr).is_empty() =>
        {
            let v = first_line(&o.stdout);
            let v = if v.is_empty() {
                first_line(&o.stderr)
            } else {
                v
            };
            ToolStatus {
                name: tool.name.into(),
                found: true,
                version: v,
            }
        }
        _ => ToolStatus {
            name: tool.name.into(),
            found: false,
            version: String::new(),
        },
    }
}

pub fn all_tools() -> Vec<ToolStatus> {
    EXT_TOOLS.iter().map(|(_, t)| tool_status(t)).collect()
}

pub(crate) fn need_tool(kind: &str) -> Result<ToolStatus> {
    let tool = EXT_TOOLS
        .iter()
        .find(|(k, _)| *k == kind)
        .map(|(_, t)| t)
        .ok_or_else(|| Error::MissingTool(kind.into()))?;
    let st = tool_status(tool);
    if !st.found {
        return Err(Error::MissingTool(tool.name.into()));
    }
    Ok(st)
}

#[derive(Debug, Eq, PartialEq)]
pub(crate) enum Archive {
    Zip,
    Rar,
    SevenZ,
    Cab,
    TarGz,
    TarBz2,
    SelfExtract,
    Plain,
}

pub(crate) fn classify(file: &Path) -> Archive {
    let n = file.to_string_lossy().to_ascii_lowercase();
    if n.ends_with(".zip") {
        Archive::Zip
    } else if n.ends_with(".rar") {
        Archive::Rar
    } else if n.ends_with(".7z") {
        Archive::SevenZ
    } else if n.ends_with(".cab") {
        Archive::Cab
    } else if n.ends_with(".tar.gz") || n.ends_with(".tgz") {
        Archive::TarGz
    } else if n.ends_with(".tar.bz2") {
        Archive::TarBz2
    } else if n.ends_with(".exe") {
        Archive::SelfExtract
    } else {
        Archive::Plain
    }
}

pub(crate) fn run(cmd: &str, args: &[&str], dir: &Path) -> Result<()> {
    let st = Command::new(cmd)
        .args(args)
        .current_dir(dir)
        .status()
        .map_err(|e| Error::Unpack(format!("{cmd}: {e}")))?;
    if !st.success() {
        return Err(Error::Unpack(format!("{cmd} exited {st}")));
    }
    Ok(())
}

pub(crate) fn password_error(output: &[u8]) -> bool {
    let text = String::from_utf8_lossy(output).to_ascii_lowercase();
    [
        "wrong password",
        "incorrect password",
        "password is incorrect",
        "can not open encrypted archive",
    ]
    .iter()
    .any(|marker| text.contains(marker))
}

fn run_7z(asset: &Path, dest: &Path, password: Option<&str>) -> Result<()> {
    need_tool("7z")?;
    let mut cmd = Command::new("7z");
    cmd.args(["x", "-y"]).arg("-o.").arg(asset).current_dir(dest);
    let output = if let Some(password) = password {
        let mut child = cmd
            .stdin(Stdio::piped())
            // 7z listings can be large; only stderr is needed for errors.
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| Error::Unpack(format!("7z: {e}")))?;
        let Some(mut stdin) = child.stdin.take() else {
            let _ = child.kill();
            let _ = child.wait();
            return Err(Error::Unpack("7z: password input unavailable".into()));
        };
        if let Err(e) = stdin
            .write_all(password.as_bytes())
            .and_then(|_| stdin.write_all(b"\n"))
        {
            let _ = child.kill();
            let _ = child.wait();
            return Err(Error::Unpack(format!("7z password input: {e}")));
        }
        drop(stdin);
        child
            .wait_with_output()
            .map_err(|e| Error::Unpack(format!("7z: {e}")))?
    } else {
        // An empty -p makes encrypted archives fail instead of prompting.
        cmd.arg("-p");
        cmd.output()
            .map_err(|e| Error::Unpack(format!("7z: {e}")))?
    };
    if output.status.success() {
        return Ok(());
    }
    if password_error(&output.stderr) || password_error(&output.stdout) {
        return Err(Error::ArchivePasswordRequired);
    }
    let detail = {
        let stderr = first_line(&output.stderr);
        if stderr.is_empty() {
            first_line(&output.stdout)
        } else {
            stderr
        }
    };
    Err(Error::Unpack(format!(
        "7z exited {}{}",
        output.status,
        if detail.is_empty() {
            String::new()
        } else {
            format!(": {detail}")
        }
    )))
}

pub(crate) fn copy_plain(asset: &Path, dest: &Path) -> Result<Vec<PathBuf>> {
    let name = asset
        .file_name()
        .ok_or_else(|| Error::Unpack("no file name".into()))?;
    let out = dest.join(name);
    fs::copy(asset, &out)?;
    Ok(vec![out])
}

fn unpack_impl(
    asset: &Path,
    dest: &Path,
    password: Option<&str>,
    use_7z: bool,
) -> Result<Vec<PathBuf>> {
    fs::create_dir_all(dest)?;
    if asset.is_dir() {
        copy_tree(asset, dest)?;
        strip_single_top_dir(dest)?;
        let mut out = Vec::new();
        collect_files(dest, &mut out)?;
        tracing::info!(unpacked = out.len(), "unpacked");
        return Ok(out);
    }
    match classify(asset) {
        Archive::Plain => {
            let out = copy_plain(asset, dest)?;
            tracing::info!(unpacked = out.len(), "unpacked");
            return Ok(out);
        }
        Archive::SelfExtract if use_7z => match run_7z(asset, dest, password) {
            Ok(()) => {}
            Err(Error::ArchivePasswordRequired) => return Err(Error::ArchivePasswordRequired),
            Err(e) => {
                tracing::warn!(
                    asset = %asset.to_string_lossy(),
                    error = %e,
                    "self-extract failed; staging installer as-is"
                );
                return copy_plain(asset, dest);
            }
        },
        Archive::Zip | Archive::Rar | Archive::SevenZ | Archive::Cab if use_7z => {
            run_7z(asset, dest, password)?;
        }
        Archive::SelfExtract => {
            let a = asset.to_string_lossy().into_owned();
            match need_tool("7z").and_then(|_| run("7z", &["x", &a, "-o.", "-y"], dest)) {
                Ok(()) => {}
                Err(e) => {
                    tracing::warn!(asset = %a, error = %e, "self-extract failed; staging installer as-is");
                    return copy_plain(asset, dest);
                }
            }
        }
        Archive::Zip => {
            need_tool("zip")?;
            let a = asset.to_string_lossy().into_owned();
            run("unzip", &["-q", "-o", &a, "-d", "."], dest)?;
        }
        Archive::Rar => {
            need_tool("rar")?;
            let a = asset.to_string_lossy().into_owned();
            run("unrar", &["x", "-o+", &a, "."], dest)?;
        }
        Archive::SevenZ | Archive::Cab => {
            need_tool("7z")?;
            let a = asset.to_string_lossy().into_owned();
            run("7z", &["x", &a, "-o.", "-y"], dest)?;
        }
        Archive::TarGz | Archive::TarBz2 => {
            need_tool("tar")?;
            let a = asset.to_string_lossy().into_owned();
            run("tar", &["-xf", &a, "-C", "."], dest)?;
        }
    }
    strip_single_top_dir(dest)?;
    let mut out = Vec::new();
    collect_files(dest, &mut out)?;
    tracing::info!(unpacked = out.len(), "unpacked");
    Ok(out)
}

/// Unpack using the existing per-format tools. This is the shared path for
/// downloads and non-Add callers.
pub fn unpack(asset: &Path, dest: &Path) -> Result<Vec<PathBuf>> {
    unpack_impl(asset, dest, None, false)
}

/// Unpack a local Add package through 7z so ZIP, RAR, and 7z can share one
/// password-aware path. `-p` with no value prevents an interactive prompt.
pub fn unpack_with_password(
    asset: &Path,
    dest: &Path,
    password: Option<&str>,
) -> Result<Vec<PathBuf>> {
    let result = unpack_impl(asset, dest, password, true);
    if password.is_none()
        && matches!(&result, Err(Error::MissingTool(tool)) if tool == "7z")
    {
        return unpack_impl(asset, dest, None, false);
    }
    result
}


pub(crate) fn strip_single_top_dir(dest: &Path) -> Result<()> {
    let entries: Vec<PathBuf> = fs::read_dir(dest)?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .collect();
    if entries.len() == 1 && entries[0].is_dir() {
        let tmp = dest.with_extension("strip");
        let _ = fs::remove_dir_all(&tmp);
        fs::rename(&entries[0], &tmp)?;
        for e in fs::read_dir(&tmp)? {
            let e = e?;
            fs::rename(e.path(), dest.join(e.file_name()))?;
        }
        fs::remove_dir_all(&tmp)?;
    }
    Ok(())
}

pub(crate) fn collect_files(dir: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    let mut entries: Vec<PathBuf> = fs::read_dir(dir)?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .collect();
    entries.sort();
    for e in entries {
        if e.is_dir() {
            collect_files(&e, out)?;
        } else if e.file_name().is_some_and(|n| n != ".provenance.toml") {
            out.push(e);
        }
    }
    Ok(())
}
