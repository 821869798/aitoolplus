//! Antigravity account management page: list, add, refresh quotas, switch active credentials.
//!
//! STRICT CONSTRAINT: Zero reverse proxy, zero proxy server, zero proxy pool.
//! Pure client-side account management, quota inspection, and credential switching.

use std::time::Duration;
use aitoolplus_core::antigravity::{
    AntigravityAccount, AntigravitySessionMeta, AntigravityStore, DeviceProfile, OAuthServerSession,
    build_account_from_refresh_token, exchange_auth_code, extract_oauth_code, extract_tokens_from_text,
    fetch_project_and_tier, fetch_quota, fetch_user_info, import_all_local_accounts,
    import_from_custom_db_path, import_from_local_system, import_from_v1_backup, load_store, open_browser,
    save_store, scan_antigravity_sessions, switch_account_target, toggle_account_disabled,
    update_account_device_profile, update_account_label,
};
use gpui::{Context, IntoElement, SharedString, div, prelude::*, px, uniform_list};
use chrono::{DateTime, NaiveDateTime, Utc};

use crate::components::{
    ButtonVariant, Tooltip, button_l, button_with_icon_l, empty_state_svg,
    error_action_link_button, error_banner, error_strip, error_strip_action,
    icon_button_svg, input_container, parse_generic_error,
};
use crate::icons::{
    ALERT_SVG, ARROW_LEFT_SVG, ARROW_RIGHT_LEFT_SVG, CHECK_SVG, CLOCK_SVG, CODE_SVG, COPY_SVG, DATABASE_SVG,
    DOWNLOAD_SVG, FINGERPRINT_SVG, FOLDER_SVG, GEMINI_SVG, GLOBE_SVG, HISTORY_SVG, INFO_SVG, PLUS_SVG,
    REFRESH_SVG, REPEAT_SVG, TAG_SVG, TERMINAL_SVG, TOGGLE_LEFT_SVG, TOGGLE_RIGHT_SVG, TRASH_SVG,
    USER_SVG, WAND_SVG, X_SVG, svg_icon,
};
use crate::text_input::TextInput;
use crate::theme::Theme;
use crate::workspace::Workspace;

use super::{
    AntigravityPageTab, AntigravitySessionFilter,
    AntigravityTierFilter, ConfirmAction, ConfirmState, modal_scaffold, modal_scaffold_custom,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AntigravityDialogTab {
    #[default]
    OAuth,
    Token,
    Import,
}

#[derive(Clone)]
pub struct AntigravityDialogState {
    pub active_tab: AntigravityDialogTab,
    pub oauth_url: Option<String>,
    pub redirect_uri: Option<String>,
    pub oauth_url_copied: bool,
    pub manual_code: gpui::Entity<TextInput>,
    pub refresh_token: gpui::Entity<TextInput>,
    pub custom_label: gpui::Entity<TextInput>,
    pub auth_status: Option<String>,
    pub is_authorizing: bool,
    pub error_message: Option<String>,
    pub session: Option<std::sync::Arc<OAuthServerSession>>,
}

/// Mask an email for privacy (e.g. "unifangg@gmail.com" -> "uni***@gmail.com").
pub fn mask_email(email: &str) -> String {
    if let Some((user, domain)) = email.split_once('@') {
        let visible_len = if user.len() <= 3 { 1 } else { 3 };
        format!("{}***@{}", &user[..visible_len], domain)
    } else if email.len() > 3 {
        format!("{}***", &email[..3])
    } else {
        "***".to_string()
    }
}

/// Render the main Antigravity page.
pub fn render_antigravity_page(ws: &mut Workspace, cx: &mut Context<Workspace>) -> gpui::AnyElement {
    let t = ws.theme.clone();
    let i = ws.i18n;

    let tab_selector = crate::components::segmented_pill_selector(
        "ag-page-tab",
        vec![
            (
                AntigravityPageTab::Accounts,
                Some(USER_SVG),
                i.t("账号与配额", "Accounts & Quota"),
            ),
            (
                AntigravityPageTab::Sessions,
                Some(HISTORY_SVG),
                i.t("会话管理", "Sessions"),
            ),
        ],
        ws.ui.antigravity_tab,
        &t,
        cx,
        |ws, tab, _, cx| {
            ws.ui.antigravity_tab = tab;
            cx.notify();
        },
    );

    let content = match ws.ui.antigravity_tab {
        AntigravityPageTab::Accounts => render_accounts_tab(ws, cx),
        AntigravityPageTab::Sessions => {
            if let Some(ref open_meta) = ws.ui.antigravity_open_session.clone() {
                render_antigravity_session_detail(open_meta, ws, cx)
            } else {
                render_antigravity_sessions_tab(ws, cx)
            }
        }
    };

    div()
        .flex()
        .flex_col()
        .w_full()
        .h_full()
        .min_w(px(0.0))
        .min_h(px(0.0))
        .gap(px(12.0))
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .gap(px(8.0))
                .flex_shrink_0()
                .child(tab_selector),
        )
        .child(
            div()
                .w_full()
                .flex_1()
                .h_full()
                .min_h(px(0.0))
                .child(content),
        )
        .into_any_element()
}

fn account_matches_tier(acc: &AntigravityAccount, filter: AntigravityTierFilter) -> bool {
    match filter {
        AntigravityTierFilter::All => true,
        AntigravityTierFilter::Pro => {
            let t = acc.get_tier();
            t.eq_ignore_ascii_case("pro") || t.eq_ignore_ascii_case("enterprise")
        }
        AntigravityTierFilter::Ultra => acc.get_tier().eq_ignore_ascii_case("ultra"),
        AntigravityTierFilter::Free => {
            let t = acc.get_tier();
            !t.eq_ignore_ascii_case("pro") && !t.eq_ignore_ascii_case("enterprise") && !t.eq_ignore_ascii_case("ultra")
        }
    }
}

pub fn render_accounts_tab(ws: &mut Workspace, cx: &mut Context<Workspace>) -> gpui::AnyElement {
    let t = ws.theme.clone();
    let i = ws.i18n;

    // Lazily load store from disk if not loaded yet
    if ws.ui.antigravity_store.is_none() {
        let store = load_store(&ws.paths.app_data);
        ws.ui.antigravity_store = Some(store);
    }

    let store = ws.ui.antigravity_store.clone().unwrap_or_default();
    let accounts = &store.accounts;

    // CC-Switch & Antigravity-Manager Parity: Auto-refresh on page entry if due
    if ws.settings.antigravity_auto_refresh
        && !ws.ui.antigravity_refreshing_all
        && !accounts.is_empty()
        && aitoolplus_core::antigravity::is_auto_refresh_due(
            ws.settings.last_antigravity_refresh_time.as_deref(),
            ws.settings.antigravity_refresh_interval_minutes,
        )
    {
        ws.ui.antigravity_refreshing_all = true;
        let accounts_to_refresh = accounts.clone();
        let app_data = ws.paths.app_data.clone();
        let weak = cx.entity().downgrade();
        cx.spawn(async move |_this, cx| {
            let updated_accounts = cx.background_spawn(async move {
                let mut s = aitoolplus_core::antigravity::AntigravityStore {
                    accounts: accounts_to_refresh,
                    active_account_id: None,
                };
                aitoolplus_core::antigravity::refresh_all_quotas(&mut s);
                s.accounts
            }).await;

            let _ = weak.update(cx, |ws: &mut Workspace, cx| {
                ws.ui.antigravity_refreshing_all = false;
                if let Some(s) = &mut ws.ui.antigravity_store {
                    s.accounts = updated_accounts;
                    let _ = save_store(&app_data, s);
                }
                ws.settings.last_antigravity_refresh_time = Some(chrono::Utc::now().to_rfc3339());
                (ws.callbacks.save_settings)(&ws.settings);
                cx.notify();
            });
        }).detach();
    }

    let add_label = i.t("添加账号", "Add Account");
    let import_label = i.t("从本机导入", "Import Local");
    let refresh_all_label = i.t("刷新全部配额", "Refresh All Quotas");
    let cur_tier_filter = ws.ui.antigravity_tier_filter;

    let query = ws.ui.antigravity_search.read(cx).text().trim().to_lowercase();
    let searched_accounts: Vec<_> = accounts
        .iter()
        .filter(|a| {
            if query.is_empty() {
                return true;
            }
            a.email.to_lowercase().contains(&query)
                || a.name.as_deref().unwrap_or("").to_lowercase().contains(&query)
                || a.custom_label.as_deref().unwrap_or("").to_lowercase().contains(&query)
        })
        .collect();

    let count_all = searched_accounts.len();
    let count_pro = searched_accounts.iter().filter(|a| account_matches_tier(a, AntigravityTierFilter::Pro)).count();
    let count_ultra = searched_accounts.iter().filter(|a| account_matches_tier(a, AntigravityTierFilter::Ultra)).count();
    let count_free = searched_accounts.iter().filter(|a| account_matches_tier(a, AntigravityTierFilter::Free)).count();

    let filtered_accounts: Vec<_> = searched_accounts
        .into_iter()
        .filter(|a| account_matches_tier(a, cur_tier_filter))
        .collect();

    let mut section = div()
        .flex()
        .flex_col()
        .w_full()
        .min_w(px(0.0))
        .gap(px(12.0))
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
                        .flex_wrap()
                        .child(button_with_icon_l(
                            "ag-add-btn",
                            PLUS_SVG,
                            add_label,
                            ButtonVariant::Primary,
                            &t,
                            cx,
                            |ws, _, window, cx| {
                                open_add_account_dialog(ws, window, cx);
                            },
                        ))
                        .child(button_with_icon_l(
                            "ag-import-btn",
                            DOWNLOAD_SVG,
                            import_label,
                            ButtonVariant::Secondary,
                            &t,
                            cx,
                            |ws, _, _, cx| {
                                import_local_account_action(ws, cx);
                            },
                        ))
                        .child(button_with_icon_l(
                            "ag-import-manager-btn",
                            DOWNLOAD_SVG,
                            i.t("从 Manager 迁移", "Import from Manager"),
                            ButtonVariant::Secondary,
                            &t,
                            cx,
                            |ws, _, _, cx| {
                                import_from_manager_action(ws, cx);
                            },
                        ))
                        .child(button_with_icon_l(
                            "ag-refresh-all-btn",
                            REFRESH_SVG,
                            refresh_all_label,
                            ButtonVariant::Secondary,
                            &t,
                            cx,
                            |ws, _, _, cx| {
                                refresh_all_quotas_action(ws, cx);
                            },
                        ))
                        .child(
                            div()
                                .px(px(8.0))
                                .py(px(4.0))
                                .rounded(px(6.0))
                                .bg(t.sidebar_bg)
                                .text_size(px(12.0))
                                .text_color(t.text_secondary)
                                .child(format!("共 {} 个账号", accounts.len())),
                        )
                        .child({
                            let auto_on = ws.settings.antigravity_auto_refresh;
                            let interval = ws.settings.antigravity_refresh_interval_minutes;
                            let chip_theme = t.clone();
                            div()
                                .id("ag-auto-refresh-chip")
                                .cursor_pointer()
                                .px(px(8.0))
                                .py(px(4.0))
                                .rounded(px(6.0))
                                .bg(if auto_on { chip_theme.tab_active_bg } else { chip_theme.sidebar_bg })
                                .border_1()
                                .border_color(if auto_on { chip_theme.accent } else { chip_theme.card_border })
                                .flex()
                                .items_center()
                                .gap(px(5.0))
                                .text_size(px(12.0))
                                .text_color(if auto_on { chip_theme.text_primary } else { chip_theme.text_muted })
                                .child(
                                    div()
                                        .w(px(6.0))
                                        .h(px(6.0))
                                        .rounded_full()
                                        .bg(if auto_on { crate::rgba_const(0x22c55eff) } else { chip_theme.text_muted }),
                                )
                                .child(if auto_on {
                                    format!("自动刷新: {}m", interval)
                                } else {
                                    i.t("自动刷新: 关", "Auto Refresh: Off").to_string()
                                })
                                .on_click(cx.listener(move |ws, _, _, cx| {
                                    ws.settings.antigravity_auto_refresh = !ws.settings.antigravity_auto_refresh;
                                    (ws.callbacks.save_settings)(&ws.settings);
                                    let msg = if ws.settings.antigravity_auto_refresh {
                                        ws.i18n.t(
                                            &format!("后台自动刷新已开启，每 {} 分钟自动更新配额", ws.settings.antigravity_refresh_interval_minutes),
                                            &format!("Auto refresh enabled (every {}m)", ws.settings.antigravity_refresh_interval_minutes),
                                        )
                                    } else {
                                        ws.i18n.t("后台自动刷新已关闭", "Auto refresh disabled")
                                    };
                                    ws.ui.toast(msg.to_string(), false);
                                    cx.notify();
                                }))
                        }),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(10.0))
                        .flex_wrap()
                        // Account tier filter (全部 | PRO | ULTRA | FREE)
                        .child({
                            let cur_tier = cur_tier_filter;
                            let make_pill = |id_str: &'static str, label: SharedString, count: usize, filter_val: AntigravityTierFilter| {
                                let is_active = cur_tier == filter_val;
                                div()
                                    .id(id_str)
                                    .px(px(8.0))
                                    .py(px(3.5))
                                    .rounded(px(5.0))
                                    .cursor_pointer()
                                    .flex()
                                    .items_center()
                                    .gap(px(4.0))
                                    .text_size(px(11.5))
                                    .font_weight(if is_active { gpui::FontWeight::BOLD } else { gpui::FontWeight::NORMAL })
                                    .text_color(if is_active { t.accent } else { t.text_secondary })
                                    .when(is_active, |s| s.bg(t.card_bg).shadow_xs())
                                    .child(label)
                                    .child(
                                        div()
                                            .px(px(4.5))
                                            .py(px(0.5))
                                            .rounded(px(4.0))
                                            .text_size(px(10.0))
                                            .font_weight(gpui::FontWeight::BOLD)
                                            .bg(if is_active { t.accent_subtle } else { t.card_border })
                                            .text_color(if is_active { t.accent } else { t.text_muted })
                                            .child(count.to_string()),
                                    )
                                    .on_click(cx.listener(move |ws, _, _, cx| {
                                        ws.ui.antigravity_tier_filter = filter_val;
                                        cx.notify();
                                    }))
                            };

                            div()
                                .flex()
                                .items_center()
                                .p(px(2.0))
                                .rounded(px(7.0))
                                .bg(t.input_bg)
                                .border_1()
                                .border_color(t.card_border)
                                .child(make_pill(
                                    "toggle-tier-all",
                                    SharedString::from(i.t("全部", "All")),
                                    count_all,
                                    AntigravityTierFilter::All,
                                ))
                                .child(make_pill(
                                    "toggle-tier-pro",
                                    SharedString::from("PRO"),
                                    count_pro,
                                    AntigravityTierFilter::Pro,
                                ))
                                .child(make_pill(
                                    "toggle-tier-ultra",
                                    SharedString::from("ULTRA"),
                                    count_ultra,
                                    AntigravityTierFilter::Ultra,
                                ))
                                .child(make_pill(
                                    "toggle-tier-free",
                                    SharedString::from("FREE"),
                                    count_free,
                                    AntigravityTierFilter::Free,
                                ))
                        })
                        // Search bar
                        .child(
                            div()
                                .w(px(260.0))
                                .child(input_container(&t, ws.ui.antigravity_search.clone())),
                        ),
                ),
        );

    if filtered_accounts.is_empty() {
        let is_filtered = !query.is_empty() || cur_tier_filter != AntigravityTierFilter::All;
        section = section.child(empty_state_svg(
            &t,
            GEMINI_SVG,
            if !is_filtered {
                i.t("还没有添加 Antigravity 账号", "No Antigravity accounts yet")
            } else {
                i.t("没有找到匹配的账号", "No matching accounts found")
            },
            if !is_filtered {
                i.t(
                    "点击上方「添加账号」进行 Google 授权或输入 Refresh Token，也可以「从本机导入」当前凭据",
                    "Click 'Add Account' or 'Import Local' above to get started",
                )
            } else {
                i.t("尝试更换筛选分类或搜索关键词", "Try switching filter tier or changing search keywords")
            },
        ));
    } else {
        let mut list = div().flex().flex_col().gap(px(12.0));
        for acc in &filtered_accounts {
            list = list.child(render_account_card(acc, ws, cx));
        }
        section = section.child(list);
    }

    section.into_any_element()
}

