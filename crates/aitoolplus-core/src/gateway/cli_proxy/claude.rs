use std::fs;
use std::path::Path;
use serde_json::{Map, Value};

pub const GATEWAY_API_KEY: &str = "aitoolplus-gateway";
pub const CLAUDE_STANDARD_MODEL: &str = "claude-sonnet-5";
pub const CLAUDE_STANDARD_HAIKU_MODEL: &str = "claude-haiku-4-5";
pub const CLAUDE_STANDARD_SONNET_MODEL: &str = "claude-sonnet-5";
pub const CLAUDE_STANDARD_OPUS_MODEL: &str = "claude-opus-5";
pub const CLAUDE_STANDARD_FABLE_MODEL: &str = "claude-fable-5";

/// Patch Claude Code's ~/.claude/settings.json to point to the local Gateway.
pub fn patch_claude_settings(
    path: &Path,
    gateway_endpoint: &str,
    primary_provider: Option<&crate::providers::ProviderRecord>,
) -> Result<(), String> {
    let mut value = if path.exists() {
        let content = fs::read_to_string(path).map_err(|e| format!("Failed to read Claude settings: {e}"))?;
        serde_json::from_str::<Value>(&content).unwrap_or_else(|_| Value::Object(Map::new()))
    } else {
        Value::Object(Map::new())
    };

    let root = value.as_object_mut().ok_or_else(|| "Claude settings must be a JSON object".to_string())?;
    let env = root
        .entry("env")
        .or_insert_with(|| Value::Object(Map::new()))
        .as_object_mut()
        .ok_or_else(|| "Claude env must be a JSON object".to_string())?;

    env.remove("ANTHROPIC_API_KEY");
    env.insert(
        "ANTHROPIC_BASE_URL".to_string(),
        Value::String(gateway_endpoint.to_string()),
    );
    env.insert(
        "ANTHROPIC_AUTH_TOKEN".to_string(),
        Value::String(GATEWAY_API_KEY.to_string()),
    );

    // Standard Claude role aliases for Gateway takeover:
    // Claude Code requests stable role names (opus/sonnet/haiku),
    // and the Gateway dynamically maps each request to the active provider's real model.
    env.insert(
        "ANTHROPIC_MODEL".to_string(),
        Value::String(CLAUDE_STANDARD_MODEL.to_string()),
    );
    env.insert(
        "ANTHROPIC_DEFAULT_HAIKU_MODEL".to_string(),
        Value::String(CLAUDE_STANDARD_HAIKU_MODEL.to_string()),
    );
    env.insert(
        "ANTHROPIC_DEFAULT_SONNET_MODEL".to_string(),
        Value::String(CLAUDE_STANDARD_SONNET_MODEL.to_string()),
    );
    env.insert(
        "ANTHROPIC_DEFAULT_OPUS_MODEL".to_string(),
        Value::String(CLAUDE_STANDARD_OPUS_MODEL.to_string()),
    );
    env.insert(
        "ANTHROPIC_DEFAULT_FABLE_MODEL".to_string(),
        Value::String(CLAUDE_STANDARD_FABLE_MODEL.to_string()),
    );

    // Populate friendly model names for Claude Code terminal UI/status bar (matching ai-toolbox)
    env.remove("ANTHROPIC_DEFAULT_HAIKU_MODEL_NAME");
    env.remove("ANTHROPIC_DEFAULT_SONNET_MODEL_NAME");
    env.remove("ANTHROPIC_DEFAULT_OPUS_MODEL_NAME");
    env.remove("ANTHROPIC_DEFAULT_FABLE_MODEL_NAME");

    if let Some(p) = primary_provider {
        if let Ok(settings) = serde_json::from_str::<Value>(&p.settings_config) {
            let p_env = settings.get("env").and_then(Value::as_object);
            let haiku_name = p_env
                .and_then(|e| e.get("ANTHROPIC_DEFAULT_HAIKU_MODEL_NAME").or_else(|| e.get("ANTHROPIC_DEFAULT_HAIKU_MODEL")))
                .or_else(|| settings.get("haikuModel"))
                .or_else(|| settings.get("model"))
                .and_then(Value::as_str);
            if let Some(n) = haiku_name {
                env.insert("ANTHROPIC_DEFAULT_HAIKU_MODEL_NAME".to_string(), Value::String(n.to_string()));
            }

            let sonnet_name = p_env
                .and_then(|e| e.get("ANTHROPIC_DEFAULT_SONNET_MODEL_NAME").or_else(|| e.get("ANTHROPIC_DEFAULT_SONNET_MODEL")))
                .or_else(|| settings.get("sonnetModel"))
                .or_else(|| settings.get("model"))
                .and_then(Value::as_str);
            if let Some(n) = sonnet_name {
                env.insert("ANTHROPIC_DEFAULT_SONNET_MODEL_NAME".to_string(), Value::String(n.to_string()));
            }

            let opus_name = p_env
                .and_then(|e| e.get("ANTHROPIC_DEFAULT_OPUS_MODEL_NAME").or_else(|| e.get("ANTHROPIC_DEFAULT_OPUS_MODEL")))
                .or_else(|| settings.get("opusModel"))
                .or_else(|| settings.get("model"))
                .and_then(Value::as_str);
            if let Some(n) = opus_name {
                env.insert("ANTHROPIC_DEFAULT_OPUS_MODEL_NAME".to_string(), Value::String(n.to_string()));
            }

            let fable_name = p_env
                .and_then(|e| e.get("ANTHROPIC_DEFAULT_FABLE_MODEL_NAME").or_else(|| e.get("ANTHROPIC_DEFAULT_FABLE_MODEL")))
                .or_else(|| settings.get("fableModel"))
                .or_else(|| settings.get("model"))
                .and_then(Value::as_str);
            if let Some(n) = fable_name {
                env.insert("ANTHROPIC_DEFAULT_FABLE_MODEL_NAME".to_string(), Value::String(n.to_string()));
            }
        }
    }

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let formatted = serde_json::to_string_pretty(&value).map_err(|e| e.to_string())?;
    fs::write(path, formatted).map_err(|e| format!("Failed to write Claude settings: {e}"))?;

    Ok(())
}

