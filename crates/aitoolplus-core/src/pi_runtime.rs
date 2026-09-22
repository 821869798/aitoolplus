//! Pi runtime provider discovery and persistence.
//!
//! Pi's provider truth is split between three runtime files:
//! - `auth.json`: credentials keyed by provider key
//! - `models.json`: custom/built-in overrides under `providers`
//! - `settings.json`: defaultProvider/defaultModel/defaultThinkingLevel
//!
//! This module builds the same merged runtime view as ai-toolbox and keeps
//! unknown JSON fields intact when writing.

use std::path::{Path, PathBuf};

use serde_json::{Map, Value};

use crate::paths::Paths;
use crate::providers::ProviderRecord;
use crate::store::save_json_atomic;
use crate::tools::ToolId;

#[derive(Debug, Clone)]
pub struct PiRuntimePaths {
    pub root: PathBuf,
    pub settings: PathBuf,
    pub auth: PathBuf,
    pub models: PathBuf,
}

impl PiRuntimePaths {
    pub fn from_paths(paths: &Paths) -> Self {
        let root = paths.tool_root(ToolId::Pi);
        Self {
            settings: root.join("settings.json"),
            auth: root.join("auth.json"),
            models: root.join("models.json"),
            root,
        }
    }
}

/// Read a JSON object, treating a missing file as an empty object.
pub fn read_object(path: &Path) -> Result<Value, String> {
    if !path.exists() {
        return Ok(Value::Object(Map::new()));
    }
    let raw = std::fs::read_to_string(path).map_err(|e| format!("read {}: {e}", path.display()))?;
    let value: Value =
        serde_json::from_str(&raw).map_err(|e| format!("parse {}: {e}", path.display()))?;
    if !value.is_object() {
        return Err(format!("{} must contain a JSON object", path.display()));
    }
    Ok(value)
}

/// Import every runtime provider visible to Pi into the local UI store.
///
/// Unlike the earlier implementation, this includes auth-only keys, model
/// only keys, the configured default provider, and preserves the complete
/// models provider object. Built-in-only providers are included when the
/// default key names one; the built-in catalog can be expanded later without
/// changing this merge algorithm.
pub fn import_runtime(paths: &Paths, providers: &mut Vec<ProviderRecord>) -> Result<usize, String> {
    let runtime = PiRuntimePaths::from_paths(paths);
    let settings = read_object(&runtime.settings)?;
    let auth = read_object(&runtime.auth)?;
    let models = read_object(&runtime.models)?;

    let auth_map = auth.as_object().cloned().unwrap_or_default();
    let model_map = models
        .get("providers")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();

    let mut keys = std::collections::BTreeSet::new();
    keys.extend(auth_map.keys().cloned());
    keys.extend(model_map.keys().cloned());
    if let Some(default) = settings.get("defaultProvider").and_then(Value::as_str)
        && !default.trim().is_empty()
    {
        keys.insert(default.to_string());
    }

    let mut added = 0;
    let mut next = providers.iter().map(|p| p.sort_index).max().unwrap_or(-1);
    for key in keys {
        let model = model_map.get(&key);
        let credential = auth_map.get(&key);
        let name = model
            .and_then(|v| v.get("name"))
            .and_then(Value::as_str)
            .filter(|s| !s.trim().is_empty())
            .unwrap_or(&key)
            .to_string();

        let mut merged = model.cloned().unwrap_or_else(|| Value::Object(Map::new()));
        if let Some(obj) = merged.as_object_mut() {
            if let Some(credential) = credential {
                // Keep credential data visible for the editor, matching the
                // runtime view. The writer splits it back into auth/models.
                obj.insert("_auth".into(), credential.clone());
            }
            obj.insert("_providerKey".into(), Value::String(key.clone()));
        }

        let is_enabled = model_map.contains_key(&key);
        let stable_id = format!("pi:{key}");
        let existing = providers.iter_mut().find(|p| p.id == stable_id);
        if let Some(record) = existing {
            record.name = name;
            record.settings_config =
                serde_json::to_string_pretty(&merged).unwrap_or_else(|_| "{}".into());
            record.is_applied = is_enabled;
            record.touch();
            continue;
        }

        next += 1;
        let mut record = ProviderRecord::new(name, pi_category(model, credential));
        record.id = stable_id;
        record.sort_index = next;
        record.is_applied = is_enabled;
        record.settings_config =
            serde_json::to_string_pretty(&merged).unwrap_or_else(|_| "{}".into());
        record.notes = Some(format!("Pi runtime provider: {key}"));
        providers.push(record);
        added += 1;
    }
    Ok(added)
}

fn pi_category(model: Option<&Value>, credential: Option<&Value>) -> String {
    if credential
        .and_then(|v| v.get("type"))
        .and_then(Value::as_str)
        == Some("oauth")
    {
        return "subscription".into();
    }
    if credential.is_some() || model.and_then(|v| v.get("apiKey")).is_some() {
        return "custom".into();
    }
    "other".into()
}

