//! MCP servers page: list, add/edit, per-tool enable toggles, sync.
//!
//! Mirrors ai-toolbox coding/mcp implementation:
//! - Header with docs link and subtitle
//! - Toolbar with search, count badge, import existing, import JSON, add server, sync all
//! - Compact 3-segment cards (status dot, name, actions, command line, tags/group/note, transport, tool pills)
//! - Detail drawer with full config breakdown, metadata, tag manager, tools sync grid, footer actions
//! - Supporting modals: Import JSON, Import Existing, Metadata Editor, Tag Editor, Add/Edit Form

use aitoolplus_core::mcp::{McpServer, McpServerType, mcp_format};
use aitoolplus_core::tools::ToolId;
use gpui::{Context, IntoElement, MouseButton, MouseDownEvent, div, prelude::*, px, uniform_list};
use serde_json::Value;

use crate::components::{
    BadgeKind, ButtonVariant, badge, button_l, button_with_icon_l, icon_button_svg,
    input_container,
};
use crate::icons;
use crate::text_area::TextArea;
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

fn tag_color_palette(tag: &str) -> (gpui::Rgba, gpui::Rgba) {
    let colors = [
        (crate::rgba_const(0x3b82f620), crate::rgba_const(0x60a5faff)), // blue
        (crate::rgba_const(0x10b98120), crate::rgba_const(0x34d399ff)), // emerald
        (crate::rgba_const(0xf59e0b20), crate::rgba_const(0xfbbf24ff)), // amber
        (crate::rgba_const(0x8b5cf620), crate::rgba_const(0xa78bfaff)), // violet
        (crate::rgba_const(0xec489920), crate::rgba_const(0xf472b6ff)), // pink
        (crate::rgba_const(0x06b6d420), crate::rgba_const(0x22d3eeff)), // cyan
        (crate::rgba_const(0x6366f120), crate::rgba_const(0x818cf8ff)), // indigo
        (crate::rgba_const(0xd946ef20), crate::rgba_const(0xe879f9ff)), // fuchsia
    ];
    let mut hash: usize = 0;
    for b in tag.bytes() {
        hash = hash.wrapping_mul(31).wrapping_add(b as usize);
    }
    colors[hash % colors.len()]
}

fn format_relative_time(updated_at: i64, i18n: &crate::i18n::I18n) -> String {
    let now = aitoolplus_core::mcp::now_ms();
    let diff = (now - updated_at).max(0);
    if diff < 60_000 {
        i18n.t("刚刚", "just now").to_string()
    } else if diff < 3_600_000 {
        let mins = diff / 60_000;
        i18n.t(&format!("{} 分钟前", mins), &format!("{}m ago", mins))
            .to_string()
    } else if diff < 86_400_000 {
        let hours = diff / 3_600_000;
        i18n.t(&format!("{} 小时前", hours), &format!("{}h ago", hours))
            .to_string()
    } else {
        let days = diff / 86_400_000;
        i18n.t(&format!("{} 天前", days), &format!("{}d ago", days))
            .to_string()
    }
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

    let query = ws.ui.mcp_search.read(cx).text().to_lowercase();
    let filtered_servers: Vec<_> = servers
        .iter()
        .filter(|s| {
            if query.is_empty() {
                return true;
            }
            s.name.to_lowercase().contains(&query)
                || s.user_group
                    .as_deref()
                    .is_some_and(|g| g.to_lowercase().contains(&query))
                || s.user_note
                    .as_deref()
                    .is_some_and(|n| n.to_lowercase().contains(&query))
                || s.tags.iter().any(|tag| tag.to_lowercase().contains(&query))
                || s.server_config.to_string().to_lowercase().contains(&query)
        })
        .cloned()
        .collect();

    // 1. Page Header: Title + View Docs link + Subtitle
    let header = div()
        .flex()
        .flex_col()
        .gap(px(4.0))
        .w_full()
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .w_full()
                .child(
                    div()
                        .text_size(px(18.0))
                        .font_weight(gpui::FontWeight::BOLD)
                        .text_color(t.text_primary)
                        .child(i.t("MCP 服务器", "MCP Servers")),
                )
                .child(
                    div()
                        .id("mcp-view-docs-btn")
                        .flex()
                        .items_center()
                        .gap(px(4.0))
                        .cursor_pointer()
                        .text_size(px(12.0))
                        .text_color(t.accent)
                        .hover(|h| h.underline())
                        .child(
                            gpui::svg()
                                .data(icons::EXTERNAL_LINK_SVG)
                                .size(px(13.0))
                                .text_color(t.accent),
                        )
                        .child(i.t("查看官方文档", "View Official Docs"))
                        .on_click(cx.listener(|_, _, _, cx| {
                            cx.open_url("https://code.claude.com/docs/en/mcp#installing-mcp-servers");
                        })),
                ),
        )
        .child(
            div()
                .text_size(px(12.5))
                .text_color(t.text_secondary)
                .child(i.t(
                    "管理跨工具的 MCP 服务器配置，一键同步到各 AI 编程工具",
                    "Manage cross-tool MCP server configurations, sync with one click",
                )),
        );

    // 2. Toolbar: Search input, Count pill, Import existing, Import JSON, Add server, Sync all
    let count_label = format!("{}/{}", filtered_servers.len(), servers.len());
    let toolbar = div()
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
                .flex_wrap()
                .child(
                    div()
                        .w(px(240.0))
                        .child(input_container(&t, ws.ui.mcp_search.clone())),
                )
                .child(badge(&t, count_label, BadgeKind::Neutral))
                .child(button_with_icon_l(
                    "mcp-import-existing-btn",
                    icons::DOWNLOAD_SVG,
                    i.t("导入已有", "Import Existing"),
                    ButtonVariant::Secondary,
                    &t,
                    cx,
                    |ws, _, _, cx| {
                        ws.ui.mcp_import_existing_modal = true;
                        cx.notify();
                    },
                ))
                .child(button_with_icon_l(
                    "mcp-import-json-btn",
                    icons::FILE_TEXT_SVG,
                    i.t("导入 JSON", "Import JSON"),
                    ButtonVariant::Secondary,
                    &t,
                    cx,
                    |ws, _, _, cx| {
                        let default_json = "{\n  \"mcpServers\": {\n    \n  }\n}";
                        let editor = cx.new(|cx| TextArea::new(default_json, cx));
                        ws.ui.mcp_import_json_modal = Some(editor);
                        cx.notify();
                    },
                ))
                .child(button_with_icon_l(
                    "mcp-add-server-btn",
                    icons::PLUS_SVG,
                    i.t("新增服务器", "Add Server"),
                    ButtonVariant::Primary,
                    &t,
                    cx,
                    |ws, _, window, cx| {
                        open_mcp_dialog(None, ws, cx);
                        if let Some(dlg) = &ws.ui.mcp_dialog {
                            dlg.name.update(cx, |name, cx| {
                                name.focus_handle.focus(window, cx);
                                name.start_blink(cx);
                            });
                        }
                    },
                )),
        );

    // 3. Card list or empty state (virtualized 2-column grid matching ai-toolbox defaultRowHeight={108})
    let content = if filtered_servers.is_empty() {
        crate::components::empty_state_svg(
            &t,
            icons::MCP_SVG,
            if query.is_empty() {
                i.t("还没有 MCP 服务器", "No MCP servers yet")
            } else {
                i.t("没有找到匹配的 MCP 服务器", "No matching MCP servers")
            },
            if query.is_empty() {
                i.t(
                    "点击“+ 新增服务器”或“导入”开始添加，或等待已安装工具的配置自动导入",
                    "Click '+ Add Server' or 'Import' to add a server, or import from installed tools",
                )
            } else {
                i.t("尝试更换搜索关键词", "Try a different search keyword")
            },
        )
    } else {
        let rows_count = (filtered_servers.len() + 1) / 2;
        let servers_arc = std::sync::Arc::new(filtered_servers);
        let tools_arc = std::sync::Arc::new(mcp_tools());
        let ws_entity = cx.entity();
        let t_clone = t.clone();
        let i_clone = i.clone();

        let v_list = uniform_list(
            "mcp-servers-vlist",
            rows_count,
            move |range: std::ops::Range<usize>, _window: &mut gpui::Window, _cx: &mut gpui::App| -> Vec<gpui::AnyElement> {
                let mut row_elements = Vec::with_capacity(range.len());
                for row_idx in range {
                    let first_idx = row_idx * 2;
                    let second_idx = first_idx + 1;

                    let mut row = div()
                        .h(px(108.0))
                        .pb(px(8.0))
                        .flex()
                        .gap(px(10.0))
                        .w_full();

                    if let Some(server1) = servers_arc.get(first_idx) {
                        row = row.child(
                            div()
                                .flex_1()
                                .h_full()
                                .min_w(px(0.0))
                                .child(render_virtual_mcp_card(
                                    server1,
                                    &tools_arc,
                                    &ws_entity,
                                    &t_clone,
                                    &i_clone,
                                )),
                        );
                    }

                    if let Some(server2) = servers_arc.get(second_idx) {
                        row = row.child(
                            div()
                                .flex_1()
                                .h_full()
                                .min_w(px(0.0))
                                .child(render_virtual_mcp_card(
                                    server2,
                                    &tools_arc,
                                    &ws_entity,
                                    &t_clone,
                                    &i_clone,
                                )),
                        );
                    } else {
                        row = row.child(div().flex_1().h_full().min_w(px(0.0)));
                    }

                    row_elements.push(row.into_any_element());
                }
                row_elements
            },
        )
        .size_full();

        div()
            .w_full()
            .flex_1()
            .h_full()
            .min_h(px(0.0))
            .overflow_hidden()
            .child(v_list)
            .into_any_element()
    };

    div()
        .flex()
        .flex_col()
        .flex_1()
        .h_full()
        .min_h(px(0.0))
        .w_full()
        .gap(px(14.0))
        .child(header)
        .child(toolbar)
        .child(content)
        .into_any_element()
}

