//! Codex adapter: writes `~/.codex/config.toml` + `~/.codex/auth.json`.
//!
//! Ported from ai-toolbox `coding/codex/commands.rs` semantics:
//! - provider config is authored as TOML text (e.g. `model_provider = "x"`
//!   + `[model_providers.x]` tables); common config is a second TOML layer
//! - merge: provider overlays common (provider scalars/tables win)
//! - config.toml write: strip protected top-level keys from the managed
//!   layer, remove previously-managed fields that are gone, then merge the
//!   new layer over the existing disk document, and stamp `#:schema none`
//! - auth.json: only `OPENAI_API_KEY` + `auth_mode` are managed; runtime
//!   OAuth `tokens` are always preserved; empty managed key removes ours
//!   and marks `chatgpt` mode when tokens remain

use serde_json::{Map, Value};

use crate::adapters::{
    AppliedReport, ApplyCtx, ApplyError, ToolAdapter, backup_file, write_atomic,
};
use crate::paths::Paths;
use crate::tools::ToolId;

pub struct CodexAdapter;

/// Top-level config.toml keys the managed layer never overwrites.
const PROTECTED_TOP_LEVEL_TOML_KEYS: &[&str] = &["mcp_servers", "projects"];

fn parse_toml_document(raw: &str, label: &str) -> Result<toml_edit::DocumentMut, String> {
    if raw.trim().is_empty() {
        return Ok(toml_edit::DocumentMut::new());
    }
    raw.parse::<toml_edit::DocumentMut>()
        .map_err(|e| format!("Failed to parse {label}: {e}"))
}

/// Merge `overlay` into `base`: tables recurse, everything else replaces.
fn merge_toml_tables(base: &mut toml_edit::Table, overlay: &toml_edit::Table) {
    for (key, overlay_item) in overlay.iter() {
        if let Some(base_item) = base.get_mut(key) {
            merge_toml_items(base_item, overlay_item);
        } else {
            base.insert(key, overlay_item.clone());
        }
    }
}

fn merge_toml_items(base: &mut toml_edit::Item, overlay: &toml_edit::Item) {
    match (base, overlay) {
        (toml_edit::Item::Table(base_table), toml_edit::Item::Table(overlay_table)) => {
            merge_toml_tables(base_table, overlay_table);
        }
        (base_item, overlay_item) => {
            *base_item = overlay_item.clone();
        }
    }
}

/// Remove previously-managed fields from `current` (recursively for tables).
#[allow(dead_code)] // upstream parity helper
fn remove_managed_toml_fields(
    current: &mut toml_edit::Table,
    previous: &toml_edit::Table,
    preserve_protected: bool,
) {
    let mut to_remove = vec![];
    for (key, previous_item) in previous.iter() {
        let key_name = key.to_string();
        if preserve_protected && PROTECTED_TOP_LEVEL_TOML_KEYS.contains(&key_name.as_str()) {
            continue;
        }
        let should_remove = if let Some(current_item) = current.get_mut(key) {
            match previous_item {
                toml_edit::Item::Table(prev_child) => {
                    if let Some(cur_child) = current_item.as_table_mut() {
                        remove_managed_toml_fields(cur_child, prev_child, false);
                        cur_child.is_empty()
                    } else {
                        true
                    }
                }
                _ => true,
            }
        } else {
            false
        };
        if should_remove {
            to_remove.push(key.to_string());
        }
    }
    for key in to_remove {
        current.remove(&key);
    }
}

fn strip_protected_top_level(doc: &mut toml_edit::DocumentMut) {
    for key in PROTECTED_TOP_LEVEL_TOML_KEYS {
        doc.as_table_mut().remove(key);
    }
}

/// Provider TOML text (from the record) + common TOML text -> managed TOML.
pub fn append_toml_configs(provider: &str, common: &str) -> Result<String, String> {
    let provider_content = provider.trim();
    let common_content = common.trim();

    if provider_content.is_empty() {
        return Ok(common_content.to_string());
    }
    if common_content.is_empty() {
        return Ok(provider_content.to_string());
    }

    let provider_doc = parse_toml_document(provider_content, "provider config")?;
    let mut common_doc = parse_toml_document(common_content, "common config")?;
    merge_toml_tables(common_doc.as_table_mut(), provider_doc.as_table());
    Ok(common_doc.to_string())
}

