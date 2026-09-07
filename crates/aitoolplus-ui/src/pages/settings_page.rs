//! Settings page: general, backup, about.

use gpui::{Context, IntoElement, div, prelude::*, px};

use crate::components::{
    ButtonVariant, button_l, card, input_container, page_header, section_title, toggle,
};
use crate::workspace::Workspace;

use super::SettingsTab;

pub fn render_settings_page(ws: &mut Workspace, cx: &mut Context<Workspace>) -> gpui::AnyElement {
    let t = ws.theme.clone();
    let i = ws.i18n;

    let tabs = [
        (SettingsTab::General, i.t("通用", "General")),
        (SettingsTab::Backup, i.t("备份", "Backup")),
        (SettingsTab::About, i.t("关于", "About")),
    ];

    let mut tab_bar = div()
        .flex()
        .items_center()
        .gap(px(4.0))
        .p(px(3.0))
        .rounded(px(8.0))
        .bg(t.sidebar_bg)
        .border_1()
        .border_color(t.card_border);
    for (tab, label) in tabs {
        let is_on = ws.ui.settings_tab == tab;
        tab_bar = tab_bar.child(
            div()
                .id(gpui::SharedString::from(format!("settings-tab-{:?}", tab)))
                .cursor_pointer()
                .px(px(12.0))
                .py(px(4.5))
                .rounded(px(6.0))
                .text_size(px(12.5))
                .when(is_on, |s| {
                    s.bg(t.card_bg)
                        .text_color(t.text_primary)
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .shadow_xs()
                })
                .when(!is_on, |s| {
                    s.text_color(t.text_secondary)
                        .hover(|h| h.text_color(t.text_primary).bg(t.card_hover))
                })
                .on_click(cx.listener(move |ws, _ev: &gpui::ClickEvent, _w, cx| {
                    ws.ui.settings_tab = tab;
                    cx.notify();
                }))
                .child(label),
        );
    }

    let body = match ws.ui.settings_tab {
        SettingsTab::General => general_tab(ws, cx),
        SettingsTab::Backup => backup_tab(ws, cx),
        SettingsTab::About => about_tab(ws, cx),
    };

    div()
        .flex()
        .flex_col()
        .w_full()
        .max_w(px(760.0))
        .min_w(px(0.0))
        .gap(px(16.0))
        .child(page_header(
            &t,
            i.t("设置", "Settings"),
            i.t("应用偏好、数据备份与关于", "Preferences, backups & about"),
        ))
        .child(tab_bar)
        .child(body)
        .into_any_element()
}

