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

/// Marker filename written inside each AIToolPlus-managed built-in Pi extension directory.
pub const BUILTIN_EXT_MARKER: &str = ".aitoolplus-managed";

/// Static specification for a built-in Pi extension maintained by AIToolPlus.
#[derive(Debug, Clone, Copy)]
pub struct BuiltinPiExtensionSpec {
    /// Directory name under `~/.pi/agent/extensions/<id>/index.ts`.
    pub id: &'static str,
    /// Key stored in `AppSettings::enabled_builtin_extensions` (e.g. `"pi:anyrouter"`).
    pub setting_key: &'static str,
    pub title_zh: &'static str,
    pub title_en: &'static str,
    pub description_zh: &'static str,
    pub description_en: &'static str,
    pub version: &'static str,
    pub source_ts: &'static str,
    /// Provider preset names unlocked when this built-in extension is enabled.
    pub preset_names: &'static [&'static str],
    /// Custom Pi API format IDs unlocked when this built-in extension is enabled.
    pub api_formats: &'static [&'static str],
}

/// Extensible registry of all built-in Pi extensions bundled with AIToolPlus.
pub const BUILTIN_PI_EXTENSIONS: &[BuiltinPiExtensionSpec] = &[BuiltinPiExtensionSpec {
    id: "anyrouter",
    setting_key: "pi:anyrouter",
    title_zh: "AnyRouter 协议适配器 (anyrouter)",
    title_en: "AnyRouter Protocol Adapter (anyrouter)",
    description_zh: "AIToolPlus 自维护内置脚本 · 提供独立的 AnyRouter Claude Code (/v1/messages?beta=true) 与 AnyRouter Codex (/v1/responses) 双协议格式支持",
    description_en: "AIToolPlus self-maintained built-in extension · Separate AnyRouter Claude Code & Codex protocol adapters",
    version: "1.1.1",
    source_ts: include_str!("../assets/pi_anyrouter_extension.ts"),
    preset_names: &["AnyRouter (Claude Code)", "AnyRouter (Codex)"],
    api_formats: &["anyrouter-claude", "anyrouter-codex"],
}];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuiltinPiExtensionStatus {
    pub id: &'static str,
    pub setting_key: &'static str,
    pub title_zh: &'static str,
    pub title_en: &'static str,
    pub description_zh: &'static str,
    pub description_en: &'static str,
    pub enabled: bool,
    pub installed: bool,
    pub installed_version: Option<String>,
    pub bundled_version: &'static str,
    pub update_available: bool,
    pub path: PathBuf,
}

/// Extract the `@aitoolplus-version: <version>` header from a TypeScript extension file.
pub fn parse_extension_version(content: &str) -> Option<String> {
    for line in content.lines().take(25) {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("// @aitoolplus-version:") {
            let v = rest.trim();
            if !v.is_empty() {
                return Some(v.to_string());
            }
        }
    }
    None
}

/// Look up a built-in Pi extension specification by its `id` (`"anyrouter"`) or `setting_key` (`"pi:anyrouter"`).
pub fn find_builtin_pi_extension(id_or_key: &str) -> Option<&'static BuiltinPiExtensionSpec> {
    BUILTIN_PI_EXTENSIONS
        .iter()
        .find(|spec| spec.id == id_or_key || spec.setting_key == id_or_key)
}

fn is_spec_enabled(spec: &BuiltinPiExtensionSpec, enabled_keys: &[String]) -> bool {
    enabled_keys
        .iter()
        .any(|k| k == spec.setting_key || k == spec.id)
}

/// Returns true if a Pi provider preset is allowed under the current enabled built-in extensions.
pub fn is_pi_preset_allowed(preset_name: &str, enabled_keys: &[String]) -> bool {
    for spec in BUILTIN_PI_EXTENSIONS {
        if spec.preset_names.contains(&preset_name) {
            return is_spec_enabled(spec, enabled_keys);
        }
    }
    true
}

/// Returns true if a Pi API format is allowed under the current enabled built-in extensions.
pub fn is_pi_api_format_allowed(api_format: &str, enabled_keys: &[String]) -> bool {
    for spec in BUILTIN_PI_EXTENSIONS {
        if spec.api_formats.contains(&api_format) {
            return is_spec_enabled(spec, enabled_keys);
        }
    }
    true
}

fn write_managed_marker(marker_path: &Path, spec: &BuiltinPiExtensionSpec) {
    let payload = serde_json::json!({
        "id": spec.id,
        "version": spec.version,
        "managedBy": "aitoolplus",
    });
    let _ = std::fs::write(
        marker_path,
        serde_json::to_string_pretty(&payload).unwrap_or_else(|_| "managed by aitoolplus\n".into()),
    );
}

