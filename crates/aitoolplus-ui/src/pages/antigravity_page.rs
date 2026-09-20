//! Antigravity account management page: list, add, refresh quotas, switch active credentials.
//!
//! STRICT CONSTRAINT: Zero reverse proxy, zero proxy server, zero proxy pool.
//! Pure client-side account management, quota inspection, and credential switching.

use std::time::Duration;
use aitoolplus_core::antigravity::{
    AntigravityAccount, AntigravityStore, ModelQuotaInfo, OAuthServerSession,
    build_account_from_refresh_token, exchange_auth_code, fetch_project_and_tier,
    fetch_quota, fetch_user_info, import_from_local_system, load_store, open_browser,
    save_store, switch_account,
};
use gpui::{Context, IntoElement, SharedString, div, prelude::*, px};
use chrono::{DateTime, Utc};

use crate::components::{
    BadgeKind, ButtonVariant, badge, button_l, button_with_icon_l, empty_state_svg,
    input_container, section_title,
};
use crate::icons::{
    CHECK_SVG, COPY_SVG, DOWNLOAD_SVG, GEMINI_SVG, PLUS_SVG, REFRESH_SVG, TRASH_SVG,
};
use crate::text_input::TextInput;
use crate::workspace::Workspace;

use super::{ConfirmAction, ConfirmState, modal_scaffold};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AntigravityDialogTab {
    #[default]
    GoogleAuth,
    RefreshToken,
}

#[derive(Clone)]
pub struct AntigravityDialogState {
    pub active_tab: AntigravityDialogTab,
    pub refresh_token: gpui::Entity<TextInput>,
    pub custom_label: gpui::Entity<TextInput>,
    pub auth_status: Option<String>,
    pub is_authorizing: bool,
    pub error_message: Option<String>,
}

