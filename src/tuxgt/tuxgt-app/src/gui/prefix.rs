use gpui_kit::component::button::ButtonVariants as _;
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::*;
use tuxgt_core::FluentArgs;

use super::widgets;
use super::Shell;

impl Shell {
    pub(super) fn prefix_body(&self, view: Entity<Self>, cx: &App) -> impl IntoElement {
        let Some(g) = self.selected_game() else {
            return v_flex().id("prefix-empty").into_any_element();
        };
        let pfx = g.prefix_path.clone();
        let native = g.platform.as_deref() == Some("native") || pfx.is_none();

        if native {
            return widgets::section_card("prefix", cx)
                .w_full()
                .flex_shrink_0()
                .child(widgets::section_title(
                    self.strings.get("gui-section-no-prefix"),
                    cx,
                ))
                .child(widgets::muted(
                    self.strings.get("gui-note-native-prefix"),
                    cx,
                ))
                .into_any_element();
        }

        let path = pfx.clone().unwrap();
        let proton = g.proton.clone();

        widgets::section_card("prefix", cx)
            .w_full()
            .flex_shrink_0()
            .child(widgets::section_title(
                self.strings.get("gui-section-tools"),
                cx,
            ))
            .child({
                let tool = |id: &'static str, name: &'static str| {
                    let view = view.clone();
                    let prefix = path.clone();
                    let proton = proton.clone();
                    widgets::btn(id, cx)
                        .secondary()
                        .child(widgets::blabel(name, cx))
                        .on_click(move |_, _, cx| {
                            view.update(cx, |this, cx| {
                                this.run_prefix_tool(name, prefix.clone(), proton.clone(), cx)
                            });
                        })
                };
                h_flex()
                    .gap_1()
                    .w_full()
                    .child(tool("winecfg", "winecfg"))
                    .child(tool("regedit", "regedit"))
                    .child(tool("explorer", "explorer"))
                    .child(tool("winetricks", "winetricks"))
            })
            .into_any_element()
    }

    pub(super) fn copy_text(&mut self, text: String, status_id: &str, cx: &mut Context<Self>) {
        cx.write_to_clipboard(ClipboardItem::new_string(text));
        self.status = self.strings.get(status_id);
        cx.notify();
    }

    /// R29: launch a prefix tool detached with `WINEPREFIX` set. Wine
    /// builtins (`winecfg`, `regedit`, `explorer`) run through the detected
    /// system wine (or a direct-path proton); `winetricks` runs from `PATH`.
    /// A missing runner is an honest status, never a disabled button.
    fn run_prefix_tool(
        &mut self,
        tool: &'static str,
        prefix: String,
        proton: Option<String>,
        cx: &mut Context<Self>,
    ) {
        let mut args = FluentArgs::new();
        args.set("tool", tool);
        self.status = self.strings.get_args("gui-status-tool-run", Some(&args));
        cx.notify();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    super::rt_block(async {
                        let paths = tuxgt_core::LaunchPaths::detect()?;
                        let mut cmd = if tool == "winetricks" {
                            std::process::Command::new("winetricks")
                        } else {
                            let bin = proton
                                .and_then(|p| {
                                    let path = std::path::PathBuf::from(&p);
                                    path.is_file().then_some(path)
                                })
                                .or_else(|| paths.wine.clone())
                                .ok_or_else(|| tuxgt_core::Error::MissingRunner(tool.into()))?;
                            let mut c = std::process::Command::new(bin);
                            c.arg(tool);
                            c
                        };
                        cmd.env("WINEPREFIX", &prefix);
                        cmd.env("STEAM_COMPAT_DATA_PATH", &prefix);
                        cmd.spawn().map_err(tuxgt_core::Error::Io)?;
                        Ok::<_, tuxgt_core::Error>(tool)
                    })
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.status = match result {
                    Ok(t) => {
                        let mut args = FluentArgs::new();
                        args.set("tool", t);
                        this.strings
                            .get_args("gui-status-tool-launched", Some(&args))
                    }
                    Err(e) => {
                        let mut args = FluentArgs::new();
                        args.set("error", e.to_string());
                        this.strings.get_args("gui-status-err-tool", Some(&args))
                    }
                };
                cx.notify();
            });
        })
        .detach();
    }
}
