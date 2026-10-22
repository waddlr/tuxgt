//! Template character rules. `{value}` is the only hole.

use super::schema::ArgKind;
use crate::{Error, Result};

pub(super) fn ident(s: &str, max: usize) -> bool {
    let b = s.as_bytes();
    !b.is_empty()
        && b.len() <= max
        && b[0].is_ascii_lowercase()
        && b[1..]
            .iter()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == b'-')
}

pub(super) fn check_template(path: &str, template: &str, kind: ArgKind) -> Result<()> {
    let n = template.matches("{value}").count();
    match kind {
        ArgKind::Flag => {
            if n != 0 {
                return Err(Error::Args(format!(
                    "{path}: flag template cannot contain {{value}}"
                )));
            }
            check_literal(path, template)
        }
        ArgKind::Enum | ArgKind::Freeform => {
            if n != 1 {
                return Err(Error::Args(format!(
                    "{path}: template must contain one {{value}}"
                )));
            }
            check_literal(path, &template.replace("{value}", ""))
        }
    }
}

fn check_literal(path: &str, skeleton: &str) -> Result<()> {
    if skeleton.chars().any(|c| {
        c.is_whitespace()
            || matches!(
                c,
                '"' | '\'' | '$' | '`' | ';' | '|' | '&' | '<' | '>' | '\\' | '/'
            )
    }) {
        return Err(Error::Args(format!(
            "{path}: template has a forbidden character"
        )));
    }
    let b = skeleton.as_bytes();
    if b.windows(2)
        .any(|w| w[0].is_ascii_alphabetic() && w[1] == b':')
    {
        return Err(Error::Args(format!("{path}: template has a drive letter")));
    }
    Ok(())
}