/// Render the main Antigravity page.
pub fn render_antigravity_page(ws: &mut Workspace, cx: &mut Context<Workspace>) -> gpui::AnyElement {
    let t = ws.theme.clone();
    let i = ws.i18n;

    // Lazily load store from disk if not loaded yet
    if ws.ui.antigravity_store.is_none() {
        let store = load_store(&ws.paths.app_data);
        ws.ui.antigravity_store = Some(store);
    }

    let store = ws.ui.antigravity_store.clone().unwrap_or_default();
    let accounts = &store.accounts;

    let add_label = i.t("添加账号", "Add Account");
    let import_label = i.t("从本机导入", "Import Local");
    let refresh_all_label = i.t("刷新全部用量", "Refresh All Quotas");

    let mut section = div()
        .flex()
        .flex_col()
        .w_full()
        .min_w(px(0.0))
        .gap(px(14.0))
        .child(section_title(
            &t,
            i.t("Antigravity 账号管理", "Antigravity Accounts"),
            Some(i.t(
                &format!("共 {} 个账号", accounts.len()),
                &format!("{} accounts total", accounts.len()),
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
                        )),
                )
                .child(
                    div()
                        .w(px(280.0))
                        .child(input_container(&t, ws.ui.antigravity_search.clone())),
                ),
        );

    let query = ws.ui.antigravity_search.read(cx).text().to_lowercase();
    let filtered_accounts: Vec<_> = accounts
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

    if filtered_accounts.is_empty() {
        section = section.child(empty_state_svg(
            &t,
            GEMINI_SVG,
            if query.is_empty() {
                i.t("还没有添加 Antigravity 账号", "No Antigravity accounts yet")
            } else {
                i.t("没有找到匹配的账号", "No matching accounts found")
            },
            if query.is_empty() {
                i.t(
                    "点击上方「添加账号」进行 Google 授权或输入 Refresh Token，也可以「从本机导入」当前凭据",
                    "Click 'Add Account' or 'Import Local' above to get started",
                )
            } else {
                i.t("尝试更换搜索关键词", "Try a different search keyword")
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

/// Render an individual account card with credentials, tier, and quota dashboard.
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

    // Tier badge kind and text
    let tier_str = account.tier.as_deref().unwrap_or("FREE");
    let (tier_kind, tier_text) = match tier_str {
        "ULTRA" => (BadgeKind::Warning, "ULTRA"),
        "PRO" => (BadgeKind::Success, "PRO"),
        "ENTERPRISE" => (BadgeKind::Accent, "ENTERPRISE"),
        _ => (BadgeKind::Neutral, "FREE"),
    };

    // Header row
    let header_row = div()
        .flex()
        .items_center()
        .justify_between()
        .gap(px(10.0))
        .flex_wrap()
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(12.0))
                .child(
                    div()
                        .size(px(36.0))
                        .rounded_full()
                        .bg(crate::rgba_const(0x3186ff22))
                        .flex()
                        .items_center()
                        .justify_center()
                        .flex_shrink_0()
                        .child(
                            gpui::svg()
                                .data(GEMINI_SVG)
                                .size(px(20.0))
                                .text_color(crate::rgba_const(0x3186ffff)),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(2.0))
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(8.0))
                                .child(
                                    div()
                                        .text_size(px(14.0))
                                        .font_weight(gpui::FontWeight::BOLD)
                                        .text_color(t.text_primary)
                                        .child(account.email.clone()),
                                )
                                .child(badge(&t, tier_text, tier_kind))
                                .when(is_active, |s| {
                                    s.child(badge(&t, i.t("当前活动", "Active"), BadgeKind::Success))
                                }),
                        )
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(8.0))
                                .text_size(px(12.0))
                                .text_color(t.text_muted)
                                .child(
                                    account.name.as_deref().unwrap_or("Google User").to_string(),
                                )
                                .when_some(account.custom_label.as_ref(), |s, label| {
                                    s.child(format!("• {}", label))
                                })
                                .when_some(account.project_id.as_ref(), |s, pid| {
                                    s.child(format!("• 项目: {}", pid))
                                }),
                        ),
                ),
        )
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(6.0))
                .when(!is_active, |s| {
                    let switch_id = acc_id.clone();
                    s.child(button_with_icon_l(
                        SharedString::from(format!("switch-{}", switch_id)),
                        CHECK_SVG,
                        i.t("切换为此账号", "Switch to this"),
                        ButtonVariant::Primary,
                        &t,
                        cx,
                        move |ws, _, _, cx| {
                            switch_account_action(&switch_id, ws, cx);
                        },
                    ))
                })
                .child({
                    let ref_id = acc_id.clone();
                    button_with_icon_l(
                        SharedString::from(format!("refresh-{}", ref_id)),
                        REFRESH_SVG,
                        i.t("刷新用量", "Refresh"),
                        ButtonVariant::Secondary,
                        &t,
                        cx,
                        move |ws, _, _, cx| {
                            refresh_account_quota_action(&ref_id, ws, cx);
                        },
                    )
                })
                .child({
                    let r_token = account.refresh_token.clone();
                    button_with_icon_l(
                        SharedString::from(format!("copy-{}", acc_id)),
                        COPY_SVG,
                        i.t("复制 Token", "Copy Token"),
                        ButtonVariant::Ghost,
                        &t,
                        cx,
                        move |ws, _, _, cx| {
                            cx.write_to_clipboard(gpui::ClipboardItem::new_string(r_token.clone()));
                            let msg = ws.i18n.t("已复制 Refresh Token 到剪贴板", "Refresh token copied to clipboard").to_string();
                            ws.ui.toast(msg, false);
                            cx.notify();
                        },
                    )
                })
                .child({
                    let del_id = acc_id.clone();
                    let del_email = email.clone();
                    button_with_icon_l(
                        SharedString::from(format!("delete-{}", del_id)),
                        TRASH_SVG,
                        i.t("删除", "Delete"),
                        ButtonVariant::Danger,
                        &t,
                        cx,
                        move |ws, _, _, cx| {
                            delete_account_action(&del_id, &del_email, ws, cx);
                        },
                    )
                }),
        );

    // Quota section
    let quota_view = render_account_quota(account, ws);

    div()
        .id(SharedString::from(format!("account-card-{}", account.id)))
        .flex()
        .flex_col()
        .gap(px(12.0))
        .p(px(16.0))
        .rounded(px(10.0))
        .bg(t.card_bg)
        .border_1()
        .border_color(if is_active { t.accent } else { t.card_border })
        .shadow_sm()
        .child(header_row)
        .child(quota_view)
        .into_any_element()
}