fn general_tab(ws: &mut Workspace, cx: &mut Context<Workspace>) -> gpui::AnyElement {
    let t = ws.theme.clone();
    let i = ws.i18n;

    // language picker
    let mut lang_row = div().flex().gap(px(6.0));
    for (lang, label) in [
        (
            aitoolplus_core::settings::Language::System,
            i.t("跟随系统", "System"),
        ),
        (aitoolplus_core::settings::Language::Zh, i.t("中文", "中文")),
        (
            aitoolplus_core::settings::Language::En,
            i.t("英文", "English"),
        ),
    ] {
        let is_on = ws.settings.language == lang;
        lang_row = lang_row.child(
            div()
                .id(gpui::SharedString::from(format!("lang-{}", lang.as_u8())))
                .cursor_pointer()
                .flex()
                .items_center()
                .h(px(26.0))
                .px(px(10.0))
                .rounded(px(6.0))
                .text_size(px(12.5))
                .when(is_on, |s| {
                    s.bg(t.accent_subtle)
                        .border_1()
                        .border_color(t.accent)
                        .text_color(t.accent)
                })
                .when(!is_on, |s| {
                    s.bg(t.input_bg)
                        .border_1()
                        .border_color(t.input_border)
                        .text_color(t.text_secondary)
                })
                .on_click(cx.listener(move |ws, _ev: &gpui::ClickEvent, _w, cx| {
                    ws.settings.language = lang;
                    ws.i18n = crate::i18n::I18n::new(lang);
                    (ws.callbacks.save_settings)(&ws.settings);
                    cx.notify();
                }))
                .child(label),
        );
    }

    // theme picker
    let mut theme_row = div().flex().gap(px(6.0));
    for (mode, label) in [
        (
            aitoolplus_core::settings::ThemeMode::System,
            i.t("跟随系统", "System"),
        ),
        (
            aitoolplus_core::settings::ThemeMode::Dark,
            i.t("暗色", "Dark"),
        ),
        (
            aitoolplus_core::settings::ThemeMode::Light,
            i.t("亮色", "Light"),
        ),
    ] {
        let is_on = ws.settings.theme_mode == mode;
        theme_row = theme_row.child(
            div()
                .id(gpui::SharedString::from(format!("theme-{}", mode.as_u8())))
                .cursor_pointer()
                .flex()
                .items_center()
                .h(px(26.0))
                .px(px(10.0))
                .rounded(px(6.0))
                .text_size(px(12.5))
                .when(is_on, |s| {
                    s.bg(t.accent_subtle)
                        .border_1()
                        .border_color(t.accent)
                        .text_color(t.accent)
                })
                .when(!is_on, |s| {
                    s.bg(t.input_bg)
                        .border_1()
                        .border_color(t.input_border)
                        .text_color(t.text_secondary)
                })
                .on_click(cx.listener(move |ws, _ev: &gpui::ClickEvent, _w, cx| {
                    ws.settings.theme_mode = mode;
                    ws.theme = crate::theme::Theme::for_mode(mode, system_prefers_dark());
                    (ws.callbacks.save_settings)(&ws.settings);
                    cx.notify();
                }))
                .child(label),
        );
    }

    // lifecycle
    let autostart_on = ws.settings.start_with_system;
    let autostart_row = div()
        .flex()
        .items_center()
        .justify_between()
        .w_full()
        .gap(px(16.0))
        .child(section_title(
            &t,
            i.t("开机自启", "Launch at Login"),
            Some(i.t(
                "登录 Windows 后自动启动 AI ToolPlus",
                "Start AI ToolPlus after Windows sign-in",
            )),
        ))
        .child(toggle(
            "autostart-toggle",
            autostart_on,
            &t,
            cx,
            |ws, _, _, cx| {
                ws.settings.start_with_system = !ws.settings.start_with_system;
                (ws.callbacks.save_settings)(&ws.settings);
                cx.notify();
            },
        ));

    let minimize_on_close = ws.settings.minimize_to_tray_on_close;
    let minimize_row = div()
        .flex()
        .items_center()
        .justify_between()
        .w_full()
        .gap(px(16.0))
        .child(section_title(
            &t,
            i.t("关闭时最小化到托盘", "Minimize to Tray on Close"),
            Some(i.t(
                "点击关闭按钮时保留后台托盘，可从托盘重新打开",
                "Keep the tray running when the close button is clicked",
            )),
        ))
        .child(toggle(
            "minimize-on-close-toggle",
            minimize_on_close,
            &t,
            cx,
            |ws, _, _, cx| {
                ws.settings.minimize_to_tray_on_close = !ws.settings.minimize_to_tray_on_close;
                (ws.callbacks.save_settings)(&ws.settings);
                cx.notify();
            },
        ));

    let start_minimized = ws.settings.start_minimized;
    let start_minimized_row = div()
        .flex()
        .items_center()
        .justify_between()
        .w_full()
        .gap(px(16.0))
        .child(section_title(
            &t,
            i.t("启动时最小化", "Start Minimized"),
            Some(i.t("应用启动后直接进入托盘", "Start directly in the tray")),
        ))
        .child(toggle(
            "start-minimized-toggle",
            start_minimized,
            &t,
            cx,
            |ws, _, _, cx| {
                ws.settings.start_minimized = !ws.settings.start_minimized;
                (ws.callbacks.save_settings)(&ws.settings);
                cx.notify();
            },
        ));

    let proxy_mode = ws.settings.proxy_mode;
    let proxy_input = ws.ui.proxy_url_input.clone();
    let proxy_save = proxy_input.clone();
    let proxy_panel = div()
        .flex()
        .flex_col()
        .items_start()
        .gap(px(8.0))
        .child(section_title(
            &t,
            i.t("网络代理", "Network Proxy"),
            Some(i.t(
                "用于更新、模型、WebDAV 和包版本请求",
                "Used by update, models, WebDAV and package requests",
            )),
        ))
        .child(
            div()
                .flex()
                .gap(px(8.0))
                .child(button_l(
                    "proxy-system",
                    i.t("系统", "System"),
                    if proxy_mode == aitoolplus_core::settings::ProxyMode::System {
                        ButtonVariant::Primary
                    } else {
                        ButtonVariant::Secondary
                    },
                    &t,
                    cx,
                    |ws, _, _, cx| {
                        ws.settings.proxy_mode = aitoolplus_core::settings::ProxyMode::System;
                        (ws.callbacks.save_settings)(&ws.settings);
                        cx.notify();
                    },
                ))
                .child(button_l(
                    "proxy-direct",
                    i.t("直连", "Direct"),
                    if proxy_mode == aitoolplus_core::settings::ProxyMode::Direct {
                        ButtonVariant::Primary
                    } else {
                        ButtonVariant::Secondary
                    },
                    &t,
                    cx,
                    |ws, _, _, cx| {
                        ws.settings.proxy_mode = aitoolplus_core::settings::ProxyMode::Direct;
                        (ws.callbacks.save_settings)(&ws.settings);
                        cx.notify();
                    },
                ))
                .child(button_l(
                    "proxy-custom",
                    i.t("自定义", "Custom"),
                    if proxy_mode == aitoolplus_core::settings::ProxyMode::Custom {
                        ButtonVariant::Primary
                    } else {
                        ButtonVariant::Secondary
                    },
                    &t,
                    cx,
                    |ws, _, _, cx| {
                        ws.settings.proxy_mode = aitoolplus_core::settings::ProxyMode::Custom;
                        (ws.callbacks.save_settings)(&ws.settings);
                        cx.notify();
                    },
                )),
        )
        .child(input_container(&t, proxy_input))
        .child(button_l(
            "proxy-save",
            i.t("保存代理 URL", "Save Proxy URL"),
            ButtonVariant::Secondary,
            &t,
            cx,
            move |ws, _, _, cx| {
                ws.settings.proxy_url =
                    proxy_save.update(cx, |input, _| input.text().trim().to_string());
                (ws.callbacks.save_settings)(&ws.settings);
                ws.ui.toast(
                    ws.i18n
                        .t("已保存；重启后生效", "saved; applies after restart")
                        .to_string(),
                    false,
                );
                cx.notify();
            },
        ));

    let cli_policy_panel = div()
        .flex()
        .flex_col()
        .items_start()
        .gap(px(8.0))
        .child(section_title(
            &t,
            i.t("CLI 启动与认证策略", "CLI Launch & Auth Policies"),
            None,
        ))
        .child(button_l(
            "claude-full-access",
            if ws.settings.claude_cli_launch_full_access {
                i.t("Claude 全权限启动：开", "Claude full-access launch: on")
            } else {
                i.t("Claude 全权限启动：关", "Claude full-access launch: off")
            },
            if ws.settings.claude_cli_launch_full_access {
                ButtonVariant::Primary
            } else {
                ButtonVariant::Secondary
            },
            &t,
            cx,
            |ws, _, _, cx| {
                ws.settings.claude_cli_launch_full_access =
                    !ws.settings.claude_cli_launch_full_access;
                (ws.callbacks.save_settings)(&ws.settings);
                cx.notify();
            },
        ))
        .child(button_l(
            "codex-preserve-auth",
            if ws.settings.codex_preserve_official_auth_on_switch {
                i.t("Codex 保留官方认证：开", "Preserve Codex official auth: on")
            } else {
                i.t(
                    "Codex 保留官方认证：关",
                    "Preserve Codex official auth: off",
                )
            },
            if ws.settings.codex_preserve_official_auth_on_switch {
                ButtonVariant::Primary
            } else {
                ButtonVariant::Secondary
            },
            &t,
            cx,
            |ws, _, _, cx| {
                ws.settings.codex_preserve_official_auth_on_switch =
                    !ws.settings.codex_preserve_official_auth_on_switch;
                (ws.callbacks.save_settings)(&ws.settings);
                cx.notify();
            },
        ))
        .child(button_l(
            "omo-legacy-config",
            if ws.settings.opencode_use_legacy_oh_my_config {
                i.t("OpenAgent Legacy 文件：开", "OpenAgent legacy file: on")
            } else {
                i.t(
                    "OpenAgent 统一 ~/.omo 配置",
                    "OpenAgent unified ~/.omo config",
                )
            },
            if ws.settings.opencode_use_legacy_oh_my_config {
                ButtonVariant::Primary
            } else {
                ButtonVariant::Secondary
            },
            &t,
            cx,
            |ws, _, _, cx| {
                ws.settings.opencode_use_legacy_oh_my_config =
                    !ws.settings.opencode_use_legacy_oh_my_config;
                (ws.callbacks.save_settings)(&ws.settings);
                cx.notify();
            },
        ))
        .child(button_l(
            "omo-clear-policy",
            if ws.settings.opencode_allow_clear_applied_oh_my_config {
                i.t("允许清除 OMO/OMOS：开", "Allow clear OMO/OMOS: on")
            } else {
                i.t("允许清除 OMO/OMOS：关", "Allow clear OMO/OMOS: off")
            },
            if ws.settings.opencode_allow_clear_applied_oh_my_config {
                ButtonVariant::Primary
            } else {
                ButtonVariant::Secondary
            },
            &t,
            cx,
            |ws, _, _, cx| {
                ws.settings.opencode_allow_clear_applied_oh_my_config =
                    !ws.settings.opencode_allow_clear_applied_oh_my_config;
                (ws.callbacks.save_settings)(&ws.settings);
                cx.notify();
            },
        ))
        .child(button_l(
            "omo-dual-reasoning",
            if ws.settings.opencode_dual_write_reasoning_variant {
                i.t("双写 reasoning/variant：开", "Dual reasoning/variant: on")
            } else {
                i.t("双写 reasoning/variant：关", "Dual reasoning/variant: off")
            },
            if ws.settings.opencode_dual_write_reasoning_variant {
                ButtonVariant::Primary
            } else {
                ButtonVariant::Secondary
            },
            &t,
            cx,
            |ws, _, _, cx| {
                ws.settings.opencode_dual_write_reasoning_variant =
                    !ws.settings.opencode_dual_write_reasoning_variant;
                (ws.callbacks.save_settings)(&ws.settings);
                cx.notify();
            },
        ));

    let mut visibility_panel = div().flex().flex_col().gap(px(8.0)).child(section_title(
        &t,
        i.t("可见工具", "Visible Tools"),
        Some(i.t("隐藏不使用的工具标签", "Hide tools you do not use")),
    ));
    let mut visibility_buttons = div().flex().flex_wrap().gap(px(6.0));
    for tool in aitoolplus_core::ToolId::ALL {
        let visible = ws.settings.visible_tools.is_empty()
            || ws
                .settings
                .visible_tools
                .iter()
                .any(|key| key == tool.key());
        visibility_buttons = visibility_buttons.child(button_l(
            gpui::SharedString::from(format!("visible-tool-{}", tool.key())),
            tool.name_en(),
            if visible {
                ButtonVariant::Primary
            } else {
                ButtonVariant::Secondary
            },
            &t,
            cx,
            move |ws, _, _, cx| {
                if ws.settings.visible_tools.is_empty() {
                    ws.settings.visible_tools = aitoolplus_core::ToolId::ALL
                        .into_iter()
                        .map(|tool| tool.key().to_string())
                        .collect();
                }
                if ws
                    .settings
                    .visible_tools
                    .iter()
                    .any(|key| key == tool.key())
                {
                    ws.settings.visible_tools.retain(|key| key != tool.key());
                } else {
                    ws.settings.visible_tools.push(tool.key().to_string());
                }
                (ws.callbacks.save_settings)(&ws.settings);
                cx.notify();
            },
        ));
    }
    visibility_panel = visibility_panel.child(visibility_buttons);

    // data dir row
    let data_dir = ws.paths.app_data.display().to_string();

    let mut roots_panel = div().flex().flex_col().gap(px(8.0)).child(section_title(
        &t,
        i.t("CLI 配置根目录", "CLI Config Roots"),
        Some(i.t(
            "空值使用自动探测；自定义路径保存后下次启动生效",
            "Blank uses auto-detection; custom paths apply on next launch",
        )),
    ));
    for tool in aitoolplus_core::ToolId::ALL {
        let override_value = ws
            .settings
            .tool_root_overrides
            .get(tool.key())
            .cloned()
            .unwrap_or_default();
        let input = ws.ui.tool_root_input(tool, &override_value, cx);
        let save_input = input.clone();
        let command = match tool {
            aitoolplus_core::ToolId::ClaudeCode => "claude",
            aitoolplus_core::ToolId::GeminiCli => "gemini",
            aitoolplus_core::ToolId::OhMyPi => "omp",
            other => other.key(),
        };
        let cli_value = ws
            .settings
            .cli_manual_paths
            .get(command)
            .cloned()
            .unwrap_or_default();
        let cli_input = ws.ui.cli_path_input(command, &cli_value, cx);
        let save_cli_input = cli_input.clone();
        let resolved = ws.paths.tool_root(tool).display().to_string();
        roots_panel = roots_panel.child(
            div()
                .flex()
                .flex_col()
                .items_start()
                .gap(px(6.0))
                .p(px(8.0))
                .rounded(px(8.0))
                .bg(t.input_bg)
                .border_1()
                .border_color(t.card_border)
                .child(
                    div()
                        .text_size(px(12.0))
                        .font_weight(gpui::FontWeight::MEDIUM)
                        .text_color(t.text_primary)
                        .child(format!("{} · {}", tool.name_en(), resolved)),
                )
                .child(input)
                .child(button_l(
                    gpui::SharedString::from(format!("save-root-{}", tool.key())),
                    i.t("保存路径覆盖", "Save Root Override"),
                    ButtonVariant::Secondary,
                    &t,
                    cx,
                    move |ws, _, _, cx| {
                        let value =
                            save_input.update(cx, |input, _| input.text().trim().to_string());
                        if value.is_empty() {
                            ws.settings.tool_root_overrides.remove(tool.key());
                        } else {
                            ws.settings
                                .tool_root_overrides
                                .insert(tool.key().to_string(), value);
                        }
                        (ws.callbacks.save_settings)(&ws.settings);
                        ws.ui.toast(
                            ws.i18n
                                .t("已保存；重启后生效", "saved; applies after restart")
                                .to_string(),
                            false,
                        );
                        cx.notify();
                    },
                ))
                .child(
                    div()
                        .text_size(px(11.0))
                        .text_color(t.text_muted)
                        .child(format!("CLI: {command}")),
                )
                .child(cli_input)
                .child(button_l(
                    gpui::SharedString::from(format!("save-cli-{command}")),
                    i.t("保存 CLI 路径", "Save CLI Path"),
                    ButtonVariant::Secondary,
                    &t,
                    cx,
                    move |ws, _, _, cx| {
                        let value =
                            save_cli_input.update(cx, |input, _| input.text().trim().to_string());
                        if value.is_empty() {
                            ws.settings.cli_manual_paths.remove(command);
                        } else {
                            ws.settings.cli_manual_paths.insert(command.into(), value);
                        }
                        (ws.callbacks.save_settings)(&ws.settings);
                        ws.ui.toast(
                            ws.i18n
                                .t("已保存；重启后生效", "saved; applies after restart")
                                .to_string(),
                            false,
                        );
                        cx.notify();
                    },
                )),
        );
    }

    card(
        &t,
        vec![
            section_title(
                &t,
                i.t("语言 / Language", "Language"),
                Some(i.t("界面显示语言", "Interface language")),
            ),
            lang_row.into_any_element(),
            section_title(
                &t,
                i.t("主题", "Theme"),
                Some(i.t("亮色或暗色工作台", "Light or dark workbench")),
            ),
            theme_row.into_any_element(),
            autostart_row.into_any_element(),
            minimize_row.into_any_element(),
            start_minimized_row.into_any_element(),
            proxy_panel.into_any_element(),
            cli_policy_panel.into_any_element(),
            visibility_panel.into_any_element(),
            section_title(
                &t,
                i.t("数据目录", "Data Directory"),
                Some(gpui::SharedString::from(data_dir.clone())),
            ),
            button_l(
                "open-data-dir",
                i.t("打开数据目录", "Open Data Directory"),
                ButtonVariant::Secondary,
                &t,
                cx,
                move |ws, _, _, cx| {
                    let _ = open_dir_in_explorer(&ws.paths.app_data);
                    cx.notify();
                },
            ),
            roots_panel.into_any_element(),
        ],
    )
    .into_any_element()
}

