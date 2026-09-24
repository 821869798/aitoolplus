//! Per-tool page, split by tab.
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

mod common;
mod extensions;
mod plugins;
mod prompts;
mod provider_dialog;
mod providers;
mod runtime;
mod sessions;

pub use prompts::{open_prompt_dialog, render_prompt_dialog};
pub use provider_dialog::{
    fetch_upstream_models_for_dialog, open_provider_dialog, render_provider_dialog,
};
pub use runtime::render_runtime_edit_dialog;
pub use sessions::load_agent_sessions;
pub(crate) use sessions::{render_virtual_agent_session_card, session_list_viewport};

pub fn render_tool_page(
    tool: ToolId,
    ws: &mut Workspace,
    cx: &mut Context<Workspace>,
) -> gpui::AnyElement {
    // If a session is open for this tool, directly render the full session detail view (covering tabs_bar)
    if let Some((open_tool, ref open_sid)) = ws.ui.open_session {
        if open_tool == tool {
            let meta_opt = ws.ui.agent_sessions.as_ref()
                .and_then(|(t, list)| if *t == tool { list.iter().find(|s| &s.session_id == open_sid).cloned() } else { None })
                .or_else(|| {
                    let sessions = session::cached_scan(&ws.paths, open_tool, session::DEFAULT_SESSION_PATH_LIMIT);
                    sessions.into_iter().find(|s| &s.session_id == open_sid)
                });
            if let Some(meta) = meta_opt {
                return crate::pages::session_detail::render_session_detail(open_tool, &meta, ws, cx);
            }
        }
    }

    let _i = ws.i18n;
    let _t = ws.theme.clone();

    // Ensure active tool_tab is supported by this tool; fall back to Providers if not.
    let valid_tab = match ws.ui.tool_tab {
        ToolTab::Providers | ToolTab::Prompts | ToolTab::Runtime | ToolTab::Sessions => true,
        ToolTab::Common => tool == ToolId::Pi,
        ToolTab::Extensions => matches!(tool, ToolId::Pi | ToolId::OhMyPi),
        ToolTab::Plugins => matches!(tool, ToolId::ClaudeCode | ToolId::Codex | ToolId::Grok),
        ToolTab::Marketplace => matches!(tool, ToolId::ClaudeCode | ToolId::Codex | ToolId::Grok),
        ToolTab::Addons => tool == ToolId::OpenCode,
    };
    if !valid_tab {
        ws.ui.tool_tab = ToolTab::Providers;
    }

    let is_custom_scroll = matches!(ws.ui.tool_tab, ToolTab::Marketplace | ToolTab::Sessions);

    let mut col = div()
        .flex()
        .flex_col()
        .w_full()
        .min_w(px(0.0))
        .gap(px(16.0));

    if is_custom_scroll {
        col = col.h_full().min_h(px(0.0));
    }

    col = col.child(tabs_bar(tool, ws, cx));

    match ws.ui.tool_tab {
        ToolTab::Providers => {
            if tool == ToolId::Pi {
                col = col.child(runtime::pi_model_settings_section(ws, cx));
            }
            col = col.child(providers::providers_section(tool, ws, cx));
        }
        ToolTab::Common => {
            if tool == ToolId::Pi {
                col = col.child(runtime::pi_other_settings_section(ws, cx));
            }
        }
        ToolTab::Prompts => col = col.child(prompts::prompts_section(tool, ws, cx)),
        ToolTab::Runtime => col = col.child(runtime::runtime_section(tool, ws, cx)),
        ToolTab::Extensions => {
            if matches!(tool, ToolId::Pi | ToolId::OhMyPi) {
                col = col.child(extensions::extensions_section(tool, ws, cx));
            }
        }
        ToolTab::Plugins => {
            if tool == ToolId::ClaudeCode {
                col = col.child(plugins::claude_installed_plugins_section(ws, cx));
            } else if tool == ToolId::Codex {
                col = col.child(plugins::codex_installed_plugins_section(ws, cx));
            } else if tool == ToolId::Grok {
                col = col.child(plugins::grok_installed_plugins_section(ws, cx));
            }
        }
        ToolTab::Marketplace => {
            if tool == ToolId::ClaudeCode {
                col = col.child(plugins::claude_marketplace_section(ws, cx));
            } else if tool == ToolId::Codex {
                col = col.child(plugins::codex_marketplace_section(ws, cx));
            } else if tool == ToolId::Grok {
                col = col.child(plugins::grok_marketplace_section(ws, cx));
            }
        }
        ToolTab::Addons => {
            if tool == ToolId::OpenCode {
                col = col.child(extensions::opencode_addons_section(ws, cx));
            }
        }
        ToolTab::Sessions => {
            col = col.child(sessions::agent_sessions_section(tool, ws, cx));
        }
    }
    col.into_any_element()
}

fn tabs_bar(tool: ToolId, ws: &mut Workspace, cx: &mut Context<Workspace>) -> gpui::AnyElement {
    let t = &ws.theme;
    let i = ws.i18n;
    let current = ws.ui.tool_tab;

    let mut tabs = vec![
        (ToolTab::Providers, i.t("供应商", "Providers")),
        (ToolTab::Prompts, i.t("全局提示词", "Prompts")),
        (ToolTab::Runtime, i.t("运行时文件", "Runtime Files")),
    ];
    if matches!(tool, ToolId::Pi | ToolId::OhMyPi) {
        tabs.push((ToolTab::Extensions, i.t("扩展", "Extensions")));
    }
    if tool == ToolId::Pi {
        tabs.push((ToolTab::Common, i.t("其他设置", "Other Settings")));
    }
    if tool == ToolId::ClaudeCode || tool == ToolId::Codex || tool == ToolId::Grok {
        tabs.push((ToolTab::Plugins, i.t("已安装插件", "Installed Plugins")));
        tabs.push((ToolTab::Marketplace, i.t("插件市场", "Marketplace")));
    }
    if tool == ToolId::OpenCode {
        tabs.push((ToolTab::Addons, i.t("附加工具", "Add-ons")));
    }
    tabs.push((ToolTab::Sessions, i.t("会话管理", "Sessions")));

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
