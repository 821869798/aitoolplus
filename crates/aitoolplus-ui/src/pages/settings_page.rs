//! Settings page: general, backup, about.

use gpui::{Context, IntoElement, div, prelude::*, px};

use crate::components::{
    ButtonVariant, button_l, button_with_icon_l, card, input_container, section_title,
    segmented_pill_selector, settings_card, settings_row, toggle,
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

    let current_tab = ws.ui.settings_tab;
    let tab_bar = crate::components::segmented_tab_bar(
        "settings",
        tabs.to_vec(),
        current_tab,
        &t,
        cx,
        |ws, tab, _window, cx| {
            ws.ui.settings_tab = tab;
            cx.notify();
        },
    );

    let body = match ws.ui.settings_tab {
        SettingsTab::General => general_tab(ws, cx),
        SettingsTab::Backup => backup_tab(ws, cx),
        SettingsTab::About => about_tab(ws, cx),
    };

    div()
        .flex()
        .flex_col()
        .w_full()
        .max_w(px(880.0))
        .min_w(px(0.0))
        .gap(px(16.0))
        .child(tab_bar)
        .child(body)
        .into_any_element()
}

fn general_tab(ws: &mut Workspace, cx: &mut Context<Workspace>) -> gpui::AnyElement {
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
        ],
    );

    // 3. Network & Proxy Card
    let proxy_mode = ws.settings.proxy_mode;
    let proxy_input = ws.ui.proxy_url_input.clone();
    let proxy_save = proxy_input.clone();

    let proxy_selector = segmented_pill_selector(
        "proxy-mode",
        vec![
            (
                aitoolplus_core::settings::ProxyMode::System,
                None,
                i.t("跟随系统代理", "System"),
            ),
            (
                aitoolplus_core::settings::ProxyMode::Direct,
                None,
                i.t("直连模式", "Direct"),
            ),
            (
                aitoolplus_core::settings::ProxyMode::Custom,
                None,
                i.t("自定义代理", "Custom"),
            ),
        ],
        proxy_mode,
        &t,
        cx,
        |ws, mode, _, cx| {
            ws.settings.proxy_mode = mode;
            (ws.callbacks.save_settings)(&ws.settings);
            cx.notify();
        },
    );

    let mut proxy_rows = vec![
        settings_row(
            &t,
            i.t("网络代理策略", "Network Proxy Policy"),
            Some(i.t(
                "用于上游模型拉取、客户端版本检查及云端同步请求",
                "Used by upstream model fetch, updater, and cloud sync requests",
            )),
            proxy_selector,
        ),
    ];

    if proxy_mode == aitoolplus_core::settings::ProxyMode::Custom {
        let custom_url_row = div()
            .flex()
            .items_center()
            .justify_between()
            .w_full()
            .min_h(px(52.0))
            .px(px(16.0))
            .py(px(12.0))
            .gap(px(12.0))
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .child(input_container(&t, proxy_input)),
            )
            .child(button_l(
                "proxy-save",
                i.t("保存代理", "Save Proxy"),
                ButtonVariant::Primary,
                &t,
                cx,
                move |ws, _, _, cx| {
                    ws.settings.proxy_url =
                        proxy_save.update(cx, |input, _| input.text().trim().to_string());
                    (ws.callbacks.save_settings)(&ws.settings);
                    ws.ui.toast(
                        ws.i18n
                            .t("代理设置已保存；重启后生效", "Proxy saved; applies after restart")
                            .to_string(),
                        false,
                    );
                    cx.notify();
                },
            ))
            .into_any_element();
        proxy_rows.push(custom_url_row);
    }

    let network_card = settings_card(
        &t,
        i.t("网络与代理", "Network & Proxy"),
        Some(i.t(
            "配置工作台发起网络请求时使用的代理方式与出口地址",
            "Configure outbound proxy URL and connection behavior",
        )),
        proxy_rows,
    );

    // 4. CLI Launch & Auth Policies Card
    let cli_policies_card = settings_card(
        &t,
        i.t("CLI 运行与认证策略", "CLI Launch & Auth Policies"),
        Some(i.t(
            "针对各命令行工具的环境变量与运行时配置写入安全策略",
            "Security, auth preservation, and runtime policies for CLI tools",
        )),
        vec![
            settings_row(
                &t,
                i.t("Claude 全权限启动 (--dangerously-skip-permissions)", "Claude Full-Access Launch"),
                Some(i.t(
                    "启动 Claude Code 时自动附加全权限参数，跳过频繁的危险确认提示",
                    "Pass --dangerously-skip-permissions on Claude Code startup",
                )),
                toggle(
                    "claude-full-access",
                    ws.settings.claude_cli_launch_full_access,
                    &t,
                    cx,
                    |ws, _, _, cx| {
                        ws.settings.claude_cli_launch_full_access =
                            !ws.settings.claude_cli_launch_full_access;
                        (ws.callbacks.save_settings)(&ws.settings);
                        cx.notify();
                    },
                ),
            ),
            settings_row(
                &t,
                i.t("Codex 保留官方登录态", "Preserve Codex Official Auth"),
                Some(i.t(
                    "切换第三方供应商时保留 ~/.codex 的官方登录凭据与会话",
                    "Keep official login session in ~/.codex on provider switch",
                )),
                toggle(
                    "codex-preserve-auth",
                    ws.settings.codex_preserve_official_auth_on_switch,
                    &t,
                    cx,
                    |ws, _, _, cx| {
                        ws.settings.codex_preserve_official_auth_on_switch =
                            !ws.settings.codex_preserve_official_auth_on_switch;
                        (ws.callbacks.save_settings)(&ws.settings);
                        cx.notify();
                    },
                ),
            ),
            settings_row(
                &t,
                i.t("OpenAgent 统一 ~/.omo 配置", "OpenAgent Unified ~/.omo Config"),
                Some(i.t(
                    "使用现代统一的 ~/.omo 目录而非旧版分散配置文件",
                    "Write unified config to ~/.omo instead of legacy files",
                )),
                toggle(
                    "omo-legacy-config",
                    ws.settings.opencode_use_legacy_oh_my_config,
                    &t,
                    cx,
                    |ws, _, _, cx| {
                        ws.settings.opencode_use_legacy_oh_my_config =
                            !ws.settings.opencode_use_legacy_oh_my_config;
                        (ws.callbacks.save_settings)(&ws.settings);
                        cx.notify();
                    },
                ),
            ),
            settings_row(
                &t,
                i.t("允许清除 OMO/OMOS 运行配置", "Allow Clearing OMO/OMOS Config"),
                Some(i.t(
                    "在重置或切换供应商时允许清空已应用的运行时配置",
                    "Allow wiping runtime config when resetting or switching",
                )),
                toggle(
                    "omo-clear-policy",
                    ws.settings.opencode_allow_clear_applied_oh_my_config,
                    &t,
                    cx,
                    |ws, _, _, cx| {
                        ws.settings.opencode_allow_clear_applied_oh_my_config =
                            !ws.settings.opencode_allow_clear_applied_oh_my_config;
                        (ws.callbacks.save_settings)(&ws.settings);
                        cx.notify();
                    },
                ),
            ),
            settings_row(
                &t,
                i.t("双写 reasoning/variant 兼容模式", "Dual Reasoning/Variant Write"),
                Some(i.t(
                    "同时写入推理模型参数以兼容旧版 OpenCode 插件",
                    "Write dual parameters for compatibility with older OpenCode",
                )),
                toggle(
                    "omo-dual-reasoning",
                    ws.settings.opencode_dual_write_reasoning_variant,
                    &t,
                    cx,
                    |ws, _, _, cx| {
                        ws.settings.opencode_dual_write_reasoning_variant =
                            !ws.settings.opencode_dual_write_reasoning_variant;
                        (ws.callbacks.save_settings)(&ws.settings);
                        cx.notify();
                    },
                ),
            ),
        ],
    );

    // 5. Visible Tools Card (Sidebar Tool Chips)
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

    // 6. Storage & CLI Roots Card
    let data_dir = ws.paths.app_data.display().to_string();
    let storage_row = settings_row(
        &t,
        i.t("应用数据存储目录", "Application Data Directory"),
        Some(gpui::SharedString::from(data_dir.clone())),
        button_with_icon_l(
            "open-data-dir",
            crate::icons::FOLDER_SVG,
            i.t("打开数据目录", "Open Folder"),
            ButtonVariant::Secondary,
            &t,
            cx,
            move |ws, _, _, cx| {
                let _ = open_dir_in_explorer(&ws.paths.app_data);
                cx.notify();
            },
        ),
    );

    let mut roots_rows = div()
        .flex()
        .flex_col()
        .gap(px(10.0))
        .p(px(16.0));
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

        roots_rows = roots_rows.child(
            div()
                .flex()
                .flex_col()
                .gap(px(6.0))
                .p(px(10.0))
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
                    i.t("保存根目录覆盖", "Save Root Override"),
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
                    i.t("保存 CLI 执行路径", "Save CLI Path"),
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

    let storage_card = settings_card(
        &t,
        i.t("数据存储与 CLI 路径覆盖", "Storage & CLI Paths"),
        Some(i.t(
            "查看核心配置存储路径，或自定义特定工具的配置文件与命令行程序位置",
            "Inspect data directory or override config roots and CLI binary paths",
        )),
        vec![
            storage_row,
            roots_rows.into_any_element(),
        ],
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
        .child(cli_policies_card)
        .child(visibility_card)
        .child(storage_card)
        .into_any_element()
}

struct LocalBackupEntry {
    path: std::path::PathBuf,
    name: String,
    size_bytes: u64,
    date_str: String,
}

fn list_local_backups(paths: &aitoolplus_core::paths::Paths) -> Vec<LocalBackupEntry> {
    let mut entries = Vec::new();
    let dirs = [
        paths.app_data.join("backups").join("manual"),
        paths.app_data.join("backups").join("auto"),
    ];
    for dir in dirs {
        if let Ok(read_dir) = std::fs::read_dir(dir) {
            for entry in read_dir.flatten() {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) == Some("zip") {
                    let name = path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("backup.zip")
                        .to_string();
                    let (size_bytes, date_str) = if let Ok(meta) = path.metadata() {
                        let size = meta.len();
                        let date = meta
                            .modified()
                            .ok()
                            .and_then(|m| m.duration_since(std::time::UNIX_EPOCH).ok())
                            .map(|d| {
                                let dt = chrono::DateTime::from_timestamp(d.as_secs() as i64, 0);
                                dt.map(|t| t.format("%Y-%m-%d %H:%M:%S").to_string())
                                    .unwrap_or_default()
                            })
                            .unwrap_or_default();
                        (size, date)
                    } else {
                        (0, String::new())
                    };
                    entries.push(LocalBackupEntry {
                        path,
                        name,
                        size_bytes,
                        date_str,
                    });
                }
            }
        }
    }
    entries.sort_by(|a, b| b.name.cmp(&a.name));
    entries
}