/// Render the quota dashboard inside an account card.
fn render_account_quota(account: &AntigravityAccount, ws: &Workspace) -> gpui::AnyElement {
    let t = &ws.theme;
    let i = &ws.i18n;

    let quota = match &account.quota {
        Some(q) => q,
        None => {
            return div()
                .p(px(12.0))
                .rounded(px(8.0))
                .bg(t.sidebar_bg)
                .text_size(px(12.0))
                .text_color(t.text_muted)
                .child(i.t(
                    "暂未获取配额信息，可点击右上角「刷新用量」查询实时额度。",
                    "No quota information yet. Click 'Refresh' to check current limits.",
                ))
                .into_any_element();
        }
    };

    if quota.is_forbidden {
        let reason = quota.forbidden_reason.as_deref().unwrap_or("API 403 Forbidden");
        return div()
            .p(px(12.0))
            .rounded(px(8.0))
            .bg(t.danger_subtle)
            .border_1()
            .border_color(t.danger)
            .flex()
            .flex_col()
            .gap(px(4.0))
            .child(
                div()
                    .text_size(px(13.0))
                    .font_weight(gpui::FontWeight::BOLD)
                    .text_color(t.danger)
                    .child(i.t("配额接口访问受限 (403)", "Quota Access Restricted (403)")),
            )
            .child(
                div()
                    .text_size(px(12.0))
                    .text_color(t.text_primary)
                    .child(reason.to_string()),
            )
            .into_any_element();
    }

    let mut content = div().flex().flex_col().gap(px(10.0));

    // Summary windows (5h & weekly)
    let mut summary_row = div().flex().items_center().gap(px(16.0)).flex_wrap();

    if let Some(w5h) = quota.window_5h {
        let pct = (w5h * 100.0).round() as i32;
        summary_row = summary_row.child(render_window_meter(
            &ws.theme,
            i.t("5 小时窗口", "5h Window"),
            pct,
        ));
    }

    if let Some(ww) = quota.window_weekly {
        let pct = (ww * 100.0).round() as i32;
        summary_row = summary_row.child(render_window_meter(
            &ws.theme,
            i.t("每周配额", "Weekly Window"),
            pct,
        ));
    }

    // Last updated timestamp
    let updated_time = DateTime::from_timestamp(quota.last_updated, 0)
        .map(|dt| dt.with_timezone(&chrono::Local).format("%Y-%m-%d %H:%M:%S").to_string())
        .unwrap_or_default();

    summary_row = summary_row.child(
        div()
            .text_size(px(11.5))
            .text_color(t.text_muted)
            .child(format!("{} {}", i.t("更新于", "Updated at"), updated_time)),
    );

    content = content.child(summary_row);

    // Models quota grid
    if !quota.models.is_empty() {
        let mut grid = div()
            .flex()
            .flex_wrap()
            .gap(px(8.0));

        for m in &quota.models {
            grid = grid.child(render_model_quota_item(m, ws));
        }

        content = content.child(grid);
    }

    div()
        .p(px(12.0))
        .rounded(px(8.0))
        .bg(t.sidebar_bg)
        .child(content)
        .into_any_element()
}