pub fn render_antigravity_sessions_tab(ws: &mut Workspace, cx: &mut Context<Workspace>) -> gpui::AnyElement {
    let t = ws.theme.clone();
    let i = ws.i18n;

    // Lazily scan sessions
    if ws.ui.antigravity_sessions.is_none() {
        let sessions = scan_antigravity_sessions(&ws.paths.home, 200);
        ws.ui.antigravity_sessions = Some(sessions);
    }

    let sessions = ws.ui.antigravity_sessions.clone().unwrap_or_default();
    let filter = ws.ui.antigravity_session_filter;
    let query = ws.ui.antigravity_session_search.read(cx).text().trim().to_lowercase();

    let filtered: Vec<AntigravitySessionMeta> = sessions
        .into_iter()
        .filter(|s| {
            let match_filter = match filter {
                AntigravitySessionFilter::All => true,
                AntigravitySessionFilter::Cli => s.source == "cli",
                AntigravitySessionFilter::App => s.source == "app",
            };
            if !match_filter {
                return false;
            }
            if query.is_empty() {
                return true;
            }
            s.title.to_lowercase().contains(&query)
                || s.session_id.to_lowercase().contains(&query)
                || s.preview.to_lowercase().contains(&query)
                || s.project_dir.as_deref().unwrap_or("").to_lowercase().contains(&query)
        })
        .collect();

    let total_count = filtered.len();

    // Toolbar
    let filter_selector = crate::components::segmented_pill_selector(
        "ag-sess-filter",
        vec![
            (
                AntigravitySessionFilter::All,
                None,
                i.t("全部", "All"),
            ),
            (
                AntigravitySessionFilter::Cli,
                Some(TERMINAL_SVG),
                i.t("终端 CLI", "Terminal CLI"),
            ),
            (
                AntigravitySessionFilter::App,
                Some(CODE_SVG),
                i.t("桌面 App", "Desktop App"),
            ),
        ],
        filter,
        &t,
        cx,
        |ws, f, _, cx| {
            ws.ui.antigravity_session_filter = f;
            cx.notify();
        },
    );

    let search_input = ws.ui.antigravity_session_search.clone();

    let refresh_btn = button_with_icon_l(
        "ag-sess-refresh-btn",
        REFRESH_SVG,
        i.t("刷新", "Refresh"),
        ButtonVariant::Secondary,
        &t,
        cx,
        |ws, _, _, cx| {
            let s = scan_antigravity_sessions(&ws.paths.home, 200);
            ws.ui.antigravity_sessions = Some(s);
            ws.ui.toast(ws.i18n.t("已刷新会话列表", "Refreshed session list").to_string(), false);
            cx.notify();
        },
    );

    let toolbar = div()
        .flex()
        .items_center()
        .justify_between()
        .gap(px(10.0))
        .flex_wrap()
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(10.0))
                .flex_wrap()
                .child(filter_selector)
                .child(div().w(px(280.0)).child(input_container(&t, search_input)))
                .child(refresh_btn)
        )
        .child(
            div()
                .text_size(px(12.0))
                .text_color(t.text_muted)
                .child(format!("{}: {}", i.t("共计", "Total"), total_count))
        );

    // List of sessions
    if filtered.is_empty() {
        return div()
            .flex()
            .flex_col()
            .w_full()
            .gap(px(12.0))
            .child(toolbar)
            .child(
                empty_state_svg(
                    &t,
                    HISTORY_SVG,
                    i.t("暂无 Antigravity 会话记录", "No Antigravity sessions found"),
                    if query.is_empty() {
                        i.t("在终端中使用 agy，或在 Antigravity 桌面端中开始对话后，历史会话将自动显示在此处",
                            "Conversations started in agy CLI or Antigravity IDE will automatically appear here.")
                    } else {
                        i.t("没有找到匹配的会话，请尝试更换关键词", "No matching sessions found, try a different keyword.")
                    }
                )
            )
            .into_any_element();
    }

    let filtered_arc = std::sync::Arc::new(filtered);
    let items_len = filtered_arc.len();
    let ws_entity = cx.entity();
    let t_clone = t.clone();
    let i_clone = i;
    let open_id = ws.ui.antigravity_open_session.as_ref().map(|s| s.session_id.clone());

    let v_list = uniform_list(
        "antigravity-sessions-vlist",
        items_len,
        move |range: std::ops::Range<usize>, _window: &mut gpui::Window, _cx: &mut gpui::App| -> Vec<gpui::AnyElement> {
            let mut elements = Vec::with_capacity(range.len());
            for idx in range {
                if let Some(sess) = filtered_arc.get(idx) {
                    let is_open = open_id.as_deref() == Some(&sess.session_id);
                    elements.push(render_virtual_antigravity_session_row(
                        sess,
                        is_open,
                        &ws_entity,
                        &t_clone,
                        &i_clone,
                    ));
                }
            }
            elements
        },
    )
    .track_scroll(&ws.ui.antigravity_session_scroll_handle)
    .size_full();

    div()
        .flex()
        .flex_col()
        .w_full()
        .h_full()
        .min_h(px(0.0))
        .gap(px(12.0))
        .child(toolbar)
        .child(
            div()
                .w_full()
                .flex_1()
                .h_full()
                .min_h(px(0.0))
                .overflow_hidden()
                .child(v_list),
        )
        .into_any_element()
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

fn render_virtual_antigravity_session_row(
    session: &AntigravitySessionMeta,
    is_open: bool,
    ws_entity: &gpui::Entity<Workspace>,
    t: &Theme,
    i: &crate::i18n::I18n,
) -> gpui::AnyElement {
    let sid = session.session_id.clone();
    let display_title = if session.title.trim().is_empty() {
        if !session.preview.trim().is_empty() {
            session.preview.clone()
        } else {
            session.session_id.clone()
        }
    } else {
        session.title.clone()
    };

    let display_time = session.last_active_at
        .and_then(|ms| chrono::DateTime::from_timestamp_millis(ms))
        .map(|dt| {
            let local: chrono::DateTime<chrono::Local> = chrono::DateTime::from(dt);
            local.format("%Y-%m-%d %H:%M").to_string()
        })
        .unwrap_or_else(|| "—".into());

    let short_hash = short_session_id(&sid);

    let (badge_text, badge_bg, badge_border, badge_text_color) = if session.source == "cli" {
        ("CLI", t.accent.opacity(0.12), t.accent.opacity(0.3), t.accent)
    } else {
        ("App", t.sidebar_bg, t.card_border, t.text_secondary)
    };

    let source_badge = div()
        .flex()
        .items_center()
        .justify_center()
        .h(px(18.0))
        .px(px(6.0))
        .rounded(px(4.0))
        .bg(badge_bg)
        .border_1()
        .border_color(badge_border)
        .text_size(px(11.0))
        .font_weight(gpui::FontWeight::MEDIUM)
        .text_color(badge_text_color)
        .flex_shrink_0()
        .child(badge_text);

    let mut meta_row = div()
        .flex()
        .items_center()
        .gap(px(12.0))
        .text_size(px(12.0))
        .text_color(t.text_secondary)
        .overflow_hidden()
        .whitespace_nowrap();

    // Time with Clock icon
    meta_row = meta_row.child(
        div()
            .flex()
            .items_center()
            .gap(px(4.0))
            .flex_shrink_0()
            .child(crate::icons::svg_icon(CLOCK_SVG, px(12.0), t.text_muted))
            .child(display_time),
    );

    // Hash (short session id)
    meta_row = meta_row.child(
        div()
            .flex()
            .items_center()
            .gap(px(2.0))
            .flex_shrink_0()
            .text_color(t.text_muted)
            .child(short_hash),
    );

    // Project Directory (if available)
    if let Some(dir) = &session.project_dir {
        meta_row = meta_row.child(
            div()
                .flex()
                .items_center()
                .gap(px(4.0))
                .min_w(px(0.0))
                .overflow_hidden()
                .text_ellipsis()
                .text_color(t.text_muted)
                .child(crate::icons::svg_icon(FOLDER_SVG, px(12.0), t.text_muted))
                .child(
                    div()
                        .overflow_hidden()
                        .text_ellipsis()
                        .child(dir.clone()),
                ),
        );
    }

    let left_info = div()
        .flex()
        .flex_col()
        .gap(px(4.0))
        .flex_1()
        .min_w(px(0.0))
        .overflow_hidden()
        // Row 1: Source badge + Title
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(8.0))
                .w_full()
                .min_w(px(0.0))
                .overflow_hidden()
                .child(source_badge)
                .child(
                    div()
                        .text_size(px(13.5))
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .text_color(t.text_primary)
                        .overflow_hidden()
                        .text_ellipsis()
                        .whitespace_nowrap()
                        .child(display_title),
                ),
        )
        // Row 2: Meta
        .child(meta_row);

    // Right actions
    let mut right_actions = div()
        .flex()
        .items_center()
        .gap(px(8.0))
        .flex_shrink_0();

    if let Some(ref cmd) = session.resume_command {
        let ws_entity = ws_entity.clone();
        let cmd_to_copy = cmd.clone();
        right_actions = right_actions.child(
            div()
                .id(gpui::SharedString::from(format!("ag-resume-{}", session.session_id)))
                .cursor_pointer()
                .flex()
                .items_center()
                .gap(px(4.0))
                .h(px(28.0))
                .px(px(10.0))
                .rounded(px(6.0))
                .bg(t.sidebar_bg)
                .border_1()
                .border_color(t.card_border)
                .text_size(px(12.0))
                .text_color(t.text_secondary)
                .hover({
                    let bg = t.card_hover;
                    let border = t.card_border_hover;
                    let text = t.text_primary;
                    move |h| h.bg(bg).border_color(border).text_color(text)
                })
                .child(crate::icons::svg_icon(TERMINAL_SVG, px(12.0), t.text_muted))
                .child(i.t("恢复命令", "Resume"))
                .on_click(move |_ev, _win, cx| {
                    cx.stop_propagation();
                    let cmd_str = cmd_to_copy.clone();
                    let _ = ws_entity.update(cx, |ws, cx| {
                        cx.write_to_clipboard(gpui::ClipboardItem::new_string(cmd_str.clone()));
                        ws.ui.toast(format!("已复制命令: {}", cmd_str), false);
                        cx.notify();
                    });
                }),
        );
    }

    let ws_entity_del = ws_entity.clone();
    let sess_for_del = session.clone();
    right_actions = right_actions.child(
        div()
            .id(gpui::SharedString::from(format!("ag-del-{}", session.session_id)))
            .cursor_pointer()
            .flex()
            .items_center()
            .gap(px(4.0))
            .h(px(28.0))
            .px(px(10.0))
            .rounded(px(6.0))
            .bg(t.sidebar_bg)
            .border_1()
            .border_color(t.card_border)
            .text_size(px(12.0))
            .text_color(t.danger)
            .hover({
                let bg = t.danger_subtle;
                let border = t.danger;
                move |h| h.bg(bg).border_color(border)
            })
            .child(crate::icons::svg_icon(TRASH_SVG, px(12.0), t.danger))
            .child(i.t("删除", "Delete"))
            .on_click(move |_ev, _win, cx| {
                cx.stop_propagation();
                let sess_del = sess_for_del.clone();
                let _ = ws_entity_del.update(cx, |ws, cx| {
                    let msg = format!("确定要删除此会话记录 ({}) 吗？磁盘上的相关数据将被永久移除。", sess_del.session_id);
                    ws.ui.confirm = Some(ConfirmState {
                        title: ws.i18n.t("删除 Antigravity 会话", "Delete Antigravity Session").to_string(),
                        message: msg,
                        action: ConfirmAction::DeleteAntigravitySession { session: sess_del },
                    });
                    cx.notify();
                });
            }),
    );

    let sess_to_open = session.clone();
    let ws_entity_card = ws_entity.clone();

    let card = div()
        .id(gpui::SharedString::from(format!("ag-v-sess-{}", sid)))
        .flex()
        .items_center()
        .justify_between()
        .w_full()
        .h_full()
        .min_w(px(0.0))
        .gap(px(12.0))
        .px(px(16.0))
        .py(px(8.0))
        .rounded(px(10.0))
        .bg(t.card_bg)
        .border_1()
        .border_color(if is_open { t.accent } else { t.card_border })
        .shadow_xs()
        .cursor_pointer()
        .hover({
            let bg = t.card_hover;
            let border = if is_open { t.accent } else { t.card_border_hover };
            move |h| h.bg(bg).border_color(border)
        })
        .on_click(move |_ev, _win, cx| {
            let sess = sess_to_open.clone();
            let _ = ws_entity_card.update(cx, |ws, cx| {
                ws.ui.antigravity_open_session = Some(sess);
                cx.notify();
            });
        })
        .child(left_info)
        .child(right_actions);

    div()
        .h(px(66.0))
        .pb(px(6.0))
        .flex()
        .w_full()
        .child(card)
        .into_any_element()
}

fn render_antigravity_session_detail(
    session: &AntigravitySessionMeta,
    ws: &mut Workspace,
    cx: &mut Context<Workspace>,
) -> gpui::AnyElement {
    let t = ws.theme.clone();
    let i = ws.i18n;
    let source_path_buf = std::path::PathBuf::from(&session.source_path);

    let session_meta: aitoolplus_core::session::SessionMeta = session.clone().into();
    let messages = aitoolplus_core::session::load_messages(&ws.paths, &session_meta).unwrap_or_default();

    let formatted_time = session.last_active_at
        .and_then(|ms| chrono::DateTime::from_timestamp_millis(ms))
        .map(|dt| {
            let local: chrono::DateTime<chrono::Local> = chrono::DateTime::from(dt);
            local.format("%Y-%m-%d %H:%M").to_string()
        })
        .unwrap_or_else(|| "—".into());

    let top_bar = div()
        .flex()
        .items_center()
        .justify_between()
        .gap(px(10.0))
        .p(px(8.0))
        .rounded(px(8.0))
        .bg(t.card_bg)
        .border_1()
        .border_color(t.card_border)
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(8.0))
                .flex_1()
                .min_w(px(0.0))
                .child(button_with_icon_l(
                    "ag-detail-back",
                    ARROW_LEFT_SVG,
                    i.t("返回列表", "Back"),
                    ButtonVariant::Secondary,
                    &t,
                    cx,
                    |ws, _, _, cx| {
                        ws.ui.antigravity_open_session = None;
                        cx.notify();
                    }
                ))
                .child(
                    if session.source == "cli" {
                        crate::components::badge(&t, "CLI", crate::components::BadgeKind::Accent)
                    } else {
                        crate::components::badge(&t, "App", crate::components::BadgeKind::Neutral)
                    }
                )
                .child(
                    div()
                        .text_size(px(14.0))
                        .font_weight(gpui::FontWeight::BOLD)
                        .text_color(t.text_primary)
                        .overflow_hidden()
                        .text_ellipsis()
                        .whitespace_nowrap()
                        .child(session.title.clone())
                )
                .child(crate::components::badge(&t, formatted_time, crate::components::BadgeKind::Neutral))
        )
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(6.0))
                .flex_shrink_0()
                .child(button_with_icon_l(
                    "ag-detail-reveal",
                    FOLDER_SVG,
                    i.t("定位文件", "Reveal File"),
                    ButtonVariant::Secondary,
                    &t,
                    cx,
                    move |_, _, _, _| {
                        super::open_path_in_default_manager(&source_path_buf);
                    }
                ))
        );

    let mut message_list = div()
        .flex()
        .flex_col()
        .w_full()
        .gap(px(12.0))
        .p(px(8.0));

    if messages.is_empty() {
        message_list = message_list.child(
            div()
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .p(px(40.0))
                .gap(px(8.0))
                .child(
                    div()
                        .text_size(px(13.0))
                        .text_color(t.text_secondary)
                        .child(if session.source == "app" {
                            i.t("此桌面 App 会话历史仅保存于本地 Protocol Buffers 二进制缓存中，无明文对话记录。",
                                "This Desktop App session history is stored in binary protocol buffers (.pb).")
                        } else {
                            i.t("此会话暂未记录明文消息。", "No text transcript messages recorded for this session.")
                        })
                )
        );
    } else {
        for (idx, msg) in messages.iter().enumerate() {
            let is_user = msg.role == "user";
            let role_label: gpui::SharedString = if is_user { i.t("用户", "User") } else { "Antigravity".into() };
            let copy_text = msg.content.clone();

            let mut msg_box = div()
                .id(gpui::SharedString::from(format!("ag-msg-{}", idx)))
                .flex()
                .flex_col()
                .w_full()
                .gap(px(6.0))
                .p(px(12.0))
                .rounded(px(8.0))
                .border_1();

            if is_user {
                msg_box = msg_box
                    .bg(t.card_bg)
                    .border_color(t.accent.opacity(0.3));
            } else {
                msg_box = msg_box
                    .bg(t.sidebar_bg)
                    .border_color(t.card_border);
            }

            // Header
            let header = div()
                .flex()
                .items_center()
                .justify_between()
                .w_full()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(6.0))
                        .child(
                            div()
                                .text_size(px(12.0))
                                .font_weight(gpui::FontWeight::BOLD)
                                .text_color(if is_user { t.accent } else { t.text_primary })
                                .child(role_label)
                        )
                )
                .child(
                    button_with_icon_l(
                        gpui::SharedString::from(format!("ag-cp-msg-{}", idx)),
                        COPY_SVG,
                        i.t("复制", "Copy"),
                        ButtonVariant::Ghost,
                        &t,
                        cx,
                        move |ws, _, _, cx| {
                            cx.write_to_clipboard(gpui::ClipboardItem::new_string(copy_text.clone()));
                            ws.ui.toast(ws.i18n.t("已复制内容", "Copied").to_string(), false);
                            cx.notify();
                        }
                    )
                );

            msg_box = msg_box.child(header);

            // Blocks or content
            for block in msg.blocks.iter() {
                if block.kind == "thinking" {
                    if let Some(ref text) = block.text {
                        msg_box = msg_box.child(
                            div()
                                .p(px(8.0))
                                .rounded(px(6.0))
                                .bg(t.card_bg)
                                .border_1()
                                .border_color(t.card_border)
                                .text_size(px(12.0))
                                .text_color(t.text_muted)
                                .child(format!("💭 思考过程:\n{}", text))
                        );
                    }
                } else if block.kind == "tool_call" || block.kind == "command" {
                    let tool_name = block.tool_name.as_deref().unwrap_or("tool");
                    let args = block.text.as_deref().unwrap_or("");
                    msg_box = msg_box.child(
                        div()
                            .p(px(6.0))
                            .rounded(px(6.0))
                            .bg(t.card_bg)
                            .border_1()
                            .border_color(t.card_border)
                            .text_size(px(12.0))
                            .text_color(t.text_secondary)
                            .child(format!("🔧 调用工具 {}: {}", tool_name, args))
                    );
                } else if let Some(ref text) = block.text {
                    msg_box = msg_box.child(
                        div()
                            .text_size(px(13.0))
                            .text_color(t.text_primary)
                            .child(text.clone())
                    );
                }
            }

            if msg.blocks.is_empty() && !msg.content.is_empty() {
                msg_box = msg_box.child(
                    div()
                        .text_size(px(13.0))
                        .text_color(t.text_primary)
                        .child(msg.content.clone())
                );
            }

            message_list = message_list.child(msg_box);
        }
    }

    div()
        .flex()
        .flex_col()
        .w_full()
        .h_full()
        .min_h(px(0.0))
        .gap(px(10.0))
        .child(top_bar)
        .child(message_list)
        .into_any_element()
}

