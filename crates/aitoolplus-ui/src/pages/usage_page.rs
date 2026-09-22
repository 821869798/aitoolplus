//! Usage statistics dashboard: hero summary, trend charts, request logs,
//! provider/model breakdowns, pricing configuration, and session log synchronization.
//!
//! Replicates CC-Switch's UsageDashboard functionality with GPUI Kit native UI.

use gpui::{Context, IntoElement, div, prelude::*, px};

use crate::components::{ButtonVariant, button_l, button_with_icon_l, toggle};
use crate::theme::Theme;
use crate::workspace::Workspace;

use super::{UsageRangePreset, UsageSubTab};
use crate::text_input::TextInput;

pub fn render_usage_page(ws: &mut Workspace, cx: &mut Context<Workspace>) -> gpui::AnyElement {
    // Lazy initial load if summary hasn't been fetched yet
    if ws.ui.usage_summary.is_none() {
        ws.refresh_usage_data();
    }

    let header = render_top_header(ws, cx);
    let hero = render_usage_hero(ws, cx);
    let chart = render_trend_chart(ws, cx);
    let tables = render_usage_tables(ws, cx);
    let session_sync = render_session_sync_card(ws, cx);

    div()
        .flex()
        .flex_col()
        .w_full()
        .gap(px(20.0))
        .pb(px(32.0))
        .on_mouse_move(cx.listener(|ws, _, _, cx| {
            if ws.ui.usage_hovered_bucket.is_some() {
                ws.ui.usage_hovered_bucket = None;
                cx.notify();
            }
        }))
        .child(header)
        .child(hero)
        .child(chart)
        .child(tables)
        .child(session_sync)
        .into_any_element()
}

// ---------------------------------------------------------------------------
// 1. Top Header & Global Filter Toolbar
// ---------------------------------------------------------------------------

fn render_top_header(ws: &mut Workspace, cx: &mut Context<Workspace>) -> gpui::AnyElement {
    let t = ws.theme.clone();
    let i = ws.i18n;

    // App Filter icons: All (Grid), Claude, Codex, Gemini, Grok, OpenCode, Pi
    let current_app: &'static str = match ws.ui.usage_app_filter.as_deref() {
        Some("claude") => "claude",
        Some("codex") => "codex",
        Some("gemini") => "gemini",
        Some("grok") => "grok",
        Some("opencode") => "opencode",
        Some("pi") => "pi",
        _ => "all",
    };

    let app_options: Vec<(&'static str, &'static [u8], &'static str, &'static str)> = vec![
        ("all", crate::icons::LAYOUT_GRID_SVG, "全部应用", "All Apps"),
        ("claude", crate::icons::CLAUDE_SVG, "Claude", "Claude"),
        ("codex", crate::icons::OPENAI_SVG, "Codex", "Codex"),
        ("gemini", crate::icons::GEMINI_SVG, "Gemini", "Gemini"),
        ("grok", crate::icons::GROK_SVG, "Grok", "Grok"),
        ("opencode", crate::icons::OPENCODE_SVG, "OpenCode", "OpenCode"),
        ("pi", crate::icons::PI_SVG, "Pi", "Pi"),
    ];

    let app_selector = div()
        .id("usage-app-filter-icons")
        .flex()
        .items_center()
        .p(px(2.0))
        .gap(px(2.0))
        .rounded(px(8.0))
        .bg(t.input_bg)
        .border_1()
        .border_color(t.card_border)
        .children(app_options.into_iter().map(|(key, icon, zh, en)| {
            let is_sel = current_app == key;
            let tooltip_text = i.t(zh, en);
            div()
                .id(gpui::ElementId::Name(format!("usage-app-tab-{}", key).into()))
                .cursor_pointer()
                .w(px(28.0))
                .h(px(26.0))
                .rounded(px(5.0))
                .flex()
                .items_center()
                .justify_center()
                .tooltip(move |_window, cx| cx.new(|_| crate::components::Tooltip::new(tooltip_text.clone())).into())
                .bg(if is_sel { t.card_bg } else { crate::rgba_const(0x00000000) })
                .border_1()
                .border_color(if is_sel { t.card_border } else { crate::rgba_const(0x00000000) })
                .when(is_sel, |s| s.shadow_sm())
                .hover(|s| if !is_sel { s.bg(t.card_hover) } else { s })
                .child(crate::icons::svg_icon(
                    icon,
                    px(14.0),
                    if is_sel { t.accent } else { t.text_muted },
                ))
                .on_click(cx.listener(move |ws, _, _, cx| {
                    if key == "all" {
                        ws.ui.usage_app_filter = None;
                    } else {
                        ws.ui.usage_app_filter = Some(key.to_string());
                    }
                    ws.ui.usage_provider_filter = None;
                    ws.ui.usage_model_filter = None;
                    ws.ui.usage_page = 0;
                    ws.ui.usage_hovered_bucket = None;
                    ws.refresh_usage_data();
                    cx.notify();
                }))
        }));

    // Provider Filter Button & Menu
    let current_provider_label: gpui::SharedString = match &ws.ui.usage_provider_filter {
        Some(p) => {
            // Find readable name from provider stats if available
            let display = ws.ui.usage_provider_stats
                .iter()
                .find(|s| &s.provider_id == p)
                .map(|s| s.provider_name.as_str())
                .unwrap_or(p.as_str());
            display.to_string().into()
        }
        None => i.t("全部来源", "All Sources"),
    };

    let provider_btn = {
        let is_filtered = ws.ui.usage_provider_filter.is_some();
        let is_open = ws.ui.usage_provider_menu_open;
        div()
            .id("usage-provider-filter-btn")
            .cursor_pointer()
            .h(px(30.0))
            .px(px(10.0))
            .rounded(px(6.0))
            .bg(if is_filtered { t.tab_active_bg } else { t.input_bg })
            .border_1()
            .border_color(if is_filtered { t.accent } else { t.card_border })
            .flex()
            .items_center()
            .gap(px(6.0))
            .text_size(px(12.0))
            .text_color(if is_filtered { t.accent } else { t.text_secondary })
            .hover(|s| s.bg(t.card_hover))
            .child(crate::icons::svg_icon(crate::icons::DATABASE_SVG, px(13.0), if is_filtered { t.accent } else { t.text_muted }))
            .child(
                div()
                    .max_w(px(100.0))
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .child(current_provider_label),
            )
            .child(crate::icons::svg_icon(
                if is_open { crate::icons::CHEVRON_UP_SVG } else { crate::icons::CHEVRON_DOWN_SVG },
                px(11.0),
                t.text_muted,
            ))
            .on_click(cx.listener(|ws, _, _, cx| {
                ws.ui.usage_provider_menu_open = !ws.ui.usage_provider_menu_open;
                ws.ui.usage_model_menu_open = false;
                ws.ui.usage_date_menu_open = false;
                ws.ui.usage_status_menu_open = false;
                cx.notify();
            }))
    };

    // Model Filter Button & Menu
    let current_model_label: gpui::SharedString = match &ws.ui.usage_model_filter {
        Some(m) => m.clone().into(),
        None => i.t("全部模型", "All Models"),
    };

    let model_btn = {
        let is_filtered = ws.ui.usage_model_filter.is_some();
        let is_open = ws.ui.usage_model_menu_open;
        div()
            .id("usage-model-filter-btn")
            .cursor_pointer()
            .h(px(30.0))
            .px(px(10.0))
            .rounded(px(6.0))
            .bg(if is_filtered { t.tab_active_bg } else { t.input_bg })
            .border_1()
            .border_color(if is_filtered { t.accent } else { t.card_border })
            .flex()
            .items_center()
            .gap(px(6.0))
            .text_size(px(12.0))
            .text_color(if is_filtered { t.accent } else { t.text_secondary })
            .hover(|s| s.bg(t.card_hover))
            .child(crate::icons::svg_icon(crate::icons::CPU_SVG, px(13.0), if is_filtered { t.accent } else { t.text_muted }))
            .child(
                div()
                    .max_w(px(100.0))
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .child(current_model_label),
            )
            .child(crate::icons::svg_icon(
                if is_open { crate::icons::CHEVRON_UP_SVG } else { crate::icons::CHEVRON_DOWN_SVG },
                px(11.0),
                t.text_muted,
            ))
            .on_click(cx.listener(|ws, _, _, cx| {
                ws.ui.usage_model_menu_open = !ws.ui.usage_model_menu_open;
                ws.ui.usage_provider_menu_open = false;
                ws.ui.usage_date_menu_open = false;
                ws.ui.usage_status_menu_open = false;
                cx.notify();
            }))
    };

    // Refresh frequency quick toggle: 0s (Off) -> 10s -> 30s -> 60s
    let refresh_interval_sec = ws.ui.usage_refresh_interval;
    let refresh_interval_label: gpui::SharedString = match refresh_interval_sec {
        0 => i.t("自动: 关", "Auto: Off"),
        10 => i.t("自动: 10s", "Auto: 10s"),
        30 => i.t("自动: 30s", "Auto: 30s"),
        60 => i.t("自动: 60s", "Auto: 60s"),
        other => format!("Auto: {}s", other).into(),
    };

    let refresh_interval_btn = div()
        .id("usage-refresh-interval-toggle")
        .cursor_pointer()
        .h(px(30.0))
        .px(px(9.0))
        .rounded(px(6.0))
        .bg(t.input_bg)
        .border_1()
        .border_color(t.card_border)
        .flex()
        .items_center()
        .gap(px(5.0))
        .text_size(px(11.5))
        .text_color(if refresh_interval_sec > 0 { t.accent } else { t.text_muted })
        .hover(|s| s.bg(t.card_hover))
        .child(crate::icons::svg_icon(
            crate::icons::REFRESH_SVG,
            px(12.0),
            if refresh_interval_sec > 0 { t.accent } else { t.text_muted },
        ))
        .child(refresh_interval_label)
        .on_click(cx.listener(|ws, _, _, cx| {
            ws.ui.usage_refresh_interval = match ws.ui.usage_refresh_interval {
                0 => 10,
                10 => 30,
                30 => 60,
                _ => 0,
            };
            let msg = if ws.ui.usage_refresh_interval > 0 {
                format!("{}: {}s", ws.i18n.t("已设置刷新频率", "Refresh interval set to"), ws.ui.usage_refresh_interval)
            } else {
                ws.i18n.t("已关闭自动刷新", "Auto refresh turned off").to_string()
            };
            ws.ui.toast(msg, false);
            cx.notify();
        }));

    // Date Range Dropdown Button & Menu (replaces 2-line tabs)
    let current_range = ws.ui.usage_range;
    let date_range_btn = {
        let is_open = ws.ui.usage_date_menu_open;
        div()
            .id("usage-date-filter-btn")
            .cursor_pointer()
            .h(px(30.0))
            .px(px(10.0))
            .rounded(px(6.0))
            .bg(t.input_bg)
            .border_1()
            .border_color(if is_open { t.accent } else { t.card_border })
            .flex()
            .items_center()
            .gap(px(6.0))
            .text_size(px(12.0))
            .text_color(t.text_primary)
            .hover(|s| s.bg(t.card_hover))
            .child(crate::icons::svg_icon(crate::icons::CALENDAR_SVG, px(13.0), t.text_muted))
            .child(current_range.label(&i))
            .child(crate::icons::svg_icon(
                if is_open { crate::icons::CHEVRON_UP_SVG } else { crate::icons::CHEVRON_DOWN_SVG },
                px(11.0),
                t.text_muted,
            ))
            .on_click(cx.listener(|ws, _, _, cx| {
                ws.ui.usage_date_menu_open = !ws.ui.usage_date_menu_open;
                ws.ui.usage_provider_menu_open = false;
                ws.ui.usage_model_menu_open = false;
                cx.notify();
            }))
    };

    let date_menu = if ws.ui.usage_date_menu_open {
        let presets = [
            UsageRangePreset::Today,
            UsageRangePreset::Days7,
            UsageRangePreset::Days30,
            UsageRangePreset::All,
        ];
        let items: Vec<_> = presets.into_iter().map(|preset| {
            let is_sel = ws.ui.usage_range == preset;
            let label = preset.label(&i);
            div()
                .id(gpui::ElementId::Name(format!("date-menu-item-{:?}", preset).into()))
                .cursor_pointer()
                .px(px(10.0))
                .py(px(6.0))
                .rounded(px(4.0))
                .text_size(px(12.0))
                .text_color(if is_sel { t.accent } else { t.text_primary })
                .bg(if is_sel { t.tab_active_bg } else { crate::rgba_const(0x00000000) })
                .hover(|s| s.bg(t.card_hover))
                .child(label)
                .on_click(cx.listener(move |ws, _, _, cx| {
                    ws.ui.usage_range = preset;
                    ws.ui.usage_date_menu_open = false;
                    ws.ui.usage_page = 0;
                    ws.ui.usage_hovered_bucket = None;
                    ws.refresh_usage_data();
                    cx.notify();
                }))
        }).collect();

        Some(
            div()
                .id("usage-date-menu-dropdown")
                .absolute()
                .top(px(36.0))
                .right_0()
                .w(px(110.0))
                .p(px(4.0))
                .rounded(px(8.0))
                .bg(t.card_bg)
                .border_1()
                .border_color(t.accent)
                .shadow_lg()
                .flex()
                .flex_col()
                .gap(px(2.0))
                .children(items),
        )
    } else {
        None
    };

    // Refresh action button
    let refresh_btn = button_with_icon_l(
        "usage-refresh-btn",
        crate::icons::REFRESH_SVG,
        i.t("刷新", "Refresh"),
        ButtonVariant::Secondary,
        &t,
        cx,
        |ws, _, _, cx| {
            ws.refresh_usage_data();
            ws.ui.toast(ws.i18n.t("已刷新使用统计数据", "Usage statistics refreshed").to_string(), false);
            cx.notify();
        },
    );

    // Floating Provider Menu (if open)
    let provider_menu = if ws.ui.usage_provider_menu_open {
        let mut list = vec![div()
            .id("prov-menu-item-all")
            .cursor_pointer()
            .px(px(10.0))
            .py(px(6.0))
            .rounded(px(4.0))
            .text_size(px(12.0))
            .text_color(if ws.ui.usage_provider_filter.is_none() { t.accent } else { t.text_primary })
            .bg(if ws.ui.usage_provider_filter.is_none() { t.tab_active_bg } else { crate::rgba_const(0x00000000) })
            .hover(|s| s.bg(t.card_hover))
            .child(i.t("全部来源", "All Sources"))
            .on_click(cx.listener(|ws, _, _, cx| {
                ws.ui.usage_provider_filter = None;
                ws.ui.usage_model_filter = None;
                ws.ui.usage_provider_menu_open = false;
                ws.ui.usage_page = 0;
                ws.refresh_usage_data();
                cx.notify();
            }))];

        for (idx, stat) in ws.ui.usage_provider_stats.iter().enumerate() {
            let pid = stat.provider_id.clone();
            let pname = stat.provider_name.clone();
            let is_sel = ws.ui.usage_provider_filter.as_deref() == Some(&pid);
            list.push(
                div()
                    .id(gpui::ElementId::Name(format!("prov-menu-item-{}", idx).into()))
                    .cursor_pointer()
                    .px(px(10.0))
                    .py(px(6.0))
                    .rounded(px(4.0))
                    .text_size(px(12.0))
                    .text_color(if is_sel { t.accent } else { t.text_primary })
                    .bg(if is_sel { t.tab_active_bg } else { crate::rgba_const(0x00000000) })
                    .hover(|s| s.bg(t.card_hover))
                    .child(pname)
                    .on_click(cx.listener(move |ws, _, _, cx| {
                        ws.ui.usage_provider_filter = Some(pid.clone());
                        ws.ui.usage_model_filter = None;
                        ws.ui.usage_provider_menu_open = false;
                        ws.ui.usage_page = 0;
                        ws.refresh_usage_data();
                        cx.notify();
                    })),
            );
        }

        Some(
            div()
                .id("usage-provider-menu-dropdown")
                .absolute()
                .top(px(42.0))
                .left_0()
                .w(px(180.0))
                .max_h(px(240.0))
                .overflow_y_scroll()
                .p(px(4.0))
                .rounded(px(8.0))
                .bg(t.card_bg)
                .border_1()
                .border_color(t.accent)
                .shadow_lg()
                .flex()
                .flex_col()
                .gap(px(2.0))
                .children(list),
        )
    } else {
        None
    };

    // Floating Model Menu (if open)
    let model_menu = if ws.ui.usage_model_menu_open {
        let mut list = vec![div()
            .id("model-menu-item-all")
            .cursor_pointer()
            .px(px(10.0))
            .py(px(6.0))
            .rounded(px(4.0))
            .text_size(px(12.0))
            .text_color(if ws.ui.usage_model_filter.is_none() { t.accent } else { t.text_primary })
            .bg(if ws.ui.usage_model_filter.is_none() { t.tab_active_bg } else { crate::rgba_const(0x00000000) })
            .hover(|s| s.bg(t.card_hover))
            .child(i.t("全部模型", "All Models"))
            .on_click(cx.listener(|ws, _, _, cx| {
                ws.ui.usage_model_filter = None;
                ws.ui.usage_model_menu_open = false;
                ws.ui.usage_page = 0;
                ws.refresh_usage_data();
                cx.notify();
            }))];

        for (idx, stat) in ws.ui.usage_model_stats.iter().enumerate() {
            let mname = stat.model.clone();
            let is_sel = ws.ui.usage_model_filter.as_deref() == Some(&mname);
            list.push(
                div()
                    .id(gpui::ElementId::Name(format!("model-menu-item-{}", idx).into()))
                    .cursor_pointer()
                    .px(px(10.0))
                    .py(px(6.0))
                    .rounded(px(4.0))
                    .font_family(".AppleSystemUIFontMonospaced")
                    .text_size(px(11.5))
                    .text_color(if is_sel { t.accent } else { t.text_primary })
                    .bg(if is_sel { t.tab_active_bg } else { crate::rgba_const(0x00000000) })
                    .hover(|s| s.bg(t.card_hover))
                    .child(mname.clone())
                    .on_click(cx.listener(move |ws, _, _, cx| {
                        ws.ui.usage_model_filter = Some(mname.clone());
                        ws.ui.usage_model_menu_open = false;
                        ws.ui.usage_page = 0;
                        ws.refresh_usage_data();
                        cx.notify();
                    })),
            );
        }

        Some(
            div()
                .id("usage-model-menu-dropdown")
                .absolute()
                .top(px(42.0))
                .left_0()
                .w(px(220.0))
                .max_h(px(240.0))
                .overflow_y_scroll()
                .p(px(4.0))
                .rounded(px(8.0))
                .bg(t.card_bg)
                .border_1()
                .border_color(t.accent)
                .shadow_lg()
                .flex()
                .flex_col()
                .gap(px(2.0))
                .children(list),
        )
    } else {
        None
    };

    div()
        .flex()
        .flex_col()
        .gap(px(12.0))
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
                        .child(
                            div()
                                .text_size(px(20.0))
                                .font_weight(gpui::FontWeight::BOLD)
                                .text_color(t.text_primary)
                                .child(i.t("使用统计", "Usage Statistics")),
                        )
                        .child(
                            div()
                                .text_size(px(12.5))
                                .text_color(t.text_muted)
                                .child(i.t(
                                    "全面对标 CC-Switch：实时统计各 AI 工具的 Token 消耗、缓存命中与调用成本",
                                    "Complete parity with CC-Switch: real-time analytics of token consumption, cache hits, and costs",
                                )),
                        ),
                ),
        )
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .gap(px(10.0))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(8.0))
                        .child(app_selector)
                        .child(
                            div()
                                .relative()
                                .child(provider_btn)
                                .children(provider_menu),
                        )
                        .child(
                            div()
                                .relative()
                                .child(model_btn)
                                .children(model_menu),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(8.0))
                        .child(refresh_interval_btn)
                        .child(
                            div()
                                .relative()
                                .child(date_range_btn)
                                .children(date_menu),
                        )
                        .child(refresh_btn),
                ),
        )
        .into_any_element()
}

