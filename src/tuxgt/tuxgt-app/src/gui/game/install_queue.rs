use std::collections::{HashMap, HashSet};

use tuxgt_core::parse_mod_type;

pub(crate) fn merge_install_queue(
    queue: &mut Vec<String>,
    current: Option<&str>,
    ids: &[String],
    info: &HashMap<String, (String, Vec<String>)>,
) -> bool {
    if current.is_none() {
        *queue = order_install_ids(ids, info);
        return true;
    }
    for id in ids {
        if current == Some(id.as_str()) {
            continue;
        }
        if !queue.iter().any(|q| q == id) {
            queue.push(id.clone());
        }
    }
    *queue = order_install_ids(queue, info);
    false
}

/// Whether to apply a finished install spawn. `None` = ignore (other game
/// or a different in-flight instance). `Some(continue_queue)` = paint the
/// result; only the spawn that still owns `install_current` continues the
/// batch. Switch-away-and-back leaves `install_current` empty: still apply,
/// do not start the next queued id.
pub(crate) fn install_spawn_apply(mine: bool, here: bool, other_inflight: bool) -> Option<bool> {
    if !here || (!mine && other_inflight) {
        None
    } else {
        Some(mine)
    }
}

/// Stable topo: ids that satisfy another selected id's type or recipe
/// requires come first. Unrelated ids keep check order. A cycle falls
/// back to the leftover check order. Unchecked requires are not added.
pub(crate) fn order_install_ids(
    ids: &[String],
    info: &HashMap<String, (String, Vec<String>)>,
) -> Vec<String> {
    if ids.len() <= 1 {
        return ids.to_vec();
    }
    let mut indeg: HashMap<&str, usize> = ids.iter().map(|id| (id.as_str(), 0)).collect();
    let mut succ: HashMap<&str, Vec<&str>> = HashMap::new();
    for b in ids {
        let (b_ty, id_reqs) = info
            .get(b)
            .map(|(t, r)| (t.as_str(), r.as_slice()))
            .unwrap_or(("", &[]));
        let type_reqs = parse_mod_type(b_ty)
            .ok()
            .and_then(|t| t.requires())
            .unwrap_or(&[]);
        for a in ids {
            if a == b {
                continue;
            }
            let a_ty = info.get(a).map(|(t, _)| t.as_str()).unwrap_or("");
            let needed = type_reqs.iter().any(|r| *r == a_ty) || id_reqs.iter().any(|r| r == a);
            if needed {
                succ.entry(a.as_str()).or_default().push(b.as_str());
                *indeg.get_mut(b.as_str()).unwrap() += 1;
            }
        }
    }
    let mut placed: HashSet<&str> = HashSet::new();
    let mut out = Vec::with_capacity(ids.len());
    while out.len() < ids.len() {
        let Some(next) = ids.iter().find(|id| {
            !placed.contains(id.as_str()) && indeg.get(id.as_str()).copied().unwrap_or(0) == 0
        }) else {
            for id in ids {
                if !placed.contains(id.as_str()) {
                    out.push(id.clone());
                }
            }
            break;
        };
        let n = next.as_str();
        placed.insert(n);
        out.push(next.clone());
        if let Some(bs) = succ.get(n) {
            for b in bs {
                if let Some(d) = indeg.get_mut(b) {
                    *d = d.saturating_sub(1);
                }
            }
        }
    }
    out
}
