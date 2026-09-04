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
        let theme = Theme::for_mode(settings.theme_mode, system_prefers_dark());
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

        let root_focus = cx.focus_handle();
        Self {
            paths,
            store,
            settings,
            theme,
            i18n,
            page,
            ui,
            callbacks,
            root_focus,
        }
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
        let mut pane = div()
            .id("page-scroll")
            .flex_1()
            .min_w(px(0.0))
            .overflow_x_hidden()
            .overflow_y_scroll()
            .p(px(24.0))
            .child(pages::render_page(self, cx));

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
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let base = div()
            .size_full()
            .flex()
            .flex_col()
            .bg(self.theme.bg)
            .text_color(self.theme.text_primary)
            .child(self.topbar(cx))
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_h(px(0.0))
                    .child(self.sidebar(cx))
                    .child(self.page_content(cx)),
            );

        // Modals render at the root as overlays (flyclip GPUI guideline #3).
        let modal_open = self.ui.modal_active();
        let modals = if modal_open {
            // take the dialog states out to avoid double borrows
            let dialogs = std::mem::take(&mut self.ui.provider_dialog);
            let prompts = std::mem::take(&mut self.ui.prompt_dialog);
            let mcps = std::mem::take(&mut self.ui.mcp_dialog);
            let confirms = std::mem::take(&mut self.ui.confirm);
            let renames = std::mem::take(&mut self.ui.rename_dialog);
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
            out
        } else {
            vec![]
        };
        if modal_open {
            div().relative().size_full().child(base).children(modals)
        } else {
            div().size_full().child(base)
        }
    }
}