fn render_virtual_mcp_card(
    s: &McpServer,
    tools: &[ToolId],
    ws_entity: &gpui::Entity<Workspace>,
    t: &crate::theme::Theme,
    i: &crate::i18n::I18n,
) -> gpui::AnyElement {
    let sid = s.id.clone();
    let s_name = s.name.clone();
    let management_enabled = s.management_enabled;

    // Command text / copy value
    let command_text: String = match s.server_type {
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
            if args.is_empty() {
                cmd.to_string()
            } else {
                format!("{cmd} {args}")
            }
        }
        McpServerType::Http | McpServerType::Sse => s
            .server_config
            .get("url")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
    };
    let copy_content = command_text.clone();
    let updated_str = format_relative_time(s.updated_at, i);

    // Transport icon
    let transport_icon = match s.server_type {
        McpServerType::Stdio => icons::CODE_SVG,
        McpServerType::Http | McpServerType::Sse => icons::GLOBE_SVG,
    };

    // Header buttons
    // 0. iOS-style toggle switch (matching system settings autostart toggle)
    let sid_for_toggle = sid.clone();
    let ws_entity_toggle = ws_entity.clone();
    let toggle_btn = div()
        .id(gpui::SharedString::from(format!("vcard-toggle-{}", sid)))
        .cursor_pointer()
        .w(px(32.0))
        .h(px(18.0))
        .p(px(2.0))
        .rounded_full()
        .flex()
        .flex_none()
        .items_center()
        .bg(if management_enabled { t.track_on } else { t.track_off })
        .hover(move |h| h.opacity(0.92))
        .active(move |a| a.opacity(0.85))
        .when(management_enabled, |s| s.justify_end())
        .when(!management_enabled, |s| s.justify_start())
        .child(
            div()
                .size(px(14.0))
                .rounded_full()
                .bg(t.thumb)
                .shadow_sm(),
        )
        .on_click(move |_, _, cx| {
            cx.stop_propagation();
            let sid = sid_for_toggle.clone();
            let _ = ws_entity_toggle.update(cx, |ws, cx| {
                let _ = ws.store.update(|db| {
                    aitoolplus_core::mcp::set_management_enabled(
                        &mut db.mcp,
                        &sid,
                        !management_enabled,
                    );
                });
                let server = ws
                    .store
                    .store()
                    .mcp
                    .servers
                    .iter()
                    .find(|x| x.id == sid)
                    .cloned();
                if let Some(s) = server {
                    if s.management_enabled {
                        for tool in &s.enabled_tools {
                            if let Some(tool_id) =
                                aitoolplus_core::ToolId::ALL.iter().find(|t| t.key() == tool.as_str())
                            {
                                let _ = aitoolplus_core::mcp::sync_server_to_tool(
                                    &ws.paths, &s, *tool_id,
                                );
                            }
                        }
                    } else {
                        for tool in &s.enabled_tools {
                            if let Some(tool_id) =
                                aitoolplus_core::ToolId::ALL.iter().find(|t| t.key() == tool.as_str())
                            {
                                let _ = aitoolplus_core::mcp::remove_server_from_tool(
                                    &ws.paths, &s.name, *tool_id,
                                );
                            }
                        }
                    }
                }
                ws.persist_store();
                cx.notify();
            });
        });

    // 1. Copy config
    let ws_entity_copy = ws_entity.clone();
    let copy_btn = div()
        .id(gpui::SharedString::from(format!("vcard-copy-{}", sid)))
        .w(px(22.0))
        .h(px(22.0))
        .rounded(px(4.0))
        .flex()
        .items_center()
        .justify_center()
        .cursor_pointer()
        .hover({
            let bg = t.card_hover;
            move |h| h.bg(bg)
        })
        .child(crate::icons::svg_icon(icons::COPY_SVG, px(12.0), t.text_secondary))
        .on_click(move |_, _, cx| {
            cx.stop_propagation();
            cx.write_to_clipboard(gpui::ClipboardItem::new_string(copy_content.clone()));
            let _ = ws_entity_copy.update(cx, |ws, cx| {
                ws.ui.toast(ws.i18n.t("已复制配置到剪贴板", "Copied config to clipboard").to_string(), false);
                cx.notify();
            });
        });

    // 2. Edit metadata
    let sid_for_meta = sid.clone();
    let ws_entity_meta = ws_entity.clone();
    let meta_btn = div()
        .id(gpui::SharedString::from(format!("vcard-meta-{}", sid)))
        .w(px(22.0))
        .h(px(22.0))
        .rounded(px(4.0))
        .flex()
        .items_center()
        .justify_center()
        .cursor_pointer()
        .hover({
            let bg = t.card_hover;
            move |h| h.bg(bg)
        })
        .child(crate::icons::svg_icon(icons::TAG_SVG, px(12.0), t.text_secondary))
        .on_click(move |_, _, cx| {
            cx.stop_propagation();
            let sid = sid_for_meta.clone();
            let _ = ws_entity_meta.update(cx, |ws, cx| {
                let existing = ws
                    .store
                    .store()
                    .mcp
                    .servers
                    .iter()
                    .find(|x| x.id == sid)
                    .cloned();
                let group_input = cx.new(|cx| {
                    let mut inp = TextInput::new(ws.i18n.t("分组", "Group"), cx);
                    if let Some(g) = existing.as_ref().and_then(|x| x.user_group.clone()) {
                        inp.set_text_silent(g, cx);
                    }
                    inp
                });
                let note_input = cx.new(|cx| {
                    let mut inp = TextInput::new(ws.i18n.t("备注", "Note"), cx);
                    if let Some(n) = existing.as_ref().and_then(|x| x.user_note.clone()) {
                        inp.set_text_silent(n, cx);
                    }
                    inp
                });
                ws.ui.mcp_editing_metadata = Some((sid.clone(), group_input, note_input));
                cx.notify();
            });
        });

    // 3. Edit server
    let sid_for_edit = sid.clone();
    let ws_entity_edit = ws_entity.clone();
    let edit_btn = div()
        .id(gpui::SharedString::from(format!("vcard-edit-{}", sid)))
        .w(px(22.0))
        .h(px(22.0))
        .rounded(px(4.0))
        .flex()
        .items_center()
        .justify_center()
        .cursor_pointer()
        .hover({
            let bg = t.card_hover;
            move |h| h.bg(bg)
        })
        .child(crate::icons::svg_icon(icons::PENCIL_SVG, px(12.0), t.text_secondary))
        .on_click(move |_, window, cx| {
            cx.stop_propagation();
            let sid = sid_for_edit.clone();
            let _ = ws_entity_edit.update(cx, |ws, cx| {
                open_mcp_dialog(Some(sid), ws, cx);
                if let Some(dlg) = &ws.ui.mcp_dialog {
                    dlg.name.update(cx, |name, cx| {
                        name.focus_handle.focus(window, cx);
                        name.start_blink(cx);
                    });
                }
            });
        });

    // 4. Delete server
    let sid_for_del = sid.clone();
    let s_name_for_del = s_name.clone();
    let ws_entity_del = ws_entity.clone();
    let del_btn = div()
        .id(gpui::SharedString::from(format!("vcard-del-{}", sid)))
        .w(px(22.0))
        .h(px(22.0))
        .rounded(px(4.0))
        .flex()
        .items_center()
        .justify_center()
        .cursor_pointer()
        .hover({
            let bg = t.danger.opacity(0.12);
            move |h| h.bg(bg)
        })
        .child(crate::icons::svg_icon(icons::TRASH_SVG, px(12.0), t.danger))
        .on_click(move |_, _, cx| {
            cx.stop_propagation();
            let sid = sid_for_del.clone();
            let sname = s_name_for_del.clone();
            let _ = ws_entity_del.update(cx, |ws, cx| {
                ws.ui.confirm = Some(super::ConfirmState {
                    title: ws.i18n.t("删除 MCP 服务器", "Delete MCP Server").to_string(),
                    message: ws
                        .i18n
                        .t(
                            &format!("确定要删除 MCP 服务器“{}”吗？", sname),
                            &format!("Are you sure you want to delete MCP server '{}'?", sname),
                        )
                        .to_string(),
                    action: super::ConfirmAction::DeleteMcp { id: sid },
                });
                cx.notify();
            });
        });

    // Header row
    let header_row = div()
        .flex()
        .items_center()
        .justify_between()
        .gap(px(6.0))
        .w_full()
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(6.0))
                .min_w(px(0.0))
                .flex_1()
                .child(
                    div()
                        .w(px(8.0))
                        .h(px(8.0))
                        .rounded(px(4.0))
                        .bg(if management_enabled {
                            crate::rgba_const(0x10b981ff)
                        } else {
                            t.card_border
                        }),
                )
                .child(
                    div()
                        .text_size(px(13.5))
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .text_color(if management_enabled {
                            t.text_primary
                        } else {
                            t.text_secondary
                        })
                        .whitespace_nowrap()
                        .overflow_hidden()
                        .text_ellipsis()
                        .child(s_name),
                ),
        )
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(3.0))
                .child(toggle_btn)
                .child(copy_btn)
                .child(meta_btn)
                .child(edit_btn)
                .child(del_btn),
        );

    // Body: Command line (monospace) + optional tags/note/desc (matching SkillCard)
    let mut body_col = div()
        .flex_1()
        .flex()
        .flex_col()
        .justify_center()
        .gap(px(2.0))
        .min_w(px(0.0))
        .child(
            div()
                .font_family("Consolas, monospace")
                .text_size(px(11.5))
                .text_color(t.text_secondary)
                .whitespace_nowrap()
                .overflow_hidden()
                .text_ellipsis()
                .child(command_text),
        );

    let desc_str = s.description.as_deref().unwrap_or("").trim();
    if !s.tags.is_empty() || s.user_group.is_some() || s.user_note.is_some() {
        let mut tags_bar = div().flex().items_center().gap(px(4.0)).min_w(px(0.0));
        for tag in s.tags.iter().take(2) {
            let (bg, fg) = tag_color_palette(tag);
            tags_bar = tags_bar.child(
                div()
                    .px(px(6.0))
                    .h(px(18.0))
                    .rounded(px(9.0))
                    .bg(bg)
                    .flex()
                    .items_center()
                    .child(
                        div()
                            .text_size(px(10.5))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(fg)
                            .whitespace_nowrap()
                            .child(tag.clone()),
                    ),
            );
        }
        if let Some(ref grp) = s.user_group {
            tags_bar = tags_bar.child(
                div()
                    .px(px(6.0))
                    .h(px(18.0))
                    .rounded(px(9.0))
                    .bg(t.sidebar_bg)
                    .border_1()
                    .border_color(t.card_border)
                    .flex()
                    .items_center()
                    .child(
                        div()
                            .text_size(px(10.5))
                            .text_color(t.text_muted)
                            .whitespace_nowrap()
                            .child(grp.clone()),
                    ),
            );
        }
        if let Some(ref note) = s.user_note {
            tags_bar = tags_bar.child(
                div()
                    .text_size(px(10.5))
                    .text_color(t.text_muted)
                    .whitespace_nowrap()
                    .overflow_hidden()
                    .text_ellipsis()
                    .child(note.clone()),
            );
        }
        body_col = body_col.child(tags_bar);
    } else if !desc_str.is_empty() {
        body_col = body_col.child(
            div()
                .text_size(px(11.5))
                .text_color(t.text_secondary)
                .whitespace_nowrap()
                .overflow_hidden()
                .text_ellipsis()
                .child(desc_str.to_string()),
        );
    }

    // Footer row: Source/transport + time on left, synced tools on right!
    let mut footer_tools = div().flex().items_center().gap(px(4.0));
    for tool in tools {
        if s.is_enabled_in(*tool) {
            let tool_id_val = *tool;
            let sid_tool = sid.clone();
            let ws_entity_tool = ws_entity.clone();
            let tool_icon_svg = crate::icons::tool_icon(*tool);
            let pill = div()
                .id(gpui::SharedString::from(format!("vcard-mcp-tool-{}-{}", sid, tool.key())))
                .w(px(20.0))
                .h(px(20.0))
                .rounded(px(4.0))
                .bg(t.input_bg)
                .border_1()
                .border_color(t.accent.opacity(0.4))
                .flex()
                .items_center()
                .justify_center()
                .cursor_pointer()
                .hover({
                    let bg = t.card_hover;
                    move |h| h.bg(bg)
                })
                .child(crate::icons::svg_icon(tool_icon_svg, px(12.0), t.text_primary))
                .on_click(move |_, _, cx| {
                    cx.stop_propagation();
                    let sid = sid_tool.clone();
                    let _ = ws_entity_tool.update(cx, |ws, cx| {
                        let _ = ws.store.update(|db| {
                            aitoolplus_core::mcp::toggle_tool(&mut db.mcp, &sid, tool_id_val);
                        });
                        let server = ws.store.store().mcp.servers.iter().find(|x| x.id == sid).cloned();
                        if let Some(s) = server {
                            if s.is_enabled_in(tool_id_val) {
                                let _ = aitoolplus_core::mcp::sync_server_to_tool(&ws.paths, &s, tool_id_val);
                            } else {
                                let _ = aitoolplus_core::mcp::remove_server_from_tool(&ws.paths, &s.name, tool_id_val);
                            }
                        }
                        ws.persist_store();
                        cx.notify();
                    });
                });
            footer_tools = footer_tools.child(pill);
        }
    }

    // Plus button to open detail drawer (mirroring ai-toolbox .addToolBtn)
    let sid_plus = sid.clone();
    let ws_entity_plus = ws_entity.clone();
    let plus_btn = div()
        .id(gpui::SharedString::from(format!("vcard-mcp-plus-{}", sid)))
        .w(px(20.0))
        .h(px(20.0))
        .rounded(px(4.0))
        .border_1()
        .border_dashed()
        .border_color(t.input_border)
        .flex()
        .items_center()
        .justify_center()
        .cursor_pointer()
        .hover({
            let bg = t.card_hover;
            move |h| h.bg(bg)
        })
        .child(crate::icons::svg_icon(icons::PLUS_SVG, px(10.0), t.text_muted))
        .on_click(move |_, _, cx| {
            cx.stop_propagation();
            let sid = sid_plus.clone();
            let _ = ws_entity_plus.update(cx, |ws, cx| {
                ws.ui.selected_mcp_id = Some(sid);
                cx.notify();
            });
        });
    footer_tools = footer_tools.child(plus_btn);

    let footer_row = div()
        .pt(px(4.0))
        .border_t_1()
        .border_color(t.card_border.opacity(0.5))
        .flex()
        .items_center()
        .justify_between()
        .gap(px(6.0))
        .w_full()
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(4.0))
                .min_w(px(0.0))
                .child(crate::icons::svg_icon(transport_icon, px(11.0), t.text_secondary))
                .child(
                    div()
                        .text_size(px(11.0))
                        .text_color(t.text_secondary)
                        .whitespace_nowrap()
                        .overflow_hidden()
                        .text_ellipsis()
                        .child(s.server_type.as_str().to_lowercase()),
                )
                .child(
                    div()
                        .text_size(px(11.0))
                        .text_color(t.text_muted)
                        .whitespace_nowrap()
                        .child(format!("· {updated_str}")),
                ),
        )
        .child(footer_tools);

    // Card click opens detail drawer
    let sid_for_drawer = sid.clone();
    let ws_entity_drawer = ws_entity.clone();

    div()
        .id(gpui::SharedString::from(format!("v-mcp-card-{}", sid)))
        .w_full()
        .h_full()
        .min_w(px(0.0))
        .p(px(8.0))
        .px(px(10.0))
        .rounded(px(8.0))
        .bg(t.card_bg)
        .border_1()
        .border_color(t.card_border)
        .shadow_xs()
        .cursor_pointer()
        .when(!management_enabled, |this| this.opacity(0.55))
        .hover({
            let bg = t.card_hover;
            let border = t.card_border_hover;
            move |h| h.bg(bg).border_color(border)
        })
        .flex()
        .flex_col()
        .justify_between()
        .child(header_row)
        .child(body_col)
        .child(footer_row)
        .on_click(move |_, _, cx| {
            let sid = sid_for_drawer.clone();
            let _ = ws_entity_drawer.update(cx, |ws, cx| {
                ws.ui.selected_mcp_id = Some(sid);
                cx.notify();
            });
        })
        .into_any_element()
}

