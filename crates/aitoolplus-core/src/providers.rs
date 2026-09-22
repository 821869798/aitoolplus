//! Provider records: per-tool API provider profiles with CRUD, reorder,
//! enable/disable, import/export. Mirrors ai-toolbox's provider semantics.

use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::tools::ToolId;

pub const CATEGORIES: [&str; 5] = ["official", "custom", "proxy", "subscription", "other"];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderRecord {
    pub id: String,
    pub name: String,
    #[serde(default = "default_category")]
    pub category: String,
    /// Tool-specific settings as a JSON object string (base_url, api_key,
    /// model, env…). Kept as string so the UI can round-trip raw user JSON.
    #[serde(default = "default_obj")]
    pub settings_config: String,
    #[serde(default)]
    pub is_applied: bool,
    #[serde(default)]
    pub is_disabled: bool,
    #[serde(default)]
    pub sort_index: i32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub website_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub meta: Option<Value>,
    pub created_at: String,
    pub updated_at: String,
}

/// Provider-level extended metadata, compatible with cc-switch and ai-toolbox.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct ProviderMeta {
    #[serde(default, skip_serializing_if = "Option::is_none", alias = "customUserAgent")]
    pub custom_user_agent: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none", alias = "customHeaders")]
    pub custom_headers: Option<Vec<CustomHeaderItem>>,
    #[serde(default, skip_serializing_if = "Option::is_none", alias = "billingEnabled")]
    pub billing_enabled: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none", alias = "costMultiplier")]
    pub cost_multiplier: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none", alias = "pricingModelSource")]
    pub pricing_model_source: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none", alias = "modelRewrites")]
    pub model_rewrites: Option<Vec<ModelRewriteRule>>,
}

/// Custom HTTP header item.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct CustomHeaderItem {
    pub name: String,
    pub value: String,
}

/// Exact model rewrite rule: rewrite requested model `from` to `to`.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct ModelRewriteRule {
    pub from: String,
    pub to: String,
}

fn default_category() -> String {
    "custom".into()
}
fn default_obj() -> String {
    "{}".into()
}