/// auth.json merge: only OPENAI_API_KEY/auth_mode are ours.
pub fn merge_codex_auth_json(existing: &Value, managed: &Value) -> Value {
    merge_codex_auth_json_with_policy(existing, managed, true)
}

pub fn merge_codex_auth_json_with_policy(
    existing: &Value,
    managed: &Value,
    preserve_official_tokens: bool,
) -> Value {
    let mut merged = existing.as_object().cloned().unwrap_or_default();
    if !preserve_official_tokens {
        merged.remove("tokens");
    }
    let api_key = managed.get("OPENAI_API_KEY").and_then(Value::as_str);
    match api_key {
        Some(key) if !key.is_empty() => {
            merged.insert("OPENAI_API_KEY".into(), Value::String(key.into()));
            merged.insert("auth_mode".into(), Value::String("apikey".into()));
        }
        _ => {
            merged.remove("OPENAI_API_KEY");
            if merged
                .get("tokens")
                .and_then(Value::as_object)
                .is_some_and(|t| !t.is_empty())
            {
                merged.insert("auth_mode".into(), Value::String("chatgpt".into()));
            }
        }
    }
    Value::Object(merged)
}

/// Build the final config.toml: existing disk minus previously-managed
/// fields, plus the new managed layer, with the schema header stamped.
pub fn build_written_config_toml(
    existing: &str,
    previous_managed: Option<&str>,
    next_managed: &str,
    sweep_stale_providers: bool,
) -> Result<String, String> {
    let mut current = parse_toml_document(existing, "existing config.toml")?;
    let mut next = parse_toml_document(next_managed, "new config.toml")?;
    strip_protected_top_level(&mut next);

    // Merge FIRST (overwrites in place, no remove-then-insert), then sweep
    // keys the new layer does not declare. This ordering avoids the
    // toml_edit duplicate-render quirk for root scalars that sit after a
    // table header in the original document.
    merge_toml_tables(current.as_table_mut(), next.as_table());

    if let Some(prev_text) = previous_managed {
        let mut prev = parse_toml_document(prev_text, "previous config.toml")?;
        strip_protected_top_level(&mut prev);
        // Only remove fields the NEXT layer no longer declares.
        let mut keep = parse_toml_document(next_managed, "next config.toml")?;
        strip_protected_top_level(&mut keep);
        remove_fields_absent_from(current.as_table_mut(), prev.as_table(), keep.as_table());
    }

    if sweep_stale_providers {
        sweep_stale_model_providers(&mut current, next.as_table());
    }

    let content = current.to_string();
    if content.trim_start().starts_with("#:schema") {
        Ok(content)
    } else {
        Ok(format!("#:schema none\n{content}"))
    }
}

/// Remove `previous` fields that `keep` no longer declares (previous keys
/// that survived into the next layer belong to the new provider).
fn remove_fields_absent_from(
    current: &mut toml_edit::Table,
    previous: &toml_edit::Table,
    keep: &toml_edit::Table,
) {
    let mut to_remove = vec![];
    for (key, previous_item) in previous.iter() {
        if keep.contains_key(key) {
            continue;
        }
        let should_remove = if let Some(current_item) = current.get_mut(key) {
            match previous_item {
                toml_edit::Item::Table(prev_child) => {
                    if let Some(cur_child) = current_item.as_table_mut() {
                        let keep_child = keep.get(key).and_then(|i| i.as_table());
                        match keep_child {
                            Some(kc) => {
                                remove_fields_absent_from(cur_child, prev_child, kc);
                                cur_child.is_empty()
                            }
                            None => true,
                        }
                    } else {
                        true
                    }
                }
                _ => true,
            }
        } else {
            false
        };
        if should_remove {
            to_remove.push(key.to_string());
        }
    }
    for key in to_remove {
        current.remove(&key);
    }
}