/// Inspect the status of all built-in Pi extensions.
pub fn builtin_pi_extensions_status(
    paths: &Paths,
    enabled_keys: &[String],
) -> Vec<BuiltinPiExtensionStatus> {
    let runtime = PiRuntimePaths::from_paths(paths);
    let ext_root = runtime.root.join("extensions");

    BUILTIN_PI_EXTENSIONS
        .iter()
        .map(|spec| {
            let ext_dir = ext_root.join(spec.id);
            let index_path = ext_dir.join("index.ts");
            let marker_path = ext_dir.join(BUILTIN_EXT_MARKER);
            let installed = index_path.is_file();
            let enabled = is_spec_enabled(spec, enabled_keys);

            let installed_version = if installed {
                std::fs::read_to_string(&index_path)
                    .ok()
                    .and_then(|c| parse_extension_version(&c))
                    .or_else(|| {
                        std::fs::read_to_string(&marker_path)
                            .ok()
                            .and_then(|raw| serde_json::from_str::<Value>(&raw).ok())
                            .and_then(|v| {
                                v.get("version").and_then(Value::as_str).map(String::from)
                            })
                    })
            } else {
                None
            };

            let update_available =
                installed && installed_version.as_deref() != Some(spec.version);

            BuiltinPiExtensionStatus {
                id: spec.id,
                setting_key: spec.setting_key,
                title_zh: spec.title_zh,
                title_en: spec.title_en,
                description_zh: spec.description_zh,
                description_en: spec.description_en,
                enabled,
                installed,
                installed_version,
                bundled_version: spec.version,
                update_available,
                path: index_path,
            }
        })
        .collect()
}

/// Enable (install/upgrade) or disable (remove) a specific built-in Pi extension on disk.
pub fn set_builtin_pi_extension_enabled(
    paths: &Paths,
    id_or_key: &str,
    enabled: bool,
) -> Result<PathBuf, String> {
    let spec = find_builtin_pi_extension(id_or_key)
        .ok_or_else(|| format!("unknown built-in Pi extension: {id_or_key}"))?;
    let runtime = PiRuntimePaths::from_paths(paths);
    let ext_dir = runtime.root.join("extensions").join(spec.id);
    let index_path = ext_dir.join("index.ts");
    let marker_path = ext_dir.join(BUILTIN_EXT_MARKER);

    if enabled {
        std::fs::create_dir_all(&ext_dir)
            .map_err(|e| format!("create {}: {e}", ext_dir.display()))?;
        let needs_write = match std::fs::read_to_string(&index_path) {
            Ok(existing) => existing != spec.source_ts,
            Err(_) => true,
        };
        if needs_write {
            std::fs::write(&index_path, spec.source_ts)
                .map_err(|e| format!("write {}: {e}", index_path.display()))?;
        }
        write_managed_marker(&marker_path, spec);
        Ok(index_path)
    } else {
        if ext_dir.exists() {
            std::fs::remove_dir_all(&ext_dir)
                .map_err(|e| format!("remove {}: {e}", ext_dir.display()))?;
        }
        Ok(index_path)
    }
}