/// 5-Hour rolling quota item with weekly exhaustion constraint awareness.
#[derive(Clone, Debug)]
pub struct CoreQuotaItem {
    pub percentage: i32,
    pub reset_time: String,
    pub is_weekly_constrained: bool,
    pub weekly_reset_time: String,
}

/// 7-Day weekly quota item.
#[derive(Clone, Debug)]
pub struct WeeklyQuotaItem {
    pub percentage: i32,
    pub reset_time: String,
}

/// Combined 5h + weekly quota for a model family (Gemini or Claude/GPT).
#[derive(Clone, Debug, Default)]
pub struct ModelFamilyQuota {
    pub five_hour: Option<CoreQuotaItem>,
    pub weekly: Option<WeeklyQuotaItem>,
}

/// Extract dual quotas (5-hour and 7-day weekly) for both Gemini and Claude/GPT model families.
/// Inspects upstream Antigravity-Manager bucket conventions:
/// If weekly quota is exhausted (remaining_fraction <= 0.001 and reset_time > now),
/// the 5-hour quota is marked `is_weekly_constrained = true`, forcing percentage to 0% and
/// displaying a deepened red border warning in the UI.
pub fn extract_family_quotas(account: &AntigravityAccount) -> (ModelFamilyQuota, ModelFamilyQuota) {
    let quota = match &account.quota {
        Some(q) => q,
        None => return (ModelFamilyQuota::default(), ModelFamilyQuota::default()),
    };

    let find_model_reset_time = |keyword: &str| -> String {
        quota.models.iter()
            .find(|m| m.name.to_lowercase().contains(keyword) && !m.reset_time.trim().is_empty())
            .map(|m| m.reset_time.clone())
            .unwrap_or_default()
    };

    let is_future_reset = |reset_time: &str| -> bool {
        if let Some(dt) = parse_flexible_date(reset_time) {
            dt > Utc::now()
        } else {
            !reset_time.trim().is_empty()
        }
    };

    let inspect_family = |is_third_party: bool| -> ModelFamilyQuota {
        let matching_groups: Vec<_> = quota.quota_groups.iter().filter(|g| {
            let d = g.display_name.to_lowercase();
            let bid = g.bucket_id.as_deref().unwrap_or("").to_lowercase();
            let is_3p = d.contains("claude") || d.contains("gpt") || d.contains("3p") || bid.contains("3p");
            if is_third_party {
                is_3p
            } else {
                !is_3p && (d.contains("gemini") || (!d.contains("claude") && !d.contains("gpt")))
            }
        }).collect();

        // 1. Weekly bucket: /week|7d/
        let weekly_bucket = matching_groups.iter().filter(|g| {
            let win = g.window.to_lowercase();
            let bid = g.bucket_id.as_deref().unwrap_or("").to_lowercase();
            win.contains("week") || bid.contains("week") || win.contains("7d") || bid.contains("7d")
        }).max_by(|a, b| {
            a.reset_time.cmp(&b.reset_time)
        });

        let weekly_item = weekly_bucket.map(|g| {
            let mut rt = g.reset_time.clone();
            if rt.trim().is_empty() {
                rt = find_model_reset_time(if is_third_party { "claude" } else { "gemini" });
            }
            WeeklyQuotaItem {
                percentage: (g.remaining_fraction * 100.0).round() as i32,
                reset_time: rt,
            }
        }).or_else(|| {
            if !is_third_party {
                quota.window_weekly.map(|w| WeeklyQuotaItem {
                    percentage: (w * 100.0).round() as i32,
                    reset_time: find_model_reset_time("gemini"),
                })
            } else {
                None
            }
        });

        // Upstream Antigravity-Manager logic:
        // const weekly = buckets.filter(bucket => /week|7d/i.test(`${bucket.window} ${bucket.bucket_id}`)
        //     && bucket.remaining_fraction <= 0.001 && Date.parse(bucket.reset_time) > Date.now())
        let is_weekly_constrained = if let Some(ref w_bucket) = weekly_bucket {
            w_bucket.remaining_fraction <= 0.001 && is_future_reset(&w_bucket.reset_time)
        } else if !is_third_party && quota.window_weekly.map_or(false, |w| w <= 0.001) {
            true
        } else {
            false
        };

        let weekly_reset_time = weekly_bucket.map(|b| b.reset_time.clone())
            .unwrap_or_else(|| weekly_item.as_ref().map(|w| w.reset_time.clone()).unwrap_or_default());

        // 2. 5-Hour bucket: /5h|hour/
        let five_hour_bucket = matching_groups.iter().filter(|g| {
            let win = g.window.to_lowercase();
            let bid = g.bucket_id.as_deref().unwrap_or("").to_lowercase();
            win.contains("5h") || bid.contains("5h") || win.contains("hour") || bid.contains("hour")
        }).min_by(|a, b| {
            a.remaining_fraction.partial_cmp(&b.remaining_fraction).unwrap_or(std::cmp::Ordering::Equal)
        });

        let five_hour_item = if let Some(b) = five_hour_bucket {
            let mut rt = b.reset_time.clone();
            if rt.trim().is_empty() {
                rt = find_model_reset_time(if is_third_party { "claude" } else { "gemini" });
            }
            let raw_pct = (b.remaining_fraction * 100.0).round() as i32;
            let final_pct = if is_weekly_constrained { 0 } else { raw_pct };
            Some(CoreQuotaItem {
                percentage: final_pct,
                reset_time: if is_weekly_constrained && !weekly_reset_time.is_empty() { weekly_reset_time.clone() } else { rt },
                is_weekly_constrained,
                weekly_reset_time: weekly_reset_time.clone(),
            })
        } else if !is_third_party && quota.window_5h.is_some() {
            let raw_pct = (quota.window_5h.unwrap() * 100.0).round() as i32;
            let final_pct = if is_weekly_constrained { 0 } else { raw_pct };
            Some(CoreQuotaItem {
                percentage: final_pct,
                reset_time: if is_weekly_constrained && !weekly_reset_time.is_empty() { weekly_reset_time.clone() } else { find_model_reset_time("gemini") },
                is_weekly_constrained,
                weekly_reset_time: weekly_reset_time.clone(),
            })
        } else {
            let model_fallback = quota.models.iter().find(|m| {
                let n = m.name.to_lowercase();
                if is_third_party {
                    n.contains("claude") || n.contains("gpt")
                } else {
                    n.contains("gemini")
                }
            });
            model_fallback.map(|m| {
                let final_pct = if is_weekly_constrained { 0 } else { m.percentage };
                CoreQuotaItem {
                    percentage: final_pct,
                    reset_time: if is_weekly_constrained && !weekly_reset_time.is_empty() { weekly_reset_time.clone() } else { m.reset_time.clone() },
                    is_weekly_constrained,
                    weekly_reset_time: weekly_reset_time.clone(),
                }
            })
        };

        ModelFamilyQuota {
            five_hour: five_hour_item,
            weekly: weekly_item,
        }
    };

    let gemini = inspect_family(false);
    let claude = inspect_family(true);
    (gemini, claude)
}

/// Parse flexible date strings (RFC3339, standard date formats, Unix timestamps).
pub fn parse_flexible_date(s: &str) -> Option<DateTime<Utc>> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }

    // 1. Try RFC3339 / ISO-8601 (e.g. 2026-09-20T14:23:38Z)
    if let Ok(dt) = DateTime::parse_from_rfc3339(s) {
        return Some(dt.with_timezone(&Utc));
    }

    // 2. Try pure integer Unix timestamp (seconds or milliseconds)
    if let Ok(ts) = s.parse::<i64>() {
        let secs = if ts > 10_000_000_000 { ts / 1000 } else { ts };
        return DateTime::from_timestamp(secs, 0);
    }

    // 3. Try common date-time strings
    let naive_formats = [
        "%m/%d/%Y %H:%M:%S",
        "%m/%d/%Y %I:%M:%S %p",
        "%Y-%m-%d %H:%M:%S",
        "%Y/%m/%d %H:%M:%S",
        "%Y-%m-%dT%H:%M:%S",
        "%Y-%m-%dT%H:%M:%SZ",
        "%d/%m/%Y %H:%M:%S",
    ];
    for fmt in naive_formats {
        if let Ok(ndt) = NaiveDateTime::parse_from_str(s, fmt) {
            return Some(ndt.and_utc());
        }
    }

    None
}

/// Format reset time into remaining duration (e.g. "3h 24m", "2d 5h", "已重置").
pub fn format_time_remaining(reset_time: &str) -> (String, &'static str) {
    let target_date = match parse_flexible_date(reset_time) {
        Some(dt) => dt,
        None => return ("—".to_string(), "neutral"),
    };

    let now = Utc::now();
    let diff = target_date.signed_duration_since(now);
    let total_secs = diff.num_seconds();
    if total_secs <= 0 {
        return ("已重置".to_string(), "success");
    }

    let diff_hrs = diff.num_hours();
    let diff_mins = diff.num_minutes() % 60;
    if diff_hrs >= 24 {
        let days = diff_hrs / 24;
        let rem_hrs = diff_hrs % 24;
        (format!("{}d {}h", days, rem_hrs), "neutral")
    } else {
        let color = if diff_hrs < 1 {
            "success"
        } else if diff_hrs < 6 {
            "warning"
        } else {
            "neutral"
        };
        (format!("{}h {}m", diff_hrs, diff_mins), color)
    }
}

/// Render the 5-hour quota bar for a model family.
/// If weekly quota is 0% (is_weekly_constrained), renders a deepened red border, red wash,
/// AlertTriangle warning icon, and "周配额 0%" status pill matching Antigravity-Manager parity.
fn render_five_hour_quota_bar(
    title: &'static str,
    icon_svg: &'static [u8],
    item: Option<&CoreQuotaItem>,
    t: &Theme,
    i: &crate::i18n::I18n,
) -> gpui::AnyElement {
    if let Some(item) = item {
        if item.is_weekly_constrained {
            let (weekly_countdown, _) = format_time_remaining(&item.weekly_reset_time);
            let weekly_reset_tip = if !item.weekly_reset_time.is_empty() {
                format!("，{}后重置", weekly_countdown)
            } else {
                String::new()
            };
            let tooltip_text = format!("{} (0%){}", i.t("周配额已耗尽", "Weekly quota exhausted"), weekly_reset_tip);

            return div()
                .id(SharedString::from(format!("ag-constrained-{}", title)))
                .relative()
                .h(px(25.0))
                .w_full()
                .flex()
                .items_center()
                .px(px(7.0))
                .rounded(px(6.0))
                .bg(crate::rgba_const(0xf43f5e18))
                .border_1()
                .border_color(crate::rgba_const(0xf43f5eff))
                .overflow_hidden()
                .tooltip(move |_window, cx| cx.new(|_| Tooltip::new(tooltip_text.clone())).into())
                .child(
                    div()
                        .relative()
                        .w_full()
                        .flex()
                        .items_center()
                        .justify_between()
                        .gap(px(4.0))
                        .text_size(px(10.5))
                        .font_family("Consolas, monospace")
                        // Left: Warning Alert + Model Icon + Title 5h
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(3.5))
                                .font_weight(gpui::FontWeight::BOLD)
                                .text_color(t.danger)
                                .child(gpui::svg().data(ALERT_SVG).size(px(11.0)).text_color(t.danger))
                                .child(gpui::svg().data(icon_svg).size(px(11.0)).text_color(t.danger))
                                .child(format!("{} 5h", title)),
                        )
                        // Middle: Weekly countdown in red
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(2.5))
                                .text_size(px(10.0))
                                .text_color(t.danger)
                                .child(gpui::svg().data(crate::icons::CLOCK_SVG).size(px(9.5)).text_color(t.danger))
                                .child(weekly_countdown),
                        )
                        // Right: Badge "周配额 0%" + "0%"
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(4.0))
                                .child(
                                    div()
                                        .px(px(4.0))
                                        .py(px(0.5))
                                        .rounded(px(3.0))
                                        .bg(crate::rgba_const(0xf43f5e33))
                                        .text_size(px(9.0))
                                        .font_weight(gpui::FontWeight::BOLD)
                                        .text_color(t.danger)
                                        .child(i.t("周配额 0%", "Weekly 0%")),
                                )
                                .child(
                                    div()
                                        .font_weight(gpui::FontWeight::BOLD)
                                        .text_color(t.danger)
                                        .child("0%"),
                                ),
                        ),
                )
                .into_any_element();
        }

        // Normal 5-hour quota
        let pct = item.percentage.clamp(0, 100);
        let (countdown, time_kind) = format_time_remaining(&item.reset_time);
        let bar_color = if pct >= 50 {
            t.success
        } else if pct >= 20 {
            t.warning
        } else {
            t.danger
        };
        let time_color = match time_kind {
            "success" => t.success,
            "warning" => t.warning,
            _ => t.text_muted,
        };

        div()
            .relative()
            .h(px(25.0))
            .w_full()
            .flex()
            .items_center()
            .px(px(7.0))
            .rounded(px(6.0))
            .bg(t.input_bg)
            .border_1()
            .border_color(t.card_border)
            .overflow_hidden()
            .child(
                div()
                    .absolute()
                    .left_0()
                    .top_0()
                    .bottom_0()
                    .w(gpui::relative(pct as f32 / 100.0))
                    .bg(bar_color)
                    .opacity(0.18),
            )
            .child(
                div()
                    .relative()
                    .w_full()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(4.0))
                    .text_size(px(10.5))
                    .font_family("Consolas, monospace")
                    // Left: Model Icon + Title 5h
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(4.0))
                            .font_weight(gpui::FontWeight::BOLD)
                            .text_color(t.text_primary)
                            .child(gpui::svg().data(icon_svg).size(px(11.5)).text_color(t.accent))
                            .child(format!("{} 5h", title)),
                    )
                    // Middle: Clock + Countdown
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(2.5))
                            .text_size(px(10.0))
                            .text_color(time_color)
                            .child(gpui::svg().data(crate::icons::CLOCK_SVG).size(px(9.5)).text_color(time_color))
                            .child(countdown),
                    )
                    // Right: Percentage
                    .child(
                        div()
                            .font_weight(gpui::FontWeight::BOLD)
                            .text_color(bar_color)
                            .child(format!("{}%", pct)),
                    ),
            )
            .into_any_element()
    } else {
        render_quota_bar_placeholder(format!("{} 5h", title), icon_svg, t)
    }
}