// ---------------------------------------------------------------------------
// 2. Hero Summary Section (UsageHero)
// ---------------------------------------------------------------------------

fn render_usage_hero(ws: &Workspace, _cx: &mut Context<Workspace>) -> gpui::AnyElement {
    let t = ws.theme.clone();
    let i = ws.i18n;
    let is_zh = ws.settings.language == aitoolplus_core::settings::Language::Zh;

    let sum = ws.ui.usage_summary.clone().unwrap_or_default();
    let total_cost_num = sum.total_cost.parse::<f64>().unwrap_or(0.0);

    // App headline badge & brand logo
    let (app_title, brand_icon, brand_color): (gpui::SharedString, &'static [u8], gpui::Rgba) = match ws.ui.usage_app_filter.as_deref() {
        Some("claude") => ("Claude Code".into(), crate::icons::CLAUDE_SVG, crate::rgba_const(0xd97757ff)),
        Some("codex") => ("Codex".into(), crate::icons::OPENAI_SVG, crate::rgba_const(0x10a37fff)),
        Some("gemini") => ("Gemini".into(), crate::icons::GEMINI_SVG, crate::rgba_const(0x3186ffff)),
        Some("grok") => ("Grok".into(), crate::icons::GROK_SVG, crate::rgba_const(0xf43f5eff)),
        Some("opencode") => ("OpenCode".into(), crate::icons::OPENCODE_SVG, crate::rgba_const(0xa855f7ff)),
        Some("pi") => ("Pi".into(), crate::icons::PI_SVG, crate::rgba_const(0xec4899ff)),
        _ => (i.t("全部工具", "All Tools"), crate::icons::ZAP_SVG, t.accent),
    };

    let formatted_real_total = format_tokens_number(sum.real_total_tokens);
    let short_real_total = format_tokens_short(sum.real_total_tokens, is_zh);

    // Top Row: Big Real Total Tokens Card + Total Requests / Cost
    let main_token_box = div()
        .flex()
        .items_center()
        .gap(px(14.0))
        .child(
            div()
                .p(px(10.0))
                .rounded(px(12.0))
                .bg(brand_color.opacity(0.12))
                .child(crate::icons::svg_icon(brand_icon, px(22.0), brand_color)),
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
                        .gap(px(6.0))
                        .text_size(px(12.0))
                        .text_color(t.text_secondary)
                        .child(
                            div()
                                .font_weight(gpui::FontWeight::SEMIBOLD)
                                .text_color(brand_color)
                                .child(app_title),
                        )
                        .child(div().text_color(t.text_muted).child("•"))
                        .child(div().child(i.t("真实消耗 Tokens", "Real Consumption Tokens"))),
                )
                .child(
                    div()
                        .flex()
                        .items_baseline()
                        .gap(px(8.0))
                        .child(
                            div()
                                .text_size(px(26.0))
                                .font_weight(gpui::FontWeight::BOLD)
                                .text_color(t.text_primary)
                                .child(formatted_real_total),
                        )
                        .child(
                            div()
                                .px(px(6.0))
                                .py(px(2.0))
                                .rounded(px(4.0))
                                .bg(t.input_bg)
                                .border_1()
                                .border_color(t.card_border)
                                .text_size(px(11.5))
                                .font_weight(gpui::FontWeight::MEDIUM)
                                .text_color(t.text_secondary)
                                .child(format!("≈ {short_real_total}")),
                        ),
                ),
        );

    let stats_overview_box = div()
        .flex()
        .items_center()
        .gap(px(16.0))
        .px(px(16.0))
        .py(px(8.0))
        .rounded(px(10.0))
        .bg(t.input_bg)
        .border_1()
        .border_color(t.card_border)
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(2.0))
                .child(
                    div()
                        .text_size(px(10.5))
                        .font_weight(gpui::FontWeight::MEDIUM)
                        .text_color(t.text_muted)
                        .child(i.t("总请求数", "TOTAL REQUESTS")),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(6.0))
                        .text_size(px(15.0))
                        .font_weight(gpui::FontWeight::BOLD)
                        .text_color(crate::rgba_const(0x3b82f6ff))
                        .child(crate::icons::svg_icon(crate::icons::ACTIVITY_SVG, px(14.0), crate::rgba_const(0x3b82f6ff)))
                        .child(format_tokens_number(sum.total_requests)),
                ),
        )
        .child(div().w(px(1.0)).h(px(24.0)).bg(t.card_border))
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(2.0))
                .child(
                    div()
                        .text_size(px(10.5))
                        .font_weight(gpui::FontWeight::MEDIUM)
                        .text_color(t.text_muted)
                        .child(i.t("预估总成本", "ESTIMATED COST")),
                )
                .child(
                    div()
                        .text_size(px(15.0))
                        .font_weight(gpui::FontWeight::BOLD)
                        .text_color(crate::rgba_const(0x10b981ff))
                        .child(format!("${total_cost_num:.4}")),
                ),
        );

    // 5 Mini Stat Cards: Fresh Input, Output, Cache Write, Cache Read, Cache Hit Rate with Progress Bar
    let hit_rate_pct = (sum.cache_hit_rate * 100.0).clamp(0.0, 100.0);

    let mini_hit_rate_card = div()
        .flex()
        .flex_col()
        .justify_between()
        .p(px(10.0))
        .rounded(px(8.0))
        .bg(t.input_bg)
        .border_1()
        .border_color(t.card_border)
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
                        .child(crate::icons::svg_icon(crate::icons::ACTIVITY_SVG, px(13.0), crate::rgba_const(0x10b981ff)))
                        .child(
                            div()
                                .text_size(px(11.0))
                                .text_color(t.text_muted)
                                .child(i.t("缓存命中率", "Cache Hit Rate")),
                        ),
                )
                .child(
                    div()
                        .text_size(px(13.0))
                        .font_weight(gpui::FontWeight::BOLD)
                        .text_color(crate::rgba_const(0x10b981ff))
                        .child(format!("{hit_rate_pct:.1}%")),
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
                        .h_full()
                        .w(gpui::relative(hit_rate_pct as f32 / 100.0))
                        .rounded(px(2.5))
                        .bg(crate::rgba_const(0x10b981ff)),
                ),
        );

    let mini_stats = div()
        .grid()
        .grid_cols(5)
        .gap(px(10.0))
        .child(render_mini_stat(
            crate::icons::ARROW_DOWN_SVG,
            i.t("新增输入", "Fresh Input"),
            &format_tokens_short(sum.total_input_tokens, is_zh),
            crate::rgba_const(0x3b82f6ff),
            &t,
        ))
        .child(render_mini_stat(
            crate::icons::ARROW_UP_SVG,
            i.t("输出 Tokens", "Output"),
            &format_tokens_short(sum.total_output_tokens, is_zh),
            crate::rgba_const(0x22c55eff),
            &t,
        ))
        .child(render_mini_stat(
            crate::icons::DATABASE_SVG,
            i.t("缓存写入", "Cache Write"),
            &format_tokens_short(sum.total_cache_creation_tokens, is_zh),
            crate::rgba_const(0xf97316ff),
            &t,
        ))
        .child(render_mini_stat(
            crate::icons::SPARKLES_SVG,
            i.t("缓存命中", "Cache Read"),
            &format_tokens_short(sum.total_cache_read_tokens, is_zh),
            crate::rgba_const(0xa855f7ff),
            &t,
        ))
        .child(mini_hit_rate_card);

    div()
        .p(px(16.0))
        .rounded(px(12.0))
        .bg(t.card_bg)
        .border_1()
        .border_color(t.card_border)
        .flex()
        .flex_col()
        .gap(px(16.0))
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .child(main_token_box)
                .child(stats_overview_box),
        )
        .child(mini_stats)
        .into_any_element()
}

