use aitoolplus_core::pi_extensions::{PiExtensionKind, PiExtensionScope};
use aitoolplus_core::pi_pages::PiModelSettings;
use aitoolplus_core::providers::{CATEGORIES, ProviderRecord};
use aitoolplus_core::session::{self, SessionMeta};
use aitoolplus_core::tools::ToolId;
use gpui::{Context, IntoElement, deferred, div, prelude::*, px, uniform_list};
use gpui_kit::base::{Align, ElementExt as _, Placement, Positioner, POPUP_PRIORITY};
use serde_json::Value;

use crate::components::{
    self, BadgeKind, ButtonVariant, Tooltip, badge, button_l, button_with_icon_l,
    button_with_icon_loading_l, input_container, page_header, section_title, text_area_scroll_container, textarea_container,
};
use crate::i18n::I18n;
use crate::text_area::TextArea;
use crate::text_input::TextInput;
use crate::theme::Theme;
use crate::workspace::Workspace;

use crate::pages::{PromptDialogState, ProviderDialogState, ToolTab, modal_scaffold_custom, modal_scaffold_sized};

use super::common::{open_in_browser, plugin_tag, spawn_tool_action};

pub fn load_grok_plugins(ws: &mut Workspace, cx: &mut Context<Workspace>) {
    if ws.ui.grok_plugins_loading {
        return;
    }
    ws.ui.grok_plugins_loading = true;
    let paths = ws.paths.clone();
    let weak = cx.entity().downgrade();
    cx.spawn(async move |_this, cx| {
        let result = cx
            .background_spawn(async move {
                aitoolplus_core::grok_plugins::list_all(&paths)
            })
            .await;
        let _ = weak.update(cx, |workspace, cx| {
            workspace.ui.grok_plugins = Some(result);
            workspace.ui.grok_plugins_loading = false;
            cx.notify();
        });
    })
    .detach();
}

pub fn load_claude_plugins(ws: &mut Workspace, cx: &mut Context<Workspace>) {
    if ws.ui.claude_plugins_loading {
        return;
    }
    ws.ui.claude_plugins_loading = true;
    let paths = ws.paths.clone();
    let weak = cx.entity().downgrade();
    cx.spawn(async move |_this, cx| {
        let result = cx
            .background_spawn(async move {
                aitoolplus_core::claude_plugins::list_all_plugins_data(&paths)
            })
            .await;
        let _ = weak.update(cx, |workspace, cx| {
            workspace.ui.claude_plugins = Some(result);
            workspace.ui.claude_plugins_loading = false;
            cx.notify();
        });
    })
    .detach();
}

// ---------------------------------------------------------------------------
// Pi Extensions tab
// ---------------------------------------------------------------------------

pub(super) fn grok_installed_plugins_section(ws: &mut Workspace, cx: &mut Context<Workspace>) -> gpui::AnyElement {
    let t = ws.theme.clone();
    let i = ws.i18n;

    if ws.ui.grok_plugins.is_none() && !ws.ui.grok_plugins_loading {
        load_grok_plugins(ws, cx);
    }

    let is_loading = ws.ui.grok_plugins_loading;
    let refresh_label = i.t("刷新", "Refresh");

    let (installed_count, can_enable_all, can_disable_all) =
        if let Some(Ok((installed, _))) = &ws.ui.grok_plugins {
            let inst_len = installed.len();
            let disabled_count = installed.iter().filter(|p| !p.enabled).count();
            let enabled_count = installed.iter().filter(|p| p.enabled).count();
            (inst_len, disabled_count > 0, enabled_count > 0)
        } else {
            (0, false, false)
        };

    let mut section = div().flex().flex_col().gap(px(12.0));

    let mut header_actions = div().flex().items_center().gap(px(8.0));

    if can_enable_all {
        header_actions = header_actions.child(button_l(
            "grok-plugins-enable-all",
            i.t("全部启用", "Enable All"),
            ButtonVariant::Secondary,
            &t,
            cx,
            move |ws, _, _, cx| {
                match aitoolplus_core::grok_plugins::set_all_enabled(&ws.paths, true) {
                    Ok(count) => {
                        let msg = ws
                            .i18n
                            .t(
                                &format!("已启用 {count} 个插件"),
                                &format!("enabled {count} plugins"),
                            )
                            .to_string();
                        ws.ui.toast(msg, false);
                        ws.ui.grok_plugins = None;
                        load_grok_plugins(ws, cx);
                    }
                    Err(e) => ws.ui.toast(format!("failed: {e}"), true),
                }
                cx.notify();
            },
        ));
    }
    if can_disable_all {
        header_actions = header_actions.child(button_l(
            "grok-plugins-disable-all",
            i.t("全部停用", "Disable All"),
            ButtonVariant::Secondary,
            &t,
            cx,
            move |ws, _, _, cx| {
                match aitoolplus_core::grok_plugins::set_all_enabled(&ws.paths, false) {
                    Ok(count) => {
                        let msg = ws
                            .i18n
                            .t(
                                &format!("已停用 {count} 个插件"),
                                &format!("disabled {count} plugins"),
                            )
                            .to_string();
                        ws.ui.toast(msg, false);
                        ws.ui.grok_plugins = None;
                        load_grok_plugins(ws, cx);
                    }
                    Err(e) => ws.ui.toast(format!("failed: {e}"), true),
                }
                cx.notify();
            },
        ));
    }

    header_actions = header_actions.child(button_with_icon_loading_l(
        "grok-plugins-refresh-btn",
        crate::icons::REFRESH_SVG,
        refresh_label,
        ButtonVariant::Secondary,
        is_loading,
        &t,
        cx,
        |ws, _, _, cx| {
            ws.ui.grok_plugins = None;
            load_grok_plugins(ws, cx);
            cx.notify();
        },
    ));

    section = section.child(
        div()
            .flex()
            .items_center()
            .justify_between()
            .child(page_header(
                &t,
                i.t("已安装插件", "Installed Plugins"),
                i.t(
                    &format!("管理已安装的 {} 个 Grok 插件", installed_count),
                    &format!("Manage {} installed Grok plugins", installed_count),
                ),
            ))
            .child(header_actions),
    );

    let Some(cached) = &ws.ui.grok_plugins else {
        return section
            .child(
                div()
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .p(px(48.0))
                    .gap(px(12.0))
                    .child(gpui_kit::component::spinner::Spinner::new().color(t.accent.into()))
                    .child(
                        div()
                            .text_size(px(12.5))
                            .text_color(t.text_muted)
                            .child(i.t("正在读取 Grok 插件…", "Loading Grok plugins…")),
                    ),
            )
            .into_any_element();
    };

    let (installed, _) = match cached {
        Ok(d) => d,
        Err(e) => {
            return section
                .child(crate::components::error_strip(
                    "opencode-plugins-read-err",
                    i.t("插件读取失败", "plugins read failed"),
                    &e,
                    &t,
                    cx,
                    None,
                ))
                .into_any_element();
        }
    };

    let filter = ws.ui.grok_installed_search.read(cx).text().trim().to_lowercase();
    let filtered_plugins: Vec<&aitoolplus_core::grok_plugins::GrokPlugin> = installed
        .iter()
        .filter(|p| {
            if filter.is_empty() {
                return true;
            }
            p.name.to_lowercase().contains(&filter)
                || p.plugin_id.to_lowercase().contains(&filter)
                || p.marketplace_name.to_lowercase().contains(&filter)
                || p.description.as_deref().unwrap_or_default().to_lowercase().contains(&filter)
        })
        .collect();

    let mut tab_col = div().flex().flex_col().gap(px(10.0));

    if !installed.is_empty() {
        tab_col = tab_col.child(
            div()
                .w_full()
                .child(input_container(&t, ws.ui.grok_installed_search.clone())),
        );
    }

    if installed.is_empty() {
        tab_col = tab_col.child(
            div()
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .p(px(40.0))
                .gap(px(8.0))
                .rounded(px(8.0))
                .bg(t.card_bg)
                .border_1()
                .border_color(t.card_border)
                .child(
                    div()
                        .text_size(px(14.0))
                        .font_weight(gpui::FontWeight::MEDIUM)
                        .text_color(t.text_primary)
                        .child(i.t("没有已安装插件", "No installed plugins")),
                )
                .child(
                    div()
                        .text_size(px(12.0))
                        .text_color(t.text_muted)
                        .child(i.t(
                            "可切换至「插件市场」标签页浏览并一键安装插件",
                            "Switch to Marketplace tab to discover and install plugins",
                        )),
                ),
        );
    } else if filtered_plugins.is_empty() {
        tab_col = tab_col.child(
            div()
                .flex()
                .items_center()
                .justify_center()
                .p(px(32.0))
                .text_size(px(12.5))
                .text_color(t.text_muted)
                .child(i.t("未找到匹配的插件", "No matching plugins found")),
        );
    } else {
        for plugin in filtered_plugins {
            let pid = plugin.plugin_id.clone();
            let pid_del = plugin.plugin_id.clone();
            let enabled = plugin.enabled;

            let mut card = div()
                .id(gpui::SharedString::from(format!("grok-p-{}", plugin.plugin_id)))
                .flex()
                .flex_col()
                .w_full()
                .min_w(px(0.0))
                .gap(px(6.0))
                .p(px(12.0))
                .rounded(px(8.0))
                .bg(t.card_bg)
                .border_1()
                .border_color(if enabled { gpui::rgba(0x22c55e44) } else { t.card_border })
                .shadow_xs();

            let mut header_row = div().flex().items_center().gap(px(8.0)).w_full();
            header_row = header_row.child(
                div()
                    .text_size(px(13.5))
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .text_color(t.text_primary)
                    .child(plugin.name.clone()),
            );
            header_row = header_row.child(badge(
                &t,
                if enabled { i.t("已启用", "Enabled") } else { i.t("已停用", "Disabled") },
                if enabled { BadgeKind::Success } else { BadgeKind::Neutral },
            ));
            header_row = header_row.child(
                div()
                    .flex()
                    .items_center()
                    .h(px(20.0))
                    .px(px(6.0))
                    .rounded(px(4.0))
                    .bg(t.sidebar_bg)
                    .border_1()
                    .border_color(t.card_border)
                    .text_size(px(11.0))
                    .text_color(t.text_secondary)
                    .child(plugin.marketplace_name.clone()),
            );
            if let Some(v) = &plugin.version {
                header_row = header_row.child(
                    div()
                        .flex()
                        .items_center()
                        .h(px(20.0))
                        .px(px(6.0))
                        .rounded(px(4.0))
                        .bg(t.sidebar_bg)
                        .border_1()
                        .border_color(t.card_border)
                        .text_size(px(11.0))
                        .text_color(t.text_muted)
                        .child(format!("v{v}")),
                );
            }
            header_row = header_row.child(div().flex_1());

            // Action buttons
            header_row = header_row.child(button_l(
                gpui::SharedString::from(format!("grok-toggle-{}", plugin.plugin_id)),
                if enabled { i.t("停用", "Disable") } else { i.t("启用", "Enable") },
                if enabled { ButtonVariant::Secondary } else { ButtonVariant::Primary },
                &t,
                cx,
                move |ws, _, _, cx| {
                    let id = pid.clone();
                    spawn_tool_action(
                        Some(ToolId::Grok),
                        ws,
                        cx,
                        "插件状态已更新".into(),
                        "plugin state updated".into(),
                        move |paths| aitoolplus_core::grok_plugins::enable(&paths, &id, !enabled),
                    );
                },
            ));
            header_row = header_row.child(button_with_icon_l(
                gpui::SharedString::from(format!("grok-del-{}", plugin.plugin_id)),
                crate::icons::TRASH_SVG,
                i.t("卸载", "Uninstall"),
                ButtonVariant::Danger,
                &t,
                cx,
                move |ws, _, _, cx| {
                    let id = pid_del.clone();
                    spawn_tool_action(
                        Some(ToolId::Grok),
                        ws,
                        cx,
                        "插件已卸载".into(),
                        "plugin uninstalled".into(),
                        move |paths| aitoolplus_core::grok_plugins::uninstall(&paths, &id),
                    );
                },
            ));

            card = card.child(header_row);

            // Row 2: Description
            if let Some(desc) = &plugin.description {
                card = card.child(
                    div()
                        .text_size(px(12.0))
                        .text_color(t.text_secondary)
                        .child(desc.clone()),
                );
            }

            // Row 3: Capabilities
            if !plugin.capabilities.is_empty() {
                let mut caps_row = div().flex().items_center().gap(px(4.0)).flex_wrap();
                for cap in &plugin.capabilities {
                    caps_row = caps_row.child(plugin_tag(cap.clone(), t.accent_subtle, t.accent));
                }
                card = card.child(caps_row);
            }

            tab_col = tab_col.child(card);
        }
    }

    section = section.child(tab_col);
    section.into_any_element()
}

