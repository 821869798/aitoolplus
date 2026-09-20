//! The root workspace view: owns all app state, routes pages, hosts modals.

use std::sync::Arc;

use aitoolplus_core::adapters::{ApplyCtx, adapter_for};
use aitoolplus_core::config::MergeStrategy;
use aitoolplus_core::paths::Paths;
use aitoolplus_core::providers::ProviderRecord;
use aitoolplus_core::settings::{AppSettings, Language as CoreLanguage};
use aitoolplus_core::store::{Store, StoreHandle};
use aitoolplus_core::tools::ToolId;
use gpui::{Context, Render, Window, div, prelude::*, px};

use crate::i18n::I18n;
use crate::pages::{self, Page, WorkspaceState};
use crate::text_input::TextInput;
use crate::theme::Theme;

/// Callbacks the host binary provides to persist state.
pub struct WorkspaceCallbacks {
    /// Persist app settings (language/theme/etc.).
    pub save_settings: Box<dyn Fn(&AppSettings)>,
    /// Persist the store after a mutation.
    pub save_store: Box<dyn Fn(&Store)>,
    /// Show a toast/banner message.
    pub notify: Box<dyn Fn(String)>,
    /// Notify the host that the store changed (e.g. tray menu refresh).
    /// Receives the just-saved store so the host can snapshot it directly.
    pub store_changed: Box<dyn Fn(&Store)>,
}

pub struct Workspace {
    pub paths: Arc<Paths>,
    pub store: StoreHandle,
    pub settings: AppSettings,
    pub theme: Theme,
    pub i18n: I18n,
    pub page: Page,
    /// UI page state (tabs, dialogs, dirty text buffers).
    pub ui: WorkspaceState,
    pub callbacks: WorkspaceCallbacks,
    /// Root focus handle for window focus requests.
    root_focus: gpui::FocusHandle,
}