fn render_mini_stat(
    icon: &'static [u8],
    label: impl Into<gpui::SharedString>,
    val: &str,
    accent: gpui::Rgba,
    t: &Theme,
) -> gpui::AnyElement {
    div()
        .flex()
        .items_center()
        .gap(px(10.0))
        .p(px(10.0))
        .rounded(px(8.0))
        .bg(t.input_bg)
        .border_1()
        .border_color(t.card_border)
        .child(
            div()
                .p(px(6.0))
                .rounded(px(6.0))
                .bg(accent.opacity(0.12))
                .child(crate::icons::svg_icon(icon, px(14.0), accent)),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(1.0))
                .child(
                    div()
                        .text_size(px(11.0))
                        .text_color(t.text_muted)
                        .child(label.into()),
                )
                .child(
                    div()
                        .text_size(px(14.0))
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .text_color(t.text_primary)
                        .child(val.to_string()),
                ),
        )
        .into_any_element()
}

// ---------------------------------------------------------------------------
// 3. Trend Chart Section (Area Spline & Stacked Bar with Bezier Curves & Parity)
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// 3. Trend Chart Section (Exact CC-Switch Area Spline & Parity)
// ---------------------------------------------------------------------------

fn calculate_nice_ceiling(max_val: f64) -> f64 {
    if max_val <= 0.000001 {
        return 4.0;
    }
    let exponent = max_val.log10().floor();
    let fraction = max_val / 10.0f64.powf(exponent);
    let nice_fraction = if fraction <= 1.0 {
        1.0
    } else if fraction <= 1.2 {
        1.2
    } else if fraction <= 1.6 {
        1.6
    } else if fraction <= 2.0 {
        2.0
    } else if fraction <= 2.2 {
        2.2
    } else if fraction <= 2.4 {
        2.4
    } else if fraction <= 2.8 {
        2.8
    } else if fraction <= 3.0 {
        3.0
    } else if fraction <= 3.2 {
        3.2
    } else if fraction <= 4.0 {
        4.0
    } else if fraction <= 5.0 {
        5.0
    } else if fraction <= 6.0 {
        6.0
    } else if fraction <= 8.0 {
        8.0
    } else {
        10.0
    };
    nice_fraction * 10.0f64.powf(exponent)
}

fn format_tokens_axis_label(val: u64, is_zh: bool) -> String {
    if val == 0 {
        return "0".to_string();
    }
    if is_zh {
        if val >= 100_000_000 {
            if val % 100_000_000 == 0 {
                format!("{}亿", val / 100_000_000)
            } else {
                format!("{:.1}亿", val as f64 / 100_000_000.0)
            }
        } else if val >= 10_000 {
            if val % 10_000 == 0 {
                format!("{}万", val / 10_000)
            } else {
                format!("{:.1}万", val as f64 / 10_000.0)
            }
        } else {
            val.to_string()
        }
    } else {
        if val >= 1_000_000_000 {
            format!("{:.1}B", val as f64 / 1_000_000_000.0)
        } else if val >= 1_000_000 {
            format!("{:.1}M", val as f64 / 1_000_000.0)
        } else if val >= 1_000 {
            format!("{:.1}K", val as f64 / 1_000.0)
        } else {
            val.to_string()
        }
    }
}

fn format_cost_axis_label(val: f64) -> String {
    if val <= 0.0001 {
        "$0".to_string()
    } else if val >= 10.0 {
        format!("${:.0}", val)
    } else if val >= 1.0 {
        format!("${:.1}", val)
    } else {
        format!("${:.2}", val)
    }
}

fn format_tooltip_date(raw_date: &str, is_hourly: bool) -> String {
    if let Ok(ndt) = chrono::NaiveDateTime::parse_from_str(raw_date, "%Y-%m-%dT%H:%M:%S") {
        if is_hourly {
            ndt.format("%Y/%m/%d %H:%M").to_string()
        } else {
            ndt.format("%Y/%m/%d").to_string()
        }
    } else if let Ok(nd) = chrono::NaiveDate::parse_from_str(raw_date, "%Y-%m-%d") {
        nd.format("%Y/%m/%d").to_string()
    } else {
        raw_date.to_string()
    }
}

fn format_x_tick_date(raw_date: &str, is_hourly: bool) -> String {
    if let Ok(ndt) = chrono::NaiveDateTime::parse_from_str(raw_date, "%Y-%m-%dT%H:%M:%S") {
        if is_hourly {
            ndt.format("%m/%d %H:%M").to_string()
        } else {
            ndt.format("%m/%d").to_string()
        }
    } else if let Ok(nd) = chrono::NaiveDate::parse_from_str(raw_date, "%Y-%m-%d") {
        nd.format("%m/%d").to_string()
    } else {
        raw_date.to_string()
    }
}

