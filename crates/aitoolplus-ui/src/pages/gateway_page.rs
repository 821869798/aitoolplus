use gpui::{Context, IntoElement, MouseButton, div, prelude::*, px};

use crate::components::{ButtonVariant, Tooltip, button_l, button_with_icon_l, button_with_icon_loading_l, toggle};
use crate::icons::{ALERT_SVG, CHECK_SVG, COPY_SVG, svg_icon};
use crate::theme::Theme;
use crate::workspace::Workspace;
use aitoolplus_core::gateway::types::{GatewayCliKey, GatewayProxyMode};

pub fn render_gateway_page(ws: &mut Workspace, cx: &mut Context<Workspace>) -> gpui::AnyElement {
    // Initial fetch if gateway_status is none
    if ws.ui.gateway_status.is_none() && !ws.ui.gateway_busy {
        ws.refresh_gateway_data(cx);
    }

    let header = render_gateway_header(ws, cx);
    let tab_bar = render_gateway_tabs(ws, cx);
    let body = match ws.ui.gateway_tab {
        super::GatewayTab::Overview => render_overview_tab(ws, cx),
        super::GatewayTab::Failover => render_failover_tab(ws, cx),
        super::GatewayTab::Requests => render_requests_tab(ws, cx),
        super::GatewayTab::Settings => render_settings_tab(ws, cx),
    };

    div()
        .flex()
        .flex_col()
        .w_full()
        .gap(px(16.0))
        .pb(px(32.0))
        .child(header)
        .child(tab_bar)
        .child(body)
        .into_any_element()
}

// ---------------------------------------------------------------------------
// Header
// ---------------------------------------------------------------------------

fn render_gateway_header(ws: &Workspace, cx: &mut Context<Workspace>) -> gpui::AnyElement {
    let t = &ws.theme;
    let i = &ws.i18n;
    let status = ws.ui.gateway_status.clone().unwrap_or_default();
    let is_running = status.running;

    let status_pill = if is_running {
        div()
            .flex()
            .items_center()
            .gap(px(6.0))
            .px(px(10.0))
            .py(px(4.0))
            .rounded(px(12.0))
            .bg(t.success.opacity(0.12))
            .border_1()
            .border_color(t.success.opacity(0.3))
            .child(
                div()
                    .w(px(7.0))
                    .h(px(7.0))
                    .rounded_full()
                    .bg(t.success)
            )
            .child(
                div()
                    .text_xs()
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .text_color(t.success)
                    .child(format!("运行中 ({}:{})", status.host, status.port))
            )
    } else {
        div()
            .flex()
            .items_center()
            .gap(px(6.0))
            .px(px(10.0))
            .py(px(4.0))
            .rounded(px(12.0))
            .bg(t.card_bg)
            .border_1()
            .border_color(t.card_border)
            .child(
                div()
                    .w(px(7.0))
                    .h(px(7.0))
                    .rounded_full()
                    .bg(t.text_muted)
            )
            .child(
                div()
                    .text_xs()
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .text_color(t.text_muted)
                    .child("已停止")
            )
    };

    let gateway_toggle = div()
        .flex()
        .items_center()
        .gap(px(8.0))
        .py(px(4.0))
        .px(px(10.0))
        .rounded(px(6.0))
        .bg(t.card_bg)
        .border_1()
        .border_color(if is_running { t.success.opacity(0.35) } else { t.card_border })
        .child(
            div()
                .text_xs()
                .font_weight(gpui::FontWeight::MEDIUM)
                .text_color(if is_running { t.success } else { t.text_muted })
                .child(if is_running { "已开启网关" } else { "网关已停止" })
        )
        .child(
            toggle(
                "tg-gateway-master",
                is_running,
                t,
                cx,
                move |ws, _, _, cx| {
                    if is_running {
                        ws.stop_gateway_server(cx);
                    } else {
                        ws.start_gateway_server(cx);
                    }
                },
            )
        );

    let failover_enabled = ws.ui.gateway_settings.failover_enabled;
    let failover_toggle = if is_running {
        Some(
            div()
                .flex()
                .items_center()
                .gap(px(8.0))
                .py(px(4.0))
                .px(px(10.0))
                .rounded(px(6.0))
                .bg(t.card_bg)
                .border_1()
                .border_color(if failover_enabled { t.accent.opacity(0.35) } else { t.card_border })
                .child(
                    div()
                        .text_xs()
                        .font_weight(gpui::FontWeight::MEDIUM)
                        .text_color(if failover_enabled { t.accent } else { t.text_muted })
                        .child(if failover_enabled { "故障转移已开启" } else { "故障转移已关闭" })
                )
                .child(
                    toggle(
                        "tg-gateway-failover-header",
                        failover_enabled,
                        t,
                        cx,
                        |ws, _, _, cx| {
                            ws.ui.gateway_settings.failover_enabled = !ws.ui.gateway_settings.failover_enabled;
                            ws.persist_gateway_settings(cx);
                            if ws.ui.gateway_settings.failover_enabled {
                                ws.ui.toast("已开启自动故障转移", false);
                            } else {
                                ws.ui.toast("已关闭故障转移，所有请求仅路由至 P0 主提供商", false);
                            }
                            cx.notify();
                        },
                    )
                ),
        )
    } else {
        None
    };

    let refresh_btn = button_with_icon_loading_l(
        "btn-refresh-gw",
        crate::icons::REFRESH_SVG,
        i.raw("刷新", "Refresh"),
        ButtonVariant::Secondary,
        ws.ui.gateway_busy,
        t,
        cx,
        |ws, _, _, cx| {
            ws.refresh_gateway_data(cx);
        },
    );

    div()
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
                .flex_col()
                .gap(px(4.0))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(10.0))
                        .child(
                            crate::icons::svg_icon(crate::icons::SERVER_SVG, px(22.0), t.accent)
                        )
                        .child(
                            div()
                                .text_xl()
                                .font_weight(gpui::FontWeight::BOLD)
                                .text_color(t.text_primary)
                                .child("本地代理网关")
                        )
                        .child(status_pill)
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(t.text_muted)
                        .child("统一协议转换 · 跨提供商自动故障转移 · 请求审计与令牌统计")
                )
        )
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(8.0))
                .child(refresh_btn)
                .when_some(failover_toggle, |el, ft| el.child(ft))
                .child(gateway_toggle)
        )
        .into_any_element()
}

// ---------------------------------------------------------------------------
// Tabs
// ---------------------------------------------------------------------------

fn render_gateway_tabs(ws: &Workspace, cx: &mut Context<Workspace>) -> gpui::AnyElement {
    let t = &ws.theme;
    let current_tab = ws.ui.gateway_tab;

    let tabs = [
        (super::GatewayTab::Overview, "概览与接管"),
        (super::GatewayTab::Failover, "故障转移队列"),
        (super::GatewayTab::Requests, "请求审计"),
        (super::GatewayTab::Settings, "网关设置"),
    ];

    div()
        .flex()
        .items_center()
        .gap(px(8.0))
        .pb(px(8.0))
        .border_b_1()
        .border_color(t.card_border)
        .children(tabs.iter().map(|(tab, label)| {
            let is_active = current_tab == *tab;
            let tab_id = format!("gw-tab-{:?}", tab);
            let target_tab = *tab;

            div()
                .id(gpui::ElementId::Name(tab_id.into()))
                .cursor_pointer()
                .px(px(12.0))
                .py(px(6.0))
                .rounded(px(6.0))
                .bg(if is_active { t.accent.opacity(0.12) } else { t.card_bg })
                .border_1()
                .border_color(if is_active { t.accent.opacity(0.4) } else { t.card_border })
                .text_sm()
                .font_weight(if is_active { gpui::FontWeight::SEMIBOLD } else { gpui::FontWeight::NORMAL })
                .text_color(if is_active { t.accent } else { t.text_primary })
                .child(*label)
                .on_mouse_down(MouseButton::Left, cx.listener(move |ws, _, _, cx| {
                    ws.ui.gateway_tab = target_tab;
                    cx.notify();
                }))
        }))
        .into_any_element()
}

