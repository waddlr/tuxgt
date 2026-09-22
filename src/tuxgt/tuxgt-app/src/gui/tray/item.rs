//! The StatusNotifierItem: status, menu, and activation.

use std::sync::atomic::Ordering;
use std::sync::mpsc::Sender;
use std::sync::Arc;

use ksni::menu::{MenuItem, StandardItem};

use super::policy::{AppCommand, RecentGame, RecentSnapshot};
use super::state::HideState;

/// The SNI item. The channel and the flag mirrors are the only ways out:
/// ksni calls these on its own thread, so they must not touch app state.
pub(super) struct TrayItem {
    /// Every label comes from the catalog (APP §9). The item is built on the
    /// main thread, where `Strings` lives.
    pub(super) show_label: String,
    pub(super) hide_label: String,
    pub(super) library_label: String,
    pub(super) settings_label: String,
    pub(super) quit_label: String,
    pub(super) tip: String,
    pub(super) sender: Sender<AppCommand>,
    pub(super) state: Arc<HideState>,
    pub(super) recents: RecentSnapshot,
}

impl TrayItem {
    fn shown(&self) -> bool {
        self.state.shown.load(Ordering::Relaxed)
    }
}

impl ksni::Tray for TrayItem {
    fn id(&self) -> String {
        "tuxgt".into()
    }

    fn icon_name(&self) -> String {
        "tuxgt".into()
    }

    fn title(&self) -> String {
        self.tip.clone()
    }

    fn status(&self) -> ksni::Status {
        if self.shown() {
            ksni::Status::Active
        } else {
            ksni::Status::Passive
        }
    }

    fn tool_tip(&self) -> ksni::ToolTip {
        ksni::ToolTip {
            title: self.tip.clone(),
            ..Default::default()
        }
    }

    fn menu(&self) -> Vec<MenuItem<Self>> {
        let mut menu: Vec<MenuItem<Self>> = Vec::new();
        // Section 1: one of Show/Hide, from the same mirror the status and
        // primary activation read. Never both.
        let (label, shown, command) = if self.shown() {
            (self.hide_label.clone(), false, AppCommand::Hide)
        } else {
            (self.show_label.clone(), true, AppCommand::Show)
        };
        menu.push(MenuItem::Standard(StandardItem {
            label,
            activate: Box::new(move |this: &mut Self| {
                this.state.shown.store(shown, Ordering::Relaxed);
                let _ = this.sender.send(command.clone());
            }),
            ..Default::default()
        }));
        // Section 2: page jumps. Both show the window, so both set the mirror.
        menu.push(MenuItem::Separator);
        for (label, command) in [
            (self.library_label.clone(), AppCommand::ShowLibrary),
            (self.settings_label.clone(), AppCommand::ShowSettings),
        ] {
            menu.push(MenuItem::Standard(StandardItem {
                label,
                activate: Box::new(move |this: &mut Self| {
                    this.state.shown.store(true, Ordering::Relaxed);
                    let _ = this.sender.send(command.clone());
                }),
                ..Default::default()
            }));
        }
        // Section 3: recent Play list. Omitted entirely when empty — no
        // placeholder row, no orphan separator. A click plays headless: the
        // mirror stays as it is.
        let recents: Vec<RecentGame> = self
            .recents
            .read()
            .map(|guard| guard.clone())
            .unwrap_or_default();
        if !recents.is_empty() {
            menu.push(MenuItem::Separator);
            for recent in recents {
                let id = recent.id.clone();
                menu.push(MenuItem::Standard(StandardItem {
                    label: recent.display.clone(),
                    activate: Box::new(move |this: &mut Self| {
                        let _ = this.sender.send(AppCommand::Play(id.clone()));
                    }),
                    ..Default::default()
                }));
            }
        }
        // Section 4: Quit is always last.
        menu.push(MenuItem::Separator);
        menu.push(MenuItem::Standard(StandardItem {
            label: self.quit_label.clone(),
            activate: Box::new(|this: &mut Self| {
                let _ = this.sender.send(AppCommand::Quit);
            }),
            ..Default::default()
        }));
        menu
    }

    /// Rebuild the menu on every open. ksni serves `GetLayout` from a cache
    /// built at spawn (before the first snapshot refresh) and only rebuilds
    /// on click/activate — unless this hook is overridden, in which case it
    /// also rebuilds on the host's `AboutToShow`. The body is empty on
    /// purpose: the rebuild itself is the point, and removing this override
    /// silently freezes the menu at its spawn-time shape. Do not remove.
    fn menu_about_to_show(&mut self) {}