/// Check if Claude Code settings currently point to a gateway endpoint.
pub fn check_claude_is_gateway(path: &Path) -> bool {
    if !path.exists() {
        return false;
    }
    let Ok(content) = fs::read_to_string(path) else { return false; };
    let Ok(value) = serde_json::from_str::<Value>(&content) else { return false; };
    if let Some(token) = value.pointer("/env/ANTHROPIC_AUTH_TOKEN").and_then(Value::as_str) {
        if token == GATEWAY_API_KEY || token == "PROXY_TOKEN_PLACEHOLDER" || token == "PROXY_API_KEY" {
            return true;
        }
    }
    if let Some(url) = value.pointer("/env/ANTHROPIC_BASE_URL").and_then(Value::as_str) {
        let lower = url.to_ascii_lowercase();
        if lower.contains("127.0.0.1") || lower.contains("localhost") || lower.contains("0.0.0.0") || lower.contains("[::1]") {
            return true;
        }
    }
    false
}

/// Restore Claude Code settings from a backup content snapshot using fine-grained JSON pointer restoration (matching ai-toolbox).
///
/// Rather than overwriting the entire file (which clobbers user-installed plugins, custom hooks, and external settings),
/// this reads the live settings.json and ONLY reverts the specific environment variables that Gateway proxy modified.
pub fn restore_claude_settings(path: &Path, backup_content: Option<&str>) -> Result<(), String> {
    let mut current = if path.exists() {
        let content = fs::read_to_string(path).map_err(|e| format!("Failed to read Claude settings: {e}"))?;
        serde_json::from_str::<Value>(&content).unwrap_or_else(|_| Value::Object(Map::new()))
    } else {
        Value::Object(Map::new())
    };

    let backup = match backup_content {
        Some(content) if !content.trim().is_empty() => {
            Some(serde_json::from_str::<Value>(content).map_err(|e| format!("Failed to parse Claude gateway backup: {e}"))?)
        }
        _ => None,
    };

    const MANAGED_POINTERS: &[&str] = &[
        "/env/ANTHROPIC_BASE_URL",
        "/env/ANTHROPIC_AUTH_TOKEN",
        "/env/ANTHROPIC_API_KEY",
        "/env/ANTHROPIC_MODEL",
        "/env/ANTHROPIC_DEFAULT_HAIKU_MODEL",
        "/env/ANTHROPIC_DEFAULT_SONNET_MODEL",
        "/env/ANTHROPIC_DEFAULT_OPUS_MODEL",
        "/env/ANTHROPIC_DEFAULT_FABLE_MODEL",
        "/env/ANTHROPIC_DEFAULT_HAIKU_MODEL_NAME",
        "/env/ANTHROPIC_DEFAULT_SONNET_MODEL_NAME",
        "/env/ANTHROPIC_DEFAULT_OPUS_MODEL_NAME",
        "/env/ANTHROPIC_DEFAULT_FABLE_MODEL_NAME",
    ];

    restore_json_pointer_fields(&mut current, backup.as_ref(), MANAGED_POINTERS);

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let formatted = serde_json::to_string_pretty(&current).map_err(|e| e.to_string())?;
    fs::write(path, formatted).map_err(|e| format!("Failed to write restored Claude settings: {e}"))?;

    Ok(())
}

