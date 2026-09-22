use gpui_kit::component::{v_flex, ActiveTheme};
use gpui_kit::*;

/// E106 section card: the sidebar's own fill (`card` = `pane`), 8px corners,
/// elevation from a drop shadow plus a top highlight derived from the card
/// fill. No outer hairline (`overview.md` Layout chrome).
pub fn section_card(id: impl Into<ElementId>, cx: &App) -> Stateful<Div> {
    let b = cx.theme();
    let hi = hsla(
        b.group_box.h,
        b.group_box.s,
        (b.group_box.l + 0.05).clamp(0., 1.),
        b.group_box.a,
    );
    v_flex()
        .id(id)
        .gap_2()
        .px_3()
        .pt_2()
        .pb_3()
        .rounded(px(8.))
        .bg(b.group_box)
        .shadow_sm()
        .border_t_1()
        .border_color(hi)
}
