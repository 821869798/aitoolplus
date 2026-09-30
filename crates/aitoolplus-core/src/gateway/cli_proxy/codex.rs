use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use serde_json::{Map, Value};
use toml_edit::{value, DocumentMut, Item};
use crate::providers::ProviderRecord;

pub const GATEWAY_PROVIDER_ID: &str = "aitoolplus-gateway";
pub const GATEWAY_API_KEY: &str = "aitoolplus-gateway";
pub const AI_TOOLBOX_CODEX_MODEL_CATALOG_FILENAME: &str = "ai-toolbox-codex-model-catalog.json";

/// Sanitize provider name into a clean URL-safe slug prefix (e.g. "AnyRouter" -> "anyrouter").
pub fn sanitize_prefix(name: &str) -> String {
    let mut out = String::new();
    for c in name.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else if c == '-' || c == '_' || c == '.' || c == ' ' {
            if !out.ends_with('-') && !out.is_empty() {
                out.push('-');
            }
        }
    }
    let trimmed = out.trim_matches('-');
    if trimmed.is_empty() {
        "p".to_string()
    } else {
        trimmed.to_string()
    }
}

#[derive(Debug, Clone)]
pub struct CodexModelSpec {
    pub model: String,
    pub display_name: String,
    pub context_window: u64,
}

/// Extract declared models for a Codex provider record.
pub fn extract_codex_models(record: &ProviderRecord) -> Vec<CodexModelSpec> {
    let mut specs = Vec::new();
    let mut seen = HashSet::new();

    let settings = serde_json::from_str::<Value>(&record.settings_config).ok().unwrap_or(Value::Null);

    // 1. Check modelCatalog.models
    if let Some(models) = settings.get("modelCatalog").and_then(|mc| mc.get("models")).and_then(|m| m.as_array()) {
        for m in models {
            if let Some(model_id) = m.get("model").and_then(|v| v.as_str()).map(str::trim).filter(|s| !s.is_empty()) {
                if seen.insert(model_id.to_string()) {
                    let disp = m.get("displayName").or_else(|| m.get("display_name")).and_then(|v| v.as_str()).unwrap_or(model_id);
                    let cw = m.get("contextWindow").or_else(|| m.get("context_window")).and_then(|v| v.as_u64()).unwrap_or(200000);
                    specs.push(CodexModelSpec {
                        model: model_id.to_string(),
                        display_name: disp.to_string(),
                        context_window: cw,
                    });
                }
            }
        }
    }

    // 2. Check TOML config
    let toml_str = settings.get("config").or_else(|| settings.get("toml")).and_then(|v| v.as_str()).unwrap_or("");
    if let Ok(doc) = toml_str.parse::<DocumentMut>() {
        if let Some(m) = doc.get("model").and_then(|v| v.as_str()).map(str::trim).filter(|s| !s.is_empty()) {
            if seen.insert(m.to_string()) {
                specs.push(CodexModelSpec {
                    model: m.to_string(),
                    display_name: m.to_string(),
                    context_window: 200000,
                });
            }
        }
        if let Some(chat) = doc.get("chat").and_then(|v| v.as_table()) {
            if let Some(m) = chat.get("model").and_then(|v| v.as_str()).map(str::trim).filter(|s| !s.is_empty()) {
                if seen.insert(m.to_string()) {
                    specs.push(CodexModelSpec {
                        model: m.to_string(),
                        display_name: m.to_string(),
                        context_window: 200000,
                    });
                }
            }
        }
    }

    // 3. Check settings.get("model")
    if let Some(m) = settings.get("model").and_then(|v| v.as_str()).map(str::trim).filter(|s| !s.is_empty()) {
        if seen.insert(m.to_string()) {
            specs.push(CodexModelSpec {
                model: m.to_string(),
                display_name: m.to_string(),
                context_window: 200000,
            });
        }
    }

    // 4. Fallback if empty: standard defaults
    if specs.is_empty() {
        for m in &["gpt-4o", "o3-mini", "claude-3-7-sonnet"] {
            specs.push(CodexModelSpec {
                model: m.to_string(),
                display_name: m.to_string(),
                context_window: 200000,
            });
        }
    }

    specs
}

