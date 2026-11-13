use std::collections::HashMap;
use std::sync::Arc;

use aitoolplus_core::antigravity::{
    AntigravitySessionMeta, delete_antigravity_sessions_batch, filter_stale_antigravity_sessions,
};
use aitoolplus_core::session::{self, SessionMeta};
use aitoolplus_core::session_search::{self, ContentHit};
use aitoolplus_core::tools::ToolId;
use gpui::{Context, IntoElement, ScrollStrategy, div, prelude::*, px, uniform_list};
use gpui_kit::component::scroll::{Scrollbar, ScrollbarMode};

use crate::components::{
    ButtonVariant, button_l, button_loading_l, button_with_icon_l, button_with_icon_loading_l,
    input_container, segmented_pill_selector,
};
use crate::i18n::I18n;
use crate::pages::{SessionCleanupDialogState, SessionCleanupPeriod, SessionCleanupTarget};
use crate::text_input::TextInput;
use crate::theme::Theme;
use crate::workspace::Workspace;



pub(crate) fn session_matches_metadata(s: &SessionMeta, query_lower: &str) -> bool {
    s.session_id.to_lowercase().contains(query_lower)
        || s.title
            .as_deref()
            .unwrap_or_default()
            .to_lowercase()
            .contains(query_lower)
        || s.summary
            .as_deref()
            .unwrap_or_default()
            .to_lowercase()
            .contains(query_lower)
        || s.project_dir
            .as_deref()
            .unwrap_or_default()
            .to_lowercase()
            .contains(query_lower)
}

pub fn schedule_agent_session_search(
    ws: &mut Workspace,
    raw_query: String,
    immediate: bool,
    cx: &mut Context<Workspace>,
) {
    let query = raw_query.trim().to_string();
    ws.ui.agent_session_search_seq = ws.ui.agent_session_search_seq.wrapping_add(1);
    let seq = ws.ui.agent_session_search_seq;

    if query.is_empty() {
        if !ws.ui.agent_session_content_hits.is_empty()
            || !ws.ui.agent_session_content_query.is_empty()
        {
            ws.ui.agent_session_content_hits.clear();
            ws.ui.agent_session_content_query.clear();
        }
        ws.ui.agent_session_scroll.scroll_to_item_strict(0, ScrollStrategy::Top);
        cx.notify();
        return;
    }

    let tool = match ws.page {
        crate::pages::Page::Tool(t) => t,
        _ => match ws.ui.agent_sessions.as_ref().map(|(t, _)| *t) {
            Some(t) => t,
            None => return,
        },
    };

    let paths = ws.paths.clone();
    let scope = vec![tool.key().to_string()];
    let weak = cx.entity().downgrade();
    cx.spawn(async move |_this, cx| {
        if !immediate {
            cx.background_executor()
                .timer(std::time::Duration::from_millis(120))
                .await;
        }
        let still_current = weak
            .update(cx, |workspace, _| workspace.ui.agent_session_search_seq == seq)
            .unwrap_or(false);
        if !still_current {
            return;
        }
        let query_bg = query.clone();
        let hits = cx
            .background_spawn(async move {
                session_search::search_sessions_content(
                    &paths,
                    Some(&scope),
                    &query_bg,
                    session_search::DEFAULT_SEARCH_LIMIT,
                )
                .unwrap_or_default()
            })
            .await;

        let _ = weak.update(cx, |workspace, cx| {
            if workspace.ui.agent_session_search_seq != seq {
                return;
            }
            workspace.ui.agent_session_content_hits = hits
                .into_iter()
                .map(|h| (h.source_path.clone(), h))
                .collect();
            workspace.ui.agent_session_content_query = query.to_lowercase();
            workspace.ui.agent_session_scroll.scroll_to_item_strict(0, ScrollStrategy::Top);
            cx.notify();
        });
    })
    .detach();
}

