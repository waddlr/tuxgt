use gpui_kit::*;
use tuxgt_core::{FluentArgs, RegistryStore};

use super::super::{load_registry, rt_block, short_pin, PendingRegistryConfirm, Shell};

/// R14 registry op as it runs off the UI thread. The store does the
/// git work, the digest verification, and the atomic lock write; this
/// only names which core call to make.
enum RegistryOp {
    Add { url: String },
    UpdateRegistry { url: String },
    RemoveRegistry { url: String },
    Install { id: String },
    UpdatePlugin { id: String },
    RemovePlugin { id: String },
    SetEnabled { id: String, on: bool },
}

impl RegistryOp {
    /// Run one registry mutation. `local` is hard-coded off rather than a
    /// parameter: the CLI has a `--local` development opt-in, and the GUI
    /// has none, so a URL typed into Add can never become a local path
    /// fetch. One call site, one value — a bool argument here would only
    /// be a flag a later edit could flip.
    fn run(self) -> tuxgt_core::Result<RegistryOutcome> {
        let mut store = RegistryStore::load()?;
        // HTTPS-only, always. See the note above.
        match self {
            RegistryOp::Add { url } => {
                let entry = store.add(&url, "HEAD", "registry.toml", false)?;
                Ok(RegistryOutcome::Added(entry.pinned))
            }
            RegistryOp::UpdateRegistry { url } => {
                let entry = store.update_registry(&url)?;
                Ok(RegistryOutcome::Repinned(entry.pinned))
            }
            RegistryOp::RemoveRegistry { url } => {
                store.remove_registry(&url)?;
                Ok(RegistryOutcome::Done)
            }
            RegistryOp::Install { id } => {
                Ok(RegistryOutcome::Installed(store.install(&id)?.pinned))
            }
            RegistryOp::UpdatePlugin { id } => {
                Ok(RegistryOutcome::Installed(store.update_plugin(&id)?.pinned))
            }
            RegistryOp::RemovePlugin { id } => {
                store.remove_plugin(&id)?;
                Ok(RegistryOutcome::Done)
            }
            RegistryOp::SetEnabled { id, on } => {
                store.set_remote_enabled(&id, on)?;
                Ok(RegistryOutcome::Enabled(on))
            }
        }
    }
}

/// What an op did, for the status line. The pin is what the user can
/// check against the registry, so a success always names it.
enum RegistryOutcome {
    Added(String),
    Repinned(String),
    Installed(String),
    Enabled(bool),
    Done,
}

impl Shell {
    /// R14: re-read the registry surface from disk. Called on Core
    /// Plugins entry and after every mutation, so the rows a user acts on
    /// are the rows the store holds. A failed read keeps the last rows and
    /// reports the error: an unreadable store is not an empty one.
    pub(crate) fn load_registry_ui(&mut self, cx: &mut Context<Self>) {
        match load_registry() {
            Ok((registries, plugins)) => {
                self.registries = registries.into_boxed_slice();
                self.remote_plugins = plugins.into_boxed_slice();
            }
            Err(e) => {
                let mut args = FluentArgs::new();
                args.set("error", e.to_string());
                self.status = self
                    .strings
                    .get_args("gui-status-err-registry", Some(&args));
            }
        }
        cx.notify();
    }

