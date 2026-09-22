use std::collections::{HashMap, HashSet};

use tuxgt_core::parse_mod_type;

use super::super::{ConfirmOp, PendingConfirm, ReqDep, SlotChoiceOp};

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
/// R12: work that lives only in the window a Hide would drop — the tail of
/// an install batch, or the E34 confirm it is parked behind. The tray veto
/// must stay armed while either holds. Only the install-family ops count:
/// a Host / ClientStop confirm owns no batch, and a single-row enable or
/// slot change has nothing queued behind it.
pub(crate) fn install_work_parked(queue: &[String], confirm: Option<&PendingConfirm>) -> bool {
    !queue.is_empty()
        || confirm.is_some_and(|c| {
            matches!(c, PendingConfirm::Requires { .. })
                || matches!(
                    c,
                    PendingConfirm::Overwrite { op, .. }
                        if matches!(
                            op,
                            ConfirmOp::Install { .. }
                                | ConfirmOp::Update { .. }
                                | ConfirmOp::UpdateForce { .. }
                        )
                )
                || matches!(
                    c,
                    PendingConfirm::SlotChoice { op, .. }
                        if matches!(
                            op,
                            SlotChoiceOp::Install { .. }
                                | SlotChoiceOp::Update { .. }
                                | SlotChoiceOp::UpdateForce { .. }
                        )
                )
        })
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

/// E34 Requires candidates for one `MissingRequires` payload. Core
/// doesn't say which loop raised it, so a payload an enabled Mod
/// carries is treated as a recipe id (that single Mod), otherwise as
/// a Mod type (every enabled Mod of that type, sorted + deduped).
/// The card installs the first, so multi-candidate order is
/// deterministic. Tuples are (id, label, mod_type, enabled).
pub(crate) fn requires_candidates<'a>(
    mods: impl IntoIterator<Item = (&'a str, &'a str, &'a str, bool)>,
    req: &str,
) -> Vec<(String, String)> {
    let mods: Vec<_> = mods.into_iter().collect();
    if let Some(found) = mods.iter().find(|m| m.3 && m.0 == req) {
        return vec![(found.0.to_string(), found.1.to_string())];
    }
    let mut out: Vec<(String, String)> = mods
        .iter()
        .filter(|m| m.3 && m.2 == req)
        .map(|m| (m.0.to_string(), m.1.to_string()))
        .collect();
    out.sort();
    out.dedup();
    out
}

/// One catalog row for missing-closure resolution.
pub(crate) struct ClosureMod<'a> {
    pub id: &'a str,
    pub label: &'a str,
    pub mod_type: &'a str,
    pub requires: &'a [String],
    pub enabled: bool,
}

