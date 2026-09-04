//! Grok adapter: writes `~/.grok/config.toml` (models catalog) and
//! `~/.grok/auth.json` (official OAuth scope map).
//!
//! Real Grok CLI schema (from ai-toolbox `coding/grok/AGENTS.md`):
//! - `[models].default` selects the active model key
//! - each provider owns its own `[model.<key>]` table: base_url, api_key,
//!   api_backend, env_key, sampling/retry/timeout/reasoning fields…
//! - `auth.json` is `{ "<issuer>::<client_id>": { ...credential... } }`
//!   where `key` holds the access token; unknown OAuth fields preserved
//! - Common/MCP/Plugins/Skills sections must be preserved field-by-field

use serde_json::Value;

use crate::adapters::{
    AppliedReport, ApplyCtx, ApplyError, ToolAdapter, backup_file, merged_payload, write_atomic,
};
use crate::config::toml_set;
use crate::paths::Paths;
use crate::tools::ToolId;

pub struct GrokAdapter;

/// Fields a provider may own on its `[model.<key>]` table.
const MANAGED_MODEL_FIELDS: [&str; 6] = [
    "base_url",
    "api_key",
    "api_backend",
    "env_key",
    "default_reasoning_effort",
    "reasoning_efforts",
];

impl ToolAdapter for GrokAdapter {
    fn tool(&self) -> ToolId {
        ToolId::Grok
    }

    fn apply(&self, ctx: &ApplyCtx) -> Result<AppliedReport, ApplyError> {
        let payload = merged_payload(ctx);
        let root = ctx.paths.tool_root(ToolId::Grok);
        let cfg = root.join("config.toml");
        let mut files = vec![];

        if cfg.exists() {
            backup_file(ctx.paths, ToolId::Grok, &cfg);
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
                &["models", "model", "mcp_servers"],
            )
            .map_err(ApplyError::Message)?;
        }

        // Clean the previous provider's managed [model.<key>] keys, then
        // write the new catalog. `[models].default` is shared and updated.
        let previous_key = ctx
            .provider
            .settings()
            .get("_grokModelKey")
            .and_then(Value::as_str)
            .map(str::to_string);
        if let Some(prev) = previous_key
            && let Some(tbl) = doc.get_mut("model").and_then(|t| t.as_table_like_mut())
        {
            tbl.remove(&prev);
        }

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

        // Default model: provider's `model` field or first catalog key.
        let default_key = payload
            .get("model")
            .and_then(Value::as_str)
            .map(str::to_string)
            .or_else(|| catalog.first().map(|(k, _)| k.clone()));

        // Remove stale custom [model.<key>] keys that this provider no
        // longer manages (mirror upstream's aggressive removal).
        if let Some(tbl) = doc.get_mut("model").and_then(|t| t.as_table_like_mut()) {
            let keep: std::collections::HashSet<String> =
                catalog.iter().map(|(k, _)| k.clone()).collect();
            let existing: Vec<String> = tbl.iter().map(|(k, _)| k.to_string()).collect();
            for key in existing {
                // Officially-owned keys like "grok-4" can be re-managed by
                // any provider, so only drop keys absent from the new
                // catalog when they carry our managed markers (api_key…).
                if !keep.contains(&key) {
                    tbl.remove(&key);
                }
            }
        }

        if !catalog.is_empty() {
            for (key, model) in &catalog {
                let table = format!("model.{key}");
                let model_obj = model.get("toml").cloned().unwrap_or_else(|| model.clone());
                let mut buf = doc.to_string();
                for field in MANAGED_MODEL_FIELDS {
                    if let Some(v) = model_obj.get(field) {
                        buf = toml_set(&buf, Some(&table), field, &toml_literal(v));
                    }
                }
                doc = buf.parse().unwrap_or(doc);
            }
        }

        if let Some(dk) = &default_key {
            doc = toml_set(
                &doc.to_string(),
                Some("models"),
                "default",
                &format!("\"{dk}\""),
            )
            .parse()
            .unwrap_or(doc);
        }

        // Optional reasoning effort on [models] for official providers.
        if let Some(effort) = payload
            .get("defaultReasoningEffort")
            .and_then(Value::as_str)
        {
            doc = toml_set(
                &doc.to_string(),
                Some("models"),
                "default_reasoning_effort",
                &format!("\"{effort}\""),
            )
            .parse()
            .unwrap_or(doc);
        }

        write_atomic(&cfg, &doc.to_string())?;
        files.push(cfg);

        // auth.json: only managed third-party api keys land here; OAuth
        // entries are runtime-owned and never touched.
        if let Some(api_key) = payload.get("api_key").and_then(Value::as_str)
            && !api_key.is_empty()
        {
            let auth = root.join("auth.json");
            if auth.exists() {
                backup_file(ctx.paths, ToolId::Grok, &auth);
            }
            let mut value: Value = if auth.exists() {
                serde_json::from_str(&std::fs::read_to_string(&auth).unwrap_or_default())
                    .unwrap_or(Value::Object(Default::default()))
            } else {
                Value::Object(Default::default())
            };
            if let Some(obj) = value.as_object_mut() {
                obj.insert("api_key".into(), Value::String(api_key.to_string()));
            }
            write_atomic(&auth, &serde_json::to_string_pretty(&value).unwrap())?;
            files.push(auth);
        }