impl ProviderRecord {
    pub fn new(name: impl Into<String>, category: impl Into<String>) -> Self {
        let now = chrono::Local::now().to_rfc3339();
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            name: name.into(),
            category: category.into(),
            settings_config: "{}".into(),
            is_applied: false,
            is_disabled: false,
            sort_index: 0,
            notes: None,
            website_url: None,
            meta: None,
            created_at: now.clone(),
            updated_at: now,
        }
    }

    /// Parsed settings JSON; tolerant of bad JSON (returns empty object).
    pub fn settings(&self) -> Value {
        serde_json::from_str(&self.settings_config).unwrap_or(Value::Object(Default::default()))
    }

    pub fn set_settings(&mut self, v: &Value) {
        self.settings_config = serde_json::to_string_pretty(v).unwrap_or_default();
        self.touch();
    }

    /// Parse structured metadata (User-Agent, headers, pricing, rewrites),
    /// tolerant of partial or legacy formats from cc-switch / ai-toolbox.
    pub fn parsed_meta(&self) -> ProviderMeta {
        let Some(meta_val) = &self.meta else {
            return ProviderMeta::default();
        };
        serde_json::from_value(meta_val.clone()).unwrap_or_default()
    }

    /// Update structured metadata.
    pub fn set_meta(&mut self, meta: &ProviderMeta) {
        if meta == &ProviderMeta::default() {
            self.meta = None;
        } else {
            self.meta = serde_json::to_value(meta).ok();
        }
        self.touch();
    }

    pub fn touch(&mut self) {
        self.updated_at = chrono::Local::now().to_rfc3339();
    }

    /// Resolves `(base_url, api_key)` for this provider under the given tool.
    /// Parity with CC-Switch `resolve_usage_credentials`.
    pub fn resolve_credentials(&self, tool: ToolId) -> (String, String) {
        let val = self.settings();
        let first_non_empty = |env: Option<&Value>, keys: &[&str]| -> String {
            if let Some(env_obj) = env.and_then(Value::as_object) {
                for k in keys {
                    if let Some(s) = env_obj.get(*k).and_then(Value::as_str) {
                        let t = s.trim();
                        if !t.is_empty() {
                            return t.to_string();
                        }
                    }
                }
            }
            String::new()
        };

        let (base_url, api_key) = match tool {
            ToolId::ClaudeCode | ToolId::ClaudeDesktop => {
                let env = val.get("env");
                let u = first_non_empty(env, &["ANTHROPIC_BASE_URL", "BASE_URL"]);
                let u = if u.is_empty() {
                    first_non_empty(Some(&val), &["baseUrl", "base_url", "ANTHROPIC_BASE_URL"])
                } else {
                    u
                };
                let k = first_non_empty(
                    env,
                    &[
                        "ANTHROPIC_AUTH_TOKEN",
                        "ANTHROPIC_API_KEY",
                        "OPENROUTER_API_KEY",
                        "GOOGLE_API_KEY",
                    ],
                );
                let k = if k.is_empty() {
                    first_non_empty(Some(&val), &["apiKey", "api_key", "token", "key"])
                } else {
                    k
                };
                (u, k)
            }
            ToolId::Codex => {
                let auth = val.get("auth");
                let mut key = first_non_empty(auth, &["OPENAI_API_KEY", "api_key", "token"]);
                if key.is_empty() {
                    key = first_non_empty(Some(&val), &["apiKey", "api_key"]);
                }

                let toml_str = val.get("config").or_else(|| val.get("toml")).and_then(Value::as_str);
                let toml_text = toml_str.unwrap_or(&self.settings_config);
                let mut base_url = String::new();

                if let Ok(doc) = toml_text.parse::<toml::Value>() {
                    // Try active model_provider
                    if let Some(active) = doc.get("model_provider").and_then(|v| v.as_str()) {
                        if let Some(providers) = doc.get("model_providers").and_then(|v| v.as_table()) {
                            if let Some(provider) = providers.get(active).and_then(|v| v.as_table()) {
                                if base_url.is_empty() {
                                    if let Some(u) = provider.get("base_url").and_then(|v| v.as_str()) {
                                        base_url = u.trim().to_string();
                                    }
                                }
                                if key.is_empty() {
                                    if let Some(k) = provider.get("experimental_bearer_token").or_else(|| provider.get("api_key")).and_then(|v| v.as_str()) {
                                        key = k.trim().to_string();
                                    }
                                }
                            }
                        }
                    }
                    // Fallbacks in TOML: any model_providers table
                    if base_url.is_empty() {
                        if let Some(providers) = doc.get("model_providers").and_then(|v| v.as_table()) {
                            for (_k, tbl_val) in providers {
                                if let Some(tbl) = tbl_val.as_table() {
                                    if let Some(u) = tbl.get("base_url").and_then(|v| v.as_str()) {
                                        let ut = u.trim();
                                        if !ut.is_empty() {
                                            base_url = ut.to_string();
                                            break;
                                        }
                                    }
                                }
                            }
                        }
                    }
                    if key.is_empty() {
                        if let Some(providers) = doc.get("model_providers").and_then(|v| v.as_table()) {
                            for (_k, tbl_val) in providers {
                                if let Some(tbl) = tbl_val.as_table() {
                                    if let Some(k) = tbl.get("experimental_bearer_token").or_else(|| tbl.get("api_key")).and_then(|v| v.as_str()) {
                                        let kt = k.trim();
                                        if !kt.is_empty() {
                                            key = kt.to_string();
                                            break;
                                        }
                                    }
                                }
                            }
                        }
                    }
                    if base_url.is_empty() {
                        if let Some(u) = doc.get("base_url").and_then(|v| v.as_str()) {
                            base_url = u.trim().to_string();
                        }
                    }
                    if key.is_empty() {
                        if let Some(k) = doc.get("experimental_bearer_token").and_then(|v| v.as_str()) {
                            key = k.trim().to_string();
                        }
                    }
                }

                if base_url.is_empty() {
                    base_url = first_non_empty(Some(&val), &["baseUrl", "base_url"]);
                }
                (base_url, key)
            }
            ToolId::GeminiCli => {
                let env = val.get("env");
                let u = first_non_empty(env, &["GOOGLE_GEMINI_BASE_URL", "GEMINI_BASE_URL", "BASE_URL"]);
                let u = if u.is_empty() {
                    first_non_empty(Some(&val), &["baseUrl", "base_url"])
                } else {
                    u
                };
                let k = first_non_empty(env, &["GEMINI_API_KEY", "GOOGLE_API_KEY"]);
                let k = if k.is_empty() {
                    first_non_empty(Some(&val), &["apiKey", "api_key"])
                } else {
                    k
                };
                (u, k)
            }
            ToolId::OpenCode => {
                let options = val.get("options");
                let mut u = first_non_empty(options, &["baseURL", "baseUrl", "base_url"]);
                if u.is_empty() {
                    u = first_non_empty(Some(&val), &["baseUrl", "base_url", "endpoint"]);
                }
                let mut k = first_non_empty(options, &["apiKey", "api_key"]);
                if k.is_empty() {
                    k = first_non_empty(Some(&val), &["apiKey", "api_key"]);
                }
                (u, k)
            }
            ToolId::Pi | ToolId::OhMyPi => {
                let mut u = first_non_empty(Some(&val), &["baseUrl", "base_url"]);
                if u.is_empty() {
                    if let Some(arr) = val.get("models").and_then(Value::as_array) {
                        for m in arr {
                            if let Some(bu) = m.get("baseUrl").or_else(|| m.get("base_url")).and_then(Value::as_str) {
                                let trimmed = bu.trim();
                                if !trimmed.is_empty() {
                                    u = trimmed.to_string();
                                    break;
                                }
                            }
                        }
                    }
                }
                let mut k = first_non_empty(Some(&val), &["apiKey", "api_key"]);
                if k.is_empty() {
                    if let Some(auth_key) = val.get("_auth").and_then(|a| a.get("key")).and_then(Value::as_str) {
                        k = auth_key.trim().to_string();
                    }
                }
                (u, k)
            }
            ToolId::Grok => {
                let toml_str = val.get("config").or_else(|| val.get("toml")).and_then(Value::as_str);
                let toml_text = toml_str.unwrap_or(&self.settings_config);
                let mut u = String::new();
                let mut k = String::new();
                if let Ok(doc) = toml_text.parse::<toml::Value>() {
                    if let Some(models) = doc.get("model").and_then(|v| v.as_table()) {
                        for (_name, tbl) in models {
                            if let Some(t) = tbl.as_table() {
                                if u.is_empty() {
                                    if let Some(url) = t.get("base_url").and_then(|v| v.as_str()) {
                                        u = url.trim().to_string();
                                    }
                                }
                                if k.is_empty() {
                                    if let Some(key) = t.get("api_key").and_then(|v| v.as_str()) {
                                        k = key.trim().to_string();
                                    }
                                }
                            }
                        }
                    }
                    if u.is_empty() {
                        if let Some(url) = doc.get("base_url").and_then(|v| v.as_str()) {
                            u = url.trim().to_string();
                        }
                    }
                }
                if u.is_empty() {
                    u = first_non_empty(Some(&val), &["baseUrl", "base_url"]);
                }
                if k.is_empty() {
                    k = first_non_empty(Some(&val), &["apiKey", "api_key"]);
                }
                (u, k)
            }
            ToolId::Hermes => {
                let u = first_non_empty(Some(&val), &["base_url", "baseUrl"]);
                let k = first_non_empty(Some(&val), &["api_key", "apiKey"]);
                (u, k)
            }
            ToolId::OpenClaw => {
                let u = first_non_empty(Some(&val), &["baseUrl", "base_url"]);
                let k = first_non_empty(Some(&val), &["apiKey", "api_key"]);
                (u, k)
            }
            ToolId::Kimi => {
                let env = val.get("env");
                let mut u = first_non_empty(env, &["KIMI_BASE_URL", "BASE_URL"]);
                let mut k = first_non_empty(env, &["KIMI_API_KEY", "API_KEY"]);
                if u.is_empty() {
                    u = first_non_empty(Some(&val), &["baseUrl", "base_url"]);
                }
                if k.is_empty() {
                    k = first_non_empty(Some(&val), &["apiKey", "api_key"]);
                }
                (u, k)
            }
            _ => {
                let u = first_non_empty(Some(&val), &["baseUrl", "base_url", "endpoint"]);
                let k = first_non_empty(Some(&val), &["apiKey", "api_key"]);
                (u, k)
            }
        };

        (base_url.trim_end_matches('/').trim().to_string(), api_key.trim().to_string())
    }

    /// Resolves the default or primary model ID for this provider.
    pub fn resolve_model(&self, tool: ToolId) -> String {
        let val = self.settings();
        match tool {
            ToolId::ClaudeCode | ToolId::ClaudeDesktop => {
                val.get("env")
                    .and_then(|e| e.get("ANTHROPIC_MODEL"))
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .trim()
                    .to_string()
            }
            ToolId::Codex => {
                let toml_str = val.get("config").or_else(|| val.get("toml")).and_then(Value::as_str);
                let toml_text = toml_str.unwrap_or(&self.settings_config);
                if let Ok(doc) = toml_text.parse::<toml::Value>() {
                    if let Some(m) = doc.get("model").and_then(|v| v.as_str()) {
                        let mt = m.trim();
                        if !mt.is_empty() {
                            return mt.to_string();
                        }
                    }
                }
                if let Some(m) = val.get("model").and_then(Value::as_str) {
                    return m.trim().to_string();
                }
                if let Some(first_m) = val.get("modelCatalog")
                    .and_then(|mc| mc.get("models"))
                    .and_then(Value::as_array)
                    .and_then(|arr| arr.first())
                    .and_then(|v| v.get("model"))
                    .and_then(Value::as_str)
                {
                    return first_m.trim().to_string();
                }
                String::new()
            }
            ToolId::GeminiCli => {
                val.get("env")
                    .and_then(|e| e.get("GEMINI_MODEL"))
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .trim()
                    .to_string()
            }
            ToolId::OpenCode => {
                if let Some(m) = val.get("model").and_then(Value::as_str) {
                    return m.trim().to_string();
                }
                if let Some(models) = val.get("models").and_then(Value::as_object) {
                    if let Some(first_key) = models.keys().next() {
                        return first_key.clone();
                    }
                }
                String::new()
            }
            ToolId::Pi | ToolId::OhMyPi => {
                if let Some(models) = val.get("models").and_then(Value::as_array) {
                    if let Some(first) = models.first() {
                        if let Some(id) = first.get("id").or_else(|| first.get("name")).and_then(Value::as_str) {
                            return id.trim().to_string();
                        }
                    }
                }
                String::new()
            }
            _ => {
                val.get("model")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .trim()
                    .to_string()
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Operations over `Vec<ProviderRecord>` (what ToolStore holds).
// ---------------------------------------------------------------------------

pub fn list(providers: &[ProviderRecord]) -> Vec<ProviderRecord> {
    let mut v = providers.to_vec();
    v.sort_by(|a, b| {
        a.sort_index
            .cmp(&b.sort_index)
            .then(a.created_at.cmp(&b.created_at))
    });
    v
}

pub fn get<'a>(providers: &'a [ProviderRecord], id: &str) -> Option<&'a ProviderRecord> {
    providers.iter().find(|p| p.id == id)
}

pub fn create(providers: &mut Vec<ProviderRecord>, name: &str, category: &str) -> ProviderRecord {
    let sort_index = providers.iter().map(|p| p.sort_index).max().unwrap_or(-1) + 1;
    let mut rec = ProviderRecord::new(name, category);
    rec.sort_index = sort_index;
    providers.push(rec.clone());
    rec
}

pub fn update<F>(providers: &mut [ProviderRecord], id: &str, f: F) -> Option<ProviderRecord>
where
    F: FnOnce(&mut ProviderRecord),
{
    let rec = providers.iter_mut().find(|p| p.id == id)?;
    f(rec);
    rec.touch();
    Some(rec.clone())
}

pub fn delete(providers: &mut Vec<ProviderRecord>, id: &str) -> bool {
    let before = providers.len();
    providers.retain(|p| p.id != id);
    before != providers.len()
}

/// Move provider `from` so it lands at index `to`.
pub fn reorder(providers: &mut Vec<ProviderRecord>, from: usize, to: usize) {
    if from >= providers.len() || to >= providers.len() || from == to {
        return;
    }
    let item = providers.remove(from);
    providers.insert(to.min(providers.len()), item);
    reindex(providers);
}

/// Re-apply consecutive sort indexes from current order.
pub fn reindex(providers: &mut [ProviderRecord]) {
    for (i, p) in providers.iter_mut().enumerate() {
        p.sort_index = i as i32;
    }
}

/// Set one provider as applied, clearing others; returns the applied record.
pub fn select(providers: &mut [ProviderRecord], id: &str) -> Option<ProviderRecord> {
    let mut applied = None;
    for p in providers.iter_mut() {
        p.is_applied = p.id == id;
        if p.is_applied {
            applied = Some(p.clone());
        }
    }
    applied
}

pub fn toggle_disabled(providers: &mut [ProviderRecord], id: &str) -> Option<bool> {
    let p = providers.iter_mut().find(|p| p.id == id)?;
    p.is_disabled = !p.is_disabled;
    p.touch();
    Some(p.is_disabled)
}

/// The currently applied provider (first `is_applied && !is_disabled`).
pub fn applied(providers: &[ProviderRecord]) -> Option<&ProviderRecord> {
    providers.iter().find(|p| p.is_applied && !p.is_disabled)
}

/// Import records (merge by name), skipping duplicates.
pub fn import(providers: &mut Vec<ProviderRecord>, incoming: &[ProviderRecord]) -> usize {
    let existing: std::collections::HashSet<String> =
        providers.iter().map(|p| p.name.clone()).collect();
    let mut added = 0;
    let mut next = providers.iter().map(|p| p.sort_index).max().unwrap_or(-1);
    for rec in incoming {
        if existing.contains(&rec.name) {
            continue;
        }
        let mut rec = rec.clone();
        next += 1;
        rec.sort_index = next;
        rec.is_applied = false;
        providers.push(rec);
        added += 1;
    }
    added
}

/// Import providers from Pi Agent's real `models.json` format.
///
/// Pi uses a map keyed by provider id. We preserve each complete provider
/// object in `settings_config`, including `api`, `models`, and auth fields.
/// Existing records are matched by the Pi id (stored as a stable id) or by
/// name/base URL, so repeated startup scans are idempotent.
pub fn import_pi_models_file(
    path: &Path,
    providers: &mut Vec<ProviderRecord>,
) -> Result<usize, String> {
    if !path.exists() {
        return Ok(0);
    }
    let raw = std::fs::read_to_string(path).map_err(|e| format!("read {}: {e}", path.display()))?;
    import_pi_models_json(&raw, providers)
}

pub fn import_pi_models_json(
    raw: &str,
    providers: &mut Vec<ProviderRecord>,
) -> Result<usize, String> {
    let root: Value =
        serde_json::from_str(raw).map_err(|e| format!("invalid Pi models.json: {e}"))?;
    let Some(map) = root.get("providers").and_then(Value::as_object) else {
        return Ok(0);
    };

    let mut added = 0;
    let mut next = providers.iter().map(|p| p.sort_index).max().unwrap_or(-1);
    for (provider_id, raw_provider) in map {
        let Some(obj) = raw_provider.as_object() else {
            continue;
        };
        let name = obj
            .get("name")
            .and_then(Value::as_str)
            .filter(|s| !s.trim().is_empty())
            .unwrap_or(provider_id)
            .to_string();
        let base_url = obj.get("baseUrl").and_then(Value::as_str);

        let existing = providers.iter_mut().find(|p| {
            p.id == pi_record_id(provider_id)
                || (p.name == name
                    && base_url.is_some_and(|url| {
                        p.settings().get("baseUrl").and_then(Value::as_str) == Some(url)
                    }))
        });

        if let Some(record) = existing {
            // Refresh imported records from the real file, preserving UI flags
            // and sort order. This is what makes external edits visible.
            record.name = name;
            record.settings_config = serde_json::to_string_pretty(raw_provider).unwrap_or_default();
            record.touch();
            continue;
        }

        next += 1;
        let mut record = ProviderRecord::new(name, classify_pi_provider(obj));
        record.id = pi_record_id(provider_id);
        record.sort_index = next;
        record.settings_config = serde_json::to_string_pretty(raw_provider).unwrap_or_default();
        record.notes = Some(format!("Pi provider: {provider_id}"));
        providers.push(record);
        added += 1;
    }
    Ok(added)
}

fn pi_record_id(provider_id: &str) -> String {
    format!("pi:{provider_id}")
}

fn classify_pi_provider(obj: &serde_json::Map<String, Value>) -> String {
    match obj.get("api").and_then(Value::as_str) {
        Some("anthropic-messages") => "official".into(),
        Some(api) if api.contains("openai") => "custom".into(),
        _ => "other".into(),
    }
}

/// Serialize for export: pretty JSON array.
pub fn export(providers: &[ProviderRecord]) -> String {
    let sorted = list(providers);
    serde_json::to_string_pretty(&sorted).unwrap_or_else(|_| "[]".into())
}

/// Standard stable id for the built-in official provider (cc-switch parity).
pub fn official_provider_id(tool: ToolId) -> Option<&'static str> {
    match tool {
        ToolId::ClaudeCode => Some("claude-official"),
        ToolId::Codex => Some("codex-official"),
        ToolId::GeminiCli => Some("gemini-official"),
        ToolId::Grok => Some("grok-official"),
        ToolId::ClaudeDesktop => Some("claude-desktop-official"),
        _ => None,
    }
}

