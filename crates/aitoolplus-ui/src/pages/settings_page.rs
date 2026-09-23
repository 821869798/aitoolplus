//! Settings page: general, backup, about.

use gpui::{Context, IntoElement, div, prelude::*, px};

use crate::components::{
    ButtonVariant, button_l, button_with_icon_l, card, input_container, section_title,
    segmented_pill_selector, settings_card, settings_row, toggle,
};
use crate::text_input::TextInput;
use crate::workspace::Workspace;

use super::SettingsTab;

pub fn render_settings_page(ws: &mut Workspace, cx: &mut Context<Workspace>) -> gpui::AnyElement {
    let t = ws.theme.clone();
    let i = ws.i18n;

    let tabs = [
        (SettingsTab::General, i.t("通用", "General")),
        (SettingsTab::DataImport, i.t("数据导入", "Data Import")),
        (SettingsTab::Usage, i.t("使用统计", "Usage Statistics")),
        (SettingsTab::Backup, i.t("备份", "Backup")),
        (SettingsTab::Advanced, i.t("高级选项", "Advanced")),
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
        SettingsTab::DataImport => data_import_tab(ws, cx),
        SettingsTab::Usage => super::usage_page::render_usage_page(ws, cx),
        SettingsTab::Backup => backup_tab(ws, cx),
        SettingsTab::Advanced => advanced_tab(ws, cx),
        SettingsTab::About => about_tab(ws, cx),
    };

    let max_width = if ws.ui.settings_tab == SettingsTab::Usage {
        px(1040.0)
    } else {
        px(880.0)
    };

    div()
        .flex()
        .flex_col()
        .w_full()
        .max_w(max_width)
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

struct LocalBackupEntry {
    path: std::path::PathBuf,
    name: String,
    size_bytes: u64,
    date_str: String,
}

fn list_local_backups(paths: &aitoolplus_core::paths::Paths) -> Vec<LocalBackupEntry> {
    let snapshots_dir = paths.local_snapshots_dir();
    let _ = std::fs::create_dir_all(&snapshots_dir);

    // Auto-migrate any legacy backup files into the dedicated snapshots directory
    let backup_dir = paths.app_data.join("backups");
    let legacy_dirs = [
        backup_dir.join("manual"),
        backup_dir.join("auto"),
        backup_dir.join("automatic"),
    ];
    for dir in legacy_dirs {
        if let Ok(read_dir) = std::fs::read_dir(&dir) {
            for entry in read_dir.flatten() {
                let path = entry.path();
                if path.is_file() && path.extension().and_then(|e| e.to_str()) == Some("zip") {
                    if let Some(name) = path.file_name() {
                        let target = snapshots_dir.join(name);
                        if !target.exists() {
                            let _ = std::fs::rename(&path, &target);
                        }
                    }
                }
            }
        }
    }
    // Also migrate loose snapshots directly in backups/
    if let Ok(read_dir) = std::fs::read_dir(&backup_dir) {
        for entry in read_dir.flatten() {
            let path = entry.path();
            if path.is_file() && path.extension().and_then(|e| e.to_str()) == Some("zip") {
                let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                if file_name.starts_with("aitoolplus-backup-") || file_name.starts_with("aitoolplus-auto-") {
                    let target = snapshots_dir.join(file_name);
                    if !target.exists() {
                        let _ = std::fs::rename(&path, &target);
                    }
                }
            }
        }
    }

    let mut entries = Vec::new();
    if let Ok(read_dir) = std::fs::read_dir(&snapshots_dir) {
        for entry in read_dir.flatten() {
            let path = entry.path();
            if path.is_file() && path.extension().and_then(|e| e.to_str()) == Some("zip") {
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
    entries.sort_by(|a, b| {
        let meta_b = b.path.metadata().and_then(|m| m.modified()).ok();
        let meta_a = a.path.metadata().and_then(|m| m.modified()).ok();
        meta_b.cmp(&meta_a).then_with(|| b.name.cmp(&a.name))
    });
    entries
}

fn get_backup_display_name(filename: &str) -> String {
    if let Some(rest) = filename.strip_prefix("aitoolplus-backup-").and_then(|s| s.strip_suffix(".zip")) {
        if rest.len() == 15 && rest.chars().nth(8) == Some('-') {
            let (d, t) = rest.split_at(8);
            let t = &t[1..];
            if d.len() == 8 && t.len() == 6 {
                return format!("{}-{}-{} {}:{}:{} (快照)", &d[0..4], &d[4..6], &d[6..8], &t[0..2], &t[2..4], &t[4..6]);
            }
        }
    }
    if let Some(rest) = filename.strip_prefix("aitoolplus-auto-").and_then(|s| s.strip_suffix(".zip")) {
        if rest.len() == 15 && rest.chars().nth(8) == Some('-') {
            let (d, t) = rest.split_at(8);
            let t = &t[1..];
            if d.len() == 8 && t.len() == 6 {
                return format!("{}-{}-{} {}:{}:{} (自动)", &d[0..4], &d[4..6], &d[6..8], &t[0..2], &t[2..4], &t[4..6]);
            }
        }
    }
    filename.strip_suffix(".zip").unwrap_or(filename).to_string()
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

fn post_restore_reconcile(ws: &mut Workspace) {
    let paths = ws.paths.clone();
    if let Ok(mut store) = aitoolplus_core::store::StoreHandle::open(&paths) {
        let mut skills_store = store.store().skills.clone();
        // 1. Automatically discover and preserve any pre-existing skills on this computer
        let _ = aitoolplus_core::skills::scan_and_import_existing(&paths, &mut skills_store);
        // 2. Re-create all Junctions/Symlinks for all tools (including .agents/skills)
        let _ = aitoolplus_core::skills::sync_all(&mut skills_store, &paths);
        let _ = store.update(|db| db.skills = skills_store);
        ws.store = store;
    }
    ws.settings = aitoolplus_core::settings::AppSettings::load(&paths.settings_file());
    ws.ui.skills_discovered = true;
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

        // 1. Policy Settings (Auto-backup Interval & Retain Count)
        let interval_val = ws.settings.backup_interval_hours;
        let interval_selector = segmented_pill_selector(
            "backup-interval",
            vec![
                (0u32, None, i.t("禁用", "Disabled")),
                (1u32, None, i.t("1小时", "1h")),
                (6u32, None, i.t("6小时", "6h")),
                (12u32, None, i.t("12小时", "12h")),
                (24u32, None, i.t("24小时", "24h")),
                (168u32, None, i.t("7天", "7d")),
            ],
            interval_val,
            &t,
            cx,
            |ws, val, _, cx| {
                ws.settings.backup_interval_hours = val;
                ws.settings.auto_backup_enabled = val > 0;
                (ws.callbacks.save_settings)(&ws.settings);
                cx.notify();
            },
        );

        let retain_val = ws.settings.backup_retain_count;
        let retain_selector = segmented_pill_selector(
            "backup-retain",
            vec![
                (5usize, None, i.t("5个", "5")),
                (10usize, None, i.t("10个", "10")),
                (20usize, None, i.t("20个", "20")),
                (50usize, None, i.t("50个", "50")),
            ],
            retain_val,
            &t,
            cx,
            |ws, val, _, cx| {
                ws.settings.backup_retain_count = val;
                ws.settings.auto_backup_max_keep = val as u32;
                (ws.callbacks.save_settings)(&ws.settings);
                cx.notify();
            },
        );

        let policy_section = div()
            .flex()
            .flex_col()
            .gap(px(8.0))
            .child(settings_row(
                &t,
                i.t("自动备份间隔", "Auto-backup Interval"),
                Some(i.t(
                    "定时自动在本地创建快照的时间间隔（设为禁用则关闭自动备份）",
                    "Time interval for auto backup (Disabled to turn off)",
                )),
                interval_selector,
            ))
            .child(settings_row(
                &t,
                i.t("备份保留数量", "Backup Retention"),
                Some(i.t(
                    "本地自动与手动快照最多保留的数量，超出将自动清理最旧快照",
                    "Maximum number of backup snapshots to keep before pruning oldest",
                )),
                retain_selector,
            ));

        // 2. Action buttons row: 立即备份、从zip恢复备份、打开备份目录
        let buttons_row = div()
            .flex()
            .gap(px(8.0))
            .flex_wrap()
            .child(button_with_icon_l(
                "backup-create-now",
                crate::icons::DOWNLOAD_SVG,
                i.t("立即备份", "Backup Now"),
                ButtonVariant::Primary,
                &t,
                cx,
                |ws, _, _, cx| {
                    let dir = ws.paths.local_snapshots_dir();
                    let _ = std::fs::create_dir_all(&dir);
                    let file_name = format!(
                        "aitoolplus-backup-{}.zip",
                        chrono::Local::now().format("%Y%m%d-%H%M%S")
                    );
                    let output = dir.join(&file_name);
                    match aitoolplus_core::backup::create_backup(&ws.paths, &ws.settings, &output) {
                        Ok(report) => {
                            let retain = ws.settings.backup_retain_count.max(1);
                            let _ = aitoolplus_core::backup::prune_local_backups(&dir, retain);
                            ws.ui.toast(
                                ws.i18n.t(
                                    &format!("备份创建成功：已保存至 {}（{} 个文件）", file_name, report.file_count),
                                    &format!("Backup created: {} ({} files)", file_name, report.file_count),
                                ).to_string(),
                                false,
                            );
                        }
                        Err(e) => {
                            ws.ui.toast(format!("创建备份失败: {e}"), true);
                        }
                    }
                    cx.notify();
                },
            ))
            .child(button_with_icon_l(
                "backup-restore-zip",
                crate::icons::UPLOAD_SVG,
                i.t("从zip恢复备份", "Restore from ZIP Backup"),
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
                                        post_restore_reconcile(ws);
                                        ws.ui.toast(
                                            ws.i18n
                                                .t(
                                                    &format!(
                                                        "从 ZIP 恢复完成：已恢复 {} 个文件 (覆盖 {}, 副本 {})",
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
                                        ws.ui.toast(format!("从 ZIP 恢复失败: {e}"), true);
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
                    let dir = ws.paths.local_snapshots_dir();
                    let _ = std::fs::create_dir_all(&dir);
                    let _ = open_dir_in_explorer(&dir);
                },
            ));

        // 3. Backup list section
        let mut list_section = div().flex().flex_col().gap(px(6.0)).child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .child(
                    div()
                        .text_size(px(12.5))
                        .font_weight(gpui::FontWeight::MEDIUM)
                        .text_color(t.text_primary)
                        .child(i.t("本地备份列表", "Local Backup List")),
                )
                .child(
                    div()
                        .text_size(px(11.0))
                        .text_color(t.text_muted)
                        .child(format!("{} {}", local_backups.len(), i.t("个快照", "snapshots"))),
                ),
        );

        if local_backups.is_empty() {
            list_section = list_section.child(
                div()
                    .text_size(px(11.5))
                    .text_color(t.text_muted)
                    .p(px(8.0))
                    .child(i.t(
                        "暂无本地备份文件，点击上方【立即备份】即可创建首个快照",
                        "No local backups found. Click 'Backup Now' to create one.",
                    )),
            );
        } else {
            for b in local_backups {
                let file_path_for_restore = b.path.clone();
                let file_path_for_rename = b.path.clone();
                let file_path_for_save_as = b.path.clone();
                let file_path_for_delete = b.path.clone();
                let file_name = b.name.clone();
                let display_title = get_backup_display_name(&file_name);
                let file_name_for_restore_id = b.name.clone();
                let file_name_for_rename_id = b.name.clone();
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
                                        .child(display_title),
                                )
                                .child(
                                    div()
                                        .text_size(px(11.0))
                                        .text_color(t.text_muted)
                                        .child(format!("{file_size_str} · {date_str} · {file_name}")),
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
                                                post_restore_reconcile(ws);
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
                                    gpui::SharedString::from(format!("local-rename-{}", file_name_for_rename_id)),
                                    crate::icons::PENCIL_SVG,
                                    i.t("重命名", "Rename"),
                                    ButtonVariant::Secondary,
                                    &t,
                                    cx,
                                    move |ws, _, _, cx| {
                                        let stem = file_name.strip_suffix(".zip").unwrap_or(&file_name).to_string();
                                        let input = cx.new(|cx| TextInput::new(stem, cx));
                                        ws.ui.backup_rename_dialog = Some((file_path_for_rename.clone(), input));
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

        div()
            .flex()
            .flex_col()
            .gap(px(12.0))
            .p(px(12.0))
            .rounded(px(8.0))
            .bg(t.input_bg)
            .border_1()
            .border_color(t.card_border)
            .child(section_title(
                &t,
                i.t("本地快照", "Local Snapshots"),
                Some(i.t(
                    "配置自动备份策略，创建本地快照与从 ZIP 恢复数据",
                    "Configure auto-backup policy, create snapshot and restore",
                )),
            ))
            .child(policy_section)
            .child(buttons_row)
            .child(list_section)
            .into_any_element()
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
                                            post_restore_reconcile(ws);
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
                                            post_restore_reconcile(ws);
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

    let backup_card = card(
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
        ],
    );

    div()
        .flex()
        .flex_col()
        .w_full()
        .gap(px(16.0))
        .child(backup_card)
        .into_any_element()
}

fn data_import_tab(ws: &mut Workspace, cx: &mut Context<Workspace>) -> gpui::AnyElement {
    div()
        .flex()
        .flex_col()
        .w_full()
        .gap(px(16.0))
        .child(cc_switch_migration_card(ws, cx))
        .child(json_config_transfer_card(ws, cx))
        .into_any_element()
}

fn cc_switch_migration_card(ws: &mut Workspace, cx: &mut Context<Workspace>) -> gpui::AnyElement {
    let t = ws.theme.clone();
    let i = ws.i18n;

    let custom_path = ws.ui.cc_switch_custom_db_path.clone();
    let detected_path = aitoolplus_core::cc_switch_import::detect_cc_switch_db(&ws.paths);
    let active_path = custom_path.clone().or_else(|| detected_path.clone());
    let is_detected = active_path.as_ref().map(|p| p.is_file()).unwrap_or(false);
    let display_path = active_path
        .as_ref()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| aitoolplus_core::cc_switch_import::default_cc_switch_db_path(&ws.paths).display().to_string());

    let status_badge = if is_detected {
        div()
            .flex()
            .items_center()
            .gap(px(6.0))
            .child(
                div()
                    .w(px(8.0))
                    .h(px(8.0))
                    .rounded_full()
                    .bg(crate::rgba_const(0x22c55eff)),
            )
            .child(
                div()
                    .text_size(px(12.0))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .text_color(crate::rgba_const(0x22c55eff))
                    .child(if custom_path.is_some() {
                        i.t("已指定 CC-Switch 数据库文件", "CC-Switch database file selected")
                    } else {
                        i.t("已检测到 CC-Switch 数据库", "CC-Switch database detected")
                    }),
            )
    } else {
        div()
            .flex()
            .items_center()
            .gap(px(6.0))
            .child(
                div()
                    .w(px(8.0))
                    .h(px(8.0))
                    .rounded_full()
                    .bg(crate::rgba_const(0xef4444ff)),
            )
            .child(
                div()
                    .text_size(px(12.0))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .text_color(crate::rgba_const(0xef4444ff))
                    .child(i.t("未检测到 CC-Switch 数据库", "CC-Switch database not detected")),
            )
    };

    let import_target_path = active_path.clone();
    let import_usage_target_path = active_path.clone();

    let header_row = div()
        .flex()
        .items_center()
        .justify_between()
        .child(status_badge)
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(8.0))
                .child(
                    button_with_icon_l(
                        "select-cc-switch-db",
                        crate::icons::FOLDER_SVG,
                        i.t("选择数据库文件", "Select Database File"),
                        ButtonVariant::Secondary,
                        &t,
                        cx,
                        |_ws, _, _, cx| {
                            let dialog = rfd::AsyncFileDialog::new()
                                .add_filter("SQLite Database", &["db", "sqlite", "sqlite3"])
                                .set_title("选择 CC-Switch 数据库文件");
                            let weak = cx.entity().downgrade();
                            cx.spawn(async move |_this, cx| {
                                if let Some(file) = dialog.pick_file().await {
                                    let path = file.path().to_path_buf();
                                    let _ = weak.update(cx, |ws: &mut Workspace, cx| {
                                        ws.ui.cc_switch_custom_db_path = Some(path);
                                        cx.notify();
                                    });
                                }
                            })
                            .detach();
                        },
                    ),
                )
                .child(
                    button_with_icon_l(
                        "import-cc-switch-action",
                        crate::icons::DOWNLOAD_SVG,
                        i.t("导入供应商配置", "Import Providers"),
                        ButtonVariant::Primary,
                        &t,
                        cx,
                        move |ws, _, _, cx| {
                            let target_path = ws
                                .ui
                                .cc_switch_custom_db_path
                                .clone()
                                .or_else(|| import_target_path.clone())
                                .or_else(|| aitoolplus_core::cc_switch_import::detect_cc_switch_db(&ws.paths));
                            match aitoolplus_core::cc_switch_import::import_from_cc_switch(
                                &ws.paths,
                                ws.store.store_mut(),
                                target_path.as_deref(),
                            ) {
                                Ok(report) => {
                                    ws.persist_store();
                                    let msg = ws
                                        .i18n
                                        .t(
                                            &format!(
                                                "CC-Switch 供应商导入完成：发现 {} 个，新增 {} 个，更新 {} 个供应商",
                                                report.total_found, report.imported_count, report.updated_count
                                            ),
                                            &format!(
                                                "CC-Switch providers imported: {} found, {} added, {} updated",
                                                report.total_found, report.imported_count, report.updated_count
                                            ),
                                        )
                                        .to_string();
                                    ws.ui.toast(msg, false);
                                }
                                Err(e) => {
                                    let msg = format!("CC-Switch 导入失败: {e}");
                                    ws.ui.toast(msg, true);
                                }
                            }
                            cx.notify();
                        },
                    ),
                )
                .child(
                    button_with_icon_l(
                        "import-cc-switch-usage-action",
                        crate::icons::DATABASE_SVG,
                        i.t("导入使用统计与定价", "Import Usage & Pricing"),
                        ButtonVariant::Secondary,
                        &t,
                        cx,
                        move |ws, _, _, cx| {
                            let target_path = import_usage_target_path.clone()
                                .or_else(|| aitoolplus_core::cc_switch_import::detect_cc_switch_db(&ws.paths));
                            if let Some(path) = target_path.as_deref() {
                                if let Some(db) = ws.ensure_usage_db() {
                                    match db.import_from_cc_switch(path) {
                                        Ok(rep) => {
                                            ws.refresh_usage_data();
                                            let msg = format!(
                                                "使用统计迁移完成：导入 {} 条请求日志、{} 条模型定价、{} 条汇总数据",
                                                rep.logs_imported, rep.pricing_imported, rep.rollups_imported
                                            );
                                            ws.ui.toast(msg, false);
                                        }
                                        Err(e) => {
                                            ws.ui.toast(format!("导入使用统计失败: {e}"), true);
                                        }
                                    }
                                } else {
                                    ws.ui.toast("数据库初始化失败".to_string(), true);
                                }
                            } else {
                                ws.ui.toast("未检测到 CC-Switch 数据库文件".to_string(), true);
                            }
                            cx.notify();
                        },
                    ),
                ),
        );

    let path_info = div()
        .flex()
        .items_center()
        .gap(px(8.0))
        .p(px(10.0))
        .rounded(px(6.0))
        .bg(t.input_bg)
        .border_1()
        .border_color(t.card_border)
        .child(
            div()
                .text_size(px(12.0))
                .text_color(t.text_secondary)
                .child(i.t("数据库文件：", "Database file: ")),
        )
        .child(
            div()
                .flex_1()
                .text_size(px(12.0))
                .text_color(t.text_primary)
                .child(display_path),
        );

    let description_points = div()
        .flex()
        .flex_col()
        .gap(px(4.0))
        .text_size(px(11.5))
        .text_color(t.text_muted)
        .child(i.t(
            "• 支持一键导入 Claude Code、Codex、Pi、OpenCode、Gemini CLI 等所有工具的供应商配置",
            "• Supports importing provider configurations for Claude Code, Codex, Pi, OpenCode, Gemini CLI, etc.",
        ))
        .child(i.t(
            "• 支持将 CC-Switch 历史请求日志（3万+条）、详细 Token 消耗及模型计费定价完整迁移至 AI ToolPlus",
            "• Seamlessly migrates CC-Switch historical request logs, token analytics, and model pricing",
        ))
        .child(i.t(
            "• 安全增量合并机制，不会覆盖或删除您在 AI ToolPlus 中现有的自定义改动",
            "• Safe incremental merge: will not overwrite or delete your existing custom modifications in AI ToolPlus",
        ));

    settings_card(
        &t,
        i.t("CC-Switch 数据迁移与导入", "CC-Switch Data Migration & Import"),
        Some(i.t(
            "从本地 CC-Switch (cc-switch.db) 自动同步迁移模型供应商、使用统计与模型定价配置",
            "Migrate model providers, usage analytics, and pricing from local CC-Switch (cc-switch.db)",
        )),
        vec![
            div()
                .flex()
                .flex_col()
                .gap(px(10.0))
                .p(px(12.0))
                .child(header_row)
                .child(path_info)
                .child(description_points)
                .into_any_element(),
        ],
    )
}

fn json_config_transfer_card(ws: &mut Workspace, cx: &mut Context<Workspace>) -> gpui::AnyElement {
    let t = ws.theme.clone();
    let i = ws.i18n;

    let export_btn = button_with_icon_l(
        "export-json-config-btn",
        crate::icons::UPLOAD_SVG,
        i.t("导出配置 (JSON)", "Export Config (JSON)"),
        ButtonVariant::Secondary,
        &t,
        cx,
        |ws, _, _, cx| {
            let dialog = rfd::AsyncFileDialog::new()
                .add_filter("JSON Config", &["json"])
                .set_file_name("aitoolplus-config.json")
                .set_title("导出配置文件");
            let store_file = ws.paths.store_file();
            let weak = cx.entity().downgrade();
            cx.spawn(async move |_this, cx| {
                if let Some(file) = dialog.save_file().await {
                    let path = file.path().to_path_buf();
                    let res = if store_file.is_file() {
                        std::fs::copy(&store_file, &path).map(|_| ()).map_err(|e| e.to_string())
                    } else {
                        Err("配置存储文件不存在".to_string())
                    };
                    let _ = weak.update(cx, |ws: &mut Workspace, cx| {
                        match res {
                            Ok(_) => ws.ui.toast("配置已成功导出".to_string(), false),
                            Err(e) => ws.ui.toast(format!("导出失败: {e}"), true),
                        }
                        cx.notify();
                    });
                }
            })
            .detach();
        },
    );

    let import_btn = button_with_icon_l(
        "import-json-config-btn",
        crate::icons::DOWNLOAD_SVG,
        i.t("导入配置 (JSON)", "Import Config (JSON)"),
        ButtonVariant::Secondary,
        &t,
        cx,
        |ws, _, _, cx| {
            let dialog = rfd::AsyncFileDialog::new()
                .add_filter("JSON Config", &["json"])
                .set_title("导入配置文件");
            let store_file = ws.paths.store_file();
            let weak = cx.entity().downgrade();
            cx.spawn(async move |_this, cx| {
                if let Some(file) = dialog.pick_file().await {
                    let path = file.path().to_path_buf();
                    let res = if path.is_file() {
                        std::fs::copy(&path, &store_file).map(|_| ()).map_err(|e| e.to_string())
                    } else {
                        Err("文件不存在".to_string())
                    };
                    let _ = weak.update(cx, |ws: &mut Workspace, cx| {
                        match res {
                            Ok(_) => {
                                if let Ok(content) = std::fs::read_to_string(&store_file) {
                                    if let Ok(new_store) = serde_json::from_str::<aitoolplus_core::store::Store>(&content) {
                                        let _ = ws.store.update(|db| *db = new_store);
                                    }
                                }
                                ws.ui.toast("配置已成功导入并刷新".to_string(), false);
                            }
                            Err(e) => ws.ui.toast(format!("导入失败: {e}"), true),
                        }
                        cx.notify();
                    });
                }
            })
            .detach();
        },
    );

    settings_card(
        &t,
        i.t("配置文件导入与导出", "Config Import & Export"),
        Some(i.t(
            "将 AI ToolPlus 的全量供应商配置、模型设置与环境参数导出为 JSON，或从现有 JSON 恢复",
            "Export all AI ToolPlus providers and settings to JSON, or restore from a JSON file",
        )),
        vec![
            div()
                .flex()
                .items_center()
                .justify_between()
                .p(px(12.0))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(2.0))
                        .child(
                            div()
                                .text_size(px(13.0))
                                .font_weight(gpui::FontWeight::MEDIUM)
                                .text_color(t.text_primary)
                                .child(i.t("单文件配置迁移", "Single File Config Transfer")),
                        )
                        .child(
                            div()
                                .text_size(px(11.5))
                                .text_color(t.text_muted)
                                .child(i.t("适用于跨机器快速同步或备份配置", "Ideal for quick backup or migrating settings between devices")),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(8.0))
                        .child(export_btn)
                        .child(import_btn),
                )
                .into_any_element(),
        ],
    )
}

fn cli_policies_card(
    ws: &Workspace,
    t: &crate::theme::Theme,
    i: &crate::i18n::I18n,
    cx: &mut Context<Workspace>,
) -> gpui::AnyElement {
    settings_card(
        t,
        i.t("CLI 运行与认证策略", "CLI Launch & Auth Policies"),
        Some(i.t(
            "针对各命令行工具的环境变量与运行时配置写入安全策略",
            "Security, auth preservation, and runtime policies for CLI tools",
        )),
        vec![
            settings_row(
                t,
                i.t("Claude 全权限启动 (--dangerously-skip-permissions)", "Claude Full-Access Launch"),
                Some(i.t(
                    "启动 Claude Code 时自动附加全权限参数，跳过频繁的危险确认提示",
                    "Pass --dangerously-skip-permissions on Claude Code startup",
                )),
                toggle(
                    "claude-full-access",
                    ws.settings.claude_cli_launch_full_access,
                    t,
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
                t,
                i.t("Codex 保留官方登录态", "Preserve Codex Official Auth"),
                Some(i.t(
                    "切换第三方供应商时保留 ~/.codex 的官方登录凭据与会话",
                    "Keep official login session in ~/.codex on provider switch",
                )),
                toggle(
                    "codex-preserve-auth",
                    ws.settings.codex_preserve_official_auth_on_switch,
                    t,
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
                t,
                i.t("OpenAgent 统一 ~/.omo 配置", "OpenAgent Unified ~/.omo Config"),
                Some(i.t(
                    "使用现代统一的 ~/.omo 目录而非旧版分散配置文件",
                    "Write unified config to ~/.omo instead of legacy files",
                )),
                toggle(
                    "omo-legacy-config",
                    ws.settings.opencode_use_legacy_oh_my_config,
                    t,
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
                t,
                i.t("允许清除 OMO/OMOS 运行配置", "Allow Clearing OMO/OMOS Config"),
                Some(i.t(
                    "在重置或切换供应商时允许清空已应用的运行时配置",
                    "Allow wiping runtime config when resetting or switching",
                )),
                toggle(
                    "omo-clear-policy",
                    ws.settings.opencode_allow_clear_applied_oh_my_config,
                    t,
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
                t,
                i.t("双写 reasoning/variant 兼容模式", "Dual Reasoning/Variant Write"),
                Some(i.t(
                    "同时写入推理模型参数以兼容旧版 OpenCode 插件",
                    "Write dual parameters for compatibility with older OpenCode",
                )),
                toggle(
                    "omo-dual-reasoning",
                    ws.settings.opencode_dual_write_reasoning_variant,
                    t,
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
    )
}

fn storage_card(
    ws: &mut Workspace,
    t: &crate::theme::Theme,
    i: &crate::i18n::I18n,
    cx: &mut Context<Workspace>,
) -> gpui::AnyElement {
    let data_dir = ws.paths.app_data.display().to_string();
    let storage_row = settings_row(
        t,
        i.t("应用数据存储目录", "Application Data Directory"),
        Some(gpui::SharedString::from(data_dir)),
        button_with_icon_l(
            "open-data-dir",
            crate::icons::FOLDER_SVG,
            i.t("打开数据目录", "Open Folder"),
            ButtonVariant::Secondary,
            t,
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
                    t,
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
                    t,
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

    settings_card(
        t,
        i.t("数据存储与 CLI 路径覆盖", "Storage & CLI Paths"),
        Some(i.t(
            "查看核心配置存储路径，或自定义特定工具的配置文件与命令行程序位置",
            "Inspect data directory or override config roots and CLI binary paths",
        )),
        vec![
            storage_row,
            roots_rows.into_any_element(),
        ],
    )
}

fn advanced_tab(ws: &mut Workspace, cx: &mut Context<Workspace>) -> gpui::AnyElement {
    let t = ws.theme.clone();
    let i = ws.i18n;

    let cli_card = cli_policies_card(ws, &t, &i, cx);
    let stor_card = storage_card(ws, &t, &i, cx);

    // 1. Conflict strategy card
    let conflict_card = settings_card(
        &t,
        i.t("恢复冲突处理策略", "Restore Conflict Policy"),
        Some(i.t(
            "遇到同名文件时的恢复处理策略，以及是否允许恢复自定义绝对路径",
            "How to handle existing files on restore, and whether to allow custom absolute paths",
        )),
        vec![
            settings_row(
                &t,
                i.t("恢复同名冲突策略", "Conflict Strategy"),
                Some(i.t(
                    "覆盖已有文件、跳过同名文件或保存为 .restored 副本",
                    "Overwrite target, skip existing, or save copy as .restored",
                )),
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
                    .into_any_element(),
            ),
            settings_row(
                &t,
                i.t("允许原绝对路径恢复", "Allow Custom Absolute Paths"),
                Some(i.t(
                    "恢复自定义备份条目时，允许写回原始绝对路径（关闭时落入安全沙箱）",
                    "Allow restoring custom files back to original paths (otherwise sandboxed)",
                )),
                toggle(
                    "restore-custom-absolute-toggle",
                    ws.ui.restore_allow_custom_absolute,
                    &t,
                    cx,
                    |ws, _, _, cx| {
                        ws.ui.restore_allow_custom_absolute =
                            !ws.ui.restore_allow_custom_absolute;
                        cx.notify();
                    },
                ),
            ),
        ],
    );

    // 2. Backup scope card (Include CLI Configs)
    let scope_card = settings_card(
        &t,
        i.t("备份范围与 CLI 配置", "Backup Scope & CLI Configs"),
        Some(i.t(
            "选择创建备份快照时包含的数据范围",
            "Choose data scope when generating backup snapshots",
        )),
        vec![
            settings_row(
                &t,
                i.t("包含各 CLI 运行时配置文件", "Include CLI Config Files"),
                Some(i.t(
                    "包含 Claude Code、Codex、Gemini CLI、Pi 等工具的配置文件与 MCP 设置",
                    "Include runtime configs, prompts, and MCP settings for CLI tools",
                )),
                toggle(
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
                ),
            ),
        ],
    );

    // 3. CLI backup file filter card
    let mut filter_rows = Vec::new();
    for file in aitoolplus_core::backup::cli_config_files(&ws.paths) {
        let display = file.display().to_string();
        let rule_path = display.clone();
        let excluded = ws
            .settings
            .backup_file_filter_rules
            .iter()
            .any(|rule| rule.file_path == display);
        let row = div()
            .flex()
            .items_center()
            .justify_between()
            .gap(px(12.0))
            .p(px(8.0))
            .rounded(px(6.0))
            .bg(t.input_bg)
            .border_1()
            .border_color(t.card_border)
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .text_size(px(11.5))
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
            ))
            .into_any_element();
        filter_rows.push(row);
    }

    let filter_card = settings_card(
        &t,
        i.t("CLI 备份文件过滤", "CLI Backup File Filters"),
        Some(i.t(
            "逐文件排除敏感或不需要打包到快照中的运行时配置文件",
            "Exclude sensitive or unwanted runtime files individually from backup snapshots",
        )),
        vec![
            div()
                .flex()
                .flex_col()
                .gap(px(6.0))
                .p(px(12.0))
                .children(filter_rows)
                .into_any_element(),
        ],
    );

    // 4. Custom backup paths card
    let custom_inputs = ws.ui.backup_custom_inputs(cx);
    let custom_source = custom_inputs.source.clone();
    let custom_restore = custom_inputs.restore.clone();

    let mut custom_items = Vec::new();
    for entry in ws.settings.backup_custom_entries.clone() {
        let id = entry.id.clone();
        let item = div()
            .flex()
            .items_center()
            .justify_between()
            .gap(px(12.0))
            .p(px(8.0))
            .rounded(px(6.0))
            .bg(t.input_bg)
            .border_1()
            .border_color(t.card_border)
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .flex()
                    .flex_col()
                    .gap(px(2.0))
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
                    })),
            )
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
            ))
            .into_any_element();
        custom_items.push(item);
    }

    let add_source = custom_source.clone();
    let add_restore = custom_restore.clone();
    let add_section = div()
        .flex()
        .flex_col()
        .gap(px(8.0))
        .p(px(10.0))
        .rounded(px(6.0))
        .bg(t.card_bg)
        .border_1()
        .border_color(t.card_border)
        .child(custom_source)
        .child(custom_restore)
        .child(button_l(
            "custom-backup-add",
            i.t("添加自定义条目", "Add Custom Entry"),
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
        ));

    let custom_card = settings_card(
        &t,
        i.t("自定义备份条目", "Custom Backup Entries"),
        Some(i.t(
            "额外指定包含在备份中的自定义文件或目录；未指定恢复路径时会安全落入沙箱",
            "Extra files or directories to include; blank restore paths use a safe sandbox",
        )),
        vec![
            div()
                .flex()
                .flex_col()
                .gap(px(8.0))
                .p(px(12.0))
                .children(custom_items)
                .child(add_section)
                .into_any_element(),
        ],
    );

    div()
        .flex()
        .flex_col()
        .w_full()
        .gap(px(20.0))
        .child(cli_card)
        .child(stor_card)
        .child(conflict_card)
        .child(scope_card)
        .child(filter_card)
        .child(custom_card)
        .into_any_element()
}

fn format_iso_time(iso: &str) -> String {
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(iso) {
        dt.with_timezone(&chrono::Local)
            .format("%Y-%m-%d %H:%M:%S")
            .to_string()
    } else {
        iso.chars().take(19).collect::<String>().replace('T', " ")
    }
}

fn about_tab(ws: &mut Workspace, cx: &mut Context<Workspace>) -> gpui::AnyElement {
    let t = ws.theme.clone();
    let i = ws.i18n;
    let version = env!("CARGO_PKG_VERSION");
    let info = ws.ui.update_info.clone();

    // 1. App Info & Data Storage Card
    let about_card = card(
        &t,
        vec![
            section_title(&t, i.t("关于 AI ToolPlus", "About AI ToolPlus"), None),
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
                                .text_size(px(14.0))
                                .font_weight(gpui::FontWeight::BOLD)
                                .text_color(t.text_primary)
                                .child("AI ToolPlus"),
                        )
                        .child(
                            crate::components::badge(
                                &t,
                                format!("v{version}"),
                                crate::components::BadgeKind::Neutral,
                            ),
                        ),
                )
                .child(
                    div()
                        .text_size(px(12.0))
                        .text_color(t.text_secondary)
                        .child(i.t(
                            "Rust + GPUI 原生多端 AI 辅助开发工具箱，全面对标 cc-switch 与 ai-toolbox",
                            "Native Rust + GPUI developer toolkit, matching cc-switch and ai-toolbox",
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
                                .child(i.t("数据存储目录：", "Data Storage Directory:")),
                        )
                        .child(crate::components::badge(
                            &t,
                            i.t(
                                "~/.aitoolplus（统一用户数据目录，对标 CC-Switch）",
                                "~/.aitoolplus (User Profile, CC-Switch Parity)",
                            ),
                            crate::components::BadgeKind::Success,
                        )),
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
                                let _ = open_dir_in_explorer(dir);
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
                                let _ = open_dir_in_explorer(&dir);
                            },
                        ))
                        .child(button_l(
                            "open-user-home-dir",
                            i.t("打开配置根目录", "Open Config Root"),
                            ButtonVariant::Secondary,
                            &t,
                            cx,
                            |ws, _, _, _| {
                                let _ = open_dir_in_explorer(&ws.paths.home);
                            },
                        )),
                )
                .into_any_element(),
        ],
    );

    // 2. Updates & CDN Mirrors Card
    let mut update_items = vec![
        section_title(
            &t,
            i.t("软件更新与 CDN 镜像加速", "Software Updates & CDN Mirrors"),
            Some(i.t(
                "全面对标 cc-switch / ai-toolbox 更新机制，支持 GitHub 官方直连、国内高速镜像代理与自定义 CDN",
                "Update engine matching cc-switch and ai-toolbox, supporting GitHub official, China mirrors, and custom CDNs",
            )),
        ),
        // Row 1: Auto check toggle
        div()
            .flex()
            .items_center()
            .justify_between()
            .w_full()
            .gap(px(16.0))
            .child(section_title(
                &t,
                i.t("启动时自动检查更新", "Check for Updates on Startup"),
                Some(i.t(
                    "应用启动 2.5 秒后在后台静默检查；绝不静默强制安装",
                    "Checks releases quietly 2.5s after launch; never forces silent installation",
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
        // Row 2: Mirror selector
        div()
            .flex()
            .flex_col()
            .gap(px(6.0))
            .w_full()
            .child({
                let current_mirror = ws.settings.update_mirror;
                let current_mirror_name = match current_mirror {
                    aitoolplus_core::updater::UpdateMirror::GhProxy => {
                        i.t("GhProxy 镜像加速 (gh-proxy.com)", "GhProxy Mirror (gh-proxy.com)")
                    }
                    aitoolplus_core::updater::UpdateMirror::Official => {
                        i.t("GitHub 官方 (直连)", "GitHub Official (Direct)")
                    }
                };
                let is_open = ws.ui.update_mirror_dropdown_open;
                let mirror_options = vec![
                    (
                        aitoolplus_core::updater::UpdateMirror::GhProxy,
                        i.t("GhProxy 镜像加速 (gh-proxy.com)", "GhProxy Mirror (gh-proxy.com)"),
                        crate::icons::CLOUD_DOWNLOAD_SVG,
                    ),
                    (
                        aitoolplus_core::updater::UpdateMirror::Official,
                        i.t("GitHub 官方 (直连)", "GitHub Official (Direct)"),
                        crate::icons::GLOBE_SVG,
                    ),
                ];

                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .w_full()
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(2.0))
                            .child(
                                div()
                                    .text_size(px(12.5))
                                    .font_weight(gpui::FontWeight::MEDIUM)
                                    .text_color(t.text_primary)
                                    .child(i.t("下载加速镜像源：", "Download Mirror Source:")),
                            )
                            .child(
                                div()
                                    .text_size(px(11.5))
                                    .text_color(t.text_secondary)
                                    .child(i.t(
                                        "国内网络推荐使用 gh-proxy.com 镜像加速，秒速完成下载",
                                        "GhProxy mirror (gh-proxy.com) recommended for high-speed downloads",
                                    )),
                            ),
                    )
                    .child(
                        div()
                            .relative()
                            .w(px(260.0))
                            .child(
                                div()
                                    .id("update-mirror-dropdown-trigger")
                                    .flex()
                                    .items_center()
                                    .justify_between()
                                    .w_full()
                                    .px(px(10.0))
                                    .py(px(5.5))
                                    .rounded(px(6.0))
                                    .bg(t.input_bg)
                                    .border_1()
                                    .border_color(if is_open { t.accent } else { t.card_border })
                                    .cursor_pointer()
                                    .hover(|s| s.border_color(t.accent))
                                    .on_click(cx.listener(|ws, _, _, cx| {
                                        ws.ui.update_mirror_dropdown_open = !ws.ui.update_mirror_dropdown_open;
                                        cx.notify();
                                    }))
                                    .child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .gap(px(6.0))
                                            .child(
                                                gpui::svg()
                                                    .data(if current_mirror == aitoolplus_core::updater::UpdateMirror::GhProxy {
                                                        crate::icons::CLOUD_DOWNLOAD_SVG
                                                    } else {
                                                        crate::icons::GLOBE_SVG
                                                    })
                                                    .size(px(13.0))
                                                    .text_color(t.accent),
                                            )
                                            .child(
                                                div()
                                                    .text_size(px(12.0))
                                                    .font_weight(gpui::FontWeight::MEDIUM)
                                                    .text_color(t.text_primary)
                                                    .child(current_mirror_name.to_string()),
                                            ),
                                    )
                                    .child(
                                        gpui::svg()
                                            .data(if is_open {
                                                crate::icons::CHEVRON_UP_SVG
                                            } else {
                                                crate::icons::CHEVRON_DOWN_SVG
                                            })
                                            .size(px(11.0))
                                            .text_color(t.text_muted),
                                    ),
                            )
                            .when(is_open, |el| {
                                el.child(gpui::deferred(
                                    div()
                                        .id("update-mirror-dropdown-menu")
                                        .occlude()
                                        .absolute()
                                        .top(px(34.0))
                                        .right_0()
                                        .w(px(260.0))
                                        .bg(t.card_bg)
                                        .border_1()
                                        .border_color(t.card_border)
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
                                                    ws.ui.update_mirror_dropdown_open = false;
                                                    cx.notify();
                                                });
                                            }
                                        })
                                        .children(mirror_options.into_iter().map(|(m, label, icon)| {
                                            let is_selected = m == current_mirror;
                                            div()
                                                .id(gpui::SharedString::from(format!("mirror-opt-{:?}", m)))
                                                .flex()
                                                .items_center()
                                                .justify_between()
                                                .px(px(8.0))
                                                .py(px(5.5))
                                                .rounded(px(4.0))
                                                .bg(if is_selected { t.accent_subtle } else { t.card_bg })
                                                .hover(|s| s.bg(t.card_hover))
                                                .cursor_pointer()
                                                .on_click(cx.listener(move |ws, _, _, cx| {
                                                    ws.settings.update_mirror = m;
                                                    ws.ui.update_mirror_dropdown_open = false;
                                                    (ws.callbacks.save_settings)(&ws.settings);
                                                    cx.notify();
                                                }))
                                                .child(
                                                    div()
                                                        .flex()
                                                        .items_center()
                                                        .gap(px(6.0))
                                                        .child(
                                                            gpui::svg()
                                                                .data(icon)
                                                                .size(px(12.0))
                                                                .text_color(if is_selected { t.accent } else { t.text_muted }),
                                                        )
                                                        .child(
                                                            div()
                                                                .text_size(px(11.5))
                                                                .text_color(if is_selected { t.accent } else { t.text_primary })
                                                                .font_weight(if is_selected { gpui::FontWeight::MEDIUM } else { gpui::FontWeight::NORMAL })
                                                                .child(label.to_string()),
                                                        ),
                                                )
                                                .when(is_selected, |s| {
                                                    s.child(
                                                        gpui::svg()
                                                            .data(crate::icons::CHECK_SVG)
                                                            .size(px(11.0))
                                                            .text_color(t.accent),
                                                    )
                                                })
                                        })),
                                ))
                            }),
                    )
            })
            .into_any_element(),
    ];

    // Check for updates action bar
    let is_checking = ws.ui.update_checking;
    let last_check_text = ws
        .settings
        .last_update_check_time
        .as_deref()
        .map(format_iso_time);

    update_items.push(
        div()
            .flex()
            .items_center()
            .justify_between()
            .w_full()
            .p(px(10.0))
            .rounded(px(8.0))
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
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .child(
                                div()
                                    .text_size(px(13.0))
                                    .font_weight(gpui::FontWeight::MEDIUM)
                                    .text_color(t.text_primary)
                                    .child(format!(
                                        "{} v{}",
                                        i.t("当前版本：", "Current:"),
                                        version
                                    )),
                            )
                            .child(if let Some(ref inf) = info {
                                if inf.update_available {
                                    crate::components::badge(
                                        &t,
                                        i.t("发现新版本", "Update Available"),
                                        crate::components::BadgeKind::Warning,
                                    )
                                } else {
                                    crate::components::badge(
                                        &t,
                                        i.t("最新版本", "Latest"),
                                        crate::components::BadgeKind::Success,
                                    )
                                }
                            } else {
                                crate::components::badge(
                                    &t,
                                    i.t("已就绪", "Ready"),
                                    crate::components::BadgeKind::Neutral,
                                )
                            }),
                    )
                    .child(
                        div()
                            .text_size(px(11.5))
                            .text_color(t.text_muted)
                            .child(if let Some(t_str) = last_check_text {
                                format!(
                                    "{} {}",
                                    i.t("上次检查时间：", "Last checked:"),
                                    t_str
                                )
                            } else {
                                i.t("尚未检查过更新", "Never checked").to_string()
                            }),
                    ),
            )
            .child(button_l(
                "check-for-updates",
                if is_checking {
                    i.t("正在检查中…", "Checking…")
                } else {
                    i.t("检查更新", "Check for Updates")
                },
                if is_checking {
                    ButtonVariant::Secondary
                } else {
                    ButtonVariant::Primary
                },
                &t,
                cx,
                |ws, _, _, cx| {
                    if ws.ui.update_checking {
                        return;
                    }
                    ws.ui.update_checking = true;
                    ws.ui.update_error = None;
                    let custom_api = ws.settings.custom_update_api_url.clone();
                    let weak = cx.entity().downgrade();
                    cx.spawn(async move |_this, cx| {
                        let result = cx
                            .background_spawn(async move {
                                if !custom_api.trim().is_empty() {
                                    aitoolplus_core::updater::check_latest_at(
                                        &custom_api,
                                        env!("CARGO_PKG_VERSION"),
                                    )
                                } else {
                                    aitoolplus_core::updater::check_latest(
                                        env!("CARGO_PKG_VERSION"),
                                    )
                                }
                            })
                            .await;
                        let _ = weak.update(cx, |ws, cx| {
                            ws.ui.update_checking = false;
                            ws.settings.last_update_check_time =
                                Some(chrono::Utc::now().to_rfc3339());
                            ws.settings.dismissed_update_version = None;
                            (ws.callbacks.save_settings)(&ws.settings);
                            match result {
                                Ok(inf) => {
                                    let message = if inf.update_available {
                                        if !aitoolplus_core::updater::is_installer_installed() {
                                            ws.i18n
                                                .t(
                                                    &format!(
                                                        "发现新版本 v{}（免安装版请前往 Release 页面下载）",
                                                        inf.latest_version
                                                    ),
                                                    &format!(
                                                        "New version v{} available (Portable: download from Releases)",
                                                        inf.latest_version
                                                    ),
                                                )
                                                .to_string()
                                        } else {
                                            ws.i18n
                                                .t(
                                                    &format!(
                                                        "发现新版本 v{}",
                                                        inf.latest_version
                                                    ),
                                                    &format!(
                                                        "New version v{} available",
                                                        inf.latest_version
                                                    ),
                                                )
                                                .to_string()
                                        }
                                    } else {
                                        ws.i18n
                                            .t("当前已是最新版", "Already up to date")
                                            .to_string()
                                    };
                                    ws.ui.update_info = Some(inf);
                                    ws.ui.toast(message, false);
                                }
                                Err(error) => {
                                    ws.ui.update_error = Some(error.clone());
                                    ws.ui.toast(
                                        format!("检查更新失败: {error}"),
                                        true,
                                    );
                                }
                            }
                            cx.notify();
                        });
                    })
                    .detach();
                },
            ))
            .into_any_element(),
    );

    // Update error banner
    if let Some(err) = &ws.ui.update_error {
        update_items.push(
            div()
                .flex()
                .items_center()
                .gap(px(8.0))
                .p(px(10.0))
                .rounded(px(8.0))
                .bg(t.danger_subtle)
                .border_1()
                .border_color(t.danger.opacity(0.3))
                .child(
                    div()
                        .text_size(px(12.0))
                        .text_color(t.danger)
                        .child(format!(
                            "{} {}",
                            i.t("更新提示：", "Notice:"),
                            err
                        )),
                )
                .into_any_element(),
        );
    }

    // Update available vs Up-to-date card
    if let Some(info) = info {
        if info.update_available {
            let asset = aitoolplus_core::updater::best_asset(&info).cloned();
            let notes = info.release_notes.chars().take(1500).collect::<String>();
            let pub_date = info.published_at.as_deref().map(format_iso_time);

            let mut release_card = div()
                .flex()
                .flex_col()
                .gap(px(10.0))
                .p(px(12.0))
                .rounded(px(8.0))
                .bg(t.input_bg)
                .border_1()
                .border_color(t.accent)
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
                                        .text_size(px(13.5))
                                        .font_weight(gpui::FontWeight::BOLD)
                                        .text_color(t.text_primary)
                                        .child(format!(
                                            "{} v{} → v{}",
                                            i.t(
                                                "发现新版本：",
                                                "Update Available:",
                                            ),
                                            info.current_version,
                                            info.latest_version
                                        )),
                                )
                                .child(crate::components::badge(
                                    &t,
                                    i.t("可升级", "Available"),
                                    crate::components::BadgeKind::Success,
                                )),
                        )
                        .children(pub_date.map(|pd| {
                            div()
                                .text_size(px(11.5))
                                .text_color(t.text_muted)
                                .child(format!(
                                    "{} {}",
                                    i.t("发布时间：", "Released:"),
                                    pd
                                ))
                                .into_any_element()
                        })),
                );

            if !notes.is_empty() {
                release_card = release_card.child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(4.0))
                        .child(
                            div()
                                .text_size(px(11.5))
                                .font_weight(gpui::FontWeight::MEDIUM)
                                .text_color(t.text_secondary)
                                .child(i.t(
                                    "更新日志 (Release Notes)：",
                                    "Release Notes:",
                                )),
                        )
                        .child(
                            div()
                                .id("update-release-notes")
                                .max_h(px(120.0))
                                .overflow_y_scroll()
                                .p(px(8.0))
                                .rounded(px(6.0))
                                .bg(t.card_bg)
                                .border_1()
                                .border_color(t.card_border)
                                .text_size(px(11.5))
                                .text_color(t.text_secondary)
                                .child(notes),
                        ),
                );
            }

            let is_installer = aitoolplus_core::updater::is_installer_installed();

            if !is_installer {
                let rel_url = info.release_url.clone();
                let rel_url_copy = info.release_url.clone();
                let target_ver = info.latest_version.clone();

                let portable_tip = div()
                    .flex_col()
                    .gap(px(8.0))
                    .p(px(12.0))
                    .rounded(px(6.0))
                    .bg(t.card_bg)
                    .border_1()
                    .border_color(t.accent.opacity(0.3))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .child(crate::components::badge(
                                &t,
                                i.t("免安装版", "Portable"),
                                crate::components::BadgeKind::Accent,
                            ))
                            .child(
                                div()
                                    .text_size(px(13.0))
                                    .font_weight(gpui::FontWeight::MEDIUM)
                                    .text_color(t.text_primary)
                                    .child(i.t(
                                        "检测到当前运行为免安装便携版",
                                        "Detected portable standalone version",
                                    )),
                            ),
                    )
                    .child(
                        div()
                            .text_size(px(12.0))
                            .line_height(gpui::relative(1.5))
                            .text_color(t.text_secondary)
                            .child(i.t(
                                "免安装版无需运行安装程序。请点击下方按钮前往 GitHub Release 页面下载最新的绿色压缩包 (aitoolplus-windows-x86_64.zip)，解压替换即可完成更新。所有用户配置与规则统一保存在 ~/.aitoolplus 目录，更新不会影响您的任何数据。",
                                "Portable edition does not require an installer. Please click below to visit GitHub Release to download the latest zip archive (aitoolplus-windows-x86_64.zip) and extract it to replace the application files. User data in ~/.aitoolplus remains safe.",
                            )),
                    );

                let portable_action_row = div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .flex_wrap()
                    .child(button_l(
                        "open-release-page-portable-btn",
                        i.t("前往 Release 下载页面", "Go to Release Page"),
                        ButtonVariant::Primary,
                        &t,
                        cx,
                        move |ws, _, _, cx| {
                            if !rel_url.is_empty() {
                                cx.open_url(&rel_url);
                                ws.ui.toast(
                                    ws.i18n
                                        .t(
                                            "正在打开 Release 下载页面…",
                                            "Opening Release page…",
                                        )
                                        .to_string(),
                                    false,
                                );
                            }
                        },
                    ))
                    .child(button_l(
                        "copy-release-link-btn",
                        i.t("复制下载链接", "Copy Link"),
                        ButtonVariant::Secondary,
                        &t,
                        cx,
                        move |ws, _, _, cx| {
                            cx.write_to_clipboard(gpui::ClipboardItem::new_string(
                                rel_url_copy.clone(),
                            ));
                            ws.ui.toast(
                                ws.i18n
                                    .t(
                                        "已复制 Release 链接到剪贴板",
                                        "Release link copied to clipboard",
                                    )
                                    .to_string(),
                                false,
                            );
                        },
                    ))
                    .child(button_l(
                        "dismiss-update-version-btn",
                        i.t("忽略此版本", "Dismiss Version"),
                        ButtonVariant::Ghost,
                        &t,
                        cx,
                        move |ws, _, _, cx| {
                            ws.settings.dismissed_update_version = Some(target_ver.clone());
                            (ws.callbacks.save_settings)(&ws.settings);
                            ws.ui.update_info = None;
                            ws.ui.toast(
                                ws.i18n
                                    .t("已忽略此版本更新", "Version update dismissed")
                                    .to_string(),
                                false,
                            );
                            cx.notify();
                        },
                    ));

                release_card = release_card.child(portable_tip).child(portable_action_row);
            } else if let Some(asset) = asset {
                let current_mirror = ws.settings.update_mirror;
                let mirror_label = current_mirror
                    .display_name(ws.settings.language == aitoolplus_core::settings::Language::Zh);

                let is_portable_asset = asset.name.ends_with(".zip");
                let asset_type_label = if is_portable_asset {
                    i.t("免安装版", "Portable")
                } else {
                    i.t("安装版", "Installer")
                };

                let asset_info_row = div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .text_size(px(11.5))
                    .text_color(t.text_secondary)
                    .child(format!(
                        "{} {} ({} - {})",
                        i.t("更新包：", "Package:"),
                        asset.name,
                        asset_type_label,
                        if asset.size > 0 {
                            format_file_size(asset.size)
                        } else {
                            i.t("完整包", "Full").to_string()
                        }
                    ))
                    .child(format!(
                        "{} {}",
                        i.t("加速源：", "Mirror:"),
                        mirror_label
                    ));

                release_card = release_card.child(asset_info_row);

                // Action area: Downloaded vs Downloading vs Ready to Download
                if let Some(downloaded_path) =
                    ws.ui.downloaded_update_asset_path.clone()
                {
                    let path_for_install = downloaded_path.clone();
                    let path_for_reveal = downloaded_path.clone();
                    let rel_url_ready = info.release_url.clone();
                    let ready_badge_text = if is_portable_asset {
                        i.t("免安装更新已就绪", "Portable Update Ready")
                    } else {
                        i.t("安装包已就绪", "Installer Ready")
                    };
                    let install_btn_text = if is_portable_asset {
                        i.t("立即更新并重启", "Update & Restart Now")
                    } else {
                        i.t("立即安装并重启", "Install & Restart Now")
                    };

                    let ready_row = div()
                        .flex()
                        .items_center()
                        .gap(px(8.0))
                        .flex_wrap()
                        .child(crate::components::badge(
                            &t,
                            ready_badge_text,
                            crate::components::BadgeKind::Success,
                        ))
                        .child(button_l(
                            "install-update-now",
                            install_btn_text,
                            ButtonVariant::Primary,
                            &t,
                            cx,
                            move |ws, _, _, cx| {
                                match aitoolplus_core::updater::install_update_and_restart(
                                    &path_for_install,
                                ) {
                                    Ok(()) => {}
                                    Err(error) => {
                                        ws.ui.toast(
                                            format!("更新失败: {error}"),
                                            true,
                                        );
                                        cx.notify();
                                    }
                                }
                            },
                        ))
                        .child(button_l(
                            "reveal-update-folder",
                            i.t("打开所在文件夹", "Open Folder"),
                            ButtonVariant::Secondary,
                            &t,
                            cx,
                            move |_, _, _, _| {
                                let _ = open_dir_in_explorer(
                                    path_for_reveal
                                        .parent()
                                        .unwrap_or(&path_for_reveal),
                                );
                            },
                        ))
                        .child(button_l(
                            "ready-release-notes-btn",
                            i.t("查看发行说明", "Release Notes"),
                            ButtonVariant::Secondary,
                            &t,
                            cx,
                            move |_, _, _, cx| {
                                if !rel_url_ready.is_empty() {
                                    cx.open_url(&rel_url_ready);
                                }
                            },
                        ))
                        .child(button_l(
                            "re-download-update-btn",
                            i.t("重新下载", "Re-download"),
                            ButtonVariant::Ghost,
                            &t,
                            cx,
                            |ws, _, _, cx| {
                                ws.ui.downloaded_update_asset_path = None;
                                cx.notify();
                            },
                        ));

                    release_card = release_card.child(ready_row);
                } else if ws.ui.update_downloading {
                    let progress = ws.ui.update_download_progress;
                    let downloaded = ws.ui.update_downloaded_bytes;
                    let total = ws.ui.update_total_bytes;
                    let speed = ws.ui.update_download_speed;

                    let progress_card = div()
                        .flex()
                        .flex_col()
                        .gap(px(6.0))
                        .w_full()
                        .p(px(8.0))
                        .rounded(px(6.0))
                        .bg(t.card_bg)
                        .border_1()
                        .border_color(t.card_border)
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .justify_between()
                                .text_size(px(11.5))
                                .child(
                                    div()
                                        .font_weight(gpui::FontWeight::MEDIUM)
                                        .text_color(t.text_primary)
                                        .child(format!(
                                            "{} {:.1}%",
                                            i.t(
                                                "正在下载更新包…",
                                                "Downloading…",
                                            ),
                                            progress
                                        )),
                                )
                                .child(
                                    div()
                                        .text_color(t.accent)
                                        .font_weight(gpui::FontWeight::BOLD)
                                        .child(format!(
                                            "{}/s",
                                            format_file_size(speed)
                                        )),
                                ),
                        )
                        .child(
                            div()
                                .w_full()
                                .h(px(8.0))
                                .bg(t.card_border)
                                .rounded(px(4.0))
                                .overflow_hidden()
                                .child(
                                    div()
                                        .h_full()
                                        .w(gpui::relative(
                                            (progress / 100.0).clamp(0.0, 1.0),
                                        ))
                                        .bg(t.accent)
                                        .rounded(px(4.0)),
                                ),
                        )
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .justify_between()
                                .text_size(px(11.0))
                                .text_color(t.text_muted)
                                .child(format!(
                                    "{} / {}",
                                    format_file_size(downloaded),
                                    if total > 0 {
                                        format_file_size(total)
                                    } else {
                                        "--".into()
                                    }
                                ))
                                .child(i.t(
                                    "支持断点续连与自动校验",
                                    "Checksum verification enabled",
                                )),
                        );

                    release_card = release_card.child(progress_card);
                } else {
                    let asset_to_download = asset.clone();
                    let mirror = ws.settings.update_mirror;
                    let custom_prefix = ws.settings.custom_update_mirror_url.clone();

                    let info_for_download = info.clone();
                    let download_btn = button_l(
                        "start-download-update",
                        i.t(
                            "立即下载更新包 (高速)",
                            "Download Update (High Speed)",
                        ),
                        ButtonVariant::Primary,
                        &t,
                        cx,
                        move |ws, _, _, cx| {
                            let output = ws
                                .paths
                                .app_data
                                .join("updates")
                                .join(&asset_to_download.name);
                            let download_url = mirror.apply_url(
                                &asset_to_download.download_url,
                                &custom_prefix,
                            );
                            let expected_size = asset_to_download.size;

                            ws.ui.update_downloading = true;
                            ws.ui.update_download_progress = 0.0;
                            ws.ui.update_downloaded_bytes = 0;
                            ws.ui.update_total_bytes = expected_size;
                            ws.ui.update_download_speed = 0;
                            ws.ui.update_error = None;

                            let (tx, rx) = async_channel::unbounded::<
                                aitoolplus_core::updater::DownloadProgress,
                            >();
                            let weak_prog = cx.entity().downgrade();

                            // Progress listener
                            cx.spawn(async move |_this, cx| {
                                while let Ok(prog) = rx.recv().await {
                                    let _ = weak_prog.update(cx, |ws, cx| {
                                        ws.ui.update_download_progress =
                                            prog.percentage;
                                        ws.ui.update_downloaded_bytes =
                                            prog.downloaded;
                                        ws.ui.update_total_bytes = prog.total;
                                        ws.ui.update_download_speed =
                                            prog.speed_bps;
                                        cx.notify();
                                    });
                                }
                            })
                            .detach();

                            // Background download worker
                            let weak_finish = cx.entity().downgrade();
                            let output_clone = output.clone();
                            let info_clone = info_for_download.clone();
                            let asset_clone = asset_to_download.clone();
                            let mirror_clone = mirror;
                            let prefix_clone = custom_prefix.clone();
                            cx.spawn(async move |_this, cx| {
                                let result = cx
                                    .background_spawn(async move {
                                        let expected_sha = aitoolplus_core::updater::fetch_expected_sha256(
                                            &info_clone,
                                            &asset_clone,
                                            &mirror_clone,
                                            &prefix_clone,
                                        );
                                        let downloaded = aitoolplus_core::updater::download_with_progress(
                                            &download_url,
                                            expected_size,
                                            &output_clone,
                                            move |p| {
                                                let _ = tx.try_send(p);
                                                true
                                            },
                                        )?;
                                        if let Some(expected) = expected_sha {
                                            match aitoolplus_core::updater::verify_asset_sha256(&downloaded, &expected) {
                                                Ok(true) => {}
                                                Ok(false) => {
                                                    let _ = std::fs::remove_file(&downloaded);
                                                    return Err("SHA-256 校验失败，文件可能已损坏，请重新下载".to_string());
                                                }
                                                Err(e) => {
                                                    let _ = std::fs::remove_file(&downloaded);
                                                    return Err(format!("校验异常: {e}"));
                                                }
                                            }
                                        }
                                        Ok(downloaded)
                                    })
                                    .await;

                                let _ = weak_finish.update(cx, |ws, cx| {
                                    ws.ui.update_downloading = false;
                                    match result {
                                        Ok(path) => {
                                            ws.ui.downloaded_update_asset_path =
                                                Some(path.clone());
                                            ws.ui.toast(
                                                ws.i18n
                                                    .t(
                                                        &format!(
                                                            "更新包下载完成并校验通过: {}",
                                                            path.display()
                                                        ),
                                                        &format!(
                                                            "Update downloaded and verified: {}",
                                                            path.display()
                                                        ),
                                                    )
                                                    .to_string(),
                                                false,
                                            );
                                        }
                                        Err(err) => {
                                            ws.ui.update_error =
                                                Some(err.clone());
                                            ws.ui.toast(
                                                format!("下载失败: {err}"),
                                                true,
                                            );
                                        }
                                    }
                                    cx.notify();
                                });
                            })
                            .detach();

                            cx.notify();
                        },
                    );

                    let rel_url = info.release_url.clone();
                    let notes_btn = button_l(
                        "open-release-notes-btn",
                        i.t("查看发行说明", "Release Notes"),
                        ButtonVariant::Secondary,
                        &t,
                        cx,
                        move |_, _, _, cx| {
                            if !rel_url.is_empty() {
                                cx.open_url(&rel_url);
                            }
                        },
                    );

                    let target_ver = info.latest_version.clone();
                    let dismiss_btn = button_l(
                        "dismiss-update-version-btn",
                        i.t("忽略此版本", "Dismiss Version"),
                        ButtonVariant::Ghost,
                        &t,
                        cx,
                        move |ws, _, _, cx| {
                            ws.settings.dismissed_update_version = Some(target_ver.clone());
                            (ws.callbacks.save_settings)(&ws.settings);
                            ws.ui.update_info = None;
                            ws.ui.toast(
                                ws.i18n
                                    .t(
                                        &format!("已忽略 v{} 版本更新提醒", target_ver),
                                        &format!("Update v{} dismissed", target_ver),
                                    )
                                    .to_string(),
                                false,
                            );
                            cx.notify();
                        },
                    );

                    if aitoolplus_core::updater::is_scoop_install() {
                        release_card = release_card.child(
                            div()
                                .p(px(8.0))
                                .rounded(px(6.0))
                                .bg(t.warning_subtle)
                                .border_1()
                                .border_color(t.warning.opacity(0.4))
                                .text_size(px(12.0))
                                .text_color(t.warning)
                                .child(i.t(
                                    "提示：检测到当前应用通过 Scoop 安装，推荐在终端执行 'scoop update aitoolplus' 完成升级。",
                                    "Note: Scoop-managed installation detected. Please upgrade via 'scoop update aitoolplus' in terminal.",
                                )),
                        );
                    }

                    release_card = release_card.child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .flex_wrap()
                            .child(download_btn)
                            .child(notes_btn)
                            .child(dismiss_btn),
                    );
                }
            }

            update_items.push(release_card.into_any_element());
        } else {
            // Already up to date
            update_items.push(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .p(px(10.0))
                    .rounded(px(8.0))
                    .bg(t.input_bg)
                    .border_1()
                    .border_color(t.success.opacity(0.3))
                    .child(crate::components::badge(
                        &t,
                        i.t("已是最新版本", "Up to Date"),
                        crate::components::BadgeKind::Success,
                    ))
                    .child(
                        div()
                            .text_size(px(12.5))
                            .text_color(t.text_secondary)
                            .child(format!(
                                "{} v{} {}",
                                i.t(
                                    "当前安装的 AI ToolPlus",
                                    "Currently installed AI ToolPlus"
                                ),
                                info.current_version,
                                i.t(
                                    "已是最新发布版本，暂无可用更新。",
                                    "is the latest version, no update needed."
                                )
                            )),
                    )
                    .into_any_element(),
            );
        }
    }

    div()
        .flex()
        .flex_col()
        .gap(px(16.0))
        .w_full()
        .child(about_card)
        .child(card(&t, update_items))
        .into_any_element()
}

