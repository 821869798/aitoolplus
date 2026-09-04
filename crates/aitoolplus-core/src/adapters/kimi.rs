//! Kimi adapter: writes `~/.kimi-code/config.toml` (providers + models
//! catalog) and `~/.kimi-code/credentials/<name>.json` (official OAuth).
//!
//! Real Kimi Code schema (from ai-toolbox `coding/kimi/AGENTS.md`):
//! - `[models].default_model` selects the active model key
//! - each model key points at a provider: `[models.<key>].provider`
//! - provider tables: `[providers.<key>]` with type/base_url/api_key
//! - Kimi CLI hard-validates `max_context_size` > 0; missing/non-positive
//!   values must fall back to 262144 (256k)
//! - official credentials live in `<root>/credentials/*.json`
//! - Common/MCP/Plugins/Skills sections preserved field-by-field

use serde_json::Value;

use crate::adapters::{
    AppliedReport, ApplyCtx, ApplyError, ToolAdapter, backup_file, merged_payload, write_atomic,
};
use crate::config::toml_set;
use crate::paths::Paths;
use crate::tools::ToolId;

pub struct KimiAdapter;

/// Conservative default context window (matches upstream kimi-for-coding).
const DEFAULT_MAX_CONTEXT_SIZE: i64 = 262144;

impl ToolAdapter for KimiAdapter {
    fn tool(&self) -> ToolId {
        ToolId::Kimi
    }

    fn apply(&self, ctx: &ApplyCtx) -> Result<AppliedReport, ApplyError> {
        let payload = merged_payload(ctx);
        let root = ctx.paths.tool_root(ToolId::Kimi);
        let cfg = root.join("config.toml");
        let mut files = vec![];

        if cfg.exists() {
            backup_file(ctx.paths, ToolId::Kimi, &cfg);
        }
        let mut doc: toml_edit::DocumentMut = if cfg.exists() {
            std::fs::read_to_string(&cfg)
                .unwrap_or_default()
                .parse()
                .unwrap_or_default()
        } else {
            Default::default()
        };
        if let Ok(common) = serde_json::from_str::<Value>(ctx.common_config) {
            crate::config::merge_json_into_toml(
                &mut doc,
                &common,
                &["models", "providers", "mcp_servers"],
            )
            .map_err(ApplyError::Message)?;
        }

        // Clean managed [models.<key>] and [providers.<key>] keys that the
        // incoming catalog no longer owns, preserving user-added locals.
        let catalog: Vec<(String, Value)> = payload
            .get("modelCatalog")
            .or_else(|| payload.get("model_catalog"))
            .and_then(Value::as_array)
            .map(|arr| {
                arr.iter()
                    .filter_map(|m| {
                        let key = m.get("key")?.as_str()?.to_string();
                        Some((key, m.clone()))
                    })
                    .collect()
            })
            .unwrap_or_default();

        let keep_keys: std::collections::HashSet<String> =
            catalog.iter().map(|(k, _)| k.clone()).collect();

        if let Some(tbl) = doc.get_mut("models").and_then(|t| t.as_table_like_mut()) {
            let existing: Vec<String> = tbl
                .iter()
                .filter(|(k, _)| *k != "default_model")
                .map(|(k, _)| k.to_string())
                .collect();
            for key in existing {
                if !keep_keys.contains(&key) {
                    tbl.remove(&key);
                }
            }
        }

        if let Some(tbl) = doc.get_mut("providers").and_then(|t| t.as_table_like_mut()) {
            let existing: Vec<String> = tbl.iter().map(|(k, _)| k.to_string()).collect();
            for key in existing {
                if !keep_keys.contains(&key) {
                    tbl.remove(&key);
                }
            }
        }

        // Project the catalog.
        for (key, model) in &catalog {
            let model_table = format!("models.{key}");
            let provider_table = format!("providers.{key}");

            // [models.<key>] links to its provider entry.
            let mut buf = doc.to_string();
            buf = toml_set(&buf, Some(&model_table), "provider", &format!("\"{key}\""));
            let max_ctx = model
                .get("maxContextSize")
                .or_else(|| model.get("max_context_size"))
                .and_then(Value::as_i64)
                .filter(|v| *v > 0)
                .unwrap_or(DEFAULT_MAX_CONTEXT_SIZE);
            buf = toml_set(
                &buf,
                Some(&model_table),
                "max_context_size",
                &max_ctx.to_string(),
            );
            if let Some(display) = model.get("displayName").and_then(Value::as_str) {
                buf = toml_set(
                    &buf,
                    Some(&model_table),
                    "displayName",
                    &format!("\"{display}\""),
                );
            }
            doc = buf.parse().unwrap_or(doc);

            // [providers.<key>] holds connection settings.
            let mut buf = doc.to_string();
            buf = toml_set(&buf, Some(&provider_table), "type", "\"openai\"");
            if let Some(base_url) = payload.get("base_url").and_then(Value::as_str) {
                buf = toml_set(
                    &buf,
                    Some(&provider_table),
                    "base_url",
                    &format!("\"{base_url}\""),
                );
            }
            if let Some(api_key) = payload.get("api_key").and_then(Value::as_str) {
                buf = toml_set(
                    &buf,
                    Some(&provider_table),
                    "api_key",
                    &format!("\"{api_key}\""),
                );
            }
            doc = buf.parse().unwrap_or(doc);
        }

        // Default model selection.
        let default_key = payload
            .get("model")
            .and_then(Value::as_str)
            .map(str::to_string)
            .or_else(|| catalog.first().map(|(k, _)| k.clone()));
        if let Some(dk) = &default_key {
            let buf = toml_set(
                &doc.to_string(),
                Some("models"),
                "default_model",
                &format!("\"{dk}\""),
            );
            doc = buf.parse().unwrap_or(doc);
        }

        write_atomic(&cfg, &doc.to_string())?;
        files.push(cfg);

        Ok(AppliedReport { files })
    }