pub(super) fn grok_marketplace_section(ws: &mut Workspace, cx: &mut Context<Workspace>) -> gpui::AnyElement {
    let t = ws.theme.clone();
    let i = ws.i18n;

    if ws.ui.grok_plugins.is_none() && !ws.ui.grok_plugins_loading {
        load_grok_plugins(ws, cx);
    }

    let is_loading = ws.ui.grok_plugins_loading;
    let refresh_label = i.t("刷新", "Refresh");

    let mut section = div().flex().flex_col().w_full().h_full().min_h(px(0.0)).gap(px(10.0));

    let Some(cached) = &ws.ui.grok_plugins else {
        return section
            .child(
                div()
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .p(px(48.0))
                    .gap(px(12.0))
                    .child(gpui_kit::component::spinner::Spinner::new().color(t.accent.into()))
                    .child(
                        div()
                            .text_size(px(12.5))
                            .text_color(t.text_muted)
                            .child(i.t("正在读取 Grok 插件市场…", "Loading Grok marketplace plugins…")),
                    ),
            )
            .into_any_element();
    };

    let (installed, available) = match cached {
        Ok(d) => d,
        Err(e) => {
            return section
                .child(crate::components::error_strip(
                    "opencode-market-read-err",
                    i.t("插件市场读取失败", "marketplace read failed"),
                    &e,
                    &t,
                    cx,
                    None,
                ))
                .into_any_element();
        }
    };

    let m_filter = ws.ui.grok_market_search.read(cx).text().trim().to_lowercase();
    let installed_set: std::collections::HashSet<String> = installed
        .iter()
        .map(|p| p.plugin_id.clone())
        .collect();

    let filtered_mkt: Vec<&aitoolplus_core::grok_plugins::GrokPlugin> = available
        .iter()
        .filter(|p| {
            if m_filter.is_empty() {
                return true;
            }
            p.name.to_lowercase().contains(&m_filter)
                || p.plugin_id.to_lowercase().contains(&m_filter)
                || p.marketplace_name.to_lowercase().contains(&m_filter)
                || p.description.as_deref().unwrap_or_default().to_lowercase().contains(&m_filter)
        })
        .collect();

    // Unified Toolbar
    let mut toolbar = div().flex().items_center().gap(px(10.0)).w_full();
    toolbar = toolbar.child(
        div()
            .flex_1()
            .min_w(px(0.0))
            .child(input_container(&t, ws.ui.grok_market_search.clone())),
    );
    toolbar = toolbar.child(
        div()
            .text_size(px(12.0))
            .text_color(t.text_muted)
            .child(format!(
                "{} {} {}",
                i.t("匹配到", "Matched"),
                filtered_mkt.len(),
                i.t("个插件", "plugins")
            )),
    );
    toolbar = toolbar.child(button_with_icon_loading_l(
        "grok-market-refresh-btn",
        crate::icons::REFRESH_SVG,
        refresh_label,
        ButtonVariant::Secondary,
        is_loading,
        &t,
        cx,
        |ws, _, _, cx| {
            ws.ui.grok_plugins = None;
            load_grok_plugins(ws, cx);
            cx.notify();
        },
    ));

    section = section.child(toolbar);

    if available.is_empty() {
        section = section.child(
            div()
                .flex()
                .items_center()
                .justify_center()
                .p(px(32.0))
                .text_size(px(12.5))
                .text_color(t.text_muted)
                .child(i.t("暂无市场插件", "No marketplace plugins available")),
        );
    } else if filtered_mkt.is_empty() {
        section = section.child(
            div()
                .flex()
                .items_center()
                .justify_center()
                .p(px(32.0))
                .text_size(px(12.5))
                .text_color(t.text_muted)
                .child(i.t("未找到匹配的市场插件", "No matching marketplace plugins found")),
        );
    } else {
        let items: std::sync::Arc<Vec<aitoolplus_core::grok_plugins::GrokPlugin>> =
            std::sync::Arc::new(filtered_mkt.into_iter().cloned().collect());
        let items_len = items.len();
        let items_for_list = items.clone();
        let installed_set = std::sync::Arc::new(installed_set);
        let ws_entity = cx.entity();
        let t_clone = t.clone();
        let i_clone = i.clone();

        let v_list = uniform_list(
            "grok-mkt-virtual-list",
            items_len,
            move |range: std::ops::Range<usize>, _window: &mut gpui::Window, _cx: &mut gpui::App| -> Vec<gpui::AnyElement> {
                let mut elements = Vec::with_capacity(range.len());
                for idx in range {
                    if let Some(plugin) = items_for_list.get(idx) {
                        elements.push(render_virtual_grok_marketplace_card(
                            plugin,
                            &installed_set,
                            &ws_entity,
                            &t_clone,
                            &i_clone,
                        ));
                    }
                }
                elements
            },
        )
        .size_full();

        section = section.child(
            div()
                .w_full()
                .flex_1()
                .h_full()
                .min_h(px(0.0))
                .overflow_hidden()
                .rounded(px(8.0))
                .bg(t.sidebar_bg)
                .border_1()
                .border_color(t.card_border)
                .p(px(8.0))
                .child(v_list),
        );
    }

    section.into_any_element()
}

