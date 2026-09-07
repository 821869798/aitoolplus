//! Per-tool page: provider list, common config editor, global prompts,
//! runtime files, Pi model settings / other settings / extensions,
//! Claude Code plugins.

use aitoolplus_core::pi_extensions::{PiExtensionKind, PiExtensionScope};
use aitoolplus_core::pi_pages::PiModelSettings;
use aitoolplus_core::providers::{CATEGORIES, ProviderRecord};
use aitoolplus_core::tools::ToolId;
use gpui::{Context, IntoElement, div, prelude::*, px};
use serde_json::Value;

use crate::components::{
    self, BadgeKind, ButtonVariant, badge, button_l, empty_state, icon_button_l, input_container,
    page_header, section_title, textarea_container,
};
use crate::text_area::TextArea;
use crate::text_input::TextInput;
use crate::workspace::Workspace;

use super::{PromptDialogState, ProviderDialogState, ToolTab, modal_scaffold};

pub fn render_tool_page(
    tool: ToolId,
    ws: &mut Workspace,
    cx: &mut Context<Workspace>,
) -> gpui::AnyElement {
    let i = ws.i18n;
    let t = ws.theme.clone();

    let mut col = div()
        .flex()
        .flex_col()
        .w_full()
        .min_w(px(0.0))
        .gap(px(16.0))
        .child(page_header(
            &t,
            i.t(tool.name_zh(), tool.name_en()),
            i.t(
                "供应商与全局 Prompt 管理",
                "Providers & global prompts",
            ),
        ))
        .child(tabs_bar(tool, ws, cx));

    match ws.ui.tool_tab {
        ToolTab::Providers | ToolTab::Common => {
            if tool == ToolId::Pi {
                col = col.child(pi_model_settings_section(ws, cx));
            }
            col = col.child(providers_section(tool, ws, cx));
            if tool == ToolId::Pi {
                col = col.child(pi_other_settings_section(ws, cx));
            }
        }
        ToolTab::Prompts => col = col.child(prompts_section(tool, ws, cx)),
        ToolTab::Runtime => col = col.child(runtime_section(tool, ws, cx)),
        ToolTab::Extensions => col = col.child(extensions_section(tool, ws, cx)),
        ToolTab::Plugins => {
            col = col.child(if tool == ToolId::Grok {
                grok_plugins_section(ws, cx)
            } else {
                plugins_section(ws, cx)
            })
        }
        ToolTab::Addons => col = col.child(opencode_addons_section(ws, cx)),
    }
    col.into_any_element()
}

fn tabs_bar(tool: ToolId, ws: &mut Workspace, cx: &mut Context<Workspace>) -> gpui::AnyElement {
    let t = ws.theme.clone();
    let i = ws.i18n;
    let current = ws.ui.tool_tab;

    let mut tabs = vec![
        (ToolTab::Providers, i.t("供应商", "Providers")),
        (ToolTab::Prompts, i.t("全局 Prompt", "Prompts")),
        (ToolTab::Runtime, i.t("运行时文件", "Runtime Files")),
    ];
    if matches!(tool, ToolId::Pi | ToolId::OhMyPi) {
        tabs.push((ToolTab::Extensions, i.t("扩展", "Extensions")));
    }
    if matches!(tool, ToolId::ClaudeCode | ToolId::Grok) {
        tabs.push((ToolTab::Plugins, i.t("插件", "Plugins")));
    }
    if tool == ToolId::OpenCode {
        tabs.push((ToolTab::Addons, i.t("附加工具", "Add-ons")));
    }

    let mut bar = div()
        .flex()
        .items_center()
        .gap(px(4.0))
        .p(px(3.0))
        .rounded(px(8.0))
        .bg(t.sidebar_bg)
        .border_1()
        .border_color(t.card_border);
    for (tab, label) in tabs {
        let is_active = tab == current;
        bar = bar.child(
            div()
                .id(gpui::ElementId::Name(format!("tool-tab-{:?}", tab).into()))
                .cursor_pointer()
                .px(px(12.0))
                .py(px(4.5))
                .rounded(px(6.0))
                .text_size(px(12.5))
                .when(is_active, |s| {
                    s.bg(t.card_bg)
                        .text_color(t.text_primary)
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .shadow_xs()
                })
                .when(!is_active, |s| {
                    s.text_color(t.text_secondary)
                        .hover(|h| h.text_color(t.text_primary).bg(t.row_hover))
                })
                .on_click(cx.listener(move |this, _ev: &gpui::ClickEvent, _w, cx| {
                    this.ui.tool_tab = tab;
                    cx.notify();
                }))
                .child(label),
        );
    }
    bar.into_any_element()
}

// ---------------------------------------------------------------------------
// Providers tab
// ---------------------------------------------------------------------------

fn providers_section(
    tool: ToolId,
    ws: &mut Workspace,
    cx: &mut Context<Workspace>,
) -> gpui::AnyElement {
    let t = ws.theme.clone();
    let i = ws.i18n;
    let providers = ws.tool_providers(tool);

    let add_label = i.t("新增供应商", "Add Provider");

    let mut actions = div().flex().items_center().gap(px(8.0));
    if let ToolId::Hermes = tool {
        let enabled = {
            let h = aitoolplus_core::HermesRuntimePaths::from_paths(&ws.paths);
            aitoolplus_core::hermes::memory_enabled(&h).unwrap_or(false)
        };
        actions = actions.child(
            div()
                .flex()
                .items_center()
                .gap(px(6.0))
                .child(
                    div()
                        .text_size(px(12.0))
                        .text_color(t.text_secondary)
                        .child(i.t("记忆系统", "Memory")),
                )
                .child(components::toggle(
                    "hermes-memory-toggle",
                    enabled,
                    &t,
                    cx,
                    move |ws, _, _, cx| {
                        let i = ws.i18n;
                        let h = aitoolplus_core::HermesRuntimePaths::from_paths(&ws.paths);
                        let target = !aitoolplus_core::hermes::memory_enabled(&h).unwrap_or(false);
                        match aitoolplus_core::hermes::set_memory_enabled(&h, target) {
                            Ok(()) => {
                                let msg = if target {
                                    i.t("记忆已启用", "memory enabled").to_string()
                                } else {
                                    i.t("记忆已停用", "memory disabled").to_string()
                                };
                                ws.ui.toast(msg, false);
                            }
                            Err(e) => {
                                let msg = format!("failed: {e}");
                                ws.ui.toast(msg, true);
                            }
                        }
                        cx.notify();
                    },
                )),
        );
    }
    actions = actions
        .child(button_l(
            "prov-test-all",
            i.t("批量测试", "Test All"),
            ButtonVariant::Secondary,
            &t,
            cx,
            move |ws, _, _, cx| batch_test_providers(tool, ws, cx),
        ))
        .child(button_l(
            "prov-add",
            add_label,
            ButtonVariant::Primary,
            &t,
            cx,
            move |ws, _, _, cx| open_provider_dialog(None, tool, ws, cx),
        ));

    let header = div()
        .flex()
        .items_center()
        .justify_between()
        .w_full()
        .child(section_title(
            &t,
            i.t("供应商列表", "Provider List"),
            None,
        ))
        .child(actions);

    let mut section = div().flex().flex_col().gap(px(12.0)).child(header);

    if providers.is_empty() {
        section = section.child(empty_state(
            &t,
            "\u{1f4e6}",
            i.t("还没有供应商", "No providers yet"),
            i.t(
                "点击右上角「新增供应商」创建第一条配置",
                "Click \"Add Provider\" to create your first config",
            ),
        ));
    } else {
        let mut list = div().flex().flex_col().gap(px(10.0));
        let total = providers.len();
        for (index, provider) in providers.iter().enumerate() {
            list = list.child(provider_row(tool, provider, index, total, ws, cx));
        }
        section = section.child(list);
    }

    section.into_any_element()
}

fn card_icon_btn(
    id: impl Into<gpui::ElementId>,
    icon: &'static str,
    tooltip: impl Into<gpui::SharedString>,
    is_danger: bool,
    t: &crate::theme::Theme,
    cx: &mut Context<Workspace>,
    on_click: impl Fn(&mut Workspace, &gpui::MouseDownEvent, &mut gpui::Window, &mut Context<Workspace>) + 'static,
) -> gpui::AnyElement {
    let tooltip = tooltip.into();
    let text_color = t.text_secondary;
    let hover_text = t.text_primary;
    let hover_bg = t.card_hover;
    div()
        .id(id.into())
        .cursor_pointer()
        .size(px(28.0))
        .rounded(px(6.0))
        .flex()
        .items_center()
        .justify_center()
        .text_size(px(13.0))
        .text_color(text_color)
        .hover(move |h| {
            if is_danger {
                h.bg(crate::rgba_const(0xef444422))
                    .text_color(crate::rgba_const(0xef4444ff))
            } else {
                h.bg(hover_bg).text_color(hover_text)
            }
        })
        .tooltip(move |_w, cx| {
            let tip = tooltip.clone();
            cx.new(|_| crate::components::Tooltip::new(tip)).into()
        })
        .on_mouse_down(gpui::MouseButton::Left, cx.listener(move |ws, ev, w, cx| {
            on_click(ws, ev, w, cx);
        }))
        .child(icon)
        .into_any_element()
}

fn provider_icon_spec(p: &ProviderRecord) -> (&'static str, gpui::Rgba, gpui::Rgba) {
    let lower_name = p.name.to_lowercase();
    if p.category == "official"
        || lower_name.contains("official")
        || lower_name.contains("官方")
        || lower_name.contains("claude")
        || lower_name.contains("anthropic")
    {
        ("A\\", crate::rgba_const(0xe07a5fff), crate::rgba_const(0xe07a5f20))
    } else if lower_name.contains("deepseek") {
        ("🐋", crate::rgba_const(0x0284c7ff), crate::rgba_const(0x0284c720))
    } else if lower_name.contains("openai") || lower_name.contains("codex") || lower_name.contains("chatgpt") {
        ("O", crate::rgba_const(0x10b981ff), crate::rgba_const(0x10b98120))
    } else if lower_name.contains("gemini") || lower_name.contains("google") {
        ("✦", crate::rgba_const(0x6366f1ff), crate::rgba_const(0x6366f120))
    } else if lower_name.contains("kimi") || lower_name.contains("moonshot") {
        ("K", crate::rgba_const(0x8b5cf6ff), crate::rgba_const(0x8b5cf620))
    } else if lower_name.contains("grok") || lower_name.contains("xai") {
        ("X", crate::rgba_const(0x94a3b8ff), crate::rgba_const(0x94a3b820))
    } else {
        ("⚡", crate::rgba_const(0x9ca3afff), crate::rgba_const(0xffffff15))
    }
}

fn extract_provider_subtitle(p: &ProviderRecord, _i: &crate::i18n::I18n) -> String {
    if p.category == "official" {
        return "https://www.anthropic.com/claude-code".to_string();
    }
    if let Some(ref w) = p.website_url {
        let trimmed = w.trim();
        if !trimmed.is_empty() {
            return trimmed.to_string();
        }
    }
    if let Ok(val) = serde_json::from_str::<Value>(&p.settings_config) {
        if let Some(env) = val.get("env").and_then(Value::as_object) {
            for key in [
                "ANTHROPIC_BASE_URL",
                "OPENAI_BASE_URL",
                "GOOGLE_GEMINI_BASE_URL",
                "BASE_URL",
            ] {
                if let Some(u) = env.get(key).and_then(Value::as_str) {
                    let trimmed = u.trim();
                    if !trimmed.is_empty() {
                        return trimmed.to_string();
                    }
                }
            }
        }
        for key in ["base_url", "baseUrl", "url", "endpoint"] {
            if let Some(u) = val.get(key).and_then(Value::as_str) {
                let trimmed = u.trim();
                if !trimmed.is_empty() {
                    return trimmed.to_string();
                }
            }
        }
        if let Some(toml_str) = val.get("toml").and_then(Value::as_str) {
            for line in toml_str.lines() {
                let line_trim = line.trim();
                if line_trim.starts_with("base_url") {
                    if let Some((_, r)) = line_trim.split_once('=') {
                        let cleaned = r.trim().trim_matches('"').trim_matches('\'');
                        if !cleaned.is_empty() {
                            return cleaned.to_string();
                        }
                    }
                }
            }
        }
    }
    if let Some(ref n) = p.notes {
        let trimmed = n.trim();
        if !trimmed.is_empty() {
            return trimmed.to_string();
        }
    }
    String::new()
}

