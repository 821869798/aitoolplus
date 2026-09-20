//! Sessions page: per-tool list with cache, view, rename, export, delete.

use aitoolplus_core::session::{self, SessionMeta};
use aitoolplus_core::tools::ToolId;
use gpui::{Context, IntoElement, div, prelude::*, px};
use serde_json::Value;

use crate::components::{
    ButtonVariant, button_l, button_with_icon_l, input_container,
};
use crate::text_input::TextInput;
use crate::workspace::Workspace;

pub fn render_sessions_page(ws: &mut Workspace, cx: &mut Context<Workspace>) -> gpui::AnyElement {
    let t = ws.theme.clone();
    let i = ws.i18n;
    let tool = ws.ui.sessions_tool;

    // If a session is open, render the full, rich session detail view
    if let Some((open_tool, ref open_sid)) = ws.ui.open_session {
        let sessions = session::cached_scan(&ws.paths, open_tool, session::DEFAULT_SESSION_PATH_LIMIT);
        if let Some(meta) = sessions.iter().find(|s| &s.session_id == open_sid) {
            return super::session_detail::render_session_detail(open_tool, meta, ws, cx);
        }
    }

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
        tools_row = tools_row.child(button_l(
            gpui::SharedString::from(format!("sess-tool-{}", candidate.key())),
            label,
            if is_on {
                ButtonVariant::Primary
            } else {
                ButtonVariant::Secondary
            },
            &t,
            cx,
            move |ws, _ev, _w, cx| {
                ws.ui.sessions_tool = key;
                ws.ui.open_session = None;
                session::invalidate_cache();
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
                || s.project_dir
                    .as_deref()
                    .is_some_and(|x| x.to_lowercase().contains(&query))
        })
        .collect();

    let mut section = div()
        .flex()
        .flex_col()
        .w_full()
        .min_w(px(0.0))
        .gap(px(12.0))
        .child(tools_row);

    let toolbar = div()
        .flex()
        .items_center()
        .justify_between()
        .gap(px(10.0))
        .child(
            div()
                .w(px(320.0))
                .child(input_container(&t, search)),
        )
        .child(
            div()
                .text_size(px(12.0))
                .text_color(t.text_muted)
                .child(format!(
                    "{} {} {}",
                    i.t("共", "Total"),
                    filtered.len(),
                    i.t("个会话", "sessions")
                )),
        );

    section = section.child(toolbar);

    if filtered.is_empty() {
        section = section.child(crate::components::empty_state_svg(
            &t,
            crate::icons::FOLDER_SVG,
            i.t("没有找到会话", "No sessions found"),
            i.t(
                "该工具的会话目录可能为空或不存在",
                "This tool's session dir may be empty or missing",
            ),
        ));
    } else {
        let mut list = div().flex().flex_col().gap(px(12.0));
        for s in &filtered {
            list = list.child(session_row(s, tool, ws, cx));
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

fn short_session_id(sid: &str) -> String {
    if sid.len() <= 12 {
        sid.to_string()
    } else {
        let prefix: String = sid.chars().take(8).collect();
        let suffix: String = sid.chars().rev().take(4).collect::<Vec<_>>().into_iter().rev().collect();
        format!("{prefix}...{suffix}")
    }
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

    let display_time = fmt_time(s.last_active_at.or(s.created_at));
    let short_hash = short_session_id(&sid);

    let sid_for_click = sid.clone();

    let row = div()
        .id(gpui::SharedString::from(format!("sess-{}", s.session_id)))
        .flex()
        .items_center()
        .justify_between()
        .w_full()
        .min_w(px(0.0))
        .gap(px(12.0))
        .px(px(16.0))
        .py(px(12.0))
        .rounded(px(12.0))
        .bg(t.card_bg)
        .border_1()
        .border_color(if is_open { t.accent } else { t.card_border })
        .shadow_xs()
        .cursor_pointer()
        .hover(move |h| {
            h.bg(t.card_hover).border_color(if is_open {
                t.accent
            } else {
                t.card_border_hover
            })
        })
        .on_click(cx.listener(move |ws, _, _, cx| {
            ws.ui.open_session = Some((tool, sid_for_click.clone()));
            cx.notify();
        }))
        // Left main info: compact 2 rows
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(6.0))
                .flex_1()
                .min_w(px(0.0))
                .overflow_hidden()
                // Row 1: Title
                .child(
                    div()
                        .text_size(px(14.0))
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .text_color(t.text_primary)
                        .overflow_hidden()
                        .text_ellipsis()
                        .whitespace_nowrap()
                        .child(display_title),
                )
                // Row 2: Meta (Time, Hash, Directory)
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(12.0))
                        .text_size(px(12.0))
                        .text_color(t.text_secondary)
                        .overflow_hidden()
                        .whitespace_nowrap()
                        // Time with Clock icon
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(4.0))
                                .flex_shrink_0()
                                .child(crate::icons::svg_icon(crate::icons::CLOCK_SVG, px(12.0), t.text_muted))
                                .child(display_time),
                        )
                        // Hash (short session id)
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(2.0))
                                .flex_shrink_0()
                                .text_color(t.text_muted)
                                .child(short_hash),
                        )
                        // Project Directory (if available)
                        .children(s.project_dir.as_ref().map(|dir| {
                            div()
                                .flex()
                                .items_center()
                                .gap(px(4.0))
                                .min_w(px(0.0))
                                .overflow_hidden()
                                .text_ellipsis()
                                .text_color(t.text_muted)
                                .child(crate::icons::svg_icon(crate::icons::FOLDER_SVG, px(12.0), t.text_muted))
                                .child(
                                    div()
                                        .overflow_hidden()
                                        .text_ellipsis()
                                        .child(dir.clone()),
                                )
                                .into_any_element()
                        })),
                ),
        )
        // Right actions: 恢复命令, 重命名, 删除
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(8.0))
                .flex_shrink_0()
                .child(button_with_icon_l(
                    gpui::SharedString::from(format!("sess-resume-{}", sid)),
                    crate::icons::TERMINAL_SVG,
                    i.t("恢复命令", "Resume"),
                    ButtonVariant::Secondary,
                    &t,
                    cx,
                    {
                        let cmd_opt = meta.resume_command.clone();
                        move |ws, _, _, cx| {
                            cx.stop_propagation();
                            if let Some(cmd) = cmd_opt.as_ref() {
                                cx.write_to_clipboard(gpui::ClipboardItem::new_string(cmd.clone()));
                                ws.ui.toast(ws.i18n.t("已复制恢复命令", "Copied resume command").to_string(), false);
                            } else {
                                ws.ui.toast(ws.i18n.t("该会话暂不支持恢复命令", "Resume not supported for this session").to_string(), true);
                            }
                            cx.notify();
                        }
                    },
                ))
                .child(button_with_icon_l(
                    gpui::SharedString::from(format!("sess-rename-{}", sid)),
                    crate::icons::PENCIL_SVG,
                    i.t("重命名", "Rename"),
                    ButtonVariant::Secondary,
                    &t,
                    cx,
                    {
                        let meta_for_rename = meta.clone();
                        move |ws, _, window, cx| {
                            cx.stop_propagation();
                            let current = session::sidecar_title(&meta_for_rename)
                                .or_else(|| meta_for_rename.title.clone())
                                .unwrap_or_default();
                            let input = cx.new(|cx| {
                                let mut inp = TextInput::new(i18n_placeholder(), cx);
                                inp.set_text_silent(current, cx);
                                inp.focus_handle.focus(window, cx);
                                inp.start_blink(cx);
                                inp
                            });
                            ws.ui.rename_dialog = Some((meta_for_rename.clone(), input));
                            cx.notify();
                        }
                    },
                ))
                .child(button_with_icon_l(
                    gpui::SharedString::from(format!("sess-del-{}", sid)),
                    crate::icons::TRASH_SVG,
                    i.t("删除", "Delete"),
                    ButtonVariant::Danger,
                    &t,
                    cx,
                    {
                        let id = sid.clone();
                        let path = source_path.clone();
                        move |ws, _, _, cx| {
                            cx.stop_propagation();
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
        .child(input_container(&t, input.clone()))
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
