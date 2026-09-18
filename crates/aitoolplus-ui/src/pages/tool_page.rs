//! Per-tool page: provider list, common config editor, global prompts,
//! runtime files, Pi model settings / other settings / extensions,
//! Claude Code plugins.

use aitoolplus_core::pi_extensions::{PiExtensionKind, PiExtensionScope};
use aitoolplus_core::pi_pages::PiModelSettings;
use aitoolplus_core::providers::{CATEGORIES, ProviderRecord};
use aitoolplus_core::tools::ToolId;
use gpui::{Context, IntoElement, deferred, div, prelude::*, px};
use gpui_kit::base::{Align, ElementExt as _, Placement, Positioner, POPUP_PRIORITY};
use serde_json::Value;

use crate::components::{
    self, BadgeKind, ButtonVariant, badge, button_l, button_with_icon_l,
    button_with_icon_loading_l, input_container, page_header, section_title, textarea_container,
};
use crate::text_area::TextArea;
use crate::text_input::TextInput;
use crate::workspace::Workspace;

use super::{PromptDialogState, ProviderDialogState, ToolTab, modal_scaffold, modal_scaffold_custom};

pub fn render_tool_page(
    tool: ToolId,
    ws: &mut Workspace,
    cx: &mut Context<Workspace>,
) -> gpui::AnyElement {
    let _i = ws.i18n;
    let _t = ws.theme.clone();

    let mut col = div()
        .flex()
        .flex_col()
        .w_full()
        .min_w(px(0.0))
        .gap(px(16.0))
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
    let t = &ws.theme;
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

    crate::components::segmented_tab_bar(
        "tool",
        tabs,
        current,
        t,
        cx,
        |this, tab, _window, cx| {
            this.ui.tool_tab = tab;
            cx.notify();
        },
    )
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
        .child(button_with_icon_l(
            "prov-test-all",
            crate::icons::REFRESH_SVG,
            i.t("批量测试", "Test All"),
            ButtonVariant::Secondary,
            &t,
            cx,
            move |ws, _, _, cx| batch_test_providers(tool, ws, cx),
        ))
        .child(button_with_icon_l(
            "prov-add",
            crate::icons::PLUS_SVG,
            add_label,
            ButtonVariant::Primary,
            &t,
            cx,
            move |ws, _, window, cx| {
                open_provider_dialog(None, tool, ws, cx);
                if let Some(dlg) = &ws.ui.provider_dialog {
                    dlg.name.update(cx, |name, cx| {
                        name.focus_handle.focus(window, cx);
                        name.start_blink(cx);
                    });
                }
            },
        ));

    let header = div()
        .flex()
        .items_center()
        .justify_between()
        .w_full()
        .child(
            div()
                .text_size(px(13.0))
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .text_color(t.text_secondary)
                .child(format!(
                    "{} {}",
                    providers.len(),
                    i.t("个供应商配置", "providers configured")
                )),
        )
        .child(actions);

    let mut section = div().flex().flex_col().gap(px(12.0)).child(header);

    if providers.is_empty() {
        section = section.child(crate::components::empty_state_svg(
            &t,
            crate::icons::PACKAGE_SVG,
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
    icon_svg: &'static [u8],
    tooltip: impl Into<gpui::SharedString>,
    is_danger: bool,
    t: &crate::theme::Theme,
    cx: &mut Context<Workspace>,
    on_click: impl Fn(&mut Workspace, &gpui::ClickEvent, &mut gpui::Window, &mut Context<Workspace>) + 'static,
) -> gpui::AnyElement {
    crate::components::icon_button_svg(id, icon_svg, tooltip, is_danger, t, cx, on_click)
}

fn extract_provider_subtitle(tool: ToolId, p: &ProviderRecord, _i: &crate::i18n::I18n) -> String {
    if p.category == "official" || aitoolplus_core::providers::is_official_provider(tool, &p.id) {
        let official_url = match tool {
            ToolId::ClaudeCode => "https://www.anthropic.com/claude-code",
            ToolId::Codex => "https://chatgpt.com",
            ToolId::GeminiCli => "https://gemini.google.com",
            ToolId::Grok => "https://x.ai",
            ToolId::Kimi => "https://kimi.moonshot.cn",
            ToolId::OpenCode => "https://opencode.ai",
            ToolId::OpenClaw => "https://openclaw.ai",
            ToolId::Pi => "https://pi.ai",
            ToolId::OhMyPi => "https://github.com/canisminor1990/oh-my-pi",
            ToolId::ClaudeDesktop => "https://claude.ai/download",
            ToolId::Hermes => "https://hermes.ai",
            ToolId::Dsh => "https://dsh.ai",
        };
        if !official_url.is_empty() {
            return official_url.to_string();
        }
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

    let avatar_spec = crate::icons::provider_avatar_spec(tool, &p.name, &p.category);
    let subtitle = extract_provider_subtitle(tool, p, &i);

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
                .size(px(16.0))
                .flex()
                .items_center()
                .justify_center()
                .cursor_grab()
                .child(
                    gpui::svg()
                        .data(crate::icons::GRIP_VERTICAL_SVG)
                        .size(px(14.0))
                        .text_color(crate::rgba_const(0xffffff28)),
                ),
        )
        .child(
            div()
                .size(px(38.0))
                .flex_shrink_0()
                .rounded(px(8.0))
                .bg(avatar_spec.bg)
                .border_1()
                .border_color(crate::rgba_const(0xffffff18))
                .flex()
                .items_center()
                .justify_center()
                .child(
                    gpui::svg()
                        .data(avatar_spec.svg_data)
                        .size(px(20.0))
                        .text_color(avatar_spec.fg)
                        .flex_none(),
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
                            crate::components::badge(&t, i.t("官方直连", "Official"), crate::components::BadgeKind::Neutral)
                        }))
                        .children(p.is_disabled.then(|| {
                            crate::components::badge(&t, i.t("已停用", "Disabled"), crate::components::BadgeKind::Danger)
                        })),
                )
                .children((!subtitle.is_empty()).then(|| {
                    div()
                        .text_size(px(12.0))
                        .text_color(t.text_muted)
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
                .rounded(px(16.0))
                .bg(gpui::rgb(0x2563eb))
                .text_size(px(12.0))
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .text_color(crate::rgba_const(0xffffffff))
                .flex()
                .items_center()
                .gap(px(5.0))
                .shadow_xs()
                .child(
                    gpui::svg()
                        .data(crate::icons::CHECK_SVG)
                        .size(px(12.0))
                        .text_color(crate::rgba_const(0xffffffff))
                        .flex_none(),
                )
                .child(i.t("使用中", "In Use")),
        );
    } else {
        actions = actions.child(
            div()
                .id(gpui::SharedString::from(format!("prov-apply-{pid}")))
                .cursor_pointer()
                .px(px(14.0))
                .py(px(5.0))
                .rounded(px(16.0))
                .bg(t.card_hover)
                .text_size(px(12.0))
                .font_weight(gpui::FontWeight::MEDIUM)
                .text_color(t.text_secondary)
                .hover(|h| h.bg(t.row_hover).text_color(t.text_primary))
                .on_click(cx.listener(move |ws, _, _, cx| {
                    ws.apply_provider(tool, &pid, cx);
                }))
                .flex()
                .items_center()
                .gap(px(5.0))
                .child(
                    gpui::svg()
                        .data(crate::icons::PLAY_SVG)
                        .size(px(10.5))
                        .text_color(t.text_muted)
                        .flex_none(),
                )
                .child(i.t("启用", "Enable")),
        );
    }

    actions = actions.child(card_icon_btn(
        format!("prov-edit-{pid2}"),
        crate::icons::PENCIL_SVG,
        i.t("编辑供应商", "Edit Provider"),
        false,
        &t,
        cx,
        move |ws, _ev, window, cx| {
            open_provider_dialog(Some(pid2.clone()), tool, ws, cx);
            if let Some(dlg) = &ws.ui.provider_dialog {
                dlg.name.update(cx, |name, cx| {
                    name.focus_handle.focus(window, cx);
                    name.start_blink(cx);
                });
            }
        },
    ));

    actions = actions.child(card_icon_btn(
        format!("prov-test-{pid_test}"),
        crate::icons::ZAP_SVG,
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
        crate::icons::BAR_CHART_SVG,
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
            crate::icons::TERMINAL_SVG,
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
                crate::icons::CHEVRON_UP_SVG,
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
                crate::icons::CHEVRON_DOWN_SVG,
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
            crate::icons::TRASH_SVG,
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
        gpui::rgb(0x3b82f6)
    } else {
        t.card_border
    };
    let bg_color = if p.is_applied {
        if t.is_dark {
            crate::rgba_const(0x121722ff)
        } else {
            crate::rgba_const(0xf0f7ffff)
        }
    } else {
        t.card_bg
    };
    let hover_bg = if t.is_dark {
        if p.is_applied {
            crate::rgba_const(0x151c2aff)
        } else {
            t.card_hover
        }
    } else {
        if p.is_applied {
            crate::rgba_const(0xe6f0feff)
        } else {
            t.card_hover
        }
    };
    let hover_border = if p.is_applied {
        gpui::rgb(0x60a5fa)
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

pub struct BuiltinPromptPreset {
    pub id: &'static str,
    pub name: &'static str,
    pub desc: &'static str,
    pub content: &'static str,
}

pub const BUILTIN_PROMPTS: &[BuiltinPromptPreset] = &[
    BuiltinPromptPreset {
        id: "architect",
        name: "资深架构师",
        desc: "高内聚低耦合系统架构、SOLID 原则与健壮扩展设计",
        content: "# 资深系统架构师工作准则\n\n你是一名拥有 15 年以上大型分布式与系统级软件工程经验的资深架构师。\n\n## 核心原则\n1. **SOLID & DRY 原则**：所有设计必须保证单一职责、高内聚、依赖倒置，消除不必要的重复代码。\n2. **模块化与关注点分离**：严格划分清晰的接口契约与数据模型边界，禁止跨层乱调。\n3. **健壮性与并发安全**：针对边界输入、空指针、并发锁竞争、网络重试和超时进行严密防护。\n4. **演进式架构**：避免过度设计（YAGNI），但在关键抽象层预留可扩展的钩子。\n\n## 输出要求\n- 给出代码前先简明扼要说明设计思路与权衡（Trade-offs）。\n- 交付生产级、类型安全、带关键设计注释的高质量代码。",
    },
    BuiltinPromptPreset {
        id: "reviewer",
        name: "严格代码审查员",
        desc: "深度挖掘边界漏洞、竞态死锁、内存泄露与代码坏味道",
        content: "# 严格代码审查员工作准则\n\n你是一名偏执而严苛的代码审查专家，以零容忍的态度对待潜在漏洞与坏味道。\n\n## 审查维度\n1. **正确性与边界情况**：检查所有可能发生溢出、下标越界、并发竞态、死锁、资源泄露的地方。\n2. **安全性 (OWASP)**：检查敏感信息硬编码、命令注入、未经校验的用户输入与权限漏洞。\n3. **代码异味 (Code Smells)**：指出长函数、巨型类、魔法数字、循环调用与无效抽象。\n4. **性能瓶颈**：指出高频内存分配、不必要的深拷贝、低效算法复杂度。\n\n## 输出规范\n- 采用 `[致命/严重/建议]` 评级结构化列出发现的问题。\n- 为每一个问题提供具体的定位和立即可用的修复后代码示例。",
    },
    BuiltinPromptPreset {
        id: "chinese-dev",
        name: "中文全栈开发专家",
        desc: "母语级中文沟通、思考链路清晰、直接产出高质量完整代码",
        content: "# 中文全栈开发专家\n\n你是一名资深全栈研发工程师，擅长以流畅地ฏ地道的中文进行技术交流与方案落地。\n\n## 工作规范\n1. **中文优先**：全程使用专业、清晰、精炼的简体中文沟通，术语保留业界通用英文。\n2. **思考清晰**：给出方案前，快速分析需求核心与潜在影响范围。\n3. **直接交付完整代码**：拒绝省略或伪代码，提供完整可运行、格式工整、开箱即用的解决方案。\n4. **现代最佳实践**：遵循现代语言规范（如 TypeScript Strict、Rust 2024、Python 3.12+）。",
    },
    BuiltinPromptPreset {
        id: "ponytail",
        name: "极简开发者 (Ponytail)",
        desc: "最懒最快的方案，拒绝过度设计，优先标准库，一行顶五十行",
        content: "# 极简实用主义开发者 (Ponytail Style)\n\n你是一位经验丰富、崇尚“最少代码办成最多事”的资深开发老兵。\n\n## 极简宪章\n1. **YAGNI 原则**：不需要的功能坚决不写，不解决未发生的问题，代码越少 Bug 越少。\n2. **标准库优先**：能用语言标准库和系统原生 API 搞定的，坚决不引入第三方依赖。\n3. **扁平化结构**：拒绝层层无意义的封装、Speculative Factory 和抽象层，直接用最简短直白的方式实现。\n4. **能删则删**：发现多余的脚手架、胶水代码和样板代码，果断重构成短小精炼的实现。",
    },
    BuiltinPromptPreset {
        id: "tdd",
        name: "TDD 单元测试专家",
        desc: "测试驱动开发，全覆盖边缘用例、Mock 隔离与清晰断言",
        content: "# TDD 单元测试专家准则\n\n你是一名严格践行测试驱动开发 (TDD) 的工程专家。\n\n## 测试准则\n1. **红-绿-重构循环**：先根据接口契约写测试，再编写满足测试的最简实现，最后重构。\n2. **边界全覆盖**：覆盖正常流、空值、极值、非法输入、超时重试与异常路径。\n3. **外部依赖 Mock 隔离**：网络、数据库、时钟等外部环境必须通过 Mock/Stub 隔离，确保测试快速且确定。\n4. **清晰的 Given-When-Then 结构**：每个测试用例意图明确，失败提示具备可诊断性。",
    },
];

fn prompts_section(
    tool: ToolId,
    ws: &mut Workspace,
    cx: &mut Context<Workspace>,
) -> gpui::AnyElement {
    let t = ws.theme.clone();
    let i = ws.i18n;
    let prompts = aitoolplus_core::prompt::list(&ws.store.store().tool(tool).prompts);

    let query = ws.ui.prompt_search.read(cx).text().to_lowercase();
    let filtered_prompts: Vec<_> = prompts
        .into_iter()
        .filter(|p| {
            query.is_empty()
                || p.name.to_lowercase().contains(&query)
                || p.content.to_lowercase().contains(&query)
        })
        .collect();

    let adapter = aitoolplus_core::adapters::adapter_for(tool);
    let target_file_str = adapter
        .prompt_file(&ws.paths)
        .and_then(|p| p.file_name().map(|f| f.to_string_lossy().to_string()))
        .unwrap_or_else(|| "Prompt".into());

    let add_label = i.t("新增 Prompt", "Add Prompt");

    // Presets quick-add bar
    let mut presets_bar = div()
        .flex()
        .items_center()
        .gap(px(6.0))
        .flex_wrap()
        .child(
            div()
                .text_size(px(12.0))
                .font_weight(gpui::FontWeight::MEDIUM)
                .text_color(t.text_secondary)
                .child(i.t("快捷预设模板：", "Quick Presets:")),
        );

    for preset in BUILTIN_PROMPTS {
        let preset_name = preset.name;
        let preset_content = preset.content;
        let p_id = preset.id;
        presets_bar = presets_bar.child(button_with_icon_l(
            gpui::SharedString::from(format!("prompt-preset-chip-{p_id}")),
            crate::icons::PLUS_SVG,
            preset_name,
            ButtonVariant::Secondary,
            &t,
            cx,
            move |ws, _, _, cx| {
                let name_str = preset_name.to_string();
                let content_str = preset_content.to_string();
                let name_inp = cx.new(|cx| {
                    let mut inp = TextInput::new("名称", cx);
                    inp.set_text_silent(name_str, cx);
                    inp
                });
                let content_ta = cx.new(|cx| {
                    let mut ta = TextArea::new("Prompt 内容…", cx);
                    ta.set_text_silent(content_str, cx);
                    ta
                });
                ws.ui.prompt_dialog = Some(PromptDialogState {
                    editing_id: None,
                    tool,
                    name: name_inp,
                    content: content_ta,
                });
                cx.notify();
            },
        ));
    }

    let mut section = div()
        .flex()
        .flex_col()
        .gap(px(12.0))
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .child(section_title(
                    &t,
                    i.t("全局 Prompt", "Global Prompts"),
                    Some(i.t(
                        &format!("应用后写入工具的 Prompt 文件（{}）", target_file_str),
                        &format!("Applied prompts are written to {}", target_file_str),
                    )),
                ))
                .child(button_l(
                    "prompt-add",
                    add_label,
                    ButtonVariant::Primary,
                    &t,
                    cx,
                    move |ws, _, window, cx| {
                        open_prompt_dialog(None, tool, ws, cx);
                        if let Some(dlg) = &ws.ui.prompt_dialog {
                            dlg.name.update(cx, |name, cx| {
                                name.focus_handle.focus(window, cx);
                                name.start_blink(cx);
                            });
                        }
                    },
                )),
        )
        .child(presets_bar)
        .child(
            div()
                .w(px(320.0))
                .child(input_container(&t, ws.ui.prompt_search.clone())),
        );

    if filtered_prompts.is_empty() {
        section = section.child(crate::components::empty_state_svg(
            &t,
            crate::icons::FILE_TEXT_SVG,
            if query.is_empty() {
                i.t("还没有 Prompt", "No prompts yet")
            } else {
                i.t("没有找到匹配的 Prompt", "No matching prompts")
            },
            if query.is_empty() {
                i.t(
                    "点击上方快捷预设模板，或新增一条可一键应用的全局 Prompt",
                    "Click a preset above or add a new global prompt",
                )
            } else {
                i.t("尝试更换搜索关键词", "Try a different search term")
            },
        ));
    } else {
        let mut list = div().flex().flex_col().gap(px(6.0));
        for p in &filtered_prompts {
            let pid = p.id.clone();
            let pid2 = p.id.clone();
            let pid3 = p.id.clone();
            let pid4 = p.id.clone();
            let pid5 = p.id.clone();
            let p_content = p.content.clone();
            let p_name = p.name.clone();

            let status_badge = if p.is_applied {
                badge(&t, i.t("应用中", "Applied"), BadgeKind::Success)
            } else {
                badge(&t, i.t("未应用", "Idle"), BadgeKind::Neutral)
            };

            let preview_snippet: String = p
                .content
                .lines()
                .filter(|l| !l.trim().is_empty() && !l.starts_with('#'))
                .take(2)
                .collect::<Vec<_>>()
                .join(" ");
            let preview_clean = if preview_snippet.chars().count() > 80 {
                format!("{}…", preview_snippet.chars().take(80).collect::<String>())
            } else {
                preview_snippet
            };

            list = list.child(
                div()
                    .id(gpui::SharedString::from(format!("prompt-{pid}")))
                    .flex()
                    .w_full()
                    .min_w(px(0.0))
                    .items_center()
                    .justify_between()
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
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(10.0))
                            .flex_1()
                            .min_w(px(0.0))
                            .child(status_badge)
                            .child(badge(
                                &t,
                                gpui::SharedString::from(target_file_str.clone()),
                                BadgeKind::Neutral,
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
                                            .text_size(px(13.5))
                                            .font_weight(gpui::FontWeight::MEDIUM)
                                            .text_color(t.text_primary)
                                            .child(p.name.clone()),
                                    )
                                    .children((!preview_clean.is_empty()).then(|| {
                                        div()
                                            .text_size(px(11.5))
                                            .text_color(t.text_muted)
                                            .child(preview_clean)
                                            .into_any_element()
                                    })),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(4.0))
                            .child(if p.is_applied {
                                div()
                                    .flex()
                                    .items_center()
                                    .px(px(8.0))
                                    .h(px(28.0))
                                    .text_size(px(12.0))
                                    .text_color(t.success)
                                    .child(i.t("生效中", "Active"))
                                    .into_any_element()
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
                            .child(crate::components::icon_button_svg(
                                gpui::SharedString::from(format!("prompt-copy-{pid4}")),
                                crate::icons::COPY_SVG,
                                i.t("复制内容", "Copy Content"),
                                false,
                                &t,
                                cx,
                                {
                                    let content_copy = p_content.clone();
                                    move |ws, _, _, cx| {
                                        cx.write_to_clipboard(gpui::ClipboardItem::new_string(
                                            content_copy.clone(),
                                        ));
                                        let msg = ws
                                            .i18n
                                            .t(
                                                "Prompt 内容已复制到剪贴板",
                                                "prompt copied to clipboard",
                                            )
                                            .to_string();
                                        ws.ui.toast(msg, false);
                                        cx.notify();
                                    }
                                },
                            ))
                            .child(crate::components::icon_button_svg(
                                gpui::SharedString::from(format!("prompt-dup-{pid5}")),
                                crate::icons::SPARKLES_SVG,
                                i.t("创建副本", "Duplicate"),
                                false,
                                &t,
                                cx,
                                {
                                    let dup_name = format!("{} (副本)", p_name);
                                    let dup_content = p_content;
                                    move |ws, _, _, cx| {
                                        let _ = ws.store.update(|store| {
                                            let s = store.tool_mut(tool);
                                            aitoolplus_core::prompt::create(
                                                &mut s.prompts,
                                                &dup_name,
                                                &dup_content,
                                            );
                                        });
                                        ws.persist_store();
                                        let msg = ws
                                            .i18n
                                            .t("已创建 Prompt 副本", "prompt duplicate created")
                                            .to_string();
                                        ws.ui.toast(msg, false);
                                        cx.notify();
                                    }
                                },
                            ))
                            .child(crate::components::icon_button_svg(
                                gpui::SharedString::from(format!("prompt-edit-{pid2}")),
                                crate::icons::PENCIL_SVG,
                                i.t("编辑", "Edit"),
                                false,
                                &t,
                                cx,
                                move |ws, _, window, cx| {
                                    let id = pid2.clone();
                                    open_prompt_dialog(Some(id), tool, ws, cx);
                                    if let Some(dlg) = &ws.ui.prompt_dialog {
                                        dlg.name.update(cx, |name, cx| {
                                            name.focus_handle.focus(window, cx);
                                            name.start_blink(cx);
                                        });
                                    }
                                },
                            ))
                            .child(crate::components::icon_button_svg(
                                gpui::SharedString::from(format!("prompt-del-{pid3}")),
                                crate::icons::TRASH_SVG,
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
            if let Some(parent) = file.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            if let Err(e) = std::fs::write(&file, &prompt.content) {
                let msg = format!("write prompt failed: {e}");
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

fn reveal_in_explorer(path: &std::path::Path) {
    #[cfg(target_os = "windows")]
    {
        let _ = std::process::Command::new("explorer")
            .arg(format!("/select,{}", path.display()))
            .spawn();
    }
    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new("open")
            .arg("-R")
            .arg(path)
            .spawn();
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        if let Some(parent) = path.parent() {
            let _ = std::process::Command::new("xdg-open").arg(parent).spawn();
        }
    }
}

fn open_in_system_editor(path: &std::path::Path) {
    #[cfg(target_os = "windows")]
    {
        let _ = std::process::Command::new("rundll32")
            .args(["url.dll,FileProtocolHandler", &path.display().to_string()])
            .spawn();
    }
    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new("open").arg(path).spawn();
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        let _ = std::process::Command::new("xdg-open").arg(path).spawn();
    }
}

pub fn render_runtime_edit_dialog(
    path: std::path::PathBuf,
    editor: gpui::Entity<TextArea>,
    ws: &mut Workspace,
    cx: &mut Context<Workspace>,
) -> gpui::AnyElement {
    let t = ws.theme.clone();
    let i = ws.i18n;
    let file_name = path
        .file_name()
        .map(|f| f.to_string_lossy().to_string())
        .unwrap_or_else(|| "config".into());

    let title = i.t(
        &format!("编辑配置文件 - {file_name}"),
        &format!("Edit Config - {file_name}"),
    );

    let body = div()
        .flex()
        .flex_col()
        .gap(px(10.0))
        .w(px(720.0))
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(8.0))
                .text_size(px(11.5))
                .text_color(t.text_muted)
                .child(crate::icons::svg_icon(
                    crate::icons::FILE_TEXT_SVG,
                    px(14.0),
                    t.text_muted,
                ))
                .child(path.display().to_string()),
        )
        .child(crate::components::textarea_container(&t, editor.clone()))
        .child(
            div()
                .flex()
                .justify_end()
                .gap(px(8.0))
                .pt(px(4.0))
                .child(button_l(
                    "runtime-edit-cancel",
                    i.t("取消", "Cancel"),
                    ButtonVariant::Secondary,
                    &t,
                    cx,
                    |ws, _, _, cx| {
                        ws.ui.runtime_edit_dialog = None;
                        cx.notify();
                    },
                ))
                .child(button_l(
                    "runtime-edit-save",
                    i.t("保存更改", "Save Changes"),
                    ButtonVariant::Primary,
                    &t,
                    cx,
                    {
                        let target_path = path.clone();
                        move |ws, _, _, cx| {
                            let text = editor.update(cx, |ed, _| ed.text().to_string());
                            if let Some(parent) = target_path.parent() {
                                let _ = std::fs::create_dir_all(parent);
                            }
                            match std::fs::write(&target_path, text) {
                                Ok(()) => {
                                    ws.ui.runtime_files_cache = None;
                                    ws.ui.runtime_edit_dialog = None;
                                    let msg = ws
                                        .i18n
                                        .t(
                                            "配置文件已成功保存",
                                            "config file saved successfully",
                                        )
                                        .to_string();
                                    ws.ui.toast(msg, false);
                                }
                                Err(e) => {
                                    let msg = format!("save error: {e}");
                                    ws.ui.toast(msg, true);
                                }
                            }
                            cx.notify();
                        }
                    },
                )),
        );

    modal_scaffold_custom(&t, &title, px(720.0), body.into_any_element(), cx, |ws, _, _, cx| {
        ws.ui.runtime_edit_dialog = None;
        cx.notify();
    })
}

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
                    String::new()
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
                    "查看、在外部编辑器打开或在线直接编辑工具的真实配置文件",
                    "Inspect, reveal, open in editor, or edit real tool config files",
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
        section = section.child(crate::components::empty_state_svg(
            &t,
            crate::icons::FILE_TEXT_SVG,
            i.t("该工具没有已知配置文件", "No known config files"),
            "",
        ));
        return section.into_any_element();
    }

    for (idx, (label, path, exists, content)) in files.into_iter().enumerate() {
        let truncated: String = content.chars().take(4000).collect();
        let overflow = content.chars().count() > 4000;
        let line_count = if exists {
            content.lines().count()
        } else {
            0
        };

        let path_clone1 = path.clone();
        let path_clone2 = path.clone();
        let path_clone4 = path.clone();
        let path_display = path.display().to_string();
        let full_content = content.clone();

        section = section.child(
            div()
                .flex()
                .flex_col()
                .gap(px(8.0))
                .p(px(12.0))
                .rounded(px(8.0))
                .bg(t.card_bg)
                .border_1()
                .border_color(if exists { t.card_border } else { t.danger })
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .gap(px(8.0))
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
                                        .text_size(px(13.0))
                                        .font_weight(gpui::FontWeight::SEMIBOLD)
                                        .text_color(t.text_primary)
                                        .child(label.clone()),
                                )
                                .child(
                                    div()
                                        .text_size(px(11.0))
                                        .text_color(t.text_muted)
                                        .child(path_display.clone()),
                                ),
                        )
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(4.0))
                                .child(button_l(
                                    gpui::SharedString::from(format!("runtime-reveal-{idx}")),
                                    i.t("定位", "Reveal"),
                                    ButtonVariant::Secondary,
                                    &t,
                                    cx,
                                    move |_, _, _, _| {
                                        reveal_in_explorer(&path_clone1);
                                    },
                                ))
                                .child(button_l(
                                    gpui::SharedString::from(format!("runtime-open-{idx}")),
                                    i.t("打开", "Open"),
                                    ButtonVariant::Secondary,
                                    &t,
                                    cx,
                                    move |_, _, _, _| {
                                        open_in_system_editor(&path_clone2);
                                    },
                                ))
                                .child(button_l(
                                    gpui::SharedString::from(format!("runtime-copy-{idx}")),
                                    i.t("复制路径", "Copy Path"),
                                    ButtonVariant::Secondary,
                                    &t,
                                    cx,
                                    {
                                        let p_str = path_display.clone();
                                        move |ws, _, _, cx| {
                                            cx.write_to_clipboard(gpui::ClipboardItem::new_string(
                                                p_str.clone(),
                                            ));
                                            let msg = ws
                                                .i18n
                                                .t("路径已复制", "path copied")
                                                .to_string();
                                            ws.ui.toast(msg, false);
                                            cx.notify();
                                        }
                                    },
                                ))
                                .child(button_l(
                                    gpui::SharedString::from(format!("runtime-edit-{idx}")),
                                    if exists {
                                        i.t("在线编辑", "Edit")
                                    } else {
                                        i.t("创建文件", "Create")
                                    },
                                    if exists {
                                        ButtonVariant::Primary
                                    } else {
                                        ButtonVariant::Secondary
                                    },
                                    &t,
                                    cx,
                                    move |ws, _, window, cx| {
                                        let init_text = full_content.clone();
                                        let ed = cx.new(|cx| {
                                            let mut ta = TextArea::new("", cx);
                                            ta.set_text_silent(init_text, cx);
                                            ta.set_max_lines(26, cx);
                                            ta.focus_handle.focus(window, cx);
                                            ta.start_blink(cx);
                                            ta
                                        });
                                        ws.ui.runtime_edit_dialog =
                                            Some((path_clone4.clone(), ed));
                                        cx.notify();
                                    },
                                )),
                        ),
                )
                .child(if exists {
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(4.0))
                        .p(px(10.0))
                        .rounded(px(6.0))
                        .bg(t.input_bg)
                        .border_1()
                        .border_color(t.input_border)
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .justify_between()
                                .text_size(px(10.5))
                                .text_color(t.text_muted)
                                .child(format!("{line_count} 行 · {} 字符", content.chars().count()))
                                .child(if overflow { "预览已截断至前 4000 字" } else { "完整预览" }),
                        )
                        .child(
                            div()
                                .text_size(px(11.5))
                                .text_color(t.text_secondary)
                                .child(truncated),
                        )
                        .into_any_element()
                } else {
                    div()
                        .p(px(10.0))
                        .rounded(px(6.0))
                        .bg(t.input_bg)
                        .text_size(px(11.5))
                        .text_color(t.text_muted)
                        .child(i.t(
                            "文件当前不存在，点击右上角“创建文件”或“在线编辑”可直接初始化保存。",
                            "File does not exist yet. Click Create or Edit above to initialize it.",
                        ))
                        .into_any_element()
                }),
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
                provider_row = provider_row.child(button_l(
                    gpui::SharedString::from(format!("pi-ms-provider-{key}")),
                    key.clone(),
                    if is_on {
                        ButtonVariant::Primary
                    } else {
                        ButtonVariant::Secondary
                    },
                    &t,
                    cx,
                    move |ws, _ev, _w, cx| {
                        if ws.ui.pi_ms_provider.as_deref() == Some(&key_owned) {
                            ws.ui.pi_ms_provider = None;
                        } else {
                            ws.ui.pi_ms_provider = Some(key_owned.clone());
                        }
                        ws.ui.pi_ms_model = None;
                        cx.notify();
                    },
                ));
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
                    model_row = model_row.child(button_l(
                        gpui::SharedString::from(format!("pi-ms-model-{model}")),
                        model.clone(),
                        if is_on {
                            ButtonVariant::Primary
                        } else {
                            ButtonVariant::Secondary
                        },
                        &t,
                        cx,
                        move |ws, _ev, _w, cx| {
                            ws.ui.pi_ms_provider = Some(p_owned.clone());
                            ws.ui.pi_ms_model = Some(m_owned.clone());
                            cx.notify();
                        },
                    ));
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
                think_row = think_row.child(button_l(
                    gpui::SharedString::from(format!("pi-ms-think-{level}")),
                    level,
                    if is_on {
                        ButtonVariant::Primary
                    } else {
                        ButtonVariant::Secondary
                    },
                    &t,
                    cx,
                    move |ws, _ev, _w, cx| {
                        if ws.ui.pi_ms_thinking.as_deref() == Some(&l_owned) {
                            ws.ui.pi_ms_thinking = None;
                        } else {
                            ws.ui.pi_ms_thinking = Some(l_owned.clone());
                        }
                        cx.notify();
                    },
                ));
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
                section = section.child(crate::components::empty_state_svg(
                    &t,
                    crate::icons::PACKAGE_SVG,
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
                            .child(crate::components::icon_button_svg(
                                gpui::SharedString::from(format!("ext-upd-{}", ext.id)),
                                crate::icons::REFRESH_SVG,
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
                            .child(crate::components::icon_button_svg(
                                gpui::SharedString::from(format!("ext-del-{}", ext.id)),
                                crate::icons::TRASH_SVG,
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
                                            .flex()
                                            .items_center()
                                            .gap(px(5.0))
                                            .children(protected.then(|| {
                                                gpui::svg()
                                                    .data(crate::icons::LOCK_SVG)
                                                    .size(px(12.0))
                                                    .text_color(t.text_muted)
                                                    .flex_none()
                                            }))
                                            .child(
                                                div()
                                                    .text_size(px(13.0))
                                                    .font_weight(gpui::FontWeight::MEDIUM)
                                                    .text_color(t.text_primary)
                                                    .child(ext.source.clone()),
                                            ),
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
                .child(crate::components::empty_state_svg(
                    &t,
                    crate::icons::ALERT_SVG,
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
                .child(crate::components::empty_state_svg(
                    &t,
                    crate::icons::ALERT_SVG,
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

fn open_in_browser(url: &str) {
    let trimmed = url.trim();
    if trimmed.is_empty() {
        return;
    }
    #[cfg(target_os = "windows")]
    {
        let _ = std::process::Command::new("rundll32")
            .args(["url.dll,FileProtocolHandler", trimmed])
            .spawn();
    }
    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new("open").arg(trimmed).spawn();
    }
    #[cfg(target_os = "linux")]
    {
        let _ = std::process::Command::new("xdg-open").arg(trimmed).spawn();
    }
}

fn strip_1m(model: &str) -> (String, bool) {
    if let Some(stripped) = model.strip_suffix("[1M]") {
        (stripped.trim().to_string(), true)
    } else {
        (model.trim().to_string(), false)
    }
}

struct ExtractedProviderConfig {
    base_url: String,
    api_key: String,
    api_format: String,
    model: String,
    sonnet_model: String,
    sonnet_name: String,
    opus_model: String,
    opus_name: String,
    haiku_model: String,
    haiku_name: String,
    fable_model: String,
    fable_name: String,
    subagent_model: String,
    sonnet_1m: bool,
    opus_1m: bool,
    haiku_1m: bool,
    fable_1m: bool,
    subagent_1m: bool,
    codex_wire_api: String,
    codex_reasoning_effort: String,
    custom_headers: String,
}

fn extract_provider_config(tool: ToolId, raw_json: &str) -> ExtractedProviderConfig {
    let val: Value = serde_json::from_str(raw_json).unwrap_or(Value::Object(Default::default()));
    let mut cfg = ExtractedProviderConfig {
        base_url: String::new(),
        api_key: String::new(),
        api_format: "anthropic".to_string(),
        model: String::new(),
        sonnet_model: String::new(),
        sonnet_name: String::new(),
        opus_model: String::new(),
        opus_name: String::new(),
        haiku_model: String::new(),
        haiku_name: String::new(),
        fable_model: String::new(),
        fable_name: String::new(),
        subagent_model: String::new(),
        sonnet_1m: false,
        opus_1m: false,
        haiku_1m: false,
        fable_1m: false,
        subagent_1m: false,
        codex_wire_api: "responses".to_string(),
        codex_reasoning_effort: "default".to_string(),
        custom_headers: String::new(),
    };

    match tool {
        ToolId::ClaudeCode | ToolId::ClaudeDesktop => {
            if let Some(env) = val.get("env").and_then(Value::as_object) {
                if let Some(v) = env.get("ANTHROPIC_BASE_URL").and_then(Value::as_str) {
                    cfg.base_url = v.to_string();
                }
                if let Some(v) = env.get("ANTHROPIC_AUTH_TOKEN").and_then(Value::as_str) {
                    cfg.api_key = v.to_string();
                } else if let Some(v) = env.get("ANTHROPIC_API_KEY").and_then(Value::as_str) {
                    cfg.api_key = v.to_string();
                }
                if let Some(v) = env.get("ANTHROPIC_MODEL").and_then(Value::as_str) {
                    cfg.model = v.to_string();
                }
                if let Some(v) = env.get("ANTHROPIC_DEFAULT_SONNET_MODEL").and_then(Value::as_str) {
                    let (m, is_1m) = strip_1m(v);
                    cfg.sonnet_model = m;
                    cfg.sonnet_1m = is_1m;
                }
                if let Some(v) = env.get("ANTHROPIC_DEFAULT_SONNET_MODEL_NAME").and_then(Value::as_str) {
                    cfg.sonnet_name = v.to_string();
                }
                if let Some(v) = env.get("ANTHROPIC_DEFAULT_OPUS_MODEL").and_then(Value::as_str) {
                    let (m, is_1m) = strip_1m(v);
                    cfg.opus_model = m;
                    cfg.opus_1m = is_1m;
                }
                if let Some(v) = env.get("ANTHROPIC_DEFAULT_OPUS_MODEL_NAME").and_then(Value::as_str) {
                    cfg.opus_name = v.to_string();
                }
                if let Some(v) = env.get("ANTHROPIC_DEFAULT_HAIKU_MODEL").and_then(Value::as_str) {
                    let (m, is_1m) = strip_1m(v);
                    cfg.haiku_model = m;
                    cfg.haiku_1m = is_1m;
                }
                if let Some(v) = env.get("ANTHROPIC_DEFAULT_HAIKU_MODEL_NAME").and_then(Value::as_str) {
                    cfg.haiku_name = v.to_string();
                }
                if let Some(v) = env.get("ANTHROPIC_DEFAULT_FABLE_MODEL").and_then(Value::as_str) {
                    let (m, is_1m) = strip_1m(v);
                    cfg.fable_model = m;
                    cfg.fable_1m = is_1m;
                }
                if let Some(v) = env.get("ANTHROPIC_DEFAULT_FABLE_MODEL_NAME").and_then(Value::as_str) {
                    cfg.fable_name = v.to_string();
                }
                if let Some(v) = env.get("CLAUDE_CODE_SUBAGENT_MODEL").and_then(Value::as_str) {
                    let (m, is_1m) = strip_1m(v);
                    cfg.subagent_model = m;
                    cfg.subagent_1m = is_1m;
                }
                if let Some(v) = env.get("API_FORMAT").and_then(Value::as_str) {
                    cfg.api_format = v.to_string();
                }
                if let Some(v) = env.get("CUSTOM_HEADERS").and_then(Value::as_str) {
                    cfg.custom_headers = v.to_string();
                }
            }
        }
        ToolId::Codex => {
            let toml_text = val.get("toml").and_then(Value::as_str).unwrap_or(raw_json);
            if let Ok(doc) = toml_text.parse::<toml_edit::DocumentMut>() {
                if let Some(m) = doc.get("model").and_then(|v| v.as_str()) {
                    cfg.model = m.to_string();
                }
                if let Some(m) = doc.get("model_reasoning_effort").and_then(|v| v.as_str()) {
                    cfg.codex_reasoning_effort = m.to_string();
                }
                if let Some(m) = doc.get("wire_api").and_then(|v| v.as_str()) {
                    cfg.codex_wire_api = m.to_string();
                }
                if let Some(mp) = doc.get("model_providers").and_then(|v| v.as_table()) {
                    for (_k, tbl) in mp.iter() {
                        if let Some(tbl) = tbl.as_table() {
                            if let Some(u) = tbl.get("base_url").and_then(|v| v.as_str()) {
                                cfg.base_url = u.to_string();
                            }
                            if let Some(k) = tbl.get("api_key").and_then(|v| v.as_str()) {
                                cfg.api_key = k.to_string();
                            }
                        }
                    }
                }
            }
        }
        ToolId::GeminiCli => {
            if let Some(env) = val.get("env").and_then(Value::as_object) {
                if let Some(v) = env
                    .get("GEMINI_API_KEY")
                    .or_else(|| env.get("GOOGLE_API_KEY"))
                    .and_then(Value::as_str)
                {
                    cfg.api_key = v.to_string();
                }
                if let Some(v) = env
                    .get("GOOGLE_GEMINI_BASE_URL")
                    .or_else(|| env.get("GEMINI_BASE_URL"))
                    .and_then(Value::as_str)
                {
                    cfg.base_url = v.to_string();
                }
                if let Some(v) = env.get("GEMINI_MODEL").and_then(Value::as_str) {
                    cfg.model = v.to_string();
                }
            }
        }
        _ => {
            if let Some(v) = val
                .get("baseUrl")
                .or_else(|| val.get("base_url"))
                .and_then(Value::as_str)
            {
                cfg.base_url = v.to_string();
            } else if let Some(v) = val
                .get("options")
                .and_then(|o| o.get("baseURL"))
                .and_then(Value::as_str)
            {
                cfg.base_url = v.to_string();
            }
            if let Some(v) = val
                .get("apiKey")
                .or_else(|| val.get("api_key"))
                .and_then(Value::as_str)
            {
                cfg.api_key = v.to_string();
            } else if let Some(v) = val
                .get("options")
                .and_then(|o| o.get("apiKey"))
                .and_then(Value::as_str)
            {
                cfg.api_key = v.to_string();
            }
            if let Some(v) = val.get("model").and_then(Value::as_str) {
                cfg.model = v.to_string();
            }
        }
    }
    cfg
}

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

    let raw_config = existing
        .as_ref()
        .map(|p| p.settings_config.clone())
        .unwrap_or_else(|| {
            serde_json::to_string_pretty(&aitoolplus_core::providers::default_settings_for(tool))
                .unwrap_or_default()
        });

    let mut extracted = extract_provider_config(tool, &raw_config);
    if existing.is_none() {
        extracted.base_url = String::new();
        extracted.api_key = String::new();
    }

    let name = cx.new(|cx| {
        let mut input = TextInput::new(i.t("名称", "Name"), cx);
        if let Some(p) = &existing {
            input.set_text_silent(p.name.clone(), cx);
        }
        input
    });

    let base_url = cx.new(|cx| {
        let mut input = TextInput::new(
            i.t(
                "https://api.anthropic.com 或第三方中转代理",
                "https://api.anthropic.com or proxy",
            ),
            cx,
        );
        input.set_text_silent(extracted.base_url, cx);
        input
    });

    let api_key = cx.new(|cx| {
        let mut input = TextInput::new(i.t("API Key / 访问密钥", "API Key / Token"), cx);
        input.set_secret(true, cx);
        input.set_text_silent(extracted.api_key, cx);
        input
    });

    let model = cx.new(|cx| {
        let mut input = TextInput::new("", cx);
        input.set_text_silent(extracted.model, cx);
        input
    });

    let sonnet_model = cx.new(|cx| {
        let mut input = TextInput::new("", cx);
        input.set_text_silent(extracted.sonnet_model, cx);
        input
    });

    let sonnet_name = cx.new(|cx| {
        let mut input = TextInput::new(
            i.t("例如 DeepSeek V4 Pro", "e.g. DeepSeek V4 Pro"),
            cx,
        );
        input.set_text_silent(extracted.sonnet_name, cx);
        input
    });

    let opus_model = cx.new(|cx| {
        let mut input = TextInput::new("", cx);
        input.set_text_silent(extracted.opus_model, cx);
        input
    });

    let opus_name = cx.new(|cx| {
        let mut input = TextInput::new(
            i.t("例如 Claude 3.7 Opus", "e.g. Claude 3.7 Opus"),
            cx,
        );
        input.set_text_silent(extracted.opus_name, cx);
        input
    });

    let haiku_model = cx.new(|cx| {
        let mut input = TextInput::new("", cx);
        input.set_text_silent(extracted.haiku_model, cx);
        input
    });

    let haiku_name = cx.new(|cx| {
        let mut input = TextInput::new(
            i.t("例如 Claude 3.5 Haiku", "e.g. Claude 3.5 Haiku"),
            cx,
        );
        input.set_text_silent(extracted.haiku_name, cx);
        input
    });

    let fable_model = cx.new(|cx| {
        let mut input = TextInput::new("", cx);
        input.set_text_silent(extracted.fable_model, cx);
        input
    });

    let fable_name = cx.new(|cx| {
        let mut input = TextInput::new(
            i.t("例如 Claude 3.5 Fable", "e.g. Claude 3.5 Fable"),
            cx,
        );
        input.set_text_silent(extracted.fable_name, cx);
        input
    });

    let subagent_model = cx.new(|cx| {
        let mut input = TextInput::new("", cx);
        input.set_text_silent(extracted.subagent_model, cx);
        input
    });

    let custom_headers = cx.new(|cx| {
        let mut input = TextInput::new(
            i.t(
                "自定义请求头（例如 X-Custom-Header: value）",
                "Custom headers (e.g. X-Custom-Header: value)",
            ),
            cx,
        );
        input.set_text_silent(extracted.custom_headers, cx);
        input
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

    let settings = cx.new(|cx| {
        let mut ta = TextArea::new("{}", cx);
        ta.set_text_silent(&raw_config, cx);
        ta
    });

    let category = existing
        .as_ref()
        .map(|p| p.category.clone())
        .unwrap_or_else(|| "custom".to_string());

    let pi_key_val = if let Some(p) = &existing {
        let v: Value = serde_json::from_str(&raw_config).unwrap_or(Value::Object(Default::default()));
        if let Some(pk) = v.get("_providerKey").and_then(Value::as_str) {
            pk.to_string()
        } else if let Some(stripped) = p.id.strip_prefix("pi:").or_else(|| p.id.strip_prefix("omp:")) {
            stripped.to_string()
        } else {
            p.id.clone()
        }
    } else {
        String::new()
    };
    let pi_provider_key = cx.new(|cx| {
        let mut input = TextInput::new(i.t("Provider Key (如 kimi / deepseek)", "Provider Key (e.g. kimi)"), cx);
        input.set_text_silent(pi_key_val, cx);
        input
    });

    let raw_val: Value = serde_json::from_str(&raw_config).unwrap_or(Value::Object(Default::default()));
    let pi_api_format = raw_val
        .get("api")
        .and_then(Value::as_str)
        .unwrap_or("openai-completions")
        .to_string();

    let mut pi_models: Vec<crate::pages::PiModelDraft> = Vec::new();
    if let Some(models_arr) = raw_val.get("models").and_then(Value::as_array) {
        for (idx, item) in models_arr.iter().enumerate() {
            let m_id = item.get("id").and_then(Value::as_str).unwrap_or_default().to_string();
            let m_name = item.get("name").and_then(Value::as_str).unwrap_or_default().to_string();
            let reasoning = item.get("reasoning").and_then(Value::as_bool).unwrap_or(false);
            let image_input = item
                .get("input")
                .and_then(Value::as_array)
                .map(|arr| arr.iter().any(|v| v.as_str() == Some("image")))
                .unwrap_or(false);
            let cw_str = item.get("contextWindow").map(|v| v.to_string()).unwrap_or_else(|| "128000".into());
            let mt_str = item.get("maxTokens").map(|v| v.to_string()).unwrap_or_else(|| "16384".into());

            let id_ent = cx.new(|cx| {
                let mut inp = TextInput::new(i.t("模型 ID，如 deepseek-chat", "Model ID"), cx);
                inp.set_text_silent(m_id, cx);
                inp
            });
            let name_ent = cx.new(|cx| {
                let mut inp = TextInput::new(i.t("显示名称，如 DeepSeek-V3", "Display name"), cx);
                inp.set_text_silent(m_name, cx);
                inp
            });
            let cw_ent = cx.new(|cx| {
                let mut inp = TextInput::new(i.t("上下文窗口，如 128000", "Context window"), cx);
                inp.set_text_silent(cw_str, cx);
                inp
            });
            let mt_ent = cx.new(|cx| {
                let mut inp = TextInput::new(i.t("最大输出，如 16384", "Max tokens"), cx);
                inp.set_text_silent(mt_str, cx);
                inp
            });
            pi_models.push(crate::pages::PiModelDraft {
                key: format!("pi_model_{idx}"),
                id: id_ent,
                name: name_ent,
                reasoning,
                image_input,
                context_window: cw_ent,
                max_tokens: mt_ent,
                is_expanded: false,
            });
        }
    }
    if pi_models.is_empty() && matches!(tool, ToolId::Pi | ToolId::OhMyPi) {
        let id_ent = cx.new(|cx| TextInput::new(i.t("模型 ID，如 deepseek-chat", "Model ID"), cx));
        let name_ent = cx.new(|cx| TextInput::new(i.t("显示名称，如 DeepSeek-V3", "Display name"), cx));
        let cw_ent = cx.new(|cx| {
            let mut inp = TextInput::new(i.t("上下文窗口，如 128000", "Context window"), cx);
            inp.set_text_silent("128000", cx);
            inp
        });
        let mt_ent = cx.new(|cx| {
            let mut inp = TextInput::new(i.t("最大输出，如 16384", "Max tokens"), cx);
            inp.set_text_silent("16384", cx);
            inp
        });
        pi_models.push(crate::pages::PiModelDraft {
            key: "pi_model_0".to_string(),
            id: id_ent,
            name: name_ent,
            reasoning: false,
            image_input: false,
            context_window: cw_ent,
            max_tokens: mt_ent,
            is_expanded: false,
        });
    }

    let model_search = cx.new(|cx| {
        TextInput::new(
            i.t(
                "搜索模型名称或厂商 (模糊过滤)...",
                "Search models by ID or provider...",
            ),
            cx,
        )
    });

    let meta = existing.as_ref().map(|p| p.parsed_meta()).unwrap_or_default();

    let custom_user_agent = cx.new(|cx| {
        let mut input = TextInput::new(
            i.t("例如 claude-cli/2.1.237 (external, cli)", "e.g. claude-cli/2.1.237"),
            cx,
        );
        if let Some(ua) = &meta.custom_user_agent {
            input.set_text_silent(ua.clone(), cx);
        }
        input
    });

    let mut custom_headers_list = Vec::new();
    if let Some(headers) = &meta.custom_headers {
        for h in headers {
            let key = cx.new(|cx| {
                let mut input = TextInput::new(i.t("Header 名称 (如 X-Title)", "Header Name"), cx);
                input.set_text_silent(h.name.clone(), cx);
                input
            });
            let value = cx.new(|cx| {
                let mut input = TextInput::new(i.t("Header 对应值", "Header Value"), cx);
                input.set_text_silent(h.value.clone(), cx);
                input
            });
            custom_headers_list.push(crate::pages::CustomHeaderDraft { key, value });
        }
    }

    let billing_enabled = meta.billing_enabled.unwrap_or_else(|| {
        meta.cost_multiplier.is_some()
            || (meta.pricing_model_source.as_deref().unwrap_or("inherit") != "inherit")
    });
    let cost_multiplier = cx.new(|cx| {
        let mut input = TextInput::new(i.t("1.0 (留空默认为 1.0)", "1.0 (default)"), cx);
        if let Some(cm) = &meta.cost_multiplier {
            input.set_text_silent(cm.clone(), cx);
        }
        input
    });
    let pricing_model_source = meta.pricing_model_source.unwrap_or_else(|| "inherit".to_string());

    let mut model_rewrites = Vec::new();
    if let Some(rewrites) = &meta.model_rewrites {
        for r in rewrites {
            let from = cx.new(|cx| {
                let mut input = TextInput::new(i.t("请求模型 (From)", "Request Model"), cx);
                input.set_text_silent(r.from.clone(), cx);
                input
            });
            let to = cx.new(|cx| {
                let mut input = TextInput::new(i.t("转发模型 (To)", "Forward Model"), cx);
                input.set_text_silent(r.to.clone(), cx);
                input
            });
            model_rewrites.push(crate::pages::ModelRewriteDraft { from, to });
        }
    }

    ws.ui.provider_dialog = Some(ProviderDialogState {
        editing_id,
        tool,
        active_tab: crate::pages::ProviderDialogTab::Connection,
        name,
        category,
        base_url,
        api_key,
        show_api_key: false,
        api_format: extracted.api_format,
        model,
        sonnet_model,
        sonnet_name,
        opus_model,
        opus_name,
        haiku_model,
        haiku_name,
        fable_model,
        fable_name,
        subagent_model,
        sonnet_1m: extracted.sonnet_1m,
        opus_1m: extracted.opus_1m,
        haiku_1m: extracted.haiku_1m,
        fable_1m: extracted.fable_1m,
        subagent_1m: extracted.subagent_1m,
        pi_provider_key,
        pi_api_format,
        pi_models,
        codex_wire_api: extracted.codex_wire_api,
        codex_reasoning_effort: extracted.codex_reasoning_effort,
        notes,
        website,
        preset_index: None,
        advanced_expanded: false,
        custom_user_agent,
        custom_headers_list,
        billing_enabled,
        cost_multiplier,
        pricing_model_source,
        model_rewrites,
        custom_headers,
        settings,
        fetched_models: Vec::new(),
        is_fetching_models: false,
        fetch_error: None,
        active_model_dropdown: None,
        model_search,
        model_bounds: std::collections::BTreeMap::new(),
    });
    cx.notify();
}

pub fn fetch_upstream_models_for_dialog(ws: &mut Workspace, cx: &mut Context<Workspace>) {
    let Some(dialog) = ws.ui.provider_dialog.as_mut() else { return; };
    let base_url = dialog.base_url.read(cx).text().trim().to_string();
    let api_key = dialog.api_key.read(cx).text().trim().to_string();
    let api_format = dialog.api_format.clone();
    let custom_headers_raw = dialog.custom_headers.read(cx).text().trim().to_string();

    if base_url.is_empty() {
        let msg = ws.i18n.t("请先填写 Base URL 接口地址", "Please enter Base URL first").to_string();
        ws.ui.toast(msg, true);
        cx.notify();
        return;
    }

    dialog.is_fetching_models = true;
    dialog.fetch_error = None;
    dialog.active_model_dropdown = None;
    cx.notify();

    let mut custom_headers = aitoolplus_core::api_hub::parse_custom_headers(&custom_headers_raw);
    let ua = dialog.custom_user_agent.read(cx).text().trim().to_string();
    if !ua.is_empty() {
        custom_headers.insert("User-Agent".to_string(), ua);
    }
    for draft in &dialog.custom_headers_list {
        let k = draft.key.read(cx).text().trim().to_string();
        let v = draft.value.read(cx).text().trim().to_string();
        if !k.is_empty() && !v.is_empty() {
            custom_headers.insert(k, v);
        }
    }
    let weak = cx.entity().downgrade();

    cx.spawn(async move |_this, cx| {
        let res = cx.background_spawn(async move {
            aitoolplus_core::api_hub::fetch_models_advanced(
                &base_url,
                &api_key,
                Some(&api_format),
                if custom_headers.is_empty() { None } else { Some(&custom_headers) },
                None,
            )
        }).await;

        let _ = weak.update(cx, |ws: &mut Workspace, cx| {
            let i = ws.i18n;
            if let Some(d) = ws.ui.provider_dialog.as_mut() {
                d.is_fetching_models = false;
                match res {
                    Ok(fetch_res) => {
                        let count = fetch_res.models.len();
                        d.fetched_models = fetch_res.models;
                        d.active_model_dropdown = None;
                        d.fetch_error = None;
                        let msg = i.t(
                            &format!("成功获取到 {count} 个可用模型，可点击下拉按钮选择"),
                            &format!("Successfully fetched {count} models, click dropdown to select"),
                        ).to_string();
                        ws.ui.toast(msg, false);
                    }
                    Err(err) => {
                        d.fetched_models.clear();
                        d.active_model_dropdown = None;
                        let err_msg = match err {
                            aitoolplus_core::api_hub::ModelsFetchError::Auth => {
                                i.t("身份认证失败 (401/403)，请检查 API Key 是否正确", "Authentication failed (401/403), check your API key").to_string()
                            }
                            aitoolplus_core::api_hub::ModelsFetchError::Network(s) => {
                                format!("{}: {s}", i.t("网络连接错误", "Network error"))
                            }
                            aitoolplus_core::api_hub::ModelsFetchError::Parse(s) => {
                                format!("{}: {s}", i.t("响应解析失败", "Parse error"))
                            }
                            aitoolplus_core::api_hub::ModelsFetchError::Unsupported(s) => {
                                format!("{}: {s}", i.t("未返回可用模型", "No models returned"))
                            }
                        };
                        d.fetch_error = Some(err_msg.clone());
                        ws.ui.toast(format!("{}: {err_msg}", i.t("获取模型失败", "Fetch models failed")), true);
                    }
                }
            }
            cx.notify();
        });
    })
    .detach();
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
        active_tab,
        name,
        category,
        base_url,
        api_key,
        show_api_key,
        api_format: _,
        model,
        sonnet_model,
        sonnet_name,
        opus_model,
        opus_name,
        haiku_model,
        haiku_name,
        fable_model,
        fable_name,
        subagent_model,
        sonnet_1m,
        opus_1m,
        haiku_1m,
        fable_1m,
        subagent_1m,
        pi_provider_key,
        pi_api_format,
        pi_models,
        codex_wire_api,
        codex_reasoning_effort,
        notes,
        website,
        preset_index,
        advanced_expanded: _,
        custom_user_agent,
        custom_headers_list,
        billing_enabled,
        cost_multiplier,
        pricing_model_source,
        model_rewrites,
        custom_headers: _,
        settings,
        fetched_models,
        is_fetching_models,
        fetch_error,
        active_model_dropdown,
        model_search,
        model_bounds,
    } = state;

    let title = if editing_id.is_some() {
        format!("{} · {}", i.t("编辑供应商", "Edit Provider"), tool.name_zh())
    } else {
        format!("{} · {}", i.t("新增供应商", "Add Provider"), tool.name_zh())
    };

    let field_label = |label: gpui::SharedString| -> gpui::AnyElement {
        div()
            .text_size(px(12.0))
            .font_weight(gpui::FontWeight::MEDIUM)
            .text_color(t.text_secondary)
            .child(label)
            .into_any_element()
    };

    let section_card = |title_text: gpui::SharedString| {
        div()
            .p(px(12.0))
            .rounded(px(8.0))
            .bg(t.sidebar_bg)
            .border_1()
            .border_color(t.card_border)
            .flex()
            .flex_col()
            .gap(px(10.0))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_size(px(12.5))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(t.text_primary)
                            .child(title_text),
                    ),
            )
    };

    let presets = aitoolplus_core::presets::presets_for(tool);
    let preset_names: Vec<gpui::SharedString> =
        std::iter::once(i.t("空白 / Blank", "Blank / Custom"))
            .chain(presets.iter().map(|p| gpui::SharedString::from(p.name)))
            .collect();

    // 1. Presets Header (if available)
    let mut preset_bar = div().flex().flex_col().gap(px(6.0));
    if !presets.is_empty() {
        preset_bar = preset_bar
            .child(field_label(i.t(
                "快速套用服务商预设（自动填入地址与模型参数）：",
                "Quick Presets (auto-fills endpoints & models):",
            )))
            .child(div().flex().flex_wrap().gap(px(5.0)).children(
                preset_names.iter().enumerate().map(|(idx, label)| {
                    let is_on = match preset_index {
                        None => idx == 0,
                        Some(p) => idx == p + 1,
                    };
                    let lbl = label.to_string();
                    button_l(
                        gpui::SharedString::from(format!("preset-pill-{idx}")),
                        lbl,
                        if is_on {
                            ButtonVariant::Primary
                        } else {
                            ButtonVariant::Secondary
                        },
                        &t,
                        cx,
                        move |ws, _ev, _w, cx| {
                            if let Some(dialog) = ws.ui.provider_dialog.as_mut() {
                                let p_idx = if idx == 0 { None } else { Some(idx - 1) };
                                dialog.preset_index = p_idx;
                                if let Some(p_idx) = p_idx {
                                    let presets = aitoolplus_core::presets::presets_for(dialog.tool);
                                    if let Some(preset) = presets.get(p_idx) {
                                        dialog.name.update(cx, |inp, cx| {
                                            inp.set_text_silent(preset.name, cx)
                                        });
                                        dialog.category = preset.category.to_string();
                                        dialog.website.update(cx, |inp, cx| {
                                            inp.set_text_silent(preset.website_url, cx)
                                        });

                                        let raw = serde_json::to_string(&preset.settings).unwrap_or_default();
                                        let mut extracted = extract_provider_config(dialog.tool, &raw);

                                        for (k, v) in preset.extra_env {
                                            match *k {
                                                "ANTHROPIC_MODEL" => extracted.model = v.to_string(),
                                                "ANTHROPIC_DEFAULT_SONNET_MODEL" => {
                                                    let (m, is_1m) = strip_1m(v);
                                                    extracted.sonnet_model = m;
                                                    extracted.sonnet_1m = is_1m;
                                                }
                                                "ANTHROPIC_DEFAULT_SONNET_MODEL_NAME" => {
                                                    extracted.sonnet_name = v.to_string();
                                                }
                                                "ANTHROPIC_DEFAULT_OPUS_MODEL" => {
                                                    let (m, is_1m) = strip_1m(v);
                                                    extracted.opus_model = m;
                                                    extracted.opus_1m = is_1m;
                                                }
                                                "ANTHROPIC_DEFAULT_OPUS_MODEL_NAME" => {
                                                    extracted.opus_name = v.to_string();
                                                }
                                                "ANTHROPIC_DEFAULT_HAIKU_MODEL" => {
                                                    let (m, is_1m) = strip_1m(v);
                                                    extracted.haiku_model = m;
                                                    extracted.haiku_1m = is_1m;
                                                }
                                                "ANTHROPIC_DEFAULT_HAIKU_MODEL_NAME" => {
                                                    extracted.haiku_name = v.to_string();
                                                }
                                                "ANTHROPIC_DEFAULT_FABLE_MODEL" => {
                                                    let (m, is_1m) = strip_1m(v);
                                                    extracted.fable_model = m;
                                                    extracted.fable_1m = is_1m;
                                                }
                                                "ANTHROPIC_DEFAULT_FABLE_MODEL_NAME" => {
                                                    extracted.fable_name = v.to_string();
                                                }
                                                "CLAUDE_CODE_SUBAGENT_MODEL" => {
                                                    let (m, is_1m) = strip_1m(v);
                                                    extracted.subagent_model = m;
                                                    extracted.subagent_1m = is_1m;
                                                }
                                                _ => {}
                                            }
                                        }

                                        dialog.base_url.update(cx, |inp, cx| {
                                            inp.set_text_silent(extracted.base_url, cx)
                                        });
                                        dialog.model.update(cx, |inp, cx| {
                                            inp.set_text_silent(extracted.model, cx)
                                        });
                                        dialog.sonnet_model.update(cx, |inp, cx| {
                                            inp.set_text_silent(extracted.sonnet_model, cx)
                                        });
                                        dialog.sonnet_name.update(cx, |inp, cx| {
                                            inp.set_text_silent(extracted.sonnet_name, cx)
                                        });
                                        dialog.opus_model.update(cx, |inp, cx| {
                                            inp.set_text_silent(extracted.opus_model, cx)
                                        });
                                        dialog.opus_name.update(cx, |inp, cx| {
                                            inp.set_text_silent(extracted.opus_name, cx)
                                        });
                                        dialog.haiku_model.update(cx, |inp, cx| {
                                            inp.set_text_silent(extracted.haiku_model, cx)
                                        });
                                        dialog.haiku_name.update(cx, |inp, cx| {
                                            inp.set_text_silent(extracted.haiku_name, cx)
                                        });
                                        dialog.fable_model.update(cx, |inp, cx| {
                                            inp.set_text_silent(extracted.fable_model, cx)
                                        });
                                        dialog.fable_name.update(cx, |inp, cx| {
                                            inp.set_text_silent(extracted.fable_name, cx)
                                        });
                                        dialog.subagent_model.update(cx, |inp, cx| {
                                            inp.set_text_silent(extracted.subagent_model, cx)
                                        });
                                        dialog.sonnet_1m = extracted.sonnet_1m;
                                        dialog.opus_1m = extracted.opus_1m;
                                        dialog.haiku_1m = extracted.haiku_1m;
                                        dialog.fable_1m = extracted.fable_1m;
                                        dialog.subagent_1m = extracted.subagent_1m;

                                        dialog.settings.update(cx, |ta, cx| {
                                            ta.set_text_silent(
                                                serde_json::to_string_pretty(&preset.settings)
                                                    .unwrap_or_default(),
                                                cx,
                                            );
                                        });

                                        if matches!(dialog.tool, ToolId::Pi | ToolId::OhMyPi) {
                                            if let Some(pk) = preset.settings.get("_providerKey").and_then(Value::as_str) {
                                                dialog.pi_provider_key.update(cx, |inp, cx| inp.set_text_silent(pk, cx));
                                            } else {
                                                let pk_default = preset.name.to_lowercase().replace(' ', "-");
                                                dialog.pi_provider_key.update(cx, |inp, cx| inp.set_text_silent(&pk_default, cx));
                                            }
                                            if let Some(fmt) = preset.settings.get("api").and_then(Value::as_str) {
                                                dialog.pi_api_format = fmt.to_string();
                                            }
                                            if let Some(arr) = preset.settings.get("models").and_then(Value::as_array) {
                                                dialog.pi_models.clear();
                                                for (m_i, item) in arr.iter().enumerate() {
                                                    let m_id = item.get("id").and_then(Value::as_str).unwrap_or_default().to_string();
                                                    let m_name = item.get("name").and_then(Value::as_str).unwrap_or_default().to_string();
                                                    let reasoning = item.get("reasoning").and_then(Value::as_bool).unwrap_or(false);
                                                    let image_input = item
                                                        .get("input")
                                                        .and_then(Value::as_array)
                                                        .map(|inputs| inputs.iter().any(|v| v.as_str() == Some("image")))
                                                        .unwrap_or(false);
                                                    let cw_str = item.get("contextWindow").map(|v| v.to_string()).unwrap_or_default();
                                                    let mt_str = item.get("maxTokens").map(|v| v.to_string()).unwrap_or_default();
                                                    let id_ent = cx.new(|cx| {
                                                        let mut inp = TextInput::new("模型 ID", cx);
                                                        inp.set_text_silent(m_id, cx);
                                                        inp
                                                    });
                                                    let name_ent = cx.new(|cx| {
                                                        let mut inp = TextInput::new("显示名称", cx);
                                                        inp.set_text_silent(m_name, cx);
                                                        inp
                                                    });
                                                    let cw_ent = cx.new(|cx| {
                                                        let mut inp = TextInput::new("上下文窗口", cx);
                                                        inp.set_text_silent(cw_str, cx);
                                                        inp
                                                    });
                                                    let mt_ent = cx.new(|cx| {
                                                        let mut inp = TextInput::new("最大输出", cx);
                                                        inp.set_text_silent(mt_str, cx);
                                                        inp
                                                    });
                                                    dialog.pi_models.push(crate::pages::PiModelDraft {
                                                        key: format!("pi_model_{m_i}"),
                                                        id: id_ent,
                                                        name: name_ent,
                                                        reasoning,
                                                        image_input,
                                                        context_window: cw_ent,
                                                        max_tokens: mt_ent,
                                                        is_expanded: false,
                                                    });
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        },
                    )
                }),
            ));
    }

    // 2. Section: 基础信息 (Basic Info)
    let is_pi_tool = matches!(tool, ToolId::Pi | ToolId::OhMyPi);
    let category_picker = {
        let mut category_row = div().flex().gap(px(4.0)).flex_wrap();
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
            category_row = category_row.child(button_l(
                gpui::SharedString::from(format!("cat-sel-{cat}")),
                label,
                if is_current {
                    ButtonVariant::Primary
                } else {
                    ButtonVariant::Secondary
                },
                &t,
                cx,
                move |this, _ev, _w, cx| {
                    if let Some(dialog) = this.ui.provider_dialog.as_mut() {
                        dialog.category = cat2.clone();
                    }
                    cx.notify();
                },
            ));
        }
        category_row
    };

    let website_field = div()
        .flex_1()
        .flex()
        .flex_col()
        .gap(px(4.0))
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .child(field_label(i.t("官网地址（可选）", "Website (optional)")))
                .child({
                    let web_txt = website.read(cx).text().to_string();
                    let has_url = !web_txt.trim().is_empty();
                    if has_url {
                        div()
                            .id("open-website-link")
                            .cursor_pointer()
                            .flex()
                            .items_center()
                            .gap(px(3.0))
                            .text_size(px(11.0))
                            .text_color(t.accent)
                            .hover(|h| h.underline())
                            .on_click(cx.listener(move |_ws, _ev, _w, _cx| {
                                open_in_browser(&web_txt);
                            }))
                            .child(gpui::svg().data(crate::icons::EXTERNAL_LINK_SVG).size(px(11.0)).text_color(t.accent))
                            .child(i.t("打开官网", "Open"))
                            .into_any_element()
                    } else {
                        div().into_any_element()
                    }
                }),
        )
        .child(input_container(&t, website.clone()));

    let notes_field = div()
        .flex_1()
        .flex()
        .flex_col()
        .gap(px(4.0))
        .child(field_label(i.t("备注（可选）", "Notes (optional)")))
        .child(input_container(&t, notes.clone()));

    let mut basic_section = section_card(i.t("基础信息", "Basic Information"));

    if is_pi_tool {
        basic_section = basic_section
            .child(
                div()
                    .flex()
                    .gap(px(10.0))
                    .child(
                        div()
                            .flex_1()
                            .flex()
                            .flex_col()
                            .gap(px(4.0))
                            .child(field_label(i.t("供应商名称 *", "Provider Name *")))
                            .child(input_container(&t, name.clone())),
                    )
                    .child(
                        div()
                            .flex_1()
                            .flex()
                            .flex_col()
                            .gap(px(4.0))
                            .child(field_label(i.t("Provider Key (配置标识) *", "Provider Key *")))
                            .child(input_container(&t, pi_provider_key.clone())),
                    ),
            )
            .child(
                div()
                    .flex()
                    .gap(px(10.0))
                    .child(
                        div()
                            .flex_1()
                            .flex()
                            .flex_col()
                            .gap(px(4.0))
                            .child(field_label(i.t("分类", "Category")))
                            .child(category_picker),
                    )
                    .child(website_field),
            )
            .child(
                div()
                    .flex()
                    .gap(px(10.0))
                    .child(notes_field),
            );
    } else {
        basic_section = basic_section
            .child(
                div()
                    .flex()
                    .gap(px(10.0))
                    .child(
                        div()
                            .flex_1()
                            .flex()
                            .flex_col()
                            .gap(px(4.0))
                            .child(field_label(i.t("供应商名称 *", "Provider Name *")))
                            .child(input_container(&t, name.clone())),
                    )
                    .child(
                        div()
                            .flex_1()
                            .flex()
                            .flex_col()
                            .gap(px(4.0))
                            .child(field_label(i.t("分类", "Category")))
                            .child(category_picker),
                    ),
            )
            .child(
                div()
                    .flex()
                    .gap(px(10.0))
                    .child(website_field)
                    .child(notes_field),
            );
    }

    // 3. Section: 接口与凭据 (Connection & Credentials)
    let is_official = category == "official";
    let mut connection_section = section_card(i.t("接口与凭据", "Connection & Credentials"));

    if is_official {
        connection_section = connection_section.child(
            div()
                .p(px(10.0))
                .rounded(px(6.0))
                .bg(t.accent_subtle)
                .border_1()
                .border_color(t.accent)
                .text_size(px(12.0))
                .text_color(t.text_primary)
                .child(i.t(
                    "💡 当前使用的是官方直连模式。无需填写 Base URL 或自定义 API Key，CLI 将直接使用官方认证/订阅登录。",
                    "💡 Using official direct connection mode. No Base URL or custom API Key required.",
                )),
        );
    } else {
        // Base URL
        connection_section = connection_section.child(
            div()
                .flex()
                .flex_col()
                .gap(px(4.0))
                .child(field_label(i.t("接口地址 (Base URL)", "Base URL")))
                .child(input_container(&t, base_url.clone()))
                .child(
                    div()
                        .text_size(px(11.0))
                        .text_color(t.text_muted)
                        .child(i.t(
                            "例如: https://api.anthropic.com 或第三方中转地址",
                            "e.g. https://api.anthropic.com or reverse proxy URL",
                        )),
                ),
        );

        let web_url_for_key = website.read(cx).text().to_string();
        connection_section = connection_section.child(
            div()
                .flex()
                .flex_col()
                .gap(px(4.0))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .child(field_label(i.t("API Key / 访问密钥", "API Key / Token")))
                        .child({
                            let has_web = !web_url_for_key.trim().is_empty();
                            if has_web {
                                div()
                                    .id("get-api-key-link")
                                    .cursor_pointer()
                                    .text_size(px(11.0))
                                    .text_color(t.accent)
                                    .hover(|h| h.underline())
                                    .on_click(cx.listener(move |_ws, _ev, _w, _cx| {
                                        open_in_browser(&web_url_for_key);
                                    }))
                                    .child(i.t("获取 API Key ->", "Get API key ->"))
                                    .into_any_element()
                            } else {
                                div().into_any_element()
                            }
                        }),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(6.0))
                        .child(div().flex_1().child(input_container(&t, api_key.clone())))
                        .child(crate::components::icon_button_svg(
                            "toggle-api-key-visibility",
                            if show_api_key {
                                crate::icons::EYE_OFF_SVG
                            } else {
                                crate::icons::EYE_SVG
                            },
                            if show_api_key {
                                i.t("隐藏密钥", "Hide API Key")
                            } else {
                                i.t("显示密钥", "Show API Key")
                            },
                            false,
                            &t,
                            cx,
                            |ws, _ev, _w, cx| {
                                if let Some(dialog) = ws.ui.provider_dialog.as_mut() {
                                    dialog.show_api_key = !dialog.show_api_key;
                                    let secret = !dialog.show_api_key;
                                    dialog.api_key.update(cx, |inp, cx| inp.set_secret(secret, cx));
                                }
                                cx.notify();
                            },
                        )),
                ),
        );

        // API Format (for Pi / Oh My Pi)
        if matches!(tool, ToolId::Pi | ToolId::OhMyPi) {
            connection_section = connection_section.child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(4.0))
                    .child(field_label(i.t("API 协议格式 (API Format)", "API Format")))
                    .child({
                        let formats = [
                            ("openai-completions", "OpenAI Chat Completions"),
                            ("openai-responses", "OpenAI Responses"),
                            ("anthropic-messages", "Anthropic Messages"),
                            ("google-generative-ai", "Google Generative AI"),
                            ("bedrock-converse-stream", "Amazon Bedrock"),
                        ];
                        let mut fmt_row = div().flex().gap(px(6.0)).flex_wrap();
                        for (f_val, f_lbl) in formats {
                            let is_curr = pi_api_format == f_val;
                            let f_val2 = f_val.to_string();
                            fmt_row = fmt_row.child(button_l(
                                gpui::SharedString::from(format!("pi-api-fmt-{f_val}")),
                                f_lbl,
                                if is_curr {
                                    ButtonVariant::Primary
                                } else {
                                    ButtonVariant::Secondary
                                },
                                &t,
                                cx,
                                move |ws, _ev, _w, cx| {
                                    if let Some(dialog) = ws.ui.provider_dialog.as_mut() {
                                        dialog.pi_api_format = f_val2.clone();
                                    }
                                    cx.notify();
                                },
                            ));
                        }
                        fmt_row
                    }),
            );
        }
    }

    // 4. Section: 模型配置 (Model Configuration)
    let model_section = {
        let has_models = !fetched_models.is_empty();
        let active_target = active_model_dropdown.clone();
        let query = model_search.read(cx).text().trim().to_lowercase();
        let filtered_models: Vec<_> = fetched_models
            .iter()
            .filter(|m| {
                if query.is_empty() {
                    true
                } else {
                    m.id.to_lowercase().contains(&query)
                        || m.owned_by.as_deref().unwrap_or("").to_lowercase().contains(&query)
                        || m.display_name.as_deref().unwrap_or("").to_lowercase().contains(&query)
                }
            })
            .collect();

        // Universal Model Input with Dropdown Component
        let render_model_input_with_fetch = {
            let t = t.clone();
            let i = i;
            let is_fetching = is_fetching_models;
            let has_models = has_models;
            let active_target = active_target.clone();
            let filtered_len = filtered_models.len();
            let total_models_len = fetched_models.len();
            let model_search = model_search.clone();
            let model_bounds = model_bounds.clone();

            let mut grouped_models: std::collections::BTreeMap<String, Vec<&aitoolplus_core::api_hub::FetchedModel>> = std::collections::BTreeMap::new();
            for m in &filtered_models {
                let vendor = m.owned_by.clone().unwrap_or_else(|| "Other".to_string());
                grouped_models.entry(vendor).or_default().push(m);
            }

            move |target_id: String,
                  model_ent: gpui::Entity<TextInput>,
                  display_name_ent: Option<gpui::Entity<TextInput>>,
                  cx: &mut Context<Workspace>| -> gpui::AnyElement {
                let is_open = active_target.as_deref() == Some(&target_id);
                let target_str = target_id.clone();
                let target_str_close = target_id.clone();
                let t2 = t.clone();

                let mut input_row = div()
                    .relative()
                    .flex()
                    .items_center()
                    .gap(px(4.0))
                    .child(div().flex_1().child(input_container(&t2, model_ent.clone())))
                    .on_prepaint({
                        let target_id = target_id.clone();
                        let entity = cx.entity().clone();
                        move |bounds, window, cx| {
                            let should_notify = entity.update(cx, |ws, cx| {
                                if let Some(d) = ws.ui.provider_dialog.as_mut() {
                                    let old = d.model_bounds.insert(target_id.clone(), bounds);
                                    if old.is_none() && d.active_model_dropdown.as_deref() == Some(&target_id) {
                                        cx.notify();
                                        return true;
                                    }
                                }
                                false
                            });
                            if should_notify {
                                window.request_animation_frame();
                            }
                        }
                    });

                if is_fetching {
                    input_row = input_row.child(
                        div()
                            .flex()
                            .items_center()
                            .justify_center()
                            .size(px(28.0))
                            .child(gpui_kit::component::spinner::Spinner::new().color(t2.accent.into()))
                    );
                } else if has_models {
                    let ts = target_str.clone();
                    input_row = input_row.child(
                        crate::components::icon_button_svg(
                            format!("btn-drop-{target_id}"),
                            if is_open { crate::icons::CHEVRON_UP_SVG } else { crate::icons::CHEVRON_DOWN_SVG },
                            if is_open { i.t("收起下拉", "Close") } else { i.t("选择模型", "Select") },
                            false,
                            &t2,
                            cx,
                            move |ws, _ev, _w, cx| {
                                if let Some(d) = ws.ui.provider_dialog.as_mut() {
                                    if d.active_model_dropdown.as_deref() == Some(&ts) {
                                        d.active_model_dropdown = None;
                                    } else {
                                        d.model_search.update(cx, |inp, cx| inp.set_text_silent("", cx));
                                        d.active_model_dropdown = Some(ts.clone());
                                    }
                                }
                                cx.notify();
                            },
                        )
                    );
                }

                let mut col = div().flex().flex_col().gap(px(4.0)).flex_1();
                col = col.child(input_row);

                if is_open && has_models {
                    let mut panel = div()
                        .id(gpui::SharedString::from(format!("dropdown-popover-{target_id}")))
                        .occlude()
                        .p(px(8.0))
                        .rounded(px(8.0))
                        .bg(t2.card_bg)
                        .border_1()
                        .border_color(t2.accent)
                        .shadow_xl()
                        .flex()
                        .flex_col()
                        .gap(px(6.0))
                        .on_mouse_down_out({
                            let entity = cx.entity().clone();
                            move |_ev, _window, cx| {
                                entity.update(cx, |ws, cx| {
                                    if let Some(d) = ws.ui.provider_dialog.as_mut() {
                                        d.active_model_dropdown = None;
                                    }
                                    cx.notify();
                                });
                            }
                        });

                    panel = panel.child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(6.0))
                            .child(gpui::svg().data(crate::icons::SEARCH_SVG).size(px(13.0)).text_color(t2.text_secondary))
                            .child(div().flex_1().child(input_container(&t2, model_search.clone())))
                            .child(crate::components::icon_button_svg(
                                format!("btn-close-pop-{target_str_close}"),
                                crate::icons::X_SVG,
                                i.t("关闭", "Close"),
                                false,
                                &t2,
                                cx,
                                |ws, _, _, cx| {
                                    if let Some(d) = ws.ui.provider_dialog.as_mut() {
                                        d.active_model_dropdown = None;
                                    }
                                    cx.notify();
                                },
                            ))
                    );

                    panel = panel.child(
                        div()
                            .px(px(2.0))
                            .text_size(px(10.5))
                            .text_color(t2.text_muted)
                            .child(format!(
                                "{} {} / {} {}",
                                i.t("匹配", "Matched"),
                                filtered_len,
                                total_models_len,
                                i.t("个模型（点击条目直接填入）", "models (click to select)")
                            ))
                    );

                    let mut list_container = div()
                        .id(gpui::SharedString::from(format!("list-scroll-{target_id}")))
                        .max_h(px(200.0))
                        .overflow_y_scroll()
                        .flex()
                        .flex_col()
                        .gap(px(3.0));

                    if filtered_len == 0 {
                        list_container = list_container.child(
                            div()
                                .py(px(12.0))
                                .flex()
                                .items_center()
                                .justify_center()
                                .text_size(px(11.5))
                                .text_color(t2.text_muted)
                                .child(i.t("未找到匹配的模型", "No matching models found"))
                        );
                    } else {
                        for (vendor, m_list) in &grouped_models {
                            list_container = list_container.child(
                                div()
                                    .px(px(6.0))
                                    .pt(px(4.0))
                                    .pb(px(1.0))
                                    .text_size(px(10.0))
                                    .font_weight(gpui::FontWeight::BOLD)
                                    .text_color(t2.accent)
                                    .child(vendor.to_uppercase())
                            );

                            for m in m_list {
                                let m_id = m.id.clone();
                                let m_disp = m.display_name.clone();
                                let m_ent_c = model_ent.clone();
                                let d_ent_c = display_name_ent.clone();
                                let t3 = t2.clone();

                                let mut row_el = div()
                                    .id(gpui::SharedString::from(format!("opt-{}-{target_id}", m_id)))
                                    .cursor_pointer()
                                    .px(px(8.0))
                                    .py(px(4.0))
                                    .rounded(px(5.0))
                                    .bg(t3.input_bg)
                                    .border_1()
                                    .border_color(t3.input_border)
                                    .hover(|h| h.bg(t3.row_hover).border_color(t3.accent))
                                    .flex()
                                    .items_center()
                                    .justify_between()
                                    .gap(px(6.0))
                                    .on_click({
                                        let m_id_c = m_id.clone();
                                        let m_disp_c = m_disp.clone();
                                        let entity = cx.entity().clone();
                                        move |_ev, _window, cx| {
                                            entity.update(cx, |ws, cx| {
                                                m_ent_c.update(cx, |inp, cx| inp.set_text_silent(&m_id_c, cx));
                                                if let Some(dn_ent) = &d_ent_c {
                                                    let curr_dn = dn_ent.read(cx).text().trim().to_string();
                                                    if curr_dn.is_empty() {
                                                        let fallback_name = m_disp_c.as_deref().unwrap_or(&m_id_c);
                                                        dn_ent.update(cx, |inp, cx| inp.set_text_silent(fallback_name, cx));
                                                    }
                                                }
                                                if let Some(d) = ws.ui.provider_dialog.as_mut() {
                                                    d.active_model_dropdown = None;
                                                }
                                                let msg = ws.i18n.t(
                                                    &format!("已选择模型: {m_id_c}"),
                                                    &format!("Selected model: {m_id_c}"),
                                                ).to_string();
                                                ws.ui.toast(msg, false);
                                                cx.notify();
                                            });
                                        }
                                    });

                                let mut left_col = div().flex_1().flex().items_center().gap(px(6.0)).min_w(px(0.0));
                                left_col = left_col.child(
                                    div()
                                        .text_size(px(11.5))
                                        .font_weight(gpui::FontWeight::MEDIUM)
                                        .text_color(t3.text_primary)
                                        .child(m.id.clone())
                                );

                                if let Some(dn) = &m.display_name {
                                    if dn != &m.id {
                                        left_col = left_col.child(
                                            div()
                                                .text_size(px(10.5))
                                                .text_color(t3.text_muted)
                                                .child(format!("({dn})"))
                                        );
                                    }
                                }

                                if let Some(ctx_len) = m.context_length {
                                    let k_len = ctx_len / 1000;
                                    left_col = left_col.child(
                                        div()
                                            .px(px(4.0))
                                            .py(px(1.0))
                                            .rounded(px(3.0))
                                            .bg(t3.sidebar_bg)
                                            .text_size(px(9.5))
                                            .text_color(t3.text_muted)
                                            .child(format!("{k_len}k"))
                                    );
                                }

                                row_el = row_el.child(left_col);
                                row_el = row_el.child(
                                    div()
                                        .px(px(6.0))
                                        .py(px(2.0))
                                        .rounded(px(4.0))
                                        .bg(t3.accent_subtle)
                                        .border_1()
                                        .border_color(t3.accent)
                                        .text_size(px(10.0))
                                        .text_color(t3.accent)
                                        .font_weight(gpui::FontWeight::MEDIUM)
                                        .child(i.t("选择", "Select"))
                                );

                                list_container = list_container.child(row_el);
                            }
                        }
                    }

                    panel = panel.child(list_container);

                    if let Some(&bounds) = model_bounds.get(&target_id) {
                        let align = if target_id.starts_with("pi_") {
                            Align::Start
                        } else {
                            Align::End
                        };
                        let floating_overlay = deferred(
                            Positioner::side(bounds)
                                .placement(Placement::Bottom)
                                .align(align)
                                .offset(px(4.0))
                                .margin(px(8.0))
                                .occlude()
                                .child(
                                    panel.w(bounds.size.width.max(px(380.0)))
                                )
                        )
                        .with_priority(POPUP_PRIORITY);

                        col = col.child(floating_overlay);
                    }
                }

                col.into_any_element()
            }
        };

        if matches!(tool, ToolId::Pi | ToolId::OhMyPi) {
            let mut pi_sec = div()
                .p(px(12.0))
                .rounded(px(8.0))
                .bg(t.sidebar_bg)
                .border_1()
                .border_color(t.card_border)
                .flex()
                .flex_col()
                .gap(px(10.0));

            // Pi Header
            pi_sec = pi_sec.child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(6.0))
                            .child(
                                div()
                                    .text_size(px(12.5))
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .text_color(t.text_primary)
                                    .child(i.t("模型配置与列表 (Models)", "Models Configuration")),
                            )
                            .child(
                                div()
                                    .px(px(6.0))
                                    .py(px(1.0))
                                    .rounded(px(4.0))
                                    .bg(t.accent_subtle)
                                    .text_size(px(10.5))
                                    .text_color(t.accent)
                                    .font_weight(gpui::FontWeight::MEDIUM)
                                    .child(format!("{} {}", pi_models.len(), i.t("个模型", "models"))),
                            )
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            // Fetch upstream
                            .child({
                                let t2 = t.clone();
                                button_with_icon_loading_l(
                                    "btn-pi-fetch-models",
                                    if has_models { crate::icons::REFRESH_SVG } else { crate::icons::DOWNLOAD_SVG },
                                    if is_fetching_models {
                                        i.t("获取中...", "Fetching...")
                                    } else if has_models {
                                        i.t("重新获取", "Refresh")
                                    } else {
                                        i.t("获取上游模型", "Fetch Models")
                                    },
                                    ButtonVariant::Secondary,
                                    is_fetching_models,
                                    &t2,
                                    cx,
                                    |ws, _ev, _w, cx| {
                                        fetch_upstream_models_for_dialog(ws, cx);
                                    },
                                )
                            })
                            // Import all
                            .when(has_models, |row| {
                                let t2 = t.clone();
                                row.child(button_with_icon_l(
                                    "btn-pi-import-all",
                                    crate::icons::PLUS_SVG,
                                    i.t("导入全部", "Import All"),
                                    ButtonVariant::Secondary,
                                    &t2,
                                    cx,
                                    move |ws, _ev, _w, cx| {
                                        if let Some(d) = ws.ui.provider_dialog.as_mut() {
                                            let mut added = 0;
                                            for fm in &d.fetched_models {
                                                let exists = d.pi_models.iter().any(|m| m.id.read(cx).text().trim() == fm.id);
                                                if !exists {
                                                    let id_ent = cx.new(|cx| {
                                                        let mut inp = TextInput::new("模型 ID", cx);
                                                        inp.set_text_silent(fm.id.clone(), cx);
                                                        inp
                                                    });
                                                    let name_ent = cx.new(|cx| {
                                                        let mut inp = TextInput::new("显示名称", cx);
                                                        inp.set_text_silent(fm.display_name.clone().unwrap_or_else(|| fm.id.clone()), cx);
                                                        inp
                                                    });
                                                    let cw_str = fm.context_length.map(|l| l.to_string()).unwrap_or_else(|| "128000".into());
                                                    let ctx_ent = cx.new(|cx| {
                                                        let mut inp = TextInput::new("例如 128000", cx);
                                                        inp.set_text_silent(cw_str, cx);
                                                        inp
                                                    });
                                                    let max_ent = cx.new(|cx| {
                                                        let mut inp = TextInput::new("例如 4096", cx);
                                                        inp.set_text_silent("16384".to_string(), cx);
                                                        inp
                                                    });
                                                    let reasoning = fm.id.contains("reasoner") || fm.id.contains("r1");
                                                    let key = format!("pi_model_{}", d.pi_models.len());
                                                    d.pi_models.push(crate::pages::PiModelDraft {
                                                        key,
                                                        id: id_ent,
                                                        name: name_ent,
                                                        context_window: ctx_ent,
                                                        max_tokens: max_ent,
                                                        reasoning,
                                                        image_input: false,
                                                        is_expanded: false,
                                                    });
                                                    added += 1;
                                                }
                                            }
                                            let msg = format!("{} {} {}", ws.i18n.t("已导入", "Imported"), added, ws.i18n.t("个模型", "models"));
                                            ws.ui.toast(msg, false);
                                        }
                                        cx.notify();
                                    },
                                ))
                            })
                            // Add model row
                            .child({
                                let t2 = t.clone();
                                button_with_icon_l(
                                    "btn-pi-add-model",
                                    crate::icons::PLUS_SVG,
                                    i.t("添加模型", "Add Model"),
                                    ButtonVariant::Primary,
                                    &t2,
                                    cx,
                                    move |ws, _ev, _w, cx| {
                                        if let Some(d) = ws.ui.provider_dialog.as_mut() {
                                            let key = format!("pi_model_{}", d.pi_models.len());
                                            let id_ent = cx.new(|cx| TextInput::new("模型 ID，如 deepseek-chat", cx));
                                            let name_ent = cx.new(|cx| TextInput::new("显示名称，如 DeepSeek-V3", cx));
                                            let cw_ent = cx.new(|cx| {
                                                let mut inp = TextInput::new("上下文窗口", cx);
                                                inp.set_text_silent("128000", cx);
                                                inp
                                            });
                                            let mt_ent = cx.new(|cx| {
                                                let mut inp = TextInput::new("最大输出", cx);
                                                inp.set_text_silent("16384", cx);
                                                inp
                                            });
                                            d.pi_models.push(crate::pages::PiModelDraft {
                                                key,
                                                id: id_ent,
                                                name: name_ent,
                                                reasoning: false,
                                                image_input: false,
                                                context_window: cw_ent,
                                                max_tokens: mt_ent,
                                                is_expanded: false,
                                            });
                                            let msg = ws.i18n.t("已添加模型行", "Model row added").to_string();
                                            ws.ui.toast(msg, false);
                                        }
                                        cx.notify();
                                    },
                                )
                            })
                    )
            );

            // Fetch Error Notice
            if let Some(err_txt) = &fetch_error {
                let t2 = t.clone();
                pi_sec = pi_sec.child(
                    div()
                        .px(px(10.0))
                        .py(px(6.0))
                        .rounded(px(6.0))
                        .bg(gpui::rgba(0xef444415))
                        .border_1()
                        .border_color(gpui::rgba(0xef444440))
                        .flex()
                        .items_center()
                        .justify_between()
                        .child(
                            div()
                                .text_size(px(11.5))
                                .text_color(gpui::rgb(0xef4444))
                                .child(format!("{}: {}", i.t("获取模型失败", "Fetch models failed"), err_txt))
                        )
                        .child(crate::components::icon_button_svg(
                            "btn-dismiss-pi-fetch-err",
                            crate::icons::X_SVG,
                            i.t("关闭", "Close"),
                            false,
                            &t2,
                            cx,
                            |ws, _, _, cx| {
                                if let Some(d) = ws.ui.provider_dialog.as_mut() {
                                    d.fetch_error = None;
                                }
                                cx.notify();
                            },
                        ))
                );
            }

            // Pi Models Table
            if pi_models.is_empty() {
                pi_sec = pi_sec.child(
                    div()
                        .py(px(20.0))
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_size(px(12.0))
                        .text_color(t.text_muted)
                        .child(i.t("暂无模型配置，请点击右上角【添加模型】或【获取上游模型】", "No models configured. Click Add Model or Fetch Models."))
                );
            } else {
                // Table Header
                pi_sec = pi_sec.child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(6.0))
                        .px(px(2.0))
                        .text_size(px(11.0))
                        .font_weight(gpui::FontWeight::MEDIUM)
                        .text_color(t.text_secondary)
                        .child(div().w(px(28.0)))
                        .child(div().flex_1().child(i.t("模型 ID *", "Model ID *")))
                        .child(div().flex_1().child(i.t("显示名称", "Display Name")))
                        .child(div().w(px(28.0)))
                );

                for (idx, draft) in pi_models.iter().enumerate() {
                    let is_expanded = draft.is_expanded;
                    let is_reasoning = draft.reasoning;
                    let has_image = draft.image_input;
                    let t2 = t.clone();

                    let mut row_card = div()
                        .p(px(8.0))
                        .rounded(px(6.0))
                        .bg(t2.card_bg)
                        .border_1()
                        .border_color(t2.card_border)
                        .flex()
                        .flex_col()
                        .gap(px(6.0));

                    // Row top controls
                    let mut top_row = div()
                        .flex()
                        .items_center()
                        .gap(px(6.0));

                    // Expand toggle
                    top_row = top_row.child(crate::components::icon_button_svg(
                        format!("btn-expand-pi-{idx}"),
                        if is_expanded { crate::icons::CHEVRON_DOWN_SVG } else { crate::icons::CHEVRON_RIGHT_SVG },
                        if is_expanded { i.t("收起参数", "Collapse") } else { i.t("展开高级参数", "Expand") },
                        false,
                        &t2,
                        cx,
                        move |ws, _ev, _w, cx| {
                            if let Some(d) = ws.ui.provider_dialog.as_mut() {
                                if let Some(m) = d.pi_models.get_mut(idx) {
                                    m.is_expanded = !m.is_expanded;
                                }
                            }
                            cx.notify();
                        },
                    ));

                    // Model ID input with fetch/dropdown
                    top_row = top_row.child(render_model_input_with_fetch(
                        format!("pi_{idx}"),
                        draft.id.clone(),
                        Some(draft.name.clone()),
                        cx,
                    ));

                    // Model Name input
                    top_row = top_row.child(
                        div()
                            .flex_1()
                            .child(input_container(&t2, draft.name.clone()))
                    );

                    // Delete button
                    top_row = top_row.child(crate::components::icon_button_svg(
                        format!("btn-del-pi-model-{idx}"),
                        crate::icons::TRASH_SVG,
                        i.t("移除模型", "Remove model"),
                        true,
                        &t2,
                        cx,
                        move |ws, _ev, _w, cx| {
                            if let Some(d) = ws.ui.provider_dialog.as_mut() {
                                if d.pi_models.len() > 1 {
                                    d.pi_models.remove(idx);
                                    let msg = ws.i18n.t("已移除模型", "Model removed").to_string();
                                    ws.ui.toast(msg, false);
                                } else {
                                    let msg = ws.i18n.t("至少保留一个模型配置", "Keep at least one model").to_string();
                                    ws.ui.toast(msg, true);
                                }
                            }
                            cx.notify();
                        },
                    ));

                    row_card = row_card.child(top_row);

                    // Expanded detail panel
                    if is_expanded {
                        let exp_panel = div()
                            .pl(px(34.0))
                            .pt(px(4.0))
                            .pb(px(2.0))
                            .border_l_2()
                            .border_color(t2.card_border)
                            .flex()
                            .flex_col()
                            .gap(px(6.0))
                            .child(
                                div()
                                    .flex()
                                    .gap(px(8.0))
                                    .items_center()
                                    // Reasoning toggle switch
                                    .child(
                                        gpui_kit::component::checkbox::Checkbox::new(gpui::SharedString::from(format!("btn-toggle-reasoning-{idx}")))
                                            .checked(is_reasoning)
                                            .label("🧠 思考推理 (Reasoning)")
                                            .on_click({
                                                let entity = cx.entity().clone();
                                                move |_checked, _window, cx| {
                                                    entity.update(cx, |ws, cx| {
                                                        if let Some(d) = ws.ui.provider_dialog.as_mut() {
                                                            if let Some(m) = d.pi_models.get_mut(idx) {
                                                                m.reasoning = !m.reasoning;
                                                            }
                                                        }
                                                        cx.notify();
                                                    });
                                                }
                                            })
                                    )
                                    // Image input switch
                                    .child(
                                        gpui_kit::component::checkbox::Checkbox::new(gpui::SharedString::from(format!("btn-toggle-img-{idx}")))
                                            .checked(has_image)
                                            .label("🖼️ 支持图片输入 (image)")
                                            .on_click({
                                                let entity = cx.entity().clone();
                                                move |_checked, _window, cx| {
                                                    entity.update(cx, |ws, cx| {
                                                        if let Some(d) = ws.ui.provider_dialog.as_mut() {
                                                            if let Some(m) = d.pi_models.get_mut(idx) {
                                                                m.image_input = !m.image_input;
                                                            }
                                                        }
                                                        cx.notify();
                                                    });
                                                }
                                            })
                                    )
                            )
                            .child(
                                div()
                                    .flex()
                                    .gap(px(8.0))
                                    .items_center()
                                    // Context Window
                                    .child(
                                        div()
                                            .flex_1()
                                            .flex()
                                            .items_center()
                                            .gap(px(4.0))
                                            .child(div().text_size(px(11.0)).text_color(t2.text_muted).child("上下文窗口:"))
                                            .child(div().flex_1().child(input_container(&t2, draft.context_window.clone())))
                                    )
                                    // Max Tokens
                                    .child(
                                        div()
                                            .flex_1()
                                            .flex()
                                            .items_center()
                                            .gap(px(4.0))
                                            .child(div().text_size(px(11.0)).text_color(t2.text_muted).child("最大输出:"))
                                            .child(div().flex_1().child(input_container(&t2, draft.max_tokens.clone())))
                                    )
                            );
                        row_card = row_card.child(exp_panel);
                    }

                    pi_sec = pi_sec.child(row_card);
                }
            }

            pi_sec.into_any_element()
        } else {
            // Model section for Claude Code / generic tools
            let mut sec = div()
                .p(px(12.0))
                .rounded(px(8.0))
                .bg(t.sidebar_bg)
                .border_1()
                .border_color(t.card_border)
                .flex()
                .flex_col()
                .gap(px(10.0));

            // Section Header: Title & Action buttons (Quick Set & Fetch Models)
            sec = sec.child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(2.0))
                            .child(
                                div()
                                    .text_size(px(12.5))
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .text_color(t.text_primary)
                                    .child(i.t("模型角色映射 (Model Mapping)", "Model Mapping")),
                            )
                            .child(
                                div()
                                    .text_size(px(11.0))
                                    .text_color(t.text_muted)
                                    .child(i.t("配置各角色的请求模型与显示名称", "Configure request models & display names for each role")),
                            )
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            // Quick Set Wand2 button
                            .child({
                                let t2 = t.clone();
                                button_with_icon_l(
                                    "btn-quick-set-roles",
                                    crate::icons::WAND_SVG,
                                    i.t("一键设置", "Quick Set"),
                                    ButtonVariant::Secondary,
                                    &t2,
                                    cx,
                                    move |ws, _ev, _w, cx| {
                                        if let Some(d) = ws.ui.provider_dialog.as_mut() {
                                            let base_m = {
                                                let m0 = d.model.read(cx).text().trim().to_string();
                                                let m1 = d.sonnet_model.read(cx).text().trim().to_string();
                                                let m2 = d.opus_model.read(cx).text().trim().to_string();
                                                let m3 = d.haiku_model.read(cx).text().trim().to_string();
                                                if !m0.is_empty() { m0 }
                                                else if !m1.is_empty() { m1 }
                                                else if !m2.is_empty() { m2 }
                                                else if !m3.is_empty() { m3 }
                                                else { String::new() }
                                            };
                                            if !base_m.is_empty() {
                                                d.sonnet_model.update(cx, |inp, cx| inp.set_text_silent(&base_m, cx));
                                                d.opus_model.update(cx, |inp, cx| inp.set_text_silent(&base_m, cx));
                                                d.haiku_model.update(cx, |inp, cx| inp.set_text_silent(&base_m, cx));
                                                d.subagent_model.update(cx, |inp, cx| inp.set_text_silent(&base_m, cx));
                                                d.fable_model.update(cx, |inp, cx| inp.set_text_silent(&base_m, cx));
                                                if d.sonnet_name.read(cx).text().trim().is_empty() {
                                                    d.sonnet_name.update(cx, |inp, cx| inp.set_text_silent(&base_m, cx));
                                                }
                                                if d.opus_name.read(cx).text().trim().is_empty() {
                                                    d.opus_name.update(cx, |inp, cx| inp.set_text_silent(&base_m, cx));
                                                }
                                                if d.haiku_name.read(cx).text().trim().is_empty() {
                                                    d.haiku_name.update(cx, |inp, cx| inp.set_text_silent(&base_m, cx));
                                                }
                                                if d.fable_name.read(cx).text().trim().is_empty() {
                                                    d.fable_name.update(cx, |inp, cx| inp.set_text_silent(&base_m, cx));
                                                }
                                                let msg = ws.i18n.t("已一键将模型应用到所有角色", "Model applied to all roles").to_string();
                                                ws.ui.toast(msg, false);
                                            } else {
                                                let msg = ws.i18n.t("请先填写任一模型", "Please enter a model name first").to_string();
                                                ws.ui.toast(msg, true);
                                            }
                                        }
                                        cx.notify();
                                    },
                                )
                            })
                            // Fetch Models button
                            .child({
                                let t2 = t.clone();
                                button_with_icon_loading_l(
                                    "btn-fetch-models-top",
                                    if has_models { crate::icons::REFRESH_SVG } else { crate::icons::DOWNLOAD_SVG },
                                    if is_fetching_models {
                                        i.t("获取中...", "Fetching...")
                                    } else if has_models {
                                        i.t("重新获取", "Refresh")
                                    } else {
                                        i.t("获取上游模型", "Fetch Models")
                                    },
                                    ButtonVariant::Secondary,
                                    is_fetching_models,
                                    &t2,
                                    cx,
                                    |ws, _ev, _w, cx| {
                                        fetch_upstream_models_for_dialog(ws, cx);
                                    },
                                )
                            })
                    )
            );

            // Fetch Error Notice
            if let Some(err_txt) = &fetch_error {
                let t2 = t.clone();
                sec = sec.child(
                    div()
                        .px(px(10.0))
                        .py(px(6.0))
                        .rounded(px(6.0))
                        .bg(gpui::rgba(0xef444415))
                        .border_1()
                        .border_color(gpui::rgba(0xef444440))
                        .flex()
                        .items_center()
                        .justify_between()
                        .child(
                            div()
                                .text_size(px(11.5))
                                .text_color(gpui::rgb(0xef4444))
                                .child(format!("{}: {}", i.t("获取模型失败", "Fetch models failed"), err_txt))
                        )
                        .child(crate::components::icon_button_svg(
                            "btn-dismiss-fetch-err",
                            crate::icons::X_SVG,
                            i.t("关闭", "Close"),
                            false,
                            &t2,
                            cx,
                            |ws, _, _, cx| {
                                if let Some(d) = ws.ui.provider_dialog.as_mut() {
                                    d.fetch_error = None;
                                }
                                cx.notify();
                            },
                        ))
                );
            }

            // Claude Code Role Mapping Table
            if matches!(tool, ToolId::ClaudeCode | ToolId::ClaudeDesktop) {
                // Table header
                sec = sec.child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(8.0))
                        .px(px(2.0))
                        .text_size(px(11.0))
                        .font_weight(gpui::FontWeight::MEDIUM)
                        .text_color(t.text_secondary)
                        .child(div().w(px(80.0)).child(i.t("模型角色", "Role")))
                        .child(div().flex_1().child(i.t("显示名称", "Display Name")))
                        .child(div().flex_1().child(i.t("实际请求模型", "Request Model")))
                        .child(div().w(px(64.0)).text_center().child(i.t("1M 模式", "1M Mode")))
                );

                // Helper to render role row
                let render_role_row = |role_lbl: &'static str,
                                       target_key: &'static str,
                                       model_ent: gpui::Entity<TextInput>,
                                       display_name_ent: Option<gpui::Entity<TextInput>>,
                                       is_1m: bool,
                                       toggle_1m: Option<Box<dyn Fn(&mut Workspace, &mut Context<Workspace>) + 'static>>,
                                       cx: &mut Context<Workspace>| -> gpui::AnyElement {
                    let t2 = t.clone();
                    let role_badge = div()
                        .w(px(80.0))
                        .h(px(32.0))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(px(6.0))
                        .bg(t2.input_bg)
                        .border_1()
                        .border_color(t2.input_border)
                        .text_size(px(12.0))
                        .font_weight(gpui::FontWeight::MEDIUM)
                        .text_color(t2.text_primary)
                        .child(role_lbl);

                    let display_name_cell = match &display_name_ent {
                        Some(dn) => div().flex_1().child(input_container(&t2, dn.clone())).into_any_element(),
                        None => div()
                            .flex_1()
                            .h(px(32.0))
                            .flex()
                            .items_center()
                            .px(px(8.0))
                            .rounded(px(6.0))
                            .bg(t2.sidebar_bg)
                            .border_1()
                            .border_color(t2.card_border)
                            .text_size(px(11.0))
                            .text_color(t2.text_muted)
                            .child(i.t("后台子代理，不显示在菜单", "Subagent (not in menu)"))
                            .into_any_element(),
                    };

                    let one_m_cell = match toggle_1m {
                        Some(toggle) => {
                            let entity = cx.entity().clone();
                            let toggle = std::rc::Rc::new(toggle);
                            div()
                                .w(px(64.0))
                                .h(px(32.0))
                                .flex()
                                .items_center()
                                .justify_center()
                                .child(
                                    gpui_kit::component::checkbox::Checkbox::new(gpui::SharedString::from(format!("role-1m-{target_key}")))
                                        .checked(is_1m)
                                        .label("1M")
                                        .on_click(move |_checked, _window, cx| {
                                            let toggle = toggle.clone();
                                            entity.update(cx, |ws, cx| {
                                                toggle(ws, cx);
                                            });
                                        })
                                )
                                .into_any_element()
                        }
                        None => div().w(px(64.0)).into_any_element(),
                    };

                    let req_model_cell = render_model_input_with_fetch(
                        target_key.to_string(),
                        model_ent,
                        display_name_ent,
                        cx,
                    );

                    div()
                        .flex()
                        .items_start()
                        .gap(px(8.0))
                        .child(role_badge)
                        .child(display_name_cell)
                        .child(req_model_cell)
                        .child(one_m_cell)
                        .into_any_element()
                };

                // Sonnet
                sec = sec.child(render_role_row(
                    "Sonnet",
                    "sonnet",
                    sonnet_model.clone(),
                    Some(sonnet_name.clone()),
                    sonnet_1m,
                    Some(Box::new(|ws, cx| {
                        if let Some(d) = ws.ui.provider_dialog.as_mut() {
                            d.sonnet_1m = !d.sonnet_1m;
                        }
                        cx.notify();
                    })),
                    cx,
                ));

                // Opus
                sec = sec.child(render_role_row(
                    "Opus",
                    "opus",
                    opus_model.clone(),
                    Some(opus_name.clone()),
                    opus_1m,
                    Some(Box::new(|ws, cx| {
                        if let Some(d) = ws.ui.provider_dialog.as_mut() {
                            d.opus_1m = !d.opus_1m;
                        }
                        cx.notify();
                    })),
                    cx,
                ));

                // Haiku
                sec = sec.child(render_role_row(
                    "Haiku",
                    "haiku",
                    haiku_model.clone(),
                    Some(haiku_name.clone()),
                    haiku_1m,
                    Some(Box::new(|ws, cx| {
                        if let Some(d) = ws.ui.provider_dialog.as_mut() {
                            d.haiku_1m = !d.haiku_1m;
                        }
                        cx.notify();
                    })),
                    cx,
                ));

                // Subagent
                sec = sec.child(render_role_row(
                    "Subagent",
                    "subagent",
                    subagent_model.clone(),
                    None,
                    subagent_1m,
                    Some(Box::new(|ws, cx| {
                        if let Some(d) = ws.ui.provider_dialog.as_mut() {
                            d.subagent_1m = !d.subagent_1m;
                        }
                        cx.notify();
                    })),
                    cx,
                ));

                // Fable
                sec = sec.child(render_role_row(
                    "Fable",
                    "fable",
                    fable_model.clone(),
                    Some(fable_name.clone()),
                    fable_1m,
                    Some(Box::new(|ws, cx| {
                        if let Some(d) = ws.ui.provider_dialog.as_mut() {
                            d.fable_1m = !d.fable_1m;
                        }
                        cx.notify();
                    })),
                    cx,
                ));

                // Divider before fallback model
                sec = sec.child(div().h(px(1.0)).bg(t.card_border).my(px(2.0)));
            }

            // Fallback / Primary Model Field
            let fallback_field = div()
                .flex()
                .flex_col()
                .gap(px(4.0))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .child(field_label(i.t("默认 / 兜底模型 (Fallback Model)", "Default / Fallback Model")))
                        .child({
                            if has_models {
                                div()
                                    .text_size(px(11.0))
                                    .text_color(t.accent)
                                    .font_weight(gpui::FontWeight::MEDIUM)
                                    .child(format!("{} {} {}", i.t("已获取", "Fetched"), fetched_models.len(), i.t("个上游模型", "models")))
                            } else {
                                div()
                            }
                        })
                )
                .child(render_model_input_with_fetch(
                    "primary".to_string(),
                    model.clone(),
                    None,
                    cx,
                ))
                .child(
                    div()
                        .text_size(px(11.0))
                        .text_color(t.text_muted)
                        .child(i.t(
                            "用于未明确落到 Sonnet、Opus、Fable、Haiku 角色的请求。使用第三方中转代理时建议填写。",
                            "Used for requests not mapped to specific roles. Recommended for third-party proxies.",
                        )),
                );

            sec = sec.child(fallback_field);

            // Codex specific fields
            if tool == ToolId::Codex {
                let reasoning_levels = [
                    ("default", i.t("默认", "Default")),
                    ("low", i.t("低 (Low)", "Low")),
                    ("medium", i.t("中 (Medium)", "Medium")),
                    ("high", i.t("高 (High)", "High")),
                ];
                let mut r_row = div().flex().gap(px(6.0)).flex_wrap();
                for (r_val, r_lbl) in reasoning_levels {
                    let is_curr = codex_reasoning_effort == r_val;
                    let r_val2 = r_val.to_string();
                    r_row = r_row.child(button_l(
                        gpui::SharedString::from(format!("reasoning-opt-{r_val}")),
                        r_lbl,
                        if is_curr {
                            ButtonVariant::Primary
                        } else {
                            ButtonVariant::Secondary
                        },
                        &t,
                        cx,
                        move |ws, _ev, _w, cx| {
                            if let Some(dialog) = ws.ui.provider_dialog.as_mut() {
                                dialog.codex_reasoning_effort = r_val2.clone();
                            }
                            cx.notify();
                        },
                    ));
                }

                let wire_apis = [
                    ("responses", i.t("Responses 原生", "Responses Native")),
                    ("chat", i.t("Chat 兼容", "Chat Compatible")),
                ];
                let mut w_row = div().flex().gap(px(6.0)).flex_wrap();
                for (w_val, w_lbl) in wire_apis {
                    let is_curr = codex_wire_api == w_val;
                    let w_val2 = w_val.to_string();
                    w_row = w_row.child(button_l(
                        gpui::SharedString::from(format!("wire-api-opt-{w_val}")),
                        w_lbl,
                        if is_curr {
                            ButtonVariant::Primary
                        } else {
                            ButtonVariant::Secondary
                        },
                        &t,
                        cx,
                        move |ws, _ev, _w, cx| {
                            if let Some(dialog) = ws.ui.provider_dialog.as_mut() {
                                dialog.codex_wire_api = w_val2.clone();
                            }
                            cx.notify();
                        },
                    ));
                }

                sec = sec
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(4.0))
                            .child(field_label(i.t("推理强度 (Reasoning Effort)", "Reasoning Effort")))
                            .child(r_row),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(4.0))
                            .child(field_label(i.t("传输协议 (Wire API)", "Wire API")))
                            .child(w_row),
                    );
            }

            sec.into_any_element()
        }
    };

    // Tab bar for Provider Dialog: Connection & Models vs Network, Headers & Billing
    let tab_bar = crate::components::segmented_tab_bar(
        "modal-tab",
        vec![
            (
                crate::pages::ProviderDialogTab::Connection,
                i.t("核心连接与模型", "Connection & Models"),
            ),
            (
                crate::pages::ProviderDialogTab::Advanced,
                i.t("网络、请求头与计费", "Network & Billing"),
            ),
        ],
        active_tab,
        &t,
        cx,
        |ws, tab, _win, cx| {
            if let Some(d) = ws.ui.provider_dialog.as_mut() {
                d.active_tab = tab;
            }
            cx.notify();
        },
    );

    // Advanced Tab Cards:
    // 1. User-Agent Card
    let ua_card = section_card(i.t("客户端 User-Agent (User-Agent 指纹)", "Client User-Agent"))
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(6.0))
                .child(field_label(i.t(
                    "自定义 User-Agent（用于国内聚合代理 403 白名单校验）",
                    "Custom User-Agent (for proxy 403 allowlist checks)",
                )))
                .child(input_container(&t, custom_user_agent.clone()))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(6.0))
                        .flex_wrap()
                        .child(
                            div()
                                .text_size(px(11.0))
                                .text_color(t.text_secondary)
                                .child(i.t("常用指纹预设:", "Quick presets:")),
                        )
                        .child(button_l(
                            "ua-pill-claude",
                            i.t("Claude Code 官方指纹", "Claude Code Official"),
                            ButtonVariant::Secondary,
                            &t,
                            cx,
                            |ws, _ev, _w, cx| {
                                if let Some(d) = ws.ui.provider_dialog.as_mut() {
                                    d.custom_user_agent.update(cx, |inp, cx| {
                                        inp.set_text_silent("claude-cli/2.1.237 (external, cli)", cx);
                                    });
                                }
                                cx.notify();
                            },
                        ))
                        .child(button_l(
                            "ua-pill-kilo",
                            "Kilo-Code",
                            ButtonVariant::Secondary,
                            &t,
                            cx,
                            |ws, _ev, _w, cx| {
                                if let Some(d) = ws.ui.provider_dialog.as_mut() {
                                    d.custom_user_agent.update(cx, |inp, cx| {
                                        inp.set_text_silent("Kilo-Code/1.0", cx);
                                    });
                                }
                                cx.notify();
                            },
                        ))
                        .child(button_l(
                            "ua-pill-codex",
                            "Codex TUI",
                            ButtonVariant::Secondary,
                            &t,
                            cx,
                            |ws, _ev, _w, cx| {
                                if let Some(d) = ws.ui.provider_dialog.as_mut() {
                                    d.custom_user_agent.update(cx, |inp, cx| {
                                        inp.set_text_silent("codex-tui/0.1", cx);
                                    });
                                }
                                cx.notify();
                            },
                        ))
                        .child(button_l(
                            "ua-pill-clear",
                            i.t("清空", "Clear"),
                            ButtonVariant::Secondary,
                            &t,
                            cx,
                            |ws, _ev, _w, cx| {
                                if let Some(d) = ws.ui.provider_dialog.as_mut() {
                                    d.custom_user_agent.update(cx, |inp, cx| {
                                        inp.set_text_silent("", cx);
                                    });
                                }
                                cx.notify();
                            },
                        )),
                )
                .child(
                    div()
                        .text_size(px(11.0))
                        .text_color(t.text_muted)
                        .child(i.t(
                            "提示: 部分中转站（如 Kimi / 火山 / GLM）仅允许官方白名单 User-Agent。若请求报 403 Forbidden，请直接套用「Claude Code 官方指纹」。",
                            "Tip: Some third-party proxies only allow official User-Agents. If you hit 403 Forbidden, apply the official fingerprint above.",
                        )),
                ),
        );

    // 2. Custom HTTP Headers Card
    let headers_card = {
        let t2 = t.clone();
        let mut card = div()
            .p(px(12.0))
            .rounded(px(8.0))
            .bg(t.sidebar_bg)
            .border_1()
            .border_color(t.card_border)
            .flex()
            .flex_col()
            .gap(px(10.0));

        card = card.child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(6.0))
                        .child(
                            div()
                                .text_size(px(12.5))
                                .font_weight(gpui::FontWeight::SEMIBOLD)
                                .text_color(t.text_primary)
                                .child(i.t("自定义 HTTP 请求头 (Custom HTTP Headers)", "Custom HTTP Headers")),
                        )
                        .child(
                            div()
                                .px(px(6.0))
                                .py(px(1.0))
                                .rounded(px(4.0))
                                .bg(t.accent_subtle)
                                .text_size(px(10.5))
                                .text_color(t.accent)
                                .font_weight(gpui::FontWeight::MEDIUM)
                                .child(format!("{} {}", custom_headers_list.len(), i.t("项", "items"))),
                        ),
                )
                .child(
                    button_with_icon_l(
                        "btn-add-custom-header",
                        crate::icons::PLUS_SVG,
                        i.t("添加 Header", "Add Header"),
                        ButtonVariant::Secondary,
                        &t2,
                        cx,
                        |ws, _ev, _w, cx| {
                            if let Some(d) = ws.ui.provider_dialog.as_mut() {
                                let key = cx.new(|cx| TextInput::new("Header 名称 (如 X-Title)", cx));
                                let value = cx.new(|cx| TextInput::new("Header 对应值", cx));
                                d.custom_headers_list.push(crate::pages::CustomHeaderDraft { key, value });
                            }
                            cx.notify();
                        },
                    ),
                ),
        );

        if custom_headers_list.is_empty() {
            card = card.child(
                div()
                    .py(px(10.0))
                    .px(px(12.0))
                    .rounded(px(6.0))
                    .bg(t2.card_bg)
                    .border_1()
                    .border_color(t2.card_border)
                    .text_size(px(11.5))
                    .text_color(t2.text_muted)
                    .child(i.t(
                        "暂未配置自定义 Header。可点击上方按钮添加，或点击下方常用预设快速填入。",
                        "No custom headers configured. Click button above or use quick presets below.",
                    )),
            );
        } else {
            let mut list_col = div().flex().flex_col().gap(px(6.0));
            for (idx, draft) in custom_headers_list.iter().enumerate() {
                let t3 = t2.clone();
                let k_inp = draft.key.clone();
                let v_inp = draft.value.clone();
                let row = div()
                    .id(gpui::SharedString::from(format!("header-row-{idx}")))
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .child(
                        div()
                            .w(px(240.0))
                            .flex_shrink_0()
                            .child(input_container(&t3, k_inp)),
                    )
                    .child(
                        div()
                            .text_size(px(12.0))
                            .font_weight(gpui::FontWeight::BOLD)
                            .text_color(t3.text_muted)
                            .child(":"),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.0))
                            .child(input_container(&t3, v_inp)),
                    )
                    .child(
                        crate::components::icon_button_svg(
                            format!("del-header-{idx}"),
                            crate::icons::TRASH_SVG,
                            i.t("删除", "Delete"),
                            false,
                            &t3,
                            cx,
                            move |ws, _ev, _w, cx| {
                                if let Some(d) = ws.ui.provider_dialog.as_mut() {
                                    if idx < d.custom_headers_list.len() {
                                        d.custom_headers_list.remove(idx);
                                    }
                                }
                                cx.notify();
                            },
                        ),
                    );
                list_col = list_col.child(row);
            }
            card = card.child(list_col);
        }

        let quick_chips = div()
            .flex()
            .items_center()
            .gap(px(6.0))
            .flex_wrap()
            .child(
                div()
                    .text_size(px(11.0))
                    .text_color(t2.text_secondary)
                    .child(i.t("常用 Header 快速填入:", "Quick Header presets:")),
            )
            .child(button_l(
                "hdr-pill-referer",
                "+ HTTP-Referer",
                ButtonVariant::Secondary,
                &t2,
                cx,
                |ws, _ev, _w, cx| {
                    if let Some(d) = ws.ui.provider_dialog.as_mut() {
                        let key = cx.new(|cx| {
                            let mut inp = TextInput::new("Header 名称", cx);
                            inp.set_text_silent("HTTP-Referer", cx);
                            inp
                        });
                        let value = cx.new(|cx| {
                            let mut inp = TextInput::new("Header 对应值", cx);
                            inp.set_text_silent("https://github.com/aitoolplus", cx);
                            inp
                        });
                        d.custom_headers_list.push(crate::pages::CustomHeaderDraft { key, value });
                    }
                    cx.notify();
                },
            ))
            .child(button_l(
                "hdr-pill-title",
                "+ X-Title",
                ButtonVariant::Secondary,
                &t2,
                cx,
                |ws, _ev, _w, cx| {
                    if let Some(d) = ws.ui.provider_dialog.as_mut() {
                        let key = cx.new(|cx| {
                            let mut inp = TextInput::new("Header 名称", cx);
                            inp.set_text_silent("X-Title", cx);
                            inp
                        });
                        let value = cx.new(|cx| {
                            let mut inp = TextInput::new("Header 对应值", cx);
                            inp.set_text_silent("AIToolPlus", cx);
                            inp
                        });
                        d.custom_headers_list.push(crate::pages::CustomHeaderDraft { key, value });
                    }
                    cx.notify();
                },
            ))
            .child(button_l(
                "hdr-pill-anthropic-version",
                "+ anthropic-version",
                ButtonVariant::Secondary,
                &t2,
                cx,
                |ws, _ev, _w, cx| {
                    if let Some(d) = ws.ui.provider_dialog.as_mut() {
                        let key = cx.new(|cx| {
                            let mut inp = TextInput::new("Header 名称", cx);
                            inp.set_text_silent("anthropic-version", cx);
                            inp
                        });
                        let value = cx.new(|cx| {
                            let mut inp = TextInput::new("Header 对应值", cx);
                            inp.set_text_silent("2023-06-01", cx);
                            inp
                        });
                        d.custom_headers_list.push(crate::pages::CustomHeaderDraft { key, value });
                    }
                    cx.notify();
                },
            ));

        card = card.child(quick_chips).child(
            div()
                .text_size(px(11.0))
                .text_color(t2.text_muted)
                .child(i.t(
                    "说明: 保存时将自动写入 Claude Code 的 CUSTOM_HEADERS 环境变量及 Pi / Codex 的对应协议请求头中。",
                    "Note: Automatically written to Claude Code's CUSTOM_HEADERS env and Pi / Codex request headers.",
                )),
        );

        card
    };

    // 3. Billing & Cost Multiplier Card
    let billing_card = {
        let t2 = t.clone();
        let mut card = div()
            .p(px(12.0))
            .rounded(px(8.0))
            .bg(t.sidebar_bg)
            .border_1()
            .border_color(t.card_border)
            .flex()
            .flex_col()
            .gap(px(10.0));

        card = card.child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(2.0))
                        .child(
                            div()
                                .text_size(px(12.5))
                                .font_weight(gpui::FontWeight::SEMIBOLD)
                                .text_color(t.text_primary)
                                .child(i.t("计费与成本倍率 (Billing & Multiplier)", "Billing & Multiplier")),
                        )
                        .child(
                            div()
                                .text_size(px(11.0))
                                .text_color(t.text_muted)
                                .child(i.t(
                                    "兼容 cc-switch / ai-toolbox 计费倍率与价格换算规则",
                                    "Compatible with cc-switch / ai-toolbox cost multiplier and billing",
                                )),
                        ),
                )
                .child(
                    crate::components::toggle(
                        "prov-billing-toggle",
                        billing_enabled,
                        &t2,
                        cx,
                        move |ws, _ev, _w, cx| {
                            if let Some(d) = ws.ui.provider_dialog.as_mut() {
                                d.billing_enabled = !d.billing_enabled;
                            }
                            cx.notify();
                        },
                    ),
                ),
        );

        if !billing_enabled {
            card = card.child(
                div()
                    .py(px(8.0))
                    .px(px(10.0))
                    .rounded(px(6.0))
                    .bg(t2.card_bg)
                    .border_1()
                    .border_color(t2.card_border)
                    .text_size(px(11.5))
                    .text_color(t2.text_muted)
                    .child(i.t(
                        "计费配置已禁用（遵循官方默认标准价格与全局规则）。点击右上角开关启用自定义计费。",
                        "Billing is disabled (using standard pricing). Click toggle above to enable.",
                    )),
            );
        } else {
            let mut form_col = div().flex().flex_col().gap(px(10.0));

            let multiplier_row = div()
                .flex()
                .flex_col()
                .gap(px(4.0))
                .child(field_label(i.t("成本倍率 (Cost Multiplier)", "Cost Multiplier")))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(8.0))
                        .child(
                            div()
                                .w(px(180.0))
                                .child(input_container(&t2, cost_multiplier.clone())),
                        )
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(4.0))
                                .flex_wrap()
                                .child(button_l(
                                    "mul-pill-10",
                                    i.t("1.0 (原价)", "1.0 (Standard)"),
                                    ButtonVariant::Secondary,
                                    &t2,
                                    cx,
                                    |ws, _ev, _w, cx| {
                                        if let Some(d) = ws.ui.provider_dialog.as_mut() {
                                            d.cost_multiplier.update(cx, |inp, cx| inp.set_text_silent("1.0", cx));
                                        }
                                        cx.notify();
                                    },
                                ))
                                .child(button_l(
                                    "mul-pill-07",
                                    i.t("0.7 (七折)", "0.7 (30% off)"),
                                    ButtonVariant::Secondary,
                                    &t2,
                                    cx,
                                    |ws, _ev, _w, cx| {
                                        if let Some(d) = ws.ui.provider_dialog.as_mut() {
                                            d.cost_multiplier.update(cx, |inp, cx| inp.set_text_silent("0.7", cx));
                                        }
                                        cx.notify();
                                    },
                                ))
                                .child(button_l(
                                    "mul-pill-15",
                                    i.t("1.5 (中转)", "1.5 (Proxy)"),
                                    ButtonVariant::Secondary,
                                    &t2,
                                    cx,
                                    |ws, _ev, _w, cx| {
                                        if let Some(d) = ws.ui.provider_dialog.as_mut() {
                                            d.cost_multiplier.update(cx, |inp, cx| inp.set_text_silent("1.5", cx));
                                        }
                                        cx.notify();
                                    },
                                ))
                                .child(button_l(
                                    "mul-pill-20",
                                    i.t("2.0 (双倍)", "2.0 (Double)"),
                                    ButtonVariant::Secondary,
                                    &t2,
                                    cx,
                                    |ws, _ev, _w, cx| {
                                        if let Some(d) = ws.ui.provider_dialog.as_mut() {
                                            d.cost_multiplier.update(cx, |inp, cx| inp.set_text_silent("2.0", cx));
                                        }
                                        cx.notify();
                                    },
                                )),
                        ),
                );

            let sources = [
                ("inherit", i.t("继承全局 (Global)", "Inherit Global")),
                ("request", i.t("按请求模型 (Request)", "By Request Model")),
                ("response", i.t("按响应模型 (Response)", "By Response Model")),
            ];
            let mut src_row = div().flex().gap(px(6.0)).flex_wrap();
            for (val, label) in sources {
                let is_sel = pricing_model_source == val;
                let val_str = val.to_string();
                src_row = src_row.child(button_l(
                    gpui::SharedString::from(format!("pms-btn-{val}")),
                    label,
                    if is_sel { ButtonVariant::Primary } else { ButtonVariant::Secondary },
                    &t2,
                    cx,
                    move |ws, _ev, _w, cx| {
                        if let Some(d) = ws.ui.provider_dialog.as_mut() {
                            d.pricing_model_source = val_str.clone();
                        }
                        cx.notify();
                    },
                ));
            }

            let src_hint = match pricing_model_source.as_str() {
                "request" => i.t("按客户端请求的模型单价进行计费换算", "Calculates cost using requested model pricing"),
                "response" => i.t("按服务端返回的真实模型单价计费（推荐在配置了模型重写映射时选用）", "Calculates cost using response model pricing (recommended with rewrites)"),
                _ => i.t("继承系统的全局默认计费策略与费率规则", "Follows global default billing rules"),
            };

            let pricing_source_sec = div()
                .flex()
                .flex_col()
                .gap(px(4.0))
                .child(field_label(i.t("计费基准模型源 (Pricing Model Source)", "Pricing Model Source")))
                .child(src_row)
                .child(
                    div()
                        .text_size(px(11.0))
                        .text_color(t2.text_muted)
                        .child(src_hint),
                );

            form_col = form_col.child(multiplier_row).child(pricing_source_sec);
            card = card.child(form_col);
        }

        card
    };

    // 4. Model Rewrites Card
    let rewrites_card = {
        let t2 = t.clone();
        let mut card = div()
            .p(px(12.0))
            .rounded(px(8.0))
            .bg(t.sidebar_bg)
            .border_1()
            .border_color(t.card_border)
            .flex()
            .flex_col()
            .gap(px(10.0));

        card = card.child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(6.0))
                        .child(
                            div()
                                .text_size(px(12.5))
                                .font_weight(gpui::FontWeight::SEMIBOLD)
                                .text_color(t.text_primary)
                                .child(i.t("模型重写映射 (Model Rewrites)", "Model Rewrites")),
                        )
                        .child(
                            div()
                                .px(px(6.0))
                                .py(px(1.0))
                                .rounded(px(4.0))
                                .bg(t.accent_subtle)
                                .text_size(px(10.5))
                                .text_color(t.accent)
                                .font_weight(gpui::FontWeight::MEDIUM)
                                .child(format!("{} {}", model_rewrites.len(), i.t("条规则", "rules"))),
                        ),
                )
                .child(
                    button_with_icon_l(
                        "btn-add-model-rewrite",
                        crate::icons::PLUS_SVG,
                        i.t("添加重写规则", "Add Rewrite Rule"),
                        ButtonVariant::Secondary,
                        &t2,
                        cx,
                        |ws, _ev, _w, cx| {
                            if let Some(d) = ws.ui.provider_dialog.as_mut() {
                                let from = cx.new(|cx| TextInput::new("请求模型 (From，如 haiku)", cx));
                                let to = cx.new(|cx| TextInput::new("重定向至 (To，如 deepseek-chat)", cx));
                                d.model_rewrites.push(crate::pages::ModelRewriteDraft { from, to });
                            }
                            cx.notify();
                        },
                    ),
                ),
        );

        if model_rewrites.is_empty() {
            card = card.child(
                div()
                    .py(px(10.0))
                    .px(px(12.0))
                    .rounded(px(6.0))
                    .bg(t2.card_bg)
                    .border_1()
                    .border_color(t2.card_border)
                    .text_size(px(11.5))
                    .text_color(t2.text_muted)
                    .child(i.t(
                        "暂未配置模型重写映射。常用于将 CLI 内部硬编码的辅助模型（如 haiku、fast 模型）重定向至第三方中转站支持的模型。",
                        "No model rewrites configured. Useful to map hardcoded helper models (e.g. haiku) to proxy models.",
                    )),
            );
        } else {
            let mut list_col = div().flex().flex_col().gap(px(6.0));
            for (idx, draft) in model_rewrites.iter().enumerate() {
                let t3 = t2.clone();
                let from_inp = draft.from.clone();
                let to_inp = draft.to.clone();
                let row = div()
                    .id(gpui::SharedString::from(format!("rewrite-row-{idx}")))
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.0))
                            .child(input_container(&t3, from_inp)),
                    )
                    .child(
                        div()
                            .px(px(4.0))
                            .text_size(px(13.0))
                            .font_weight(gpui::FontWeight::BOLD)
                            .text_color(t3.accent)
                            .child("➔"),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.0))
                            .child(input_container(&t3, to_inp)),
                    )
                    .child(
                        crate::components::icon_button_svg(
                            format!("del-rewrite-{idx}"),
                            crate::icons::TRASH_SVG,
                            i.t("删除", "Delete"),
                            false,
                            &t3,
                            cx,
                            move |ws, _ev, _w, cx| {
                                if let Some(d) = ws.ui.provider_dialog.as_mut() {
                                    if idx < d.model_rewrites.len() {
                                        d.model_rewrites.remove(idx);
                                    }
                                }
                                cx.notify();
                            },
                        ),
                    );
                list_col = list_col.child(row);
            }
            card = card.child(list_col);
        }

        card = card.child(
            div()
                .text_size(px(11.0))
                .text_color(t2.text_muted)
                .child(i.t(
                    "例如: 将「claude-3-5-haiku-20241022」重定向至「deepseek-chat」或「gpt-4o-mini」，避免中转代理报错。",
                    "e.g. Map 'claude-3-5-haiku-20241022' to 'deepseek-chat' or 'gpt-4o-mini' to prevent 400 errors.",
                )),
        );

        card
    };

    // 5. Raw Config Card
    let raw_card = section_card(i.t("原始底层配置代码 (Raw JSON / TOML)", "Raw Config JSON / TOML"))
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(4.0))
                .child(field_label(i.t(
                    "高级用户底层配置（保存时将与上方表单字段自动安全合并）",
                    "Underlying config (merged with form fields automatically on save)",
                )))
                .child(textarea_container(&t, settings.clone())),
        );

    // Scrollable Form Container with Active Tab View
    let mut scrollable_form = div()
        .id("provider-dialog-scroll-container")
        .flex_1()
        .min_h(px(0.0))
        .overflow_y_scroll()
        .pr(px(6.0))
        .flex()
        .flex_col()
        .gap(px(12.0));

    match active_tab {
        crate::pages::ProviderDialogTab::Connection => {
            scrollable_form = scrollable_form
                .child(preset_bar)
                .child(basic_section)
                .child(connection_section)
                .child(model_section);
        }
        crate::pages::ProviderDialogTab::Advanced => {
            scrollable_form = scrollable_form
                .child(ua_card)
                .child(headers_card)
                .child(billing_card)
                .child(rewrites_card)
                .child(raw_card);
        }
    }

    // Fixed Footer Bar
    let dialog_clone = ws.ui.provider_dialog.clone();
    let footer_bar = div()
        .pt(px(12.0))
        .border_t_1()
        .border_color(t.card_border)
        .bg(t.card_bg)
        .flex_shrink_0()
        .flex()
        .items_center()
        .justify_between()
        .child(
            div()
                .text_size(px(11.5))
                .text_color(t.text_muted)
                .child(if let Some(p_idx) = preset_index {
                    let p_name = presets.get(p_idx).map(|p| p.name).unwrap_or("");
                    format!("{} {}", i.t("已选用预设:", "Selected preset:"), p_name)
                } else {
                    "".to_string()
                }),
        )
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(8.0))
                .child(button_l(
                    "prov-cancel-btn",
                    i.t("取消", "Cancel"),
                    ButtonVariant::Secondary,
                    &t,
                    cx,
                    |ws, _ev, _w, cx| {
                        ws.ui.provider_dialog = None;
                        cx.notify();
                    },
                ))
                .child(button_with_icon_l(
                    "prov-save-btn",
                    crate::icons::CHECK_SVG,
                    i.t("保存配置", "Save Configuration"),
                    ButtonVariant::Primary,
                    &t,
                    cx,
                    move |ws, _ev, _w, cx| {
                        if let Some(dialog) = dialog_clone.clone() {
                            save_provider(dialog, ws, cx);
                        }
                    },
                )),
        );

    let content = div()
        .flex()
        .flex_col()
        .flex_1()
        .min_h(px(0.0))
        .gap(px(10.0))
        .child(tab_bar)
        .child(scrollable_form)
        .child(footer_bar);

    modal_scaffold_custom(
        &t,
        &title,
        px(780.0),
        content.into_any_element(),
        cx,
        |ws, _, _, cx| {
            ws.ui.provider_dialog = None;
            cx.notify();
        },
    )
}

