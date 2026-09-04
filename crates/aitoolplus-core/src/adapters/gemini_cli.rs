//! Gemini CLI adapter: `~/.gemini/.env` (managed keys) + `settings.json`.
//!
//! Ported from ai-toolbox `coding/gemini_cli/commands.rs` semantics:
//! - `.env` has a fixed MANAGED_ENV_KEYS set; applying a provider removes all
//!   previous managed keys (stale ones contaminate the auth selection), then
//!   writes the provider's env; unrelated lines survive untouched
//! - `.env` parsing understands quotes and `export` prefixes
//! - `settings.json` merges deeply (objects recurse, leaves replace)
//! - auth-selector normalization both directions:
//!   * official providers: `security.auth.selectedType = "oauth-personal"`
//!     and every API-key env var is removed from the stored settings
//!   * custom/gateway providers: `selectedType = "gemini-api-key"`
//! - prompt file follows `settings.json` `context.fileName` (first valid,
//!   `../`-free entry), falling back to `GEMINI.md`

use serde_json::{Map, Value};

use crate::adapters::{
    AppliedReport, ApplyCtx, ApplyError, ToolAdapter, backup_file, merged_payload, write_atomic,
};
use crate::paths::Paths;
use crate::tools::ToolId;

pub struct GeminiCliAdapter;

/// Env keys this app manages in `.env` (upstream MANAGED_ENV_KEYS).
pub const MANAGED_ENV_KEYS: [&str; 14] = [
    "GEMINI_API_KEY",
    "GOOGLE_API_KEY",
    "GOOGLE_GEMINI_BASE_URL",
    "GOOGLE_VERTEX_BASE_URL",
    "GOOGLE_GENAI_USE_GCA",
    "GOOGLE_GENAI_USE_VERTEXAI",
    "GEMINI_CLI_USE_COMPUTE_ADC",
    "GEMINI_CLI_CUSTOM_HEADERS",
    "GEMINI_MODEL",
    "GEMINI_API_KEY_AUTH_MECHANISM",
    "GOOGLE_GENAI_API_VERSION",
    "GOOGLE_CLOUD_PROJECT",
    "GOOGLE_CLOUD_PROJECT_ID",
    "GOOGLE_CLOUD_LOCATION",
];

/// Env keys removed when an official OAuth provider is applied/stored
/// (everything managed except GEMINI_MODEL, which is mode-independent).
const OFFICIAL_REMOVED_ENV_KEYS: [&str; 13] = [
    "GEMINI_API_KEY",
    "GOOGLE_API_KEY",
    "GOOGLE_GEMINI_BASE_URL",
    "GOOGLE_VERTEX_BASE_URL",
    "GOOGLE_GENAI_USE_GCA",
    "GOOGLE_GENAI_USE_VERTEXAI",
    "GEMINI_CLI_USE_COMPUTE_ADC",
    "GEMINI_CLI_CUSTOM_HEADERS",
    "GEMINI_API_KEY_AUTH_MECHANISM",
    "GOOGLE_GENAI_API_VERSION",
    "GOOGLE_CLOUD_PROJECT",
    "GOOGLE_CLOUD_PROJECT_ID",
    "GOOGLE_CLOUD_LOCATION",
];

pub const OFFICIAL_AUTH_TYPE: &str = "oauth-personal";
pub const CUSTOM_AUTH_TYPE: &str = "gemini-api-key";
pub const DEFAULT_PROMPT_FILE: &str = "GEMINI.md";

/// Parse `.env` content into a key map, understanding quotes and `export`.
pub fn parse_env_content(content: &str) -> std::collections::BTreeMap<String, String> {
    let mut out = std::collections::BTreeMap::new();
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let stripped = trimmed.strip_prefix("export ").unwrap_or(trimmed);
        let Some(eq) = stripped.find('=') else {
            continue;
        };
        let key = stripped[..eq].trim().to_string();
        let mut val = stripped[eq + 1..].trim().to_string();
        // strip surrounding quotes (single or double)
        if val.len() >= 2
            && ((val.starts_with('"') && val.ends_with('"'))
                || (val.starts_with('\'') && val.ends_with('\'')))
        {
            val = val[1..val.len() - 1].to_string();
        }
        if !key.is_empty() {
            out.insert(key, val);
        }
    }
    out
}