pub fn render_mcp_card(
    s: &McpServer,
    ws: &mut Workspace,
    cx: &mut Context<Workspace>,
) -> gpui::AnyElement {
    let tools = mcp_tools();
    let ws_entity = cx.entity();
    let t = ws.theme.clone();
    let i = ws.i18n.clone();
    render_virtual_mcp_card(s, &tools, &ws_entity, &t, &i)
}

pub fn render_mcp_detail_drawer(
    mcp_id: &str,
    ws: &mut Workspace,
    cx: &mut Context<Workspace>,
) -> gpui::AnyElement {
    let t = ws.theme.clone();
    let i = ws.i18n;

    let srv = ws
        .store
        .store()
        .mcp
        .servers
        .iter()
        .find(|s| s.id == mcp_id)
        .cloned();

    let Some(srv) = srv else {
        return div().into_any_element();
    };

    let sid = srv.id.clone();
    let management_enabled = srv.management_enabled;

    // Overlay backdrop
    let backdrop = div()
        .id("mcp-drawer-backdrop")
        .absolute()
        .inset_0()
        .occlude()
        .bg(gpui::rgba(0x00000038))
        .on_scroll_wheel(|_, _, cx| {
            cx.stop_propagation();
        })
        .on_click(cx.listener(|ws, _, _, cx| {
            ws.ui.selected_mcp_id = None;
            cx.notify();
        }));

    // Header: Status dot + Title + Close Button
    let status_dot = if management_enabled {
        div()
            .size(px(9.0))
            .rounded_full()
            .bg(crate::rgba_const(0x10b981ff))
    } else {
        div()
            .size(px(9.0))
            .rounded_full()
            .bg(crate::rgba_const(0x6b7280ff))
    };

    let header = div()
        .flex()
        .items_center()
        .justify_between()
        .w_full()
        .pb(px(12.0))
        .border_b_1()
        .border_color(t.card_border)
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(8.0))
                .child(status_dot)
                .child(
                    div()
                        .text_size(px(16.0))
                        .font_weight(gpui::FontWeight::BOLD)
                        .text_color(t.text_primary)
                        .child(srv.name.clone()),
                ),
        )
        .child(
            div()
                .id("mcp-drawer-close-btn")
                .cursor_pointer()
                .size(px(26.0))
                .rounded(px(6.0))
                .flex()
                .items_center()
                .justify_center()
                .text_color(t.text_secondary)
                .hover(move |h| h.bg(t.card_hover).text_color(t.text_primary))
                .child(
                    gpui::svg()
                        .data(icons::X_SVG)
                        .size(px(14.0))
                        .text_color(t.text_secondary),
                )
                .on_click(cx.listener(|ws, _, _, cx| {
                    ws.ui.selected_mcp_id = None;
                    cx.notify();
                })),
        );

    // Source line: Icon + Transport/Cmd + Updated relative time
    let transport_icon = match srv.server_type {
        McpServerType::Stdio => icons::CODE_SVG,
        McpServerType::Http | McpServerType::Sse => icons::GLOBE_SVG,
    };
    let rel_time = format_relative_time(srv.updated_at, &i);

    let source_line = div()
        .flex()
        .items_center()
        .gap(px(6.0))
        .text_size(px(12.0))
        .text_color(t.text_secondary)
        .child(
            gpui::svg()
                .data(transport_icon)
                .size(px(13.0))
                .text_color(t.text_secondary),
        )
        .child(div().child(srv.server_type.as_str().to_uppercase()))
        .child(div().text_color(t.text_muted).child("·"))
        .child(div().text_color(t.text_muted).child(rel_time));

    // Config Card
    let config_card = {
        let mut card = div()
            .flex()
            .flex_col()
            .gap(px(8.0))
            .p(px(12.0))
            .rounded(px(8.0))
            .bg(t.card_bg)
            .border_1()
            .border_color(t.card_border)
            .w_full();

        card = card.child(
            div()
                .text_size(px(12.5))
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .text_color(t.text_primary)
                .child(i.t("配置信息", "Configuration")),
        );

        match srv.server_type {
            McpServerType::Stdio => {
                let cmd = srv
                    .server_config
                    .get("command")
                    .and_then(Value::as_str)
                    .unwrap_or("—");
                let cmd_copy = cmd.to_string();
                card = card.child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .gap(px(8.0))
                        .child(
                            div()
                                .text_size(px(11.5))
                                .text_color(t.text_secondary)
                                .child("command:"),
                        )
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(4.0))
                                .child(
                                    div()
                                        .font_family("Consolas, monospace")
                                        .text_size(px(11.5))
                                        .text_color(t.text_primary)
                                        .child(cmd.to_string()),
                                )
                                .child(icon_button_svg(
                                    "copy-cmd",
                                    icons::COPY_SVG,
                                    i.t("复制", "Copy"),
                                    false,
                                    &t,
                                    cx,
                                    move |ws, _, _, cx| {
                                        cx.write_to_clipboard(gpui::ClipboardItem::new_string(
                                            cmd_copy.clone(),
                                        ));
                                        ws.ui.toast(
                                            ws.i18n.t("已复制", "Copied").to_string(),
                                            false,
                                        );
                                        cx.notify();
                                    },
                                )),
                        ),
                );

                if let Some(args_arr) = srv.server_config.get("args").and_then(Value::as_array)
                    && !args_arr.is_empty()
                {
                    let mut args_row = div().flex().items_center().gap(px(4.0)).flex_wrap();
                    for a in args_arr {
                        if let Some(arg_str) = a.as_str() {
                            args_row = args_row.child(
                                div()
                                    .px(px(6.0))
                                    .py(px(1.5))
                                    .rounded(px(4.0))
                                    .bg(t.pill_bg)
                                    .border_1()
                                    .border_color(t.card_border)
                                    .font_family("Consolas, monospace")
                                    .text_size(px(11.0))
                                    .text_color(t.text_primary)
                                    .child(arg_str.to_string()),
                            );
                        }
                    }
                    card = card.child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(4.0))
                            .child(
                                div()
                                    .text_size(px(11.5))
                                    .text_color(t.text_secondary)
                                    .child("args:"),
                            )
                            .child(args_row),
                    );
                }

                if let Some(env_obj) = srv.server_config.get("env").and_then(Value::as_object)
                    && !env_obj.is_empty()
                {
                    let mut env_list = div().flex().flex_col().gap(px(4.0));
                    for (k, v) in env_obj {
                        let v_str = v.as_str().unwrap_or("");
                        let v_copy = v_str.to_string();
                        env_list = env_list.child(
                            div()
                                .flex()
                                .items_center()
                                .justify_between()
                                .gap(px(6.0))
                                .child(
                                    div()
                                        .font_family("Consolas, monospace")
                                        .text_size(px(11.0))
                                        .text_color(t.accent)
                                        .child(format!("{k}:")),
                                )
                                .child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .gap(px(4.0))
                                        .child(
                                            div()
                                                .font_family("Consolas, monospace")
                                                .text_size(px(11.0))
                                                .text_color(t.text_secondary)
                                                .child(v_str.to_string()),
                                        )
                                        .child(icon_button_svg(
                                            gpui::SharedString::from(format!("copy-env-{k}")),
                                            icons::COPY_SVG,
                                            i.t("复制", "Copy"),
                                            false,
                                            &t,
                                            cx,
                                            move |ws, _, _, cx| {
                                                cx.write_to_clipboard(
                                                    gpui::ClipboardItem::new_string(v_copy.clone()),
                                                );
                                                ws.ui.toast(
                                                    ws.i18n.t("已复制", "Copied").to_string(),
                                                    false,
                                                );
                                                cx.notify();
                                            },
                                        )),
                                ),
                        );
                    }
                    card = card.child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(4.0))
                            .child(
                                div()
                                    .text_size(px(11.5))
                                    .text_color(t.text_secondary)
                                    .child("env:"),
                            )
                            .child(env_list),
                    );
                }
            }
            McpServerType::Http | McpServerType::Sse => {
                let url = srv
                    .server_config
                    .get("url")
                    .and_then(Value::as_str)
                    .unwrap_or("—");
                let url_copy = url.to_string();
                card = card.child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .gap(px(8.0))
                        .child(
                            div()
                                .text_size(px(11.5))
                                .text_color(t.text_secondary)
                                .child("url:"),
                        )
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(4.0))
                                .child(
                                    div()
                                        .font_family("Consolas, monospace")
                                        .text_size(px(11.5))
                                        .text_color(t.text_primary)
                                        .child(url.to_string()),
                                )
                                .child(icon_button_svg(
                                    "copy-url",
                                    icons::COPY_SVG,
                                    i.t("复制", "Copy"),
                                    false,
                                    &t,
                                    cx,
                                    move |ws, _, _, cx| {
                                        cx.write_to_clipboard(gpui::ClipboardItem::new_string(
                                            url_copy.clone(),
                                        ));
                                        ws.ui.toast(
                                            ws.i18n.t("已复制", "Copied").to_string(),
                                            false,
                                        );
                                        cx.notify();
                                    },
                                )),
                        ),
                );
            }
        }
        card
    };

    // Metadata Card
    let sid_for_meta_edit = sid.clone();
    let meta_card = div()
        .flex()
        .flex_col()
        .gap(px(8.0))
        .p(px(12.0))
        .rounded(px(8.0))
        .bg(t.card_bg)
        .border_1()
        .border_color(t.card_border)
        .w_full()
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .child(
                    div()
                        .text_size(px(12.5))
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .text_color(t.text_primary)
                        .child(i.t("元数据", "Metadata")),
                )
                .child(
                    div()
                        .id("mcp-meta-edit-btn")
                        .cursor_pointer()
                        .text_size(px(11.5))
                        .text_color(t.accent)
                        .hover(|h| h.underline())
                        .child(i.t("编辑", "Edit"))
                        .on_click(cx.listener(move |ws, _, _, cx| {
                            let existing = ws
                                .store
                                .store()
                                .mcp
                                .servers
                                .iter()
                                .find(|x| x.id == sid_for_meta_edit)
                                .cloned();
                            let group_input = cx.new(|cx| {
                                let mut inp = TextInput::new(ws.i18n.t("分组", "Group"), cx);
                                if let Some(g) =
                                    existing.as_ref().and_then(|x| x.user_group.clone())
                                {
                                    inp.set_text_silent(g, cx);
                                }
                                inp
                            });
                            let note_input = cx.new(|cx| {
                                let mut inp = TextInput::new(ws.i18n.t("备注", "Note"), cx);
                                if let Some(n) = existing.as_ref().and_then(|x| x.user_note.clone())
                                {
                                    inp.set_text_silent(n, cx);
                                }
                                inp
                            });
                            ws.ui.mcp_editing_metadata =
                                Some((sid_for_meta_edit.clone(), group_input, note_input));
                            cx.notify();
                        })),
                ),
        )
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .child(
                    div()
                        .text_size(px(11.5))
                        .text_color(t.text_secondary)
                        .child(i.t("分组：", "Group:")),
                )
                .child(
                    div()
                        .text_size(px(11.5))
                        .text_color(if srv.user_group.is_some() {
                            t.text_primary
                        } else {
                            t.text_muted
                        })
                        .child(
                            srv.user_group
                                .clone()
                                .unwrap_or_else(|| i.t("未分组", "Ungrouped").to_string()),
                        ),
                ),
        )
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .child(
                    div()
                        .text_size(px(11.5))
                        .text_color(t.text_secondary)
                        .child(i.t("备注：", "Note:")),
                )
                .child(
                    div()
                        .text_size(px(11.5))
                        .text_color(if srv.user_note.is_some() {
                            t.text_primary
                        } else {
                            t.text_muted
                        })
                        .child(
                            srv.user_note
                                .clone()
                                .unwrap_or_else(|| i.t("无备注", "No notes").to_string()),
                        ),
                ),
        );

    // Tags Section
    let sid_for_tag = sid.clone();
    let mut tags_section = div()
        .flex()
        .flex_col()
        .gap(px(6.0))
        .w_full()
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .child(
                    div()
                        .text_size(px(12.5))
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .text_color(t.text_primary)
                        .child(i.t("标签", "Tags")),
                )
                .child(
                    div()
                        .id("mcp-add-tag-btn")
                        .cursor_pointer()
                        .text_size(px(11.5))
                        .text_color(t.accent)
                        .hover(|h| h.underline())
                        .child(i.t("+ 添加标签", "+ Add Tag"))
                        .on_click(cx.listener(move |ws, _, _, cx| {
                            let tag_input =
                                cx.new(|cx| TextInput::new(ws.i18n.t("标签名", "Tag Name"), cx));
                            ws.ui.mcp_adding_tag = Some((sid_for_tag.clone(), tag_input));
                            cx.notify();
                        })),
                ),
        );

    if srv.tags.is_empty() {
        tags_section = tags_section.child(
            div()
                .text_size(px(11.5))
                .text_color(t.text_muted)
                .child(i.t("暂无标签", "No tags yet")),
        );
    } else {
        let mut tag_list = div().flex().items_center().gap(px(6.0)).flex_wrap();
        for (idx, tag) in srv.tags.iter().enumerate() {
            let (bg_color, fg_color) = tag_color_palette(tag);
            let remove_sid = sid.clone();
            let tag_name = tag.clone();
            tag_list = tag_list.child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(4.0))
                    .px(px(6.0))
                    .py(px(2.0))
                    .rounded(px(4.0))
                    .bg(bg_color)
                    .text_size(px(11.0))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .text_color(fg_color)
                    .child(tag.clone())
                    .child(
                        div()
                            .id(gpui::SharedString::from(format!(
                                "del-tag-{sid}-{idx}"
                            )))
                            .cursor_pointer()
                            .size(px(12.0))
                            .rounded_full()
                            .flex()
                            .items_center()
                            .justify_center()
                            .hover(move |h| h.bg(crate::rgba_const(0x00000022)))
                            .child(
                                gpui::svg()
                                    .data(icons::X_SVG)
                                    .size(px(9.0))
                                    .text_color(fg_color),
                            )
                            .on_click(cx.listener(move |ws, _, _, cx| {
                                let mut updated = ws
                                    .store
                                    .store()
                                    .mcp
                                    .servers
                                    .iter()
                                    .find(|x| x.id == remove_sid)
                                    .map(|x| x.tags.clone())
                                    .unwrap_or_default();
                                updated.retain(|x| x != &tag_name);
                                let _ = ws.store.update(|db| {
                                    aitoolplus_core::mcp::update_tags(
                                        &mut db.mcp,
                                        &remove_sid,
                                        updated,
                                    );
                                });
                                ws.persist_store();
                                cx.notify();
                            })),
                    ),
            );
        }
        tags_section = tags_section.child(tag_list);
    }

    // Tools Grid
    let enabled_tools_set: std::collections::HashSet<&str> =
        srv.enabled_tools.iter().map(|k| k.as_str()).collect();

    let mut tools_grid = div()
        .flex()
        .flex_col()
        .gap(px(6.0))
        .w_full()
        .child(
            div()
                .text_size(px(12.5))
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .text_color(t.text_primary)
                .child(i.t("支持工具同步", "Supported Tools Sync")),
        );

    let mut grid_items = div().flex().gap(px(8.0)).flex_wrap().w_full();

    for tool in mcp_tools() {
        let is_on = enabled_tools_set.contains(tool.key());
        let toggle_sid = sid.clone();
        let cell_bg = if is_on {
            t.accent_subtle
        } else {
            t.pill_bg
        };
        let cell_border = if is_on {
            t.accent
        } else {
            t.card_border
        };

        grid_items = grid_items.child(
            div()
                .id(gpui::SharedString::from(format!(
                    "drawer-tool-{sid}-{}",
                    tool.key()
                )))
                .cursor_pointer()
                .flex()
                .items_center()
                .gap(px(6.0))
                .px(px(10.0))
                .py(px(6.0))
                .rounded(px(6.0))
                .bg(cell_bg)
                .border_1()
                .border_color(cell_border)
                .hover(|h| h.opacity(0.85))
                .child(
                    div()
                        .size(px(6.0))
                        .rounded_full()
                        .bg(if is_on {
                            t.accent
                        } else {
                            crate::rgba_const(0x6b7280ff)
                        }),
                )
                .child(
                    div()
                        .text_size(px(12.0))
                        .font_weight(gpui::FontWeight::MEDIUM)
                        .text_color(if is_on {
                            t.accent
                        } else {
                            t.text_secondary
                        })
                        .child(i.t(tool.name_zh(), tool.name_en())),
                )
                .on_click(cx.listener(move |ws, _, _, cx| {
                    let _ = ws.store.update(|db| {
                        aitoolplus_core::mcp::toggle_tool(&mut db.mcp, &toggle_sid, tool);
                    });
                    ws.persist_store();
                    cx.notify();
                })),
        );
    }
    tools_grid = tools_grid.child(grid_items);

    // Footer Bar: Edit, Metadata, Disable/Enable, Delete
    let sid_footer_edit = sid.clone();
    let sid_footer_meta = sid.clone();
    let sid_footer_state = sid.clone();
    let sid_footer_del = sid.clone();
    let name_footer_del = srv.name.clone();

    let footer_bar = div()
        .flex()
        .items_center()
        .justify_between()
        .pt(px(12.0))
        .border_t_1()
        .border_color(t.card_border)
        .w_full()
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(6.0))
                .child(button_with_icon_l(
                    "mcp-drawer-footer-edit",
                    icons::PENCIL_SVG,
                    i.t("编辑", "Edit"),
                    ButtonVariant::Secondary,
                    &t,
                    cx,
                    move |ws, _, window, cx| {
                        open_mcp_dialog(Some(sid_footer_edit.clone()), ws, cx);
                        if let Some(dlg) = &ws.ui.mcp_dialog {
                            dlg.name.update(cx, |name, cx| {
                                name.focus_handle.focus(window, cx);
                                name.start_blink(cx);
                            });
                        }
                    },
                ))
                .child(button_with_icon_l(
                    "mcp-drawer-footer-meta",
                    icons::TAG_SVG,
                    i.t("元数据", "Metadata"),
                    ButtonVariant::Secondary,
                    &t,
                    cx,
                    move |ws, _, _, cx| {
                        let existing = ws
                            .store
                            .store()
                            .mcp
                            .servers
                            .iter()
                            .find(|x| x.id == sid_footer_meta)
                            .cloned();
                        let group_input = cx.new(|cx| {
                            let mut inp = TextInput::new(ws.i18n.t("分组", "Group"), cx);
                            if let Some(g) = existing.as_ref().and_then(|x| x.user_group.clone()) {
                                inp.set_text_silent(g, cx);
                            }
                            inp
                        });
                        let note_input = cx.new(|cx| {
                            let mut inp = TextInput::new(ws.i18n.t("备注", "Note"), cx);
                            if let Some(n) = existing.as_ref().and_then(|x| x.user_note.clone()) {
                                inp.set_text_silent(n, cx);
                            }
                            inp
                        });
                        ws.ui.mcp_editing_metadata =
                            Some((sid_footer_meta.clone(), group_input, note_input));
                        cx.notify();
                    },
                ))
                .child(button_with_icon_l(
                    "mcp-drawer-footer-toggle",
                    if management_enabled {
                        icons::POWER_OFF_SVG
                    } else {
                        icons::POWER_SVG
                    },
                    if management_enabled {
                        i.t("停用", "Disable")
                    } else {
                        i.t("启用", "Enable")
                    },
                    ButtonVariant::Secondary,
                    &t,
                    cx,
                    move |ws, _, _, cx| {
                        let _ = ws.store.update(|db| {
                            aitoolplus_core::mcp::set_management_enabled(
                                &mut db.mcp,
                                &sid_footer_state,
                                !management_enabled,
                            );
                        });
                        ws.persist_store();
                        cx.notify();
                    },
                )),
        )
        .child(button_with_icon_l(
            "mcp-drawer-footer-del",
            icons::TRASH_SVG,
            i.t("删除", "Delete"),
            ButtonVariant::Danger,
            &t,
            cx,
            move |ws, _, _, cx| {
                ws.ui.confirm = Some(super::ConfirmState {
                    title: ws.i18n.t("删除 MCP 服务器", "Delete MCP Server").to_string(),
                    message: ws
                        .i18n
                        .t(
                            &format!("确定要删除 MCP 服务器“{}”吗？", name_footer_del),
                            &format!(
                                "Are you sure you want to delete MCP server '{}'?",
                                name_footer_del
                            ),
                        )
                        .to_string(),
                    action: super::ConfirmAction::DeleteMcp {
                        id: sid_footer_del.clone(),
                    },
                });
                cx.notify();
            },
        ));

    // Drawer Container
    let drawer = div()
        .id("mcp-drawer-panel")
        .absolute()
        .top_0()
        .right_0()
        .bottom_0()
        .h_full()
        .w(px(540.0))
        .occlude()
        .bg(t.card_bg)
        .border_l_1()
        .border_color(t.card_border)
        .shadow_lg()
        .flex()
        .flex_col()
        .overflow_hidden()
        .on_scroll_wheel(|_, _, cx| {
            cx.stop_propagation();
        })
        .child(
            div()
                .p(px(20.0))
                .pb(px(0.0))
                .child(header),
        )
        .child(
            div()
                .id("mcp-drawer-scroll")
                .flex_1()
                .overflow_y_scroll()
                .p(px(20.0))
                .flex()
                .flex_col()
                .gap(px(16.0))
                .child(source_line)
                .child(config_card)
                .child(meta_card)
                .child(tags_section)
                .child(tools_grid),
        )
        .child(
            div()
                .p(px(20.0))
                .pt(px(0.0))
                .child(footer_bar),
        );

    div()
        .id("mcp-drawer-overlay")
        .absolute()
        .inset_0()
        .child(backdrop)
        .child(drawer)
        .into_any_element()
}

