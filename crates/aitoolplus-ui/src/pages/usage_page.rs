//! Usage statistics dashboard: hero summary, trend charts, request logs,
//! provider/model breakdowns, pricing configuration, and session log synchronization.
//!
//! Replicates CC-Switch's UsageDashboard functionality with GPUI Kit native UI.

use gpui::{Context, IntoElement, div, prelude::*, px};

use crate::components::{ButtonVariant, button_l, button_with_icon_l, button_with_icon_loading_l, toggle};
use crate::theme::Theme;
use crate::workspace::Workspace;

use super::{UsageChartMetric, UsageRangePreset, UsageSubTab};
use crate::text_input::TextInput;

pub fn render_usage_page(ws: &mut Workspace, cx: &mut Context<Workspace>) -> gpui::AnyElement {
    // Lazy initial load if summary hasn't been fetched yet
    if ws.ui.usage_summary.is_none() && !ws.ui.usage_refreshing && !ws.ui.usage_load_failed {
        ws.refresh_usage_data(cx);
    }

    // Auto-scan local sessions on enter if enabled
    // Wait until the first read has landed. Starting the scan in the same
    // frame writes usage.db while that read is still open.
    if ws.settings.usage_auto_scan_sessions
        && !ws.ui.usage_has_auto_scanned
        && !ws.ui.usage_syncing
        && !ws.ui.usage_refreshing
        && ws.ui.usage_summary.is_some()
    {
        ws.ui.usage_has_auto_scanned = true;
        ws.trigger_session_sync(cx, false);
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
            let tooltip_text = i.raw(zh, en);
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
                    ws.refresh_usage_data(cx);
                    cx.notify();
                }))
        }));

    // Provider Filter Button & Menu
    let current_provider_label: gpui::SharedString = match &ws.ui.usage_provider_filter {
        Some(p) => {
            // Find readable name from provider stats if available
            let display = ws
                .ui
                .usage_provider_options
                .iter()
                .chain(ws.ui.usage_provider_stats.iter())
                .find(|s| &s.provider_id == p)
                .map(|s| s.provider_name.as_str())
                .unwrap_or(p.as_str());
            display.to_string().into()
        }
        None => i.t("usage.all_sources"),
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
                    .id(gpui::ElementId::Name(
                        format!("usage-provider-filter-label-{current_provider_label}").into(),
                    ))
                    .max_w(px(140.0))
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .child(current_provider_label),
            )
            .child(crate::icons::svg_icon(
                if is_open { crate::icons::CHEVRON_UP_SVG } else { crate::icons::CHEVRON_DOWN_SVG },
                px(11.0),
                t.text_muted,
            ))
            .on_mouse_down(gpui::MouseButton::Left, cx.listener(move |ws, _, _, cx| {
                cx.stop_propagation();
                ws.ui.usage_provider_menu_open = !is_open;
                ws.ui.usage_model_menu_open = false;
                ws.ui.usage_date_menu_open = false;
                ws.ui.usage_status_menu_open = false;
                cx.notify();
            }))
    };

    // Model Filter Button & Menu
    let current_model_label: gpui::SharedString = match &ws.ui.usage_model_filter {
        Some(m) => m.clone().into(),
        None => i.t("usage.all_models"),
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
                    .id(gpui::ElementId::Name(
                        format!("usage-model-filter-label-{current_model_label}").into(),
                    ))
                    .max_w(px(140.0))
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .child(current_model_label),
            )
            .child(crate::icons::svg_icon(
                if is_open { crate::icons::CHEVRON_UP_SVG } else { crate::icons::CHEVRON_DOWN_SVG },
                px(11.0),
                t.text_muted,
            ))
            .on_mouse_down(gpui::MouseButton::Left, cx.listener(move |ws, _, _, cx| {
                cx.stop_propagation();
                ws.ui.usage_model_menu_open = !is_open;
                ws.ui.usage_provider_menu_open = false;
                ws.ui.usage_date_menu_open = false;
                ws.ui.usage_status_menu_open = false;
                cx.notify();
            }))
    };

    // Refresh frequency quick toggle: 0s (Off) -> 10s -> 30s -> 60s
    let refresh_interval_sec = ws.ui.usage_refresh_interval;
    let refresh_interval_label: gpui::SharedString = match refresh_interval_sec {
        0 => i.t("usage.auto_off"),
        10 => i.t("usage.auto_10s"),
        30 => i.t("usage.auto_30s"),
        60 => i.t("usage.auto_60s"),
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
                format!("{}: {}s", ws.i18n.t("usage.refresh_interval_set_to"), ws.ui.usage_refresh_interval)
            } else {
                ws.i18n.t("usage.auto_refresh_turned_off").to_string()
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
            .on_mouse_down(gpui::MouseButton::Left, cx.listener(move |ws, _, _, cx| {
                cx.stop_propagation();
                ws.ui.usage_date_menu_open = !is_open;
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
                .on_mouse_down(gpui::MouseButton::Left, cx.listener(move |ws, _, _, cx| {
                    cx.stop_propagation();
                    ws.ui.usage_range = preset;
                    ws.ui.usage_date_menu_open = false;
                    ws.ui.usage_page = 0;
                    ws.ui.usage_hovered_bucket = None;
                    ws.refresh_usage_data(cx);
                    cx.notify();
                }))
        }).collect();

        Some(
            div()
                .id("usage-date-menu-dropdown")
                .on_mouse_down_out(cx.listener(|ws, _, window, cx| {
                    let entity = cx.entity().clone();
                    window.defer(cx, move |window, cx| {
                        let _ = entity.update(cx, |ws, cx| {
                            ws.ui.usage_date_menu_open = false;
                            cx.notify();
                        });
                        window.refresh();
                    });
                }))
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
    let refresh_btn = button_with_icon_loading_l(
        "usage-refresh-btn",
        crate::icons::REFRESH_SVG,
        i.t("usage.refresh"),
        ButtonVariant::Secondary,
        ws.ui.usage_refreshing,
        &t,
        cx,
        |ws, _, _, cx| {
            ws.ui.usage_load_failed = false;
            ws.refresh_usage_data(cx);
        },
    );

    // Session sync action button
    let is_syncing = ws.ui.usage_syncing;
    let sync_sessions_btn = button_with_icon_loading_l(
        "usage-header-sync-sessions-btn",
        crate::icons::REFRESH_SVG,
        i.t("usage.sync_sessions"),
        ButtonVariant::Secondary,
        is_syncing,
        &t,
        cx,
        |ws, _, _, cx| {
            ws.trigger_session_sync(cx, true);
        },
    );

    // Auto-scan sessions pill / toggle
    let auto_scan_sessions = ws.settings.usage_auto_scan_sessions;
    let auto_scan_toggle = div()
        .id("usage-header-auto-scan-pill")
        .cursor_pointer()
        .h(px(30.0))
        .px(px(8.0))
        .rounded(px(6.0))
        .bg(if auto_scan_sessions { t.tab_active_bg } else { t.input_bg })
        .border_1()
        .border_color(if auto_scan_sessions { t.accent } else { t.card_border })
        .flex()
        .items_center()
        .gap(px(6.0))
        .tooltip(move |_window, cx| {
            cx.new(|_| crate::components::Tooltip::new(
                i.t("usage.auto_scan_local_cli").to_string()
            )).into()
        })
        .on_click(cx.listener(|ws, _, _, cx| {
            ws.settings.usage_auto_scan_sessions = !ws.settings.usage_auto_scan_sessions;
            ws.persist_settings();
            let msg = if ws.settings.usage_auto_scan_sessions {
                ws.trigger_session_sync(cx, true);
                ws.i18n.t("usage.auto_scan_session_history_2").to_string()
            } else {
                ws.i18n.t("usage.auto_scan_session_history").to_string()
            };
            ws.ui.toast(msg, false);
            cx.notify();
        }))
        .child(crate::icons::svg_icon(
            crate::icons::DATABASE_SVG,
            px(12.5),
            if auto_scan_sessions { t.accent } else { t.text_muted },
        ))
        .child(
            div()
                .text_size(px(12.0))
                .text_color(if auto_scan_sessions { t.accent } else { t.text_secondary })
                .child(i.t("usage.auto_scan")),
        )
        .child(
            div()
                .w(px(7.0))
                .h(px(7.0))
                .rounded_full()
                .bg(if auto_scan_sessions { crate::rgba_const(0x10b981ff) } else { t.text_muted }),
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
            .child(i.t("usage.all_sources"))
            .on_mouse_down(gpui::MouseButton::Left, cx.listener(|ws, _, _, cx| {
                cx.stop_propagation();
                ws.ui.usage_provider_filter = None;
                ws.ui.usage_model_filter = None;
                ws.ui.usage_provider_menu_open = false;
                ws.ui.usage_page = 0;
                ws.refresh_usage_data(cx);
                cx.notify();
            }))];

        let provider_choices = if ws.ui.usage_provider_options.is_empty() {
            &ws.ui.usage_provider_stats
        } else {
            &ws.ui.usage_provider_options
        };
        for (idx, stat) in provider_choices.iter().enumerate() {
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
                    .on_mouse_down(gpui::MouseButton::Left, cx.listener(move |ws, _, _, cx| {
                        cx.stop_propagation();
                        ws.ui.usage_provider_filter = Some(pid.clone());
                        ws.ui.usage_model_filter = None;
                        ws.ui.usage_provider_menu_open = false;
                        ws.ui.usage_page = 0;
                        ws.refresh_usage_data(cx);
                        cx.notify();
                    })),
            );
        }

        Some(
            div()
                .id("usage-provider-menu-dropdown")
                .on_mouse_down_out(cx.listener(|ws, _, window, cx| {
                    let entity = cx.entity().clone();
                    window.defer(cx, move |window, cx| {
                        let _ = entity.update(cx, |ws, cx| {
                            ws.ui.usage_provider_menu_open = false;
                            cx.notify();
                        });
                        window.refresh();
                    });
                }))
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
            .child(i.t("usage.all_models"))
            .on_mouse_down(gpui::MouseButton::Left, cx.listener(|ws, _, _, cx| {
                cx.stop_propagation();
                ws.ui.usage_model_filter = None;
                ws.ui.usage_model_menu_open = false;
                ws.ui.usage_page = 0;
                ws.refresh_usage_data(cx);
                cx.notify();
            }))];

        let model_choices = if ws.ui.usage_model_options.is_empty() {
            &ws.ui.usage_model_stats
        } else {
            &ws.ui.usage_model_options
        };
        for (idx, stat) in model_choices.iter().enumerate() {
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
                    .on_mouse_down(gpui::MouseButton::Left, cx.listener(move |ws, _, _, cx| {
                        cx.stop_propagation();
                        ws.ui.usage_model_filter = Some(mname.clone());
                        ws.ui.usage_model_menu_open = false;
                        ws.ui.usage_page = 0;
                        ws.refresh_usage_data(cx);
                        cx.notify();
                    })),
            );
        }

        Some(
            div()
                .id("usage-model-menu-dropdown")
                .on_mouse_down_out(cx.listener(|ws, _, window, cx| {
                    let entity = cx.entity().clone();
                    window.defer(cx, move |window, cx| {
                        let _ = entity.update(cx, |ws, cx| {
                            ws.ui.usage_model_menu_open = false;
                            cx.notify();
                        });
                        window.refresh();
                    });
                }))
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
                                .child(i.t("usage.usage_statistics")),
                        )
                        .child(
                            div()
                                .text_size(px(12.5))
                                .text_color(t.text_muted)
                                .child(i.t("usage.complete_parity_with_cc")),
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
                                .children(provider_menu.map(|menu| gpui::deferred(menu.occlude()))),
                        )
                        .child(
                            div()
                                .relative()
                                .child(model_btn)
                                .children(model_menu.map(|menu| gpui::deferred(menu.occlude()))),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(8.0))
                        .child(auto_scan_toggle)
                        .child(sync_sessions_btn)
                        .child(refresh_interval_btn)
                        .child(
                            div()
                                .relative()
                                .child(date_range_btn)
                                .children(date_menu.map(|menu| gpui::deferred(menu.occlude()))),
                        )
                        .child(refresh_btn),
                ),
        )
        .into_any_element()
}

