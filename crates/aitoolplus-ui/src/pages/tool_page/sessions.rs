use aitoolplus_core::pi_extensions::{PiExtensionKind, PiExtensionScope};
use aitoolplus_core::pi_pages::PiModelSettings;
use aitoolplus_core::providers::{CATEGORIES, ProviderRecord};
use aitoolplus_core::session::{self, SessionMeta};
use aitoolplus_core::tools::ToolId;
use gpui::{Context, IntoElement, ScrollStrategy, deferred, div, prelude::*, px, uniform_list};
use gpui_kit::base::{Align, ElementExt as _, Placement, Positioner, POPUP_PRIORITY};
use gpui_kit::component::scroll::{Scrollbar, ScrollbarMode};
use serde_json::Value;

use crate::components::{
    self, ButtonVariant, Tooltip, button_l, button_with_icon_l,
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
    ws.ui.agent_session_scan = ws.ui.agent_session_scan.wrapping_add(1);
    let generation = ws.ui.agent_session_scan;
    ws.ui.agent_sessions = Some((tool, Vec::new()));
    ws.ui.agent_session_scroll.scroll_to_item_strict(0, ScrollStrategy::Top);
    let paths = ws.paths.clone();
    let weak = cx.entity().downgrade();
    cx.spawn(async move |_this, cx| {
        if let Some(hit) = session::cached_all_if_fresh(tool) {
            let _ = weak.update(cx, |workspace, cx| {
                if workspace.ui.agent_session_scan != generation {
                    return;
                }
                workspace.ui.agent_sessions = Some((tool, hit));
                workspace.ui.agent_sessions_loading = false;
                cx.notify();
            });
            return;
        }
        let mut scan = cx
            .background_spawn(async move { session::SessionScan::open(&paths, tool) })
            .await;
        loop {
            let (next, batch) = cx
                .background_spawn(async move {
                    let batch = scan.next_batch(200);
                    (scan, batch)
                })
                .await;
            scan = next;
            let Some(batch) = batch else {
                break;
            };
            if batch.is_empty() {
                continue;
            }
            let keep = weak
                .update(cx, |workspace, cx| {
                    if workspace.ui.agent_session_scan != generation {
                        return false;
                    }
                    if let Some((loaded, list)) = &mut workspace.ui.agent_sessions {
                        if *loaded == tool {
                            list.extend(batch);
                        }
                    }
                    cx.notify();
                    true
                })
                .unwrap_or(false);
            if !keep {
                return;
            }
        }
        let _ = weak.update(cx, |workspace, cx| {
            if workspace.ui.agent_session_scan != generation {
                return;
            }
            if let Some((loaded, list)) = &mut workspace.ui.agent_sessions {
                if *loaded == tool {
                    list.sort_by_key(|item| std::cmp::Reverse(item.last_active_at));
                    session::store_full_list_cache(tool, list.clone());
                }
            }
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
        let items = std::sync::Arc::new(filtered);
        section = section.child(session_list_viewport(
            "agent-sessions-virtual-list",
            "agent-sessions-scrollbar",
            items,
            tool,
            ws.ui.agent_session_scroll.clone(),
            cx.entity(),
            &t,
            &i,
        ));
    }

    section.into_any_element()
}

pub(crate) fn session_list_viewport(
    list_id: &'static str,
    bar_id: &'static str,
    items: std::sync::Arc<Vec<SessionMeta>>,
    tool: ToolId,
    scroll: gpui::UniformListScrollHandle,
    ws_entity: gpui::Entity<Workspace>,
    t: &Theme,
    i: &I18n,
) -> gpui::AnyElement {
    let items_len = items.len();
    let items_for_list = items.clone();
    let t_clone = t.clone();
    let i_clone = i.clone();
    let v_list = uniform_list(
        list_id,
        items_len,
        move |range: std::ops::Range<usize>, _window: &mut gpui::Window, _cx: &mut gpui::App| {
            let mut elements = Vec::with_capacity(range.len());
            for idx in range {
                if let Some(item) = items_for_list.get(idx) {
                    elements.push(render_virtual_agent_session_card(
                        item,
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
    .size_full()
    .track_scroll(&scroll);
    let scrollbar = Scrollbar::vertical(&scroll)
        .id(bar_id)
        .mode(ScrollbarMode::Always)
        .viewport_from_layout();
    div()
        .relative()
        .w_full()
        .flex_1()
        .h_full()
        .min_h(px(0.0))
        .overflow_hidden()
        .rounded(px(8.0))
        .bg(t.sidebar_bg)
        .border_1()
        .border_color(t.card_border)
        .child(
            div()
                .size_full()
                .pt(px(8.0))
                .pb(px(8.0))
                .pl(px(8.0))
                .pr(px(20.0))
                .child(v_list),
        )
        .child(div().absolute().inset_0().child(scrollbar))
        .into_any_element()
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

pub(crate) fn render_virtual_agent_session_card(
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
    let meta_open = meta.clone();

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
            let opened = meta_open.clone();
            let _ = ws_entity_click.update(cx, |ws, cx| {
                if opened.provider_id.starts_with("antigravity:") {
                    ws.ui.antigravity_open_session = Some(antigravity_meta(&opened));
                } else {
                    ws.ui.open_session = Some((tool, sid_c));
                }
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
        .child(session_title_row(s, &display_title, t))
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

    if !s.provider_id.starts_with("antigravity:") {
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
                    let deleted = meta.clone();
                    let _ = ws_entity.update(cx, |ws, cx| {
                        if deleted.provider_id.starts_with("antigravity:") {
                            let session = antigravity_meta(&deleted);
                            let msg = format!(
                                "确定要删除此会话记录 ({}) 吗？磁盘上的相关数据将被永久移除。",
                                session.session_id
                            );
                            ws.ui.confirm = Some(crate::pages::ConfirmState {
                                title: ws.i18n.t("删除 Antigravity 会话", "Delete Antigravity Session").to_string(),
                                message: msg,
                                action: crate::pages::ConfirmAction::DeleteAntigravitySession { session },
                            });
                        } else {
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
                        }
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

fn session_title_row(s: &SessionMeta, title: &str, t: &Theme) -> gpui::Div {
    let mut row = div()
        .flex()
        .items_center()
        .gap(px(8.0))
        .min_w(px(0.0))
        .overflow_hidden();
    if let Some(source) = s.provider_id.strip_prefix("antigravity:") {
        let (label, bg, border, color) = if source == "cli" {
            ("CLI", t.accent.opacity(0.12), t.accent.opacity(0.3), t.accent)
        } else {
            ("App", t.sidebar_bg, t.card_border, t.text_secondary)
        };
        row = row.child(
            div()
                .flex()
                .items_center()
                .justify_center()
                .h(px(18.0))
                .px(px(6.0))
                .rounded(px(4.0))
                .bg(bg)
                .border_1()
                .border_color(border)
                .text_size(px(11.0))
                .font_weight(gpui::FontWeight::MEDIUM)
                .text_color(color)
                .flex_shrink_0()
                .child(label),
        );
    }
    row.child(
        div()
            .text_size(px(14.0))
            .font_weight(gpui::FontWeight::SEMIBOLD)
            .text_color(t.text_primary)
            .overflow_hidden()
            .text_ellipsis()
            .whitespace_nowrap()
            .child(title.to_string()),
    )
}

fn antigravity_meta(s: &SessionMeta) -> aitoolplus_core::antigravity::AntigravitySessionMeta {
    aitoolplus_core::antigravity::AntigravitySessionMeta {
        session_id: s.session_id.clone(),
        source: s
            .provider_id
            .strip_prefix("antigravity:")
            .unwrap_or("app")
            .to_string(),
        title: s.title.clone().unwrap_or_default(),
        preview: s.summary.clone().unwrap_or_default(),
        project_dir: s.project_dir.clone(),
        last_active_at: s.last_active_at,
        step_count: 0,
        source_path: s.source_path.clone(),
        resume_command: s.resume_command.clone(),
    }
}

pub(crate) fn render_rename_dialog(
    meta: SessionMeta,
    input: gpui::Entity<TextInput>,
    ws: &mut Workspace,
    cx: &mut Context<Workspace>,
) -> gpui::AnyElement {
    let t = ws.theme.clone();
    let i = ws.i18n;
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
                                ws.ui.toast(ws.i18n.t("已重命名", "renamed").to_string(), false);
                            }
                            Err(e) => ws.ui.toast(format!("rename failed: {e}"), true),
                        }
                        ws.ui.rename_dialog = None;
                        cx.notify();
                    },
                )),
        );
    crate::pages::modal_scaffold(
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

