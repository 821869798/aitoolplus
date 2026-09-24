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
                i.t("跟随系统", "System"),
            ),
            (
                aitoolplus_core::settings::ThemeMode::Dark,
                Some(crate::icons::MOON_SVG),
                i.t("暗色模式", "Dark"),
            ),
            (
                aitoolplus_core::settings::ThemeMode::Light,
                Some(crate::icons::SUN_SVG),
                i.t("亮色模式", "Light"),
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
                i.t("跟随系统", "System"),
            ),
            (
                aitoolplus_core::settings::Language::Zh,
                None,
                i.t("简体中文", "中文"),
            ),
            (
                aitoolplus_core::settings::Language::En,
                None,
                i.t("English", "EN"),
            ),
        ],
        lang_val,
        &t,
        cx,
        |ws, lang, _, cx| {
            ws.settings.language = lang;
            ws.i18n = crate::i18n::I18n::new(lang);
            (ws.callbacks.save_settings)(&ws.settings);
            cx.notify();
        },
    );

    let appearance_card = settings_card(
        &t,
        i.t("外观与语言", "Appearance & Language"),
        Some(i.t(
            "选择工作台的色彩主题风格与界面文本显示语言",
            "Choose workbench color theme and interface language",
        )),
        vec![
            settings_row(
                &t,
                i.t("界面主题", "Theme Mode"),
                Some(i.t(
                    "暗色、亮色或自动同步操作系统的深浅色偏好",
                    "Dark, light or automatically sync with OS preference",
                )),
                theme_selector,
            ),
            settings_row(
                &t,
                i.t("界面语言", "Interface Language"),
                Some(i.t(
                    "切换应用内所有界面文案、提示与状态标签的语言",
                    "Select language for UI labels, notifications and buttons",
                )),
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
        i.t("系统与启动行为", "System & Launch Behavior"),
        Some(i.t(
            "配置开机自启、关闭窗口策略与托盘驻留行为",
            "Configure startup, window close, and background tray actions",
        )),
        vec![
            settings_row(
                &t,
                i.t("开机自动启动", "Launch at Login"),
                Some(i.t(
                    "登录 Windows 后在后台自动启动 AI ToolPlus 守护服务",
                    "Start AI ToolPlus in background after Windows user login",
                )),
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
                i.t("关闭时最小化到托盘", "Minimize to Tray on Close"),
                Some(i.t(
                    "点击主窗口关闭按钮时保留后台托盘运行，避免中断会话",
                    "Keep running in system tray when window is closed",
                )),
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
                i.t("启动时直接最小化", "Start Minimized"),
                Some(i.t(
                    "应用启动后静默进入系统托盘，不主动弹出主窗口",
                    "Launch directly into tray without showing main window",
                )),
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
                i.t("自动扫描本地会话用量", "Auto-Scan Local Session Usage"),
                Some(i.t(
                    "自动追踪并扫描 Claude Code、Codex、Pi 等本地会话记录与 Token 消耗",
                    "Automatically scan Claude Code, Codex, Pi session files for token analytics",
                )),
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

    let protocol_dropdown = div()
        .relative()
        .w(px(150.0))
        .child(
            div()
                .id("proxy-protocol-dropdown-trigger")
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
                .cursor_pointer()
                .hover(|s| s.border_color(t.card_border_hover))
                .on_mouse_down(gpui::MouseButton::Left, cx.listener(move |ws, _, _, cx| {
                    cx.stop_propagation();
                    ws.ui.proxy_protocol_dropdown_open = !is_dropdown_open;
                    cx.notify();
                }))
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
        .when(is_dropdown_open, |el| {
            let t_menu = t.clone();
            el.child(gpui::deferred(
                div()
                    .id("proxy-protocol-dropdown-menu")
                    .occlude()
                    .absolute()
                    .top(px(36.0))
                    .left_0()
                    .w(px(160.0))
                    .bg(t_menu.card_bg)
                    .border_1()
                    .border_color(t_menu.card_border)
                    .rounded(px(6.0))
                    .shadow_xl()
                    .p(px(4.0))
                    .flex()
                    .flex_col()
                    .gap(px(2.0))
                    .on_mouse_down_out({
                        let entity = cx.entity().clone();
                        move |_ev, _window, cx| {
                            entity.update(cx, |ws, cx| {
                                ws.ui.proxy_protocol_dropdown_open = false;
                                cx.notify();
                            });
                        }
                    })
                    .children(aitoolplus_core::settings::ProxyType::all().iter().copied().map(|pt| {
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
                    })),
            ))
        });

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
                    .child(i.t("代理协议", "Proxy Protocol")),
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
                            .child(i.t("代理服务器", "Proxy Server")),
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
                            .child(i.t("代理端口", "Port")),
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
                            i.t(
                                "直连模式：不使用任何网络代理，直接连接上游模型与更新服务器",
                                "Direct mode: no proxy, connects directly to target servers",
                            )
                        } else {
                            i.t(
                                "跟随系统代理：自动检测并使用当前操作系统配置的系统代理",
                                "System mode: follow system proxy settings configured in OS",
                            )
                        }),
                ),
        );
    }

    // Test Proxy Button
    let test_button = button_l(
        "proxy-test-btn",
        if is_testing_proxy {
            i.t("正在测试…", "Testing…")
        } else if is_custom_proxy {
            i.t("测试代理", "Test Proxy")
        } else {
            i.t("测试网络连接", "Test Connectivity")
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
                        ws.i18n.t("请输入代理服务器地址", "Please enter proxy server").to_string(),
                        true,
                    );
                    return;
                }
                if port.is_empty() {
                    ws.ui.toast(
                        ws.i18n.t("请输入代理端口", "Please enter proxy port").to_string(),
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
                                    .t(
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
                                    .t(
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
        i.t("保存设置", "Save Settings"),
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
                    .t("代理网络设置已保存并立即生效", "Proxy settings saved and applied")
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
                    .child(i.t("正在检测连接延时…", "Testing connection latency…")),
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
        i.t("网络与代理", "Network & Proxy"),
        Some(i.t(
            "配置工作台发起网络请求时使用的代理方式与出口地址",
            "Configure outbound proxy URL and connection behavior",
        )),
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
        i.t("侧边栏可见工具", "Visible Sidebar Tools"),
        Some(i.t(
            "点击切换工具卡片，定制侧边栏中常驻显示的 AI 编码助手",
            "Click tool chips to customize which AI coding tools appear in navigation",
        )),
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