fn provider_row(
    tool: ToolId,
    p: &ProviderRecord,
    index: usize,
    total: usize,
    ws: &mut Workspace,
    cx: &mut Context<Workspace>,
) -> gpui::AnyElement {
    let t = ws.theme.clone();
    let i = ws.i18n;
    let pid = p.id.clone();
    let pid2 = p.id.clone();
    let pid4 = p.id.clone();
    let pid_models = p.id.clone();
    let pid_up = p.id.clone();
    let pid_down = p.id.clone();
    let pid_test = p.id.clone();

    let (icon_sym, icon_color, icon_bg) = provider_icon_spec(p);
    let subtitle = extract_provider_subtitle(p, &i);

    let test_badge = ws.ui.provider_test_results.get(&p.id).map(|result| {
        if result.ok {
            div()
                .px(px(6.0))
                .py(px(2.0))
                .rounded(px(4.0))
                .bg(crate::rgba_const(0x10b98115))
                .text_size(px(11.5))
                .font_weight(gpui::FontWeight::MEDIUM)
                .text_color(crate::rgba_const(0x10b981ff))
                .child(format!("{}ms", result.latency_ms))
        } else {
            div()
                .px(px(6.0))
                .py(px(2.0))
                .rounded(px(4.0))
                .bg(crate::rgba_const(0xef444415))
                .text_size(px(11.5))
                .font_weight(gpui::FontWeight::MEDIUM)
                .text_color(crate::rgba_const(0xef4444ff))
                .child(i.t("连通失败", "Failed"))
        }
    });

    let is_official =
        p.category == "official" || aitoolplus_core::providers::is_official_provider(tool, &p.id);

    let left = div()
        .flex()
        .items_center()
        .gap(px(12.0))
        .min_w(px(0.0))
        .flex_1()
        .child(
            div()
                .text_size(px(14.0))
                .font_weight(gpui::FontWeight::BOLD)
                .text_color(crate::rgba_const(0xffffff24))
                .cursor_grab()
                .child("⋮⋮"),
        )
        .child(
            div()
                .size(px(36.0))
                .flex_shrink_0()
                .rounded_full()
                .bg(icon_bg)
                .border_1()
                .border_color(crate::rgba_const(0xffffff18))
                .flex()
                .items_center()
                .justify_center()
                .child(
                    div()
                        .text_size(px(13.5))
                        .font_weight(gpui::FontWeight::BOLD)
                        .text_color(icon_color)
                        .child(icon_sym),
                ),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(2.5))
                .min_w(px(0.0))
                .flex_1()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(8.0))
                        .child(
                            div()
                                .text_size(px(15.0))
                                .font_weight(gpui::FontWeight::SEMIBOLD)
                                .text_color(t.text_primary)
                                .child(p.name.clone()),
                        )
                        .children(is_official.then(|| {
                            div()
                                .px(px(6.0))
                                .py(px(1.5))
                                .rounded(px(4.0))
                                .bg(crate::rgba_const(0x23252eff))
                                .text_size(px(11.0))
                                .text_color(crate::rgba_const(0x9ca3afff))
                                .child(i.t("官方直连", "Official"))
                        }))
                        .children(p.is_disabled.then(|| {
                            div()
                                .px(px(6.0))
                                .py(px(1.5))
                                .rounded(px(4.0))
                                .bg(crate::rgba_const(0x23252eff))
                                .text_size(px(11.0))
                                .text_color(crate::rgba_const(0xef4444bb))
                                .child(i.t("已停用", "Disabled"))
                        })),
                )
                .children((!subtitle.is_empty()).then(|| {
                    div()
                        .text_size(px(12.5))
                        .text_color(crate::rgba_const(0x3b82f6ee))
                        .cursor_pointer()
                        .child(subtitle)
                        .into_any_element()
                })),
        );

    let mut actions = div()
        .flex()
        .items_center()
        .gap(px(6.0))
        .flex_shrink_0();

    if let Some(tb) = test_badge {
        actions = actions.child(tb);
    }

    if p.is_applied {
        actions = actions.child(
            div()
                .px(px(14.0))
                .py(px(5.0))
                .rounded(px(8.0))
                .bg(crate::rgba_const(0x10b98118))
                .border_1()
                .border_color(crate::rgba_const(0x10b98144))
                .text_size(px(12.5))
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .text_color(crate::rgba_const(0x10b981ff))
                .flex()
                .items_center()
                .gap(px(4.0))
                .child("✓")
                .child(i.t("使用中", "In Use")),
        );
    } else {
        actions = actions.child(
            div()
                .id(gpui::SharedString::from(format!("prov-apply-{pid}")))
                .cursor_pointer()
                .px(px(14.0))
                .py(px(5.0))
                .rounded(px(8.0))
                .bg(crate::rgba_const(0x2563ebff))
                .hover(|h| h.bg(crate::rgba_const(0x1d4ed8ff)))
                .text_size(px(12.5))
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .text_color(crate::rgba_const(0xffffffff))
                .shadow_xs()
                .flex()
                .items_center()
                .gap(px(5.0))
                .child("▷")
                .child(i.t("启用", "Enable"))
                .on_mouse_down(gpui::MouseButton::Left, cx.listener(move |ws, _ev, _w, cx| {
                    ws.apply_provider(tool, &pid, cx);
                })),
        );
    }

    actions = actions.child(card_icon_btn(
        format!("prov-edit-{pid2}"),
        "✎",
        i.t("编辑供应商", "Edit Provider"),
        false,
        &t,
        cx,
        move |ws, _ev, _w, cx| {
            open_provider_dialog(Some(pid2.clone()), tool, ws, cx);
        },
    ));

    actions = actions.child(card_icon_btn(
        format!("prov-test-{pid_test}"),
        "⚡",
        i.t("连通测试", "Test Connection"),
        false,
        &t,
        cx,
        move |ws, _ev, _w, cx| {
            test_single_provider_action(tool, pid_test.clone(), ws, cx);
        },
    ));

    actions = actions.child(card_icon_btn(
        format!("prov-models-{pid_models}"),
        "📊",
        i.t("拉取可用模型", "Fetch Models"),
        false,
        &t,
        cx,
        move |ws, _ev, _w, cx| {
            fetch_models_action(tool, pid_models.clone(), ws, cx);
        },
    ));

    if aitoolplus_core::cli_launch::command_name(tool).is_some() {
        let launch_id = p.id.clone();
        actions = actions.child(card_icon_btn(
            format!("prov-launch-{launch_id}"),
            ">_",
            i.t("启动 CLI 终端", "Launch CLI"),
            false,
            &t,
            cx,
            move |ws, _ev, _w, cx| {
                if !ws
                    .store
                    .store()
                    .tool(tool)
                    .providers
                    .iter()
                    .any(|provider| provider.id == launch_id && provider.is_applied)
                {
                    ws.apply_provider(tool, &launch_id, cx);
                }
                match aitoolplus_core::cli_launch::launch(
                    &ws.paths,
                    tool,
                    None,
                    ws.settings.claude_cli_launch_full_access,
                ) {
                    Ok(_) => ws
                        .ui
                        .toast(ws.i18n.t("CLI 已启动", "CLI launched").to_string(), false),
                    Err(error) => ws.ui.toast(error, true),
                }
                cx.notify();
            },
        ));
    }

    if total > 1 {
        if index > 0 {
            actions = actions.child(card_icon_btn(
                format!("prov-up-{pid_up}"),
                "▲",
                i.t("上移", "Move Up"),
                false,
                &t,
                cx,
                move |ws, _ev, _w, cx| {
                    let _ = ws.store.update(|store| {
                        let providers = &mut store.tool_mut(tool).providers;
                        if let Some(from) = providers.iter().position(|p| p.id == pid_up) {
                            aitoolplus_core::providers::reorder(providers, from, from - 1);
                        }
                    });
                    ws.persist_store();
                    cx.notify();
                },
            ));
        }
        if index + 1 < total {
            actions = actions.child(card_icon_btn(
                format!("prov-down-{pid_down}"),
                "▼",
                i.t("下移", "Move Down"),
                false,
                &t,
                cx,
                move |ws, _ev, _w, cx| {
                    let _ = ws.store.update(|store| {
                        let providers = &mut store.tool_mut(tool).providers;
                        if let Some(from) = providers.iter().position(|p| p.id == pid_down) {
                            aitoolplus_core::providers::reorder(providers, from, from + 1);
                        }
                    });
                    ws.persist_store();
                    cx.notify();
                },
            ));
        }
    }

    if !is_official {
        actions = actions.child(card_icon_btn(
            format!("prov-del-{pid4}"),
            "🗑",
            i.t("删除供应商", "Delete Provider"),
            true,
            &t,
            cx,
            move |ws, _ev, _w, cx| {
                ws.ui.confirm = Some(super::ConfirmState {
                    title: ws.i18n.t("删除供应商", "Delete Provider").to_string(),
                    message: ws
                        .i18n
                        .t("确定要删除这条供应商配置吗？", "Delete this provider?")
                        .to_string(),
                    action: super::ConfirmAction::DeleteProvider {
                        tool,
                        id: pid4.clone(),
                    },
                });
                cx.notify();
            },
        ));
    }

    let border_color = if p.is_applied {
        t.accent
    } else {
        t.card_border
    };
    let bg_color = if p.is_applied {
        if t.is_dark {
            crate::rgba_const(0x181a24ff)
        } else {
            crate::rgba_const(0xf0f7ffff)
        }
    } else {
        t.card_bg
    };
    let hover_bg = if t.is_dark {
        crate::rgba_const(0x1c1e28ff)
    } else {
        t.card_hover
    };
    let hover_border = if p.is_applied {
        t.accent_hover
    } else {
        t.card_border_hover
    };

    div()
        .id(gpui::SharedString::from(format!("provider-card-{}", p.id)))
        .flex()
        .items_center()
        .justify_between()
        .w_full()
        .min_w(px(0.0))
        .gap(px(14.0))
        .px(px(18.0))
        .py(px(14.0))
        .rounded(px(12.0))
        .bg(bg_color)
        .border_1()
        .border_color(border_color)
        .shadow_xs()
        .hover(move |h| {
            h.bg(hover_bg).border_color(hover_border)
        })
        .child(left)
        .child(actions)
        .into_any_element()
}

#[allow(dead_code)]
fn import_providers(tool: ToolId, _ws: &mut Workspace, cx: &mut Context<Workspace>) {
    let dialog = rfd::AsyncFileDialog::new().add_filter("JSON", &["json"]);
    let weak = cx.entity().downgrade();
    cx.spawn(async move |_this, cx| {
        if let Some(file) = dialog.pick_file().await {
            let path = file.path().to_path_buf();
            let result = std::fs::read_to_string(&path);
            let _ =
                weak.update(cx, |ws, cx| {
                    match result {
                        Ok(json) => {
                            let mut imported = 0;
                            let mut error = None;
                            let _ = ws.store.update(|store| {
                                match aitoolplus_core::providers::import_json(
                                    &mut store.tool_mut(tool).providers,
                                    &json,
                                ) {
                                    Ok(count) => imported = count,
                                    Err(message) => error = Some(message),
                                }
                            });
                            if let Some(error) = error {
                                ws.ui.toast(error, true);
                            } else {
                                ws.persist_store();
                                ws.ui.toast(
                                    ws.i18n
                                        .t(
                                            &format!("已导入 {imported} 条供应商"),
                                            &format!("imported {imported} providers"),
                                        )
                                        .to_string(),
                                    false,
                                );
                            }
                        }
                        Err(error) => ws.ui.toast(format!("import failed: {error}"), true),
                    }
                    cx.notify();
                });
        }
    })
    .detach();
}

#[allow(dead_code)]
fn export_providers(tool: ToolId, ws: &mut Workspace, cx: &mut Context<Workspace>) {
    let providers = ws.tool_providers(tool);
    let json = aitoolplus_core::providers::export(&providers);
    if let Some(dir) = ws.paths.app_data.to_str() {
        let path = format!("{dir}/providers-{}.json", tool.key());
        match std::fs::write(&path, &json) {
            Ok(()) => {
                let i = ws.i18n;
                let msg = i
                    .t(&format!("已导出到 {path}"), &format!("exported to {path}"))
                    .to_string();
                ws.ui.toast(msg, false);
            }
            Err(e) => {
                let msg = format!("export failed: {e}");
                ws.ui.toast(msg, true);
            }
        }
    }
    cx.notify();
}

// ---------------------------------------------------------------------------
// Common config tab
// ---------------------------------------------------------------------------

#[allow(dead_code)]
fn common_section(
    tool: ToolId,
    ws: &mut Workspace,
    cx: &mut Context<Workspace>,
) -> gpui::AnyElement {
    let t = ws.theme.clone();
    let i = ws.i18n;
    let editor = {
        let current = ws.store.store().tool(tool).common_config;
        ws.ui.common_editor(tool, &current, cx)
    };

    let common_is_toml = tool == ToolId::Codex;
    let save_label = i.t("保存并应用通用配置", "Save & Apply Common Config");
    let format_label = i.t("格式化", "Format");
    let extract_label = i.t("从当前文件读取", "Read Current File");

    div()
        .flex()
        .flex_col()
        .gap(px(12.0))
        .child(section_title(
            &t,
            i.t(
                "通用配置（与供应商配置合并）",
                "Common Config (merged with provider)",
            ),
            Some(i.t(
                if common_is_toml {
                    "此 TOML 会作为基础层，与所选供应商配置合并后写入"
                } else {
                    "此 JSON 会作为基础层，与所选供应商配置合并后写入"
                },
                if common_is_toml {
                    "This TOML is the base layer merged under the selected provider"
                } else {
                    "This JSON is the base layer merged under the selected provider"
                },
            )),
        ))
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(8.0))
                .p(px(12.0))
                .rounded(px(8.0))
                .bg(t.input_bg)
                .border_1()
                .border_color(t.input_border)
                .child(editor.clone()),
        )
        .child(
            div()
                .flex()
                .gap(px(8.0))
                .child(button_l(
                    "common-format",
                    format_label,
                    ButtonVariant::Secondary,
                    &t,
                    cx,
                    move |ws, _ev, _w, cx| {
                        let current = ws.store.store().tool(tool).common_config;
                        let editor = ws.ui.common_editor(tool, &current, cx);
                        editor.update(cx, |ta, cx| {
                            let text = ta.text().to_string();
                            if common_is_toml {
                                if let Ok(document) = text.parse::<toml_edit::DocumentMut>() {
                                    ta.set_text_silent(document.to_string(), cx);
                                }
                            } else if let Ok(pretty) = serde_json::from_str::<Value>(&text)
                                .and_then(|v| serde_json::to_string_pretty(&v))
                            {
                                ta.set_text_silent(pretty, cx);
                            }
                        });
                        cx.notify();
                    },
                ))
                .child(button_l(
                    "common-extract",
                    extract_label,
                    ButtonVariant::Secondary,
                    &t,
                    cx,
                    move |ws, _, _, cx| {
                        let adapter = aitoolplus_core::adapters::adapter_for(tool);
                        let paths = ws.paths.clone();
                        let current = ws.store.store().tool(tool).common_config;
                        let editor = ws.ui.common_editor(tool, &current, cx);
                        match adapter.read_current(&paths) {
                            Ok(v) => {
                                let content = if common_is_toml {
                                    v.get("toml")
                                        .and_then(Value::as_str)
                                        .map(String::from)
                                        .unwrap_or_default()
                                } else {
                                    serde_json::to_string_pretty(&v).unwrap_or_default()
                                };
                                editor.update(cx, |ta, cx| ta.set_text_silent(content, cx));
                            }
                            Err(_) => {
                                let msg = ws.i18n.t("读取失败", "read failed").to_string();
                                ws.ui.toast(msg, true);
                            }
                        }
                        cx.notify();
                    },
                ))
                .child(button_l(
                    "common-save",
                    save_label,
                    ButtonVariant::Primary,
                    &t,
                    cx,
                    move |ws, _, _, cx| {
                        let current = ws.store.store().tool(tool).common_config;
                        let editor = ws.ui.common_editor(tool, &current, cx);
                        let text = editor.update(cx, |ta, _| ta.text().to_string());
                        let validation = if common_is_toml {
                            text.parse::<toml_edit::DocumentMut>()
                                .map(|_| ())
                                .map_err(|error| error.to_string())
                        } else {
                            serde_json::from_str::<Value>(&text)
                                .map(|_| ())
                                .map_err(|error| error.to_string())
                        };
                        match validation {
                            Ok(()) => {
                                let _ = ws.store.update(|store| {
                                    store.tool_mut(tool).common_config = text.clone();
                                });
                                let section = ws.store.store().tool(tool);
                                let applied = section
                                    .providers
                                    .iter()
                                    .find(|provider| provider.is_applied && !provider.is_disabled)
                                    .cloned();
                                if let Some(provider) = applied {
                                    let context = aitoolplus_core::adapters::ApplyCtx {
                                        paths: &ws.paths,
                                        common_config: &text,
                                        provider: &provider,
                                        strategy: aitoolplus_core::config::MergeStrategy::default(),
                                        provider_optional: false,
                                    };
                                    if let Err(error) =
                                        aitoolplus_core::adapters::adapter_for(tool).apply(&context)
                                    {
                                        ws.ui.toast(format!("apply failed: {error}"), true);
                                        cx.notify();
                                        return;
                                    }
                                }
                                ws.persist_store();
                                let msg =
                                    ws.i18n.t("已保存并应用", "saved and applied").to_string();
                                ws.ui.toast(msg, false);
                            }
                            Err(error) => {
                                let format_name = if common_is_toml { "TOML" } else { "JSON" };
                                ws.ui.toast(format!("invalid {format_name}: {error}"), true);
                            }
                        }
                        cx.notify();
                    },
                )),
        )
        .into_any_element()
}