/// Remove every managed key from `.env` text, then write the provider's env
/// entries. Unrelated lines and comments survive.
pub fn merge_env_content(
    existing: &str,
    provider_env: &std::collections::BTreeMap<String, String>,
) -> String {
    let mut kept: Vec<String> = vec![];
    let mut seen: std::collections::HashSet<String> = Default::default();

    for line in existing.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let stripped = trimmed.strip_prefix("export ").unwrap_or(trimmed);
        if let Some(eq) = stripped.find('=') {
            let key = stripped[..eq].trim();
            if MANAGED_ENV_KEYS.contains(&key) {
                continue; // drop all managed keys, provider re-adds its own
            }
            seen.insert(key.to_string());
        }
        kept.push(line.to_string());
    }

    for (k, v) in provider_env {
        if seen.contains(k) {
            // update in place
            for slot in kept.iter_mut() {
                let stripped = slot.trim().strip_prefix("export ").unwrap_or(slot.trim());
                if let Some(eq) = stripped.find('=')
                    && stripped[..eq].trim() == k
                {
                    *slot = format!("{k}={v}");
                }
            }
        } else {
            kept.push(format!("{k}={v}"));
        }
    }

    let mut out = kept.join("\n");
    if !out.is_empty() {
        out.push('\n');
    }
    out
}

/// Deep-merge `patch` into `base` (objects recurse, leaves replace).
pub fn merge_json_value(base: &mut Value, patch: &Value) {
    match (base, patch) {
        (Value::Object(b), Value::Object(p)) => {
            for (k, v) in p {
                match b.get_mut(k) {
                    Some(existing) => merge_json_value(existing, v),
                    None => {
                        b.insert(k.clone(), v.clone());
                    }
                }
            }
        }
        (b, p) => *b = p.clone(),
    }
}

/// Normalize provider settings before storage/apply:
/// - `official` category: force `selectedType = oauth-personal`, strip all
///   API-key/gateway env keys
/// - otherwise: force `selectedType = gemini-api-key`, keep custom env keys
pub fn normalize_provider_settings(settings: &Value, category: &str) -> Value {
    let mut out = settings.clone();
    let target = if category == "official" {
        OFFICIAL_AUTH_TYPE
    } else {
        CUSTOM_AUTH_TYPE
    };

    if out.get("env").is_some() {
        let env = out.get("env").cloned().unwrap_or(Value::Object(Map::new()));
        let mut env_obj = env.as_object().cloned().unwrap_or_default();
        if category == "official" {
            for key in OFFICIAL_REMOVED_ENV_KEYS {
                env_obj.remove(key);
            }
        }
        if env_obj.is_empty() {
            out.as_object_mut().unwrap().remove("env");
        } else {
            out.as_object_mut()
                .unwrap()
                .insert("env".into(), Value::Object(env_obj));
        }
    }

    // Force security.auth.selectedType inside the `config` sub-object (or at
    // the top level when the provider stores settings.json shape directly).
    let pointer = if out.get("config").is_some() {
        "/config/security/auth/selectedType"
    } else {
        "/security/auth/selectedType"
    };
    if let Some(sel) = out.pointer_mut(pointer) {
        *sel = Value::String(target.into());
    }
    out
}

/// Resolve the effective prompt file name from settings.json content:
/// `context.fileName` may be a string or an array; use the first valid
/// (non-empty, no `..`) entry; fall back to `GEMINI.md`.
pub fn prompt_file_name_from_settings(settings: &Value) -> String {
    let raw = settings
        .pointer("/context/fileName")
        .or_else(|| settings.pointer("/config/context/fileName"));
    let candidates: Vec<String> = match raw {
        Some(Value::String(s)) => vec![s.clone()],
        Some(Value::Array(arr)) => arr
            .iter()
            .filter_map(Value::as_str)
            .map(String::from)
            .collect(),
        _ => vec![],
    };
    for candidate in candidates {
        let trimmed = candidate.trim();
        if !trimmed.is_empty() && !trimmed.contains("..") {
            return trimmed.to_string();
        }
    }
    DEFAULT_PROMPT_FILE.to_string()
}

impl ToolAdapter for GeminiCliAdapter {
    fn tool(&self) -> ToolId {
        ToolId::GeminiCli
    }