/// Fritsch-Carlson Monotone Cubic Spline (matching Recharts type="monotone")
fn draw_monotone_spline(
    builder: &mut gpui::PathBuilder,
    pts: &[gpui::Point<gpui::Pixels>],
) {
    let n = pts.len();
    if n < 2 {
        return;
    }
    if n == 2 {
        builder.line_to(pts[1]);
        return;
    }

    let mut dx = Vec::with_capacity(n - 1);
    let mut dy = Vec::with_capacity(n - 1);
    let mut d = Vec::with_capacity(n - 1);
    for k in 0..n - 1 {
        let delta_x = pts[k + 1].x.as_f32() - pts[k].x.as_f32();
        let delta_y = pts[k + 1].y.as_f32() - pts[k].y.as_f32();
        dx.push(delta_x);
        dy.push(delta_y);
        d.push(if delta_x.abs() > 1e-6 { delta_y / delta_x } else { 0.0 });
    }

    let mut m = Vec::with_capacity(n);
    m.push(d[0]);
    for k in 1..n - 1 {
        m.push((d[k - 1] + d[k]) * 0.5);
    }
    m.push(d[n - 2]);

    for k in 0..n - 1 {
        if d[k].abs() < 1e-7 {
            m[k] = 0.0;
            m[k + 1] = 0.0;
        } else {
            let alpha = m[k] / d[k];
            let beta = m[k + 1] / d[k];
            let dist = alpha * alpha + beta * beta;
            if dist > 9.0 {
                let tau = 3.0 / dist.sqrt();
                m[k] = tau * alpha * d[k];
                m[k + 1] = tau * beta * d[k];
            }
        }
    }

    for k in 0..n - 1 {
        let delta_x = dx[k];
        let cp1 = gpui::point(
            gpui::px(pts[k].x.as_f32() + delta_x / 3.0),
            gpui::px(pts[k].y.as_f32() + m[k] * delta_x / 3.0),
        );
        let cp2 = gpui::point(
            gpui::px(pts[k + 1].x.as_f32() - delta_x / 3.0),
            gpui::px(pts[k + 1].y.as_f32() - m[k + 1] * delta_x / 3.0),
        );
        builder.cubic_bezier_to(pts[k + 1], cp1, cp2);
    }
}

fn render_legend_item(label: impl Into<gpui::SharedString>, color: gpui::Rgba, is_dashed: bool) -> gpui::Div {
    let line_el = if is_dashed {
        div()
            .w_full()
            .h(px(1.5))
            .flex()
            .items_center()
            .justify_between()
            .child(div().w(px(5.0)).h(px(1.5)).bg(color))
            .child(div().w(px(5.0)).h(px(1.5)).bg(color))
            .child(div().w(px(5.0)).h(px(1.5)).bg(color))
    } else {
        div().w_full().h(px(1.5)).bg(color)
    };

    div()
        .flex()
        .items_center()
        .gap(px(6.0))
        .text_size(px(12.0))
        .font_weight(gpui::FontWeight::MEDIUM)
        .text_color(color)
        .child(
            div()
                .w(px(22.0))
                .h(px(12.0))
                .relative()
                .flex()
                .items_center()
                .justify_center()
                .child(line_el)
                .child(
                    div()
                        .absolute()
                        .w(px(6.0))
                        .h(px(6.0))
                        .rounded_full()
                        .bg(color)
                        .border_1()
                        .border_color(crate::rgba_const(0xffffffff)),
                ),
        )
        .child(label.into())
}

