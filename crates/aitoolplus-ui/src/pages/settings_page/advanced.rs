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
        i.t("settings_advanced.restore_conflict_policy"),
        Some(i.t("settings_advanced.how_to_handle_existing")),
        vec![
            settings_row(
                &t,
                i.t("settings_advanced.conflict_strategy"),
                Some(i.t("settings_advanced.overwrite_target_skip_existing")),
                div()
                    .flex()
                    .gap(px(8.0))
                    .child(button_l(
                        "conflict-strategy-overwrite",
                        i.t("settings_advanced.overwrite"),
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
                        i.t("settings_advanced.skip_existing"),
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
                        i.t("settings_advanced.save_copy_restored"),
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
                i.t("settings_advanced.allow_custom_absolute_paths"),
                Some(i.t("settings_advanced.allow_restoring_custom_files")),
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
        i.t("settings_advanced.backup_scope_cli_configs"),
        Some(i.t("settings_advanced.choose_data_scope_when")),
        vec![
            settings_row(
                &t,
                i.t("settings_advanced.include_cli_config_files"),
                Some(i.t("settings_advanced.include_runtime_configs_prompt")),
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
                    i.t("settings_advanced.excluded")
                } else {
                    i.t("settings_advanced.included")
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
        i.t("settings_advanced.cli_backup_file_filters"),
        Some(i.t("settings_advanced.exclude_sensitive_or_unwanted")),
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
                i.t("settings_advanced.remove_entry"),
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
            i.t("settings_advanced.add_custom_entry"),
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
                            .t("settings_advanced.source_path_required")
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
        i.t("settings_advanced.custom_backup_entries"),
        Some(i.t("settings_advanced.extra_files_or_directories")),
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