/// Full missing-require closure for an install target, in core check
/// order (type Requires, then recipe requires, breadth-first): one
/// `ReqDep` line per require no installed manifest satisfies. The
/// target itself never appears: id requires naming it are skipped and
/// it is filtered from type candidates. A line with empty candidates
/// (disabled or
/// unknown target, or no enabled Mod of a required type) is
/// unresolvable — the caller falls back to the single-miss card or the
/// status line. Transitive requires resolve through the picked
/// candidate; a radio switch with divergent transitive requires lands
/// its stragglers on a follow-up card.
pub(crate) fn missing_closure(
    mods: &[ClosureMod<'_>],
    installed: &[(&str, &str)],
    target: &str,
) -> Vec<ReqDep> {
    use std::collections::VecDeque;
    let by_id: HashMap<&str, &ClosureMod> = mods.iter().map(|m| (m.id, m)).collect();
    let Some(tm) = by_id.get(target) else {
        return Vec::new();
    };
    let installed_ids: HashSet<&str> = installed.iter().map(|(i, _)| *i).collect();
    let installed_types: HashSet<&str> = installed.iter().map(|(_, t)| *t).collect();
    let tuples: Vec<(&str, &str, &str, bool)> = mods
        .iter()
        .map(|m| (m.id, m.label, m.mod_type, m.enabled))
        .collect();
    // (is_type, text) worklist, owned so enqueues from any catalog row fit.
    let mut queue: VecDeque<(bool, String)> = VecDeque::new();
    let enqueue = |m: &ClosureMod, queue: &mut VecDeque<(bool, String)>| {
        for r in parse_mod_type(m.mod_type)
            .ok()
            .and_then(|t| t.requires())
            .unwrap_or(&[])
        {
            queue.push_back((true, r.to_string()));
        }
        for r in m.requires {
            queue.push_back((false, r.to_string()));
        }
    };
    let mut planned: HashSet<String> = HashSet::new();
    let mut out: Vec<ReqDep> = Vec::new();
    enqueue(tm, &mut queue);
    while let Some((is_type, text)) = queue.pop_front() {
        if is_type {
            if installed_types.contains(text.as_str())
                || planned
                    .iter()
                    .any(|id| by_id.get(id.as_str()).is_some_and(|m| m.mod_type == text))
            {
                continue;
            }
            let mut candidates = requires_candidates(tuples.iter().copied(), &text);
            candidates.retain(|(id, _)| id != target);
            if candidates.is_empty() {
                out.push(ReqDep {
                    req: text.to_string(),
                    candidates: Box::default(),
                    chosen: None,
                });
                continue;
            }
            let chosen = candidates[0].0.clone();
            if planned.contains(&chosen) {
                continue;
            }
            planned.insert(chosen.clone());
            if let Some(m) = by_id.get(chosen.as_str()) {
                enqueue(m, &mut queue);
            }
            out.push(ReqDep {
                req: text,
                candidates: candidates.into_boxed_slice(),
                chosen: Some(chosen),
            });
        } else {
            if text.as_str() == target
                || installed_ids.contains(text.as_str())
                || planned.contains(&text)
            {
                continue;
            }
            let Some(m) = by_id.get(text.as_str()).filter(|m| m.enabled) else {
                out.push(ReqDep {
                    req: text,
                    candidates: Box::default(),
                    chosen: None,
                });
                continue;
            };
            planned.insert(text.clone());
            enqueue(m, &mut queue);
            out.push(ReqDep {
                req: text.clone(),
                candidates: Box::new([(text.clone(), m.label.to_string())]),
                chosen: Some(text),
            });
        }
    }
    out
}

/// Requires-confirm queue: chosen deps topo-first, then the target,
/// then the rest of the batch with moved-earlier dupes dropped.
pub(crate) fn splice_requires_order(
    chosen: &[String],
    target: &str,
    rest: &[String],
    info: &HashMap<String, (String, Vec<String>)>,
) -> Vec<String> {
    let mut ids: Vec<String> = chosen.to_vec();
    ids.push(target.to_string());
    let mut out = order_install_ids(&ids, info);
    for r in rest {
        if !out.iter().any(|o| o == r) {
            out.push(r.clone());
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// E34 Requires card: a `MissingRequires` payload names a recipe id
    /// when an enabled Mod carries it, otherwise a Mod type. The card
    /// lists the candidates and installs the first.
    #[test]
    fn requires_candidates_id_then_type() {
        let mods = [
            ("zebra", "Zebra", "reshade", true),
            ("arlene", "Arlene", "reshade", true),
            ("off", "Off", "reshade", false),
            ("mbase", "Base", "custom", true),
            ("zebra", "Zebra", "reshade", true),
        ];
        // Recipe id: the single Mod, even with same-type siblings around.
        assert_eq!(
            requires_candidates(mods, "mbase"),
            [("mbase".to_string(), "Base".to_string())]
        );
        // Type: every enabled Mod of that type, sorted + deduped.
        assert_eq!(
            requires_candidates(mods, "reshade"),
            [
                ("arlene".to_string(), "Arlene".to_string()),
                ("zebra".to_string(), "Zebra".to_string()),
            ]
        );
        // Disabled-only and unknown payloads: no card, status line instead.
        assert!(requires_candidates(mods, "off").is_empty());
        assert!(requires_candidates(mods, "nope").is_empty());
        // Ambiguous payload (also a Mod id, like official `reshade`): the
        // id match wins — here it satisfies a type require too, and
        // Install required takes the first, so a type listing could
        // install (and core would refuse) the wrong Mod for an id require.
        let amb = [
            ("zebra", "Zebra", "reshade", true),
            ("reshade", "ReShade", "reshade", true),
        ];
        assert_eq!(
            requires_candidates(amb, "reshade"),
            [("reshade".to_string(), "ReShade".to_string())]
        );
    }

    fn cmod<'a>(
        id: &'a str,
        label: &'a str,
        mod_type: &'a str,
        requires: &'a [String],
        enabled: bool,
    ) -> ClosureMod<'a> {
        ClosureMod {
            id,
            label,
            mod_type,
            requires,
            enabled,
        }
    }

    /// Requires card lists every missing dep: type Requires first (core
    /// order), then recipe requires, each with its candidates resolved.
    #[test]
    fn missing_closure_lists_all_missing_type_first() {
        let dfc_reqs = [
            "d3dcompiler-47".to_string(),
            "nvidia-streamline".to_string(),
            "nvngx-dlssnr-proxy".to_string(),
        ];
        let mods = [
            cmod("dfc", "DFC", "reshade_addon", &dfc_reqs, true),
            cmod("reshade", "ReShade", "reshade", &[], true),
            cmod("d3dcompiler-47", "d3dcompiler_47", "custom", &[], true),
            cmod("nvidia-streamline", "Streamline", "custom", &[], true),
            cmod("nvngx-dlssnr-proxy", "Proxy", "custom", &[], true),
        ];
        let deps = missing_closure(&mods, &[], "dfc");
        assert_eq!(
            deps.iter().map(|d| d.req.as_str()).collect::<Vec<_>>(),
            [
                "reshade",
                "d3dcompiler-47",
                "nvidia-streamline",
                "nvngx-dlssnr-proxy"
            ]
        );
        assert_eq!(
            deps.iter()
                .map(|d| d.chosen.clone().unwrap())
                .collect::<Vec<_>>(),
            [
                "reshade",
                "d3dcompiler-47",
                "nvidia-streamline",
                "nvngx-dlssnr-proxy"
            ]
        );
        assert!(deps.iter().all(|d| !d.candidates.is_empty()));
    }

    /// Installed manifests satisfy by instance (id requires) and by type
    /// (type requires); transitive requires resolve through the pick, and
    /// a require cycle back to the target terminates.
    #[test]
    fn missing_closure_skips_installed_and_follows_transitive() {
        let app_reqs = ["mid".to_string()];
        let mid_reqs = ["base".to_string()];
        let mods = [
            cmod("app", "App", "custom", &app_reqs, true),
            cmod("mid", "Mid", "custom", &mid_reqs, true),
            cmod("base", "Base", "custom", &[], true),
        ];
        let installed = [("base", "custom")];
        let deps = missing_closure(&mods, &installed, "app");
        assert_eq!(
            deps.iter().map(|d| d.req.as_str()).collect::<Vec<_>>(),
            ["mid"]
        );
        let cyc_reqs = ["cycb".to_string()];
        let cycb_reqs = ["cyca".to_string()];
        let cyc = [
            cmod("cyca", "A", "custom", &cyc_reqs, true),
            cmod("cycb", "B", "custom", &cycb_reqs, true),
        ];
        let deps = missing_closure(&cyc, &[], "cyca");
        assert_eq!(
            deps.iter().map(|d| d.req.as_str()).collect::<Vec<_>>(),
            ["cycb"]
        );
        let shade_reqs: [String; 0] = [];
        let tmods = [
            cmod("fx", "Fx", "reshade_addon", &shade_reqs, true),
            cmod("reshade", "ReShade", "reshade", &[], true),
        ];
        let installed = [("my-reshade", "reshade")];
        let deps = missing_closure(&tmods, &installed, "fx");
        assert!(deps.is_empty());
    }

    /// Disabled or unknown require targets resolve to an empty-candidate
    /// line so the caller falls back to the single-miss card or status.
    #[test]
    fn missing_closure_marks_unresolvable_lines() {
        let reqs = ["off".to_string(), "ghost".to_string()];
        let mods = [
            cmod("app", "App", "custom", &reqs, true),
            cmod("off", "Off", "custom", &[], false),
        ];
        let deps = missing_closure(&mods, &[], "app");
        assert_eq!(deps.len(), 2);
        assert!(deps
            .iter()
            .all(|d| d.candidates.is_empty() && d.chosen.is_none()));
    }

    /// Unknown target (recipe gone mid-flow): empty closure, so the
    /// caller falls back to the single miss that provoked the card.
    #[test]
    fn missing_closure_unknown_target_is_empty() {
        let mods = [cmod("a", "A", "custom", &[], true)];
        assert!(missing_closure(&mods, &[], "ghost").is_empty());
    }

    /// The target itself is filtered from type candidates: a custom
    /// ReShade build requiring an effect must not pick itself for the
    /// effect's ReShade-type require (that would re-park forever).
    #[test]
    fn missing_closure_never_picks_the_target_for_a_type_require() {
        let myr_reqs = ["myfx".to_string()];
        let mods = [
            cmod("myr", "MyR", "reshade", &myr_reqs, true),
            cmod("myfx", "MyFx", "effect", &[], true),
        ];
        let deps = missing_closure(&mods, &[], "myr");
        assert_eq!(
            deps.iter().map(|d| d.req.as_str()).collect::<Vec<_>>(),
            ["myfx", "reshade"]
        );
        assert!(deps[1].candidates.is_empty() && deps[1].chosen.is_none());
    }

    /// Install required queues chosen deps topo-first, then the target,
    /// then the rest of the batch with moved-earlier dupes dropped.
    #[test]
    fn splice_requires_order_deps_first_target_then_rest() {
        let info: HashMap<String, (String, Vec<String>)> = [
            (
                "dfc".to_string(),
                (
                    "reshade_addon".to_string(),
                    vec!["d3dcompiler-47".to_string()],
                ),
            ),
            (
                "d3dcompiler-47".to_string(),
                ("custom".to_string(), Vec::new()),
            ),
            ("reshade".to_string(), ("reshade".to_string(), Vec::new())),
            ("unrelated".to_string(), ("custom".to_string(), Vec::new())),
        ]
        .into_iter()
        .collect();
        let out = splice_requires_order(
            &["d3dcompiler-47".to_string(), "reshade".to_string()],
            "dfc",
            &["unrelated".to_string(), "reshade".to_string()],
            &info,
        );
        assert_eq!(out, ["d3dcompiler-47", "reshade", "dfc", "unrelated",]);
    }

    /// Requires-card radio state: `set_requires_chosen` switches the
    /// candidate Install required installs for one dep line; other
    /// lines and other confirms ignore it.
    #[test]
    fn set_requires_chosen_switches_requires_ignores_host() {
        use super::super::super::{ConfirmOp, PendingConfirm, ReqDep};
        let mut requires = PendingConfirm::Requires {
            game: "quake".into(),
            instance: "my-effect".into(),
            deps: Box::new([
                ReqDep {
                    req: "reshade".into(),
                    candidates: Box::new([
                        ("arlene".to_string(), "Arlene".into()),
                        ("zebra".to_string(), "Zebra".into()),
                    ]),
                    chosen: Some("arlene".into()),
                },
                ReqDep {
                    req: "d3dcompiler-47".into(),
                    candidates: Box::new([("d3dcompiler-47".to_string(), "d3dcompiler_47".into())]),
                    chosen: Some("d3dcompiler-47".into()),
                },
            ]),
        };
        requires.set_requires_chosen("reshade", "zebra");
        let PendingConfirm::Requires { deps, .. } = &requires else {
            panic!("still a Requires confirm");
        };
        assert_eq!(deps[0].chosen.as_deref(), Some("zebra"));
        assert_eq!(deps[1].chosen.as_deref(), Some("d3dcompiler-47"));
        requires.set_requires_chosen("nope", "zebra");
        let PendingConfirm::Requires { deps, .. } = &requires else {
            panic!("still a Requires confirm");
        };
        assert_eq!(deps[0].chosen.as_deref(), Some("zebra"));
        let mut other = PendingConfirm::Host {
            op: ConfirmOp::HostInstall,
            paths: Vec::<String>::new().into_boxed_slice(),
        };
        other.set_requires_chosen("reshade", "zebra");
        assert!(matches!(other, PendingConfirm::Host { .. }));
    }
}