/// Reconcile all built-in Pi extensions on disk to match `enabled_keys` (e.g. on Startup,
/// Cloud Sync pull, or Backup Restore), automatically installing enabled extensions and
/// auto-upgrading any outdated built-in extension scripts.
pub fn sync_builtin_pi_extensions(
    paths: &Paths,
    enabled_keys: &[String],
) -> Result<Vec<PathBuf>, String> {
    let mut touched = Vec::new();
    for spec in BUILTIN_PI_EXTENSIONS {
        let enabled = is_spec_enabled(spec, enabled_keys);
        let path = set_builtin_pi_extension_enabled(paths, spec.id, enabled)?;
        if enabled {
            touched.push(path);
        }
    }
    Ok(touched)
}

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
    let mut auth = read_object(&runtime.auth)?;
    let mut models = read_object(&runtime.models)?;

    let mut auth_map = auth.as_object().cloned().unwrap_or_default();
    let mut model_map = models
        .get("providers")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();

    // Reconcile store -> runtime: if store.json has enabled Pi providers (e.g. pulled via
    // Cloud Sync / Git / Backup Restore onto a machine where ~/.pi/agent/models.json does
    // not have them yet), materialize them into models.json, auth.json, and extensions/.
    let mut materialized_any = false;
    for record in providers.iter() {
        if !record.is_applied {
            continue;
        }
        let key = record.id.strip_prefix("pi:").unwrap_or(&record.id);
        if !key.is_empty() && !model_map.contains_key(key) && !auth_map.contains_key(key) {
            if set_provider_enabled(paths, record, true).is_ok() {
                materialized_any = true;
            }
        }
    }
    if materialized_any {
        auth = read_object(&runtime.auth)?;
        models = read_object(&runtime.models)?;
        auth_map = auth.as_object().cloned().unwrap_or_default();
        model_map = models
            .get("providers")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
    }

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
    if !enabled {
        return remove_provider(paths, &provider.id);
    }
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

    let mut model_provider = incoming.clone();
    if let Some(obj) = model_provider.as_object_mut() {
        obj.remove("_auth");
        obj.remove("_providerKey");

        // Ensure custom headers/User-Agent from provider metadata are included in models.json if missing
        let meta = provider.parsed_meta();
        if !obj.contains_key("headers") {
            let mut h_map = Map::new();
            if let Some(ua) = &meta.custom_user_agent {
                if !ua.trim().is_empty() {
                    h_map.insert("User-Agent".into(), Value::String(ua.trim().to_string()));
                }
            }
            if let Some(headers) = &meta.custom_headers {
                for h in headers {
                    if !h.name.trim().is_empty() && !h.value.trim().is_empty() {
                        h_map.insert(h.name.trim().to_string(), Value::String(h.value.trim().to_string()));
                    }
                }
            }
            if !h_map.is_empty() {
                obj.insert("headers".into(), Value::Object(h_map));
            }
        }
    }
    let uses_builtin_ext_api = model_provider
        .get("api")
        .and_then(Value::as_str)
        .is_some_and(|api| {
            BUILTIN_PI_EXTENSIONS
                .iter()
                .any(|spec| spec.api_formats.contains(&api.trim()))
        });
    providers.insert(key.to_string(), model_provider);

    let explicit_api_key = incoming
        .get("apiKey")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty());

    if let Some(credential) = incoming.get("_auth") {
        let mut cred_to_save = credential.clone();
        if let Some(k) = explicit_api_key {
            if let Some(c_obj) = cred_to_save.as_object_mut() {
                if c_obj.get("type").and_then(Value::as_str) == Some("api_key") {
                    c_obj.insert("key".into(), Value::String(k.to_string()));
                }
            }
        }
        let mut auth = read_object(&runtime.auth)?;
        auth.as_object_mut()
            .ok_or_else(|| "Pi auth.json must be an object".to_string())?
            .insert(key.to_string(), cred_to_save);
        save_json_atomic(&runtime.auth, &auth).map_err(|e| e.to_string())?;
        files.push(runtime.auth.clone());
    } else if uses_builtin_ext_api {
        if let Some(k) = explicit_api_key {
            let mut auth = read_object(&runtime.auth)?;
            auth.as_object_mut()
                .ok_or_else(|| "Pi auth.json must be an object".to_string())?
                .insert(
                    key.to_string(),
                    serde_json::json!({
                        "type": "api_key",
                        "key": k,
                    }),
                );
            save_json_atomic(&runtime.auth, &auth).map_err(|e| e.to_string())?;
            files.push(runtime.auth.clone());
        }
    }

    save_json_atomic(&runtime.models, &models).map_err(|e| e.to_string())?;
    Ok(files)
}

