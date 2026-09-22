//! Binary `shortcuts.vdf` `LaunchOptions` surgery for `steam:standalone:*` rows.
//!
//! Byte map (Valve binary KeyValues; tags confirmed against `steamlocate`'s
//! shortcut parser and `hidden.rs`'s `IsHidden` scan):
//! - the file is a node sequence closed by `0x08`.
//! - node = kind byte + NUL-terminated key, then by kind: `0x00` object
//!   (children, closed by `0x08`), `0x01` string (NUL-terminated value),
//!   `0x02` int (LE u32). Any other kind is refused with its offset.
//! - the root object `"shortcuts"` holds one object per shortcut (`"0"`,
//!   `"1"`, ...), each with an `appid` int child plus string children
//!   (`AppName`, `Exe`, `StartDir`, `LaunchOptions`) and int flags.
//! Strings carry no length prefix, so an edit splices value bytes at the
//! node's own offset and every other byte is copied verbatim.
//!
//! No new binary-VDF dependency on purpose: full-tree crates
//! (`steam_shortcuts_util`, `new-vdf-parser`) re-serialize the whole file and
//! cannot promise sibling-entry byte preservation (`new-vdf-parser` is also
//! LGPL-2.1). This module touches only the one string field.

use std::ops::Range;

use super::*;
use crate::{Error, Result};

const KIND_OBJ: u8 = 0;
const KIND_STR: u8 = 1;
const KIND_INT: u8 = 2;
const KIND_END: u8 = 8;
const OPTIONS_KEY: &str = "LaunchOptions";

enum Val {
    /// Children plus the offset of this object's closing `0x08`.
    Obj(Vec<Node>, usize),
    Str(Range<usize>),
    Int(u32),
}

struct Node {
    kind: u8,
    /// Key bytes, NUL excluded.
    key: Range<usize>,
    val: Val,
    /// Kind byte through value end, so a whole field can be spliced out.
    span: Range<usize>,
}

impl Node {
    fn key_is(&self, bytes: &[u8], name: &str) -> bool {
        let key = bytes.get(self.key.clone()).unwrap_or_default();
        let want = name.as_bytes();
        key.len() == want.len() && key.eq_ignore_ascii_case(want)
    }
}

/// Where one shortcut's `LaunchOptions` lives in a blob.
struct Hit {
    /// Value bytes of the existing field; `None` when Steam holds none.
    options: Option<Range<usize>>,
    /// The field's whole node span, needed to splice it out on restore.
    field: Option<Range<usize>>,
    /// The entry object's closing `0x08`: where a new field is appended.
    end: usize,
}

fn malformed(off: usize, detail: &str) -> Error {
    Error::Apply(format!(
        "shortcuts.vdf is malformed at offset {off}: {detail}"
    ))
}

fn read_cstr(bytes: &[u8], off: &mut usize) -> Result<Range<usize>> {
    let start = *off;
    let Some(len) = bytes[start..].iter().position(|b| *b == 0) else {
        return Err(malformed(start, "unterminated string"));
    };
    *off = start + len + 1;
    Ok(start..start + len)
}

/// Nodes until `0x08`; a nested object must close, the top-level sequence
/// may also end with the file.
fn parse_nodes(bytes: &[u8], off: &mut usize, nested: bool) -> Result<Vec<Node>> {
    let mut out = Vec::new();
    loop {
        let start = *off;
        let Some(&kind) = bytes.get(start) else {
            return if nested {
                Err(malformed(start, "truncated object"))
            } else {
                Ok(out)
            };
        };
        *off = start + 1;
        if kind == KIND_END {
            return Ok(out);
        }
        if kind != KIND_OBJ && kind != KIND_STR && kind != KIND_INT {
            return Err(malformed(start, "unexpected kind byte"));
        }
        let key = read_cstr(bytes, off)?;
        let val = match kind {
            KIND_OBJ => {
                let children = parse_nodes(bytes, off, true)?;
                Val::Obj(children, *off - 1)
            }
            KIND_STR => {
                let vstart = *off;
                let Some(len) = bytes[vstart..].iter().position(|b| *b == 0) else {
                    return Err(malformed(vstart, "unterminated value"));
                };
                *off = vstart + len + 1;
                Val::Str(vstart..vstart + len)
            }
            _ => {
                let Some(raw) = bytes.get(*off..*off + 4) else {
                    return Err(malformed(*off, "truncated int value"));
                };
                let mut b = [0u8; 4];
                b.copy_from_slice(raw);
                *off += 4;
                Val::Int(u32::from_le_bytes(b))
            }
        };
        let span = start..*off;
        out.push(Node {
            kind,
            key,
            val,
            span,
        });
    }
}

fn parse(bytes: &[u8]) -> Result<Vec<Node>> {
    parse_nodes(bytes, &mut 0, false)
}

fn children<'a>(node: &'a Node) -> &'a [Node] {
    match &node.val {
        Val::Obj(c, _) => c,
        _ => &[],
    }
}

/// Steam matches its own keys case-insensitively, so do we.
fn field<'a>(bytes: &'a [u8], node: &'a Node, name: &str) -> Option<&'a Node> {
    children(node).iter().find(|c| c.key_is(bytes, name))
}

fn text(bytes: &[u8], val: &Range<usize>) -> String {
    bytes
        .get(val.clone())
        .map_or_else(String::new, |b| String::from_utf8_lossy(b).into_owned())
}