    /// Left-click toggles. Never Quit: a stray click must not end the app.
    fn activate(&mut self, _x: i32, _y: i32) {
        let next = !self.shown();
        self.state.shown.store(next, Ordering::Relaxed);
        let _ = self.sender.send(if next {
            AppCommand::Show
        } else {
            AppCommand::Hide
        });
    }

    /// The host's SNI watcher went away. The item is still registered, so the
    /// app must stop treating the tray as a way back until it returns.
    fn watcher_offline(&self, _reason: ksni::OfflineReason) -> bool {
        self.state.offline.store(true, Ordering::Release);
        true
    }

    fn watcher_online(&self) {
        self.state.offline.store(false, Ordering::Release);
    }
}

#[cfg(test)]
mod tests {
    use super::super::policy::recent;
    use super::*;
    use std::sync::mpsc::Receiver;
    use std::sync::RwLock;

    use ksni::Tray as _;
    use tuxgt_core::Strings;

    fn item(state: Arc<HideState>) -> (TrayItem, Receiver<AppCommand>) {
        item_with_recents(state, Vec::new())
    }

    fn item_with_recents(
        state: Arc<HideState>,
        recents: Vec<RecentGame>,
    ) -> (TrayItem, Receiver<AppCommand>) {
        let strings = Strings::en_us().expect("en-US catalog");
        let (sender, receiver) = std::sync::mpsc::channel();
        (
            TrayItem {
                show_label: strings.get("gui-tray-show"),
                hide_label: strings.get("gui-tray-hide"),
                library_label: strings.get("gui-tray-show-library"),
                settings_label: strings.get("gui-tray-show-settings"),
                quit_label: strings.get("gui-tray-quit"),
                tip: strings.get("gui-tray-tooltip"),
                sender,
                state,
                recents: Arc::new(RwLock::new(recents)),
            },
            receiver,
        )
    }

    /// Labels in painted order; separators read as `None`.
    fn shape(menu: &[MenuItem<TrayItem>]) -> Vec<Option<&str>> {
        menu.iter()
            .map(|entry| match entry {
                MenuItem::Standard(standard) => Some(standard.label.as_str()),
                MenuItem::Separator => None,
                _ => Some(""),
            })
            .collect()
    }

    #[test]
    fn menu_shows_hide_without_show_while_visible() {
        let strings = Strings::en_us().expect("en-US catalog");
        let (mut item, receiver) = item_with_recents(
            HideState::new(),
            vec![recent("steam::1", "Celeste"), recent("steam::2", "Hades")],
        );
        let mut menu = item.menu();
        let show = strings.get("gui-tray-show");
        let hide = strings.get("gui-tray-hide");
        let library = strings.get("gui-tray-show-library");
        let settings = strings.get("gui-tray-show-settings");
        let quit = strings.get("gui-tray-quit");
        assert_eq!(
            shape(&menu),
            vec![
                Some(hide.as_str()),
                None,
                Some(library.as_str()),
                Some(settings.as_str()),
                None,
                Some("Celeste"),
                Some("Hades"),
                None,
                Some(quit.as_str()),
            ]
        );
        assert!(
            !shape(&menu).contains(&Some(show.as_str())),
            "Show and Hide never share a menu"
        );
        for entry in &mut menu {
            if let MenuItem::Standard(entry) = entry {
                (entry.activate)(&mut item);
            }
        }
        // The menu is the only way to reach a real Quit, so close-to-tray
        // never becomes a one-way door — and Quit stays last.
        let got: Vec<AppCommand> = receiver.try_iter().collect();
        assert_eq!(
            got,
            vec![
                AppCommand::Hide,
                AppCommand::ShowLibrary,
                AppCommand::ShowSettings,
                AppCommand::Play("steam::1".into()),
                AppCommand::Play("steam::2".into()),
                AppCommand::Quit,
            ]
        );
    }

    #[test]
    fn menu_shows_show_without_hide_while_hidden() {
        let strings = Strings::en_us().expect("en-US catalog");
        let state = HideState::new();
        state.set_hidden(true);
        let (item, _receiver) = item(Arc::clone(&state));
        let menu = item.menu();
        let painted = shape(&menu);
        let show = strings.get("gui-tray-show");
        let hide = strings.get("gui-tray-hide");
        assert_eq!(painted[0], Some(show.as_str()));
        assert!(
            !painted.contains(&Some(hide.as_str())),
            "Show and Hide never share a menu"
        );
    }