/// Remove a provider completely from Pi's runtime files:
/// - removes key from `models.json` under `providers`
/// - removes key from `auth.json`
/// - clears `defaultProvider` and `defaultModel` in `settings.json` if pointing to key
pub fn remove_provider(paths: &Paths, provider_id: &str) -> Result<Vec<PathBuf>, String> {
    let runtime = PiRuntimePaths::from_paths(paths);
    let key = provider_id
        .strip_prefix("pi:")
        .unwrap_or(provider_id);

    let mut files = Vec::new();

    let mut models = read_object(&runtime.models)?;
    let mut model_changed = false;
    if let Some(model_root) = models.as_object_mut() {
        if let Some(providers) = model_root.get_mut("providers").and_then(Value::as_object_mut) {
            if providers.remove(key).is_some() {
                model_changed = true;
            }
        }
    }
    if model_changed {
        save_json_atomic(&runtime.models, &models).map_err(|e| e.to_string())?;
        files.push(runtime.models.clone());
    }

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

    #[test]
    fn remove_provider_cleans_models_auth_and_settings() {
        let dir = tempfile::tempdir().unwrap();
        let paths = Paths::new(dir.path().join("home"), dir.path().join("data"));
        let root = paths.tool_root(ToolId::Pi);
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(
            root.join("models.json"),
            r#"{"providers":{"alpha":{"name":"Alpha"},"beta":{"name":"Beta"}}}"#,
        )
        .unwrap();
        std::fs::write(
            root.join("auth.json"),
            r#"{"alpha":{"type":"api_key","key":"k"},"beta":{"type":"api_key","key":"k2"}}"#,
        )
        .unwrap();
        std::fs::write(
            root.join("settings.json"),
            r#"{"defaultProvider":"alpha","defaultModel":"m1","other":1}"#,
        )
        .unwrap();

        let files = remove_provider(&paths, "pi:alpha").unwrap();
        assert_eq!(files.len(), 3);

        let models: Value =
            serde_json::from_str(&std::fs::read_to_string(root.join("models.json")).unwrap())
                .unwrap();
        assert!(models["providers"].get("alpha").is_none());
        assert!(models["providers"].get("beta").is_some());

        let auth: Value =
            serde_json::from_str(&std::fs::read_to_string(root.join("auth.json")).unwrap())
                .unwrap();
        assert!(auth.get("alpha").is_none());
        assert!(auth.get("beta").is_some());

        let settings: Value =
            serde_json::from_str(&std::fs::read_to_string(root.join("settings.json")).unwrap())
                .unwrap();
        assert!(settings.get("defaultProvider").is_none());
        assert!(settings.get("defaultModel").is_none());
        assert_eq!(settings["other"], 1);
    }

    #[test]
    fn builtin_extensions_registry_toggle_upgrade_and_sync() {
        let dir = tempfile::tempdir().unwrap();
        let paths = Paths::new(dir.path().join("home"), dir.path().join("data"));
        let root = paths.tool_root(ToolId::Pi);

        // 1. Initially disabled: AnyRouter presets & API formats are gated off
        let empty_enabled: Vec<String> = vec![];
        assert!(!is_pi_preset_allowed("AnyRouter (Claude Code)", &empty_enabled));
        assert!(!is_pi_preset_allowed("AnyRouter (Codex)", &empty_enabled));
        assert!(!is_pi_api_format_allowed("anyrouter-claude", &empty_enabled));
        assert!(!is_pi_api_format_allowed("anyrouter-codex", &empty_enabled));
        assert!(is_pi_preset_allowed("DeepSeek", &empty_enabled));

        // 2. Simulate an older version (0.9.0) on disk, then sync with enabled list -> auto-upgrades to 1.1.0
        let ext_dir = root.join("extensions").join("anyrouter");
        std::fs::create_dir_all(&ext_dir).unwrap();
        std::fs::write(
            ext_dir.join("index.ts"),
            "// @aitoolplus-extension: anyrouter\n// @aitoolplus-version: 0.9.0\nexport default function() {}\n",
        )
        .unwrap();

        let enabled = vec!["pi:anyrouter".to_string()];
        let statuses_before = builtin_pi_extensions_status(&paths, &enabled);
        assert_eq!(statuses_before.len(), 1);
        assert_eq!(statuses_before[0].installed_version.as_deref(), Some("0.9.0"));
        assert!(statuses_before[0].update_available);

        // Syncing with enabled_keys installs/auto-upgrades to bundled version
        sync_builtin_pi_extensions(&paths, &enabled).unwrap();
        let statuses_after = builtin_pi_extensions_status(&paths, &enabled);
        assert!(statuses_after[0].installed);
        assert!(statuses_after[0].enabled);
        assert_eq!(statuses_after[0].installed_version.as_deref(), Some("1.1.1"));
        assert!(!statuses_after[0].update_available);
        assert!(is_pi_preset_allowed("AnyRouter (Claude Code)", &enabled));
        assert!(is_pi_preset_allowed("AnyRouter (Codex)", &enabled));
        assert!(is_pi_api_format_allowed("anyrouter-claude", &enabled));
        assert!(is_pi_api_format_allowed("anyrouter-codex", &enabled));

        // 3. Turning the switch off in enabled_keys and syncing removes the extension
        sync_builtin_pi_extensions(&paths, &empty_enabled).unwrap();
        let statuses_off = builtin_pi_extensions_status(&paths, &empty_enabled);
        assert!(!statuses_off[0].installed);
        assert!(!statuses_off[0].enabled);
    }
}