impl Workspace {
    pub fn new(
        paths: Arc<Paths>,
        mut store: StoreHandle,
        settings: AppSettings,
        callbacks: WorkspaceCallbacks,
        cx: &mut Context<Self>,
    ) -> Self {
        // Discover the user's existing live configurations before the first
        // render. This is cc-switch first-run parity: already-working CLI
        // configs appear as applied providers instead of empty pages.
        let _ = store.update(|db| {
            let _ = aitoolplus_core::import_current::import_claude_current(
                &paths,
                &mut db.tool_mut(ToolId::ClaudeCode).providers,
            );
            let _ = aitoolplus_core::import_current::import_codex_current(
                &paths,
                &mut db.tool_mut(ToolId::Codex).providers,
            );
            let _ = aitoolplus_core::import_current::import_gemini_current(
                &paths,
                &mut db.tool_mut(ToolId::GeminiCli).providers,
            );
            let _ = aitoolplus_core::import_current::import_opencode_current(
                &paths,
                &mut db.tool_mut(ToolId::OpenCode).providers,
            );
            let _ = aitoolplus_core::import_current::import_grok_current(
                &paths,
                &mut db.tool_mut(ToolId::Grok).providers,
            );

            // Ensure cc-switch parity official persistent providers.
            for tool in [
                ToolId::ClaudeCode,
                ToolId::Codex,
                ToolId::GeminiCli,
                ToolId::Grok,
                ToolId::ClaudeDesktop,
            ] {
                aitoolplus_core::providers::ensure_official_provider(
                    tool,
                    &mut db.tool_mut(tool).providers,
                );
            }

            // Runtime-native provider catalogs.
            let _ = aitoolplus_core::pi_runtime::import_runtime(
                &paths,
                &mut db.tool_mut(ToolId::Pi).providers,
            );
            let _ = aitoolplus_core::adapters::oh_my_pi::import_into(
                &mut db.tool_mut(ToolId::OhMyPi).providers,
                &paths,
            );
            let _ = aitoolplus_core::adapters::claude_desktop::import_into(
                &mut db.tool_mut(ToolId::ClaudeDesktop).providers,
                &paths,
            );
            let _ = aitoolplus_core::adapters::hermes::import_into(
                &mut db.tool_mut(ToolId::Hermes).providers,
                &paths,
            );
            let _ = aitoolplus_core::adapters::dsh::import_into(
                &mut db.tool_mut(ToolId::Dsh).providers,
                &paths,
            );
        });
        let _ = store.save();
        let requested_theme = std::env::var("AITOOLPLUS_THEME_MODE")
            .ok()
            .and_then(|v| match v.to_lowercase().as_str() {
                "dark" => Some(crate::theme::ThemeMode::Dark),
                "light" => Some(crate::theme::ThemeMode::Light),
                _ => None,
            })
            .unwrap_or(settings.theme_mode);
        let theme = Theme::for_mode(requested_theme, system_prefers_dark());
        let kit_mode = if theme.is_dark {
            gpui_kit::component::ThemeMode::Dark
        } else {
            gpui_kit::component::ThemeMode::Light
        };
        gpui_kit::component::Theme::change(kit_mode, None, cx);
        let i18n = I18n::new(settings.language);
        let mut ui = pages::WorkspaceState::new(cx);
        if !settings.proxy_url.is_empty() {
            ui.proxy_url_input.update(cx, |input, cx| {
                input.set_text_silent(settings.proxy_url.clone(), cx)
            });
        }

        // Restore the last page; test automation may override it explicitly.
        let requested_page = std::env::var("AITOOLPLUS_START_PAGE")
            .ok()
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| settings.last_page.clone());
        tracing::info!(
            start_page = %requested_page,
            start_tab = ?std::env::var("AITOOLPLUS_START_TAB").ok(),
            "resolved workspace start route"
        );
        let page = match requested_page.as_str() {
            "mcp" => Page::Mcp,
            "skills" => Page::Skills,
            "sessions" => Page::Sessions,
            "antigravity" => Page::Antigravity,
            "settings" => Page::Settings,
            key => ToolId::from_key(key)
                .map(Page::Tool)
                .unwrap_or(Page::Tool(ToolId::ClaudeCode)),
        };
        if let Ok(tab) = std::env::var("AITOOLPLUS_START_TAB") {
            ui.tool_tab = match tab.as_str() {
                "common" => pages::ToolTab::Common,
                "prompts" => pages::ToolTab::Prompts,
                "runtime" => pages::ToolTab::Runtime,
                "extensions" => pages::ToolTab::Extensions,
                "plugins" => pages::ToolTab::Plugins,
                "marketplace" => pages::ToolTab::Marketplace,
                "sessions" => pages::ToolTab::Sessions,
                "addons" => pages::ToolTab::Addons,
                _ => pages::ToolTab::Providers,
            };
            if page == Page::Settings {
                ui.settings_tab = match tab.as_str() {
                    "backup" => pages::SettingsTab::Backup,
                    "about" => pages::SettingsTab::About,
                    _ => pages::SettingsTab::General,
                };
            }
        }
        if let Ok(sess_id) = std::env::var("AITOOLPLUS_START_SESSION") {
            match page {
                Page::Tool(tool) => ui.open_session = Some((tool, sess_id)),
                Page::Sessions => ui.open_session = Some((ui.sessions_tool, sess_id)),
                _ => {}
            }
        }

        let root_focus = cx.focus_handle();
        let mut ws = Self {
            paths,
            store,
            settings,
            theme,
            i18n,
            page,
            ui,
            callbacks,
            root_focus,
        };

        if std::env::var("AITOOLPLUS_OPEN_SESSION_ACTIONS").ok().as_deref() == Some("1") {
            ws.ui.session_actions_menu_open = true;
        }

