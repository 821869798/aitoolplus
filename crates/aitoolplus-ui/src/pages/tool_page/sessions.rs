use aitoolplus_core::pi_extensions::{PiExtensionKind, PiExtensionScope};
use aitoolplus_core::pi_pages::PiModelSettings;
use aitoolplus_core::providers::{CATEGORIES, ProviderRecord};
use aitoolplus_core::session::{self, SessionMeta};
use aitoolplus_core::tools::ToolId;
use gpui::{Context, IntoElement, deferred, div, prelude::*, px, uniform_list};
use gpui_kit::base::{Align, ElementExt as _, Placement, Positioner, POPUP_PRIORITY};
use serde_json::Value;

use crate::components::{
    self, BadgeKind, ButtonVariant, Tooltip, badge, button_l, button_with_icon_l,
    button_with_icon_loading_l, input_container, page_header, section_title, text_area_scroll_container, textarea_container,
};
use crate::i18n::I18n;
use crate::text_area::TextArea;
use crate::text_input::TextInput;
use crate::theme::Theme;
use crate::workspace::Workspace;

use crate::pages::{PromptDialogState, ProviderDialogState, ToolTab, modal_scaffold_custom, modal_scaffold_sized};

use super::common::{open_in_browser, plugin_tag, spawn_tool_action};

pub fn load_agent_sessions(tool: ToolId, ws: &mut Workspace, cx: &mut Context<Workspace>) {
    if ws.ui.agent_sessions_loading {
        return;
    }
    ws.ui.agent_sessions_loading = true;
    let paths = ws.paths.clone();
    let weak = cx.entity().downgrade();
    cx.spawn(async move |_this, cx| {
        let result = cx
            .background_spawn(async move {
                session::cached_scan(&paths, tool, session::DEFAULT_SESSION_PATH_LIMIT)
            })
            .await;
        let _ = weak.update(cx, |workspace, cx| {
            workspace.ui.agent_sessions = Some((tool, result));
            workspace.ui.agent_sessions_loading = false;
            cx.notify();
        });
    })
    .detach();
}