fn render_trend_chart(ws: &Workspace, cx: &mut Context<Workspace>) -> gpui::AnyElement {
    let t = ws.theme.clone();
    let i = ws.i18n;
    let is_zh = ws.i18n.is_zh();

    let trends = &ws.ui.usage_trends;
    if trends.is_empty() {
        return div()
            .p(px(20.0))
            .rounded(px(12.0))
            .bg(t.card_bg)
            .border_1()
            .border_color(t.card_border)
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .py(px(50.0))
            .child(
                div()
                    .text_size(px(13.0))
                    .text_color(t.text_muted)
                    .child(i.t("所选时间范围内暂无用量趋势数据", "No trend data available for selected range")),
            )
            .into_any_element();
    }

    let is_hourly = ws.ui.usage_range == UsageRangePreset::Today;
    let range_label = match ws.ui.usage_range {
        UsageRangePreset::Today => i.t("当天", "Today"),
        UsageRangePreset::Days7 => i.t("近 7 天", "7 Days"),
        UsageRangePreset::Days30 => i.t("近 30 天", "30 Days"),
        UsageRangePreset::All => i.t("全部", "All Time"),
    };

    // Card Layout Constants (matches Recharts h-[350px] in CC-Switch)
    let pad_left = 72.0f32;
    let pad_right = 56.0f32;
    let pad_top = 16.0f32;
    let pad_bottom = 32.0f32;
    let plot_h = 280.0f32;
    let total_chart_h = pad_top + plot_h + pad_bottom;

    let bucket_count = trends.len();

    // In CC-Switch, all 4 token areas are unstacked, so max is max of any individual token type
    let max_token_val: u64 = trends
        .iter()
        .map(|s| s.total_input_tokens.max(s.total_output_tokens).max(s.total_cache_creation_tokens).max(s.total_cache_read_tokens))
        .max()
        .unwrap_or(1000)
        .max(1000);

    let max_cost_val: f64 = trends
        .iter()
        .map(|s| s.total_cost.parse::<f64>().unwrap_or(0.0))
        .fold(0.0, f64::max)
        .max(0.01);

    let token_ceiling = calculate_nice_ceiling(max_token_val as f64).max(10_000.0);
    let cost_ceiling = calculate_nice_ceiling(max_cost_val).max(1.0);

    // 1. Y-Axis Labels (Left Tokens, Right Cost)
    let mut y_axis_labels = Vec::new();
    for level in 0..=4 {
        let ratio = level as f32 / 4.0;
        let line_y = pad_top + (1.0 - ratio) * plot_h;

        let left_lbl = format_tokens_axis_label((token_ceiling * ratio as f64) as u64, is_zh);
        y_axis_labels.push(
            div()
                .absolute()
                .top(px(line_y - 8.0))
                .left_0()
                .w(px(pad_left - 10.0))
                .flex()
                .justify_end()
                .child(
                    div()
                        .text_size(px(12.0))
                        .text_color(t.text_muted)
                        .child(left_lbl),
                ),
        );

        let right_lbl = format_cost_axis_label(cost_ceiling * ratio as f64);
        y_axis_labels.push(
            div()
                .absolute()
                .top(px(line_y - 8.0))
                .right_0()
                .w(px(pad_right - 10.0))
                .flex()
                .justify_start()
                .child(
                    div()
                        .text_size(px(12.0))
                        .text_color(t.text_muted)
                        .child(right_lbl),
                ),
        );
    }

    // 2. Data Points & Spline Canvas
    let stats_data: Vec<(f32, f32, f32, f32, f32)> = trends
        .iter()
        .map(|s| {
            let inp = (s.total_input_tokens as f64 / token_ceiling).clamp(0.0, 1.0) as f32;
            let out = (s.total_output_tokens as f64 / token_ceiling).clamp(0.0, 1.0) as f32;
            let cw = (s.total_cache_creation_tokens as f64 / token_ceiling).clamp(0.0, 1.0) as f32;
            let cr = (s.total_cache_read_tokens as f64 / token_ceiling).clamp(0.0, 1.0) as f32;
            let cost_val = s.total_cost.parse::<f64>().unwrap_or(0.0);
            let cost_r = (cost_val / cost_ceiling).clamp(0.0, 1.0) as f32;
            (inp, out, cw, cr, cost_r)
        })
        .collect();

    let hovered_idx = ws.ui.usage_hovered_bucket;
    let grid_color = t.card_border.opacity(0.40);

    let spline_canvas = div()
        .absolute()
        .top_0()
        .bottom_0()
        .left_0()
        .right_0()
        .child(
            gpui::canvas(
                move |bounds, _window, _cx| bounds,
                move |bounds, _, window, _cx| {
                    let cnt = stats_data.len();
                    if cnt == 0 {
                        return;
                    }
                    let left_x = bounds.origin.x + px(pad_left);
                    let right_x = bounds.origin.x + bounds.size.width - px(pad_right);
                    let plot_w = right_x.as_f32() - left_x.as_f32();
                    let top_y = bounds.origin.y + px(pad_top);
                    let bottom_y = bounds.origin.y + px(pad_top + plot_h);

                    // 1. Horizontal dashed grid lines (levels 1, 2, 3, 4)
                    for level in 1..=4 {
                        let ratio = level as f32 / 4.0;
                        let y = top_y.as_f32() + (1.0 - ratio) * plot_h;
                        let mut grid_b = gpui::PathBuilder::stroke(px(1.0)).dash_array(&[px(3.0), px(3.0)]);
                        grid_b.move_to(gpui::point(left_x, px(y)));
                        grid_b.line_to(gpui::point(right_x, px(y)));
                        if let Ok(p) = grid_b.build() {
                            window.paint_path(p, grid_color);
                        }
                    }

                    // 2. Build point coordinates for all 5 series
                    let mut cr_pts = Vec::with_capacity(cnt);
                    let mut inp_pts = Vec::with_capacity(cnt);
                    let mut cw_pts = Vec::with_capacity(cnt);
                    let mut out_pts = Vec::with_capacity(cnt);
                    let mut cost_pts = Vec::with_capacity(cnt);

                    for (i, &(inp, out, cw, cr, cost_r)) in stats_data.iter().enumerate() {
                        let x = if cnt > 1 {
                            left_x.as_f32() + plot_w * (i as f32 / (cnt - 1) as f32)
                        } else {
                            left_x.as_f32() + plot_w * 0.5
                        };
                        cr_pts.push(gpui::point(px(x), px(bottom_y.as_f32() - plot_h * cr)));
                        inp_pts.push(gpui::point(px(x), px(bottom_y.as_f32() - plot_h * inp)));
                        cw_pts.push(gpui::point(px(x), px(bottom_y.as_f32() - plot_h * cw)));
                        out_pts.push(gpui::point(px(x), px(bottom_y.as_f32() - plot_h * out)));
                        cost_pts.push(gpui::point(px(x), px(bottom_y.as_f32() - plot_h * cost_r)));
                    }

                    // 3. Render 4 Area series (stroke + gradient-like opacity fill)
                    let area_series = [
                        (&cr_pts, crate::rgba_const(0xa855f7ff)),
                        (&inp_pts, crate::rgba_const(0x3b82f6ff)),
                        (&cw_pts, crate::rgba_const(0xf97316ff)),
                        (&out_pts, crate::rgba_const(0x22c55eff)),
                    ];

                    for (pts, color) in area_series {
                        if cnt >= 2 {
                            let mut area_b = gpui::PathBuilder::fill();
                            area_b.move_to(gpui::point(pts[0].x, bottom_y));
                            area_b.line_to(pts[0]);
                            draw_monotone_spline(&mut area_b, pts);
                            area_b.line_to(gpui::point(pts[cnt - 1].x, bottom_y));
                            area_b.close();
                            if let Ok(p) = area_b.build() {
                                window.paint_path(p, color.opacity(0.18));
                            }

                            let mut stroke_b = gpui::PathBuilder::stroke(px(2.0));
                            stroke_b.move_to(pts[0]);
                            draw_monotone_spline(&mut stroke_b, pts);
                            if let Ok(p) = stroke_b.build() {
                                window.paint_path(p, color);
                            }
                        } else {
                            // single point line
                            let y = pts[0].y;
                            let mut b = gpui::PathBuilder::stroke(px(2.0));
                            b.move_to(gpui::point(left_x, y));
                            b.line_to(gpui::point(right_x, y));
                            if let Ok(p) = b.build() {
                                window.paint_path(p, color);
                            }
                        }
                    }

                    // 4. Render 1 Cost dashed line
                    if cnt >= 2 {
                        let mut cost_b = gpui::PathBuilder::stroke(px(2.0)).dash_array(&[px(4.0), px(4.0)]);
                        cost_b.move_to(cost_pts[0]);
                        draw_monotone_spline(&mut cost_b, &cost_pts);
                        if let Ok(p) = cost_b.build() {
                            window.paint_path(p, crate::rgba_const(0xf43f5eff));
                        }
                    } else {
                        let y = cost_pts[0].y;
                        let mut cost_b = gpui::PathBuilder::stroke(px(2.0)).dash_array(&[px(4.0), px(4.0)]);
                        cost_b.move_to(gpui::point(left_x, y));
                        cost_b.line_to(gpui::point(right_x, y));
                        if let Ok(p) = cost_b.build() {
                            window.paint_path(p, crate::rgba_const(0xf43f5eff));
                        }
                    }

                    // 5. Active hover hairline and marker circle
                    if let Some(h_idx) = hovered_idx {
                        if h_idx < cnt {
                            let h_x = if cnt > 1 {
                                left_x.as_f32() + plot_w * (h_idx as f32 / (cnt - 1) as f32)
                            } else {
                                left_x.as_f32() + plot_w * 0.5
                            };

                            // Vertical hairline
                            let mut hair_b = gpui::PathBuilder::stroke(px(1.0));
                            hair_b.move_to(gpui::point(px(h_x), top_y));
                            hair_b.line_to(gpui::point(px(h_x), bottom_y));
                            if let Ok(p) = hair_b.build() {
                                window.paint_path(p, crate::rgba_const(0xffffff60));
                            }

                            // Glowing marker dots
                            let dot_pts = [
                                (cost_pts[h_idx], crate::rgba_const(0xf43f5eff)),
                                (cr_pts[h_idx], crate::rgba_const(0xa855f7ff)),
                                (inp_pts[h_idx], crate::rgba_const(0x3b82f6ff)),
                                (cw_pts[h_idx], crate::rgba_const(0xf97316ff)),
                                (out_pts[h_idx], crate::rgba_const(0x22c55eff)),
                            ];

                            for (pt, col) in dot_pts {
                                let outer = gpui::Bounds {
                                    origin: gpui::point(pt.x - px(4.0), pt.y - px(4.0)),
                                    size: gpui::size(px(8.0), px(8.0)),
                                };
                                window.paint_quad(gpui::fill(outer, col));
                                let inner = gpui::Bounds {
                                    origin: gpui::point(pt.x - px(2.0), pt.y - px(2.0)),
                                    size: gpui::size(px(4.0), px(4.0)),
                                };
                                window.paint_quad(gpui::fill(inner, gpui::white()));
                            }
                        }
                    }
                },
            )
            .size_full(),
        );

    // 3. X-Axis Tick Labels (spaced cleanly, strictly bounded within [pad_left, width - pad_right])
    let mut x_ticks = Vec::new();
    let tick_interval = if is_hourly {
        if bucket_count <= 8 {
            1
        } else if bucket_count <= 15 {
            2
        } else if bucket_count <= 24 {
            3
        } else {
            4
        }
    } else {
        if bucket_count <= 8 {
            1
        } else if bucket_count <= 16 {
            2
        } else if bucket_count <= 28 {
            3
        } else {
            4
        }
    };

    let mut last_tick_idx: Option<usize> = None;
    for i in 0..bucket_count {
        let is_regular_tick = if is_hourly {
            i % tick_interval == 1
        } else {
            i % tick_interval == 0
        };

        // Don't crowd the last tick if it's right next to the previous tick
        let is_last_tick = i == bucket_count - 1
            && last_tick_idx.map_or(true, |last| i >= last + tick_interval);

        if is_regular_tick || is_last_tick {
            last_tick_idx = Some(i);
            let pct = if bucket_count > 1 {
                (i as f32 / (bucket_count - 1) as f32) * 100.0
            } else {
                50.0
            };
            let tick_text = format_x_tick_date(&trends[i].date, is_hourly);

            let tick_box = div()
                .absolute()
                .top_0()
                .bottom_0()
                .w(px(80.0))
                .left(gpui::relative(pct / 100.0))
                .ml(px(-40.0))
                .flex()
                .items_center()
                .justify_center();

            x_ticks.push(
                tick_box.child(
                    div()
                        .text_size(px(11.5))
                        .text_color(t.text_muted)
                        .child(tick_text),
                ),
            );
        }
    }

    let x_axis_row = div()
        .absolute()
        .bottom(px(4.0))
        .left(px(pad_left))
        .right(px(pad_right))
        .h(px(20.0))
        .children(x_ticks);

    // 4. Hit-testing interactive columns
    let hover_columns = div()
        .absolute()
        .top(px(pad_top))
        .bottom(px(pad_bottom))
        .left(px(pad_left))
        .right(px(pad_right))
        .flex()
        .items_stretch()
        .children((0..bucket_count).map(|idx| {
            div()
                .id(gpui::ElementId::Name(format!("trend-col-{}", idx).into()))
                .flex_1()
                .h_full()
                .cursor_crosshair()
                .on_mouse_move(cx.listener(move |ws, _, _, cx| {
                    cx.stop_propagation();
                    if ws.ui.usage_hovered_bucket != Some(idx) {
                        ws.ui.usage_hovered_bucket = Some(idx);
                        cx.notify();
                    }
                }))
        }));

    // 5. Floating Tooltip Card (Exact CC-Switch Parity)
    let floating_tooltip = if let Some(h_idx) = hovered_idx {
        if h_idx < bucket_count {
            let stat = &trends[h_idx];
            let heading = format_tooltip_date(&stat.date, is_hourly);
            let cost_num = stat.total_cost.parse::<f64>().unwrap_or(0.0);

            let pct = if bucket_count > 1 {
                (h_idx as f32 / (bucket_count - 1) as f32) * 100.0
            } else {
                50.0
            };

            let is_right_half = h_idx > bucket_count / 2;
            let mut card = div()
                .absolute()
                .top(px(pad_top + 20.0))
                .w(px(190.0))
                .p(px(12.0))
                .rounded(px(8.0))
                .bg(t.card_bg)
                .border_1()
                .border_color(t.card_border)
                .shadow_lg()
                .flex()
                .flex_col()
                .gap(px(6.0))
                .child(
                    div()
                        .text_size(px(12.5))
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .text_color(t.text_primary)
                        .child(heading),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(6.0))
                        .text_size(px(12.0))
                        .text_color(crate::rgba_const(0x3b82f6ff))
                        .child(div().w(px(6.0)).h(px(6.0)).rounded_full().bg(crate::rgba_const(0x3b82f6ff)))
                        .child(div().font_weight(gpui::FontWeight::MEDIUM).child("输入:"))
                        .child(div().child(format_tokens_number(stat.total_input_tokens))),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(6.0))
                        .text_size(px(12.0))
                        .text_color(crate::rgba_const(0x22c55eff))
                        .child(div().w(px(6.0)).h(px(6.0)).rounded_full().bg(crate::rgba_const(0x22c55eff)))
                        .child(div().font_weight(gpui::FontWeight::MEDIUM).child("输出:"))
                        .child(div().child(format_tokens_number(stat.total_output_tokens))),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(6.0))
                        .text_size(px(12.0))
                        .text_color(crate::rgba_const(0xf97316ff))
                        .child(div().w(px(6.0)).h(px(6.0)).rounded_full().bg(crate::rgba_const(0xf97316ff)))
                        .child(div().font_weight(gpui::FontWeight::MEDIUM).child("缓存创建:"))
                        .child(div().child(format_tokens_number(stat.total_cache_creation_tokens))),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(6.0))
                        .text_size(px(12.0))
                        .text_color(crate::rgba_const(0xa855f7ff))
                        .child(div().w(px(6.0)).h(px(6.0)).rounded_full().bg(crate::rgba_const(0xa855f7ff)))
                        .child(div().font_weight(gpui::FontWeight::MEDIUM).child("缓存命中:"))
                        .child(div().child(format_tokens_number(stat.total_cache_read_tokens))),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(6.0))
                        .text_size(px(12.0))
                        .text_color(crate::rgba_const(0xf43f5eff))
                        .child(div().w(px(6.0)).h(px(6.0)).rounded_full().bg(crate::rgba_const(0xf43f5eff)))
                        .child(div().font_weight(gpui::FontWeight::MEDIUM).child("成本:"))
                        .child(div().child(format!("${:.6}", cost_num))),
                );

            if is_right_half {
                card = card.right(gpui::relative((100.0 - pct) / 100.0)).mr(px(pad_right + 14.0));
            } else {
                card = card.left(gpui::relative(pct / 100.0)).ml(px(pad_left + 14.0));
            }

            Some(card)
        } else {
            None
        }
    } else {
        None
    };

    // 6. Bottom Centered Legend (Exact CC-Switch Parity)
    let bottom_legend = div()
        .flex()
        .items_center()
        .justify_center()
        .gap(px(20.0))
        .pt(px(4.0))
        .child(render_legend_item(i.t("成本", "Cost"), crate::rgba_const(0xf43f5eff), true))
        .child(render_legend_item(i.t("缓存创建", "Cache Write"), crate::rgba_const(0xf97316ff), false))
        .child(render_legend_item(i.t("缓存命中", "Cache Read"), crate::rgba_const(0xa855f7ff), false))
        .child(render_legend_item(i.t("输入", "Input"), crate::rgba_const(0x3b82f6ff), false))
        .child(render_legend_item(i.t("输出", "Output"), crate::rgba_const(0x22c55eff), false));

    div()
        .p(px(20.0))
        .rounded(px(12.0))
        .bg(t.card_bg)
        .border_1()
        .border_color(t.card_border)
        .flex()
        .flex_col()
        .gap(px(12.0))
        .on_mouse_move(cx.listener(|ws, _, _, cx| {
            if ws.ui.usage_hovered_bucket.is_some() {
                ws.ui.usage_hovered_bucket = None;
                cx.notify();
            }
        }))
        .child(
            // Header: title on left, range label on right
            div()
                .flex()
                .items_center()
                .justify_between()
                .child(
                    div()
                        .text_size(px(18.0))
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .text_color(t.text_primary)
                        .child(i.t("使用趋势", "Usage Trends")),
                )
                .child(
                    div()
                        .text_size(px(14.0))
                        .text_color(t.text_muted)
                        .child(range_label),
                ),
        )
        .child(
            div()
                .relative()
                .w_full()
                .h(px(total_chart_h))
                .on_mouse_move(cx.listener(|ws, _, _, cx| {
                    if ws.ui.usage_hovered_bucket.is_some() {
                        ws.ui.usage_hovered_bucket = None;
                        cx.notify();
                    }
                }))
                .children(y_axis_labels)
                .child(spline_canvas)
                .children(floating_tooltip)
                .child(hover_columns)
                .child(x_axis_row),
        )
        .child(bottom_legend)
        .into_any_element()
}

// ---------------------------------------------------------------------------
// 4. Tabbed Detail Tables (Logs, Providers, Models, Pricing)
// ---------------------------------------------------------------------------

