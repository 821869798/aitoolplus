//! OpenClaw adapter: single-file `~/.openclaw/openclaw.json`.
//!
//! OpenClaw is a config-FILE module (not a root module): every page section
//! edits the same JSON object, so writes must deep-merge and never drop
//! unknown top-level fields (upstream `open_claw/AGENTS.md`).

use crate::adapters::{
    AppliedReport, ApplyCtx, ApplyError, ToolAdapter, backup_file, merged_payload,
};
use crate::tools::ToolId;

pub struct OpenClawAdapter;

impl ToolAdapter for OpenClawAdapter {
    fn tool(&self) -> ToolId {
        ToolId::OpenClaw
    }

    fn apply(&self, ctx: &ApplyCtx) -> Result<AppliedReport, ApplyError> {
        let path = ctx.paths.primary_config(ToolId::OpenClaw);
        if path.exists() {
            backup_file(ctx.paths, ToolId::OpenClaw, &path);
        }
        let payload = merged_payload(ctx);

        let current: serde_json::Value = if path.exists() {
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap_or_default())
                .unwrap_or(serde_json::Value::Object(serde_json::Map::new()))
        } else {
            serde_json::Value::Object(serde_json::Map::new())
        };
        let mut merged = current;

        let provider_key = ctx.provider.id.strip_prefix("openclaw:").unwrap_or(&ctx.provider.id);
        if payload.get("models").and_then(|m| m.get("providers")).is_none()
            && (payload.get("baseUrl").is_some()
                || payload.get("apiKey").is_some()
                || payload.get("models").is_some()
                || payload.get("api").is_some())
        {
            if !merged.get("models").is_some_and(serde_json::Value::is_object) {
                merged["models"] = serde_json::json!({
                    "mode": "merge",
                    "providers": {}
                });
            }
            if !merged["models"].get("providers").is_some_and(serde_json::Value::is_object) {
                merged["models"]["providers"] = serde_json::json!({});
            }
            if let Some(providers) = merged["models"]["providers"].as_object_mut() {
                providers.insert(provider_key.to_string(), payload.clone());
            }

            if let Some(m) = payload.get("model").and_then(serde_json::Value::as_str) {
                if !m.trim().is_empty() {
                    let full_m = if m.contains('/') {
                        m.to_string()
                    } else {
                        format!("{}/{}", provider_key, m)
                    };
                    if !merged.get("agents").is_some_and(serde_json::Value::is_object) {
                        merged["agents"] = serde_json::json!({});
                    }
                    if !merged["agents"].get("defaults").is_some_and(serde_json::Value::is_object) {
                        merged["agents"]["defaults"] = serde_json::json!({});
                    }
                    if !merged["agents"]["defaults"].get("model").is_some_and(serde_json::Value::is_object) {
                        merged["agents"]["defaults"]["model"] = serde_json::json!({});
                    }
                    merged["agents"]["defaults"]["model"]["primary"] = serde_json::Value::String(full_m);
                }
            }
        }

        crate::adapters::gemini_cli::merge_json_value(&mut merged, &payload);
        crate::adapters::write_atomic(&path, &serde_json::to_string_pretty(&merged).unwrap())?;
        Ok(AppliedReport { files: vec![path] })
    }
}