pub struct ProviderFormData<'a> {
    pub tool: ToolId,
    pub category: &'a str,
    pub base_url: &'a str,
    pub api_key: &'a str,
    pub api_format: &'a str,
    pub model: &'a str,
    pub sonnet_model: &'a str,
    pub sonnet_name: &'a str,
    pub opus_model: &'a str,
    pub opus_name: &'a str,
    pub haiku_model: &'a str,
    pub haiku_name: &'a str,
    pub fable_model: &'a str,
    pub fable_name: &'a str,
    pub subagent_model: &'a str,
    pub sonnet_1m: bool,
    pub opus_1m: bool,
    pub haiku_1m: bool,
    pub fable_1m: bool,
    pub subagent_1m: bool,
    pub pi_provider_key: &'a str,
    pub pi_api_format: &'a str,
    pub pi_models: &'a [Value],
    pub codex_wire_api: &'a str,
    pub codex_reasoning_effort: &'a str,
    pub custom_user_agent: &'a str,
    pub custom_headers: &'a str,
    pub headers_map: &'a [(String, String)],
    pub raw_settings_json: &'a str,
}

fn build_provider_settings(form: &ProviderFormData<'_>) -> Result<String, String> {
    let ProviderFormData {
        tool,
        category,
        base_url,
        api_key,
        api_format,
        model,
        sonnet_model,
        sonnet_name,
        opus_model,
        opus_name,
        haiku_model,
        haiku_name,
        fable_model,
        fable_name,
        subagent_model,
        sonnet_1m,
        opus_1m,
        haiku_1m,
        fable_1m,
        subagent_1m,
        pi_provider_key,
        pi_api_format,
        pi_models,
        codex_wire_api,
        codex_reasoning_effort,
        custom_user_agent,
        custom_headers,
        headers_map,
        raw_settings_json,
    } = *form;

    if category == "official" {
        let mut val: Value = serde_json::from_str(raw_settings_json)
            .unwrap_or_else(|_| serde_json::json!({ "env": {} }));
        if let Some(env) = val.get_mut("env").and_then(Value::as_object_mut) {
            if !model.trim().is_empty() {
                env.insert("ANTHROPIC_MODEL".into(), Value::String(model.trim().into()));
            }
            env.remove("ANTHROPIC_AUTH_TOKEN");
            env.remove("ANTHROPIC_API_KEY");
            env.remove("ANTHROPIC_BASE_URL");
        }
        return Ok(serde_json::to_string_pretty(&val).unwrap_or_default());
    }

    match tool {
        ToolId::ClaudeCode | ToolId::ClaudeDesktop => {
            let mut val: Value = serde_json::from_str(raw_settings_json)
                .unwrap_or_else(|_| serde_json::json!({ "env": {} }));
            if !val.is_object() {
                val = serde_json::json!({ "env": {} });
            }
            if val.get("env").is_none() {
                if let Some(obj) = val.as_object_mut() {
                    obj.insert("env".into(), serde_json::json!({}));
                }
            }
            if let Some(env) = val.get_mut("env").and_then(Value::as_object_mut) {
                if !base_url.trim().is_empty() {
                    env.insert("ANTHROPIC_BASE_URL".into(), Value::String(base_url.trim().into()));
                } else {
                    env.remove("ANTHROPIC_BASE_URL");
                }

                if !api_key.trim().is_empty() {
                    env.insert("ANTHROPIC_AUTH_TOKEN".into(), Value::String(api_key.trim().into()));
                } else {
                    env.remove("ANTHROPIC_AUTH_TOKEN");
                }

                if !model.trim().is_empty() {
                    env.insert("ANTHROPIC_MODEL".into(), Value::String(model.trim().into()));
                } else {
                    env.remove("ANTHROPIC_MODEL");
                }

                if !sonnet_model.trim().is_empty() {
                    let m = if sonnet_1m {
                        format!("{}[1M]", sonnet_model.trim())
                    } else {
                        sonnet_model.trim().to_string()
                    };
                    env.insert("ANTHROPIC_DEFAULT_SONNET_MODEL".into(), Value::String(m));
                } else {
                    env.remove("ANTHROPIC_DEFAULT_SONNET_MODEL");
                }
                if !sonnet_name.trim().is_empty() {
                    env.insert("ANTHROPIC_DEFAULT_SONNET_MODEL_NAME".into(), Value::String(sonnet_name.trim().into()));
                } else {
                    env.remove("ANTHROPIC_DEFAULT_SONNET_MODEL_NAME");
                }

                if !opus_model.trim().is_empty() {
                    let m = if opus_1m {
                        format!("{}[1M]", opus_model.trim())
                    } else {
                        opus_model.trim().to_string()
                    };
                    env.insert("ANTHROPIC_DEFAULT_OPUS_MODEL".into(), Value::String(m));
                } else {
                    env.remove("ANTHROPIC_DEFAULT_OPUS_MODEL");
                }
                if !opus_name.trim().is_empty() {
                    env.insert("ANTHROPIC_DEFAULT_OPUS_MODEL_NAME".into(), Value::String(opus_name.trim().into()));
                } else {
                    env.remove("ANTHROPIC_DEFAULT_OPUS_MODEL_NAME");
                }

                if !haiku_model.trim().is_empty() {
                    let m = if haiku_1m {
                        format!("{}[1M]", haiku_model.trim())
                    } else {
                        haiku_model.trim().to_string()
                    };
                    env.insert("ANTHROPIC_DEFAULT_HAIKU_MODEL".into(), Value::String(m));
                } else {
                    env.remove("ANTHROPIC_DEFAULT_HAIKU_MODEL");
                }
                if !haiku_name.trim().is_empty() {
                    env.insert("ANTHROPIC_DEFAULT_HAIKU_MODEL_NAME".into(), Value::String(haiku_name.trim().into()));
                } else {
                    env.remove("ANTHROPIC_DEFAULT_HAIKU_MODEL_NAME");
                }

                if !fable_model.trim().is_empty() {
                    let m = if fable_1m {
                        format!("{}[1M]", fable_model.trim())
                    } else {
                        fable_model.trim().to_string()
                    };
                    env.insert("ANTHROPIC_DEFAULT_FABLE_MODEL".into(), Value::String(m));
                } else {
                    env.remove("ANTHROPIC_DEFAULT_FABLE_MODEL");
                }
                if !fable_name.trim().is_empty() {
                    env.insert("ANTHROPIC_DEFAULT_FABLE_MODEL_NAME".into(), Value::String(fable_name.trim().into()));
                } else {
                    env.remove("ANTHROPIC_DEFAULT_FABLE_MODEL_NAME");
                }

                if !subagent_model.trim().is_empty() {
                    let m = if subagent_1m {
                        format!("{}[1M]", subagent_model.trim())
                    } else {
                        subagent_model.trim().to_string()
                    };
                    env.insert("CLAUDE_CODE_SUBAGENT_MODEL".into(), Value::String(m));
                } else {
                    env.remove("CLAUDE_CODE_SUBAGENT_MODEL");
                }

                if api_format != "anthropic" && !api_format.trim().is_empty() {
                    env.insert("API_FORMAT".into(), Value::String(api_format.trim().into()));
                } else {
                    env.remove("API_FORMAT");
                }

                if !custom_user_agent.trim().is_empty() {
                    env.insert("USER_AGENT".into(), Value::String(custom_user_agent.trim().into()));
                    env.insert("ANTHROPIC_USER_AGENT".into(), Value::String(custom_user_agent.trim().into()));
                } else {
                    env.remove("USER_AGENT");
                    env.remove("ANTHROPIC_USER_AGENT");
                }

                if !custom_headers.trim().is_empty() {
                    env.insert("CUSTOM_HEADERS".into(), Value::String(custom_headers.trim().into()));
                    env.insert("ANTHROPIC_CUSTOM_HEADERS".into(), Value::String(custom_headers.trim().into()));
                } else {
                    env.remove("CUSTOM_HEADERS");
                    env.remove("ANTHROPIC_CUSTOM_HEADERS");
                }
            }
            Ok(serde_json::to_string_pretty(&val).unwrap_or_default())
        }
        ToolId::Codex => {
            let val: Value = serde_json::from_str(raw_settings_json).unwrap_or_else(|_| serde_json::json!({}));
            let existing_toml = val.get("toml").and_then(Value::as_str).unwrap_or(raw_settings_json);
            let mut doc = existing_toml.parse::<toml_edit::DocumentMut>().unwrap_or_default();
            if !model.trim().is_empty() {
                doc.insert("model", toml_edit::value(model.trim()));
            }
            if codex_reasoning_effort != "default" && !codex_reasoning_effort.trim().is_empty() {
                doc.insert("model_reasoning_effort", toml_edit::value(codex_reasoning_effort.trim()));
            }
            if !codex_wire_api.trim().is_empty() {
                doc.insert("wire_api", toml_edit::value(codex_wire_api.trim()));
            }
            if !base_url.trim().is_empty() || !api_key.trim().is_empty() {
                let mp = doc.entry("model_providers").or_insert(toml_edit::Item::Table(toml_edit::Table::new()));
                if let Some(mp_tbl) = mp.as_table_mut() {
                    let custom_p = mp_tbl.entry("custom").or_insert(toml_edit::Item::Table(toml_edit::Table::new()));
                    if let Some(cp_tbl) = custom_p.as_table_mut() {
                        if !base_url.trim().is_empty() {
                            cp_tbl.insert("base_url", toml_edit::value(base_url.trim()));
                        }
                        if !api_key.trim().is_empty() {
                            cp_tbl.insert("api_key", toml_edit::value(api_key.trim()));
                        }
                        cp_tbl.insert("wire_api", toml_edit::value(codex_wire_api.trim()));
                    }
                }
                doc.insert("model_provider", toml_edit::value("custom"));
            }
            let toml_str = doc.to_string();
            let mut obj = serde_json::Map::new();
            obj.insert("toml".into(), Value::String(toml_str));
            Ok(serde_json::to_string_pretty(&Value::Object(obj)).unwrap_or_default())
        }
        ToolId::GeminiCli => {
            let mut val: Value = serde_json::from_str(raw_settings_json).unwrap_or_else(|_| serde_json::json!({ "env": {} }));
            if let Some(env) = val.get_mut("env").and_then(Value::as_object_mut) {
                if !api_key.trim().is_empty() {
                    env.insert("GEMINI_API_KEY".into(), Value::String(api_key.trim().into()));
                }
                if !base_url.trim().is_empty() {
                    env.insert("GOOGLE_GEMINI_BASE_URL".into(), Value::String(base_url.trim().into()));
                }
                if !model.trim().is_empty() {
                    env.insert("GEMINI_MODEL".into(), Value::String(model.trim().into()));
                }
            }
            Ok(serde_json::to_string_pretty(&val).unwrap_or_default())
        }
        ToolId::Pi | ToolId::OhMyPi => {
            let mut val: Value = serde_json::from_str(raw_settings_json).unwrap_or_else(|_| serde_json::json!({}));
            if !val.is_object() {
                val = serde_json::json!({});
            }
            if let Some(obj) = val.as_object_mut() {
                if !base_url.trim().is_empty() {
                    obj.insert("baseUrl".into(), Value::String(base_url.trim().into()));
                } else {
                    obj.remove("baseUrl");
                }
                if !api_key.trim().is_empty() {
                    obj.insert("apiKey".into(), Value::String(api_key.trim().into()));
                } else {
                    obj.remove("apiKey");
                }
                if !pi_api_format.trim().is_empty() {
                    obj.insert("api".into(), Value::String(pi_api_format.trim().into()));
                }
                if !pi_provider_key.trim().is_empty() {
                    obj.insert("_providerKey".into(), Value::String(pi_provider_key.trim().into()));
                }
                obj.insert("models".into(), Value::Array(pi_models.to_vec()));
                if !headers_map.is_empty() || !custom_user_agent.trim().is_empty() {
                    let mut h_obj = serde_json::Map::new();
                    if !custom_user_agent.trim().is_empty() {
                        h_obj.insert("User-Agent".into(), Value::String(custom_user_agent.trim().into()));
                    }
                    for (k, v) in headers_map {
                        if !k.trim().is_empty() && !v.trim().is_empty() {
                            h_obj.insert(k.trim().into(), Value::String(v.trim().into()));
                        }
                    }
                    if !h_obj.is_empty() {
                        obj.insert("headers".into(), Value::Object(h_obj));
                    } else {
                        obj.remove("headers");
                    }
                }
            }
            Ok(serde_json::to_string_pretty(&val).unwrap_or_default())
        }
        _ => {
            let mut val: Value = serde_json::from_str(raw_settings_json).unwrap_or_else(|_| serde_json::json!({}));
            if let Some(obj) = val.as_object_mut() {
                if !base_url.trim().is_empty() {
                    obj.insert("baseUrl".into(), Value::String(base_url.trim().into()));
                }
                if !api_key.trim().is_empty() {
                    obj.insert("apiKey".into(), Value::String(api_key.trim().into()));
                }
                if !model.trim().is_empty() {
                    obj.insert("model".into(), Value::String(model.trim().into()));
                }
            }
            Ok(serde_json::to_string_pretty(&val).unwrap_or_default())
        }
    }
}