// ---------------------------------------------------------------------------
// 2. Hero Summary Section (UsageHero)
// ---------------------------------------------------------------------------

fn render_metric_card(
    id: impl Into<gpui::ElementId>,
    label: impl Into<gpui::SharedString>,
    value: impl Into<gpui::SharedString>,
    tooltip_text: Option<gpui::SharedString>,
    help_text: Option<gpui::SharedString>,
    t: &Theme,
) -> gpui::Stateful<gpui::Div> {
    let id: gpui::ElementId = id.into();
    let help_id = gpui::ElementId::Name(format!("{}-help", id).into());
    let mut card = div()
        .id(id)
        .h(px(76.0))
        .min_w_0()
        .flex()
        .flex_col()
        .justify_between()
        .rounded(px(8.0))
        .border_1()
        .border_color(t.card_border)
        .bg(t.card_bg)
        .px(px(14.0))
        .py(px(12.0));

    if let Some(tip) = tooltip_text {
        card = card.tooltip(move |_win, cx| {
            cx.new(|_| crate::components::Tooltip::new(tip.clone())).into()
        });
    }

    let mut header = div()
        .flex()
        .items_center()
        .gap(px(4.0))
        .min_w_0()
        .child(
            div()
                .truncate()
                .text_size(px(12.0))
                .text_color(t.text_secondary)
                .child(label.into()),
        );

    if let Some(help) = help_text {
        header = header.child(
            div()
                .id(help_id)
                .flex_none()
                .cursor_pointer()
                .tooltip(move |_win, cx| {
                    cx.new(|_| crate::components::Tooltip::with_max_width(help.clone(), px(320.0))).into()
                })
                .child(crate::icons::svg_icon(crate::icons::INFO_SVG, px(12.0), t.text_muted)),
        );
    }

    card.child(header).child(
        div()
            .truncate()
            .text_size(px(22.0))
            .font_weight(gpui::FontWeight::BOLD)
            .text_color(t.text_primary)
            .child(value.into()),
    )
}