    #[test]
    fn menu_omits_recents_entirely_when_empty() {
        let strings = Strings::en_us().expect("en-US catalog");
        let (item, _receiver) = item(HideState::new());
        let menu = item.menu();
        let painted = shape(&menu);
        let hide = strings.get("gui-tray-hide");
        let library = strings.get("gui-tray-show-library");
        let settings = strings.get("gui-tray-show-settings");
        let quit = strings.get("gui-tray-quit");
        // No placeholder row, no orphan separator: the recents section and
        // its separator vanish together.
        assert_eq!(
            painted,
            vec![
                Some(hide.as_str()),
                None,
                Some(library.as_str()),
                Some(settings.as_str()),
                None,
                Some(quit.as_str()),
            ]
        );
    }

    #[test]
    fn menu_separators_sit_between_sections_only() {
        for recents in [
            Vec::new(),
            vec![recent("steam::1", "Celeste")],
            vec![
                recent("steam::1", "Celeste"),
                recent("steam::2", "Hades"),
                recent("steam::3", "Balatro"),
            ],
        ] {
            let (item, _receiver) = item_with_recents(HideState::new(), recents);
            let menu = item.menu();
            let painted = shape(&menu);
            assert!(
                !painted.is_empty() && painted[0].is_some(),
                "no leading separator: {painted:?}"
            );
            assert!(
                painted.last().is_some_and(|last| last.is_some()),
                "no trailing separator: {painted:?}"
            );
            assert!(
                !painted.windows(2).any(|w| w == [None, None]),
                "no doubled separator: {painted:?}"
            );
            // Quit is always the last row.
            let strings = Strings::en_us().expect("en-US catalog");
            let quit = strings.get("gui-tray-quit");
            assert_eq!(painted.last(), Some(&Some(quit.as_str())), "{painted:?}");
        }
    }

    #[test]
    fn menu_labels_come_from_the_catalog() {
        // APP §9: the tray is a user-visible surface, so a missing id must
        // not ship as its own id string.
        for id in [
            "gui-tray-show",
            "gui-tray-hide",
            "gui-tray-show-library",
            "gui-tray-show-settings",
            "gui-tray-quit",
        ] {
            let strings = Strings::en_us().expect("en-US catalog");
            assert_ne!(strings.get(id), id, "tray label echoes: {id}");
        }
        let (item, _receiver) = item(HideState::new());
        let menu = item.menu();
        let labels: Vec<&str> = menu
            .iter()
            .filter_map(|entry| match entry {
                MenuItem::Standard(standard) => Some(standard.label.as_str()),
                _ => None,
            })
            .collect();
        assert!(!labels.is_empty());
        assert!(labels.iter().all(|l| !l.is_empty()), "{labels:?}");
    }

    #[test]
    fn primary_activation_toggles_and_never_quits() {
        let state = HideState::new();
        let (mut item, receiver) = item(Arc::clone(&state));
        let _ = item.activate(0, 0);
        assert_eq!(receiver.recv().expect("cmd"), AppCommand::Hide);
        let _ = item.activate(0, 0);
        assert_eq!(receiver.recv().expect("cmd"), AppCommand::Show);
    }

    #[test]
    fn status_mirrors_the_window_and_hide_flips_it_back() {
        let state = HideState::new();
        let (item, _receiver) = item(Arc::clone(&state));
        assert_eq!(item.status(), ksni::Status::Active);
        // The controller's Hide is the one authority on the mirror, so a
        // left-click arriving after it reads the new state.
        state.set_hidden(true);
        assert_eq!(item.status(), ksni::Status::Passive);
        state.set_hidden(false);
        assert_eq!(item.status(), ksni::Status::Active);
    }

    #[test]
    fn an_offline_watcher_is_reported_until_it_returns() {
        let state = HideState::new();
        assert!(!state.is_offline());
        let (item, _receiver) = item(Arc::clone(&state));
        let _keep_going = item.watcher_offline(ksni::OfflineReason::No);
        assert!(state.is_offline());
        item.watcher_online();
        assert!(!state.is_offline());
    }
}