    fn apply(&self, ctx: &ApplyCtx) -> Result<AppliedReport, ApplyError> {
        let payload = merged_payload(ctx);
        let root = ctx.paths.tool_root(ToolId::GeminiCli);
        let mut files = vec![];

        // normalize auth selector before writing
        let payload = normalize_provider_settings(&payload, &ctx.provider.category);

        // 1) .env: strip managed keys, write provider env
        if let Some(env_obj) = payload.get("env").and_then(Value::as_object) {
            let env_file = root.join(".env");
            if env_file.exists() {
                backup_file(ctx.paths, ToolId::GeminiCli, &env_file);
            }
            let current = if env_file.exists() {
                std::fs::read_to_string(&env_file).unwrap_or_default()
            } else {
                String::new()
            };
            let kvs: std::collections::BTreeMap<String, String> = env_obj
                .iter()
                .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                .collect();
            write_atomic(&env_file, &merge_env_content(&current, &kvs))?;
            files.push(env_file);
        }

        // 2) settings.json: deep-merge everything except env
        let mut layer = payload.clone();
        if let Some(obj) = layer.as_object_mut() {
            obj.remove("env");
        }
        if !layer.as_object().map(|o| o.is_empty()).unwrap_or(true) {
            let settings_file = root.join("settings.json");
            if settings_file.exists() {
                backup_file(ctx.paths, ToolId::GeminiCli, &settings_file);
            }
            let mut current: Value = if settings_file.exists() {
                serde_json::from_str(&std::fs::read_to_string(&settings_file).unwrap_or_default())
                    .unwrap_or(Value::Object(Map::new()))
            } else {
                Value::Object(Map::new())
            };
            merge_json_value(&mut current, &layer);
            write_atomic(
                &settings_file,
                &serde_json::to_string_pretty(&current).unwrap(),
            )?;
            files.push(settings_file);
        }

        Ok(AppliedReport { files })
    }

    fn read_current(&self, paths: &Paths) -> Result<Value, ApplyError> {
        let root = paths.tool_root(ToolId::GeminiCli);
        let mut out = Value::Object(Map::new());
        let settings = root.join("settings.json");
        if settings.exists()
            && let Ok(v) = serde_json::from_str::<Value>(
                &std::fs::read_to_string(&settings).unwrap_or_default(),
            )
        {
            out = v;
        }
        let env_file = root.join(".env");
        if env_file.exists()
            && let Ok(raw) = std::fs::read_to_string(&env_file)
        {
            let env = parse_env_content(&raw);
            let mut env_obj = Map::new();
            for (k, v) in env {
                env_obj.insert(k, Value::String(v));
            }
            if let Some(obj) = out.as_object_mut() {
                obj.insert("env".into(), Value::Object(env_obj));
            }
        }
        Ok(out)
    }