pub fn render_mcp_import_json_modal(
    json_editor: gpui::Entity<TextArea>,
    ws: &mut Workspace,
    cx: &mut Context<Workspace>,
) -> gpui::AnyElement {
    let t = ws.theme.clone();
    let i = ws.i18n;

    let body = div()
        .flex()
        .flex_col()
        .gap(px(12.0))
        .w_full()
        .child(
            div()
                .text_size(px(12.5))
                .text_color(t.text_secondary)
                .child(i.t(
                    "支持从 Claude Desktop、Cursor、VSCode 或标准 MCP JSON 配置片段直接粘贴并导入：",
                    "Supports pasting MCP JSON configuration from Claude Desktop, Cursor, VSCode, etc.:",
                )),
        )
        .child(
            div()
                .w_full()
                .h(px(380.0))
                .id("mcp-json-editor-wrap")
                .rounded(px(6.0))
                .bg(t.input_bg)
                .border_1()
                .border_color(t.input_border)
                .shadow_xs()
                .cursor_text()
                .track_focus(&json_editor.read(cx).focus_handle)
                .focus(|s| s.border_color(crate::rgba_const(0x3b82f6cc)))
                .hover(move |h| h.border_color(t.card_border_hover))
                .overflow_y_scroll()
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener({
                        let editor = json_editor.clone();
                        move |_this, event: &MouseDownEvent, window, cx| {
                            editor.update(cx, |ta, cx| {
                                ta.focus_handle.focus(window, cx);
                                ta.start_blink(cx);
                                ta.on_mouse_down(event.position, event.click_count, cx);
                            });
                        }
                    }),
                )
                .child({
                    json_editor.update(cx, |ed, _| ed.borderless = true);
                    json_editor.clone()
                }),
        )
        .child(
            div()
                .flex()
                .justify_end()
                .gap(px(8.0))
                .child(button_l(
                    "mcp-import-json-cancel",
                    i.t("取消", "Cancel"),
                    ButtonVariant::Secondary,
                    &t,
                    cx,
                    |ws, _, _, cx| {
                        ws.ui.mcp_import_json_modal = None;
                        cx.notify();
                    },
                ))
                .child(button_l(
                    "mcp-import-json-submit",
                    i.t("解析并导入", "Parse & Import"),
                    ButtonVariant::Primary,
                    &t,
                    cx,
                    move |ws, _, _, cx| {
                        let json_text: String = json_editor.update(cx, |ed, _| ed.content.clone());
                        match aitoolplus_core::mcp::parse_mcp_servers_from_json(&json_text) {
                            Ok(servers) if !servers.is_empty() => {
                                let count = servers.len();
                                let _ = ws.store.update(|db| {
                                    for (name, stype, config) in servers {
                                        let s = McpServer::new(name, stype, config);
                                        aitoolplus_core::mcp::upsert(&mut db.mcp, s);
                                    }
                                });
                                ws.persist_store();

                                // Auto sync imported servers to tools
                                let mut mcp_store = ws.store.store().mcp.clone();
                                aitoolplus_core::mcp::sync_all_enabled(&ws.paths, &mut mcp_store);
                                let _ = ws.store.update(|db| db.mcp = mcp_store);
                                ws.persist_store();

                                ws.ui.mcp_import_json_modal = None;
                                let msg = ws
                                    .i18n
                                    .t(
                                        &format!("成功导入 {} 个 MCP 服务器", count),
                                        &format!("Successfully imported {count} MCP servers"),
                                    )
                                    .to_string();
                                ws.ui.toast(msg, false);
                                cx.notify();
                            }
                            Ok(_) => {
                                ws.ui.toast(
                                    ws.i18n
                                        .t(
                                            "未能从 JSON 中解析出任何 MCP 服务器",
                                            "No MCP servers found in JSON",
                                        )
                                        .to_string(),
                                    true,
                                );
                                cx.notify();
                            }
                            Err(e) => {
                                ws.ui.toast(
                                    ws.i18n
                                        .t(
                                            &format!("JSON 解析失败: {}", e),
                                            &format!("JSON parse error: {e}"),
                                        )
                                        .to_string(),
                                    true,
                                );
                                cx.notify();
                            }
                        }
                    },
                )),
        );

    super::modal_scaffold_sized(
        &t,
        i.t("导入 MCP 配置 (JSON)", "Import MCP (JSON)").as_ref(),
        px(780.0),
        None,
        body.into_any_element(),
        cx,
        |ws, _, _, cx| {
            ws.ui.mcp_import_json_modal = None;
            cx.notify();
        },
    )
}