fn open_dir_in_explorer(path: &std::path::Path) -> std::io::Result<()> {
    super::open_path_in_default_manager(path);
    Ok(())
}

/// Local copy of the dark detection (keeps this module self-contained).
fn system_prefers_dark() -> bool {
    crate::workspace::system_prefers_dark_pub()
}

/// Backup snapshot rename dialog (rendered by pages::render_modals).
pub fn render_backup_rename_dialog(
    path: std::path::PathBuf,
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
        .child(crate::components::input_container(&t, input.clone()))
        .child(
            div()
                .flex()
                .justify_end()
                .gap(px(8.0))
                .child(button_l(
                    "backup-rename-cancel",
                    i.t("取消", "Cancel"),
                    ButtonVariant::Secondary,
                    &t,
                    cx,
                    |ws, _, _, cx| {
                        ws.ui.backup_rename_dialog = None;
                        cx.notify();
                    },
                ))
                .child(button_l(
                    "backup-rename-save",
                    i.t("保存", "Save"),
                    ButtonVariant::Primary,
                    &t,
                    cx,
                    move |ws, _, _, cx| {
                        let new_name: String = input.update(cx, |inp, _| inp.text().trim().to_string());
                        if !new_name.is_empty() {
                            let filename = if new_name.ends_with(".zip") {
                                new_name
                            } else {
                                format!("{new_name}.zip")
                            };
                            if let Some(parent) = path.parent() {
                                let new_path = parent.join(&filename);
                                if new_path.exists() && new_path != path {
                                    ws.ui.toast(
                                        ws.i18n.t("同名备份文件已存在", "Backup with this name already exists").to_string(),
                                        true,
                                    );
                                } else {
                                    match std::fs::rename(&path, &new_path) {
                                        Ok(()) => {
                                            ws.ui.toast(
                                                ws.i18n.t("备份已重命名", "Backup renamed").to_string(),
                                                false,
                                            );
                                        }
                                        Err(e) => {
                                            ws.ui.toast(format!("重命名失败: {e}"), true);
                                        }
                                    }
                                }
                            }
                        }
                        ws.ui.backup_rename_dialog = None;
                        cx.notify();
                    },
                )),
        );

    super::modal_scaffold(
        &t,
        i.t("重命名备份快照", "Rename Backup Snapshot").as_ref(),
        body.into_any_element(),
        cx,
        |ws, _, _, cx| {
            ws.ui.backup_rename_dialog = None;
            cx.notify();
        },
    )
}