pub(super) fn render_virtual_grok_marketplace_card(
    plugin: &aitoolplus_core::grok_plugins::GrokPlugin,
    installed_set: &std::collections::HashSet<String>,
    ws_entity: &gpui::Entity<Workspace>,
    t: &Theme,
    i: &I18n,
) -> gpui::AnyElement {
    let is_installed = installed_set.contains(&plugin.plugin_id);
    let action_plugin = plugin.clone();

    let mut card = div()
        .id(gpui::SharedString::from(format!("v-grok-card-{}", plugin.plugin_id)))
        .flex()
        .flex_col()
        .justify_between()
        .w_full()
        .min_w(px(0.0))
        .h(px(90.0))
        .p(px(10.0))
        .rounded(px(6.0))
        .bg(t.card_bg)
        .border_1()
        .border_color(t.card_border)
        .shadow_xs()
        .hover({
            let bg = t.card_hover;
            let border = t.card_border_hover;
            move |h| h.bg(bg).border_color(border)
        });

    let mut row1 = div().flex().items_center().gap(px(8.0)).w_full();
    row1 = row1.child(
        div()
            .text_size(px(13.0))
            .font_weight(gpui::FontWeight::SEMIBOLD)
            .text_color(t.text_primary)
            .child(plugin.name.clone()),
    );
    if is_installed {
        row1 = row1.child(badge(t, i.t("已安装", "Installed"), BadgeKind::Success));
    }
    row1 = row1.child(
        div()
            .flex()
            .items_center()
            .h(px(18.0))
            .px(px(5.0))
            .rounded(px(3.0))
            .bg(t.sidebar_bg)
            .border_1()
            .border_color(t.card_border)
            .text_size(px(10.5))
            .text_color(t.text_secondary)
            .child(plugin.marketplace_name.clone()),
    );
    row1 = row1.child(div().flex_1());

    if !is_installed {
        let ws_entity = ws_entity.clone();
        let bg = t.accent;
        let hover_bg = t.accent_hover;
        row1 = row1.child(
            div()
                .id(gpui::SharedString::from(format!("v-grok-inst-{}", plugin.plugin_id)))
                .cursor_pointer()
                .flex()
                .items_center()
                .justify_center()
                .gap(px(4.0))
                .h(px(26.0))
                .px(px(12.0))
                .rounded(px(6.0))
                .bg(bg)
                .text_color(gpui::white())
                .text_size(px(12.0))
                .font_weight(gpui::FontWeight::MEDIUM)
                .shadow_xs()
                .hover(move |h| h.bg(hover_bg))
                .on_click(move |_ev, _win, cx| {
                    let plugin = action_plugin.clone();
                    let _ = ws_entity.update(cx, |ws, cx| {
                        spawn_tool_action(
                            Some(ToolId::Grok),
                            ws,
                            cx,
                            "插件已安装".into(),
                            "plugin installed".into(),
                            move |paths| aitoolplus_core::grok_plugins::install(&paths, &plugin),
                        );
                    });
                })
                .child(i.t("安装并信任", "Install & Trust")),
        );
    } else {
        row1 = row1.child(
            div()
                .flex()
                .items_center()
                .h(px(26.0))
                .px(px(10.0))
                .rounded(px(6.0))
                .bg(t.sidebar_bg)
                .border_1()
                .border_color(t.card_border)
                .text_size(px(11.5))
                .text_color(t.text_muted)
                .child(i.t("已安装", "Installed")),
        );
    }

    card = card.child(row1);

    if let Some(desc) = &plugin.description {
        card = card.child(
            div()
                .text_size(px(11.5))
                .text_color(t.text_secondary)
                .overflow_hidden()
                .text_ellipsis()
                .whitespace_nowrap()
                .child(desc.clone()),
        );
    }

    if !plugin.capabilities.is_empty() {
        let mut caps_row = div().flex().items_center().gap(px(4.0)).flex_wrap();
        for cap in &plugin.capabilities {
            caps_row = caps_row.child(plugin_tag(cap.clone(), t.accent_subtle, t.accent));
        }
        card = card.child(caps_row);
    }

    card.into_any_element()
}

pub(super) fn load_codex_plugins(ws: &mut Workspace, cx: &mut Context<Workspace>) {
    ws.ui.codex_plugins_loading = true;
    let paths = ws.paths.clone();
    let weak = cx.entity().downgrade();
    cx.spawn(async move |_this, cx| {
        let res = cx
            .background_spawn(async move { aitoolplus_core::codex_plugins::list_all(&paths) })
            .await;
        let _ = weak.update(cx, |ws, cx| {
            ws.ui.codex_plugins = Some(res);
            ws.ui.codex_plugins_loading = false;
            cx.notify();
        });
    })
    .detach();
}

pub(super) fn codex_installed_plugins_section(ws: &mut Workspace, cx: &mut Context<Workspace>) -> gpui::AnyElement {
    let t = ws.theme.clone();
    let i = ws.i18n;

    if ws.ui.codex_plugins.is_none() && !ws.ui.codex_plugins_loading {
        load_codex_plugins(ws, cx);
    }

    let is_loading = ws.ui.codex_plugins_loading;
    let refresh_label = i.t("刷新", "Refresh");

    let (installed_count, can_enable_all, can_disable_all) =
        if let Some(Ok(data)) = &ws.ui.codex_plugins {
            let inst_len = data.installed_plugins.len();
            let disabled_count = data.installed_plugins.iter().filter(|p| !p.enabled).count();
            let enabled_count = data.installed_plugins.iter().filter(|p| p.enabled).count();
            (inst_len, disabled_count > 0, enabled_count > 0)
        } else {
            (0, false, false)
        };

    let mut section = div().flex().flex_col().gap(px(12.0));

    let mut header_actions = div().flex().items_center().gap(px(8.0));

    if can_enable_all {
        header_actions = header_actions.child(button_l(
            "codex-plugins-enable-all",
            i.t("全部启用", "Enable All"),
            ButtonVariant::Secondary,
            &t,
            cx,
            move |ws, _, _, cx| {
                match aitoolplus_core::codex_plugins::set_all_plugins_enabled(&ws.paths, true) {
                    Ok(count) => {
                        let msg = ws
                            .i18n
                            .t(
                                &format!("已启用 {count} 个插件"),
                                &format!("enabled {count} plugins"),
                            )
                            .to_string();
                        ws.ui.toast(msg, false);
                        ws.ui.codex_plugins = None;
                        load_codex_plugins(ws, cx);
                    }
                    Err(e) => ws.ui.toast(format!("failed: {e}"), true),
                }
                cx.notify();
            },
        ));
    }
    if can_disable_all {
        header_actions = header_actions.child(button_l(
            "codex-plugins-disable-all",
            i.t("全部停用", "Disable All"),
            ButtonVariant::Secondary,
            &t,
            cx,
            move |ws, _, _, cx| {
                match aitoolplus_core::codex_plugins::set_all_plugins_enabled(&ws.paths, false) {
                    Ok(count) => {
                        let msg = ws
                            .i18n
                            .t(
                                &format!("已停用 {count} 个插件"),
                                &format!("disabled {count} plugins"),
                            )
                            .to_string();
                        ws.ui.toast(msg, false);
                        ws.ui.codex_plugins = None;
                        load_codex_plugins(ws, cx);
                    }
                    Err(e) => ws.ui.toast(format!("failed: {e}"), true),
                }
                cx.notify();
            },
        ));
    }

    header_actions = header_actions.child(button_with_icon_loading_l(
        "codex-plugins-refresh-btn",
        crate::icons::REFRESH_SVG,
        refresh_label,
        ButtonVariant::Secondary,
        is_loading,
        &t,
        cx,
        |ws, _, _, cx| {
            ws.ui.codex_plugins = None;
            load_codex_plugins(ws, cx);
            cx.notify();
        },
    ));

    section = section.child(
        div()
            .flex()
            .items_center()
            .justify_between()
            .child(page_header(
                &t,
                i.t("已安装插件", "Installed Plugins"),
                i.t(
                    &format!("管理已安装的 {} 个 Codex 插件", installed_count),
                    &format!("Manage {} installed Codex plugins", installed_count),
                ),
            ))
            .child(header_actions),
    );

    let Some(cached) = &ws.ui.codex_plugins else {
        return section
            .child(
                div()
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .p(px(48.0))
                    .gap(px(12.0))
                    .child(gpui_kit::component::spinner::Spinner::new().color(t.accent.into()))
                    .child(
                        div()
                            .text_size(px(12.5))
                            .text_color(t.text_muted)
                            .child(i.t("正在读取 Codex 插件…", "Loading Codex plugins…")),
                    ),
            )
            .into_any_element();
    };

    let data = match cached {
        Ok(d) => d,
        Err(e) => {
            return section
                .child(crate::components::error_strip(
                    "codex-plugins-read-err",
                    i.t("插件读取失败", "plugins read failed"),
                    &e,
                    &t,
                    cx,
                    None,
                ))
                .into_any_element();
        }
    };

    let filter = ws.ui.codex_installed_search.read(cx).text().trim().to_lowercase();
    let filtered_plugins: Vec<&aitoolplus_core::codex_plugins::CodexInstalledPlugin> = data
        .installed_plugins
        .iter()
        .filter(|p| {
            if filter.is_empty() {
                return true;
            }
            p.name.to_lowercase().contains(&filter)
                || p.plugin_id.to_lowercase().contains(&filter)
                || p.marketplace_name.to_lowercase().contains(&filter)
                || p.description.as_deref().unwrap_or_default().to_lowercase().contains(&filter)
        })
        .collect();

    let mut tab_col = div().flex().flex_col().gap(px(10.0));

    if !data.installed_plugins.is_empty() {
        tab_col = tab_col.child(
            div()
                .w_full()
                .child(input_container(&t, ws.ui.codex_installed_search.clone())),
        );
    }

    if data.installed_plugins.is_empty() {
        tab_col = tab_col.child(
            div()
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .p(px(40.0))
                .gap(px(8.0))
                .rounded(px(8.0))
                .bg(t.card_bg)
                .border_1()
                .border_color(t.card_border)
                .child(
                    div()
                        .text_size(px(14.0))
                        .font_weight(gpui::FontWeight::MEDIUM)
                        .text_color(t.text_primary)
                        .child(i.t("没有已安装插件", "No installed plugins")),
                )
                .child(
                    div()
                        .text_size(px(12.0))
                        .text_color(t.text_muted)
                        .child(i.t(
                            "可切换至「插件市场」标签页浏览并一键安装插件",
                            "Switch to Marketplace tab to discover and install plugins",
                        )),
                ),
        );
    } else if filtered_plugins.is_empty() {
        tab_col = tab_col.child(
            div()
                .flex()
                .items_center()
                .justify_center()
                .p(px(32.0))
                .text_size(px(12.5))
                .text_color(t.text_muted)
                .child(i.t("未找到匹配的插件", "No matching plugins found")),
        );
    } else {
        for plugin in filtered_plugins {
            let pid = plugin.plugin_id.clone();
            let pid_del = plugin.plugin_id.clone();
            let enabled = plugin.enabled;

            let mut card = div()
                .id(gpui::SharedString::from(format!("codex-p-{}", plugin.plugin_id)))
                .flex()
                .flex_col()
                .w_full()
                .min_w(px(0.0))
                .gap(px(6.0))
                .p(px(12.0))
                .rounded(px(8.0))
                .bg(t.card_bg)
                .border_1()
                .border_color(if enabled { gpui::rgba(0x22c55e44) } else { t.card_border })
                .shadow_xs();

            let mut header_row = div().flex().items_center().gap(px(8.0)).w_full();
            header_row = header_row.child(
                div()
                    .text_size(px(13.5))
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .text_color(t.text_primary)
                    .child(plugin.name.clone()),
            );
            header_row = header_row.child(badge(
                &t,
                if enabled { i.t("已启用", "Enabled") } else { i.t("已停用", "Disabled") },
                if enabled { BadgeKind::Success } else { BadgeKind::Neutral },
            ));
            header_row = header_row.child(
                div()
                    .flex()
                    .items_center()
                    .h(px(20.0))
                    .px(px(6.0))
                    .rounded(px(4.0))
                    .bg(t.sidebar_bg)
                    .border_1()
                    .border_color(t.card_border)
                    .text_size(px(11.0))
                    .text_color(t.text_secondary)
                    .child(plugin.marketplace_name.clone()),
            );
            if let Some(v) = &plugin.active_version {
                header_row = header_row.child(
                    div()
                        .flex()
                        .items_center()
                        .h(px(20.0))
                        .px(px(6.0))
                        .rounded(px(4.0))
                        .bg(t.sidebar_bg)
                        .border_1()
                        .border_color(t.card_border)
                        .text_size(px(11.0))
                        .text_color(t.text_muted)
                        .child(format!("v{v}")),
                );
            }
            header_row = header_row.child(div().flex_1());

            // Action buttons
            header_row = header_row.child(button_l(
                gpui::SharedString::from(format!("codex-toggle-{}", plugin.plugin_id)),
                if enabled { i.t("停用", "Disable") } else { i.t("启用", "Enable") },
                if enabled { ButtonVariant::Secondary } else { ButtonVariant::Primary },
                &t,
                cx,
                move |ws, _, _, cx| {
                    let id = pid.clone();
                    match aitoolplus_core::codex_plugins::set_plugin_enabled(&ws.paths, &id, !enabled) {
                        Ok(_) => {
                            let msg = if !enabled {
                                ws.i18n.t("插件已启用", "plugin enabled")
                            } else {
                                ws.i18n.t("插件已停用", "plugin disabled")
                            };
                            ws.ui.toast(msg.to_string(), false);
                            ws.ui.codex_plugins = None;
                            load_codex_plugins(ws, cx);
                        }
                        Err(e) => ws.ui.toast(format!("failed: {e}"), true),
                    }
                    cx.notify();
                },
            ));
            header_row = header_row.child(button_with_icon_l(
                gpui::SharedString::from(format!("codex-del-{}", plugin.plugin_id)),
                crate::icons::TRASH_SVG,
                i.t("卸载", "Uninstall"),
                ButtonVariant::Danger,
                &t,
                cx,
                move |ws, _, _, cx| {
                    let id = pid_del.clone();
                    match aitoolplus_core::codex_plugins::uninstall_plugin(&ws.paths, &id) {
                        Ok(_) => {
                            ws.ui.toast(ws.i18n.t("插件已卸载", "plugin uninstalled").to_string(), false);
                            ws.ui.codex_plugins = None;
                            load_codex_plugins(ws, cx);
                        }
                        Err(e) => ws.ui.toast(format!("failed: {e}"), true),
                    }
                    cx.notify();
                },
            ));

            card = card.child(header_row);

            if let Some(desc) = &plugin.description {
                card = card.child(
                    div()
                        .text_size(px(12.0))
                        .text_color(t.text_secondary)
                        .child(desc.clone()),
                );
            }

            if let Some(path) = &plugin.installed_path {
                card = card.child(
                    div()
                        .text_size(px(11.0))
                        .text_color(t.text_muted)
                        .child(format!("{}: {}", i.t("路径", "Path"), path)),
                );
            }

            tab_col = tab_col.child(card);
        }
    }

    section = section.child(tab_col);
    section.into_any_element()
}