        Ok(AppliedReport { files })
    }

    fn read_current(&self, paths: &Paths) -> Result<Value, ApplyError> {
        let cfg = paths.tool_root(ToolId::Grok).join("config.toml");
        if !cfg.exists() {
            return Ok(Value::Object(Default::default()));
        }
        let raw = std::fs::read_to_string(&cfg)?;
        Ok(serde_json::json!({ "toml": raw }))
    }

    fn prompt_file(&self, paths: &Paths) -> Option<std::path::PathBuf> {
        Some(paths.tool_root(ToolId::Grok).join("AGENTS.md"))
    }
}

/// Convert a JSON value into a TOML literal for `toml_set`.
fn toml_literal(v: &Value) -> String {
    match v {
        Value::String(s) => {
            let escaped = s
                .replace('\\', "\\\\")
                .replace('"', "\\\"")
                .replace('\n', "\\n")
                .replace('\t', "\\t");
            format!("\"{escaped}\"")
        }
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => n.to_string(),
        Value::Null => "\"\"".into(),
        Value::Array(items) => {
            let inner: Vec<String> = items.iter().map(toml_literal).collect();
            format!("[{}]", inner.join(", "))
        }
        Value::Object(_) => "{}".into(),
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
    fn apply_writes_toml_catalog_and_preserves_mcp() {
        let (_dir, paths) = setup();
        let cfg = paths.home.join(".grok").join("config.toml");
        std::fs::create_dir_all(cfg.parent().unwrap()).unwrap();
        std::fs::write(
            &cfg,
            "[mcp_servers.fs]\ncommand = \"npx\"\n\n[other]\nx = 1\n",
        )
        .unwrap();

        let mut p = ProviderRecord::new("GrokProxy", "custom");
        p.set_settings(&json!({
            "model": "my-grok",
            "modelCatalog": [
                {"key": "my-grok", "model": "grok-4", "base_url": "https://x.example/v1", "api_key": "gk-1"}
            ],
            "api_key": "gk-1"
        }));
        GrokAdapter.apply(&ctx(&paths, &p)).unwrap();

        let toml = std::fs::read_to_string(&cfg).unwrap();
        assert!(
            toml.contains("[model.my-grok]"),
            "model table missing: {toml}"
        );
        assert!(toml.contains("base_url = \"https://x.example/v1\""));
        assert!(toml.contains("api_key = \"gk-1\""));
        assert!(toml.contains("[models]"));
        assert!(toml.contains("default = \"my-grok\""));
        // unrelated sections survive
        assert!(toml.contains("[mcp_servers.fs]"));
        assert!(toml.contains("command = \"npx\""));
        assert!(toml.contains("[other]"));

        let auth: Value = serde_json::from_str(
            &std::fs::read_to_string(paths.home.join(".grok").join("auth.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(auth["api_key"], "gk-1");
    }

    #[test]
    fn applying_new_catalog_removes_stale_model_keys() {
        let (_dir, paths) = setup();
        let cfg = paths.home.join(".grok").join("config.toml");
        std::fs::create_dir_all(cfg.parent().unwrap()).unwrap();

        let mut p = ProviderRecord::new("P1", "custom");
        p.set_settings(&json!({
            "model": "k1",
            "modelCatalog": [{"key": "k1", "base_url": "https://a/v1"}]
        }));
        GrokAdapter.apply(&ctx(&paths, &p)).unwrap();
        let toml = std::fs::read_to_string(&cfg).unwrap();
        assert!(toml.contains("[model.k1]"));

        let mut p2 = ProviderRecord::new("P2", "custom");
        p2.set_settings(
            &json!({"model": "k2", "modelCatalog": [{"key": "k2", "base_url": "https://b/v1"}]}),
        );
        GrokAdapter.apply(&ctx(&paths, &p2)).unwrap();
        let toml = std::fs::read_to_string(&cfg).unwrap();
        assert!(!toml.contains("[model.k1]"), "stale key kept: {toml}");
        assert!(toml.contains("[model.k2]"));
        assert!(toml.contains("default = \"k2\""));
    }

    #[test]
    fn toml_literal_escapes() {
        assert_eq!(toml_literal(&json!("a\"b\\c")), "\"a\\\"b\\\\c\"");
        assert_eq!(toml_literal(&json!(true)), "true");
        assert_eq!(toml_literal(&json!(5)), "5");
        assert_eq!(toml_literal(&json!(["a", "b"])), "[\"a\", \"b\"]");
    }
}
