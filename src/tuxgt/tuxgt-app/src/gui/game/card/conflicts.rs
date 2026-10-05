use gpui_kit::component::button::ButtonVariants as _;
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::component::{
    h_flex, v_flex, ActiveTheme, Disableable as _, Icon, IconName, Sizable as _,
};
use gpui_kit::*;
use tuxgt_core::FluentArgs;

use super::super::super::{widgets, Shell};
use super::super::{dest_short, DestConflict, Reachability};

impl Shell {
    /// Card conflict marker: a plain icon after the name, warning-tinted
    /// when this card loses any contested file, success-tinted when it
    /// wins them all. The tooltip names every contested dest with the
    /// rivals on each side. Display-only; winning is a card reorder.
    pub(crate) fn conflict_icon(
        &self,
        marks: &[DestConflict],
        id: SharedString,
        cx: &App,
    ) -> impl IntoElement {
        let losing = marks.iter().any(|m| !m.winning);
        let tip = marks
            .iter()
            .flat_map(|m| {
                let mut lines = Vec::new();
                for (key, mods) in [
                    ("gui-conflict-loses", &m.loses_to),
                    ("gui-conflict-wins", &m.wins_over),
                ] {
                    if mods.is_empty() {
                        continue;
                    }
                    let mut args = FluentArgs::new();
                    args.set("mods", mods.join(", "));
                    lines.push(format!(
                        "{} · {}",
                        dest_short(&m.dest),
                        self.strings.get_args(key, Some(&args))
                    ));
                }
                lines
            })
            .collect::<Vec<_>>()
            .join("\n");
        div()
            .id(id)
            .child(
                Icon::new(IconName::TriangleAlert)
                    .small()
                    .text_color(if losing {
                        cx.theme().warning
                    } else {
                        cx.theme().success
                    }),
            )
            .tooltip(move |window, cx| Tooltip::new(tip.clone()).build(window, cx))
    }

    /// One row per contested dest this card loses: the dest, who takes
    /// it, and a Make-win button that moves this card past the group's
    /// other members. Unreachable groups paint the row with a disabled
    /// button that says why.
    pub(crate) fn conflict_rows(
        &self,
        game_id: &str,
        inst: &str,
        marks: &[DestConflict],
        view: Entity<Self>,
        cx: &App,
    ) -> impl IntoElement {
        v_flex().gap_1().children(
            marks
                .iter()
                .filter(|m| !m.winning)
                .enumerate()
                .map(|(n, m)| {
                    let mut args = FluentArgs::new();
                    args.set("mods", m.loses_to.join(", "));
                    let text = format!(
                        "{} · {}",
                        dest_short(&m.dest),
                        self.strings.get_args("gui-conflict-loses", Some(&args))
                    );
                    let (disabled, tip) = match m.reachable {
                        Reachability::Reachable => (false, None),
                        Reachability::CrossSection => {
                            (true, Some(self.strings.get("gui-conflict-no-win-section")))
                        }
                        Reachability::PinnedKind => {
                            (true, Some(self.strings.get("gui-conflict-no-win-pinned")))
                        }
                    };
                    let game_id = game_id.to_string();
                    let inst = inst.to_string();
                    let group = m.group.clone();
                    let view = view.clone();
                    let mut button =
                        widgets::btn(SharedString::from(format!("make-win-{inst}-{n}")), cx)
                            .secondary()
                            .child(widgets::blabel(self.strings.get("gui-action-make-win"), cx))
                            .disabled(disabled)
                            .on_click(move |_, _, cx| {
                                let view = view.clone();
                                let game_id = game_id.clone();
                                let inst = inst.clone();
                                let group = group.clone();
                                view.update(cx, |this, cx| {
                                    this.make_win_ui(&game_id, &inst, &group, cx);
                                });
                            });
                    if let Some(tip) = tip {
                        button = button.tooltip(tip);
                    }
                    h_flex()
                        .gap_2()
                        .items_center()
                        .child(widgets::muted(text, cx))
                        .child(button)
                        .into_any_element()
                }),
        )
    }
}