/// Render the 7-day weekly quota bar for a model family.
fn render_weekly_quota_bar(
    title: &'static str,
    icon_svg: &'static [u8],
    item: Option<&WeeklyQuotaItem>,
    t: &Theme,
    i: &crate::i18n::I18n,
) -> gpui::AnyElement {
    if let Some(item) = item {
        let pct = item.percentage.clamp(0, 100);
        let (countdown, time_kind) = format_time_remaining(&item.reset_time);
        let bar_color = if pct >= 50 {
            t.success
        } else if pct >= 20 {
            t.warning
        } else {
            t.danger
        };
        let time_color = match time_kind {
            "success" => t.success,
            "warning" => t.warning,
            _ => t.text_muted,
        };

        div()
            .relative()
            .h(px(25.0))
            .w_full()
            .flex()
            .items_center()
            .px(px(7.0))
            .rounded(px(6.0))
            .bg(t.input_bg)
            .border_1()
            .border_color(if pct == 0 { t.danger.opacity(0.4) } else { t.card_border })
            .overflow_hidden()
            .child(
                div()
                    .absolute()
                    .left_0()
                    .top_0()
                    .bottom_0()
                    .w(gpui::relative(pct as f32 / 100.0))
                    .bg(bar_color)
                    .opacity(0.18),
            )
            .child(
                div()
                    .relative()
                    .w_full()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(4.0))
                    .text_size(px(10.5))
                    .font_family("Consolas, monospace")
                    // Left: Model Icon + Title 周配额
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(4.0))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(t.text_secondary)
                            .child(gpui::svg().data(icon_svg).size(px(11.0)).text_color(t.text_muted))
                            .child(i.t(&format!("{} 周配额", title), &format!("{} Weekly", title))),
                    )
                    // Middle: Clock + Countdown
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(2.5))
                            .text_size(px(10.0))
                            .text_color(time_color)
                            .child(gpui::svg().data(crate::icons::CLOCK_SVG).size(px(9.5)).text_color(time_color))
                            .child(countdown),
                    )
                    // Right: Percentage
                    .child(
                        div()
                            .font_weight(gpui::FontWeight::BOLD)
                            .text_color(bar_color)
                            .child(format!("{}%", pct)),
                    ),
            )
            .into_any_element()
    } else {
        render_quota_bar_placeholder(i.t(&format!("{} 周配额", title), &format!("{} Weekly", title)).to_string(), icon_svg, t)
    }
}

/// Subtle placeholder bar when a quota bucket is not applicable or unconfigured.
fn render_quota_bar_placeholder(
    label: impl Into<SharedString>,
    icon_svg: &'static [u8],
    t: &Theme,
) -> gpui::AnyElement {
    div()
        .relative()
        .h(px(25.0))
        .w_full()
        .flex()
        .items_center()
        .justify_between()
        .px(px(7.0))
        .rounded(px(6.0))
        .bg(t.input_bg.opacity(0.45))
        .border_1()
        .border_color(t.card_border.opacity(0.6))
        .text_size(px(10.5))
        .font_family("Consolas, monospace")
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(4.0))
                .text_color(t.text_muted)
                .child(gpui::svg().data(icon_svg).size(px(11.0)).text_color(t.text_muted))
                .child(label.into()),
        )
        .child(
            div()
                .text_color(t.text_muted)
                .child("—"),
        )
        .into_any_element()
}

/// Render a column combining 5-hour rolling quota and 7-day weekly quota for a model family.
fn render_model_quota_column(
    title: &'static str,
    icon_svg: &'static [u8],
    family: &ModelFamilyQuota,
    theme: &Theme,
    i18n: &crate::i18n::I18n,
) -> gpui::AnyElement {
    let t = theme;
    let i = i18n;

    div()
        .flex_1()
        .min_w(px(200.0))
        .flex()
        .flex_col()
        .gap(px(4.0))
        .child(render_five_hour_quota_bar(title, icon_svg, family.five_hour.as_ref(), t, i))
        .child(render_weekly_quota_bar(title, icon_svg, family.weekly.as_ref(), t, i))
        .into_any_element()
}

/// Compact action icon button for the third column of the account card.
fn render_action_icon_btn<F>(
    id: impl Into<gpui::ElementId>,
    svg_data: &'static [u8],
    tooltip: impl Into<SharedString>,
    danger: bool,
    t: &Theme,
    cx: &mut Context<Workspace>,
    on_click: F,
) -> gpui::AnyElement
where
    F: Fn(&mut Workspace, &gpui::ClickEvent, &mut gpui::Window, &mut Context<Workspace>) + 'static,
{
    let tc_danger = t.clone();
    let tc_normal = t.clone();
    let tooltip_str: SharedString = tooltip.into();
    let fg = if danger { t.danger } else { t.text_secondary };
    div()
        .id(id.into())
        .cursor_pointer()
        .size(px(25.0))
        .rounded(px(5.0))
        .bg(t.input_bg.opacity(0.6))
        .border_1()
        .border_color(t.card_border)
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .text_color(fg)
        .tooltip(move |_window, cx| cx.new(|_| Tooltip::new(tooltip_str.clone())).into())
        .when(danger, move |this| {
            this.hover(move |h| h.bg(tc_danger.danger_subtle).border_color(tc_danger.danger).text_color(tc_danger.danger))
                .active(|a| a.opacity(0.8))
        })
        .when(!danger, move |this| {
            this.hover(move |h| h.bg(tc_normal.card_hover).border_color(tc_normal.card_border).text_color(tc_normal.text_primary))
                .active(move |a| a.bg(tc_normal.row_hover))
        })
        .child(
            gpui::svg()
                .data(svg_data)
                .size(px(13.5))
                .text_color(fg),
        )
        .on_click(cx.listener(move |ws, ev, window, cx| {
            on_click(ws, ev, window, cx);
        }))
        .into_any_element()
}

/// Render a distinctive subscription tier badge (e.g. emerald PRO with diamond, violet ULTRA, or subtle FREE).
pub fn render_tier_badge(theme: &Theme, tier_str: &str) -> gpui::AnyElement {
    let t = theme;
    match tier_str {
        "PRO" => div()
            .flex()
            .items_center()
            .gap(px(3.0))
            .px(px(6.5))
            .py(px(1.5))
            .rounded(px(4.0))
            .bg(gpui::rgb(0x10b981))
            .text_size(px(10.5))
            .font_weight(gpui::FontWeight::BOLD)
            .text_color(gpui::rgb(0xffffff))
            .shadow_xs()
            .child("◆ PRO")
            .into_any_element(),
        "ULTRA" => div()
            .flex()
            .items_center()
            .gap(px(3.0))
            .px(px(6.5))
            .py(px(1.5))
            .rounded(px(4.0))
            .bg(gpui::rgb(0x8b5cf6))
            .text_size(px(10.5))
            .font_weight(gpui::FontWeight::BOLD)
            .text_color(gpui::rgb(0xffffff))
            .shadow_xs()
            .child("★ ULTRA")
            .into_any_element(),
        "ENTERPRISE" => div()
            .flex()
            .items_center()
            .gap(px(3.0))
            .px(px(6.5))
            .py(px(1.5))
            .rounded(px(4.0))
            .bg(gpui::rgb(0x6366f1))
            .text_size(px(10.5))
            .font_weight(gpui::FontWeight::BOLD)
            .text_color(gpui::rgb(0xffffff))
            .shadow_xs()
            .child("🏢 ENTERPRISE")
            .into_any_element(),
        _ => div()
            .flex()
            .items_center()
            .gap(px(3.0))
            .px(px(6.0))
            .py(px(1.5))
            .rounded(px(4.0))
            .bg(t.tab_bar_bg)
            .border_1()
            .border_color(t.card_border)
            .text_size(px(10.5))
            .font_weight(gpui::FontWeight::MEDIUM)
            .text_color(t.text_muted)
            .child("○ FREE")
            .into_any_element(),
    }
}

/// Render an individual account card with credentials, tier, two-usage meters, and action buttons.
fn render_account_card(
    account: &AntigravityAccount,
    ws: &mut Workspace,
    cx: &mut Context<Workspace>,
) -> gpui::AnyElement {
    let t = ws.theme.clone();
    let i = ws.i18n;
    let acc_id = account.id.clone();
    let email = account.email.clone();
    let is_active = account.is_active;
    let is_disabled = account.disabled;

    let last_used_str = DateTime::from_timestamp(account.last_used, 0)
        .map(|dt| dt.with_timezone(&chrono::Local).format("%Y-%m-%d %H:%M").to_string())
        .unwrap_or_default();

    // 1. Header row: Email + Badges + Project ID + Last used (Compact, no big avatar)
    let header_row = div()
        .flex()
        .items_center()
        .justify_between()
        .gap(px(8.0))
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(6.0))
                .flex_wrap()
                .child(
                    div()
                        .text_size(px(13.0))
                        .font_weight(gpui::FontWeight::BOLD)
                        .text_color(if is_active { t.accent } else if is_disabled { t.text_muted } else { t.text_primary })
                        .child(mask_email(&account.email)),
                )
                .child(render_tier_badge(&t, account.get_tier()))
                .when(is_active, |s| {
                    s.child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(4.5))
                            .px(px(7.0))
                            .py(px(2.0))
                            .rounded(px(10.0))
                            .bg(t.accent_subtle)
                            .border_1()
                            .border_color(t.accent.opacity(0.35))
                            .text_size(px(11.0))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(t.accent)
                            .child(
                                div()
                                    .size(px(5.5))
                                    .rounded_full()
                                    .bg(t.accent),
                            )
                            .child(i.t("当前活动", "Active")),
                    )
                })
                .when(is_disabled, |s| {
                    let disabled_reason = account.disabled_reason.clone();
                    let d_acc_id = acc_id.clone();
                    let mut badge = div()
                        .id(SharedString::from(format!("ag-disabled-badge-{}", d_acc_id)))
                        .flex()
                        .items_center()
                        .gap(px(4.5))
                        .px(px(7.0))
                        .py(px(2.0))
                        .rounded(px(10.0))
                        .bg(t.danger_subtle)
                        .border_1()
                        .border_color(t.danger.opacity(0.35))
                        .text_size(px(11.0))
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .text_color(t.danger)
                        .child(
                            div()
                                .size(px(5.5))
                                .rounded_full()
                                .bg(t.danger),
                        )
                        .child(i.t("已禁用", "Disabled"));

                    if let Some(reason) = disabled_reason {
                        badge = badge.tooltip(move |_window, cx| {
                            cx.new(|_| Tooltip::with_max_width(format!("禁用原因: {reason}"), px(320.0))).into()
                        });
                    }

                    s.child(badge)
                })
                .when_some(account.custom_label.as_ref(), |s, label| {
                    let trimmed = label.trim();
                    if trimmed.is_empty()
                        || trimmed.contains("Antigravity Manager")
                        || trimmed.contains("Manager")
                        || trimmed.contains("已从")
                        || trimmed.contains("迁移")
                        || trimmed.contains("导入")
                    {
                        return s;
                    }
                    s.child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(3.0))
                            .px(px(5.0))
                            .py(px(1.5))
                            .rounded(px(4.0))
                            .bg(t.warning_subtle)
                            .text_size(px(10.0))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(t.warning)
                            .child(gpui::svg().data(TAG_SVG).size(px(9.5)).text_color(t.warning))
                            .child(trimmed.to_string()),
                    )
                })
                .when_some(account.project_id.as_ref(), |s, pid| {
                    s.child(
                        div()
                            .text_size(px(11.0))
                            .text_color(t.text_muted)
                            .child(format!("• {}", pid)),
                    )
                }),
        )
        .child(
            div()
                .text_size(px(10.5))
                .font_family("Consolas, monospace")
                .text_color(t.text_muted)
                .child(last_used_str),
        );

    // 2. Below header row: 3-column layout (Column 1: Gemini, Column 2: Claude/GPT, Column 3: Actions)
    let account_for_details = account.clone();
    let account_for_device = account.clone();
    let account_id_for_label = acc_id.clone();
    let current_label = account.custom_label.clone();
    let r_token = account.refresh_token.clone();
    let account_for_export = account.clone();
    let del_id = acc_id.clone();
    let del_email = email.clone();

    let actions_column = div()
        .flex_shrink_0()
        .flex()
        .flex_col()
        .items_end()
        .gap(px(4.0))
        // Row 1: 6 buttons
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(2.0))
                .child(render_action_icon_btn(
                    format!("details-{}", acc_id),
                    INFO_SVG,
                    i.t("详情", "Details"),
                    false,
                    &t,
                    cx,
                    move |ws, _, _, cx| {
                        ws.ui.antigravity_details_account = Some(account_for_details.clone());
                        cx.notify();
                    },
                ))
                .child(render_action_icon_btn(
                    format!("device-{}", acc_id),
                    FINGERPRINT_SVG,
                    i.t("设备指纹", "Device Fingerprint"),
                    false,
                    &t,
                    cx,
                    move |ws, _, _, cx| {
                        let prof = account_for_device.device_profile.clone().unwrap_or_else(DeviceProfile::generate_random);
                        ws.ui.antigravity_device_account = Some((account_for_device.clone(), prof));
                        cx.notify();
                    },
                ))
                .child(render_action_icon_btn(
                    format!("tag-{}", acc_id),
                    TAG_SVG,
                    i.t("编辑标签", "Edit Label"),
                    false,
                    &t,
                    cx,
                    move |ws, _, window, cx| {
                        let initial = current_label.clone().unwrap_or_default();
                        let input = cx.new(|cx| {
                            let mut inp = TextInput::new("输入自定义备注/标签…", cx);
                            inp.set_text_silent(initial, cx);
                            inp
                        });
                        input.update(cx, |inp, cx| {
                            inp.focus_handle.focus(window, cx);
                            inp.start_blink(cx);
                        });
                        ws.ui.antigravity_editing_label = Some((account_id_for_label.clone(), input));
                        cx.notify();
                    },
                ))
                .child({
                    let ref_id = acc_id.clone();
                    render_action_icon_btn(
                        format!("refresh-{}", ref_id),
                        REFRESH_SVG,
                        i.t("刷新配额", "Refresh Quota"),
                        false,
                        &t,
                        cx,
                        move |ws, _, _, cx| {
                            refresh_account_quota_action(&ref_id, ws, cx);
                        },
                    )
                })
                .child(render_action_icon_btn(
                    format!("export-{}", acc_id),
                    DOWNLOAD_SVG,
                    i.t("导出凭据", "Export Credentials"),
                    false,
                    &t,
                    cx,
                    move |ws, _, _, cx| {
                        export_account_action(&account_for_export, ws, cx);
                    },
                ))
                .child({
                    let token_clone = r_token.clone();
                    render_action_icon_btn(
                        format!("copy-{}", acc_id),
                        COPY_SVG,
                        i.t("复制 Refresh Token", "Copy Refresh Token"),
                        false,
                        &t,
                        cx,
                        move |ws, _, _, cx| {
                            cx.write_to_clipboard(gpui::ClipboardItem::new_string(token_clone.clone()));
                            let msg = ws.i18n.t("已复制 Refresh Token 到剪贴板", "Refresh token copied to clipboard").to_string();
                            ws.ui.toast(msg, false);
                            cx.notify();
                        },
                    )
                }),
        )
        // Row 2: 5 buttons
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(2.0))
                .child({
                    let switch_id = acc_id.clone();
                    render_action_icon_btn(
                        format!("switch-app-{}", switch_id),
                        ARROW_RIGHT_LEFT_SVG,
                        i.t("切换到 Antigravity App", "Switch to Antigravity App"),
                        false,
                        &t,
                        cx,
                        move |ws, _, _, cx| {
                            switch_account_target_action(&switch_id, None, ws, cx);
                        },
                    )
                })
                .child({
                    let switch_id = acc_id.clone();
                    render_action_icon_btn(
                        format!("switch-ide-{}", switch_id),
                        REPEAT_SVG,
                        i.t("切换到 Antigravity IDE", "Switch to Antigravity IDE"),
                        false,
                        &t,
                        cx,
                        move |ws, _, _, cx| {
                            switch_account_target_action(&switch_id, Some("ide"), ws, cx);
                        },
                    )
                })
                .child({
                    let switch_id = acc_id.clone();
                    render_action_icon_btn(
                        format!("switch-cli-{}", switch_id),
                        TERMINAL_SVG,
                        i.t("切换到 CLI (agy)", "Switch to CLI (agy)"),
                        false,
                        &t,
                        cx,
                        move |ws, _, _, cx| {
                            switch_account_target_action(&switch_id, Some("agy"), ws, cx);
                        },
                    )
                })
                .child({
                    let tog_id = acc_id.clone();
                    render_action_icon_btn(
                        format!("toggle-{}", tog_id),
                        if is_disabled { TOGGLE_LEFT_SVG } else { TOGGLE_RIGHT_SVG },
                        if is_disabled { i.t("启用账号", "Enable Account") } else { i.t("禁用账号", "Disable Account") },
                        false,
                        &t,
                        cx,
                        move |ws, _, _, cx| {
                            toggle_disabled_action(&tog_id, ws, cx);
                        },
                    )
                })
                .child(render_action_icon_btn(
                    format!("delete-{}", del_id),
                    TRASH_SVG,
                    i.t("删除账号", "Delete Account"),
                    true,
                    &t,
                    cx,
                    move |ws, _, _, cx| {
                        delete_account_action(&del_id, &del_email, ws, cx);
                    },
                )),
        );

    let quota = account.quota.as_ref();
    let lower_row = if let Some(q) = quota {
        if q.is_forbidden {
            let raw_reason = q.forbidden_reason.as_deref().unwrap_or("API 403 Forbidden");
            let parsed = parse_generic_error(raw_reason);
            let appeal_btn = parsed.action_url.as_ref().map(|url| {
                error_action_link_button(
                    format!("ag-appeal-btn-{}", account.id),
                    "前往申诉 ↗",
                    url.clone(),
                    &t,
                    cx,
                )
            });
            div()
                .flex()
                .items_center()
                .justify_between()
                .gap(px(10.0))
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.0))
                        .child(error_strip_action(
                            format!("ag-quota-err-{}", account.id),
                            i.t("配额受限 (403)", "Quota 403"),
                            raw_reason,
                            &t,
                            cx,
                            appeal_btn,
                            |ws: &mut Workspace, _raw, cx| {
                                ws.ui.toast("已复制错误详情到剪贴板".to_string(), false);
                                cx.notify();
                            },
                        )),
                )
                .child(actions_column)
        } else {
            let (gemini_data, claude_data) = extract_family_quotas(account);
            let col1_gemini = render_model_quota_column("Gemini", crate::icons::GEMINI_SVG, &gemini_data, &t, &i);
            let col2_claude = render_model_quota_column("Claude / GPT", crate::icons::CLAUDE_SVG, &claude_data, &t, &i);

            div()
                .flex()
                .items_center()
                .justify_between()
                .gap(px(10.0))
                .child(col1_gemini)
                .child(col2_claude)
                .child(actions_column)
        }
    } else {
        let uninit_view = div()
            .flex_1()
            .h(px(54.0))
            .flex()
            .items_center()
            .px(px(10.0))
            .rounded(px(6.0))
            .bg(t.sidebar_bg)
            .text_size(px(11.0))
            .text_color(t.text_muted)
            .child(i.t(
                "暂未获取配额信息，点击右侧「刷新配额」获取当前额度。",
                "No quota data yet. Click 'Refresh Quota' on the right to check current limits.",
            ));

        div()
            .flex()
            .items_center()
            .justify_between()
            .gap(px(10.0))
            .child(uninit_view)
            .child(actions_column)
    };

    div()
        .id(SharedString::from(format!("account-card-{}", account.id)))
        .flex()
        .flex_col()
        .gap(px(8.0))
        .p(px(10.0))
        .rounded(px(8.0))
        .bg(t.card_bg)
        .border_1()
        .border_color(if is_active { t.accent } else { t.card_border })
        .shadow_xs()
        .child(header_row)
        .child(lower_row)
        .into_any_element()
}

