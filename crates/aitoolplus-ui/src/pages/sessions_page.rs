//! Sessions page: per-tool list with cache, view, rename, export, delete.

use aitoolplus_core::session::{self, SessionMeta};
use aitoolplus_core::tools::ToolId;
use gpui::{Context, IntoElement, div, prelude::*, px};
use serde_json::Value;

use crate::components::{BadgeKind, ButtonVariant, badge, button_l, empty_state, page_header};
use crate::text_input::TextInput;
use crate::workspace::Workspace;

pub fn render_sessions_page(ws: &mut Workspace, cx: &mut Context<Workspace>) -> gpui::AnyElement {
    let t = ws.theme.clone();
    let i = ws.i18n;
    let tool = ws.ui.sessions_tool;

    // tool selector
    let mut tools_row = div()
        .flex()
        .w_full()
        .min_w(px(0.0))
        .gap(px(6.0))
        .flex_wrap();
    for candidate in ToolId::ALL {
        let is_on = candidate == tool;
        let label = i.t(candidate.name_zh(), candidate.name_en());
        let key = candidate;
        tools_row = tools_row.child(
            div()
                .id(gpui::SharedString::from(format!(
                    "sess-tool-{}",
                    candidate.key()
                )))
                .cursor_pointer()
                .flex()
                .items_center()
                .h(px(26.0))
                .px(px(10.0))
                .rounded(px(6.0))
                .text_size(px(12.5))
                .when(is_on, |s| {
                    s.bg(t.accent_subtle)
                        .border_1()
                        .border_color(t.accent)
                        .text_color(t.accent)
                })
                .when(!is_on, |s| {
                    s.bg(t.input_bg)
                        .border_1()
                        .border_color(t.input_border)
                        .text_color(t.text_secondary)
                })
                .on_click(cx.listener(move |ws, _ev: &gpui::ClickEvent, _w, cx| {
                    ws.ui.sessions_tool = key;
                    ws.ui.open_session = None;
                    session::invalidate_cache();
                    cx.notify();
                }))
                .child(label),
        );
    }

    let filter = ws.settings.session_filters.clone();
    let mut filter_row = div().flex().flex_wrap().gap(px(6.0));
    for (key, label, enabled) in [
        ("user", i.t("用户", "User"), filter.user),
        ("assistant", i.t("助手", "Assistant"), filter.assistant),
        ("text", i.t("文本", "Text"), filter.text),
        ("thinking", i.t("思考", "Thinking"), filter.thinking),
        ("tool_call", i.t("工具调用", "Tool Calls"), filter.tool_call),
        ("command", i.t("命令", "Commands"), filter.command),
    ] {
        filter_row = filter_row.child(button_l(
            gpui::SharedString::from(format!("session-filter-{key}")),
            label,
            if enabled {
                ButtonVariant::Primary
            } else {
                ButtonVariant::Secondary
            },
            &t,
            cx,
            move |ws, _, _, cx| {
                let filters = &mut ws.settings.session_filters;
                match key {
                    "user" => filters.user = !filters.user,
                    "assistant" => filters.assistant = !filters.assistant,
                    "text" => filters.text = !filters.text,
                    "thinking" => filters.thinking = !filters.thinking,
                    "tool_call" => filters.tool_call = !filters.tool_call,
                    "command" => filters.command = !filters.command,
                    _ => {}
                }
                (ws.callbacks.save_settings)(&ws.settings);
                cx.notify();
            },
        ));
    }

    let search = ws.ui.session_search.clone();
    let query = search.read(cx).text().to_lowercase();

    let sessions = session::cached_scan(&ws.paths, tool, session::DEFAULT_SESSION_PATH_LIMIT);
    let filtered: Vec<SessionMeta> = sessions
        .into_iter()
        .filter(|s| {
            query.is_empty()
                || s.session_id.to_lowercase().contains(&query)
                || s.title
                    .as_deref()
                    .is_some_and(|x| x.to_lowercase().contains(&query))
                || s.summary
                    .as_deref()
                    .is_some_and(|x| x.to_lowercase().contains(&query))
        })
        .collect();

    let mut section = div()
        .flex()
        .flex_col()
        .gap(px(12.0))
        .child(page_header(
            &t,
            i.t("会话管理", "Sessions"),
            i.t(
                "浏览、重命名、导出与删除各 CLI 的会话历史（15 秒缓存）",
                "Browse, rename, export and delete CLI sessions (15s cache)",
            ),
        ))
        .child(tools_row)
        .child(filter_row)
        .child(
            div()
                .w(px(320.0))
                .p(px(8.0))
                .rounded(px(8.0))
                .bg(t.input_bg)
                .border_1()
                .border_color(t.input_border)
                .child(search),
        );

    if filtered.is_empty() {
        section = section.child(empty_state(
            &t,
            "🗂",
            i.t("没有找到会话", "No sessions found"),
            i.t(
                "该工具的会话目录可能为空或不存在",
                "This tool's session dir may be empty or missing",
            ),
        ));
    } else {
        let mut list = div().flex().flex_col().gap(px(6.0));
        for s in &filtered {
            list = list.child(session_row(s, tool, ws, cx));
            // expanded message view
            if ws.ui.open_session == Some((tool, s.session_id.clone())) {
                list = list.child(expanded_messages(s, ws, cx));
            }
        }
        section = section.child(list);
    }

    section.into_any_element()
}

