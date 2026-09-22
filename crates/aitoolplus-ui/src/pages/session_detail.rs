//! Unified Session Detail view: rich message blocks (Bash commands, thinking, tool calls, user/assistant text),
//! command output collapsing, copy actions, and virtualized/chunked performance optimization.

use std::sync::Arc;

use aitoolplus_core::session::{self, SessionMessage, SessionMessageBlock, SessionMeta};
use aitoolplus_core::tools::ToolId;
use gpui::{Context, IntoElement, ListAlignment, ListState, div, list, prelude::*, px};
use gpui_kit::component::scroll::{Scrollbar, ScrollbarMode};

use crate::components::{
    BadgeKind, ButtonVariant, badge, button_with_icon_l,
};
use crate::text_input::TextInput;
use crate::workspace::Workspace;

fn fmt_time(ms: Option<i64>) -> String {
    ms.and_then(|m| {
        chrono::DateTime::from_timestamp_millis(m).map(|dt| dt.format("%Y-%m-%d %H:%M").to_string())
    })
    .unwrap_or_else(|| "—".into())
}

fn reveal_in_explorer(path: &std::path::Path) {
    super::open_path_in_default_manager(path);
}

pub fn render_session_detail(
    tool: ToolId,
    meta: &SessionMeta,
    ws: &mut Workspace,
    cx: &mut Context<Workspace>,
) -> gpui::AnyElement {
    let t = ws.theme.clone();
    let i = ws.i18n;

    // Cache-backed message resolution: avoids re-reading/re-parsing JSONL on every render frame
    let messages: Arc<Vec<SessionMessage>> = match &ws.ui.session_messages_cache {
        Some((cached_tool, cached_sid, cached_msgs))
            if *cached_tool == tool && cached_sid == &meta.session_id =>
        {
            cached_msgs.clone()
        }
        _ => {
            let loaded = Arc::new(session::load_messages(&ws.paths, meta).unwrap_or_default());
            ws.ui.session_messages_cache = Some((tool, meta.session_id.clone(), loaded.clone()));
            loaded
        }
    };

    let display_title = session::sidecar_title(meta)
        .or_else(|| meta.title.clone())
        .or_else(|| meta.summary.clone())
        .unwrap_or_else(|| meta.session_id.clone());

    let mut section = div().flex().flex_col().w_full().h_full().min_h(px(0.0)).gap(px(10.0));

    // ---------------------------------------------------------------------------
    // Top Command Bar (Session info + actions + filter chips)
    // ---------------------------------------------------------------------------
    // Top Command Bar (Session info + actions dropdown + compact filter chips)
    let is_actions_menu_open = ws.ui.session_actions_menu_open;
    let mut top_bar = div()
        .relative()
        .flex()
        .flex_col()
        .gap(px(6.0))
        .w_full()
        .p(px(8.0))
        .rounded(px(8.0))
        .bg(t.card_bg)
        .border_1()
        .border_color(t.card_border);

    // Row 1: Back button + Title + Badges + Actions
    let mut row1 = div().flex().items_center().justify_between().gap(px(8.0)).w_full();

    let mut left_info = div().flex().items_center().gap(px(6.0)).flex_1().min_w(px(0.0)).overflow_hidden();
    left_info = left_info.child(button_with_icon_l(
        "session-detail-back-btn",
        crate::icons::ARROW_LEFT_SVG,
        i.t("返回", "Back"),
        ButtonVariant::Secondary,
        &t,
        cx,
        |ws, _, _, cx| {
            ws.ui.open_session = None;
            ws.ui.session_actions_menu_open = false;
            cx.notify();
        },
    ));

    left_info = left_info.child(
        div()
            .text_size(px(14.0))
            .font_weight(gpui::FontWeight::BOLD)
            .text_color(t.text_primary)
            .overflow_hidden()
            .text_ellipsis()
            .whitespace_nowrap()
            .child(display_title),
    );

    left_info = left_info.child(badge(&t, i.t(tool.name_zh(), tool.name_en()), BadgeKind::Accent));
    left_info = left_info.child(badge(&t, fmt_time(meta.last_active_at), BadgeKind::Neutral));
    if let Some(ref dir) = meta.project_dir {
        left_info = left_info.child(badge(&t, dir.clone(), BadgeKind::Neutral));
    }

    let mut right_actions = div().flex().items_center().gap(px(6.0)).flex_shrink_0().relative();

    // Quick Resume command if available
    if let Some(ref cmd) = meta.resume_command {
        let cmd_clone = cmd.clone();
        right_actions = right_actions.child(button_with_icon_l(
            "sess-copy-resume-btn",
            crate::icons::TERMINAL_SVG,
            i.t("恢复命令", "Resume"),
            ButtonVariant::Secondary,
            &t,
            cx,
            move |ws, _, _, cx| {
                cx.write_to_clipboard(gpui::ClipboardItem::new_string(cmd_clone.clone()));
                ws.ui.toast(ws.i18n.t("已复制恢复命令", "Copied resume command").to_string(), false);
                cx.notify();
            },
        ));
    }

    // Actions Dropdown Trigger (constant dimensions, zero layout shift)
    let btn_bg = if is_actions_menu_open { t.card_hover } else { t.tab_active_bg };
    let btn_border = if is_actions_menu_open { t.accent } else { t.card_border };
    let btn_text = if is_actions_menu_open { t.accent } else { t.text_primary };

    right_actions = right_actions.child(
        div()
            .id("sess-actions-dropdown-trigger")
            .cursor_pointer()
            .h(px(30.0))
            .px(px(10.0))
            .rounded(px(6.0))
            .bg(btn_bg)
            .border_1()
            .border_color(btn_border)
            .flex()
            .items_center()
            .justify_center()
            .gap(px(4.0))
            .text_size(px(12.5))
            .font_weight(gpui::FontWeight::MEDIUM)
            .text_color(btn_text)
            .hover(|h| h.bg(t.card_hover))
            .on_click(cx.listener(move |ws, _, _, cx| {
                if is_actions_menu_open {
                    ws.ui.session_actions_menu_open = false;
                } else {
                    ws.ui.session_actions_menu_open = true;
                }
                cx.notify();
            }))
            .child(i.t("操作", "Actions"))
            .child(crate::icons::svg_icon(
                if is_actions_menu_open {
                    crate::icons::CHEVRON_UP_SVG
                } else {
                    crate::icons::CHEVRON_DOWN_SVG
                },
                px(12.0),
                btn_text,
            )),
    );

    row1 = row1.child(left_info).child(right_actions);
    top_bar = top_bar.child(row1);

    // Row 2: Filter chips bar & pagination controls
    let filters = ws.settings.session_filters.clone();
    let total_count = messages.len();
    let visible_indices: Arc<Vec<usize>> = Arc::new(
        messages
            .iter()
            .enumerate()
            .filter_map(|(idx, message)| {
                let is_user = message.role == "user";
                let is_assistant = message.role == "assistant";
                let has_thinking = message.blocks.iter().any(|b| b.kind == "thinking");
                let has_command = message.blocks.iter().any(|b| b.kind == "command");
                let has_tool_call = message.blocks.iter().any(|b| b.kind == "tool_call" || b.kind == "tool_result");
                let has_text = !message.content.is_empty() || message.blocks.iter().any(|b| b.kind == "text");

                // Role filtering
                if is_user && !filters.user {
                    return None;
                }
                if is_assistant && !filters.assistant {
                    return None;
                }

                // Type filtering
                if has_thinking && !filters.thinking && !has_text && !has_command && !has_tool_call {
                    return None;
                }
                if has_command && !filters.command && !has_text && !has_thinking && !has_tool_call {
                    return None;
                }
                if has_tool_call && !filters.tool_call && !has_text && !has_thinking && !has_command {
                    return None;
                }
                if has_text && !filters.text && !has_thinking && !has_command && !has_tool_call {
                    return None;
                }

                Some(idx)
            })
            .collect(),
    );
    let visible_count = visible_indices.len();

    let mut filter_row = div().flex().items_center().justify_between().gap(px(8.0)).w_full();
    let stats_label = format!(
        "{} {} ({} {})",
        visible_count,
        i.t("条消息", "msgs"),
        total_count,
        i.t("总计", "total")
    );
    filter_row = filter_row.child(
        div()
            .text_size(px(11.5))
            .font_weight(gpui::FontWeight::MEDIUM)
            .text_color(t.text_secondary)
            .child(stats_label),
    );

    let filter_keys = [
        ("user", i.t("用户", "User"), filters.user, crate::icons::USER_SVG),
        ("assistant", i.t("助手", "Assistant"), filters.assistant, crate::icons::BOT_SVG),
        ("text", i.t("对话", "Text"), filters.text, crate::icons::FILE_TEXT_SVG),
        ("thinking", i.t("思考", "Thinking"), filters.thinking, crate::icons::SPARKLES_SVG),
        ("tool_call", i.t("工具", "Tool"), filters.tool_call, crate::icons::WAND_SVG),
        ("command", i.t("命令", "Bash"), filters.command, crate::icons::TERMINAL_SVG),
    ];

    let mut filter_chips = div().flex().items_center().gap(px(4.0)).flex_wrap();
    for (key, label, is_on, icon_svg) in filter_keys {
        let key_c = key;
        filter_chips = filter_chips.child(
            div()
                .id(gpui::SharedString::from(format!("chip-{}", key)))
                .cursor_pointer()
                .flex()
                .items_center()
                .gap(px(3.0))
                .px(px(6.0))
                .py(px(2.0))
                .rounded(px(4.0))
                .text_size(px(11.0))
                .border_1()
                .when(is_on, |s| {
                    s.bg(t.accent).border_color(t.accent).text_color(crate::rgba_const(0xffffffff))
                })
                .when(!is_on, |s| {
                    s.bg(t.input_bg).border_color(t.card_border).text_color(t.text_muted)
                        .hover(|h| h.bg(t.card_hover).text_color(t.text_primary))
                })
                .on_click(cx.listener(move |ws, _, _, cx| {
                    let f = &mut ws.settings.session_filters;
                    match key_c {
                        "user" => f.user = !f.user,
                        "assistant" => f.assistant = !f.assistant,
                        "text" => f.text = !f.text,
                        "thinking" => f.thinking = !f.thinking,
                        "tool_call" => f.tool_call = !f.tool_call,
                        "command" => f.command = !f.command,
                        _ => {}
                    }
                    (ws.callbacks.save_settings)(&ws.settings);
                    cx.notify();
                }))
                .child(crate::icons::svg_icon(
                    icon_svg,
                    px(11.0),
                    if is_on { crate::rgba_const(0xffffffff) } else { t.text_muted },
                ))
                .child(label),
        );
    }

    filter_row = filter_row.child(filter_chips);
    top_bar = top_bar.child(filter_row);
    section = section.child(top_bar);

    // ---------------------------------------------------------------------------
    // Virtualized Message Viewer with Draggable Scrollbar
    // ---------------------------------------------------------------------------
    let filter_key = (
        filters.user,
        filters.assistant,
        filters.text,
        filters.thinking,
        filters.tool_call,
        filters.command,
    );

    let list_state = match &ws.ui.session_list_state {
        Some((cached_tool, cached_sid, cached_filters, state))
            if *cached_tool == tool && cached_sid == &meta.session_id && *cached_filters == filter_key =>
        {
            if state.item_count() != visible_count {
                state.reset(visible_count);
            }
            state.clone()
        }
        _ => {
            let state = ListState::new(visible_count, ListAlignment::Top, px(400.0))
                .measure_all();
            ws.ui.session_list_state = Some((tool, meta.session_id.clone(), filter_key, state.clone()));
            state
        }
    };

    let mut msg_container = div()
        .id("session-detail-msg-viewport")
        .relative()
        .w_full()
        .flex_1()
        .min_h(px(0.0))
        .overflow_hidden()
        .rounded(px(8.0))
        .bg(t.sidebar_bg)
        .border_1()
        .border_color(t.card_border);

    if visible_count == 0 {
        msg_container = msg_container.child(
            div()
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .size_full()
                .child(
                    crate::components::empty_state_svg(
                        &t,
                        crate::icons::FILE_TEXT_SVG,
                        i.t("暂无匹配消息", "No matching messages"),
                        i.t("可尝试调整上方的角色或内容筛选条件", "Try adjusting the filter chips above"),
                    ),
                ),
        );
    } else {
        let ws_entity = cx.entity();
        let msgs_clone = messages.clone();
        let indices_clone = visible_indices.clone();
        let session_id_str = meta.session_id.clone();
        let list_state_for_items = list_state.clone();
        let t_for_items = t.clone();
        let i_for_items = i;

        let list_el = list(list_state.clone(), move |index, _window, cx| {
            let msg_idx = indices_clone[index];
            let m = &msgs_clone[msg_idx];
            render_session_message_card(
                m,
                index,
                msg_idx,
                &session_id_str,
                tool,
                &ws_entity,
                &list_state_for_items,
                &t_for_items,
                &i_for_items,
                cx,
            )
        })
        .size_full()
        .min_h(px(0.0));

        let scrollbar = Scrollbar::vertical(&list_state)
            .id("session-detail-scrollbar")
            .mode(ScrollbarMode::Always)
            .viewport_from_layout();

        msg_container = msg_container
            .child(list_el)
            .child(
                div()
                    .absolute()
                    .inset_0()
                    .child(scrollbar),
            );
    }

    section = section.child(msg_container);

    if is_actions_menu_open {
        let meta_c = meta.clone();
        let sid = meta.session_id.clone();
        let source_p = meta.source_path.clone();

        // 1. Transparent click-away backdrop to close the menu on clicking outside.
        let backdrop = div()
            .id("session-actions-backdrop")
            .occlude()
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .on_click(cx.listener(|ws, _, _, cx| {
                ws.ui.session_actions_menu_open = false;
                cx.notify();
            }));
        section = section.child(backdrop);

        // 2. Dropdown Menu positioned under the [操作] button, painted on top of msg_list
        let mut menu = div()
            .id("session-actions-menu")
            .occlude()
            .absolute()
            .top(px(40.0))
            .right(px(8.0))
            .w(px(160.0))
            .p(px(4.0))
            .rounded(px(6.0))
            .bg(t.card_bg)
            .border_1()
            .border_color(t.card_border)
            .shadow_lg()
            .flex()
            .flex_col()
            .gap(px(2.0));

        // 1. Rename
        let meta_for_rename = meta_c.clone();
        menu = menu.child(
            div()
                .id("menu-rename-btn")
                .cursor_pointer()
                .flex()
                .items_center()
                .gap(px(6.0))
                .px(px(8.0))
                .py(px(6.0))
                .rounded(px(4.0))
                .text_size(px(12.0))
                .text_color(t.text_primary)
                .hover(|h| h.bg(t.card_hover))
                .on_click(cx.listener(move |ws, _, window, cx| {
                    ws.ui.session_actions_menu_open = false;
                    let current = session::sidecar_title(&meta_for_rename)
                        .or_else(|| meta_for_rename.title.clone())
                        .unwrap_or_default();
                    let input = cx.new(|cx| {
                        let mut inp = TextInput::new("输入新标题…", cx);
                        inp.set_text_silent(current, cx);
                        inp.focus_handle.focus(window, cx);
                        inp.start_blink(cx);
                        inp
                    });
                    ws.ui.rename_dialog = Some((meta_for_rename.clone(), input));
                    cx.notify();
                }))
                .child(crate::icons::svg_icon(crate::icons::PENCIL_SVG, px(12.0), t.text_secondary))
                .child(i.t("重命名会话", "Rename Session")),
        );

        // 2. Export JSON
        let meta_for_json = meta_c.clone();
        menu = menu.child(
            div()
                .id("menu-export-json-btn")
                .cursor_pointer()
                .flex()
                .items_center()
                .gap(px(6.0))
                .px(px(8.0))
                .py(px(6.0))
                .rounded(px(4.0))
                .text_size(px(12.0))
                .text_color(t.text_primary)
                .hover(|h| h.bg(t.card_hover))
                .on_click(cx.listener(move |ws, _, _, cx| {
                    ws.ui.session_actions_menu_open = false;
                    match session::export_session(&ws.paths, &meta_for_json) {
                        Ok(json) => {
                            let out = ws.paths.app_data.join(format!("session-{}.json", meta_for_json.session_id));
                            match std::fs::write(&out, json) {
                                Ok(()) => {
                                    let msg = ws.i18n.t(
                                        &format!("已导出 JSON 到 {}", out.display()),
                                        &format!("exported JSON to {}", out.display()),
                                    ).to_string();
                                    ws.ui.toast(msg, false);
                                }
                                Err(e) => ws.ui.toast(format!("export failed: {e}"), true),
                            }
                        }
                        Err(e) => ws.ui.toast(format!("export failed: {e}"), true),
                    }
                    cx.notify();
                }))
                .child(crate::icons::svg_icon(crate::icons::DOWNLOAD_SVG, px(12.0), t.text_secondary))
                .child(i.t("导出 JSON", "Export JSON")),
        );

        // 3. Export Markdown
        let meta_for_md = meta_c.clone();
        menu = menu.child(
            div()
                .id("menu-export-md-btn")
                .cursor_pointer()
                .flex()
                .items_center()
                .gap(px(6.0))
                .px(px(8.0))
                .py(px(6.0))
                .rounded(px(4.0))
                .text_size(px(12.0))
                .text_color(t.text_primary)
                .hover(|h| h.bg(t.card_hover))
                .on_click(cx.listener(move |ws, _, _, cx| {
                    ws.ui.session_actions_menu_open = false;
                    match session::export_session_markdown(&ws.paths, &meta_for_md) {
                        Ok(md) => {
                            let out = ws.paths.app_data.join(format!("session-{}.md", meta_for_md.session_id));
                            match std::fs::write(&out, md) {
                                Ok(()) => {
                                    let msg = ws.i18n.t(
                                        &format!("已导出 Markdown 到 {}", out.display()),
                                        &format!("exported Markdown to {}", out.display()),
                                    ).to_string();
                                    ws.ui.toast(msg, false);
                                }
                                Err(e) => ws.ui.toast(format!("export failed: {e}"), true),
                            }
                        }
                        Err(e) => ws.ui.toast(format!("export failed: {e}"), true),
                    }
                    cx.notify();
                }))
                .child(crate::icons::svg_icon(crate::icons::FILE_TEXT_SVG, px(12.0), t.text_secondary))
                .child(i.t("导出 Markdown", "Export Markdown")),
        );

        // 4. Reveal in explorer
        let p_clone = source_p.clone();
        menu = menu.child(
            div()
                .id("menu-reveal-btn")
                .cursor_pointer()
                .flex()
                .items_center()
                .gap(px(6.0))
                .px(px(8.0))
                .py(px(6.0))
                .rounded(px(4.0))
                .text_size(px(12.0))
                .text_color(t.text_primary)
                .hover(|h| h.bg(t.card_hover))
                .on_click(cx.listener(move |ws, _, _, cx| {
                    ws.ui.session_actions_menu_open = false;
                    let path = std::path::PathBuf::from(&p_clone);
                    if path.exists() {
                        reveal_in_explorer(&path);
                    } else {
                        ws.ui.toast(ws.i18n.t("源文件路径不存在", "source file not found").to_string(), true);
                    }
                    cx.notify();
                }))
                .child(crate::icons::svg_icon(crate::icons::FOLDER_SVG, px(12.0), t.text_secondary))
                .child(i.t("定位文件", "Reveal in Explorer")),
        );

        // Separator
        menu = menu.child(
            div().h(px(1.0)).bg(t.card_border).my(px(2.0))
        );

        // 5. Delete session
        let sid_del = sid.clone();
        menu = menu.child(
            div()
                .id("menu-delete-btn")
                .cursor_pointer()
                .flex()
                .items_center()
                .gap(px(6.0))
                .px(px(8.0))
                .py(px(6.0))
                .rounded(px(4.0))
                .text_size(px(12.0))
                .text_color(t.danger)
                .hover(|h| h.bg(crate::rgba_const(0xef444415)))
                .on_click(cx.listener(move |ws, _, _, cx| {
                    ws.ui.session_actions_menu_open = false;
                    ws.ui.confirm = Some(super::ConfirmState {
                        title: ws.i18n.t("删除会话", "Delete Session").to_string(),
                        message: ws.i18n.t("确定要删除这条会话记录吗？此操作无法恢复。", "Delete this session? This action cannot be undone.").to_string(),
                        action: super::ConfirmAction::DeleteSession { tool, id: sid_del.clone() },
                    });
                    cx.notify();
                }))
                .child(crate::icons::svg_icon(crate::icons::TRASH_SVG, px(12.0), t.danger))
                .child(i.t("删除会话", "Delete Session")),
        );

        section = section.child(menu);
    }

    section.into_any_element()
}