// ---------------------------------------------------------------------------
// Prompts tab
// ---------------------------------------------------------------------------

fn prompts_section(
    tool: ToolId,
    ws: &mut Workspace,
    cx: &mut Context<Workspace>,
) -> gpui::AnyElement {
    let t = ws.theme.clone();
    let i = ws.i18n;
    let prompts = aitoolplus_core::prompt::list(&ws.store.store().tool(tool).prompts);

    let add_label = i.t("新增 Prompt", "Add Prompt");

    let mut section = div().flex().flex_col().gap(px(12.0)).child(
        div()
            .flex()
            .items_center()
            .justify_between()
            .child(section_title(
                &t,
                i.t("全局 Prompt", "Global Prompts"),
                Some(i.t(
                    "应用后写入工具的 Prompt 文件（如 CLAUDE.md / AGENTS.md）",
                    "Applied prompts are written to the tool's prompt file",
                )),
            ))
            .child(button_l(
                "prompt-add",
                add_label,
                ButtonVariant::Primary,
                &t,
                cx,
                move |ws, _, _, cx| {
                    open_prompt_dialog(None, tool, ws, cx);
                },
            )),
    );

    if prompts.is_empty() {
        section = section.child(empty_state(
            &t,
            "\u{1f4dd}",
            i.t("还没有 Prompt", "No prompts yet"),
            i.t(
                "新增一条可一键应用的全局 Prompt",
                "Add one to apply globally",
            ),
        ));
    } else {
        let mut list = div().flex().flex_col().gap(px(6.0));
        for p in &prompts {
            let pid = p.id.clone();
            let pid2 = p.id.clone();
            let pid3 = p.id.clone();
            let status_badge = if p.is_applied {
                badge(&t, i.t("应用中", "Applied"), BadgeKind::Success)
            } else {
                badge(&t, i.t("未应用", "Idle"), BadgeKind::Neutral)
            };
            list = list.child(
                div()
                    .id(gpui::SharedString::from(format!("prompt-{pid}")))
                    .flex()
                    .w_full()
                    .min_w(px(0.0))
                    .items_center()
                    .gap(px(12.0))
                    .p(px(12.0))
                    .rounded(px(8.0))
                    .bg(t.card_bg)
                    .border_1()
                    .border_color(if p.is_applied {
                        t.success
                    } else {
                        t.card_border
                    })
                    .hover(|h| h.bg(t.card_hover))
                    .child(status_badge)
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.0))
                            .text_size(px(13.5))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(t.text_primary)
                            .child(p.name.clone()),
                    )
                    .child(
                        div()
                            .flex()
                            .gap(px(4.0))
                            .child(if p.is_applied {
                                div().flex().items_center().h(px(28.0)).into_any_element()
                            } else {
                                button_l(
                                    gpui::SharedString::from(format!("prompt-apply-{pid}")),
                                    i.t("应用", "Apply"),
                                    ButtonVariant::Primary,
                                    &t,
                                    cx,
                                    move |ws, _, _, cx| apply_prompt(tool, &pid, ws, cx),
                                )
                            })
                            .child(icon_button_l(
                                gpui::SharedString::from(format!("prompt-edit-{pid2}")),
                                "\u{270e}",
                                i.t("编辑", "Edit"),
                                false,
                                &t,
                                cx,
                                move |ws, _, _, cx| {
                                    let id = pid2.clone();
                                    open_prompt_dialog(Some(id), tool, ws, cx);
                                },
                            ))
                            .child(icon_button_l(
                                gpui::SharedString::from(format!("prompt-del-{pid3}")),
                                "\u{1f5d1}",
                                i.t("删除", "Delete"),
                                true,
                                &t,
                                cx,
                                move |ws, _, _, cx| {
                                    ws.ui.confirm = Some(super::ConfirmState {
                                        title: ws
                                            .i18n
                                            .t("删除 Prompt", "Delete Prompt")
                                            .to_string(),
                                        message: ws
                                            .i18n
                                            .t("确定要删除这条 Prompt 吗？", "Delete this prompt?")
                                            .to_string(),
                                        action: super::ConfirmAction::DeletePrompt {
                                            tool,
                                            id: pid3.clone(),
                                        },
                                    });
                                    cx.notify();
                                },
                            )),
                    ),
            );
        }
        section = section.child(list);
    }

    section.into_any_element()
}

fn apply_prompt(tool: ToolId, id: &str, ws: &mut Workspace, cx: &mut Context<Workspace>) {
    let i = ws.i18n;
    let _ = ws.store.update(|store| {
        let s = store.tool_mut(tool);
        aitoolplus_core::prompt::select(&mut s.prompts, id);
    });
    let section = ws.store.store().tool(tool);
    let prompt = section.prompts.iter().find(|p| p.id == id).cloned();

    if let Some(prompt) = prompt {
        let adapter = aitoolplus_core::adapters::adapter_for(tool);
        if let Some(file) = adapter.prompt_file(&ws.paths) {
            if let Err(e) = std::fs::create_dir_all(file.parent().unwrap_or(&file)) {
                let msg = format!("write failed: {e}");
                ws.ui.toast(msg, true);
            } else if let Err(e) = std::fs::write(&file, &prompt.content) {
                let msg = format!("write failed: {e}");
                ws.ui.toast(msg, true);
            } else {
                let msg = i
                    .t(
                        &format!("已应用 Prompt 到 {}", file.display()),
                        &format!("prompt applied to {}", file.display()),
                    )
                    .to_string();
                ws.ui.toast(msg, false);
            }
        } else {
            let msg = i
                .t("该工具没有 Prompt 文件", "tool has no prompt file")
                .to_string();
            ws.ui.toast(msg, true);
        }
    }
    ws.persist_store();
    cx.notify();
}

// ---------------------------------------------------------------------------
// Runtime files tab
// ---------------------------------------------------------------------------

fn runtime_section(
    tool: ToolId,
    ws: &mut Workspace,
    cx: &mut Context<Workspace>,
) -> gpui::AnyElement {
    let t = ws.theme.clone();
    let i = ws.i18n;

    let need_reload = match &ws.ui.runtime_files_cache {
        Some((cached_tool, _)) => *cached_tool != tool,
        None => true,
    };
    if need_reload {
        let adapter = aitoolplus_core::adapters::adapter_for(tool);
        let files = adapter.runtime_files(&ws.paths);
        let cached = files
            .into_iter()
            .map(|(label, path)| {
                let exists = path.exists();
                let content = if exists {
                    std::fs::read_to_string(&path).unwrap_or_else(|e| format!("<read error: {e}>"))
                } else {
                    i.t("（文件不存在）", "(file missing)").to_string()
                };
                (label, path, exists, content)
            })
            .collect();
        ws.ui.runtime_files_cache = Some((tool, cached));
    }

    let files = match &ws.ui.runtime_files_cache {
        Some((_, f)) => f.clone(),
        None => vec![],
    };

    let mut section = div().flex().flex_col().gap(px(12.0)).child(
        div()
            .flex()
            .items_center()
            .justify_between()
            .child(page_header(
                &t,
                i.t("运行时文件", "Runtime Files"),
                i.t(
                    "只读预览工具当前的真实配置文件",
                    "Read-only preview of the tool's real config files",
                ),
            ))
            .child(button_l(
                "runtime-refresh-btn",
                i.t("刷新", "Refresh"),
                ButtonVariant::Secondary,
                &t,
                cx,
                move |ws, _, _, cx| {
                    ws.ui.runtime_files_cache = None;
                    cx.notify();
                },
            )),
    );

    if files.is_empty() {
        section = section.child(empty_state(
            &t,
            "\u{1f4c4}",
            i.t("该工具没有已知配置文件", "No known config files"),
            "",
        ));
        return section.into_any_element();
    }

    for (label, path, exists, content) in files {
        let truncated: String = content.chars().take(4000).collect();
        let overflow = content.chars().count() > 4000;

        section = section.child(
            div()
                .flex()
                .flex_col()
                .gap(px(6.0))
                .p(px(12.0))
                .rounded(px(8.0))
                .bg(t.card_bg)
                .border_1()
                .border_color(if exists { t.card_border } else { t.danger })
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(8.0))
                        .child(badge(
                            &t,
                            if exists {
                                i.t("存在", "exists")
                            } else {
                                i.t("缺失", "missing")
                            },
                            if exists {
                                BadgeKind::Success
                            } else {
                                BadgeKind::Danger
                            },
                        ))
                        .child(
                            div()
                                .text_size(px(12.5))
                                .font_weight(gpui::FontWeight::MEDIUM)
                                .text_color(t.text_primary)
                                .child(label.clone()),
                        )
                        .child(
                            div()
                                .text_size(px(11.0))
                                .text_color(t.text_muted)
                                .child(path.display().to_string()),
                        ),
                )
                .child(
                    div()
                        .p(px(10.0))
                        .rounded(px(6.0))
                        .bg(t.input_bg)
                        .text_size(px(11.5))
                        .text_color(t.text_secondary)
                        .child(truncated)
                        .when(overflow, |el| {
                            el.child(
                                div()
                                    .pt(px(6.0))
                                    .text_size(px(11.0))
                                    .text_color(t.text_muted)
                                    .child(i.t("…（已截断）", "…(truncated)")),
                            )
                        }),
                ),
        );
    }

    section.into_any_element()
}

// ---------------------------------------------------------------------------
// Pi Model Settings / Other Settings sections
// ---------------------------------------------------------------------------

/// Default provider / model / thinking level selectors (settings.json).
fn pi_model_settings_section(ws: &mut Workspace, cx: &mut Context<Workspace>) -> gpui::AnyElement {
    let t = ws.theme.clone();
    let i = ws.i18n;
    let mut section = div()
        .flex()
        .flex_col()
        .gap(px(10.0))
        .p(px(14.0))
        .rounded(px(10.0))
        .bg(t.card_bg)
        .border_1()
        .border_color(t.card_border)
        .child(section_title(
            &t,
            i.t("模型设置", "Model Settings"),
            Some(i.t(
                "写入 settings.json 的 defaultProvider / defaultModel / defaultThinkingLevel",
                "writes settings.json defaultProvider / defaultModel / defaultThinkingLevel",
            )),
        ));

    match (
        aitoolplus_core::pi_pages::read_model_settings(&ws.paths),
        aitoolplus_core::pi_pages::models_catalog(&ws.paths),
    ) {
        (Ok(current), Ok(catalog)) => {
            let cur_provider = current.provider_key.clone();
            let cur_model = current.model_id.clone();
            let cur_thinking = current.thinking_level.clone();

            let mut rows = div().flex().flex_col().gap(px(8.0));

            let mut provider_row = div()
                .flex()
                .w_full()
                .min_w(px(0.0))
                .flex_wrap()
                .gap(px(4.0));
            for key in catalog.keys() {
                let is_on =
                    ws.ui.pi_ms_provider.as_deref().or(cur_provider.as_deref()) == Some(key);
                let key_owned = key.clone();
                let t2 = t.clone();
                provider_row = provider_row.child(
                    div()
                        .id(gpui::SharedString::from(format!("pi-ms-provider-{key}")))
                        .cursor_pointer()
                        .flex()
                        .items_center()
                        .h(px(24.0))
                        .px(px(8.0))
                        .rounded(px(6.0))
                        .text_size(px(11.5))
                        .when(is_on, |st| {
                            st.bg(t2.accent_subtle)
                                .border_1()
                                .border_color(t2.accent)
                                .text_color(t2.accent)
                        })
                        .when(!is_on, |st| {
                            st.bg(t2.input_bg)
                                .border_1()
                                .border_color(t2.input_border)
                                .text_color(t2.text_secondary)
                        })
                        .on_click(cx.listener(move |ws, _ev, _w, cx| {
                            if ws.ui.pi_ms_provider.as_deref() == Some(&key_owned) {
                                ws.ui.pi_ms_provider = None;
                            } else {
                                ws.ui.pi_ms_provider = Some(key_owned.clone());
                            }
                            ws.ui.pi_ms_model = None;
                            cx.notify();
                        }))
                        .child(key.clone()),
                );
            }
            rows = rows
                .child(
                    div()
                        .text_size(px(12.0))
                        .font_weight(gpui::FontWeight::MEDIUM)
                        .text_color(t.text_secondary)
                        .child(i.t("默认供应商", "Default Provider")),
                )
                .child(provider_row);

            let selected_provider = ws
                .ui
                .pi_ms_provider
                .clone()
                .or_else(|| cur_provider.clone());
            if let Some(provider) = &selected_provider
                && let Some(models) = catalog.get(provider)
            {
                let mut model_row = div()
                    .flex()
                    .w_full()
                    .min_w(px(0.0))
                    .flex_wrap()
                    .gap(px(4.0));
                for model in models {
                    let is_on = ws
                        .ui
                        .pi_ms_model
                        .clone()
                        .or_else(|| cur_model.clone())
                        .as_deref()
                        == Some(model.as_str());
                    let m_owned = model.clone();
                    let p_owned = provider.clone();
                    let t2 = t.clone();
                    model_row = model_row.child(
                        div()
                            .id(gpui::SharedString::from(format!("pi-ms-model-{model}")))
                            .cursor_pointer()
                            .flex()
                            .items_center()
                            .h(px(24.0))
                            .px(px(8.0))
                            .rounded(px(6.0))
                            .text_size(px(11.5))
                            .when(is_on, |st| {
                                st.bg(t2.accent_subtle)
                                    .border_1()
                                    .border_color(t2.accent)
                                    .text_color(t2.accent)
                            })
                            .when(!is_on, |st| {
                                st.bg(t2.input_bg)
                                    .border_1()
                                    .border_color(t2.input_border)
                                    .text_color(t2.text_secondary)
                            })
                            .on_click(cx.listener(move |ws, _ev, _w, cx| {
                                ws.ui.pi_ms_provider = Some(p_owned.clone());
                                ws.ui.pi_ms_model = Some(m_owned.clone());
                                cx.notify();
                            }))
                            .child(model.clone()),
                    );
                }
                if !models.is_empty() {
                    rows = rows
                        .child(
                            div()
                                .text_size(px(12.0))
                                .font_weight(gpui::FontWeight::MEDIUM)
                                .text_color(t.text_secondary)
                                .child(i.t("默认模型", "Default Model")),
                        )
                        .child(model_row);
                }
            }

            let mut think_row = div()
                .flex()
                .w_full()
                .min_w(px(0.0))
                .flex_wrap()
                .gap(px(4.0));
            for level in aitoolplus_core::pi_pages::KNOWN_THINKING_LEVELS {
                let is_on = ws
                    .ui
                    .pi_ms_thinking
                    .clone()
                    .or_else(|| cur_thinking.clone())
                    .as_deref()
                    == Some(level);
                let l_owned = level.to_string();
                let t2 = t.clone();
                think_row = think_row.child(
                    div()
                        .id(gpui::SharedString::from(format!("pi-ms-think-{level}")))
                        .cursor_pointer()
                        .flex()
                        .items_center()
                        .h(px(24.0))
                        .px(px(8.0))
                        .rounded(px(6.0))
                        .text_size(px(11.5))
                        .when(is_on, |st| {
                            st.bg(t2.accent_subtle)
                                .border_1()
                                .border_color(t2.accent)
                                .text_color(t2.accent)
                        })
                        .when(!is_on, |st| {
                            st.bg(t2.input_bg)
                                .border_1()
                                .border_color(t2.input_border)
                                .text_color(t2.text_secondary)
                        })
                        .on_click(cx.listener(move |ws, _ev, _w, cx| {
                            if ws.ui.pi_ms_thinking.as_deref() == Some(&l_owned) {
                                ws.ui.pi_ms_thinking = None;
                            } else {
                                ws.ui.pi_ms_thinking = Some(l_owned.clone());
                            }
                            cx.notify();
                        }))
                        .child(level),
                );
            }
            rows = rows
                .child(
                    div()
                        .text_size(px(12.0))
                        .font_weight(gpui::FontWeight::MEDIUM)
                        .text_color(t.text_secondary)
                        .child(i.t("思考等级", "Thinking Level")),
                )
                .child(think_row);

            section = section
                .child(rows)
                .child(div().flex().justify_end().child(button_l(
                    "pi-ms-save",
                    i.t("保存模型设置", "Save Model Settings"),
                    ButtonVariant::Primary,
                    &t,
                    cx,
                    move |ws, _, _, cx| {
                        let settings = PiModelSettings {
                            provider_key: ws
                                .ui
                                .pi_ms_provider
                                .clone()
                                .or_else(|| cur_provider.clone()),
                            model_id: ws.ui.pi_ms_model.clone().or_else(|| cur_model.clone()),
                            thinking_level: ws
                                .ui
                                .pi_ms_thinking
                                .clone()
                                .or_else(|| cur_thinking.clone()),
                        };
                        match aitoolplus_core::pi_pages::models_catalog(&ws.paths) {
                            Ok(cat) => {
                                if let Err(e) = aitoolplus_core::pi_pages::validate_model_settings(
                                    &cat, &settings,
                                ) {
                                    ws.ui.toast(e.to_string(), true);
                                    cx.notify();
                                    return;
                                }
                            }
                            Err(e) => {
                                ws.ui.toast(format!("catalog failed: {e}"), true);
                                cx.notify();
                                return;
                            }
                        }
                        match aitoolplus_core::pi_pages::write_model_settings(&ws.paths, &settings)
                        {
                            Ok(_) => {
                                let msg = ws.i18n.t("已保存", "saved").to_string();
                                ws.ui.toast(msg, false);
                            }
                            Err(e) => ws.ui.toast(format!("save failed: {e}"), true),
                        }
                        cx.notify();
                    },
                )));
        }
        (Err(e), _) | (_, Err(e)) => {
            section = section.child(
                div()
                    .text_size(px(12.0))
                    .text_color(t.danger)
                    .child(format!("{}: {e}", i.t("读取失败", "read failed"))),
            );
        }
    }

    section.into_any_element()
}

