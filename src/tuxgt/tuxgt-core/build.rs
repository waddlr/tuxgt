use std::env;
use std::fs;
use std::path::Path;

fn main() {
    println!("cargo:rerun-if-changed=l10n/en-US/cli.ftl");
    let text = fs::read_to_string("l10n/en-US/cli.ftl").expect("read cli.ftl for gui ids");
    let ids = gui_ids_with_values(&text);
    let mut out = String::from("pub(crate) const GUI_IDS: &[&str] = &[\n");
    for id in &ids {
        out.push_str(&format!("    \"{id}\",\n"));
    }
    out.push_str("];\n");
    let dest = Path::new(&env::var("OUT_DIR").expect("OUT_DIR")).join("gui_ids_generated.rs");
    fs::write(dest, out).expect("write gui ids");
}

/// `gui-*` message ids that carry a value (same-line or indented
/// continuation). Attribute-only messages have no `value()` and would echo.
fn gui_ids_with_values(text: &str) -> Vec<&str> {
    let mut ids = Vec::new();
    let mut cur: Option<(&str, bool)> = None;
    for line in text.lines() {
        if line.starts_with([' ', '\t']) {
            let t = line.trim();
            if let Some(c) = cur.as_mut() {
                if !t.is_empty() && !t.starts_with('.') {
                    c.1 = true;
                }
            }
            continue;
        }
        if let Some((id, valued)) = cur.take() {
            if valued {
                ids.push(id);
            }
        }
        if let Some((id, valued)) = gui_entry(line) {
            cur = Some((id, valued));
        }
    }
    if let Some((id, valued)) = cur.take() {
        if valued {
            ids.push(id);
        }
    }
    ids
}

/// Parse a column-0 `gui-<id> = ...` line; returns the full id plus whether
/// the same line already carries a non-empty value.
fn gui_entry(line: &str) -> Option<(&str, bool)> {
    let rest = line.strip_prefix("gui-")?;
    let len = rest
        .bytes()
        .take_while(|b| b.is_ascii_alphanumeric() || *b == b'_' || *b == b'-')
        .count();
    if len == 0 {
        return None;
    }
    let after = &rest[len..];
    let eq = after.find('=')?;
    if !after[..eq].trim().is_empty() {
        return None;
    }
    let full = &line[..4 + len];
    Some((full, !after[eq + 1..].trim().is_empty()))
}
