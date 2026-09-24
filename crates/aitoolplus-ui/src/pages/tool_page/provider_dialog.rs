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

pub(super) fn strip_1m(model: &str) -> (String, bool) {
    if let Some(stripped) = model.strip_suffix("[1M]") {
        (stripped.trim().to_string(), true)
    } else {
        (model.trim().to_string(), false)
    }
}

pub(super) struct ExtractedProviderConfig {
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

pub(super) fn extract_provider_config(tool: ToolId, raw_json: &str) -> ExtractedProviderConfig {
    let val: Value = serde_json::from_str(raw_json).unwrap_or(Value::Object(Default::default()));
    let default_api_format = match tool {
        ToolId::ClaudeCode | ToolId::ClaudeDesktop => "anthropic",
        ToolId::Codex => "openai_responses",
        ToolId::OpenCode => "openai",
        ToolId::GeminiCli => "gemini",
        ToolId::Pi | ToolId::OhMyPi => "openai-completions",
        _ => "openai",
    };
    let mut cfg = ExtractedProviderConfig {
        base_url: String::new(),
        api_key: String::new(),
        api_format: default_api_format.to_string(),
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
            if cfg.base_url.is_empty() {
                if let Some(v) = val.get("baseUrl").or_else(|| val.get("base_url")).or_else(|| val.get("ANTHROPIC_BASE_URL")).and_then(Value::as_str) {
                    cfg.base_url = v.to_string();
                }
            }
            if cfg.api_key.is_empty() {
                if let Some(v) = val.get("apiKey").or_else(|| val.get("api_key")).or_else(|| val.get("ANTHROPIC_AUTH_TOKEN")).or_else(|| val.get("ANTHROPIC_API_KEY")).and_then(Value::as_str) {
                    cfg.api_key = v.to_string();
                }
            }
        }
        ToolId::Codex => {
            let toml_text = val.get("config").or_else(|| val.get("toml")).and_then(Value::as_str).unwrap_or(raw_json);
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
                            if let Some(w) = tbl.get("wire_api").and_then(|v| v.as_str()) {
                                cfg.codex_wire_api = w.to_string();
                            }
                        }
                    }
                }
            }
            if cfg.api_key.is_empty() {
                if let Some(auth) = val.get("auth").and_then(Value::as_object) {
                    if let Some(k) = auth.get("OPENAI_API_KEY").or_else(|| auth.get("api_key")).or_else(|| auth.get("token")).and_then(Value::as_str) {
                        cfg.api_key = k.trim().to_string();
                    }
                }
            }
            if cfg.base_url.is_empty() {
                if let Some(v) = val.get("baseUrl").or_else(|| val.get("base_url")).and_then(Value::as_str) {
                    cfg.base_url = v.trim().to_string();
                }
            }
            if cfg.api_key.is_empty() {
                if let Some(v) = val.get("apiKey").or_else(|| val.get("api_key")).and_then(Value::as_str) {
                    cfg.api_key = v.trim().to_string();
                }
            }
            if cfg.model.is_empty() {
                if let Some(first_m) = val.get("modelCatalog")
                    .and_then(|mc| mc.get("models"))
                    .and_then(Value::as_array)
                    .and_then(|arr| arr.first())
                    .and_then(|v| v.get("model"))
                    .and_then(Value::as_str)
                {
                    cfg.model = first_m.to_string();
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
            if cfg.api_key.is_empty() {
                if let Some(v) = val.get("apiKey").or_else(|| val.get("api_key")).and_then(Value::as_str) {
                    cfg.api_key = v.to_string();
                }
            }
            if cfg.base_url.is_empty() {
                if let Some(v) = val.get("baseUrl").or_else(|| val.get("base_url")).and_then(Value::as_str) {
                    cfg.base_url = v.to_string();
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
                .or_else(|| val.get("_auth").and_then(|a| a.get("key")))
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
            } else if let Some(first_m) = val.get("models").and_then(Value::as_object).and_then(|m| m.keys().next()) {
                cfg.model = first_m.clone();
            }
        }
    }
    let dummy = aitoolplus_core::providers::ProviderRecord {
        id: String::new(),
        name: String::new(),
        category: "custom".to_string(),
        settings_config: raw_json.to_string(),
        is_applied: false,
        is_disabled: false,
        sort_index: 0,
        notes: None,
        website_url: None,
        meta: None,
        created_at: String::new(),
        updated_at: String::new(),
    };
    let (res_url, res_key) = dummy.resolve_credentials(tool);
    if cfg.base_url.is_empty() {
        cfg.base_url = res_url;
    }
    if cfg.api_key.is_empty() {
        cfg.api_key = res_key;
    }
    if cfg.model.is_empty() {
        cfg.model = dummy.resolve_model(tool);
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
    if let Some(p) = &existing {
        if let Some(fmt) = &p.parsed_meta().api_format {
            extracted.api_format = fmt.clone();
        }
    }
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
            let cw_str = item.get("contextWindow").map(|v| v.to_string()).unwrap_or_else(|| "1000000".into());
            let mt_str = item.get("maxTokens").map(|v| v.to_string()).unwrap_or_else(|| "128000".into());

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
                let mut inp = TextInput::new(i.t("上下文窗口，如 1000000", "Context window"), cx);
                inp.set_text_silent(cw_str, cx);
                inp
            });
            let mt_ent = cx.new(|cx| {
                let mut inp = TextInput::new(i.t("最大输出，如 128000", "Max tokens"), cx);
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
            let mut inp = TextInput::new(i.t("上下文窗口，如 1000000", "Context window"), cx);
            inp.set_text_silent("1000000", cx);
            inp
        });
        let mt_ent = cx.new(|cx| {
            let mut inp = TextInput::new(i.t("最大输出，如 128000", "Max tokens"), cx);
            inp.set_text_silent("128000", cx);
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

    let mut codex_catalog_models = Vec::new();
    if tool == ToolId::Codex {
        if let Ok(val) = serde_json::from_str::<Value>(&raw_config) {
            if let Some(models) = val
                .get("modelCatalog")
                .and_then(|mc| mc.get("models"))
                .and_then(Value::as_array)
            {
                for (idx, m) in models.iter().enumerate() {
                    let d_name = m.get("displayName").and_then(Value::as_str).unwrap_or("");
                    let m_name = m.get("model").and_then(Value::as_str).unwrap_or("");
                    let cw = m
                        .get("contextWindow")
                        .map(|v| {
                            if let Some(s) = v.as_str() {
                                s.to_string()
                            } else if let Some(n) = v.as_i64() {
                                n.to_string()
                            } else {
                                String::new()
                            }
                        })
                        .unwrap_or_default();
                    let reasoning = if let Some(arr) = m.get("reasoningLevels").and_then(Value::as_array) {
                        arr.iter().filter_map(Value::as_str).collect::<Vec<_>>().join(",")
                    } else if let Some(s) = m.get("reasoningLevels").and_then(Value::as_str) {
                        s.to_string()
                    } else {
                        String::new()
                    };

                    let display_name = cx.new(|cx| {
                        let mut inp = TextInput::new(i.t("例如 DeepSeek V3", "e.g. DeepSeek V3"), cx);
                        inp.set_text_silent(d_name.to_string(), cx);
                        inp
                    });
                    let model_ent = cx.new(|cx| {
                        let mut inp = TextInput::new(i.t("实际模型如 deepseek-chat", "Model name e.g. deepseek-chat"), cx);
                        inp.set_text_silent(m_name.to_string(), cx);
                        inp
                    });
                    let cw_ent = cx.new(|cx| {
                        let mut inp = TextInput::new(i.t("如 128000", "e.g. 128000"), cx);
                        inp.set_text_silent(cw, cx);
                        inp
                    });
                    codex_catalog_models.push(crate::pages::CodexCatalogModelDraft {
                        key: format!("codex_cat_{idx}"),
                        display_name,
                        model: model_ent,
                        context_window: cw_ent,
                        reasoning_levels: reasoning,
                    });
                }
            }
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
        codex_catalog_models,
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
        codex_catalog_models,
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
        if tool == ToolId::Codex && category != "official" {
            connection_section = connection_section.child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(4.0))
                    .child(field_label(i.t("上游协议格式 (Upstream Format)", "Upstream Format")))
                    .child({
                        let formats = [
                            ("responses", "Responses (原生直连)"),
                            ("openai_chat", "Chat Completions (需开启路由)"),
                            ("anthropic", "Anthropic Messages (需开启路由)"),
                        ];
                        let mut fmt_row = div().flex().gap(px(6.0)).flex_wrap();
                        for (f_val, f_lbl) in formats {
                            let is_curr = if codex_wire_api.is_empty() {
                                f_val == "responses"
                            } else {
                                codex_wire_api == f_val
                            };
                            let f_val2 = f_val.to_string();
                            fmt_row = fmt_row.child(button_l(
                                gpui::SharedString::from(format!("codex-fmt-{f_val}")),
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
                                        dialog.codex_wire_api = f_val2.clone();
                                    }
                                    cx.notify();
                                },
                            ));
                        }
                        fmt_row
                    }),
            );

            connection_section = connection_section.child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(4.0))
                    .child(field_label(i.t("思考等级 (Reasoning Effort)", "Reasoning Effort")))
                    .child({
                        let efforts = [
                            ("default", "默认 (Default)"),
                            ("none", "none"),
                            ("low", "low"),
                            ("medium", "medium"),
                            ("high", "high"),
                            ("xhigh", "xhigh"),
                        ];
                        let mut eff_row = div().flex().gap(px(6.0)).flex_wrap();
                        for (e_val, e_lbl) in efforts {
                            let is_curr = if codex_reasoning_effort.is_empty() {
                                e_val == "default"
                            } else {
                                codex_reasoning_effort == e_val
                            };
                            let e_val2 = e_val.to_string();
                            eff_row = eff_row.child(button_l(
                                gpui::SharedString::from(format!("codex-eff-{e_val}")),
                                e_lbl,
                                if is_curr {
                                    ButtonVariant::Primary
                                } else {
                                    ButtonVariant::Secondary
                                },
                                &t,
                                cx,
                                move |ws, _ev, _w, cx| {
                                    if let Some(dialog) = ws.ui.provider_dialog.as_mut() {
                                        dialog.codex_reasoning_effort = e_val2.clone();
                                    }
                                    cx.notify();
                                },
                            ));
                        }
                        eff_row
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
                    .w_full()
                    .child(div().flex_1().min_w(px(0.0)).child(input_container(&t2, model_ent.clone())))
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

                let mut col = div().flex().flex_col().gap(px(4.0)).flex_1().min_w(px(0.0));
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
                                                    let cw_str = fm.context_length.map(|l| l.to_string()).unwrap_or_else(|| "1000000".into());
                                                    let ctx_ent = cx.new(|cx| {
                                                        let mut inp = TextInput::new("例如 1000000", cx);
                                                        inp.set_text_silent(cw_str, cx);
                                                        inp
                                                    });
                                                    let max_ent = cx.new(|cx| {
                                                        let mut inp = TextInput::new("例如 128000", cx);
                                                        inp.set_text_silent("128000".to_string(), cx);
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
                                                inp.set_text_silent("1000000", cx);
                                                inp
                                            });
                                            let mt_ent = cx.new(|cx| {
                                                let mut inp = TextInput::new("最大输出", cx);
                                                inp.set_text_silent("128000", cx);
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
                let dismiss_btn = crate::components::icon_button_svg(
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
                );
                pi_sec = pi_sec.child(
                    crate::components::error_strip(
                        "pi-fetch-err-strip",
                        i.t("获取模型失败", "Fetch models failed"),
                        err_txt,
                        &t2,
                        cx,
                        Some(dismiss_btn),
                    )
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
                            .min_w(px(0.0))
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
        } else if tool == ToolId::Codex {
            // Codex Model Configuration
            let mut codex_sec = div().flex().flex_col().gap(px(10.0));

            // 1. Default Model Card
            let mut def_card = div()
                .p(px(12.0))
                .rounded(px(8.0))
                .bg(t.sidebar_bg)
                .border_1()
                .border_color(t.card_border)
                .flex()
                .flex_col()
                .gap(px(8.0));

            def_card = def_card.child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_size(px(12.5))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(t.text_primary)
                            .child(i.t("默认模型 (Default Model)", "Default Model")),
                    ),
            );

            def_card = def_card.child(
                div()
                    .text_size(px(11.0))
                    .text_color(t.text_muted)
                    .child(i.t(
                        "Codex 默认请求的模型，随时可改。留空且配置了模型映射时，默认使用映射第一行。",
                        "Default model for Codex. Leave empty to use the first mapped model.",
                    )),
            );

            let def_row = div()
                .flex()
                .items_center()
                .gap(px(6.0))
                .child(div().flex_1().child(render_model_input_with_fetch(
                    "codex_default_model".to_string(),
                    model.clone(),
                    None,
                    cx,
                )))
                .child({
                    let t2 = t.clone();
                    button_with_icon_loading_l(
                        "btn-fetch-models-codex-def",
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
                });
            def_card = def_card.child(def_row);
            codex_sec = codex_sec.child(def_card);

            // 2. Model Mapping Card
            let mut map_card = div()
                .p(px(12.0))
                .rounded(px(8.0))
                .bg(t.sidebar_bg)
                .border_1()
                .border_color(t.card_border)
                .flex()
                .flex_col()
                .gap(px(10.0));

            map_card = map_card.child(
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
                                    .child(i.t("模型映射 (Model Mapping)", "Model Mapping")),
                            )
                            .child(
                                div()
                                    .text_size(px(11.0))
                                    .text_color(t.text_muted)
                                    .child(i.t(
                                        "配置 Codex 菜单显示名与实际请求模型的映射（多模型支持）",
                                        "Configure model mapping between menu display name and actual upstream model",
                                    )),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(6.0))
                            .child({
                                let t2 = t.clone();
                                button_with_icon_loading_l(
                                    "btn-fetch-models-codex-map",
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
                            .child({
                                let t2 = t.clone();
                                button_with_icon_l(
                                    "btn-add-codex-model",
                                    crate::icons::PLUS_SVG,
                                    i.t("添加模型", "Add Model"),
                                    ButtonVariant::Secondary,
                                    &t2,
                                    cx,
                                    |ws, _ev, _w, cx| {
                                        if let Some(d) = ws.ui.provider_dialog.as_mut() {
                                            let idx = d.codex_catalog_models.len();
                                            let display_name = cx.new(|cx| TextInput::new(ws.i18n.t("例如 DeepSeek V3", "e.g. DeepSeek V3"), cx));
                                            let model_ent = cx.new(|cx| TextInput::new(ws.i18n.t("实际模型如 deepseek-chat", "Model e.g. deepseek-chat"), cx));
                                            let cw_ent = cx.new(|cx| {
                                                let mut inp = TextInput::new(ws.i18n.t("如 128000", "e.g. 128000"), cx);
                                                inp.set_text_silent("128000", cx);
                                                inp
                                            });
                                            d.codex_catalog_models.push(crate::pages::CodexCatalogModelDraft {
                                                key: format!("codex_cat_{idx}"),
                                                display_name,
                                                model: model_ent,
                                                context_window: cw_ent,
                                                reasoning_levels: String::new(),
                                            });
                                        }
                                        cx.notify();
                                    },
                                )
                            }),
                    ),
            );

            // Fetch error banner
            if let Some(err_txt) = &fetch_error {
                let t2 = t.clone();
                let dismiss_btn = crate::components::icon_button_svg(
                    "btn-dismiss-codex-fetch-err",
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
                );
                map_card = map_card.child(crate::components::error_strip(
                    "codex-fetch-err-strip",
                    i.t("获取模型失败", "Fetch models failed"),
                    err_txt,
                    &t2,
                    cx,
                    Some(dismiss_btn),
                ));
            }

            if codex_catalog_models.is_empty() {
                map_card = map_card.child(
                    div()
                        .py(px(16.0))
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_size(px(12.0))
                        .text_color(t.text_muted)
                        .child(i.t(
                            "暂无模型映射配置（非必填），点击右上角【添加模型】或【获取上游模型】进行多模型映射",
                            "No model mapping configured. Click Add Model or Fetch Models.",
                        )),
                );
            } else {
                // Table header
                map_card = map_card.child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(6.0))
                        .px(px(2.0))
                        .text_size(px(11.0))
                        .font_weight(gpui::FontWeight::MEDIUM)
                        .text_color(t.text_secondary)
                        .child(div().w(px(160.0)).child(i.t("菜单显示名", "Menu Display Name")))
                        .child(div().flex_1().child(i.t("实际请求模型 *", "Actual Request Model *")))
                        .child(div().w(px(100.0)).child(i.t("上下文窗口", "Context Window")))
                        .child(div().w(px(90.0)).child(i.t("思考等级", "Reasoning Levels")))
                        .child(div().w(px(28.0))),
                );

                for (idx, draft) in codex_catalog_models.iter().enumerate() {
                    let t2 = t.clone();
                    let current_reasoning = draft.reasoning_levels.clone();

                    let row_div = div()
                        .flex()
                        .items_center()
                        .gap(px(6.0))
                        .p(px(6.0))
                        .rounded(px(6.0))
                        .bg(t2.card_bg)
                        .border_1()
                        .border_color(t2.card_border)
                        .child(div().w(px(160.0)).child(input_container(&t2, draft.display_name.clone())))
                        .child(div().flex_1().child(render_model_input_with_fetch(
                            format!("codex_row_{idx}"),
                            draft.model.clone(),
                            Some(draft.display_name.clone()),
                            cx,
                        )))
                        .child(div().w(px(100.0)).child(input_container(&t2, draft.context_window.clone())))
                        .child({
                            let label = if current_reasoning.is_empty() {
                                i.t("未设置", "Not set").to_string()
                            } else {
                                current_reasoning.clone()
                            };
                            button_l(
                                gpui::SharedString::from(format!("btn-codex-row-eff-{idx}")),
                                gpui::SharedString::from(label),
                                ButtonVariant::Secondary,
                                &t2,
                                cx,
                                move |ws, _ev, _w, cx| {
                                    if let Some(d) = ws.ui.provider_dialog.as_mut() {
                                        if let Some(m) = d.codex_catalog_models.get_mut(idx) {
                                            m.reasoning_levels = match m.reasoning_levels.as_str() {
                                                "" => "none".to_string(),
                                                "none" => "low".to_string(),
                                                "low" => "medium".to_string(),
                                                "medium" => "high".to_string(),
                                                "high" => "xhigh".to_string(),
                                                _ => String::new(),
                                            };
                                        }
                                    }
                                    cx.notify();
                                },
                            )
                        })
                        .child(crate::components::icon_button_svg(
                            format!("btn-del-codex-model-{idx}"),
                            crate::icons::TRASH_SVG,
                            i.t("移除模型", "Remove model"),
                            true,
                            &t2,
                            cx,
                            move |ws, _ev, _w, cx| {
                                if let Some(d) = ws.ui.provider_dialog.as_mut() {
                                    d.codex_catalog_models.remove(idx);
                                    let msg = ws.i18n.t("已移除模型", "Model removed").to_string();
                                    ws.ui.toast(msg, false);
                                }
                                cx.notify();
                            },
                        ));

                    map_card = map_card.child(row_div);
                }
            }

            codex_sec = codex_sec.child(map_card);
            codex_sec.into_any_element()
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
                let dismiss_btn = crate::components::icon_button_svg(
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
                );
                sec = sec.child(
                    crate::components::error_strip(
                        "provider-fetch-err-strip",
                        i.t("获取模型失败", "Fetch models failed"),
                        err_txt,
                        &t2,
                        cx,
                        Some(dismiss_btn),
                    )
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
                        .child(div().flex_1().min_w(px(0.0)).child(i.t("显示名称", "Display Name")))
                        .child(div().flex_1().min_w(px(0.0)).child(i.t("实际请求模型", "Request Model")))
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
                        Some(dn) => div().flex_1().min_w(px(0.0)).child(input_container(&t2, dn.clone())).into_any_element(),
                        None => {
                            let disabled_box = div()
                                .w_full()
                                .h(px(32.0))
                                .flex()
                                .items_center()
                                .px(px(10.0))
                                .rounded(px(6.0))
                                .bg(t2.sidebar_bg)
                                .border_1()
                                .border_color(t2.card_border)
                                .shadow_xs()
                                .cursor_not_allowed()
                                .overflow_hidden()
                                .child(
                                    div()
                                        .text_size(px(11.0))
                                        .text_color(t2.text_muted)
                                        .child(i.t("后台子代理，不显示在菜单", "Subagent (not in menu)"))
                                );
                            div().flex_1().min_w(px(0.0)).child(disabled_box).into_any_element()
                        }
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
                        .w_full()
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
    pub codex_catalog_models: &'a [Value],
    pub custom_user_agent: &'a str,
    pub custom_headers: &'a str,
    pub headers_map: &'a [(String, String)],
    pub raw_settings_json: &'a str,
}

pub(super) fn build_provider_settings(form: &ProviderFormData<'_>) -> Result<String, String> {
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
        codex_catalog_models,
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
            if tool == ToolId::ClaudeDesktop && !base_url.trim().is_empty() {
                if let Some(obj) = val.as_object_mut() {
                    obj.insert("inferenceGatewayBaseUrl".into(), Value::String(base_url.trim().into()));
                }
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
            let existing_toml = val
                .get("config")
                .or_else(|| val.get("toml"))
                .and_then(Value::as_str)
                .unwrap_or(raw_settings_json);
            let mut doc = existing_toml.parse::<toml_edit::DocumentMut>().unwrap_or_default();

            let effective_model = if !model.trim().is_empty() {
                model.trim().to_string()
            } else if let Some(first_cat) = codex_catalog_models.first().and_then(|v| v.get("model")).and_then(Value::as_str) {
                first_cat.to_string()
            } else {
                String::new()
            };

            if !effective_model.is_empty() {
                doc.insert("model", toml_edit::value(effective_model));
            }
            if codex_reasoning_effort != "default" && !codex_reasoning_effort.trim().is_empty() {
                doc.insert("model_reasoning_effort", toml_edit::value(codex_reasoning_effort.trim()));
            }
            let wire = if !codex_wire_api.trim().is_empty() {
                codex_wire_api.trim()
            } else {
                "responses"
            };
            doc.insert("wire_api", toml_edit::value(wire));

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
                        cp_tbl.insert("wire_api", toml_edit::value(wire));
                    }
                }
                doc.insert("model_provider", toml_edit::value("custom"));
            }
            let toml_str = doc.to_string();

            let mut auth_obj = val.get("auth").and_then(Value::as_object).cloned().unwrap_or_default();
            if !api_key.trim().is_empty() {
                auth_obj.insert("OPENAI_API_KEY".into(), Value::String(api_key.trim().into()));
            }

            let mut out = serde_json::Map::new();
            out.insert("auth".into(), Value::Object(auth_obj));
            out.insert("config".into(), Value::String(toml_str.clone()));
            out.insert("toml".into(), Value::String(toml_str));
            if !codex_catalog_models.is_empty() {
                out.insert(
                    "modelCatalog".into(),
                    serde_json::json!({
                        "models": codex_catalog_models
                    }),
                );
            }
            Ok(serde_json::to_string_pretty(&Value::Object(out)).unwrap_or_default())
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
        ToolId::OpenCode => {
            let mut val: Value = serde_json::from_str(raw_settings_json).unwrap_or_else(|_| serde_json::json!({}));
            if !val.is_object() {
                val = serde_json::json!({});
            }
            if let Some(obj) = val.as_object_mut() {
                if !obj.contains_key("npm") {
                    obj.insert("npm".into(), Value::String("@ai-sdk/openai-compatible".into()));
                }
                if !base_url.trim().is_empty() {
                    obj.insert("baseUrl".into(), Value::String(base_url.trim().into()));
                    let options = obj.entry("options").or_insert_with(|| serde_json::json!({}));
                    if let Some(opt_obj) = options.as_object_mut() {
                        opt_obj.insert("baseURL".into(), Value::String(base_url.trim().into()));
                    }
                }
                if !api_key.trim().is_empty() {
                    obj.insert("apiKey".into(), Value::String(api_key.trim().into()));
                    let options = obj.entry("options").or_insert_with(|| serde_json::json!({}));
                    if let Some(opt_obj) = options.as_object_mut() {
                        opt_obj.insert("apiKey".into(), Value::String(api_key.trim().into()));
                    }
                }
                if !model.trim().is_empty() {
                    obj.insert("model".into(), Value::String(model.trim().into()));
                }
            }
            Ok(serde_json::to_string_pretty(&val).unwrap_or_default())
        }
        ToolId::OpenClaw => {
            let mut val: Value = serde_json::from_str(raw_settings_json).unwrap_or_else(|_| serde_json::json!({}));
            if !val.is_object() {
                val = serde_json::json!({});
            }
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
        ToolId::Hermes => {
            let mut val: Value = serde_json::from_str(raw_settings_json).unwrap_or_else(|_| serde_json::json!({}));
            if !val.is_object() {
                val = serde_json::json!({});
            }
            if let Some(obj) = val.as_object_mut() {
                if !base_url.trim().is_empty() {
                    obj.insert("base_url".into(), Value::String(base_url.trim().into()));
                }
                if !api_key.trim().is_empty() {
                    obj.insert("api_key".into(), Value::String(api_key.trim().into()));
                }
                if !model.trim().is_empty() {
                    obj.insert("model".into(), Value::String(model.trim().into()));
                }
            }
            Ok(serde_json::to_string_pretty(&val).unwrap_or_default())
        }
        ToolId::Grok => {
            let val: Value = serde_json::from_str(raw_settings_json).unwrap_or_else(|_| serde_json::json!({}));
            let existing_toml = val
                .get("config")
                .or_else(|| val.get("toml"))
                .and_then(Value::as_str)
                .unwrap_or(raw_settings_json);
            let mut doc = existing_toml.parse::<toml_edit::DocumentMut>().unwrap_or_default();
            if !base_url.trim().is_empty() || !api_key.trim().is_empty() {
                let m_tbl = doc.entry("model").or_insert(toml_edit::Item::Table(toml_edit::Table::new()));
                if let Some(mt) = m_tbl.as_table_mut() {
                    let custom = mt.entry("custom").or_insert(toml_edit::Item::Table(toml_edit::Table::new()));
                    if let Some(ct) = custom.as_table_mut() {
                        if !base_url.trim().is_empty() {
                            ct.insert("base_url", toml_edit::value(base_url.trim()));
                        }
                        if !api_key.trim().is_empty() {
                            ct.insert("api_key", toml_edit::value(api_key.trim()));
                        }
                    }
                }
            }
            let toml_str = doc.to_string();
            let mut out = serde_json::Map::new();
            out.insert("config".into(), Value::String(toml_str.clone()));
            out.insert("toml".into(), Value::String(toml_str));
            Ok(serde_json::to_string_pretty(&Value::Object(out)).unwrap_or_default())
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

pub(super) fn save_provider(
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
        api_format: (!api_format.is_empty()).then(|| api_format.clone()),
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

    let mut codex_catalog_models_json = Vec::new();
    if tool == ToolId::Codex {
        for draft in &state.codex_catalog_models {
            let d_name = draft.display_name.update(cx, |inp, _| inp.text().trim().to_string());
            let m_name = draft.model.update(cx, |inp, _| inp.text().trim().to_string());
            let cw = draft.context_window.update(cx, |inp, _| inp.text().trim().to_string());
            let reasoning = &draft.reasoning_levels;
            if !m_name.is_empty() || !d_name.is_empty() {
                let mut obj = serde_json::Map::new();
                obj.insert(
                    "displayName".to_string(),
                    Value::String(if d_name.is_empty() {
                        m_name.clone()
                    } else {
                        d_name
                    }),
                );
                obj.insert("model".to_string(), Value::String(m_name));
                if !cw.is_empty() {
                    obj.insert("contextWindow".to_string(), Value::String(cw));
                }
                if !reasoning.is_empty() {
                    let levels: Vec<Value> = reasoning
                        .split(',')
                        .map(|s| Value::String(s.trim().to_string()))
                        .collect();
                    obj.insert("reasoningLevels".to_string(), Value::Array(levels));
                }
                codex_catalog_models_json.push(Value::Object(obj));
            }
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
        codex_catalog_models: &codex_catalog_models_json,
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

    let was_applied = editing_id.as_ref().and_then(|id| {
        ws.store
            .store()
            .tool(tool)
            .providers
            .iter()
            .find(|p| p.id == *id)
            .map(|p| p.is_applied)
    }).unwrap_or(false);

    let has_other_applied = ws.store
        .store()
        .tool(tool)
        .providers
        .iter()
        .filter(|p| editing_id.as_ref().map(|id| p.id != *id).unwrap_or(true))
        .any(|p| p.is_applied);
    let should_apply_new = editing_id.is_none() && !has_other_applied;

    let mut saved_target_id = String::new();
    let _ = ws.store.update(|store| {
        let section = store.tool_mut(tool);
        match editing_id.clone() {
            Some(id) => {
                saved_target_id = id.clone();
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
                if should_apply_new {
                    p.is_applied = true;
                }
                saved_target_id = p.id.clone();
                section.providers.push(p);
            }
        }
    });
    ws.persist_store();

    // Re-apply if this was the saved/active provider for Pi / OhMyPi or any other agent
    if tool == ToolId::Pi {
        let pi_target_id = editing_id.clone().unwrap_or_else(|| {
            let key = if !pi_provider_key_txt.is_empty() {
                pi_provider_key_txt.clone()
            } else {
                name_txt.to_lowercase().chars().filter(|c| c.is_alphanumeric() || *c == '-').collect()
            };
            let key = if key.is_empty() { "custom".to_string() } else { key };
            format!("pi:{key}")
        });

        // Ensure this saved provider is marked applied and persisted to ~/.pi/agent/models.json
        let _ = ws.store.update(|store| {
            if let Some(p) = store.tool_mut(ToolId::Pi).providers.iter_mut().find(|p| p.id == pi_target_id) {
                p.is_applied = true;
            }
        });
        ws.persist_store();

        if let Some(saved) = ws.store.store().tool(tool).providers.iter().find(|p| p.id == pi_target_id).cloned() {
            let _ = aitoolplus_core::pi_runtime::apply_provider(&ws.paths, &saved);
        }
    } else if tool == ToolId::OhMyPi {
        let omp_target_id = editing_id.clone().unwrap_or_else(|| {
            let key = if !pi_provider_key_txt.is_empty() {
                pi_provider_key_txt.clone()
            } else {
                name_txt.to_lowercase().chars().filter(|c| c.is_alphanumeric() || *c == '-').collect()
            };
            let key = if key.is_empty() { "custom".to_string() } else { key };
            format!("omp:{key}")
        });
        let _ = ws.store.update(|store| {
            if let Some(p) = store.tool_mut(ToolId::OhMyPi).providers.iter_mut().find(|p| p.id == omp_target_id) {
                p.is_applied = true;
            }
        });
        ws.persist_store();
        if let Some(saved) = ws.store.store().tool(tool).providers.iter().find(|p| p.id == omp_target_id).cloned() {
            let omp_paths = aitoolplus_core::oh_my_pi::OmpRuntimePaths::from_paths(&ws.paths);
            let _ = aitoolplus_core::oh_my_pi::apply_provider(&omp_paths, &saved);
        }
    } else if was_applied || should_apply_new {
        let common = ws.store.store().tool(tool).common_config.clone();
        if let Some(saved) = ws.store.store().tool(tool).providers.iter().find(|p| p.id == saved_target_id).cloned() {
            let adapter = aitoolplus_core::adapters::adapter_for(tool);
            let ctx = aitoolplus_core::adapters::ApplyCtx {
                paths: &ws.paths,
                common_config: &common,
                provider: &saved,
                strategy: aitoolplus_core::config::MergeStrategy::default(),
                provider_optional: false,
            };
            match adapter.apply(&ctx) {
                Ok(report) => {
                    tracing::info!("Auto-applied updated provider {} for {:?} ({} files)", saved_target_id, tool, report.files.len());
                }
                Err(e) => {
                    tracing::error!("Failed to re-apply updated provider {} for {:?}: {e}", saved_target_id, tool);
                }
            }
        }
    }

    ws.ui.provider_dialog = None;
    let msg = i.t("供应商已保存", "Provider saved").to_string();
    ws.ui.toast(msg, false);
    cx.notify();
}