/// Remove `[model_providers.*]` tables and root `model_provider` from
/// `current` when the incoming layer doesn't declare them.
fn sweep_stale_model_providers(current: &mut toml_edit::DocumentMut, next: &toml_edit::Table) {
    let next_providers: std::collections::HashSet<String> = next
        .get("model_providers")
        .and_then(|t| t.as_table())
        .map(|t| t.iter().map(|(k, _)| k.to_string()).collect())
        .unwrap_or_default();

    // The root selector is overwritten by the merge when the new layer
    // declares it; only remove it when the new layer has no selector
    // (avoids remove+insert duplicate renders on docs where the key sits
    // after a table header — a toml_edit position quirk).
    if !next.contains_key("model_provider") {
        current.as_table_mut().remove("model_provider");
    }

    // Drop provider tables the new layer doesn't declare.
    let stale: Vec<String> = current
        .get("model_providers")
        .and_then(|t| t.as_table())
        .map(|t| {
            t.iter()
                .map(|(k, _)| k.to_string())
                .filter(|k| !next_providers.contains(k))
                .collect()
        })
        .unwrap_or_default();
    if let Some(existing) = current
        .get_mut("model_providers")
        .and_then(|t| t.as_table_mut())
    {
        for k in &stale {
            existing.remove(k);
        }
        if existing.is_empty() {
            current.as_table_mut().remove("model_providers");
        }
    }
}

/// The provider record's settings_config for Codex is TOML text or JSON containing config/toml.
fn provider_toml_text(record_settings: &str) -> String {
    // UI/presets store Codex config as {"config":"..."} or {"toml":"..."};
    // imported legacy records may still contain raw TOML text.
    let trimmed = record_settings.trim();
    if trimmed.starts_with('{') {
        serde_json::from_str::<Value>(trimmed)
            .ok()
            .and_then(|value| {
                value
                    .get("config")
                    .or_else(|| value.get("toml"))
                    .and_then(Value::as_str)
                    .map(String::from)
            })
            .unwrap_or_default()
    } else {
        trimmed.to_string()
    }
}

/// Extract the managed api key from JSON auth or TOML text.
fn extract_provider_api_key(record_settings: &str, provider_toml: &str) -> Option<String> {
    let trimmed = record_settings.trim();
    if trimmed.starts_with('{') {
        if let Ok(val) = serde_json::from_str::<Value>(trimmed) {
            if let Some(auth) = val.get("auth").and_then(Value::as_object) {
                if let Some(k) = auth
                    .get("OPENAI_API_KEY")
                    .or_else(|| auth.get("api_key"))
                    .or_else(|| auth.get("token"))
                    .and_then(Value::as_str)
                {
                    if !k.trim().is_empty() {
                        return Some(k.trim().to_string());
                    }
                }
            }
        }
    }
    let doc = parse_toml_document(provider_toml, "provider config").ok()?;
    if let Some(key) = doc.get("model_providers").and_then(|t| t.as_table()) {
        for (_, item) in key.iter() {
            if let Some(tbl) = item.as_table() {
                if let Some(k) = tbl
                    .get("experimental_bearer_token")
                    .or_else(|| tbl.get("api_key"))
                    .and_then(|v| v.as_str())
                {
                    let kt = k.trim();
                    if !kt.is_empty() {
                        return Some(kt.to_string());
                    }
                }
            }
        }
    }
    if let Some(k) = doc
        .get("experimental_bearer_token")
        .and_then(|v| v.as_str())
    {
        let kt = k.trim();
        if !kt.is_empty() {
            return Some(kt.to_string());
        }
    }
    None
}

impl ToolAdapter for CodexAdapter {
    fn tool(&self) -> ToolId {
        ToolId::Codex
    }

