use std::path::Path;
use std::process::Command;

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