        if std::env::var("AITOOLPLUS_OPEN_ANTIGRAVITY_DIALOG").ok().as_deref() == Some("1") {
            let refresh_token = cx.new(|cx| TextInput::new("输入 Google OAuth Refresh Token…", cx));
            let custom_label = cx.new(|cx| TextInput::new("自定义备注（例如：工作主账号、备用账号）…", cx));
            let active_tab = if std::env::var("AITOOLPLUS_ANTIGRAVITY_TAB").ok().as_deref() == Some("token") {
                pages::antigravity_page::AntigravityDialogTab::RefreshToken
            } else {
                pages::antigravity_page::AntigravityDialogTab::GoogleAuth
            };
            ws.ui.antigravity_dialog = Some(pages::antigravity_page::AntigravityDialogState {
                active_tab,
                refresh_token,
                custom_label,
                auth_status: None,
                is_authorizing: false,
                error_message: None,
            });
        }

        if let Ok(target_tool) = std::env::var("AITOOLPLUS_OPEN_PROVIDER") {
            tracing::info!(open_provider = %target_tool, "open provider requested");
            if let Some(tool) = ToolId::from_key(&target_tool) {
                let existing_id = std::env::var("AITOOLPLUS_PROVIDER_ID")
                    .ok()
                    .filter(|s| !s.trim().is_empty());
                tracing::info!(?tool, ?existing_id, "calling open_provider_dialog");
                pages::tool_page::open_provider_dialog(existing_id, tool, &mut ws, cx);
                tracing::info!(has_dialog = ws.ui.provider_dialog.is_some(), "dialog state after open");
                if std::env::var("AITOOLPLUS_FETCH_MODELS").ok().as_deref() == Some("1") {
                    if let Some(d) = ws.ui.provider_dialog.as_mut() {
                        d.fetched_models = vec![
                            aitoolplus_core::api_hub::FetchedModel {
                                id: "claude-3-7-sonnet-20250219".to_string(),
                                display_name: Some("Claude 3.7 Sonnet".to_string()),
                                owned_by: Some("Anthropic".to_string()),
                                context_length: Some(200000),
                                ..Default::default()
                            },
                            aitoolplus_core::api_hub::FetchedModel {
                                id: "claude-3-5-sonnet-20241022".to_string(),
                                display_name: Some("Claude 3.5 Sonnet".to_string()),
                                owned_by: Some("Anthropic".to_string()),
                                context_length: Some(200000),
                                ..Default::default()
                            },
                            aitoolplus_core::api_hub::FetchedModel {
                                id: "claude-3-opus-20240229".to_string(),
                                display_name: Some("Claude 3 Opus".to_string()),
                                owned_by: Some("Anthropic".to_string()),
                                context_length: Some(200000),
                                ..Default::default()
                            },
                            aitoolplus_core::api_hub::FetchedModel {
                                id: "claude-3-5-haiku-20241022".to_string(),
                                display_name: Some("Claude 3.5 Haiku".to_string()),
                                owned_by: Some("Anthropic".to_string()),
                                context_length: Some(200000),
                                ..Default::default()
                            },
                            aitoolplus_core::api_hub::FetchedModel {
                                id: "deepseek-ai/DeepSeek-V3".to_string(),
                                display_name: Some("DeepSeek V3".to_string()),
                                owned_by: Some("DeepSeek".to_string()),
                                context_length: Some(64000),
                                ..Default::default()
                            },
                            aitoolplus_core::api_hub::FetchedModel {
                                id: "deepseek-ai/DeepSeek-R1".to_string(),
                                display_name: Some("DeepSeek R1".to_string()),
                                owned_by: Some("DeepSeek".to_string()),
                                context_length: Some(64000),
                                ..Default::default()
                            },
                        ];
                        if let Some(active_dd) = std::env::var("AITOOLPLUS_ACTIVE_DROPDOWN")
                            .ok()
                            .filter(|s| !s.trim().is_empty())
                        {
                            d.active_model_dropdown = Some(active_dd);
                        }
                    }
                }
                if std::env::var("AITOOLPLUS_EXPAND_PI").ok().as_deref() == Some("1") {
                    if let Some(d) = ws.ui.provider_dialog.as_mut() {
                        for m in &mut d.pi_models {
                            m.is_expanded = true;
                        }
                    }
                }
                if let Ok(dialog_tab) = std::env::var("AITOOLPLUS_DIALOG_TAB") {
                    if let Some(d) = ws.ui.provider_dialog.as_mut() {
                        if dialog_tab == "advanced" {
                            d.active_tab = pages::ProviderDialogTab::Advanced;
                        }
                    }
                }
                if std::env::var("AITOOLPLUS_SEED_ADVANCED").ok().as_deref() == Some("1") {
                    if let Some(d) = ws.ui.provider_dialog.as_mut() {
                        d.custom_user_agent.update(cx, |inp, cx| {
                            inp.set_text_silent("claude-cli/2.1.237 (external, cli)", cx);
                        });
                        let k1 = cx.new(|cx| {
                            let mut inp = TextInput::new("Header 名称", cx);
                            inp.set_text_silent("HTTP-Referer", cx);
                            inp
                        });
                        let v1 = cx.new(|cx| {
                            let mut inp = TextInput::new("Header 对应值", cx);
                            inp.set_text_silent("https://github.com/aitoolplus", cx);
                            inp
                        });
                        let k2 = cx.new(|cx| {
                            let mut inp = TextInput::new("Header 名称", cx);
                            inp.set_text_silent("X-Title", cx);
                            inp
                        });
                        let v2 = cx.new(|cx| {
                            let mut inp = TextInput::new("Header 对应值", cx);
                            inp.set_text_silent("MyProject", cx);
                            inp
                        });
                        d.custom_headers_list = vec![
                            crate::pages::CustomHeaderDraft { key: k1, value: v1 },
                            crate::pages::CustomHeaderDraft { key: k2, value: v2 },
                        ];
                        d.billing_enabled = true;
                        d.cost_multiplier.update(cx, |inp, cx| {
                            inp.set_text_silent("1.5", cx);
                        });
                        d.pricing_model_source = "request".to_string();
                        let f1 = cx.new(|cx| {
                            let mut inp = TextInput::new("From", cx);
                            inp.set_text_silent("claude-3-5-haiku-20241022", cx);
                            inp
                        });
                        let t1 = cx.new(|cx| {
                            let mut inp = TextInput::new("To", cx);
                            inp.set_text_silent("deepseek-chat", cx);
                            inp
                        });
                        d.model_rewrites = vec![
                            crate::pages::ModelRewriteDraft { from: f1, to: t1 },
                        ];
                    }
                }
            }
        }