pub fn start_agent_session_index_sync(
    tool: ToolId,
    sessions: Vec<SessionMeta>,
    ws: &mut Workspace,
    cx: &mut Context<Workspace>,
) {
    if sessions.is_empty() {
        if ws
            .ui
            .agent_session_indexing
            .is_some_and(|(t, _, _)| t == tool)
        {
            ws.ui.agent_session_indexing = None;
        }
        return;
    }
    let total = sessions.len();
    ws.ui.agent_session_indexing = Some((tool, 0, total));
    let paths = ws.paths.clone();
    let scope = vec![tool.key().to_string()];
    let (tx, rx) = async_channel::unbounded::<(usize, usize)>();
    let weak = cx.entity().downgrade();

    cx.spawn(async move |_this, cx| {
        let sync_task = cx.background_spawn(async move {
            let _ = session_search::sync_sessions_with_progress(
                &paths,
                &scope,
                &sessions,
                |processed, total| {
                    let _ = tx.try_send((processed, total));
                },
            );
        });
        while let Ok((processed, total)) = rx.recv().await {
            let keep = weak
                .update(cx, |workspace, cx| {
                    workspace.ui.agent_session_indexing = Some((tool, processed, total));
                    cx.notify();
                    true
                })
                .unwrap_or(false);
            if !keep {
                break;
            }
        }
        sync_task.await;
        let _ = weak.update(cx, |workspace, cx| {
            if workspace
                .ui
                .agent_session_indexing
                .is_some_and(|(t, _, _)| t == tool)
            {
                workspace.ui.agent_session_indexing = None;
            }
            let cur_query = workspace.ui.agent_session_search.read(cx).text().to_string();
            if !cur_query.trim().is_empty()
                && matches!(workspace.page, crate::pages::Page::Tool(active) if active == tool)
            {
                schedule_agent_session_search(workspace, cur_query, true, cx);
            } else {
                cx.notify();
            }
        });
    })
    .detach();
}