fn render_usage_tables(ws: &mut Workspace, cx: &mut Context<Workspace>) -> gpui::AnyElement {
    let t = ws.theme.clone();
    let i = ws.i18n;

    let current_subtab = ws.ui.usage_subtab;
    let tabs = vec![
        (UsageSubTab::Logs, UsageSubTab::Logs.label(&i)),
        (UsageSubTab::Providers, UsageSubTab::Providers.label(&i)),
        (UsageSubTab::Models, UsageSubTab::Models.label(&i)),
        (UsageSubTab::Pricing, UsageSubTab::Pricing.label(&i)),
    ];

    let tab_bar = crate::components::segmented_tab_bar(
        "usage-subtabs",
        tabs,
        current_subtab,
        &t,
        cx,
        |ws, tab, _window, cx| {
            ws.ui.usage_subtab = tab;
            ws.refresh_usage_data();
            cx.notify();
        },
    );

    let body = match current_subtab {
        UsageSubTab::Logs => render_logs_table(ws, cx),
        UsageSubTab::Providers => render_providers_table(ws, cx),
        UsageSubTab::Models => render_models_table(ws, cx),
        UsageSubTab::Pricing => render_pricing_table(ws, cx),
    };

    div()
        .flex()
        .flex_col()
        .gap(px(12.0))
        .child(tab_bar)
        .child(body)
        .into_any_element()
}

fn render_logs_table(ws: &mut Workspace, cx: &mut Context<Workspace>) -> gpui::AnyElement {
    let t = ws.theme.clone();
    let i = ws.i18n;

    let logs = &ws.ui.usage_logs.data;
    let total = ws.ui.usage_logs.total;
    let page = ws.ui.usage_page;
    let page_size = 20;
    let total_pages = (total + page_size - 1) / page_size;

    // Status code filter toolbar above logs table
    let status_options = vec![
        (None, i.t("全部状态", "All Status")),
        (Some(200), "200 OK".into()),
        (Some(400), "400 Bad Request".into()),
        (Some(401), "401 Unauthorized".into()),
        (Some(429), "429 Rate Limit".into()),
        (Some(500), "500 Server Error".into()),
    ];

    let current_status = ws.ui.usage_status_filter;
    let status_filter_bar = div()
        .flex()
        .items_center()
        .gap(px(6.0))
        .children(status_options.into_iter().map(|(val, lbl)| {
            let is_active = current_status == val;
            let tab_theme = t.clone();
            let item_id = format!("status-filter-{:?}", val);
            div()
                .id(gpui::ElementId::Name(item_id.into()))
                .cursor_pointer()
                .px(px(8.0))
                .py(px(3.0))
                .rounded(px(5.0))
                .text_size(px(11.0))
                .font_weight(if is_active { gpui::FontWeight::SEMIBOLD } else { gpui::FontWeight::NORMAL })
                .text_color(if is_active { tab_theme.accent } else { tab_theme.text_secondary })
                .bg(if is_active { tab_theme.tab_active_bg } else { tab_theme.input_bg })
                .border_1()
                .border_color(if is_active { tab_theme.accent } else { tab_theme.card_border })
                .hover(|s| s.bg(tab_theme.card_hover))
                .child(lbl)
                .on_click(cx.listener(move |ws, _, _, cx| {
                    ws.ui.usage_status_filter = val;
                    ws.ui.usage_page = 0;
                    ws.refresh_usage_data();
                    cx.notify();
                }))
        }));

    // Header row
    let thead = div()
        .flex()
        .items_center()
        .px(px(12.0))
        .py(px(8.0))
        .bg(t.input_bg)
        .rounded_t(px(8.0))
        .text_size(px(11.5))
        .font_weight(gpui::FontWeight::SEMIBOLD)
        .text_color(t.text_secondary)
        .child(div().w(px(110.0)).child(i.t("请求时间", "Time")))
        .child(div().w(px(130.0)).child(i.t("来源 (Provider)", "Provider")))
        .child(div().flex_1().child(i.t("计费模型", "Model")))
        .child(div().w(px(85.0)).text_right().child(i.t("输入", "Input")))
        .child(div().w(px(80.0)).text_right().child(i.t("输出", "Output")))
        .child(div().w(px(85.0)).text_right().child(i.t("费用 (USD)", "Cost")))
        .child(div().w(px(85.0)).text_right().child(i.t("耗时", "Latency")))
        .child(div().w(px(65.0)).text_center().child(i.t("状态", "Status")))
        .child(div().w(px(60.0)).text_center().child(i.t("渠道", "Source")));

    let tbody = if logs.is_empty() {
        div()
            .py(px(40.0))
            .flex()
            .items_center()
            .justify_center()
            .text_size(px(12.5))
            .text_color(t.text_muted)
            .child(i.t("暂无请求日志记录", "No request logs found"))
            .into_any_element()
    } else {
        div()
            .flex()
            .flex_col()
            .children(logs.iter().map(|log| {
                let time_str = format_timestamp(log.created_at);
                let is_success = log.status_code >= 200 && log.status_code < 300;
                let cost_num = log.total_cost_usd.parse::<f64>().unwrap_or(0.0);
                let latency_sec = log.latency_ms as f64 / 1000.0;

                let model_display = if let Some(req_m) = &log.request_model {
                    if req_m != &log.model && !req_m.is_empty() {
                        format!("{} → {}", req_m, log.model)
                    } else {
                        log.model.clone()
                    }
                } else {
                    log.model.clone()
                };

                let cache_info = if log.cache_read_tokens > 0 || log.cache_creation_tokens > 0 {
                    let mut parts = Vec::new();
                    if log.cache_read_tokens > 0 {
                        parts.push(format!("R:{}", format_tokens_number(log.cache_read_tokens as u64)));
                    }
                    if log.cache_creation_tokens > 0 {
                        parts.push(format!("W:{}", format_tokens_number(log.cache_creation_tokens as u64)));
                    }
                    parts.join(" · ")
                } else {
                    String::new()
                };

                div()
                    .flex()
                    .items_center()
                    .px(px(12.0))
                    .py(px(7.5))
                    .border_b_1()
                    .border_color(t.card_border)
                    .text_size(px(12.0))
                    .text_color(t.text_primary)
                    .hover(|s| s.bg(t.input_bg))
                    .child(div().w(px(110.0)).text_color(t.text_secondary).text_size(px(11.0)).child(time_str))
                    .child(div().w(px(130.0)).text_color(t.accent).font_weight(gpui::FontWeight::MEDIUM).child(log.provider_name.clone().unwrap_or(log.provider_id.clone())))
                    .child(
                        div()
                            .flex_1()
                            .font_family(".AppleSystemUIFontMonospaced")
                            .text_size(px(11.0))
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .child(model_display),
                    )
                    .child(
                        div()
                            .w(px(85.0))
                            .flex()
                            .flex_col()
                            .items_end()
                            .child(div().child(format_tokens_number(log.input_tokens as u64)))
                            .when(!cache_info.is_empty(), |s| {
                                s.child(div().text_size(px(9.5)).text_color(crate::rgba_const(0xa855f7ff)).child(cache_info))
                            }),
                    )
                    .child(div().w(px(80.0)).text_right().child(format_tokens_number(log.output_tokens as u64)))
                    .child(div().w(px(85.0)).text_right().font_weight(gpui::FontWeight::MEDIUM).text_color(crate::rgba_const(0x10b981ff)).child(format!("${cost_num:.4}")))
                    .child(div().w(px(85.0)).text_right().text_color(t.text_secondary).text_size(px(11.0)).child(format!("{:.1}s", latency_sec)))
                    .child(
                        div()
                            .w(px(65.0))
                            .flex()
                            .justify_center()
                            .child(
                                div()
                                    .px(px(5.0))
                                    .py(px(1.0))
                                    .rounded(px(4.0))
                                    .bg(if is_success { crate::rgba_const(0x22c55e20) } else { crate::rgba_const(0xef444420) })
                                    .text_color(if is_success { crate::rgba_const(0x22c55eff) } else { crate::rgba_const(0xef4444ff) })
                                    .text_size(px(10.5))
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .child(log.status_code.to_string()),
                            ),
                    )
                    .child(
                        div()
                            .w(px(60.0))
                            .flex()
                            .justify_center()
                            .child(
                                div()
                                    .text_size(px(10.5))
                                    .text_color(t.text_muted)
                                    .child(log.data_source.clone().unwrap_or_else(|| "proxy".to_string())),
                            ),
                    )
            }))
            .into_any_element()
    };

    // Pagination
    let pagination = div()
        .flex()
        .items_center()
        .justify_between()
        .pt(px(8.0))
        .text_size(px(12.0))
        .text_color(t.text_secondary)
        .child(
            div().child(format!(
                "{}: {} ({} / {})",
                i.t("总记录数", "Total records"),
                total,
                if total == 0 { 0 } else { page + 1 },
                total_pages.max(1)
            )),
        )
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(8.0))
                .child(
                    button_l(
                        "logs-prev-page",
                        i.t("上一页", "Previous"),
                        ButtonVariant::Secondary,
                        &t,
                        cx,
                        move |ws, _, _, cx| {
                            if ws.ui.usage_page > 0 {
                                ws.ui.usage_page -= 1;
                                ws.refresh_usage_data();
                                cx.notify();
                            }
                        },
                    ),
                )
                .child(
                    button_l(
                        "logs-next-page",
                        i.t("下一页", "Next"),
                        ButtonVariant::Secondary,
                        &t,
                        cx,
                        move |ws, _, _, cx| {
                            if (ws.ui.usage_page + 1) < total_pages {
                                ws.ui.usage_page += 1;
                                ws.refresh_usage_data();
                                cx.notify();
                            }
                        },
                    ),
                ),
        );

    div()
        .p(px(16.0))
        .rounded(px(12.0))
        .bg(t.card_bg)
        .border_1()
        .border_color(t.card_border)
        .flex()
        .flex_col()
        .gap(px(10.0))
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .child(status_filter_bar)
                .child(
                    div()
                        .text_size(px(11.5))
                        .text_color(t.text_muted)
                        .child(format!("{}: {} 条", i.t("当前页展示", "Showing"), logs.len())),
                ),
        )
        .child(thead)
        .child(tbody)
        .child(pagination)
        .into_any_element()
}

