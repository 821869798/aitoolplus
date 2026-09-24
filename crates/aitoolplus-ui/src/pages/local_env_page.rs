//! Local environment tab: one card per Agent CLI.
use gpui::{Context, IntoElement, div, prelude::*, px};
use crate::components::{ButtonVariant, button_with_icon_loading_l, spinner};
use crate::workspace::Workspace;

fn start_local_env_scan(cx: &mut Context<Workspace>) {
    let weak = cx.entity().downgrade();
    cx.spawn(async move |_this, cx| {
        let tools = cx
            .background_spawn(async { aitoolplus_core::local_env::check_all() })
            .await;
        let _ = weak.update(cx, |ws, cx| {
            ws.ui.local_env_tools = tools;
            ws.ui.local_env_loading = false;
            ws.ui.local_env_loaded = true;
            cx.notify();
        });
    })
    .detach();
}

fn run_local_env_action(id: String, install: bool, cx: &mut Context<Workspace>) {
    let weak = cx.entity().downgrade();
    let id_for_task = id.clone();
    cx.spawn(async move |_this, cx| {
        let result = cx
            .background_spawn(async move {
                aitoolplus_core::local_env::run_action(&id_for_task, install)
            })
            .await;
        let refreshed = cx
            .background_spawn(async move { aitoolplus_core::local_env::check_one_id(&id) })
            .await;
        let _ = weak.update(cx, |ws, cx| {
            ws.ui.local_env_busy = None;
            let mut unchanged = false;
            if let Some(status) = refreshed {
                if let Some(slot) = ws.ui.local_env_tools.iter_mut().find(|tool| tool.id == status.id) {
                    unchanged = !install && status.version == slot.version;
                    *slot = status;
                }
            }
            match result {
                Ok(()) if unchanged => ws.ui.toast(
                    "版本没有变化。当前安装源的最新版就是这个版本".to_string(),
                    false,
                ),
                Ok(()) => ws.ui.toast(
                    if install { "安装完成" } else { "升级完成" }.to_string(),
                    false,
                ),
                Err(error) => ws.ui.toast(format!("操作失败: {error}"), true),
            }
            cx.notify();
        });
    })
    .detach();
}

