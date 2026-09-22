use gpui_kit::component::{v_flex, ActiveTheme, Root};
use gpui_kit::*;

use super::frame::frame_shell;
use super::notice::NoticeKind;
use super::*;

impl Render for Shell {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.evict_offpage_art(cx);
        self.tick_grid_metrics(cx);
        // E104: focus return runs the poll when due (one Instant check;
        // unfocused runs stay skipped at the timer).
        self.poll_on_focus_return(window.is_window_active(), cx);
        if self.add_sync_queued {
            self.add_sync_queued = false;
            self.sync_add_inputs(window, cx);
        }
        // E101: the two flush sites feed the owned store, never the kit
        // notification list. `status` is text-only, so severity is inferred
        // from the text (as the kit toast did); `pending_note` carries its
        // own kind.
        if !self.status.is_empty() {
            let s = std::mem::take(&mut self.status);
            let err =
                s.to_ascii_lowercase().contains("fail") || s.to_ascii_lowercase().contains("error");
            let kind = if err {
                NoticeKind::Err
            } else {
                NoticeKind::Info
            };
            self.emit_activity(kind, s, cx);
        }
        if let Some(n) = self.pending_note.take() {
            let (kind, text) = match n {
                Note::Info(s) => (NoticeKind::Info, s),
                Note::Ok(s) => (NoticeKind::Ok, s),
                Note::Warn(s) => (NoticeKind::Warn, s),
                Note::Err(s) => (NoticeKind::Err, s),
            };
            self.emit_activity(kind, text, cx);
        }
        let view = cx.entity();
        let frame_r = match window.window_decorations() {
            Decorations::Client { tiling } if !tiling.is_tiled() && !window.is_maximized() => {
                px(10.)
            }
            _ => px(0.),
        };
        let content = v_flex()
            .id("shell")
            .track_focus(&self.focus)
            .on_key_down({
                let view = view.clone();
                move |event: &KeyDownEvent, _, cx| {
                    // E101: Escape closes the sidecar (a click outside and
                    // the bell do too).
                    if event.keystroke.key != "escape" {
                        return;
                    }
                    view.update(cx, |this, cx| {
                        if this.sidecar_open {
                            this.close_sidecar(cx);
                        }
                    });
                }
            })
            .size_full()
            .relative()
            .overflow_hidden()
            .rounded(frame_r)
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .on_mouse_down(MouseButton::Navigate(NavigationDirection::Back), {
                let view = view.clone();
                move |_, _, cx| {
                    cx.stop_propagation();
                    view.update(cx, |this, cx| this.go_back(cx));
                }
            })
            .on_mouse_down(MouseButton::Navigate(NavigationDirection::Forward), {
                let view = view.clone();
                move |_, _, cx| {
                    cx.stop_propagation();
                    view.update(cx, |this, cx| this.go_forward(cx));
                }
            })
            .child(
                div()
                    .id("chrome-top")
                    .flex_none()
                    .w_full()
                    .child(self.titlebar(view.clone(), frame_r, window, cx)),
            )
            .child(
                // Do not use h_flex(): it is items_center, so a content-tall
                // main column is vertically centered, covers the titlebar, and
                // never shrinks enough for overflow_y_scroll to take effect.
                div()
                    .id("body")
                    .flex()
                    .flex_row()
                    .items_stretch()
                    .flex_1()
                    .min_h_0()
                    .w_full()
                    .overflow_hidden()
                    .child(self.sidebar(view.clone(), frame_r, cx))
                    .child(
                        v_flex()
                            .id("main")
                            .flex_1()
                            .min_w_0()
                            .h_full()
                            .min_h_0()
                            .overflow_hidden()
                            .rounded_br(frame_r)
                            .bg(cx.theme().background)
                            .child(self.page_chrome(view.clone(), cx))
                            .child(self.page_scroller(view.clone(), cx)),
                    ),
            )
            // E101: the app owns its toast stack; the kit notification
            // layer is gone so nothing can paint a second, unexpirable one.
            .children(self.notice_layer(view.clone(), cx))
            .children(self.client_stop_layer(view.clone(), cx))
            .children(self.config_discard_layer(view.clone(), cx))
            // `Root`'s own render paints no dialogs: `open_dialog` is invisible
            // until the app view renders this layer (E62 picker).
            .children(Root::render_dialog_layer(window, cx));
        // E120: CSD frame owned by the app (`Root` is `bordered(false)`).
        frame_shell(content, window)
    }
}