/// Check if a provider id belongs to the official persistent provider.
pub fn is_official_provider(tool: ToolId, id: &str) -> bool {
    official_provider_id(tool).is_some_and(|off_id| off_id == id)
}

/// Create the official provider record for a tool (cc-switch parity).
pub fn official_provider_record(tool: ToolId) -> Option<ProviderRecord> {
    match tool {
        ToolId::ClaudeCode => {
            let mut rec = ProviderRecord::new("Claude 官方 / Official", "official");
            rec.id = "claude-official".into();
            rec.website_url = Some("https://claude.ai".into());
            rec.notes = Some("Anthropic 官方直连，使用 claude login 网页登录凭据".into());
            rec.settings_config = serde_json::to_string_pretty(&serde_json::json!({
                "env": {}
            }))
            .unwrap_or_default();
            Some(rec)
        }
        ToolId::Codex => {
            let mut rec = ProviderRecord::new("OpenAI 官方 / Official", "official");
            rec.id = "codex-official".into();
            rec.website_url = Some("https://platform.openai.com".into());
            rec.notes = Some("OpenAI 官方直连，使用官方 codex login 凭据".into());
            rec.settings_config = serde_json::to_string_pretty(&serde_json::json!({
                "toml": "model_provider = \"openai\"\n\n[model_providers.openai]\nname = \"OpenAI\"\nwire_api = \"responses\"\nrequires_openai_auth = true\n"
            }))
            .unwrap_or_default();
            Some(rec)
        }
        ToolId::GeminiCli => {
            let mut rec = ProviderRecord::new("Google 官方 / Official", "official");
            rec.id = "gemini-official".into();
            rec.website_url = Some("https://aistudio.google.com".into());
            rec.notes = Some("Google 官方直连，使用官方 OAuth 登录凭据".into());
            rec.settings_config = serde_json::to_string_pretty(&serde_json::json!({
                "env": {},
                "security": { "auth": { "selectedType": "oauth" } }
            }))
            .unwrap_or_default();
            Some(rec)
        }
        ToolId::Grok => {
            let mut rec = ProviderRecord::new("Grok 官方 / Official", "official");
            rec.id = "grok-official".into();
            rec.website_url = Some("https://x.ai".into());
            rec.notes = Some("xAI 官方直连".into());
            rec.settings_config = serde_json::to_string_pretty(&serde_json::json!({
                "toml": "model_provider = \"grok\"\n"
            }))
            .unwrap_or_default();
            Some(rec)
        }
        ToolId::ClaudeDesktop => {
            let mut rec = ProviderRecord::new("Claude Desktop 官方 / Official", "official");
            rec.id = "claude-desktop-official".into();
            rec.website_url = Some("https://claude.ai/download".into());
            rec.notes = Some("Claude Desktop 官方端点".into());
            rec.settings_config = "{}".into();
            Some(rec)
        }
        _ => None,
    }
}