pub(super) fn codex_marketplace_section(ws: &mut Workspace, cx: &mut Context<Workspace>) -> gpui::AnyElement {
    let t = ws.theme.clone();
    let i = ws.i18n;

    if ws.ui.codex_plugins.is_none() && !ws.ui.codex_plugins_loading {
        load_codex_plugins(ws, cx);
    }

    let is_loading = ws.ui.codex_plugins_loading;
    let refresh_label = i.t("刷新", "Refresh");

    let mut section = div().flex().flex_col().w_full().h_full().min_h(px(0.0)).gap(px(10.0));

    let Some(cached) = &ws.ui.codex_plugins else {
        return section
            .child(
                div()
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .p(px(48.0))
                    .gap(px(12.0))
                    .child(gpui_kit::component::spinner::Spinner::new().color(t.accent.into()))
                    .child(
                        div()
                            .text_size(px(12.5))
                            .text_color(t.text_muted)
                            .child(i.t("正在读取 Codex 插件市场…", "Loading Codex marketplace plugins…")),
                    ),
            )
            .into_any_element();
    };

    let data = match cached {
        Ok(d) => d,
        Err(e) => {
            return section
                .child(crate::components::error_strip(
                    "codex-market-read-err",
                    i.t("插件市场读取失败", "marketplace read failed"),
                    &e,
                    &t,
                    cx,
                    None,
                ))
                .into_any_element();
        }
    };

    let m_filter = ws.ui.codex_market_search.read(cx).text().trim().to_lowercase();
    let installed_set: std::collections::HashSet<String> = data
        .installed_plugins
        .iter()
        .map(|p| p.plugin_id.clone())
        .collect();

    let filtered_mkt: Vec<&aitoolplus_core::codex_plugins::CodexMarketplacePlugin> = data
        .marketplace_plugins
        .iter()
        .filter(|p| {
            if m_filter.is_empty() {
                return true;
            }
            p.name.to_lowercase().contains(&m_filter)
                || p.plugin_id.to_lowercase().contains(&m_filter)
                || p.marketplace_name.to_lowercase().contains(&m_filter)
                || p.description.as_deref().unwrap_or_default().to_lowercase().contains(&m_filter)
                || p.category.as_deref().unwrap_or_default().to_lowercase().contains(&m_filter)
                || p.capabilities.iter().any(|t| t.to_lowercase().contains(&m_filter))
        })
        .collect();

    let mut toolbar = div().flex().items_center().gap(px(10.0)).w_full();
    toolbar = toolbar.child(
        div()
            .flex_1()
            .min_w(px(0.0))
            .child(input_container(&t, ws.ui.codex_market_search.clone())),
    );
    toolbar = toolbar.child(
        div()
            .text_size(px(12.0))
            .text_color(t.text_muted)
            .child(format!(
                "{} {} {}",
                i.t("匹配到", "Matched"),
                filtered_mkt.len(),
                i.t("个插件", "plugins")
            )),
    );
    toolbar = toolbar.child(button_with_icon_loading_l(
        "codex-market-refresh-btn",
        crate::icons::REFRESH_SVG,
        refresh_label,
        ButtonVariant::Secondary,
        is_loading,
        &t,
        cx,
        |ws, _, _, cx| {
            ws.ui.codex_plugins = None;
            load_codex_plugins(ws, cx);
            cx.notify();
        },
    ));

    section = section.child(toolbar);

    if data.marketplace_plugins.is_empty() {
        section = section.child(
            div()
                .flex()
                .items_center()
                .justify_center()
                .p(px(32.0))
                .text_size(px(12.5))
                .text_color(t.text_muted)
                .child(i.t("暂无市场插件", "No marketplace plugins available")),
        );
    } else if filtered_mkt.is_empty() {
        section = section.child(
            div()
                .flex()
                .items_center()
                .justify_center()
                .p(px(32.0))
                .text_size(px(12.5))
                .text_color(t.text_muted)
                .child(i.t("未找到匹配的市场插件", "No matching marketplace plugins found")),
        );
    } else {
        let items: std::sync::Arc<Vec<aitoolplus_core::codex_plugins::CodexMarketplacePlugin>> =
            std::sync::Arc::new(filtered_mkt.into_iter().cloned().collect());
        let items_len = items.len();
        let items_for_list = items.clone();
        let installed_set = std::sync::Arc::new(installed_set);
        let ws_entity = cx.entity();
        let t_clone = t.clone();
        let i_clone = i.clone();

        let v_list = uniform_list(
            "codex-mkt-virtual-list",
            items_len,
            move |range: std::ops::Range<usize>, _window: &mut gpui::Window, _cx: &mut gpui::App| -> Vec<gpui::AnyElement> {
                let mut elements = Vec::with_capacity(range.len());
                for idx in range {
                    if let Some(plugin) = items_for_list.get(idx) {
                        elements.push(render_virtual_codex_marketplace_card(
                            plugin,
                            &installed_set,
                            &ws_entity,
                            &t_clone,
                            &i_clone,
                        ));
                    }
                }
                elements
            },
        )
        .size_full();

        section = section.child(
            div()
                .w_full()
                .flex_1()
                .h_full()
                .min_h(px(0.0))
                .overflow_hidden()
                .rounded(px(8.0))
                .bg(t.sidebar_bg)
                .border_1()
                .border_color(t.card_border)
                .p(px(8.0))
                .child(v_list),
        );
    }

    section.into_any_element()
}