// ---------------------------------------------------------------------------
// Tab 1: Overview
// ---------------------------------------------------------------------------

fn render_overview_tab(ws: &Workspace, cx: &mut Context<Workspace>) -> gpui::AnyElement {
    let t = &ws.theme;
    let status = ws.ui.gateway_status.clone().unwrap_or_default();

    // 4 Stat Cards
    let uptime_str = if status.running {
        let mins = status.uptime_seconds / 60;
        let secs = status.uptime_seconds % 60;
        format!("{mins}分 {secs}秒")
    } else {
        "已停机".to_string()
    };

    let stat_cards = div()
        .flex()
        .gap(px(12.0))
        .w_full()
        .child(render_stat_card("服务状态", if status.running { "正常运行" } else { "已停止" }, format!("{}:{}", status.host, status.port), t))
        .child(render_stat_card("运行时间", &uptime_str, "本地环回", t))
        .child(render_stat_card("请求总数", &status.total_requests.to_string(), "总接入流量", t))
        .child(render_stat_card("成功 / 失败", &format!("{} / {}", status.success_requests, status.failed_requests), "故障容灾已就绪", t));

    // CLI Takeover Cards
    let takeover_section = render_cli_takeover_section(status.running, ws, cx);

    div()
        .flex()
        .flex_col()
        .gap(px(16.0))
        .w_full()
        .child(stat_cards)
        .child(takeover_section)
        .into_any_element()
}

fn render_stat_card(label: &str, value: &str, subtext: impl Into<String>, t: &Theme) -> impl IntoElement {
    div()
        .flex_1()
        .p(px(14.0))
        .rounded(px(8.0))
        .bg(t.card_bg)
        .border_1()
        .border_color(t.card_border)
        .flex()
        .flex_col()
        .gap(px(4.0))
        .child(
            div()
                .text_xs()
                .text_color(t.text_muted)
                .child(label.to_string())
        )
        .child(
            div()
                .text_lg()
                .font_weight(gpui::FontWeight::BOLD)
                .text_color(t.text_primary)
                .child(value.to_string())
        )
        .child(
            div()
                .text_xs()
                .text_color(t.text_muted)
                .child(subtext.into())
        )
}

fn render_cli_takeover_section(is_running: bool, ws: &Workspace, cx: &mut Context<Workspace>) -> impl IntoElement {
    let t = &ws.theme;
    let takeovers = ws.ui.gateway_takeovers.clone();
    let port = ws.ui.gateway_status.as_ref().map(|s| s.port).unwrap_or(15721);

    let mut takeover_rows = Vec::new();
    for item in &takeovers {
        takeover_rows.push(render_cli_takeover_row(item, port, is_running, ws, cx));
    }

    div()
        .flex()
        .flex_col()
        .gap(px(12.0))
        .p(px(16.0))
        .rounded(px(8.0))
        .bg(t.card_bg)
        .border_1()
        .border_color(t.card_border)
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(4.0))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(8.0))
                        .child(
                            div()
                                .text_base()
                                .font_weight(gpui::FontWeight::BOLD)
                                .text_color(t.text_primary)
                                .child("CLI 工具一键接管")
                        )
                        .when(!is_running, |el| {
                            el.child(
                                div()
                                    .px(px(6.0))
                                    .py(px(2.0))
                                    .rounded(px(4.0))
                                    .bg(t.card_border.opacity(0.4))
                                    .text_xs()
                                    .font_weight(gpui::FontWeight::MEDIUM)
                                    .text_color(t.text_muted)
                                    .child("网关已停止 · 接管已暂停")
                            )
                        })
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(t.text_muted)
                        .child("一键修改 CLI 运行时配置，将官方请求定向至本地网关。配置接管自动备份原文件，可随时平滑还原。")
                )
        )
        .when(!is_running, |el| {
            el.child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .px(px(12.0))
                    .py(px(8.0))
                    .rounded(px(6.0))
                    .bg(t.card_border.opacity(0.25))
                    .border_1()
                    .border_color(t.card_border)
                    .child(svg_icon(ALERT_SVG, px(14.0), t.text_muted))
                    .child(
                        div()
                            .text_xs()
                            .text_color(t.text_muted)
                            .child("本地网关处于停止状态，下方 CLI 接管已失效并灰置锁定。请先在页面右上角开启网关。")
                    )
            )
        })
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(10.0))
                .children(takeover_rows)
        )
}

