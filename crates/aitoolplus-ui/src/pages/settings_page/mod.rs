//! Settings, split by tab.
use gpui::{Context, IntoElement, div, prelude::*, px};

use crate::components::{
    ButtonVariant, button_l, button_with_icon_l, button_with_icon_loading_l, card, input_container, section_title,
    segmented_pill_selector, settings_card, settings_row, toggle,
};
use crate::text_input::TextInput;
use crate::workspace::Workspace;

use crate::pages::SettingsTab;

mod about;
mod advanced;
mod backup;
mod data_import;
mod general;

pub use backup::render_backup_rename_dialog;

pub fn render_settings_page(ws: &mut Workspace, cx: &mut Context<Workspace>) -> gpui::AnyElement {
    let t = ws.theme.clone();
    let i = ws.i18n;

    let has_update = ws
        .ui
        .update_info
        .as_ref()
        .map(|u| u.update_available)
        .unwrap_or(false);

    let tabs = vec![
        (SettingsTab::General, i.t("通用", "General"), false),
        (SettingsTab::DataImport, i.t("数据导入", "Data Import"), false),
        (SettingsTab::Usage, i.t("使用统计", "Usage Statistics"), false),
        (SettingsTab::Backup, i.t("备份", "Backup"), false),
        (SettingsTab::Advanced, i.t("高级选项", "Advanced"), false),
        (SettingsTab::LocalEnv, i.t("本地环境", "Local Environment"), false),
        (SettingsTab::About, i.t("关于", "About"), has_update),
    ];

    let current_tab = ws.ui.settings_tab;
    let tab_bar = crate::components::segmented_tab_bar_with_dots(
        "settings",
        tabs,
        current_tab,
        &t,
        cx,
        |ws, tab, _window, cx| {
            ws.ui.settings_tab = tab;
            cx.notify();
        },
    );

    let body = match ws.ui.settings_tab {
        SettingsTab::General => general::general_tab(ws, cx),
        SettingsTab::DataImport => data_import::data_import_tab(ws, cx),
        SettingsTab::Usage => crate::pages::usage_page::render_usage_page(ws, cx),
        SettingsTab::Backup => backup::backup_tab(ws, cx),
        SettingsTab::Advanced => advanced::advanced_tab(ws, cx),
        SettingsTab::LocalEnv => crate::pages::local_env_page::local_env_tab(ws, cx),
        SettingsTab::About => about::about_tab(ws, cx),
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