    fn apply(&self, ctx: &ApplyCtx) -> Result<AppliedReport, ApplyError> {
        let root = ctx.paths.tool_root(ToolId::Codex);
        let cfg = root.join("config.toml");
        let auth = root.join("auth.json");
        let mut files = vec![];

        let provider_toml = provider_toml_text(&ctx.provider.settings_config);
        let common_toml = ctx.common_config.trim().to_string();

        // Managed layer for this provider.
        let next_managed =
            append_toml_configs(&provider_toml, &common_toml).map_err(ApplyError::Message)?;

        // Existing disk state.
        let existing_cfg = if cfg.exists() {
            std::fs::read_to_string(&cfg).unwrap_or_default()
        } else {
            String::new()
        };
        if cfg.exists() {
            backup_file(ctx.paths, ToolId::Codex, &cfg);
        }

        let _ = ctx.strategy; // Codex uses its own TOML-layer merge.

        // Handle model catalog if present in provider settings
        let mut catalog_file_opt = None;
        if let Ok(val) = serde_json::from_str::<Value>(&ctx.provider.settings_config) {
            if let Some(models) = val
                .get("modelCatalog")
                .and_then(|mc| mc.get("models"))
                .and_then(Value::as_array)
            {
                if !models.is_empty() {
                    let cat_path = root.join("cc-switch-model-catalog.json");
                    let cat_val = serde_json::json!({ "models": models });
                    let cat_text = serde_json::to_string_pretty(&cat_val).unwrap_or_default();
                    write_atomic(&cat_path, &cat_text)?;
                    files.push(cat_path.clone());
                    catalog_file_opt = Some(cat_path);
                }
            }
        }

        // Remove the previous provider's footprint: any root `model_provider`
        // + `[model_providers.*]` tables the new layer does not declare.
        let mut final_toml =
            build_written_config_toml(&existing_cfg, Some(&provider_toml), &next_managed, true)
                .map_err(ApplyError::Message)?;

        if let Some(cat_path) = catalog_file_opt {
            let cat_str = cat_path.to_string_lossy().replace('\\', "/");
            if !final_toml.contains("model_catalog_json") {
                final_toml = format!("{final_toml}\nmodel_catalog_json = \"{cat_str}\"\n");
            }
        }

        write_atomic(&cfg, &final_toml)?;
        files.push(cfg);

        // auth.json: replace only managed fields, keep OAuth runtime data.
        if let Some(api_key) = extract_provider_api_key(&ctx.provider.settings_config, &provider_toml) {
            let existing_auth: Value = if auth.exists() {
                serde_json::from_str(&std::fs::read_to_string(&auth).unwrap_or_default())
                    .unwrap_or(Value::Object(Map::new()))
            } else {
                Value::Object(Map::new())
            };
            let managed = serde_json::json!({ "OPENAI_API_KEY": api_key });
            let preserve = std::env::var("AITOOLPLUS_CODEX_PRESERVE_AUTH")
                .ok()
                .map(|value| value == "1")
                .unwrap_or(true);
            let merged = merge_codex_auth_json_with_policy(&existing_auth, &managed, preserve);
            if auth.exists() {
                backup_file(ctx.paths, ToolId::Codex, &auth);
            }
            write_atomic(&auth, &serde_json::to_string_pretty(&merged).unwrap())?;
            files.push(auth);
        }

        Ok(AppliedReport { files })
    }

    fn read_current(&self, paths: &Paths) -> Result<Value, ApplyError> {
        let path = paths.primary_config(ToolId::Codex);
        if !path.exists() {
            return Ok(Value::Object(Map::new()));
        }
        let raw = std::fs::read_to_string(&path)?;
        Ok(serde_json::json!({ "toml": raw }))
    }