pub(super) fn render_virtual_codex_marketplace_card(
    plugin: &aitoolplus_core::codex_plugins::CodexMarketplacePlugin,
    installed_set: &std::collections::HashSet<String>,
    ws_entity: &gpui::Entity<Workspace>,
    t: &Theme,
    i: &I18n,
) -> gpui::AnyElement {
    let is_installed = installed_set.contains(&plugin.plugin_id);

    let mut card = div()
        .id(gpui::SharedString::from(format!("v-codex-card-{}", plugin.plugin_id)))
        .flex()
        .flex_col()
        .justify_between()
        .w_full()
        .min_w(px(0.0))
        .h(px(100.0))
        .p(px(10.0))
        .rounded(px(8.0))
        .bg(t.card_bg)
        .border_1()
        .border_color(t.card_border)
        .shadow_xs()
        .hover({
            let bg = t.card_hover;
            let border = t.card_border_hover;
            move |h| h.bg(bg).border_color(border)
        });

    let mut header_row = div().flex().items_center().gap(px(8.0)).w_full();
    header_row = header_row.child(
        div()
            .text_size(px(13.5))
            .font_weight(gpui::FontWeight::SEMIBOLD)
            .text_color(t.text_primary)
            .child(plugin.name.clone()),
    );
    header_row = header_row.child(
        div()
            .flex()
            .items_center()
            .h(px(20.0))
            .px(px(6.0))
            .rounded(px(4.0))
            .bg(t.sidebar_bg)
            .border_1()
            .border_color(t.card_border)
            .text_size(px(11.0))
            .text_color(t.text_secondary)
            .child(plugin.marketplace_name.clone()),
    );
    if let Some(cat) = &plugin.category {
        header_row = header_row.child(plugin_tag(cat.clone(), t.accent_subtle, t.accent));
    }
    if is_installed {
        header_row = header_row.child(badge(t, i.t("已安装", "Installed"), BadgeKind::Success));
    }
    header_row = header_row.child(div().flex_1());

    if is_installed {
        header_row = header_row.child(
            div()
                .flex()
                .items_center()
                .h(px(26.0))
                .px(px(10.0))
                .rounded(px(6.0))
                .bg(t.sidebar_bg)
                .border_1()
                .border_color(t.card_border)
                .text_size(px(11.5))
                .text_color(t.text_muted)
                .child(i.t("已安装", "Installed")),
        );
    } else {
        let ws_entity = ws_entity.clone();
        let to_install = plugin.plugin_id.clone();
        let bg = t.accent;
        let hover_bg = t.accent_hover;
        header_row = header_row.child(
            div()
                .id(gpui::SharedString::from(format!("v-install-codex-{}", plugin.plugin_id)))
                .cursor_pointer()
                .flex()
                .items_center()
                .justify_center()
                .gap(px(4.0))
                .h(px(26.0))
                .px(px(12.0))
                .rounded(px(6.0))
                .bg(bg)
                .text_color(gpui::white())
                .text_size(px(12.0))
                .font_weight(gpui::FontWeight::MEDIUM)
                .shadow_xs()
                .hover(move |h| h.bg(hover_bg))
                .on_click(move |_event, _window, cx| {
                    let pid = to_install.clone();
                    let _ = ws_entity.update(cx, |ws, cx| {
                        match aitoolplus_core::codex_plugins::install_plugin(&ws.paths, &pid) {
                            Ok(_) => {
                                ws.ui.toast(ws.i18n.t("插件已安装", "plugin installed").to_string(), false);
                                ws.ui.codex_plugins = None;
                                load_codex_plugins(ws, cx);
                            }
                            Err(e) => ws.ui.toast(format!("failed: {e}"), true),
                        }
                        cx.notify();
                    });
                })
                .child(i.t("安装", "Install")),
        );
    }

    card = card.child(header_row);

    if let Some(desc) = &plugin.description {
        card = card.child(
            div()
                .text_size(px(12.0))
                .text_color(t.text_secondary)
                .overflow_hidden()
                .text_ellipsis()
                .whitespace_nowrap()
                .child(desc.clone()),
        );
    }

    let mut caps_row = div().flex().items_center().gap(px(4.0)).flex_wrap();
    for cap in &plugin.capabilities {
        caps_row = caps_row.child(plugin_tag(cap.clone(), gpui::rgba(0x3b82f620), gpui::rgb(0x2563eb)));
    }
    card = card.child(caps_row);

    card.into_any_element()
}