/// Render a single window meter pill.
fn render_window_meter(t: &crate::theme::Theme, label: SharedString, percentage: i32) -> gpui::AnyElement {
    let bar_color = if percentage > 50 {
        t.success
    } else if percentage > 20 {
        t.warning
    } else {
        t.danger
    };

    div()
        .flex()
        .items_center()
        .gap(px(8.0))
        .px(px(10.0))
        .py(px(4.0))
        .rounded(px(6.0))
        .bg(t.card_bg)
        .border_1()
        .border_color(t.card_border)
        .child(
            div()
                .text_size(px(12.0))
                .font_weight(gpui::FontWeight::MEDIUM)
                .text_color(t.text_secondary)
                .child(label),
        )
        .child(
            div()
                .w(px(60.0))
                .h(px(6.0))
                .rounded(px(3.0))
                .bg(t.card_border)
                .overflow_hidden()
                .child(
                    div()
                        .w(gpui::relative(percentage.clamp(0, 100) as f32 / 100.0))
                        .h_full()
                        .bg(bar_color)
                        .rounded(px(3.0)),
                ),
        )
        .child(
            div()
                .text_size(px(12.0))
                .font_weight(gpui::FontWeight::BOLD)
                .text_color(bar_color)
                .child(format!("{}%", percentage)),
        )
        .into_any_element()
}

/// Render a model quota box with progress bar and countdown.
fn render_model_quota_item(m: &ModelQuotaInfo, ws: &Workspace) -> gpui::AnyElement {
    let t = &ws.theme;
    let name = m.display_name.as_deref().unwrap_or(&m.name);
    let pct = m.percentage.clamp(0, 100);

    let bar_color = if pct > 50 {
        t.success
    } else if pct > 20 {
        t.warning
    } else {
        t.danger
    };

    // Calculate reset countdown if available
    let reset_text = if !m.reset_time.is_empty() {
        if let Ok(reset_dt) = DateTime::parse_from_rfc3339(&m.reset_time) {
            let now = Utc::now();
            let duration = reset_dt.with_timezone(&Utc).signed_duration_since(now);
            if duration.num_seconds() > 0 {
                let hours = duration.num_hours();
                let mins = duration.num_minutes() % 60;
                if hours > 0 {
                    format!("重置于 {}h{}m 后", hours, mins)
                } else {
                    format!("重置于 {}m 后", mins)
                }
            } else {
                "已重置".to_string()
            }
        } else {
            "".to_string()
        }
    } else {
        "".to_string()
    };

    div()
        .w(px(240.0))
        .p(px(8.0))
        .rounded(px(6.0))
        .bg(t.card_bg)
        .border_1()
        .border_color(t.card_border)
        .flex()
        .flex_col()
        .gap(px(4.0))
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
        .when(!reset_text.is_empty(), |s| {
            s.child(
                div()
                    .text_size(px(10.5))
                    .text_color(t.text_muted)
                    .child(reset_text),
            )
        })
        .into_any_element()
}

// ---------------------------------------------------------------------------
// Actions
// ---------------------------------------------------------------------------