pub fn render_mcp_import_existing_modal(
    ws: &mut Workspace,
    cx: &mut Context<Workspace>,
) -> gpui::AnyElement {
    let t = ws.theme.clone();
    let i = ws.i18n;

    let body = div()
        .flex()
        .flex_col()
        .gap(px(14.0))
        .child(
            div()
                .text_size(px(12.5))
                .text_color(t.text_secondary)
                .child(i.t(
                    "扫描本地所有已安装的 AI 编程工具（Claude Desktop、Cursor、VSCode 等），导入已有的 MCP 服务器配置。",
                    "Scan all installed AI coding tools (Claude Desktop, Cursor, VSCode, etc.) and import existing MCP configs.",
                )),
        )
        .child(
            div()
                .flex()
                .justify_end()
                .gap(px(8.0))
                .child(button_l(
                    "mcp-import-exist-cancel",
                    i.t("取消", "Cancel"),
                    ButtonVariant::Secondary,
                    &t,
                    cx,
                    |ws, _, _, cx| {
                        ws.ui.mcp_import_existing_modal = false;
                        cx.notify();
                    },
                ))
                .child(button_l(
                    "mcp-import-exist-submit",
                    i.t("立即扫描并导入", "Scan & Import Now"),
                    ButtonVariant::Primary,
                    &t,
                    cx,
                    |ws, _, _, cx| {
                        let mut store = ws.store.store().mcp.clone();
                        let (imported, sources) =
                            aitoolplus_core::mcp::import_discovered(&ws.paths, &mut store);
                        if imported > 0 {
                            let _ = ws.store.update(|db| db.mcp = store);
                            ws.persist_store();
                            let names: Vec<String> = sources
                                .iter()
                                .map(|(tool, name)| format!("{name} ({})", tool.name_en()))
                                .collect();
                            let msg = ws
                                .i18n
                                .t(
                                    &format!(
                                        "成功导入 {} 个 MCP 服务器：{}",
                                        imported,
                                        names.join(", ")
                                    ),
                                    &format!(
                                        "imported {imported} MCP servers: {}",
                                        names.join(", ")
                                    ),
                                )
                                .to_string();
                            ws.ui.toast(msg, false);
                        } else {
                            let msg = ws
                                .i18n
                                .t("未发现新的 MCP 服务器", "No new MCP servers discovered")
                                .to_string();
                            ws.ui.toast(msg, false);
                        }
                        ws.ui.mcp_import_existing_modal = false;
                        cx.notify();
                    },
                )),
        );

    modal_scaffold(
        &t,
        i.t("导入已有 MCP 配置", "Import Existing MCP").as_ref(),
        body.into_any_element(),
        cx,
        |ws, _, _, cx| {
            ws.ui.mcp_import_existing_modal = false;
            cx.notify();
        },
    )
}