/// Cleanup any lingering takeover placeholders or local gateway URLs from Claude settings.json (parity with cc-switch).
pub fn cleanup_claude_takeover_placeholders(path: &Path) -> Result<(), String> {
    if !path.exists() {
        return Ok(());
    }
    let content = fs::read_to_string(path).map_err(|e| format!("Failed to read Claude settings: {e}"))?;
    let mut value = match serde_json::from_str::<Value>(&content) {
        Ok(v) => v,
        Err(_) => return Ok(()),
    };

    let mut changed = false;
    if let Some(env) = value.get_mut("env").and_then(Value::as_object_mut) {
        for key in ["ANTHROPIC_AUTH_TOKEN", "ANTHROPIC_API_KEY"] {
            if let Some(token) = env.get(key).and_then(Value::as_str) {
                if token == GATEWAY_API_KEY || token == "PROXY_TOKEN_PLACEHOLDER" || token == "PROXY_API_KEY" {
                    env.remove(key);
                    changed = true;
                }
            }
        }

        if let Some(url) = env.get("ANTHROPIC_BASE_URL").and_then(Value::as_str) {
            let lower = url.to_ascii_lowercase();
            if lower.contains("127.0.0.1") || lower.contains("localhost") || lower.contains("0.0.0.0") || lower.contains("[::1]") {
                env.remove("ANTHROPIC_BASE_URL");
                changed = true;
            }
        }

        // Clean up gateway standard model aliases if present
        for key in [
            "ANTHROPIC_DEFAULT_HAIKU_MODEL",
            "ANTHROPIC_DEFAULT_SONNET_MODEL",
            "ANTHROPIC_DEFAULT_OPUS_MODEL",
            "ANTHROPIC_DEFAULT_FABLE_MODEL",
        ] {
            if let Some(m) = env.get(key).and_then(Value::as_str) {
                if m.starts_with("claude-") && (m.ends_with("-5") || m.ends_with("-4-5")) {
                    env.remove(key);
                    changed = true;
                }
            }
        }
    }

    if changed {
        let formatted = serde_json::to_string_pretty(&value).map_err(|e| e.to_string())?;
        fs::write(path, formatted).map_err(|e| format!("Failed to write cleaned Claude settings: {e}"))?;
        tracing::info!("Cleaned up leftover Gateway takeover placeholders from {}", path.display());
    }

    Ok(())
}

fn restore_json_pointer_fields(current: &mut Value, backup: Option<&Value>, pointers: &[&str]) {
    for pointer in pointers {
        match backup.and_then(|val| val.pointer(pointer)).cloned() {
            Some(val) => set_json_pointer(current, pointer, val),
            None => remove_json_pointer(current, pointer),
        }
    }
}

fn ensure_json_object(value: &mut Value) -> &mut Map<String, Value> {
    if !value.is_object() {
        *value = Value::Object(Map::new());
    }
    value.as_object_mut().unwrap()
}

fn set_json_pointer(current: &mut Value, pointer: &str, next_value: Value) {
    let parts: Vec<&str> = pointer.trim_start_matches('/').split('/').collect();
    let mut value = current;
    for part in &parts[..parts.len().saturating_sub(1)] {
        let object = ensure_json_object(value);
        value = object
            .entry((*part).to_string())
            .or_insert_with(|| Value::Object(Map::new()));
    }
    if let Some(last_part) = parts.last() {
        ensure_json_object(value).insert((*last_part).to_string(), next_value);
    }
}