/// Other Settings: settings.json minus packages, editable + save.
fn pi_other_settings_section(ws: &mut Workspace, cx: &mut Context<Workspace>) -> gpui::AnyElement {
    let t = ws.theme.clone();
    let i = ws.i18n;
    let mut section = div()
        .flex()
        .flex_col()
        .gap(px(10.0))
        .p(px(14.0))
        .rounded(px(10.0))
        .bg(t.card_bg)
        .border_1()
        .border_color(t.card_border)
        .child(section_title(
            &t,
            i.t("其他设置", "Other Settings"),
            Some(i.t(
                "settings.json 的其余字段；packages 由扩展链管理，保存时自动保留",
                "other settings.json fields; packages is owned by the extension chain and preserved on save",
            )),
        ));

    match aitoolplus_core::pi_pages::read_other_settings(&ws.paths) {
        Ok(other) => {
            if let Some(obj) = other.as_object()
                && obj.is_empty()
            {
                section = section.child(
                    div()
                        .text_size(px(12.0))
                        .text_color(t.text_muted)
                        .child(i.t("（无其他设置）", "(no other settings)")),
                );
            }
            if let Some(obj) = other.as_object() {
                let mut rows = div().flex().flex_col().gap(px(6.0));
                for (key, value) in obj {
                    let display = match value {
                        Value::String(s) => s.clone(),
                        other => serde_json::to_string(other).unwrap_or_default(),
                    };
                    rows = rows.child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .child(
                                div()
                                    .w(px(180.0))
                                    .text_size(px(12.0))
                                    .text_color(t.text_secondary)
                                    .child(key.clone()),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .text_size(px(12.0))
                                    .text_color(t.text_primary)
                                    .child(display),
                            ),
                    );
                }
                section = section.child(rows);
            }

            let pretty = serde_json::to_string_pretty(&other).unwrap_or_default();
            let editor = ws.ui.pi_other_editor(&pretty, cx);
            let editor_save = editor.clone();
            section = section
                .child(
                    div()
                        .p(px(10.0))
                        .rounded(px(8.0))
                        .bg(t.input_bg)
                        .border_1()
                        .border_color(t.input_border)
                        .child(editor),
                )
                .child(div().flex().justify_end().child(button_l(
                    "pi-other-save",
                    i.t("保存其他设置", "Save Other Settings"),
                    ButtonVariant::Primary,
                    &t,
                    cx,
                    move |ws, _, _, cx| {
                        let text: String = editor_save.update(cx, |ta, _| ta.text().to_string());
                        match serde_json::from_str::<Value>(&text) {
                            Ok(edited) => {
                                match aitoolplus_core::pi_pages::write_other_settings(
                                    &ws.paths, &edited,
                                ) {
                                    Ok(_) => {
                                        ws.ui.pi_other_editor = None;
                                        let msg = ws.i18n.t("已保存", "saved").to_string();
                                        ws.ui.toast(msg, false);
                                    }
                                    Err(e) => ws.ui.toast(format!("save failed: {e}"), true),
                                }
                            }
                            Err(e) => {
                                let msg = ws
                                    .i18n
                                    .t(&format!("JSON 无效：{e}"), &format!("invalid JSON: {e}"))
                                    .to_string();
                                ws.ui.toast(msg, true);
                            }
                        }
                        cx.notify();
                    },
                )));
        }
        Err(e) => {
            section = section.child(
                div()
                    .text_size(px(12.0))
                    .text_color(t.danger)
                    .child(format!("{}: {e}", i.t("读取失败", "read failed"))),
            );
        }
    }

    section.into_any_element()
}

fn spawn_tool_action<F>(
    target: Option<ToolId>,
    ws: &mut Workspace,
    cx: &mut Context<Workspace>,
    success_zh: String,
    success_en: String,
    operation: F,
) where
    F: FnOnce(std::sync::Arc<aitoolplus_core::Paths>) -> Result<(), String> + Send + 'static,
{
    let paths = ws.paths.clone();
    let weak = cx.entity().downgrade();
    cx.spawn(async move |_this, cx| {
        let result = cx.background_spawn(async move { operation(paths) }).await;
        let _ = weak.update(cx, |workspace, cx| {
            match result {
                Ok(()) => {
                    workspace.ui.toast(
                        workspace.i18n.t(&success_zh, &success_en).to_string(),
                        false,
                    );
                    if let Some(tool) = target {
                        match tool {
                            ToolId::Pi => {
                                workspace.ui.pi_extensions = None;
                                load_pi_extensions(workspace, cx);
                            }
                            ToolId::OhMyPi => {
                                workspace.ui.omp_extensions = None;
                                load_omp_extensions(workspace, cx);
                            }
                            ToolId::Grok => {
                                workspace.ui.grok_plugins = None;
                                load_grok_plugins(workspace, cx);
                            }
                            _ => {}
                        }
                    }
                }
                Err(error) => workspace.ui.toast(error, true),
            }
            cx.notify();
        });
    })
    .detach();
}

pub fn load_pi_extensions(ws: &mut Workspace, cx: &mut Context<Workspace>) {
    if ws.ui.pi_extensions_loading {
        return;
    }
    ws.ui.pi_extensions_loading = true;
    let paths = ws.paths.clone();
    let weak = cx.entity().downgrade();
    cx.spawn(async move |_this, cx| {
        let result = cx
            .background_spawn(async move {
                aitoolplus_core::pi_extensions::list_extensions(&paths)
            })
            .await;
        let _ = weak.update(cx, |workspace, cx| {
            workspace.ui.pi_extensions = Some(result);
            workspace.ui.pi_extensions_loading = false;
            cx.notify();
        });
    })
    .detach();
}

pub fn load_omp_extensions(ws: &mut Workspace, cx: &mut Context<Workspace>) {
    if ws.ui.omp_extensions_loading {
        return;
    }
    ws.ui.omp_extensions_loading = true;
    let paths = ws.paths.clone();
    let weak = cx.entity().downgrade();
    cx.spawn(async move |_this, cx| {
        let result = cx
            .background_spawn(async move {
                aitoolplus_core::omp_extensions::list(&paths).map(|result| {
                    aitoolplus_core::pi_extensions::PiExtensionListResult {
                        extensions: result.extensions,
                        cli_path: result.cli_path,
                        cli_version: result.cli_version,
                    }
                })
            })
            .await;
        let _ = weak.update(cx, |workspace, cx| {
            workspace.ui.omp_extensions = Some(result);
            workspace.ui.omp_extensions_loading = false;
            cx.notify();
        });
    })
    .detach();
}

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

// ---------------------------------------------------------------------------
// Pi Extensions tab
// ---------------------------------------------------------------------------

