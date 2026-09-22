use gpui_kit::assets::IconName as FullIconName;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::menu::{DropdownMenu as _, PopupMenu, PopupMenuItem};
use gpui_kit::component::{
    h_flex, ActiveTheme, Disableable as _, Icon, IconName, Sizable as _, Size,
};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

use std::rc::Rc;
use std::path::Path;

use super::super::theme::{types, TypeStyled as _};
use super::*;

pub fn placeholder_note(text: impl Into<SharedString>, cx: &App) -> impl IntoElement {
    div()
        .tx(types(cx).body_md)
        .italic()
        .text_color(cx.theme().muted_foreground)
        .child(text.into())
}

/// GUI.md §7 button label (body-md size/line, semibold). Pass as a child:
/// Button paints its internal label slot at 16px, which would override an
/// outer `text_size`.
pub fn blabel(text: impl Into<SharedString>, cx: &App) -> Div {
    div()
        .min_w_0()
        .whitespace_nowrap()
        .text_ellipsis()
        .tx(types(cx).body_md)
        .font_weight(FontWeight::SEMIBOLD)
        .child(text.into())
}

/// Control-scale button icon (0.875rem, so it follows `font_scale`).
pub fn bicon(name: IconName) -> Icon {
    Icon::new(name).small()
}

/// GUI.md §3 control height (24 / 26 / 28). gpui-kit has no 26px Button size,
/// and a labeled `Size::Size` Button takes its height from the outer style —
/// so this sets both, with text/icons passed as children (`blabel`/`bicon`).
pub trait CtlButton {
    fn ctl(self, cx: &App) -> Self;
}

impl CtlButton for Button {
    fn ctl(self, cx: &App) -> Self {
        let h = types(cx).control_h;
        self.with_size(Size::Size(h)).h(h)
    }
}

/// Control-scale button constructor: `Button::new(id)` with `.ctl(cx)`
/// applied, so call sites stop repeating the 26px sizing tax. Chain
/// variants and children on the result as before — pixel-identical.
pub fn btn(id: impl Into<ElementId>, cx: &App) -> Button {
    Button::new(id).ctl(cx)
}

/// Page CTA (`Play` / `Enable & Play`, E99): taller **and** wider than a
/// control — 32 at Default with the extra horizontal pad and a `headline-sm`
/// semibold label. Compact/Large follow `font_scale` like `control_h`
/// (27 / 32 / 37). Variant and `on_click` stay with the caller (primary, success
/// when hooked).
pub fn page_cta(id: impl Into<ElementId>, label: impl Into<SharedString>, cx: &App) -> Button {
    let h = px((f32::from(types(cx).control_h) * (32. / 28.)).round());
    Button::new(id)
        .with_size(Size::Size(h))
        .h(h)
        .px(px(13.))
        .child(Icon::new(IconName::Play).small())
        .child(
            div()
                .min_w_0()
                .whitespace_nowrap()
                .text_ellipsis()
                .tx(types(cx).headline_sm)
                .font_weight(FontWeight::SEMIBOLD)
                .child(label.into()),
        )
}

/// Weighted row that wraps. `grow` is flex-grow (60/40 → 6 and 4).
pub fn value_btn(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    menu: impl Fn(PopupMenu, &mut Window, &mut Context<PopupMenu>) -> PopupMenu + 'static,
    cx: &App,
) -> impl IntoElement {
    btn(id, cx)
        .secondary()
        .child(blabel(label, cx))
        .child(bicon(IconName::ChevronDown))
        .dropdown_menu(menu)
}

/// Destructive cluster control: danger trash icon, the word (`Remove` /
/// `Uninstall`) is the tooltip — never the word or a `Close` glyph as the
/// control. Single encoding of the destroy rule: `SectionAction::destroy`
/// paints through here (see `action_cluster`). Callers may chain `disabled`
/// (this returns the `Button`).
pub fn destroy_btn(
    id: impl Into<ElementId>,
    word: impl Into<SharedString>,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    cx: &App,
) -> Button {
    btn(id, cx)
        .danger()
        .child(Icon::new(FullIconName::Trash).small())
        .tooltip(word)
        .on_click(on_click)
}

/// What an `open_btn` hands to the desktop — also picks its type icon.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum OpenKind {
    /// Directory: opened as-is.
    Folder,
    /// File or exe: revealed through its parent, never exec'd.
    File,
    /// URL: handed to the handler as-is.
    Link,
}

/// Open-external control: type icon + label + trailing `ExternalLink`.
/// Folder/File go through `open_location`, Link through the same `xdg-open`
/// path — no new dependency. A `None` target paints disabled. Returns the
/// `Button` so callers may chain `tooltip`.
pub fn open_btn(
    id: impl Into<ElementId>,
    kind: OpenKind,
    label: impl Into<SharedString>,
    target: Option<String>,
    cx: &App,
) -> Button {
    let icon = match kind {
        OpenKind::Folder => Icon::new(IconName::Folder).small(),
        OpenKind::File => Icon::new(IconName::File).small(),
        OpenKind::Link => Icon::new(FullIconName::Link).small(),
    };
    let missing = target.is_none();
    btn(id, cx)
        .secondary()
        .child(icon)
        .child(blabel(label, cx))
        .child(bicon(IconName::ExternalLink))
        .when_some(target, |this, t| {
            this.on_click(move |_, _, _| match kind {
                OpenKind::Link => {
                    let _ = std::process::Command::new("xdg-open").arg(&t).spawn();
                }
                OpenKind::Folder | OpenKind::File => open_location(&t),
            })
        })
        .disabled(missing)
}