    fn prompt_file(&self, paths: &Paths) -> Option<std::path::PathBuf> {
        let root = paths.tool_root(ToolId::GeminiCli);
        let settings = root.join("settings.json");
        let name = if settings.exists() {
            std::fs::read_to_string(&settings)
                .ok()
                .and_then(|raw| serde_json::from_str::<Value>(&raw).ok())
                .map(|v| prompt_file_name_from_settings(&v))
                .unwrap_or_else(|| DEFAULT_PROMPT_FILE.to_string())
        } else {
            DEFAULT_PROMPT_FILE.to_string()
        };
        Some(root.join(name))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::MergeStrategy;
    use crate::providers::ProviderRecord;
    use serde_json::json;
    use std::collections::BTreeMap;

    fn setup() -> (tempfile::TempDir, Paths) {
        let dir = tempfile::tempdir().unwrap();
        let paths = Paths::new(dir.path().join("home"), dir.path().join("data"));
        (dir, paths)
    }

    fn ctx<'a>(paths: &'a Paths, p: &'a ProviderRecord) -> ApplyCtx<'a> {
        ApplyCtx {
            paths,
            common_config: "{}",
            provider: p,
            strategy: MergeStrategy::default(),
            provider_optional: false,
        }
    }

    #[test]
    fn env_merge_removes_all_previous_managed_keys() {
        let existing = "# keep\nOTHER=1\nGEMINI_API_KEY=old\nGOOGLE_GENAI_USE_GCA=true\nGOOGLE_GENAI_USE_VERTEXAI=true\nGOOGLE_VERTEX_BASE_URL=https://old.vertex.example\nGOOGLE_CLOUD_PROJECT=old-project\nGEMINI_CLI_CUSTOM_HEADERS=old\n";
        let provider_env = BTreeMap::from([
            ("GEMINI_API_KEY".to_string(), "new".to_string()),
            ("GEMINI_MODEL".to_string(), "gemini-3.1-pro".to_string()),
            (
                "GEMINI_CLI_CUSTOM_HEADERS".to_string(),
                "x-provider:direct".to_string(),
            ),
        ]);
        let merged = merge_env_content(existing, &provider_env);
        assert!(merged.contains("# keep"));
        assert!(merged.contains("OTHER=1"));
        assert!(!merged.contains("GEMINI_API_KEY=old"));
        assert!(!merged.contains("GOOGLE_GENAI_USE_GCA=true"));
        assert!(!merged.contains("GOOGLE_GENAI_USE_VERTEXAI=true"));
        assert!(!merged.contains("GOOGLE_VERTEX_BASE_URL=https://old.vertex.example"));
        assert!(!merged.contains("GOOGLE_CLOUD_PROJECT=old-project"));
        assert!(!merged.contains("GEMINI_CLI_CUSTOM_HEADERS=old"));
        assert!(merged.contains("GEMINI_API_KEY=new"));
        assert!(merged.contains("GEMINI_MODEL=gemini-3.1-pro"));
        assert!(merged.contains("GEMINI_CLI_CUSTOM_HEADERS=x-provider:direct"));
    }

    #[test]
    fn env_parser_handles_quotes_and_export() {
        let parsed = parse_env_content("GEMINI_API_KEY=\"abc 123\"\nexport GEMINI_MODEL='flash'\n");
        assert_eq!(
            parsed.get("GEMINI_API_KEY").map(String::as_str),
            Some("abc 123")
        );
        assert_eq!(
            parsed.get("GEMINI_MODEL").map(String::as_str),
            Some("flash")
        );
    }

    #[test]
    fn official_provider_forces_oauth_and_strips_api_env() {
        let settings = json!({
            "env": {
                "GEMINI_MODEL": "gemini-3.1-pro-preview",
                "GEMINI_API_KEY": "stale-key",
                "GOOGLE_GEMINI_BASE_URL": "https://proxy.example/v1"
            },
            "config": {
                "security": { "auth": { "selectedType": "gemini-api-key" } }
            }
        });
        let normalized = normalize_provider_settings(&settings, "official");
        assert_eq!(
            normalized.pointer("/config/security/auth/selectedType"),
            Some(&Value::String("oauth-personal".into()))
        );
        assert_eq!(normalized["env"]["GEMINI_MODEL"], "gemini-3.1-pro-preview");
        assert!(normalized["env"].get("GEMINI_API_KEY").is_none());
        assert!(normalized["env"].get("GOOGLE_GEMINI_BASE_URL").is_none());
    }

    #[test]
    fn custom_provider_forces_api_key_auth_type() {
        let settings = json!({
            "env": { "GEMINI_API_KEY": "k1" },
            "config": {
                "security": { "auth": { "selectedType": "oauth-personal" } }
            }
        });
        let normalized = normalize_provider_settings(&settings, "custom");
        assert_eq!(
            normalized.pointer("/config/security/auth/selectedType"),
            Some(&Value::String("gemini-api-key".into()))
        );
        assert_eq!(normalized["env"]["GEMINI_API_KEY"], "k1");
    }

    #[test]
    fn prompt_file_follows_context_file_name() {
        let settings = json!({
            "context": { "fileName": [" AGENTS.md ", "GEMINI.md"] }
        });
        assert_eq!(prompt_file_name_from_settings(&settings), "AGENTS.md");

        let invalid = json!({
            "context": { "fileName": ["../outside.md", "", "GEMINI.md"] }
        });
        assert_eq!(prompt_file_name_from_settings(&invalid), "GEMINI.md");

        assert_eq!(prompt_file_name_from_settings(&json!({})), "GEMINI.md");
    }

    #[test]
    fn apply_writes_both_files_and_clears_stale_auth() {
        let (_dir, paths) = setup();
        let root = paths.tool_root(ToolId::GeminiCli);
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join(".env"), "# keep\nOTHER=1\nGEMINI_API_KEY=stale\n").unwrap();
        std::fs::write(
            root.join("settings.json"),
            r#"{"theme":"dark","security":{"auth":{"selectedType":"oauth-personal"}}}"#,
        )
        .unwrap();

        let mut p = ProviderRecord::new("MyProxy", "custom");
        p.set_settings(&json!({
            "env": {
                "GEMINI_API_KEY": "new-key",
                "GOOGLE_GEMINI_BASE_URL": "https://proxy.example/v1"
            },
            "security": { "auth": { "selectedType": "gemini-api-key" } }
        }));

        let report = GeminiCliAdapter.apply(&ctx(&paths, &p)).unwrap();
        assert_eq!(report.files.len(), 2);

        let env = std::fs::read_to_string(root.join(".env")).unwrap();
        assert!(env.contains("# keep"));
        assert!(env.contains("OTHER=1"));
        assert!(!env.contains("GEMINI_API_KEY=stale"));
        assert!(env.contains("GEMINI_API_KEY=new-key"));
        assert!(env.contains("GOOGLE_GEMINI_BASE_URL=https://proxy.example/v1"));

        let settings: Value =
            serde_json::from_str(&std::fs::read_to_string(root.join("settings.json")).unwrap())
                .unwrap();
        assert_eq!(settings["theme"], "dark", "unrelated setting lost");
        assert_eq!(
            settings["security"]["auth"]["selectedType"],
            "gemini-api-key"
        );
    }
}
