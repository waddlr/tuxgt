//! Same-rank stem collisions. A per-game stem may repeat an engine stem.

use super::schema::CatalogFile;
use crate::Result;

pub fn check_same_rank(files: &[CatalogFile]) -> Result<()> {
    for (i, a) in files.iter().enumerate() {
        for b in files.iter().skip(i + 1) {
            if a.trust != b.trust || !same_identity(a, b) {
                continue;
            }
            for arg in &a.args {
                if b.args.iter().any(|o| o.stem == arg.stem) {
                    return Err(crate::Error::Args(format!(
                        "{}: stem {} collides with {}",
                        a.path, arg.stem, b.path
                    )));
                }
            }
        }
    }
    Ok(())
}

fn same_identity(a: &CatalogFile, b: &CatalogFile) -> bool {
    match (&a.engine, &b.engine) {
        (Some(x), Some(y)) => return x == y,
        (Some(_), None) | (None, Some(_)) => return false,
        (None, None) => {}
    }
    let (Some(ga), Some(gb)) = (&a.game, &b.game) else {
        return false;
    };
    share(&ga.steam_appid, &gb.steam_appid)
        || share(&ga.gog_id, &gb.gog_id)
        || share(&ga.epic_id, &gb.epic_id)
        || (ga.engine.is_some()
            && ga.engine == gb.engine
            && ga
                .exe_names
                .iter()
                .any(|n| gb.exe_names.iter().any(|m| exe_stem(n) == exe_stem(m))))
}

fn share(a: &Option<String>, b: &Option<String>) -> bool {
    matches!((a, b), (Some(x), Some(y)) if x == y)
}

pub fn exe_stem(name: &str) -> String {
    let base = name.rsplit(['/', '\\']).next().unwrap_or(name);
    let stem = base.rsplit_once('.').map(|(s, _)| s).unwrap_or(base);
    stem.to_ascii_lowercase()
}