pub(super) fn claude_installed_plugins_section(ws: &mut Workspace, cx: &mut Context<Workspace>) -> gpui::AnyElement {
    let t = ws.theme.clone();
    let i = ws.i18n;

    if ws.ui.claude_plugins.is_none() && !ws.ui.claude_plugins_loading {
        load_claude_plugins(ws, cx);
    }

    let is_loading = ws.ui.claude_plugins_loading;
    let refresh_label = i.t("刷新", "Refresh");

    let (installed_count, can_enable_all, can_disable_all) =
        if let Some(Ok(data)) = &ws.ui.claude_plugins {
            let inst_len = data.installed_plugins.len();
            let disabled_count = data
                .installed_plugins
                .iter()
                .filter(|p| p.user_scope_installed && !p.user_scope_enabled)
                .count();
            let enabled_count = data
                .installed_plugins
                .iter()
                .filter(|p| p.user_scope_installed && p.user_scope_enabled)
                .count();
            (inst_len, disabled_count > 0, enabled_count > 0)
        } else {
            (0, false, false)
        };

    let mut section = div().flex().flex_col().gap(px(12.0));

    // ---- Top Header with Actions ----
    let mut header_actions = div().flex().items_center().gap(px(8.0));

    if can_enable_all {
        header_actions = header_actions.child(button_l(
            "plugins-enable-all-top",
            i.t("全部启用", "Enable All"),
            ButtonVariant::Secondary,
            &t,
            cx,
            move |ws, _, _, cx| {
                match aitoolplus_core::claude_plugins::set_all_plugins_enabled(&ws.paths, true) {
                    Ok((count, _)) => {
                        let msg = ws
                            .i18n
                            .t(
                                &format!("已启用 {count} 个插件"),
                                &format!("enabled {count} plugins"),
                            )
                            .to_string();
                        ws.ui.toast(msg, false);
                        ws.ui.claude_plugins = None;
                        load_claude_plugins(ws, cx);
                    }
                    Err(e) => ws.ui.toast(format!("failed: {e}"), true),
                }
                cx.notify();
            },
        ));
    }
    if can_disable_all {
        header_actions = header_actions.child(button_l(
            "plugins-disable-all-top",
            i.t("全部停用", "Disable All"),
            ButtonVariant::Secondary,
            &t,
            cx,
            move |ws, _, _, cx| {
                match aitoolplus_core::claude_plugins::set_all_plugins_enabled(&ws.paths, false) {
                    Ok((count, _)) => {
                        let msg = ws
                            .i18n
                            .t(
                                &format!("已停用 {count} 个插件"),
                                &format!("disabled {count} plugins"),
                            )
                            .to_string();
                        ws.ui.toast(msg, false);
                        ws.ui.claude_plugins = None;
                        load_claude_plugins(ws, cx);
                    }
                    Err(e) => ws.ui.toast(format!("failed: {e}"), true),
                }
                cx.notify();
            },
        ));
    }

    header_actions = header_actions.child(button_with_icon_loading_l(
        "claude-plugins-refresh-btn",
        crate::icons::REFRESH_SVG,
        refresh_label,
        ButtonVariant::Secondary,
        is_loading,
        &t,
        cx,
        |ws, _, _, cx| {
            ws.ui.claude_plugins = None;
            load_claude_plugins(ws, cx);
            cx.notify();
        },
    ));

    header_actions = header_actions.child(button_with_icon_l(
        "claude-plugins-docs-btn",
        crate::icons::BOOK_OPEN_SVG,
        i.t("插件文档", "Docs"),
        ButtonVariant::Ghost,
        &t,
        cx,
        |_ws, _, _, _| {
            open_in_browser("https://code.claude.com/docs/en/discover-plugins");
        },
    ));

    section = section.child(
        div()
            .flex()
            .items_center()
            .justify_between()
            .child(page_header(
                &t,
                i.t("已安装插件", "Installed Plugins"),
                i.t(
                    &format!("管理已安装的 {} 个 Claude Code 插件", installed_count),
                    &format!("Manage {} installed Claude Code plugins", installed_count),
                ),
            ))
            .child(header_actions),
    );

    // ---- Content Body ----
    let Some(cached) = &ws.ui.claude_plugins else {
        return section
            .child(
                div()
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .p(px(48.0))
                    .gap(px(12.0))
                    .child(gpui_kit::component::spinner::Spinner::new().color(t.accent.into()))
                    .child(
                        div()
                            .text_size(px(12.5))
                            .text_color(t.text_muted)
                            .child(i.t("正在读取插件信息…", "Loading plugins…")),
                    ),
            )
            .into_any_element();
    };

    let data = match cached {
        Ok(d) => d,
        Err(e) => {
            return section
                .child(crate::components::error_strip(
                    "claude-plugins-read-err",
                    i.t("插件读取失败", "plugins read failed"),
                    &e,
                    &t,
                    cx,
                    None,
                ))
                .into_any_element();
        }
    };

    let filter = ws
        .ui
        .claude_installed_search
        .read(cx)
        .text()
        .trim()
        .to_lowercase();

            let filtered_plugins: Vec<&aitoolplus_core::claude_plugins::InstalledPlugin> = data
                .installed_plugins
                .iter()
                .filter(|p| {
                    if filter.is_empty() {
                        return true;
                    }
                    p.name.to_lowercase().contains(&filter)
                        || p.plugin_id.to_lowercase().contains(&filter)
                        || p.marketplace_name.to_lowercase().contains(&filter)
                        || p.description
                            .as_deref()
                            .unwrap_or_default()
                            .to_lowercase()
                            .contains(&filter)
                })
                .collect();

            let mut tab_col = div().flex().flex_col().gap(px(10.0));

            // Search toolbar
            if !data.installed_plugins.is_empty() {
                tab_col = tab_col.child(
                    div()
                        .w_full()
                        .child(input_container(&t, ws.ui.claude_installed_search.clone())),
                );
            }

            if data.installed_plugins.is_empty() {
                tab_col = tab_col.child(
                    div()
                        .flex()
                        .flex_col()
                        .items_center()
                        .justify_center()
                        .p(px(40.0))
                        .gap(px(8.0))
                        .rounded(px(8.0))
                        .bg(t.card_bg)
                        .border_1()
                        .border_color(t.card_border)
                        .child(
                            div()
                                .text_size(px(14.0))
                                .font_weight(gpui::FontWeight::MEDIUM)
                                .text_color(t.text_primary)
                                .child(i.t("没有已安装插件", "No installed plugins")),
                        )
                        .child(
                            div()
                                .text_size(px(12.0))
                                .text_color(t.text_muted)
                                .child(i.t(
                                    "可切换至「插件市场」标签页浏览并一键安装插件",
                                    "Switch to Marketplaces tab to discover and install plugins",
                                )),
                        ),
                );
            } else if filtered_plugins.is_empty() {
                tab_col = tab_col.child(
                    div()
                        .flex()
                        .items_center()
                        .justify_center()
                        .p(px(32.0))
                        .text_size(px(12.5))
                        .text_color(t.text_muted)
                        .child(i.t("未找到匹配的插件", "No matching plugins found")),
                );
            } else {
                for plugin in filtered_plugins {
                    let pid = plugin.plugin_id.clone();
                    let pid2 = plugin.plugin_id.clone();
                    let enabled = plugin.user_scope_enabled;
                    let user_installed = plugin.user_scope_installed;

                    let mut card = div()
                        .id(gpui::SharedString::from(format!("plugin-card-{}", plugin.plugin_id)))
                        .flex()
                        .flex_col()
                        .w_full()
                        .min_w(px(0.0))
                        .gap(px(6.0))
                        .p(px(12.0))
                        .rounded(px(8.0))
                        .bg(t.card_bg)
                        .border_1()
                        .border_color(if enabled {
                            gpui::rgba(0x22c55e44)
                        } else {
                            t.card_border
                        })
                        .shadow_xs();

                    // Row 1: Title, Status Badge, Marketplace Tag, Version Tag, Spacer, Actions
                    let mut header_row = div().flex().items_center().gap(px(8.0)).w_full();

                    header_row = header_row.child(
                        div()
                            .text_size(px(13.5))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(t.text_primary)
                            .child(plugin.name.clone()),
                    );

                    if user_installed && enabled {
                        header_row = header_row.child(badge(&t, i.t("已启用", "Enabled"), BadgeKind::Success));
                    } else if user_installed && !enabled {
                        header_row = header_row.child(badge(&t, i.t("已停用", "Disabled"), BadgeKind::Neutral));
                    } else {
                        header_row = header_row.child(badge(&t, i.t("非用户级", "Non-user scope"), BadgeKind::Warning));
                    }

                    header_row = header_row.child(
                        div()
                            .flex()
                            .items_center()
                            .h(px(20.0))
                            .px(px(6.0))
                            .rounded(px(4.0))
                            .bg(t.sidebar_bg)
                            .border_1()
                            .border_color(t.card_border)
                            .text_size(px(11.0))
                            .text_color(t.text_secondary)
                            .child(plugin.marketplace_name.clone()),
                    );

                    if let Some(v) = &plugin.version {
                        header_row = header_row.child(
                            div()
                                .flex()
                                .items_center()
                                .h(px(20.0))
                                .px(px(6.0))
                                .rounded(px(4.0))
                                .bg(t.sidebar_bg)
                                .border_1()
                                .border_color(t.card_border)
                                .text_size(px(11.0))
                                .text_color(t.text_muted)
                                .child(format!("v{v}")),
                        );
                    }

                    header_row = header_row.child(div().flex_1());

                    // Actions: Enable/Disable button + Uninstall button
                    if user_installed {
                        header_row = header_row.child(button_l(
                            gpui::SharedString::from(format!("plugin-toggle-{}", plugin.plugin_id)),
                            if enabled { i.t("停用", "Disable") } else { i.t("启用", "Enable") },
                            if enabled { ButtonVariant::Secondary } else { ButtonVariant::Primary },
                            &t,
                            cx,
                            {
                                let pid = pid.clone();
                                let target = !enabled;
                                move |ws, _, _, cx| {
                                    match aitoolplus_core::claude_plugins::set_plugin_enabled(
                                        &ws.paths, &pid, target,
                                    ) {
                                        Ok(_) => {
                                            let msg = if target {
                                                ws.i18n.t("已启用插件", "Plugin enabled")
                                            } else {
                                                ws.i18n.t("已停用插件", "Plugin disabled")
                                            };
                                            ws.ui.toast(msg.to_string(), false);
                                            ws.ui.claude_plugins = None;
                                            load_claude_plugins(ws, cx);
                                        }
                                        Err(e) => ws.ui.toast(format!("failed: {e}"), true),
                                    }
                                    cx.notify();
                                }
                            },
                        ));

                        header_row = header_row.child(button_with_icon_l(
                            gpui::SharedString::from(format!("plugin-del-{}", plugin.plugin_id)),
                            crate::icons::TRASH_SVG,
                            i.t("卸载", "Uninstall"),
                            ButtonVariant::Danger,
                            &t,
                            cx,
                            {
                                let pid2 = pid2.clone();
                                move |ws, _, _, cx| {
                                    let operation_id = pid2.clone();
                                    let success_zh = format!("已卸载 {operation_id}");
                                    let success_en = format!("uninstalled {operation_id}");
                                    spawn_tool_action(
                                        Some(ToolId::ClaudeCode),
                                        ws,
                                        cx,
                                        success_zh,
                                        success_en,
                                        move |paths| {
                                            aitoolplus_core::claude_plugins::uninstall_plugin(
                                                &paths,
                                                &operation_id,
                                            )
                                            .map_err(|error| format!("uninstall failed: {error}"))
                                        },
                                    );
                                }
                            },
                        ));
                    }

                    card = card.child(header_row);

                    // Row 2: Plugin ID
                    card = card.child(
                        div()
                            .text_size(px(11.0))
                            .font_family("Consolas, monospace")
                            .text_color(t.text_muted)
                            .child(plugin.plugin_id.clone()),
                    );

                    // Row 3: Description
                    if let Some(desc) = &plugin.description {
                        card = card.child(
                            div()
                                .text_size(px(12.0))
                                .text_color(t.text_secondary)
                                .child(desc.clone()),
                        );
                    }

                    // Row 4: Meta info (Scopes + Install Path)
                    let mut meta_items = vec![];
                    if !plugin.install_scopes.is_empty() {
                        meta_items.push(format!("{}: {}", i.t("作用域", "Scopes"), plugin.install_scopes.join(", ")));
                    }
                    if let Some(path) = &plugin.install_path {
                        meta_items.push(format!("{}: {}", i.t("路径", "Path"), path));
                    }
                    if !meta_items.is_empty() {
                        card = card.child(
                            div()
                                .text_size(px(11.0))
                                .text_color(t.text_muted)
                                .child(meta_items.join(" · ")),
                        );
                    }

                    // Row 5: Capabilities & Homepage
                    let mut caps_row = div().flex().items_center().gap(px(4.0)).flex_wrap();
                    if plugin.has_skills {
                        caps_row = caps_row.child(plugin_tag("skills", t.accent_subtle, t.accent));
                    }
                    if plugin.has_agents {
                        caps_row = caps_row.child(plugin_tag("agents", gpui::rgba(0x06b6d420), gpui::rgb(0x0891b2)));
                    }
                    if plugin.has_hooks {
                        caps_row = caps_row.child(plugin_tag("hooks", gpui::rgba(0xf59e0b20), gpui::rgb(0xd97706)));
                    }
                    if plugin.has_mcp_servers {
                        caps_row = caps_row.child(plugin_tag("MCP", gpui::rgba(0xa855f720), gpui::rgb(0x9333ea)));
                    }
                    if plugin.has_lsp_servers {
                        caps_row = caps_row.child(plugin_tag("LSP", gpui::rgba(0x3b82f620), gpui::rgb(0x2563eb)));
                    }

                    card = card.child(caps_row);
                    tab_col = tab_col.child(card);
                }
            }

    section = section.child(tab_col);
    section.into_any_element()
}