/// Open Add Account modal dialog.
pub fn open_add_account_dialog(
    ws: &mut Workspace,
    window: &mut gpui::Window,
    cx: &mut Context<Workspace>,
) {
    let refresh_token = cx.new(|cx| TextInput::new("输入 Google OAuth Refresh Token…", cx));
    let custom_label = cx.new(|cx| TextInput::new("自定义备注（例如：工作主账号、备用账号）…", cx));

    ws.ui.antigravity_dialog = Some(AntigravityDialogState {
        active_tab: AntigravityDialogTab::GoogleAuth,
        refresh_token: refresh_token.clone(),
        custom_label,
        auth_status: None,
        is_authorizing: false,
        error_message: None,
    });

    refresh_token.update(cx, |input, cx| {
        input.focus_handle.focus(window, cx);
        input.start_blink(cx);
    });

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

    let tab_google = i.t("Google 授权登录", "Google OAuth");
    let tab_token = i.t("输入 Refresh Token", "Manual Refresh Token");

    let is_google = state.active_tab == AntigravityDialogTab::GoogleAuth;

    let tab_bar = div()
        .flex()
        .items_center()
        .gap(px(8.0))
        .mb(px(14.0))
        .child(
            div()
                .id("ag-tab-google")
                .cursor_pointer()
                .px(px(14.0))
                .py(px(6.0))
                .rounded(px(6.0))
                .text_size(px(13.0))
                .font_weight(gpui::FontWeight::MEDIUM)
                .when(is_google, |s| {
                    s.bg(t.accent_subtle).text_color(t.accent).font_weight(gpui::FontWeight::SEMIBOLD)
                })
                .when(!is_google, |s| {
                    s.text_color(t.text_secondary).hover(|h| h.bg(t.card_hover))
                })
                .on_click(cx.listener(|ws, _, _, cx| {
                    if let Some(d) = &mut ws.ui.antigravity_dialog {
                        d.active_tab = AntigravityDialogTab::GoogleAuth;
                        d.error_message = None;
                        cx.notify();
                    }
                }))
                .child(tab_google),
        )
        .child(
            div()
                .id("ag-tab-token")
                .cursor_pointer()
                .px(px(14.0))
                .py(px(6.0))
                .rounded(px(6.0))
                .text_size(px(13.0))
                .font_weight(gpui::FontWeight::MEDIUM)
                .when(!is_google, |s| {
                    s.bg(t.accent_subtle).text_color(t.accent).font_weight(gpui::FontWeight::SEMIBOLD)
                })
                .when(is_google, |s| {
                    s.text_color(t.text_secondary).hover(|h| h.bg(t.card_hover))
                })
                .on_click(cx.listener(|ws, _, _, cx| {
                    if let Some(d) = &mut ws.ui.antigravity_dialog {
                        d.active_tab = AntigravityDialogTab::RefreshToken;
                        d.error_message = None;
                        cx.notify();
                    }
                }))
                .child(tab_token),
        );

    let body = if is_google {
        // Tab 1: Google OAuth
        div()
            .flex()
            .flex_col()
            .gap(px(14.0))
            .child(tab_bar)
            .child(
                div()
                    .text_size(px(13.0))
                    .text_color(t.text_secondary)
                    .child(i.t(
                        "点击下方按钮将启动本地安全授权服务，并在默认浏览器中打开 Google 授权页面。授权完成后将自动完成账号绑定。",
                        "Click the button below to start a local authorization server and open the Google sign-in page in your browser.",
                    )),
            )
            .when_some(state.auth_status.as_ref(), |s, status| {
                s.child(
                    div()
                        .p(px(12.0))
                        .rounded(px(8.0))
                        .bg(t.accent_subtle)
                        .border_1()
                        .border_color(t.accent)
                        .text_size(px(12.5))
                        .text_color(t.accent)
                        .child(status.clone()),
                )
            })
            .when_some(state.error_message.as_ref(), |s, err| {
                s.child(
                    div()
                        .p(px(12.0))
                        .rounded(px(8.0))
                        .bg(t.danger_subtle)
                        .border_1()
                        .border_color(t.danger)
                        .text_size(px(12.5))
                        .text_color(t.danger)
                        .child(err.clone()),
                )
            })
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
                            ws.ui.antigravity_dialog = None;
                            cx.notify();
                        },
                    ))
                    .child(button_l(
                        "ag-dlg-start-auth",
                        if state.is_authorizing {
                            i.t("等待授权中…", "Waiting for auth…")
                        } else {
                            i.t("启动浏览器授权", "Start Browser OAuth")
                        },
                        ButtonVariant::Primary,
                        &t,
                        cx,
                        |ws, _, _, cx| {
                            start_oauth_flow_action(ws, cx);
                        },
                    )),
            )
    } else {
        // Tab 2: Manual Refresh Token
        div()
            .flex()
            .flex_col()
            .gap(px(12.0))
            .child(tab_bar)
            .child(
                div()
                    .text_size(px(13.0))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .text_color(t.text_primary)
                    .child(i.t("Google Refresh Token *", "Google Refresh Token *")),
            )
            .child(input_container(&t, state.refresh_token.clone()))
            .child(
                div()
                    .text_size(px(13.0))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .text_color(t.text_primary)
                    .child(i.t("账号备注（可选）", "Custom Label (Optional)")),
            )
            .child(input_container(&t, state.custom_label.clone()))
            .when_some(state.error_message.as_ref(), |s, err| {
                s.child(
                    div()
                        .p(px(10.0))
                        .rounded(px(6.0))
                        .bg(t.danger_subtle)
                        .border_1()
                        .border_color(t.danger)
                        .text_size(px(12.0))
                        .text_color(t.danger)
                        .child(err.clone()),
                )
            })
            .child(
                div()
                    .flex()
                    .justify_end()
                    .gap(px(8.0))
                    .mt(px(8.0))
                    .child(button_l(
                        "ag-dlg-cancel-token",
                        i.t("取消", "Cancel"),
                        ButtonVariant::Secondary,
                        &t,
                        cx,
                        |ws, _, _, cx| {
                            ws.ui.antigravity_dialog = None;
                            cx.notify();
                        },
                    ))
                    .child(button_l(
                        "ag-dlg-submit-token",
                        i.t("验证并导入", "Validate & Import"),
                        ButtonVariant::Primary,
                        &t,
                        cx,
                        |ws, _, _, cx| {
                            submit_refresh_token_action(ws, cx);
                        },
                    )),
            )
    };

    modal_scaffold(
        &t,
        &i.t("添加 Antigravity 账号", "Add Antigravity Account"),
        body.into_any_element(),
        cx,
        |ws, _, _, cx| {
            ws.ui.antigravity_dialog = None;
            cx.notify();
        },
    )
}