fn format_file_size(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{bytes} B")
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    }
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

    let transport_row = segmented_pill_selector(
        "backup-type",
        vec![
            (
                aitoolplus_core::settings::BackupType::Local,
                Some(crate::icons::FOLDER_SVG),
                i.t("本地快照", "Local"),
            ),
            (
                aitoolplus_core::settings::BackupType::Webdav,
                Some(crate::icons::GLOBE_SVG),
                i.t("WebDAV 云端", "WebDAV"),
            ),
            (
                aitoolplus_core::settings::BackupType::S3,
                Some(crate::icons::DOWNLOAD_SVG),
                i.t("S3 兼容存储", "S3 Storage"),
            ),
        ],
        backup_type,
        &t,
        cx,
        |ws, b_type, _, cx| {
            ws.settings.backup_type = b_type;
            (ws.callbacks.save_settings)(&ws.settings);
            cx.notify();
        },
    );

    let local_panel = (backup_type == aitoolplus_core::settings::BackupType::Local).then(|| {
        let local_backups = list_local_backups(&ws.paths);
        let panel = div()
            .flex()
            .flex_col()
            .gap(px(10.0))
            .p(px(10.0))
            .rounded(px(8.0))
            .bg(t.input_bg)
            .border_1()
            .border_color(t.card_border)
            .child(section_title(
                &t,
                i.t("本地备份与恢复", "Local Backup & Restore"),
                Some(i.t(
                    "下载导出 ZIP 备份到本地，或选择已有备份文件上传恢复配置",
                    "Download ZIP backup or upload an existing backup to restore",
                )),
            ))
            .child(
                div()
                    .flex()
                    .gap(px(8.0))
                    .flex_wrap()
                    .child(button_with_icon_l(
                        "backup-download-zip",
                        crate::icons::DOWNLOAD_SVG,
                        i.t("下载备份 (导出 ZIP)", "Download Backup (Export ZIP)"),
                        ButtonVariant::Primary,
                        &t,
                        cx,
                        |_ws, _, _, cx| {
                            let dialog = rfd::AsyncFileDialog::new()
                                .set_file_name(&format!(
                                    "aitoolplus-backup-{}.zip",
                                    chrono::Local::now().format("%Y%m%d-%H%M%S")
                                ))
                                .add_filter("ZIP", &["zip"]);
                            let weak = cx.entity().downgrade();
                            cx.spawn(async move |_this, cx| {
                                if let Some(file) = dialog.save_file().await {
                                    let output = file.path().to_path_buf();
                                    let _ = weak.update(cx, |ws: &mut Workspace, cx| {
                                        match aitoolplus_core::backup::create_backup(
                                            &ws.paths,
                                            &ws.settings,
                                            &output,
                                        ) {
                                            Ok(report) => {
                                                ws.ui.toast(
                                                    ws.i18n
                                                        .t(
                                                            &format!(
                                                                "下载备份成功：已导出至 {}（{} 个文件）",
                                                                report.output.display(),
                                                                report.file_count
                                                            ),
                                                            &format!(
                                                                "Backup downloaded: {} ({} files)",
                                                                report.output.display(),
                                                                report.file_count
                                                            ),
                                                        )
                                                        .to_string(),
                                                    false,
                                                );
                                            }
                                            Err(e) => {
                                                ws.ui.toast(format!("下载备份失败: {e}"), true);
                                            }
                                        }
                                        cx.notify();
                                    });
                                }
                            })
                            .detach();
                        },
                    ))
                    .child(button_with_icon_l(
                        "backup-upload-zip",
                        crate::icons::UPLOAD_SVG,
                        i.t("上传备份 (从 ZIP 恢复)", "Upload Backup (Restore ZIP)"),
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
                                                allow_custom_absolute: ws
                                                    .ui
                                                    .restore_allow_custom_absolute,
                                                conflict_strategy: ws
                                                    .ui
                                                    .restore_conflict_strategy,
                                            },
                                        ) {
                                            Ok(report) => {
                                                if let Ok(store) =
                                                    aitoolplus_core::store::StoreHandle::open(&paths)
                                                {
                                                    ws.store = store;
                                                }
                                                ws.settings =
                                                    aitoolplus_core::settings::AppSettings::load(
                                                        &paths.settings_file(),
                                                    );
                                                ws.ui.toast(
                                                    ws.i18n
                                                        .t(
                                                            &format!(
                                                                "上传恢复完成：{} 个文件 (覆盖 {}, 副本 {})",
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
                                            Err(e) => {
                                                ws.ui.toast(format!("上传恢复失败: {e}"), true)
                                            }
                                        }
                                        cx.notify();
                                    });
                                }
                            })
                            .detach();
                        },
                    ))
                    .child(button_with_icon_l(
                        "backup-open-folder",
                        crate::icons::FOLDER_SVG,
                        i.t("打开备份目录", "Open Backup Directory"),
                        ButtonVariant::Secondary,
                        &t,
                        cx,
                        |ws, _, _, _| {
                            let dir = ws.paths.app_data.join("backups");
                            let _ = std::fs::create_dir_all(&dir);
                            let _ = std::process::Command::new("explorer").arg(&dir).spawn();
                        },
                    )),
            );

        let mut list_section = div().flex().flex_col().gap(px(6.0)).child(
            div()
                .text_size(px(12.5))
                .font_weight(gpui::FontWeight::MEDIUM)
                .text_color(t.text_primary)
                .child(i.t("本地备份文件列表", "Local Backup Files")),
        );

        if local_backups.is_empty() {
            list_section = list_section.child(
                div()
                    .text_size(px(11.5))
                    .text_color(t.text_muted)
                    .child(i.t(
                        "暂无本地备份文件，点击上方【下载备份 (导出 ZIP)】即可创建首个备份",
                        "No local backups found. Click 'Download Backup (Export ZIP)' to create one.",
                    )),
            );
        } else {
            for b in local_backups {
                let file_path_for_restore = b.path.clone();
                let file_path_for_save_as = b.path.clone();
                let file_path_for_delete = b.path.clone();
                let file_name = b.name.clone();
                let file_name_for_restore_id = b.name.clone();
                let file_name_for_save_as_id = b.name.clone();
                let file_name_for_save_as_dialog = b.name.clone();
                let file_name_for_delete_id = b.name.clone();
                let file_size_str = format_file_size(b.size_bytes);
                let date_str = b.date_str.clone();

                list_section = list_section.child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .gap(px(8.0))
                        .p(px(8.0))
                        .rounded(px(6.0))
                        .bg(t.card_bg)
                        .border_1()
                        .border_color(t.card_border)
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .gap(px(2.0))
                                .child(
                                    div()
                                        .text_size(px(12.0))
                                        .font_weight(gpui::FontWeight::MEDIUM)
                                        .text_color(t.text_primary)
                                        .child(file_name),
                                )
                                .child(
                                    div()
                                        .text_size(px(11.0))
                                        .text_color(t.text_muted)
                                        .child(format!("{file_size_str} · {date_str}")),
                                ),
                        )
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(6.0))
                                .child(button_with_icon_l(
                                    gpui::SharedString::from(format!("local-restore-{}", file_name_for_restore_id)),
                                    crate::icons::REFRESH_SVG,
                                    i.t("恢复", "Restore"),
                                    ButtonVariant::Secondary,
                                    &t,
                                    cx,
                                    move |ws, _, _, cx| {
                                        let paths = ws.paths.clone();
                                        match aitoolplus_core::backup::restore_backup_with_options(
                                            &paths,
                                            &file_path_for_restore,
                                            &aitoolplus_core::backup::RestoreOptions {
                                                allow_custom_absolute: ws
                                                    .ui
                                                    .restore_allow_custom_absolute,
                                                conflict_strategy: ws
                                                    .ui
                                                    .restore_conflict_strategy,
                                            },
                                        ) {
                                            Ok(report) => {
                                                if let Ok(store) =
                                                    aitoolplus_core::store::StoreHandle::open(&paths)
                                                {
                                                    ws.store = store;
                                                }
                                                ws.settings =
                                                    aitoolplus_core::settings::AppSettings::load(
                                                        &paths.settings_file(),
                                                    );
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
                                            Err(e) => {
                                                ws.ui.toast(format!("恢复失败: {e}"), true);
                                            }
                                        }
                                        cx.notify();
                                    },
                                ))
                                .child(button_with_icon_l(
                                    gpui::SharedString::from(format!("local-save-as-{}", file_name_for_save_as_id)),
                                    crate::icons::DOWNLOAD_SVG,
                                    i.t("另存为", "Save As"),
                                    ButtonVariant::Secondary,
                                    &t,
                                    cx,
                                    move |_ws, _, _, cx| {
                                        let dialog = rfd::AsyncFileDialog::new()
                                            .set_file_name(&file_name_for_save_as_dialog)
                                            .add_filter("ZIP", &["zip"]);
                                        let src = file_path_for_save_as.clone();
                                        let weak = cx.entity().downgrade();
                                        cx.spawn(async move |_this, cx| {
                                            if let Some(file) = dialog.save_file().await {
                                                let dst = file.path().to_path_buf();
                                                let _ = std::fs::copy(&src, &dst);
                                                let _ = weak.update(cx, |ws, cx| {
                                                    ws.ui.toast(
                                                        ws.i18n.t("文件已另存为指定位置", "Saved as specified").to_string(),
                                                        false,
                                                    );
                                                    cx.notify();
                                                });
                                            }
                                        }).detach();
                                    },
                                ))
                                .child(button_with_icon_l(
                                    gpui::SharedString::from(format!("local-delete-{}", file_name_for_delete_id)),
                                    crate::icons::TRASH_SVG,
                                    i.t("删除", "Delete"),
                                    ButtonVariant::Danger,
                                    &t,
                                    cx,
                                    move |ws, _, _, cx| {
                                        let _ = std::fs::remove_file(&file_path_for_delete);
                                        ws.ui.toast(
                                            ws.i18n.t("本地备份文件已删除", "Backup file deleted").to_string(),
                                            false,
                                        );
                                        cx.notify();
                                    },
                                )),
                        ),
                );
            }
        }

        panel.child(list_section).into_any_element()
    });

    let webdav_panel = (backup_type == aitoolplus_core::settings::BackupType::Webdav).then(|| {
        let test_url = dav_url.clone();
        let test_user = dav_user.clone();
        let test_password = dav_password.clone();
        let test_directory = dav_directory.clone();

        let save_url = dav_url.clone();
        let save_user = dav_user.clone();
        let save_password = dav_password.clone();
        let save_directory = dav_directory.clone();

        let upload_url = dav_url.clone();
        let upload_user = dav_user.clone();
        let upload_password = dav_password.clone();
        let upload_directory = dav_directory.clone();

        let download_url = dav_url.clone();
        let download_user = dav_user.clone();
        let download_password = dav_password.clone();
        let download_directory = dav_directory.clone();

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
                    .flex_wrap()
                    .child(button_with_icon_l(
                        "webdav-test",
                        crate::icons::GLOBE_SVG,
                        i.t("测试连接", "Test Connection"),
                        ButtonVariant::Secondary,
                        &t,
                        cx,
                        move |ws, _, _, cx| {
                            let config = aitoolplus_core::settings::WebDavConfig {
                                url: test_url.update(cx, |input, _| input.text().trim().to_string()),
                                username: test_user.update(cx, |input, _| input.text().trim().to_string()),
                                password: test_password.update(cx, |input, _| input.text().to_string()),
                                remote_directory: test_directory.update(cx, |input, _| input.text().trim().to_string()),
                            };
                            match aitoolplus_core::webdav::test_connection(&config) {
                                Ok(()) => ws.ui.toast(
                                    ws.i18n.t("WebDAV 连接成功", "WebDAV connection succeeded").to_string(),
                                    false,
                                ),
                                Err(error) => ws.ui.toast(format!("WebDAV 连接失败: {error}"), true),
                            }
                            cx.notify();
                        },
                    ))
                    .child(button_with_icon_l(
                        "webdav-save",
                        crate::icons::CHECK_SVG,
                        i.t("保存设置", "Save Settings"),
                        ButtonVariant::Secondary,
                        &t,
                        cx,
                        move |ws, _, _, cx| {
                            ws.settings.webdav.url = save_url.update(cx, |input, _| input.text().trim().to_string());
                            ws.settings.webdav.username = save_user.update(cx, |input, _| input.text().trim().to_string());
                            ws.settings.webdav.password = save_password.update(cx, |input, _| input.text().to_string());
                            ws.settings.webdav.remote_directory = save_directory.update(cx, |input, _| input.text().trim().to_string());
                            (ws.callbacks.save_settings)(&ws.settings);
                            ws.ui.toast(ws.i18n.t("已保存", "saved").to_string(), false);
                            cx.notify();
                        },
                    ))
                    .child(button_with_icon_l(
                        "webdav-upload",
                        crate::icons::CLOUD_UPLOAD_SVG,
                        i.t("上传云端", "Upload to Cloud"),
                        ButtonVariant::Primary,
                        &t,
                        cx,
                        move |ws, _, _, cx| {
                            ws.settings.webdav.url = upload_url.update(cx, |input, _| input.text().trim().to_string());
                            ws.settings.webdav.username = upload_user.update(cx, |input, _| input.text().trim().to_string());
                            ws.settings.webdav.password = upload_password.update(cx, |input, _| input.text().to_string());
                            ws.settings.webdav.remote_directory = upload_directory.update(cx, |input, _| input.text().trim().to_string());
                            (ws.callbacks.save_settings)(&ws.settings);

                            let directory = ws.paths.app_data.join("backups").join("sync");
                            let _ = std::fs::create_dir_all(&directory);
                            let output = directory.join("aitoolplus-sync.zip");
                            match aitoolplus_core::backup::create_backup(&ws.paths, &ws.settings, &output) {
                                Ok(report) => {
                                    match aitoolplus_core::webdav::upload(&ws.settings.webdav, &report.output) {
                                        Ok(_) => {
                                            ws.ui.toast(
                                                ws.i18n.t("上传成功！已同步至 WebDAV 云端", "Upload succeeded: Synced to WebDAV cloud").to_string(),
                                                false,
                                            );
                                        }
                                        Err(e) => {
                                            ws.ui.toast(format!("上传到 WebDAV 失败: {e}"), true);
                                        }
                                    }
                                }
                                Err(e) => {
                                    ws.ui.toast(format!("创建备份失败: {e}"), true);
                                }
                            }
                            cx.notify();
                        },
                    ))
                    .child(button_with_icon_l(
                        "webdav-download",
                        crate::icons::CLOUD_DOWNLOAD_SVG,
                        i.t("从云端下载并恢复", "Download & Restore"),
                        ButtonVariant::Secondary,
                        &t,
                        cx,
                        move |ws, _, _, cx| {
                            ws.settings.webdav.url = download_url.update(cx, |input, _| input.text().trim().to_string());
                            ws.settings.webdav.username = download_user.update(cx, |input, _| input.text().trim().to_string());
                            ws.settings.webdav.password = download_password.update(cx, |input, _| input.text().to_string());
                            ws.settings.webdav.remote_directory = download_directory.update(cx, |input, _| input.text().trim().to_string());
                            (ws.callbacks.save_settings)(&ws.settings);

                            match aitoolplus_core::webdav::list(&ws.settings.webdav) {
                                Ok(backups) if backups.is_empty() => {
                                    ws.ui.toast(
                                        ws.i18n.t("云端暂无备份数据，请先上传", "No backup found in cloud, please upload first").to_string(),
                                        true,
                                    );
                                }
                                Ok(backups) => {
                                    let target_name = backups
                                        .iter()
                                        .find(|b| b.name == "aitoolplus-sync.zip")
                                        .map(|b| b.name.clone())
                                        .unwrap_or_else(|| backups[0].name.clone());

                                    let temporary = ws
                                        .paths
                                        .app_data
                                        .join("backups")
                                        .join("downloads")
                                        .join(&target_name);

                                    match aitoolplus_core::webdav::download(&ws.settings.webdav, &target_name, &temporary)
                                        .and_then(|path| {
                                            aitoolplus_core::backup::restore_backup_with_options(
                                                &ws.paths,
                                                &path,
                                                &aitoolplus_core::backup::RestoreOptions {
                                                    allow_custom_absolute: ws.ui.restore_allow_custom_absolute,
                                                    conflict_strategy: ws.ui.restore_conflict_strategy,
                                                },
                                            )
                                        })
                                    {
                                        Ok(report) => {
                                            if let Ok(store) = aitoolplus_core::store::StoreHandle::open(&ws.paths) {
                                                ws.store = store;
                                            }
                                            ws.settings = aitoolplus_core::settings::AppSettings::load(&ws.paths.settings_file());
                                            ws.ui.toast(
                                                ws.i18n.t(
                                                    &format!("云端恢复完成：已恢复 {} 个文件", report.restored),
                                                    &format!("Cloud restore complete: {} files restored", report.restored),
                                                ).to_string(),
                                                false,
                                            );
                                        }
                                        Err(error) => {
                                            ws.ui.toast(format!("云端下载恢复失败: {error}"), true);
                                        }
                                    }
                                }
                                Err(error) => {
                                    ws.ui.toast(format!("获取 WebDAV 云端备份失败: {error}"), true);
                                }
                            }
                            cx.notify();
                        },
                    )),
            )
            .into_any_element()
    });

    let s3_panel = (backup_type == aitoolplus_core::settings::BackupType::S3).then(|| {
        let test_endpoint = s3_endpoint.clone();
        let test_region = s3_region.clone();
        let test_bucket = s3_bucket.clone();
        let test_access_key = s3_access_key.clone();
        let test_secret_key = s3_secret_key.clone();
        let test_prefix = s3_prefix.clone();

        let save_endpoint = s3_endpoint.clone();
        let save_region = s3_region.clone();
        let save_bucket = s3_bucket.clone();
        let save_access_key = s3_access_key.clone();
        let save_secret_key = s3_secret_key.clone();
        let save_prefix = s3_prefix.clone();

        let upload_endpoint = s3_endpoint.clone();
        let upload_region = s3_region.clone();
        let upload_bucket = s3_bucket.clone();
        let upload_access_key = s3_access_key.clone();
        let upload_secret_key = s3_secret_key.clone();
        let upload_prefix = s3_prefix.clone();

        let download_endpoint = s3_endpoint.clone();
        let download_region = s3_region.clone();
        let download_bucket = s3_bucket.clone();
        let download_access_key = s3_access_key.clone();
        let download_secret_key = s3_secret_key.clone();
        let download_prefix = s3_prefix.clone();

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
                    .items_center()
                    .gap(px(12.0))
                    .child(div().flex_1().child(s3_prefix.clone()))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .child(
                                div()
                                    .text_size(px(12.5))
                                    .text_color(t.text_secondary)
                                    .child(i.t("路径模式 (Path-Style)", "Path-Style")),
                            )
                            .child(toggle(
                                "s3-path-style-toggle",
                                ws.settings.s3.path_style,
                                &t,
                                cx,
                                |ws, _, _, cx| {
                                    ws.settings.s3.path_style = !ws.settings.s3.path_style;
                                    (ws.callbacks.save_settings)(&ws.settings);
                                    cx.notify();
                                },
                            )),
                    ),
            )
            .child(
                div()
                    .flex()
                    .gap(px(8.0))
                    .flex_wrap()
                    .child(button_with_icon_l(
                        "s3-test",
                        crate::icons::GLOBE_SVG,
                        i.t("测试连接", "Test Connection"),
                        ButtonVariant::Secondary,
                        &t,
                        cx,
                        move |ws, _, _, cx| {
                            let config = aitoolplus_core::settings::S3Config {
                                endpoint: test_endpoint.update(cx, |input, _| input.text().trim().to_string()),
                                region: test_region.update(cx, |input, _| input.text().trim().to_string()),
                                bucket: test_bucket.update(cx, |input, _| input.text().trim().to_string()),
                                access_key_id: test_access_key.update(cx, |input, _| input.text().trim().to_string()),
                                secret_access_key: test_secret_key.update(cx, |input, _| input.text().to_string()),
                                prefix: test_prefix.update(cx, |input, _| input.text().trim().to_string()),
                                path_style: ws.settings.s3.path_style,
                            };
                            match aitoolplus_core::s3::test_connection(&config) {
                                Ok(()) => ws.ui.toast(
                                    ws.i18n.t("S3 连接成功", "S3 connection succeeded").to_string(),
                                    false,
                                ),
                                Err(error) => ws.ui.toast(format!("S3 连接失败: {error}"), true),
                            }
                            cx.notify();
                        },
                    ))
                    .child(button_with_icon_l(
                        "s3-save",
                        crate::icons::CHECK_SVG,
                        i.t("保存设置", "Save Settings"),
                        ButtonVariant::Secondary,
                        &t,
                        cx,
                        move |ws, _, _, cx| {
                            ws.settings.s3.endpoint = save_endpoint.update(cx, |input, _| input.text().trim().to_string());
                            ws.settings.s3.region = save_region.update(cx, |input, _| input.text().trim().to_string());
                            ws.settings.s3.bucket = save_bucket.update(cx, |input, _| input.text().trim().to_string());
                            ws.settings.s3.access_key_id = save_access_key.update(cx, |input, _| input.text().trim().to_string());
                            ws.settings.s3.secret_access_key = save_secret_key.update(cx, |input, _| input.text().to_string());
                            ws.settings.s3.prefix = save_prefix.update(cx, |input, _| input.text().trim().to_string());
                            (ws.callbacks.save_settings)(&ws.settings);
                            ws.ui.toast(ws.i18n.t("已保存", "saved").to_string(), false);
                            cx.notify();
                        },
                    ))
                    .child(button_with_icon_l(
                        "s3-upload",
                        crate::icons::CLOUD_UPLOAD_SVG,
                        i.t("上传云端", "Upload to Cloud"),
                        ButtonVariant::Primary,
                        &t,
                        cx,
                        move |ws, _, _, cx| {
                            ws.settings.s3.endpoint = upload_endpoint.update(cx, |input, _| input.text().trim().to_string());
                            ws.settings.s3.region = upload_region.update(cx, |input, _| input.text().trim().to_string());
                            ws.settings.s3.bucket = upload_bucket.update(cx, |input, _| input.text().trim().to_string());
                            ws.settings.s3.access_key_id = upload_access_key.update(cx, |input, _| input.text().trim().to_string());
                            ws.settings.s3.secret_access_key = upload_secret_key.update(cx, |input, _| input.text().to_string());
                            ws.settings.s3.prefix = upload_prefix.update(cx, |input, _| input.text().trim().to_string());
                            (ws.callbacks.save_settings)(&ws.settings);

                            let directory = ws.paths.app_data.join("backups").join("sync");
                            let _ = std::fs::create_dir_all(&directory);
                            let output = directory.join("aitoolplus-sync.zip");
                            match aitoolplus_core::backup::create_backup(&ws.paths, &ws.settings, &output) {
                                Ok(report) => {
                                    match aitoolplus_core::s3::upload(&ws.settings.s3, &report.output) {
                                        Ok(_) => {
                                            ws.ui.toast(
                                                ws.i18n.t("上传成功！已同步至 S3 云端", "Upload succeeded: Synced to S3 cloud").to_string(),
                                                false,
                                            );
                                        }
                                        Err(e) => {
                                            ws.ui.toast(format!("上传到 S3 失败: {e}"), true);
                                        }
                                    }
                                }
                                Err(e) => {
                                    ws.ui.toast(format!("创建备份失败: {e}"), true);
                                }
                            }
                            cx.notify();
                        },
                    ))
                    .child(button_with_icon_l(
                        "s3-download",
                        crate::icons::CLOUD_DOWNLOAD_SVG,
                        i.t("从云端下载并恢复", "Download & Restore"),
                        ButtonVariant::Secondary,
                        &t,
                        cx,
                        move |ws, _, _, cx| {
                            ws.settings.s3.endpoint = download_endpoint.update(cx, |input, _| input.text().trim().to_string());
                            ws.settings.s3.region = download_region.update(cx, |input, _| input.text().trim().to_string());
                            ws.settings.s3.bucket = download_bucket.update(cx, |input, _| input.text().trim().to_string());
                            ws.settings.s3.access_key_id = download_access_key.update(cx, |input, _| input.text().trim().to_string());
                            ws.settings.s3.secret_access_key = download_secret_key.update(cx, |input, _| input.text().to_string());
                            ws.settings.s3.prefix = download_prefix.update(cx, |input, _| input.text().trim().to_string());
                            (ws.callbacks.save_settings)(&ws.settings);

                            match aitoolplus_core::s3::list(&ws.settings.s3) {
                                Ok(backups) if backups.is_empty() => {
                                    ws.ui.toast(
                                        ws.i18n.t("云端暂无备份数据，请先上传", "No backup found in cloud, please upload first").to_string(),
                                        true,
                                    );
                                }
                                Ok(backups) => {
                                    let target_name = backups
                                        .iter()
                                        .find(|b| b.name == "aitoolplus-sync.zip")
                                        .map(|b| b.name.clone())
                                        .unwrap_or_else(|| backups[0].name.clone());

                                    let temporary = ws
                                        .paths
                                        .app_data
                                        .join("backups")
                                        .join("downloads")
                                        .join(&target_name);

                                    match aitoolplus_core::s3::download(&ws.settings.s3, &target_name, &temporary)
                                        .and_then(|path| {
                                            aitoolplus_core::backup::restore_backup_with_options(
                                                &ws.paths,
                                                &path,
                                                &aitoolplus_core::backup::RestoreOptions {
                                                    allow_custom_absolute: ws.ui.restore_allow_custom_absolute,
                                                    conflict_strategy: ws.ui.restore_conflict_strategy,
                                                },
                                            )
                                        })
                                    {
                                        Ok(report) => {
                                            if let Ok(store) = aitoolplus_core::store::StoreHandle::open(&ws.paths) {
                                                ws.store = store;
                                            }
                                            ws.settings = aitoolplus_core::settings::AppSettings::load(&ws.paths.settings_file());
                                            ws.ui.toast(
                                                ws.i18n.t(
                                                    &format!("云端恢复完成：已恢复 {} 个文件", report.restored),
                                                    &format!("Cloud restore complete: {} files restored", report.restored),
                                                ).to_string(),
                                                false,
                                            );
                                        }
                                        Err(error) => {
                                            ws.ui.toast(format!("云端下载恢复失败: {error}"), true);
                                        }
                                    }
                                }
                                Err(error) => {
                                    ws.ui.toast(format!("获取 S3 云端备份失败: {error}"), true);
                                }
                            }
                            cx.notify();
                        },
                    )),
            )
            .into_any_element()
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
            local_panel.unwrap_or_else(|| div().into_any_element()),
            webdav_panel.unwrap_or_else(|| div().into_any_element()),
            s3_panel.unwrap_or_else(|| div().into_any_element()),
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
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(8.0))
                                .child(
                                    div()
                                        .text_size(px(12.5))
                                        .text_color(t.text_secondary)
                                        .child(i.t("允许原绝对路径", "Custom Absolute")),
                                )
                                .child(toggle(
                                    "restore-custom-absolute-toggle",
                                    ws.ui.restore_allow_custom_absolute,
                                    &t,
                                    cx,
                                    |ws, _, _, cx| {
                                        ws.ui.restore_allow_custom_absolute =
                                            !ws.ui.restore_allow_custom_absolute;
                                        cx.notify();
                                    },
                                )),
                        ),
                )
                .into_any_element(),
            filter_panel.into_any_element(),
            custom_panel.into_any_element(),
            div()
                .flex()
                .items_center()
                .justify_between()
                .w_full()
                .gap(px(16.0))
                .child(section_title(
                    &t,
                    i.t("包含 CLI 配置", "Include CLI Configs"),
                    Some(i.t(
                        "包含各工具配置、Prompt、MCP 和插件状态",
                        "Include tool configs, prompts, MCP and plugin state",
                    )),
                ))
                .child(toggle(
                    "backup-cli-toggle",
                    ws.settings.backup_cli_config_files_enabled,
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
                .items_center()
                .justify_between()
                .w_full()
                .gap(px(16.0))
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
                .child(toggle(
                    "auto-backup-toggle",
                    ws.settings.auto_backup_enabled,
                    &t,
                    cx,
                    |ws, _, _, cx| {
                        ws.settings.auto_backup_enabled = !ws.settings.auto_backup_enabled;
                        (ws.callbacks.save_settings)(&ws.settings);
                        cx.notify();
                    },
                ))
                .into_any_element(),
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
                        "Rust + GPUI 原生桌面应用，全面对标 cc-switch 与 ai-toolbox",
                        "Native Rust + GPUI desktop app, matching cc-switch and ai-toolbox",
                    )),
            )
            .into_any_element(),
        div()
            .flex()
            .flex_col()
            .gap(px(8.0))
            .p(px(10.0))
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
                            .text_size(px(12.5))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(t.text_primary)
                            .child(i.t("数据存储模式：", "Data Storage Mode:")),
                    )
                    .child(if ws.paths.is_portable() {
                        crate::components::badge(
                            &t,
                            i.t("便携模式：已激活（保存在应用同级 data/ 目录）", "Portable: Active (app data/ folder)"),
                            crate::components::BadgeKind::Success,
                        )
                    } else {
                        crate::components::badge(
                            &t,
                            i.t("系统模式（保存在用户 AppData）", "Standard Mode (%APPDATA%)"),
                            crate::components::BadgeKind::Neutral,
                        )
                    }),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .flex_wrap()
                    .child(button_l(
                        "open-app-data-dir",
                        i.t("打开应用数据目录", "Open AppData Dir"),
                        ButtonVariant::Secondary,
                        &t,
                        cx,
                        |ws, _, _, _| {
                            let dir = &ws.paths.app_data;
                            let _ = std::fs::create_dir_all(dir);
                            let _ = std::process::Command::new("explorer").arg(dir).spawn();
                        },
                    ))
                    .child(button_l(
                        "open-backups-dir",
                        i.t("打开备份存储目录", "Open Backups Dir"),
                        ButtonVariant::Secondary,
                        &t,
                        cx,
                        |ws, _, _, _| {
                            let dir = ws.paths.app_data.join("backups");
                            let _ = std::fs::create_dir_all(&dir);
                            let _ = std::process::Command::new("explorer").arg(&dir).spawn();
                        },
                    ))
                    .child(button_l(
                        "open-user-home-dir",
                        i.t("打开配置根目录", "Open Config Root"),
                        ButtonVariant::Secondary,
                        &t,
                        cx,
                        |ws, _, _, _| {
                            let _ = std::process::Command::new("explorer").arg(&ws.paths.home).spawn();
                        },
                    )),
            )
            .into_any_element(),
        div()
            .flex()
            .items_center()
            .justify_between()
            .w_full()
            .gap(px(16.0))
            .child(section_title(
                &t,
                i.t("自动检查更新", "Automatic Update Check"),
                Some(i.t(
                    "启动后可检查 GitHub Releases；不会静默安装",
                    "Check GitHub Releases at startup; never installs silently",
                )),
            ))
            .child(toggle(
                "auto-update-check-toggle",
                ws.settings.auto_update_check_enabled,
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