fn extensions_section(
    tool: ToolId,
    ws: &mut Workspace,
    cx: &mut Context<Workspace>,
) -> gpui::AnyElement {
    let t = ws.theme.clone();
    let i = ws.i18n;
    debug_assert!(matches!(tool, ToolId::Pi | ToolId::OhMyPi));

    let is_pi = tool == ToolId::Pi;
    if is_pi {
        if ws.ui.pi_extensions.is_none() && !ws.ui.pi_extensions_loading {
            load_pi_extensions(ws, cx);
        }
    } else if ws.ui.omp_extensions.is_none() && !ws.ui.omp_extensions_loading {
        load_omp_extensions(ws, cx);
    }

    let is_loading = if is_pi {
        ws.ui.pi_extensions_loading
    } else {
        ws.ui.omp_extensions_loading
    };

    let cached_list = if is_pi {
        ws.ui.pi_extensions.as_ref()
    } else {
        ws.ui.omp_extensions.as_ref()
    };

    let refresh_label = if is_loading {
        i.t("刷新中…", "Refreshing…")
    } else {
        i.t("刷新", "Refresh")
    };

    let mut section = div().flex().flex_col().gap(px(12.0)).child(
        div()
            .flex()
            .items_center()
            .justify_between()
            .child(page_header(
                &t,
                i.t("扩展管理", "Extensions"),
                if is_pi {
                    i.t(
                        "pi list 为事实源：包扩展走 Pi CLI，本地扩展来自 <root>/extensions",
                        "pi list is the source of truth: packages via the Pi CLI, locals from <root>/extensions",
                    )
                } else {
                    i.t(
                        "omp plugin list 为事实源：包扩展走 OMP CLI，本地扩展来自 <root>/extensions",
                        "omp plugin list is the source of truth; locals come from <root>/extensions",
                    )
                },
            ))
            .child(button_l(
                if is_pi { "pi-ext-refresh-btn" } else { "omp-ext-refresh-btn" },
                refresh_label,
                ButtonVariant::Secondary,
                &t,
                cx,
                move |ws, _, _, cx| {
                    if is_pi {
                        ws.ui.pi_extensions = None;
                        load_pi_extensions(ws, cx);
                    } else {
                        ws.ui.omp_extensions = None;
                        load_omp_extensions(ws, cx);
                    }
                    cx.notify();
                },
            )),
    );

    let install_input = if is_pi {
        ws.ui.pi_extension_input.clone()
    } else {
        ws.ui.omp_extension_input.clone()
    };

    let Some(list) = cached_list else {
        return section
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_center()
                    .p(px(32.0))
                    .rounded(px(8.0))
                    .bg(t.card_bg)
                    .border_1()
                    .border_color(t.card_border)
                    .text_size(px(13.0))
                    .text_color(t.text_secondary)
                    .child(i.t("正在查询扩展列表…", "Loading extensions…")),
            )
            .into_any_element();
    };

    match list {
        Ok(result) => {
            let meta = match (&result.cli_path, &result.cli_version) {
                (Some(p), Some(v)) => format!("pi_cli={p} \u{b7} pi --version → {v}"),
                (Some(p), None) => format!("pi_cli={p}"),
                _ => {
                    if is_pi {
                        i.t("未找到 pi CLI", "pi CLI not found").to_string()
                    } else {
                        i.t("未找到 omp CLI", "omp CLI not found").to_string()
                    }
                }
            };
            section = section.child(
                div()
                    .p(px(10.0))
                    .rounded(px(8.0))
                    .bg(t.card_bg)
                    .border_1()
                    .border_color(t.card_border)
                    .text_size(px(11.5))
                    .text_color(t.text_secondary)
                    .child(meta),
            );

            if result.extensions.is_empty() {
                section = section.child(empty_state(
                    &t,
                    "\u{1f9e9}",
                    i.t("没有已安装扩展", "No extensions installed"),
                    i.t("在下方输入来源并安装", "Enter a source below and install"),
                ));
            } else {
                let mut list_el = div().flex().flex_col().gap(px(6.0));
                for ext in &result.extensions {
                    let kind_badge = match ext.kind {
                        PiExtensionKind::Package => badge(&t, "pkg", BadgeKind::Accent),
                        PiExtensionKind::LocalFile => badge(&t, ".ts", BadgeKind::Neutral),
                        PiExtensionKind::LocalDirectory => badge(&t, "dir", BadgeKind::Neutral),
                    };
                    let scope_badge = badge(
                        &t,
                        match ext.scope {
                            PiExtensionScope::User => "user",
                            PiExtensionScope::Project => "project",
                            PiExtensionScope::Unknown => "?",
                        },
                        BadgeKind::Neutral,
                    );
                    let version_line = match (
                        ext.current_version.as_deref(),
                        ext.latest_version.as_deref(),
                    ) {
                        (Some(c), Some(l)) if ext.update_available => format!("{c} → {l}"),
                        (Some(c), _) => c.to_string(),
                        (None, Some(l)) => format!("latest: {l}"),
                        _ => String::new(),
                    };

                    let source_remove = ext.source.clone();
                    let source_update = ext.source.clone();
                    let is_pkg = ext.kind == PiExtensionKind::Package;
                    let protected = ext.built_in;

                    let mut actions = div().flex().items_center().gap(px(4.0));
                    if is_pkg && !protected {
                        actions = actions
                            .child(icon_button_l(
                                gpui::SharedString::from(format!("ext-upd-{}", ext.id)),
                                "\u{27f3}",
                                i.t("更新", "Update"),
                                false,
                                &t,
                                cx,
                                move |ws, _, _, cx| {
                                    let source = source_update.clone();
                                    spawn_tool_action(
                                        Some(tool),
                                        ws,
                                        cx,
                                        "扩展已更新".into(),
                                        "extension updated".into(),
                                        move |paths| {
                                            let result = if tool == ToolId::Pi {
                                                aitoolplus_core::pi_extensions::update_extension(
                                                    &paths,
                                                    Some(&source),
                                                )
                                                .map(|_| ())
                                            } else {
                                                aitoolplus_core::omp_extensions::update(
                                                    &paths,
                                                    Some(&source),
                                                )
                                            };
                                            result
                                                .map_err(|error| format!("update failed: {error}"))
                                        },
                                    );
                                },
                            ))
                            .child(icon_button_l(
                                gpui::SharedString::from(format!("ext-del-{}", ext.id)),
                                "\u{1f5d1}",
                                i.t("移除", "Remove"),
                                true,
                                &t,
                                cx,
                                move |ws, _, _, cx| {
                                    let source = source_remove.clone();
                                    spawn_tool_action(
                                        Some(tool),
                                        ws,
                                        cx,
                                        format!("已移除 {source}"),
                                        format!("removed {source}"),
                                        move |paths| {
                                            let result = if tool == ToolId::Pi {
                                                aitoolplus_core::pi_extensions::remove_extension(
                                                    &paths, &source,
                                                )
                                                .map(|_| ())
                                            } else {
                                                aitoolplus_core::omp_extensions::uninstall(
                                                    &paths,
                                                    &source,
                                                    PiExtensionKind::Package,
                                                    None,
                                                )
                                            };
                                            result
                                                .map_err(|error| format!("remove failed: {error}"))
                                        },
                                    );
                                },
                            ));
                    } else if !is_pi && !protected {
                        let local_source = ext.source.clone();
                        let local_path = ext.path.clone();
                        let local_kind = ext.kind;
                        actions = actions.child(button_l(
                            gpui::SharedString::from(format!("omp-local-del-{}", ext.id)),
                            i.t("删除本地扩展", "Delete Local Extension"),
                            ButtonVariant::Danger,
                            &t,
                            cx,
                            move |ws, _, _, cx| {
                                match aitoolplus_core::omp_extensions::uninstall(
                                    &ws.paths,
                                    &local_source,
                                    local_kind,
                                    local_path.as_deref(),
                                ) {
                                    Ok(()) => {
                                        ws.ui.toast("local extension removed", false);
                                        ws.ui.omp_extensions = None;
                                        load_omp_extensions(ws, cx);
                                    }
                                    Err(error) => ws.ui.toast(error, true),
                                }
                                cx.notify();
                            },
                        ));
                    }

                    list_el = list_el.child(
                        div()
                            .id(gpui::SharedString::from(format!("ext-{}", ext.id)))
                            .flex()
                            .items_center()
                            .gap(px(10.0))
                            .p(px(12.0))
                            .rounded(px(8.0))
                            .bg(t.card_bg)
                            .border_1()
                            .border_color(if ext.update_available {
                                t.warning
                            } else {
                                t.card_border
                            })
                            .hover(|h| h.bg(t.card_hover))
                            .child(kind_badge)
                            .child(scope_badge)
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap(px(2.0))
                                    .flex_1()
                                    .min_w(px(0.0))
                                    .child(
                                        div()
                                            .text_size(px(13.0))
                                            .font_weight(gpui::FontWeight::MEDIUM)
                                            .text_color(t.text_primary)
                                            .child(if protected {
                                                format!("\u{1f512} {}", ext.source)
                                            } else {
                                                ext.source.clone()
                                            }),
                                    )
                                    .when(!version_line.is_empty(), |el| {
                                        el.child(
                                            div()
                                                .text_size(px(11.0))
                                                .text_color(if ext.update_available {
                                                    t.warning
                                                } else {
                                                    t.text_muted
                                                })
                                                .child(version_line.clone()),
                                        )
                                    }),
                            )
                            .when(ext.update_available, |el| {
                                el.child(badge(&t, i.t("有更新", "Update!"), BadgeKind::Warning))
                            })
                            .child(actions),
                    );
                }
                section = section.child(list_el);
            }

            let input_entity = install_input.clone();
            section = section.child(
                div()
                    .id("pi-ext-install")
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .p(px(10.0))
                    .rounded(px(8.0))
                    .bg(t.card_bg)
                    .border_1()
                    .border_color(t.card_border)
                    .child(div().flex_1().min_w(px(0.0)).child(input_entity.clone()))
                    .child(button_l(
                        "pi-ext-install-btn",
                        i.t("安装", "Install"),
                        ButtonVariant::Primary,
                        &t,
                        cx,
                        move |ws, _, _, cx| {
                            let src: String =
                                input_entity.update(cx, |inp, cx| {
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
                            let success_zh = format!("已安装 {src}");
                            let success_en = format!("installed {src}");
                            spawn_tool_action(Some(tool), ws, cx, success_zh, success_en, move |paths| {
                                let result = if tool == ToolId::Pi {
                                    aitoolplus_core::pi_extensions::install_extension(&paths, &src)
                                        .map(|_| ())
                                } else {
                                    aitoolplus_core::omp_extensions::install(&paths, &src)
                                };
                                result.map_err(|error| format!("install failed: {error}"))
                            });
                        },
                    )),
            );
        }
        Err(e) => {
            section = section
                .child(empty_state(
                    &t,
                    "\u{26a0}\u{fe0f}",
                    i.t("扩展列表获取失败", "Failed to list extensions"),
                    "",
                ))
                .child(
                    div()
                        .p(px(12.0))
                        .rounded(px(8.0))
                        .bg(t.danger_subtle)
                        .border_1()
                        .border_color(t.danger)
                        .text_size(px(12.0))
                        .text_color(t.danger)
                        .child(e.clone()),
                );
        }
    }

    section.into_any_element()
}

fn test_single_provider_action(
    tool: ToolId,
    provider_id: String,
    ws: &mut Workspace,
    cx: &mut Context<Workspace>,
) {
    let settings = ws
        .store
        .store()
        .tool(tool)
        .providers
        .iter()
        .find(|p| p.id == provider_id)
        .map(|p| p.settings())
        .unwrap_or_default();

    let pid = provider_id.clone();
    let weak = cx.entity().downgrade();
    cx.spawn(async move |_this, cx| {
        let result = cx
            .background_spawn(async move {
                aitoolplus_core::api_hub::test_connectivity(&settings)
            })
            .await;
        let _ = weak.update(cx, |ws, cx| {
            let msg = if result.ok {
                format!("连接成功 ({}ms)", result.latency_ms)
            } else {
                format!("连接失败: {}", result.message)
            };
            ws.ui.toast(msg, !result.ok);
            ws.ui.provider_test_results.insert(pid, result);
            cx.notify();
        });
    })
    .detach();
}

/// Fetch this provider's live model list (API Hub) and toast the result.
fn batch_test_providers(tool: ToolId, ws: &mut Workspace, cx: &mut Context<Workspace>) {
    let providers: Vec<(String, Value)> = ws
        .store
        .store()
        .tool(tool)
        .providers
        .into_iter()
        .filter(|provider| !provider.is_disabled)
        .map(|provider| {
            let settings = provider.settings();
            (provider.id, settings)
        })
        .collect();
    if providers.is_empty() {
        ws.ui.toast(
            ws.i18n
                .t("没有可测试的供应商", "no providers to test")
                .to_string(),
            true,
        );
        cx.notify();
        return;
    }
    let weak = cx.entity().downgrade();
    cx.spawn(async move |_this, cx| {
        let results = cx
            .background_spawn(async move {
                providers
                    .into_iter()
                    .map(|(id, settings)| {
                        (id, aitoolplus_core::api_hub::test_connectivity(&settings))
                    })
                    .collect::<Vec<_>>()
            })
            .await;
        let _ = weak.update(cx, |ws, cx| {
            let passed = results.iter().filter(|(_, result)| result.ok).count();
            let total = results.len();
            for (id, result) in results {
                ws.ui.provider_test_results.insert(id, result);
            }
            ws.ui.toast(
                ws.i18n
                    .t(
                        &format!("测试完成：{passed}/{total} 通过"),
                        &format!("test complete: {passed}/{total} passed"),
                    )
                    .to_string(),
                passed != total,
            );
            cx.notify();
        });
    })
    .detach();
}

fn fetch_models_action(
    tool: ToolId,
    provider_id: String,
    ws: &mut Workspace,
    cx: &mut Context<Workspace>,
) {
    let settings = ws
        .store
        .store()
        .tool(tool)
        .providers
        .iter()
        .find(|p| p.id == provider_id)
        .map(|p| p.settings())
        .unwrap_or_default();

    let Some((base_url, api_key)) = aitoolplus_core::api_hub::provider_endpoint(&settings) else {
        let msg = ws
            .i18n
            .t("供应商配置缺少 baseUrl", "provider config lacks baseUrl")
            .to_string();
        ws.ui.toast(msg, true);
        cx.notify();
        return;
    };

    let weak = cx.entity().downgrade();
    cx.spawn(async move |this, cx| {
        let result = cx
            .background_spawn(
                async move { aitoolplus_core::api_hub::fetch_models(&base_url, &api_key) },
            )
            .await;
        let _ = weak.update(cx, |ws: &mut Workspace, cx| {
            let i = ws.i18n;
            match result {
                Ok(res) => {
                    ws.ui.provider_test_results.insert(
                        provider_id.clone(),
                        aitoolplus_core::api_hub::ConnectivityResult {
                            ok: true,
                            latency_ms: 0,
                            status: Some(200),
                            message: "ok".into(),
                            models_count: res.models.len(),
                        },
                    );
                    let names: Vec<String> = res.models.iter().map(|m| m.id.clone()).collect();
                    let shown: String =
                        names.iter().take(8).cloned().collect::<Vec<_>>().join(", ");
                    let suffix = if names.len() > 8 { " …" } else { "" };
                    let msg = i
                        .t(
                            &format!("获取到 {} 个模型：{shown}{suffix}", res.models.len()),
                            &format!("fetched {} models: {shown}{suffix}", res.models.len()),
                        )
                        .to_string();
                    ws.ui.toast(msg, false);
                }
                Err(e) => {
                    let msg = match e {
                        aitoolplus_core::api_hub::ModelsFetchError::Auth => i
                            .t("认证失败（401/403）", "auth failed (401/403)")
                            .to_string(),
                        aitoolplus_core::api_hub::ModelsFetchError::Network(d) => {
                            format!("network error: {d}")
                        }
                        aitoolplus_core::api_hub::ModelsFetchError::Parse(d) => {
                            format!("parse error: {d}")
                        }
                        aitoolplus_core::api_hub::ModelsFetchError::Unsupported(d) => {
                            format!("unsupported: {d}")
                        }
                    };
                    ws.ui.provider_test_results.insert(
                        provider_id.clone(),
                        aitoolplus_core::api_hub::ConnectivityResult {
                            ok: false,
                            latency_ms: 0,
                            status: None,
                            message: msg.clone(),
                            models_count: 0,
                        },
                    );
                    ws.ui.toast(msg, true);
                }
            }
            let _ = this;
            cx.notify();
        });
    })
    .detach();
}

// ---------------------------------------------------------------------------
// OpenCode companion add-ons
// ---------------------------------------------------------------------------

