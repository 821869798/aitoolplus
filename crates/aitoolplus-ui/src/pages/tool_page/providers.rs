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

pub(super) fn providers_section(
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
                super::provider_dialog::open_provider_dialog(None, tool, ws, cx);
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

pub(super) fn card_icon_btn(
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

pub(super) fn extract_provider_subtitle(tool: ToolId, p: &ProviderRecord, _i: &crate::i18n::I18n) -> String {
    // 1. 优先显示 备注
    if let Some(ref n) = p.notes {
        let trimmed = n.trim();
        if !trimmed.is_empty() {
            return trimmed.to_string();
        }
    }

    // 2. 其次显示 官网链接
    if let Some(ref w) = p.website_url {
        let trimmed = w.trim();
        if !trimmed.is_empty() {
            return trimmed.to_string();
        }
    }
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
            ToolId::Agents => "",
        };
        if !official_url.is_empty() {
            return official_url.to_string();
        }
    }

    // 3. 最后显示 接口地址
    let (endpoint_url, _) = p.resolve_credentials(tool);
    if !endpoint_url.is_empty() {
        return endpoint_url;
    }

    String::new()
}

pub(super) fn provider_row(
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

    let avatar_spec = crate::icons::provider_avatar_spec(tool, &p.name, &p.category, &t);
    let subtitle = extract_provider_subtitle(tool, p, &i);

    let test_badge = ws.ui.provider_test_results.get(&p.id).map(|result| {
        let pid = p.id.clone();
        if result.ok {
            let msg = if result.models_count > 0 {
                format!("测试连通成功 ({}ms, 可用模型: {})", result.latency_ms, result.models_count)
            } else {
                format!("测试连通成功 ({}ms)", result.latency_ms)
            };
            div()
                .id(gpui::SharedString::from(format!("provider-test-ok-{pid}")))
                .px(px(6.0))
                .py(px(2.0))
                .rounded(px(4.0))
                .bg(crate::rgba_const(0x10b98115))
                .text_size(px(11.5))
                .font_weight(gpui::FontWeight::MEDIUM)
                .text_color(crate::rgba_const(0x10b981ff))
                .child(format!("{}ms", result.latency_ms))
                .tooltip(move |_window, cx| cx.new(|_| Tooltip::new(msg.clone())).into())
                .into_any_element()
        } else {
            let err_msg = if result.message.trim().is_empty() {
                "连通测试失败，请检查网络或 API密钥".to_string()
            } else {
                format!("测试失败: {}", result.message)
            };
            let parsed = crate::components::parse_generic_error(&err_msg);
            crate::components::error_badge_tooltip(
                format!("provider-test-fail-{pid}"),
                i.t("连通失败", "Failed"),
                parsed.detail,
                &t,
            )
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
                        .text_color(if t.is_dark { crate::rgba_const(0xffffff28) } else { crate::rgba_const(0x00000028) }),
                ),
        )
        .child(match avatar_spec {
            crate::icons::ProviderAvatarSpec::Svg { svg_data, bg, fg } => {
                div()
                    .size(px(38.0))
                    .flex_shrink_0()
                    .rounded(px(8.0))
                    .bg(bg)
                    .border_1()
                    .border_color(if t.is_dark { crate::rgba_const(0xffffff18) } else { t.card_border })
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(
                        gpui::svg()
                            .data(svg_data)
                            .size(px(20.0))
                            .text_color(fg)
                            .flex_none(),
                    )
            }
            crate::icons::ProviderAvatarSpec::Initials { text, .. } => {
                let font_size = if text.chars().count() > 1 { px(12.5) } else { px(15.0) };
                div()
                    .size(px(38.0))
                    .flex_shrink_0()
                    .rounded(px(8.0))
                    .bg(t.row_hover)
                    .border_1()
                    .border_color(t.card_border)
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(
                        div()
                            .text_size(font_size)
                            .font_weight(gpui::FontWeight::BOLD)
                            .text_color(t.text_secondary)
                            .child(text),
                    )
            }
        })
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
                        }))
                        .children(p.website_url.as_deref().map(str::trim).filter(|s| !s.is_empty()).map(|web_url| {
                            let web_url_for_click = web_url.to_string();
                            div()
                                .id(gpui::SharedString::from(format!("provider-site-{}", p.id)))
                                .flex()
                                .items_center()
                                .gap(px(3.5))
                                .px(px(6.0))
                                .py(px(1.5))
                                .rounded(px(4.0))
                                .bg(t.card_bg)
                                .border_1()
                                .border_color(t.card_border)
                                .cursor_pointer()
                                .hover(|s| s.border_color(t.accent).bg(t.card_hover))
                                .tooltip({
                                    let tip = format!("官网: {web_url_for_click}");
                                    move |_window, cx| cx.new(|_| Tooltip::new(tip.clone())).into()
                                })
                                .on_click(cx.listener(move |_ws, _ev, _w, _cx| {
                                    open_in_browser(&web_url_for_click);
                                }))
                                .child(
                                    gpui::svg()
                                        .data(crate::icons::EXTERNAL_LINK_SVG)
                                        .size(px(10.5))
                                        .text_color(t.accent),
                                )
                                .child(
                                    div()
                                        .text_size(px(11.0))
                                        .font_weight(gpui::FontWeight::MEDIUM)
                                        .text_color(t.accent)
                                        .child(i.t("官网", "Website")),
                                )
                        })),
                )
                .children((!subtitle.is_empty()).then(|| {
                    let is_url = subtitle.starts_with("http://") || subtitle.starts_with("https://");
                    if is_url {
                        let open_url = subtitle.clone();
                        div()
                            .id(gpui::SharedString::from(format!("provider-sub-link-{}", p.id)))
                            .flex()
                            .items_center()
                            .gap(px(4.0))
                            .cursor_pointer()
                            .hover(|s| s.text_color(t.accent).underline())
                            .tooltip({
                                let tip = format!("在浏览器中打开: {open_url}");
                                move |_window, cx| cx.new(|_| Tooltip::new(tip.clone())).into()
                            })
                            .on_click(cx.listener(move |_ws, _ev, _w, _cx| {
                                open_in_browser(&open_url);
                            }))
                            .child(
                                gpui::svg()
                                    .data(crate::icons::EXTERNAL_LINK_SVG)
                                    .size(px(10.5))
                                    .text_color(t.text_muted),
                            )
                            .child(
                                div()
                                    .text_size(px(12.0))
                                    .text_color(t.text_muted)
                                    .hover(|s| s.text_color(t.accent))
                                    .child(subtitle),
                            )
                            .into_any_element()
                    } else {
                        div()
                            .text_size(px(12.0))
                            .text_color(t.text_muted)
                            .child(subtitle)
                            .into_any_element()
                    }
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

    if tool == ToolId::Pi {
        let pid_toggle = p.id.clone();
        let is_enabled = p.is_applied;
        actions = actions.child(
            div()
                .flex()
                .items_center()
                .gap(px(8.0))
                .child(
                    badge(
                        &t,
                        if is_enabled { i.t("已启用", "Enabled") } else { i.t("未启用", "Disabled") },
                        if is_enabled { BadgeKind::Success } else { BadgeKind::Neutral },
                    )
                )
                .child(components::toggle(
                    gpui::SharedString::from(format!("pi-prov-toggle-{}", p.id)),
                    is_enabled,
                    &t,
                    cx,
                    move |ws, _, _, cx| {
                        ws.set_pi_provider_enabled(&pid_toggle, !is_enabled, cx);
                    },
                ))
        );
    } else if p.is_applied {
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
            super::provider_dialog::open_provider_dialog(Some(pid2.clone()), tool, ws, cx);
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
                ws.ui.confirm = Some(crate::pages::ConfirmState {
                    title: ws.i18n.t("删除供应商", "Delete Provider").to_string(),
                    message: ws
                        .i18n
                        .t("确定要删除这条供应商配置吗？", "Delete this provider?")
                        .to_string(),
                    action: crate::pages::ConfirmAction::DeleteProvider {
                        tool,
                        id: pid4.clone(),
                    },
                });
                cx.notify();
            },
        ));
    }

    let border_color = if p.is_applied {
        if tool == ToolId::Pi {
            gpui::rgba(0x22c55e66)
        } else {
            gpui::rgb(0x3b82f6)
        }
    } else {
        t.card_border
    };
    let bg_color = if p.is_applied {
        if tool == ToolId::Pi {
            if t.is_dark {
                crate::rgba_const(0x102418ff)
            } else {
                crate::rgba_const(0xf0fdf4ff)
            }
        } else if t.is_dark {
            crate::rgba_const(0x121722ff)
        } else {
            crate::rgba_const(0xf0f7ffff)
        }
    } else {
        t.card_bg
    };
    let hover_bg = if t.is_dark {
        if p.is_applied {
            if tool == ToolId::Pi {
                crate::rgba_const(0x143020ff)
            } else {
                crate::rgba_const(0x151c2aff)
            }
        } else {
            t.card_hover
        }
    } else {
        if p.is_applied {
            if tool == ToolId::Pi {
                crate::rgba_const(0xdcfce7ff)
            } else {
                crate::rgba_const(0xe6f0feff)
            }
        } else {
            t.card_hover
        }
    };
    let hover_border = if p.is_applied {
        if tool == ToolId::Pi {
            gpui::rgb(0x22c55e)
        } else {
            gpui::rgb(0x60a5fa)
        }
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
pub(super) fn import_providers(tool: ToolId, _ws: &mut Workspace, cx: &mut Context<Workspace>) {
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
pub(super) fn export_providers(tool: ToolId, ws: &mut Workspace, cx: &mut Context<Workspace>) {
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
pub(super) fn test_single_provider_action(
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
pub(super) fn batch_test_providers(tool: ToolId, ws: &mut Workspace, cx: &mut Context<Workspace>) {
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

pub(super) fn fetch_models_action(
    tool: ToolId,
    provider_id: String,
    ws: &mut Workspace,
    cx: &mut Context<Workspace>,
) {
    let provider = ws
        .store
        .store()
        .tool(tool)
        .providers
        .iter()
        .find(|p| p.id == provider_id)
        .cloned();
    let settings = provider.as_ref().map(|p| p.settings()).unwrap_or_default();
    let api_format = provider.as_ref().and_then(|p| p.parsed_meta().api_format);
    let custom_headers = provider.as_ref().and_then(|p| {
        p.parsed_meta().custom_headers.map(|list| {
            let mut map = std::collections::BTreeMap::new();
            for item in list {
                map.insert(item.name, item.value);
            }
            map
        })
    });

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
            .background_spawn(async move {
                aitoolplus_core::api_hub::fetch_models_advanced(
                    &base_url,
                    &api_key,
                    api_format.as_deref(),
                    custom_headers.as_ref(),
                    None,
                )
            })
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