fn render_cli_takeover_row(
    status: &aitoolplus_core::gateway::GatewayCliTakeoverStatus,
    port: u16,
    is_running: bool,
    ws: &Workspace,
    cx: &mut Context<Workspace>,
) -> gpui::AnyElement {
    let t = &ws.theme;
    let cli = status.cli_key;
    let is_enabled = status.enabled;

    let endpoint = aitoolplus_core::gateway::cli_proxy::cli_gateway_endpoint(cli, port);
    let target_file_desc = match cli {
        GatewayCliKey::Claude => "~/.claude/settings.json",
        GatewayCliKey::Codex => "~/.codex/config.toml & auth.json",
        _ => "CLI config",
    };

    let is_aggregate = status.mode == aitoolplus_core::gateway::GatewayProxyMode::Aggregate;

    let status_badge = if !is_running {
        div()
            .flex()
            .items_center()
            .gap(px(4.0))
            .px(px(8.0))
            .py(px(2.0))
            .rounded(px(4.0))
            .bg(t.input_bg)
            .text_xs()
            .text_color(t.text_muted)
            .child(if is_enabled { "已接管 (网关未运行)" } else { "官方直连" })
    } else if is_enabled {
        div()
            .flex()
            .items_center()
            .gap(px(4.0))
            .px(px(8.0))
            .py(px(2.0))
            .rounded(px(4.0))
            .bg(t.success.opacity(0.15))
            .text_xs()
            .text_color(t.success)
            .child(if is_aggregate { "网关接管中 (聚合模式)" } else { "网关接管中" })
    } else {
        div()
            .flex()
            .items_center()
            .gap(px(4.0))
            .px(px(8.0))
            .py(px(2.0))
            .rounded(px(4.0))
            .bg(t.input_bg)
            .text_xs()
            .text_color(t.text_muted)
            .child("官方直连")
    };

    let toggle_button = if is_running {
        toggle(
            format!("tg-takeover-{}", cli.as_str()),
            is_enabled,
            t,
            cx,
            move |ws, _, _, cx| {
                if is_enabled {
                    ws.restore_cli_takeover(cli, cx);
                } else {
                    ws.engage_cli_takeover(cli, cx);
                }
            },
        )
    } else {
        // Disabled & grayed-out toggle switch
        div()
            .id(gpui::ElementId::Name(format!("tg-takeover-dis-{}", cli.as_str()).into()))
            .cursor_not_allowed()
            .w(px(38.0))
            .h(px(22.0))
            .p(px(2.5))
            .rounded_full()
            .flex()
            .flex_none()
            .items_center()
            .bg(if is_enabled { t.track_on.opacity(0.35) } else { t.track_off.opacity(0.4) })
            .when(is_enabled, |s| s.justify_end())
            .when(!is_enabled, |s| s.justify_start())
            .child(
                div()
                    .size(px(17.0))
                    .rounded_full()
                    .bg(t.thumb.opacity(0.55))
                    .shadow_sm()
            )
            .on_mouse_down(MouseButton::Left, cx.listener(|ws, _, _, _| {
                ws.ui.toast("本地网关当前已停止，请先在右上角开启网关总开关后再启用接管", true);
            }))
            .into_any_element()
    };

    let takeover_control = div()
        .flex()
        .items_center()
        .gap(px(10.0))
        .when(cli == GatewayCliKey::Codex && is_enabled, |el| {
            if is_running {
                el.child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(4.0))
                        .p(px(2.0))
                        .rounded(px(6.0))
                        .bg(t.input_bg)
                        .border_1()
                        .border_color(t.card_border)
                        .child(
                            div()
                                .cursor_pointer()
                                .px(px(8.0))
                                .py(px(2.0))
                                .rounded(px(4.0))
                                .bg(if !is_aggregate { t.accent.opacity(0.2) } else { t.accent.opacity(0.0) })
                                .text_xs()
                                .font_weight(gpui::FontWeight::MEDIUM)
                                .text_color(if !is_aggregate { t.accent } else { t.text_muted })
                                .hover(|s| s.bg(t.accent.opacity(0.15)))
                                .child("容灾模式")
                                .on_mouse_down(MouseButton::Left, cx.listener(move |ws, _, _, cx| {
                                    if is_aggregate {
                                        ws.switch_gateway_cli_mode(GatewayCliKey::Codex, aitoolplus_core::gateway::GatewayProxyMode::Failover, cx);
                                    }
                                }))
                        )
                        .child(
                            div()
                                .cursor_pointer()
                                .px(px(8.0))
                                .py(px(2.0))
                                .rounded(px(4.0))
                                .bg(if is_aggregate { t.accent.opacity(0.2) } else { t.accent.opacity(0.0) })
                                .text_xs()
                                .font_weight(gpui::FontWeight::MEDIUM)
                                .text_color(if is_aggregate { t.accent } else { t.text_muted })
                                .hover(|s| s.bg(t.accent.opacity(0.15)))
                                .child("聚合模式")
                                .on_mouse_down(MouseButton::Left, cx.listener(move |ws, _, _, cx| {
                                    if !is_aggregate {
                                        ws.switch_gateway_cli_mode(GatewayCliKey::Codex, aitoolplus_core::gateway::GatewayProxyMode::Aggregate, cx);
                                    }
                                }))
                        )
                )
            } else {
                el.child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(4.0))
                        .p(px(2.0))
                        .rounded(px(6.0))
                        .bg(t.input_bg.opacity(0.5))
                        .border_1()
                        .border_color(t.card_border.opacity(0.5))
                        .cursor_not_allowed()
                        .child(
                            div()
                                .px(px(8.0))
                                .py(px(2.0))
                                .rounded(px(4.0))
                                .text_xs()
                                .font_weight(gpui::FontWeight::MEDIUM)
                                .text_color(t.text_muted)
                                .child(if !is_aggregate { "容灾模式 (挂起)" } else { "聚合模式 (挂起)" })
                        )
                        .on_mouse_down(MouseButton::Left, cx.listener(|ws, _, _, _| {
                            ws.ui.toast("本地网关已停止，请先开启网关", true);
                        }))
                )
            }
        })
        .child(
            div()
                .text_xs()
                .font_weight(gpui::FontWeight::MEDIUM)
                .text_color(if is_running && is_enabled { t.success } else { t.text_muted })
                .child(if !is_running {
                    if is_enabled { "已接管 (网关已停止)" } else { "未接管" }
                } else if is_enabled {
                    "已接管代理"
                } else {
                    "未接管"
                })
        )
        .child(toggle_button);

    div()
        .flex()
        .items_center()
        .justify_between()
        .p(px(12.0))
        .rounded(px(6.0))
        .bg(t.card_bg)
        .border_1()
        .border_color(t.card_border)
        .when(!is_running, |el| el.opacity(0.62))
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
                                .font_weight(gpui::FontWeight::BOLD)
                                .text_sm()
                                .text_color(if is_running { t.text_primary } else { t.text_secondary })
                                .child(cli.display_name())
                        )
                        .child(status_badge)
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(t.text_muted)
                        .child(format!("目标文件: {target_file_desc} · 接口: {endpoint}"))
                )
        )
        .child(takeover_control)
        .into_any_element()
}

// ---------------------------------------------------------------------------
// Tab 2: Failover
// ---------------------------------------------------------------------------

fn render_failover_tab(ws: &Workspace, cx: &mut Context<Workspace>) -> gpui::AnyElement {
    let t = &ws.theme;
    let failover_enabled = ws.ui.gateway_settings.failover_enabled;

    div()
        .flex()
        .flex_col()
        .gap(px(16.0))
        .child(
            div()
                .p(px(12.0))
                .rounded(px(6.0))
                .bg(t.accent.opacity(0.08))
                .border_1()
                .border_color(t.accent.opacity(0.2))
                .child(
                    div()
                        .text_xs()
                        .text_color(t.text_primary)
                        .child("自动故障转移规则：当主提供商（P0）在输出首个 Token 前遭遇 429 限流、5xx 服务器错误或超时，网关将无感切换至后续可用提供商并记录审计日志。熔断器将在提供商连续失败后进入熔断保护，防止持续卡顿。")
                )
        )
        .when(!failover_enabled, |el| {
            el.child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(10.0))
                    .p(px(12.0))
                    .rounded(px(6.0))
                    .bg(t.warning.opacity(0.1))
                    .border_1()
                    .border_color(t.warning.opacity(0.35))
                    .child(svg_icon(ALERT_SVG, px(16.0), t.warning))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(2.0))
                            .child(
                                div()
                                    .text_xs()
                                    .font_weight(gpui::FontWeight::BOLD)
                                    .text_color(t.warning)
                                    .child("故障转移当前未开启")
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(t.text_secondary)
                                    .child("所有请求严格仅由各工具的 P0 主提供商响应；如需开启跨提供商自动容灾，可在右上角网关开启后切换【故障转移】开关。")
                            )
                    )
            )
        })
        .child(render_tool_failover_card(GatewayCliKey::Claude, ws, cx))
        .child(render_tool_failover_card(GatewayCliKey::Codex, ws, cx))
        .into_any_element()
}