pub(super) fn agent_sessions_section(
    tool: ToolId,
    ws: &mut Workspace,
    cx: &mut Context<Workspace>,
) -> gpui::AnyElement {
    let t = ws.theme.clone();
    let i = ws.i18n;

    let need_load = match &ws.ui.agent_sessions {
        Some((loaded_tool, _)) => *loaded_tool != tool,
        None => true,
    };
    if need_load && !ws.ui.agent_sessions_loading {
        load_agent_sessions(tool, ws, cx);
    }

    if let Some((open_tool, ref open_sid)) = ws.ui.open_session {
        if open_tool == tool {
            let maybe_meta = ws
                .ui
                .agent_sessions
                .as_ref()
                .and_then(|(_, list)| list.iter().find(|s| &s.session_id == open_sid).cloned());
            if let Some(meta) = maybe_meta {
                return render_agent_session_detail(tool, &meta, ws, cx);
            }
        }
    }

    let query = ws.ui.agent_session_search.read(cx).text().trim().to_lowercase();
    let empty_vec = vec![];
    let sessions = ws
        .ui
        .agent_sessions
        .as_ref()
        .filter(|(t, _)| *t == tool)
        .map(|(_, list)| list)
        .unwrap_or(&empty_vec);

    let filtered: Vec<SessionMeta> = sessions
        .iter()
        .filter(|s| {
            if query.is_empty() {
                return true;
            }
            s.session_id.to_lowercase().contains(&query)
                || s.title.as_deref().unwrap_or_default().to_lowercase().contains(&query)
                || s.summary.as_deref().unwrap_or_default().to_lowercase().contains(&query)
                || s.project_dir.as_deref().unwrap_or_default().to_lowercase().contains(&query)
        })
        .cloned()
        .collect();

    let mut section = div().flex().flex_col().w_full().h_full().min_h(px(0.0)).gap(px(10.0));

    // Toolbar
    let mut toolbar = div().flex().items_center().gap(px(10.0)).w_full();
    toolbar = toolbar.child(
        div()
            .flex_1()
            .min_w(px(0.0))
            .child(input_container(&t, ws.ui.agent_session_search.clone())),
    );
    if ws.ui.agent_sessions_loading {
        toolbar = toolbar.child(badge(&t, i.t("正在扫描会话…", "Scanning sessions…"), BadgeKind::Neutral));
    }
    toolbar = toolbar.child(
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
    toolbar = toolbar.child(button_with_icon_loading_l(
        "agent-sessions-refresh-btn",
        crate::icons::REFRESH_SVG,
        i.t("刷新", "Refresh"),
        ButtonVariant::Secondary,
        ws.ui.agent_sessions_loading,
        &t,
        cx,
        move |ws, _, _, cx| {
            session::invalidate_cache();
            ws.ui.agent_sessions = None;
            load_agent_sessions(tool, ws, cx);
            cx.notify();
        },
    ));

    section = section.child(toolbar);

    if ws.ui.agent_sessions_loading && sessions.is_empty() {
        section = section.child(
            crate::components::empty_state_svg(
                &t,
                crate::icons::REFRESH_SVG,
                i.t("正在加载会话列表…", "Loading sessions…"),
                i.t(
                    "后台正在快速扫描会话历史文件，请稍候",
                    "Scanning session history files in background, please wait",
                ),
            ),
        );
    } else if filtered.is_empty() {
        section = section.child(
            crate::components::empty_state_svg(
                &t,
                crate::icons::FOLDER_SVG,
                i.t("没有找到会话", "No sessions found"),
                i.t(
                    "该 Agent 的会话目录可能为空或没有匹配的搜索结果",
                    "This agent's session directory may be empty or no results matched",
                ),
            ),
        );
    } else {
        let items: std::sync::Arc<Vec<SessionMeta>> = std::sync::Arc::new(filtered);
        let items_len = items.len();
        let items_for_list = items.clone();
        let ws_entity = cx.entity();
        let t_clone = t.clone();
        let i_clone = i.clone();

        let v_list = uniform_list(
            "agent-sessions-virtual-list",
            items_len,
            move |range: std::ops::Range<usize>, _window: &mut gpui::Window, _cx: &mut gpui::App| -> Vec<gpui::AnyElement> {
                let mut elements = Vec::with_capacity(range.len());
                for idx in range {
                    if let Some(s) = items_for_list.get(idx) {
                        elements.push(render_virtual_agent_session_card(
                            s,
                            tool,
                            &ws_entity,
                            &t_clone,
                            &i_clone,
                        ));
                    }
                }
                elements
            },
        )
        .size_full();

        section = section.child(
            div()
                .w_full()
                .flex_1()
                .h_full()
                .min_h(px(0.0))
                .overflow_hidden()
                .rounded(px(8.0))
                .bg(t.sidebar_bg)
                .border_1()
                .border_color(t.card_border)
                .p(px(8.0))
                .child(v_list),
        );
    }

    section.into_any_element()
}

pub(super) fn fmt_time(ms: Option<i64>) -> String {
    ms.and_then(|m| {
        chrono::DateTime::from_timestamp_millis(m).map(|dt| dt.format("%Y-%m-%d %H:%M").to_string())
    })
    .unwrap_or_else(|| "—".into())
}

pub(super) fn short_session_id_tool(sid: &str) -> String {
    if sid.len() <= 12 {
        sid.to_string()
    } else {
        let prefix: String = sid.chars().take(8).collect();
        let suffix: String = sid.chars().rev().take(4).collect::<Vec<_>>().into_iter().rev().collect();
        format!("{prefix}...{suffix}")
    }
}

pub(super) fn render_virtual_agent_session_card(
    s: &SessionMeta,
    tool: ToolId,
    ws_entity: &gpui::Entity<Workspace>,
    t: &Theme,
    i: &I18n,
) -> gpui::AnyElement {
    let sid = s.session_id.clone();
    let meta = s.clone();
    let display_title = session::sidecar_title(s)
        .or_else(|| s.title.clone())
        .or_else(|| s.summary.clone())
        .unwrap_or_else(|| s.session_id.clone());

    let display_time = fmt_time(s.last_active_at.or(s.created_at));
    let short_hash = short_session_id_tool(&sid);

    let sid_click = sid.clone();
    let ws_entity_click = ws_entity.clone();

    let card = div()
        .id(gpui::SharedString::from(format!("v-sess-{}", s.session_id)))
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
        .border_color(t.card_border)
        .shadow_xs()
        .cursor_pointer()
        .hover({
            let bg = t.card_hover;
            let border = t.card_border_hover;
            move |h| h.bg(bg).border_color(border)
        })
        .on_click(move |_ev, _win, cx| {
            let sid_c = sid_click.clone();
            let _ = ws_entity_click.update(cx, |ws, cx| {
                ws.ui.open_session = Some((tool, sid_c));
                cx.notify();
            });
        });

    let left = div()
        .id(gpui::SharedString::from(format!("v-sess-left-{}", s.session_id)))
        .flex()
        .flex_col()
        .gap(px(6.0))
        .min_w(px(0.0))
        .flex_1()
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
        );

    let mut actions = div()
        .flex()
        .items_center()
        .gap(px(8.0))
        .flex_shrink_0();

    // [恢复命令]
    {
        let ws_entity = ws_entity.clone();
        let cmd_opt = meta.resume_command.clone();
        actions = actions.child(
            div()
                .id(format!("sess-resume-{}", meta.session_id))
                .cursor_pointer()
                .flex()
                .items_center()
                .gap(px(4.0))
                .px(px(8.0))
                .py(px(4.0))
                .rounded(px(4.0))
                .bg(t.card_hover)
                .border_1()
                .border_color(t.card_border)
                .text_size(px(11.5))
                .text_color(t.text_secondary)
                .hover({
                    let bg = t.row_hover;
                    let fg = t.text_primary;
                    move |h| h.bg(bg).text_color(fg)
                })
                .on_click(move |_ev, _win, cx| {
                    cx.stop_propagation();
                    let cmd_opt = cmd_opt.clone();
                    let _ = ws_entity.update(cx, |ws, cx| {
                        if let Some(cmd) = cmd_opt.as_ref() {
                            cx.write_to_clipboard(gpui::ClipboardItem::new_string(cmd.clone()));
                            ws.ui.toast(ws.i18n.t("已复制恢复命令", "Copied resume command").to_string(), false);
                        } else {
                            ws.ui.toast(ws.i18n.t("该会话暂不支持恢复命令", "Resume not supported for this session").to_string(), true);
                        }
                        cx.notify();
                    });
                })
                .child(crate::icons::svg_icon(crate::icons::TERMINAL_SVG, px(11.5), t.text_secondary))
                .child(i.t("恢复命令", "Resume")),
        );
    }

    // [重命名]
    {
        let ws_entity = ws_entity.clone();
        let meta_for_rename = meta.clone();
        actions = actions.child(
            div()
                .id(format!("sess-rename-{}", meta_for_rename.session_id))
                .cursor_pointer()
                .flex()
                .items_center()
                .gap(px(4.0))
                .px(px(8.0))
                .py(px(4.0))
                .rounded(px(4.0))
                .bg(t.card_hover)
                .border_1()
                .border_color(t.card_border)
                .text_size(px(11.5))
                .text_color(t.text_secondary)
                .hover({
                    let bg = t.row_hover;
                    let fg = t.text_primary;
                    move |h| h.bg(bg).text_color(fg)
                })
                .on_click(move |_ev, window, cx| {
                    cx.stop_propagation();
                    let current = session::sidecar_title(&meta_for_rename)
                        .or_else(|| meta_for_rename.title.clone())
                        .unwrap_or_default();
                    let meta_c = meta_for_rename.clone();
                    let _ = ws_entity.update(cx, |ws, cx| {
                        let input = cx.new(|cx| {
                            let mut inp = TextInput::new("输入新标题…", cx);
                            inp.set_text_silent(current, cx);
                            inp.focus_handle.focus(window, cx);
                            inp.start_blink(cx);
                            inp
                        });
                        ws.ui.rename_dialog = Some((meta_c, input));
                        cx.notify();
                    });
                })
                .child(crate::icons::svg_icon(crate::icons::PENCIL_SVG, px(11.5), t.text_secondary))
                .child(i.t("重命名", "Rename")),
        );
    }

    // [删除]
    {
        let ws_entity = ws_entity.clone();
        let sid_del = sid.clone();
        actions = actions.child(
            div()
                .id(format!("sess-del-{}", sid_del))
                .cursor_pointer()
                .flex()
                .items_center()
                .gap(px(4.0))
                .px(px(8.0))
                .py(px(4.0))
                .rounded(px(4.0))
                .bg(t.danger_subtle)
                .text_size(px(11.5))
                .text_color(t.danger)
                .hover(|h| h.bg(gpui::rgba(0xef444425)))
                .on_click(move |_ev, _win, cx| {
                    cx.stop_propagation();
                    let s_id = sid_del.clone();
                    let _ = ws_entity.update(cx, |ws, cx| {
                        ws.ui.confirm = Some(crate::pages::ConfirmState {
                            title: ws.i18n.t("删除会话", "Delete Session").to_string(),
                            message: ws
                                .i18n
                                .t("确定要删除这条会话记录吗？此操作无法恢复。", "Delete this session? This action cannot be undone.")
                                .to_string(),
                            action: crate::pages::ConfirmAction::DeleteSession {
                                tool,
                                id: s_id,
                            },
                        });
                        cx.notify();
                    });
                })
                .child(crate::icons::svg_icon(crate::icons::TRASH_SVG, px(11.5), t.danger))
                .child(i.t("删除", "Delete")),
        );
    }

    div()
        .w_full()
        .pb(px(12.0))
        .child(card.child(left).child(actions))
        .into_any_element()
}

pub(super) fn render_agent_session_detail(
    tool: ToolId,
    meta: &SessionMeta,
    ws: &mut Workspace,
    cx: &mut Context<Workspace>,
) -> gpui::AnyElement {
    crate::pages::session_detail::render_session_detail(tool, meta, ws, cx)
}


// ---------------------------------------------------------------------------
// Claude Code Plugins tab
// ---------------------------------------------------------------------------

