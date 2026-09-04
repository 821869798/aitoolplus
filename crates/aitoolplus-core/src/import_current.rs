//! Import a tool's current real config as a provider record (cc-switch
//! first-run parity: your existing live setup shows up immediately).
//!
//! Each base tool gets an `import_current(paths, providers)` that reads its
//! real config file, builds one ProviderRecord named after the live
//! provider/base URL, and marks it applied. Idempotent by stable id:
//! repeated imports refresh the record instead of duplicating.

use serde_json::{Map, Value};

use crate::adapters::ToolAdapter;
use crate::adapters::claude_code::{KNOWN_ENV_FIELDS, build_provider_managed_env};
use crate::paths::Paths;
use crate::providers::ProviderRecord;
use crate::tools::ToolId;

/// Stable id for imported records: `live:<tool>`.
fn live_id(tool: ToolId) -> String {
    format!("live:{}", tool.key())
}

fn upsert_live(
    providers: &mut Vec<ProviderRecord>,
    tool: ToolId,
    name: impl Into<String>,
    category: &str,
    settings: Value,
) -> bool {
    let id = live_id(tool);
    let name = name.into();
    let settings_txt = serde_json::to_string_pretty(&settings).unwrap_or_default();
    // The live runtime is authoritative for what is currently applied.
    for provider in providers.iter_mut() {
        provider.is_applied = false;
    }
    if let Some(existing) = providers.iter_mut().find(|p| p.id == id) {
        let changed = existing.settings_config != settings_txt || existing.name != name;
        existing.name = name;
        existing.category = category.to_string();
        existing.settings_config = settings_txt;
        existing.is_applied = true;
        if changed {
            existing.touch();
        }
        return changed;
    }
    let mut rec = ProviderRecord::new(name, category);
    rec.id = id;
    rec.is_applied = true;
    rec.settings_config = settings_txt;
    rec.notes = Some("导入自当前生效配置 / imported from live config".into());
    providers.push(rec);
    true
}

fn read_json(path: &std::path::Path) -> Option<Value> {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
}

/// Claude Code: reconstruct the provider from settings.json's managed env.
pub fn import_claude_current(paths: &Paths, providers: &mut Vec<ProviderRecord>) -> bool {
    let settings_path = paths.primary_config(ToolId::ClaudeCode);
    let Some(settings) = read_json(&settings_path) else {
        return false;
    };
    if settings.get("env").and_then(Value::as_object).is_none() {
        return false;
    }

    let managed = build_provider_managed_env(&settings, &KNOWN_ENV_FIELDS);
    if managed.is_empty() {
        return false;
    }

    let base_url = managed
        .get("ANTHROPIC_BASE_URL")
        .and_then(Value::as_str)
        .unwrap_or("official");
    let name = if base_url == "official" {
        "官方 / Official".to_string()
    } else {
        host_of(base_url).unwrap_or_else(|| base_url.to_string())
    };
    upsert_live(
        providers,
        ToolId::ClaudeCode,
        name,
        if base_url == "official" {
            "official"
        } else {
            "custom"
        },
        serde_json::json!({ "env": Value::Object(managed) }),
    )
}

