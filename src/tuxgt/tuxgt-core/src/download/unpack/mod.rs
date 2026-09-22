mod files;
mod runners;
mod tools;

pub(crate) use files::{collect_files, copy_plain, strip_single_top_dir};
#[cfg(test)]
pub(crate) use runners::password_error;
pub(crate) use runners::{classify, Archive};
pub use tools::{all_tools, tool_status, ExtTool, ToolStatus, EXT_TOOLS};
pub(crate) use tools::{first_line, need_tool, run};

use std::fs;
use std::path::{Path, PathBuf};

use super::copy_tree;
use crate::{Error, Result};
use runners::{run_7z, unpack_archive};

fn unpack_impl(asset: &Path, dest: &Path, password: Option<&str>) -> Result<Vec<PathBuf>> {
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
        Archive::TarGz | Archive::TarBz2 => {
            need_tool("tar")?;
            let a = asset.to_string_lossy().into_owned();
            run("tar", &["-xf", &a, "-C", "."], dest)?;
        }
        Archive::SelfExtract => {
            // 7z opens most self-extracting exes. When it cannot, stage
            // the installer as-is and return it without strip/collect.
            match run_7z(asset, dest, password) {
                Ok(()) => {}
                Err(Error::ArchivePasswordRequired) => return Err(Error::ArchivePasswordRequired),
                Err(e) => {
                    tracing::warn!(
                        asset = %asset.to_string_lossy(),
                        error = %e,
                        "self-extract failed; staging installer as-is"
                    );
                    let out = copy_plain(asset, dest)?;
                    tracing::info!(unpacked = out.len(), "unpacked");
                    return Ok(out);
                }
            }
        }
        Archive::Zip | Archive::Rar | Archive::SevenZ | Archive::Cab => {
            unpack_archive(asset, dest, password)?;
        }
    }
    strip_single_top_dir(dest)?;
    let mut out = Vec::new();
    collect_files(dest, &mut out)?;
    tracing::info!(unpacked = out.len(), "unpacked");
    Ok(out)
}

/// Unpack an archive. A missing or wrong password returns
/// [`Error::ArchivePasswordRequired`] instead of prompting on the terminal.
pub fn unpack(asset: &Path, dest: &Path) -> Result<Vec<PathBuf>> {
    unpack_with_password(asset, dest, None)
}

/// Same as [`unpack`], with the archive password when the caller has one.
/// `None` still refuses an interactive prompt.
pub fn unpack_with_password(
    asset: &Path,
    dest: &Path,
    password: Option<&str>,
) -> Result<Vec<PathBuf>> {
    unpack_impl(asset, dest, password)
}