fn inline_action_btn(
    id: impl Into<gpui::ElementId>,
    icon: &'static [u8],
    label: impl Into<gpui::SharedString>,
    t: &crate::theme::Theme,
    on_click: impl Fn(&gpui::ClickEvent, &mut gpui::Window, &mut gpui::App) + 'static,
) -> gpui::AnyElement {
    let bg = t.card_hover;
    let text = t.text_primary;
    div()
        .id(id)
        .cursor_pointer()
        .flex()
        .items_center()
        .gap(px(3.0))
        .px(px(6.0))
        .py(px(2.0))
        .rounded(px(4.0))
        .text_size(px(11.0))
        .text_color(t.text_muted)
        .hover(move |h| h.bg(bg).text_color(text))
        .on_click(on_click)
        .child(crate::icons::svg_icon(icon, px(11.0), t.text_muted))
        .child(label.into())
        .into_any_element()
}

fn render_session_message_card(
    m: &SessionMessage,
    item_index: usize,
    raw_msg_index: usize,
    session_id: &str,
    tool: ToolId,
    ws_entity: &gpui::Entity<Workspace>,
    list_state: &ListState,
    t: &crate::theme::Theme,
    i: &crate::i18n::I18n,
    cx: &mut gpui::App,
) -> gpui::AnyElement {
    let is_user = m.role == "user";
    let role_display = if is_user {
        i.t("用户", "User")
    } else {
        i.t(tool.name_zh(), tool.name_en())
    };

    let mut msg_card = div()
        .id(gpui::SharedString::from(format!("msg-{}-{}", session_id, item_index)))
        .flex()
        .flex_col()
        .gap(px(4.0))
        .w_full();

    let content_copy = m.content.clone();

    if is_user {
        // User Message: Aligned to Right (Chat Bubble)
        msg_card = msg_card.items_end();

        let ws_entity_c = ws_entity.clone();
        let hover_bg = t.card_hover;
        let meta_row = div()
            .flex()
            .items_center()
            .justify_end()
            .gap(px(6.0))
            .px(px(4.0))
            .children(m.ts.map(|ts| {
                div()
                    .text_size(px(11.0))
                    .text_color(t.text_muted)
                    .child(fmt_time(Some(ts)))
            }))
            .child(
                div()
                    .text_size(px(11.5))
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .text_color(t.text_secondary)
                    .child(role_display),
            )
            .child(
                div()
                    .id(gpui::SharedString::from(format!("user-copy-{}-{}", session_id, item_index)))
                    .cursor_pointer()
                    .p(px(2.0))
                    .rounded(px(4.0))
                    .hover(move |h| h.bg(hover_bg))
                    .on_click(move |_ev, _win, cx| {
                        let text = content_copy.clone();
                        let _ = ws_entity_c.update(cx, |ws, cx| {
                            cx.write_to_clipboard(gpui::ClipboardItem::new_string(text));
                            ws.ui.toast(
                                ws.i18n.t("已复制消息内容", "Copied message content").to_string(),
                                false,
                            );
                            cx.notify();
                        });
                    })
                    .child(crate::icons::svg_icon(crate::icons::COPY_SVG, px(11.0), t.text_muted)),
            );

        let user_bubble = div()
            .max_w(px(760.0))
            .p(px(10.0))
            .px(px(14.0))
            .rounded(px(16.0))
            .rounded_br(px(4.0))
            .bg(t.accent)
            .text_color(crate::rgba_const(0xffffffff))
            .text_size(px(13.5))
            .line_height(px(20.0))
            .shadow_xs()
            .child(m.content.clone());

        msg_card = msg_card.child(meta_row).child(user_bubble);
    } else {
        // Assistant Message: Aligned to Left (Streamlined Chat Item)
        msg_card = msg_card.items_start();

        let ws_entity_c = ws_entity.clone();
        let hover_bg = t.card_hover;
        let meta_row = div()
            .flex()
            .items_center()
            .justify_start()
            .gap(px(6.0))
            .px(px(4.0))
            .child(crate::icons::svg_icon(crate::icons::BOT_SVG, px(12.0), t.accent))
            .child(
                div()
                    .text_size(px(12.0))
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .text_color(t.text_primary)
                    .child(role_display),
            )
            .children(m.model.as_ref().map(|mod_name| badge(t, mod_name.clone(), BadgeKind::Neutral)))
            .children(m.ts.map(|ts| {
                div()
                    .text_size(px(11.0))
                    .text_color(t.text_muted)
                    .child(fmt_time(Some(ts)))
            }))
            .child(
                div()
                    .id(gpui::SharedString::from(format!("agent-copy-{}-{}", session_id, item_index)))
                    .cursor_pointer()
                    .p(px(2.0))
                    .rounded(px(4.0))
                    .hover(move |h| h.bg(hover_bg))
                    .on_click(move |_ev, _win, cx| {
                        let text = content_copy.clone();
                        let _ = ws_entity_c.update(cx, |ws, cx| {
                            cx.write_to_clipboard(gpui::ClipboardItem::new_string(text));
                            ws.ui.toast(
                                ws.i18n.t("已复制消息内容", "Copied message content").to_string(),
                                false,
                            );
                            cx.notify();
                        });
                    })
                    .child(crate::icons::svg_icon(crate::icons::COPY_SVG, px(11.0), t.text_muted)),
            );

        let mut content_shell = div()
            .flex()
            .flex_col()
            .w_full()
            .max_w(px(980.0))
            .gap(px(6.0));

        if !m.blocks.is_empty() {
            for (b_idx, b) in m.blocks.iter().enumerate() {
                let block_key = format!("{}-{}-{}", session_id, raw_msg_index, b_idx);
                content_shell = content_shell.child(render_message_block(
                    b,
                    &block_key,
                    item_index,
                    ws_entity,
                    list_state,
                    t,
                    i,
                    cx,
                ));
            }
        } else if !m.content.is_empty() {
            content_shell = content_shell.child(
                div()
                    .p(px(12.0))
                    .px(px(16.0))
                    .rounded(px(16.0))
                    .rounded_bl(px(4.0))
                    .bg(t.card_bg)
                    .border_1()
                    .border_color(t.card_border)
                    .text_size(px(13.5))
                    .line_height(px(21.0))
                    .text_color(t.text_primary)
                    .child(m.content.clone()),
            );
        }

        msg_card = msg_card.child(meta_row).child(content_shell);
    }

    div()
        .w_full()
        .min_w(px(0.0))
        .pl(px(14.0))
        .pr(px(24.0))
        .py(px(6.0))
        .child(msg_card)
        .into_any_element()
}