/// Render the Account Details Dialog (displaying all specific models, thinking budgets, and quota groups).
pub fn render_account_details_dialog(
    account: &AntigravityAccount,
    ws: &mut Workspace,
    cx: &mut Context<Workspace>,
) -> gpui::AnyElement {
    let t = ws.theme.clone();
    let i = ws.i18n;

    let header_info = div()
        .flex()
        .items_center()
        .justify_between()
        .gap(px(8.0))
        .p(px(12.0))
        .rounded(px(8.0))
        .bg(t.input_bg)
        .border_1()
        .border_color(t.card_border)
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(8.0))
                .child(
                    div()
                        .text_size(px(13.0))
                        .font_weight(gpui::FontWeight::BOLD)
                        .text_color(t.text_primary)
                        .child(mask_email(&account.email)),
                )
                .child(render_tier_badge(&t, account.get_tier()))
                .when_some(account.project_id.as_ref(), |s, pid| {
                    s.child(
                        div()
                            .text_size(px(11.5))
                            .text_color(t.text_muted)
                            .child(format!("项目: {}", pid)),
                    )
                }),
        );

    let quota = account.quota.as_ref();
    let models = quota.map(|q| &q.models[..]).unwrap_or(&[]);
    let quota_groups = quota.map(|q| &q.quota_groups[..]).unwrap_or(&[]);

    let mut models_grid = div()
        .flex()
        .flex_col()
        .gap(px(8.0));

    if let Some(q) = quota {
        if q.is_forbidden {
            let raw_reason = q.forbidden_reason.as_deref().unwrap_or("API 403 Forbidden");
            let parsed = parse_generic_error(raw_reason);
            let appeal_btn = parsed.action_url.as_ref().map(|url| {
                let u = url.clone();
                button_l(
                    "ag-modal-appeal",
                    "前往官方申诉 ↗",
                    ButtonVariant::Danger,
                    &t,
                    cx,
                    move |ws, _, _, cx| {
                        cx.open_url(&u);
                        ws.ui.toast("已在浏览器中打开官方申诉表单".to_string(), false);
                        cx.notify();
                    },
                )
            });

            models_grid = models_grid.child(error_banner(
                "ag-modal-err",
                "配额接口受限 (HTTP 403 Forbidden)",
                raw_reason,
                &t,
                cx,
                appeal_btn,
            ));
        }
    }

    if models.is_empty() {
        if quota.map(|q| !q.is_forbidden).unwrap_or(true) {
            models_grid = models_grid.child(
                div()
                    .p(px(20.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_size(px(12.5))
                    .text_color(t.text_muted)
                    .child(i.t("暂无各模型配额数据", "No detailed model quotas available")),
            );
        }
    } else {
        let mut grid = div().flex().flex_wrap().gap(px(8.0));
        for m in models {
            let pct = m.percentage.clamp(0, 100);
            let bar_color = if pct > 50 {
                t.success
            } else if pct > 20 {
                t.warning
            } else {
                t.danger
            };
            let name = m.display_name.as_deref().unwrap_or(&m.name);
            let (reset_countdown, _) = format_time_remaining(&m.reset_time);

            grid = grid.child(
                div()
                    .w(px(260.0))
                    .p(px(10.0))
                    .rounded(px(6.0))
                    .bg(t.card_bg)
                    .border_1()
                    .border_color(t.card_border)
                    .flex()
                    .flex_col()
                    .gap(px(6.0))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .child(
                                div()
                                    .text_size(px(12.0))
                                    .font_weight(gpui::FontWeight::MEDIUM)
                                    .text_color(t.text_primary)
                                    .child(name.to_string()),
                            )
                            .child(
                                div()
                                    .text_size(px(12.0))
                                    .font_weight(gpui::FontWeight::BOLD)
                                    .text_color(bar_color)
                                    .child(format!("{}%", pct)),
                            ),
                    )
                    .child(
                        div()
                            .w_full()
                            .h(px(5.0))
                            .rounded(px(2.5))
                            .bg(t.card_border)
                            .overflow_hidden()
                            .child(
                                div()
                                    .w(gpui::relative(pct as f32 / 100.0))
                                    .h_full()
                                    .bg(bar_color)
                                    .rounded(px(2.5)),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .text_size(px(10.5))
                            .text_color(t.text_muted)
                            .child(if reset_countdown.is_empty() {
                                div()
                            } else {
                                div().child(reset_countdown)
                            })
                            .when(m.supports_thinking, |s| {
                                s.child(
                                    div()
                                        .px(px(4.0))
                                        .py(px(1.0))
                                        .rounded(px(3.0))
                                        .bg(t.accent_subtle)
                                        .text_color(t.accent)
                                        .child("Thinking"),
                                )
                            }),
                    ),
            );
        }
        models_grid = models_grid.child(grid);
    }

    // Quota groups summary
    let mut groups_view = div().flex().flex_col().gap(px(8.0));
    if !quota_groups.is_empty() {
        let mut grp_row = div().flex().items_center().gap(px(10.0)).flex_wrap();
        for g in quota_groups {
            let pct = (g.remaining_fraction * 100.0).round() as i32;
            let bar_color = if pct > 50 { t.success } else if pct > 20 { t.warning } else { t.danger };
            grp_row = grp_row.child(
                div()
                    .p(px(8.0))
                    .rounded(px(6.0))
                    .bg(t.input_bg)
                    .border_1()
                    .border_color(t.card_border)
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .child(
                        div()
                            .text_size(px(11.5))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(t.text_secondary)
                            .child(format!("{} ({})", g.display_name, g.window)),
                    )
                    .child(
                        div()
                            .text_size(px(12.0))
                            .font_weight(gpui::FontWeight::BOLD)
                            .text_color(bar_color)
                            .child(format!("{}%", pct)),
                    ),
            );
        }
        groups_view = groups_view.child(grp_row);
    }

    let body = div()
        .id("account-details-scroll")
        .flex()
        .flex_col()
        .gap(px(14.0))
        .max_h(px(520.0))
        .overflow_y_scroll()
        .child(header_info)
        .child(
            div()
                .text_size(px(12.5))
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .text_color(t.text_secondary)
                .child(i.t("全部模型详细配额", "All Model Detailed Quotas")),
        )
        .child(models_grid)
        .when(!quota_groups.is_empty(), |s| {
            s.child(
                div()
                    .text_size(px(12.5))
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .text_color(t.text_secondary)
                    .child(i.t("配额组窗口摘要", "Quota Group Windows")),
            ).child(groups_view)
        })
        .child(
            div()
                .flex()
                .justify_end()
                .pt(px(10.0))
                .border_t_1()
                .border_color(t.card_border)
                .child(button_l(
                    "close-details-btn",
                    i.t("关闭", "Close"),
                    ButtonVariant::Secondary,
                    &t,
                    cx,
                    |ws, _, _, cx| {
                        ws.ui.antigravity_details_account = None;
                        cx.notify();
                    },
                )),
        );

    modal_scaffold_custom(
        &t,
        &format!("{} - {}", i.t("账号详情", "Account Details"), account.email),
        px(680.0),
        body.into_any_element(),
        cx,
        |ws, _, _, cx| {
            ws.ui.antigravity_details_account = None;
            cx.notify();
        },
    )
}

/// Render the Device Fingerprint Dialog.
pub fn render_device_fingerprint_dialog(
    account: &AntigravityAccount,
    profile: DeviceProfile,
    ws: &mut Workspace,
    cx: &mut Context<Workspace>,
) -> gpui::AnyElement {
    let t = ws.theme.clone();
    let i = ws.i18n;

    let profile_for_copy = profile.clone();
    let profile_for_save = profile.clone();
    let account_id_for_save = account.id.clone();
    let account_for_regenerate = account.clone();

    let render_field = |name: &'static str, val: &str| -> gpui::AnyElement {
        let v_copy = val.to_string();
        div()
            .flex()
            .flex_col()
            .gap(px(4.0))
            .child(
                div()
                    .text_size(px(11.5))
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .text_color(t.text_secondary)
                    .child(name),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(8.0))
                    .px(px(10.0))
                    .py(px(6.0))
                    .rounded(px(6.0))
                    .bg(t.input_bg)
                    .border_1()
                    .border_color(t.card_border)
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.0))
                            .overflow_hidden()
                            .text_ellipsis()
                            .text_size(px(12.0))
                            .font_family("Consolas, monospace")
                            .text_color(t.text_primary)
                            .child(val.to_string()),
                    )
                    .child(
                        div()
                            .id(SharedString::from(format!("copy-{}", name)))
                            .cursor_pointer()
                            .p(px(2.0))
                            .hover(|h| h.opacity(0.8))
                            .child(gpui::svg().data(COPY_SVG).size(px(12.0)).text_color(t.text_muted))
                            .on_click(cx.listener(move |ws, _, _, cx| {
                                cx.write_to_clipboard(gpui::ClipboardItem::new_string(v_copy.clone()));
                                ws.ui.toast(format!("已复制 {}", name), false);
                                cx.notify();
                            })),
                    ),
            )
            .into_any_element()
    };

    let body = div()
        .flex()
        .flex_col()
        .gap(px(12.0))
        .child(
            div()
                .text_size(px(12.5))
                .text_color(t.text_secondary)
                .child(i.t(
                    "设备指纹用于隔离不同账号的 IDE/VS Code 设备标识，避免多账号关联或风控限制。",
                    "Device fingerprints isolate account machine identifiers to avoid multi-account association.",
                )),
        )
        .child(render_field("Machine ID", &profile.machine_id))
        .child(render_field("Mac Machine ID", &profile.mac_machine_id))
        .child(render_field("Dev Device ID", &profile.dev_device_id))
        .child(render_field("SQM ID", &profile.sqm_id))
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .pt(px(12.0))
                .border_t_1()
                .border_color(t.card_border)
                .child(
                    div()
                        .flex()
                        .gap(px(8.0))
                        .child(button_with_icon_l(
                            "btn-regen-fp",
                            WAND_SVG,
                            i.t("重新生成随机指纹", "Generate New"),
                            ButtonVariant::Secondary,
                            &t,
                            cx,
                            move |ws, _, _, cx| {
                                let new_prof = DeviceProfile::generate_random();
                                ws.ui.antigravity_device_account = Some((account_for_regenerate.clone(), new_prof));
                                ws.ui.toast(ws.i18n.t("已生成新随机指纹，点击「保存并绑定」生效", "New fingerprint generated. Click Save to apply.").to_string(), false);
                                cx.notify();
                            },
                        ))
                        .child(button_with_icon_l(
                            "btn-copy-all-fp",
                            COPY_SVG,
                            i.t("复制全部", "Copy All"),
                            ButtonVariant::Ghost,
                            &t,
                            cx,
                            move |ws, _, _, cx| {
                                if let Ok(json) = serde_json::to_string_pretty(&profile_for_copy) {
                                    cx.write_to_clipboard(gpui::ClipboardItem::new_string(json));
                                    ws.ui.toast(ws.i18n.t("已复制完整设备指纹到剪贴板", "Copied device profile to clipboard").to_string(), false);
                                    cx.notify();
                                }
                            },
                        )),
                )
                .child(
                    div()
                        .flex()
                        .gap(px(8.0))
                        .child(button_l(
                            "btn-close-fp",
                            i.t("取消", "Cancel"),
                            ButtonVariant::Secondary,
                            &t,
                            cx,
                            |ws, _, _, cx| {
                                ws.ui.antigravity_device_account = None;
                                cx.notify();
                            },
                        ))
                        .child(button_l(
                            "btn-save-fp",
                            i.t("保存并绑定", "Save & Bind"),
                            ButtonVariant::Primary,
                            &t,
                            cx,
                            move |ws, _, _, cx| {
                                let app_data = ws.paths.app_data.clone();
                                if let Some(mut store) = ws.ui.antigravity_store.clone() {
                                    let _ = update_account_device_profile(&app_data, &mut store, &account_id_for_save, profile_for_save.clone());
                                    ws.ui.antigravity_store = Some(store);
                                    ws.ui.toast(ws.i18n.t("设备指纹已绑定并保存", "Device fingerprint bound and saved").to_string(), false);
                                }
                                ws.ui.antigravity_device_account = None;
                                cx.notify();
                            },
                        )),
                ),
        );

    modal_scaffold(
        &t,
        &format!("{} - {}", i.t("设备指纹", "Device Fingerprint"), account.email),
        body.into_any_element(),
        cx,
        |ws, _, _, cx| {
            ws.ui.antigravity_device_account = None;
            cx.notify();
        },
    )
}

/// Render the Custom Label Edit Dialog.
pub fn render_label_edit_dialog(
    account_id: &str,
    input: gpui::Entity<TextInput>,
    ws: &mut Workspace,
    cx: &mut Context<Workspace>,
) -> gpui::AnyElement {
    let t = ws.theme.clone();
    let i = ws.i18n;
    let acc_id_save = account_id.to_string();
    let acc_id_clear = account_id.to_string();
    let input_for_save = input.clone();

    let body = div()
        .flex()
        .flex_col()
        .gap(px(14.0))
        .child(
            div()
                .text_size(px(12.5))
                .text_color(t.text_secondary)
                .child(i.t(
                    "设置自定义标签方便在多账号列表中快速辨识（例如：工作号、主力账号、备用、测试等）。",
                    "Set a custom label to easily identify this account in the list.",
                )),
        )
        .child(input_container(&t, input))
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .pt(px(10.0))
                .border_t_1()
                .border_color(t.card_border)
                .child(button_l(
                    "btn-clear-label",
                    i.t("清除标签", "Clear Label"),
                    ButtonVariant::Danger,
                    &t,
                    cx,
                    move |ws, _, _, cx| {
                        let app_data = ws.paths.app_data.clone();
                        if let Some(mut store) = ws.ui.antigravity_store.clone() {
                            let _ = update_account_label(&app_data, &mut store, &acc_id_clear, None);
                            ws.ui.antigravity_store = Some(store);
                            ws.ui.toast(ws.i18n.t("已清除标签", "Label cleared").to_string(), false);
                        }
                        ws.ui.antigravity_editing_label = None;
                        cx.notify();
                    },
                ))
                .child(
                    div()
                        .flex()
                        .gap(px(8.0))
                        .child(button_l(
                            "btn-cancel-label",
                            i.t("取消", "Cancel"),
                            ButtonVariant::Secondary,
                            &t,
                            cx,
                            |ws, _, _, cx| {
                                ws.ui.antigravity_editing_label = None;
                                cx.notify();
                            },
                        ))
                        .child(button_l(
                            "btn-save-label",
                            i.t("保存", "Save"),
                            ButtonVariant::Primary,
                            &t,
                            cx,
                            move |ws, _, _, cx| {
                                let text = input_for_save.read(cx).text().trim().to_string();
                                let app_data = ws.paths.app_data.clone();
                                if let Some(mut store) = ws.ui.antigravity_store.clone() {
                                    let val = if text.is_empty() { None } else { Some(text) };
                                    let _ = update_account_label(&app_data, &mut store, &acc_id_save, val);
                                    ws.ui.antigravity_store = Some(store);
                                    ws.ui.toast(ws.i18n.t("标签已保存", "Label saved").to_string(), false);
                                }
                                ws.ui.antigravity_editing_label = None;
                                cx.notify();
                            },
                        )),
                ),
        );

    modal_scaffold(
        &t,
        &i.t("编辑自定义标签", "Edit Custom Label"),
        body.into_any_element(),
        cx,
        |ws, _, _, cx| {
            ws.ui.antigravity_editing_label = None;
            cx.notify();
        },
    )
}