fn opencode_addons_section(ws: &mut Workspace, cx: &mut Context<Workspace>) -> gpui::AnyElement {
    let t = ws.theme.clone();
    let i = ws.i18n;
    let allow_clear = ws.settings.opencode_allow_clear_applied_oh_my_config;
    let mut section = div().flex().flex_col().gap(px(12.0)).child(page_header(
        &t,
        i.t("OpenCode 附加工具", "OpenCode Add-ons"),
        i.t(
            "Oh My OpenAgent 与 Oh My OpenCode Slim 配置档案",
            "Oh My OpenAgent and Oh My OpenCode Slim profiles",
        ),
    ));

    for (kind, label) in [
        (
            aitoolplus_core::opencode_addons::AddonKind::OhMyOpenAgent,
            "Oh My OpenAgent",
        ),
        (
            aitoolplus_core::opencode_addons::AddonKind::OhMyOpenCodeSlim,
            "Oh My OpenCode Slim",
        ),
    ] {
        let local = aitoolplus_core::opencode_addons::load_local(&ws.paths, kind)
            .ok()
            .flatten();
        let initial = local
            .as_ref()
            .map(|profile| serde_json::to_string_pretty(&profile.config).unwrap_or_default())
            .unwrap_or_else(|| "{}".into());
        let editor = ws.ui.addon_editor(kind, &initial, cx);
        let save_editor = editor.clone();
        let apply_editor = editor.clone();
        let profiles: Vec<_> = ws
            .store
            .store()
            .opencode_addons
            .profiles
            .iter()
            .filter(|profile| profile.kind == kind)
            .cloned()
            .collect();
        let runtime = aitoolplus_core::opencode_addons::config_path(&ws.paths, kind);
        let mut card = div()
            .flex()
            .flex_col()
            .gap(px(8.0))
            .p(px(12.0))
            .rounded(px(8.0))
            .bg(t.card_bg)
            .border_1()
            .border_color(t.card_border)
            .child(section_title(
                &t,
                label,
                Some(gpui::SharedString::from(format!(
                    "runtime: {}",
                    runtime.display()
                ))),
            ));
        for profile in profiles {
            let apply_id = profile.id.clone();
            let delete_id = profile.id.clone();
            card = card.child(
                div()
                    .flex()
                    .flex_col()
                    .items_start()
                    .gap(px(6.0))
                    .p(px(8.0))
                    .rounded(px(6.0))
                    .bg(t.input_bg)
                    .child(
                        div()
                            .text_size(px(12.0))
                            .text_color(t.text_primary)
                            .child(profile.name),
                    )
                    .child(
                        div()
                            .flex()
                            .gap(px(8.0))
                            .child(button_l(
                                gpui::SharedString::from(format!("addon-apply-{apply_id}")),
                                i.t("应用", "Apply"),
                                ButtonVariant::Primary,
                                &t,
                                cx,
                                move |ws, _, _, cx| {
                                    let result = {
                                        let store = ws.store.store_mut();
                                        aitoolplus_core::opencode_addons::apply(
                                            &ws.paths,
                                            &mut store.opencode_addons,
                                            &apply_id,
                                        )
                                    };
                                    match result {
                                        Ok(path) => {
                                            ws.persist_store();
                                            ws.ui.toast(
                                                format!("applied: {}", path.display()),
                                                false,
                                            );
                                        }
                                        Err(error) => ws.ui.toast(error, true),
                                    }
                                    cx.notify();
                                },
                            ))
                            .child(button_l(
                                gpui::SharedString::from(format!("addon-delete-{delete_id}")),
                                i.t("删除", "Delete"),
                                ButtonVariant::Danger,
                                &t,
                                cx,
                                move |ws, _, _, cx| {
                                    aitoolplus_core::opencode_addons::delete(
                                        &mut ws.store.store_mut().opencode_addons,
                                        &delete_id,
                                    );
                                    ws.persist_store();
                                    cx.notify();
                                },
                            )),
                    ),
            );
        }
        card = card
            .child(
                div()
                    .p(px(10.0))
                    .rounded(px(8.0))
                    .bg(t.input_bg)
                    .border_1()
                    .border_color(t.card_border)
                    .child(editor),
            )
            .child(
                div()
                    .flex()
                    .gap(px(8.0))
                    .child(button_l(
                        gpui::SharedString::from(format!("addon-save-{}", kind.key())),
                        i.t("保存为档案", "Save as Profile"),
                        ButtonVariant::Secondary,
                        &t,
                        cx,
                        move |ws, _, _, cx| {
                            let text =
                                save_editor.update(cx, |editor, _| editor.text().to_string());
                            match serde_json::from_str::<Value>(&text) {
                                Ok(config) if config.is_object() => {
                                    let profile =
                                        aitoolplus_core::opencode_addons::AddonProfile::new(
                                            format!(
                                                "{} {}",
                                                label,
                                                chrono::Local::now().format("%H:%M:%S")
                                            ),
                                            kind,
                                            config,
                                        );
                                    aitoolplus_core::opencode_addons::upsert(
                                        &mut ws.store.store_mut().opencode_addons,
                                        profile,
                                    );
                                    ws.persist_store();
                                    cx.notify();
                                }
                                _ => ws.ui.toast("configuration must be a JSON object", true),
                            }
                        },
                    ))
                    .child(button_l(
                        gpui::SharedString::from(format!("addon-quick-apply-{}", kind.key())),
                        i.t("直接应用", "Apply Directly"),
                        ButtonVariant::Primary,
                        &t,
                        cx,
                        move |ws, _, _, cx| {
                            let text =
                                apply_editor.update(cx, |editor, _| editor.text().to_string());
                            match serde_json::from_str::<Value>(&text) {
                                Ok(config) if config.is_object() => {
                                    let profile =
                                        aitoolplus_core::opencode_addons::AddonProfile::new(
                                            "Quick Apply",
                                            kind,
                                            config,
                                        );
                                    let id = profile.id.clone();
                                    aitoolplus_core::opencode_addons::upsert(
                                        &mut ws.store.store_mut().opencode_addons,
                                        profile,
                                    );
                                    let result = aitoolplus_core::opencode_addons::apply(
                                        &ws.paths,
                                        &mut ws.store.store_mut().opencode_addons,
                                        &id,
                                    );
                                    match result {
                                        Ok(path) => {
                                            ws.persist_store();
                                            ws.ui.toast(
                                                format!("applied: {}", path.display()),
                                                false,
                                            );
                                        }
                                        Err(error) => ws.ui.toast(error, true),
                                    }
                                    cx.notify();
                                }
                                _ => ws.ui.toast("configuration must be a JSON object", true),
                            }
                        },
                    ))
                    .children(allow_clear.then(|| {
                        button_l(
                            gpui::SharedString::from(format!("addon-clear-{}", kind.key())),
                            i.t("清除运行时", "Clear Runtime"),
                            ButtonVariant::Danger,
                            &t,
                            cx,
                            move |ws, _, _, cx| {
                                match aitoolplus_core::opencode_addons::clear_applied(
                                    &ws.paths,
                                    &mut ws.store.store_mut().opencode_addons,
                                    kind,
                                ) {
                                    Ok(()) => {
                                        ws.persist_store();
                                        ws.ui.toast("runtime cleared", false);
                                    }
                                    Err(error) => ws.ui.toast(error, true),
                                }
                                cx.notify();
                            },
                        )
                    })),
            );
        section = section.child(card);
    }
    section.into_any_element()
}

// ---------------------------------------------------------------------------
// Grok native Plugins tab
// ---------------------------------------------------------------------------

fn grok_plugins_section(ws: &mut Workspace, cx: &mut Context<Workspace>) -> gpui::AnyElement {
    let t = ws.theme.clone();
    let i = ws.i18n;

    if ws.ui.grok_plugins.is_none() && !ws.ui.grok_plugins_loading {
        load_grok_plugins(ws, cx);
    }

    let is_loading = ws.ui.grok_plugins_loading;
    let refresh_label = if is_loading {
        i.t("刷新中…", "Refreshing…")
    } else {
        i.t("刷新", "Refresh")
    };

    let mut section = div().flex().flex_col().gap(px(12.0)).child(
        div()
            .flex()
            .items_center()
            .justify_between()
            .child(page_header(
                &t,
                i.t("Grok 插件", "Grok Plugins"),
                i.t(
                    "通过 grok plugin 管理原生插件",
                    "Manage native plugins via grok plugin",
                ),
            ))
            .child(button_l(
                "grok-plugins-refresh-btn",
                refresh_label,
                ButtonVariant::Secondary,
                &t,
                cx,
                move |ws, _, _, cx| {
                    ws.ui.grok_plugins = None;
                    load_grok_plugins(ws, cx);
                    cx.notify();
                },
            )),
    );

    let Some(cached) = &ws.ui.grok_plugins else {
        return section
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_center()
                    .p(px(32.0))
                    .rounded(px(8.0))
                    .bg(t.card_bg)
                    .border_1()
                    .border_color(t.card_border)
                    .text_size(px(13.0))
                    .text_color(t.text_secondary)
                    .child(i.t("正在查询 Grok 插件列表…", "Loading Grok plugins…")),
            )
            .into_any_element();
    };

    match cached {
        Ok((installed, available)) => {
            section = section.child(section_title(
                &t,
                i.t("已安装", "Installed"),
                Some(gpui::SharedString::from(format!(
                    "{} plugins",
                    installed.len()
                ))),
            ));
            for plugin in installed {
                let toggle_id = plugin.plugin_id.clone();
                let uninstall_id = plugin.plugin_id.clone();
                let update_id = plugin.plugin_id.clone();
                let enabled = plugin.enabled;
                section = section.child(
                    div()
                        .flex()
                        .flex_col()
                        .items_start()
                        .gap(px(6.0))
                        .p(px(10.0))
                        .rounded(px(8.0))
                        .bg(t.card_bg)
                        .border_1()
                        .border_color(if enabled { t.success } else { t.card_border })
                        .child(
                            div()
                                .text_size(px(12.5))
                                .font_weight(gpui::FontWeight::MEDIUM)
                                .text_color(t.text_primary)
                                .child(format!("{}@{}", plugin.name, plugin.marketplace_name)),
                        )
                        .children(plugin.description.as_ref().map(|description| {
                            div()
                                .text_size(px(11.0))
                                .text_color(t.text_muted)
                                .child(description.clone())
                                .into_any_element()
                        }))
                        .child(
                            div()
                                .flex()
                                .gap(px(8.0))
                                .child(button_l(
                                    gpui::SharedString::from(format!("grok-toggle-{toggle_id}")),
                                    if enabled {
                                        i.t("停用", "Disable")
                                    } else {
                                        i.t("启用", "Enable")
                                    },
                                    ButtonVariant::Secondary,
                                    &t,
                                    cx,
                                    move |ws, _, _, cx| {
                                        let id = toggle_id.clone();
                                        spawn_tool_action(
                                            Some(ToolId::Grok),
                                            ws,
                                            cx,
                                            "插件状态已更新".into(),
                                            "plugin state updated".into(),
                                            move |paths| {
                                                aitoolplus_core::grok_plugins::enable(
                                                    &paths, &id, !enabled,
                                                )
                                            },
                                        );
                                    },
                                ))
                                .child(button_l(
                                    gpui::SharedString::from(format!("grok-update-{update_id}")),
                                    i.t("更新", "Update"),
                                    ButtonVariant::Secondary,
                                    &t,
                                    cx,
                                    move |ws, _, _, cx| {
                                        let id = update_id.clone();
                                        spawn_tool_action(
                                            Some(ToolId::Grok),
                                            ws,
                                            cx,
                                            "插件已更新".into(),
                                            "plugin updated".into(),
                                            move |paths| {
                                                aitoolplus_core::grok_plugins::update(&paths, &id)
                                            },
                                        );
                                    },
                                ))
                                .child(button_l(
                                    gpui::SharedString::from(format!(
                                        "grok-uninstall-{uninstall_id}"
                                    )),
                                    i.t("卸载", "Uninstall"),
                                    ButtonVariant::Danger,
                                    &t,
                                    cx,
                                    move |ws, _, _, cx| {
                                        let id = uninstall_id.clone();
                                        spawn_tool_action(
                                            Some(ToolId::Grok),
                                            ws,
                                            cx,
                                            "插件已卸载".into(),
                                            "plugin uninstalled".into(),
                                            move |paths| {
                                                aitoolplus_core::grok_plugins::uninstall(
                                                    &paths, &id,
                                                )
                                            },
                                        );
                                    },
                                )),
                        ),
                );
            }

            let installed_ids: std::collections::HashSet<String> =
                installed.iter().map(|plugin| plugin.plugin_id.clone()).collect();
            let available_filtered: Vec<_> = available
                .iter()
                .filter(|plugin| !installed_ids.contains(&plugin.plugin_id))
                .collect();
            if !available_filtered.is_empty() {
                section = section.child(section_title(&t, i.t("可安装", "Available"), None));
                for plugin in available_filtered {
                    let action_plugin = plugin.clone();
                    section = section.child(
                        div()
                            .flex()
                            .flex_col()
                            .items_start()
                            .gap(px(6.0))
                            .p(px(10.0))
                            .rounded(px(8.0))
                            .bg(t.card_bg)
                            .border_1()
                            .border_color(t.card_border)
                            .child(
                                div()
                                    .text_size(px(12.5))
                                    .text_color(t.text_primary)
                                    .child(plugin.name.clone()),
                            )
                            .child(button_l(
                                gpui::SharedString::from(format!("grok-install-{}", plugin.plugin_id)),
                                i.t("安装并信任", "Install & Trust"),
                                ButtonVariant::Primary,
                                &t,
                                cx,
                                move |ws, _, _, cx| {
                                    let plugin = action_plugin.clone();
                                    spawn_tool_action(
                                        Some(ToolId::Grok),
                                        ws,
                                        cx,
                                        "插件已安装".into(),
                                        "plugin installed".into(),
                                        move |paths| {
                                            aitoolplus_core::grok_plugins::install(&paths, &plugin)
                                        },
                                    );
                                },
                            )),
                    );
                }
            }
        }
        Err(error) => {
            section = section
                .child(empty_state(
                    &t,
                    "\u{26a0}\u{fe0f}",
                    i.t("Grok 插件列表获取失败", "Failed to list Grok plugins"),
                    "",
                ))
                .child(
                    div()
                        .p(px(12.0))
                        .rounded(px(8.0))
                        .bg(t.danger_subtle)
                        .border_1()
                        .border_color(t.danger)
                        .text_size(px(12.0))
                        .text_color(t.danger)
                        .child(error.clone()),
                );
        }
    }
    section.into_any_element()
}

// ---------------------------------------------------------------------------
// Claude Code Plugins tab
// ---------------------------------------------------------------------------

