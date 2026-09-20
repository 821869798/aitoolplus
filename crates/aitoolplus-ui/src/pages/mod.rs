//! Feature pages: tool pages (providers/common/prompts), MCP, Skills,
//! Sessions, Settings.

pub mod antigravity_page;
pub mod mcp_page;
pub mod session_detail;
pub mod sessions_page;
pub mod settings_page;
pub mod skills_page;
pub mod tool_page;

use aitoolplus_core::tools::ToolId;
use gpui::{ClickEvent, Context, IntoElement, MouseButton, Window, div, prelude::*, px};

use crate::components::{ButtonVariant, button_l};
use crate::text_area::TextArea;
use crate::text_input::TextInput;
use crate::theme::Theme;
use crate::workspace::Workspace;

/// Which top-level page is showing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Page {
    Tool(ToolId),
    Mcp,
    Skills,
    Sessions,
    Antigravity,
    Settings,
}

impl Page {
    pub fn key(self) -> &'static str {
        match self {
            Page::Tool(t) => t.key(),
            Page::Mcp => "mcp",
            Page::Skills => "skills",
            Page::Sessions => "sessions",
            Page::Antigravity => "antigravity",
            Page::Settings => "settings",
        }
    }
}

/// Tabs within a tool page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolTab {
    Providers,
    Common,
    Prompts,
    Runtime,
    /// Pi extension management (packages + local .ts).
    Extensions,
    /// Claude Code/Codex/Grok plugin management.
    Plugins,
    /// Claude Code/Codex/Grok marketplace discovery.
    Marketplace,
    /// OpenCode companion profiles (OpenAgent / Slim).
    Addons,
    /// Agent session management.
    Sessions,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PiDropdownField {
    Provider,
    Model,
    Thinking,
}

