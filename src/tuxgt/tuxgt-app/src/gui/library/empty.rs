use gpui_kit::component::button::ButtonVariants as _;
use gpui_kit::component::{h_flex, v_flex, ActiveTheme, Icon, IconName};
use gpui_kit::*;
use tuxgt_core::{
    build_launch_spec, data_dir, open_db_shared, FluentArgs, LaunchPaths, PluginHost,
};

use super::super::theme::{types, TypeStyled as _};
use super::super::widgets;
use super::super::{rt_block, Shell};

impl Shell {
    pub(crate) fn empty_library(&self, view: Entity<Self>, cx: &App) -> impl IntoElement {
        let b = cx.theme();
        v_flex()
            .id("empty")
            .gap_4()
            .items_center()
            .py_8()
            .child(
                div()
                    .w(px(72.))
                    .h(px(72.))
                    .rounded(px(6.))
                    .border_1()
                    .border_color(b.border)
                    .border_dashed()
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(Icon::new(IconName::Folder).text_color(cx.theme().muted_foreground)),
            )
            .child(
                div()
                    .tx(types(cx).headline_md)
                    .child(self.strings.get("gui-empty-no-games")),
            )
            .child(widgets::muted(self.strings.get("gui-empty-scan-hint"), cx))
            .child(
                h_flex()
                    .gap_3()
                    .items_start()
                    .child(self.provider_card(
                        self.strings.get("gui-provider-steam-title"),
                        self.strings.get("gui-provider-steam-body"),
                        self.strings.get("gui-provider-steam-action"),
                        "scan-steam",
                        view.clone(),
                        cx,
                    ))
                    .child(self.provider_card(
                        self.strings.get("gui-provider-heroic-title"),
                        self.strings.get("gui-provider-heroic-body"),
                        self.strings.get("gui-provider-heroic-action"),
                        "scan-heroic",
                        view.clone(),
                        cx,
                    )),
            )
            .child(
                widgets::btn("cfg-paths", cx)
                    .secondary()
                    .child(widgets::bicon(IconName::Settings))
                    .child(widgets::blabel(
                        self.strings.get("gui-action-configure-paths"),
                        cx,
                    ))
                    .on_click({
                        let view = view.clone();
                        move |_, _, cx| {
                            view.update(cx, |this, cx| this.enter_settings(cx));
                        }
                    }),
            )
    }

    pub(crate) fn provider_card(
        &self,
        title: String,
        body: String,
        action: String,
        id: &'static str,
        view: Entity<Self>,
        cx: &App,
    ) -> impl IntoElement {
        widgets::hairline_card(id, cx)
            .w(px(260.))
            .p_3()
            .gap_2()
            .child(div().tx(types(cx).headline_md).child(title))
            .child(widgets::muted(body, cx))
            .child(
                widgets::btn(format!("{id}-go"), cx)
                    .primary()
                    .child(widgets::blabel(action, cx))
                    .on_click(move |_, _, cx| {
                        view.update(cx, |this, cx| this.rescan(cx));
                    }),
            )
    }

    /// Vanilla launch for one Library card (E30 path): build the spec and
    /// spawn it without touching selection or navigation. No ReShade
    /// injection here; that is E33. R01: success path persists last-played.
    pub(crate) fn play_game(&mut self, id: String, cx: &mut Context<Self>) {
        // E70: the card Play shares the handled/loader inject path with the
        // game page. Warn only when this row's handle is on; handle-off Play
        // is vanilla and never toasts. Warning only — the launch always runs.
        let health = if self.handle.get(&id).copied().unwrap_or(false) {
            self.host_health_note()
        } else {
            None
        };
        let launch_id = id.clone();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    rt_block(async {
                        let pool = open_db_shared(&data_dir()).await?;
                        let host = PluginHost::load()?;
                        let paths = LaunchPaths::detect()?;
                        let spec = build_launch_spec(&pool, &host, &launch_id, &paths, &data_dir())
                            .await?;
                        spec.command_detached().spawn().map_err(tuxgt_core::Error::Io)?;
                        let now = tuxgt_core::touch_last_played(&pool, &launch_id).await?;
                        Ok((launch_id, now))
                    })
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                let mut args = FluentArgs::new();
                this.status = match result {
                    Ok((gid, now)) => {
                        if let Some(g) = this.games.iter_mut().find(|g| g.id == gid) {
                            g.last_played = Some(now);
                        }
                        // `recent` sort reads the stored base.
                        this.rebuild_base();
                        args.set("id", gid);
                        this.strings.get_args("gui-status-play-id", Some(&args))
                    }
                    Err(e) => {
                        args.set("error", e.to_string());
                        this.strings.get_args("gui-status-err-play", Some(&args))
                    }
                };
                if let Some(n) = health {
                    this.pending_note = Some(n);
                }
                cx.notify();
            });
        })
        .detach();
    }
}