fn render_providers_table(ws: &Workspace, _cx: &mut Context<Workspace>) -> gpui::AnyElement {
    let t = ws.theme.clone();
    let i = ws.i18n;

    let stats = &ws.ui.usage_provider_stats;

    let thead = div()
        .flex()
        .items_center()
        .px(px(12.0))
        .py(px(8.0))
        .bg(t.input_bg)
        .rounded_t(px(8.0))
        .text_size(px(11.5))
        .font_weight(gpui::FontWeight::SEMIBOLD)
        .text_color(t.text_secondary)
        .child(div().flex_1().child(i.t("Provider 来源", "Provider Name")))
        .child(div().w(px(100.0)).text_right().child(i.t("请求次数", "Requests")))
        .child(div().w(px(120.0)).text_right().child(i.t("消耗 Tokens", "Tokens")))
        .child(div().w(px(110.0)).text_right().child(i.t("成本 (USD)", "Total Cost")))
        .child(div().w(px(100.0)).text_right().child(i.t("成功率", "Success Rate")))
        .child(div().w(px(90.0)).text_right().child(i.t("平均延迟", "Avg Latency")));

    let tbody = if stats.is_empty() {
        div()
            .py(px(40.0))
            .flex()
            .items_center()
            .justify_center()
            .text_size(px(12.5))
            .text_color(t.text_muted)
            .child(i.t("暂无 Provider 统计数据", "No provider statistics available"))
            .into_any_element()
    } else {
        div()
            .flex()
            .flex_col()
            .children(stats.iter().map(|p| {
                let cost_num = p.total_cost.parse::<f64>().unwrap_or(0.0);
                div()
                    .flex()
                    .items_center()
                    .px(px(12.0))
                    .py(px(8.0))
                    .border_b_1()
                    .border_color(t.card_border)
                    .text_size(px(12.0))
                    .text_color(t.text_primary)
                    .hover(|s| s.bg(t.input_bg))
                    .child(
                        div()
                            .flex_1()
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(t.accent)
                            .child(p.provider_name.clone()),
                    )
                    .child(div().w(px(100.0)).text_right().child(format_tokens_number(p.request_count)))
                    .child(div().w(px(120.0)).text_right().child(format_tokens_number(p.total_tokens)))
                    .child(div().w(px(110.0)).text_right().font_weight(gpui::FontWeight::SEMIBOLD).text_color(crate::rgba_const(0x10b981ff)).child(format!("${cost_num:.4}")))
                    .child(div().w(px(100.0)).text_right().child(format!("{:.1}%", p.success_rate)))
                    .child(div().w(px(90.0)).text_right().text_color(t.text_muted).child(format!("{}ms", p.avg_latency_ms)))
            }))
            .into_any_element()
    };

    div()
        .p(px(16.0))
        .rounded(px(12.0))
        .bg(t.card_bg)
        .border_1()
        .border_color(t.card_border)
        .flex()
        .flex_col()
        .gap(px(8.0))
        .child(thead)
        .child(tbody)
        .into_any_element()
}

fn render_models_table(ws: &Workspace, _cx: &mut Context<Workspace>) -> gpui::AnyElement {
    let t = ws.theme.clone();
    let i = ws.i18n;

    let stats = &ws.ui.usage_model_stats;

    let thead = div()
        .flex()
        .items_center()
        .px(px(12.0))
        .py(px(8.0))
        .bg(t.input_bg)
        .rounded_t(px(8.0))
        .text_size(px(11.5))
        .font_weight(gpui::FontWeight::SEMIBOLD)
        .text_color(t.text_secondary)
        .child(div().flex_1().child(i.t("模型名称", "Model Name")))
        .child(div().w(px(100.0)).text_right().child(i.t("请求次数", "Requests")))
        .child(div().w(px(130.0)).text_right().child(i.t("消耗 Tokens", "Tokens")))
        .child(div().w(px(120.0)).text_right().child(i.t("总成本 (USD)", "Total Cost")))
        .child(div().w(px(120.0)).text_right().child(i.t("每次请求均价", "Avg Cost / Req")));

    let tbody = if stats.is_empty() {
        div()
            .py(px(40.0))
            .flex()
            .items_center()
            .justify_center()
            .text_size(px(12.5))
            .text_color(t.text_muted)
            .child(i.t("暂无模型统计数据", "No model statistics available"))
            .into_any_element()
    } else {
        div()
            .flex()
            .flex_col()
            .children(stats.iter().map(|m| {
                let cost_num = m.total_cost.parse::<f64>().unwrap_or(0.0);
                let avg_cost_num = m.avg_cost_per_request.parse::<f64>().unwrap_or(0.0);
                div()
                    .flex()
                    .items_center()
                    .px(px(12.0))
                    .py(px(8.0))
                    .border_b_1()
                    .border_color(t.card_border)
                    .text_size(px(12.0))
                    .text_color(t.text_primary)
                    .hover(|s| s.bg(t.input_bg))
                    .child(
                        div()
                            .flex_1()
                            .font_family(".AppleSystemUIFontMonospaced")
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .child(m.model.clone()),
                    )
                    .child(div().w(px(100.0)).text_right().child(format_tokens_number(m.request_count)))
                    .child(div().w(px(130.0)).text_right().child(format_tokens_number(m.total_tokens)))
                    .child(div().w(px(120.0)).text_right().font_weight(gpui::FontWeight::SEMIBOLD).text_color(crate::rgba_const(0x10b981ff)).child(format!("${cost_num:.4}")))
                    .child(div().w(px(120.0)).text_right().text_color(t.text_secondary).child(format!("${avg_cost_num:.6}")))
            }))
            .into_any_element()
    };

    div()
        .p(px(16.0))
        .rounded(px(12.0))
        .bg(t.card_bg)
        .border_1()
        .border_color(t.card_border)
        .flex()
        .flex_col()
        .gap(px(8.0))
        .child(thead)
        .child(tbody)
        .into_any_element()
}

fn render_pricing_table(ws: &mut Workspace, cx: &mut Context<Workspace>) -> gpui::AnyElement {
    let t = ws.theme.clone();
    let i = ws.i18n;

    let apps = [
        ("claude", "Claude Code", crate::icons::CLAUDE_SVG, crate::rgba_const(0xd97757ff)),
        ("codex", "Codex", crate::icons::OPENAI_SVG, crate::rgba_const(0x10a37fff)),
        ("gemini", "Gemini", crate::icons::GEMINI_SVG, crate::rgba_const(0x3186ffff)),
        ("grok", "Grok", crate::icons::GROK_SVG, crate::rgba_const(0xf43f5eff)),
    ];

    // Ensure inputs and sources are initialized
    for (app_id, _, _, _) in &apps {
        let app_key = app_id.to_string();
        if !ws.ui.usage_app_pricing_inputs.contains_key(&app_key) {
            let mult = ws.ui.usage_app_pricing_configs
                .iter()
                .find(|c| c.app_type == *app_id)
                .map(|c| c.cost_multiplier.clone())
                .unwrap_or_else(|| "1.0".to_string());
            let ti = cx.new(|cx| {
                let mut inp = TextInput::new("1.0", cx);
                inp.set_text_silent(mult, cx);
                inp
            });
            ws.ui.usage_app_pricing_inputs.insert(app_key.clone(), ti);
        }
        if !ws.ui.usage_app_pricing_sources.contains_key(&app_key) {
            let src = ws.ui.usage_app_pricing_configs
                .iter()
                .find(|c| c.app_type == *app_id)
                .map(|c| c.pricing_model_source.clone())
                .unwrap_or_else(|| "response".to_string());
            ws.ui.usage_app_pricing_sources.insert(app_key, src);
        }
    }

    let save_btn = button_with_icon_l(
        "save-app-pricing-btn",
        crate::icons::CHECK_SVG,
        i.t("保存全局配置", "Save Configuration"),
        ButtonVariant::Primary,
        &t,
        cx,
        |ws, _, _, cx| {
            let db = match ws.ensure_usage_db() {
                Some(d) => d,
                None => return,
            };
            let apps = ["claude", "codex", "gemini", "grok"];
            let mut valid = true;
            for app in &apps {
                let mult_str = ws.ui.usage_app_pricing_inputs.get(*app)
                    .map(|ti| ti.read(cx).text().trim().to_string())
                    .unwrap_or_else(|| "1.0".to_string());
                if mult_str.is_empty() || mult_str.parse::<f64>().is_err() || mult_str.parse::<f64>().unwrap() < 0.0 {
                    ws.ui.toast(format!("{}: {}", app, ws.i18n.t("倍率必须是非负有效数字", "Multiplier must be a valid non-negative number")), true);
                    valid = false;
                    break;
                }
            }
            if valid {
                for app in &apps {
                    let mult_str = ws.ui.usage_app_pricing_inputs.get(*app)
                        .map(|ti| ti.read(cx).text().trim().to_string())
                        .unwrap_or_else(|| "1.0".to_string());
                    let src = ws.ui.usage_app_pricing_sources.get(*app)
                        .cloned()
                        .unwrap_or_else(|| "response".to_string());
                    let _ = db.set_app_pricing_config(app, &mult_str, &src);
                }
                ws.refresh_usage_data();
                ws.ui.toast(ws.i18n.t("全局计费默认配置已保存", "Global pricing configuration saved").to_string(), false);
                cx.notify();
            }
        },
    );

    let global_thead = div()
        .flex()
        .items_center()
        .px(px(14.0))
        .py(px(8.0))
        .bg(t.input_bg)
        .rounded_t(px(8.0))
        .text_size(px(11.5))
        .font_weight(gpui::FontWeight::SEMIBOLD)
        .text_color(t.text_secondary)
        .child(div().w(px(160.0)).child(i.t("应用名称", "Application")))
        .child(div().w(px(140.0)).child(i.t("成本倍率", "Cost Multiplier")))
        .child(div().w(px(220.0)).child(i.t("定价模型来源", "Pricing Model Source")))
        .child(div().flex_1().child(i.t("计费解析说明", "Billing Behavior")));

    let mut global_rows = Vec::new();
    for (app_id, app_name, icon, brand_color) in &apps {
        let app_key = app_id.to_string();
        let ti = ws.ui.usage_app_pricing_inputs.get(&app_key).unwrap().clone();
        let current_source = ws.ui.usage_app_pricing_sources.get(&app_key).cloned().unwrap_or_else(|| "response".to_string());
        let is_resp = current_source == "response";

        let source_toggle = {
            let key1 = app_key.clone();
            let key2 = app_key.clone();
            div()
                .flex()
                .items_center()
                .p(px(2.0))
                .rounded(px(6.0))
                .bg(t.input_bg)
                .border_1()
                .border_color(t.card_border)
                .child(
                    div()
                        .id(gpui::ElementId::Name(format!("source-toggle-resp-{}", app_id).into()))
                        .cursor_pointer()
                        .px(px(8.0))
                        .py(px(3.0))
                        .rounded(px(4.0))
                        .text_size(px(11.0))
                        .font_weight(if is_resp { gpui::FontWeight::SEMIBOLD } else { gpui::FontWeight::NORMAL })
                        .text_color(if is_resp { t.accent } else { t.text_muted })
                        .bg(if is_resp { t.tab_active_bg } else { crate::rgba_const(0x00000000) })
                        .hover(|s| s.bg(t.card_hover))
                        .child(i.t("响应模型 (优先)", "Response (Priority)"))
                        .on_click(cx.listener(move |ws, _, _, cx| {
                            ws.ui.usage_app_pricing_sources.insert(key1.clone(), "response".to_string());
                            cx.notify();
                        })),
                )
                .child(
                    div()
                        .id(gpui::ElementId::Name(format!("source-toggle-req-{}", app_id).into()))
                        .cursor_pointer()
                        .px(px(8.0))
                        .py(px(3.0))
                        .rounded(px(4.0))
                        .text_size(px(11.0))
                        .font_weight(if !is_resp { gpui::FontWeight::SEMIBOLD } else { gpui::FontWeight::NORMAL })
                        .text_color(if !is_resp { t.accent } else { t.text_muted })
                        .bg(if !is_resp { t.tab_active_bg } else { crate::rgba_const(0x00000000) })
                        .hover(|s| s.bg(t.card_hover))
                        .child(i.t("请求模型", "Request Model"))
                        .on_click(cx.listener(move |ws, _, _, cx| {
                            ws.ui.usage_app_pricing_sources.insert(key2.clone(), "request".to_string());
                            cx.notify();
                        })),
                )
        };

        let behavior_desc = if is_resp {
            i.t("优先按服务端响应中实际调用的模型单价 × 倍率计费", "Uses upstream response model price × multiplier")
        } else {
            i.t("强制按客户端发起请求时填写的模型单价 × 倍率计费", "Uses client request model price × multiplier")
        };

        global_rows.push(
            div()
                .flex()
                .items_center()
                .px(px(14.0))
                .py(px(10.0))
                .border_b_1()
                .border_color(t.card_border)
                .hover(|s| s.bg(t.input_bg.opacity(0.4)))
                .child(
                    div()
                        .w(px(160.0))
                        .flex()
                        .items_center()
                        .gap(px(8.0))
                        .child(crate::icons::svg_icon(icon, px(16.0), *brand_color))
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .child(
                                    div()
                                        .text_size(px(12.5))
                                        .font_weight(gpui::FontWeight::MEDIUM)
                                        .text_color(t.text_primary)
                                        .child(*app_name),
                                )
                                .child(
                                    div()
                                        .text_size(px(10.5))
                                        .font_family(".AppleSystemUIFontMonospaced")
                                        .text_color(t.text_muted)
                                        .child(*app_id),
                                ),
                        ),
                )
                .child(
                    div()
                        .w(px(140.0))
                        .flex()
                        .items_center()
                        .gap(px(6.0))
                        .child(
                            div()
                                .w(px(85.0))
                                .h(px(28.0))
                                .px(px(8.0))
                                .rounded(px(5.0))
                                .bg(t.input_bg)
                                .border_1()
                                .border_color(t.card_border)
                                .flex()
                                .items_center()
                                .child(ti),
                        )
                        .child(
                            div()
                                .text_size(px(11.0))
                                .text_color(t.text_muted)
                                .child("x"),
                        ),
                )
                .child(div().w(px(220.0)).child(source_toggle))
                .child(
                    div()
                        .flex_1()
                        .text_size(px(11.5))
                        .text_color(t.text_secondary)
                        .child(behavior_desc),
                ),
        );
    }

    let global_pricing_card = div()
        .p(px(16.0))
        .rounded(px(12.0))
        .bg(t.card_bg)
        .border_1()
        .border_color(t.card_border)
        .flex()
        .flex_col()
        .gap(px(12.0))
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
                        .child(
                            div()
                                .text_size(px(14.0))
                                .font_weight(gpui::FontWeight::SEMIBOLD)
                                .text_color(t.text_primary)
                                .child(i.t("全局计费默认配置", "Global Pricing Defaults")),
                        )
                        .child(
                            div()
                                .text_size(px(11.5))
                                .text_color(t.text_muted)
                                .child(i.t(
                                    "设置各应用的基础计费倍率与模型解析来源 (对标 CC-Switch pricing config)",
                                    "Configure default cost multipliers and pricing model sources per application (CC-Switch parity)",
                                )),
                        ),
                )
                .child(save_btn),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .rounded(px(8.0))
                .overflow_hidden()
                .border_1()
                .border_color(t.card_border)
                .child(global_thead)
                .children(global_rows),
        );

    // 2. Model Pricing Rules Table
    let pricing = &ws.ui.usage_pricing;

    let thead = div()
        .flex()
        .items_center()
        .px(px(12.0))
        .py(px(8.0))
        .bg(t.input_bg)
        .rounded_t(px(8.0))
        .text_size(px(11.5))
        .font_weight(gpui::FontWeight::SEMIBOLD)
        .text_color(t.text_secondary)
        .child(div().flex_1().child(i.t("模型名称", "Model Name")))
        .child(div().w(px(120.0)).text_right().child(i.t("输入 $/1M", "Input $/1M")))
        .child(div().w(px(120.0)).text_right().child(i.t("输出 $/1M", "Output $/1M")))
        .child(div().w(px(120.0)).text_right().child(i.t("缓存读 $/1M", "Cache Read $/1M")))
        .child(div().w(px(120.0)).text_right().child(i.t("缓存写 $/1M", "Cache Write $/1M")));

    let tbody = if pricing.is_empty() {
        div()
            .py(px(40.0))
            .flex()
            .items_center()
            .justify_center()
            .text_size(px(12.5))
            .text_color(t.text_muted)
            .child(i.t("定价库为空", "No pricing models registered"))
            .into_any_element()
    } else {
        div()
            .id("pricing-rows-scroll")
            .flex()
            .flex_col()
            .max_h(px(400.0))
            .overflow_y_scroll()
            .children(pricing.iter().map(|p| {
                div()
                    .flex()
                    .items_center()
                    .px(px(12.0))
                    .py(px(8.0))
                    .border_b_1()
                    .border_color(t.card_border)
                    .text_size(px(12.0))
                    .text_color(t.text_primary)
                    .hover(|s| s.bg(t.input_bg))
                    .child(
                        div()
                            .flex_1()
                            .flex()
                            .flex_col()
                            .child(div().font_weight(gpui::FontWeight::MEDIUM).child(p.display_name.clone()))
                            .child(div().text_size(px(10.5)).font_family(".AppleSystemUIFontMonospaced").text_color(t.text_muted).child(p.model_id.clone())),
                    )
                    .child(div().w(px(120.0)).text_right().child(format!("${}", p.input_cost_per_million)))
                    .child(div().w(px(120.0)).text_right().child(format!("${}", p.output_cost_per_million)))
                    .child(div().w(px(120.0)).text_right().text_color(crate::rgba_const(0x10b981ff)).child(format!("${}", p.cache_read_cost_per_million)))
                    .child(div().w(px(120.0)).text_right().text_color(crate::rgba_const(0xf59e0bff)).child(format!("${}", p.cache_creation_cost_per_million)))
            }))
            .into_any_element()
    };

    let model_pricing_card = div()
        .p(px(16.0))
        .rounded(px(12.0))
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
                        .text_size(px(13.5))
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .text_color(t.text_primary)
                        .child(format!("{}: {} 个已配置", i.t("模型定价规则库", "Model Pricing Rules"), pricing.len())),
                ),
        )
        .child(thead)
        .child(tbody);

    div()
        .flex()
        .flex_col()
        .gap(px(16.0))
        .child(global_pricing_card)
        .child(model_pricing_card)
        .into_any_element()
}