fn plugins_section(ws: &mut Workspace, cx: &mut Context<Workspace>) -> gpui::AnyElement {
    let t = ws.theme.clone();
    let i = ws.i18n;
    let mut section = div().flex().flex_col().gap(px(12.0)).child(page_header(
        &t,
        i.t("插件管理", "Plugins"),
        i.t(
            "installed_plugins.json + known_marketplaces.json + settings.json enabledPlugins",
            "installed_plugins.json + known_marketplaces.json + settings.json enabledPlugins",
        ),
    ));

    // ---- marketplaces card ----
    match aitoolplus_core::claude_plugins::list_marketplaces(&ws.paths) {
        Ok(markets) => {
            let mut card_inner = div().flex().flex_col().gap(px(8.0));

            if markets.is_empty() {
                card_inner =
                    card_inner.child(div().text_size(px(12.0)).text_color(t.text_muted).child(
                        i.t(
                            "还没有插件市场，在下方输入来源添加（如 anthropics/claude-code）",
                            "No marketplaces yet; add one below (e.g. anthropics/claude-code)",
                        ),
                    ));
            } else {
                for market in &markets {
                    let name = market.name.clone();
                    let name2 = market.name.clone();
                    let name2_del = market.name.clone();
                    let auto = market.auto_update_enabled;
                    let source_text = match &market.source {
                        Value::String(v) => v.clone(),
                        other => serde_json::to_string(other).unwrap_or_default(),
                    };
                    let meta = [
                        market.version.clone().map(|v| format!("v{v}")),
                        (market.plugin_count > 0)
                            .then(|| format!("{} plugins", market.plugin_count)),
                        (!source_text.is_empty()).then_some(source_text.clone()),
                    ]
                    .into_iter()
                    .flatten()
                    .collect::<Vec<_>>()
                    .join(" · ");

                    card_inner = card_inner.child(
                        div()
                            .id(gpui::SharedString::from(format!("market-{}", market.name)))
                            .flex()
                            .flex_col()
                            .w_full()
                            .min_w(px(0.0))
                            .items_start()
                            .gap(px(8.0))
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
                                    .flex_1()
                                    .min_w(px(0.0))
                                    .child(
                                        div()
                                            .text_size(px(12.5))
                                            .font_weight(gpui::FontWeight::MEDIUM)
                                            .text_color(t.text_primary)
                                            .child(name.clone()),
                                    )
                                    .children((!meta.is_empty()).then(|| {
                                        div()
                                            .text_size(px(11.0))
                                            .text_color(t.text_muted)
                                            .child(meta.clone())
                                            .into_any_element()
                                    })),
                            )
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(6.0))
                                    .text_size(px(11.0))
                                    .text_color(t.text_secondary)
                                    .child(i.t("自动更新", "Auto-update"))
                                    .child(components::toggle(
                                        gpui::SharedString::from(format!(
                                            "mkt-auto-{}",
                                            market.name
                                        )),
                                        auto,
                                        &t,
                                        cx,
                                        move |ws, _, _, cx| {
                                            let target = !auto;
                                            match aitoolplus_core::claude_plugins::set_marketplace_auto_update(
                                                &ws.paths,
                                                &name,
                                                target,
                                            ) {
                                                Ok(_) => {
                                                    let msg = if target {
                                                        ws.i18n
                                                            .t("已开启自动更新", "auto-update on")
                                                            .to_string()
                                                    } else {
                                                        ws.i18n
                                                            .t("已关闭自动更新", "auto-update off")
                                                            .to_string()
                                                    };
                                                    ws.ui.toast(msg, false);
                                                }
                                                Err(e) => {
                                                    ws.ui.toast(format!("failed: {e}"), true);
                                                }
                                            }
                                            cx.notify();
                                        },
                                    )),
                            )
                            .child(button_l(
                                gpui::SharedString::from(format!("mkt-upd-{}", market.name)),
                                i.t("更新市场", "Update"),
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
                                            "插件市场已更新".into(),
                                            "marketplace updated".into(),
                                            move |paths| {
                                                aitoolplus_core::claude_plugins::update_marketplace(
                                                    &paths,
                                                    Some(&operation_name),
                                                )
                                                .map_err(|error| format!("update failed: {error}"))
                                            },
                                        );
                                    }
                                }
                            ))
                            .child(button_l(
                                gpui::SharedString::from(format!("mkt-del-{}", market.name)),
                                i.t("移除市场", "Remove"),
                                ButtonVariant::Danger,
                                &t,
                                cx,
                                move |ws, _, _, cx| {
                                    let success_zh = format!("已移除 {name2}");
                                    let success_en = format!("removed {name2}");
                                    let operation_name = name2_del.clone();
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
                            )),
                    );
                }
            }

            let add_entity = ws.ui.claude_marketplaces_input.clone();
            card_inner = card_inner.child(
                div()
                    .id("mkt-add-row")
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .p(px(8.0))
                    .rounded(px(8.0))
                    .bg(t.input_bg)
                    .border_1()
                    .border_color(t.card_border)
                    .child(div().flex_1().min_w(px(0.0)).child(add_entity.clone()))
                    .child(button_l(
                        "mkt-add-btn",
                        i.t("添加市场", "Add"),
                        ButtonVariant::Secondary,
                        &t,
                        cx,
                        move |ws, _, _, cx| {
                            let src: String =
                                add_entity.update(cx, |inp, cx| {
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
                            let success_zh = format!("已添加 {src}");
                            let success_en = format!("added {src}");
                            spawn_tool_action(Some(ToolId::ClaudeCode), ws, cx, success_zh, success_en, move |paths| {
                                aitoolplus_core::claude_plugins::add_marketplace(&paths, &src)
                                    .map_err(|error| format!("add failed: {error}"))
                            });
                        },
                    )),
            );

            section = section.child(section_title(&t, i.t("插件市场", "Marketplaces"), None));
            section = section.child(card_inner);
        }
        Err(e) => {
            section = section.child(
                div()
                    .p(px(12.0))
                    .rounded(px(8.0))
                    .bg(t.danger_subtle)
                    .border_1()
                    .border_color(t.danger)
                    .text_size(px(12.0))
                    .text_color(t.danger)
                    .child(format!(
                        "{}: {e}",
                        i.t("市场读取失败", "marketplaces read failed")
                    )),
            );
        }
    }

    // ---- marketplace-available plugins ----
    if let (Ok(available), Ok(installed)) = (
        aitoolplus_core::claude_plugins::list_marketplace_plugins(&ws.paths),
        aitoolplus_core::claude_plugins::list_installed_plugins(&ws.paths),
    ) {
        let installed_ids: std::collections::HashSet<String> = installed
            .iter()
            .map(|plugin| plugin.plugin_id.clone())
            .collect();
        let available: Vec<_> = available
            .into_iter()
            .filter(|plugin| !installed_ids.contains(&plugin.plugin_id))
            .collect();
        if !available.is_empty() {
            section = section.child(section_title(
                &t,
                i.t("可安装插件", "Available Plugins"),
                Some(i.t(
                    "来自已添加市场的插件",
                    "Plugins from configured marketplaces",
                )),
            ));
            let mut panel = div().flex().flex_col().gap(px(6.0));
            for plugin in available {
                let plugin_id = plugin.plugin_id.clone();
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
                                .text_size(px(12.5))
                                .font_weight(gpui::FontWeight::MEDIUM)
                                .text_color(t.text_primary)
                                .child(format!("{}@{}", plugin.name, plugin.marketplace_name)),
                        )
                        .children(plugin.description.map(|description| {
                            div()
                                .text_size(px(11.0))
                                .text_color(t.text_muted)
                                .child(description)
                                .into_any_element()
                        }))
                        .child(button_l(
                            gpui::SharedString::from(format!("plugin-install-{plugin_id}")),
                            i.t("安装", "Install"),
                            ButtonVariant::Primary,
                            &t,
                            cx,
                            move |_ws, _, _, cx| {
                                let id = plugin_id.clone();
                                let weak = cx.entity().downgrade();
                                let paths =
                                    weak.update(cx, |workspace, _| workspace.paths.clone()).ok();
                                if let Some(paths) = paths {
                                    cx.spawn(async move |_this, cx| {
                                        let result = cx
                                            .background_spawn(async move {
                                                aitoolplus_core::claude_plugins::install_plugin(
                                                    &paths, &id,
                                                )
                                            })
                                            .await;
                                        let _ = weak.update(cx, |workspace, cx| {
                                            match result {
                                                Ok(()) => workspace.ui.toast(
                                                    workspace
                                                        .i18n
                                                        .t("插件已安装", "plugin installed")
                                                        .to_string(),
                                                    false,
                                                ),
                                                Err(error) => workspace.ui.toast(
                                                    format!("install failed: {error}"),
                                                    true,
                                                ),
                                            }
                                            cx.notify();
                                        });
                                    })
                                    .detach();
                                }
                            },
                        )),
                );
            }
            section = section.child(panel);
        }
    }

    // ---- installed plugins card ----
    match aitoolplus_core::claude_plugins::list_installed_plugins(&ws.paths) {
        Ok(plugins) => {
            let mut card_inner = div().flex().flex_col().gap(px(8.0));

            if plugins.is_empty() {
                card_inner =
                    card_inner.child(div().text_size(px(12.0)).text_color(t.text_muted).child(
                        i.t(
                            "没有已安装插件（先添加市场，再用 claude plugin install 安装）",
                            "No installed plugins (add a marketplace, then claude plugin install)",
                        ),
                    ));
            } else {
                for plugin in &plugins {
                    let pid = plugin.plugin_id.clone();
                    let pid2 = plugin.plugin_id.clone();
                    let enabled = plugin.user_scope_enabled;

                    let mut caps = div().flex().gap(px(4.0)).flex_wrap();
                    for (has, label) in [
                        (plugin.has_skills, "skills"),
                        (plugin.has_agents, "agents"),
                        (plugin.has_hooks, "hooks"),
                        (plugin.has_mcp_servers, "mcp"),
                        (plugin.has_lsp_servers, "lsp"),
                    ] {
                        if has {
                            let t2 = t.clone();
                            let label: gpui::SharedString = label.into();
                            caps = caps.child(
                                div()
                                    .flex()
                                    .items_center()
                                    .h(px(18.0))
                                    .px(px(6.0))
                                    .rounded(px(4.0))
                                    .text_size(px(10.0))
                                    .bg(t2.success_subtle)
                                    .text_color(t2.success)
                                    .child(label),
                            );
                        }
                    }

                    let version_line = [
                        plugin.version.clone().map(|v| format!("v{v}")),
                        plugin.install_path.clone(),
                    ]
                    .into_iter()
                    .flatten()
                    .collect::<Vec<_>>()
                    .join(" · ");

                    card_inner = card_inner.child(
                        div()
                            .id(gpui::SharedString::from(format!(
                                "plugin-{}",
                                plugin.plugin_id
                            )))
                            .flex()
                            .flex_col()
                            .w_full()
                            .min_w(px(0.0))
                            .items_start()
                            .gap(px(8.0))
                            .p(px(10.0))
                            .rounded(px(8.0))
                            .bg(t.input_bg)
                            .border_1()
                            .border_color(if enabled { t.success } else { t.card_border })
                            .child(components::toggle(
                                gpui::SharedString::from(format!(
                                    "plugin-toggle-{}",
                                    plugin.plugin_id
                                )),
                                enabled,
                                &t,
                                cx,
                                move |ws, _, _, cx| {
                                    let target = !enabled;
                                    match aitoolplus_core::claude_plugins::set_plugin_enabled(
                                        &ws.paths, &pid, target,
                                    ) {
                                        Ok(_) => {
                                            let msg = if target {
                                                ws.i18n.t("已启用", "enabled").to_string()
                                            } else {
                                                ws.i18n.t("已停用", "disabled").to_string()
                                            };
                                            ws.ui.toast(msg, false);
                                        }
                                        Err(e) => ws.ui.toast(format!("failed: {e}"), true),
                                    }
                                    cx.notify();
                                },
                            ))
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap(px(2.0))
                                    .flex_1()
                                    .min_w(px(0.0))
                                    .child(
                                        div()
                                            .text_size(px(12.5))
                                            .font_weight(gpui::FontWeight::MEDIUM)
                                            .text_color(t.text_primary)
                                            .child(format!(
                                                "{}@{}",
                                                plugin.name, plugin.marketplace_name
                                            )),
                                    )
                                    .children(plugin.description.clone().map(|d| {
                                        div()
                                            .text_size(px(11.0))
                                            .text_color(t.text_muted)
                                            .child(d)
                                            .into_any_element()
                                    }))
                                    .when(!version_line.is_empty(), |el| {
                                        el.child(
                                            div()
                                                .text_size(px(10.5))
                                                .text_color(t.text_muted)
                                                .child(version_line.clone()),
                                        )
                                    }),
                            )
                            .child(caps)
                            .child(button_l(
                                gpui::SharedString::from(format!(
                                    "plugin-del-{}",
                                    plugin.plugin_id
                                )),
                                i.t("卸载", "Uninstall"),
                                ButtonVariant::Danger,
                                &t,
                                cx,
                                move |ws, _, _, cx| {
                                    let success_zh = format!("已卸载 {pid2}");
                                    let success_en = format!("uninstalled {pid2}");
                                    let operation_id = pid2.clone();
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
                                },
                            )),
                    );
                }

                card_inner = card_inner.child(
                    div()
                        .flex()
                        .justify_end()
                        .gap(px(8.0))
                        .child(button_l(
                            "plugins-disable-all",
                            i.t("全部停用", "Disable All"),
                            ButtonVariant::Secondary,
                            &t,
                            cx,
                            move |ws, _, _, cx| {
                                match aitoolplus_core::claude_plugins::set_all_plugins_enabled(
                                    &ws.paths, false,
                                ) {
                                    Ok((count, _)) => {
                                        let msg = ws
                                            .i18n
                                            .t(
                                                &format!("已停用 {count} 个插件"),
                                                &format!("disabled {count} plugins"),
                                            )
                                            .to_string();
                                        ws.ui.toast(msg, false);
                                    }
                                    Err(e) => ws.ui.toast(format!("failed: {e}"), true),
                                }
                                cx.notify();
                            },
                        ))
                        .child(button_l(
                            "plugins-enable-all",
                            i.t("全部启用", "Enable All"),
                            ButtonVariant::Secondary,
                            &t,
                            cx,
                            move |ws, _, _, cx| {
                                match aitoolplus_core::claude_plugins::set_all_plugins_enabled(
                                    &ws.paths, true,
                                ) {
                                    Ok((count, _)) => {
                                        let msg = ws
                                            .i18n
                                            .t(
                                                &format!("已启用 {count} 个插件"),
                                                &format!("enabled {count} plugins"),
                                            )
                                            .to_string();
                                        ws.ui.toast(msg, false);
                                    }
                                    Err(e) => ws.ui.toast(format!("failed: {e}"), true),
                                }
                                cx.notify();
                            },
                        )),
                );
            }

            section = section.child(section_title(
                &t,
                i.t("已安装插件", "Installed Plugins"),
                None,
            ));
            section = section.child(card_inner);
        }
        Err(e) => {
            section = section.child(
                div()
                    .p(px(12.0))
                    .rounded(px(8.0))
                    .bg(t.danger_subtle)
                    .border_1()
                    .border_color(t.danger)
                    .text_size(px(12.0))
                    .text_color(t.danger)
                    .child(format!(
                        "{}: {e}",
                        i.t("插件读取失败", "plugins read failed")
                    )),
            );
        }
    }

    section.into_any_element()
}

// ---------------------------------------------------------------------------
// Dialogs
// ---------------------------------------------------------------------------

pub fn open_provider_dialog(
    editing_id: Option<String>,
    tool: ToolId,
    ws: &mut Workspace,
    cx: &mut Context<Workspace>,
) {
    let i = ws.i18n;
    let existing = editing_id.as_ref().and_then(|id| {
        ws.store
            .store()
            .tool(tool)
            .providers
            .iter()
            .find(|p| p.id == *id)
            .cloned()
    });

    let name = cx.new(|cx| {
        let mut input = TextInput::new(i.t("名称", "Name"), cx);
        if let Some(p) = &existing {
            input.set_text_silent(p.name.clone(), cx);
        }
        input
    });
    let settings = cx.new(|cx| {
        let mut ta = TextArea::new("{}", cx);
        let content = existing
            .as_ref()
            .map(|p| p.settings_config.clone())
            .unwrap_or_else(|| {
                serde_json::to_string_pretty(&aitoolplus_core::providers::default_settings_for(
                    tool,
                ))
                .unwrap_or_default()
            });
        ta.set_text_silent(content, cx);
        ta
    });
    let notes = cx.new(|cx| {
        let mut input = TextInput::new(i.t("备注（可选）", "Notes (optional)"), cx);
        if let Some(p) = &existing {
            input.set_text_silent(p.notes.clone().unwrap_or_default(), cx);
        }
        input
    });
    let website = cx.new(|cx| {
        let mut input = TextInput::new(i.t("网址（可选）", "Website (optional)"), cx);
        if let Some(p) = &existing {
            input.set_text_silent(p.website_url.clone().unwrap_or_default(), cx);
        }
        input
    });
    let api_key = cx.new(|cx| TextInput::new(i.t("API Key", "API Key"), cx));

    let category = existing
        .as_ref()
        .map(|p| p.category.clone())
        .unwrap_or_else(|| "custom".to_string());

    ws.ui.provider_dialog = Some(ProviderDialogState {
        editing_id,
        tool,
        name,
        category,
        settings,
        notes,
        website,
        preset_index: None,
        api_key,
    });
    cx.notify();
}