fn encode_options_field(options: &str) -> Vec<u8> {
    let mut out = vec![KIND_STR];
    out.extend_from_slice(OPTIONS_KEY.as_bytes());
    out.push(0);
    out.extend_from_slice(options.as_bytes());
    out.push(0);
    out
}

fn hit_of(bytes: &[u8], node: &Node) -> Result<Hit> {
    let Val::Obj(_, end) = &node.val else {
        return Err(malformed(node.span.start, "entry is not an object"));
    };
    // A `LaunchOptions` of the wrong kind is corrupt input, not an absent
    // field: refuse rather than append a second one Steam would misread.
    let (options, field) = match field(bytes, node, OPTIONS_KEY) {
        Some(f) => match &f.val {
            Val::Str(val) => (Some(val.clone()), Some(f.span.clone())),
            _ => return Err(malformed(f.span.start, "LaunchOptions is not a string")),
        },
        None => (None, None),
    };
    Ok(Hit {
        options,
        field,
        end: *end,
    })
}

/// Locate one entry by appid. Entry objects sit under the root `"shortcuts"`
/// wrapper, so the search descends; two entries sharing an appid is corrupt
/// and refused rather than picked between.
fn find_entry(bytes: &[u8], nodes: &[Node], appid: u32, out: &mut Option<Hit>) -> Result<()> {
    for n in nodes {
        if n.kind != KIND_OBJ {
            continue;
        }
        let id = match field(bytes, n, "appid") {
            Some(f) => match f.val {
                Val::Int(v) => Some(v),
                _ => None,
            },
            None => None,
        };
        if id == Some(appid) {
            if out.replace(hit_of(bytes, n)?).is_some() {
                return Err(malformed(n.span.start, "duplicate shortcut appid"));
            }
        } else if let Val::Obj(c, _) = &n.val {
            find_entry(bytes, c, appid, out)?;
        }
    }
    Ok(())
}

fn locate(bytes: &[u8], appid: u32) -> Result<Option<Hit>> {
    let nodes = parse(bytes)?;
    let mut out = None;
    find_entry(bytes, &nodes, appid, &mut out)?;
    Ok(out)
}

/// This shortcut's `LaunchOptions`, or `None` when Steam holds none or the
/// blob is malformed: the About reference reads, it never rewrites.
pub(crate) fn shortcut_get_options(bytes: &[u8], appid: u32) -> Option<String> {
    let val = locate(bytes, appid).ok()??.options?;
    Some(text(bytes, &val))
}

/// Set one shortcut's `LaunchOptions` byte-surgically. Returns the new blob
/// and the previous value (`None` = the field was absent). An unknown appid
/// or a malformed blob errors with nothing written: Steam owns shortcut ids,
/// so we never invent an entry.
pub(crate) fn shortcut_set_options(
    bytes: &[u8],
    appid: u32,
    options: &str,
) -> Result<(Vec<u8>, Option<String>)> {
    let hit = locate(bytes, appid)?
        .ok_or_else(|| Error::Apply(format!("shortcuts.vdf has no shortcut {appid}")))?;
    let previous = hit.options.as_ref().map(|v| text(bytes, v));
    let mut out = bytes.to_vec();
    match hit.options {
        Some(val) => {
            out.splice(val, options.as_bytes().iter().copied());
        }
        // New field: appended last inside the entry, right before its
        // closing `0x08`, which is where Steam itself writes it.
        None => {
            out.splice(hit.end..hit.end, encode_options_field(options));
        }
    }
    Ok((out, previous))
}

/// Key-absent restore: splice the whole `LaunchOptions` field out, leaving
/// the entry and every sibling byte untouched.
pub(crate) fn shortcut_remove_options(bytes: &[u8], appid: u32) -> Result<Vec<u8>> {
    let Some(hit) = locate(bytes, appid)? else {
        return Ok(bytes.to_vec());
    };
    let Some(field) = hit.field else {
        return Ok(bytes.to_vec());
    };
    let mut out = bytes.to_vec();
    out.drain(field);
    Ok(out)
}

/// Whether the blob carries an entry for `appid`. `Ok(false)` means the
/// user simply has no such shortcut (skip, don't invent one); `Err` means
/// the blob is malformed or corrupt (refuse loudly, never skip silently).
pub(crate) fn shortcut_contains(bytes: &[u8], appid: u32) -> Result<bool> {
    Ok(locate(bytes, appid)?.is_some())
}

/// Standalone rows key on the shortcut's u32 appid; anything else is a
/// malformed id, refused before a file is touched.
pub(crate) fn shortcut_appid(game: &str) -> Result<u32> {
    game.parse()
        .map_err(|_| Error::Apply(format!("{game} is not a Steam shortcut appid")))
}

/// Live `LaunchOptions` for a standalone shortcut, read from every local
/// user's `shortcuts.vdf` (last non-empty wins). Same read-only contract as
/// the owned `localconfig.vdf` reference.
pub(crate) fn shortcut_launch_options(appid: u32) -> Option<String> {
    let mut out: Option<String> = None;
    for path in shortcut_files(&steam_roots()) {
        let Ok(bytes) = std::fs::read(&path) else {
            continue;
        };
        if let Some(opt) = shortcut_get_options(&bytes, appid)
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
        {
            out = Some(opt);
        }
    }
    out
}