fn render_tool_failover_card(
    cli: GatewayCliKey,
    ws: &Workspace,
    cx: &mut Context<Workspace>,
) -> impl IntoElement {
    let t = &ws.theme;
    let failover_enabled = ws.ui.gateway_settings.failover_enabled;
    let cli_label = cli.as_str();
    let paths = aitoolplus_core::Paths::system();
    let candidates = aitoolplus_core::gateway::GatewayRouter::resolve_candidates(
        &paths,
        cli,
        GatewayProxyMode::Failover,
        None,
    );

    let total = candidates.len();
    let rows = if candidates.is_empty() {
        div()
            .p(px(12.0))
            .text_xs()
            .text_color(t.text_muted)
            .child("未配置可用提供商，请先在左侧工具页中添加提供商")
            .into_any_element()
    } else {
        let mut list = div().flex().flex_col().gap(px(6.0));
        for (idx, target) in candidates.iter().enumerate() {
            // Drop indicator line BEFORE this candidate (when dragging UP towards idx)
            if let Some(crate::pages::DragReorderState::GatewayFailover { cli: d_cli, from_pos, hover_pos: Some(h_pos) }) = ws.ui.drag_reorder {
                if d_cli == cli && h_pos == idx && from_pos > idx {
                    list = list.child(crate::pages::render_drop_indicator_line(t));
                }
            }

            let rank_label = if idx == 0 {
                "P0 (主)".to_string()
            } else {
                format!("P{} (备)", idx)
            };
            let pid_set_p0 = target.provider_id.clone();
            let is_this_being_dragged = matches!(
                ws.ui.drag_reorder,
                Some(crate::pages::DragReorderState::GatewayFailover { cli: d_cli, from_pos, .. }) if d_cli == cli && from_pos == idx
            );

            let card = div()
                .id(gpui::SharedString::from(format!("failover-card-{}-{}-{}", cli_label, idx, target.provider_id)))
                .flex()
                .items_center()
                .justify_between()
                .p(px(8.0))
                .rounded(px(6.0))
                .bg(if is_this_being_dragged { t.input_bg.opacity(0.55) } else { t.input_bg })
                .border_1()
                .border_color(if is_this_being_dragged {
                    t.accent
                } else if idx == 0 {
                    t.accent.opacity(0.3)
                } else {
                    t.card_border
                })
                .when(is_this_being_dragged, |el| el.opacity(0.65))
                .when(!failover_enabled && idx > 0, |el| el.opacity(0.65))
                .on_mouse_move(cx.listener(move |ws, _, _, cx| {
                    if let Some(crate::pages::DragReorderState::GatewayFailover { cli: d_cli, from_pos, hover_pos }) = ws.ui.drag_reorder {
                        if d_cli == cli && hover_pos != Some(idx) {
                            ws.ui.drag_reorder = Some(crate::pages::DragReorderState::GatewayFailover {
                                cli,
                                from_pos,
                                hover_pos: Some(idx),
                            });
                            cx.notify();
                        }
                    }
                }))
                .on_mouse_up(MouseButton::Left, cx.listener(move |ws, _, _, cx| {
                    if let Some(crate::pages::DragReorderState::GatewayFailover { cli: d_cli, from_pos, hover_pos }) = ws.ui.drag_reorder.take() {
                        if d_cli == cli {
                            let to = hover_pos.unwrap_or(idx);
                            if from_pos != to {
                                ws.reorder_gateway_failover_candidate(cli, from_pos, to, cx);
                            }
                        }
                        cx.notify();
                    }
                }))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(8.0))
                        .child(
                            div()
                                .id(gpui::SharedString::from(format!("drag-failover-{}-{}", cli_label, target.provider_id)))
                                .flex()
                                .items_center()
                                .gap(px(5.0))
                                .px(px(6.0))
                                .py(px(3.0))
                                .rounded(px(5.0))
                                .bg(if is_this_being_dragged {
                                    t.accent.opacity(0.2)
                                } else if idx == 0 {
                                    t.accent.opacity(0.12)
                                } else {
                                    t.card_border.opacity(0.35)
                                })
                                .border_1()
                                .border_color(if is_this_being_dragged || idx == 0 { t.accent.opacity(0.3) } else { t.card_border })
                                .cursor_grab()
                                .hover(|s| s.bg(t.card_hover).border_color(t.accent))
                                .active(|s| s.cursor_grabbing())
                                .on_mouse_down(MouseButton::Left, cx.listener(move |ws, _, _, cx| {
                                    ws.ui.drag_reorder = Some(crate::pages::DragReorderState::GatewayFailover {
                                        cli,
                                        from_pos: idx,
                                        hover_pos: Some(idx),
                                    });
                                    cx.notify();
                                }))
                                .tooltip({
                                    let tip = format!("{} · 拖拽调整顺序", rank_label);
                                    move |_win, cx| cx.new(|_| Tooltip::new(tip.clone())).into()
                                    })
                                .child(
                                    gpui::svg()
                                        .data(crate::icons::GRIP_VERTICAL_SVG)
                                        .size(px(13.0))
                                        .text_color(if idx == 0 || is_this_being_dragged { t.accent } else { t.text_muted })
                                        .hover(|s| s.text_color(t.accent)),
                                )
                                .child(
                                    div()
                                        .text_xs()
                                        .font_weight(gpui::FontWeight::BOLD)
                                        .text_color(if idx == 0 || is_this_being_dragged { t.accent } else { t.text_secondary })
                                        .child(rank_label),
                                ),
                        )
                        .child(
                            div()
                                .text_sm()
                                .font_weight(gpui::FontWeight::MEDIUM)
                                .text_color(t.text_primary)
                                .child(target.provider_name.clone())
                        )
                        .child(
                            div()
                                .text_xs()
                                .text_color(t.text_muted)
                                .child(format!("({})", target.base_url))
                        )
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(6.0))
                        .child(
                            div()
                                .px(px(6.0))
                                .py(px(2.0))
                                .rounded(px(4.0))
                                .bg(t.card_bg)
                                .border_1()
                                .border_color(t.card_border)
                                .text_xs()
                                .text_color(t.text_muted)
                                .child(format!("{:?}", target.target_protocol))
                        )
                        // Set as P0 button for secondary providers
                        .when(idx > 0, |el| {
                            el.child(
                                div()
                                    .cursor_pointer()
                                    .px(px(8.0))
                                    .py(px(2.0))
                                    .rounded(px(4.0))
                                    .bg(t.accent.opacity(0.12))
                                    .border_1()
                                    .border_color(t.accent.opacity(0.3))
                                    .text_xs()
                                    .font_weight(gpui::FontWeight::MEDIUM)
                                    .text_color(t.accent)
                                    .hover(|s| s.bg(t.accent.opacity(0.25)))
                                    .flex()
                                    .items_center()
                                    .gap(px(3.0))
                                    .child(
                                        gpui::svg()
                                            .data(crate::icons::ZAP_SVG)
                                            .size(px(10.0))
                                            .text_color(t.accent),
                                    )
                                    .child("设为 P0")
                                    .on_mouse_down(MouseButton::Left, cx.listener(move |ws, _, _, cx| {
                                        ws.set_gateway_primary_provider(cli, &pid_set_p0, cx);
                                    }))
                            )
                        })
                        .child(
                            if idx == 0 {
                                div()
                                    .px(px(6.0))
                                    .py(px(2.0))
                                    .rounded(px(4.0))
                                    .bg(t.success.opacity(0.12))
                                    .text_xs()
                                    .font_weight(gpui::FontWeight::MEDIUM)
                                    .text_color(t.success)
                                    .child(if failover_enabled { "主渠道" } else { "主渠道 (唯一生效)" })
                            } else if failover_enabled {
                                div()
                                    .px(px(6.0))
                                    .py(px(2.0))
                                    .rounded(px(4.0))
                                    .bg(t.success.opacity(0.12))
                                    .text_xs()
                                    .text_color(t.success)
                                    .child("正常")
                            } else {
                                div()
                                    .px(px(6.0))
                                    .py(px(2.0))
                                    .rounded(px(4.0))
                                    .bg(t.card_border.opacity(0.3))
                                    .text_xs()
                                    .text_color(t.text_muted)
                                    .child("备用 (未启用转移)")
                            }
                        )
                );
            list = list.child(card);

            // Drop indicator line AFTER this candidate (when dragging DOWN towards idx)
            if let Some(crate::pages::DragReorderState::GatewayFailover { cli: d_cli, from_pos, hover_pos: Some(h_pos) }) = ws.ui.drag_reorder {
                if d_cli == cli && h_pos == idx && from_pos < idx {
                    list = list.child(crate::pages::render_drop_indicator_line(t));
                }
            }
        }

        // Bottom drop indicator
        if let Some(crate::pages::DragReorderState::GatewayFailover { cli: d_cli, hover_pos: Some(h_pos), .. }) = ws.ui.drag_reorder {
            if d_cli == cli && h_pos >= total {
                list = list.child(crate::pages::render_drop_indicator_line(t));
            }
        }

        // Bottom end zone
        if let Some(crate::pages::DragReorderState::GatewayFailover { cli: d_cli, .. }) = ws.ui.drag_reorder {
            if d_cli == cli {
                list = list.child(
                    div()
                        .h(px(26.0))
                        .w_full()
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(px(5.0))
                        .border_1()
                        .border_dashed()
                        .border_color(t.accent.opacity(0.3))
                        .bg(t.accent.opacity(0.04))
                        .text_xs()
                        .text_color(t.accent)
                        .child("移至末尾备用")
                        .on_mouse_move(cx.listener(move |ws, _, _, cx| {
                            if let Some(crate::pages::DragReorderState::GatewayFailover { cli: d_cli, from_pos, hover_pos }) = ws.ui.drag_reorder {
                                if d_cli == cli && hover_pos != Some(total) {
                                    ws.ui.drag_reorder = Some(crate::pages::DragReorderState::GatewayFailover {
                                        cli,
                                        from_pos,
                                        hover_pos: Some(total),
                                    });
                                    cx.notify();
                                }
                            }
                        }))
                        .on_mouse_up(MouseButton::Left, cx.listener(move |ws, _, _, cx| {
                            if let Some(crate::pages::DragReorderState::GatewayFailover { cli: d_cli, from_pos, .. }) = ws.ui.drag_reorder.take() {
                                if d_cli == cli {
                                    ws.reorder_gateway_failover_candidate(cli, from_pos, total, cx);
                                }
                                cx.notify();
                            }
                        }))
                );
            }
        }

        list.into_any_element()
    };

    let takeover_status = ws.ui.gateway_takeovers.iter().find(|s| s.cli_key == cli);
    let is_aggregate = takeover_status.map(|s| s.mode == GatewayProxyMode::Aggregate).unwrap_or(false);

    div()
        .flex()
        .flex_col()
        .gap(px(8.0))
        .p(px(14.0))
        .rounded(px(8.0))
        .bg(t.card_bg)
        .border_1()
        .border_color(t.card_border)
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(8.0))
                        .child(
                            div()
                                .text_sm()
                                .font_weight(gpui::FontWeight::BOLD)
                                .text_color(t.text_primary)
                                .child(format!("{} 容灾队列", cli.display_name()))
                        )
                        .when(!failover_enabled, |el| {
                            el.child(
                                div()
                                    .px(px(6.0))
                                    .py(px(1.5))
                                    .rounded(px(4.0))
                                    .bg(t.card_border.opacity(0.4))
                                    .text_xs()
                                    .font_weight(gpui::FontWeight::MEDIUM)
                                    .text_color(t.text_muted)
                                    .child("故障转移已关闭")
                            )
                        })
                        .when(cli == GatewayCliKey::Codex && is_aggregate, |el| {
                            el.child(
                                div()
                                    .px(px(6.0))
                                    .py(px(1.0))
                                    .rounded(px(4.0))
                                    .bg(t.accent.opacity(0.15))
                                    .text_xs()
                                    .font_weight(gpui::FontWeight::MEDIUM)
                                    .text_color(t.accent)
                                    .child("聚合模式运行中")
                            )
                        })
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(t.text_muted)
                        .child(format!("候选提供商: {} 个", candidates.len()))
                )
        )
        .when(cli == GatewayCliKey::Codex && is_aggregate, |el| {
            el.child(
                div()
                    .p(px(8.0))
                    .rounded(px(4.0))
                    .bg(t.accent.opacity(0.08))
                    .border_1()
                    .border_color(t.accent.opacity(0.2))
                    .text_xs()
                    .text_color(t.accent)
                    .child("Codex 处于聚合模式：所有供应商已统一汇聚至模型目录（如 anyrouter.claude-3-7-sonnet），可直接在 Codex 中按前缀指定调用各家模型。下表顺序作为默认 P0 主机及故障转移兜底次序。")
            )
        })
        .child(rows)
}