    /// R14: one background mutation, then a repaint from disk. Every
    /// registry call is blocking (git, tar, digest), so it runs on the
    /// background executor and the page never waits on it. A failure
    /// names the plugin whose op failed, so the row it sits on keeps
    /// painting the version that is still installed.
    fn run_registry_op(
        &mut self,
        op: RegistryOp,
        failed_id: Option<String>,
        cx: &mut Context<Self>,
    ) {
        if self.registry_busy {
            self.status = self.strings.get("gui-status-registry-busy");
            cx.notify();
            return;
        }
        self.registry_busy = true;
        cx.notify();
        // A mutation vetoes a Hide like a transfer: the stub handoff would
        // kill it mid-write. Held inside the future: a local would drop on
        // return, before the op starts.
        let transfer = self.hide_state.installing();
        cx.spawn(async move |this, cx| {
            let _transfer = transfer;
            let result = cx
                .background_spawn(async move { rt_block(async move { op.run() }) })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.registry_busy = false;
                match result {
                    Ok(outcome) => {
                        this.registry_error = None;
                        this.status = this.registry_status(&outcome);
                    }
                    Err(e) => {
                        // A failed update leaves the installed pin in
                        // `remote.toml`; the row is reloaded from disk, so
                        // the old version stays on screen under the error.
                        this.registry_error = Some((failed_id.unwrap_or_default(), e.to_string()));
                        this.status = this.strings.get("gui-status-registry-failed");
                    }
                }
                this.load_registry_ui(cx);
            });
        })
        .detach();
    }

    fn registry_status(&self, outcome: &RegistryOutcome) -> String {
        let mut args = FluentArgs::new();
        let key = match outcome {
            RegistryOutcome::Added(pin) => {
                args.set("pin", short_pin(pin));
                "gui-status-registry-added"
            }
            RegistryOutcome::Repinned(pin) => {
                args.set("pin", short_pin(pin));
                "gui-status-registry-repinned"
            }
            RegistryOutcome::Installed(pin) => {
                args.set("pin", short_pin(pin));
                "gui-status-registry-installed"
            }
            RegistryOutcome::Enabled(on) => {
                let key = if *on {
                    "plugin-enabled"
                } else {
                    "plugin-disabled"
                };
                return self.strings.get(key);
            }
            RegistryOutcome::Done => return self.strings.get("gui-status-registry-done"),
        };
        self.strings.get_args(key, Some(&args))
    }

    /// R14: Add registry. HTTPS only — the GUI never passes the `local`
    /// opt-in the CLI has for development paths, so a typed URL is
    /// validated as a URL by core, not trusted as one.
    pub(crate) fn add_registry_ui(
        &mut self,
        url: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let url = url.trim().to_string();
        // Review P2: reject a non-HTTPS source here, in the GUI's own words.
        // Core's `check_url` refuses the same inputs, but its message names
        // the CLI's `--local` opt-in — advice this page has no control for,
        // and a local path fetch must not look like something to try here.
        if let Some(key) = registry_url_rejected(&url) {
            self.registry_error = Some((String::new(), self.strings.get(key)));
            self.status = self.strings.get(key);
            cx.notify();
            return;
        }
        tracing::debug!(action = "add-registry");
        self.run_registry_op(RegistryOp::Add { url }, None, cx);
        // Clear the field on the next frame: the op is already in flight
        // and its result repaints the page.
        let input = self.registry_url_input.clone();
        cx.spawn_in(window, async move |this, cx| {
            let _ = this.update_in(cx, |_this, window, cx| {
                input.update(cx, |inp, cx| {
                    inp.set_value(String::new(), window, cx);
                });
            });
        })
        .detach();
    }

    /// R14: re-resolve a registry's discovery ref to a new pinned commit.
    /// A broken upstream leaves the old pin and the old rows in place.
    pub(crate) fn update_registry_pin_ui(&mut self, url: String, cx: &mut Context<Self>) {
        tracing::debug!(action = "update-registry-pin");
        self.run_registry_op(RegistryOp::UpdateRegistry { url }, None, cx);
    }

    /// R14: ask first, then drop the registry, the payloads it installed,
    /// and its cache. First-party `plugins.toml` is never touched.
    pub(crate) fn request_remove_registry(
        &mut self,
        url: String,
        plugin_count: usize,
        cx: &mut Context<Self>,
    ) {
        self.registry_confirm = Some(PendingRegistryConfirm::RemoveRegistry { url, plugin_count });
        cx.notify();
    }

    /// R14: install the plugin at its registry's pinned commit, verified
    /// by payload digest before the lock moves.
    pub(crate) fn request_install_plugin(&mut self, id: String, cx: &mut Context<Self>) {
        self.registry_confirm = Some(PendingRegistryConfirm::Install { id });
        cx.notify();
    }

    /// R14: ask first, naming the version the update replaces.
    pub(crate) fn request_update_plugin(
        &mut self,
        id: String,
        from_pin: String,
        cx: &mut Context<Self>,
    ) {
        self.registry_confirm = Some(PendingRegistryConfirm::Update { id, from_pin });
        cx.notify();
    }

    /// R14: ask first, then remove the plugin and every staged version.
    pub(crate) fn request_remove_plugin(&mut self, id: String, cx: &mut Context<Self>) {
        self.registry_confirm = Some(PendingRegistryConfirm::Remove { id });
        cx.notify();
    }

    /// R14: Confirm runs the stashed op. The consent is the only gate;
    /// core re-validates the digest either way, so Confirm can never
    /// install bytes the registry no longer vouches for.
    pub(crate) fn confirm_registry_op(&mut self, cx: &mut Context<Self>) {
        // Review P2: check busy BEFORE taking. `run_registry_op` refuses to
        // start while another op is in flight, so taking first would throw
        // away a consent the user just gave and leave the op unrun. The
        // stash stays armed and Confirm works once the op finishes.
        if self.registry_busy {
            self.status = self.strings.get("gui-status-registry-busy");
            cx.notify();
            return;
        }
        let Some(pending) = self.registry_confirm.take() else {
            return;
        };
        match pending {
            PendingRegistryConfirm::Install { id } => {
                self.run_registry_op(RegistryOp::Install { id: id.clone() }, Some(id), cx);
            }
            PendingRegistryConfirm::Update { id, .. } => {
                self.run_registry_op(RegistryOp::UpdatePlugin { id: id.clone() }, Some(id), cx);
            }
            PendingRegistryConfirm::Remove { id } => {
                self.run_registry_op(RegistryOp::RemovePlugin { id }, None, cx);
            }
            PendingRegistryConfirm::RemoveRegistry { url, .. } => {
                self.run_registry_op(RegistryOp::RemoveRegistry { url }, None, cx);
            }
        }
    }

    /// R14: Cancel makes no core call.
    pub(crate) fn cancel_registry_op(&mut self, cx: &mut Context<Self>) {
        self.registry_confirm = None;
        cx.notify();
    }

    /// R14: the remote enable toggle is immediate like every other toggle
    /// on the page, and it writes only `remote.toml` — never
    /// `PluginHost::set_enabled`, which knows only first-party ids.
    pub(crate) fn set_remote_plugin_enabled_ui(
        &mut self,
        id: String,
        on: bool,
        cx: &mut Context<Self>,
    ) {
        tracing::debug!(
            action = "set-remote-plugin-enabled",
            source = id.as_str(),
            on
        );
        self.run_registry_op(RegistryOp::SetEnabled { id, on }, None, cx);
    }

    /// Review P2: one rule for every registry control on the page — see
    /// `registry_controls_locked`.
    pub(crate) fn registry_locked(&self) -> bool {
        registry_controls_locked(self.registry_busy, self.registry_confirm.is_some())
    }
}