/// Render an individual message block: thinking, command execution, tool call, or text.
fn render_message_block(
    block: &SessionMessageBlock,
    key: &str,
    item_idx: usize,
    ws_entity: &gpui::Entity<Workspace>,
    list_state: &ListState,
    t: &crate::theme::Theme,
    i: &crate::i18n::I18n,
    cx: &mut gpui::App,
) -> gpui::AnyElement {
    let ws = ws_entity.read(cx);

    match block.kind.as_str() {
        // -----------------------------------------------------------------------
        // 1. Thinking / Reasoning block
        // -----------------------------------------------------------------------
        "thinking" => {
            let th_key = format!("th-{}", key);
            let is_expanded = ws.ui.session_expanded_thinkings.contains(&th_key);
            let text = block.text.clone().unwrap_or_default();
            let char_count = text.chars().count();

            let th_key_c = th_key.clone();
            let ws_entity_c = ws_entity.clone();
            let list_state_c = list_state.clone();

            let mut card = div()
                .flex()
                .flex_col()
                .p(px(8.0))
                .rounded(px(6.0))
                .bg(t.sidebar_bg)
                .border_1()
                .border_color(crate::rgba_const(0x8b5cf644));

            let header = div()
                .id(gpui::SharedString::from(format!("th-hdr-{}", th_key_c)))
                .flex()
                .items_center()
                .justify_between()
                .cursor_pointer()
                .on_click(move |_ev, _win, cx| {
                    let _ = ws_entity_c.update(cx, |ws, cx| {
                        if ws.ui.session_expanded_thinkings.contains(&th_key_c) {
                            ws.ui.session_expanded_thinkings.remove(&th_key_c);
                        } else {
                            ws.ui.session_expanded_thinkings.insert(th_key_c.clone());
                        }
                        cx.notify();
                    });
                    list_state_c.remeasure_items(item_idx..item_idx + 1);
                })
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(6.0))
                        .child(crate::icons::svg_icon(
                            crate::icons::SPARKLES_SVG,
                            px(13.0),
                            crate::rgba_const(0xa855f7ff),
                        ))
                        .child(
                            div()
                                .text_size(px(12.0))
                                .font_weight(gpui::FontWeight::SEMIBOLD)
                                .text_color(crate::rgba_const(0xa855f7ff))
                                .child(i.t("🧠 思考过程 (Thinking)", "🧠 Thinking Process")),
                        )
                        .child(badge(t, format!("{char_count} {}", i.t("字", "chars")), BadgeKind::Neutral)),
                )
                .child(
                    div()
                        .text_size(px(11.5))
                        .text_color(t.text_muted)
                        .child(if is_expanded {
                            i.t("收起 ▲", "Collapse ▲")
                        } else {
                            i.t("展开 ▼", "Expand ▼")
                        }),
                );

            card = card.child(header);

            if is_expanded {
                card = card.child(
                    div()
                        .mt(px(6.0))
                        .pt(px(6.0))
                        .border_t_1()
                        .border_color(crate::rgba_const(0x8b5cf622))
                        .text_size(px(12.0))
                        .line_height(px(18.0))
                        .text_color(t.text_secondary)
                        .child(text),
                );
            }

            card.into_any_element()
        }

        // -----------------------------------------------------------------------
        // 2. Command / Bash Execution block (Collapsed by default)
        // -----------------------------------------------------------------------
        "command" => {
            let cmd_text = block.command.clone().or_else(|| block.text.clone()).unwrap_or_default();
            let title = block.title.clone();
            let output = block.output.clone();
            let is_error = block.is_error.unwrap_or(false);

            let cmd_key = format!("cmd-{}", key);
            let is_expanded = ws.ui.session_expanded_blocks.contains(&cmd_key);
            let cmd_key_c = cmd_key.clone();
            let cmd_copy = cmd_text.clone();

            if !is_expanded {
                // Collapsed single-line view
                let preview = if let Some(ref desc) = title {
                    desc.clone()
                } else {
                    let first_line = cmd_text.lines().next().unwrap_or("").trim();
                    format!("$ {}", first_line)
                };
                let preview_short: String = preview.chars().take(55).collect();
                let has_more = preview.chars().count() > 55;

                let ws_entity_c = ws_entity.clone();
                let list_state_c = list_state.clone();
                let hover_bg = t.card_hover;

                div()
                    .id(gpui::SharedString::from(format!("cmd-collapsed-{}", key)))
                    .cursor_pointer()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(6.0))
                    .px(px(8.0))
                    .py(px(4.0))
                    .rounded(px(6.0))
                    .bg(t.sidebar_bg)
                    .border_1()
                    .border_color(if is_error {
                        crate::rgba_const(0xef444466)
                    } else {
                        crate::rgba_const(0x3b82f644)
                    })
                    .hover(move |h| h.bg(hover_bg))
                    .on_click(move |_ev, _win, cx| {
                        let _ = ws_entity_c.update(cx, |ws, cx| {
                            ws.ui.session_expanded_blocks.insert(cmd_key_c.clone());
                            cx.notify();
                        });
                        list_state_c.remeasure_items(item_idx..item_idx + 1);
                    })
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(6.0))
                            .min_w(px(0.0))
                            .flex_1()
                            .overflow_hidden()
                            .child(crate::icons::svg_icon(
                                crate::icons::TERMINAL_SVG,
                                px(12.0),
                                if is_error { t.danger } else { t.accent },
                            ))
                            .child(
                                div()
                                    .text_size(px(11.5))
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .text_color(if is_error { t.danger } else { t.accent })
                                    .child("Bash"),
                            )
                            .child(if is_error {
                                badge(t, i.t("失败", "Failed"), BadgeKind::Danger)
                            } else {
                                badge(t, i.t("已执行", "Done"), BadgeKind::Success)
                            })
                            .child(
                                div()
                                    .text_size(px(11.5))
                                    .text_color(t.text_muted)
                                    .overflow_hidden()
                                    .text_ellipsis()
                                    .whitespace_nowrap()
                                    .child(if has_more { format!("{}…", preview_short) } else { preview_short }),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(4.0))
                            .flex_shrink_0()
                            .child(inline_action_btn(
                                gpui::SharedString::from(format!("cmd-cp-{}", key)),
                                crate::icons::COPY_SVG,
                                i.t("复制", "Copy"),
                                t,
                                {
                                    let ws_entity_c = ws_entity.clone();
                                    let copy_val = cmd_copy.clone();
                                    let toast_msg = i.t("已复制命令", "Copied command").to_string();
                                    move |_ev, _win, cx| {
                                        let _ = ws_entity_c.update(cx, |ws, cx| {
                                            cx.write_to_clipboard(gpui::ClipboardItem::new_string(copy_val.clone()));
                                            ws.ui.toast(toast_msg.clone(), false);
                                            cx.notify();
                                        });
                                    }
                                },
                            ))
                            .child(
                                div()
                                    .text_size(px(11.0))
                                    .text_color(t.accent)
                                    .child(i.t("展开 ▼", "Expand ▼")),
                            ),
                    )
                    .into_any_element()
            } else {
                // Expanded full view
                let out_key = format!("out-{}", key);
                let is_out_expanded = ws.ui.session_expanded_outputs.contains(&out_key);

                let mut card = div()
                    .flex()
                    .flex_col()
                    .gap(px(6.0))
                    .p(px(8.0))
                    .rounded(px(6.0))
                    .bg(t.sidebar_bg)
                    .border_1()
                    .border_color(if is_error {
                        crate::rgba_const(0xef444466)
                    } else {
                        crate::rgba_const(0x3b82f666)
                    });

                let cmd_key_toggle = cmd_key.clone();
                let ws_entity_c = ws_entity.clone();
                let list_state_c = list_state.clone();

                let header = div()
                    .id(gpui::SharedString::from(format!("cmd-hdr-{}", key)))
                    .cursor_pointer()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(6.0))
                    .on_click(move |_ev, _win, cx| {
                        let _ = ws_entity_c.update(cx, |ws, cx| {
                            ws.ui.session_expanded_blocks.remove(&cmd_key_toggle);
                            cx.notify();
                        });
                        list_state_c.remeasure_items(item_idx..item_idx + 1);
                    })
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(6.0))
                            .child(crate::icons::svg_icon(
                                crate::icons::TERMINAL_SVG,
                                px(13.0),
                                if is_error { t.danger } else { t.accent },
                            ))
                            .child(
                                div()
                                    .text_size(px(12.0))
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .text_color(if is_error { t.danger } else { t.accent })
                                    .child(i.t(">_ 终端命令 (Bash)", ">_ Bash Command")),
                            )
                            .child(if is_error {
                                badge(t, i.t("执行失败", "Failed"), BadgeKind::Danger)
                            } else {
                                badge(t, i.t("已执行", "Completed"), BadgeKind::Success)
                            }),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(4.0))
                            .child(inline_action_btn(
                                gpui::SharedString::from(format!("cmd-copy-{}", key)),
                                crate::icons::COPY_SVG,
                                i.t("复制命令", "Copy Cmd"),
                                t,
                                {
                                    let ws_entity_c = ws_entity.clone();
                                    let copy_val = cmd_copy.clone();
                                    let toast_msg = i.t("已复制命令", "Copied command").to_string();
                                    move |_ev, _win, cx| {
                                        let _ = ws_entity_c.update(cx, |ws, cx| {
                                            cx.write_to_clipboard(gpui::ClipboardItem::new_string(copy_val.clone()));
                                            ws.ui.toast(toast_msg.clone(), false);
                                            cx.notify();
                                        });
                                    }
                                },
                            ))
                            .child(
                                div()
                                    .text_size(px(11.0))
                                    .text_color(t.accent)
                                    .child(i.t("收起 ▲", "Collapse ▲")),
                            ),
                    );

                card = card.child(header);

                if let Some(desc) = title {
                    card = card.child(
                        div()
                            .text_size(px(11.5))
                            .text_color(t.text_muted)
                            .child(format!("{}: {}", i.t("说明", "Desc"), desc)),
                    );
                }

                // Command Box (Monospace Dark Terminal style)
                card = card.child(
                    div()
                        .p(px(8.0))
                        .rounded(px(4.0))
                        .bg(t.input_bg)
                        .border_1()
                        .border_color(t.card_border)
                        .text_size(px(12.0))
                        .line_height(px(18.0))
                        .font_weight(gpui::FontWeight::MEDIUM)
                        .text_color(t.text_primary)
                        .child(format!("$ {cmd_text}")),
                );

                // Output Box (if present)
                if let Some(out_str) = output {
                    let out_copy = out_str.clone();
                    let out_lines: Vec<&str> = out_str.lines().collect();
                    let total_lines = out_lines.len();
                    let needs_fold = total_lines > 12;

                    let out_key_c = out_key.clone();
                    let ws_entity_c = ws_entity.clone();
                    let list_state_c = list_state.clone();

                    let out_header = div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .mt(px(4.0))
                        .child(
                            div()
                                .text_size(px(11.5))
                                .font_weight(gpui::FontWeight::SEMIBOLD)
                                .text_color(t.text_secondary)
                                .child(format!("{}: ({} {})", i.t("输出", "Output"), total_lines, i.t("行", "lines"))),
                        )
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(6.0))
                                .when(needs_fold, |s| {
                                    s.child(
                                        div()
                                            .id(gpui::SharedString::from(format!("out-toggle-{}", out_key_c)))
                                            .cursor_pointer()
                                            .text_size(px(11.0))
                                            .text_color(t.accent)
                                            .on_click(move |_ev, _win, cx| {
                                                let _ = ws_entity_c.update(cx, |ws, cx| {
                                                    if ws.ui.session_expanded_outputs.contains(&out_key_c) {
                                                        ws.ui.session_expanded_outputs.remove(&out_key_c);
                                                    } else {
                                                        ws.ui.session_expanded_outputs.insert(out_key_c.clone());
                                                    }
                                                    cx.notify();
                                                });
                                                list_state_c.remeasure_items(item_idx..item_idx + 1);
                                            })
                                            .child(if is_out_expanded {
                                                i.t("收起 ▲", "Collapse ▲")
                                            } else {
                                                i.t("展开全部 ▼", "Expand All ▼")
                                            }),
                                    )
                                })
                                .child(inline_action_btn(
                                    gpui::SharedString::from(format!("out-copy-{}", key)),
                                    crate::icons::COPY_SVG,
                                    i.t("复制输出", "Copy Output"),
                                    t,
                                    {
                                        let ws_entity_c = ws_entity.clone();
                                        let copy_val = out_copy.clone();
                                        let toast_msg = i.t("已复制命令输出", "Copied output").to_string();
                                        move |_ev, _win, cx| {
                                            let _ = ws_entity_c.update(cx, |ws, cx| {
                                                cx.write_to_clipboard(gpui::ClipboardItem::new_string(copy_val.clone()));
                                                ws.ui.toast(toast_msg.clone(), false);
                                                cx.notify();
                                            });
                                        }
                                    },
                                )),
                        );

                    let display_out = if needs_fold && !is_out_expanded {
                        format!("{}\n… ({} {})", out_lines.iter().take(10).cloned().collect::<Vec<_>>().join("\n"), total_lines - 10, i.t("行已折叠", "lines collapsed"))
                    } else {
                        out_str
                    };

                    let out_box = div()
                        .p(px(8.0))
                        .rounded(px(4.0))
                        .bg(t.card_bg)
                        .border_1()
                        .border_color(t.card_border)
                        .text_size(px(11.5))
                        .line_height(px(16.0))
                        .text_color(t.text_secondary)
                        .child(display_out);

                    card = card.child(out_header).child(out_box);
                }

                card.into_any_element()
            }
        }

        // -----------------------------------------------------------------------
        // 3. Tool Call block (Read, Edit, Write, Grep, etc. - Collapsed by default)
        // -----------------------------------------------------------------------
        "tool_call" => {
            let tool_name = block.tool_name.clone().unwrap_or_else(|| "tool".into());
            let title = block.title.clone();
            let text = block.text.clone().unwrap_or_default();
            let output = block.output.clone();
            let copy_txt = text.clone();

            let tool_key = format!("tool-{}", key);
            let is_expanded = ws.ui.session_expanded_blocks.contains(&tool_key);
            let tool_key_c = tool_key.clone();

            if !is_expanded {
                // Collapsed single-line view
                let preview = if let Some(ref d) = title {
                    d.clone()
                } else if !text.is_empty() {
                    text.lines().next().unwrap_or("").trim().to_string()
                } else {
                    tool_name.clone()
                };
                let preview_short: String = preview.chars().take(55).collect();
                let has_more = preview.chars().count() > 55;

                let ws_entity_c = ws_entity.clone();
                let list_state_c = list_state.clone();
                let hover_bg = t.card_hover;

                div()
                    .id(gpui::SharedString::from(format!("tool-collapsed-{}", key)))
                    .cursor_pointer()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(6.0))
                    .px(px(8.0))
                    .py(px(4.0))
                    .rounded(px(6.0))
                    .bg(t.sidebar_bg)
                    .border_1()
                    .border_color(crate::rgba_const(0x06b6d444))
                    .hover(move |h| h.bg(hover_bg))
                    .on_click(move |_ev, _win, cx| {
                        let _ = ws_entity_c.update(cx, |ws, cx| {
                            ws.ui.session_expanded_blocks.insert(tool_key_c.clone());
                            cx.notify();
                        });
                        list_state_c.remeasure_items(item_idx..item_idx + 1);
                    })
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(6.0))
                            .min_w(px(0.0))
                            .flex_1()
                            .overflow_hidden()
                            .child(crate::icons::svg_icon(
                                crate::icons::WAND_SVG,
                                px(12.0),
                                crate::rgba_const(0x06b6d4ff),
                            ))
                            .child(
                                div()
                                    .text_size(px(11.5))
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .text_color(crate::rgba_const(0x06b6d4ff))
                                    .child(tool_name.clone()),
                            )
                            .child(
                                div()
                                    .text_size(px(11.5))
                                    .text_color(t.text_muted)
                                    .overflow_hidden()
                                    .text_ellipsis()
                                    .whitespace_nowrap()
                                    .child(if has_more { format!("{}…", preview_short) } else { preview_short }),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(4.0))
                            .flex_shrink_0()
                            .child(inline_action_btn(
                                gpui::SharedString::from(format!("tool-cp-{}", key)),
                                crate::icons::COPY_SVG,
                                i.t("复制", "Copy"),
                                t,
                                {
                                    let ws_entity_c = ws_entity.clone();
                                    let copy_val = copy_txt.clone();
                                    let toast_msg = i.t("已复制工具参数", "Copied params").to_string();
                                    move |_ev, _win, cx| {
                                        let _ = ws_entity_c.update(cx, |ws, cx| {
                                            cx.write_to_clipboard(gpui::ClipboardItem::new_string(copy_val.clone()));
                                            ws.ui.toast(toast_msg.clone(), false);
                                            cx.notify();
                                        });
                                    }
                                },
                            ))
                            .child(
                                div()
                                    .text_size(px(11.0))
                                    .text_color(crate::rgba_const(0x06b6d4ff))
                                    .child(i.t("展开 ▼", "Expand ▼")),
                            ),
                    )
                    .into_any_element()
            } else {
                // Expanded full view
                let tool_key_toggle = tool_key.clone();
                let ws_entity_c = ws_entity.clone();
                let list_state_c = list_state.clone();

                let mut card = div()
                    .flex()
                    .flex_col()
                    .gap(px(6.0))
                    .p(px(8.0))
                    .rounded(px(6.0))
                    .bg(t.sidebar_bg)
                    .border_1()
                    .border_color(crate::rgba_const(0x06b6d444));

                let header = div()
                    .id(gpui::SharedString::from(format!("tool-hdr-{}", key)))
                    .cursor_pointer()
                    .flex()
                    .items_center()
                    .justify_between()
                    .on_click(move |_ev, _win, cx| {
                        let _ = ws_entity_c.update(cx, |ws, cx| {
                            ws.ui.session_expanded_blocks.remove(&tool_key_toggle);
                            cx.notify();
                        });
                        list_state_c.remeasure_items(item_idx..item_idx + 1);
                    })
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(6.0))
                            .child(crate::icons::svg_icon(
                                crate::icons::WAND_SVG,
                                px(13.0),
                                crate::rgba_const(0x06b6d4ff),
                            ))
                            .child(
                                div()
                                    .text_size(px(12.0))
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .text_color(crate::rgba_const(0x06b6d4ff))
                                    .child(format!("🔧 {} ({})", i.t("工具调用", "Tool"), tool_name)),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(4.0))
                            .child(inline_action_btn(
                                gpui::SharedString::from(format!("tool-copy-{}", key)),
                                crate::icons::COPY_SVG,
                                i.t("复制参数", "Copy"),
                                t,
                                {
                                    let ws_entity_c = ws_entity.clone();
                                    let copy_val = copy_txt.clone();
                                    let toast_msg = i.t("已复制工具参数", "Copied params").to_string();
                                    move |_ev, _win, cx| {
                                        let _ = ws_entity_c.update(cx, |ws, cx| {
                                            cx.write_to_clipboard(gpui::ClipboardItem::new_string(copy_val.clone()));
                                            ws.ui.toast(toast_msg.clone(), false);
                                            cx.notify();
                                        });
                                    }
                                },
                            ))
                            .child(
                                div()
                                    .text_size(px(11.0))
                                    .text_color(crate::rgba_const(0x06b6d4ff))
                                    .child(i.t("收起 ▲", "Collapse ▲")),
                            ),
                    );

                card = card.child(header);

                if let Some(t_desc) = title {
                    card = card.child(
                        div()
                            .text_size(px(11.5))
                            .text_color(t.text_muted)
                            .child(t_desc),
                    );
                }

                if !text.is_empty() {
                    card = card.child(
                        div()
                            .p(px(6.0))
                            .rounded(px(4.0))
                            .bg(t.input_bg)
                            .border_1()
                            .border_color(t.card_border)
                            .text_size(px(11.5))
                            .line_height(px(16.0))
                            .text_color(t.text_secondary)
                            .child(text),
                    );
                }

                if let Some(out_str) = output {
                    card = card.child(
                        div()
                            .p(px(6.0))
                            .rounded(px(4.0))
                            .bg(t.card_bg)
                            .border_1()
                            .border_color(t.card_border)
                            .text_size(px(11.0))
                            .line_height(px(16.0))
                            .text_color(t.text_secondary)
                            .child(out_str),
                    );
                }

                card.into_any_element()
            }
        }

        // -----------------------------------------------------------------------
        // 4. Default Text block
        // -----------------------------------------------------------------------
        _ => {
            let text = block.text.clone().unwrap_or_default();
            div()
                .text_size(px(13.0))
                .line_height(px(20.0))
                .text_color(t.text_primary)
                .child(text)
                .into_any_element()
        }
    }
}