/// Codex: read `model_provider` + its `[model_providers.<key>]` table.
pub fn import_codex_current(paths: &Paths, providers: &mut Vec<ProviderRecord>) -> bool {
    let cfg_path = paths.primary_config(ToolId::Codex);
    if !cfg_path.exists() {
        return false;
    }
    let raw = std::fs::read_to_string(&cfg_path).unwrap_or_default();
    let Ok(doc) = raw.parse::<toml_edit::DocumentMut>() else {
        return false;
    };

    let Some(selector) = doc.get("model_provider").and_then(|v| v.as_str()) else {
        return false;
    };
    let provider_table = doc
        .get("model_providers")
        .and_then(|t| t.get(selector))
        .cloned();
    let base_url = provider_table
        .as_ref()
        .and_then(|t| t.get("base_url"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if base_url.is_empty() && selector == "openai" {
        // official OpenAI needs no table
        return upsert_live(
            providers,
            ToolId::Codex,
            "OpenAI 官方 / Official",
            "official",
            serde_json::json!({
                "toml": "model_provider = \"openai\"\n\n[model_providers.openai]\nname = \"OpenAI\"\nwire_api = \"responses\"\nrequires_openai_auth = true\n"
            }),
        );
    }
    if base_url.is_empty() {
        return false;
    }

    // rebuild the TOML text exactly as this app's adapter writes it
    let mut toml = format!("model_provider = \"{selector}\"\n\n[model_providers.{selector}]\n");
    let mut fields = vec![];
    if let Some(table) = provider_table.as_ref().and_then(|t| t.as_table()) {
        let mut pairs: Vec<(String, String)> = table
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string().trim().to_string()))
            .collect();
        pairs.sort();
        for (k, v) in pairs {
            if k == "name" || k == "base_url" || k == "wire_api" || k == "api_key" || k == "env_key"
            {
                toml.push_str(&format!("{k} = {v}\n"));
            } else {
                fields.push(format!("{k} = {v}"));
            }
        }
    }
    for extra in fields {
        toml.push_str(&extra);
        toml.push('\n');
    }

    let api_key = std::fs::read_to_string(paths.tool_root(ToolId::Codex).join("auth.json"))
        .ok()
        .and_then(|raw| serde_json::from_str::<Value>(&raw).ok())
        .and_then(|a| {
            a.get("OPENAI_API_KEY")
                .and_then(Value::as_str)
                .map(String::from)
        })
        .unwrap_or_default();
    if !api_key.is_empty() {
        // auth.json is the runtime authority. Drop any stale api_key from the
        // TOML projection, then append the fresh value to the provider table.
        toml = toml
            .lines()
            .filter(|line| !line.trim_start().starts_with("api_key ="))
            .collect::<Vec<_>>()
            .join("\n");
        toml.push('\n');
        toml.push_str(&format!("api_key = \"{api_key}\"\n"));
    }

    let host = host_of(base_url).unwrap_or_else(|| base_url.to_string());
    upsert_live(
        providers,
        ToolId::Codex,
        host,
        "custom",
        serde_json::json!({ "toml": toml }),
    )
}

/// Gemini CLI: `.env` managed keys.
pub fn import_gemini_current(paths: &Paths, providers: &mut Vec<ProviderRecord>) -> bool {
    let root = paths.tool_root(ToolId::GeminiCli);
    let env_path = root.join(".env");
    if !env_path.exists() {
        return false;
    }
    let env = crate::adapters::gemini_cli::parse_env_content(
        &std::fs::read_to_string(&env_path).unwrap_or_default(),
    );

    let has_key = env.contains_key("GEMINI_API_KEY") || env.contains_key("GOOGLE_API_KEY");
    if !has_key {
        return false;
    }
    let mut settings_env = Map::new();
    for (k, v) in &env {
        if crate::adapters::gemini_cli::MANAGED_ENV_KEYS.contains(&k.as_str()) {
            settings_env.insert(k.clone(), Value::String(v.clone()));
        }
    }
    let base_url = settings_env
        .get("GOOGLE_GEMINI_BASE_URL")
        .and_then(Value::as_str)
        .map(String::from);
    let name = base_url
        .as_deref()
        .and_then(host_of)
        .unwrap_or_else(|| "Gemini API Key".into());
    let mut settings = serde_json::json!({
        "env": Value::Object(settings_env),
        "security": {"auth": {"selectedType": "gemini-api-key"}},
    });
    if let Some(url) = base_url {
        settings["env"]["GOOGLE_GEMINI_BASE_URL"] = Value::String(url);
    }
    upsert_live(providers, ToolId::GeminiCli, name, "custom", settings)
}

