use gpui_kit::component::button::ButtonVariants as _;
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::*;
use tuxgt_core::{config_dir, data_dir, list_mods, FluentArgs};

use super::super::settings::{pattern_list, pick_provide};
use super::super::widgets;
use super::super::{provided_status, InstanceRow, PendingProvide, ProvidedRow, Shell};

impl Shell {
    /// Note plus Missing / More than one match pattern lists for one
    /// not-ready `provided` row. The install park card also shows the
    /// Present list; the picker row stays compact without it.
    pub(crate) fn provided_lines(
        &self,
        p: &ProvidedRow,
        show_present: bool,
        cx: &App,
    ) -> Vec<AnyElement> {
        let mut out = Vec::new();
        if !p.note.is_empty() {
            out.push(widgets::muted(p.note.clone(), cx).into_any_element());
        }
        for el in [
            pattern_list(self.strings.get("gui-provide-missing"), &p.missing, cx),
            pattern_list(self.strings.get("gui-provide-ambiguous"), &p.ambiguous, cx),
            show_present
                .then(|| pattern_list(self.strings.get("gui-provide-present"), &p.present, cx))
                .flatten(),
        ]
        .into_iter()
        .flatten()
        {
            out.push(el);
        }
        out
    }

    /// Park the install behind the Provide card. `msg` is the core
    /// `NeedsProvidedFiles` payload (`{id}: files not provided: …`); the
    /// named Mod may be a dep the batch queued ahead of `target`. The
    /// label and pattern snapshot come from a transient catalog read
    /// (install-error path only, never paint), falling back to the names
    /// packed in `msg` when the read misses.
    pub(crate) fn park_provide(
        &mut self,
        game: String,
        target: String,
        msg: &str,
        with_requires: Option<String>,
        yes: bool,
        slot: Option<String>,
    ) {
        let needs = needs_id(msg, &target).to_string();
        let data = data_dir();
        let found = list_mods(&config_dir(), &data)
            .ok()
            .and_then(|l| l.mods.into_iter().find(|m| m.id == needs));
        let (label, provided) = match &found {
            Some(inst) => (
                inst.label.clone(),
                provided_status(&data, inst).1.unwrap_or(ProvidedRow {
                    present: Vec::new(),
                    missing: msg_names(msg),
                    ambiguous: Vec::new(),
                    ready: false,
                    note: String::new(),
                }),
            ),
            None => (
                self.catalog_label(&needs),
                ProvidedRow {
                    present: Vec::new(),
                    missing: msg_names(msg),
                    ambiguous: Vec::new(),
                    ready: false,
                    note: String::new(),
                },
            ),
        };
        self.pending_provide = Some(PendingProvide {
            game,
            target,
            needs,
            label,
            provided,
            with_requires,
            yes,
            slot,
        });
        self.scroll_page_top();
    }

    /// Cancel: drop the parked install and the batch behind it, like
    /// abandoning a parked archive password. Disk is unchanged — core
    /// returns `NeedsProvidedFiles` before any write.
    pub(crate) fn clear_pending_provide(&mut self) {
        if let Some(park) = self.pending_provide.take() {
            self.clear_install_current(&park.game, &park.target);
            self.install_queue.clear();
            self.install_hold = None;
        }
    }

    /// A Provide pick finished for `id` (`rows` is the fresh catalog
    /// reload that pick already paid for). Ready retries the parked
    /// install; still short refreshes the card's pattern snapshot.
    pub(crate) fn resume_after_provide(
        &mut self,
        id: &str,
        rows: &[InstanceRow],
        cx: &mut Context<Self>,
    ) {
        let Some(park) = self.pending_provide.clone() else {
            return;
        };
        // A park only ever resumes on its own game page, like the
        // Requires confirm: anywhere else the retry would install
        // into a game the user is no longer looking at.
        if self.selected.as_deref() != Some(park.game.as_str()) {
            return;
        }
        let fresh = rows
            .iter()
            .find(|r| r.id == id)
            .and_then(|r| r.provided.as_ref());
        match resume_decision(&park.needs, id, fresh) {
            ResumeDecision::Retry => {
                self.pending_provide = None;
                self.install_mod_ui(
                    &park.game,
                    &park.target,
                    park.with_requires,
                    park.yes,
                    None,
                    park.slot,
                    cx,
                );
            }
            ResumeDecision::Refresh(p) => {
                if let Some(cur) = self.pending_provide.as_mut() {
                    cur.provided = p.clone();
                }
            }
            ResumeDecision::Ignore => {}
        }
    }

