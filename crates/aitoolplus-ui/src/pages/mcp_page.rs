//! MCP servers page: list, add/edit, per-tool enable toggles, sync.
//!
//! Mirrors upstream behavior: each server carries its own enabled-tool set;
//! "sync all" writes every enabled server to its tools and removes disabled
//! ones; per-server sync status is shown after sync.

use aitoolplus_core::mcp::{McpServer, McpServerType, mcp_format};
use aitoolplus_core::tools::ToolId;
use gpui::{Context, IntoElement, div, prelude::*, px};
use serde_json::Value;

use crate::components::{
    BadgeKind, ButtonVariant, badge, button_l, empty_state, page_header, section_title,
};
use crate::text_input::TextInput;
use crate::workspace::Workspace;

use super::{McpDialogState, modal_scaffold};

/// Tools that support MCP, in sidebar order.
fn mcp_tools() -> Vec<ToolId> {
    ToolId::ALL
        .iter()
        .copied()
        .filter(|t| mcp_format(*t).is_some())
        .collect()
}

pub fn render_mcp_page(ws: &mut Workspace, cx: &mut Context<Workspace>) -> gpui::AnyElement {
    let t = ws.theme.clone();
    let i = ws.i18n;

    // First visit: discover existing servers from installed tools.
    if !ws.ui.mcp_discovered {
        let mut store = ws.store.store().mcp.clone();
        let (imported, sources) = aitoolplus_core::mcp::import_discovered(&ws.paths, &mut store);
        if imported > 0 || !store.servers.is_empty() {
            let _ = ws.store.update(|db| db.mcp = store);
            ws.persist_store();
        }
        if imported > 0 {
            let names: Vec<String> = sources
                .iter()
                .map(|(tool, name)| format!("{name} ({})", tool.name_en()))
                .collect();
            let msg = i
                .t(
                    &format!(
                        "发现并导入 {} 个 MCP 服务器：{}",
                        imported,
                        names.join(", ")
                    ),
                    &format!(
                        "discovered and imported {imported} MCP servers: {}",
                        names.join(", ")
                    ),
                )
                .to_string();
            ws.ui.toast(msg, false);
        }
        ws.ui.mcp_discovered = true;
    }

    let store = ws.store.store().mcp.clone();
    let servers = aitoolplus_core::mcp::list(&store);

    let add_label = i.t("新增服务器", "Add Server");
    let sync_label = i.t("全部同步", "Sync All");

    let mut section = div()
        .flex()
        .flex_col()
        .gap(px(12.0))
        .child(page_header(
            &t,
            i.t("MCP 服务器", "MCP Servers"),
            i.t(
                "集中管理 MCP 定义，按工具启停并同步到各 CLI 配置",
                "Central MCP definitions; toggle per tool and sync to CLI configs",
            ),
        ))
        .child(section_title(
            &t,
            i.t("服务器列表", "Server List"),
            Some(i.t(
                &format!("共 {} 个", servers.len()),
                &format!("{} total", servers.len()),
            )),
        ))
        .child(
            div()
                .flex()
                .items_center()
                .justify_start()
                .gap(px(8.0))
                .child(button_l(
                    "mcp-sync",
                    sync_label,
                    ButtonVariant::Primary,
                    &t,
                    cx,
                    |ws, _, _, cx| sync_all_action(ws, cx),
                ))
                .child(button_l(
                    "mcp-add",
                    add_label,
                    ButtonVariant::Secondary,
                    &t,
                    cx,
                    |ws, _, _, cx| open_mcp_dialog(None, ws, cx),
                )),
        );

    if servers.is_empty() {
        section = section.child(empty_state(
            &t,
            "⬡",
            i.t("还没有 MCP 服务器", "No MCP servers yet"),
            i.t(
                "已安装工具的现有配置会在首次打开时自动导入",
                "Existing configs from installed tools are imported on first open",
            ),
        ));
    } else {
        let mut list = div().flex().flex_col().gap(px(6.0));
        for s in &servers {
            list = list.child(server_row(s, ws, cx));
        }
        section = section.child(list);
    }

    section.into_any_element()
}