fn backup_tab(ws: &mut Workspace, cx: &mut Context<Workspace>) -> gpui::AnyElement {
    let t = ws.theme.clone();
    let i = ws.i18n;
    let backup_type = ws.settings.backup_type;
    let webdav_config = ws.settings.webdav.clone();
    let webdav_inputs = ws.ui.webdav_inputs(&webdav_config, cx);
    let dav_url = webdav_inputs.url.clone();
    let dav_user = webdav_inputs.username.clone();
    let dav_password = webdav_inputs.password.clone();
    let dav_directory = webdav_inputs.remote_directory.clone();
    let s3_config = ws.settings.s3.clone();
    let s3_inputs = ws.ui.s3_inputs(&s3_config, cx);
    let s3_endpoint = s3_inputs.endpoint.clone();
    let s3_region = s3_inputs.region.clone();
    let s3_bucket = s3_inputs.bucket.clone();
    let s3_access_key = s3_inputs.access_key_id.clone();
    let s3_secret_key = s3_inputs.secret_access_key.clone();
    let s3_prefix = s3_inputs.prefix.clone();
    let custom_inputs = ws.ui.backup_custom_inputs(cx);
    let custom_source = custom_inputs.source.clone();
    let custom_restore = custom_inputs.restore.clone();

    let transport_row = div()
        .flex()
        .items_center()
        .gap(px(8.0))
        .child(button_l(
            "backup-type-local",
            i.t("本地", "Local"),
            if backup_type == aitoolplus_core::settings::BackupType::Local {
                ButtonVariant::Primary
            } else {
                ButtonVariant::Secondary
            },
            &t,
            cx,
            |ws, _, _, cx| {
                ws.settings.backup_type = aitoolplus_core::settings::BackupType::Local;
                (ws.callbacks.save_settings)(&ws.settings);
                cx.notify();
            },
        ))
        .child(button_l(
            "backup-type-webdav",
            "WebDAV",
            if backup_type == aitoolplus_core::settings::BackupType::Webdav {
                ButtonVariant::Primary
            } else {
                ButtonVariant::Secondary
            },
            &t,
            cx,
            |ws, _, _, cx| {
                ws.settings.backup_type = aitoolplus_core::settings::BackupType::Webdav;
                (ws.callbacks.save_settings)(&ws.settings);
                cx.notify();
            },
        ))
        .child(button_l(
            "backup-type-s3",
            "S3",
            if backup_type == aitoolplus_core::settings::BackupType::S3 {
                ButtonVariant::Primary
            } else {
                ButtonVariant::Secondary
            },
            &t,
            cx,
            |ws, _, _, cx| {
                ws.settings.backup_type = aitoolplus_core::settings::BackupType::S3;
                (ws.callbacks.save_settings)(&ws.settings);
                cx.notify();
            },
        ))
        .into_any_element();

    let webdav_panel = (backup_type == aitoolplus_core::settings::BackupType::Webdav).then(|| {
        let list_url = dav_url.clone();
        let list_user = dav_user.clone();
        let list_password = dav_password.clone();
        let list_directory = dav_directory.clone();
        let test_url = dav_url.clone();
        let test_user = dav_user.clone();
        let test_password = dav_password.clone();
        let test_directory = dav_directory.clone();
        let save_url = dav_url.clone();
        let save_user = dav_user.clone();
        let save_password = dav_password.clone();
        let save_directory = dav_directory.clone();
        div()
            .flex()
            .flex_col()
            .gap(px(8.0))
            .p(px(10.0))
            .rounded(px(8.0))
            .bg(t.input_bg)
            .border_1()
            .border_color(t.card_border)
            .child(dav_url.clone())
            .child(dav_user.clone())
            .child(dav_password.clone())
            .child(dav_directory.clone())
            .child(
                div()
                    .flex()
                    .gap(px(8.0))
                    .child(button_l(
                        "webdav-save",
                        i.t("保存 WebDAV 设置", "Save WebDAV Settings"),
                        ButtonVariant::Secondary,
                        &t,
                        cx,
                        move |ws, _, _, cx| {
                            ws.settings.webdav.url =
                                save_url.update(cx, |input, _| input.text().trim().to_string());
                            ws.settings.webdav.username =
                                save_user.update(cx, |input, _| input.text().trim().to_string());
                            ws.settings.webdav.password =
                                save_password.update(cx, |input, _| input.text().to_string());
                            ws.settings.webdav.remote_directory = save_directory
                                .update(cx, |input, _| input.text().trim().to_string());
                            (ws.callbacks.save_settings)(&ws.settings);
                            ws.ui.toast(ws.i18n.t("已保存", "saved").to_string(), false);
                            cx.notify();
                        },
                    ))
                    .child(button_l(
                        "webdav-list",
                        i.t("列出远端备份", "List Remote Backups"),
                        ButtonVariant::Secondary,
                        &t,
                        cx,
                        move |ws, _, _, cx| {
                            let config = aitoolplus_core::settings::WebDavConfig {
                                url: list_url
                                    .update(cx, |input, _| input.text().trim().to_string()),
                                username: list_user
                                    .update(cx, |input, _| input.text().trim().to_string()),
                                password: list_password
                                    .update(cx, |input, _| input.text().to_string()),
                                remote_directory: list_directory
                                    .update(cx, |input, _| input.text().trim().to_string()),
                            };
                            match aitoolplus_core::webdav::list(&config) {
                                Ok(backups) => {
                                    let count = backups.len();
                                    ws.ui.remote_backups = backups;
                                    ws.ui.toast(
                                        ws.i18n
                                            .t(
                                                &format!("发现 {count} 个远端备份"),
                                                &format!("found {count} remote backups"),
                                            )
                                            .to_string(),
                                        false,
                                    );
                                }
                                Err(error) => {
                                    ws.ui.toast(format!("WebDAV list failed: {error}"), true)
                                }
                            }
                            cx.notify();
                        },
                    ))
                    .child(button_l(
                        "webdav-test",
                        i.t("测试连接", "Test Connection"),
                        ButtonVariant::Secondary,
                        &t,
                        cx,
                        move |ws, _, _, cx| {
                            let config = aitoolplus_core::settings::WebDavConfig {
                                url: test_url
                                    .update(cx, |input, _| input.text().trim().to_string()),
                                username: test_user
                                    .update(cx, |input, _| input.text().trim().to_string()),
                                password: test_password
                                    .update(cx, |input, _| input.text().to_string()),
                                remote_directory: test_directory
                                    .update(cx, |input, _| input.text().trim().to_string()),
                            };
                            match aitoolplus_core::webdav::test_connection(&config) {
                                Ok(()) => ws.ui.toast(
                                    ws.i18n
                                        .t("WebDAV 连接成功", "WebDAV connection succeeded")
                                        .to_string(),
                                    false,
                                ),
                                Err(error) => ws.ui.toast(format!("WebDAV failed: {error}"), true),
                            }
                            cx.notify();
                        },
                    )),
            )
            .into_any_element()
    });

    let s3_panel = (backup_type == aitoolplus_core::settings::BackupType::S3).then(|| {
        let save_endpoint = s3_endpoint.clone();
        let save_region = s3_region.clone();
        let save_bucket = s3_bucket.clone();
        let save_access_key = s3_access_key.clone();
        let save_secret_key = s3_secret_key.clone();
        let save_prefix = s3_prefix.clone();

        let list_endpoint = s3_endpoint.clone();
        let list_region = s3_region.clone();
        let list_bucket = s3_bucket.clone();
        let list_access_key = s3_access_key.clone();
        let list_secret_key = s3_secret_key.clone();
        let list_prefix = s3_prefix.clone();

        let test_endpoint = s3_endpoint.clone();
        let test_region = s3_region.clone();
        let test_bucket = s3_bucket.clone();
        let test_access_key = s3_access_key.clone();
        let test_secret_key = s3_secret_key.clone();
        let test_prefix = s3_prefix.clone();

        div()
            .flex()
            .flex_col()
            .gap(px(8.0))
            .p(px(10.0))
            .rounded(px(8.0))
            .bg(t.input_bg)
            .border_1()
            .border_color(t.card_border)
            .child(s3_endpoint.clone())
            .child(
                div()
                    .flex()
                    .gap(px(8.0))
                    .child(div().flex_1().child(s3_region.clone()))
                    .child(div().flex_1().child(s3_bucket.clone())),
            )
            .child(
                div()
                    .flex()
                    .gap(px(8.0))
                    .child(div().flex_1().child(s3_access_key.clone()))
                    .child(div().flex_1().child(s3_secret_key.clone())),
            )
            .child(
                div()
                    .flex()
                    .gap(px(8.0))
                    .child(div().flex_1().child(s3_prefix.clone()))
                    .child(button_l(
                        "s3-path-style-toggle",
                        if ws.settings.s3.path_style {
                            i.t("路径模式: 开", "Path-Style: ON")
                        } else {
                            i.t("路径模式: 关", "Path-Style: OFF")
                        },
                        ButtonVariant::Secondary,
                        &t,
                        cx,
                        |ws, _, _, cx| {
                            ws.settings.s3.path_style = !ws.settings.s3.path_style;
                            (ws.callbacks.save_settings)(&ws.settings);
                            cx.notify();
                        },
                    )),
            )
            .child(
                div()
                    .flex()
                    .gap(px(8.0))
                    .child(button_l(
                        "s3-save",
                        i.t("保存 S3 设置", "Save S3 Settings"),
                        ButtonVariant::Secondary,
                        &t,
                        cx,
                        move |ws, _, _, cx| {
                            ws.settings.s3.endpoint = save_endpoint
                                .update(cx, |input, _| input.text().trim().to_string());
                            ws.settings.s3.region =
                                save_region.update(cx, |input, _| input.text().trim().to_string());
                            ws.settings.s3.bucket =
                                save_bucket.update(cx, |input, _| input.text().trim().to_string());
                            ws.settings.s3.access_key_id = save_access_key
                                .update(cx, |input, _| input.text().trim().to_string());
                            ws.settings.s3.secret_access_key =
                                save_secret_key.update(cx, |input, _| input.text().to_string());
                            ws.settings.s3.prefix =
                                save_prefix.update(cx, |input, _| input.text().trim().to_string());
                            (ws.callbacks.save_settings)(&ws.settings);
                            ws.ui.toast(ws.i18n.t("已保存", "saved").to_string(), false);
                            cx.notify();
                        },
                    ))
                    .child(button_l(
                        "s3-list",
                        i.t("列出 S3 备份", "List S3 Backups"),
                        ButtonVariant::Secondary,
                        &t,
                        cx,
                        move |ws, _, _, cx| {
                            let config = aitoolplus_core::settings::S3Config {
                                endpoint: list_endpoint
                                    .update(cx, |input, _| input.text().trim().to_string()),
                                region: list_region
                                    .update(cx, |input, _| input.text().trim().to_string()),
                                bucket: list_bucket
                                    .update(cx, |input, _| input.text().trim().to_string()),
                                access_key_id: list_access_key
                                    .update(cx, |input, _| input.text().trim().to_string()),
                                secret_access_key: list_secret_key
                                    .update(cx, |input, _| input.text().to_string()),
                                prefix: list_prefix
                                    .update(cx, |input, _| input.text().trim().to_string()),
                                path_style: ws.settings.s3.path_style,
                            };
                            match aitoolplus_core::s3::list(&config) {
                                Ok(backups) => {
                                    let count = backups.len();
                                    ws.ui.remote_backups = backups;
                                    ws.ui.toast(
                                        ws.i18n
                                            .t(
                                                &format!("发现 {count} 个 S3 备份"),
                                                &format!("found {count} S3 backups"),
                                            )
                                            .to_string(),
                                        false,
                                    );
                                }
                                Err(error) => ws.ui.toast(format!("S3 list failed: {error}"), true),
                            }
                            cx.notify();
                        },
                    ))
                    .child(button_l(
                        "s3-test",
                        i.t("测试 S3 连接", "Test S3 Connection"),
                        ButtonVariant::Secondary,
                        &t,
                        cx,
                        move |ws, _, _, cx| {
                            let config = aitoolplus_core::settings::S3Config {
                                endpoint: test_endpoint
                                    .update(cx, |input, _| input.text().trim().to_string()),
                                region: test_region
                                    .update(cx, |input, _| input.text().trim().to_string()),
                                bucket: test_bucket
                                    .update(cx, |input, _| input.text().trim().to_string()),
                                access_key_id: test_access_key
                                    .update(cx, |input, _| input.text().trim().to_string()),
                                secret_access_key: test_secret_key
                                    .update(cx, |input, _| input.text().to_string()),
                                prefix: test_prefix
                                    .update(cx, |input, _| input.text().trim().to_string()),
                                path_style: ws.settings.s3.path_style,
                            };
                            match aitoolplus_core::s3::test_connection(&config) {
                                Ok(()) => ws.ui.toast(
                                    ws.i18n
                                        .t("S3 连接成功", "S3 connection succeeded")
                                        .to_string(),
                                    false,
                                ),
                                Err(error) => ws.ui.toast(format!("S3 failed: {error}"), true),
                            }
                            cx.notify();
                        },
                    )),
            )
            .into_any_element()
    });

    let remote_panel = ((backup_type == aitoolplus_core::settings::BackupType::Webdav
        || backup_type == aitoolplus_core::settings::BackupType::S3)
        && !ws.ui.remote_backups.is_empty())
    .then(|| {
        let mut panel = div().flex().flex_col().gap(px(6.0)).child(section_title(
            &t,
            i.t("远端备份", "Remote Backups"),
            None,
        ));
        for backup in ws.ui.remote_backups.clone() {
            let restore_name = backup.name.clone();
            let delete_name = backup.name.clone();
            panel = panel.child(
                div()
                    .flex()
                    .flex_col()
                    .items_start()
                    .gap(px(6.0))
                    .p(px(8.0))
                    .rounded(px(8.0))
                    .bg(t.input_bg)
                    .border_1()
                    .border_color(t.card_border)
                    .child(
                        div()
                            .text_size(px(12.0))
                            .text_color(t.text_primary)
                            .child(backup.name),
                    )
                    .child(
                        div()
                            .flex()
                            .gap(px(8.0))
                            .child(button_l(
                                gpui::SharedString::from(format!("remote-restore-{restore_name}")),
                                i.t("下载并恢复", "Download & Restore"),
                                ButtonVariant::Secondary,
                                &t,
                                cx,
                                move |ws, _, _, cx| {
                                    let temporary = ws
                                        .paths
                                        .app_data
                                        .join("backups")
                                        .join("downloads")
                                        .join(&restore_name);
                                    let download_res = if ws.settings.backup_type
                                        == aitoolplus_core::settings::BackupType::Webdav
                                    {
                                        aitoolplus_core::webdav::download(
                                            &ws.settings.webdav,
                                            &restore_name,
                                            &temporary,
                                        )
                                    } else {
                                        aitoolplus_core::s3::download(
                                            &ws.settings.s3,
                                            &restore_name,
                                            &temporary,
                                        )
                                    };
                                    match download_res.and_then(|path| {
                                        aitoolplus_core::backup::restore_backup_with_options(
                                            &ws.paths,
                                            &path,
                                            &aitoolplus_core::backup::RestoreOptions {
                                                allow_custom_absolute: ws
                                                    .ui
                                                    .restore_allow_custom_absolute,
                                                conflict_strategy: ws.ui.restore_conflict_strategy,
                                            },
                                        )
                                    }) {
                                        Ok(report) => {
                                            if let Ok(store) =
                                                aitoolplus_core::store::StoreHandle::open(&ws.paths)
                                            {
                                                ws.store = store;
                                            }
                                            ws.ui.toast(
                                                ws.i18n
                                                    .t(
                                                        &format!(
                                                            "恢复完成：{} 个文件",
                                                            report.restored
                                                        ),
                                                        &format!(
                                                            "restore complete: {} files",
                                                            report.restored
                                                        ),
                                                    )
                                                    .to_string(),
                                                false,
                                            );
                                        }
                                        Err(error) => ws
                                            .ui
                                            .toast(format!("remote restore failed: {error}"), true),
                                    }
                                    cx.notify();
                                },
                            ))
                            .child(button_l(
                                gpui::SharedString::from(format!("remote-delete-{delete_name}")),
                                i.t("删除远端", "Delete Remote"),
                                ButtonVariant::Danger,
                                &t,
                                cx,
                                move |ws, _, _, cx| {
                                    let del_res = if ws.settings.backup_type
                                        == aitoolplus_core::settings::BackupType::Webdav
                                    {
                                        aitoolplus_core::webdav::delete(
                                            &ws.settings.webdav,
                                            &delete_name,
                                        )
                                    } else {
                                        aitoolplus_core::s3::delete(&ws.settings.s3, &delete_name)
                                    };
                                    match del_res {
                                        Ok(()) => {
                                            ws.ui
                                                .remote_backups
                                                .retain(|item| item.name != delete_name);
                                            ws.ui.toast(
                                                ws.i18n
                                                    .t("远端备份已删除", "remote backup deleted")
                                                    .to_string(),
                                                false,
                                            );
                                        }
                                        Err(error) => {
                                            ws.ui.toast(format!("delete failed: {error}"), true)
                                        }
                                    }
                                    cx.notify();
                                },
                            )),
                    ),
            );
        }
        panel.into_any_element()
    });

    let mut filter_panel = div().flex().flex_col().gap(px(6.0)).child(section_title(
        &t,
        i.t("CLI 备份文件过滤", "CLI Backup File Filters"),
        Some(i.t(
            "逐文件排除敏感或不需要的运行时配置",
            "Exclude sensitive or unwanted runtime files individually",
        )),
    ));
    for file in aitoolplus_core::backup::cli_config_files(&ws.paths) {
        let display = file.display().to_string();
        let rule_path = display.clone();
        let excluded = ws
            .settings
            .backup_file_filter_rules
            .iter()
            .any(|rule| rule.file_path == display);
        filter_panel = filter_panel.child(
            div()
                .flex()
                .flex_col()
                .items_start()
                .gap(px(6.0))
                .p(px(8.0))
                .rounded(px(8.0))
                .bg(t.input_bg)
                .border_1()
                .border_color(t.card_border)
                .child(
                    div()
                        .text_size(px(11.0))
                        .text_color(t.text_secondary)
                        .child(display),
                )
                .child(button_l(
                    gpui::SharedString::from(format!("backup-filter-{rule_path}")),
                    if excluded {
                        i.t("已排除", "Excluded")
                    } else {
                        i.t("已包含", "Included")
                    },
                    if excluded {
                        ButtonVariant::Danger
                    } else {
                        ButtonVariant::Secondary
                    },
                    &t,
                    cx,
                    move |ws, _, _, cx| {
                        if ws
                            .settings
                            .backup_file_filter_rules
                            .iter()
                            .any(|rule| rule.file_path == rule_path)
                        {
                            ws.settings
                                .backup_file_filter_rules
                                .retain(|rule| rule.file_path != rule_path);
                        } else {
                            ws.settings.backup_file_filter_rules.push(
                                aitoolplus_core::settings::BackupFileFilterRule {
                                    tool: String::new(),
                                    file_path: rule_path.clone(),
                                },
                            );
                        }
                        (ws.callbacks.save_settings)(&ws.settings);
                        cx.notify();
                    },
                )),
        );
    }

    let mut custom_panel = div().flex().flex_col().gap(px(8.0)).child(section_title(
        &t,
        i.t("自定义备份条目", "Custom Backup Entries"),
        Some(i.t(
            "额外文件或目录；空恢复路径会落到安全沙箱",
            "Extra files or directories; blank restore paths use a safe sandbox",
        )),
    ));
    for entry in ws.settings.backup_custom_entries.clone() {
        let id = entry.id.clone();
        custom_panel = custom_panel.child(
            div()
                .flex()
                .flex_col()
                .items_start()
                .gap(px(6.0))
                .p(px(8.0))
                .rounded(px(8.0))
                .bg(t.input_bg)
                .border_1()
                .border_color(t.card_border)
                .child(
                    div()
                        .text_size(px(12.0))
                        .text_color(t.text_primary)
                        .child(entry.source_path),
                )
                .children(entry.restore_path.map(|path| {
                    div()
                        .text_size(px(11.0))
                        .text_color(t.text_muted)
                        .child(format!("restore: {path}"))
                        .into_any_element()
                }))
                .child(button_l(
                    gpui::SharedString::from(format!("custom-backup-remove-{id}")),
                    i.t("移除条目", "Remove Entry"),
                    ButtonVariant::Danger,
                    &t,
                    cx,
                    move |ws, _, _, cx| {
                        ws.settings
                            .backup_custom_entries
                            .retain(|entry| entry.id != id);
                        (ws.callbacks.save_settings)(&ws.settings);
                        cx.notify();
                    },
                )),
        );
    }
    let add_source = custom_source.clone();
    let add_restore = custom_restore.clone();
    custom_panel = custom_panel.child(
        div()
            .flex()
            .flex_col()
            .gap(px(6.0))
            .child(custom_source)
            .child(custom_restore)
            .child(button_l(
                "custom-backup-add",
                i.t("添加条目", "Add Entry"),
                ButtonVariant::Secondary,
                &t,
                cx,
                move |ws, _, _, cx| {
                    let source = add_source.update(cx, |input, _| input.text().trim().to_string());
                    let restore =
                        add_restore.update(cx, |input, _| input.text().trim().to_string());
                    if source.is_empty() {
                        ws.ui.toast(
                            ws.i18n
                                .t("请输入源路径", "source path required")
                                .to_string(),
                            true,
                        );
                    } else {
                        ws.settings.backup_custom_entries.push(
                            aitoolplus_core::settings::BackupCustomEntry {
                                id: uuid::Uuid::new_v4().to_string(),
                                source_path: source,
                                restore_path: (!restore.is_empty()).then_some(restore),
                            },
                        );
                        (ws.callbacks.save_settings)(&ws.settings);
                    }
                    cx.notify();
                },
            )),
    );

    card(
        &t,
        vec![
            section_title(
                &t,
                i.t("备份与恢复", "Backup & Restore"),
                Some(i.t(
                    "ZIP 备份包含应用数据、CLI 配置与中央 Skills 仓库",
                    "ZIP bundles include app data, CLI configs and the central Skills repo",
                )),
            ),
            transport_row,
            webdav_panel.unwrap_or_else(|| div().into_any_element()),
            s3_panel.unwrap_or_else(|| div().into_any_element()),
            remote_panel.unwrap_or_else(|| div().into_any_element()),
            div()
                .flex()
                .flex_col()
                .gap(px(6.0))
                .child(section_title(
                    &t,
                    i.t("恢复冲突策略与选项", "Restore Conflict Strategy & Options"),
                    Some(i.t(
                        "遇到同名文件时的处理方式，以及是否允许恢复自定义绝对路径",
                        "How to handle existing files, and whether to allow custom absolute paths",
                    )),
                ))
                .child(
                    div()
                        .flex()
                        .gap(px(8.0))
                        .child(button_l(
                            "conflict-strategy-overwrite",
                            i.t("覆盖原文件", "Overwrite"),
                            if ws.ui.restore_conflict_strategy
                                == aitoolplus_core::backup::ConflictStrategy::Overwrite
                            {
                                ButtonVariant::Primary
                            } else {
                                ButtonVariant::Secondary
                            },
                            &t,
                            cx,
                            |ws, _, _, cx| {
                                ws.ui.restore_conflict_strategy =
                                    aitoolplus_core::backup::ConflictStrategy::Overwrite;
                                cx.notify();
                            },
                        ))
                        .child(button_l(
                            "conflict-strategy-skip",
                            i.t("跳过同名文件", "Skip Existing"),
                            if ws.ui.restore_conflict_strategy
                                == aitoolplus_core::backup::ConflictStrategy::Skip
                            {
                                ButtonVariant::Primary
                            } else {
                                ButtonVariant::Secondary
                            },
                            &t,
                            cx,
                            |ws, _, _, cx| {
                                ws.ui.restore_conflict_strategy =
                                    aitoolplus_core::backup::ConflictStrategy::Skip;
                                cx.notify();
                            },
                        ))
                        .child(button_l(
                            "conflict-strategy-savecopy",
                            i.t("另存副本 (.restored)", "Save Copy (.restored)"),
                            if ws.ui.restore_conflict_strategy
                                == aitoolplus_core::backup::ConflictStrategy::SaveCopy
                            {
                                ButtonVariant::Primary
                            } else {
                                ButtonVariant::Secondary
                            },
                            &t,
                            cx,
                            |ws, _, _, cx| {
                                ws.ui.restore_conflict_strategy =
                                    aitoolplus_core::backup::ConflictStrategy::SaveCopy;
                                cx.notify();
                            },
                        ))
                        .child(button_l(
                            "restore-custom-absolute-toggle",
                            if ws.ui.restore_allow_custom_absolute {
                                i.t("允许原绝对路径: 开", "Custom Absolute: ON")
                            } else {
                                i.t("沙箱隔离恢复: 关", "Sandbox Safe: OFF")
                            },
                            ButtonVariant::Secondary,
                            &t,
                            cx,
                            |ws, _, _, cx| {
                                ws.ui.restore_allow_custom_absolute =
                                    !ws.ui.restore_allow_custom_absolute;
                                cx.notify();
                            },
                        )),
                )
                .into_any_element(),
            filter_panel.into_any_element(),
            custom_panel.into_any_element(),
            div()
                .flex()
                .flex_col()
                .items_start()
                .gap(px(8.0))
                .child(section_title(
                    &t,
                    i.t("包含 CLI 配置", "Include CLI Configs"),
                    Some(i.t(
                        "包含各工具配置、Prompt、MCP 和插件状态",
                        "Include tool configs, prompts, MCP and plugin state",
                    )),
                ))
                .child(button_l(
                    "backup-cli-toggle",
                    if ws.settings.backup_cli_config_files_enabled {
                        i.t("已包含", "Included")
                    } else {
                        i.t("未包含", "Excluded")
                    },
                    if ws.settings.backup_cli_config_files_enabled {
                        ButtonVariant::Primary
                    } else {
                        ButtonVariant::Secondary
                    },
                    &t,
                    cx,
                    |ws, _, _, cx| {
                        ws.settings.backup_cli_config_files_enabled =
                            !ws.settings.backup_cli_config_files_enabled;
                        (ws.callbacks.save_settings)(&ws.settings);
                        cx.notify();
                    },
                ))
                .into_any_element(),
            div()
                .flex()
                .flex_col()
                .items_start()
                .gap(px(8.0))
                .child(section_title(
                    &t,
                    i.t("自动备份", "Automatic Backup"),
                    Some(gpui::SharedString::from(format!(
                        "{} {} {} · {} {}",
                        i.t("每", "Every"),
                        ws.settings.auto_backup_interval_days,
                        i.t("天", "days"),
                        i.t("保留", "keep"),
                        ws.settings.auto_backup_max_keep
                    ))),
                ))
                .child(button_l(
                    "auto-backup-toggle",
                    if ws.settings.auto_backup_enabled {
                        i.t("已开启", "Enabled")
                    } else {
                        i.t("已关闭", "Disabled")
                    },
                    if ws.settings.auto_backup_enabled {
                        ButtonVariant::Primary
                    } else {
                        ButtonVariant::Secondary
                    },
                    &t,
                    cx,
                    |ws, _, _, cx| {
                        ws.settings.auto_backup_enabled = !ws.settings.auto_backup_enabled;
                        (ws.callbacks.save_settings)(&ws.settings);
                        cx.notify();
                    },
                ))
                .into_any_element(),
            button_l(
                "backup-export",
                i.t("立即创建 ZIP 备份", "Create ZIP Backup Now"),
                ButtonVariant::Primary,
                &t,
                cx,
                |ws, _, _, cx| {
                    let directory = ws.paths.app_data.join("backups").join("manual");
                    let output = directory.join(format!(
                        "aitoolplus-backup-{}.zip",
                        chrono::Local::now().format("%Y%m%d-%H%M%S")
                    ));
                    match aitoolplus_core::backup::create_backup(&ws.paths, &ws.settings, &output) {
                        Ok(report) => {
                            let remote = if ws.settings.backup_type
                                == aitoolplus_core::settings::BackupType::Webdav
                            {
                                aitoolplus_core::webdav::upload(&ws.settings.webdav, &report.output)
                                    .map(|backup| format!(" · WebDAV: {}", backup.name))
                            } else if ws.settings.backup_type
                                == aitoolplus_core::settings::BackupType::S3
                            {
                                aitoolplus_core::s3::upload(&ws.settings.s3, &report.output)
                                    .map(|backup| format!(" · S3: {}", backup.name))
                            } else {
                                Ok(String::new())
                            };
                            match remote {
                                Ok(remote) => ws.ui.toast(
                                    ws.i18n
                                        .t(
                                            &format!(
                                                "备份完成：{}（{} 个文件）{}",
                                                report.output.display(),
                                                report.file_count,
                                                remote
                                            ),
                                            &format!(
                                                "backup complete: {} ({} files){}",
                                                report.output.display(),
                                                report.file_count,
                                                remote
                                            ),
                                        )
                                        .to_string(),
                                    false,
                                ),
                                Err(error) => ws.ui.toast(
                                    format!("local backup ok; remote upload failed: {error}"),
                                    true,
                                ),
                            }
                        }
                        Err(error) => ws.ui.toast(format!("backup failed: {error}"), true),
                    }
                    cx.notify();
                },
            ),
            button_l(
                "backup-restore",
                i.t("恢复 ZIP 备份", "Restore ZIP Backup"),
                ButtonVariant::Secondary,
                &t,
                cx,
                |_ws, _, _, cx| {
                    let dialog = rfd::AsyncFileDialog::new().add_filter("ZIP", &["zip"]);
                    let weak = cx.entity().downgrade();
                    cx.spawn(async move |_this, cx| {
                        if let Some(file) = dialog.pick_file().await {
                            let archive = file.path().to_path_buf();
                            let _ = weak.update(cx, |ws: &mut Workspace, cx| {
                                let paths = ws.paths.clone();
                                match aitoolplus_core::backup::restore_backup_with_options(
                                    &paths,
                                    &archive,
                                    &aitoolplus_core::backup::RestoreOptions {
                                        allow_custom_absolute: ws.ui.restore_allow_custom_absolute,
                                        conflict_strategy: ws.ui.restore_conflict_strategy,
                                    },
                                ) {
                                    Ok(report) => {
                                        if let Ok(store) =
                                            aitoolplus_core::store::StoreHandle::open(&paths)
                                        {
                                            ws.store = store;
                                        }
                                        ws.settings = aitoolplus_core::settings::AppSettings::load(
                                            &paths.settings_file(),
                                        );
                                        ws.ui.toast(
                                            ws.i18n
                                                .t(
                                                    &format!(
                                                        "恢复完成：{} 个文件 (覆盖 {}, 副本 {})",
                                                        report.restored,
                                                        report.overwritten,
                                                        report.copies.len()
                                                    ),
                                                    &format!(
                                                        "restore complete: {} files (overwritten {}, copies {})",
                                                        report.restored,
                                                        report.overwritten,
                                                        report.copies.len()
                                                    ),
                                                )
                                                .to_string(),
                                            false,
                                        );
                                    }
                                    Err(error) => {
                                        ws.ui.toast(format!("restore failed: {error}"), true)
                                    }
                                }
                                cx.notify();
                            });
                        }
                    })
                    .detach();
                },
            ),
        ],
    )
    .into_any_element()
}