pub fn render_mcp_metadata_modal(
    mcp_id: String,
    group_input: gpui::Entity<TextInput>,
    note_input: gpui::Entity<TextInput>,
    ws: &mut Workspace,
    cx: &mut Context<Workspace>,
) -> gpui::AnyElement {
    let t = ws.theme.clone();
    let i = ws.i18n;

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
                .child(field_label(i.t("分组 (Group)", "Group")))
                .child(input_container(&t, group_input.clone())),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(6.0))
                .child(field_label(i.t("备注 (Note)", "Note")))
                .child(input_container(&t, note_input.clone())),
        )
        .child(
            div()
                .flex()
                .justify_end()
                .gap(px(8.0))
                .child(button_l(
                    "mcp-meta-cancel",
                    i.t("取消", "Cancel"),
                    ButtonVariant::Secondary,
                    &t,
                    cx,
                    |ws, _, _, cx| {
                        ws.ui.mcp_editing_metadata = None;
                        cx.notify();
                    },
                ))
                .child(button_l(
                    "mcp-meta-save",
                    i.t("保存", "Save"),
                    ButtonVariant::Primary,
                    &t,
                    cx,
                    move |ws, _, _, cx| {
                        let group_text = group_input.update(cx, |inp, _| inp.text().to_string());
                        let note_text = note_input.update(cx, |inp, _| inp.text().to_string());
                        let _ = ws.store.update(|db| {
                            aitoolplus_core::mcp::update_metadata(
                                &mut db.mcp,
                                &mcp_id,
                                if group_text.trim().is_empty() {
                                    None
                                } else {
                                    Some(group_text.trim().to_string())
                                },
                                if note_text.trim().is_empty() {
                                    None
                                } else {
                                    Some(note_text.trim().to_string())
                                },
                            );
                        });
                        ws.persist_store();
                        ws.ui.mcp_editing_metadata = None;
                        let msg = ws
                            .i18n
                            .t("元数据已更新", "Metadata updated")
                            .to_string();
                        ws.ui.toast(msg, false);
                        cx.notify();
                    },
                )),
        );

    modal_scaffold(
        &t,
        i.t("编辑元数据", "Edit Metadata").as_ref(),
        body.into_any_element(),
        cx,
        |ws, _, _, cx| {
            ws.ui.mcp_editing_metadata = None;
            cx.notify();
        },
    )
}