fn server_row(s: &McpServer, ws: &mut Workspace, cx: &mut Context<Workspace>) -> gpui::AnyElement {
    let t = ws.theme.clone();
    let i = ws.i18n;
    let id = s.id.clone();
    let favorite_id = s.id.clone();
    let favorite = s.favorite;

    let detail: String = match s.server_type {
        McpServerType::Stdio => {
            let cmd = s
                .server_config
                .get("command")
                .and_then(Value::as_str)
                .unwrap_or("");
            let args = s
                .server_config
                .get("args")
                .and_then(Value::as_array)
                .map(|a| {
                    a.iter()
                        .filter_map(Value::as_str)
                        .collect::<Vec<_>>()
                        .join(" ")
                })
                .unwrap_or_default();
            format!("{cmd} {args}")
        }
        McpServerType::Http | McpServerType::Sse => s
            .server_config
            .get("url")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
    };

    let enabled_count = s.enabled_tools.len();
    let sync_badge = if s.sync_details.is_none() {
        None
    } else {
        let failed = s
            .sync_details
            .as_ref()
            .and_then(Value::as_object)
            .map(|o| {
                o.values()
                    .filter(|v| v.get("status") == Some(&Value::String("error".into())))
                    .count()
            })
            .unwrap_or(0);
        Some(if failed > 0 {
            badge(
                &t,
                i.t(
                    &format!("{} 项同步失败", failed),
                    &format!("{failed} failed"),
                ),
                BadgeKind::Danger,
            )
        } else {
            badge(&t, i.t("已同步", "Synced"), BadgeKind::Success)
        })
    };

    let mut row = div()
        .id(gpui::SharedString::from(format!("mcp-{}", s.name)))
        .flex()
        .flex_col()
        .gap(px(8.0))
        .p(px(12.0))
        .rounded(px(8.0))
        .bg(t.card_bg)
        .border_1()
        .border_color(if enabled_count > 0 {
            t.success
        } else {
            t.card_border
        })
        .hover(|h| h.bg(t.card_hover));

    // header line
    row = row.child(
        div()
            .flex()
            .items_center()
            .gap(px(10.0))
            .child(badge(
                &t,
                s.server_type.as_str().to_uppercase(),
                BadgeKind::Accent,
            ))
            .child(
                div()
                    .text_size(px(13.5))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .text_color(t.text_primary)
                    .child(s.name.clone()),
            )
            .children(
                s.user_group
                    .clone()
                    .map(|g| badge(&t, g, BadgeKind::Neutral)),
            )
            .child(
                div()
                    .text_size(px(11.5))
                    .text_color(t.text_muted)
                    .flex_1()
                    .min_w(px(0.0))
                    .child(detail),
            )
            .children(sync_badge),
    );

    row = row.child(
        div()
            .flex()
            .items_center()
            .gap(px(8.0))
            .child(button_l(
                gpui::SharedString::from(format!("mcp-favorite-{favorite_id}")),
                if favorite {
                    i.t("取消收藏", "Unfavorite")
                } else {
                    i.t("收藏", "Favorite")
                },
                ButtonVariant::Secondary,
                &t,
                cx,
                move |ws, _, _, cx| {
                    let _ = ws.store.update(|db| {
                        aitoolplus_core::mcp::toggle_favorite(&mut db.mcp, &favorite_id);
                    });
                    ws.persist_store();
                    cx.notify();
                },
            ))
            .child(button_l(
                gpui::SharedString::from(format!("mcp-edit-{id}")),
                i.t("编辑", "Edit"),
                ButtonVariant::Secondary,
                &t,
                cx,
                move |ws, _, _, cx| open_mcp_dialog(Some(id.clone()), ws, cx),
            )),
    );

    // per-tool toggles
    let mut tools_row = div().flex().gap(px(6.0)).flex_wrap();
    for tool in mcp_tools() {
        let is_on = s.enabled_tools.iter().any(|k| k == tool.key());
        let label = i.t(tool.name_zh(), tool.name_en());
        let key = format!("mcp-tool-{}-{}", s.id, tool.key());
        let sid = s.id.clone();
        let t2 = t.clone();
        tools_row = tools_row.child(
            div()
                .id(gpui::SharedString::from(key))
                .cursor_pointer()
                .flex()
                .items_center()
                .h(px(24.0))
                .px(px(10.0))
                .rounded(px(6.0))
                .text_size(px(11.5))
                .when(is_on, |st| {
                    st.bg(t2.accent_subtle)
                        .border_1()
                        .border_color(t2.accent)
                        .text_color(t2.accent)
                })
                .when(!is_on, |st| {
                    st.bg(t2.input_bg)
                        .border_1()
                        .border_color(t2.input_border)
                        .text_color(t2.text_secondary)
                })
                .on_click(cx.listener(move |ws, _ev: &gpui::ClickEvent, _w, cx| {
                    let _ = ws.store.update(|db| {
                        aitoolplus_core::mcp::toggle_tool(&mut db.mcp, &sid, tool);
                    });
                    ws.persist_store();
                    cx.notify();
                }))
                .child(label),
        );
    }
    row = row.child(tools_row);
    row.into_any_element()
}