pub fn render_provider_dialog(
    state: ProviderDialogState,
    ws: &mut Workspace,
    cx: &mut Context<Workspace>,
) -> gpui::AnyElement {
    let t = ws.theme.clone();
    let i = ws.i18n;
    let ProviderDialogState {
        editing_id,
        tool,
        name,
        category,
        settings,
        notes,
        website,
        preset_index,
        api_key,
    } = state;

    let title = if editing_id.is_some() {
        i.t("编辑供应商", "Edit Provider")
    } else {
        i.t("新增供应商", "Add Provider")
    };

    let field_label = |label: gpui::SharedString| -> gpui::AnyElement {
        div()
            .text_size(px(12.0))
            .font_weight(gpui::FontWeight::MEDIUM)
            .text_color(t.text_secondary)
            .child(label)
            .into_any_element()
    };

    let presets = aitoolplus_core::presets::presets_for(tool);
    let preset_names: Vec<gpui::SharedString> =
        std::iter::once(i.t("空白 / Blank", "Blank / Custom"))
            .chain(presets.iter().map(|p| gpui::SharedString::from(p.name)))
            .collect();

    let tool_owned = tool;
    let mut body = div().flex().flex_col().gap(px(12.0));

    // Preset selector (cc-switch parity): pick a vendor, only the key is needed.
    if !presets.is_empty() && editing_id.is_none() {
        body = body.child(
            div()
                .flex()
                .flex_col()
                .gap(px(6.0))
                .child(field_label(i.t("预设 / Preset", "Preset")))
                .child(div().flex().flex_wrap().gap(px(4.0)).children(
                    preset_names.iter().enumerate().map(|(idx, label)| {
                        let is_on = match preset_index {
                            None => idx == 0,
                            Some(p) => idx == p + 1,
                        };
                        let t2 = t.clone();
                        div()
                            .id(gpui::SharedString::from(format!("preset-opt-{idx}")))
                            .cursor_pointer()
                            .flex()
                            .items_center()
                            .h(px(24.0))
                            .px(px(8.0))
                            .rounded(px(6.0))
                            .text_size(px(11.5))
                            .when(is_on, |st| {
                                st.bg(t2.accent_subtle)
                                    .border_1()
                                    .border_color(t2.accent)
                                    .text_color(t2.accent)
                            })
                            .when(!is_on, |st| {
                                st.bg(t2.input_bg)
                                    .border_1()
                                    .border_color(t2.input_border)
                                    .text_color(t2.text_secondary)
                            })
                            .hover(|h| h.text_color(t2.text_primary))
                            .on_click(cx.listener(move |ws, _ev, _w, cx| {
                                if let Some(dialog) = ws.ui.provider_dialog.as_mut() {
                                    let p_idx = if idx == 0 { None } else { Some(idx - 1) };
                                    dialog.preset_index = p_idx;
                                    if let Some(p_idx) = p_idx {
                                        let presets =
                                            aitoolplus_core::presets::presets_for(dialog.tool);
                                        if let Some(preset) = presets.get(p_idx) {
                                            dialog.name.update(cx, |inp, cx| {
                                                inp.set_text_silent(preset.name, cx)
                                            });
                                            dialog.category = preset.category.to_string();
                                            dialog.website.update(cx, |inp, cx| {
                                                inp.set_text_silent(preset.website_url, cx)
                                            });
                                            dialog.settings.update(cx, |ta, cx| {
                                                ta.set_text_silent(
                                                    serde_json::to_string_pretty(&preset.settings)
                                                        .unwrap_or_default(),
                                                    cx,
                                                );
                                            });
                                        }
                                    }
                                }
                                cx.notify();
                            }))
                            .child(label.clone())
                    }),
                )),
        );
    }

    body = body.child(
        div()
            .flex()
            .flex_col()
            .gap(px(6.0))
            .child(field_label(i.t("名称", "Name")))
            .child(input_container(&t, name.clone())),
    );

    // API-key-only input when a preset is selected (cc-switch flow)
    if let Some(p_idx) = preset_index {
        let presets = aitoolplus_core::presets::presets_for(tool);
        if let Some(preset) = presets.get(p_idx) {
            if !preset.api_key_field.is_empty() {
                body = body.child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(6.0))
                        .child(field_label(
                            i.t("API Key（填好即可保存）", "API Key (that's all you need)"),
                        ))
                        .child(input_container(&t, api_key.clone()))
                        .child(
                            div()
                                .text_size(px(11.0))
                                .text_color(t.text_muted)
                                .child(format!(
                                    "{}: {}",
                                    i.t("获取 Key", "Get key"),
                                    preset.website_url
                                )),
                        ),
                );
            } else {
                body = body.child(
                    div()
                        .px(px(10.0))
                        .py(px(8.0))
                        .rounded(px(6.0))
                        .bg(t.input_bg)
                        .border_1()
                        .border_color(t.input_border)
                        .text_size(px(12.0))
                        .text_color(t.text_secondary)
                        .child(i.t(
                            "官方预设使用官方原生登录凭据，无需填写 API Key，保存后点击「应用」即可恢复官方直连。",
                            "Official preset uses native login credentials, no API key required. Click Apply to restore official access.",
                        )),
                );
            }
        }
    }

    body = body
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(6.0))
                .child(field_label(i.t("分类", "Category")))
                .child({
                    let mut category_row = div().flex().gap(px(6.0)).flex_wrap();
                    for cat in CATEGORIES {
                        let is_current = cat == category;
                        let label: gpui::SharedString = match cat {
                            "official" => i.t("官方", "Official"),
                            "custom" => i.t("自定义", "Custom"),
                            "proxy" => i.t("代理", "Proxy"),
                            "subscription" => i.t("订阅", "Subscription"),
                            _ => i.t("其他", "Other"),
                        };
                        let cat2 = cat.to_string();
                        category_row = category_row.child(
                            div()
                                .id(gpui::SharedString::from(format!("cat-{cat}")))
                                .cursor_pointer()
                                .flex()
                                .items_center()
                                .h(px(24.0))
                                .px(px(10.0))
                                .rounded(px(6.0))
                                .text_size(px(12.0))
                                .when(is_current, |s| {
                                    s.bg(t.accent_subtle)
                                        .border_1()
                                        .border_color(t.accent)
                                        .text_color(t.accent)
                                })
                                .when(!is_current, |s| {
                                    s.bg(t.input_bg)
                                        .border_1()
                                        .border_color(t.input_border)
                                        .text_color(t.text_secondary)
                                })
                                .on_click(cx.listener(move |this, _ev, _w, cx| {
                                    if let Some(dialog) = this.ui.provider_dialog.as_mut() {
                                        dialog.category = cat2.clone();
                                    }
                                    cx.notify();
                                }))
                                .child(label),
                        );
                    }
                    category_row
                }),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(6.0))
                .child(field_label(i.t("配置 JSON", "Settings JSON")))
                .child(textarea_container(&t, settings.clone())),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(6.0))
                .child(field_label(i.t("备注", "Notes")))
                .child(input_container(&t, notes.clone())),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(6.0))
                .child(field_label(i.t("网址", "Website")))
                .child(input_container(&t, website.clone())),
        )
        .child(
            div()
                .flex()
                .justify_end()
                .gap(px(8.0))
                .child(button_l(
                    "prov-dlg-cancel",
                    i.t("取消", "Cancel"),
                    ButtonVariant::Secondary,
                    &t,
                    cx,
                    |ws, _, _, cx| {
                        ws.ui.provider_dialog = None;
                        cx.notify();
                    },
                ))
                .child(button_l(
                    "prov-dlg-save",
                    i.t("保存", "Save"),
                    ButtonVariant::Primary,
                    &t,
                    cx,
                    move |ws, _, _, cx| {
                        save_provider(
                            tool_owned,
                            editing_id.clone(),
                            name.clone(),
                            category.clone(),
                            settings.clone(),
                            notes.clone(),
                            website.clone(),
                            preset_index,
                            api_key.clone(),
                            ws,
                            cx,
                        );
                    },
                )),
        );

    modal_scaffold(
        &t,
        title.as_ref(),
        body.into_any_element(),
        cx,
        |ws, _, _, cx| {
            ws.ui.provider_dialog = None;
            cx.notify();
        },
    )
}

#[allow(clippy::too_many_arguments)]
fn save_provider(
    tool: ToolId,
    editing_id: Option<String>,
    name: gpui::Entity<TextInput>,
    category: String,
    settings: gpui::Entity<TextArea>,
    notes: gpui::Entity<TextInput>,
    website: gpui::Entity<TextInput>,
    preset_index: Option<usize>,
    api_key: gpui::Entity<TextInput>,
    ws: &mut Workspace,
    cx: &mut Context<Workspace>,
) {
    let i = ws.i18n;
    let name_txt: String = name.update(cx, |inp, _| inp.text().to_string());
    let notes_txt: String = notes.update(cx, |inp, _| inp.text().to_string());
    let website_txt: String = website.update(cx, |inp, _| inp.text().to_string());

    let settings_txt: String = match preset_index {
        Some(p_idx) => {
            let key_txt: String = api_key.update(cx, |inp, _| inp.text().to_string());
            let presets = aitoolplus_core::presets::presets_for(tool);
            match presets.get(p_idx) {
                Some(preset) => {
                    if !preset.api_key_field.is_empty() && key_txt.trim().is_empty() {
                        let msg = i
                            .t(
                                "预设已选择，请填写 API Key",
                                "preset selected; API key required",
                            )
                            .to_string();
                        ws.ui.toast(msg, true);
                        cx.notify();
                        return;
                    }
                    serde_json::to_string_pretty(&preset.settings_with_key(key_txt.trim()))
                        .unwrap_or_default()
                }
                None => {
                    let msg = i.t("预设不存在", "preset missing").to_string();
                    ws.ui.toast(msg, true);
                    cx.notify();
                    return;
                }
            }
        }
        None => settings.update(cx, |ta, _| ta.text().to_string()),
    };

    if name_txt.trim().is_empty() {
        let msg = i.t("名称不能为空", "name is required").to_string();
        ws.ui.toast(msg, true);
        cx.notify();
        return;
    }
    if let Err(e) = serde_json::from_str::<Value>(&settings_txt) {
        let msg = i
            .t(
                &format!("配置 JSON 无效：{e}"),
                &format!("invalid settings JSON: {e}"),
            )
            .to_string();
        ws.ui.toast(msg, true);
        cx.notify();
        return;
    }

    let _ = ws.store.update(|store| {
        let section = store.tool_mut(tool);
        match editing_id {
            Some(id) => {
                aitoolplus_core::providers::update(&mut section.providers, &id, |p| {
                    p.name = name_txt.clone();
                    p.category = category.clone();
                    p.settings_config = settings_txt.clone();
                    p.notes = (!notes_txt.is_empty()).then(|| notes_txt.clone());
                    p.website_url = (!website_txt.is_empty()).then(|| website_txt.clone());
                });
            }
            None => {
                let mut p = aitoolplus_core::providers::ProviderRecord::new(
                    name_txt.clone(),
                    category.clone(),
                );
                p.settings_config = settings_txt.clone();
                p.notes = (!notes_txt.is_empty()).then(|| notes_txt.clone());
                p.website_url = (!website_txt.is_empty()).then(|| website_txt.clone());
                section.providers.push(p);
            }
        }
    });
    ws.persist_store();
    ws.ui.provider_dialog = None;
    let msg = i.t("已保存", "saved").to_string();
    ws.ui.toast(msg, false);
    cx.notify();
}

pub fn open_prompt_dialog(
    editing_id: Option<String>,
    tool: ToolId,
    ws: &mut Workspace,
    cx: &mut Context<Workspace>,
) {
    let i = ws.i18n;
    let existing = editing_id.as_ref().and_then(|id| {
        ws.store
            .store()
            .tool(tool)
            .prompts
            .iter()
            .find(|p| p.id == *id)
            .cloned()
    });

    let name = cx.new(|cx| {
        let mut input = TextInput::new(i.t("名称", "Name"), cx);
        if let Some(p) = &existing {
            input.set_text_silent(p.name.clone(), cx);
        }
        input
    });
    let content = cx.new(|cx| {
        let mut ta = TextArea::new(i.t("Prompt 内容…", "Prompt content…"), cx);
        if let Some(p) = &existing {
            ta.set_text_silent(p.content.clone(), cx);
        }
        ta
    });

    ws.ui.prompt_dialog = Some(PromptDialogState {
        editing_id,
        tool,
        name,
        content,
    });
    cx.notify();
}

pub fn render_prompt_dialog(
    state: PromptDialogState,
    ws: &mut Workspace,
    cx: &mut Context<Workspace>,
) -> gpui::AnyElement {
    let t = ws.theme.clone();
    let i = ws.i18n;
    let PromptDialogState {
        editing_id,
        tool,
        name,
        content,
    } = state;

    let title = if editing_id.is_some() {
        i.t("编辑 Prompt", "Edit Prompt")
    } else {
        i.t("新增 Prompt", "Add Prompt")
    };

    let field_label = |label: gpui::SharedString| -> gpui::AnyElement {
        div()
            .text_size(px(12.0))
            .font_weight(gpui::FontWeight::MEDIUM)
            .text_color(t.text_secondary)
            .child(label)
            .into_any_element()
    };

    let tool_owned = tool;
    let body = div()
        .flex()
        .flex_col()
        .gap(px(12.0))
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(6.0))
                .child(field_label(i.t("名称", "Name")))
                .child(input_container(&t, name.clone())),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(6.0))
                .child(field_label(i.t("内容（Markdown）", "Content (Markdown)")))
                .child(textarea_container(&t, content.clone())),
        )
        .child(
            div()
                .flex()
                .justify_end()
                .gap(px(8.0))
                .child(button_l(
                    "prompt-dlg-cancel",
                    i.t("取消", "Cancel"),
                    ButtonVariant::Secondary,
                    &t,
                    cx,
                    |ws, _, _, cx| {
                        ws.ui.prompt_dialog = None;
                        cx.notify();
                    },
                ))
                .child(button_l(
                    "prompt-dlg-save",
                    i.t("保存", "Save"),
                    ButtonVariant::Primary,
                    &t,
                    cx,
                    move |ws, _, _, cx| {
                        let name_txt: String = name.update(cx, |inp, _| inp.text().to_string());
                        let content_txt: String = content.update(cx, |ta, _| ta.text().to_string());
                        if name_txt.trim().is_empty() {
                            let msg = ws.i18n.t("名称不能为空", "name is required").to_string();
                            ws.ui.toast(msg, true);
                            cx.notify();
                            return;
                        }
                        let _ = ws.store.update(|store| {
                            let section = store.tool_mut(tool_owned);
                            match editing_id.clone() {
                                Some(id) => {
                                    aitoolplus_core::prompt::update(
                                        &mut section.prompts,
                                        &id,
                                        |p| {
                                            p.name = name_txt.clone();
                                            p.content = content_txt.clone();
                                        },
                                    );
                                }
                                None => {
                                    section.prompts.push(
                                        aitoolplus_core::prompt::PromptRecord::new(
                                            name_txt.clone(),
                                            content_txt.clone(),
                                        ),
                                    );
                                }
                            }
                        });
                        ws.persist_store();
                        ws.ui.prompt_dialog = None;
                        let msg = ws.i18n.t("已保存", "saved").to_string();
                        ws.ui.toast(msg, false);
                        cx.notify();
                    },
                )),
        );

    modal_scaffold(
        &t,
        title.as_ref(),
        body.into_any_element(),
        cx,
        |ws, _, _, cx| {
            ws.ui.prompt_dialog = None;
            cx.notify();
        },
    )
}