    fn read_current(&self, paths: &Paths) -> Result<Value, ApplyError> {
        let cfg = paths.tool_root(ToolId::Kimi).join("config.toml");
        if !cfg.exists() {
            return Ok(Value::Object(Default::default()));
        }
        let raw = std::fs::read_to_string(&cfg)?;
        Ok(serde_json::json!({ "toml": raw }))
    }

    fn prompt_file(&self, paths: &Paths) -> Option<std::path::PathBuf> {
        Some(paths.tool_root(ToolId::Kimi).join("AGENTS.md"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::MergeStrategy;
    use crate::providers::ProviderRecord;
    use serde_json::json;

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
    fn root_is_kimi_code_dir() {
        let (_dir, paths) = setup();
        assert_eq!(paths.tool_root(ToolId::Kimi), paths.home.join(".kimi-code"));
    }

    #[test]
    fn apply_writes_models_providers_and_preserves_sections() {
        let (_dir, paths) = setup();
        let cfg = paths.home.join(".kimi-code").join("config.toml");
        std::fs::create_dir_all(cfg.parent().unwrap()).unwrap();
        std::fs::write(
            &cfg,
            "[mcp_servers.fs]\ncommand = \"npx\"\n\n[other]\nx = 1\n",
        )
        .unwrap();

        let mut p = ProviderRecord::new("Moonshot", "custom");
        p.set_settings(&json!({
            "base_url": "https://api.moonshot.cn/v1",
            "api_key": "mk-1",
            "model": "kimi-k2",
            "modelCatalog": [{"key": "kimi-k2", "model": "kimi-k2", "maxContextSize": 1048576}]
        }));
        KimiAdapter.apply(&ctx(&paths, &p)).unwrap();

        let toml = std::fs::read_to_string(&cfg).unwrap();
        assert!(
            toml.contains("[models.kimi-k2]"),
            "models table missing: {toml}"
        );
        assert!(toml.contains("max_context_size = 1048576"));
        assert!(toml.contains("provider = \"kimi-k2\""));
        assert!(toml.contains("[providers.kimi-k2]"));
        assert!(toml.contains("base_url = \"https://api.moonshot.cn/v1\""));
        assert!(toml.contains("api_key = \"mk-1\""));
        assert!(toml.contains("[models]"));
        assert!(toml.contains("default_model = \"kimi-k2\""));
        // unrelated sections survive
        assert!(toml.contains("[mcp_servers.fs]"));
        assert!(toml.contains("[other]"));
    }

    #[test]
    fn missing_or_nonpositive_context_falls_back() {
        let (_dir, paths) = setup();
        let mut p = ProviderRecord::new("X", "custom");
        p.set_settings(&json!({
            "base_url": "https://a/v1",
            "modelCatalog": [{"key": "m1", "model": "m1"}]
        }));
        KimiAdapter.apply(&ctx(&paths, &p)).unwrap();
        let toml =
            std::fs::read_to_string(paths.home.join(".kimi-code").join("config.toml")).unwrap();
        assert!(toml.contains("max_context_size = 262144"));

        let mut p2 = ProviderRecord::new("Y", "custom");
        p2.set_settings(&json!({
            "base_url": "https://b/v1",
            "modelCatalog": [{"key": "m2", "maxContextSize": 0}]
        }));
        KimiAdapter.apply(&ctx(&paths, &p2)).unwrap();
        let toml =
            std::fs::read_to_string(paths.home.join(".kimi-code").join("config.toml")).unwrap();
        assert!(!toml.contains("[models.m1]"));
        assert!(toml.contains("max_context_size = 262144"));
    }
}