// ---------------------------------------------------------------------------
// Actions
// ---------------------------------------------------------------------------

/// Open Add Account modal dialog.
pub fn open_add_account_dialog(
    ws: &mut Workspace,
    _window: &mut gpui::Window,
    cx: &mut Context<Workspace>,
) {
    let manual_code = cx.new(|cx| TextInput::new("粘贴回调链接或 Code…", cx));
    let refresh_token = cx.new(|cx| {
        TextInput::new(
            "在此处粘贴您的 Refresh Token (支持批量)\n\n支持格式:\n1. 单个 Token (1//...)\n2. JSON 数组 (含 refresh_token 字段)\n3. 任意包含 Token 的文本 (自动提取)",
            cx,
        )
    });
    let custom_label = cx.new(|cx| TextInput::new("自定义备注（可选）…", cx));

    let mut state = AntigravityDialogState {
        active_tab: AntigravityDialogTab::OAuth,
        oauth_url: None,
        redirect_uri: None,
        oauth_url_copied: false,
        manual_code,
        refresh_token: refresh_token.clone(),
        custom_label,
        auth_status: None,
        is_authorizing: false,
        error_message: None,
        session: None,
    };

    // Pre-generate OAuth URL when dialog opens on OAuth tab (matching Antigravity-Manager prepare_oauth_url)
    match OAuthServerSession::start() {
        Ok(s) => {
            state.oauth_url = Some(s.auth_url.clone());
            state.redirect_uri = Some(s.redirect_uri.clone());
            let session_arc = std::sync::Arc::new(s);
            state.session = Some(session_arc.clone());
            start_oauth_listener_background(session_arc, ws, cx);
        }
        Err(e) => {
            tracing::warn!("Failed to pre-generate OAuth URL: {}", e);
        }
    }

    ws.ui.antigravity_dialog = Some(state);
    cx.notify();
}

/// Render the Add Account modal dialog.
pub fn render_add_account_dialog(
    state: &AntigravityDialogState,
    ws: &mut Workspace,
    cx: &mut Context<Workspace>,
) -> gpui::AnyElement {
    let t = ws.theme.clone();
    let i = ws.i18n;

    let tab_oauth = i.t("OAuth 授权", "OAuth Auth");
    let tab_token = i.t("Refresh Token", "Refresh Token");
    let tab_import = i.t("从数据库导入", "Import from DB");

    let is_oauth = state.active_tab == AntigravityDialogTab::OAuth;
    let is_token = state.active_tab == AntigravityDialogTab::Token;
    let is_import = state.active_tab == AntigravityDialogTab::Import;

    let tab_bar = div()
        .grid()
        .grid_cols(3)
        .gap(px(4.0))
        .p(px(3.0))
        .rounded(px(10.0))
        .bg(t.card_bg)
        .border_1()
        .border_color(t.card_border)
        .mb(px(12.0))
        .child(
            div()
                .id("ag-tab-oauth")
                .cursor_pointer()
                .py(px(6.0))
                .px(px(10.0))
                .rounded(px(7.0))
                .text_size(px(12.5))
                .font_weight(if is_oauth { gpui::FontWeight::SEMIBOLD } else { gpui::FontWeight::MEDIUM })
                .text_center()
                .when(is_oauth, |s| {
                    s.bg(t.input_bg).text_color(t.accent).shadow_sm()
                })
                .when(!is_oauth, |s| {
                    s.text_color(t.text_secondary).hover(|h| h.bg(t.card_hover))
                })
                .on_click(cx.listener(|ws, _, _, cx| {
                    if let Some(d) = &mut ws.ui.antigravity_dialog {
                        d.active_tab = AntigravityDialogTab::OAuth;
                        d.error_message = None;
                        // If no active session, start one
                        if d.session.is_none() || d.session.as_ref().map(|s| s.is_cancelled()).unwrap_or(false) {
                            if let Ok(s) = OAuthServerSession::start() {
                                d.oauth_url = Some(s.auth_url.clone());
                                d.redirect_uri = Some(s.redirect_uri.clone());
                                let session_arc = std::sync::Arc::new(s);
                                d.session = Some(session_arc.clone());
                                start_oauth_listener_background(session_arc, ws, cx);
                            }
                        }
                        cx.notify();
                    }
                }))
                .child(tab_oauth),
        )
        .child(
            div()
                .id("ag-tab-token")
                .cursor_pointer()
                .py(px(6.0))
                .px(px(10.0))
                .rounded(px(7.0))
                .text_size(px(12.5))
                .font_weight(if is_token { gpui::FontWeight::SEMIBOLD } else { gpui::FontWeight::MEDIUM })
                .text_center()
                .when(is_token, |s| {
                    s.bg(t.input_bg).text_color(t.accent).shadow_sm()
                })
                .when(!is_token, |s| {
                    s.text_color(t.text_secondary).hover(|h| h.bg(t.card_hover))
                })
                .on_click(cx.listener(|ws, _, _, cx| {
                    if let Some(d) = &mut ws.ui.antigravity_dialog {
                        d.active_tab = AntigravityDialogTab::Token;
                        d.error_message = None;
                        if let Some(s) = &d.session {
                            s.cancel();
                        }
                        cx.notify();
                    }
                }))
                .child(tab_token),
        )
        .child(
            div()
                .id("ag-tab-import")
                .cursor_pointer()
                .py(px(6.0))
                .px(px(10.0))
                .rounded(px(7.0))
                .text_size(px(12.5))
                .font_weight(if is_import { gpui::FontWeight::SEMIBOLD } else { gpui::FontWeight::MEDIUM })
                .text_center()
                .when(is_import, |s| {
                    s.bg(t.input_bg).text_color(t.accent).shadow_sm()
                })
                .when(!is_import, |s| {
                    s.text_color(t.text_secondary).hover(|h| h.bg(t.card_hover))
                })
                .on_click(cx.listener(|ws, _, _, cx| {
                    if let Some(d) = &mut ws.ui.antigravity_dialog {
                        d.active_tab = AntigravityDialogTab::Import;
                        d.error_message = None;
                        if let Some(s) = &d.session {
                            s.cancel();
                        }
                        cx.notify();
                    }
                }))
                .child(tab_import),
        );

    // Status alert message
    let status_strip = if let Some(ref status) = state.auth_status {
        Some(
            div()
                .p(px(10.0))
                .rounded(px(8.0))
                .bg(t.accent_subtle)
                .border_1()
                .border_color(t.accent)
                .text_size(px(12.0))
                .text_color(t.accent)
                .child(status.clone())
        )
    } else {
        None
    };

    // Error strip
    let error_strip = if let Some(ref err) = state.error_message {
        let dismiss_btn = icon_button_svg(
            "btn-dismiss-dlg-err",
            X_SVG,
            i.t("关闭", "Close"),
            false,
            &t,
            cx,
            |ws, _, _, cx| {
                if let Some(d) = ws.ui.antigravity_dialog.as_mut() {
                    d.error_message = None;
                }
                cx.notify();
            },
        );
        Some(error_strip(
            "ag-dlg-error-msg",
            i.t("操作失败", "Failed"),
            err,
            &t,
            cx,
            Some(dismiss_btn),
        ))
    } else {
        None
    };

    let content = match state.active_tab {
        AntigravityDialogTab::OAuth => {
            // Tab 1: OAuth 授权
            div()
                .flex()
                .flex_col()
                .gap(px(14.0))
                .py(px(2.0))
                // Soft rounded circle Globe header (matching AddAccountDialog.tsx)
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .items_center()
                        .text_center()
                        .gap(px(8.0))
                        .child(
                            div()
                                .w(px(56.0))
                                .h(px(56.0))
                                .rounded_full()
                                .bg(t.accent_subtle)
                                .flex()
                                .items_center()
                                .justify_center()
                                .child(svg_icon(GLOBE_SVG, px(28.0), t.accent)),
                        )
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .gap(px(2.0))
                                .child(
                                    div()
                                        .text_size(px(14.0))
                                        .font_weight(gpui::FontWeight::SEMIBOLD)
                                        .text_color(t.text_primary)
                                        .child(i.t("推荐方式", "Recommended Method")),
                                )
                                .child(
                                    div()
                                        .text_size(px(12.0))
                                        .text_color(t.text_secondary)
                                        .child(i.t(
                                            "将打开默认浏览器进行 Google 登录授权，自动获取并保存 Token。",
                                            "Will open default browser for Google sign-in and save Token automatically.",
                                        )),
                                ),
                        ),
                )
                // Action Button: "开始 OAuth 授权"
                .child(
                    button_l(
                        "ag-btn-start-oauth",
                        if state.is_authorizing {
                            i.t("正在等待授权…", "Waiting for authorization…")
                        } else {
                            i.t("开始 OAuth 授权", "Start OAuth Login")
                        },
                        ButtonVariant::Primary,
                        &t,
                        cx,
                        |ws, _, _, cx| {
                            start_browser_oauth_action(ws, cx);
                        },
                    )
                )
                // Pre-generated OAuth link dashed box (matching AddAccountDialog.tsx)
                .when_some(state.oauth_url.as_ref(), |s, url| {
                    let url_copy = url.clone();
                    let is_copied = state.oauth_url_copied;
                    s.child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(6.0))
                            .child(
                                div()
                                    .text_size(px(11.0))
                                    .text_color(t.text_secondary)
                                    .child(i.t("授权链接", "Authorization Link")),
                            )
                            .child(
                                div()
                                    .id("ag-card-auth-url")
                                    .cursor_pointer()
                                    .flex()
                                    .items_center()
                                    .justify_between()
                                    .gap(px(8.0))
                                    .px(px(10.0))
                                    .py(px(7.0))
                                    .rounded(px(8.0))
                                    .bg(t.input_bg)
                                    .border_1()
                                    .border_color(t.card_border)
                                    .on_click(cx.listener({
                                        let url_copy = url_copy.clone();
                                        move |ws, _, _, cx| {
                                            cx.write_to_clipboard(gpui::ClipboardItem::new_string(url_copy.clone()));
                                            if let Some(d) = &mut ws.ui.antigravity_dialog {
                                                d.oauth_url_copied = true;
                                            }
                                            ws.ui.toast(ws.i18n.t("已复制授权链接", "Copied link").to_string(), false);
                                            cx.notify();
                                        }
                                    }))
                                    .child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .gap(px(6.0))
                                            .flex_1()
                                            .overflow_hidden()
                                            .child(svg_icon(
                                                if is_copied { CHECK_SVG } else { COPY_SVG },
                                                px(12.0),
                                                if is_copied { gpui::rgb(0x10b981) } else { t.text_muted },
                                            ))
                                            .child(
                                                div()
                                                    .text_size(px(11.0))
                                                    .font_family("Consolas, monospace")
                                                    .text_color(t.text_muted)
                                                    .line_clamp(1)
                                                    .child(url.clone()),
                                            ),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(11.0))
                                            .text_color(if is_copied { gpui::rgb(0x10b981) } else { t.accent })
                                            .child(if is_copied {
                                                i.t("已复制", "Copied")
                                            } else {
                                                i.t("复制授权链接", "Copy Link")
                                            }),
                                    ),
                            )
                            .child(
                                button_with_icon_l(
                                    "ag-btn-finish-oauth",
                                    CHECK_SVG,
                                    i.t("我已授权，继续", "I Have Authorized, Continue"),
                                    ButtonVariant::Secondary,
                                    &t,
                                    cx,
                                    |ws, _, _, cx| {
                                        complete_oauth_flow_action(ws, cx);
                                    },
                                )
                            ),
                    )
                })
                // Manual code entry
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(6.0))
                        .pt(px(10.0))
                        .border_t_1()
                        .border_color(t.card_border)
                        .child(
                            div()
                                .text_size(px(11.0))
                                .text_color(t.text_muted)
                                .child(i.t(
                                    "浏览器没有自动跳转？请在此处粘贴回调链接或 Authorization Code：",
                                    "Browser did not redirect? Paste callback URL or Authorization Code:",
                                )),
                        )
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(8.0))
                                .child(
                                    div()
                                        .flex_1()
                                        .child(input_container(&t, state.manual_code.clone())),
                                )
                                .child(
                                    button_l(
                                        "ag-btn-manual-code-submit",
                                        i.t("提交", "Submit"),
                                        ButtonVariant::Primary,
                                        &t,
                                        cx,
                                        |ws, _, _, cx| {
                                            submit_manual_code_action(ws, cx);
                                        },
                                    )
                                ),
                        ),
                )
        }
        AntigravityDialogTab::Token => {
            // Tab 2: Refresh Token (Multi-line batch input)
            div()
                .flex()
                .flex_col()
                .gap(px(12.0))
                .child(
                    div()
                        .p(px(12.0))
                        .rounded(px(8.0))
                        .bg(t.card_bg)
                        .border_1()
                        .border_color(t.card_border)
                        .flex()
                        .flex_col()
                        .gap(px(8.0))
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .justify_between()
                                .child(
                                    div()
                                        .text_size(px(12.5))
                                        .font_weight(gpui::FontWeight::MEDIUM)
                                        .text_color(t.text_primary)
                                        .child(i.t("Refresh Token", "Refresh Token")),
                                ),
                        )
                        .child(input_container(&t, state.refresh_token.clone()))
                        .child(
                            div()
                                .text_size(px(10.5))
                                .text_color(t.text_muted)
                                .child(i.t(
                                    "提示: 支持一次性粘贴多个 Token 或 JSON 数组，系统将自动识别并批量导入。",
                                    "Hint: Supports pasting multiple tokens or JSON array, system will auto-extract and batch import.",
                                )),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(4.0))
                        .child(
                            div()
                                .text_size(px(12.0))
                                .font_weight(gpui::FontWeight::MEDIUM)
                                .text_color(t.text_secondary)
                                .child(i.t("账号备注（可选）", "Custom Label (Optional)")),
                        )
                        .child(input_container(&t, state.custom_label.clone())),
                )
        }
        AntigravityDialogTab::Import => {
            // Tab 3: 从数据库导入
            div()
                .flex()
                .flex_col()
                .gap(px(14.0))
                // Scheme A: 自动扫描本地已登录账号
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(8.0))
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(6.0))
                                .child(svg_icon(DATABASE_SVG, px(16.0), t.accent))
                                .child(
                                    div()
                                        .text_size(px(13.0))
                                        .font_weight(gpui::FontWeight::SEMIBOLD)
                                        .text_color(t.text_primary)
                                        .child(i.t("方案 A: 自动扫描本地已登录账号", "Scheme A: Auto Scan Local Accounts")),
                                ),
                        )
                        .child(
                            div()
                                .text_size(px(11.5))
                                .text_color(t.text_secondary)
                                .child(i.t(
                                    "自动对本地系统 Keyring、Antigravity IDE、插件版及 CLI 进行全量扫描并批量导入已登录账号。",
                                    "Automatically scans System Keyring, Antigravity IDE, extension, and CLI to import accounts.",
                                )),
                        )
                        .child(
                            button_with_icon_l(
                                "ag-btn-scan-local-db",
                                CHECK_SVG,
                                i.t("一键导入所有本地已登录账号", "Import All Local Logged-in Accounts"),
                                ButtonVariant::Primary,
                                &t,
                                cx,
                                |ws, _, _, cx| {
                                    import_all_local_accounts_action(ws, cx);
                                },
                            )
                        )
                        .child(
                            button_with_icon_l(
                                "ag-btn-custom-db",
                                DATABASE_SVG,
                                i.t("从自定义 DB 导入 (state.vscdb)", "Import Custom DB (state.vscdb)"),
                                ButtonVariant::Secondary,
                                &t,
                                cx,
                                |ws, _, _, cx| {
                                    import_custom_db_picker_action(ws, cx);
                                },
                            )
                        ),
                )
                // Divider
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(10.0))
                        .child(div().flex_1().h(px(1.0)).bg(t.card_border))
                        .child(
                            div()
                                .text_size(px(11.0))
                                .text_color(t.text_muted)
                                .child(i.t("或者", "OR")),
                        )
                        .child(div().flex_1().h(px(1.0)).bg(t.card_border)),
                )
                // Scheme B: 从旧版数据文件导入
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(8.0))
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(6.0))
                                .child(svg_icon(HISTORY_SVG, px(16.0), t.accent))
                                .child(
                                    div()
                                        .text_size(px(13.0))
                                        .font_weight(gpui::FontWeight::SEMIBOLD)
                                        .text_color(t.text_primary)
                                        .child(i.t("方案 B: 从旧版数据文件导入", "Scheme B: Import from Legacy Backup")),
                                ),
                        )
                        .child(
                            div()
                                .text_size(px(11.5))
                                .text_color(t.text_secondary)
                                .child(i.t(
                                    "扫描 ~/.antigravity-agent 目录，批量导入旧版本的账号数据。",
                                    "Scans ~/.antigravity-agent directory to batch import legacy accounts.",
                                )),
                        )
                        .child(
                            button_with_icon_l(
                                "ag-btn-import-v1",
                                HISTORY_SVG,
                                i.t("从旧版数据导入", "Import from Legacy Backup"),
                                ButtonVariant::Secondary,
                                &t,
                                cx,
                                |ws, _, _, cx| {
                                    import_v1_backup_action(ws, cx);
                                },
                            )
                        ),
                )
        }
    };

    let body = div()
        .flex()
        .flex_col()
        .gap(px(12.0))
        .child(tab_bar)
        .when_some(status_strip, |s, st| s.child(st))
        .when_some(error_strip, |s, er| s.child(er))
        .child(content)
        .child(
            div()
                .flex()
                .justify_end()
                .gap(px(8.0))
                .mt(px(8.0))
                .child(button_l(
                    "ag-dlg-cancel",
                    i.t("取消", "Cancel"),
                    ButtonVariant::Secondary,
                    &t,
                    cx,
                    |ws, _, _, cx| {
                        if let Some(d) = &ws.ui.antigravity_dialog {
                            if let Some(s) = &d.session {
                                s.cancel();
                            }
                        }
                        ws.ui.antigravity_dialog = None;
                        cx.notify();
                    },
                ))
                .when(is_token, |s| {
                    s.child(button_l(
                        "ag-dlg-submit-token-batch",
                        i.t("确认添加", "Confirm Add"),
                        ButtonVariant::Primary,
                        &t,
                        cx,
                        |ws, _, _, cx| {
                            submit_batch_refresh_token_action(ws, cx);
                        },
                    ))
                }),
        );

    modal_scaffold_custom(
        &t,
        &i.t("添加新账号", "Add Account"),
        px(520.0),
        body.into_any_element(),
        cx,
        |ws, _, _, cx| {
            if let Some(d) = &ws.ui.antigravity_dialog {
                if let Some(s) = &d.session {
                    s.cancel();
                }
            }
            ws.ui.antigravity_dialog = None;
            cx.notify();
        },
    )
}