// ---------------------------------------------------------------------------
// Tab 3: Requests
// ---------------------------------------------------------------------------

fn render_requests_tab(ws: &Workspace, cx: &mut Context<Workspace>) -> gpui::AnyElement {
    let t = &ws.theme;
    let requests = ws.ui.gateway_requests.clone();

    let toolbar = div()
        .flex()
        .items_center()
        .justify_between()
        .w_full()
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(8.0))
                .child(
                    div()
                        .text_sm()
                        .font_weight(gpui::FontWeight::BOLD)
                        .text_color(t.text_primary)
                        .child(format!("实时审计日志 ({} 条)", requests.len()))
                )
        )
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(8.0))
                .child(
                    button_l(
                        "btn-refresh-logs",
                        "刷新日志",
                        ButtonVariant::Secondary,
                        t,
                        cx,
                        |ws, _, _, cx| {
                            ws.refresh_gateway_requests(cx);
                        },
                    )
                )
                .child(
                    button_l(
                        "btn-clear-logs",
                        "清空日志",
                        ButtonVariant::Danger,
                        t,
                        cx,
                        |ws, _, _, cx| {
                            ws.clear_gateway_logs(cx);
                        },
                    )
                )
        );

    let table = if requests.is_empty() {
        div()
            .p(px(24.0))
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap(px(6.0))
            .rounded(px(8.0))
            .bg(t.card_bg)
            .border_1()
            .border_color(t.card_border)
            .child(
                div()
                    .text_sm()
                    .text_color(t.text_muted)
                    .child("暂无请求记录")
            )
            .child(
                div()
                    .text_xs()
                    .text_color(t.text_muted)
                    .child("通过接管后的 CLI 发起一次请求，网关将实时记录调用与性能指标")
            )
            .into_any_element()
    } else {
        let mut request_rows = Vec::new();
        for item in &requests {
            request_rows.push(render_request_table_row(item, ws, cx));
        }

        div()
            .flex()
            .flex_col()
            .rounded(px(8.0))
            .bg(t.card_bg)
            .border_1()
            .border_color(t.card_border)
            .overflow_hidden()
            .child(render_request_table_header(t))
            .children(request_rows)
            .into_any_element()
    };

    div()
        .flex()
        .flex_col()
        .gap(px(12.0))
        .child(toolbar)
        .child(table)
        .into_any_element()
}

fn render_request_table_header(t: &Theme) -> impl IntoElement {
    div()
        .flex()
        .items_center()
        .px(px(12.0))
        .py(px(8.0))
        .bg(t.card_bg)
        .border_b_1()
        .border_color(t.card_border)
        .text_xs()
        .font_weight(gpui::FontWeight::SEMIBOLD)
        .text_color(t.text_muted)
        .child(div().w(px(140.0)).child("时间"))
        .child(div().w(px(80.0)).child("CLI"))
        .child(div().w(px(140.0)).child("提供商"))
        .child(div().w(px(130.0)).child("模型"))
        .child(div().w(px(80.0)).child("状态"))
        .child(div().w(px(80.0)).child("耗时"))
        .child(div().w(px(100.0)).child("Token"))
        .child(div().flex_1().child("动作"))
}