fn fmt_time(ms: Option<i64>) -> String {
    ms.and_then(|m| {
        chrono::DateTime::from_timestamp_millis(m).map(|dt| dt.format("%Y-%m-%d %H:%M").to_string())
    })
    .unwrap_or_else(|| "—".into())
}

fn session_row(
    s: &SessionMeta,
    tool: ToolId,
    ws: &mut Workspace,
    cx: &mut Context<Workspace>,
) -> gpui::AnyElement {
    let meta = s.clone();
    let sid = s.session_id.clone();
    let source_path = s.source_path.clone();
    let t = ws.theme.clone();
    let i = ws.i18n;
    let is_open = ws.ui.open_session == Some((tool, s.session_id.clone()));

    // merged title: sidecar rename wins, then title, then summary, then id
    let display_title = session::sidecar_title(s)
        .or_else(|| s.title.clone())
        .or_else(|| s.summary.clone())
        .unwrap_or_else(|| s.session_id.clone());

    let row = div()
        .id(gpui::SharedString::from(format!("sess-{}", s.session_id)))
        .flex()
        .flex_col()
        .w_full()
        .min_w(px(0.0))
        .items_start()
        .gap(px(8.0))
        .p(px(12.0))
        .rounded(px(8.0))
        .bg(t.card_bg)
        .border_1()
        .border_color(if is_open { t.accent } else { t.card_border })
        .hover(|h| h.bg(t.card_hover))
        .child(badge(&t, fmt_time(s.last_active_at), BadgeKind::Accent))
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(2.0))
                .flex_1()
                .min_w(px(0.0))
                .child(
                    div()
                        .text_size(px(13.0))
                        .font_weight(gpui::FontWeight::MEDIUM)
                        .text_color(t.text_primary)
                        .child(display_title),
                )
                .children(s.project_dir.clone().map(|d| {
                    div()
                        .text_size(px(11.0))
                        .text_color(t.text_muted)
                        .child(d)
                        .into_any_element()
                })),
        )
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(4.0))
                .child(button_l(
                    gpui::SharedString::from(format!("sess-view-{}", sid)),
                    if is_open {
                        i.t("收起", "Collapse")
                    } else {
                        i.t("查看", "View")
                    },
                    ButtonVariant::Secondary,
                    &t,
                    cx,
                    {
                        let sid = sid.clone();
                        let open_target = if is_open {
                            None
                        } else {
                            Some((tool, sid.clone()))
                        };
                        move |ws, _, _, cx| {
                            ws.ui.open_session = open_target.clone();
                            cx.notify();
                        }
                    },
                ))
                .child(button_l(
                    gpui::SharedString::from(format!("sess-rename-{}", sid)),
                    i.t("重命名", "Rename"),
                    ButtonVariant::Secondary,
                    &t,
                    cx,
                    {
                        let meta_for_rename = meta.clone();
                        move |ws, _, _, cx| {
                            let current = session::sidecar_title(&meta_for_rename)
                                .or_else(|| meta_for_rename.title.clone())
                                .unwrap_or_default();
                            let input = cx.new(|cx| {
                                let mut inp = TextInput::new(i18n_placeholder(), cx);
                                inp.set_text_silent(current, cx);
                                inp
                            });
                            ws.ui.rename_dialog = Some((meta_for_rename.clone(), input));
                            cx.notify();
                        }
                    },
                ))
                .child(button_l(
                    gpui::SharedString::from(format!("sess-export-{}", sid)),
                    i.t("导出", "Export"),
                    ButtonVariant::Secondary,
                    &t,
                    cx,
                    {
                        let meta_for_export = meta.clone();
                        move |ws, _, _, cx| {
                            match session::export_session(&ws.paths, &meta_for_export) {
                                Ok(json) => {
                                    let out = ws.paths.app_data.join(format!(
                                        "session-{}.json",
                                        meta_for_export.session_id
                                    ));
                                    match std::fs::write(&out, json) {
                                        Ok(()) => {
                                            let msg = ws
                                                .i18n
                                                .t(
                                                    &format!("已导出到 {}", out.display()),
                                                    &format!("exported to {}", out.display()),
                                                )
                                                .to_string();
                                            ws.ui.toast(msg, false);
                                        }
                                        Err(e) => {
                                            let msg = format!("export failed: {e}");
                                            ws.ui.toast(msg, true);
                                        }
                                    }
                                }
                                Err(e) => {
                                    let msg = format!("export failed: {e}");
                                    ws.ui.toast(msg, true);
                                }
                            }
                            cx.notify();
                        }
                    },
                ))
                .child(button_l(
                    gpui::SharedString::from(format!("sess-del-{}", sid)),
                    i.t("删除", "Delete"),
                    ButtonVariant::Danger,
                    &t,
                    cx,
                    {
                        let id = sid.clone();
                        let path = source_path.clone();
                        move |ws, _, _, cx| {
                            ws.ui.confirm = Some(super::ConfirmState {
                                title: ws.i18n.t("删除会话", "Delete Session").to_string(),
                                message: ws
                                    .i18n
                                    .t(
                                        &format!("删除 {path}？此操作不可恢复。"),
                                        &format!("Delete {path}? This cannot be undone."),
                                    )
                                    .to_string(),
                                action: super::ConfirmAction::DeleteSession {
                                    tool,
                                    id: id.clone(),
                                },
                            });
                            cx.notify();
                        }
                    },
                )),
        );
    row.into_any_element()
}

