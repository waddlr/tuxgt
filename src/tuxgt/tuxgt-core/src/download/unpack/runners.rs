use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

use super::{first_line, need_tool};
use crate::{Error, Result};

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

pub(crate) fn password_error(output: &[u8]) -> bool {
    let text = String::from_utf8_lossy(output).to_ascii_lowercase();
    [
        "wrong password",
        "incorrect password",
        "password is incorrect",
        "password incorrect",
        "can not open encrypted archive",
        "cannot open encrypted archive",
        "unable to get password",
    ]
    .iter()
    .any(|marker| text.contains(marker))
}

fn finish_extract(cmd: &str, output: std::process::Output, unzip: bool) -> Result<()> {
    if output.status.success() {
        return Ok(());
    }
    // Info-ZIP uses 82 for a bad password and may print nothing with `-q`.
    if (unzip && output.status.code() == Some(82))
        || password_error(&output.stderr)
        || password_error(&output.stdout)
    {
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
        "{cmd} exited {}{}",
        output.status,
        if detail.is_empty() {
            String::new()
        } else {
            format!(": {detail}")
        }
    )))
}

pub(crate) fn run_7z(asset: &Path, dest: &Path, password: Option<&str>) -> Result<()> {
    need_tool("7z")?;
    let mut cmd = Command::new("7z");
    cmd.args(["x", "-y"])
        .arg("-o.")
        .arg(asset)
        .current_dir(dest);
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
        cmd.stdin(Stdio::null());
        cmd.output()
            .map_err(|e| Error::Unpack(format!("7z: {e}")))?
    };
    finish_extract("7z", output, false)
}

/// `unzip` prompts on stdin unless `-P` is set. An empty `-P` still extracts
/// a normal zip and fails a password-protected one with exit 82.
fn run_unzip(asset: &Path, dest: &Path, password: Option<&str>) -> Result<()> {
    need_tool("zip")?;
    let a = asset.to_string_lossy().into_owned();
    let output = Command::new("unzip")
        .args(["-o", "-P", password.unwrap_or(""), &a, "-d", "."])
        .current_dir(dest)
        .stdin(Stdio::null())
        .output()
        .map_err(|e| Error::Unpack(format!("unzip: {e}")))?;
    finish_extract("unzip", output, true)
}

/// `-p-` tells unrar not to ask. A supplied password is one argument so
/// spaces stay inside it.
fn run_unrar(asset: &Path, dest: &Path, password: Option<&str>) -> Result<()> {
    need_tool("rar")?;
    let a = asset.to_string_lossy().into_owned();
    let pw = match password {
        Some(password) => format!("-p{password}"),
        None => "-p-".to_string(),
    };
    let output = Command::new("unrar")
        .args(["x", "-o+", &pw, &a, "."])
        .current_dir(dest)
        .stdin(Stdio::null())
        .output()
        .map_err(|e| Error::Unpack(format!("unrar: {e}")))?;
    finish_extract("unrar", output, false)
}

fn fallback_reader(failed: Error, read: Result<()>) -> Result<()> {
    match read {
        Err(Error::MissingTool(_)) => Err(failed),
        other => other,
    }
}

pub(crate) fn unpack_archive(asset: &Path, dest: &Path, password: Option<&str>) -> Result<()> {
    match classify(asset) {
        Archive::Zip => match run_7z(asset, dest, password) {
            Ok(()) => Ok(()),
            Err(Error::ArchivePasswordRequired) => Err(Error::ArchivePasswordRequired),
            Err(failed @ (Error::MissingTool(_) | Error::Unpack(_))) => {
                fallback_reader(failed, run_unzip(asset, dest, password))
            }
            Err(e) => Err(e),
        },
        Archive::Rar => match run_7z(asset, dest, password) {
            Ok(()) => Ok(()),
            Err(Error::ArchivePasswordRequired) => Err(Error::ArchivePasswordRequired),
            // 7z is missing, or this build cannot read RAR. unrar is the
            // other password-aware reader.
            Err(failed @ (Error::MissingTool(_) | Error::Unpack(_))) => {
                fallback_reader(failed, run_unrar(asset, dest, password))
            }
            Err(e) => Err(e),
        },
        Archive::SevenZ | Archive::Cab => run_7z(asset, dest, password),
        Archive::Plain | Archive::TarGz | Archive::TarBz2 | Archive::SelfExtract => {
            Err(Error::Unpack("not a 7z-family archive".into()))
        }
    }
}