fn render_request_table_row(
    item: &aitoolplus_core::gateway::GatewayRequestLogSummary,
    ws: &Workspace,
    cx: &mut Context<Workspace>,
) -> gpui::AnyElement {
    let t = &ws.theme;
    let req_id = item.id.clone();
    let is_ok = item.status_code < 400;

    let time_str = chrono::DateTime::from_timestamp(item.timestamp, 0)
        .map(|dt| dt.with_timezone(&chrono::Local).format("%m-%d %H:%M:%S").to_string())
        .unwrap_or_else(|| "未知时间".into());

    let status_badge = div()
        .px(px(6.0))
        .py(px(2.0))
        .rounded(px(4.0))
        .bg(if is_ok { t.success.opacity(0.12) } else { t.danger.opacity(0.12) })
        .text_xs()
        .font_weight(gpui::FontWeight::SEMIBOLD)
        .text_color(if is_ok { t.success } else { t.danger })
        .child(format!("{}", item.status_code));

    div()
        .flex()
        .items_center()
        .px(px(12.0))
        .py(px(8.0))
        .border_b_1()
        .border_color(t.card_border)
        .hover(|s| s.bg(t.card_hover))
        .child(div().w(px(140.0)).text_xs().text_color(t.text_muted).child(time_str))
        .child(
            div()
                .w(px(140.0))
                .flex()
                .items_center()
                .gap(px(4.0))
                .child(
                    div()
                        .text_xs()
                        .text_color(t.text_primary)
                        .child(item.provider_name.clone())
                )
                .when(item.failover, |el| {
                    el.child(
                        div()
                            .px(px(4.0))
                            .py(px(0.5))
                            .rounded(px(3.0))
                            .bg(t.warning.opacity(0.15))
                            .text_xs()
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(t.warning)
                            .child("⚡容灾")
                    )
                })
        )
        .child(div().w(px(130.0)).text_xs().text_color(t.text_muted).child(item.model.clone()))
        .child(div().w(px(80.0)).child(status_badge))
        .child(div().w(px(80.0)).text_xs().text_color(t.text_muted).child(format!("{}ms", item.duration_ms)))
        .child(div().w(px(100.0)).text_xs().text_color(t.text_muted).child(format!("{}/{}", item.input_tokens, item.output_tokens)))
        .child(
            div()
                .flex_1()
                .child(
                    div()
                        .cursor_pointer()
                        .text_xs()
                        .text_color(t.accent)
                        .hover(|s| s.underline())
                        .child("查看详情")
                        .on_mouse_down(MouseButton::Left, cx.listener(move |ws, _, _, cx| {
                            ws.inspect_gateway_request(&req_id, cx);
                        }))
                )
        )
        .into_any_element()
}

fn format_json_pretty(raw: &str) -> String {
    if let Ok(val) = serde_json::from_str::<serde_json::Value>(raw) {
        serde_json::to_string_pretty(&val).unwrap_or_else(|_| raw.to_string())
    } else {
        raw.to_string()
    }
}

fn prepare_display_payload(raw: &str) -> (String, bool, usize, usize) {
    let formatted = format_json_pretty(raw);
    let total_bytes = raw.len();
    let lines: Vec<&str> = formatted.lines().collect();
    let total_lines = lines.len();
    if lines.len() > 300 {
        let preview = lines[..300].join("\n");
        (preview, true, total_bytes, total_lines)
    } else if formatted.len() > 24000 {
        let preview = formatted[..24000].to_string();
        (preview, true, total_bytes, total_lines)
    } else {
        (formatted, false, total_bytes, total_lines)
    }
}

