//! Linux CSD frame applied directly to the Shell view (E120).
//!
//! Adapted from `gpui-component` 0.6.4 `window_border.rs`, kept whole to
//! stay diffable against upstream (`gpui-component-0.6.4/src/window_border.rs`;
//! re-diff on every kit bump): same shadow, client-inset, and resize
//! geometry, but the 1px frame stroke is transparent (kit hardcodes its
//! color), there is no separate wrapper element — `Root` runs with
//! `bordered(false)` and `frame_shell` wraps our content — and the input
//! region is clipped to the visible frame (kit sets none, so its shadow
//! swallows clicks). Upstream still
//! clips children to rectangles, so rounding paints our own backgrounds
//! only; content reaching past a rounded container stays square.

use gpui_kit::gpui::{
    div, point, px, transparent_black, AnyElement, Bounds, BoxShadow, CursorStyle, Decorations,
    Edges, Hsla, InteractiveElement as _, IntoElement, MouseButton, ParentElement, Pixels, Point,
    ResizeEdge, Size, Styled as _, Tiling, Window,
};
use gpui_kit::prelude::FluentBuilder;

#[cfg(not(target_os = "linux"))]
const SHADOW_SIZE: Pixels = px(0.0);
#[cfg(target_os = "linux")]
const SHADOW_SIZE: Pixels = px(20.0);
const BORDER_SIZE: Pixels = px(1.0);
/// Half-width of the resize hit band on each side of the visible frame.
const RESIZE_HIT_SIZE: Pixels = px(4.0);

/// Transparent frame stroke. The 1px border keeps kit-identical geometry;
/// only its paint goes away.
fn frame_stroke() -> Hsla {
    Hsla {
        h: 0.,
        s: 0.,
        l: 0.,
        a: 0.,
    }
}

/// Per-side inset of the visible frame from the outer window bounds.
fn client_frame_insets(shadow_size: Pixels, tiling: &Tiling) -> Edges<Pixels> {
    let mut insets = Edges::all(shadow_size);
    if tiling.top {
        insets.top = px(0.0);
    }
    if tiling.bottom {
        insets.bottom = px(0.0);
    }
    if tiling.left {
        insets.left = px(0.0);
    }
    if tiling.right {
        insets.right = px(0.0);
    }
    insets
}