    /// The Files needed card: page-body replacement while an install is
    /// parked, mirroring the archive-password card.
    pub(crate) fn provide_box(&self, view: Entity<Self>, cx: &App) -> AnyElement {
        let Some(park) = self.pending_provide.clone() else {
            return div().into_any_element();
        };
        let mut args = FluentArgs::new();
        args.set("label", park.label.clone());
        let title = self
            .strings
            .get_args("gui-provide-install-title", Some(&args));
        let provide_id = park.needs.clone();
        let provide_view = view.clone();
        let cancel_view = view;
        let body = v_flex()
            .gap_2()
            .child(widgets::muted(
                self.strings.get("gui-provide-install-note"),
                cx,
            ))
            .children(self.provided_lines(&park.provided, true, cx));
        let footer = h_flex()
            .w_full()
            .gap_2()
            .justify_end()
            .child(
                widgets::btn("provide-install-pick", cx)
                    .primary()
                    .child(widgets::blabel(
                        self.strings.get("gui-action-provide-files"),
                        cx,
                    ))
                    .on_click(move |_, window, cx| {
                        pick_provide(provide_view.clone(), provide_id.clone(), window, cx);
                    }),
            )
            .child(
                widgets::btn("provide-install-cancel", cx)
                    .secondary()
                    .child(widgets::blabel(self.strings.get("gui-action-cancel"), cx))
                    .on_click(move |_, _, cx| {
                        cancel_view.update(cx, |this, cx| {
                            this.clear_pending_provide();
                            cx.notify();
                        });
                    }),
            );
        widgets::section_card("provide-install-panel", cx)
            .child(widgets::section_title(title, cx))
            .child(body)
            .child(footer)
            .into_any_element()
    }
}

/// The Mod a `NeedsProvidedFiles` payload names (`{id}: …`); ids
/// cannot contain `:` (`valid_id`), so the first segment is the id.
/// A blank payload falls back to the install target.
fn needs_id<'a>(msg: &'a str, fallback: &'a str) -> &'a str {
    msg.split(':')
        .next()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(fallback)
}

/// Names packed after `files not provided: ` in a `NeedsProvidedFiles`
/// payload, for the snapshot fallback when the catalog read misses.
fn msg_names(msg: &str) -> Vec<String> {
    msg.split_once("files not provided: ")
        .map(|(_, names)| {
            names
                .split(", ")
                .filter(|s| !s.is_empty())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

/// What a finished Provide pick means for a parked install.
enum ResumeDecision<'a> {
    /// The named Mod is ready: retry the parked install.
    Retry,
    /// Still short: show the fresh pattern snapshot on the card.
    Refresh(&'a ProvidedRow),
    /// Another Mod's pick, or no snapshot to judge by.
    Ignore,
}

fn resume_decision<'a>(
    needs: &str,
    id: &str,
    fresh: Option<&'a ProvidedRow>,
) -> ResumeDecision<'a> {
    if needs != id {
        return ResumeDecision::Ignore;
    }
    match fresh {
        Some(p) if p.ready => ResumeDecision::Retry,
        Some(p) => ResumeDecision::Refresh(p),
        None => ResumeDecision::Ignore,
    }
}

#[cfg(test)]
mod tests {
    use super::{msg_names, needs_id, resume_decision, ProvidedRow, ResumeDecision};

    fn row(ready: bool) -> ProvidedRow {
        ProvidedRow {
            present: Vec::new(),
            missing: Vec::new(),
            ambiguous: Vec::new(),
            ready,
            note: String::new(),
        }
    }

    /// The payload names the not-ready Mod, which for a nested
    /// require is the dep, not the install target.
    #[test]
    fn needs_id_names_the_dep_not_the_target() {
        let msg = "nvidia-streamline: files not provided: sl.common.dll";
        assert_eq!(needs_id(msg, "renodx-dlss"), "nvidia-streamline");
        assert_eq!(needs_id("", "renodx-dlss"), "renodx-dlss");
        assert_eq!(needs_id(" : files not provided: x", "t"), "t");
    }

    #[test]
    fn msg_names_splits_the_packed_list() {
        let msg = "nvidia-streamline: files not provided: a.dll, b.dll";
        assert_eq!(msg_names(msg), ["a.dll", "b.dll"]);
        assert!(msg_names("nvidia-streamline").is_empty());
    }

    /// Ready retries, still-short refreshes the snapshot, and a pick
    /// for another Mod (or no snapshot) leaves the park alone.
    #[test]
    fn resume_decision_follows_readiness() {
        let ready = row(true);
        let short = row(false);
        assert!(matches!(
            resume_decision("sl", "sl", Some(&ready)),
            ResumeDecision::Retry
        ));
        assert!(matches!(
            resume_decision("sl", "sl", Some(&short)),
            ResumeDecision::Refresh(p) if !p.ready
        ));
        assert!(matches!(
            resume_decision("sl", "other", Some(&ready)),
            ResumeDecision::Ignore
        ));
        assert!(matches!(
            resume_decision("sl", "sl", None),
            ResumeDecision::Ignore
        ));
    }
}