pub fn render_request_detail_modal(
    detail: &aitoolplus_core::gateway::GatewayRequestLogDetail,
    ws: &Workspace,
    cx: &mut Context<Workspace>,
) -> gpui::AnyElement {
    let t = ws.theme.clone();
    let summary = &detail.summary;
    let is_ok = summary.status_code < 400;

    let time_str = chrono::DateTime::from_timestamp(summary.timestamp, 0)
        .map(|dt| dt.with_timezone(&chrono::Local).format("%Y-%m-%d %H:%M:%S").to_string())
        .unwrap_or_else(|| "未知时间".into());

    let req_id_copy = detail.id.clone();
    let short_id = if detail.id.len() > 8 {
        &detail.id[..8]
    } else {
        &detail.id
    };

    // Prepare Request Body
    let raw_inbound = detail.inbound_body.clone().unwrap_or_default();
    let has_inbound = !raw_inbound.is_empty();
    let (inbound_display, inbound_truncated, inbound_bytes, inbound_lines) = if has_inbound {
        prepare_display_payload(&raw_inbound)
    } else {
        ("(空客户端请求体)".to_string(), false, 0, 0)
    };
    let copy_inbound = raw_inbound.clone();

    // Prepare Response Body
    let raw_response = detail.client_response_body.clone().unwrap_or_default();
    let has_response = !raw_response.is_empty();
    let (response_display, response_truncated, response_bytes, response_lines) = if has_response {
        prepare_display_payload(&raw_response)
    } else {
        ("(空响应体)".to_string(), false, 0, 0)
    };
    let copy_response = raw_response.clone();

    let title = format!("请求审计详情 ({})", short_id);

    // Failover Trace Section
    let attempts_section = if !detail.attempts.is_empty() || summary.failover {
        let mut trace_rows = Vec::new();
        if detail.attempts.is_empty() {
            trace_rows.push(
                div()
                    .p(px(8.0))
                    .rounded(px(6.0))
                    .bg(t.warning.opacity(0.08))
                    .border_1()
                    .border_color(t.warning.opacity(0.2))
                    .text_xs()
                    .text_color(t.text_secondary)
                    .child("⚡ 该请求由于主候选提供商不可用，已自动转移至备用候选提供商成功完成。")
                    .into_any_element(),
            );
        } else {
            for (idx, attempt) in detail.attempts.iter().enumerate() {
                let is_attempt_ok = attempt.status_code.map(|s| s < 400).unwrap_or(false);
                let status_text = if let Some(code) = attempt.status_code {
                    if is_attempt_ok {
                        format!("HTTP {} (成功接管)", code)
                    } else {
                        format!("HTTP {} (错误)", code)
                    }
                } else {
                    "连接失败".to_string()
                };

                let err_hint = attempt.error.clone().unwrap_or_default();

                trace_rows.push(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(3.0))
                        .p(px(8.0))
                        .rounded(px(6.0))
                        .bg(if is_attempt_ok { t.success.opacity(0.06) } else { t.danger.opacity(0.06) })
                        .border_1()
                        .border_color(if is_attempt_ok { t.success.opacity(0.25) } else { t.danger.opacity(0.25) })
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .justify_between()
                                .child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .gap(px(6.0))
                                        .child(
                                            div()
                                                .px(px(6.0))
                                                .py(px(1.0))
                                                .rounded(px(3.0))
                                                .bg(t.card_bg)
                                                .border_1()
                                                .border_color(t.card_border)
                                                .text_xs()
                                                .font_weight(gpui::FontWeight::BOLD)
                                                .text_color(t.text_primary)
                                                .child(format!("#{}", idx + 1))
                                        )
                                        .child(
                                            div()
                                                .text_xs()
                                                .font_weight(gpui::FontWeight::SEMIBOLD)
                                                .text_color(t.text_primary)
                                                .child(format!("{} (模型: {})", attempt.provider_name, attempt.model))
                                        )
                                )
                                .child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .gap(px(8.0))
                                        .child(
                                            div()
                                                .text_xs()
                                                .font_weight(gpui::FontWeight::BOLD)
                                                .text_color(if is_attempt_ok { t.success } else { t.danger })
                                                .child(status_text)
                                        )
                                        .child(
                                            div()
                                                .text_xs()
                                                .text_color(t.text_muted)
                                                .child(format!("{}ms", attempt.duration_ms))
                                        )
                                )
                        )
                        .when(!err_hint.is_empty(), |s| {
                            s.child(
                                div()
                                    .text_xs()
                                    .font_family("Consolas")
                                    .text_color(t.danger)
                                    .child(err_hint)
                            )
                        })
                        .into_any_element(),
                );
            }
        }

        Some(
            div()
                .flex()
                .flex_col()
                .gap(px(8.0))
                .p(px(12.0))
                .rounded(px(8.0))
                .bg(t.card_bg)
                .border_1()
                .border_color(t.card_border)
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(6.0))
                        .child(svg_icon(ALERT_SVG, px(14.0), t.warning))
                        .child(
                            div()
                                .text_xs()
                                .font_weight(gpui::FontWeight::BOLD)
                                .text_color(t.text_primary)
                                .child("故障转移执行链路 (Failover Trace)")
                        )
                )
                .children(trace_rows)
        )
    } else {
        None
    };

    // Request Body Card
    let inbound_copy_action = copy_inbound;
    let inbound_section = div()
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
                        .flex()
                        .items_center()
                        .gap(px(6.0))
                        .child(
                            div()
                                .text_xs()
                                .font_weight(gpui::FontWeight::BOLD)
                                .text_color(t.text_primary)
                                .child("客户端请求体 (Request Body)")
                        )
                        .child(
                            div()
                                .text_xs()
                                .text_color(t.text_muted)
                                .child(format!("({:.1} KB, {} 行)", inbound_bytes as f64 / 1024.0, inbound_lines))
                        )
                )
                .child(
                    div()
                        .cursor_pointer()
                        .flex()
                        .items_center()
                        .gap(px(4.0))
                        .px(px(8.0))
                        .py(px(2.0))
                        .rounded(px(4.0))
                        .bg(t.input_bg)
                        .border_1()
                        .border_color(t.card_border)
                        .hover(|s| s.bg(t.card_hover))
                        .text_xs()
                        .text_color(t.accent)
                        .child(svg_icon(COPY_SVG, px(12.0), t.accent))
                        .child("复制请求体")
                        .on_mouse_down(MouseButton::Left, cx.listener(move |ws, _, _, cx| {
                            cx.write_to_clipboard(gpui::ClipboardItem::new_string(inbound_copy_action.clone()));
                            ws.ui.toast("客户端请求体已复制到剪贴板", false);
                        }))
                )
        )
        .child(
            div()
                .id("inbound-body-scroll")
                .flex()
                .flex_col()
                .rounded(px(6.0))
                .bg(t.input_bg)
                .border_1()
                .border_color(t.card_border)
                .p(px(10.0))
                .max_h(px(200.0))
                .overflow_y_scroll()
                .overflow_x_scroll()
                .child(
                    div()
                        .text_xs()
                        .font_family("Consolas")
                        .text_color(t.text_secondary)
                        .child(inbound_display)
                )
        )
        .when(inbound_truncated, |s| {
            s.child(
                div()
                    .px(px(8.0))
                    .py(px(3.0))
                    .rounded(px(4.0))
                    .bg(t.warning.opacity(0.1))
                    .text_xs()
                    .text_color(t.warning)
                    .child(format!("⚠️ 内容过长（共 {} 行），已截断显示前 300 行预览。可点击右上角「复制请求体」获取完整数据。", inbound_lines))
            )
        });

    // Response Body Card
    let response_copy_action = copy_response;
    let response_section = div()
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
                        .flex()
                        .items_center()
                        .gap(px(6.0))
                        .child(
                            div()
                                .text_xs()
                                .font_weight(gpui::FontWeight::BOLD)
                                .text_color(t.text_primary)
                                .child("响应体 (Response Body)")
                        )
                        .child(
                            div()
                                .text_xs()
                                .text_color(t.text_muted)
                                .child(format!("({:.1} KB, {} 行)", response_bytes as f64 / 1024.0, response_lines))
                        )
                )
                .child(
                    div()
                        .cursor_pointer()
                        .flex()
                        .items_center()
                        .gap(px(4.0))
                        .px(px(8.0))
                        .py(px(2.0))
                        .rounded(px(4.0))
                        .bg(t.input_bg)
                        .border_1()
                        .border_color(t.card_border)
                        .hover(|s| s.bg(t.card_hover))
                        .text_xs()
                        .text_color(t.accent)
                        .child(svg_icon(COPY_SVG, px(12.0), t.accent))
                        .child("复制响应体")
                        .on_mouse_down(MouseButton::Left, cx.listener(move |ws, _, _, cx| {
                            cx.write_to_clipboard(gpui::ClipboardItem::new_string(response_copy_action.clone()));
                            ws.ui.toast("响应体已复制到剪贴板", false);
                        }))
                )
        )
        .child(
            div()
                .id("response-body-scroll")
                .flex()
                .flex_col()
                .rounded(px(6.0))
                .bg(t.input_bg)
                .border_1()
                .border_color(if is_ok { t.card_border } else { t.danger.opacity(0.3) })
                .p(px(10.0))
                .max_h(px(200.0))
                .overflow_y_scroll()
                .overflow_x_scroll()
                .child(
                    div()
                        .text_xs()
                        .font_family("Consolas")
                        .text_color(if is_ok { t.text_secondary } else { t.danger })
                        .child(response_display)
                )
        )
        .when(response_truncated, |s| {
            s.child(
                div()
                    .px(px(8.0))
                    .py(px(3.0))
                    .rounded(px(4.0))
                    .bg(t.warning.opacity(0.1))
                    .text_xs()
                    .text_color(t.warning)
                    .child(format!("⚠️ 内容过长（共 {} 行），已截断显示前 300 行预览。可点击右上角「复制响应体」获取全部完整数据。", response_lines))
            )
        });

    let body = div()
        .id("request-detail-scroll")
        .flex()
        .flex_col()
        .gap(px(14.0))
        .w_full()
        .max_h(px(580.0))
        .overflow_y_scroll()
        .pr(px(4.0))
        // Top Meta Card
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(8.0))
                .p(px(12.0))
                .rounded(px(8.0))
                .bg(t.input_bg)
                .border_1()
                .border_color(t.card_border)
                // Row 1: Badges
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .flex_wrap()
                        .gap(px(8.0))
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(8.0))
                                .child(
                                    div()
                                        .px(px(8.0))
                                        .py(px(3.0))
                                        .rounded(px(4.0))
                                        .bg(if is_ok { t.success.opacity(0.15) } else { t.danger.opacity(0.15) })
                                        .text_xs()
                                        .font_weight(gpui::FontWeight::BOLD)
                                        .text_color(if is_ok { t.success } else { t.danger })
                                        .child(format!("HTTP {}", summary.status_code))
                                )
                                .child(
                                    if summary.failover {
                                        div()
                                            .flex()
                                            .items_center()
                                            .gap(px(4.0))
                                            .px(px(8.0))
                                            .py(px(3.0))
                                            .rounded(px(4.0))
                                            .bg(t.warning.opacity(0.15))
                                            .text_xs()
                                            .font_weight(gpui::FontWeight::SEMIBOLD)
                                            .text_color(t.warning)
                                            .child("⚡ 已触发故障转移")
                                    } else {
                                        div()
                                            .px(px(8.0))
                                            .py(px(3.0))
                                            .rounded(px(4.0))
                                            .bg(t.card_bg)
                                            .border_1()
                                            .border_color(t.card_border)
                                            .text_xs()
                                            .text_color(t.text_muted)
                                            .child("直连")
                                    }
                                )
                                .child(
                                    div()
                                        .px(px(8.0))
                                        .py(px(3.0))
                                        .rounded(px(4.0))
                                        .bg(t.card_bg)
                                        .border_1()
                                        .border_color(t.card_border)
                                        .text_xs()
                                        .font_weight(gpui::FontWeight::MEDIUM)
                                        .text_color(t.text_primary)
                                        .child(summary.cli_key.to_uppercase())
                                )
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(t.text_muted)
                                        .child(format!("耗时 {}ms", summary.duration_ms))
                                )
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(t.text_muted)
                                        .child(format!("Token: {} (入: {} / 出: {})", summary.total_tokens, summary.input_tokens, summary.output_tokens))
                                )
                        )
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(6.0))
                                .child(div().text_xs().text_color(t.text_muted).child(time_str))
                                .child(
                                    div()
                                        .cursor_pointer()
                                        .text_xs()
                                        .text_color(t.accent)
                                        .hover(|s| s.underline())
                                        .child("复制 ID")
                                        .on_mouse_down(MouseButton::Left, cx.listener(move |ws, _, _, cx| {
                                            cx.write_to_clipboard(gpui::ClipboardItem::new_string(req_id_copy.clone()));
                                            ws.ui.toast("请求 ID 已复制到剪贴板", false);
                                        }))
                                )
                        )
                )
                // Row 2: Route, Provider, Target details
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(4.0))
                        .pt(px(6.0))
                        .border_t_1()
                        .border_color(t.card_border.opacity(0.5))
                        .text_xs()
                        .text_color(t.text_secondary)
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(8.0))
                                .child(div().w(px(70.0)).text_color(t.text_muted).child("请求路由:"))
                                .child(div().font_family("Consolas").text_color(t.text_primary).child(summary.route_name.clone()))
                        )
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(8.0))
                                .child(div().w(px(70.0)).text_color(t.text_muted).child("响应提供商:"))
                                .child(div().font_weight(gpui::FontWeight::MEDIUM).text_color(t.text_primary).child(format!("{} ({})", summary.provider_name, summary.provider_id)))
                                .child(div().text_color(t.text_muted).child("模型:"))
                                .child(div().font_family("Consolas").font_weight(gpui::FontWeight::MEDIUM).text_color(t.text_primary).child(summary.model.clone()))
                        )
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(8.0))
                                .child(div().w(px(70.0)).text_color(t.text_muted).child("上游目标:"))
                                .child(div().font_family("Consolas").text_color(t.text_muted).child(detail.upstream_url.clone()))
                        )
                )
        )
        .children(attempts_section)
        .child(inbound_section)
        .child(response_section);

    super::modal_scaffold_sized(
        &t,
        &title,
        px(860.0),
        Some(px(680.0)),
        body.into_any_element(),
        cx,
        |ws, _, _, cx| {
            ws.ui.gateway_selected_request = None;
            cx.notify();
        },
    )
}