/// OpenCode: `opencode.json(c)` top-level provider block.
pub fn import_opencode_current(paths: &Paths, providers: &mut Vec<ProviderRecord>) -> bool {
    let cfg = match crate::adapters::opencode::OpenCodeAdapter.read_current(paths) {
        Ok(value) => value,
        Err(_) => return false,
    };
    let Some(provider) = cfg.get("provider").and_then(Value::as_object) else {
        return false;
    };

    // find the first custom provider entry with a baseURL
    for (key, entry) in provider {
        let Some(obj) = entry.as_object() else {
            continue;
        };
        let base_url = obj
            .get("options")
            .and_then(|options| options.get("baseURL"))
            .or_else(|| obj.get("base_url"))
            .and_then(Value::as_str)
            .unwrap_or("");
        if base_url.is_empty() {
            continue;
        }
        let name = obj
            .get("name")
            .and_then(Value::as_str)
            .map(String::from)
            .unwrap_or_else(|| key.clone());
        let mut settings = Map::new();
        // Preserve OpenCode's real config shape so re-applying the imported
        // record recreates exactly the provider object. Also retain the live
        // model selector when present.
        settings.insert(
            "provider".into(),
            serde_json::json!({ key.clone(): entry.clone() }),
        );
        if let Some(model) = cfg.get("model").cloned() {
            settings.insert("model".into(), model);
        }
        if let Some(model) = cfg.get("small_model").cloned() {
            settings.insert("small_model".into(), model);
        }
        return upsert_live(
            providers,
            ToolId::OpenCode,
            name,
            "custom",
            Value::Object(settings),
        );
    }
    false
}

/// Grok: `config.toml` `[models].default` + the referenced `[model.<key>]`.
pub fn import_grok_current(paths: &Paths, providers: &mut Vec<ProviderRecord>) -> bool {
    let cfg = paths.primary_config(ToolId::Grok);
    if !cfg.exists() {
        return false;
    }
    let raw = std::fs::read_to_string(&cfg).unwrap_or_default();
    let Ok(doc) = raw.parse::<toml_edit::DocumentMut>() else {
        return false;
    };
    let Some(default_key) = doc
        .get("models")
        .and_then(|m| m.get("default"))
        .and_then(|v| v.as_str())
    else {
        return false;
    };
    let Some(model) = doc.get("model").and_then(|m| m.get(default_key)) else {
        return false;
    };
    let base_url = model.get("base_url").and_then(|v| v.as_str()).unwrap_or("");
    if base_url.is_empty() {
        return false;
    }
    let mut fields = Map::new();
    for key in [
        "base_url",
        "api_key",
        "api_backend",
        "default_reasoning_effort",
    ] {
        if let Some(v) = model.get(key).and_then(|v| v.as_str()) {
            fields.insert(key.into(), Value::String(v.to_string()));
        }
    }
    let mut model_entry = fields;
    model_entry.insert("key".into(), Value::String(default_key.to_string()));
    let catalog = Value::Array(vec![Value::Object(model_entry)]);
    let name = host_of(base_url).unwrap_or_else(|| base_url.to_string());
    upsert_live(
        providers,
        ToolId::Grok,
        name,
        "custom",
        serde_json::json!({
            "model": default_key,
            "modelCatalog": catalog,
        }),
    )
}