pub(super) fn claude_marketplace_section(ws: &mut Workspace, cx: &mut Context<Workspace>) -> gpui::AnyElement {
    let t = ws.theme.clone();
    let i = ws.i18n;

    if ws.ui.claude_plugins.is_none() && !ws.ui.claude_plugins_loading {
        load_claude_plugins(ws, cx);
    }

    let is_loading = ws.ui.claude_plugins_loading;
    let refresh_label = i.t("刷新", "Refresh");

    let mut section = div().flex().flex_col().w_full().h_full().min_h(px(0.0)).gap(px(10.0));

    let Some(cached) = &ws.ui.claude_plugins else {
        return section
            .child(
                div()
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .p(px(48.0))
                    .gap(px(12.0))
                    .child(gpui_kit::component::spinner::Spinner::new().color(t.accent.into()))
                    .child(
                        div()
                            .text_size(px(12.5))
                            .text_color(t.text_muted)
                            .child(i.t("正在读取插件市场…", "Loading marketplace plugins…")),
                    ),
            )
            .into_any_element();
    };

    let data = match cached {
        Ok(d) => d,
        Err(e) => {
            return section
                .child(crate::components::error_strip(
                    "claude-market-read-err",
                    i.t("插件市场读取失败", "marketplace read failed"),
                    &e,
                    &t,
                    cx,
                    None,
                ))
                .into_any_element();
        }
    };

    let is_expanded = ws.ui.claude_marketplaces_expanded;
    let m_filter = ws.ui.claude_market_search.read(cx).text().trim().to_lowercase();
    let installed_set: std::collections::HashSet<String> = data
        .installed_plugins
        .iter()
        .map(|p| p.plugin_id.clone())
        .collect();
    let user_scope_set: std::collections::HashSet<String> = data
        .installed_plugins
        .iter()
        .filter(|p| p.user_scope_installed)
        .map(|p| p.plugin_id.clone())
        .collect();

    let filtered_mkt: Vec<&aitoolplus_core::claude_plugins::MarketplacePlugin> = data
        .marketplace_plugins
        .iter()
        .filter(|p| {
            if m_filter.is_empty() {
                return true;
            }
            p.name.to_lowercase().contains(&m_filter)
                || p.plugin_id.to_lowercase().contains(&m_filter)
                || p.marketplace_name.to_lowercase().contains(&m_filter)
                || p.description
                    .as_deref()
                    .unwrap_or_default()
                    .to_lowercase()
                    .contains(&m_filter)
                || p.category
                    .as_deref()
                    .unwrap_or_default()
                    .to_lowercase()
                    .contains(&m_filter)
                || p.tags.iter().any(|t| t.to_lowercase().contains(&m_filter))
        })
        .collect();

    // ---- Single Unified Toolbar Row ----
    let mut toolbar = div().flex().items_center().gap(px(10.0)).w_full();

    toolbar = toolbar.child(
        div()
            .flex_1()
            .min_w(px(0.0))
            .child(input_container(&t, ws.ui.claude_market_search.clone())),
    );

    toolbar = toolbar.child(
        div()
            .text_size(px(12.0))
            .text_color(t.text_muted)
            .child(format!(
                "{} {} {}",
                i.t("匹配到", "Matched"),
                filtered_mkt.len(),
                i.t("个插件", "plugins")
            )),
    );

    let mkt_btn_label = if is_expanded {
        i.t("收起市场源 ▴", "Collapse Sources ▴")
    } else {
        i.t(
            &format!("管理市场源 ({}) ▾", data.marketplaces.len()),
            &format!("Manage Sources ({}) ▾", data.marketplaces.len()),
        )
    };
    toolbar = toolbar.child(
        button_l(
            "btn-toggle-marketplaces",
            mkt_btn_label,
            ButtonVariant::Secondary,
            &t,
            cx,
            move |ws, _, _, cx| {
                ws.ui.claude_marketplaces_expanded = !ws.ui.claude_marketplaces_expanded;
                cx.notify();
            },
        ),
    );

    toolbar = toolbar.child(
        button_with_icon_loading_l(
            "claude-market-refresh-btn",
            crate::icons::REFRESH_SVG,
            refresh_label,
            ButtonVariant::Secondary,
            is_loading,
            &t,
            cx,
            |ws, _, _, cx| {
                ws.ui.claude_plugins = None;
                load_claude_plugins(ws, cx);
                cx.notify();
            },
        ),
    );

    section = section.child(toolbar);

    // ---- Optional Expanded Marketplace Sources Panel ----
    if is_expanded {
        let mut mkt_sec = div()
            .flex()
            .flex_col()
            .gap(px(8.0))
            .p(px(10.0))
            .rounded(px(8.0))
            .bg(t.card_bg)
            .border_1()
            .border_color(t.card_border);

        let add_entity = ws.ui.claude_marketplaces_input.clone();
        for market in &data.marketplaces {
            let name = market.name.clone();
            let name2 = market.name.clone();
            let name2_del = market.name.clone();
            let auto = market.auto_update_enabled;

            let mut m_row = div().flex().items_center().justify_between().w_full();
            let left = div().flex().items_center().gap(px(8.0))
                .child(
                    div()
                        .text_size(px(13.0))
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .text_color(t.text_primary)
                        .child(name.clone()),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .h(px(20.0))
                        .px(px(6.0))
                        .rounded(px(4.0))
                        .bg(t.sidebar_bg)
                        .border_1()
                        .border_color(t.card_border)
                        .text_size(px(11.0))
                        .text_color(t.text_secondary)
                        .child(format!("{} plugins", market.plugin_count)),
                );
            m_row = m_row.child(left);

            let mut actions = div().flex().items_center().gap(px(6.0));
            actions = actions.child(
                crate::components::toggle(
                    gpui::SharedString::from(format!("mkt-auto-{}", market.name)),
                    auto,
                    &t,
                    cx,
                    {
                        let name = name.clone();
                        let target = !auto;
                        move |ws, _, _, cx| {
                            match aitoolplus_core::claude_plugins::set_marketplace_auto_update(
                                &ws.paths, &name, target,
                            ) {
                                Ok(_) => {
                                    let msg = if target {
                                        ws.i18n.t("已开启自动更新", "Auto-update enabled")
                                    } else {
                                        ws.i18n.t("已关闭自动更新", "Auto-update disabled")
                                    };
                                    ws.ui.toast(msg.to_string(), false);
                                    ws.ui.claude_plugins = None;
                                    load_claude_plugins(ws, cx);
                                }
                                Err(e) => ws.ui.toast(format!("failed: {e}"), true),
                            }
                            cx.notify();
                        }
                    },
                ),
            );

            actions = actions.child(button_l(
                gpui::SharedString::from(format!("mkt-upd-{}", market.name)),
                i.t("更新", "Update"),
                ButtonVariant::Secondary,
                &t,
                cx,
                {
                    let name_for_update = name2.clone();
                    move |ws, _, _, cx| {
                        let operation_name = name_for_update.clone();
                        spawn_tool_action(
                            Some(ToolId::ClaudeCode),
                            ws,
                            cx,
                            format!("已更新市场 {operation_name}"),
                            format!("updated marketplace {operation_name}"),
                            move |paths| {
                                aitoolplus_core::claude_plugins::update_marketplace(
                                    &paths,
                                    Some(&operation_name),
                                )
                                .map_err(|error| format!("update failed: {error}"))
                            },
                        );
                    }
                },
            ));

            actions = actions.child(button_with_icon_l(
                gpui::SharedString::from(format!("mkt-del-{}", market.name)),
                crate::icons::TRASH_SVG,
                i.t("移除", "Remove"),
                ButtonVariant::Danger,
                &t,
                cx,
                move |ws, _, _, cx| {
                    let operation_name = name2_del.clone();
                    let success_zh = format!("已移除市场 {operation_name}");
                    let success_en = format!("removed marketplace {operation_name}");
                    spawn_tool_action(
                        Some(ToolId::ClaudeCode),
                        ws,
                        cx,
                        success_zh,
                        success_en,
                        move |paths| {
                            aitoolplus_core::claude_plugins::remove_marketplace(
                                &paths,
                                &operation_name,
                            )
                            .map_err(|error| format!("remove failed: {error}"))
                        },
                    );
                },
            ));

            m_row = m_row.child(actions);
            mkt_sec = mkt_sec.child(m_row);
        }

        mkt_sec = mkt_sec.child(
            div()
                .flex()
                .items_center()
                .gap(px(8.0))
                .w_full()
                .child(div().flex_1().min_w(px(0.0)).child(input_container(&t, add_entity.clone())))
                .child(button_with_icon_l(
                    "mkt-add-btn",
                    crate::icons::PLUS_SVG,
                    i.t("添加市场", "Add"),
                    ButtonVariant::Primary,
                    &t,
                    cx,
                    move |ws, _, _, cx| {
                        let src: String = add_entity.update(cx, |inp, cx| {
                            let val = inp.text().trim().to_string();
                            inp.set_text("", cx);
                            val
                        });
                        if src.is_empty() {
                            let msg = ws.i18n.t("请输入来源", "source required").to_string();
                            ws.ui.toast(msg, true);
                            cx.notify();
                            return;
                        }
                        let success_zh = format!("已添加市场 {src}");
                        let success_en = format!("added marketplace {src}");
                        spawn_tool_action(
                            Some(ToolId::ClaudeCode),
                            ws,
                            cx,
                            success_zh,
                            success_en,
                            move |paths| {
                                aitoolplus_core::claude_plugins::add_marketplace(&paths, &src)
                                    .map_err(|error| format!("add failed: {error}"))
                            },
                        );
                    },
                )),
        );

        section = section.child(mkt_sec);
    }

    // ---- Virtual List (Remaining 100% Height) ----
    if data.marketplace_plugins.is_empty() {
        section = section.child(
            div()
                .flex()
                .items_center()
                .justify_center()
                .p(px(32.0))
                .text_size(px(12.5))
                .text_color(t.text_muted)
                .child(i.t("暂无市场插件", "No marketplace plugins available")),
        );
    } else if filtered_mkt.is_empty() {
        section = section.child(
            div()
                .flex()
                .items_center()
                .justify_center()
                .p(px(32.0))
                .text_size(px(12.5))
                .text_color(t.text_muted)
                .child(i.t("未找到匹配的市场插件", "No matching marketplace plugins found")),
        );
    } else {
        let items: std::sync::Arc<Vec<aitoolplus_core::claude_plugins::MarketplacePlugin>> =
            std::sync::Arc::new(filtered_mkt.into_iter().cloned().collect());
        let items_len = items.len();
        let items_for_list = items.clone();
        let installed_set = std::sync::Arc::new(installed_set);
        let user_scope_set = std::sync::Arc::new(user_scope_set);
        let ws_entity = cx.entity();
        let t_clone = t.clone();
        let i_clone = i.clone();

        let v_list = uniform_list(
            "mkt-plugins-virtual-list",
            items_len,
            move |range: std::ops::Range<usize>, _window: &mut gpui::Window, _cx: &mut gpui::App| -> Vec<gpui::AnyElement> {
                let mut elements = Vec::with_capacity(range.len());
                for idx in range {
                    if let Some(plugin) = items_for_list.get(idx) {
                        elements.push(render_virtual_marketplace_card(
                            plugin,
                            &installed_set,
                            &user_scope_set,
                            &ws_entity,
                            &t_clone,
                            &i_clone,
                        ));
                    }
                }
                elements
            },
        )
        .track_scroll(&ws.ui.claude_market_scroll_handle)
        .size_full();

        section = section.child(
            div()
                .w_full()
                .flex_1()
                .h_full()
                .min_h(px(0.0))
                .overflow_hidden()
                .rounded(px(8.0))
                .bg(t.sidebar_bg)
                .border_1()
                .border_color(t.card_border)
                .p(px(8.0))
                .child(v_list),
        );
    }

    section.into_any_element()
}