pub(super) fn local_env_tab(ws: &mut Workspace, cx: &mut Context<Workspace>) -> gpui::AnyElement {
    if !ws.ui.local_env_loaded && !ws.ui.local_env_loading {
        ws.ui.local_env_loading = true;
        start_local_env_scan(cx);
    }
    let t = ws.theme.clone();
    let i = ws.i18n;
    let busy = ws.ui.local_env_busy.clone();
    let loading = ws.ui.local_env_loading;
    let updatable = ws
        .ui
        .local_env_tools
        .iter()
        .filter(|tool| {
            tool.version.as_deref().is_some_and(|local| {
                tool.latest_version
                    .as_deref()
                    .is_some_and(|latest| aitoolplus_core::local_env::update_available(local, latest))
            })
        })
        .count();

    let mut grid = div().flex().flex_wrap().gap(px(12.0));
    if loading && ws.ui.local_env_tools.is_empty() {
        grid = grid.child(
            div()
                .text_size(px(13.0))
                .text_color(t.text_muted)
                .child(i.t("正在检查…", "Checking…")),
        );
    }
    for tool in ws.ui.local_env_tools.clone() {
        let outdated = tool.version.as_deref().is_some_and(|local| {
            tool.latest_version
                .as_deref()
                .is_some_and(|latest| aitoolplus_core::local_env::update_available(local, latest))
        });
        let tool_busy = busy.as_deref() == Some(tool.id.as_str()) || busy.as_deref() == Some("*");
        let checking = loading && tool.version.is_none() && tool.error.is_none();
        let current = if checking {
            i.t("检查中", "Checking").to_string()
        } else if tool.installed_but_broken {
            i.t("已安装但无法运行", "Installed but cannot run").to_string()
        } else {
            tool.version
                .clone()
                .unwrap_or_else(|| i.t("未安装", "Not installed").to_string())
        };
        let latest = tool
            .latest_version
            .clone()
            .unwrap_or_else(|| i.t("未知", "Unknown").to_string());
        let id = tool.id.clone();
        let action = if checking || tool.installed_but_broken {
            None
        } else if tool.version.is_none() {
            Some((true, i.t("安装", "Install")))
        } else if outdated {
            Some((false, i.t("升级", "Update")))
        } else {
            None
        };
        let status_icon = if checking {
            spinner(gpui::SharedString::from(format!("env-spin-{id}")), t.text_muted)
        } else if tool.version.is_some() && tool.latest_version.is_some() && !outdated {
            gpui::svg()
                .data(crate::icons::CHECK_SVG)
                .size(px(16.0))
                .text_color(t.success)
                .into_any_element()
        } else {
            gpui::svg()
                .data(crate::icons::ALERT_SVG)
                .size(px(16.0))
                .text_color(t.warning)
                .into_any_element()
        };
        let mut card = div()
            .flex()
            .flex_col()
            .gap(px(12.0))
            .min_w(px(280.0))
            .flex_1()
            .min_h(px(156.0))
            .p(px(16.0))
            .rounded(px(12.0))
            .bg(t.card_bg)
            .border_1()
            .border_color(t.card_border)
            .shadow_xs()
            .child(
                div()
                    .flex()
                    .items_start()
                    .justify_between()
                    .gap(px(8.0))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .min_w(px(0.0))
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .size(px(28.0))
                                    .rounded(px(6.0))
                                    .bg(t.bg)
                                    .child(
                                        gpui::svg()
                                            .data(local_env_icon(&tool.id))
                                            .size(px(16.0))
                                            .text_color(t.text_primary),
                                    ),
                            )
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap(px(4.0))
                                    .min_w(px(0.0))
                                    .child(
                                        div()
                                            .text_size(px(13.0))
                                            .font_weight(gpui::FontWeight::MEDIUM)
                                            .text_color(t.text_primary)
                                            .child(tool.name.clone()),
                                    )
                                    .child(
                                        div()
                                            .px(px(6.0))
                                            .py(px(1.0))
                                            .rounded_full()
                                            .border_1()
                                            .border_color(t.card_border)
                                            .text_size(px(10.0))
                                            .text_color(t.text_muted)
                                            .child(tool.source.clone()),
                                    ),
                            ),
                    )
                    .child(status_icon),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(6.0))
                    .child(version_line(&t, i.t("当前版本", "Current"), current))
                    .child(version_line(&t, i.t("最新版本", "Latest"), latest)),
            );
        if let Some(error) = tool.error.clone().filter(|_| tool.version.is_none() && !checking) {
            card = card.child(
                div()
                    .text_size(px(11.0))
                    .text_color(t.text_muted)
                    .child(error),
            );
        }
        let footer = if tool.installed_but_broken {
            div()
                .text_size(px(12.0))
                .text_color(t.warning)
                .child(i.t("请检查运行环境", "Check the runtime"))
                .into_any_element()
        } else if let Some((install, label)) = action {
            button_with_icon_loading_l(
                gpui::SharedString::from(format!("local-env-{id}")),
                if install {
                    crate::icons::DOWNLOAD_SVG
                } else {
                    crate::icons::REFRESH_SVG
                },
                label,
                if install {
                    ButtonVariant::Secondary
                } else {
                    ButtonVariant::Primary
                },
                tool_busy,
                &t,
                cx,
                move |ws, _, _, cx| {
                    if ws.ui.local_env_busy.is_some() || ws.ui.local_env_loading {
                        return;
                    }
                    ws.ui.local_env_busy = Some(id.clone());
                    cx.notify();
                    run_local_env_action(id.clone(), install, cx);
                },
            )
        } else if tool.version.is_some() && tool.latest_version.is_some() {
            div()
                .text_size(px(12.0))
                .text_color(t.text_muted)
                .child(i.t("已是最新", "Up to date"))
                .into_any_element()
        } else {
            div().into_any_element()
        };
        card = card.child(
            div()
                .flex()
                .flex_grow(1.0)
                .items_end()
                .justify_end()
                .child(footer),
        );
        grid = grid.child(card);
    }

    div()
        .flex()
        .flex_col()
        .gap(px(12.0))
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .gap(px(12.0))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(2.0))
                        .child(
                            div()
                                .text_size(px(14.0))
                                .font_weight(gpui::FontWeight::MEDIUM)
                                .text_color(t.text_primary)
                                .child(i.t("本地环境检查", "Local Environment")),
                        )
                        .child(
                            div()
                                .text_size(px(12.0))
                                .text_color(t.text_muted)
                                .child(i.t(
                                    "检查本机 Agent CLI，并按安装来源升级。",
                                    "Check local Agent CLIs and update them in place.",
                                )),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .gap(px(8.0))
                        .child(button_with_icon_loading_l(
                            "local-env-refresh",
                            crate::icons::REFRESH_SVG,
                            i.t("刷新", "Refresh"),
                            ButtonVariant::Secondary,
                            loading,
                            &t,
                            cx,
                            |ws, _, _, cx| {
                                if ws.ui.local_env_loading || ws.ui.local_env_busy.is_some() {
                                    return;
                                }
                                ws.ui.local_env_loading = true;
                                cx.notify();
                                start_local_env_scan(cx);
                            },
                        ))
                        .child(button_with_icon_loading_l(
                            "local-env-update-all",
                            crate::icons::REFRESH_SVG,
                            i.t(
                                &format!("全部升级 ({updatable})"),
                                &format!("Update all ({updatable})"),
                            ),
                            ButtonVariant::Primary,
                            busy.as_deref() == Some("*"),
                            &t,
                            cx,
                            |ws, _, _, cx| {
                                if ws.ui.local_env_loading || ws.ui.local_env_busy.is_some() {
                                    return;
                                }
                                let ids: Vec<String> = ws
                                    .ui
                                    .local_env_tools
                                    .iter()
                                    .filter(|tool| {
                                        tool.version.as_deref().is_some_and(|local| {
                                            tool.latest_version.as_deref().is_some_and(|latest| {
                                                aitoolplus_core::local_env::update_available(
                                                    local, latest,
                                                )
                                            })
                                        })
                                    })
                                    .map(|tool| tool.id.clone())
                                    .collect();
                                if ids.is_empty() {
                                    ws.ui.toast("没有可升级的工具".to_string(), false);
                                    cx.notify();
                                    return;
                                }
                                ws.ui.local_env_busy = Some("*".to_string());
                                cx.notify();
                                let weak = cx.entity().downgrade();
                                cx.spawn(async move |_this, cx| {
                                    let mut failed = Vec::new();
                                    for id in ids {
                                        if let Err(error) = cx
                                            .background_spawn({
                                                let id = id.clone();
                                                async move {
                                                    aitoolplus_core::local_env::run_action(&id, false)
                                                }
                                            })
                                            .await
                                        {
                                            failed.push(format!("{id}: {error}"));
                                        }
                                    }
                                    let tools = cx
                                        .background_spawn(async {
                                            aitoolplus_core::local_env::check_all()
                                        })
                                        .await;
                                    let _ = weak.update(cx, |ws, cx| {
                                        ws.ui.local_env_busy = None;
                                        ws.ui.local_env_tools = tools;
                                        if failed.is_empty() {
                                            ws.ui.toast("全部升级完成".to_string(), false);
                                        } else {
                                            ws.ui.toast(failed.join("\n"), true);
                                        }
                                        cx.notify();
                                    });
                                })
                                .detach();
                            },
                        )),
                ),
        )
        .child(grid)
        .into_any_element()
}

fn local_env_icon(id: &str) -> &'static [u8] {
    match id {
        "claude" => crate::icons::CLAUDE_SVG,
        "codex" => crate::icons::OPENAI_SVG,
        "agy" => crate::icons::GEMINI_SVG,
        "grok" => crate::icons::GROK_SVG,
        "opencode" => crate::icons::OPENCODE_SVG,
        "openclaw" => crate::icons::CLAW_SVG,
        "hermes" => crate::icons::HERMES_SVG,
        "pi" => crate::icons::PI_SVG,
        _ => crate::icons::TERMINAL_SVG,
    }
}

fn version_line(t: &crate::theme::Theme, label: gpui::SharedString, value: String) -> gpui::AnyElement {
    div()
        .flex()
        .items_center()
        .justify_between()
        .gap(px(12.0))
        .child(
            div()
                .text_size(px(12.0))
                .text_color(t.text_muted)
                .child(label),
        )
        .child(
            div()
                .text_size(px(12.0))
                .font_family("ui-monospace, SFMono-Regular, Consolas, monospace")
                .text_color(t.text_primary)
                .child(value),
        )
        .into_any_element()
}