/// Start Google OAuth authorization in default browser.
fn start_browser_oauth_action(ws: &mut Workspace, cx: &mut Context<Workspace>) {
    let state = match ws.ui.antigravity_dialog.as_mut() {
        Some(s) => s,
        None => return,
    };
    state.is_authorizing = true;
    state.auth_status = Some(ws.i18n.t("已在浏览器中打开授权页面，请在浏览器中完成登录…", "Opened browser for authorization…").to_string());
    state.error_message = None;

    if let Some(ref url) = state.oauth_url {
        open_browser(url);
    } else {
        match OAuthServerSession::start() {
            Ok(s) => {
                let url = s.auth_url.clone();
                state.oauth_url = Some(url.clone());
                state.redirect_uri = Some(s.redirect_uri.clone());
                let session_arc = std::sync::Arc::new(s);
                state.session = Some(session_arc.clone());
                open_browser(&url);
                start_oauth_listener_background(session_arc, ws, cx);
            }
            Err(e) => {
                state.is_authorizing = false;
                state.error_message = Some(format!("启动本地监听失败: {}", e));
            }
        }
    }
    cx.notify();
}

/// User clicked "I have authorized, continue"
fn complete_oauth_flow_action(ws: &mut Workspace, cx: &mut Context<Workspace>) {
    if let Some(d) = ws.ui.antigravity_dialog.as_mut() {
        d.is_authorizing = true;
        d.auth_status = Some(ws.i18n.t("正在等待授权回调…", "Waiting for OAuth callback…").to_string());
    }
    cx.notify();
}

/// Listen for OAuth loopback callback in background
fn start_oauth_listener_background(
    session: std::sync::Arc<OAuthServerSession>,
    ws: &mut Workspace,
    cx: &mut Context<Workspace>,
) {
    let weak = cx.entity().downgrade();
    let app_data = ws.paths.app_data.clone();
    let session_redirect_uri = session.redirect_uri.clone();

    cx.spawn(async move |_this, cx| {
        let exchange_result = cx.background_spawn(async move {
            let code = session.wait_for_code(Duration::from_secs(300))?;
            let (access_token, refresh_token, expires_in) = exchange_auth_code(&code, &session_redirect_uri)?;
            let expiry_timestamp = Utc::now().timestamp() + expires_in;

            let user_info = fetch_user_info(&access_token)?;
            let (project_id, tier) = fetch_project_and_tier(&access_token);
            let quota = fetch_quota(&access_token, project_id.as_deref(), None).ok();

            let mut account = AntigravityAccount::new(
                uuid::Uuid::new_v4().to_string(),
                user_info.email,
                access_token,
                refresh_token,
                expiry_timestamp,
            );
            account.name = user_info.name;
            account.picture = user_info.picture;
            account.project_id = project_id;
            account.tier = tier;
            account.quota = quota;

            Ok::<AntigravityAccount, String>(account)
        }).await;

        let _ = weak.update(cx, |ws: &mut Workspace, cx| {
            match exchange_result {
                Ok(account) => {
                    let email = account.email.clone();
                    let mut store = ws.ui.antigravity_store.clone().unwrap_or_default();
                    if let Some(pos) = store.accounts.iter().position(|a| a.email == email) {
                        store.accounts[pos] = account;
                    } else {
                        store.accounts.push(account);
                    }
                    let _ = save_store(&app_data, &store);
                    ws.ui.antigravity_store = Some(store);
                    ws.ui.antigravity_dialog = None;

                    let msg = ws.i18n.t(
                        &format!("已成功添加账号：{}", email),
                        &format!("Account added: {}", email),
                    ).to_string();
                    ws.ui.toast(msg, false);
                }
                Err(e) => {
                    if e != "OAuth authorization cancelled." {
                        if let Some(d) = &mut ws.ui.antigravity_dialog {
                            d.is_authorizing = false;
                            d.error_message = Some(format!("授权失败: {}", e));
                            d.auth_status = None;
                        }
                    }
                }
            }
            cx.notify();
        });
    }).detach();
}

/// Submit manual OAuth authorization code or callback URL.
fn submit_manual_code_action(ws: &mut Workspace, cx: &mut Context<Workspace>) {
    let state = match &ws.ui.antigravity_dialog {
        Some(s) => s,
        None => return,
    };
    let raw_input = state.manual_code.read(cx).text().trim().to_string();
    if raw_input.is_empty() {
        if let Some(d) = ws.ui.antigravity_dialog.as_mut() {
            d.error_message = Some(ws.i18n.t("请先输入回调 URL 或授权码", "Please enter the callback URL or code").to_string());
        }
        cx.notify();
        return;
    }

    let code = extract_oauth_code(&raw_input);
    let redirect_uri = state.redirect_uri.clone().unwrap_or_else(|| "http://localhost:0/oauth-callback".to_string());
    let app_data = ws.paths.app_data.clone();
    let weak = cx.entity().downgrade();

    if let Some(d) = ws.ui.antigravity_dialog.as_mut() {
        d.auth_status = Some(ws.i18n.t("正在验证授权码并获取账号信息…", "Validating code and fetching account info…").to_string());
        d.error_message = None;
    }
    cx.notify();

    cx.spawn(async move |_this, cx| {
        let exchange_result = cx.background_spawn(async move {
            let (access_token, refresh_token, expires_in) = exchange_auth_code(&code, &redirect_uri)?;
            let expiry_timestamp = Utc::now().timestamp() + expires_in;

            let user_info = fetch_user_info(&access_token)?;
            let (project_id, tier) = fetch_project_and_tier(&access_token);
            let quota = fetch_quota(&access_token, project_id.as_deref(), None).ok();

            let mut account = AntigravityAccount::new(
                uuid::Uuid::new_v4().to_string(),
                user_info.email,
                access_token,
                refresh_token,
                expiry_timestamp,
            );
            account.name = user_info.name;
            account.picture = user_info.picture;
            account.project_id = project_id;
            account.tier = tier;
            account.quota = quota;

            Ok::<AntigravityAccount, String>(account)
        }).await;

        let _ = weak.update(cx, |ws: &mut Workspace, cx| {
            match exchange_result {
                Ok(account) => {
                    let email = account.email.clone();
                    let mut store = ws.ui.antigravity_store.clone().unwrap_or_default();
                    if let Some(pos) = store.accounts.iter().position(|a| a.email == email) {
                        store.accounts[pos] = account;
                    } else {
                        store.accounts.push(account);
                    }
                    let _ = save_store(&app_data, &store);
                    ws.ui.antigravity_store = Some(store);
                    ws.ui.antigravity_dialog = None;

                    let msg = ws.i18n.t(
                        &format!("已成功添加账号：{}", email),
                        &format!("Account added: {}", email),
                    ).to_string();
                    ws.ui.toast(msg, false);
                }
                Err(e) => {
                    if let Some(d) = ws.ui.antigravity_dialog.as_mut() {
                        d.error_message = Some(format!("手动提交失败: {}", e));
                        d.auth_status = None;
                    }
                }
            }
            cx.notify();
        });
    }).detach();
}

/// Submit batch refresh tokens (Tab 2)
fn submit_batch_refresh_token_action(ws: &mut Workspace, cx: &mut Context<Workspace>) {
    let state = match ws.ui.antigravity_dialog.as_ref() {
        Some(s) => s,
        None => return,
    };
    let raw_input = state.refresh_token.read(cx).text().trim().to_string();
    let custom_label = state.custom_label.read(cx).text().trim().to_string();
    let label_opt = if custom_label.is_empty() { None } else { Some(custom_label) };

    let tokens = extract_tokens_from_text(&raw_input);
    if tokens.is_empty() {
        if let Some(d) = ws.ui.antigravity_dialog.as_mut() {
            d.error_message = Some(ws.i18n.t("请填写 Refresh Token (以 1// 开头)", "Please provide a valid Refresh Token (starting with 1//)").to_string());
        }
        cx.notify();
        return;
    }

    if let Some(d) = ws.ui.antigravity_dialog.as_mut() {
        d.is_authorizing = true;
        d.auth_status = Some(ws.i18n.t(
            &format!("正在导入第 1/{} 个账户…", tokens.len()),
            &format!("Importing 1/{} accounts…", tokens.len()),
        ).to_string());
        d.error_message = None;
    }
    cx.notify();

    let weak = cx.entity().downgrade();
    let app_data = ws.paths.app_data.clone();

    cx.spawn(async move |_this, cx| {
        let total = tokens.len();
        let mut success_count = 0;
        let mut fail_count = 0;
        let mut imported_accounts = Vec::new();

        for token in tokens {
            match build_account_from_refresh_token(&token, None, label_opt.clone()) {
                Ok(acc) => {
                    success_count += 1;
                    imported_accounts.push(acc);
                }
                Err(e) => {
                    tracing::warn!("Failed to import token: {}", e);
                    fail_count += 1;
                }
            }
        }

        let _ = weak.update(cx, |ws: &mut Workspace, cx| {
            if !imported_accounts.is_empty() {
                let mut store = ws.ui.antigravity_store.clone().unwrap_or_default();
                for acc in imported_accounts {
                    if let Some(pos) = store.accounts.iter().position(|a| a.email == acc.email) {
                        store.accounts[pos] = acc;
                    } else {
                        store.accounts.push(acc);
                    }
                }
                let _ = save_store(&app_data, &store);
                ws.ui.antigravity_store = Some(store);
            }

            if success_count == total {
                ws.ui.antigravity_dialog = None;
                let msg = ws.i18n.t(
                    &format!("成功导入 {} 个账户", success_count),
                    &format!("Successfully imported {} accounts", success_count),
                ).to_string();
                ws.ui.toast(msg, false);
            } else if success_count > 0 {
                if let Some(d) = ws.ui.antigravity_dialog.as_mut() {
                    d.is_authorizing = false;
                    d.auth_status = Some(ws.i18n.t(
                        &format!("导入完成: {} 个成功, {} 个失败", success_count, fail_count),
                        &format!("Import finished: {} succeeded, {} failed", success_count, fail_count),
                    ).to_string());
                }
            } else {
                if let Some(d) = ws.ui.antigravity_dialog.as_mut() {
                    d.is_authorizing = false;
                    d.auth_status = None;
                    d.error_message = Some(ws.i18n.t("导入失败，未能验证 Token", "Import failed, token could not be verified").to_string());
                }
            }
            cx.notify();
        });
    }).detach();
}

/// One-click scan all local accounts from candidate DBs (state.vscdb), keyring, and legacy files
fn import_all_local_accounts_action(ws: &mut Workspace, cx: &mut Context<Workspace>) {
    if let Some(d) = ws.ui.antigravity_dialog.as_mut() {
        d.is_authorizing = true;
        d.auth_status = Some(ws.i18n.t("正在扫描本地已登录账号…", "Scanning local accounts…").to_string());
        d.error_message = None;
    }
    cx.notify();

    let weak = cx.entity().downgrade();
    let home_dir = ws.paths.home.clone();
    let app_data = ws.paths.app_data.clone();

    cx.spawn(async move |_this, cx| {
        let result = cx.background_spawn(async move {
            import_all_local_accounts(&home_dir)
        }).await;

        let _ = weak.update(cx, |ws: &mut Workspace, cx| {
            match result {
                Ok(accounts) => {
                    let count = accounts.len();
                    let mut store = ws.ui.antigravity_store.clone().unwrap_or_default();
                    for acc in accounts {
                        if let Some(pos) = store.accounts.iter().position(|a| a.email == acc.email) {
                            store.accounts[pos] = acc;
                        } else {
                            store.accounts.push(acc);
                        }
                    }
                    let _ = save_store(&app_data, &store);
                    ws.ui.antigravity_store = Some(store);
                    ws.ui.antigravity_dialog = None;

                    let msg = ws.i18n.t(
                        &format!("成功导入 {} 个账户", count),
                        &format!("Successfully imported {} accounts", count),
                    ).to_string();
                    ws.ui.toast(msg, false);
                }
                Err(e) => {
                    if let Some(d) = ws.ui.antigravity_dialog.as_mut() {
                        d.is_authorizing = false;
                        d.auth_status = None;
                        d.error_message = Some(e);
                    }
                }
            }
            cx.notify();
        });
    }).detach();
}

/// Open file picker for custom state.vscdb and import
fn import_custom_db_picker_action(ws: &mut Workspace, cx: &mut Context<Workspace>) {
    let weak = cx.entity().downgrade();
    let app_data = ws.paths.app_data.clone();

    cx.spawn(async move |_this, cx| {
        let picked = cx.background_spawn(async move {
            rfd::FileDialog::new()
                .set_title("选择 Antigravity state.vscdb 数据库文件")
                .add_filter("VSCode DB (*.vscdb)", &["vscdb"])
                .add_filter("所有文件 (*.*)", &["*"])
                .pick_file()
        }).await;

        let Some(path) = picked else { return };

        let result = cx.background_spawn(async move {
            import_from_custom_db_path(&path)
        }).await;

        let _ = weak.update(cx, |ws: &mut Workspace, cx| {
            match result {
                Ok(acc) => {
                    let email = acc.email.clone();
                    let mut store = ws.ui.antigravity_store.clone().unwrap_or_default();
                    if let Some(pos) = store.accounts.iter().position(|a| a.email == email) {
                        store.accounts[pos] = acc;
                    } else {
                        store.accounts.push(acc);
                    }
                    let _ = save_store(&app_data, &store);
                    ws.ui.antigravity_store = Some(store);
                    ws.ui.antigravity_dialog = None;

                    let msg = ws.i18n.t(
                        &format!("成功导入账户：{}", email),
                        &format!("Successfully imported: {}", email),
                    ).to_string();
                    ws.ui.toast(msg, false);
                }
                Err(e) => {
                    if let Some(d) = ws.ui.antigravity_dialog.as_mut() {
                        d.error_message = Some(format!("导入自定义数据库失败: {}", e));
                    }
                }
            }
            cx.notify();
        });
    }).detach();
}

/// Import accounts from V1 / legacy backup (~/.antigravity-agent)
fn import_v1_backup_action(ws: &mut Workspace, cx: &mut Context<Workspace>) {
    if let Some(d) = ws.ui.antigravity_dialog.as_mut() {
        d.is_authorizing = true;
        d.auth_status = Some(ws.i18n.t("正在扫描旧版备份数据…", "Scanning legacy backup data…").to_string());
        d.error_message = None;
    }
    cx.notify();

    let weak = cx.entity().downgrade();
    let home_dir = ws.paths.home.clone();
    let app_data = ws.paths.app_data.clone();

    cx.spawn(async move |_this, cx| {
        let result = cx.background_spawn(async move {
            import_from_v1_backup(&home_dir)
        }).await;

        let _ = weak.update(cx, |ws: &mut Workspace, cx| {
            match result {
                Ok(accounts) => {
                    let count = accounts.len();
                    let mut store = ws.ui.antigravity_store.clone().unwrap_or_default();
                    for acc in accounts {
                        if let Some(pos) = store.accounts.iter().position(|a| a.email == acc.email) {
                            store.accounts[pos] = acc;
                        } else {
                            store.accounts.push(acc);
                        }
                    }
                    let _ = save_store(&app_data, &store);
                    ws.ui.antigravity_store = Some(store);
                    ws.ui.antigravity_dialog = None;

                    let msg = ws.i18n.t(
                        &format!("成功导入 {} 个账户", count),
                        &format!("Successfully imported {} accounts", count),
                    ).to_string();
                    ws.ui.toast(msg, false);
                }
                Err(e) => {
                    if let Some(d) = ws.ui.antigravity_dialog.as_mut() {
                        d.is_authorizing = false;
                        d.auth_status = None;
                        d.error_message = Some(e);
                    }
                }
            }
            cx.notify();
        });
    }).detach();
}