fn about_tab(ws: &mut Workspace, cx: &mut Context<Workspace>) -> gpui::AnyElement {
    let t = ws.theme.clone();
    let i = ws.i18n;
    let version = env!("CARGO_PKG_VERSION");
    let info = ws.ui.update_info.clone();

    let mut children = vec![
        section_title(&t, i.t("关于", "About"), None),
        div()
            .flex()
            .flex_col()
            .gap(px(4.0))
            .child(
                div()
                    .text_size(px(13.0))
                    .text_color(t.text_primary)
                    .child(format!("AI ToolPlus v{version}")),
            )
            .child(
                div()
                    .text_size(px(12.0))
                    .text_color(t.text_secondary)
                    .child(i.t(
                        "Rust + GPUI 原生桌面应用，对标 ai-toolbox",
                        "Native Rust + GPUI desktop app, mirroring ai-toolbox",
                    )),
            )
            .into_any_element(),
        div()
            .flex()
            .flex_col()
            .items_start()
            .gap(px(8.0))
            .child(section_title(
                &t,
                i.t("自动检查更新", "Automatic Update Check"),
                Some(i.t(
                    "启动后可检查 GitHub Releases；不会静默安装",
                    "Check GitHub Releases at startup; never installs silently",
                )),
            ))
            .child(button_l(
                "auto-update-check-toggle",
                if ws.settings.auto_update_check_enabled {
                    i.t("已开启", "Enabled")
                } else {
                    i.t("已关闭", "Disabled")
                },
                if ws.settings.auto_update_check_enabled {
                    ButtonVariant::Primary
                } else {
                    ButtonVariant::Secondary
                },
                &t,
                cx,
                |ws, _, _, cx| {
                    ws.settings.auto_update_check_enabled = !ws.settings.auto_update_check_enabled;
                    (ws.callbacks.save_settings)(&ws.settings);
                    cx.notify();
                },
            ))
            .into_any_element(),
        button_l(
            "check-for-updates",
            i.t("检查更新", "Check for Updates"),
            ButtonVariant::Primary,
            &t,
            cx,
            |_ws, _, _, cx| {
                let weak = cx.entity().downgrade();
                cx.spawn(async move |_this, cx| {
                    let result = cx
                        .background_spawn(async move {
                            aitoolplus_core::updater::check_latest(env!("CARGO_PKG_VERSION"))
                        })
                        .await;
                    let _ = weak.update(cx, |ws, cx| {
                        ws.settings.last_update_check_time = Some(chrono::Utc::now().to_rfc3339());
                        (ws.callbacks.save_settings)(&ws.settings);
                        match result {
                            Ok(info) => {
                                let message = if info.update_available {
                                    ws.i18n
                                        .t(
                                            &format!("发现新版本 {}", info.latest_version),
                                            &format!(
                                                "new version {} available",
                                                info.latest_version
                                            ),
                                        )
                                        .to_string()
                                } else {
                                    ws.i18n
                                        .t("当前已是最新版", "already up to date")
                                        .to_string()
                                };
                                ws.ui.update_info = Some(info);
                                ws.ui.toast(message, false);
                            }
                            Err(error) => {
                                ws.ui.toast(format!("update check failed: {error}"), true)
                            }
                        }
                        cx.notify();
                    });
                })
                .detach();
            },
        ),
    ];

    if let Some(info) = info {
        let notes: String = info.release_notes.chars().take(800).collect();
        let asset = aitoolplus_core::updater::best_asset(&info).cloned();
        children.push(
            div()
                .flex()
                .flex_col()
                .gap(px(6.0))
                .p(px(10.0))
                .rounded(px(8.0))
                .bg(t.input_bg)
                .border_1()
                .border_color(if info.update_available {
                    t.warning
                } else {
                    t.success
                })
                .child(
                    div()
                        .text_size(px(13.0))
                        .font_weight(gpui::FontWeight::MEDIUM)
                        .text_color(t.text_primary)
                        .child(format!(
                            "{} → {}",
                            info.current_version, info.latest_version
                        )),
                )
                .children((!notes.is_empty()).then(|| {
                    div()
                        .text_size(px(11.5))
                        .text_color(t.text_secondary)
                        .child(notes)
                        .into_any_element()
                }))
                .children(asset.map(|asset| {
                    button_l(
                        "download-update-asset",
                        i.t("下载更新", "Download Update"),
                        ButtonVariant::Primary,
                        &t,
                        cx,
                        move |ws, _, _, cx| {
                            let output = ws.paths.app_data.join("updates").join(&asset.name);
                            match aitoolplus_core::updater::download(&asset, &output) {
                                Ok(path) => {
                                    ws.ui.downloaded_update_asset_path = Some(path.clone());
                                    ws.ui.toast(
                                        ws.i18n
                                            .t(
                                                &format!("已下载到 {}", path.display()),
                                                &format!("downloaded to {}", path.display()),
                                            )
                                            .to_string(),
                                        false,
                                    );
                                }
                                Err(error) => {
                                    ws.ui.toast(format!("download failed: {error}"), true)
                                }
                            }
                            cx.notify();
                        },
                    )
                }))
                .children(ws.ui.downloaded_update_asset_path.as_ref().map(|path| {
                    let asset_path = path.clone();
                    button_l(
                        "install-update-now",
                        i.t("立即安装并重启", "Install & Restart Now"),
                        ButtonVariant::Primary,
                        &t,
                        cx,
                        move |ws, _, _, cx| {
                            match aitoolplus_core::updater::install_update_and_restart(&asset_path)
                            {
                                Ok(()) => {}
                                Err(error) => {
                                    ws.ui.toast(format!("install update failed: {error}"), true);
                                    cx.notify();
                                }
                            }
                        },
                    )
                }))
                .into_any_element(),
        );
    }

    card(&t, children).into_any_element()
}

fn open_dir_in_explorer(path: &std::path::Path) -> std::io::Result<()> {
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("explorer")
            .arg(path)
            .spawn()
            .map(|_| ())
    }
    #[cfg(not(target_os = "windows"))]
    {
        std::process::Command::new("xdg-open")
            .arg(path)
            .spawn()
            .map(|_| ())
    }
}

/// Local copy of the dark detection (keeps this module self-contained).
fn system_prefers_dark() -> bool {
    crate::workspace::system_prefers_dark_pub()
}