/// Generate aggregate Codex model catalog JSON file containing `<prefix>.<model>` slugs.
pub fn generate_aggregate_codex_catalog(
    codex_root: &Path,
    providers: &[ProviderRecord],
) -> Result<PathBuf, String> {
    let mut catalog_models = Vec::new();
    let mut idx = 0;
    let mut used_prefixes = HashSet::new();

    for (p_idx, provider) in providers.iter().enumerate() {
        let mut prefix = sanitize_prefix(&provider.name);
        if used_prefixes.contains(&prefix) {
            prefix = format!("{}-{}", prefix, p_idx + 1);
        }
        used_prefixes.insert(prefix.clone());

        let models = extract_codex_models(provider);
        for m in models {
            let slug = format!("{prefix}.{}", m.model);
            let display_name = format!("{} · {}", provider.name, m.display_name);

            let entry = serde_json::json!({
                "slug": slug,
                "display_name": display_name,
                "description": display_name,
                "default_reasoning_level": "medium",
                "supported_reasoning_levels": [
                    { "effort": "low", "description": "Fast responses with lighter reasoning" },
                    { "effort": "medium", "description": "Balances speed and reasoning depth for everyday tasks" },
                    { "effort": "high", "description": "Greater reasoning depth for complex problems" },
                    { "effort": "xhigh", "description": "Extra high reasoning depth for complex problems" },
                    { "effort": "max", "description": "Maximum reasoning depth for the hardest problems" },
                    { "effort": "ultra", "description": "Maximum reasoning with automatic task delegation" }
                ],
                "shell_type": "unified_exec",
                "visibility": "list",
                "supported_in_api": true,
                "priority": 1000 + idx,
                "base_instructions": "You are Codex, a coding agent. Follow the user's instructions and use tools carefully.",
                "supports_reasoning_summaries": true,
                "default_reasoning_summary": "none",
                "support_verbosity": true,
                "default_verbosity": "low",
                "apply_patch_tool_type": "freeform",
                "web_search_tool_type": "text_and_image",
                "truncation_policy": {
                    "mode": "tokens",
                    "limit": 10000
                },
                "supports_parallel_tool_calls": true,
                "supports_image_detail_original": true,
                "context_window": m.context_window,
                "max_context_window": m.context_window,
                "effective_context_window_percent": 95,
                "experimental_supported_tools": [],
                "input_modalities": ["text", "image"],
                "supports_search_tool": true
            });

            catalog_models.push(entry);
            idx += 1;
        }
    }

    let catalog_obj = serde_json::json!({
        "models": catalog_models
    });

    if !codex_root.exists() {
        fs::create_dir_all(codex_root).map_err(|e| e.to_string())?;
    }
    let catalog_path = codex_root.join(AI_TOOLBOX_CODEX_MODEL_CATALOG_FILENAME);
    let json_text = serde_json::to_string_pretty(&catalog_obj)
        .map_err(|e| format!("Failed to serialize aggregate catalog: {e}"))?;
    fs::write(&catalog_path, json_text)
        .map_err(|e| format!("Failed to write aggregate catalog to {}: {e}", catalog_path.display()))?;

    Ok(catalog_path)
}

/// Remove aggregate catalog file and strip model_catalog_json pointer from config.toml.
pub fn cleanup_codex_aggregate_catalog(codex_root: &Path) {
    let catalog_path = codex_root.join(AI_TOOLBOX_CODEX_MODEL_CATALOG_FILENAME);
    if catalog_path.exists() {
        let _ = fs::remove_file(catalog_path);
    }
    let config_path = codex_root.join("config.toml");
    if config_path.exists() {
        if let Ok(content) = fs::read_to_string(&config_path) {
            if let Ok(mut doc) = content.parse::<DocumentMut>() {
                if let Some(val) = doc.get("model_catalog_json").and_then(Item::as_str) {
                    if val == AI_TOOLBOX_CODEX_MODEL_CATALOG_FILENAME {
                        doc.as_table_mut().remove("model_catalog_json");
                        let _ = fs::write(&config_path, doc.to_string());
                    }
                }
            }
        }
    }
}

/// Patch Codex ~/.codex/config.toml and ~/.codex/auth.json to point to Gateway.
pub fn patch_codex_config(
    config_path: &Path,
    auth_path: &Path,
    gateway_endpoint: &str,
    is_aggregate: bool,
) -> Result<(), String> {
    // 1. Patch config.toml
    let content = if config_path.exists() {
        fs::read_to_string(config_path).unwrap_or_default()
    } else {
        String::new()
    };
    let mut doc = if content.trim().is_empty() {
        DocumentMut::new()
    } else {
        content.parse::<DocumentMut>().map_err(|e| format!("Failed to parse config.toml: {e}"))?
    };

    doc["model_provider"] = value(GATEWAY_PROVIDER_ID);

    if is_aggregate {
        doc["model_catalog_json"] = value(AI_TOOLBOX_CODEX_MODEL_CATALOG_FILENAME);
    } else {
        let _ = doc.as_table_mut().remove("model_catalog_json");
    }

    if doc.get("model_providers").is_none() {
        let mut parent = toml_edit::Table::new();
        parent.set_implicit(true);
        doc["model_providers"] = Item::Table(parent);
    }
    let providers = doc["model_providers"]
        .as_table_mut()
        .ok_or_else(|| "Codex [model_providers] must be a table".to_string())?;

    if !providers.contains_key(GATEWAY_PROVIDER_ID) {
        providers.insert(GATEWAY_PROVIDER_ID, Item::Table(toml_edit::Table::new()));
    }
    let provider_table = providers
        .get_mut(GATEWAY_PROVIDER_ID)
        .and_then(Item::as_table_mut)
        .ok_or_else(|| format!("Codex [model_providers.{GATEWAY_PROVIDER_ID}] must be a table"))?;

    provider_table["base_url"] = value(gateway_endpoint);
    provider_table["wire_api"] = value("responses");
    provider_table["requires_openai_auth"] = value(true);

    if let Some(parent) = config_path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    fs::write(config_path, doc.to_string()).map_err(|e| format!("Failed to write config.toml: {e}"))?;

    // 2. Patch auth.json
    let mut auth_val = if auth_path.exists() {
        let auth_str = fs::read_to_string(auth_path).unwrap_or_default();
        serde_json::from_str::<Value>(&auth_str).unwrap_or_else(|_| Value::Object(Map::new()))
    } else {
        Value::Object(Map::new())
    };
    let auth_obj = auth_val.as_object_mut().ok_or_else(|| "Codex auth.json must be a JSON object".to_string())?;
    auth_obj.insert("OPENAI_API_KEY".to_string(), Value::String(GATEWAY_API_KEY.to_string()));
    auth_obj.insert("auth_mode".to_string(), Value::String("apikey".to_string()));

    if let Some(parent) = auth_path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let formatted_auth = serde_json::to_string_pretty(&auth_val).map_err(|e| e.to_string())?;
    fs::write(auth_path, formatted_auth).map_err(|e| format!("Failed to write auth.json: {e}"))?;

    Ok(())
}