/// All transient UI state owned by the workspace.
pub struct WorkspaceState {
    pub tool_tab: ToolTab,
    /// JSON text buffers for the common-config editor, per tool.
    pub common_editors: std::collections::BTreeMap<String, gpui::Entity<TextArea>>,
    pub common_dirty: bool,
    /// Provider edit dialog state.
    pub provider_dialog: Option<ProviderDialogState>,
    /// Prompt edit dialog state.
    pub prompt_dialog: Option<PromptDialogState>,
    /// Which session is expanded (tool key + session id).
    pub open_session: Option<(ToolId, String)>,
    /// MCP editor dialog.
    pub mcp_dialog: Option<McpDialogState>,
    /// Delete confirmations (title, message, action id).
    pub confirm: Option<ConfirmState>,
    /// Settings page: import/export feedback.
    pub settings_tab: SettingsTab,
    pub skills_tool_filter: Option<ToolId>,
    pub sessions_tool: ToolId,
    pub session_search: gpui::Entity<TextInput>,
    /// Sticky status message.
    pub toast: Option<Toast>,
    /// Whether MCP discovery has already run this session.
    pub mcp_discovered: bool,
    /// Whether the skills central repo has been scanned this session.
    pub skills_discovered: bool,
    /// Session rename dialog: (meta, title input).
    pub rename_dialog: Option<(
        aitoolplus_core::session::SessionMeta,
        gpui::Entity<TextInput>,
    )>,
    /// Pi Model Settings transient selections (None = use on-disk value).
    pub pi_ms_provider: Option<String>,
    pub pi_ms_model: Option<String>,
    pub pi_ms_thinking: Option<String>,
    pub webdav_inputs: Option<WebDavInputs>,
    pub s3_inputs: Option<S3Inputs>,
    pub restore_conflict_strategy: aitoolplus_core::backup::ConflictStrategy,
    pub restore_allow_custom_absolute: bool,
    pub downloaded_update_asset_path: Option<std::path::PathBuf>,
    pub provider_test_results:
        std::collections::BTreeMap<String, aitoolplus_core::api_hub::ConnectivityResult>,
    pub update_info: Option<aitoolplus_core::updater::UpdateInfo>,
    pub remote_backups: Vec<aitoolplus_core::webdav::RemoteBackup>,
    pub backup_custom_inputs: Option<BackupCustomInputs>,
    pub tool_root_inputs: std::collections::BTreeMap<String, gpui::Entity<TextInput>>,
    pub addon_editors: std::collections::BTreeMap<String, gpui::Entity<TextArea>>,
    pub skill_git_url: gpui::Entity<TextInput>,
    pub proxy_url_input: gpui::Entity<TextInput>,
    pub cli_path_inputs: std::collections::BTreeMap<String, gpui::Entity<TextInput>>,
    pub pi_extensions:
        Option<Result<aitoolplus_core::pi_extensions::PiExtensionListResult, String>>,
    pub pi_extensions_loading: bool,
    pub pi_extension_input: gpui::Entity<TextInput>,
    pub omp_extensions:
        Option<Result<aitoolplus_core::pi_extensions::PiExtensionListResult, String>>,
    pub omp_extensions_loading: bool,
    pub omp_extension_input: gpui::Entity<TextInput>,
    pub grok_plugins: Option<
        Result<
            (
                Vec<aitoolplus_core::grok_plugins::GrokPlugin>,
                Vec<aitoolplus_core::grok_plugins::GrokPlugin>,
            ),
            String,
        >,
    >,
    pub grok_plugins_loading: bool,
    pub claude_marketplaces_input: gpui::Entity<TextInput>,
    pub claude_installed_search: gpui::Entity<TextInput>,
    pub claude_market_search: gpui::Entity<TextInput>,
    pub claude_market_page: usize,
    pub claude_market_page_size: usize,
    pub claude_market_scroll_handle: gpui::UniformListScrollHandle,
    pub claude_marketplaces_expanded: bool,
    pub claude_plugins_tab: ClaudePluginsTab,
    pub claude_plugins:
        Option<Result<aitoolplus_core::claude_plugins::ClaudePluginsData, String>>,
    pub claude_plugins_loading: bool,
    pub pi_other_editor: Option<gpui::Entity<TextArea>>,
    pub runtime_files_cache: Option<(ToolId, Vec<(String, std::path::PathBuf, bool, String)>)>,
    pub runtime_edit_dialog: Option<(std::path::PathBuf, gpui::Entity<TextArea>)>,
    pub prompt_search: gpui::Entity<TextInput>,
    pub mcp_search: gpui::Entity<TextInput>,
    pub skill_search: gpui::Entity<TextInput>,
    pub skill_detail_dialog: Option<SkillDetailState>,
    pub expanded_prompts: std::collections::HashSet<String>,
    pub pi_ms_initialized: bool,
    pub pi_ms_provider_input: gpui::Entity<TextInput>,
    pub pi_ms_model_input: gpui::Entity<TextInput>,
    pub pi_ms_thinking_input: gpui::Entity<TextInput>,
    pub pi_dropdown_open: Option<PiDropdownField>,
    pub pi_dropdown_typing: bool,
    pub pi_dropdown_search: gpui::Entity<TextInput>,
    pub pi_dropdown_just_closed: Option<(PiDropdownField, std::time::Instant)>,
    pub codex_plugins: Option<
        Result<
            aitoolplus_core::codex_plugins::CodexPluginData,
            String,
        >,
    >,
    pub codex_plugins_loading: bool,
    pub codex_installed_search: gpui::Entity<TextInput>,
    pub codex_market_search: gpui::Entity<TextInput>,
    pub grok_installed_search: gpui::Entity<TextInput>,
    pub grok_market_search: gpui::Entity<TextInput>,
    pub agent_session_search: gpui::Entity<TextInput>,
    pub agent_sessions: Option<(ToolId, Vec<aitoolplus_core::session::SessionMeta>)>,
    pub agent_sessions_loading: bool,
    pub antigravity_store: Option<aitoolplus_core::antigravity::AntigravityStore>,
    pub antigravity_loading: bool,
    pub antigravity_dialog: Option<antigravity_page::AntigravityDialogState>,
    pub antigravity_refreshing_all: bool,
    pub antigravity_search: gpui::Entity<TextInput>,
    pub session_messages_cache: Option<(ToolId, String, std::sync::Arc<Vec<aitoolplus_core::session::SessionMessage>>)>,
    pub session_actions_menu_open: bool,
    pub session_expanded_blocks: std::collections::HashSet<String>,
    pub session_expanded_thinkings: std::collections::HashSet<String>,
    pub session_expanded_outputs: std::collections::HashSet<String>,
    pub session_message_limit: usize,
    pub session_list_state: Option<(ToolId, String, (bool, bool, bool, bool, bool, bool), gpui::ListState)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ClaudePluginsTab {
    #[default]
    Installed,
    Marketplaces,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsTab {
    General,
    Backup,
    About,
}

pub struct Toast {
    pub message: String,
    pub error: bool,
}

pub struct BackupCustomInputs {
    pub source: gpui::Entity<TextInput>,
    pub restore: gpui::Entity<TextInput>,
}

pub struct WebDavInputs {
    pub url: gpui::Entity<TextInput>,
    pub username: gpui::Entity<TextInput>,
    pub password: gpui::Entity<TextInput>,
    pub remote_directory: gpui::Entity<TextInput>,
}

pub struct S3Inputs {
    pub endpoint: gpui::Entity<TextInput>,
    pub region: gpui::Entity<TextInput>,
    pub bucket: gpui::Entity<TextInput>,
    pub access_key_id: gpui::Entity<TextInput>,
    pub secret_access_key: gpui::Entity<TextInput>,
    pub prefix: gpui::Entity<TextInput>,
}

#[derive(Clone)]
pub struct PiModelDraft {
    pub key: String,
    pub id: gpui::Entity<TextInput>,
    pub name: gpui::Entity<TextInput>,
    pub reasoning: bool,
    pub image_input: bool,
    pub context_window: gpui::Entity<TextInput>,
    pub max_tokens: gpui::Entity<TextInput>,
    pub is_expanded: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ProviderDialogTab {
    #[default]
    Connection,
    Advanced,
}

#[derive(Clone)]
pub struct CustomHeaderDraft {
    pub key: gpui::Entity<TextInput>,
    pub value: gpui::Entity<TextInput>,
}

#[derive(Clone)]
pub struct ModelRewriteDraft {
    pub from: gpui::Entity<TextInput>,
    pub to: gpui::Entity<TextInput>,
}

#[derive(Clone)]
pub struct ProviderDialogState {
    /// None = create mode.
    pub editing_id: Option<String>,
    pub tool: ToolId,
    pub active_tab: ProviderDialogTab,
    pub name: gpui::Entity<TextInput>,
    pub category: String,

    // Core Connection
    pub base_url: gpui::Entity<TextInput>,
    pub api_key: gpui::Entity<TextInput>,
    pub show_api_key: bool,
    pub api_format: String,

    // Model configuration
    pub model: gpui::Entity<TextInput>,
    pub sonnet_model: gpui::Entity<TextInput>,
    pub sonnet_name: gpui::Entity<TextInput>,
    pub opus_model: gpui::Entity<TextInput>,
    pub opus_name: gpui::Entity<TextInput>,
    pub haiku_model: gpui::Entity<TextInput>,
    pub haiku_name: gpui::Entity<TextInput>,
    pub fable_model: gpui::Entity<TextInput>,
    pub fable_name: gpui::Entity<TextInput>,
    pub subagent_model: gpui::Entity<TextInput>,
    pub sonnet_1m: bool,
    pub opus_1m: bool,
    pub haiku_1m: bool,
    pub fable_1m: bool,
    pub subagent_1m: bool,

    // Pi specific
    pub pi_provider_key: gpui::Entity<TextInput>,
    pub pi_api_format: String,
    pub pi_models: Vec<PiModelDraft>,

    // Codex specific
    pub codex_wire_api: String,
    pub codex_reasoning_effort: String,

    // Meta & Advanced
    pub notes: gpui::Entity<TextInput>,
    pub website: gpui::Entity<TextInput>,
    pub preset_index: Option<usize>,
    pub advanced_expanded: bool,
    pub custom_user_agent: gpui::Entity<TextInput>,
    pub custom_headers_list: Vec<CustomHeaderDraft>,
    pub billing_enabled: bool,
    pub cost_multiplier: gpui::Entity<TextInput>,
    pub pricing_model_source: String,
    pub model_rewrites: Vec<ModelRewriteDraft>,
    pub custom_headers: gpui::Entity<TextInput>,
    pub settings: gpui::Entity<TextArea>,

    // Upstream Model Discovery
    pub fetched_models: Vec<aitoolplus_core::api_hub::FetchedModel>,
    pub is_fetching_models: bool,
    pub fetch_error: Option<String>,
    pub active_model_dropdown: Option<String>,
    pub model_search: gpui::Entity<TextInput>,
    pub model_bounds: std::collections::BTreeMap<String, gpui::Bounds<gpui::Pixels>>,
}

#[derive(Clone)]
pub struct PromptDialogState {
    pub editing_id: Option<String>,
    pub tool: ToolId,
    pub name: gpui::Entity<TextInput>,
    pub content: gpui::Entity<TextArea>,
}

#[derive(Clone)]
pub struct McpDialogState {
    pub editing_id: Option<String>,
    pub name: gpui::Entity<TextInput>,
    pub server_type: aitoolplus_core::mcp::McpServerType,
    pub command: gpui::Entity<TextInput>,
    pub args: gpui::Entity<TextInput>,
    pub environment: gpui::Entity<TextInput>,
    pub url: gpui::Entity<TextInput>,
    pub headers: gpui::Entity<TextInput>,
    pub timeout_seconds: gpui::Entity<TextInput>,
    pub group: gpui::Entity<TextInput>,
    pub enabled_tools: Vec<ToolId>,
}

#[derive(Clone)]
pub struct ConfirmState {
    pub title: String,
    pub message: String,
    pub action: ConfirmAction,
}

#[derive(Clone)]
pub struct SkillDetailState {
    pub skill_id: String,
    pub name: String,
    pub path: std::path::PathBuf,
    pub skill_md: String,
    pub tools: Vec<ToolId>,
}

#[derive(Clone)]
pub enum ConfirmAction {
    DeleteProvider { tool: ToolId, id: String },
    DeletePrompt { tool: ToolId, id: String },
    DeleteSession { tool: ToolId, id: String },
    DeleteMcp { id: String },
    DeleteSkill { id: String },
    DeleteAntigravityAccount { id: String },
}

impl WorkspaceState {
    pub fn new(cx: &mut Context<Workspace>) -> Self {
        let session_search =
            cx.new(|cx| TextInput::new(crate::pages::workspace_search_placeholder(), cx));
        let skill_git_url = cx.new(|cx| TextInput::new("https://github.com/owner/skill.git", cx));
        let proxy_url_input = cx.new(|cx| TextInput::new("http://127.0.0.1:7890", cx));
        let pi_extension_input =
            cx.new(|cx| TextInput::new("来源，如 npm:pi-mcp-adapter", cx));
        let omp_extension_input =
            cx.new(|cx| TextInput::new("来源，如 npm:context-mode", cx));
        let claude_marketplaces_input =
            cx.new(|cx| TextInput::new("来源，如 anthropics/claude-code", cx));
        let claude_installed_search = cx.new(|cx| TextInput::new("搜索已安装插件…", cx));
        let claude_market_search = cx.new(|cx| TextInput::new("搜索市场插件…", cx));
        let prompt_search = cx.new(|cx| TextInput::new("搜索全局提示词…", cx));
        let mcp_search = cx.new(|cx| TextInput::new("搜索 MCP 服务器…", cx));
        let skill_search = cx.new(|cx| TextInput::new("搜索 Skill 技能…", cx));
        let pi_dropdown_search = cx.new(|cx| TextInput::new("输入搜索…", cx));
        cx.subscribe(&pi_dropdown_search, |this, _emitter, event: &crate::text_input::TextInputEvent, cx| {
            match event {
                crate::text_input::TextInputEvent::Escape => {
                    this.ui.pi_dropdown_open = None;
                    cx.notify();
                }
                _ => {
                    cx.notify();
                }
            }
        }).detach();
        let pi_ms_provider_input = cx.new(|cx| {
            let mut inp = TextInput::new("请选择或输入默认供应商…", cx);
            inp.set_borderless(true);
            inp
        });
        let pi_ms_model_input = cx.new(|cx| {
            let mut inp = TextInput::new("请选择或输入默认模型…", cx);
            inp.set_borderless(true);
            inp
        });
        let pi_ms_thinking_input = cx.new(|cx| {
            let mut inp = TextInput::new("请选择思考等级…", cx);
            inp.set_borderless(true);
            inp
        });

        cx.subscribe(&pi_ms_provider_input, |this, _emitter, event: &crate::text_input::TextInputEvent, cx| {
            match event {
                crate::text_input::TextInputEvent::Change(text) => {
                    this.ui.pi_dropdown_open = Some(PiDropdownField::Provider);
                    this.ui.pi_dropdown_typing = true;
                    this.ui.pi_ms_provider = if text.trim().is_empty() { None } else { Some(text.trim().to_string()) };
                    cx.notify();
                }
                crate::text_input::TextInputEvent::Escape | crate::text_input::TextInputEvent::Enter => {
                    this.ui.pi_dropdown_open = None;
                    this.ui.pi_dropdown_typing = false;
                    cx.notify();
                }
            }
        }).detach();

        cx.subscribe(&pi_ms_model_input, |this, _emitter, event: &crate::text_input::TextInputEvent, cx| {
            match event {
                crate::text_input::TextInputEvent::Change(text) => {
                    this.ui.pi_dropdown_open = Some(PiDropdownField::Model);
                    this.ui.pi_dropdown_typing = true;
                    this.ui.pi_ms_model = if text.trim().is_empty() { None } else { Some(text.trim().to_string()) };
                    cx.notify();
                }
                crate::text_input::TextInputEvent::Escape | crate::text_input::TextInputEvent::Enter => {
                    this.ui.pi_dropdown_open = None;
                    this.ui.pi_dropdown_typing = false;
                    cx.notify();
                }
            }
        }).detach();

        cx.subscribe(&pi_ms_thinking_input, |this, _emitter, event: &crate::text_input::TextInputEvent, cx| {
            match event {
                crate::text_input::TextInputEvent::Change(text) => {
                    this.ui.pi_dropdown_open = Some(PiDropdownField::Thinking);
                    this.ui.pi_dropdown_typing = true;
                    this.ui.pi_ms_thinking = if text.trim().is_empty() { None } else { Some(text.trim().to_string()) };
                    cx.notify();
                }
                crate::text_input::TextInputEvent::Escape | crate::text_input::TextInputEvent::Enter => {
                    this.ui.pi_dropdown_open = None;
                    this.ui.pi_dropdown_typing = false;
                    cx.notify();
                }
            }
        }).detach();

        let codex_installed_search = cx.new(|cx| TextInput::new("搜索已安装插件…", cx));
        let codex_market_search = cx.new(|cx| TextInput::new("搜索市场插件…", cx));
        let grok_installed_search = cx.new(|cx| TextInput::new("搜索已安装插件…", cx));
        let grok_market_search = cx.new(|cx| TextInput::new("搜索市场插件…", cx));
        let agent_session_search = cx.new(|cx| TextInput::new("搜索会话（标题、ID、项目路径、摘要）…", cx));
        let antigravity_search = cx.new(|cx| TextInput::new("搜索账号（邮箱、备注）…", cx));
        Self {
            tool_tab: ToolTab::Providers,
            common_editors: Default::default(),
            common_dirty: false,
            provider_dialog: None,
            prompt_dialog: None,
            open_session: None,
            mcp_dialog: None,
            confirm: None,
            settings_tab: SettingsTab::General,
            skills_tool_filter: None,
            sessions_tool: std::env::var("AITOOLPLUS_SESSIONS_TOOL")
                .ok()
                .and_then(|k| ToolId::from_key(&k))
                .unwrap_or(ToolId::ClaudeCode),
            session_search,
            toast: None,
            mcp_discovered: false,
            skills_discovered: false,
            rename_dialog: None,
            pi_ms_initialized: false,
            pi_ms_provider_input,
            pi_ms_model_input,
            pi_ms_thinking_input,
            pi_ms_provider: None,
            pi_ms_model: None,
            pi_ms_thinking: None,
            webdav_inputs: None,
            s3_inputs: None,
            restore_conflict_strategy: aitoolplus_core::backup::ConflictStrategy::Overwrite,
            restore_allow_custom_absolute: false,
            downloaded_update_asset_path: None,
            provider_test_results: Default::default(),
            update_info: None,
            remote_backups: vec![],
            backup_custom_inputs: None,
            tool_root_inputs: Default::default(),
            addon_editors: Default::default(),
            skill_git_url,
            proxy_url_input,
            cli_path_inputs: Default::default(),
            pi_extensions: None,
            pi_extensions_loading: false,
            pi_extension_input,
            omp_extensions: None,
            omp_extensions_loading: false,
            omp_extension_input,
            grok_plugins: None,
            grok_plugins_loading: false,
            claude_marketplaces_input,
            claude_installed_search,
            claude_market_search,
            claude_market_page: 0,
            claude_market_page_size: 20,
            claude_market_scroll_handle: gpui::UniformListScrollHandle::new(),
            claude_marketplaces_expanded: false,
            claude_plugins_tab: ClaudePluginsTab::Installed,
            claude_plugins: None,
            claude_plugins_loading: false,
            pi_other_editor: None,
            runtime_files_cache: None,
            runtime_edit_dialog: None,
            prompt_search,
            mcp_search,
            skill_search,
            skill_detail_dialog: None,
            expanded_prompts: std::collections::HashSet::new(),
            pi_dropdown_open: None,
            pi_dropdown_typing: false,
            pi_dropdown_search,
            pi_dropdown_just_closed: None,
            codex_plugins: None,
            codex_plugins_loading: false,
            codex_installed_search,
            codex_market_search,
            grok_installed_search,
            grok_market_search,
            agent_session_search,
            agent_sessions: None,
            agent_sessions_loading: false,
            antigravity_store: None,
            antigravity_loading: false,
            antigravity_dialog: None,
            antigravity_refreshing_all: false,
            antigravity_search,
            session_messages_cache: None,
            session_actions_menu_open: false,
            session_expanded_blocks: std::collections::HashSet::new(),
            session_expanded_thinkings: std::collections::HashSet::new(),
            session_expanded_outputs: std::collections::HashSet::new(),
            session_message_limit: 80,
            session_list_state: None,
        }
    }

    pub fn pi_other_editor(
        &mut self,
        initial: &str,
        cx: &mut Context<Workspace>,
    ) -> gpui::Entity<TextArea> {
        if let Some(editor) = &self.pi_other_editor {
            return editor.clone();
        }
        let initial = initial.to_string();
        let editor = cx.new(|cx| {
            let mut ta = TextArea::new("{}", cx);
            ta.set_text_silent(initial, cx);
            ta.set_max_lines(10, cx);
            ta
        });
        self.pi_other_editor = Some(editor.clone());
        editor
    }

    pub fn addon_editor(
        &mut self,
        kind: aitoolplus_core::opencode_addons::AddonKind,
        current: &str,
        cx: &mut Context<Workspace>,
    ) -> gpui::Entity<TextArea> {
        let key = kind.key().to_string();
        if let Some(editor) = self.addon_editors.get(&key) {
            return editor.clone();
        }
        let current = current.to_string();
        let editor = cx.new(|cx| {
            let mut editor = TextArea::new("{}", cx);
            editor.set_max_lines(16, cx);
            editor.set_text_silent(current, cx);
            editor
        });
        self.addon_editors.insert(key, editor.clone());
        editor
    }

    pub fn cli_path_input(
        &mut self,
        command: &str,
        current: &str,
        cx: &mut Context<Workspace>,
    ) -> gpui::Entity<TextInput> {
        if let Some(input) = self.cli_path_inputs.get(command) {
            return input.clone();
        }
        let current = current.to_string();
        let input = cx.new(|cx| {
            let mut input = TextInput::new("C:\\path\\to\\cli.exe", cx);
            input.set_text_silent(current, cx);
            input
        });
        self.cli_path_inputs.insert(command.into(), input.clone());
        input
    }

    pub fn tool_root_input(
        &mut self,
        tool: ToolId,
        current: &str,
        cx: &mut Context<Workspace>,
    ) -> gpui::Entity<TextInput> {
        let key = tool.key().to_string();
        if let Some(input) = self.tool_root_inputs.get(&key) {
            return input.clone();
        }
        let current = current.to_string();
        let input = cx.new(|cx| {
            let mut input = TextInput::new("D:\\custom\\tool-root", cx);
            input.set_text_silent(current, cx);
            input
        });
        self.tool_root_inputs.insert(key, input.clone());
        input
    }

    pub fn backup_custom_inputs(&mut self, cx: &mut Context<Workspace>) -> &BackupCustomInputs {
        if self.backup_custom_inputs.is_none() {
            self.backup_custom_inputs = Some(BackupCustomInputs {
                source: cx.new(|cx| TextInput::new("D:\\path\\to\\file-or-folder", cx)),
                restore: cx.new(|cx| TextInput::new("可选恢复路径 / optional restore path", cx)),
            });
        }
        self.backup_custom_inputs
            .as_ref()
            .expect("backup custom inputs initialized")
    }

    pub fn webdav_inputs(
        &mut self,
        config: &aitoolplus_core::settings::WebDavConfig,
        cx: &mut Context<Workspace>,
    ) -> &WebDavInputs {
        if self.webdav_inputs.is_none() {
            let url = cx.new(|cx| {
                let mut input =
                    TextInput::new("https://dav.example.com/remote.php/dav/files/user", cx);
                input.set_text_silent(config.url.clone(), cx);
                input
            });
            let username = cx.new(|cx| {
                let mut input = TextInput::new("Username", cx);
                input.set_text_silent(config.username.clone(), cx);
                input
            });
            let password = cx.new(|cx| {
                let mut input = TextInput::new("Password", cx);
                input.set_secret(true, cx);
                input.set_text_silent(config.password.clone(), cx);
                input
            });
            let remote_directory = cx.new(|cx| {
                let mut input = TextInput::new("aitoolplus", cx);
                input.set_text_silent(config.remote_directory.clone(), cx);
                input
            });
            self.webdav_inputs = Some(WebDavInputs {
                url,
                username,
                password,
                remote_directory,
            });
        }
        self.webdav_inputs
            .as_ref()
            .expect("webdav inputs initialized")
    }

    pub fn s3_inputs(
        &mut self,
        config: &aitoolplus_core::settings::S3Config,
        cx: &mut Context<Workspace>,
    ) -> &S3Inputs {
        if self.s3_inputs.is_none() {
            let endpoint = cx.new(|cx| {
                let mut input = TextInput::new("https://s3.amazonaws.com", cx);
                input.set_text_silent(config.endpoint.clone(), cx);
                input
            });
            let region = cx.new(|cx| {
                let mut input = TextInput::new("us-east-1", cx);
                input.set_text_silent(config.region.clone(), cx);
                input
            });
            let bucket = cx.new(|cx| {
                let mut input = TextInput::new("my-backup-bucket", cx);
                input.set_text_silent(config.bucket.clone(), cx);
                input
            });
            let access_key_id = cx.new(|cx| {
                let mut input = TextInput::new("Access Key ID / AKIA...", cx);
                input.set_text_silent(config.access_key_id.clone(), cx);
                input
            });
            let secret_access_key = cx.new(|cx| {
                let mut input = TextInput::new("Secret Access Key", cx);
                input.set_secret(true, cx);
                input.set_text_silent(config.secret_access_key.clone(), cx);
                input
            });
            let prefix = cx.new(|cx| {
                let mut input = TextInput::new("aitoolplus", cx);
                input.set_text_silent(config.prefix.clone(), cx);
                input
            });
            self.s3_inputs = Some(S3Inputs {
                endpoint,
                region,
                bucket,
                access_key_id,
                secret_access_key,
                prefix,
            });
        }
        self.s3_inputs.as_ref().expect("s3 inputs initialized")
    }

    pub fn on_page_change(&mut self, page: Page, _cx: &mut Context<Workspace>) {
        self.toast = None;
        if let Page::Sessions = page {
            self.open_session = None;
        }
        if let Page::Tool(tool) = page {
            let valid_tab = match self.tool_tab {
                ToolTab::Providers | ToolTab::Common | ToolTab::Prompts | ToolTab::Runtime | ToolTab::Sessions => true,
                ToolTab::Extensions => matches!(tool, aitoolplus_core::tools::ToolId::Pi | aitoolplus_core::tools::ToolId::OhMyPi),
                ToolTab::Plugins => matches!(tool, aitoolplus_core::tools::ToolId::ClaudeCode | aitoolplus_core::tools::ToolId::Codex | aitoolplus_core::tools::ToolId::Grok),
                ToolTab::Marketplace => matches!(tool, aitoolplus_core::tools::ToolId::ClaudeCode | aitoolplus_core::tools::ToolId::Codex | aitoolplus_core::tools::ToolId::Grok),
                ToolTab::Addons => tool == aitoolplus_core::tools::ToolId::OpenCode,
            };
            if !valid_tab {
                self.tool_tab = ToolTab::Providers;
            }
        }
    }

    pub fn modal_active(&self) -> bool {
        self.provider_dialog.is_some()
            || self.prompt_dialog.is_some()
            || self.mcp_dialog.is_some()
            || self.confirm.is_some()
            || self.rename_dialog.is_some()
            || self.runtime_edit_dialog.is_some()
            || self.skill_detail_dialog.is_some()
            || self.antigravity_dialog.is_some()
    }

    /// Resolve or lazily create the common-config editor for a tool.
    /// `current` is the persisted common config text, read by the caller.
    pub fn common_editor(
        &mut self,
        tool: ToolId,
        current: &str,
        cx: &mut Context<Workspace>,
    ) -> gpui::Entity<TextArea> {
        let key = tool.key().to_string();
        if let Some(e) = self.common_editors.get(&key) {
            return e.clone();
        }
        let current = current.to_string();
        let editor = cx.new(|cx| {
            let mut ta = TextArea::new("{}", cx);
            ta.set_text_silent(current, cx);
            ta
        });
        self.common_editors.insert(key, editor.clone());
        editor
    }

    pub fn toast(&mut self, message: impl Into<String>, error: bool) {
        self.toast = Some(Toast {
            message: message.into(),
            error,
        });
    }
}

/// Shared modal scaffold: darkened backdrop + centered card.
pub fn modal_scaffold(
    theme: &Theme,
    title: &str,
    body: gpui::AnyElement,
    cx: &mut Context<Workspace>,
    on_close: impl Fn(&mut Workspace, &ClickEvent, &mut Window, &mut Context<Workspace>) + 'static,
) -> gpui::AnyElement {
    modal_scaffold_sized(theme, title, px(560.0), None, body, cx, on_close)
}

/// Shared modal scaffold with configurable card width.
pub fn modal_scaffold_custom(
    theme: &Theme,
    title: &str,
    width: gpui::Pixels,
    body: gpui::AnyElement,
    cx: &mut Context<Workspace>,
    on_close: impl Fn(&mut Workspace, &ClickEvent, &mut Window, &mut Context<Workspace>) + 'static,
) -> gpui::AnyElement {
    modal_scaffold_sized(theme, title, width, Some(px(720.0)), body, cx, on_close)
}

pub fn modal_scaffold_sized(
    theme: &Theme,
    title: &str,
    width: gpui::Pixels,
    height: Option<gpui::Pixels>,
    body: gpui::AnyElement,
    cx: &mut Context<Workspace>,
    on_close: impl Fn(&mut Workspace, &ClickEvent, &mut Window, &mut Context<Workspace>) + 'static,
) -> gpui::AnyElement {
    let t = theme.clone();
    let on_close = std::rc::Rc::new(on_close);
    let on_close_backdrop = on_close.clone();
    let on_close_button = on_close.clone();

    let backdrop = div()
        .id("modal-backdrop")
        .absolute()
        .top(px(0.0))
        .left(px(0.0))
        .size_full()
        .bg(t.modal_backdrop)
        .flex()
        .items_center()
        .justify_center()
        .on_click(cx.listener(move |ws, ev, window, cx| on_close_backdrop(ws, ev, window, cx)));

    let mut dialog = div()
        .id("modal-card")
        .w(width)
        .max_w(gpui::relative(0.92))
        .max_h(gpui::relative(0.90))
        .p(px(20.0))
        .rounded(px(12.0))
        .bg(t.card_bg)
        .border_1()
        .border_color(t.card_border)
        .shadow_lg()
        .flex()
        .flex_col()
        .overflow_hidden()
        .on_mouse_down(MouseButton::Left, |_, _, cx| {
            // stop mouse down inside the dialog from bubbling
            cx.stop_propagation();
        })
        .on_click(|_, _, cx| {
            // stop clicks inside the dialog from closing it
            cx.stop_propagation();
        });

    if let Some(h) = height {
        dialog = dialog.h(h);
    }

    let dialog = dialog
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .mb(px(12.0))
                .flex_shrink_0()
                .child(
                    div()
                        .text_size(px(16.0))
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .text_color(t.text_primary)
                        .child(title.to_string()),
                )
                .child(crate::components::icon_button_svg(
                    "modal-close",
                    crate::icons::X_SVG,
                    "Close",
                    true,
                    theme,
                    cx,
                    move |ws, ev, window, cx| {
                        ws.ui.provider_dialog = None;
                        ws.ui.prompt_dialog = None;
                        ws.ui.mcp_dialog = None;
                        ws.ui.confirm = None;
                        ws.ui.rename_dialog = None;
                        ws.ui.antigravity_dialog = None;
                        ws.ui.skill_detail_dialog = None;
                        ws.ui.runtime_edit_dialog = None;
                        on_close_button(ws, ev, window, cx);
                        cx.notify();
                    },
                )),
        )
        .child(
            div()
                .flex_1()
                .min_h(px(0.0))
                .flex()
                .flex_col()
                .child(body),
        );

    backdrop.child(dialog).into_any_element()
}

pub fn render_confirm_dialog(
    state: ConfirmState,
    ws: &mut Workspace,
    cx: &mut Context<Workspace>,
) -> gpui::AnyElement {
    let t = ws.theme.clone();
    let ConfirmState {
        title,
        message,
        action,
    } = state;
    let i = ws.i18n;
    let danger_label = i.t("删除", "Delete");
    let cancel_label = i.t("取消", "Cancel");

    let body = div()
        .flex()
        .flex_col()
        .gap(px(12.0))
        .child(
            div()
                .text_size(px(13.0))
                .text_color(t.text_secondary)
                .child(message),
        )
        .child(
            div()
                .flex()
                .justify_end()
                .gap(px(8.0))
                .child(button_l(
                    "confirm-cancel",
                    cancel_label,
                    ButtonVariant::Secondary,
                    &t,
                    cx,
                    |ws, _, _, cx| {
                        ws.ui.confirm = None;
                        cx.notify();
                    },
                ))
                .child(button_l(
                    "confirm-ok",
                    danger_label,
                    ButtonVariant::Danger,
                    &t,
                    cx,
                    move |ws, _, _, cx| {
                        ws.ui.confirm = None;
                        execute_confirm(action.clone(), ws, cx);
                    },
                )),
        );

    modal_scaffold_sized(&t, &title, px(420.0), None, body.into_any_element(), cx, |ws, _, _, cx| {
        ws.ui.confirm = None;
        cx.notify();
    })
}

fn execute_confirm(action: ConfirmAction, ws: &mut Workspace, cx: &mut Context<Workspace>) {
    let i = ws.i18n;
    match action {
        ConfirmAction::DeleteProvider { tool, id } => {
            let _ = ws.store.update(|store| {
                let section = store.tool_mut(tool);
                aitoolplus_core::providers::delete(&mut section.providers, &id);
            });
            ws.persist_store();
            let msg = i.t("供应商已删除", "provider deleted").to_string();
            ws.ui.toast(msg, false);
        }
        ConfirmAction::DeletePrompt { tool, id } => {
            let _ = ws.store.update(|store| {
                let section = store.tool_mut(tool);
                aitoolplus_core::prompt::delete(&mut section.prompts, &id);
            });
            ws.persist_store();
            let msg = i.t("Prompt 已删除", "prompt deleted").to_string();
            ws.ui.toast(msg, false);
        }
        ConfirmAction::DeleteSession { tool, id } => {
            let sessions = aitoolplus_core::session::cached_scan(&ws.paths, tool, 500);
            if let Some(meta) = sessions.iter().find(|s| s.session_id == id) {
                let _ = aitoolplus_core::session::delete_session(meta);
                aitoolplus_core::session::invalidate_cache();
                ws.ui.agent_sessions = None;
                if ws.ui.open_session.as_ref().map(|(t, sid)| *t == tool && sid == &id).unwrap_or(false) {
                    ws.ui.open_session = None;
                }
                let msg = i.t("会话已删除", "session deleted").to_string();
                ws.ui.toast(msg, false);
            }
        }
        ConfirmAction::DeleteMcp { id } => {
            let _ = ws.store.update(|store| {
                aitoolplus_core::mcp::delete(&mut store.mcp, &id);
            });
            ws.persist_store();
            let msg = i.t("MCP 服务器已删除", "MCP server deleted").to_string();
            ws.ui.toast(msg, false);
        }
        ConfirmAction::DeleteSkill { id } => {
            let mut store = ws.store.store().skills.clone();
            let settings = store.settings.clone();
            if let Some(pos) = store.skills.iter().position(|s| s.id == id) {
                let mut skill = store.skills[pos].clone();
                for tool in aitoolplus_core::skills::skills_tools().iter().copied() {
                    let _ = aitoolplus_core::skills::remove_skill_from_tool(&settings, &ws.paths, &mut skill, tool);
                }
                let repo = aitoolplus_core::skills::central_repo_path(&settings, &ws.paths);
                let skill_dir = repo.join(&skill.central_path);
                if skill_dir.exists() {
                    let _ = std::fs::remove_dir_all(&skill_dir);
                }
                store.skills.remove(pos);
                let _ = ws.store.update(|db| db.skills = store);
                ws.persist_store();
                let msg = i.t("Skill 已卸载删除", "skill deleted").to_string();
                ws.ui.toast(msg, false);
            }
        }
        ConfirmAction::DeleteAntigravityAccount { id } => {
            if let Some(mut store) = ws.ui.antigravity_store.clone() {
                if store.remove_account(&id) {
                    let _ = aitoolplus_core::antigravity::save_store(&ws.paths.app_data, &store);
                    ws.ui.antigravity_store = Some(store);
                    let msg = i.t("Antigravity 账号已删除", "Account deleted").to_string();
                    ws.ui.toast(msg, false);
                }
            }
        }
    }
    cx.notify();
}

/// Toast pill pinned bottom-right.
pub fn render_toast(ws: &mut Workspace, _cx: &mut Context<Workspace>) -> Option<gpui::AnyElement> {
    let toast = ws.ui.toast.as_ref()?;
    let t = &ws.theme;
    let (border, bg) = if toast.error {
        (t.danger, t.danger_subtle)
    } else {
        (t.success, t.success_subtle)
    };
    Some(
        div()
            .absolute()
            .bottom(px(16.0))
            .right(px(16.0))
            .px(px(14.0))
            .py(px(10.0))
            .rounded(px(8.0))
            .bg(bg)
            .border_1()
            .border_color(border)
            .text_size(px(12.5))
            .text_color(border)
            .shadow_lg()
            .child(toast.message.clone())
            .into_any_element(),
    )
}

pub fn workspace_search_placeholder() -> &'static str {
    "搜索会话…"
}

/// Page content router.
pub fn render_page(ws: &mut Workspace, cx: &mut Context<Workspace>) -> gpui::AnyElement {
    match ws.page {
        Page::Tool(tool) => tool_page::render_tool_page(tool, ws, cx),
        Page::Mcp => mcp_page::render_mcp_page(ws, cx),
        Page::Skills => skills_page::render_skills_page(ws, cx),
        Page::Sessions => sessions_page::render_sessions_page(ws, cx),
        Page::Antigravity => antigravity_page::render_antigravity_page(ws, cx),
        Page::Settings => settings_page::render_settings_page(ws, cx),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_confirm_state_clone() {
        let confirm = ConfirmState {
            title: "Delete Provider".into(),
            message: "Are you sure?".into(),
            action: ConfirmAction::DeleteProvider {
                tool: ToolId::ClaudeCode,
                id: "prov-1".into(),
            },
        };

        // Ensure ConfirmState derives Clone properly
        let cloned = confirm.clone();
        assert_eq!(cloned.title, confirm.title);
        assert_eq!(cloned.message, confirm.message);
        match cloned.action {
            ConfirmAction::DeleteProvider { tool, id } => {
                assert_eq!(tool, ToolId::ClaudeCode);
                assert_eq!(id, "prov-1");
            }
            _ => panic!("unexpected action"),
        }
    }
}