fn host_of(url: &str) -> Option<String> {
    let rest = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))?;
    let host = rest.split(['/']).next()?;
    Some(host.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() -> (tempfile::TempDir, Paths) {
        let dir = tempfile::tempdir().unwrap();
        let paths = Paths::new(dir.path().join("home"), dir.path().join("data"));
        (dir, paths)
    }

    #[test]
    fn claude_live_import_and_refresh() {
        let (_dir, paths) = setup();
        let root = paths.tool_root(ToolId::ClaudeCode);
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(
            root.join("settings.json"),
            r#"{"env": {"ANTHROPIC_AUTH_TOKEN": "tok-1", "ANTHROPIC_BASE_URL": "https://gw.example.com", "ANTHROPIC_MODEL": "m1", "USER_KEEP": "x"}}"#,
        )
        .unwrap();

        let mut providers = vec![];
        assert!(import_claude_current(&paths, &mut providers));
        assert_eq!(providers.len(), 1);
        let p = &providers[0];
        assert_eq!(p.id, "live:claude_code");
        assert_eq!(p.name, "gw.example.com");
        assert!(p.is_applied);
        let s = p.settings();
        assert_eq!(s["env"]["ANTHROPIC_BASE_URL"], "https://gw.example.com");
        assert_eq!(s["env"]["ANTHROPIC_MODEL"], "m1");
        // non-managed keys are not part of the provider record
        assert!(s["env"].get("USER_KEEP").is_none());

        // re-import is idempotent (refresh, no duplicate)
        assert!(!import_claude_current(&paths, &mut providers));
        assert_eq!(providers.len(), 1);

        // switching the live URL refreshes the record
        std::fs::write(
            root.join("settings.json"),
            r#"{"env": {"ANTHROPIC_AUTH_TOKEN": "tok-2", "ANTHROPIC_BASE_URL": "https://other.example.com"}}"#,
        )
        .unwrap();
        assert!(import_claude_current(&paths, &mut providers));
        assert_eq!(providers.len(), 1);
        assert_eq!(providers[0].name, "other.example.com");
    }

    #[test]
    fn claude_official_detected() {
        let (_dir, paths) = setup();
        let root = paths.tool_root(ToolId::ClaudeCode);
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(
            root.join("settings.json"),
            r#"{"env": {"ANTHROPIC_AUTH_TOKEN": "tok"}}"#,
        )
        .unwrap();
        let mut providers = vec![];
        assert!(import_claude_current(&paths, &mut providers));
        assert_eq!(providers[0].name, "官方 / Official");
        assert_eq!(providers[0].category, "official");
    }

    #[test]
    fn codex_live_import_with_auth_key() {
        let (_dir, paths) = setup();
        let root = paths.tool_root(ToolId::Codex);
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(
            root.join("config.toml"),
            "model_provider = \"custom\"\n\n[model_providers.custom]\nname = \"custom\"\nbase_url = \"https://gw.example.com/v1\"\nwire_api = \"chat\"\napi_key = \"stale\"\n",
        )
        .unwrap();
        std::fs::write(root.join("auth.json"), r#"{"OPENAI_API_KEY": "fresh-key"}"#).unwrap();

        let mut providers = vec![];
        assert!(import_codex_current(&paths, &mut providers));
        let p = &providers[0];
        assert_eq!(p.name, "gw.example.com");
        let settings = p.settings();
        let toml = settings["toml"].as_str().unwrap();
        assert!(toml.contains("model_provider = \"custom\""));
        assert!(toml.contains("base_url = \"https://gw.example.com/v1\""));
        assert!(toml.contains("api_key = \"fresh-key\""), "auth key merged");
    }

    #[test]
    fn codex_official_openai() {
        let (_dir, paths) = setup();
        let root = paths.tool_root(ToolId::Codex);
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("config.toml"), "model_provider = \"openai\"\n").unwrap();
        let mut providers = vec![];
        assert!(import_codex_current(&paths, &mut providers));
        assert_eq!(providers[0].category, "official");
    }

    #[test]
    fn gemini_live_import() {
        let (_dir, paths) = setup();
        let root = paths.tool_root(ToolId::GeminiCli);
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(
            root.join(".env"),
            "# keep\nGEMINI_API_KEY=k-1\nGOOGLE_GEMINI_BASE_URL=https://relay.example.com\nOTHER=1\n",
        )
        .unwrap();
        let mut providers = vec![];
        assert!(import_gemini_current(&paths, &mut providers));
        let p = &providers[0];
        assert_eq!(p.name, "relay.example.com");
        assert_eq!(p.settings()["env"]["GEMINI_API_KEY"], "k-1");
        assert_eq!(
            p.settings()["security"]["auth"]["selectedType"],
            "gemini-api-key"
        );
        assert!(p.settings()["env"].get("OTHER").is_none());
    }

    #[test]
    fn opencode_live_import_jsonc() {
        let (_dir, paths) = setup();
        let root = paths.tool_root(ToolId::OpenCode);
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(
            root.join("opencode.jsonc"),
            "// c\n{\n  \"provider\": {\n    \"qzzio\": {\n      \"name\": \"我的中转站\",\n      \"npm\": \"@ai-sdk/openai-compatible\",\n      \"options\": { \"baseURL\": \"https://relay.example.com/v1\" },\n      \"models\": {}\n    }\n  }\n}\n",
        )
        .unwrap();
        let mut providers = vec![];
        assert!(import_opencode_current(&paths, &mut providers));
        let p = &providers[0];
        assert_eq!(p.name, "我的中转站");
        assert_eq!(p.settings()["provider"]["qzzio"]["name"], "我的中转站");
        assert_eq!(
            p.settings()["provider"]["qzzio"]["options"]["baseURL"],
            "https://relay.example.com/v1"
        );
    }

    #[test]
    fn grok_live_import() {
        let (_dir, paths) = setup();
        let root = paths.tool_root(ToolId::Grok);
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(
            root.join("config.toml"),
            "[model.gw]\nbase_url = \"https://gw.example.com\"\napi_key = \"k\"\n\n[models]\ndefault = \"gw\"\n",
        )
        .unwrap();
        let mut providers = vec![];
        assert!(import_grok_current(&paths, &mut providers));
        let p = &providers[0];
        assert_eq!(p.name, "gw.example.com");
        assert_eq!(p.settings()["model"], "gw");
        assert_eq!(
            p.settings()["modelCatalog"][0]["base_url"],
            "https://gw.example.com"
        );
    }

    #[test]
    fn imported_records_reapply_without_losing_core_shape() {
        let (_dir, source) = setup();

        // Claude live source -> imported record.
        let claude_root = source.tool_root(ToolId::ClaudeCode);
        std::fs::create_dir_all(&claude_root).unwrap();
        std::fs::write(
            claude_root.join("settings.json"),
            r#"{"env":{"ANTHROPIC_AUTH_TOKEN":"ak","ANTHROPIC_BASE_URL":"https://a.example","ANTHROPIC_MODEL":"m1"},"permissions":{"allow":["Bash"]}}"#,
        )
        .unwrap();
        let mut claude = vec![];
        assert!(import_claude_current(&source, &mut claude));

        let target_dir = tempfile::tempdir().unwrap();
        let target = Paths::new(
            target_dir.path().join("home"),
            target_dir.path().join("data"),
        );
        let ctx = crate::adapters::ApplyCtx {
            paths: &target,
            common_config: "{}",
            provider: &claude[0],
            strategy: crate::config::MergeStrategy::default(),
            provider_optional: false,
        };
        crate::adapters::adapter_for(ToolId::ClaudeCode)
            .apply(&ctx)
            .unwrap();
        let out = read_json(&target.primary_config(ToolId::ClaudeCode)).unwrap();
        assert_eq!(out["env"]["ANTHROPIC_BASE_URL"], "https://a.example");
        assert_eq!(out["env"]["ANTHROPIC_MODEL"], "m1");

        // OpenCode live source -> imported record -> apply keeps provider map.
        let open_root = source.tool_root(ToolId::OpenCode);
        std::fs::create_dir_all(&open_root).unwrap();
        std::fs::write(
            open_root.join("opencode.json"),
            r#"{"model":"qzzio/gpt-5","provider":{"qzzio":{"name":"Q","options":{"baseURL":"https://q.example/v1","apiKey":"k"},"models":{"gpt-5":{"name":"G"}}}}}"#,
        )
        .unwrap();
        let mut open = vec![];
        assert!(import_opencode_current(&source, &mut open));
        let ctx = crate::adapters::ApplyCtx {
            paths: &target,
            common_config: "{}",
            provider: &open[0],
            strategy: crate::config::MergeStrategy::default(),
            provider_optional: false,
        };
        crate::adapters::adapter_for(ToolId::OpenCode)
            .apply(&ctx)
            .unwrap();
        let out = read_json(&crate::adapters::opencode::resolve_config_file(&target)).unwrap();
        assert_eq!(out["model"], "qzzio/gpt-5");
        assert_eq!(
            out["provider"]["qzzio"]["options"]["baseURL"],
            "https://q.example/v1"
        );
        assert_eq!(out["provider"]["qzzio"]["models"]["gpt-5"]["name"], "G");
    }

    #[test]
    fn empty_configs_import_nothing() {
        let (_dir, paths) = setup();
        let mut providers = vec![];
        assert!(!import_claude_current(&paths, &mut providers));
        assert!(!import_codex_current(&paths, &mut providers));
        assert!(!import_gemini_current(&paths, &mut providers));
        assert!(!import_opencode_current(&paths, &mut providers));
        assert!(!import_grok_current(&paths, &mut providers));
        assert!(providers.is_empty());
    }
}