// ---------------------------------------------------------------------------
// Tab 4: Settings
// ---------------------------------------------------------------------------

fn render_settings_tab(ws: &Workspace, cx: &mut Context<Workspace>) -> gpui::AnyElement {
    let t = &ws.theme;
    let settings = &ws.ui.gateway_settings;

    div()
        .flex()
        .flex_col()
        .gap(px(16.0))
        .w(px(600.0))
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(12.0))
                .p(px(16.0))
                .rounded(px(8.0))
                .bg(t.card_bg)
                .border_1()
                .border_color(t.card_border)
                .child(
                    div()
                        .text_base()
                        .font_weight(gpui::FontWeight::BOLD)
                        .text_color(t.text_primary)
                        .child("网关网络与启动设置")
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .gap(px(2.0))
                                .child(div().text_sm().font_weight(gpui::FontWeight::MEDIUM).text_color(t.text_primary).child("监听端口 (Listen Port)"))
                                .child(div().text_xs().text_color(t.text_muted).child("默认 15721，仅绑定本地回环地址 127.0.0.1"))
                        )
                        .child(
                            div()
                                .px(px(8.0))
                                .py(px(4.0))
                                .rounded(px(4.0))
                                .bg(t.input_bg)
                                .border_1()
                                .border_color(t.card_border)
                                .text_sm()
                                .text_color(t.text_primary)
                                .child(format!("{}", settings.port))
                        )
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .gap(px(2.0))
                                .child(div().text_sm().font_weight(gpui::FontWeight::MEDIUM).text_color(t.text_primary).child("端口冲突自动顺延"))
                                .child(div().text_xs().text_color(t.text_muted).child("当 15721 被占用时自动尝试可用端口"))
                        )
                        .child(
                            toggle("tg-port-auto", settings.port_auto_select, t, cx, |ws, _, _, cx| {
                                ws.ui.gateway_settings.port_auto_select = !ws.ui.gateway_settings.port_auto_select;
                                ws.persist_gateway_settings(cx);
                                cx.notify();
                            })
                        )
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .gap(px(2.0))
                                .child(div().text_sm().font_weight(gpui::FontWeight::MEDIUM).text_color(t.text_primary).child("应用启动时自动开启网关"))
                                .child(div().text_xs().text_color(t.text_muted).child("随 aitoolplus 启动在后台静默监听"))
                        )
                        .child(
                            toggle("tg-auto-start", settings.enabled_on_startup, t, cx, |ws, _, _, cx| {
                                ws.ui.gateway_settings.enabled_on_startup = !ws.ui.gateway_settings.enabled_on_startup;
                                ws.persist_gateway_settings(cx);
                                cx.notify();
                            })
                        )
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .gap(px(2.0))
                                .child(div().text_sm().font_weight(gpui::FontWeight::MEDIUM).text_color(t.text_primary).child("自动故障转移 (Failover)"))
                                .child(div().text_xs().text_color(t.text_muted).child("主提供商异常时自动尝试后续候补提供商"))
                        )
                        .child(
                            toggle("tg-settings-failover", settings.failover_enabled, t, cx, |ws, _, _, cx| {
                                ws.ui.gateway_settings.failover_enabled = !ws.ui.gateway_settings.failover_enabled;
                                ws.persist_gateway_settings(cx);
                                if ws.ui.gateway_settings.failover_enabled {
                                    ws.ui.toast("已开启自动故障转移", false);
                                } else {
                                    ws.ui.toast("已关闭故障转移，所有请求仅路由至 P0 主提供商", false);
                                }
                                cx.notify();
                            })
                        )
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .gap(px(2.0))
                                .child(div().text_sm().font_weight(gpui::FontWeight::MEDIUM).text_color(t.text_primary).child("记录完整请求审计体"))
                                .child(div().text_xs().text_color(t.text_muted).child("用于调试查看完整的 Inbound / Outbound 报文"))
                        )
                        .child(
                            toggle("tg-log-body", settings.request_log_body_enabled, t, cx, |ws, _, _, cx| {
                                ws.ui.gateway_settings.request_log_body_enabled = !ws.ui.gateway_settings.request_log_body_enabled;
                                ws.persist_gateway_settings(cx);
                                cx.notify();
                            })
                        )
                )
        )
        .into_any_element()
}
