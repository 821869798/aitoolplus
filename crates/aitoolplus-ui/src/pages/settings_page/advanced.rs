use gpui::{Context, IntoElement, div, prelude::*, px};

use crate::components::{
    ButtonVariant, button_l, button_with_icon_l, button_with_icon_loading_l, card, input_container, section_title,
    segmented_pill_selector, settings_card, settings_row, toggle,
};
use crate::text_input::TextInput;
use crate::workspace::Workspace;

use crate::pages::SettingsTab;

pub(super) fn advanced_tab(ws: &mut Workspace, cx: &mut Context<Workspace>) -> gpui::AnyElement {
    let t = ws.theme.clone();
    let i = ws.i18n;

    let cli_card = super::data_import::cli_policies_card(ws, &t, &i, cx);
    let stor_card = super::data_import::storage_card(ws, &t, &i, cx);

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

