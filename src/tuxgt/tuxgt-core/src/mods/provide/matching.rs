use super::{Cand, Pick};

/// Payload basename for a pattern. Glob marks are stripped so
/// `my-mod-*.dll` is stored as `my-mod-.dll`.
pub(crate) fn concrete_name(pat: &str) -> String {
    let base = pat.rsplit(['/', '\\']).next().unwrap_or(pat);
    base.chars().filter(|c| *c != '*' && *c != '?').collect()
}

fn split_ext(name: &str) -> (&str, &str) {
    match name.rsplit_once('.') {
        Some((stem, ext)) if !stem.is_empty() && !ext.is_empty() => (stem, ext),
        _ => (name, ""),
    }
}

/// Same extension, and the source stem is the dest stem plus a `-` or `.`
/// version tail (`renodx-dlss-v1` for `renodx-dlss`). `_` is not a separator:
/// `nvngx_dlssd` must not fill `nvngx_dlss`, and `sl.dlss_g` must not fill `sl.dlss`.
fn prefix_match(pat: &str, base: &str) -> bool {
    let dest = concrete_name(pat);
    let (dst_stem, dst_ext) = split_ext(&dest);
    let (src_stem, src_ext) = split_ext(base);
    if dst_stem.is_empty() || !dst_ext.eq_ignore_ascii_case(src_ext) {
        return false;
    }
    let src_l = src_stem.to_ascii_lowercase();
    let dst_l = dst_stem.to_ascii_lowercase();
    match src_l.strip_prefix(&dst_l) {
        Some(rest) => rest.starts_with('-') || rest.starts_with('.'),
        None => false,
    }
}

fn pattern_hit(pat: &str, rel: &str) -> bool {
    let base = rel.rsplit('/').next().unwrap_or(rel);
    let glob_ok = if pat.contains('/') {
        crate::download::glob_match(pat, rel)
    } else {
        crate::download::glob_match(pat, base)
    };
    glob_ok || prefix_match(pat, base)
}

/// Hits for one pattern. A candidate that exactly fulfills a *sibling*
/// pattern is not a hit here, not even as a `-`/`.` version tail:
/// `nvngx_dlssnr.real.dll` must not fill `nvngx_dlssnr.dll`.
pub(crate) fn pattern_hits<'a>(files: &[String], pat: &str, cands: &'a [Cand]) -> Vec<&'a Cand> {
    let mine = concrete_name(pat);
    cands
        .iter()
        .filter(|c| {
            if !pattern_hit(pat, &c.rel) {
                return false;
            }
            let base = c.rel.rsplit('/').next().unwrap_or(&c.rel);
            base == mine
                || !files
                    .iter()
                    .any(|f| concrete_name(f).eq_ignore_ascii_case(base))
        })
        .collect()
}

fn has_segment(rel: &str, name: &str) -> bool {
    rel.split('/').any(|seg| seg.eq_ignore_ascii_case(name))
}

pub(crate) fn choose(pat: &str, hits: &[&Cand]) -> Pick {
    if hits.is_empty() {
        return Pick::Missing;
    }
    let dest = concrete_name(pat);
    let exact: Vec<&Cand> = hits
        .iter()
        .copied()
        .filter(|c| c.rel.rsplit('/').next().unwrap_or(&c.rel) == dest)
        .collect();
    let pool: Vec<&Cand> = if exact.is_empty() {
        hits.to_vec()
    } else {
        exact
    };
    if pool.len() == 1 {
        return Pick::One(pool[0].path.clone());
    }
    let hits = pool.as_slice();
    let mut kept: Vec<&Cand> = hits.to_vec();
    let x64: Vec<&Cand> = kept
        .iter()
        .copied()
        .filter(|c| has_segment(&c.rel, "x64"))
        .collect();
    if !x64.is_empty() {
        kept = x64;
    }
    kept.retain(|c| {
        !has_segment(&c.rel, "development")
            && !has_segment(&c.rel, "x86")
            && !has_segment(&c.rel, "win32")
    });
    if kept.len() == 1 {
        Pick::One(kept[0].path.clone())
    } else {
        Pick::Ambiguous
    }
}