        if std::env::var("AITOOLPLUS_OPEN_PROMPT_DIALOG").ok().as_deref() == Some("1") {
            if let Page::Tool(tool) = ws.page {
                let editing_id = std::env::var("AITOOLPLUS_PROMPT_ID")
                    .ok()
                    .filter(|s| !s.trim().is_empty());
                pages::tool_page::open_prompt_dialog(editing_id, tool, &mut ws, cx);
            }
        }
        if let Ok(pid) = std::env::var("AITOOLPLUS_EXPAND_PROMPT") {
            ws.ui.expanded_prompts.insert(pid);
        }

        if let Ok(dd) = std::env::var("AITOOLPLUS_OPEN_PI_DROPDOWN") {
            ws.ui.pi_dropdown_open = match dd.as_str() {
                "prov" => Some(pages::PiDropdownField::Provider),
                "model" => Some(pages::PiDropdownField::Model),
                "think" => Some(pages::PiDropdownField::Thinking),
                _ => None,
            };
        }

        ws
    }

    // -- navigation ---------------------------------------------------------

    pub fn navigate(&mut self, page: Page, cx: &mut Context<Self>) {
        if self.page == page {
            return;
        }
        self.page = page;
        self.settings.last_page = page.key().to_string();
        (self.callbacks.save_settings)(&self.settings);
        self.ui.on_page_change(page, cx);
        cx.notify();
    }

    pub fn cycle_theme(&mut self, cx: &mut Context<Self>) {
        self.settings.theme_mode = match self.settings.theme_mode {
            aitoolplus_core::settings::ThemeMode::System => {
                aitoolplus_core::settings::ThemeMode::Dark
            }
            aitoolplus_core::settings::ThemeMode::Dark => {
                aitoolplus_core::settings::ThemeMode::Light
            }
            aitoolplus_core::settings::ThemeMode::Light => {
                aitoolplus_core::settings::ThemeMode::System
            }
        };
        self.apply_theme(cx);
    }

    pub fn cycle_language(&mut self, cx: &mut Context<Self>) {
        self.settings.language = match self.settings.language {
            CoreLanguage::System => CoreLanguage::Zh,
            CoreLanguage::Zh => CoreLanguage::En,
            CoreLanguage::En => CoreLanguage::System,
        };
        self.i18n = I18n::new(self.settings.language);
        (self.callbacks.save_settings)(&self.settings);
        cx.notify();
    }

    fn apply_theme(&mut self, cx: &mut Context<Self>) {
        self.theme = Theme::for_mode(self.settings.theme_mode, system_prefers_dark());
        let kit_mode = if self.theme.is_dark {
            gpui_kit::component::ThemeMode::Dark
        } else {
            gpui_kit::component::ThemeMode::Light
        };
        gpui_kit::component::Theme::change(kit_mode, None, cx);
        (self.callbacks.save_settings)(&self.settings);
        cx.notify();
    }

    // -- store helpers -----------------------------------------------------

    pub fn tool_providers(&self, tool: ToolId) -> Vec<ProviderRecord> {
        aitoolplus_core::providers::list(&self.store.store().tool(tool).providers)
    }

    pub fn persist_store(&self) {
        if let Err(e) = self.store.save() {
            tracing::error!("failed to save store: {e}");
            (self.callbacks.notify)(format!("save failed: {e}"));
        }
        (self.callbacks.store_changed)(self.store.store());
    }

    /// The scrollable content area for the current page.
    pub(crate) fn page_content(&mut self, cx: &mut Context<Self>) -> gpui::AnyElement {
        let is_session_open = self.ui.open_session.is_some();
        let is_custom_scroll = is_session_open
            || (matches!(self.page, Page::Tool(_))
                && matches!(self.ui.tool_tab, pages::ToolTab::Marketplace | pages::ToolTab::Sessions))
            || matches!(self.page, Page::Sessions);

        let mut pane = if is_session_open {
            div()
                .id("page-scroll")
                .flex()
                .flex_col()
                .flex_1()
                .min_w(px(0.0))
                .h_full()
                .overflow_hidden()
                .px(px(14.0))
                .py(px(10.0))
                .items_center()
                .child(
                    div()
                        .w_full()
                        .h_full()
                        .min_w(px(0.0))
                        .min_h(px(0.0))
                        .flex()
                        .flex_col()
                        .child(pages::render_page(self, cx)),
                )
        } else if is_custom_scroll {
            div()
                .id("page-scroll")
                .flex()
                .flex_col()
                .flex_1()
                .min_w(px(0.0))
                .h_full()
                .overflow_hidden()
                .px(px(24.0))
                .py(px(20.0))
                .items_center()
                .child(
                    div()
                        .w_full()
                        .max_w(px(1120.0))
                        .h_full()
                        .min_w(px(0.0))
                        .min_h(px(0.0))
                        .flex()
                        .flex_col()
                        .child(pages::render_page(self, cx)),
                )
        } else {
            div()
                .id("page-scroll")
                .flex()
                .flex_col()
                .flex_1()
                .min_w(px(0.0))
                .overflow_x_hidden()
                .overflow_y_scroll()
                .px(px(24.0))
                .py(px(20.0))
                .items_center()
                .child(
                    div()
                        .w_full()
                        .max_w(px(1120.0))
                        .min_w(px(0.0))
                        .child(pages::render_page(self, cx)),
                )
        };

        if let Some(toast) = pages::render_toast(self, cx) {
            pane = pane.child(toast);
        }
        pane.into_any_element()
    }

    /// Apply the selected provider of `tool` to its real config files.
    pub fn apply_provider(&mut self, tool: ToolId, provider_id: &str, cx: &mut Context<Self>) {
        let i = self.i18n;
        let result = self.store.update(|store| {
            let section = store.tool_mut(tool);
            // mark applied
            aitoolplus_core::providers::select(&mut section.providers, provider_id);
        });
        if let Err(e) = result {
            let msg = format!("{e}");
            (self.callbacks.notify)(msg);
            cx.notify();
            return;
        }

        let common = self.store.store().tool(tool).common_config.clone();
        let provider = self
            .store
            .store()
            .tool(tool)
            .providers
            .iter()
            .find(|p| p.id == provider_id)
            .cloned();

        let Some(provider) = provider else {
            let msg = i.t("未找到该供应商", "provider not found").to_string();
            (self.callbacks.notify)(msg);
            cx.notify();
            return;
        };

        let adapter = adapter_for(tool);
        let ctx = ApplyCtx {
            paths: &self.paths,
            common_config: &common,
            provider: &provider,
            strategy: MergeStrategy::default(),
            provider_optional: false,
        };
        match adapter.apply(&ctx) {
            Ok(report) => {
                let msg = i.t(
                    &format!("已应用 {}（{} 个文件）", provider.name, report.files.len()),
                    &format!("applied {} ({} files)", provider.name, report.files.len()),
                );
                (self.callbacks.notify)(msg.to_string());
            }
            Err(e) => {
                let msg = i.t(&format!("应用失败：{e}"), &format!("apply failed: {e}"));
                (self.callbacks.notify)(msg.to_string());
            }
        }
        self.persist_store();
        cx.notify();
    }

    /// Toggle or set a Pi provider's enabled state in models.json and auth.json.
    pub fn set_pi_provider_enabled(&mut self, provider_id: &str, enabled: bool, cx: &mut Context<Self>) {
        let i = self.i18n;
        let provider = self
            .store
            .store()
            .tool(ToolId::Pi)
            .providers
            .iter()
            .find(|p| p.id == provider_id)
            .cloned();

        let Some(provider) = provider else {
            let msg = i.t("未找到该供应商", "provider not found").to_string();
            (self.callbacks.notify)(msg);
            cx.notify();
            return;
        };

        match aitoolplus_core::pi_runtime::set_provider_enabled(&self.paths, &provider, enabled) {
            Ok(files) => {
                let _ = self.store.update(|store| {
                    let section = store.tool_mut(ToolId::Pi);
                    if let Some(p) = section.providers.iter_mut().find(|p| p.id == provider_id) {
                        p.is_applied = enabled;
                    }
                });
                self.persist_store();
                let msg = if enabled {
                    i.t("供应商已启用", "Provider enabled").to_string()
                } else {
                    i.t("供应商已停用", "Provider disabled").to_string()
                };
                self.ui.toast(msg, false);
                for f in files {
                    (self.callbacks.notify)(format!("runtime updated: {}", f.display()));
                }
            }
            Err(e) => {
                let msg = format!("{e}");
                self.ui.toast(msg, true);
            }
        }
        cx.notify();
    }
}