/// Wrap Shell content in the CSD frame: shadow padding, transparent 1px
/// stroke, edge resize, tiling-aware insets. Server decorations pass through.
pub(crate) fn frame_shell(content: impl IntoElement, window: &mut Window) -> impl IntoElement {
    let decorations = window.window_decorations();
    // Divergence: defensive maximized→tiled. Wayland/X11 already force
    // Tiling::tiled() when maximized/fullscreen; this is a no-op on
    // current backends and keeps insets/shadow/resize coherent if a
    // future backend reports maximized without tiling.
    let decorations = if window.is_maximized() {
        match decorations {
            Decorations::Client { .. } => Decorations::Client {
                tiling: Tiling::tiled(),
            },
            _ => decorations,
        }
    } else {
        decorations
    };
    let platform_inset = SHADOW_SIZE;
    let visual_shadow = match decorations {
        Decorations::Client { tiling }
            if tiling.top && tiling.bottom && tiling.left && tiling.right =>
        {
            px(0.0)
        }
        _ => SHADOW_SIZE,
    };
    if matches!(decorations, Decorations::Client { .. }) {
        // Keep the platform inset stable when fully tiled, else the first
        // resize after restore double-counts the shadow and jumps (kit).
        window.set_client_inset(platform_inset);
    }
    // Live surface size: `window.bounds().size` is the synchronous platform
    // query (global coords — use `.size` only); `viewport_size()` is a
    // cached copy from the last `bounds_changed` and can lag on the first
    // frame. `window_bounds()` also goes stale while maximized/fullscreen.
    let window_size = window.bounds().size;
    let active = window.is_window_active();
    let stroke = frame_stroke();
    // Divergence: Server decorations use the compositor default whole-window
    // input region. Skipping `set_input_region(None)` avoids a commit on
    // every render (each call commits the Wayland surface, even `None`).
    if let Decorations::Client { tiling } = decorations {
        let insets = client_frame_insets(platform_inset, &tiling);
        window.set_input_region(Some(&[input_bounds(window_size, insets, &tiling)]));
    }

    div()
        .id("window-backdrop")
        .bg(transparent_black())
        .map(|div| match decorations {
            Decorations::Server => div,
            Decorations::Client { tiling, .. } => div
                .flex()
                .flex_col()
                .overflow_hidden()
                .bg(transparent_black())
                .when(!tiling.top, |div| div.pt(visual_shadow))
                .when(!tiling.bottom, |div| div.pb(visual_shadow))
                .when(!tiling.left, |div| div.pl(visual_shadow))
                .when(!tiling.right, |div| div.pr(visual_shadow))
                .on_mouse_down(MouseButton::Left, move |_, window, _| {
                    let Decorations::Client { tiling } = window.window_decorations() else {
                        return;
                    };
                    // Keep resize disabled when maximized; defensive tiled
                    // mirror of the render-time shadowing above (no-op on
                    // current backends which already force tiling).
                    let tiling = if window.is_maximized() {
                        Tiling::tiled()
                    } else {
                        tiling
                    };
                    if tiling.top && tiling.bottom && tiling.left && tiling.right {
                        return;
                    }
                    let size = window.bounds().size;
                    let pos = window.mouse_position();
                    let insets = client_frame_insets(platform_inset, &tiling);
                    if let Some(edge) = resize_edge(pos, size, insets, &tiling, RESIZE_HIT_SIZE) {
                        window.start_window_resize(edge);
                    }
                }),
        })
        .size_full()
        .child(
            div()
                .cursor(CursorStyle::default())
                .map(|div| match decorations {
                    Decorations::Server => div.size_full(),
                    Decorations::Client { tiling } => div
                        .flex_1()
                        .min_h_0()
                        .min_w_0()
                        .overflow_hidden()
                        .border_color(stroke)
                        .when(!tiling.top, |div| div.border_t(BORDER_SIZE))
                        .when(!tiling.bottom, |div| div.border_b(BORDER_SIZE))
                        .when(!tiling.left, |div| div.border_l(BORDER_SIZE))
                        .when(!tiling.right, |div| div.border_r(BORDER_SIZE))
                        .when(!tiling.is_tiled(), |div| {
                            let opacity = if active { 1.0 } else { 0.7 };
                            // Keep the outer reach below SHADOW_SIZE: gpui
                            // does not grow paint bounds for blur (kit).
                            div.shadow(vec![
                                BoxShadow {
                                    color: Hsla {
                                        h: 0.,
                                        s: 0.,
                                        l: 0.,
                                        a: 0.18 * opacity,
                                    },
                                    blur_radius: px(10.),
                                    spread_radius: px(-1.),
                                    offset: point(px(0.0), px(2.0)),
                                    inset: false,
                                },
                                BoxShadow {
                                    color: Hsla {
                                        h: 0.,
                                        s: 0.,
                                        l: 0.,
                                        a: 0.18 * opacity,
                                    },
                                    blur_radius: px(3.),
                                    spread_radius: px(0.),
                                    offset: point(px(0.0), px(1.0)),
                                    inset: false,
                                },
                            ])
                        }),
                })
                .on_mouse_move(|_e, _, cx| {
                    cx.stop_propagation();
                })
                .bg(transparent_black())
                .child(content),
        )
        .when(matches!(decorations, Decorations::Client { .. }), |this| {
            let Decorations::Client { tiling, .. } = decorations else {
                return this;
            };
            // Resize hit zones are cursor-only overlays; they must stay
            // handler-free — backdrop `on_mouse_down` + `resize_edge` owns
            // resize. No handlers here.
            this.child(div().absolute().size_full().children(resize_hit_zones(
                window_size,
                platform_inset,
                RESIZE_HIT_SIZE,
                &tiling,
            )))
        })
}

fn cursor_style_for_resize_edge(edge: ResizeEdge) -> CursorStyle {
    match edge {
        ResizeEdge::Top | ResizeEdge::Bottom => CursorStyle::ResizeUpDown,
        ResizeEdge::Left | ResizeEdge::Right => CursorStyle::ResizeLeftRight,
        ResizeEdge::TopLeft | ResizeEdge::BottomRight => CursorStyle::ResizeUpLeftDownRight,
        ResizeEdge::TopRight | ResizeEdge::BottomLeft => CursorStyle::ResizeUpRightDownLeft,
    }
}