pub fn load_agent_sessions(tool: ToolId, ws: &mut Workspace, cx: &mut Context<Workspace>) {
    if ws.ui.agent_sessions_loading {
        return;
    }
    ws.ui.agent_sessions_loading = true;
    ws.ui.agent_session_scan = ws.ui.agent_session_scan.wrapping_add(1);
    let generation = ws.ui.agent_session_scan;
    ws.ui.agent_sessions = Some((tool, Vec::new()));
    ws.ui.agent_session_scroll.scroll_to_item_strict(0, ScrollStrategy::Top);
    let cur_query = ws.ui.agent_session_search.read(cx).text().to_string();
    if !cur_query.trim().is_empty() {
        schedule_agent_session_search(ws, cur_query, true, cx);
    }
    let paths = ws.paths.clone();
    let weak = cx.entity().downgrade();
    cx.spawn(async move |_this, cx| {
        if let Some(hit) = session::cached_all_if_fresh(tool) {
            let _ = weak.update(cx, |workspace, cx| {
                if workspace.ui.agent_session_scan != generation {
                    return;
                }
                workspace.ui.agent_sessions = Some((tool, hit.clone()));
                workspace.ui.agent_sessions_loading = false;
                start_agent_session_index_sync(tool, hit, workspace, cx);
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
            let mut synced_list = None;
            if let Some((loaded, list)) = &mut workspace.ui.agent_sessions {
                if *loaded == tool {
                    list.sort_by_key(|item| std::cmp::Reverse(item.last_active_at));
                    session::store_full_list_cache(tool, list.clone());
                    synced_list = Some(list.clone());
                }
            }
            workspace.ui.agent_sessions_loading = false;
            if let Some(list) = synced_list {
                start_agent_session_index_sync(tool, list, workspace, cx);
            }
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

    if let Some(ref open_ag) = ws.ui.antigravity_open_session.clone() {
        if tool == ToolId::Agents {
            let meta: aitoolplus_core::session::SessionMeta = open_ag.clone().into();
            return render_agent_session_detail(tool, &meta, ws, cx);
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

    let hits_ready = !query.is_empty() && ws.ui.agent_session_content_query == query;
    let filtered: Vec<SessionMeta> = sessions
        .iter()
        .filter(|s| {
            if query.is_empty() {
                return true;
            }
            session_matches_metadata(s, &query)
                || (hits_ready && ws.ui.agent_session_content_hits.contains_key(&s.source_path))
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
    if let Some((idx_tool, processed, total)) = ws.ui.agent_session_indexing {
        if idx_tool == tool && total > 0 {
            let label = i
                .raw(
                    &format!("索引正文 {processed}/{total}…"),
                    &format!("Indexing {processed}/{total}…"),
                )
                .to_string();
            toolbar = toolbar.child(
                div()
                    .px(px(8.0))
                    .py(px(3.0))
                    .rounded(px(6.0))
                    .bg(t.tab_bar_bg)
                    .text_size(px(11.5))
                    .text_color(t.text_muted)
                    .child(label),
            );
        }
    }
    toolbar = toolbar.child(
        div()
            .text_size(px(12.0))
            .text_color(t.text_muted)
            .child(format!(
                "{} {} {}",
                i.t("tool_sessions.total"),
                filtered.len(),
                i.t("tool_sessions.sessions")
            )),
    );
    toolbar = toolbar.child(button_with_icon_loading_l(
        "agent-sessions-refresh-btn",
        crate::icons::REFRESH_SVG,
        i.t("tool_sessions.refresh"),
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
    toolbar = toolbar.child(button_with_icon_l(
        "agent-sessions-cleanup-btn",
        crate::icons::TRASH_SVG,
        i.t("tool_sessions.cleanup_old_sessions"),
        ButtonVariant::Secondary,
        &t,
        cx,
        move |ws, _, _, cx| {
            ws.ui.session_cleanup_dialog = Some(SessionCleanupDialogState {
                target: SessionCleanupTarget::Tool(tool),
                period: SessionCleanupPeriod::Days90,
                deleting: false,
            });
            cx.notify();
        },
    ));

    section = section.child(toolbar);

    if ws.ui.agent_sessions_loading && sessions.is_empty() {
        section = section.child(
            crate::components::empty_state_svg(
                &t,
                crate::icons::REFRESH_SVG,
                i.t("tool_sessions.loading_sessions"),
                i.t("tool_sessions.scanning_session_history_files"),
            ),
        );
    } else if filtered.is_empty() {
        section = section.child(
            crate::components::empty_state_svg(
                &t,
                crate::icons::FOLDER_SVG,
                i.t("tool_sessions.no_sessions_found"),
                i.t("tool_sessions.this_agent_s_session"),
            ),
        );
    } else {
        let items = Arc::new(filtered);
        let hits_arc = if hits_ready {
            Arc::new(ws.ui.agent_session_content_hits.clone())
        } else {
            Arc::new(HashMap::new())
        };
        section = section.child(session_list_viewport(
            "agent-sessions-virtual-list",
            "agent-sessions-scrollbar",
            items,
            hits_arc,
            !query.is_empty(),
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
    items: Arc<Vec<SessionMeta>>,
    content_hits: Arc<HashMap<String, ContentHit>>,
    search_active: bool,
    tool: ToolId,
    scroll: gpui::UniformListScrollHandle,
    ws_entity: gpui::Entity<Workspace>,
    t: &Theme,
    i: &I18n,
) -> gpui::AnyElement {
    let items_len = items.len();
    let items_for_list = items.clone();
    let hits_for_list = content_hits;
    let t_clone = t.clone();
    let i_clone = i.clone();
    let v_list = uniform_list(
        list_id,
        items_len,
        move |range: std::ops::Range<usize>, _window: &mut gpui::Window, _cx: &mut gpui::App| {
            let mut elements = Vec::with_capacity(range.len());
            for idx in range {
                if let Some(item) = items_for_list.get(idx) {
                    let hit = hits_for_list.get(&item.source_path);
                    elements.push(render_virtual_agent_session_card(
                        item,
                        hit,
                        search_active,
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
    content_hit: Option<&ContentHit>,
    search_active: bool,
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
        )
        .children(if search_active {
            Some(
                div()
                    .h(px(18.0))
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .child(match content_hit.and_then(|h| h.snippets.first().map(|sn| (h.match_count, sn))) {
                        Some((count, snippet)) => div()
                            .flex()
                            .items_center()
                            .gap(px(6.0))
                            .min_w(px(0.0))
                            .flex_1()
                            .overflow_hidden()
                            .child(
                                div()
                                    .flex_shrink_0()
                                    .px(px(5.0))
                                    .py(px(1.0))
                                    .rounded(px(4.0))
                                    .bg(t.accent_subtle)
                                    .text_size(px(10.5))
                                    .font_weight(gpui::FontWeight::MEDIUM)
                                    .text_color(t.accent)
                                    .child(
                                        i.raw(
                                            &format!("正文 {count} 处"),
                                            &format!("{count} in body"),
                                        )
                                        .to_string(),
                                    ),
                            )
                            .child(
                                div()
                                    .min_w(px(0.0))
                                    .flex_1()
                                    .overflow_hidden()
                                    .text_ellipsis()
                                    .text_size(px(11.5))
                                    .text_color(t.text_secondary)
                                    .child(snippet.text.clone()),
                            )
                            .into_any_element(),
                        None => div()
                            .text_size(px(11.0))
                            .text_color(t.text_muted)
                            .child(i.raw("标题 / 元数据匹配", "Title / metadata match").to_string())
                            .into_any_element(),
                    })
                    .into_any_element(),
            )
        } else {
            None
        });

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
                            ws.ui.toast(ws.i18n.t("tool_sessions.copied_resume_command").to_string(), false);
                        } else {
                            ws.ui.toast(ws.i18n.t("tool_sessions.resume_not_supported_for").to_string(), true);
                        }
                        cx.notify();
                    });
                })
                .child(crate::icons::svg_icon(crate::icons::TERMINAL_SVG, px(11.5), t.text_secondary))
                .child(i.t("tool_sessions.resume")),
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
                .child(i.t("tool_sessions.rename")),
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
                                title: ws.i18n.t("tool_sessions.delete_antigravity_session").to_string(),
                                message: msg,
                                action: crate::pages::ConfirmAction::DeleteAntigravitySession { session },
                            });
                        } else {
                            ws.ui.confirm = Some(crate::pages::ConfirmState {
                                title: ws.i18n.t("tool_sessions.delete_session").to_string(),
                                message: ws
                                    .i18n
                                    .t("tool_sessions.delete_this_session_this")
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
                .child(i.t("tool_sessions.delete")),
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
                    i.t("tool_sessions.cancel"),
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
                    i.t("tool_sessions.save"),
                    ButtonVariant::Primary,
                    &t,
                    cx,
                    move |ws, _, _, cx| {
                        let title: String = input.update(cx, |inp, _| inp.text().to_string());
                        match session::rename_session(&meta, &title) {
                            Ok(()) => {
                                session::invalidate_cache();
                                if let Some(ref mut list) = ws.ui.antigravity_sessions {
                                    if let Some(item) = list.iter_mut().find(|s| s.session_id == meta.session_id) {
                                        item.title = title.clone();
                                    }
                                }
                                if let Some(ref mut open) = ws.ui.antigravity_open_session {
                                    if open.session_id == meta.session_id {
                                        open.title = title.clone();
                                    }
                                }
                                ws.ui.toast(ws.i18n.t("tool_sessions.renamed").to_string(), false);
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
        i.t("tool_sessions.rename_session").as_ref(),
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

#[derive(Clone, Debug)]
struct CleanupPreviewSummary {
    total_count: usize,
    matched_count: usize,
    cutoff_date: String,
    samples: Vec<(String, String)>,
}

fn format_ts_date_only(ms: i64) -> String {
    chrono::DateTime::<chrono::Utc>::from_timestamp_millis(ms)
        .map(|dt| dt.with_timezone(&chrono::Local).format("%Y-%m-%d").to_string())
        .unwrap_or_else(|| "-".to_string())
}

fn format_ts_short(ms: Option<i64>) -> String {
    ms.and_then(chrono::DateTime::<chrono::Utc>::from_timestamp_millis)
        .map(|dt| {
            dt.with_timezone(&chrono::Local)
                .format("%Y-%m-%d %H:%M")
                .to_string()
        })
        .unwrap_or_else(|| "-".to_string())
}

fn cleanup_preview_summary(
    all_sessions: &[SessionMeta],
    stale_sessions: &[SessionMeta],
    older_than_days: u32,
    now_ms: i64,
) -> CleanupPreviewSummary {
    let cutoff_ms = now_ms - (older_than_days as i64).saturating_mul(86_400_000);
    let samples = stale_sessions
        .iter()
        .take(5)
        .map(|s| {
            let title = s
                .title
                .as_deref()
                .map(str::trim)
                .filter(|t| !t.is_empty())
                .unwrap_or(s.session_id.as_str())
                .to_string();
            let ts = format_ts_short(s.last_active_at.or(s.created_at));
            (title, ts)
        })
        .collect();
    CleanupPreviewSummary {
        total_count: all_sessions.len(),
        matched_count: stale_sessions.len(),
        cutoff_date: format_ts_date_only(cutoff_ms),
        samples,
    }
}

pub(crate) fn render_session_cleanup_dialog(
    state: SessionCleanupDialogState,
    ws: &mut Workspace,
    cx: &mut Context<Workspace>,
) -> gpui::AnyElement {
    let t = ws.theme.clone();
    let i = ws.i18n;
    let now_ms = chrono::Utc::now().timestamp_millis();
    let days = state.period.days();

    let (target_label, loading, all_as_meta, stale_tool_metas, stale_ag_metas): (
        String,
        bool,
        Vec<SessionMeta>,
        Vec<SessionMeta>,
        Vec<AntigravitySessionMeta>,
    ) = match state.target {
        SessionCleanupTarget::Tool(tool) => {
            let list = ws
                .ui
                .agent_sessions
                .as_ref()
                .filter(|(loaded, _)| *loaded == tool)
                .map(|(_, l)| l.clone())
                .unwrap_or_default();
            let stale = session::filter_stale_sessions(&list, days, now_ms);
            (
                i.raw(tool.name_zh(), tool.name_en()).to_string(),
                ws.ui.agent_sessions_loading,
                list,
                stale,
                Vec::new(),
            )
        }
        SessionCleanupTarget::Antigravity => {
            let list = ws.ui.antigravity_sessions.clone().unwrap_or_default();
            let stale_ag = filter_stale_antigravity_sessions(&list, days, now_ms);
            let all_meta: Vec<SessionMeta> = list.into_iter().map(Into::into).collect();
            let stale_meta: Vec<SessionMeta> = stale_ag.iter().cloned().map(Into::into).collect();
            (
                "Antigravity".to_string(),
                ws.ui.antigravity_sessions_loading,
                all_meta,
                stale_meta,
                stale_ag,
            )
        }
    };

    let preview = cleanup_preview_summary(&all_as_meta, &stale_tool_metas, days, now_ms);
    let can_confirm = !loading && !state.deleting && preview.matched_count > 0;

    let period_selector = segmented_pill_selector(
        "sess-cleanup-period",
        vec![
            (SessionCleanupPeriod::Days14, None, i.t("tool_sessions.cleanup_14d")),
            (SessionCleanupPeriod::Days30, None, i.t("tool_sessions.cleanup_1m")),
            (SessionCleanupPeriod::Days90, None, i.t("tool_sessions.cleanup_3m")),
            (SessionCleanupPeriod::Days180, None, i.t("tool_sessions.cleanup_6m")),
        ],
        state.period,
        &t,
        cx,
        |ws, next_period, _, cx| {
            if let Some(ref mut dlg) = ws.ui.session_cleanup_dialog {
                if !dlg.deleting {
                    dlg.period = next_period;
                    cx.notify();
                }
            }
        },
    );

    let mut preview_box = div()
        .w_full()
        .p(px(12.0))
        .rounded(px(8.0))
        .bg(t.tab_bar_bg)
        .border_1()
        .border_color(t.card_border)
        .flex()
        .flex_col()
        .gap(px(8.0));

    if loading {
        preview_box = preview_box.child(
            div()
                .text_size(px(12.5))
                .text_color(t.text_muted)
                .child(i.t("tool_sessions.cleanup_scanning_hint")),
        );
    } else if preview.matched_count == 0 {
        preview_box = preview_box.child(
            div()
                .text_size(px(12.5))
                .text_color(t.text_secondary)
                .child(i.t("tool_sessions.cleanup_none_matched")),
        );
    } else {
        let count = preview.matched_count;
        let total = preview.total_count;
        let cutoff = preview.cutoff_date.clone();
        let summary_text = i
            .raw(
                &format!(
                    "将清理 {count} 条会话记录（共 {total} 条，最后活跃早于 {cutoff}）"
                ),
                &format!(
                    "{count} of {total} sessions will be removed (last active before {cutoff})"
                ),
            )
            .to_string();

        preview_box = preview_box.child(
            div()
                .text_size(px(13.0))
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .text_color(t.danger)
                .child(summary_text),
        );

        let mut samples_col = div().flex().flex_col().gap(px(4.0));
        for (title, ts) in &preview.samples {
            samples_col = samples_col.child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(8.0))
                    .text_size(px(11.5))
                    .child(
                        div()
                            .min_w(px(0.0))
                            .flex_1()
                            .overflow_hidden()
                            .text_ellipsis()
                            .whitespace_nowrap()
                            .text_color(t.text_primary)
                            .child(title.clone()),
                    )
                    .child(
                        div()
                            .flex_shrink_0()
                            .text_color(t.text_muted)
                            .child(ts.clone()),
                    ),
            );
        }
        if preview.matched_count > preview.samples.len() {
            let remaining = preview.matched_count - preview.samples.len();
            samples_col = samples_col.child(
                div()
                    .text_size(px(11.0))
                    .text_color(t.text_muted)
                    .child(
                        i.raw(
                            &format!("… 其余 {remaining} 条已省略"),
                            &format!("… and {remaining} more"),
                        )
                        .to_string(),
                    ),
            );
        }
        preview_box = preview_box.child(samples_col);
    }

    let confirm_label = if preview.matched_count > 0 {
        i.raw(
            &format!("确认清理 ({})", preview.matched_count),
            &format!("Clean Up ({})", preview.matched_count),
        )
        .to_string()
    } else {
        i.t("tool_sessions.cleanup_confirm_btn").to_string()
    };

    let confirm_btn = if can_confirm || state.deleting {
        let stale_tool = stale_tool_metas;
        let stale_ag = stale_ag_metas;
        let target = state.target;
        button_loading_l(
            "sess-cleanup-confirm",
            confirm_label,
            ButtonVariant::Danger,
            state.deleting,
            &t,
            cx,
            move |ws, _, _, cx| {
                if let Some(ref mut dlg) = ws.ui.session_cleanup_dialog {
                    if dlg.deleting {
                        return;
                    }
                    dlg.deleting = true;
                }
                cx.notify();

                let paths_bg = ws.paths.clone();
                let home = ws.paths.home.clone();
                let weak = cx.entity().downgrade();
                let deleted_ids: std::collections::HashSet<String> = match target {
                    SessionCleanupTarget::Tool(_) => {
                        stale_tool.iter().map(|s| s.session_id.clone()).collect()
                    }
                    SessionCleanupTarget::Antigravity => {
                        stale_ag.iter().map(|s| s.session_id.clone()).collect()
                    }
                };
                let deleted_paths: std::collections::HashSet<String> = stale_tool
                    .iter()
                    .map(|s| s.source_path.clone())
                    .collect();
                let stale_tool_bg = stale_tool.clone();
                let stale_ag_bg = stale_ag.clone();

                cx.spawn(async move |_this, cx| {
                    let (deleted, failed) = cx
                        .background_spawn(async move {
                            match target {
                                SessionCleanupTarget::Tool(_) => {
                                    let res = session::delete_sessions_batch(&stale_tool_bg);
                                    if res.0 > 0 {
                                        let items: Vec<(String, String)> = stale_tool_bg
                                            .iter()
                                            .map(|s| (s.provider_id.clone(), s.source_path.clone()))
                                            .collect();
                                        session_search::forget_indexed_sources(&paths_bg, &items);
                                    }
                                    res
                                }
                                SessionCleanupTarget::Antigravity => {
                                    let res = delete_antigravity_sessions_batch(&home, &stale_ag_bg);
                                    if res.0 > 0 {
                                        session::invalidate_cache();
                                        let items: Vec<(String, String)> = stale_ag_bg
                                            .iter()
                                            .map(|s| {
                                                let meta: SessionMeta = s.clone().into();
                                                (meta.provider_id, meta.source_path)
                                            })
                                            .collect();
                                        session_search::forget_indexed_sources(&paths_bg, &items);
                                    }
                                    res
                                }
                            }
                        })
                        .await;

                    let _ = weak.update(cx, |ws, cx| {
                        match target {
                            SessionCleanupTarget::Tool(t_id) => {
                                if let Some((loaded, list)) = &mut ws.ui.agent_sessions {
                                    if *loaded == t_id {
                                        list.retain(|s| !deleted_ids.contains(&s.session_id));
                                    }
                                }
                                ws.ui
                                    .agent_session_content_hits
                                    .retain(|path, _| !deleted_paths.contains(path));
                                if let Some((open_t, open_sid)) = &ws.ui.open_session {
                                    if *open_t == t_id && deleted_ids.contains(open_sid) {
                                        ws.ui.open_session = None;
                                    }
                                }
                            }
                            SessionCleanupTarget::Antigravity => {
                                if let Some(ref mut list) = ws.ui.antigravity_sessions {
                                    list.retain(|s| !deleted_ids.contains(&s.session_id));
                                }
                                ws.ui
                                    .antigravity_session_content_hits
                                    .retain(|path, _| !deleted_paths.contains(path));
                                if let Some(ref open) = ws.ui.antigravity_open_session {
                                    if deleted_ids.contains(&open.session_id) {
                                        ws.ui.antigravity_open_session = None;
                                    }
                                }
                                if let Some((_, ref open_sid)) = ws.ui.open_session {
                                    if deleted_ids.contains(open_sid) {
                                        ws.ui.open_session = None;
                                    }
                                }
                                ws.ui.agent_sessions = None;
                            }
                        }

                        ws.ui.session_cleanup_dialog = None;
                        let msg = if failed == 0 {
                            ws.i18n
                                .raw(
                                    &format!("已清理 {deleted} 条旧会话"),
                                    &format!("Cleaned up {deleted} old sessions"),
                                )
                                .to_string()
                        } else {
                            ws.i18n
                                .raw(
                                    &format!("已清理 {deleted} 条旧会话，{failed} 条删除失败"),
                                    &format!("Cleaned up {deleted} sessions, {failed} failed"),
                                )
                                .to_string()
                        };
                        ws.ui.toast(msg, failed > 0 && deleted == 0);
                        cx.notify();
                    });
                })
                .detach();
            },
        )
    } else {
        div()
            .id("sess-cleanup-confirm-disabled")
            .h(px(30.0))
            .px(px(12.0))
            .rounded(px(6.0))
            .flex()
            .flex_none()
            .items_center()
            .justify_center()
            .text_size(px(12.5))
            .font_weight(gpui::FontWeight::MEDIUM)
            .bg(t.tab_bar_bg)
            .border_1()
            .border_color(t.card_border)
            .text_color(t.text_muted)
            .opacity(0.6)
            .child(confirm_label)
            .into_any_element()
    };

    let body = div()
        .flex()
        .flex_col()
        .gap(px(14.0))
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(6.0))
                .child(
                    div()
                        .text_size(px(12.5))
                        .text_color(t.text_secondary)
                        .child(
                            i.raw(
                                &format!("{}（{}）", i.t("tool_sessions.cleanup_period_label"), target_label),
                                &format!("{} ({})", i.t("tool_sessions.cleanup_period_label"), target_label),
                            )
                            .to_string(),
                        ),
                )
                .child(period_selector),
        )
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
                        .child(i.t("tool_sessions.cleanup_preview_title")),
                )
                .child(preview_box),
        )
        .child(
            div()
                .text_size(px(11.5))
                .text_color(t.text_muted)
                .child(i.t("tool_sessions.cleanup_warning")),
        )
        .child(
            div()
                .flex()
                .justify_end()
                .gap(px(8.0))
                .child(button_l(
                    "sess-cleanup-cancel",
                    i.t("tool_sessions.cancel"),
                    ButtonVariant::Secondary,
                    &t,
                    cx,
                    |ws, _, _, cx| {
                        if let Some(ref dlg) = ws.ui.session_cleanup_dialog {
                            if dlg.deleting {
                                return;
                            }
                        }
                        ws.ui.session_cleanup_dialog = None;
                        cx.notify();
                    },
                ))
                .child(confirm_btn),
        );

    let title = format!("{} - {}", i.t("tool_sessions.cleanup_dialog_title"), target_label);
    crate::pages::modal_scaffold_sized(
        &t,
        &title,
        px(500.0),
        None,
        body.into_any_element(),
        cx,
        |ws, _, _, cx| {
            ws.ui.session_cleanup_dialog = None;
            cx.notify();
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cleanup_period_days_matches_user_presets() {
        assert_eq!(SessionCleanupPeriod::Days14.days(), 14);
        assert_eq!(SessionCleanupPeriod::Days30.days(), 30);
        assert_eq!(SessionCleanupPeriod::Days90.days(), 90);
        assert_eq!(SessionCleanupPeriod::Days180.days(), 180);
    }

    #[test]
    fn cleanup_preview_summary_caps_samples_and_reports_counts() {
        let now_ms = 1_700_000_000_000_i64;
        let day_ms = 86_400_000_i64;
        let all: Vec<SessionMeta> = (0..8)
            .map(|idx| SessionMeta {
                provider_id: "codex".into(),
                session_id: format!("sid-{idx}"),
                title: Some(format!("Session {idx}")),
                summary: None,
                project_dir: None,
                created_at: None,
                last_active_at: Some(if idx < 6 {
                    now_ms - 100 * day_ms
                } else {
                    now_ms - 2 * day_ms
                }),
                source_path: format!("/tmp/{idx}.jsonl"),
                resume_command: None,
            })
            .collect();

        let stale = session::filter_stale_sessions(&all, 90, now_ms);
        let summary = cleanup_preview_summary(&all, &stale, 90, now_ms);
        assert_eq!(summary.total_count, 8);
        assert_eq!(summary.matched_count, 6);
        assert_eq!(summary.samples.len(), 5);
        assert_eq!(summary.samples[0].0, "Session 0");
    }
}


// ---------------------------------------------------------------------------
// Claude Code Plugins tab
// ---------------------------------------------------------------------------