fn render_mini_metric(
    id: impl Into<gpui::ElementId>,
    label: impl Into<gpui::SharedString>,
    value: impl Into<gpui::SharedString>,
    tooltip_text: Option<gpui::SharedString>,
    is_muted: bool,
    t: &Theme,
) -> gpui::Stateful<gpui::Div> {
    let mut card = div()
        .id(id)
        .h(px(54.0))
        .min_w_0()
        .flex()
        .flex_col()
        .justify_between()
        .rounded(px(8.0))
        .bg(t.input_bg)
        .border_1()
        .border_color(t.card_border)
        .px(px(14.0))
        .py(px(8.0));

    if let Some(tip) = tooltip_text {
        card = card.tooltip(move |_win, cx| {
            cx.new(|_| crate::components::Tooltip::new(tip.clone())).into()
        });
    }

    card.child(
        div()
            .truncate()
            .text_size(px(11.5))
            .text_color(t.text_muted)
            .child(label.into()),
    )
    .child(
        div()
            .truncate()
            .text_size(px(15.0))
            .font_weight(gpui::FontWeight::SEMIBOLD)
            .text_color(if is_muted { t.text_muted } else { t.text_primary })
            .child(value.into()),
    )
}

fn render_usage_hero(ws: &Workspace, _cx: &mut Context<Workspace>) -> gpui::AnyElement {
    let t = ws.theme.clone();
    let i = ws.i18n;
    let is_zh = ws.settings.language == aitoolplus_core::settings::Language::Zh;

    let sum = ws.ui.usage_summary.clone().unwrap_or_default();
    let total_cost_num = sum.total_cost.parse::<f64>().unwrap_or(0.0);

    let total_cost_display = if total_cost_num == 0.0 {
        "$0.00".to_string()
    } else if total_cost_num >= 0.01 {
        format!("${:.2}", total_cost_num)
    } else {
        format!("${:.4}", total_cost_num)
    };
    let total_cost_full = format!("${:.6}", total_cost_num);

    let total_requests_display = format_tokens_number(sum.total_requests);

    let real_total_short = format_tokens_short(sum.real_total_tokens, is_zh);
    let real_total_full = format_tokens_number(sum.real_total_tokens);

    let hit_rate_pct = (sum.cache_hit_rate * 100.0).clamp(0.0, 100.0);
    let hit_percent_label = if hit_rate_pct >= 99.95 {
        format!("{:.0}%", hit_rate_pct)
    } else {
        format!("{:.1}%", hit_rate_pct)
    };

    let primary_grid = div()
        .grid()
        .grid_cols(4)
        .gap(px(10.0))
        .child(render_metric_card(
            "metric-total-cost",
            i.t("usage.total_cost"),
            total_cost_display,
            Some(total_cost_full.into()),
            None,
            &t,
        ))
        .child(render_metric_card(
            "metric-total-requests",
            i.t("usage.total_requests"),
            total_requests_display,
            None,
            None,
            &t,
        ))
        .child(render_metric_card(
            "metric-real-total",
            i.t("usage.real_consumption_tokens"),
            real_total_short,
            Some(real_total_full.into()),
            Some(i.raw(
                "真实消耗 Tokens = 新增输入 + 输出 + 缓存写入 + 缓存命中（和指标卡同口径）",
                "Real Total Tokens = Fresh Input + Output + Cache Creation + Cache Read",
            )),
            &t,
        ))
        .child(render_metric_card(
            "metric-cache-hit-rate",
            i.t("usage.cache_hit_rate"),
            hit_percent_label,
            None,
            Some(i.raw(
                "缓存命中率 = 缓存命中 / (新增输入 + 缓存写入 + 缓存命中)",
                "Cache Hit Rate = Cache Read / (Fresh Input + Cache Write + Cache Read)",
            )),
            &t,
        ));

    let secondary_grid = div()
        .grid()
        .grid_cols(4)
        .gap(px(10.0))
        .child(render_mini_metric(
            "mini-fresh-input",
            i.t("usage.fresh_input"),
            format_tokens_short(sum.total_input_tokens, is_zh),
            Some(format_tokens_number(sum.total_input_tokens).into()),
            false,
            &t,
        ))
        .child(render_mini_metric(
            "mini-output",
            i.t("usage.output_2"),
            format_tokens_short(sum.total_output_tokens, is_zh),
            Some(format_tokens_number(sum.total_output_tokens).into()),
            false,
            &t,
        ))
        .child(render_mini_metric(
            "mini-cache-write",
            i.t("usage.cache_write_2"),
            format_tokens_short(sum.total_cache_creation_tokens, is_zh),
            Some(format_tokens_number(sum.total_cache_creation_tokens).into()),
            sum.total_cache_creation_tokens == 0,
            &t,
        ))
        .child(render_mini_metric(
            "mini-cache-read",
            i.t("usage.cache_read"),
            format_tokens_short(sum.total_cache_read_tokens, is_zh),
            Some(format_tokens_number(sum.total_cache_read_tokens).into()),
            false,
            &t,
        ));

    div()
        .flex()
        .flex_col()
        .gap(px(10.0))
        .child(primary_grid)
        .child(secondary_grid)
        .into_any_element()
}