/// Whether the OS prefers dark (cheap heuristic; Windows registry-free).
fn system_prefers_dark() -> bool {
    #[cfg(target_os = "windows")]
    {
        // AppsUseLightTheme == 0 means dark
        use windows::Win32::System::Registry::RegGetValueW;
        use windows::core::PCWSTR;
        let key: Vec<u16> = "Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize"
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();
        let value: Vec<u16> = "AppsUseLightTheme"
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();
        let mut data: u32 = 0;
        let mut size = std::mem::size_of::<u32>() as u32;
        let ok = unsafe {
            RegGetValueW(
                windows::Win32::System::Registry::HKEY_CURRENT_USER,
                PCWSTR(key.as_ptr()),
                PCWSTR(value.as_ptr()),
                windows::Win32::System::Registry::REG_ROUTINE_FLAGS(2), // RRF_RT_REG_DWORD
                None,
                Some(&mut data as *mut u32 as _),
                Some(&mut size),
            )
        };
        ok == windows::Win32::Foundation::ERROR_SUCCESS && data == 0
    }
    #[cfg(not(target_os = "windows"))]
    {
        false
    }
}

/// Public re-export for pages modules.
pub fn system_prefers_dark_pub() -> bool {
    system_prefers_dark()
}

impl gpui::Focusable for Workspace {
    fn focus_handle(&self, _cx: &gpui::App) -> gpui::FocusHandle {
        // The workspace delegates focus to whichever page input is active;
        // a root handle keeps window focus requests valid.
        self.root_focus.clone()
    }
}

