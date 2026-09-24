use gpui::{Context, IntoElement, div, prelude::*, px};

use crate::components::{
    ButtonVariant, button_l, button_with_icon_l, button_with_icon_loading_l, card, input_container, section_title,
    segmented_pill_selector, settings_card, settings_row, toggle,
};
use crate::text_input::TextInput;
use crate::workspace::Workspace;

use crate::pages::SettingsTab;

use super::backup::{apply_restored, defer_io, format_file_size, open_dir_in_explorer, reload_after_restore, CloudRestoreError};

pub(super) fn data_import_tab(ws: &mut Workspace, cx: &mut Context<Workspace>) -> gpui::AnyElement {
    div()
        .flex()
        .flex_col()
        .w_full()
        .gap(px(16.0))
        .child(cc_switch_migration_card(ws, cx))
        .child(antigravity_manager_import_card(ws, cx))
        .child(json_config_transfer_card(ws, cx))
        .into_any_element()
}

pub(super) fn cc_switch_migration_card(ws: &mut Workspace, cx: &mut Context<Workspace>) -> gpui::AnyElement {
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
                    button_with_icon_loading_l(
                        "import-cc-switch-action",
                        crate::icons::DOWNLOAD_SVG,
                        i.t("导入供应商配置", "Import Providers"),
                        ButtonVariant::Primary,
                        ws.ui.cc_switch_busy,
                        &t,
                        cx,
                        move |ws, _, _, cx| {
                            if ws.ui.cc_switch_busy {
                                return;
                            }
                            ws.ui.cc_switch_busy = true;
                            cx.notify();
                            let db_path = ws
                                .ui
                                .cc_switch_custom_db_path
                                .clone()
                                .or_else(|| import_target_path.clone())
                                .or_else(|| aitoolplus_core::cc_switch_import::detect_cc_switch_db(&ws.paths));
                            let Some(db_path) = db_path else {
                                ws.ui.cc_switch_busy = false;
                                ws.ui.toast("未找到 cc-switch.db 数据库文件".to_string(), true);
                                cx.notify();
                                return;
                            };
                            defer_io(
                                cx,
                                move || {
                                    aitoolplus_core::cc_switch_import::read_cc_switch_providers(
                                        &db_path,
                                    )
                                },
                                |ws, cx, result| {
                                    ws.ui.cc_switch_busy = false;
                                    match result {
                                        Ok(rows) => {
                                            let report = aitoolplus_core::cc_switch_import::apply_cc_switch_providers(
                                                ws.store.store_mut(),
                                                rows,
                                            );
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
                                        Err(e) => ws.ui.toast(format!("CC-Switch 导入失败: {e}"), true),
                                    }
                                    cx.notify();
                                },
                            );
                        },
                    ),
                )
                .child(
                    button_with_icon_loading_l(
                        "import-cc-switch-usage-action",
                        crate::icons::DATABASE_SVG,
                        i.t("导入使用统计与定价", "Import Usage & Pricing"),
                        ButtonVariant::Secondary,
                        ws.ui.cc_switch_busy,
                        &t,
                        cx,
                        move |ws, _, _, cx| {
                            if ws.ui.cc_switch_busy {
                                return;
                            }
                            let target_path = import_usage_target_path.clone()
                                .or_else(|| aitoolplus_core::cc_switch_import::detect_cc_switch_db(&ws.paths));
                            let Some(path) = target_path else {
                                ws.ui.toast("未检测到 CC-Switch 数据库文件".to_string(), true);
                                cx.notify();
                                return;
                            };
                            let Some(db) = ws.ensure_usage_db() else {
                                ws.ui.toast("数据库初始化失败".to_string(), true);
                                cx.notify();
                                return;
                            };
                            ws.ui.cc_switch_busy = true;
                            cx.notify();
                            defer_io(
                                cx,
                                move || db.import_from_cc_switch(&path),
                                |ws, cx, result| {
                                    ws.ui.cc_switch_busy = false;
                                    match result {
                                        Ok(rep) => {
                                            ws.refresh_usage_data(cx);
                                            ws.ui.toast(
                                                format!(
                                                    "使用统计迁移完成：导入 {} 条请求日志、{} 条模型定价、{} 条汇总数据",
                                                    rep.logs_imported, rep.pricing_imported, rep.rollups_imported
                                                ),
                                                false,
                                            );
                                        }
                                        Err(e) => ws.ui.toast(format!("导入使用统计失败: {e}"), true),
                                    }
                                    cx.notify();
                                },
                            );
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

pub(super) fn antigravity_manager_import_card(ws: &mut Workspace, cx: &mut Context<Workspace>) -> gpui::AnyElement {
    let t = ws.theme.clone();
    let i = ws.i18n;
    let manager_dir = ws.paths.home.join(".antigravity_tools");
    let found = manager_dir.is_dir();
    settings_card(
        &t,
        i.t("Antigravity Manager 迁移", "Antigravity Manager Import"),
        Some(i.t(
            "从本机 ~/.antigravity_tools 读取账号，按邮箱合并进 Antigravity 账号列表",
            "Read accounts from ~/.antigravity_tools and merge them into the Antigravity account list by email",
        )),
        vec![settings_row(
            &t,
            if found {
                i.t("数据目录", "Data folder")
            } else {
                i.t("未找到数据目录", "Data folder not found")
            },
            Some(i.t(
                &manager_dir.display().to_string(),
                &manager_dir.display().to_string(),
            )),
            button_with_icon_loading_l(
                "settings-ag-import-manager",
                crate::icons::DOWNLOAD_SVG,
                i.t("从 Manager 迁移", "Import from Manager"),
                ButtonVariant::Secondary,
                ws.ui.antigravity_manager_importing,
                &t,
                cx,
                |ws, _, _, cx| crate::pages::antigravity_page::import_from_manager_action(ws, cx),
            ),
        )],
    )
}

pub(super) fn json_config_transfer_card(ws: &mut Workspace, cx: &mut Context<Workspace>) -> gpui::AnyElement {
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

pub(super) fn cli_policies_card(
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

pub(super) fn storage_card(
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