fn sync_all_action(ws: &mut Workspace, cx: &mut Context<Workspace>) {
    let i = ws.i18n;
    let report = {
        let mut mcp_store = ws.store.store().mcp.clone();
        let report = aitoolplus_core::mcp::sync_all_enabled(&ws.paths, &mut mcp_store);
        let _ = ws.store.update(|db| db.mcp = mcp_store);
        report
    };
    ws.persist_store();
    let total_ok: usize = report.iter().map(|(_, ok, _)| ok).sum();
    let total_failed: usize = report.iter().map(|(_, _, f)| f).sum();
    if total_failed > 0 {
        let msg = i
            .t(
                &format!("同步完成：成功 {}，失败 {}", total_ok, total_failed),
                &format!("sync done: {total_ok} ok, {total_failed} failed"),
            )
            .to_string();
        ws.ui.toast(msg, true);
    } else if total_ok > 0 {
        let msg = i
            .t(
                &format!("已同步 {} 项", total_ok),
                &format!("synced {total_ok} entries"),
            )
            .to_string();
        ws.ui.toast(msg, false);
    } else {
        let msg = i.t("没有启用的服务器", "no servers enabled").to_string();
        ws.ui.toast(msg, false);
    }
    cx.notify();
}

fn open_mcp_dialog(editing_id: Option<String>, ws: &mut Workspace, cx: &mut Context<Workspace>) {
    let i = ws.i18n;
    let existing = editing_id.as_ref().and_then(|id| {
        ws.store
            .store()
            .mcp
            .servers
            .iter()
            .find(|s| s.id == *id)
            .cloned()
    });

    let name = cx.new(|cx| {
        let mut input = TextInput::new(i.t("名称", "Name"), cx);
        if let Some(s) = &existing {
            input.set_text_silent(s.name.clone(), cx);
        }
        input
    });
    let command = cx.new(|cx| {
        let mut input = TextInput::new(i.t("命令（stdio）", "Command (stdio)"), cx);
        if let Some(s) = &existing
            && let Some(c) = s.server_config.get("command").and_then(Value::as_str)
        {
            input.set_text_silent(c.to_string(), cx);
        }
        input
    });
    let args = cx.new(|cx| {
        let mut input = TextInput::new(i.t("参数（空格分隔）", "Args (space separated)"), cx);
        if let Some(s) = &existing
            && let Some(a) = s.server_config.get("args").and_then(Value::as_array)
        {
            let text = a
                .iter()
                .filter_map(Value::as_str)
                .collect::<Vec<_>>()
                .join(" ");
            input.set_text_silent(text, cx);
        }
        input
    });
    let environment = cx.new(|cx| {
        let mut input = TextInput::new(
            i.t("环境变量 JSON，如 {\"KEY\":\"value\"}", "Environment JSON"),
            cx,
        );
        if let Some(server) = &existing
            && let Some(env) = server.server_config.get("env")
        {
            input.set_text_silent(serde_json::to_string(env).unwrap_or_default(), cx);
        }
        input
    });
    let url = cx.new(|cx| {
        let mut input = TextInput::new(i.t("URL（http/sse）", "URL (http/sse)"), cx);
        if let Some(s) = &existing
            && let Some(u) = s.server_config.get("url").and_then(Value::as_str)
        {
            input.set_text_silent(u.to_string(), cx);
        }
        input
    });
    let headers = cx.new(|cx| {
        let mut input = TextInput::new(
            i.t(
                "Headers JSON，如 {\"Authorization\":\"Bearer ...\"}",
                "Headers JSON",
            ),
            cx,
        );
        if let Some(server) = &existing
            && let Some(value) = server.server_config.get("headers")
        {
            input.set_text_silent(serde_json::to_string(value).unwrap_or_default(), cx);
        }
        input
    });
    let timeout_seconds = cx.new(|cx| {
        let mut input = TextInput::new(
            i.t("工具超时秒数（可选）", "Tool timeout seconds (optional)"),
            cx,
        );
        if let Some(server) = &existing
            && let Some(value) = server.server_config.get("tool_timeout_sec")
        {
            input.set_text_silent(value.to_string(), cx);
        }
        input
    });
    let group = cx.new(|cx| {
        let mut input = TextInput::new(i.t("分组（可选）", "Group (optional)"), cx);
        if let Some(s) = &existing {
            input.set_text_silent(s.user_group.clone().unwrap_or_default(), cx);
        }
        input
    });

    let server_type = existing
        .as_ref()
        .map(|s| s.server_type)
        .unwrap_or(McpServerType::Stdio);

    ws.ui.mcp_dialog = Some(McpDialogState {
        editing_id,
        name,
        server_type,
        command,
        args,
        environment,
        url,
        headers,
        timeout_seconds,
        group,
    });
    cx.notify();
}

