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

pub(super) fn spawn_tool_action<F>(
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
                                super::extensions::load_pi_extensions(workspace, cx);
                            }
                            ToolId::OhMyPi => {
                                workspace.ui.omp_extensions = None;
                                super::extensions::load_omp_extensions(workspace, cx);
                            }
                            ToolId::Grok => {
                                workspace.ui.grok_plugins = None;
                                super::plugins::load_grok_plugins(workspace, cx);
                            }
                            ToolId::ClaudeCode => {
                                workspace.ui.claude_plugins = None;
                                super::plugins::load_claude_plugins(workspace, cx);
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

pub(super) fn plugin_tag(text: impl Into<gpui::SharedString>, bg: gpui::Rgba, fg: gpui::Rgba) -> gpui::Div {
    div()
        .flex()
        .items_center()
        .h(px(20.0))
        .px(px(6.0))
        .rounded(px(4.0))
        .text_size(px(11.0))
        .font_weight(gpui::FontWeight::MEDIUM)
        .bg(bg)
        .text_color(fg)
        .child(text.into())
}

pub(super) fn open_in_browser(url: &str) {
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

