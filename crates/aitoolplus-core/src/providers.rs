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
    pub created_at: String,
    pub updated_at: String,
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

    pub fn touch(&mut self) {
        self.updated_at = chrono::Local::now().to_rfc3339();
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
    let any_applied = providers.iter().any(|p| p.is_applied);
    if let Some(existing) = providers.iter_mut().find(|p| p.id == id) {
        if !any_applied {
            existing.is_applied = true;
        }
        return false;
    }
    let Some(mut official) = official_provider_record(tool) else {
        return false;
    };
    if !any_applied {
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
}