/// Remove an OpenClaw provider from openclaw.json.
pub fn remove_provider(paths: &crate::paths::Paths, provider_id: &str) -> Result<Vec<std::path::PathBuf>, String> {
    let path = paths.primary_config(ToolId::OpenClaw);
    if !path.exists() {
        return Ok(vec![]);
    }
    let key = provider_id.strip_prefix("openclaw:").unwrap_or(provider_id);
    let raw = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let mut config: serde_json::Value = serde_json::from_str(&raw).unwrap_or(serde_json::Value::Object(serde_json::Map::new()));
    let mut changed = false;

    if let Some(providers) = config
        .get_mut("models")
        .and_then(|m| m.get_mut("providers"))
        .and_then(serde_json::Value::as_object_mut)
    {
        if providers.remove(key).is_some() {
            changed = true;
        }
    }
    if let Some(m) = config.get("models").and_then(|m| m.get("model")).and_then(serde_json::Value::as_str) {
        if m.starts_with(&format!("{key}/")) || m == key {
            if let Some(models) = config.get_mut("models").and_then(serde_json::Value::as_object_mut) {
                models.remove("model");
                changed = true;
            }
        }
    }
    if let Some(primary) = config
        .get("agents")
        .and_then(|a| a.get("defaults"))
        .and_then(|d| d.get("model"))
        .and_then(|m| m.get("primary"))
        .and_then(serde_json::Value::as_str)
    {
        if primary.starts_with(&format!("{key}/")) || primary == key {
            if let Some(m) = config
                .get_mut("agents")
                .and_then(|a| a.get_mut("defaults"))
                .and_then(|d| d.get_mut("model"))
                .and_then(serde_json::Value::as_object_mut)
            {
                m.remove("primary");
                changed = true;
            }
        }
    }

    if changed {
        crate::adapters::write_atomic(&path, &serde_json::to_string_pretty(&config).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
        Ok(vec![path])
    } else {
        Ok(vec![])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::MergeStrategy;
    use crate::paths::Paths;
    use crate::providers::ProviderRecord;
    use serde_json::json;

    #[test]
    fn writes_openclaw_json_not_config_json() {
        let dir = tempfile::tempdir().unwrap();
        let paths = Paths::new(dir.path().join("home"), dir.path().join("appdata"));
        let mut p = ProviderRecord::new("OC", "custom");
        p.set_settings(&json!({"model": "gpt-4o"}));
        let ctx = ApplyCtx {
            paths: &paths,
            common_config: "{}",
            provider: &p,
            strategy: MergeStrategy::default(),
            provider_optional: false,
        };
        let report = OpenClawAdapter.apply(&ctx).unwrap();
        assert!(
            report.files[0].ends_with("openclaw.json"),
            "wrong file: {:?}",
            report.files[0]
        );
        let v: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&report.files[0]).unwrap()).unwrap();
        assert_eq!(v["model"], "gpt-4o");
    }

    #[test]
    fn apply_preserves_other_sections_of_same_object() {
        let dir = tempfile::tempdir().unwrap();
        let paths = Paths::new(dir.path().join("home"), dir.path().join("appdata"));
        let cfg = paths.home.join(".openclaw").join("openclaw.json");
        std::fs::create_dir_all(cfg.parent().unwrap()).unwrap();
        std::fs::write(
            &cfg,
            r#"{"env":{"CLAW_KEY":"keep"},"tools":{"web":{"enabled":true}},"agents":{"defaults":{"temperature":0.7}}}"#,
        )
        .unwrap();

        let mut p = ProviderRecord::new("OC", "custom");
        p.set_settings(&json!({"model": "claude-sonnet-4"}));
        let ctx = ApplyCtx {
            paths: &paths,
            common_config: "{}",
            provider: &p,
            strategy: MergeStrategy::default(),
            provider_optional: false,
        };
        OpenClawAdapter.apply(&ctx).unwrap();

        let v: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&cfg).unwrap()).unwrap();
        // provider field written
        assert_eq!(v["model"], "claude-sonnet-4");
        // other sections of the same object survive
        assert_eq!(v["env"]["CLAW_KEY"], "keep");
        assert_eq!(v["tools"]["web"]["enabled"], true);
        assert_eq!(v["agents"]["defaults"]["temperature"], 0.7);
    }

    #[test]
    fn apply_projects_models_providers_and_primary_model() {
        let dir = tempfile::tempdir().unwrap();
        let paths = Paths::new(dir.path().join("home"), dir.path().join("appdata"));
        let mut p = ProviderRecord::new("MyOpenClaw", "custom");
        p.id = "openclaw:custom-gw".to_string();
        p.set_settings(&json!({
            "baseUrl": "https://api.gw.com/v1",
            "apiKey": "sk-123",
            "model": "claude-3-5-sonnet"
        }));
        let ctx = ApplyCtx {
            paths: &paths,
            common_config: "{}",
            provider: &p,
            strategy: MergeStrategy::default(),
            provider_optional: false,
        };
        let report = OpenClawAdapter.apply(&ctx).unwrap();
        let v: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&report.files[0]).unwrap()).unwrap();
        assert_eq!(v["models"]["providers"]["custom-gw"]["baseUrl"], "https://api.gw.com/v1");
        assert_eq!(v["models"]["providers"]["custom-gw"]["apiKey"], "sk-123");
        assert_eq!(v["agents"]["defaults"]["model"]["primary"], "custom-gw/claude-3-5-sonnet");
    }

    #[test]
    fn test_remove_provider_cleans_config() {
        let dir = tempfile::tempdir().unwrap();
        let paths = Paths::new(dir.path().join("home"), dir.path().join("appdata"));
        let cfg = paths.primary_config(ToolId::OpenClaw);
        std::fs::create_dir_all(cfg.parent().unwrap()).unwrap();
        std::fs::write(
            &cfg,
            r#"{
                "models": {
                    "providers": {
                        "to_remove": {"baseUrl": "https://r.com"},
                        "keep": {"baseUrl": "https://k.com"}
                    }
                },
                "agents": {
                    "defaults": {
                        "model": {
                            "primary": "to_remove/m1"
                        }
                    }
                }
            }"#,
        ).unwrap();

        let removed = remove_provider(&paths, "openclaw:to_remove").unwrap();
        assert!(!removed.is_empty());

        let content = std::fs::read_to_string(&cfg).unwrap();
        let json: serde_json::Value = serde_json::from_str(&content).unwrap();
        assert!(json["models"]["providers"].get("to_remove").is_none());
        assert!(json["models"]["providers"].get("keep").is_some());
        assert!(json["agents"]["defaults"]["model"].get("primary").is_none());
    }
}