fn save_provider(
    state: ProviderDialogState,
    ws: &mut Workspace,
    cx: &mut Context<Workspace>,
) {
    let i = ws.i18n;
    let tool = state.tool;
    let editing_id = state.editing_id.clone();
    let name_txt: String = state.name.update(cx, |inp, _| inp.text().trim().to_string());
    let category: String = state.category.clone();
    let base_url_txt: String = state.base_url.update(cx, |inp, _| inp.text().trim().to_string());
    let api_key_txt: String = state.api_key.update(cx, |inp, _| inp.text().trim().to_string());
    let api_format: String = state.api_format.clone();
    let model_txt: String = state.model.update(cx, |inp, _| inp.text().trim().to_string());
    let sonnet_txt: String = state.sonnet_model.update(cx, |inp, _| inp.text().trim().to_string());
    let sonnet_name_txt: String = state.sonnet_name.update(cx, |inp, _| inp.text().trim().to_string());
    let opus_txt: String = state.opus_model.update(cx, |inp, _| inp.text().trim().to_string());
    let opus_name_txt: String = state.opus_name.update(cx, |inp, _| inp.text().trim().to_string());
    let haiku_txt: String = state.haiku_model.update(cx, |inp, _| inp.text().trim().to_string());
    let haiku_name_txt: String = state.haiku_name.update(cx, |inp, _| inp.text().trim().to_string());
    let fable_txt: String = state.fable_model.update(cx, |inp, _| inp.text().trim().to_string());
    let fable_name_txt: String = state.fable_name.update(cx, |inp, _| inp.text().trim().to_string());
    let subagent_model_txt: String = state.subagent_model.update(cx, |inp, _| inp.text().trim().to_string());
    let sonnet_1m: bool = state.sonnet_1m;
    let opus_1m: bool = state.opus_1m;
    let haiku_1m: bool = state.haiku_1m;
    let fable_1m: bool = state.fable_1m;
    let subagent_1m: bool = state.subagent_1m;
    let pi_provider_key_txt: String = state.pi_provider_key.update(cx, |inp, _| inp.text().trim().to_string());
    let pi_api_format: String = state.pi_api_format.clone();
    let codex_wire_api: String = state.codex_wire_api.clone();
    let codex_reasoning_effort: String = state.codex_reasoning_effort.clone();
    let custom_headers_txt: String = state.custom_headers.update(cx, |inp, _| inp.text().trim().to_string());
    let notes_txt: String = state.notes.update(cx, |inp, _| inp.text().trim().to_string());
    let website_txt: String = state.website.update(cx, |inp, _| inp.text().trim().to_string());
    let raw_settings_txt: String = state.settings.update(cx, |ta, _| ta.text().trim().to_string());

    // Advanced & Meta configuration
    let custom_user_agent_txt = state.custom_user_agent.update(cx, |inp, _| inp.text().trim().to_string());
    let mut custom_headers_items = Vec::new();
    let mut headers_kv = Vec::new();
    for draft in &state.custom_headers_list {
        let k = draft.key.update(cx, |inp, _| inp.text().trim().to_string());
        let v = draft.value.update(cx, |inp, _| inp.text().trim().to_string());
        if !k.is_empty() || !v.is_empty() {
            custom_headers_items.push(aitoolplus_core::providers::CustomHeaderItem {
                name: k.clone(),
                value: v.clone(),
            });
            if !k.is_empty() && !v.is_empty() {
                headers_kv.push((k, v));
            }
        }
    }
    let billing_enabled = state.billing_enabled;
    let cost_multiplier_txt = state.cost_multiplier.update(cx, |inp, _| inp.text().trim().to_string());
    let pricing_model_source = state.pricing_model_source.clone();

    let mut model_rewrites = Vec::new();
    for draft in &state.model_rewrites {
        let from = draft.from.update(cx, |inp, _| inp.text().trim().to_string());
        let to = draft.to.update(cx, |inp, _| inp.text().trim().to_string());
        if !from.is_empty() || !to.is_empty() {
            model_rewrites.push(aitoolplus_core::providers::ModelRewriteRule {
                from,
                to,
            });
        }
    }

    let meta = aitoolplus_core::providers::ProviderMeta {
        custom_user_agent: (!custom_user_agent_txt.is_empty()).then(|| custom_user_agent_txt.clone()),
        custom_headers: (!custom_headers_items.is_empty()).then(|| custom_headers_items),
        billing_enabled: billing_enabled.then_some(true),
        cost_multiplier: (!cost_multiplier_txt.is_empty()).then(|| cost_multiplier_txt),
        pricing_model_source: (pricing_model_source != "inherit").then(|| pricing_model_source),
        model_rewrites: (!model_rewrites.is_empty()).then(|| model_rewrites),
    };

    let mut env_headers = Vec::new();
    for (k, v) in &headers_kv {
        env_headers.push(format!("{k}: {v}"));
    }
    if !custom_headers_txt.is_empty() && !env_headers.iter().any(|h| h == &custom_headers_txt) {
        env_headers.push(custom_headers_txt);
    }
    let effective_custom_headers = env_headers.join(", ");

    if name_txt.is_empty() {
        let msg = i.t("供应商名称不能为空", "Provider name is required").to_string();
        ws.ui.toast(msg, true);
        cx.notify();
        return;
    }

    let is_pi = matches!(tool, ToolId::Pi | ToolId::OhMyPi);
    let mut pi_models_json = Vec::new();
    if is_pi {
        for draft in &state.pi_models {
            let m_id = draft.id.update(cx, |inp, _| inp.text().trim().to_string());
            if m_id.is_empty() {
                continue;
            }
            let m_name = draft.name.update(cx, |inp, _| inp.text().trim().to_string());
            let mut m_obj = serde_json::Map::new();
            m_obj.insert("id".into(), Value::String(m_id));
            if !m_name.is_empty() {
                m_obj.insert("name".into(), Value::String(m_name));
            }
            if draft.reasoning {
                m_obj.insert("reasoning".into(), Value::Bool(true));
            }
            let mut inputs = vec![Value::String("text".into())];
            if draft.image_input {
                inputs.push(Value::String("image".into()));
            }
            m_obj.insert("input".into(), Value::Array(inputs));
            let cw = draft.context_window.update(cx, |inp, _| inp.text().trim().to_string());
            if let Ok(cw_num) = cw.parse::<u64>() {
                m_obj.insert("contextWindow".into(), Value::Number(cw_num.into()));
            }
            let mt = draft.max_tokens.update(cx, |inp, _| inp.text().trim().to_string());
            if let Ok(mt_num) = mt.parse::<u64>() {
                m_obj.insert("maxTokens".into(), Value::Number(mt_num.into()));
            }
            pi_models_json.push(Value::Object(m_obj));
        }

        if pi_models_json.is_empty() {
            let msg = i.t("请至少填写一个模型 ID", "Please configure at least one model ID").to_string();
            ws.ui.toast(msg, true);
            cx.notify();
            return;
        }
    }

    let form_data = ProviderFormData {
        tool,
        category: &category,
        base_url: &base_url_txt,
        api_key: &api_key_txt,
        api_format: &api_format,
        model: &model_txt,
        sonnet_model: &sonnet_txt,
        sonnet_name: &sonnet_name_txt,
        opus_model: &opus_txt,
        opus_name: &opus_name_txt,
        haiku_model: &haiku_txt,
        haiku_name: &haiku_name_txt,
        fable_model: &fable_txt,
        fable_name: &fable_name_txt,
        subagent_model: &subagent_model_txt,
        sonnet_1m,
        opus_1m,
        haiku_1m,
        fable_1m,
        subagent_1m,
        pi_provider_key: &pi_provider_key_txt,
        pi_api_format: &pi_api_format,
        pi_models: &pi_models_json,
        codex_wire_api: &codex_wire_api,
        codex_reasoning_effort: &codex_reasoning_effort,
        custom_user_agent: &custom_user_agent_txt,
        custom_headers: &effective_custom_headers,
        headers_map: &headers_kv,
        raw_settings_json: &raw_settings_txt,
    };

    let settings_txt = match build_provider_settings(&form_data) {
        Ok(s) => s,
        Err(e) => {
            let msg = format!("{}: {}", i.t("构建配置失败", "Failed to build settings"), e);
            ws.ui.toast(msg, true);
            cx.notify();
            return;
        }
    };

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
                    p.set_meta(&meta);
                });
            }
            None => {
                let mut p = aitoolplus_core::providers::ProviderRecord::new(
                    name_txt.clone(),
                    category.clone(),
                );
                if is_pi {
                    let key = if !pi_provider_key_txt.is_empty() {
                        pi_provider_key_txt.clone()
                    } else {
                        name_txt.to_lowercase().chars().filter(|c| c.is_alphanumeric() || *c == '-').collect()
                    };
                    let key = if key.is_empty() { "custom".to_string() } else { key };
                    p.id = format!("{}:{}", if tool == ToolId::Pi { "pi" } else { "omp" }, key);
                }
                p.settings_config = settings_txt.clone();
                p.notes = (!notes_txt.is_empty()).then(|| notes_txt.clone());
                p.website_url = (!website_txt.is_empty()).then(|| website_txt.clone());
                p.set_meta(&meta);
                section.providers.push(p);
            }
        }
    });
    ws.persist_store();

    // Re-apply if this was the active provider for Pi / OhMyPi
    if tool == ToolId::Pi {
        if let Some(applied) = ws.store.store().tool(tool).providers.iter().find(|p| p.is_applied).cloned() {
            let _ = aitoolplus_core::pi_runtime::apply_provider(&ws.paths, &applied);
        }
    } else if tool == ToolId::OhMyPi {
        if let Some(applied) = ws.store.store().tool(tool).providers.iter().find(|p| p.is_applied).cloned() {
            let omp_paths = aitoolplus_core::oh_my_pi::OmpRuntimePaths::from_paths(&ws.paths);
            let _ = aitoolplus_core::oh_my_pi::apply_provider(&omp_paths, &applied);
        }
    }

    ws.ui.provider_dialog = None;
    let msg = i.t("供应商已保存", "Provider saved").to_string();
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
        .children((editing_id.is_none()).then(|| {
            let mut row = div().flex().items_center().gap(px(6.0)).flex_wrap();
            row = row.child(div().text_size(px(11.5)).text_color(t.text_secondary).child(i.t("快捷填入预设：", "Quick Presets:")));
            for preset in BUILTIN_PROMPTS {
                let p_name = preset.name;
                let p_content = preset.content;
                let name_clone = name.clone();
                let content_clone = content.clone();
                row = row.child(button_l(
                    gpui::SharedString::from(format!("dlg-preset-{}", preset.id)),
                    p_name,
                    ButtonVariant::Secondary,
                    &t,
                    cx,
                    move |_ws, _, _, cx| {
                        name_clone.update(cx, |inp, cx| inp.set_text_silent(p_name.to_string(), cx));
                        content_clone.update(cx, |ta, cx| ta.set_text_silent(p_content.to_string(), cx));
                        cx.notify();
                    },
                ));
            }
            row.into_any_element()
        }))
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    #[test]
    fn test_claude_code_provider_settings_building() {
        let form = ProviderFormData {
            tool: ToolId::ClaudeCode,
            category: "custom",
            base_url: "https://api.krill.io/v1",
            api_key: "sk-krill-secret",
            api_format: "openai",
            model: "",
            sonnet_model: "anthropic/claude-3-7-sonnet",
            sonnet_name: "Krill Sonnet 3.7",
            opus_model: "anthropic/claude-3-opus",
            opus_name: "Krill Opus",
            haiku_model: "anthropic/claude-3-5-haiku",
            haiku_name: "Krill Haiku",
            fable_model: "",
            fable_name: "",
            subagent_model: "anthropic/claude-3-5-haiku",
            sonnet_1m: true,
            opus_1m: false,
            haiku_1m: false,
            fable_1m: false,
            subagent_1m: true,
            pi_provider_key: "",
            pi_api_format: "",
            pi_models: &[],
            codex_wire_api: "",
            codex_reasoning_effort: "",
            custom_user_agent: "claude-cli/2.1.237 (external, cli)",
            custom_headers: "X-Krill-Custom: 123",
            headers_map: &[],
            raw_settings_json: "{}",
        };

        let settings_str = build_provider_settings(&form).expect("should build Claude settings");
        let parsed: Value = serde_json::from_str(&settings_str).expect("must be valid JSON");
        let env = parsed.get("env").expect("must contain env object");

        assert_eq!(
            env.get("USER_AGENT").and_then(Value::as_str),
            Some("claude-cli/2.1.237 (external, cli)")
        );
        assert_eq!(
            env.get("CUSTOM_HEADERS").and_then(Value::as_str),
            Some("X-Krill-Custom: 123")
        );
        assert_eq!(
            env.get("ANTHROPIC_BASE_URL").and_then(Value::as_str),
            Some("https://api.krill.io/v1")
        );
        assert_eq!(
            env.get("ANTHROPIC_AUTH_TOKEN").and_then(Value::as_str),
            Some("sk-krill-secret")
        );
        assert_eq!(
            env.get("ANTHROPIC_DEFAULT_SONNET_MODEL").and_then(Value::as_str),
            Some("anthropic/claude-3-7-sonnet[1M]"),
            "1M suffix should be appended when sonnet_1m is true"
        );
        assert_eq!(
            env.get("ANTHROPIC_DEFAULT_SONNET_MODEL_NAME").and_then(Value::as_str),
            Some("Krill Sonnet 3.7")
        );
        assert_eq!(
            env.get("ANTHROPIC_DEFAULT_OPUS_MODEL").and_then(Value::as_str),
            Some("anthropic/claude-3-opus")
        );
        assert_eq!(
            env.get("CLAUDE_CODE_SUBAGENT_MODEL").and_then(Value::as_str),
            Some("anthropic/claude-3-5-haiku[1M]")
        );
        assert_eq!(
            env.get("API_FORMAT").and_then(Value::as_str),
            Some("openai")
        );
        assert_eq!(
            env.get("CUSTOM_HEADERS").and_then(Value::as_str),
            Some("X-Krill-Custom: 123")
        );
    }

    #[test]
    fn test_pi_provider_settings_building() {
        let pi_models = vec![
            serde_json::json!({
                "id": "deepseek-chat",
                "name": "DeepSeek V3",
                "reasoning": false,
                "input": ["text"],
                "contextWindow": 128000,
                "maxTokens": 16384
            }),
            serde_json::json!({
                "id": "deepseek-reasoner",
                "name": "DeepSeek R1",
                "reasoning": true,
                "input": ["text", "image"],
                "contextWindow": 64000,
                "maxTokens": 8192
            }),
        ];

        let form = ProviderFormData {
            tool: ToolId::Pi,
            category: "custom",
            base_url: "https://api.deepseek.com",
            api_key: "sk-deepseek-test",
            api_format: "openai",
            model: "",
            sonnet_model: "",
            sonnet_name: "",
            opus_model: "",
            opus_name: "",
            haiku_model: "",
            haiku_name: "",
            fable_model: "",
            fable_name: "",
            subagent_model: "",
            sonnet_1m: false,
            opus_1m: false,
            haiku_1m: false,
            fable_1m: false,
            subagent_1m: false,
            pi_provider_key: "deepseek-test",
            pi_api_format: "openai-completions",
            pi_models: &pi_models,
            codex_wire_api: "",
            codex_reasoning_effort: "",
            custom_user_agent: "Kilo-Code/1.0",
            custom_headers: "",
            headers_map: &[("X-Title".to_string(), "DeepSeekApp".to_string())],
            raw_settings_json: "{}",
        };

        let settings_str = build_provider_settings(&form).expect("should build Pi settings");
        let parsed: Value = serde_json::from_str(&settings_str).expect("must be valid JSON");

        assert_eq!(
            parsed.get("baseUrl").and_then(Value::as_str),
            Some("https://api.deepseek.com")
        );
        assert_eq!(
            parsed.get("apiKey").and_then(Value::as_str),
            Some("sk-deepseek-test")
        );
        assert_eq!(
            parsed.get("api").and_then(Value::as_str),
            Some("openai-completions")
        );
        assert_eq!(
            parsed.get("_providerKey").and_then(Value::as_str),
            Some("deepseek-test")
        );

        let headers = parsed.get("headers").expect("headers object");
        assert_eq!(headers.get("User-Agent").and_then(Value::as_str), Some("Kilo-Code/1.0"));
        assert_eq!(headers.get("X-Title").and_then(Value::as_str), Some("DeepSeekApp"));

        let models = parsed.get("models").and_then(Value::as_array).expect("models array");
        assert_eq!(models.len(), 2);
        assert_eq!(models[0]["id"], "deepseek-chat");
        assert_eq!(models[0]["name"], "DeepSeek V3");
        assert_eq!(models[0]["contextWindow"], 128000);
        assert_eq!(models[1]["id"], "deepseek-reasoner");
        assert_eq!(models[1]["reasoning"], true);
        assert_eq!(models[1]["input"], serde_json::json!(["text", "image"]));
    }

    #[test]
    fn test_model_search_filtering_logic() {
        let models = vec![
            aitoolplus_core::api_hub::FetchedModel {
                id: "anthropic/claude-3-7-sonnet".to_string(),
                display_name: Some("Claude 3.7 Sonnet".to_string()),
                owned_by: Some("Anthropic".to_string()),
                ..Default::default()
            },
            aitoolplus_core::api_hub::FetchedModel {
                id: "deepseek-ai/DeepSeek-V3".to_string(),
                display_name: Some("DeepSeek V3".to_string()),
                owned_by: Some("DeepSeek".to_string()),
                ..Default::default()
            },
        ];

        let query = "sonnet".to_lowercase();
        let matched: Vec<_> = models
            .iter()
            .filter(|m| {
                m.id.to_lowercase().contains(&query)
                    || m.owned_by.as_deref().unwrap_or("").to_lowercase().contains(&query)
                    || m.display_name.as_deref().unwrap_or("").to_lowercase().contains(&query)
            })
            .collect();

        assert_eq!(matched.len(), 1);
        assert_eq!(matched[0].id, "anthropic/claude-3-7-sonnet");

        let query_vendor = "deepseek".to_lowercase();
        let matched_vendor: Vec<_> = models
            .iter()
            .filter(|m| {
                m.id.to_lowercase().contains(&query_vendor)
                    || m.owned_by.as_deref().unwrap_or("").to_lowercase().contains(&query_vendor)
                    || m.display_name.as_deref().unwrap_or("").to_lowercase().contains(&query_vendor)
            })
            .collect();

        assert_eq!(matched_vendor.len(), 1);
        assert_eq!(matched_vendor[0].id, "deepseek-ai/DeepSeek-V3");
    }
}