/// Start Google OAuth authorization flow.
fn start_oauth_flow_action(ws: &mut Workspace, cx: &mut Context<Workspace>) {
    if let Some(d) = &mut ws.ui.antigravity_dialog {
        d.is_authorizing = true;
        d.auth_status = Some("正在启动本地授权服务…".to_string());
        d.error_message = None;
    }
    cx.notify();

    let weak = cx.entity().downgrade();
    let app_data = ws.paths.app_data.clone();

    cx.spawn(async move |_this, cx| {
        let session = match OAuthServerSession::start() {
            Ok(s) => s,
            Err(e) => {
                let _ = weak.update(cx, |ws: &mut Workspace, cx| {
                    if let Some(d) = &mut ws.ui.antigravity_dialog {
                        d.is_authorizing = false;
                        d.error_message = Some(format!("启动本地监听失败: {}", e));
                    }
                    cx.notify();
                });
                return;
            }
        };

        let auth_url = session.auth_url.clone();
        let port = session.port;

        // Open browser
        open_browser(&auth_url);

        let _ = weak.update(cx, |ws: &mut Workspace, cx| {
            if let Some(d) = &mut ws.ui.antigravity_dialog {
                d.auth_status = Some(format!(
                    "已在浏览器中打开授权页面 (本地端口: {})，请在浏览器中完成登录…",
                    port
                ));
            }
            cx.notify();
        });

        // Await code in background
        let exchange_result = cx.background_spawn(async move {
            let code = session.wait_for_code(Duration::from_secs(180))?;
            let redirect_uri = format!("http://127.0.0.1:{}/oauth-callback", port);
            let (access_token, refresh_token, expires_in) = exchange_auth_code(&code, &redirect_uri)?;
            let expiry_timestamp = Utc::now().timestamp() + expires_in;

            let user_info = fetch_user_info(&access_token)?;
            let (project_id, tier) = fetch_project_and_tier(&access_token);
            let quota = fetch_quota(&access_token).ok();

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
                    // If account already exists with same email, update it
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
                    if let Some(d) = &mut ws.ui.antigravity_dialog {
                        d.is_authorizing = false;
                        d.error_message = Some(format!("授权失败: {}", e));
                    }
                }
            }
            cx.notify();
        });
    }).detach();
}