// ---------------------------------------------------------------------------
// 3. Trend Chart Section (Single Metric Bar Chart)
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

fn get_metric_stat_value(stat: &aitoolplus_core::usage::DailyStats, metric: UsageChartMetric) -> f64 {
    match metric {
        UsageChartMetric::Tokens => {
            (stat.total_input_tokens
                + stat.total_output_tokens
                + stat.total_cache_creation_tokens
                + stat.total_cache_read_tokens) as f64
        }
        UsageChartMetric::Requests => stat.request_count as f64,
        UsageChartMetric::Cost => stat.total_cost.parse::<f64>().unwrap_or(0.0),
    }
}

fn format_y_tick_label(val: f64, metric: UsageChartMetric, is_zh: bool) -> String {
    if val <= 0.00001 {
        return match metric {
            UsageChartMetric::Cost => "$0".to_string(),
            _ => "0".to_string(),
        };
    }
    match metric {
        UsageChartMetric::Tokens => format_tokens_axis_label(val.round() as u64, is_zh),
        UsageChartMetric::Requests => {
            if val >= 10_000.0 && is_zh {
                format!("{:.1}万", val / 10_000.0)
            } else if val >= 1_000.0 && !is_zh {
                format!("{:.1}K", val / 1_000.0)
            } else {
                format!("{:.0}", val)
            }
        }
        UsageChartMetric::Cost => format_cost_axis_label(val),
    }
}

fn format_metric_value(val: f64, metric: UsageChartMetric, is_zh: bool) -> String {
    match metric {
        UsageChartMetric::Tokens => {
            let num = val.round() as u64;
            format!("{} Tokens", format_tokens_number(num))
        }
        UsageChartMetric::Requests => {
            let num = val.round() as u64;
            if is_zh {
                format!("{} 次", format_tokens_number(num))
            } else {
                format!("{} reqs", format_tokens_number(num))
            }
        }
        UsageChartMetric::Cost => format!("${:.4}", val),
    }
}

