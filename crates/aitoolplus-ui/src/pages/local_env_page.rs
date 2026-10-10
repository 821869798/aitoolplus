//! Local environment & Apps page: unified list of Agent CLIs matching cc-switch AppsPage.
use gpui::{Context, IntoElement, div, prelude::*, px};
use crate::components::{ButtonVariant, button_with_icon_loading_l, spinner, toggle};
use crate::workspace::Workspace;

fn start_local_env_scan(cx: &mut Context<Workspace>, manual: bool) {
    let weak = cx.entity().downgrade();
    cx.spawn(async move |_this, cx| {
        let tools = cx
            .background_spawn(async { aitoolplus_core::local_env::check_all() })
            .await;
        let _ = weak.update(cx, |ws, cx| {
            let updatable = tools
                .iter()
                .filter(|tool| {
                    tool.version.as_deref().is_some_and(|local| {
                        tool.latest_version
                            .as_deref()
                            .is_some_and(|latest| aitoolplus_core::local_env::update_available(local, latest))
                    })
                })
                .count();

            ws.ui.local_env_tools = tools;
            ws.ui.local_env_loading = false;
            ws.ui.local_env_loaded = true;
            ws.ui.local_env_last_checked = Some(chrono::Local::now());

            if manual {
                if updatable == 0 {
                    ws.ui.toast(ws.i18n.t("local_env.all_up_to_date").to_string(), false);
                } else {
                    ws.ui.toast(format!("检查完成，发现 {} 个可用更新", updatable), false);
                }
            }
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

fn recheck_single_tool(id: String, cx: &mut Context<Workspace>) {
    let weak = cx.entity().downgrade();
    let id_for_task = id.clone();
    cx.spawn(async move |_this, cx| {
        let refreshed = cx
            .background_spawn(async move { aitoolplus_core::local_env::check_one_id(&id_for_task) })
            .await;
        let _ = weak.update(cx, |ws, cx| {
            if let Some(status) = refreshed {
                if let Some(slot) = ws.ui.local_env_tools.iter_mut().find(|tool| tool.id == status.id) {
                    *slot = status;
                }
            }
            cx.notify();
        });
    })
    .detach();
}

fn tool_sidebar_key(id: &str) -> &'static str {
    match id {
        "claude" => "claude_code",
        "claude_desktop" => "claude_desktop",
        "codex" => "codex",
        "agy" => "antigravity",
        "grok" => "grok",
        "opencode" => "opencode",
        "openclaw" => "openclaw",
        "hermes" => "hermes",
        "pi" => "pi",
        _ => "",
    }
}

fn is_tool_visible(settings: &aitoolplus_core::settings::AppSettings, sidebar_key: &str) -> bool {
    if sidebar_key.is_empty() {
        return false;
    }
    settings.visible_tools.is_empty() || settings.visible_tools.iter().any(|k| k == sidebar_key)
}

fn toggle_tool_visibility(
    sidebar_key: &'static str,
    ws: &mut Workspace,
    cx: &mut Context<Workspace>,
) {
    if sidebar_key.is_empty() {
        return;
    }
    if ws.settings.visible_tools.is_empty() {
        let mut all: Vec<String> = aitoolplus_core::ToolId::ALL
            .into_iter()
            .map(|t| t.key().to_string())
            .collect();
        all.push("antigravity".to_string());
        ws.settings.visible_tools = all;
    }
    let is_currently_visible = ws.settings.visible_tools.iter().any(|k| k == sidebar_key);
    if is_currently_visible && ws.settings.visible_tools.len() <= 1 {
        ws.ui.toast("至少需要保留一个侧栏工具".to_string(), true);
    } else if is_currently_visible {
        ws.settings.visible_tools.retain(|k| k != sidebar_key);
        (ws.callbacks.save_settings)(&ws.settings);
    } else {
        ws.settings.visible_tools.push(sidebar_key.to_string());
        (ws.callbacks.save_settings)(&ws.settings);
    }
    cx.notify();
}

fn local_env_icon(id: &str) -> &'static [u8] {
    match id {
        "claude" | "claude_desktop" => crate::icons::CLAUDE_SVG,
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

#[derive(Clone)]
struct AppRowData {
    id: String,
    name: String,
    source: String,
    path: Option<String>,
    version: Option<String>,
    latest_version: Option<String>,
    error: Option<String>,
    installed_but_broken: bool,
    is_desktop: bool,
}

pub(super) fn local_env_tab(ws: &mut Workspace, cx: &mut Context<Workspace>) -> gpui::AnyElement {
    if !ws.ui.local_env_loaded && !ws.ui.local_env_loading {
        ws.ui.local_env_loading = true;
        start_local_env_scan(cx, false);
    }
    let t = ws.theme.clone();
    let i = ws.i18n;
    let busy = ws.ui.local_env_busy.clone();
    let loading = ws.ui.local_env_loading;

    let updatable_tools: Vec<String> = ws
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
        .map(|t| t.id.clone())
        .collect();
    let updatable_count = updatable_tools.len();

    // Prepare list of rows: Claude Code, Claude Desktop, then rest of tools
    let cd_paths = aitoolplus_core::ClaudeDesktopPaths::from_paths(&ws.paths);
    let cd_configured = cd_paths.live_config.is_file();
    let cd_row = AppRowData {
        id: "claude_desktop".to_string(),
        name: "Claude Desktop".to_string(),
        source: "官方".to_string(),
        path: if cd_configured {
            Some(cd_paths.live_config.to_string_lossy().to_string())
        } else {
            None
        },
        version: if cd_configured {
            Some(i.t("local_env.connected").to_string())
        } else {
            None
        },
        latest_version: None,
        error: None,
        installed_but_broken: false,
        is_desktop: true,
    };

    let tool_order = [
        ("claude", "Claude Code"),
        ("codex", "Codex"),
        ("agy", "Antigravity CLI"),
        ("grok", "Grok Build"),
        ("opencode", "OpenCode"),
        ("openclaw", "OpenClaw"),
        ("hermes", "Hermes"),
        ("pi", "Pi"),
    ];

    let mut rows: Vec<AppRowData> = Vec::new();
    for (idx, (tid, default_name)) in tool_order.iter().enumerate() {
        if let Some(status) = ws.ui.local_env_tools.iter().find(|t| t.id == *tid) {
            rows.push(AppRowData {
                id: status.id.clone(),
                name: status.name.clone(),
                source: status.source.clone(),
                path: status.path.clone(),
                version: status.version.clone(),
                latest_version: status.latest_version.clone(),
                error: status.error.clone(),
                installed_but_broken: status.installed_but_broken,
                is_desktop: false,
            });
        } else {
            rows.push(AppRowData {
                id: (*tid).to_string(),
                name: (*default_name).to_string(),
                source: String::new(),
                path: None,
                version: None,
                latest_version: None,
                error: None,
                installed_but_broken: false,
                is_desktop: false,
            });
        }

        // Insert Claude Desktop right after Claude Code
        if idx == 0 {
            rows.push(cd_row.clone());
        }
    }

    // Header actions
    let last_checked_text = ws
        .ui
        .local_env_last_checked
        .as_ref()
        .map(|dt| format!("{} {}", i.t("local_env.last_checked"), dt.format("%H:%M")))
        .unwrap_or_else(|| i.t("local_env.never_checked").to_string());

    let header_actions = div()
        .flex()
        .items_center()
        .gap(px(10.0))
        .child(
            div()
                .text_size(px(12.0))
                .text_color(t.text_muted)
                .child(last_checked_text),
        )
        .when(updatable_count > 0 || busy.as_deref() == Some("*"), |el| {
            let tools_to_update = updatable_tools.clone();
            el.child(button_with_icon_loading_l(
                "local-env-update-all",
                crate::icons::REFRESH_SVG,
                i.raw(
                    &format!("全部升级 ({updatable_count})"),
                    &format!("Update all ({updatable_count})"),
                ),
                ButtonVariant::Primary,
                busy.as_deref() == Some("*"),
                &t,
                cx,
                move |ws, _, _, cx| {
                    if ws.ui.local_env_loading || ws.ui.local_env_busy.is_some() {
                        return;
                    }
                    if tools_to_update.is_empty() {
                        ws.ui.toast("没有可升级的工具".to_string(), false);
                        cx.notify();
                        return;
                    }
                    ws.ui.local_env_busy = Some("*".to_string());
                    cx.notify();
                    let weak = cx.entity().downgrade();
                    let ids = tools_to_update.clone();
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
            ))
        })
        .child(button_with_icon_loading_l(
            "local-env-refresh",
            crate::icons::REFRESH_SVG,
            if loading {
                i.t("local_env.checking_2")
            } else {
                i.t("local_env.check_updates")
            },
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
                start_local_env_scan(cx, true);
            },
        ));

    // Table Header Row
    let table_header = div()
        .flex()
        .items_center()
        .px(px(16.0))
        .py(px(9.0))
        .bg(t.tab_bar_bg)
        .border_b_1()
        .border_color(t.card_border)
        .text_size(px(12.0))
        .font_weight(gpui::FontWeight::MEDIUM)
        .text_color(t.text_muted)
        .child(
            div()
                .flex_1()
                .child(i.t("local_env.column_app")),
        )
        .child(
            div()
                .w(px(160.0))
                .flex()
                .justify_end()
                .child(i.t("local_env.column_version")),
        )
        .child(
            div()
                .w(px(100.0))
                .flex()
                .justify_end()
                .child(i.t("local_env.column_action")),
        )
        .child(
            div()
                .w(px(68.0))
                .flex()
                .justify_end()
                .child(i.t("local_env.column_visible")),
        );

    // Table Body Rows
    let mut table_rows = div().flex().flex_col();
    for (row_idx, row) in rows.into_iter().enumerate() {
        let id = row.id.clone();
        let sidebar_key = tool_sidebar_key(&id);
        let is_visible = is_tool_visible(&ws.settings, sidebar_key);
        let row_busy = busy.as_deref() == Some(id.as_str()) || busy.as_deref() == Some("*");
        let checking = loading && row.version.is_none() && row.error.is_none() && !row.is_desktop;
        let outdated = row.version.as_deref().is_some_and(|local| {
            row.latest_version
                .as_deref()
                .is_some_and(|latest| aitoolplus_core::local_env::update_available(local, latest))
        });

        // Column 1: App Info (Icon + Name + Path/Meta)
        let icon_el = div()
            .flex()
            .items_center()
            .justify_center()
            .size(px(30.0))
            .rounded(px(7.0))
            .bg(t.input_bg)
            .border_1()
            .border_color(t.card_border)
            .child(
                gpui::svg()
                    .data(local_env_icon(&id))
                    .size(px(17.0))
                    .text_color(t.text_primary),
            );

        let mut app_info = div()
            .flex()
            .flex_col()
            .min_w(px(0.0))
            .gap(px(2.0))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .child(
                        div()
                            .text_size(px(13.0))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(t.text_primary)
                            .child(row.name.clone()),
                    )
                    .when(!row.source.is_empty() && row.source != "未安装" && row.source != "未知", |el| {
                        el.child(
                            div()
                                .px(px(6.0))
                                .py(px(0.5))
                                .rounded_full()
                                .border_1()
                                .border_color(t.card_border)
                                .bg(t.input_bg)
                                .text_size(px(10.0))
                                .text_color(t.text_muted)
                                .child(row.source.clone()),
                        )
                    }),
            );

        if let Some(path) = row.path {
            app_info = app_info.child(
                div()
                    .text_size(px(11.0))
                    .font_family("ui-monospace, SFMono-Regular, Consolas, monospace")
                    .text_color(t.text_muted)
                    .overflow_hidden()
                    .text_ellipsis()
                    .child(path),
            );
        } else if row.is_desktop {
            app_info = app_info.child(
                div()
                    .text_size(px(11.0))
                    .text_color(t.text_muted)
                    .child(if cd_configured {
                        "已绑定 Claude 桌面客户端"
                    } else {
                        "未检测到 Claude 桌面客户端配置文件"
                    }),
            );
        } else if let Some(cmd) = aitoolplus_core::local_env::manual_command(&id) {
            app_info = app_info.child(
                div()
                    .text_size(px(11.0))
                    .font_family("ui-monospace, SFMono-Regular, Consolas, monospace")
                    .text_color(t.text_muted)
                    .overflow_hidden()
                    .text_ellipsis()
                    .child(cmd),
            );
        }

        let col_app = div()
            .flex_1()
            .min_w(px(0.0))
            .flex()
            .items_center()
            .gap(px(10.0))
            .child(icon_el)
            .child(app_info);

        // Column 2: Version status
        let col_version = div()
            .w(px(160.0))
            .flex()
            .flex_col()
            .items_end()
            .justify_center()
            .gap(px(2.0));

        let col_version = if checking {
            col_version.child(spinner(format!("spin-{id}").into(), t.text_muted))
        } else if row.installed_but_broken {
            let error_text = row.error.clone();
            col_version
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(4.0))
                        .child(
                            gpui::svg()
                                .data(crate::icons::ALERT_SVG)
                                .size(px(13.0))
                                .text_color(t.warning),
                        )
                        .child(
                            div()
                                .text_size(px(12.0))
                                .font_weight(gpui::FontWeight::MEDIUM)
                                .text_color(t.warning)
                                .child(i.t("local_env.installed_but_cannot_run")),
                        ),
                )
                .when_some(error_text, |el, err| {
                    let first_line = err.lines().next().unwrap_or("").trim().to_string();
                    el.child(
                        div()
                            .text_size(px(10.5))
                            .text_color(t.text_muted)
                            .max_w(px(150.0))
                            .overflow_hidden()
                            .text_ellipsis()
                            .child(first_line),
                    )
                })
        } else if row.is_desktop {
            if cd_configured {
                col_version.child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(4.0))
                        .child(
                            div()
                                .size(px(6.0))
                                .rounded_full()
                                .bg(t.success),
                        )
                        .child(
                            div()
                                .text_size(px(12.0))
                                .font_weight(gpui::FontWeight::MEDIUM)
                                .text_color(t.success)
                                .child(i.t("local_env.connected")),
                        ),
                )
            } else {
                col_version.child(
                    div()
                        .text_size(px(12.0))
                        .text_color(t.text_muted)
                        .child(i.t("local_env.not_connected")),
                )
            }
        } else if let Some(v) = row.version.clone() {
            col_version
                .child(
                    div()
                        .text_size(px(12.5))
                        .font_family("ui-monospace, SFMono-Regular, Consolas, monospace")
                        .font_weight(gpui::FontWeight::MEDIUM)
                        .text_color(t.text_primary)
                        .child(v),
                )
                .when(outdated, |el| {
                    let latest = row.latest_version.clone().unwrap_or_default();
                    el.child(
                        div()
                            .text_size(px(10.5))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(t.accent)
                            .child(format!("新版本: {latest}")),
                    )
                })
        } else {
            col_version.child(
                div()
                    .text_size(px(12.0))
                    .text_color(t.text_muted)
                    .child(i.t("local_env.not_installed")),
            )
        };

        // Column 3: Action Button
        let id_for_action = id.clone();
        let col_action = div()
            .w(px(100.0))
            .flex()
            .items_center()
            .justify_end();

        let col_action = if row.is_desktop {
            col_action
        } else if row_busy {
            col_action.child(spinner(format!("busy-spin-{id}").into(), t.text_muted))
        } else if row.installed_but_broken {
            let recheck_id = id_for_action.clone();
            col_action.child(button_with_icon_loading_l(
                format!("recheck-{id}"),
                crate::icons::REFRESH_SVG,
                i.t("local_env.retry"),
                ButtonVariant::Secondary,
                false,
                &t,
                cx,
                move |_ws, _, _, cx| {
                    recheck_single_tool(recheck_id.clone(), cx);
                },
            ))
        } else if row.version.is_none() && !checking {
            let install_id = id_for_action.clone();
            col_action.child(button_with_icon_loading_l(
                format!("install-{id}"),
                crate::icons::DOWNLOAD_SVG,
                i.t("local_env.install"),
                ButtonVariant::Secondary,
                false,
                &t,
                cx,
                move |ws, _, _, cx| {
                    if ws.ui.local_env_busy.is_some() || ws.ui.local_env_loading {
                        return;
                    }
                    ws.ui.local_env_busy = Some(install_id.clone());
                    cx.notify();
                    run_local_env_action(install_id.clone(), true, cx);
                },
            ))
        } else if outdated {
            let update_id = id_for_action.clone();
            col_action.child(button_with_icon_loading_l(
                format!("update-{id}"),
                crate::icons::REFRESH_SVG,
                i.t("local_env.update"),
                ButtonVariant::Primary,
                false,
                &t,
                cx,
                move |ws, _, _, cx| {
                    if ws.ui.local_env_busy.is_some() || ws.ui.local_env_loading {
                        return;
                    }
                    ws.ui.local_env_busy = Some(update_id.clone());
                    cx.notify();
                    run_local_env_action(update_id.clone(), false, cx);
                },
            ))
        } else {
            col_action
        };

        // Column 4: Sidebar Visibility Toggle
        let col_visible = div()
            .w(px(68.0))
            .flex()
            .items_center()
            .justify_end()
            .child(toggle(
                format!("sidebar-toggle-{}", sidebar_key),
                is_visible,
                &t,
                cx,
                move |ws, _, _, cx| {
                    toggle_tool_visibility(sidebar_key, ws, cx);
                },
            ));

        // Assemble row
        let row_container = div()
            .flex()
            .items_center()
            .px(px(16.0))
            .py(px(11.0))
            .when(row_idx > 0, |el| el.border_t_1().border_color(t.card_border))
            .hover(move |h| h.bg(t.card_hover))
            .child(col_app)
            .child(col_version)
            .child(col_action)
            .child(col_visible);

        table_rows = table_rows.child(row_container);
    }

    // Main Card
    let table_card = div()
        .w_full()
        .rounded(px(10.0))
        .border_1()
        .border_color(t.card_border)
        .bg(t.card_bg)
        .shadow_xs()
        .overflow_hidden()
        .child(table_header)
        .child(table_rows);

    // Page layout
    div()
        .flex()
        .flex_col()
        .gap(px(16.0))
        .w_full()
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .gap(px(16.0))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(2.0))
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(8.0))
                                .child(
                                    gpui::svg()
                                        .data(crate::icons::LAYOUT_GRID_SVG)
                                        .size(px(18.0))
                                        .text_color(t.text_primary),
                                )
                                .child(
                                    div()
                                        .text_size(px(16.0))
                                        .font_weight(gpui::FontWeight::SEMIBOLD)
                                        .text_color(t.text_primary)
                                        .child(i.t("local_env.local_environment")),
                                ),
                        )
                        .child(
                            div()
                                .text_size(px(12.0))
                                .text_color(t.text_muted)
                                .child(i.t("local_env.app_subtitle")),
                        ),
                )
                .child(header_actions),
        )
        .child(table_card)
        .into_any_element()
}