fn expanded_messages(
    s: &SessionMeta,
    ws: &mut Workspace,
    _cx: &mut Context<Workspace>,
) -> gpui::AnyElement {
    let t = ws.theme.clone();
    let messages = session::load_messages(&ws.paths, s).unwrap_or_default();
    let mut msgs = div()
        .flex()
        .flex_col()
        .gap(px(4.0))
        .pl(px(40.0))
        .pt(px(2.0))
        .pb(px(6.0));
    let filters = &ws.settings.session_filters;
    for m in messages
        .iter()
        .filter(|message| {
            (message.role != "user" || filters.user)
                && (message.role != "assistant" || filters.assistant)
                && (message.message_type.as_deref() != Some("thinking") || filters.thinking)
                && (message.message_type.as_deref() != Some("tool_call") || filters.tool_call)
                && (message.message_type.as_deref() != Some("command") || filters.command)
                && (!message.blocks.is_empty() || filters.text)
        })
        .take(200)
    {
        let is_user = m.role == "user";
        msgs = msgs.child(
            div()
                .flex()
                .flex_col()
                .gap(px(1.0))
                .p(px(8.0))
                .rounded(px(6.0))
                .bg(if is_user { t.accent_subtle } else { t.input_bg })
                .child(
                    div()
                        .text_size(px(10.5))
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .text_color(if is_user { t.accent } else { t.text_secondary })
                        .child(m.role.clone()),
                )
                .child(
                    div()
                        .text_size(px(12.0))
                        .text_color(t.text_primary)
                        .child(m.content.chars().take(600).collect::<String>()),
                ),
        );
    }
    msgs.into_any_element()
}

fn i18n_placeholder() -> &'static str {
    "标题…"
}

/// Rename dialog body (rendered by pages::render_modals).
pub fn render_rename_dialog(
    meta: SessionMeta,
    input: gpui::Entity<TextInput>,
    ws: &mut Workspace,
    cx: &mut Context<Workspace>,
) -> gpui::AnyElement {
    let t = ws.theme.clone();
    let i = ws.i18n;
    let tool = ws.ui.sessions_tool;
    let _ = Value::Null; // keep serde_json imported for other helpers

    let body = div()
        .flex()
        .flex_col()
        .gap(px(12.0))
        .child(input.clone())
        .child(
            div()
                .flex()
                .justify_end()
                .gap(px(8.0))
                .child(button_l(
                    "rename-cancel",
                    i.t("取消", "Cancel"),
                    ButtonVariant::Secondary,
                    &t,
                    cx,
                    |ws, _, _, cx| {
                        ws.ui.rename_dialog = None;
                        cx.notify();
                    },
                ))
                .child(button_l(
                    "rename-save",
                    i.t("保存", "Save"),
                    ButtonVariant::Primary,
                    &t,
                    cx,
                    move |ws, _, _, cx| {
                        let title: String = input.update(cx, |inp, _| inp.text().to_string());
                        match session::rename_session(&meta, &title) {
                            Ok(()) => {
                                session::invalidate_cache();
                                let msg = ws.i18n.t("已重命名", "renamed").to_string();
                                ws.ui.toast(msg, false);
                            }
                            Err(e) => {
                                let msg = format!("rename failed: {e}");
                                ws.ui.toast(msg, true);
                            }
                        }
                        let _ = tool;
                        ws.ui.rename_dialog = None;
                        cx.notify();
                    },
                )),
        );

    super::modal_scaffold(
        &t,
        i.t("重命名会话", "Rename Session").as_ref(),
        body.into_any_element(),
        cx,
        |ws, _, _, cx| {
            ws.ui.rename_dialog = None;
            cx.notify();
        },
    )
}
