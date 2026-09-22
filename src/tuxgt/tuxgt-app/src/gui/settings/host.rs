use gpui_kit::component::button::ButtonVariants as _;
use gpui_kit::component::{h_flex, ActiveTheme, IconName};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use std::path::PathBuf;

use tuxgt_core::{
    all_tools, data_dir, install_userland, load_host_inventory, packaged_prefix,
    uninstall_userland, verify_host_install, FluentArgs, HostStatus,
};

use super::super::widgets;
use super::super::{host_row_id, tool_id, ConfirmOp, Note, PendingConfirm, Shell};

impl Shell {
    pub(crate) fn host_tools_box(&self, view: Entity<Self>, cx: &App) -> impl IntoElement {
        let b = cx.theme();
        widgets::section_card("host-tools", cx)
            .child(
                h_flex()
                    .w_full()
                    .items_center()
                    .justify_between()
                    .child(widgets::section_title(
                        self.strings.get("gui-section-host-tools"),
                        cx,
                    ))
                    .child(
                        widgets::btn("host-tools-retest", cx)
                            .secondary()
                            .child(widgets::bicon(IconName::RotateCw))
                            .child(widgets::blabel(self.strings.get("gui-action-retest"), cx))
                            .on_click({
                                let view = view.clone();
                                move |_, _, cx| {
                                    view.update(cx, |this, cx| this.rescan_tools_ui(cx));
                                }
                            }),
                    ),
            )
            .children(self.tools.iter().map(|t| {
                h_flex()
                    .id(tool_id(&t.name))
                    .w_full()
                    .gap_2()
                    .items_center()
                    .justify_between()
                    .child(widgets::mono(t.name.clone(), cx))
                    .child(widgets::pill(
                        self.strings.get(if t.found {
                            "gui-state-found"
                        } else {
                            "gui-state-missing"
                        }),
                        if t.found {
                            b.success
                        } else {
                            cx.theme().danger
                        },
                        b.border,
                        cx,
                    ))
            }))
    }

    /// Retest button probe: same `all_tools()` the Settings entry takes in
    /// `reload_tools`, once per click on the background thread. Paint only
    /// reads `self.tools`.
    pub(crate) fn rescan_tools_ui(&mut self, cx: &mut Context<Self>) {
        tracing::debug!(action = "rescan-tools");
        cx.spawn(async move |this, cx| {
            let tools = cx.background_spawn(async { all_tools() }).await;
            let _ = this.update(cx, |this, cx| {
                this.tools = tools.into_boxed_slice();
                cx.notify();
            });
        })
        .detach();
    }

    /// About card (top of Settings General left column): branded icon +
    pub(crate) fn host_install_box(&self, view: Entity<Self>, cx: &App) -> impl IntoElement {
        let b = cx.theme();
        let title = self.strings.get("gui-section-host-install");
        let ok_note = self.strings.get("gui-note-host-install-ok");
        let missing_note = self.strings.get("gui-note-host-install-no-inventory");
        let failures: Vec<_> = self
            .host_install
            .iter()
            .filter(|e| e.status != HostStatus::Ok && !tuxgt_core::is_icon_host_path(&e.path))
            .collect();
        let icon_line = tuxgt_core::icons_check_line(&self.host_install);
        let installed = !self.host_install.is_empty()
            && failures.is_empty()
            && icon_line.is_none()
            && !self.host_inventory_missing;
        let none_present = !self.host_install.is_empty()
            && self
                .host_install
                .iter()
                .all(|e| e.status == HostStatus::Missing);
        let partial = !self.host_install.is_empty()
            && (!failures.is_empty() || icon_line.is_some())
            && !none_present;
        let partial_note = if partial {
            let ok = self
                .host_install
                .iter()
                .filter(|e| e.status == HostStatus::Ok)
                .count();
            let mut args = FluentArgs::new();
            args.set("ok", ok.to_string());
            args.set("total", self.host_install.len().to_string());
            Some(
                self.strings
                    .get_args("gui-note-host-install-partial", Some(&args)),
            )
        } else {
            None
        };
        widgets::section_card("host-install", cx)
            .child(widgets::section_title(title, cx))
            .when(self.host_inventory_missing, |this| {
                this.child(widgets::muted(missing_note.clone(), cx))
            })
            .when(failures.is_empty() && icon_line.is_none(), |this| {
                this.child(widgets::muted(ok_note.clone(), cx))
            })
            .when_some(partial_note.clone(), |this, note| {
                this.child(widgets::muted(note, cx))
            })
            .children(failures.iter().enumerate().map(|(i, e)| {
                let fg = if e.status == HostStatus::Missing {
                    cx.theme().danger
                } else {
                    b.warning
                };
                widgets::labeled_row(
                    host_row_id(i),
                    e.path.display().to_string(),
                    None,
                    widgets::pill(e.status.to_string(), fg, b.border, cx),
                    cx,
                )
            }))
            .when_some(icon_line.clone(), |this, line| {
                let mut parts = line.split('\t');
                parts.next();
                let count = SharedString::from(parts.next().unwrap_or_default().to_string());
                let status = parts.next().unwrap_or_default();
                let fg = if status == "missing" {
                    cx.theme().danger
                } else {
                    b.warning
                };
                this.child(widgets::labeled_row(
                    "host-icons",
                    count,
                    None,
                    widgets::pill(status, fg, b.border, cx),
                    cx,
                ))
            })
            .child(
                h_flex()
                    .gap_1()
                    .child(
                        widgets::btn("host-install-run", cx)
                            .primary()
                            .child(widgets::blabel(
                                self.strings.get(if installed {
                                    "gui-action-host-reinstall"
                                } else {
                                    "gui-action-host-install"
                                }),
                                cx,
                            ))
                            .on_click({
                                let view = view.clone();
                                move |_, _, cx| {
                                    view.update(cx, |this, cx| this.host_install_ui(false, cx));
                                }
                            }),
                    )
                    .when(!none_present, |this| {
                        this.child(
                            widgets::btn("host-uninstall-run", cx)
                                .danger()
                                .child(widgets::blabel(
                                    self.strings.get("gui-action-host-uninstall"),
                                    cx,
                                ))
                                .on_click({
                                    let view = view.clone();
                                    move |_, _, cx| {
                                        view.update(cx, |this, cx| {
                                            this.host_uninstall_ui(false, cx)
                                        });
                                    }
                                }),
                        )
                    }),
            )
            .when(
                self.pending_confirm
                    .as_ref()
                    .is_some_and(|p| matches!(p, PendingConfirm::Host { .. })),
                |this| this.child(self.confirm_card(view.clone(), cx)),
            )
    }

