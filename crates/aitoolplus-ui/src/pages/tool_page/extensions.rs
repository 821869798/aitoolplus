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

pub(super) fn extensions_section(
    tool: ToolId,
    ws: &mut Workspace,
    cx: &mut Context<Workspace>,
) -> gpui::AnyElement {
    if !matches!(tool, ToolId::Pi | ToolId::OhMyPi) {
        return div().into_any_element();
    }
    let t = ws.theme.clone();
    let i = ws.i18n;

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

    let refresh_label = i.t("刷新", "Refresh");

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
            .child(button_with_icon_loading_l(
                if is_pi { "pi-ext-refresh-btn" } else { "omp-ext-refresh-btn" },
                crate::icons::REFRESH_SVG,
                refresh_label,
                ButtonVariant::Secondary,
                is_loading,
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
                .child(crate::components::error_strip(
                    "claude-plugins-read-err",
                    i.t("扩展读取失败", "Extensions read failed"),
                    &e,
                    &t,
                    cx,
                    None,
                ));
        }
    }

    section.into_any_element()
}

pub(super) fn opencode_addons_section(ws: &mut Workspace, cx: &mut Context<Workspace>) -> gpui::AnyElement {
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
        let scroll_handle = editor.read(cx).scroll_handle.clone();
        let focus_handle = editor.read(cx).focus_handle.clone();
        card = card
            .child(text_area_scroll_container(
                gpui::SharedString::from(format!("addon-editor-wrap-{}", kind.key())),
                gpui::SharedString::from(format!("addon-scrollbar-{}", kind.key())),
                &t,
                px(220.0),
                &scroll_handle,
                &focus_handle,
                editor,
            ))
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

