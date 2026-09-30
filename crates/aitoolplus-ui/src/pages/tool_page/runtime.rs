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

pub(super) fn common_section(
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
    let save_label = i.t("tool_runtime.save_apply_common_config");
    let format_label = i.t("tool_runtime.format");
    let extract_label = i.t("tool_runtime.read_current_file");

    div()
        .flex()
        .flex_col()
        .gap(px(12.0))
        .child(section_title(
            &t,
            i.t("tool_runtime.common_config_merged_with"),
            Some(i.raw(
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
                .h(px(240.0))
                .id("common-editor-wrap")
                .overflow_y_scroll()
                .rounded(px(8.0))
                .bg(t.input_bg)
                .border_1()
                .border_color(t.input_border)
                .track_focus(&editor.read(cx).focus_handle)
                .focus(|s| s.border_color(crate::rgba_const(0x3b82f6cc)))
                .hover(move |h| h.border_color(t.card_border_hover))
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
                                let msg = ws.i18n.t("tool_runtime.read_failed").to_string();
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
                                    ws.i18n.t("tool_runtime.saved_and_applied").to_string();
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

pub(super) fn reveal_in_explorer(path: &std::path::Path) {
    crate::pages::open_path_in_default_manager(path);
}

pub(super) fn open_in_system_editor(path: &std::path::Path) {
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

    let title = i.raw(
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
        .child({
            let scroll_handle = editor.read(cx).scroll_handle.clone();
            let focus_handle = editor.read(cx).focus_handle.clone();
            text_area_scroll_container(
                "runtime-editor-wrap",
                "runtime-editor-scrollbar",
                &t,
                px(380.0),
                &scroll_handle,
                &focus_handle,
                editor.clone(),
            )
        })
        .child(
            div()
                .flex()
                .justify_end()
                .gap(px(8.0))
                .pt(px(4.0))
                .child(button_l(
                    "runtime-edit-cancel",
                    i.t("tool_runtime.cancel"),
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
                    i.t("tool_runtime.save_changes"),
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
                                        .t("tool_runtime.config_file_saved_successfully")
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

pub(super) fn runtime_section(
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
                i.t("tool_runtime.runtime_files"),
                i.t("tool_runtime.inspect_reveal_open_in"),
            ))
            .child(button_l(
                "runtime-refresh-btn",
                i.t("tool_runtime.refresh"),
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
            i.t("tool_runtime.no_known_config_files"),
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
                                        i.t("tool_runtime.exists")
                                    } else {
                                        i.t("tool_runtime.missing")
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
                                    i.t("tool_runtime.reveal"),
                                    ButtonVariant::Secondary,
                                    &t,
                                    cx,
                                    move |_, _, _, _| {
                                        reveal_in_explorer(&path_clone1);
                                    },
                                ))
                                .child(button_l(
                                    gpui::SharedString::from(format!("runtime-open-{idx}")),
                                    i.t("tool_runtime.open"),
                                    ButtonVariant::Secondary,
                                    &t,
                                    cx,
                                    move |_, _, _, _| {
                                        open_in_system_editor(&path_clone2);
                                    },
                                ))
                                .child(button_l(
                                    gpui::SharedString::from(format!("runtime-copy-{idx}")),
                                    i.t("tool_runtime.copy_path"),
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
                                                .t("tool_runtime.path_copied")
                                                .to_string();
                                            ws.ui.toast(msg, false);
                                            cx.notify();
                                        }
                                    },
                                ))
                                .child(button_l(
                                    gpui::SharedString::from(format!("runtime-edit-{idx}")),
                                    if exists {
                                        i.t("tool_runtime.edit")
                                    } else {
                                        i.t("tool_runtime.create")
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
                        .child(i.t("tool_runtime.file_does_not_exist"))
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
pub(super) fn pi_model_settings_section(ws: &mut Workspace, cx: &mut Context<Workspace>) -> gpui::AnyElement {
    let t = ws.theme.clone();
    let i = ws.i18n;
    let mut section = div()
        .flex()
        .flex_col()
        .gap(px(12.0))
        .p(px(14.0))
        .rounded(px(10.0))
        .bg(t.card_bg)
        .border_1()
        .border_color(t.card_border);

    match (
        aitoolplus_core::pi_pages::read_model_settings(&ws.paths),
        aitoolplus_core::pi_pages::models_catalog(&ws.paths),
    ) {
        (Ok(current), Ok(catalog)) => {
            if !ws.ui.pi_ms_initialized {
                ws.ui.pi_ms_provider = current.provider_key.clone();
                ws.ui.pi_ms_model = current.model_id.clone();
                ws.ui.pi_ms_thinking = current.thinking_level.clone();
                let p_str = current.provider_key.as_deref().unwrap_or("");
                let m_str = current.model_id.as_deref().unwrap_or("");
                let t_str = current.thinking_level.as_deref().unwrap_or("");
                ws.ui.pi_ms_provider_input.update(cx, |inp, cx| inp.set_text_silent(p_str, cx));
                ws.ui.pi_ms_model_input.update(cx, |inp, cx| inp.set_text_silent(m_str, cx));
                ws.ui.pi_ms_thinking_input.update(cx, |inp, cx| inp.set_text_silent(t_str, cx));
                ws.ui.pi_ms_initialized = true;
            }

            let selected_provider = ws.ui.pi_ms_provider.clone();

            let provider_options: Vec<String> = catalog.keys().cloned().collect();
            let model_options: Vec<String> = if let Some(p) = &selected_provider {
                catalog.get(p).cloned().unwrap_or_default()
            } else {
                vec![]
            };
            let thinking_options: Vec<String> = aitoolplus_core::pi_pages::KNOWN_THINKING_LEVELS
                .iter()
                .map(|s| s.to_string())
                .collect();

            let header = div()
                .flex()
                .items_center()
                .justify_between()
                .w_full()
                .child(section_title(
                    &t,
                    i.t("tool_runtime.model_settings"),
                    Some(i.t("tool_runtime.writes_settings_json_defaultpr")),
                ))
                .child(button_l(
                    "pi-ms-save",
                    i.t("tool_runtime.save_model_settings"),
                    ButtonVariant::Primary,
                    &t,
                    cx,
                    move |ws, _, _, cx| {
                        let p_txt = ws.ui.pi_ms_provider_input.read(cx).text().trim().to_string();
                        let m_txt = ws.ui.pi_ms_model_input.read(cx).text().trim().to_string();
                        let t_txt = ws.ui.pi_ms_thinking_input.read(cx).text().trim().to_string();
                        let settings = PiModelSettings {
                            provider_key: if p_txt.is_empty() { None } else { Some(p_txt) },
                            model_id: if m_txt.is_empty() { None } else { Some(m_txt) },
                            thinking_level: if t_txt.is_empty() { None } else { Some(t_txt) },
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
                        match aitoolplus_core::pi_pages::write_model_settings(&ws.paths, &settings) {
                            Ok(_) => {
                                ws.ui.toast(
                                    ws.i18n
                                        .t("tool_runtime.model_settings_saved")
                                        .to_string(),
                                    false,
                                );
                            }
                            Err(e) => {
                                ws.ui.toast(format!("save failed: {e}"), true);
                            }
                        }
                        cx.notify();
                    },
                ));

            // Single-row 3-column dropdown layout
            let row = div()
                .flex()
                .flex_row()
                .gap(px(12.0))
                .w_full()
                .items_start()
                .child(pi_searchable_select(
                    "pi-ms-prov-select",
                    crate::pages::PiDropdownField::Provider,
                    "默认供应商",
                    ws.ui.pi_ms_provider_input.clone(),
                    provider_options,
                    ws,
                    cx,
                ))
                .child(pi_searchable_select(
                    "pi-ms-model-select",
                    crate::pages::PiDropdownField::Model,
                    "默认模型",
                    ws.ui.pi_ms_model_input.clone(),
                    model_options,
                    ws,
                    cx,
                ))
                .child(pi_searchable_select(
                    "pi-ms-think-select",
                    crate::pages::PiDropdownField::Thinking,
                    "思考等级",
                    ws.ui.pi_ms_thinking_input.clone(),
                    thinking_options,
                    ws,
                    cx,
                ));

            section = section.child(header).child(row);
        }
        (Err(e), _) | (_, Err(e)) => {
            section = section.child(crate::components::error_strip(
                "pi-cfg-read-err",
                i.t("tool_runtime.config_read_failed"),
                &e,
                &t,
                cx,
                None,
            ));
        }
    }

    section.into_any_element()
}

pub(super) fn pi_searchable_select(
    id: &'static str,
    field: crate::pages::PiDropdownField,
    label: &'static str,
    input_entity: gpui::Entity<TextInput>,
    options: Vec<String>,
    ws: &mut Workspace,
    cx: &mut Context<Workspace>,
) -> gpui::AnyElement {
    let t = ws.theme.clone();
    let i = ws.i18n;
    let is_open = ws.ui.pi_dropdown_open == Some(field);
    let is_typing = ws.ui.pi_dropdown_typing;

    let f = field;
    let input_ent_open = input_entity.clone();
    let input_ent_clear = input_entity.clone();
    let input_ent_pick = input_entity.clone();

    components::fused_combobox(
        id,
        Some(i.raw(label, label)),
        input_entity,
        is_open,
        is_typing,
        options,
        Some(i.t("tool_runtime.no_matches_found")),
        &t,
        cx,
        move |ws, window, cx| {
            if let Some((field, instant)) = ws.ui.pi_dropdown_just_closed {
                if field == f && instant.elapsed() < std::time::Duration::from_millis(200) {
                    return;
                }
            }
            ws.ui.pi_dropdown_open = Some(f);
            ws.ui.pi_dropdown_typing = false;
            input_ent_open.update(cx, |inp, cx| {
                inp.focus_handle.focus(window, cx);
                inp.start_blink(cx);
                inp.select_all(cx);
            });
            cx.notify();
        },
        move |ws, cx| {
            if ws.ui.pi_dropdown_open == Some(f) {
                ws.ui.pi_dropdown_open = None;
                ws.ui.pi_dropdown_typing = false;
                ws.ui.pi_dropdown_just_closed = Some((f, std::time::Instant::now()));
                cx.notify();
            }
        },
        move |ws, window, cx| {
            input_ent_clear.update(cx, |inp, cx| {
                inp.set_text_silent("", cx);
                inp.focus_handle.focus(window, cx);
                inp.start_blink(cx);
            });
            match f {
                crate::pages::PiDropdownField::Provider => {
                    ws.ui.pi_ms_provider = None;
                    ws.ui.pi_ms_model = None;
                    ws.ui.pi_ms_model_input.update(cx, |inp, cx| inp.set_text_silent("", cx));
                }
                crate::pages::PiDropdownField::Model => {
                    ws.ui.pi_ms_model = None;
                }
                crate::pages::PiDropdownField::Thinking => {
                    ws.ui.pi_ms_thinking = None;
                }
            }
            ws.ui.pi_dropdown_open = Some(f);
            ws.ui.pi_dropdown_typing = true;
            cx.notify();
        },
        move |ws, opt, _window, cx| {
            input_ent_pick.update(cx, |inp, cx| {
                inp.set_text_silent(opt.clone(), cx);
            });
            match f {
                crate::pages::PiDropdownField::Provider => {
                    ws.ui.pi_ms_provider = Some(opt.clone());
                    ws.ui.pi_ms_model = None;
                    ws.ui.pi_ms_model_input.update(cx, |inp, cx| inp.set_text_silent("", cx));
                }
                crate::pages::PiDropdownField::Model => {
                    ws.ui.pi_ms_model = Some(opt.clone());
                }
                crate::pages::PiDropdownField::Thinking => {
                    ws.ui.pi_ms_thinking = Some(opt.clone());
                }
            }
            ws.ui.pi_dropdown_open = None;
            ws.ui.pi_dropdown_typing = false;
            cx.notify();
        },
    )
}

/// Other Settings: settings.json minus packages, editable + save.
pub(super) fn pi_other_settings_section(ws: &mut Workspace, cx: &mut Context<Workspace>) -> gpui::AnyElement {
    let t = ws.theme.clone();
    let i = ws.i18n;

    match aitoolplus_core::pi_pages::read_other_settings(&ws.paths) {
        Ok(other) => {
            let pretty = serde_json::to_string_pretty(&other).unwrap_or_else(|_| "{}".to_string());
            let editor = ws.ui.pi_other_editor(&pretty, cx);
            let editor_save = editor.clone();
            let editor_fmt = editor.clone();
            let scroll_handle = editor.read(cx).scroll_handle.clone();
            let focus_handle = editor.read(cx).focus_handle.clone();

            let header = div()
                .flex()
                .items_center()
                .justify_between()
                .w_full()
                .child(section_title(
                    &t,
                    i.t("tool_runtime.pi_other_settings_settings"),
                    Some(i.t("tool_runtime.all_settings_in_pi")),
                ))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(8.0))
                        .child(button_l(
                            "pi-other-fmt",
                            i.t("tool_runtime.format_json"),
                            ButtonVariant::Secondary,
                            &t,
                            cx,
                            move |ws, _, _, cx| {
                                let text: String = editor_fmt.read(cx).text().to_string();
                                match serde_json::from_str::<Value>(&text) {
                                    Ok(val) => {
                                        if let Ok(formatted) = serde_json::to_string_pretty(&val) {
                                            editor_fmt.update(cx, |ta, cx| ta.set_text(formatted, cx));
                                            let msg = ws.i18n.t("tool_runtime.formatted_json").to_string();
                                            ws.ui.toast(msg, false);
                                        }
                                    }
                                    Err(e) => {
                                        let msg = ws.i18n.raw(&format!("JSON 格式不正确：{e}"), &format!("Invalid JSON: {e}")).to_string();
                                        ws.ui.toast(msg, true);
                                    }
                                }
                                cx.notify();
                            },
                        ))
                        .child(button_l(
                            "pi-other-reload",
                            i.t("tool_runtime.reload"),
                            ButtonVariant::Secondary,
                            &t,
                            cx,
                            |ws, _, _, cx| {
                                ws.ui.pi_other_editor = None;
                                let msg = ws.i18n.t("tool_runtime.reloaded_settings_from_disk").to_string();
                                ws.ui.toast(msg, false);
                                cx.notify();
                            },
                        ))
                        .child(button_l(
                            "pi-other-save",
                            i.t("tool_runtime.save_settings"),
                            ButtonVariant::Primary,
                            &t,
                            cx,
                            move |ws, _, _, cx| {
                                let text: String = editor_save.read(cx).text().to_string();
                                match serde_json::from_str::<Value>(&text) {
                                    Ok(edited) => {
                                        match aitoolplus_core::pi_pages::write_other_settings(
                                            &ws.paths, &edited,
                                        ) {
                                            Ok(_) => {
                                                ws.ui.pi_other_editor = None;
                                                let msg = ws.i18n.t("tool_runtime.other_settings_saved").to_string();
                                                ws.ui.toast(msg, false);
                                            }
                                            Err(e) => ws.ui.toast(format!("save failed: {e}"), true),
                                        }
                                    }
                                    Err(e) => {
                                        let msg = ws
                                            .i18n
                                            .raw(&format!("JSON 无效：{e}"), &format!("invalid JSON: {e}"))
                                            .to_string();
                                        ws.ui.toast(msg, true);
                                    }
                                }
                                cx.notify();
                            },
                        )),
                );

            let editor_box = div()
                .flex()
                .flex_col()
                .gap(px(8.0))
                .w_full()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .text_size(px(12.0))
                        .text_color(t.text_secondary)
                        .child(i.t("tool_runtime.settings_file_pi_agent"))
                        .child(
                            div()
                                .px(px(6.0))
                                .py(px(2.0))
                                .rounded(px(4.0))
                                .bg(t.card_border)
                                .text_size(px(11.0))
                                .font_weight(gpui::FontWeight::MEDIUM)
                                .text_color(t.text_primary)
                                .child("JSON"),
                        ),
                )
                .child(text_area_scroll_container(
                    "pi-other-editor-wrap",
                    "pi-other-scrollbar",
                    &t,
                    px(520.0),
                    &scroll_handle,
                    &focus_handle,
                    editor,
                ))
                .child(
                    div()
                        .text_size(px(11.5))
                        .text_color(t.text_muted)
                        .child(i.t("tool_runtime.note_the_packages_field")),
                );

            div()
                .flex()
                .flex_col()
                .gap(px(14.0))
                .p(px(16.0))
                .rounded(px(10.0))
                .bg(t.card_bg)
                .border_1()
                .border_color(t.card_border)
                .child(header)
                .child(editor_box)
                .into_any_element()
        }
        Err(e) => div()
            .flex()
            .flex_col()
            .gap(px(10.0))
            .p(px(16.0))
            .rounded(px(10.0))
            .bg(t.card_bg)
            .border_1()
            .border_color(t.card_border)
            .child(crate::components::error_strip(
                "pi-other-read-err",
                i.t("tool_runtime.failed_to_read_settings"),
                &e,
                &t,
                cx,
                None,
            ))
            .into_any_element(),
    }
}