    /// E69: run `install --yes` to the current prefix (`data_dir()`).
    /// An unpackaged binary errors before any write. A `modified` path
    /// stages an E34 host confirm instead; Confirm retries with `yes=true`.
    pub(crate) fn host_install_ui(&mut self, yes: bool, cx: &mut Context<Self>) {
        tracing::debug!(action = "host-install", yes);
        if !yes {
            let modified_paths: Vec<std::path::PathBuf> = self
                .host_install
                .iter()
                .filter(|e| e.status == HostStatus::Modified)
                .map(|e| e.path.clone())
                .collect();
            let (rest, icons) = tuxgt_core::collapse_icon_paths(&modified_paths);
            let mut modified: Vec<String> = rest.iter().map(|p| p.display().to_string()).collect();
            if let Some(line) = icons {
                modified.push(line);
            }
            if !modified.is_empty() {
                self.pending_confirm = Some(PendingConfirm::Host {
                    op: ConfirmOp::HostInstall,
                    paths: modified.into_boxed_slice(),
                });
                cx.notify();
                return;
            }
        }
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    let src = packaged_prefix()?;
                    let dest = data_dir();
                    install_userland(&src, &dest)?;
                    Ok::<_, tuxgt_core::Error>(())
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                match result {
                    Ok(()) => {
                        this.pending_note =
                            Some(Note::Ok(this.strings.get("gui-status-host-install-done")));
                    }
                    Err(e) => {
                        let mut args = FluentArgs::new();
                        args.set("error", e.to_string());
                        this.pending_note = Some(Note::Err(
                            this.strings
                                .get_args("gui-status-err-host-install", Some(&args)),
                        ));
                    }
                }
                this.refresh_host_install();
                cx.notify();
            });
        })
        .detach();
    }

    /// E69: run `uninstall --yes` from the last install inventory; PREFIX
    /// stays. Always confirms first (PATH links + boot conf). A missing
    /// inventory has nothing to confirm, so the op runs and errors honestly.
    pub(crate) fn host_uninstall_ui(&mut self, yes: bool, cx: &mut Context<Self>) {
        tracing::debug!(action = "host-uninstall", yes);
        if !yes {
            let paths: Vec<String> = load_host_inventory(&data_dir())
                .ok()
                .flatten()
                .map(|inv| {
                    let list: Vec<std::path::PathBuf> =
                        inv.paths.iter().map(|e| e.path.clone()).collect();
                    let (rest, icons) = tuxgt_core::collapse_icon_paths(&list);
                    let mut out: Vec<String> =
                        rest.iter().map(|p| p.display().to_string()).collect();
                    if let Some(line) = icons {
                        out.push(line);
                    }
                    out
                })
                .unwrap_or_default();
            if !paths.is_empty() {
                self.pending_confirm = Some(PendingConfirm::Host {
                    op: ConfirmOp::HostUninstall,
                    paths: paths.into_boxed_slice(),
                });
                cx.notify();
                return;
            }
        }
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move { uninstall_userland(&data_dir()) })
                .await;
            let _ = this.update(cx, |this, cx| {
                match result {
                    Ok(rep) => {
                        let mut args = FluentArgs::new();
                        args.set("removed", rep.removed.len().to_string());
                        args.set("skipped", rep.skipped.len().to_string());
                        this.pending_note = Some(Note::Ok(
                            this.strings
                                .get_args("gui-status-host-uninstall-done", Some(&args)),
                        ));
                    }
                    Err(e) => {
                        let mut args = FluentArgs::new();
                        args.set("error", e.to_string());
                        this.pending_note = Some(Note::Err(
                            this.strings
                                .get_args("gui-status-err-host-uninstall", Some(&args)),
                        ));
                    }
                }
                this.refresh_host_install();
                cx.notify();
            });
        })
        .detach();
    }
    /// E69: re-verify the intended host set and inventory presence after an
    /// install/uninstall, so the card and the E70 op toast read fresh state.
    pub(crate) fn refresh_host_install(&mut self) {
        let prefix = data_dir();
        match std::env::var_os("HOME").map(PathBuf::from) {
            Some(home) => {
                self.host_install = verify_host_install(&prefix, &home).into_boxed_slice();
            }
            None => self.host_install = Default::default(),
        }
        self.host_inventory_missing = load_host_inventory(&prefix).ok().flatten().is_none();
    }

    /// Slot dropdown options (Add form used these; per-game installed cards
    /// in `game.rs` still do). Same closed set as core `ProxySlot`.
    pub(crate) const ADD_SLOTS: [&'static str; 5] = ["dxgi", "d3d11", "d3d12", "winmm", "version"];
}