/// Review P2: every registry control except Confirm/Cancel is locked while
/// an op is in flight *or* while a confirm card is open. The old code only
/// gated on `busy`, so an Add or Refresh could start underneath a parked
/// install and then swallow the consent when Confirm arrived. One predicate
/// for every call site, so the rule cannot drift between rows.
///
/// Confirm and Cancel deliberately stay live: a busy guard is a dead button
/// there, and Cancel must always be able to drop a parked op.
pub(crate) fn registry_controls_locked(busy: bool, confirm_open: bool) -> bool {
    busy || confirm_open
}

/// Review P2: the fluent key for a URL the GUI refuses before it reaches
/// core, or `None` when the string is worth handing to core.
///
/// This is exactly the set core's `check_url` refuses with the `--local`
/// hint attached: everything that is not `https://` and not empty. Core
/// stays the authority — this only replaces the one message that would
/// advertise a CLI flag the Settings page does not have.
pub(crate) fn registry_url_rejected(url: &str) -> Option<&'static str> {
    let url = url.trim();
    if url.is_empty() {
        return Some("gui-status-registry-empty-url");
    }
    if !url.starts_with("https://") {
        return Some("gui-status-registry-https-only");
    }
    None
}

#[cfg(test)]
mod tests {
    use super::{registry_controls_locked, registry_url_rejected};

    /// Review P2: a parked confirm must lock every other control, or an
    /// Add/Refresh started underneath it swallows the consent on Confirm.
    #[test]
    fn confirm_open_locks_the_rest_of_the_page() {
        assert!(!registry_controls_locked(false, false), "idle page is live");
        assert!(registry_controls_locked(true, false), "op in flight");
        assert!(
            registry_controls_locked(false, true),
            "parked confirm locks Add/Refresh/rows"
        );
        assert!(registry_controls_locked(true, true), "both, still locked");
    }

    /// Review P2: the GUI must never hand core a source it would reject
    /// with the CLI `--local` hint, and must pass through real https URLs.
    #[test]
    fn gui_refuses_non_https_before_core() {
        // Exactly the inputs that would surface `--local` in the message.
        for refused in [
            "/tmp/some/registry",
            "file:///tmp/registry",
            "http://example.invalid/repo.git",
            "git@github.com:owner/repo.git",
            "example.invalid/repo.git",
        ] {
            assert_eq!(
                registry_url_rejected(refused),
                Some("gui-status-registry-https-only"),
                "{refused} must not reach core"
            );
        }
        // Empty is its own message, and trims before either check.
        assert_eq!(
            registry_url_rejected("   "),
            Some("gui-status-registry-empty-url")
        );
        assert_eq!(
            registry_url_rejected(""),
            Some("gui-status-registry-empty-url")
        );
        // Padded https is accepted; a bare `https://` is handed to core,
        // which owns the "no host" check.
        assert_eq!(registry_url_rejected("  https://example.invalid/r  "), None);
        assert_eq!(
            registry_url_rejected("https://"),
            None,
            "core reports the missing host, not this page"
        );
    }
}