impl Render for Workspace {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let sidebar = self.sidebar(cx);
        let is_session_open = self.ui.open_session.is_some();
        let topbar = if is_session_open {
            None
        } else {
            Some(self.topbar(cx))
        };
        let content = self.page_content(cx);

        let mut right_col = div()
            .flex_1()
            .min_w(px(0.0))
            .h_full()
            .flex()
            .flex_col();

        if let Some(tb) = topbar {
            right_col = right_col.child(tb);
        }
        right_col = right_col.child(content);

        let base = div()
            .size_full()
            .flex()
            .bg(self.theme.bg)
            .text_color(self.theme.text_primary)
            .child(sidebar)
            .child(right_col);

        // Standard gpui-kit Root overlay layers: dialogs, sheets, notifications
        let overlays = div()
            .children(gpui_kit::component::Root::render_dialog_layer(window, cx))
            .children(gpui_kit::component::Root::render_sheet_layer(window, cx))
            .children(gpui_kit::component::Root::render_notification_layer(window, cx));

        // Modals render at the root as overlays (flyclip GPUI guideline #3).
        let modal_open = self.ui.modal_active();
        let modals = if modal_open {
            // clone dialog states so they persist across frames until explicitly closed
            let dialogs = self.ui.provider_dialog.clone();
            let prompts = self.ui.prompt_dialog.clone();
            let mcps = self.ui.mcp_dialog.clone();
            let confirms = self.ui.confirm.clone();
            let renames = self.ui.rename_dialog.clone();
            let runtime_edits = self.ui.runtime_edit_dialog.clone();
            let skill_details = self.ui.skill_detail_dialog.clone();
            let antigravity_dialog = self.ui.antigravity_dialog.clone();
            let mut out = vec![];
            if let Some(d) = dialogs {
                out.push(pages::tool_page::render_provider_dialog(d, self, cx));
            }
            if let Some(d) = prompts {
                out.push(pages::tool_page::render_prompt_dialog(d, self, cx));
            }
            if let Some(d) = mcps {
                out.push(pages::mcp_page::render_mcp_dialog(d, self, cx));
            }
            if let Some(d) = confirms {
                out.push(pages::render_confirm_dialog(d, self, cx));
            }
            if let Some((meta, input)) = renames {
                out.push(pages::sessions_page::render_rename_dialog(
                    meta, input, self, cx,
                ));
            }
            if let Some((path, editor)) = runtime_edits {
                out.push(pages::tool_page::render_runtime_edit_dialog(
                    path, editor, self, cx,
                ));
            }
            if let Some(detail) = skill_details {
                out.push(pages::skills_page::render_skill_detail_dialog(
                    detail, self, cx,
                ));
            }
            if let Some(d) = antigravity_dialog {
                out.push(pages::antigravity_page::render_add_account_dialog(&d, self, cx));
            }
            out
        } else {
            vec![]
        };
        if modal_open {
            div()
                .relative()
                .size_full()
                .child(base)
                .children(modals)
                .child(overlays)
        } else {
            div().relative().size_full().child(base).child(overlays)
        }
    }
}