pub(super) fn render_virtual_marketplace_card(
    plugin: &aitoolplus_core::claude_plugins::MarketplacePlugin,
    installed_set: &std::collections::HashSet<String>,
    user_scope_set: &std::collections::HashSet<String>,
    ws_entity: &gpui::Entity<Workspace>,
    t: &Theme,
    i: &I18n,
) -> gpui::AnyElement {
    let is_installed = installed_set.contains(&plugin.plugin_id);
    let is_user_installed = user_scope_set.contains(&plugin.plugin_id);

    let mut card = div()
        .id(gpui::SharedString::from(format!("v-card-{}", plugin.plugin_id)))
        .flex()
        .flex_col()
        .justify_between()
        .w_full()
        .min_w(px(0.0))
        .h(px(100.0))
        .p(px(10.0))
        .rounded(px(8.0))
        .bg(t.card_bg)
        .border_1()
        .border_color(t.card_border)
        .shadow_xs()
        .hover({
            let bg = t.card_hover;
            let border = t.card_border_hover;
            move |h| h.bg(bg).border_color(border)
        });

    // Row 1: Title, Tags, Spacer, Action Button
    let mut header_row = div().flex().items_center().gap(px(8.0)).w_full();

    header_row = header_row.child(
        div()
            .text_size(px(13.5))
            .font_weight(gpui::FontWeight::SEMIBOLD)
            .text_color(t.text_primary)
            .child(plugin.name.clone()),
    );

    header_row = header_row.child(
        div()
            .flex()
            .items_center()
            .h(px(20.0))
            .px(px(6.0))
            .rounded(px(4.0))
            .bg(t.sidebar_bg)
            .border_1()
            .border_color(t.card_border)
            .text_size(px(11.0))
            .text_color(t.text_secondary)
            .child(plugin.marketplace_name.clone()),
    );

    if let Some(v) = &plugin.version {
        header_row = header_row.child(
            div()
                .flex()
                .items_center()
                .h(px(20.0))
                .px(px(6.0))
                .rounded(px(4.0))
                .bg(t.sidebar_bg)
                .border_1()
                .border_color(t.card_border)
                .text_size(px(11.0))
                .text_color(t.text_muted)
                .child(format!("v{v}")),
        );
    }

    if let Some(cat) = &plugin.category {
        header_row = header_row.child(plugin_tag(cat.clone(), t.accent_subtle, t.accent));
    }

    if is_installed {
        header_row = header_row.child(badge(
            t,
            if is_user_installed {
                i.t("已安装", "Installed")
            } else {
                i.t("项目级已安装", "Installed (other scope)")
            },
            BadgeKind::Success,
        ));
    }

    header_row = header_row.child(div().flex_1());

    // Action button
    if is_user_installed {
        header_row = header_row.child(
            div()
                .flex()
                .items_center()
                .h(px(26.0))
                .px(px(10.0))
                .rounded(px(6.0))
                .bg(t.sidebar_bg)
                .border_1()
                .border_color(t.card_border)
                .text_size(px(11.5))
                .text_color(t.text_muted)
                .child(i.t("已安装", "Installed")),
        );
    } else {
        let ws_entity = ws_entity.clone();
        let to_install = plugin.plugin_id.clone();
        let bg = t.accent;
        let hover_bg = t.accent_hover;
        header_row = header_row.child(
            div()
                .id(gpui::SharedString::from(format!("v-install-{}", plugin.plugin_id)))
                .cursor_pointer()
                .flex()
                .items_center()
                .justify_center()
                .gap(px(4.0))
                .h(px(26.0))
                .px(px(12.0))
                .rounded(px(6.0))
                .bg(bg)
                .text_color(gpui::white())
                .text_size(px(12.0))
                .font_weight(gpui::FontWeight::MEDIUM)
                .shadow_xs()
                .hover(move |h| h.bg(hover_bg))
                .active(move |a| a.opacity(0.88))
                .child(
                    gpui::svg()
                        .data(crate::icons::DOWNLOAD_SVG)
                        .size(px(13.0))
                        .text_color(gpui::white()),
                )
                .child(i.t("安装", "Install"))
                .on_click(move |_ev, _win, cx| {
                    let id = to_install.clone();
                    let success_zh = format!("插件 {id} 已安装");
                    let success_en = format!("plugin {id} installed");
                    let _ = ws_entity.update(cx, |ws, cx| {
                        spawn_tool_action(
                            Some(ToolId::ClaudeCode),
                            ws,
                            cx,
                            success_zh,
                            success_en,
                            move |paths| {
                                aitoolplus_core::claude_plugins::install_plugin(&paths, &id)
                                    .map_err(|error| format!("install failed: {error}"))
                            },
                        );
                    });
                }),
        );
    }

    card = card.child(header_row);

    // Row 2: ID and Description
    let mut row2 = div().flex().items_center().gap(px(8.0)).w_full().overflow_hidden();
    row2 = row2.child(
        div()
            .flex_none()
            .text_size(px(11.0))
            .font_family("Consolas, monospace")
            .text_color(t.text_muted)
            .child(plugin.plugin_id.clone()),
    );
    if let Some(desc) = &plugin.description {
        row2 = row2.child(
            div()
                .flex_1()
                .text_size(px(11.5))
                .text_color(t.text_secondary)
                .whitespace_nowrap()
                .overflow_hidden()
                .text_ellipsis()
                .child(desc.clone()),
        );
    }
    card = card.child(row2);

    // Row 3: Tags & Homepage
    let mut row3 = div().flex().items_center().gap(px(4.0)).w_full().overflow_hidden();
    for tag in plugin.tags.iter().take(5) {
        row3 = row3.child(
            div()
                .flex_none()
                .flex()
                .items_center()
                .h(px(18.0))
                .px(px(5.0))
                .rounded(px(4.0))
                .bg(t.sidebar_bg)
                .border_1()
                .border_color(t.card_border)
                .text_size(px(10.0))
                .text_color(t.text_secondary)
                .child(tag.clone()),
        );
    }
    if let Some(hp) = &plugin.homepage {
        let hp_clone = hp.clone();
        let hover_bg = t.card_hover;
        row3 = row3.child(
            div()
                .id(gpui::SharedString::from(format!("v-hp-{}", plugin.plugin_id)))
                .cursor_pointer()
                .flex()
                .items_center()
                .gap(px(3.0))
                .h(px(18.0))
                .px(px(6.0))
                .rounded(px(4.0))
                .text_size(px(10.5))
                .text_color(t.text_secondary)
                .hover(move |h| h.bg(hover_bg).text_color(t.text_primary))
                .child(
                    gpui::svg()
                        .data(crate::icons::BOOK_OPEN_SVG)
                        .size(px(11.0))
                        .text_color(t.text_secondary),
                )
                .child(i.t("主页", "Homepage"))
                .on_click(move |_ev, _win, _cx| {
                    open_in_browser(&hp_clone);
                }),
        );
    }
    card = card.child(row3);

    // Outer slot: exactly 108px high with 8px bottom margin
    div()
        .id(gpui::SharedString::from(format!("v-slot-{}", plugin.plugin_id)))
        .w_full()
        .h(px(108.0))
        .pb(px(8.0))
        .child(card)
        .into_any_element()
}

// ---------------------------------------------------------------------------
// Dialogs
// ---------------------------------------------------------------------------