/// Ensure the persistent official provider exists in `providers` (cc-switch parity).
/// If not present, it is inserted at index 0. If no provider in the list is currently
/// applied, marks the official provider as applied.
pub fn ensure_official_provider(tool: ToolId, providers: &mut Vec<ProviderRecord>) -> bool {
    let Some(id) = official_provider_id(tool) else {
        return false;
    };
    // Deduplicate any stale official records with different ids (e.g. live:claude)
    let had_duplicate = providers.iter().any(|p| p.category == "official" && p.id != id);
    let duplicate_applied =
        providers.iter().any(|p| p.category == "official" && p.id != id && p.is_applied);
    providers.retain(|p| !(p.category == "official" && p.id != id));

    let any_applied = providers.iter().any(|p| p.is_applied);
    if let Some(existing) = providers.iter_mut().find(|p| p.id == id) {
        if !any_applied || duplicate_applied {
            existing.is_applied = true;
        }
        return had_duplicate;
    }
    let Some(mut official) = official_provider_record(tool) else {
        return false;
    };
    if !any_applied || duplicate_applied {
        official.is_applied = true;
    }
    official.sort_index = 0;
    for p in providers.iter_mut() {
        p.sort_index += 1;
    }
    providers.insert(0, official);
    true
}

/// Parse an export and import it.
pub fn import_json(providers: &mut Vec<ProviderRecord>, json: &str) -> Result<usize, String> {
    let incoming: Vec<ProviderRecord> =
        serde_json::from_str(json).map_err(|e| format!("invalid provider export: {e}"))?;
    Ok(import(providers, &incoming))
}