/// Cursor-only overlay per resize edge/corner. Resize starts from the
/// backdrop `on_mouse_down` via `resize_edge` (kit).
/// Must stay handler-free: the backdrop owns resize, these divs only set
/// cursor. Adding handlers here would fragment hit-testing.
fn resize_hit_zones(
    window_size: Size<Pixels>,
    shadow_size: Pixels,
    hit_size: Pixels,
    tiling: &Tiling,
) -> Vec<AnyElement> {
    if tiling.top && tiling.bottom && tiling.left && tiling.right {
        return Vec::new();
    }

    let insets = client_frame_insets(shadow_size, tiling);
    let inner_left = insets.left;
    let inner_right = window_size.width - insets.right;
    let inner_top = insets.top;
    let inner_bottom = window_size.height - insets.bottom;
    let frame_origin = point(insets.left, insets.top);
    let band = hit_size + hit_size;
    let span_x = inner_right - inner_left + band;
    let span_y = inner_bottom - inner_top + band;

    let mut zones: Vec<AnyElement> = Vec::new();
    let mut push_zone = |edge: ResizeEdge, origin: Point<Pixels>, zone_size: Size<Pixels>| {
        let origin = origin - frame_origin;
        zones.push(
            div()
                .absolute()
                .left(origin.x)
                .top(origin.y)
                .w(zone_size.width)
                .h(zone_size.height)
                .cursor(cursor_style_for_resize_edge(edge))
                .into_any_element(),
        );
    };

    if !tiling.top {
        let o = point(inner_left - hit_size, inner_top - hit_size);
        push_zone(ResizeEdge::Top, o, Size::new(span_x, band));
    }
    if !tiling.bottom {
        let o = point(inner_left - hit_size, inner_bottom - hit_size);
        push_zone(ResizeEdge::Bottom, o, Size::new(span_x, band));
    }
    if !tiling.left {
        let o = point(inner_left - hit_size, inner_top - hit_size);
        push_zone(ResizeEdge::Left, o, Size::new(band, span_y));
    }
    if !tiling.right {
        let o = point(inner_right - hit_size, inner_top - hit_size);
        push_zone(ResizeEdge::Right, o, Size::new(band, span_y));
    }
    // Corners last so hit-testing prefers them over adjacent edges.
    if !tiling.top && !tiling.left {
        let o = point(inner_left - hit_size, inner_top - hit_size);
        push_zone(ResizeEdge::TopLeft, o, Size::new(band, band));
    }
    if !tiling.top && !tiling.right {
        let o = point(inner_right - hit_size, inner_top - hit_size);
        push_zone(ResizeEdge::TopRight, o, Size::new(band, band));
    }
    if !tiling.bottom && !tiling.left {
        let o = point(inner_left - hit_size, inner_bottom - hit_size);
        push_zone(ResizeEdge::BottomLeft, o, Size::new(band, band));
    }
    if !tiling.bottom && !tiling.right {
        let o = point(inner_right - hit_size, inner_bottom - hit_size);
        push_zone(ResizeEdge::BottomRight, o, Size::new(band, band));
    }

    zones
}

/// Input region: the visible frame plus the outer resize rim, so shadow
/// clicks pass through to whatever is behind (kit sets no region).
fn input_bounds(size: Size<Pixels>, insets: Edges<Pixels>, tiling: &Tiling) -> Bounds<Pixels> {
    let rim = f32::from(RESIZE_HIT_SIZE);
    let x0 = (f32::from(insets.left) - if tiling.left { 0. } else { rim }).max(0.);
    let y0 = (f32::from(insets.top) - if tiling.top { 0. } else { rim }).max(0.);
    let x1 = (f32::from(size.width) - f32::from(insets.right)
        + if tiling.right { 0. } else { rim })
    .min(f32::from(size.width));
    let y1 = (f32::from(size.height) - f32::from(insets.bottom)
        + if tiling.bottom { 0. } else { rim })
    .min(f32::from(size.height));
    // A window narrower than its insets (mid-resize) must not yield a
    // negative region rect: compositors treat that as a protocol error.
    Bounds {
        origin: point(px(x0), px(y0)),
        size: Size::new(px((x1 - x0).max(0.)), px((y1 - y0).max(0.))),
    }
}

