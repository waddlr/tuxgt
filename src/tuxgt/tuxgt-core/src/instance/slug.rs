use super::*;
use crate::{Error, Result};

pub(crate) fn sanitize_slug(name: &str) -> String {
    let mut s = String::new();
    for c in name.to_lowercase().chars() {
        let c = if c == ' ' || c == '_' { '-' } else { c };
        if c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' {
            if c == '-' && (s.is_empty() || s.ends_with('-')) {
                continue;
            }
            s.push(c);
        }
    }
    s.trim_matches('-').to_string()
}

/// Variant-preserving truncation: over-long stems keep trailing
/// `test`/`dev`/`x32` tokens, truncating the base to fit 32 columns.
pub(crate) fn truncate_slug_variant(s: &str) -> String {
    if s.len() <= 32 {
        return s.to_string();
    }
    let toks: Vec<&str> = s.split('-').collect();
    let mut order: Vec<&str> = Vec::new();
    for t in &toks {
        if matches!(*t, "test" | "dev" | "x32") && !order.contains(t) {
            order.push(t);
        }
    }
    if order.is_empty() {
        let mut t: String = s.chars().take(32).collect();
        while t.ends_with('-') {
            t.pop();
        }
        return t;
    }
    let variant: String = order.iter().map(|t| format!("-{t}")).collect();
    let base_toks: Vec<&str> = toks
        .iter()
        .copied()
        .filter(|t| !matches!(*t, "test" | "dev" | "x32"))
        .collect();
    let mut base = base_toks.join("-");
    let allow = 32usize.saturating_sub(variant.len());
    if base.len() > allow {
        base.truncate(allow);
        while base.ends_with('-') {
            base.pop();
        }
    }
    format!("{base}{variant}")
}

pub(crate) fn package_slug(name: &str) -> Result<String> {
    let s = truncate_slug_variant(&sanitize_slug(name));
    if !valid_id(&s) {
        return Err(Error::InvalidInstance(format!(
            "{name}: id slug {s:?} is not [a-z][a-z0-9-]{{0,31}}"
        )));
    }
    Ok(s)
}

/// R38 arch identity: `B-x32` / `B-x64`, the suffix applied before the
/// 32-column truncation so the arch tag is never cut off. Only `"32"`
/// is 32-bit; every other arch (including `""`) is 64-bit, matching
/// `url_for_arch`.
pub(crate) fn arch_slug(base: &str, arch: &str) -> Result<String> {
    let suffix = if arch == "32" { "-x32" } else { "-x64" };
    let allow = 32usize.saturating_sub(suffix.len());
    let mut b = base.to_string();
    if b.len() > allow {
        b.truncate(allow);
        while b.ends_with('-') {
            b.pop();
        }
    }
    let id = format!("{b}{suffix}");
    if !valid_id(&id) {
        return Err(Error::InvalidInstance(format!(
            "{base}: arch id {id:?} is not [a-z][a-z0-9-]{{0,31}}"
        )));
    }
    Ok(id)
}
