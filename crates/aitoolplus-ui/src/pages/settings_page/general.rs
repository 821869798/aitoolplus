use gpui::{Context, IntoElement, div, prelude::*, px};

use crate::components::{
    ButtonVariant, button_l, button_with_icon_l, button_with_icon_loading_l, card, input_container, section_title,
    segmented_pill_selector, settings_card, settings_row, toggle,
};
use crate::text_input::TextInput;
use crate::workspace::Workspace;

use crate::pages::SettingsTab;

pub(super) fn general_tab(ws: &mut Workspace, cx: &mut Context<Workspace>) -> gpui::AnyElement {
    let t = ws.theme.clone();
    let i = ws.i18n;

    // 1. Appearance Card (Theme & Language)
    let theme_mode = ws.settings.theme_mode;
    let theme_selector = segmented_pill_selector(
        "theme-mode",
        vec![
            (
                aitoolplus_core::settings::ThemeMode::System,
                Some(crate::icons::SUN_MOON_SVG),
                i.t("settings_general.system"),
            ),
            (
                aitoolplus_core::settings::ThemeMode::Dark,
                Some(crate::icons::MOON_SVG),
                i.t("settings_general.dark"),
            ),
            (
                aitoolplus_core::settings::ThemeMode::Light,
                Some(crate::icons::SUN_SVG),
                i.t("settings_general.light"),
            ),
        ],
        theme_mode,
        &t,
        cx,
        |ws, mode, _, cx| {
            ws.settings.theme_mode = mode;
            ws.theme = crate::theme::Theme::for_mode(mode, system_prefers_dark());
            (ws.callbacks.save_settings)(&ws.settings);
            cx.notify();
        },
    );

    let lang_val = ws.settings.language;
    let lang_selector = segmented_pill_selector(
        "lang-choice",
        vec![
            (
                aitoolplus_core::settings::Language::System,
                Some(crate::icons::GLOBE_SVG),
                i.t("settings_general.system"),
            ),
            (
                aitoolplus_core::settings::Language::Zh,
                None,
                i.t("settings_general.zh_cn"),
            ),
            (
                aitoolplus_core::settings::Language::En,
                None,
                i.t("settings_general.english"),
            ),
        ],
        lang_val,
        &t,
        cx,
        |ws, lang, _, cx| {
            ws.settings.language = lang;
            ws.i18n = crate::i18n::I18n::new(lang);
            (ws.callbacks.save_settings)(&ws.settings);
            cx.refresh_windows();
            cx.notify();
        },
    );

    let appearance_card = settings_card(
        &t,
        i.t("settings_general.appearance_language"),
        Some(i.t("settings_general.choose_workbench_color_theme")),
        vec![
            settings_row(
                &t,
                i.t("settings_general.theme_mode"),
                Some(i.t("settings_general.dark_light_or_automatically")),
                theme_selector,
            ),
            settings_row(
                &t,
                i.t("settings_general.interface_language"),
                Some(i.t("settings_general.select_language_for_ui")),
                lang_selector,
            ),
        ],
    );

    // 2. System & Launch Behavior Card
    let autostart_on = ws.settings.start_with_system;
    let minimize_on_close = ws.settings.minimize_to_tray_on_close;
    let start_minimized = ws.settings.start_minimized;

    let behavior_card = settings_card(
        &t,
        i.t("settings_general.system_launch_behavior"),
        Some(i.t("settings_general.configure_startup_window_close")),
        vec![
            settings_row(
                &t,
                i.t("settings_general.launch_at_login"),
                Some(i.t("settings_general.start_ai_toolplus_in")),
                toggle(
                    "autostart-toggle",
                    autostart_on,
                    &t,
                    cx,
                    |ws, _, _, cx| {
                        ws.settings.start_with_system = !ws.settings.start_with_system;
                        (ws.callbacks.save_settings)(&ws.settings);
                        cx.notify();
                    },
                ),
            ),
            settings_row(
                &t,
                i.t("settings_general.minimize_to_tray_on"),
                Some(i.t("settings_general.keep_running_in_system")),
                toggle(
                    "minimize-on-close-toggle",
                    minimize_on_close,
                    &t,
                    cx,
                    |ws, _, _, cx| {
                        ws.settings.minimize_to_tray_on_close =
                            !ws.settings.minimize_to_tray_on_close;
                        (ws.callbacks.save_settings)(&ws.settings);
                        cx.notify();
                    },
                ),
            ),
            settings_row(
                &t,
                i.t("settings_general.start_minimized"),
                Some(i.t("settings_general.launch_directly_into_tray")),
                toggle(
                    "start-minimized-toggle",
                    start_minimized,
                    &t,
                    cx,
                    |ws, _, _, cx| {
                        ws.settings.start_minimized = !ws.settings.start_minimized;
                        (ws.callbacks.save_settings)(&ws.settings);
                        cx.notify();
                    },
                ),
            ),
            settings_row(
                &t,
                i.t("settings_general.auto_scan_local_session"),
                Some(i.t("settings_general.automatically_scan_claude_code")),
                toggle(
                    "usage-auto-scan-toggle-settings",
                    ws.settings.usage_auto_scan_sessions,
                    &t,
                    cx,
                    |ws, _, _, cx| {
                        ws.settings.usage_auto_scan_sessions = !ws.settings.usage_auto_scan_sessions;
                        (ws.callbacks.save_settings)(&ws.settings);
                        if ws.settings.usage_auto_scan_sessions {
                            ws.trigger_session_sync(cx, true);
                        }
                        cx.notify();
                    },
                ),
            ),
        ],
    );

    // 3. Network & Proxy Card
    let is_dropdown_open = ws.ui.proxy_protocol_dropdown_open;
    let current_proxy_type = ws.settings.proxy_type;
    let is_custom_proxy = current_proxy_type.is_custom();
    let is_testing_proxy = ws.ui.is_testing_proxy;
    let proxy_test_result = ws.ui.proxy_test_result.clone();
    let is_zh = ws.i18n.is_zh();

    let proxy_host_input = ws.ui.proxy_host_input.clone();
    let proxy_port_input = ws.ui.proxy_port_input.clone();

    let t_menu = t.clone();
    let protocol_options = aitoolplus_core::settings::ProxyType::all()
        .iter()
        .copied()
        .map(|pt| {
                        let is_selected = pt == current_proxy_type;
                        let t_opt = t_menu.clone();
                        div()
                            .id(gpui::SharedString::from(format!("proxy-opt-{:?}", pt)))
                            .flex()
                            .items_center()
                            .justify_between()
                            .px(px(8.0))
                            .py(px(6.0))
                            .rounded(px(4.0))
                            .bg(if is_selected { t_opt.accent_subtle } else { t_opt.card_bg })
                            .hover(move |s| s.bg(t_opt.card_hover))
                            .cursor_pointer()
                            .on_click(cx.listener(move |ws, _, _, cx| {
                                ws.settings.proxy_type = pt;
                                ws.ui.proxy_protocol_dropdown_open = false;
                                let host = ws.ui.proxy_host_input.read(cx).text().trim().to_string();
                                let port = ws.ui.proxy_port_input.read(cx).text().trim().to_string();
                                ws.settings.proxy_host = host;
                                ws.settings.proxy_port = port;
                                ws.settings.sync_proxy();
                                (ws.callbacks.save_settings)(&ws.settings);
                                aitoolplus_core::settings::apply_proxy_env(&ws.settings);
                                ws.ui.proxy_test_result = None;
                                cx.notify();
                            }))
                            .child(
                                div()
                                    .text_size(px(12.0))
                                    .font_weight(if is_selected {
                                        gpui::FontWeight::SEMIBOLD
                                    } else {
                                        gpui::FontWeight::NORMAL
                                    })
                                    .text_color(if is_selected { t_opt.accent } else { t_opt.text_primary })
                                    .child(pt.label(is_zh)),
                            )
                            .when(is_selected, |row| {
                                row.child(
                                    gpui::svg()
                                        .data(crate::icons::CHECK_SVG)
                                        .size(px(12.0))
                                        .text_color(t_opt.accent),
                                )
                            })
                    .into_any_element()
        }).collect::<Vec<_>>();
    let protocol_dropdown = div().w(px(150.0)).child(
        crate::components::MenuDrop::new("proxy-protocol-dropdown", is_dropdown_open, &t, cx)
            .menu_width(160.0)
            .trigger(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .w_full()
                    .h(px(32.0))
                    .px(px(10.0))
                    .rounded(px(6.0))
                    .bg(t.input_bg)
                    .border_1()
                    .border_color(if is_dropdown_open { t.accent } else { t.input_border })
                    .shadow_xs()
                    .hover(|s| s.border_color(t.card_border_hover))
                    .child(
                        div()
                            .text_size(px(12.5))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(t.text_primary)
                            .child(current_proxy_type.label(is_zh)),
                    )
                    .child(
                        gpui::svg()
                            .data(if is_dropdown_open {
                                crate::icons::CHEVRON_UP_SVG
                            } else {
                                crate::icons::CHEVRON_DOWN_SVG
                            })
                            .size(px(11.0))
                            .text_color(t.text_muted),
                    ),
            )
            .menu(div().flex().flex_col().gap(px(2.0)).children(protocol_options))
            .render(
                |ws, was_open, _cx| {
                    ws.ui.proxy_protocol_dropdown_open = !was_open;
                },
                |ws, _cx| {
                    ws.ui.proxy_protocol_dropdown_open = false;
                },
            ),
    );

    let mut columns_row = div()
        .flex()
        .items_start()
        .gap(px(12.0))
        .w_full();

    // Column 1: Proxy Protocol
    columns_row = columns_row.child(
        div()
            .flex()
            .flex_col()
            .w(px(150.0))
            .gap(px(6.0))
            .child(
                div()
                    .text_size(px(12.0))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .text_color(t.text_secondary)
                    .child(i.t("settings_general.proxy_protocol")),
            )
            .child(protocol_dropdown),
    );

    // Conditional columns 2 & 3: only when non-direct (custom proxy type)
    if is_custom_proxy {
        columns_row = columns_row
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_w(px(180.0))
                    .gap(px(6.0))
                    .child(
                        div()
                            .text_size(px(12.0))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(t.text_secondary)
                            .child(i.t("settings_general.proxy_server")),
                    )
                    .child(input_container(&t, proxy_host_input)),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .w(px(110.0))
                    .gap(px(6.0))
                    .child(
                        div()
                            .text_size(px(12.0))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(t.text_secondary)
                            .child(i.t("settings_general.port")),
                    )
                    .child(input_container(&t, proxy_port_input)),
            );
    } else {
        columns_row = columns_row.child(
            div()
                .flex()
                .flex_col()
                .flex_1()
                .justify_center()
                .h(px(32.0))
                .mt(px(22.0))
                .child(
                    div()
                        .text_size(px(12.0))
                        .text_color(t.text_secondary)
                        .child(if current_proxy_type == aitoolplus_core::settings::ProxyType::Direct {
                            i.t("settings_general.direct_mode_no_proxy")
                        } else {
                            i.t("settings_general.system_mode_follow_system")
                        }),
                ),
        );
    }

    // Test Proxy Button
    let test_button = button_l(
        "proxy-test-btn",
        if is_testing_proxy {
            i.t("settings_general.testing")
        } else if is_custom_proxy {
            i.t("settings_general.test_proxy")
        } else {
            i.t("settings_general.test_connectivity")
        },
        ButtonVariant::Secondary,
        &t,
        cx,
        move |ws, _, _, cx| {
            if ws.ui.is_testing_proxy {
                return;
            }
            let current_pt = ws.settings.proxy_type;
            let test_url = if current_pt.is_custom() {
                let host = ws.ui.proxy_host_input.read(cx).text().trim().to_string();
                let port = ws.ui.proxy_port_input.read(cx).text().trim().to_string();
                if host.is_empty() {
                    ws.ui.toast(
                        ws.i18n.t("settings_general.please_enter_proxy_server").to_string(),
                        true,
                    );
                    return;
                }
                if port.is_empty() {
                    ws.ui.toast(
                        ws.i18n.t("settings_general.please_enter_proxy_port").to_string(),
                        true,
                    );
                    return;
                }
                let scheme = current_pt.scheme();
                format!("{scheme}://{host}:{port}")
            } else if current_pt == aitoolplus_core::settings::ProxyType::System {
                aitoolplus_core::skills::resolve_proxy().unwrap_or_else(|| "direct".to_string())
            } else {
                "direct".to_string()
            };

            ws.ui.is_testing_proxy = true;
            ws.ui.proxy_test_result = None;
            cx.notify();

            let weak = cx.entity().downgrade();
            cx.spawn(async move |_this, cx| {
                let res = cx
                    .background_spawn(async move {
                        aitoolplus_core::settings::test_proxy_connectivity(&test_url)
                    })
                    .await;
                let _ = weak.update(cx, |ws, cx| {
                    ws.ui.is_testing_proxy = false;
                    match &res {
                        Ok(ms) => {
                            ws.ui.toast(
                                ws.i18n
                                    .raw(
                                        &format!("网络测试成功，响应延时: {} ms", ms),
                                        &format!("Connection test succeeded: {} ms", ms),
                                    )
                                    .to_string(),
                                false,
                            );
                        }
                        Err(err) => {
                            ws.ui.toast(
                                ws.i18n
                                    .raw(
                                        &format!("网络测试失败: {}", err),
                                        &format!("Connection test failed: {}", err),
                                    )
                                    .to_string(),
                                true,
                            );
                        }
                    }
                    ws.ui.proxy_test_result = Some(res);
                    cx.notify();
                });
            })
            .detach();
        },
    );

    // Save Settings Button
    let save_button = button_l(
        "proxy-save-btn",
        i.t("settings_general.save_settings"),
        ButtonVariant::Primary,
        &t,
        cx,
        move |ws, _, _, cx| {
            let host = ws.ui.proxy_host_input.read(cx).text().trim().to_string();
            let port = ws.ui.proxy_port_input.read(cx).text().trim().to_string();
            ws.settings.proxy_host = host;
            ws.settings.proxy_port = port;
            ws.settings.sync_proxy();
            (ws.callbacks.save_settings)(&ws.settings);
            aitoolplus_core::settings::apply_proxy_env(&ws.settings);
            ws.ui.toast(
                ws.i18n
                    .t("settings_general.proxy_settings_saved_and")
                    .to_string(),
                false,
            );
            cx.notify();
        },
    );

    let result_badge = if is_testing_proxy {
        div()
            .flex()
            .items_center()
            .gap(px(6.0))
            .px(px(10.0))
            .py(px(4.0))
            .rounded(px(6.0))
            .bg(t.card_bg)
            .border_1()
            .border_color(t.card_border)
            .child(
                div()
                    .text_size(px(11.5))
                    .text_color(t.text_secondary)
                    .child(i.t("settings_general.testing_connection_latency")),
            )
            .into_any_element()
    } else if let Some(ref res) = proxy_test_result {
        match res {
            Ok(ms) => div()
                .flex()
                .items_center()
                .gap(px(6.0))
                .px(px(10.0))
                .py(px(4.0))
                .rounded(px(6.0))
                .bg(t.success_subtle)
                .border_1()
                .border_color(t.success.opacity(0.3))
                .child(
                    gpui::svg()
                        .data(crate::icons::CHECK_SVG)
                        .size(px(12.0))
                        .text_color(t.success),
                )
                .child(
                    div()
                        .text_size(px(11.5))
                        .font_weight(gpui::FontWeight::MEDIUM)
                        .text_color(t.success)
                        .child(format!("连接正常 (延时 {} ms)", ms)),
                )
                .into_any_element(),
            Err(err) => div()
                .flex()
                .items_center()
                .gap(px(6.0))
                .px(px(10.0))
                .py(px(4.0))
                .rounded(px(6.0))
                .bg(t.danger_subtle)
                .border_1()
                .border_color(t.danger.opacity(0.3))
                .child(
                    div()
                        .text_size(px(11.5))
                        .font_weight(gpui::FontWeight::MEDIUM)
                        .text_color(t.danger)
                        .child(format!("连接失败: {}", err)),
                )
                .into_any_element(),
        }
    } else {
        div().into_any_element()
    };

    let proxy_card_content = div()
        .flex()
        .flex_col()
        .w_full()
        .p(px(16.0))
        .gap(px(14.0))
        .child(columns_row)
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .w_full()
                .pt(px(2.0))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(10.0))
                        .child(test_button)
                        .child(save_button),
                )
                .child(result_badge),
        )
        .into_any_element();

    let network_card = settings_card(
        &t,
        i.t("settings_general.network_proxy"),
        Some(i.t("settings_general.configure_outbound_proxy_url")),
        vec![proxy_card_content],
    );

    // 4. Visible Tools Card (Sidebar Tool Chips)
    let mut visibility_chips = div().flex().flex_wrap().gap(px(8.0)).p(px(16.0));
    for tool in aitoolplus_core::ToolId::ALL {
        let visible = ws.settings.visible_tools.is_empty()
            || ws
                .settings
                .visible_tools
                .iter()
                .any(|key| key == tool.key());
        let chip_theme = t.clone();
        let chip = div()
            .id(gpui::ElementId::Name(format!("visible-tool-{}", tool.key()).into()))
            .cursor_pointer()
            .h(px(32.0))
            .px(px(14.0))
            .rounded(px(6.0))
            .flex()
            .items_center()
            .gap(px(6.0))
            .text_size(px(12.5))
            .when(visible, |s| {
                s.bg(chip_theme.tab_active_bg)
                    .border_1()
                    .border_color(chip_theme.accent)
                    .text_color(chip_theme.text_primary)
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .child(
                        gpui::svg()
                            .data(crate::icons::CHECK_SVG)
                            .size(px(13.0))
                            .text_color(chip_theme.accent),
                    )
            })
            .when(!visible, |s| {
                s.bg(chip_theme.input_bg)
                    .border_1()
                    .border_color(chip_theme.card_border)
                    .text_color(chip_theme.text_muted)
                    .hover(move |h| {
                        h.border_color(chip_theme.card_border_hover)
                            .text_color(chip_theme.text_secondary)
                    })
            })
            .child(tool.name_en())
            .on_click(cx.listener(move |ws, _, _, cx| {
                if ws.settings.visible_tools.is_empty() {
                    let mut all: Vec<String> = aitoolplus_core::ToolId::ALL
                        .into_iter()
                        .map(|tool| tool.key().to_string())
                        .collect();
                    all.push("antigravity".to_string());
                    ws.settings.visible_tools = all;
                }
                if ws.settings.visible_tools.iter().any(|key| key == tool.key()) {
                    ws.settings.visible_tools.retain(|key| key != tool.key());
                } else {
                    ws.settings.visible_tools.push(tool.key().to_string());
                }
                (ws.callbacks.save_settings)(&ws.settings);
                cx.notify();
            }));
        visibility_chips = visibility_chips.child(chip);
    }

    // Antigravity visibility chip
    let ag_visible = ws.settings.visible_tools.is_empty()
        || ws.settings.visible_tools.iter().any(|key| key == "antigravity");
    let chip_theme = t.clone();
    let ag_chip = div()
        .id("tool-visibility-antigravity")
        .cursor_pointer()
        .px(px(12.0))
        .py(px(6.0))
        .rounded(px(6.0))
        .flex()
        .items_center()
        .gap(px(6.0))
        .text_size(px(12.5))
        .when(ag_visible, |s| {
            s.bg(chip_theme.tab_active_bg)
                .border_1()
                .border_color(chip_theme.accent)
                .text_color(chip_theme.text_primary)
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .child(
                    gpui::svg()
                        .data(crate::icons::CHECK_SVG)
                        .size(px(13.0))
                        .text_color(chip_theme.accent),
                )
        })
        .when(!ag_visible, |s| {
            s.bg(chip_theme.input_bg)
                .border_1()
                .border_color(chip_theme.card_border)
                .text_color(chip_theme.text_muted)
                .hover(move |h| {
                    h.border_color(chip_theme.card_border_hover)
                        .text_color(chip_theme.text_secondary)
                })
        })
        .child("Antigravity")
        .on_click(cx.listener(move |ws, _, _, cx| {
            if ws.settings.visible_tools.is_empty() {
                let mut all: Vec<String> = aitoolplus_core::ToolId::ALL
                    .into_iter()
                    .map(|tool| tool.key().to_string())
                    .collect();
                all.push("antigravity".to_string());
                ws.settings.visible_tools = all;
            }
            if ws.settings.visible_tools.iter().any(|key| key == "antigravity") {
                ws.settings.visible_tools.retain(|key| key != "antigravity");
            } else {
                ws.settings.visible_tools.push("antigravity".to_string());
            }
            (ws.callbacks.save_settings)(&ws.settings);
            cx.notify();
        }));
    visibility_chips = visibility_chips.child(ag_chip);

    let visibility_card = settings_card(
        &t,
        i.t("settings_general.visible_sidebar_tools"),
        Some(i.t("settings_general.click_tool_chips_to")),
        vec![visibility_chips.into_any_element()],
    );

    // Combine all modular cards with ample vertical spacing
    div()
        .flex()
        .flex_col()
        .w_full()
        .gap(px(20.0))
        .child(appearance_card)
        .child(behavior_card)
        .child(network_card)
        .child(visibility_card)
        .into_any_element()
}

pub(super) fn system_prefers_dark() -> bool {
    crate::workspace::system_prefers_dark_pub()
}
