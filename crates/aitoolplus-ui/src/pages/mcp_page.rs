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
    BadgeKind, ButtonVariant, badge, button_l, button_with_icon_l, input_container,
    section_title,
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

#[derive(Clone)]
pub struct McpPreset {
    pub name: &'static str,
    pub server_type: McpServerType,
    pub command: &'static str,
    pub args: &'static str,
    pub env: &'static str,
    pub url: &'static str,
    pub desc: &'static str,
}

pub const BUILTIN_MCP_PRESETS: &[McpPreset] = &[
    McpPreset {
        name: "Filesystem",
        server_type: McpServerType::Stdio,
        command: "npx",
        args: "-y @modelcontextprotocol/server-filesystem D:\\",
        env: "",
        url: "",
        desc: "本地文件读写",
    },
    McpPreset {
        name: "GitHub",
        server_type: McpServerType::Stdio,
        command: "npx",
        args: "-y @modelcontextprotocol/server-github",
        env: "{\"GITHUB_PERSONAL_ACCESS_TOKEN\":\"\"}",
        url: "",
        desc: "GitHub 仓库管理",
    },
    McpPreset {
        name: "Fetch",
        server_type: McpServerType::Stdio,
        command: "uvx",
        args: "mcp-server-fetch",
        env: "",
        url: "",
        desc: "网页抓取与解析",
    },
    McpPreset {
        name: "SQLite",
        server_type: McpServerType::Stdio,
        command: "uvx",
        args: "mcp-server-sqlite --db-path ./data.db",
        env: "",
        url: "",
        desc: "SQLite 数据库查询",
    },
    McpPreset {
        name: "PostgreSQL",
        server_type: McpServerType::Stdio,
        command: "npx",
        args: "-y @modelcontextprotocol/server-postgres postgresql://localhost/mydb",
        env: "",
        url: "",
        desc: "Postgres 数据库",
    },
    McpPreset {
        name: "Memory Graph",
        server_type: McpServerType::Stdio,
        command: "npx",
        args: "-y @modelcontextprotocol/server-memory",
        env: "",
        url: "",
        desc: "知识图谱持久记忆",
    },
    McpPreset {
        name: "Puppeteer",
        server_type: McpServerType::Stdio,
        command: "npx",
        args: "-y @modelcontextprotocol/server-puppeteer",
        env: "",
        url: "",
        desc: "无头浏览器截图抓取",
    },
    McpPreset {
        name: "Brave Search",
        server_type: McpServerType::Stdio,
        command: "npx",
        args: "-y @modelcontextprotocol/server-brave-search",
        env: "{\"BRAVE_API_KEY\":\"\"}",
        url: "",
        desc: "Brave 互联网搜索",
    },
];

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

    // Presets bar
    let mut rec_bar = div()
        .flex()
        .items_center()
        .gap(px(6.0))
        .flex_wrap()
        .child(
            div()
                .text_size(px(12.0))
                .font_weight(gpui::FontWeight::MEDIUM)
                .text_color(t.text_secondary)
                .child(i.t("常用模板预设：", "Quick Presets:")),
        );

    for preset in BUILTIN_MCP_PRESETS {
        let p_name = preset.name;
        let p_desc = i.t(preset.desc, preset.desc);
        let preset_clone = preset.clone();
        let label = format!("{p_name} ({p_desc})");
        rec_bar = rec_bar.child(button_with_icon_l(
            gpui::SharedString::from(format!("mcp-pre-{}", preset.name)),
            crate::icons::PLUS_SVG,
            label,
            ButtonVariant::Secondary,
            &t,
            cx,
            move |ws, _, window, cx| {
                open_mcp_dialog(None, Some(&preset_clone), ws, cx);
                if let Some(dlg) = &ws.ui.mcp_dialog {
                    dlg.name.update(cx, |name, cx| {
                        name.focus_handle.focus(window, cx);
                        name.start_blink(cx);
                    });
                }
            },
        ));
    }

    let mut section = div()
        .flex()
        .flex_col()
        .w_full()
        .min_w(px(0.0))
        .gap(px(12.0))
        .child(rec_bar)
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
                .justify_between()
                .gap(px(8.0))
                .flex_wrap()
                .child(
                    div()
                        .flex()
                        .items_center()
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
                            |ws, _, window, cx| {
                                open_mcp_dialog(None, None, ws, cx);
                                if let Some(dlg) = &ws.ui.mcp_dialog {
                                    dlg.name.update(cx, |name, cx| {
                                        name.focus_handle.focus(window, cx);
                                        name.start_blink(cx);
                                    });
                                }
                            },
                        )),
                )
                .child(
                    div()
                        .w(px(280.0))
                        .child(input_container(&t, ws.ui.mcp_search.clone())),
                ),
        );

    let query = ws.ui.mcp_search.read(cx).text().to_lowercase();
    let filtered_servers: Vec<_> = servers
        .into_iter()
        .filter(|s| {
            if query.is_empty() {
                return true;
            }
            s.name.to_lowercase().contains(&query)
                || s.user_group.as_deref().is_some_and(|g| g.to_lowercase().contains(&query))
                || s.server_config.to_string().to_lowercase().contains(&query)
        })
        .collect();

    if filtered_servers.is_empty() {
        section = section.child(crate::components::empty_state_svg(
            &t,
            crate::icons::MCP_SVG,
            if query.is_empty() {
                i.t("还没有 MCP 服务器", "No MCP servers yet")
            } else {
                i.t("没有找到匹配的 MCP 服务器", "No matching MCP servers")
            },
            if query.is_empty() {
                i.t(
                    "点击上方模板快速添加，或等待已安装工具的配置自动导入",
                    "Click a preset above or add a server to get started",
                )
            } else {
                i.t("尝试更换搜索关键词", "Try a different search keyword")
            },
        ));
    } else {
        let mut list = div().flex().flex_col().gap(px(6.0));
        for s in &filtered_servers {
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
            crate::rgba_const(0x10b98166)
        } else {
            t.card_border
        })
        .shadow_xs()
        .hover(move |h| {
            h.bg(t.card_hover).border_color(if enabled_count > 0 {
                crate::rgba_const(0x10b981aa)
            } else {
                t.card_border_hover
            })
        });

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

    let dup_id = s.id.clone();
    let copy_id = s.id.clone();
    let del_id = s.id.clone();
    let del_name = s.name.clone();

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
                move |ws, _, window, cx| {
                    open_mcp_dialog(Some(id.clone()), None, ws, cx);
                    if let Some(dlg) = &ws.ui.mcp_dialog {
                        dlg.name.update(cx, |name, cx| {
                            name.focus_handle.focus(window, cx);
                            name.start_blink(cx);
                        });
                    }
                },
            ))
            .child(button_l(
                gpui::SharedString::from(format!("mcp-dup-{dup_id}")),
                i.t("克隆", "Duplicate"),
                ButtonVariant::Secondary,
                &t,
                cx,
                move |ws, _, _, cx| {
                    let mut cloned = false;
                    let _ = ws.store.update(|db| {
                        if let Some(existing) = db.mcp.servers.iter().find(|s| s.id == dup_id) {
                            let mut c = existing.clone();
                            c.id = uuid::Uuid::new_v4().to_string();
                            c.name = format!("{} (副本)", existing.name);
                            c.sync_details = None;
                            db.mcp.servers.push(c);
                            cloned = true;
                        }
                    });
                    if cloned {
                        ws.persist_store();
                        let msg = ws.i18n.t("已克隆 MCP 服务器", "MCP server duplicated").to_string();
                        ws.ui.toast(msg, false);
                        cx.notify();
                    }
                },
            ))
            .child(button_l(
                gpui::SharedString::from(format!("mcp-copy-{copy_id}")),
                i.t("复制配置", "Copy Config"),
                ButtonVariant::Secondary,
                &t,
                cx,
                move |ws, _, _, cx| {
                    if let Some(srv) = ws.store.store().mcp.servers.iter().find(|x| x.id == copy_id) {
                        let json = serde_json::to_string_pretty(srv).unwrap_or_default();
                        cx.write_to_clipboard(gpui::ClipboardItem::new_string(json));
                        let msg = ws.i18n.t("已复制 MCP 配置到剪贴板", "Copied MCP config to clipboard").to_string();
                        ws.ui.toast(msg, false);
                        cx.notify();
                    }
                },
            ))
            .child(button_l(
                gpui::SharedString::from(format!("mcp-del-{del_id}")),
                i.t("删除", "Delete"),
                ButtonVariant::Danger,
                &t,
                cx,
                move |ws, _, _, cx| {
                    ws.ui.confirm = Some(super::ConfirmState {
                        title: ws.i18n.t("删除 MCP 服务器", "Delete MCP Server").to_string(),
                        message: ws.i18n.t(
                            &format!("确定要删除 MCP 服务器“{}”吗？", del_name),
                            &format!("Are you sure you want to delete MCP server '{}'?", del_name),
                        ).to_string(),
                        action: super::ConfirmAction::DeleteMcp { id: del_id.clone() },
                    });
                    cx.notify();
                },
            )),
    );

    // per-tool toggles
    let mut tools_row = div().flex().gap(px(6.0)).flex_wrap();
    for tool in mcp_tools() {
        let is_on = s.enabled_tools.iter().any(|k| k == tool.key());
        let label = i.t(tool.name_zh(), tool.name_en());
        let key = format!("mcp-tool-{}-{}", s.id, tool.key());
        let sid = s.id.clone();
        tools_row = tools_row.child(button_l(
            gpui::SharedString::from(key),
            label,
            if is_on {
                ButtonVariant::Primary
            } else {
                ButtonVariant::Secondary
            },
            &t,
            cx,
            move |ws, _ev, _w, cx| {
                let _ = ws.store.update(|db| {
                    aitoolplus_core::mcp::toggle_tool(&mut db.mcp, &sid, tool);
                });
                ws.persist_store();
                cx.notify();
            },
        ));
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

fn open_mcp_dialog(
    editing_id: Option<String>,
    preset: Option<&McpPreset>,
    ws: &mut Workspace,
    cx: &mut Context<Workspace>,
) {
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

    let default_name = existing
        .as_ref()
        .map(|s| s.name.clone())
        .or_else(|| preset.map(|p| p.name.to_string()))
        .unwrap_or_default();
    let name = cx.new(|cx| {
        let mut input = TextInput::new(i.t("名称", "Name"), cx);
        input.set_text_silent(default_name, cx);
        input
    });

    let default_cmd = existing
        .as_ref()
        .and_then(|s| {
            s.server_config
                .get("command")
                .and_then(Value::as_str)
                .map(|c| c.to_string())
        })
        .or_else(|| preset.map(|p| p.command.to_string()))
        .unwrap_or_default();
    let command = cx.new(|cx| {
        let mut input = TextInput::new(i.t("命令（stdio）", "Command (stdio)"), cx);
        input.set_text_silent(default_cmd, cx);
        input
    });

    let default_args = existing
        .as_ref()
        .and_then(|s| {
            s.server_config
                .get("args")
                .and_then(Value::as_array)
                .map(|a| {
                    a.iter()
                        .filter_map(Value::as_str)
                        .collect::<Vec<_>>()
                        .join(" ")
                })
        })
        .or_else(|| preset.map(|p| p.args.to_string()))
        .unwrap_or_default();
    let args = cx.new(|cx| {
        let mut input = TextInput::new(i.t("参数（空格分隔）", "Args (space separated)"), cx);
        input.set_text_silent(default_args, cx);
        input
    });

    let default_env = existing
        .as_ref()
        .and_then(|s| {
            s.server_config
                .get("env")
                .map(|e| serde_json::to_string(e).unwrap_or_default())
        })
        .or_else(|| preset.map(|p| p.env.to_string()))
        .unwrap_or_default();
    let environment = cx.new(|cx| {
        let mut input = TextInput::new(
            i.t("环境变量 JSON，如 {\"KEY\":\"value\"}", "Environment JSON"),
            cx,
        );
        input.set_text_silent(default_env, cx);
        input
    });

    let default_url = existing
        .as_ref()
        .and_then(|s| {
            s.server_config
                .get("url")
                .and_then(Value::as_str)
                .map(|u| u.to_string())
        })
        .or_else(|| preset.map(|p| p.url.to_string()))
        .unwrap_or_default();
    let url = cx.new(|cx| {
        let mut input = TextInput::new(i.t("URL（http/sse）", "URL (http/sse)"), cx);
        input.set_text_silent(default_url, cx);
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
        .or_else(|| preset.map(|p| p.server_type))
        .unwrap_or(McpServerType::Stdio);

    let enabled_tools = existing
        .as_ref()
        .map(|s| {
            s.enabled_tools
                .iter()
                .filter_map(|k| ToolId::from_key(k))
                .collect()
        })
        .unwrap_or_else(mcp_tools);

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
        enabled_tools,
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
        enabled_tools,
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

    let mut dialog_preset_bar = div()
        .flex()
        .items_center()
        .gap(px(4.0))
        .flex_wrap()
        .child(
            div()
                .text_size(px(11.5))
                .font_weight(gpui::FontWeight::MEDIUM)
                .text_color(t.text_secondary)
                .child(i.t("填充预设：", "Fill Preset:")),
        );

    for preset in BUILTIN_MCP_PRESETS {
        let name_val = preset.name.to_string();
        let cmd_val = preset.command.to_string();
        let args_val = preset.args.to_string();
        let env_val = preset.env.to_string();
        let p_type = preset.server_type;
        let p_name = preset.name;
        let name_inp = name.clone();
        let cmd_inp = command.clone();
        let args_inp = args.clone();
        let env_inp = environment.clone();

        dialog_preset_bar = dialog_preset_bar.child(button_l(
            gpui::SharedString::from(format!("dlg-pre-{}", preset.name)),
            p_name,
            ButtonVariant::Secondary,
            &t,
            cx,
            move |ws, _, _, cx| {
                name_inp.update(cx, |inp, cx| inp.set_text_silent(name_val.clone(), cx));
                cmd_inp.update(cx, |inp, cx| inp.set_text_silent(cmd_val.clone(), cx));
                args_inp.update(cx, |inp, cx| inp.set_text_silent(args_val.clone(), cx));
                env_inp.update(cx, |inp, cx| inp.set_text_silent(env_val.clone(), cx));
                if let Some(d) = ws.ui.mcp_dialog.as_mut() {
                    d.server_type = p_type;
                }
                cx.notify();
            },
        ));
    }

    let tools_picker = div()
        .flex()
        .flex_col()
        .gap(px(6.0))
        .child(field_label(i.t("应用到目标工具", "Target Tools")))
        .child({
            let mut row = div().flex().gap(px(6.0)).flex_wrap();
            for tool in mcp_tools() {
                let is_on = enabled_tools.contains(&tool);
                let label = i.t(tool.name_zh(), tool.name_en());
                row = row.child(button_l(
                    gpui::SharedString::from(format!("dlg-tool-{}", tool.key())),
                    label,
                    if is_on {
                        ButtonVariant::Primary
                    } else {
                        ButtonVariant::Secondary
                    },
                    &t,
                    cx,
                    move |ws, _, _, cx| {
                        if let Some(d) = ws.ui.mcp_dialog.as_mut() {
                            if let Some(pos) = d.enabled_tools.iter().position(|x| *x == tool) {
                                d.enabled_tools.remove(pos);
                            } else {
                                d.enabled_tools.push(tool);
                            }
                        }
                        cx.notify();
                    },
                ));
            }
            row
        });

    let body = div()
        .flex()
        .flex_col()
        .gap(px(12.0))
        .child(dialog_preset_bar)
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(6.0))
                .child(field_label(i.t("名称", "Name")))
                .child(input_container(&t, name.clone())),
        )
        .child(tools_picker)
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
                            row = row.child(button_l(
                                gpui::SharedString::from(format!("mcp-type-{}", ty.as_str())),
                                label,
                                if is_on {
                                    ButtonVariant::Primary
                                } else {
                                    ButtonVariant::Secondary
                                },
                                &t,
                                cx,
                                move |ws, _ev, _w, cx| {
                                    if let Some(d) = ws.ui.mcp_dialog.as_mut() {
                                        d.server_type = ty;
                                    }
                                    cx.notify();
                                },
                            ));
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
                        .child(div().flex_1().min_w(px(0.0)).child(input_container(&t, command.clone())))
                        .child(div().w(px(200.0)).child(input_container(&t, args.clone()))),
                )
                .child(field_label(i.t("环境变量", "Environment")))
                .child(input_container(&t, environment.clone()))
                .into_any_element()
        } else {
            div()
                .flex()
                .flex_col()
                .gap(px(6.0))
                .child(field_label(i.t("服务器 URL", "Server URL")))
                .child(input_container(&t, url.clone()))
                .child(field_label(i.t("请求头", "Headers")))
                .child(input_container(&t, headers.clone()))
                .into_any_element()
        })
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(6.0))
                .child(field_label(i.t("工具超时（秒）", "Tool timeout (seconds)")))
                .child(input_container(&t, timeout_seconds.clone())),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(6.0))
                .child(field_label(i.t("分组", "Group")))
                .child(input_container(&t, group.clone())),
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
                            server.enabled_tools = enabled_tools
                                .iter()
                                .map(|t| t.key().to_string())
                                .collect();
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