// ---------------------------------------------------------------------------
// 5. Session Sync Bottom Card
// ---------------------------------------------------------------------------

fn render_session_sync_card(ws: &mut Workspace, cx: &mut Context<Workspace>) -> gpui::AnyElement {
    let t = ws.theme.clone();
    let i = ws.i18n;

    let is_syncing = ws.ui.usage_syncing;
    let auto_sync = ws.ui.usage_auto_sync;

    let sync_btn = button_with_icon_l(
        "manual-session-sync-btn",
        crate::icons::REFRESH_SVG,
        if is_syncing {
            i.t("正在同步中...", "Syncing...")
        } else {
            i.t("立即扫描同步", "Sync Now")
        },
        ButtonVariant::Primary,
        &t,
        cx,
        |ws, _, _, cx| {
            if ws.ui.usage_syncing {
                return;
            }
            ws.ui.usage_syncing = true;
            cx.notify();

            let paths = ws.paths.clone();
            let db_opt = ws.ensure_usage_db();

            let weak = cx.entity().downgrade();
            cx.spawn(async move |_this, cx| {
                let res = if let Some(db) = db_opt {
                    db.sync_session_usage(&paths)
                } else {
                    Err("数据库未初始化".to_string())
                };

                let _ = weak.update(cx, |ws: &mut Workspace, cx| {
                    ws.ui.usage_syncing = false;
                    match res {
                        Ok(rep) => {
                            ws.refresh_usage_data();
                            let msg = format!(
                                "会话用量同步完成：扫描 {} 个文件，新增入库 {} 条记录",
                                rep.files_scanned, rep.imported
                            );
                            ws.ui.toast(msg, false);
                        }
                        Err(e) => {
                            ws.ui.toast(format!("同步失败: {e}"), true);
                        }
                    }
                    cx.notify();
                });
            })
            .detach();
        },
    );

    let auto_sync_toggle = toggle(
        "auto-session-sync-toggle",
        auto_sync,
        &t,
        cx,
        |ws, _, _, cx| {
            ws.ui.usage_auto_sync = !ws.ui.usage_auto_sync;
            cx.notify();
        },
    );

    div()
        .p(px(16.0))
        .rounded(px(12.0))
        .bg(t.card_bg)
        .border_1()
        .border_color(t.card_border)
        .flex()
        .items_center()
        .justify_between()
        .gap(px(16.0))
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(12.0))
                .child(
                    div()
                        .p(px(8.0))
                        .rounded(px(8.0))
                        .bg(crate::rgba_const(0x0284c720))
                        .child(crate::icons::svg_icon(crate::icons::DATABASE_SVG, px(18.0), crate::rgba_const(0x0284c7ff))),
                )
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(2.0))
                        .child(
                            div()
                                .text_size(px(13.5))
                                .font_weight(gpui::FontWeight::SEMIBOLD)
                                .text_color(t.text_primary)
                                .child(i.t("本地会话记录自动扫描同步", "Local Session Record Sync")),
                        )
                        .child(
                            div()
                                .text_size(px(11.5))
                                .text_color(t.text_muted)
                                .child(i.t(
                                    "自动追踪 Claude Code、Codex、Gemini、Grok、Pi 等本地 CLI 工具的会话日志与 Token 消耗",
                                    "Track local CLI session logs and token usage from Claude Code, Codex, Gemini, Grok, Pi",
                                )),
                        ),
                ),
        )
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(12.0))
                .child(auto_sync_toggle)
                .child(sync_btn),
        )
        .into_any_element()
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn format_tokens_number(val: u64) -> String {
    let s = val.to_string();
    let mut result = String::new();
    let mut count = 0;
    for c in s.chars().rev() {
        if count > 0 && count % 3 == 0 {
            result.push(',');
        }
        result.push(c);
        count += 1;
    }
    result.chars().rev().collect()
}

fn format_tokens_short(val: u64, is_zh: bool) -> String {
    if is_zh {
        if val >= 100_000_000 {
            format!("{:.2} 亿", val as f64 / 100_000_000.0)
        } else if val >= 10_000 {
            format!("{:.1} 万", val as f64 / 10_000.0)
        } else {
            format_tokens_number(val)
        }
    } else {
        if val >= 1_000_000_000 {
            format!("{:.2}B", val as f64 / 1_000_000_000.0)
        } else if val >= 1_000_000 {
            format!("{:.2}M", val as f64 / 1_000_000.0)
        } else if val >= 1_000 {
            format!("{:.1}K", val as f64 / 1_000.0)
        } else {
            format_tokens_number(val)
        }
    }
}

fn format_timestamp(ts: i64) -> String {
    use chrono::{Local, TimeZone};
    if let Some(dt) = Local.timestamp_opt(ts, 0).single() {
        dt.format("%m-%d %H:%M").to_string()
    } else {
        "--".to_string()
    }
}