fn remove_json_pointer(current: &mut Value, pointer: &str) {
    let parts: Vec<&str> = pointer.trim_start_matches('/').split('/').collect();
    if parts.is_empty() {
        return;
    }
    let mut value = current;
    for part in &parts[..parts.len().saturating_sub(1)] {
        let Some(next_value) = value
            .as_object_mut()
            .and_then(|object| object.get_mut(*part))
        else {
            return;
        };
        value = next_value;
    }
    if let Some(last_part) = parts.last() {
        if let Some(object) = value.as_object_mut() {
            object.remove(*last_part);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_patch_claude_settings() {
        let temp = tempdir().unwrap();
        let settings_path = temp.path().join("settings.json");

        let mut provider = crate::providers::ProviderRecord::new("DeepSeek", "cn_official");
        provider.settings_config = r#"{
            "env": {
                "ANTHROPIC_DEFAULT_OPUS_MODEL": "deepseek-flash",
                "ANTHROPIC_DEFAULT_OPUS_MODEL_NAME": "DeepSeek Flash (Opus)",
                "ANTHROPIC_DEFAULT_SONNET_MODEL": "deepseek-flash",
                "ANTHROPIC_DEFAULT_HAIKU_MODEL": "deepseek-flash"
            }
        }"#.to_string();

        patch_claude_settings(&settings_path, "http://127.0.0.1:15728/anthropic", Some(&provider)).unwrap();

        let content = fs::read_to_string(&settings_path).unwrap();
        let val: Value = serde_json::from_str(&content).unwrap();
        let env = val["env"].as_object().unwrap();

        // Technical standard aliases for Claude Code request routing
        assert_eq!(env["ANTHROPIC_BASE_URL"], "http://127.0.0.1:15728/anthropic");
        assert_eq!(env["ANTHROPIC_AUTH_TOKEN"], "aitoolplus-gateway");
        assert_eq!(env["ANTHROPIC_DEFAULT_OPUS_MODEL"], "claude-opus-5");
        assert_eq!(env["ANTHROPIC_DEFAULT_SONNET_MODEL"], "claude-sonnet-5");
        assert_eq!(env["ANTHROPIC_DEFAULT_HAIKU_MODEL"], "claude-haiku-4-5");

        // Display names for Claude Code terminal UI status bar / /model menu
        assert_eq!(env["ANTHROPIC_DEFAULT_OPUS_MODEL_NAME"], "DeepSeek Flash (Opus)");
        assert_eq!(env["ANTHROPIC_DEFAULT_SONNET_MODEL_NAME"], "deepseek-flash");
        assert_eq!(env["ANTHROPIC_DEFAULT_HAIKU_MODEL_NAME"], "deepseek-flash");

        assert!(check_claude_is_gateway(&settings_path));
    }

    #[test]
    fn test_restore_claude_settings_preserves_custom_settings() {
        let temp = tempdir().unwrap();
        let settings_path = temp.path().join("settings.json");

        // User had custom hooks and plugins
        let original_json = r#"{
            "hooks": { "Stop": [{ "type": "command", "command": "my-hook.exe" }] },
            "enabledPlugins": { "rust-analyzer": true },
            "env": {
                "ANTHROPIC_BASE_URL": "https://api.myprovider.com",
                "ANTHROPIC_AUTH_TOKEN": "sk-real-token",
                "MY_CUSTOM_ENV": "keep-this"
            }
        }"#;
        fs::write(&settings_path, original_json).unwrap();

        // 1. Takeover engages
        patch_claude_settings(&settings_path, "http://127.0.0.1:15721/anthropic", None).unwrap();
        let taken_over = fs::read_to_string(&settings_path).unwrap();
        assert!(taken_over.contains("http://127.0.0.1:15721/anthropic"));
        assert!(taken_over.contains("my-hook.exe"));
        assert!(taken_over.contains("MY_CUSTOM_ENV"));

        // 2. Restore from backup content
        restore_claude_settings(&settings_path, Some(original_json)).unwrap();
        let restored: Value = serde_json::from_str(&fs::read_to_string(&settings_path).unwrap()).unwrap();

        assert_eq!(restored.pointer("/env/ANTHROPIC_BASE_URL").and_then(Value::as_str), Some("https://api.myprovider.com"));
        assert_eq!(restored.pointer("/env/ANTHROPIC_AUTH_TOKEN").and_then(Value::as_str), Some("sk-real-token"));
        assert_eq!(restored.pointer("/env/MY_CUSTOM_ENV").and_then(Value::as_str), Some("keep-this"));
        assert!(restored.get("hooks").is_some(), "hooks preserved");
        assert!(restored.get("enabledPlugins").is_some(), "plugins preserved");
        assert!(!check_claude_is_gateway(&settings_path));
    }

    #[test]
    fn test_cleanup_claude_takeover_placeholders() {
        let temp = tempdir().unwrap();
        let settings_path = temp.path().join("settings.json");

        // Stale takeover residue from crash
        fs::write(&settings_path, r#"{
            "env": {
                "ANTHROPIC_BASE_URL": "http://127.0.0.1:15721/anthropic",
                "ANTHROPIC_AUTH_TOKEN": "aitoolplus-gateway",
                "ANTHROPIC_DEFAULT_OPUS_MODEL": "claude-opus-5",
                "USER_VAR": "val"
            }
        }"#).unwrap();

        cleanup_claude_takeover_placeholders(&settings_path).unwrap();

        let cleaned: Value = serde_json::from_str(&fs::read_to_string(&settings_path).unwrap()).unwrap();
        assert!(cleaned.pointer("/env/ANTHROPIC_BASE_URL").is_none());
        assert!(cleaned.pointer("/env/ANTHROPIC_AUTH_TOKEN").is_none());
        assert!(cleaned.pointer("/env/ANTHROPIC_DEFAULT_OPUS_MODEL").is_none());
        assert_eq!(cleaned.pointer("/env/USER_VAR").and_then(Value::as_str), Some("val"));
    }
}