/// Hit-test resize edges on a narrow band around the visible inner frame,
/// not the full shadow padding (kit).
fn resize_edge(
    pos: Point<Pixels>,
    size: Size<Pixels>,
    insets: Edges<Pixels>,
    tiling: &Tiling,
    hit_size: Pixels,
) -> Option<ResizeEdge> {
    let inner_left = insets.left;
    let inner_right = size.width - insets.right;
    let inner_top = insets.top;
    let inner_bottom = size.height - insets.bottom;

    let on_left = pos.x >= inner_left - hit_size
        && pos.x <= inner_left + hit_size
        && pos.y >= inner_top - hit_size
        && pos.y <= inner_bottom + hit_size;
    let on_right = pos.x >= inner_right - hit_size
        && pos.x <= inner_right + hit_size
        && pos.y >= inner_top - hit_size
        && pos.y <= inner_bottom + hit_size;
    let on_top = pos.y >= inner_top - hit_size
        && pos.y <= inner_top + hit_size
        && pos.x >= inner_left - hit_size
        && pos.x <= inner_right + hit_size;
    let on_bottom = pos.y >= inner_bottom - hit_size
        && pos.y <= inner_bottom + hit_size
        && pos.x >= inner_left - hit_size
        && pos.x <= inner_right + hit_size;

    if !tiling.top && !tiling.left && on_top && on_left {
        return Some(ResizeEdge::TopLeft);
    }
    if !tiling.top && !tiling.right && on_top && on_right {
        return Some(ResizeEdge::TopRight);
    }
    if !tiling.bottom && !tiling.left && on_bottom && on_left {
        return Some(ResizeEdge::BottomLeft);
    }
    if !tiling.bottom && !tiling.right && on_bottom && on_right {
        return Some(ResizeEdge::BottomRight);
    }
    if !tiling.top && on_top {
        return Some(ResizeEdge::Top);
    }
    if !tiling.bottom && on_bottom {
        return Some(ResizeEdge::Bottom);
    }
    if !tiling.left && on_left {
        return Some(ResizeEdge::Left);
    }
    if !tiling.right && on_right {
        return Some(ResizeEdge::Right);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn untiled() -> Tiling {
        Tiling {
            top: false,
            left: false,
            right: false,
            bottom: false,
        }
    }

    #[test]
    fn stroke_is_transparent() {
        assert_eq!(frame_stroke().a, 0.);
    }

    #[test]
    fn insets_follow_tiling() {
        let all = client_frame_insets(SHADOW_SIZE, &untiled());
        assert_eq!([all.top, all.left, all.right, all.bottom], [SHADOW_SIZE; 4]);
        let none = client_frame_insets(SHADOW_SIZE, &Tiling::tiled());
        assert_eq!([none.top, none.left, none.right, none.bottom], [px(0.); 4]);
        let top = Tiling {
            top: true,
            ..untiled()
        };
        let part = client_frame_insets(SHADOW_SIZE, &top);
        assert_eq!(part.top, px(0.));
        assert_eq!([part.left, part.right, part.bottom], [SHADOW_SIZE; 3]);
    }

    #[test]
    fn input_region_is_frame_plus_rim() {
        let size = Size::new(px(1280.), px(800.));
        let insets = client_frame_insets(SHADOW_SIZE, &untiled());
        let b = input_bounds(size, insets, &untiled());
        assert_eq!(b.origin, point(px(16.), px(16.)));
        assert_eq!(b.size, Size::new(px(1248.), px(768.)));
        let top = Tiling {
            top: true,
            ..untiled()
        };
        let insets = client_frame_insets(SHADOW_SIZE, &top);
        let b = input_bounds(size, insets, &top);
        assert_eq!(b.origin, point(px(16.), px(0.)));
        let tiled = Tiling::tiled();
        let insets = client_frame_insets(SHADOW_SIZE, &tiled);
        let b = input_bounds(size, insets, &tiled);
        assert_eq!(b.origin, point(px(0.), px(0.)));
        assert_eq!(b.size, size);
        let tiny = Size::new(px(30.), px(30.));
        let insets = client_frame_insets(SHADOW_SIZE, &untiled());
        let b = input_bounds(tiny, insets, &untiled());
        assert_eq!(b.size, Size::new(px(0.), px(0.)));
    }

    #[test]
    fn resize_hits_corners_first() {
        let size = Size::new(px(1000.), px(800.));
        let insets = client_frame_insets(SHADOW_SIZE, &untiled());
        let at = |x: Pixels, y: Pixels| {
            resize_edge(point(x, y), size, insets, &untiled(), RESIZE_HIT_SIZE)
        };
        assert_eq!(at(insets.left, insets.top), Some(ResizeEdge::TopLeft));
        assert_eq!(
            at(size.width - insets.right, px(400.)),
            Some(ResizeEdge::Right)
        );
        assert_eq!(at(px(500.), px(400.)), None);
        let tiled = Tiling::tiled();
        assert_eq!(
            resize_edge(
                point(insets.left, insets.top),
                size,
                insets,
                &tiled,
                RESIZE_HIT_SIZE
            ),
            None
        );
    }
}
