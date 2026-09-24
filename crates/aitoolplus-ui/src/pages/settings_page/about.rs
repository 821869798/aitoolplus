use gpui::{Context, IntoElement, div, prelude::*, px};

use crate::components::{
    ButtonVariant, button_l, button_with_icon_l, button_with_icon_loading_l, card, input_container, section_title,
    segmented_pill_selector, settings_card, settings_row, toggle,
};
use crate::text_input::TextInput;
use crate::workspace::Workspace;

use crate::pages::SettingsTab;
use super::backup::{format_file_size, open_dir_in_explorer};

pub(super) fn format_iso_time(iso: &str) -> String {
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(iso) {
        dt.with_timezone(&chrono::Local)
            .format("%Y-%m-%d %H:%M:%S")
            .to_string()
    } else {
        iso.chars().take(19).collect::<String>().replace('T', " ")
    }
}

pub(super) fn about_tab(ws: &mut Workspace, cx: &mut Context<Workspace>) -> gpui::AnyElement {
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
                                ws.ui.update_install_confirm_dialog =
                                    Some(path_for_install.clone());
                                cx.notify();
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
                                            ws.ui.update_install_confirm_dialog =
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