pub fn render_mcp_add_tag_modal(
    mcp_id: String,
    tag_input: gpui::Entity<TextInput>,
    ws: &mut Workspace,
    cx: &mut Context<Workspace>,
) -> gpui::AnyElement {
    let t = ws.theme.clone();
    let i = ws.i18n;

    let body = div()
        .flex()
        .flex_col()
        .gap(px(12.0))
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(6.0))
                .child(
                    div()
                        .text_size(px(12.0))
                        .font_weight(gpui::FontWeight::MEDIUM)
                        .text_color(t.text_secondary)
                        .child(i.t("标签名称", "Tag Name")),
                )
                .child(input_container(&t, tag_input.clone())),
        )
        .child(
            div()
                .flex()
                .justify_end()
                .gap(px(8.0))
                .child(button_l(
                    "mcp-tag-cancel",
                    i.t("取消", "Cancel"),
                    ButtonVariant::Secondary,
                    &t,
                    cx,
                    |ws, _, _, cx| {
                        ws.ui.mcp_adding_tag = None;
                        cx.notify();
                    },
                ))
                .child(button_l(
                    "mcp-tag-submit",
                    i.t("添加", "Add"),
                    ButtonVariant::Primary,
                    &t,
                    cx,
                    move |ws, _, _, cx| {
                        let tag_text = tag_input.update(cx, |inp, _| inp.text().trim().to_string());
                        if !tag_text.is_empty() {
                            let mut tags = ws
                                .store
                                .store()
                                .mcp
                                .servers
                                .iter()
                                .find(|x| x.id == mcp_id)
                                .map(|x| x.tags.clone())
                                .unwrap_or_default();
                            if !tags.contains(&tag_text) {
                                tags.push(tag_text);
                                let _ = ws.store.update(|db| {
                                    aitoolplus_core::mcp::update_tags(
                                        &mut db.mcp,
                                        &mcp_id,
                                        tags,
                                    );
                                });
                                ws.persist_store();
                            }
                        }
                        ws.ui.mcp_adding_tag = None;
                        cx.notify();
                    },
                )),
        );

    modal_scaffold(
        &t,
        i.t("添加标签", "Add Tag").as_ref(),
        body.into_any_element(),
        cx,
        |ws, _, _, cx| {
            ws.ui.mcp_adding_tag = None;
            cx.notify();
        },
    )
}

#[allow(dead_code)]
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

pub fn open_mcp_dialog(
    editing_id: Option<String>,
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
                        .child(
                            div()
                                .flex_1()
                                .min_w(px(0.0))
                                .child(input_container(&t, command.clone())),
                        )
                        .child(
                            div()
                                .w(px(200.0))
                                .child(input_container(&t, args.clone())),
                        ),
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