/// Import active account from local system (Credential Manager or ~/.gemini/oauth_creds.json).
fn import_local_account_action(ws: &mut Workspace, cx: &mut Context<Workspace>) {
    let home = ws.paths.home.clone();
    let app_data = ws.paths.app_data.clone();
    let weak = cx.entity().downgrade();

    ws.ui.toast(
        ws.i18n.t("正在从本机导入 Antigravity 凭据…", "Importing local Antigravity credentials…"),
        false,
    );
    cx.notify();

    cx.spawn(async move |_this, cx| {
        let result = cx.background_spawn(async move {
            import_from_local_system(&home)
        }).await;

        let _ = weak.update(cx, |ws: &mut Workspace, cx| {
            match result {
                Ok(account) => {
                    let email = account.email.clone();
                    let mut store = ws.ui.antigravity_store.clone().unwrap_or_default();
                    if let Some(pos) = store.accounts.iter().position(|a| a.email == email) {
                        store.accounts[pos] = account;
                    } else {
                        store.accounts.push(account);
                    }
                    let _ = save_store(&app_data, &store);
                    ws.ui.antigravity_store = Some(store);

                    let msg = ws.i18n.t(
                        &format!("成功从本机导入账号：{}", email),
                        &format!("Imported local account: {}", email),
                    ).to_string();
                    ws.ui.toast(msg, false);
                }
                Err(e) => {
                    let msg = format!("从本机导入失败: {}", e);
                    ws.ui.toast(msg, true);
                }
            }
            cx.notify();
        });
    }).detach();
}

/// Import all accounts from Antigravity Manager (~/.antigravity_tools).
fn import_from_manager_action(ws: &mut Workspace, cx: &mut Context<Workspace>) {
    let home = ws.paths.home.clone();
    let app_data = ws.paths.app_data.clone();
    let weak = cx.entity().downgrade();

    ws.ui.toast(
        ws.i18n.t("正在从 Antigravity Manager 迁移账号…", "Importing accounts from Antigravity Manager…"),
        false,
    );
    cx.notify();

    cx.spawn(async move |_this, cx| {
        let result = cx.background_spawn(async move {
            aitoolplus_core::antigravity::import_from_antigravity_manager(&home)
        }).await;

        let _ = weak.update(cx, |ws: &mut Workspace, cx| {
            match result {
                Ok(imported) => {
                    let count = imported.len();
                    let mut store = ws.ui.antigravity_store.clone().unwrap_or_default();
                    for acc in imported {
                        if let Some(pos) = store.accounts.iter().position(|a| a.email == acc.email) {
                            store.accounts[pos] = acc;
                        } else {
                            store.accounts.push(acc);
                        }
                    }
                    let _ = save_store(&app_data, &store);
                    ws.ui.antigravity_store = Some(store);

                    let msg = ws.i18n.t(
                        &format!("成功从 Antigravity Manager 导入 {} 个账号！", count),
                        &format!("Successfully imported {} accounts from Antigravity Manager!", count),
                    ).to_string();
                    ws.ui.toast(msg, false);
                }
                Err(e) => {
                    let msg = format!("从 Antigravity Manager 导入失败: {}", e);
                    ws.ui.toast(msg, true);
                }
            }
            cx.notify();
        });
    }).detach();
}

/// Refresh quota for a single account.
fn refresh_account_quota_action(account_id: &str, ws: &mut Workspace, cx: &mut Context<Workspace>) {
    let account_id = account_id.to_string();
    let app_data = ws.paths.app_data.clone();
    let weak = cx.entity().downgrade();

    let account = match ws.ui.antigravity_store.as_ref().and_then(|s| s.get_account(&account_id)) {
        Some(a) => a.clone(),
        None => return,
    };

    ws.ui.toast(
        ws.i18n.t(&format!("正在刷新 {} 的用量…", account.email), &format!("Refreshing quota for {}…", account.email)),
        false,
    );
    cx.notify();

    cx.spawn(async move |_this, cx| {
        let result = cx.background_spawn(async move {
            let mut acc = account;
            aitoolplus_core::antigravity::ensure_fresh_token(&mut acc)?;
            if acc.project_id.is_none() || acc.tier.is_none() {
                let (project_id, tier) = fetch_project_and_tier(&acc.access_token);
                if project_id.is_some() {
                    acc.project_id = project_id;
                }
                if tier.is_some() {
                    acc.tier = tier;
                }
            }
            let quota = fetch_quota(&acc.access_token, acc.project_id.as_deref(), acc.quota.as_ref())?;
            acc.quota = Some(quota);
            Ok::<AntigravityAccount, String>(acc)
        }).await;

        let _ = weak.update(cx, |ws: &mut Workspace, cx| {
            match result {
                Ok(acc) => {
                    let mut store = ws.ui.antigravity_store.clone().unwrap_or_default();
                    if let Some(pos) = store.accounts.iter().position(|a| a.id == account_id) {
                        store.accounts[pos] = acc;
                        let _ = save_store(&app_data, &store);
                        ws.ui.antigravity_store = Some(store);
                    }
                    let msg = ws.i18n.t("配额已刷新", "Quota refreshed").to_string();
                    ws.ui.toast(msg, false);
                }
                Err(e) => {
                    let msg = format!("刷新配额失败: {}", e);
                    ws.ui.toast(msg, true);
                }
            }
            cx.notify();
        });
    }).detach();
}

/// Refresh quotas for all accounts in parallel.
fn refresh_all_quotas_action(ws: &mut Workspace, cx: &mut Context<Workspace>) {
    let store = match ws.ui.antigravity_store.clone() {
        Some(s) => s,
        None => return,
    };

    if store.accounts.is_empty() {
        return;
    }

    let accounts_to_refresh = store.accounts.clone();
    let app_data = ws.paths.app_data.clone();
    let weak = cx.entity().downgrade();

    ws.ui.toast(
        ws.i18n.t("正在刷新全部账号配额…", "Refreshing all account quotas…"),
        false,
    );
    cx.notify();

    cx.spawn(async move |_this, cx| {
        let updated_accounts = cx.background_spawn(async move {
            let mut accounts = accounts_to_refresh;
            std::thread::scope(|s| {
                let mut handles = Vec::new();
                for acc in &mut accounts {
                    handles.push(s.spawn(move || {
                        // Skip accounts that are already known to be forbidden due to TOS violation (aligned with Antigravity-Manager)
                        if let Some(quota) = &acc.quota {
                            if quota.is_forbidden {
                                if let Some(reason) = &quota.forbidden_reason {
                                    if reason.contains("TOS_VIOLATION") || reason.contains("violation of Terms of Service") {
                                        return;
                                    }
                                }
                            }
                        }
                        if let Ok(()) = aitoolplus_core::antigravity::ensure_fresh_token(acc) {
                            if acc.project_id.is_none() || acc.tier.is_none() {
                                let (pid, tier) = fetch_project_and_tier(&acc.access_token);
                                if pid.is_some() { acc.project_id = pid; }
                                if tier.is_some() { acc.tier = tier; }
                            }
                            if let Ok(q) = fetch_quota(&acc.access_token, acc.project_id.as_deref(), acc.quota.as_ref()) {
                                acc.quota = Some(q);
                            }
                        }
                    }));
                }
                for handle in handles {
                    let _ = handle.join();
                }
            });
            accounts
        }).await;

        let _ = weak.update(cx, |ws: &mut Workspace, cx| {
            if let Some(s) = &mut ws.ui.antigravity_store {
                s.accounts = updated_accounts;
                let _ = save_store(&app_data, s);
            }
            ws.settings.last_antigravity_refresh_time = Some(chrono::Utc::now().to_rfc3339());
            (ws.callbacks.save_settings)(&ws.settings);
            let msg = ws.i18n.t("全部账号配额已刷新", "All account quotas refreshed").to_string();
            ws.ui.toast(msg, false);
            cx.notify();
        });
    }).detach();
}

/// Switch active account with optional target (classic, IDE, CLI).
fn switch_account_target_action(
    account_id: &str,
    target: Option<&str>,
    ws: &mut Workspace,
    cx: &mut Context<Workspace>,
) {
    let home = ws.paths.home.clone();
    let app_data = ws.paths.app_data.clone();
    let mut store = match ws.ui.antigravity_store.clone() {
        Some(s) => s,
        None => return,
    };

    let account_email = store.get_account(account_id).map(|a| a.email.clone()).unwrap_or_default();
    let weak = cx.entity().downgrade();
    let account_id_str = account_id.to_string();
    let target_owned = target.map(String::from);

    let target_name = match target {
        Some("ide") => "IDE",
        Some("agy") => "CLI (agy)",
        _ => "App",
    };
    let target_name_en = match target {
        Some("ide") => "IDE",
        Some("agy") => "CLI (agy)",
        _ => "App",
    };

    ws.ui.toast(
        ws.i18n.t(
            &format!("正在切换账号到 {}：{}…", target_name, account_email),
            &format!("Switching account to {} for: {}…", target_name_en, account_email),
        ),
        false,
    );
    cx.notify();

    cx.spawn(async move |_this, cx| {
        let result = cx.background_spawn(async move {
            switch_account_target(&home, &app_data, &mut store, &account_id_str, target_owned.as_deref())?;
            Ok::<(AntigravityStore, String), String>((store, account_email))
        }).await;

        let _ = weak.update(cx, |ws: &mut Workspace, cx| {
            match result {
                Ok((new_store, email)) => {
                    ws.ui.antigravity_store = Some(new_store);
                    let msg = ws.i18n.t(
                        &format!("已成功切换 {} 活动账号为：{}", target_name, email),
                        &format!("Switched {} active account to: {}", target_name_en, email),
                    ).to_string();
                    ws.ui.toast(msg, false);
                }
                Err(e) => {
                    let msg = format!("切换账号失败: {}", e);
                    ws.ui.toast(msg, true);
                }
            }
            cx.notify();
        });
    }).detach();
}

/// Switch active account (defaults to classic).
#[allow(dead_code)]
fn switch_account_action(account_id: &str, ws: &mut Workspace, cx: &mut Context<Workspace>) {
    switch_account_target_action(account_id, None, ws, cx);
}

/// Toggle account disabled state.
fn toggle_disabled_action(account_id: &str, ws: &mut Workspace, cx: &mut Context<Workspace>) {
    let app_data = ws.paths.app_data.clone();
    let mut store = match ws.ui.antigravity_store.clone() {
        Some(s) => s,
        None => return,
    };

    match toggle_account_disabled(&app_data, &mut store, account_id) {
        Ok(disabled) => {
            let msg = if disabled {
                ws.i18n.t("账号已禁用", "Account disabled").to_string()
            } else {
                ws.i18n.t("账号已启用", "Account enabled").to_string()
            };
            ws.ui.antigravity_store = Some(store);
            ws.ui.toast(msg, false);
        }
        Err(e) => {
            let msg = format!("操作失败: {}", e);
            ws.ui.toast(msg, true);
        }
    }
    cx.notify();
}

/// Export account credentials as JSON to clipboard.
fn export_account_action(account: &AntigravityAccount, ws: &mut Workspace, cx: &mut Context<Workspace>) {
    match serde_json::to_string_pretty(account) {
        Ok(json) => {
            cx.write_to_clipboard(gpui::ClipboardItem::new_string(json));
            let msg = ws.i18n.t(
                &format!("已将账号 {} 凭据复制到剪贴板（JSON）", account.email),
                &format!("Copied credentials for {} to clipboard (JSON)", account.email),
            ).to_string();
            ws.ui.toast(msg, false);
        }
        Err(e) => {
            let msg = format!("导出账号凭据失败: {}", e);
            ws.ui.toast(msg, true);
        }
    }
    cx.notify();
}

/// Delete an account.
fn delete_account_action(
    account_id: &str,
    email: &str,
    ws: &mut Workspace,
    cx: &mut Context<Workspace>,
) {
    let i = ws.i18n;
    let title = i.t("删除 Antigravity 账号", "Delete Antigravity Account").to_string();
    let message = i.t(
        &format!("确定要删除账号「{}」吗？此操作不会影响您的 Google 账号本身。", email),
        &format!("Are you sure you want to delete '{}'? This will not affect your Google account.", email),
    ).to_string();

    ws.ui.confirm = Some(ConfirmState {
        title,
        message,
        action: ConfirmAction::DeleteAntigravityAccount {
            id: account_id.to_string(),
        },
    });
    cx.notify();
}

#[cfg(test)]
mod tests {
    use super::*;
    use aitoolplus_core::antigravity::{AntigravityAccount, AntigravityQuota, QuotaGroupInfo};

    #[test]
    fn test_extract_family_quotas_normal() {
        let mut account = AntigravityAccount::new(
            "acc-1".into(),
            "normal@example.com".into(),
            "access".into(),
            "refresh".into(),
            Utc::now().timestamp() + 3600,
        );
        let mut quota = AntigravityQuota::default();
        quota.quota_groups = vec![
            QuotaGroupInfo {
                display_name: "Gemini Models".into(),
                window: "5h".into(),
                remaining_fraction: 1.0,
                reset_time: "2026-09-22T14:45:12Z".into(),
                bucket_id: Some("gemini-5h".into()),
            },
            QuotaGroupInfo {
                display_name: "Gemini Models".into(),
                window: "weekly".into(),
                remaining_fraction: 0.85,
                reset_time: "2026-09-28T14:45:12Z".into(),
                bucket_id: Some("gemini-weekly".into()),
            },
            QuotaGroupInfo {
                display_name: "Claude and GPT models".into(),
                window: "5h".into(),
                remaining_fraction: 0.50,
                reset_time: "2026-09-22T14:45:12Z".into(),
                bucket_id: Some("3p-5h".into()),
            },
            QuotaGroupInfo {
                display_name: "Claude and GPT models".into(),
                window: "weekly".into(),
                remaining_fraction: 0.90,
                reset_time: "2026-09-28T14:45:12Z".into(),
                bucket_id: Some("3p-weekly".into()),
            },
        ];
        account.quota = Some(quota);

        let (gemini, claude) = extract_family_quotas(&account);

        let g_5h = gemini.five_hour.unwrap();
        assert_eq!(g_5h.percentage, 100);
        assert!(!g_5h.is_weekly_constrained);

        let g_w = gemini.weekly.unwrap();
        assert_eq!(g_w.percentage, 85);

        let c_5h = claude.five_hour.unwrap();
        assert_eq!(c_5h.percentage, 50);
        assert!(!c_5h.is_weekly_constrained);

        let c_w = claude.weekly.unwrap();
        assert_eq!(c_w.percentage, 90);
    }

    #[test]
    fn test_extract_family_quotas_weekly_constrained() {
        let mut account = AntigravityAccount::new(
            "acc-2".into(),
            "constrained@example.com".into(),
            "access".into(),
            "refresh".into(),
            Utc::now().timestamp() + 3600,
        );
        let future_reset = (Utc::now() + chrono::Duration::days(3)).to_rfc3339();
        let mut quota = AntigravityQuota::default();
        quota.quota_groups = vec![
            QuotaGroupInfo {
                display_name: "Gemini Models".into(),
                window: "5h".into(),
                remaining_fraction: 1.0, // 5-hour rolling bucket is 100%
                reset_time: "2026-09-22T14:45:12Z".into(),
                bucket_id: Some("gemini-5h".into()),
            },
            QuotaGroupInfo {
                display_name: "Gemini Models".into(),
                window: "weekly".into(),
                remaining_fraction: 0.0, // Weekly bucket is 0.0% (exhausted)
                reset_time: future_reset.clone(),
                bucket_id: Some("gemini-weekly".into()),
            },
        ];
        account.quota = Some(quota);

        let (gemini, _claude) = extract_family_quotas(&account);

        let g_5h = gemini.five_hour.unwrap();
        // Constraint: when weekly is 0%, 5h is forced to 0% and marked is_weekly_constrained
        assert!(g_5h.is_weekly_constrained);
        assert_eq!(g_5h.percentage, 0);
        assert_eq!(g_5h.weekly_reset_time, future_reset);
        assert_eq!(g_5h.reset_time, future_reset);

        let g_w = gemini.weekly.unwrap();
        assert_eq!(g_w.percentage, 0);
    }

    #[test]
    fn test_mask_email_privacy() {
        assert_eq!(mask_email("unifangg@gmail.com"), "uni***@gmail.com");
        assert_eq!(mask_email("ggvoking@gmail.com"), "ggv***@gmail.com");
        assert_eq!(mask_email("a@b.com"), "a***@b.com");
        assert_eq!(mask_email("ab@c.org"), "a***@c.org");
        assert_eq!(mask_email("invalid"), "inv***");
    }
}