/// Enable or disable a provider record in Pi's runtime models.json / auth.json.
/// In Pi, multiple providers can be enabled simultaneously (present in models.json).
pub fn set_provider_enabled(
    paths: &Paths,
    provider: &ProviderRecord,
    enabled: bool,
) -> Result<Vec<PathBuf>, String> {
    let runtime = PiRuntimePaths::from_paths(paths);
    let key = provider
        .id
        .strip_prefix("pi:")
        .unwrap_or(&provider.id);
    let incoming: Value = serde_json::from_str(&provider.settings_config)
        .map_err(|e| format!("invalid Pi provider JSON: {e}"))?;

    let mut models = read_object(&runtime.models)?;
    let model_root = models
        .as_object_mut()
        .ok_or_else(|| "Pi models.json must be an object".to_string())?;
    let providers = model_root
        .entry("providers")
        .or_insert_with(|| Value::Object(Map::new()))
        .as_object_mut()
        .ok_or_else(|| "Pi models.json providers must be an object".to_string())?;

    let mut files = vec![runtime.models.clone()];

    if enabled {
        let mut model_provider = incoming.clone();
        if let Some(obj) = model_provider.as_object_mut() {
            obj.remove("_auth");
            obj.remove("_providerKey");
        }
        providers.insert(key.to_string(), model_provider);

        if let Some(credential) = incoming.get("_auth") {
            let mut auth = read_object(&runtime.auth)?;
            auth.as_object_mut()
                .ok_or_else(|| "Pi auth.json must be an object".to_string())?
                .insert(key.to_string(), credential.clone());
            save_json_atomic(&runtime.auth, &auth).map_err(|e| e.to_string())?;
            files.push(runtime.auth.clone());
        }
    } else {
        providers.remove(key);
        let mut auth = read_object(&runtime.auth)?;
        if let Some(auth_obj) = auth.as_object_mut() {
            if auth_obj.remove(key).is_some() {
                save_json_atomic(&runtime.auth, &auth).map_err(|e| e.to_string())?;
                files.push(runtime.auth.clone());
            }
        }
        let mut settings = read_object(&runtime.settings)?;
        if let Some(obj) = settings.as_object_mut() {
            if obj.get("defaultProvider").and_then(Value::as_str) == Some(key) {
                obj.remove("defaultProvider");
                obj.remove("defaultModel");
                save_json_atomic(&runtime.settings, &settings).map_err(|e| e.to_string())?;
                files.push(runtime.settings.clone());
            }
        }
    }

    save_json_atomic(&runtime.models, &models).map_err(|e| e.to_string())?;
    Ok(files)
}

/// Apply a provider record back to Pi's runtime files, preserving all
/// unknown top-level and sibling provider fields.
pub fn apply_provider(paths: &Paths, provider: &ProviderRecord) -> Result<Vec<PathBuf>, String> {
    set_provider_enabled(paths, provider, true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::ProviderRecord;

    #[test]
    fn merges_auth_models_and_default_without_duplicates() {
        let dir = tempfile::tempdir().unwrap();
        let paths = Paths::new(dir.path().join("home"), dir.path().join("data"));
        let root = paths.tool_root(ToolId::Pi);
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(
            root.join("models.json"),
            r#"{"unknownTop":true,"providers":{"alpha":{"name":"Alpha","baseUrl":"https://x","models":[{"id":"m1"}]},"modelsOnly":{"models":[]}}}"#,
        )
        .unwrap();
        std::fs::write(
            root.join("auth.json"),
            r#"{"alpha":{"type":"api_key","key":"k"},"authOnly":{"type":"oauth","access":"a"}}"#,
        )
        .unwrap();
        std::fs::write(
            root.join("settings.json"),
            r#"{"defaultProvider":"defaultOnly","defaultModel":"m0"}"#,
        )
        .unwrap();

        let mut records = vec![];
        assert_eq!(import_runtime(&paths, &mut records).unwrap(), 4);
        assert_eq!(records.len(), 4);
        let alpha = records.iter().find(|p| p.id == "pi:alpha").unwrap();
        assert_eq!(alpha.name, "Alpha");
        assert_eq!(alpha.settings()["_auth"]["key"], "k");
        // In Pi, enabled is determined by presence in models.json (alpha is present)
        assert!(alpha.is_applied);
        assert!(
            records
                .iter()
                .any(|p| p.id == "pi:defaultOnly" && !p.is_applied)
        );
        assert_eq!(import_runtime(&paths, &mut records).unwrap(), 0);
    }

    #[test]
    fn apply_preserves_unknown_models_fields_and_splits_auth() {
        let dir = tempfile::tempdir().unwrap();
        let paths = Paths::new(dir.path().join("home"), dir.path().join("data"));
        let root = paths.tool_root(ToolId::Pi);
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(
            root.join("models.json"),
            r#"{"unknownTop":{"x":1},"providers":{"other":{"keep":true}}}"#,
        )
        .unwrap();
        std::fs::write(root.join("settings.json"), r#"{"packages":[],"keep":42}"#).unwrap();

        let mut p = ProviderRecord::new("Alpha", "custom");
        p.id = "pi:alpha".into();
        p.settings_config = r#"{"name":"Alpha","baseUrl":"https://x","models":[{"id":"m1"}],"_auth":{"type":"api_key","key":"secret"},"_providerKey":"alpha"}"#.into();
        let files = apply_provider(&paths, &p).unwrap();
        assert_eq!(files.len(), 2);
        let models: Value =
            serde_json::from_str(&std::fs::read_to_string(root.join("models.json")).unwrap())
                .unwrap();
        assert_eq!(models["unknownTop"]["x"], 1);
        assert_eq!(models["providers"]["other"]["keep"], true);
        assert_eq!(models["providers"]["alpha"]["baseUrl"], "https://x");
        assert!(models["providers"]["alpha"].get("_auth").is_none());
        let auth: Value =
            serde_json::from_str(&std::fs::read_to_string(root.join("auth.json")).unwrap())
                .unwrap();
        assert_eq!(auth["alpha"]["key"], "secret");
        let settings: Value =
            serde_json::from_str(&std::fs::read_to_string(root.join("settings.json")).unwrap())
                .unwrap();
        assert_eq!(settings["keep"], 42);
    }
}