/// Submit manual refresh token.
fn submit_refresh_token_action(ws: &mut Workspace, cx: &mut Context<Workspace>) {
    let state = match &ws.ui.antigravity_dialog {
        Some(s) => s,
        None => return,
    };

    let token = state.refresh_token.read(cx).text().trim().to_string();
    let label = {
        let l = state.custom_label.read(cx).text().trim().to_string();
        if l.is_empty() { None } else { Some(l) }
    };

    if token.is_empty() {
        if let Some(d) = &mut ws.ui.antigravity_dialog {
            d.error_message = Some("请输入 Refresh Token".to_string());
        }
        cx.notify();
        return;
    }

    let weak = cx.entity().downgrade();
    let app_data = ws.paths.app_data.clone();

    cx.spawn(async move |_this, cx| {
        let result = cx.background_spawn(async move {
            build_account_from_refresh_token(&token, None, label)
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
                    ws.ui.antigravity_dialog = None;

                    let msg = ws.i18n.t(
                        &format!("已成功添加账号：{}", email),
                        &format!("Account added: {}", email),
                    ).to_string();
                    ws.ui.toast(msg, false);
                }
                Err(e) => {
                    if let Some(d) = &mut ws.ui.antigravity_dialog {
                        d.error_message = Some(format!("验证失败: {}", e));
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
            let quota = fetch_quota(&acc.access_token)?;
            let (project_id, tier) = fetch_project_and_tier(&acc.access_token);
            acc.quota = Some(quota);
            if project_id.is_some() {
                acc.project_id = project_id;
            }
            if tier.is_some() {
                acc.tier = tier;
            }
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

/// Refresh quotas for all accounts.
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
            for acc in &mut accounts {
                if let Ok(()) = aitoolplus_core::antigravity::ensure_fresh_token(acc) {
                    if let Ok(q) = fetch_quota(&acc.access_token) {
                        acc.quota = Some(q);
                    }
                    let (pid, tier) = fetch_project_and_tier(&acc.access_token);
                    if pid.is_some() { acc.project_id = pid; }
                    if tier.is_some() { acc.tier = tier; }
                }
            }
            accounts
        }).await;

        let _ = weak.update(cx, |ws: &mut Workspace, cx| {
            if let Some(s) = &mut ws.ui.antigravity_store {
                s.accounts = updated_accounts;
                let _ = save_store(&app_data, s);
            }
            let msg = ws.i18n.t("全部账号配额已刷新", "All account quotas refreshed").to_string();
            ws.ui.toast(msg, false);
            cx.notify();
        });
    }).detach();
}

/// Switch active account.
fn switch_account_action(account_id: &str, ws: &mut Workspace, cx: &mut Context<Workspace>) {
    let home = ws.paths.home.clone();
    let app_data = ws.paths.app_data.clone();
    let mut store = match ws.ui.antigravity_store.clone() {
        Some(s) => s,
        None => return,
    };

    let account_email = store.get_account(account_id).map(|a| a.email.clone()).unwrap_or_default();
    let weak = cx.entity().downgrade();
    let account_id_str = account_id.to_string();

    cx.spawn(async move |_this, cx| {
        let result = cx.background_spawn(async move {
            switch_account(&home, &app_data, &mut store, &account_id_str)?;
            Ok::<(AntigravityStore, String), String>((store, account_email))
        }).await;

        let _ = weak.update(cx, |ws: &mut Workspace, cx| {
            match result {
                Ok((new_store, email)) => {
                    ws.ui.antigravity_store = Some(new_store);
                    let msg = ws.i18n.t(
                        &format!("已成功切换活动账号为：{}", email),
                        &format!("Switched active account to: {}", email),
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