/// Well-known starter settings for a tool (used by the "add provider" dialog).
pub fn default_settings_for(tool: ToolId) -> Value {
    match tool {
        ToolId::ClaudeCode => serde_json::json!({
            "env": {
                "ANTHROPIC_BASE_URL": "https://api.example.com",
                "ANTHROPIC_AUTH_TOKEN": "sk-..."
            }
        }),
        ToolId::Agents => serde_json::json!({}),
        ToolId::Codex => serde_json::json!({
            "base_url": "https://api.example.com/v1",
            "api_key": "sk-...",
            "model": "gpt-5-codex",
            "env_key": "OPENAI_API_KEY"
        }),
        ToolId::GeminiCli => serde_json::json!({
            "env": {
                "GEMINI_API_KEY": "sk-..."
            }
        }),
        ToolId::Grok => serde_json::json!({
            "base_url": "https://api.x.ai/v1",
            "api_key": "xai-...",
            "model": "grok-4"
        }),
        ToolId::Kimi => serde_json::json!({
            "base_url": "https://api.moonshot.cn/v1",
            "api_key": "sk-...",
            "model": "kimi-k2"
        }),
        ToolId::OpenCode => serde_json::json!({
            "provider": "custom",
            "base_url": "https://api.example.com/v1",
            "api_key": "sk-...",
            "model": "claude-sonnet-4"
        }),
        ToolId::OpenClaw => serde_json::json!({
            "base_url": "https://api.example.com/v1",
            "api_key": "sk-...",
            "model": "gpt-4o"
        }),
        ToolId::Pi => serde_json::json!({
            "name": "New Provider",
            "baseUrl": "https://api.example.com/v1",
            "apiKey": "sk-...",
            "models": [{"id": "model-1", "name": "Model 1"}]
        }),
        ToolId::OhMyPi => serde_json::json!({
            "name": "New Provider",
            "baseUrl": "https://api.example.com/v1"
        }),
        ToolId::ClaudeDesktop => serde_json::json!({
            "inferenceGatewayBaseUrl": "https://api.example.com",
            "inferenceModels": ["model-1"]
        }),
        ToolId::Hermes => serde_json::json!({
            "name": "New Provider",
            "base_url": "https://api.example.com/v1",
            "api_key": "sk-...",
            "model": "model-1"
        }),
        ToolId::Dsh => serde_json::json!({
            "name": "New Provider",
            "baseUrl": "https://api.example.com/v1",
            "apiKeyEnv": "NEW_API_KEY",
            "apiKey": "sk-...",
            "defaultModel": "model-1"
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn recs() -> Vec<ProviderRecord> {
        let mut v = vec![];
        create(&mut v, "A", "official");
        create(&mut v, "B", "custom");
        create(&mut v, "C", "custom");
        v[0].id = "a".into();
        v[1].id = "b".into();
        v[2].id = "c".into();
        v
    }

    #[test]
    fn crud_and_ordering() {
        let mut v = recs();
        assert_eq!(list(&v).len(), 3);
        assert_eq!(list(&v)[0].name, "A");
        assert!(delete(&mut v, "b"));
        assert!(!delete(&mut v, "nope"));
        assert_eq!(v.len(), 2);
    }

    #[test]
    fn select_marks_exactly_one() {
        let mut v = recs();
        let sel = select(&mut v, "c").unwrap();
        assert_eq!(sel.id, "c");
        assert!(v[2].is_applied);
        assert!(!v[0].is_applied);
        let mut v2 = v.clone();
        select(&mut v2, "a");
        assert!(applied(&v2).unwrap().id == "a");
    }

    #[test]
    fn reorder_and_reindex() {
        let mut v = recs();
        reorder(&mut v, 0, 2);
        assert_eq!(
            list(&v).iter().map(|p| p.name.clone()).collect::<Vec<_>>(),
            ["B", "C", "A"]
        );
        assert!((0..3).all(|i| list(&v)[i].sort_index == i as i32));
    }

    #[test]
    fn import_merges_by_name() {
        let mut v = recs();
        let mut incoming = ProviderRecord::new("A", "custom");
        incoming.id = "x".into();
        let mut fresh = ProviderRecord::new("New", "custom");
        fresh.id = "y".into();
        let added = import(&mut v, &[incoming, fresh]);
        assert_eq!(added, 1);
        assert!(v.iter().any(|p| p.name == "New"));
    }

    #[test]
    fn export_import_roundtrip() {
        let v = recs();
        let json = export(&v);
        let mut target = vec![];
        let n = import_json(&mut target, &json).unwrap();
        assert_eq!(n, 3);
        assert!(target.iter().any(|p| p.name == "B"));
    }

    #[test]
    fn settings_json_tolerant() {
        let mut p = ProviderRecord::new("t", "custom");
        p.settings_config = "{broken".into();
        assert!(p.settings().is_object());
        p.set_settings(&serde_json::json!({"a": 1}));
        assert_eq!(p.settings()["a"], 1);
    }

    #[test]
    fn default_settings_have_content() {
        for t in ToolId::ALL {
            let v = default_settings_for(t);
            assert!(
                v.as_object().map(|o| !o.is_empty()).unwrap_or(false),
                "{} empty",
                t
            );
        }
    }

    #[test]
    fn imports_real_pi_models_without_duplicates() {
        let json = r#"{
          "providers": {
            "alpha": {
              "name": "Alpha Display",
              "baseUrl": "https://alpha.example/v1",
              "api": "openai-responses",
              "apiKey": "secret",
              "models": [{"id": "model-a", "name": "Model A"}]
            },
            "claude": {
              "baseUrl": "https://claude.example",
              "api": "anthropic-messages",
              "apiKey": "secret-2",
              "models": []
            }
          }
        }"#;
        let mut providers = vec![];
        assert_eq!(import_pi_models_json(json, &mut providers).unwrap(), 2);
        assert_eq!(import_pi_models_json(json, &mut providers).unwrap(), 0);
        assert_eq!(providers.len(), 2);
        let alpha = providers.iter().find(|p| p.id == "pi:alpha").unwrap();
        assert_eq!(alpha.name, "Alpha Display");
        assert_eq!(alpha.settings()["api"], "openai-responses");
        assert_eq!(alpha.settings()["models"][0]["id"], "model-a");
        assert_eq!(
            providers
                .iter()
                .find(|p| p.id == "pi:claude")
                .unwrap()
                .category,
            "official"
        );
    }

    #[test]
    fn official_persistent_provider_lifecycle() {
        let mut providers = vec![];
        assert!(ensure_official_provider(ToolId::ClaudeCode, &mut providers));
        assert_eq!(providers.len(), 1);
        assert_eq!(providers[0].id, "claude-official");
        assert!(providers[0].is_applied, "first provider applied by default");
        assert_eq!(providers[0].category, "official");
        assert!(is_official_provider(ToolId::ClaudeCode, "claude-official"));
        assert!(!is_official_provider(ToolId::ClaudeCode, "custom-id"));

        // Idempotent: does not duplicate
        assert!(!ensure_official_provider(ToolId::ClaudeCode, &mut providers));
        assert_eq!(providers.len(), 1);

        // Codex official provider
        let mut codex_providers = vec![];
        assert!(ensure_official_provider(ToolId::Codex, &mut codex_providers));
        assert_eq!(codex_providers[0].id, "codex-official");
        assert!(codex_providers[0].settings()["toml"]
            .as_str()
            .unwrap()
            .contains("model_provider = \"openai\""));
    }

    #[test]
    fn provider_meta_roundtrip_and_compatibility() {
        let mut record = ProviderRecord::new("Test Provider", "custom");
        assert_eq!(record.parsed_meta(), ProviderMeta::default());

        let meta = ProviderMeta {
            custom_user_agent: Some("claude-cli/2.1.237 (external, cli)".into()),
            custom_headers: Some(vec![
                CustomHeaderItem {
                    name: "X-Title".into(),
                    value: "MyProject".into(),
                },
            ]),
            billing_enabled: Some(true),
            cost_multiplier: Some("1.5".into()),
            pricing_model_source: Some("request".into()),
            model_rewrites: Some(vec![
                ModelRewriteRule {
                    from: "claude-3-5-haiku-20241022".into(),
                    to: "deepseek-chat".into(),
                },
            ]),
        };
        record.set_meta(&meta);
        assert!(record.meta.is_some());
        assert_eq!(record.parsed_meta(), meta);

        // Test compatibility with camelCase json (from ai-toolbox / cc-switch)
        let camel_json = serde_json::json!({
            "customUserAgent": "Kilo-Code/1.0",
            "customHeaders": [{"name": "HTTP-Referer", "value": "https://example.com"}],
            "costMultiplier": "0.7",
            "pricingModelSource": "response",
            "modelRewrites": [{"from": "gpt-4o-mini", "to": "deepseek-chat"}]
        });
        record.meta = Some(camel_json);
        let parsed = record.parsed_meta();
        assert_eq!(parsed.custom_user_agent.as_deref(), Some("Kilo-Code/1.0"));
        assert_eq!(parsed.cost_multiplier.as_deref(), Some("0.7"));
        assert_eq!(parsed.pricing_model_source.as_deref(), Some("response"));
        assert_eq!(parsed.custom_headers.as_ref().unwrap().len(), 1);
        assert_eq!(parsed.custom_headers.as_ref().unwrap()[0].name, "HTTP-Referer");
        assert_eq!(parsed.model_rewrites.as_ref().unwrap()[0].from, "gpt-4o-mini");
    }

    #[test]
    fn test_resolve_credentials_parity_all_tools() {
        // Claude Code
        let mut p_claude = ProviderRecord::new("Claude Test", "custom");
        p_claude.settings_config = r#"{"env":{"ANTHROPIC_BASE_URL":"https://api.anthropic.com/v1/","ANTHROPIC_AUTH_TOKEN":"sk-ant-test"}}"#.into();
        assert_eq!(
            p_claude.resolve_credentials(ToolId::ClaudeCode),
            ("https://api.anthropic.com/v1".to_string(), "sk-ant-test".to_string())
        );

        // Codex with auth and config TOML
        let mut p_codex = ProviderRecord::new("Codex Test", "custom");
        p_codex.settings_config = r#"{"auth":{"OPENAI_API_KEY":"sk-openai-key"},"config":"model_provider = \"custom\"\n[model_providers.custom]\nbase_url = \"https://anyrouter.top/v1\"\n"}"#.into();
        assert_eq!(
            p_codex.resolve_credentials(ToolId::Codex),
            ("https://anyrouter.top/v1".to_string(), "sk-openai-key".to_string())
        );

        // Codex with bearer token in TOML
        let mut p_codex2 = ProviderRecord::new("Codex Bearer", "custom");
        p_codex2.settings_config = r#"{"toml":"model_provider = \"custom\"\n[model_providers.custom]\nbase_url = \"https://bearer.test/v1/\"\nexperimental_bearer_token = \"sk-bearer-token\"\n"}"#.into();
        assert_eq!(
            p_codex2.resolve_credentials(ToolId::Codex),
            ("https://bearer.test/v1".to_string(), "sk-bearer-token".to_string())
        );

        // OpenCode with options
        let mut p_opencode = ProviderRecord::new("OpenCode Test", "custom");
        p_opencode.settings_config = r#"{"npm":"@ai-sdk/openai-compatible","options":{"baseURL":"https://88996api.cloud/v1","apiKey":"sk-opencode-key"}}"#.into();
        assert_eq!(
            p_opencode.resolve_credentials(ToolId::OpenCode),
            ("https://88996api.cloud/v1".to_string(), "sk-opencode-key".to_string())
        );

        // Pi with models array
        let mut p_pi = ProviderRecord::new("Pi Test", "custom");
        p_pi.settings_config = r#"{"apiKey":"sk-pi-key","models":[{"id":"m1","baseUrl":"https://pi.example.com/v1"}]}"#.into();
        assert_eq!(
            p_pi.resolve_credentials(ToolId::Pi),
            ("https://pi.example.com/v1".to_string(), "sk-pi-key".to_string())
        );

        // Gemini CLI
        let mut p_gemini = ProviderRecord::new("Gemini Test", "custom");
        p_gemini.settings_config = r#"{"env":{"GOOGLE_GEMINI_BASE_URL":"https://gemini.example.com","GEMINI_API_KEY":"ai-gemini-key"}}"#.into();
        assert_eq!(
            p_gemini.resolve_credentials(ToolId::GeminiCli),
            ("https://gemini.example.com".to_string(), "ai-gemini-key".to_string())
        );

        // Hermes
        let mut p_hermes = ProviderRecord::new("Hermes Test", "custom");
        p_hermes.settings_config = r#"{"base_url":"https://hermes.example.com","api_key":"sk-hermes"}"#.into();
        assert_eq!(
            p_hermes.resolve_credentials(ToolId::Hermes),
            ("https://hermes.example.com".to_string(), "sk-hermes".to_string())
        );

        // OpenClaw
        let mut p_openclaw = ProviderRecord::new("OpenClaw Test", "custom");
        p_openclaw.settings_config = r#"{"baseUrl":"https://openclaw.example.com","apiKey":"sk-openclaw"}"#.into();
        assert_eq!(
            p_openclaw.resolve_credentials(ToolId::OpenClaw),
            ("https://openclaw.example.com".to_string(), "sk-openclaw".to_string())
        );
    }
}