pub fn render_mcp_dialog(
    state: McpDialogState,
    ws: &mut Workspace,
    cx: &mut Context<Workspace>,
) -> gpui::AnyElement {
    let t = ws.theme.clone();
    let i = ws.i18n;
    let McpDialogState {
        editing_id,
        name,
        server_type,
        command,
        args,
        environment,
        url,
        headers,
        timeout_seconds,
        group,
    } = state;

    let title = if editing_id.is_some() {
        i.t("编辑 MCP 服务器", "Edit MCP Server")
    } else {
        i.t("新增 MCP 服务器", "Add MCP Server")
    };

    let field_label = |label: gpui::SharedString| -> gpui::AnyElement {
        div()
            .text_size(px(12.0))
            .font_weight(gpui::FontWeight::MEDIUM)
            .text_color(t.text_secondary)
            .child(label)
            .into_any_element()
    };

    let body = div()
        .flex()
        .flex_col()
        .gap(px(12.0))
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(6.0))
                .child(field_label(i.t("名称", "Name")))
                .child(name.clone()),
        )
        .child(
            div().flex().gap(px(8.0)).child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .text_size(px(12.0))
                    .text_color(t.text_secondary)
                    .child(i.t("类型：", "Type:"))
                    .child({
                        let mut row = div().flex().gap(px(6.0));
                        for (ty, label) in [
                            (McpServerType::Stdio, i.t("Stdio", "Stdio")),
                            (McpServerType::Http, i.t("HTTP", "HTTP")),
                            (McpServerType::Sse, i.t("SSE", "SSE")),
                        ] {
                            let is_on = ty == server_type;
                            let t2 = t.clone();
                            row = row.child(
                                div()
                                    .id(gpui::SharedString::from(format!(
                                        "mcp-type-{}",
                                        ty.as_str()
                                    )))
                                    .cursor_pointer()
                                    .flex()
                                    .items_center()
                                    .h(px(24.0))
                                    .px(px(10.0))
                                    .rounded(px(6.0))
                                    .when(is_on, |st| {
                                        st.bg(t2.accent_subtle)
                                            .border_1()
                                            .border_color(t2.accent)
                                            .text_color(t2.accent)
                                    })
                                    .when(!is_on, |st| {
                                        st.bg(t2.input_bg)
                                            .border_1()
                                            .border_color(t2.input_border)
                                            .text_color(t2.text_secondary)
                                    })
                                    .on_click(cx.listener(move |ws, _ev, _w, cx| {
                                        if let Some(d) = ws.ui.mcp_dialog.as_mut() {
                                            d.server_type = ty;
                                        }
                                        cx.notify();
                                    }))
                                    .child(label),
                            );
                        }
                        row
                    }),
            ),
        )
        .child(if server_type == McpServerType::Stdio {
            div()
                .flex()
                .flex_col()
                .gap(px(6.0))
                .child(field_label(i.t("命令与参数", "Command & Args")))
                .child(
                    div()
                        .flex()
                        .gap(px(8.0))
                        .w_full()
                        .child(div().flex_1().min_w(px(0.0)).child(command.clone()))
                        .child(div().w(px(200.0)).child(args.clone())),
                )
                .child(field_label(i.t("环境变量", "Environment")))
                .child(environment.clone())
                .into_any_element()
        } else {
            div()
                .flex()
                .flex_col()
                .gap(px(6.0))
                .child(field_label(i.t("服务器 URL", "Server URL")))
                .child(url.clone())
                .child(field_label(i.t("请求头", "Headers")))
                .child(headers.clone())
                .into_any_element()
        })
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(6.0))
                .child(field_label(i.t("工具超时（秒）", "Tool timeout (seconds)")))
                .child(timeout_seconds.clone()),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(6.0))
                .child(field_label(i.t("分组", "Group")))
                .child(group.clone()),
        )
        .child(
            div()
                .flex()
                .justify_end()
                .gap(px(8.0))
                .child(button_l(
                    "mcp-dlg-cancel",
                    i.t("取消", "Cancel"),
                    ButtonVariant::Secondary,
                    &t,
                    cx,
                    |ws, _, _, cx| {
                        ws.ui.mcp_dialog = None;
                        cx.notify();
                    },
                ))
                .child(button_l(
                    "mcp-dlg-save",
                    i.t("保存并同步", "Save & Sync"),
                    ButtonVariant::Primary,
                    &t,
                    cx,
                    move |ws, _, _, cx| {
                        let name_txt: String = name.update(cx, |inp, _| inp.text().to_string());
                        let group_txt: String = group.update(cx, |inp, _| inp.text().to_string());
                        let mut config = match server_type {
                            McpServerType::Stdio => {
                                let cmd: String =
                                    command.update(cx, |inp, _| inp.text().to_string());
                                let args_txt: String =
                                    args.update(cx, |inp, _| inp.text().to_string());
                                let env_txt =
                                    environment.update(cx, |inp, _| inp.text().trim().to_string());
                                let env: Value = if env_txt.is_empty() {
                                    serde_json::json!({})
                                } else {
                                    match serde_json::from_str::<Value>(&env_txt) {
                                        Ok(value) if value.is_object() => value,
                                        _ => {
                                            ws.ui.toast(
                                                ws.i18n
                                                    .t(
                                                        "环境变量必须是 JSON 对象",
                                                        "environment must be a JSON object",
                                                    )
                                                    .to_string(),
                                                true,
                                            );
                                            cx.notify();
                                            return;
                                        }
                                    }
                                };
                                serde_json::json!({
                                    "command": cmd.trim(),
                                    "args": if args_txt.trim().is_empty() { vec![] } else {
                                        args_txt.split_whitespace().collect::<Vec<_>>()
                                    },
                                    "env": env,
                                })
                            }
                            McpServerType::Http | McpServerType::Sse => {
                                let url_txt: String =
                                    url.update(cx, |inp, _| inp.text().to_string());
                                let headers_txt =
                                    headers.update(cx, |inp, _| inp.text().trim().to_string());
                                let headers: Value = if headers_txt.is_empty() {
                                    serde_json::json!({})
                                } else {
                                    match serde_json::from_str::<Value>(&headers_txt) {
                                        Ok(value) if value.is_object() => value,
                                        _ => {
                                            ws.ui.toast(
                                                ws.i18n
                                                    .t(
                                                        "请求头必须是 JSON 对象",
                                                        "headers must be a JSON object",
                                                    )
                                                    .to_string(),
                                                true,
                                            );
                                            cx.notify();
                                            return;
                                        }
                                    }
                                };
                                serde_json::json!({ "url": url_txt.trim(), "headers": headers })
                            }
                        };
                        let timeout_txt =
                            timeout_seconds.update(cx, |input, _| input.text().trim().to_string());
                        if !timeout_txt.is_empty() {
                            match timeout_txt.parse::<u64>() {
                                Ok(timeout) if timeout > 0 => {
                                    config["tool_timeout_sec"] = Value::Number(timeout.into());
                                }
                                _ => {
                                    ws.ui.toast(
                                        ws.i18n
                                            .t(
                                                "超时必须是正整数",
                                                "timeout must be a positive integer",
                                            )
                                            .to_string(),
                                        true,
                                    );
                                    cx.notify();
                                    return;
                                }
                            }
                        }

                        if name_txt.trim().is_empty() {
                            let msg = ws.i18n.t("名称不能为空", "name is required").to_string();
                            ws.ui.toast(msg, true);
                            cx.notify();
                            return;
                        }

                        // Build or update the record.
                        let server = {
                            let existing = editing_id.as_ref().and_then(|id| {
                                ws.store
                                    .store()
                                    .mcp
                                    .servers
                                    .iter()
                                    .find(|s| s.id == *id)
                                    .cloned()
                            });
                            let mut server = existing.unwrap_or_else(|| {
                                McpServer::new(
                                    name_txt.trim().to_string(),
                                    server_type,
                                    config.clone(),
                                )
                            });
                            server.name = name_txt.trim().to_string();
                            server.server_type = server_type;
                            server.server_config = config.clone();
                            server.user_group = if group_txt.trim().is_empty() {
                                None
                            } else {
                                Some(group_txt.trim().to_string())
                            };
                            server
                        };

                        // Persist, then sync this server into all enabled tools.
                        let sync_targets: Vec<ToolId> = server
                            .enabled_tools
                            .iter()
                            .filter_map(|k| ToolId::from_key(k))
                            .collect();
                        let sid = server.id.clone();
                        let _ = ws.store.update(|db| {
                            aitoolplus_core::mcp::upsert(&mut db.mcp, server);
                        });
                        ws.persist_store();

                        let mut errors: Vec<String> = vec![];
                        let _ = ws.store.update(|db| {
                            if let Some(s) = db.mcp.servers.iter_mut().find(|s| s.id == sid) {
                                for tool in &sync_targets {
                                    match aitoolplus_core::mcp::sync_server_to_tool(
                                        &ws.paths, s, *tool,
                                    ) {
                                        Ok(()) => s.record_sync_pub(*tool, true, None),
                                        Err(e) => {
                                            errors.push(e.clone());
                                            s.record_sync_pub(*tool, false, Some(e));
                                        }
                                    }
                                }
                            }
                        });
                        ws.persist_store();

                        ws.ui.mcp_dialog = None;
                        if errors.is_empty() {
                            let msg = ws.i18n.t("已保存并同步", "saved and synced").to_string();
                            ws.ui.toast(msg, false);
                        } else {
                            let msg = format!("sync failed: {}", errors.join("; "));
                            ws.ui.toast(msg, true);
                        }
                        cx.notify();
                    },
                )),
        );

    modal_scaffold(
        &t,
        title.as_ref(),
        body.into_any_element(),
        cx,
        |ws, _, _, cx| {
            ws.ui.mcp_dialog = None;
            cx.notify();
        },
    )
}