/// Check if Codex is currently taken over by the Gateway.
pub fn check_codex_is_gateway(config_path: &Path) -> bool {
    if !config_path.exists() {
        return false;
    }
    let Ok(content) = fs::read_to_string(config_path) else { return false; };
    let Ok(doc) = content.parse::<DocumentMut>() else { return false; };
    if let Some(mp) = doc.get("model_provider").and_then(Item::as_str) {
        if mp == GATEWAY_PROVIDER_ID {
            return true;
        }
        if let Some(providers) = doc.get("model_providers").and_then(Item::as_table) {
            if let Some(p) = providers.get(mp).and_then(Item::as_table) {
                if let Some(bu) = p.get("base_url").and_then(Item::as_str) {
                    if bu.contains("127.0.0.1") || bu.contains("localhost") {
                        return true;
                    }
                }
            }
        }
    }
    false
}

/// Remove Gateway configuration from Codex ~/.codex/config.toml and ~/.codex/auth.json.
pub fn unpatch_codex_config(config_path: &Path, auth_path: &Path) {
    if config_path.exists() {
        if let Ok(content) = fs::read_to_string(config_path) {
            if let Ok(mut doc) = content.parse::<DocumentMut>() {
                let is_gw = doc.get("model_provider").and_then(Item::as_str) == Some(GATEWAY_PROVIDER_ID);
                if is_gw {
                    doc.as_table_mut().remove("model_provider");
                }
                if let Some(providers) = doc.get_mut("model_providers").and_then(Item::as_table_mut) {
                    providers.remove(GATEWAY_PROVIDER_ID);
                }
                doc.as_table_mut().remove("model_catalog_json");
                let _ = fs::write(config_path, doc.to_string());
            }
        }
    }
    if auth_path.exists() {
        if let Ok(content) = fs::read_to_string(auth_path) {
            if content.contains(GATEWAY_API_KEY) {
                let _ = fs::remove_file(auth_path);
            }
        }
    }
    if let Some(root) = config_path.parent() {
        cleanup_codex_aggregate_catalog(root);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_sanitize_prefix() {
        assert_eq!(sanitize_prefix("AnyRouter"), "anyrouter");
        assert_eq!(sanitize_prefix("aihub.top"), "aihub-top");
        assert_eq!(sanitize_prefix("gt-token"), "gt-token");
        assert_eq!(sanitize_prefix("   DeepSeek Chat   "), "deepseek-chat");
        assert_eq!(sanitize_prefix("!!!"), "p");
    }

    #[test]
    fn test_generate_aggregate_codex_catalog() {
        let temp = tempdir().unwrap();
        let codex_root = temp.path().to_path_buf();

        let mut p1 = ProviderRecord::new("AnyRouter", "custom");
        p1.id = "p1".to_string();
        p1.settings_config = r#"{
            "modelCatalog": {
                "models": [
                    { "model": "claude-3-7-sonnet", "displayName": "Claude 3.7 Sonnet", "contextWindow": 200000 }
                ]
            }
        }"#.to_string();

        let mut p2 = ProviderRecord::new("DeepSeek", "custom");
        p2.id = "p2".to_string();
        p2.settings_config = r#"{
            "config": "model = \"deepseek-chat\"\n"
        }"#.to_string();

        let providers = vec![p1, p2];
        let cat_path = generate_aggregate_codex_catalog(&codex_root, &providers).unwrap();
        assert!(cat_path.exists());

        let content = fs::read_to_string(&cat_path).unwrap();
        let val: Value = serde_json::from_str(&content).unwrap();
        let models = val["models"].as_array().unwrap();
        assert_eq!(models.len(), 2);
        assert_eq!(models[0]["slug"], "anyrouter.claude-3-7-sonnet");
        assert_eq!(models[0]["display_name"], "AnyRouter · Claude 3.7 Sonnet");
        assert_eq!(models[1]["slug"], "deepseek.deepseek-chat");

        // Test cleanup
        cleanup_codex_aggregate_catalog(&codex_root);
        assert!(!cat_path.exists());
    }
}