fn render_trend_chart(ws: &Workspace, cx: &mut Context<Workspace>) -> gpui::AnyElement {
    let t = ws.theme.clone();
    let i = ws.i18n;
    let is_zh = ws.i18n.is_zh();

    let trends = &ws.ui.usage_trends;
    if trends.is_empty() {
        return div()
            .p(px(20.0))
            .rounded(px(10.0))
            .bg(t.card_bg)
            .border_1()
            .border_color(t.card_border)
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .py(px(40.0))
            .child(
                div()
                    .text_size(px(13.0))
                    .text_color(t.text_muted)
                    .child(i.t("usage.no_trend_data_available")),
            )
            .into_any_element();
    }

    let is_hourly = ws.ui.usage_range == UsageRangePreset::Today;
    let range_label = match ws.ui.usage_range {
        UsageRangePreset::Today => i.t("usage.today"),
        UsageRangePreset::Days7 => i.t("usage.k_7_days"),
        UsageRangePreset::Days30 => i.t("usage.k_30_days"),
        UsageRangePreset::All => i.t("usage.all_time"),
    };

    let metric = ws.ui.usage_chart_metric;
    let metric_label = match metric {
        UsageChartMetric::Tokens => "Tokens".into(),
        UsageChartMetric::Requests => i.raw("请求数", "Requests"),
        UsageChartMetric::Cost => i.raw("成本", "Cost"),
    };

    // Card Layout Constants
    let pad_left = 52.0f32;
    let pad_right = 16.0f32;
    let pad_top = 10.0f32;
    let pad_bottom = 24.0f32;
    let plot_h = 120.0f32;
    let total_chart_h = pad_top + plot_h + pad_bottom;

    let bucket_count = trends.len();

    let values: Vec<f64> = trends
        .iter()
        .map(|s| get_metric_stat_value(s, metric))
        .collect();

    let max_val = values.iter().copied().fold(0.0, f64::max);

    let ceiling = match metric {
        UsageChartMetric::Tokens => calculate_nice_ceiling(max_val).max(10_000.0),
        UsageChartMetric::Requests => calculate_nice_ceiling(max_val).max(10.0),
        UsageChartMetric::Cost => calculate_nice_ceiling(max_val).max(0.10),
    };

    // 1. Y-Axis Labels (3 levels: 0%, 50%, 100%)
    let mut y_axis_labels = Vec::new();
    for level in 0..=2 {
        let ratio = level as f32 / 2.0;
        let line_y = pad_top + (1.0 - ratio) * plot_h;
        let tick_val = ceiling * ratio as f64;
        let lbl = format_y_tick_label(tick_val, metric, is_zh);

        y_axis_labels.push(
            div()
                .absolute()
                .top(px(line_y - 8.0))
                .left_0()
                .w(px(pad_left - 8.0))
                .flex()
                .justify_end()
                .child(
                    div()
                        .text_size(px(11.0))
                        .text_color(t.text_muted)
                        .child(lbl),
                ),
        );
    }

    // 2. Bar Canvas
    let hovered_idx = ws.ui.usage_hovered_bucket;
    let grid_color = t.card_border.opacity(0.40);
    let accent_color = t.accent;
    let subtle_hover_bg = t.text_primary.opacity(0.05);

    let values_for_canvas = values.clone();
    let bar_canvas = div()
        .absolute()
        .top_0()
        .bottom_0()
        .left_0()
        .right_0()
        .child(
            gpui::canvas(
                move |bounds, _window, _cx| bounds,
                move |bounds, _, window, _cx| {
                    let cnt = values_for_canvas.len();
                    if cnt == 0 {
                        return;
                    }
                    let left_x = bounds.origin.x + px(pad_left);
                    let right_x = bounds.origin.x + bounds.size.width - px(pad_right);
                    let plot_w = (right_x.as_f32() - left_x.as_f32()).max(1.0);
                    let top_y = bounds.origin.y + px(pad_top);
                    let bottom_y = bounds.origin.y + px(pad_top + plot_h);

                    // 1. Horizontal dashed grid lines (levels 1, 2)
                    for level in 1..=2 {
                        let ratio = level as f32 / 2.0;
                        let y = bottom_y.as_f32() - ratio * plot_h;
                        let mut grid_b = gpui::PathBuilder::stroke(px(1.0)).dash_array(&[px(3.0), px(3.0)]);
                        grid_b.move_to(gpui::point(left_x, px(y)));
                        grid_b.line_to(gpui::point(right_x, px(y)));
                        if let Ok(p) = grid_b.build() {
                            window.paint_path(p, grid_color);
                        }
                    }

                    // Bottom baseline (level 0)
                    let mut baseline_b = gpui::PathBuilder::stroke(px(1.0));
                    baseline_b.move_to(gpui::point(left_x, bottom_y));
                    baseline_b.line_to(gpui::point(right_x, bottom_y));
                    if let Ok(p) = baseline_b.build() {
                        window.paint_path(p, grid_color);
                    }

                    // 2. Bars and column hover background
                    let bucket_w = plot_w / cnt as f32;
                    let max_bar_w = 36.0f32;
                    let bar_w = (bucket_w * 0.65).clamp(3.0, max_bar_w);

                    for (i, &val) in values_for_canvas.iter().enumerate() {
                        let col_left = left_x.as_f32() + i as f32 * bucket_w;
                        let col_cx = col_left + bucket_w * 0.5;

                        // Hovered column background
                        if hovered_idx == Some(i) {
                            let hover_quad = gpui::Bounds {
                                origin: gpui::point(px(col_left + 1.0), top_y),
                                size: gpui::size(px((bucket_w - 2.0).max(2.0)), px(plot_h)),
                            };
                            window.paint_quad(
                                gpui::fill(hover_quad, subtle_hover_bg).corner_radii(gpui::Corners {
                                    top_left: px(4.0),
                                    top_right: px(4.0),
                                    bottom_right: px(0.0),
                                    bottom_left: px(0.0),
                                }),
                            );
                        }

                        if val > 0.0 {
                            let ratio = (val / ceiling).clamp(0.0, 1.0) as f32;
                            let bar_h = (ratio * plot_h).max(2.0);
                            let bar_bounds = gpui::Bounds {
                                origin: gpui::point(px(col_cx - bar_w * 0.5), px(bottom_y.as_f32() - bar_h)),
                                size: gpui::size(px(bar_w), px(bar_h)),
                            };

                            let bar_color = if hovered_idx == Some(i) {
                                accent_color
                            } else {
                                accent_color.opacity(0.85)
                            };

                            window.paint_quad(
                                gpui::fill(bar_bounds, bar_color).corner_radii(gpui::Corners {
                                    top_left: px(3.0),
                                    top_right: px(3.0),
                                    bottom_right: px(0.0),
                                    bottom_left: px(0.0),
                                }),
                            );
                        }
                    }
                },
            )
            .size_full(),
        );

    // 3. X-Axis Tick Labels
    let mut x_ticks = Vec::new();
    let tick_interval = if is_hourly {
        if bucket_count <= 8 {
            1
        } else if bucket_count <= 16 {
            2
        } else {
            4
        }
    } else {
        if bucket_count <= 8 {
            1
        } else if bucket_count <= 16 {
            2
        } else if bucket_count <= 31 {
            4
        } else {
            7
        }
    };

    let mut last_tick_idx: Option<usize> = None;
    for i in 0..bucket_count {
        let is_regular_tick = i % tick_interval == 0;
        let is_last_tick = i == bucket_count - 1
            && last_tick_idx.map_or(true, |last| i >= last + tick_interval.min(2));

        if is_regular_tick || is_last_tick {
            last_tick_idx = Some(i);
            let pct = (i as f32 + 0.5) / bucket_count as f32 * 100.0;
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
                        .text_size(px(11.0))
                        .text_color(t.text_muted)
                        .child(tick_text),
                ),
            );
        }
    }

    let x_axis_row = div()
        .absolute()
        .bottom_0()
        .left(px(pad_left))
        .right(px(pad_right))
        .h(px(pad_bottom))
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

    // 5. Floating Tooltip Card
    let floating_tooltip = if let Some(h_idx) = hovered_idx {
        if h_idx < bucket_count {
            let stat = &trends[h_idx];
            let heading = format_tooltip_date(&stat.date, is_hourly);
            let val = get_metric_stat_value(stat, metric);
            let formatted_val = format_metric_value(val, metric, is_zh);

            let pct = (h_idx as f32 + 0.5) / bucket_count as f32 * 100.0;
            let is_right_half = h_idx > bucket_count / 2;

            let mut card = div()
                .absolute()
                .top(px(pad_top + 8.0))
                .px(px(12.0))
                .py(px(8.0))
                .rounded(px(8.0))
                .bg(t.card_bg)
                .border_1()
                .border_color(t.card_border)
                .shadow_md()
                .flex()
                .flex_col()
                .gap(px(4.0))
                .child(
                    div()
                        .text_size(px(12.0))
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
                        .child(div().size(px(8.0)).rounded(px(2.0)).bg(t.accent))
                        .child(div().text_color(t.text_secondary).child(metric_label.clone()))
                        .child(
                            div()
                                .font_weight(gpui::FontWeight::SEMIBOLD)
                                .text_color(t.text_primary)
                                .child(formatted_val),
                        ),
                );

            if is_right_half {
                card = card.right(gpui::relative((100.0 - pct) / 100.0)).mr(px(pad_right + 12.0));
            } else {
                card = card.left(gpui::relative(pct / 100.0)).ml(px(pad_left + 12.0));
            }

            Some(card)
        } else {
            None
        }
    } else {
        None
    };

    // Metric selector (Segmented Control)
    let metric_selector = crate::components::segmented_pill_selector(
        "usage-trend-metric",
        vec![
            (UsageChartMetric::Tokens, None, "Tokens".into()),
            (UsageChartMetric::Requests, None, i.raw("请求数", "Requests")),
            (UsageChartMetric::Cost, None, i.raw("成本", "Cost")),
        ],
        metric,
        &t,
        cx,
        |ws, m, _, cx| {
            ws.ui.usage_chart_metric = m;
            cx.notify();
        },
    );

    div()
        .rounded(px(10.0))
        .bg(t.card_bg)
        .border_1()
        .border_color(t.card_border)
        .px(px(16.0))
        .py(px(12.0))
        .flex()
        .flex_col()
        .gap(px(10.0))
        .on_mouse_move(cx.listener(|ws, _, _, cx| {
            if ws.ui.usage_hovered_bucket.is_some() {
                ws.ui.usage_hovered_bucket = None;
                cx.notify();
            }
        }))
        .child(
            // Header: title + active legend pill on left, metric selector on right
            div()
                .flex()
                .items_center()
                .justify_between()
                .gap(px(12.0))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(12.0))
                        .child(
                            div()
                                .text_size(px(13.0))
                                .font_weight(gpui::FontWeight::SEMIBOLD)
                                .text_color(t.text_secondary)
                                .child(format!("{} {}", range_label, i.t("usage.usage_trends"))),
                        )
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(6.0))
                                .child(div().size(px(8.0)).rounded(px(2.0)).bg(t.accent))
                                .child(
                                    div()
                                        .text_size(px(12.0))
                                        .text_color(t.text_secondary)
                                        .child(metric_label),
                                ),
                        ),
                )
                .child(metric_selector),
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
                .child(bar_canvas)
                .children(floating_tooltip)
                .child(hover_columns)
                .child(x_axis_row),
        )
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
            ws.refresh_usage_data(cx);
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
        (None, i.t("usage.all_status")),
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
                    ws.refresh_usage_data(cx);
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
        .child(div().w(px(110.0)).child(i.t("usage.time")))
        .child(div().w(px(130.0)).child(i.t("usage.provider")))
        .child(div().flex_1().child(i.t("usage.model")))
        .child(div().w(px(85.0)).text_right().child(i.t("usage.input")))
        .child(div().w(px(80.0)).text_right().child(i.t("usage.output")))
        .child(div().w(px(85.0)).text_right().child(i.t("usage.cost")))
        .child(div().w(px(85.0)).text_right().child(i.t("usage.latency")))
        .child(div().w(px(65.0)).text_center().child(i.t("usage.status")))
        .child(div().w(px(60.0)).text_center().child(i.t("usage.source")));

    let tbody = if logs.is_empty() {
        div()
            .py(px(40.0))
            .flex()
            .items_center()
            .justify_center()
            .text_size(px(12.5))
            .text_color(t.text_muted)
            .child(i.t("usage.no_request_logs_found"))
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
                i.t("usage.total_records"),
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
                        i.t("usage.previous"),
                        ButtonVariant::Secondary,
                        &t,
                        cx,
                        move |ws, _, _, cx| {
                            if ws.ui.usage_page > 0 {
                                ws.ui.usage_page -= 1;
                                ws.refresh_usage_data(cx);
                                cx.notify();
                            }
                        },
                    ),
                )
                .child(
                    button_l(
                        "logs-next-page",
                        i.t("usage.next"),
                        ButtonVariant::Secondary,
                        &t,
                        cx,
                        move |ws, _, _, cx| {
                            if (ws.ui.usage_page + 1) < total_pages {
                                ws.ui.usage_page += 1;
                                ws.refresh_usage_data(cx);
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
                        .child(format!("{}: {} 条", i.t("usage.showing"), logs.len())),
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
        .child(div().flex_1().child(i.t("usage.provider_name")))
        .child(div().w(px(100.0)).text_right().child(i.t("usage.requests")))
        .child(div().w(px(120.0)).text_right().child(i.t("usage.tokens")))
        .child(div().w(px(110.0)).text_right().child(i.t("usage.total_cost_2")))
        .child(div().w(px(100.0)).text_right().child(i.t("usage.success_rate")))
        .child(div().w(px(90.0)).text_right().child(i.t("usage.avg_latency")));

    let tbody = if stats.is_empty() {
        div()
            .py(px(40.0))
            .flex()
            .items_center()
            .justify_center()
            .text_size(px(12.5))
            .text_color(t.text_muted)
            .child(i.t("usage.no_provider_statistics_availab"))
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
        .child(div().flex_1().child(i.t("usage.model_name")))
        .child(div().w(px(100.0)).text_right().child(i.t("usage.requests")))
        .child(div().w(px(130.0)).text_right().child(i.t("usage.tokens")))
        .child(div().w(px(120.0)).text_right().child(i.t("usage.total_cost")))
        .child(div().w(px(120.0)).text_right().child(i.t("usage.avg_cost_req")));

    let tbody = if stats.is_empty() {
        div()
            .py(px(40.0))
            .flex()
            .items_center()
            .justify_center()
            .text_size(px(12.5))
            .text_color(t.text_muted)
            .child(i.t("usage.no_model_statistics_available"))
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
        i.t("usage.save_configuration"),
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
                    ws.ui.toast(format!("{}: {}", app, ws.i18n.t("usage.multiplier_must_be_a")), true);
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
                ws.refresh_usage_data(cx);
                ws.ui.toast(ws.i18n.t("usage.global_pricing_configuration_s").to_string(), false);
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
        .child(div().w(px(160.0)).child(i.t("usage.application")))
        .child(div().w(px(140.0)).child(i.t("usage.cost_multiplier")))
        .child(div().w(px(220.0)).child(i.t("usage.pricing_model_source")))
        .child(div().flex_1().child(i.t("usage.billing_behavior")));

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
                        .child(i.t("usage.response_priority"))
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
                        .child(i.t("usage.request_model"))
                        .on_click(cx.listener(move |ws, _, _, cx| {
                            ws.ui.usage_app_pricing_sources.insert(key2.clone(), "request".to_string());
                            cx.notify();
                        })),
                )
        };

        let behavior_desc = if is_resp {
            i.t("usage.uses_upstream_response_model")
        } else {
            i.t("usage.uses_client_request_model")
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
                                .child(i.t("usage.global_pricing_defaults")),
                        )
                        .child(
                            div()
                                .text_size(px(11.5))
                                .text_color(t.text_muted)
                                .child(i.t("usage.configure_default_cost_multipl")),
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
        .child(div().flex_1().child(i.t("usage.model_name")))
        .child(div().w(px(120.0)).text_right().child(i.t("usage.input_1m")))
        .child(div().w(px(120.0)).text_right().child(i.t("usage.output_1m")))
        .child(div().w(px(120.0)).text_right().child(i.t("usage.cache_read_1m")))
        .child(div().w(px(120.0)).text_right().child(i.t("usage.cache_write_1m")));

    let tbody = if pricing.is_empty() {
        div()
            .py(px(40.0))
            .flex()
            .items_center()
            .justify_center()
            .text_size(px(12.5))
            .text_color(t.text_muted)
            .child(i.t("usage.no_pricing_models_registered"))
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
                        .child(format!("{}: {} 个已配置", i.t("usage.model_pricing_rules"), pricing.len())),
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
    let auto_sync = ws.settings.usage_auto_scan_sessions;

    let sync_btn = button_with_icon_loading_l(
        "manual-session-sync-btn",
        crate::icons::REFRESH_SVG,
        i.t("usage.sync_now"),
        ButtonVariant::Primary,
        is_syncing,
        &t,
        cx,
        |ws, _, _, cx| {
            ws.trigger_session_sync(cx, true);
        },
    );

    let auto_sync_toggle = toggle(
        "auto-session-sync-toggle",
        auto_sync,
        &t,
        cx,
        |ws, _, _, cx| {
            ws.settings.usage_auto_scan_sessions = !ws.settings.usage_auto_scan_sessions;
            ws.persist_settings();
            let msg = if ws.settings.usage_auto_scan_sessions {
                ws.trigger_session_sync(cx, true);
                ws.i18n.t("usage.auto_scan_session_history_2").to_string()
            } else {
                ws.i18n.t("usage.auto_scan_session_history").to_string()
            };
            ws.ui.toast(msg, false);
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
                                .child(i.t("usage.auto_scan_session_records")),
                        )
                        .child(
                            div()
                                .text_size(px(11.5))
                                .text_color(t.text_muted)
                                .child(i.t("usage.periodically_scan_local_sessio")),
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

#[cfg(test)]
mod tests {
    use super::*;
    use aitoolplus_core::usage::DailyStats;

    #[test]
    fn test_get_metric_stat_value() {
        let stat = DailyStats {
            date: "2026-10-10".into(),
            request_count: 42,
            total_cost: "1.234560".into(),
            total_tokens: 1000,
            total_input_tokens: 300,
            total_output_tokens: 200,
            total_cache_creation_tokens: 100,
            total_cache_read_tokens: 400,
        };

        assert_eq!(
            get_metric_stat_value(&stat, UsageChartMetric::Tokens),
            1000.0
        );
        assert_eq!(
            get_metric_stat_value(&stat, UsageChartMetric::Requests),
            42.0
        );
        assert_eq!(
            get_metric_stat_value(&stat, UsageChartMetric::Cost),
            1.23456
        );
    }

    #[test]
    fn test_format_y_tick_label() {
        assert_eq!(format_y_tick_label(0.0, UsageChartMetric::Tokens, true), "0");
        assert_eq!(format_y_tick_label(0.0, UsageChartMetric::Cost, true), "$0");
        assert_eq!(format_y_tick_label(50_000.0, UsageChartMetric::Tokens, true), "5万");
        assert_eq!(format_y_tick_label(50_000.0, UsageChartMetric::Tokens, false), "50.0K");
        assert_eq!(format_y_tick_label(20_000.0, UsageChartMetric::Requests, true), "2.0万");
        assert_eq!(format_y_tick_label(250.0, UsageChartMetric::Requests, false), "250");
        assert_eq!(format_y_tick_label(2.5, UsageChartMetric::Cost, true), "$2.5");
    }

    #[test]
    fn test_format_metric_value() {
        assert_eq!(
            format_metric_value(1234567.0, UsageChartMetric::Tokens, true),
            "1,234,567 Tokens"
        );
        assert_eq!(
            format_metric_value(42.0, UsageChartMetric::Requests, true),
            "42 次"
        );
        assert_eq!(
            format_metric_value(42.0, UsageChartMetric::Requests, false),
            "42 reqs"
        );
        assert_eq!(
            format_metric_value(0.1234, UsageChartMetric::Cost, true),
            "$0.1234"
        );
    }

    #[test]
    fn test_calculate_nice_ceiling() {
        assert_eq!(calculate_nice_ceiling(0.0), 4.0);
        assert_eq!(calculate_nice_ceiling(950.0), 1000.0);
        assert_eq!(calculate_nice_ceiling(15.0), 16.0);
        assert_eq!(calculate_nice_ceiling(185.0), 200.0);
    }
}