    fn prompt_file(&self, paths: &Paths) -> Option<std::path::PathBuf> {
        Some(paths.tool_root(ToolId::Codex).join("AGENTS.md"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::MergeStrategy;
    use crate::providers::ProviderRecord;

    fn setup() -> (tempfile::TempDir, Paths) {
        let dir = tempfile::tempdir().unwrap();
        let paths = Paths::new(dir.path().join("home"), dir.path().join("data"));
        (dir, paths)
    }

    fn ctx<'a>(paths: &'a Paths, p: &'a ProviderRecord) -> ApplyCtx<'a> {
        ApplyCtx {
            paths,
            common_config: "",
            provider: p,
            strategy: MergeStrategy::default(),
            provider_optional: false,
        }
    }

    #[test]
    fn provider_overlays_common() {
        let provider = "model = \"glm-5.2\"\nmodel_reasoning_effort = \"high\"\n";
        let common = "approval_policy = \"never\"\nmodel = \"gpt-5.4\"\n";
        let merged = append_toml_configs(provider, common).unwrap();
        let doc = parse_toml_document(&merged, "merged").unwrap();
        assert_eq!(doc["model"].as_str(), Some("glm-5.2"));
        assert_eq!(doc["model_reasoning_effort"].as_str(), Some("high"));
        assert_eq!(doc["approval_policy"].as_str(), Some("never"));
    }

    #[test]
    fn provider_tables_merge_into_common_tables() {
        let provider = "[model_providers.custom]\nname = \"custom\"\n";
        let common = "[model_providers.custom]\nwire_api = \"responses\"\n";
        let merged = append_toml_configs(provider, common).unwrap();
        let doc = parse_toml_document(&merged, "merged").unwrap();
        assert_eq!(
            doc["model_providers"]["custom"]["name"].as_str(),
            Some("custom")
        );
        assert_eq!(
            doc["model_providers"]["custom"]["wire_api"].as_str(),
            Some("responses")
        );
    }

    #[test]
    fn empty_layers_pass_through() {
        assert_eq!(append_toml_configs("", "a = 1").unwrap(), "a = 1");
        assert_eq!(append_toml_configs("b = 2", "").unwrap(), "b = 2");
    }

    #[test]
    fn write_preserves_mcp_and_oauth_and_stamps_schema() {
        let (_dir, paths) = setup();
        let cfg = paths.home.join(".codex").join("config.toml");
        let auth = paths.home.join(".codex").join("auth.json");
        std::fs::create_dir_all(cfg.parent().unwrap()).unwrap();
        // Realistic layout: root scalars before any table header (in TOML,
        // keys after [mcp_servers.fs] belong to that table, not the root).
        std::fs::write(&cfg, "model_provider = \"old\"\n\n[mcp_servers.fs]\ncommand = \"npx\"\n\n[model_providers.old]\nname = \"old\"\n").unwrap();
        std::fs::write(
            &auth,
            r#"{"OPENAI_API_KEY": "old-key", "tokens": {"access_token": "t"}}"#,
        )
        .unwrap();

        let mut p = ProviderRecord::new("My Provider", "custom");
        p.settings_config = "model_provider = \"my-provider\"\n\n[model_providers.my-provider]\nname = \"my-provider\"\nbase_url = \"https://api.example.com/v1\"\nwire_api = \"responses\"\napi_key = \"sk-1\"\n".into();

        let report = CodexAdapter.apply(&ctx(&paths, &p)).unwrap();
        assert_eq!(report.files.len(), 2);

        let toml = std::fs::read_to_string(&cfg).unwrap();
        assert!(toml.starts_with("#:schema none"), "schema header missing");
        assert!(toml.contains("[mcp_servers.fs]"), "mcp lost");
        assert!(toml.contains("model_provider = \"my-provider\""));
        assert!(toml.contains("[model_providers.my-provider]"));
        assert!(toml.contains("base_url = \"https://api.example.com/v1\""));
        assert!(
            !toml.contains("[model_providers.old]"),
            "previous provider table kept"
        );
        assert!(
            !toml.contains("model_provider = \"old\""),
            "dangling model_provider kept"
        );

        let auth_v: Value = serde_json::from_str(&std::fs::read_to_string(&auth).unwrap()).unwrap();
        assert_eq!(auth_v["OPENAI_API_KEY"], "sk-1");
        assert_eq!(auth_v["auth_mode"], "apikey");
        assert_eq!(
            auth_v["tokens"]["access_token"], "t",
            "oauth runtime data lost"
        );
    }

    #[test]
    fn auth_preservation_policy() {
        let existing = serde_json::json!({
            "tokens": {"access_token": "t"},
            "other": true
        });
        let managed = serde_json::json!({"OPENAI_API_KEY": "k"});
        let preserved = merge_codex_auth_json_with_policy(&existing, &managed, true);
        assert_eq!(preserved["tokens"]["access_token"], "t");
        let removed = merge_codex_auth_json_with_policy(&existing, &managed, false);
        assert!(removed.get("tokens").is_none());
        assert_eq!(removed["other"], true);
    }

    #[test]
    fn empty_api_key_removes_managed_and_marks_chatgpt() {
        let existing = serde_json::json!({
            "OPENAI_API_KEY": "old",
            "tokens": {"id_token": "x"},
        });
        let merged = merge_codex_auth_json(&existing, &serde_json::json!({}));
        assert!(merged.get("OPENAI_API_KEY").is_none());
        assert_eq!(merged["auth_mode"], "chatgpt");
        assert_eq!(merged["tokens"]["id_token"], "x");
    }

    #[test]
    fn empty_provider_keeps_common_only() {
        let (_dir, paths) = setup();
        let cfg = paths.home.join(".codex").join("config.toml");
        std::fs::create_dir_all(cfg.parent().unwrap()).unwrap();

        let mut p = ProviderRecord::new("P", "custom");
        p.settings_config = "{}".into(); // JSON-shaped -> treated as empty TOML
        let mut c = ctx(&paths, &p);
        c.common_config = "approval_policy = \"never\"";
        let report = CodexAdapter.apply(&c).unwrap();
        let toml = std::fs::read_to_string(&report.files[0]).unwrap();
        assert!(toml.contains("approval_policy = \"never\""));
    }
}