/// Open a file in the desktop handler (`xdg-open`): the `OpenKind::Link`
/// path `open_btn` uses for URLs, applied to a file. Unlike
/// `open_location` (which reveals the parent), this opens the file itself.
/// Returns whether the handler launched (refocus only re-reads pills then).
pub fn open_file_external(path: &Path) -> bool {
    std::process::Command::new("xdg-open").arg(path).spawn().is_ok()
}

/// One action in a `section_header` / `action_cluster` cluster.
#[derive(Clone)]
pub struct SectionAction {
    pub(crate) id: SharedString,
    pub(crate) label: SharedString,
    pub(crate) icon: Option<Icon>,
    pub(crate) primary: bool,
    pub(crate) outline: bool,
    /// Destructive: paints as `destroy_btn` via `action_cluster`.
    pub(crate) danger: bool,
    pub(crate) tooltip: Option<SharedString>,
    pub(crate) enabled: bool,
    pub(crate) click: Rc<dyn Fn(&ClickEvent, &mut Window, &mut App)>,
}

impl SectionAction {
    pub fn new(
        id: impl Into<SharedString>,
        label: impl Into<SharedString>,
        click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            icon: None,
            primary: false,
            outline: false,
            danger: false,
            tooltip: None,
            enabled: true,
            click: Rc::new(click),
        }
    }

    /// Destructive cluster entry, last in the cluster — paints as
    /// `destroy_btn` with the word (`Uninstall visible`) as the tooltip.
    pub fn destroy(
        id: impl Into<SharedString>,
        word: impl Into<SharedString>,
        click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        Self::new(id, word, click).danger()
    }

    /// Leading glyph, as the section CTA painted before this helper.
    pub fn icon(mut self, icon: Icon) -> Self {
        self.icon = Some(icon);
        self
    }

    /// Filled primary variant — at most one per section.
    pub fn primary(mut self) -> Self {
        self.primary = true;
        self
    }

    /// Outline + hairline idle contrast (Force re-sync, E77).
    pub fn outline(mut self) -> Self {
        self.outline = true;
        self
    }

    pub(crate) fn danger(mut self) -> Self {
        self.danger = true;
        self
    }

    pub fn tooltip(mut self, tip: impl Into<SharedString>) -> Self {
        self.tooltip = Some(tip.into());
        self
    }

    /// Paint the action disabled — the click never fires.
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.enabled = !disabled;
        self
    }
}

/// ~560px two-col wrap breakpoint (`cols` at `min_w` 280): below it a
/// section's action cluster would wrap instead of sitting on the title line.
pub(crate) const HEADER_WRAP_W: f32 = 560.;

/// One action cluster: the actions far right, or the same actions inside a
/// `⋯` (`Ellipsis`) menu when the pane is below `HEADER_WRAP_W`.
pub(crate) fn action_cluster(
    id: &'static str,
    actions: Vec<SectionAction>,
    compact: bool,
    cx: &App,
) -> AnyElement {
    if compact {
        let menu_actions = actions;
        return btn(SharedString::from(format!("{id}-more")), cx)
            .secondary()
            .child(bicon(IconName::Ellipsis))
            .dropdown_menu(move |menu, _, _| {
                let mut menu = menu;
                for a in &menu_actions {
                    let click = a.click.clone();
                    menu = menu.item(
                        PopupMenuItem::new(a.label.clone())
                            .disabled(!a.enabled)
                            .on_click(move |ev, window, cx| click(ev, window, cx)),
                    );
                }
                menu
            })
            .into_any_element();
    }
    h_flex()
        .flex_shrink_0()
        .gap_1()
        .children(actions.into_iter().map(|a| {
            if a.danger {
                let click = a.click.clone();
                let tip = a.tooltip.clone().unwrap_or_else(|| a.label.clone());
                return destroy_btn(
                    a.id.clone(),
                    tip,
                    move |ev, window, cx| click(ev, window, cx),
                    cx,
                )
                .disabled(!a.enabled);
            }
            let mut b = btn(a.id.clone(), cx)
                .when(a.primary, |b| b.primary())
                .when(!a.primary, |b| b.secondary())
                .when(a.outline, |b| {
                    b.outline().border_1().border_color(cx.theme().border)
                });
            if let Some(icon) = a.icon.clone() {
                b = b.child(icon);
            }
            b = b.child(blabel(a.label.clone(), cx));
            if let Some(tip) = a.tooltip.clone() {
                b = b.tooltip(tip);
            }
            b.disabled(!a.enabled)
                .on_click(move |ev, window, cx| (a.click)(ev, window, cx))
        }))
        .into_any_element()
}

/// Section header: title left, actions far right. `width` is the pane the
/// header lays out in (the page scroller's `bounds()` — the app's wrap
/// signal); the cluster follows the `action_cluster` wrap rule.
pub fn section_header(
    id: &'static str,
    title: impl Into<SharedString>,
    width: Pixels,
    actions: Vec<SectionAction>,
    cx: &App,
) -> impl IntoElement {
    h_flex()
        .id(id)
        .w_full()
        .gap_2()
        .items_center()
        .child(div().flex_1().min_w_0().child(section_title(title, cx)))
        .child(action_cluster(id, actions, width < px(HEADER_WRAP_W), cx))
}
