//! Pi "Model Settings" + "Other Settings" page semantics
//! (mirrors ai-toolbox `coding/pi/commands.rs`).
//!
//! - Model Settings: `settings.json` `defaultProvider` / `defaultModel` /
//!   `defaultThinkingLevel` — validated against models.json (provider must
//!   exist; model must exist under that provider)
//! - Other Settings: settings.json minus `packages` (owned by the extension
//!   chain); saving merges back and always preserves `packages`
//! - `auth.json` / `models.json` / `settings.json` file previews are raw JSON
//!   with prompt (AGENTS.md) rendered as markdown

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use serde_json::Value;

use crate::paths::Paths;
use crate::pi_runtime::PiRuntimePaths;

/// The model-settings triple the Pi page edits.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PiModelSettings {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thinking_level: Option<String>,
}

pub const KNOWN_THINKING_LEVELS: [&str; 4] = ["off", "minimal", "medium", "high"];

/// Read the current model settings from `settings.json`.
pub fn read_model_settings(paths: &Paths) -> Result<PiModelSettings, String> {
    let rt = PiRuntimePaths::from_paths(paths);
    let settings = crate::pi_runtime::read_object(&rt.settings)?;
    Ok(PiModelSettings {
        provider_key: settings
            .get("defaultProvider")
            .and_then(Value::as_str)
            .map(String::from),
        model_id: settings
            .get("defaultModel")
            .and_then(Value::as_str)
            .map(String::from),
        thinking_level: settings
            .get("defaultThinkingLevel")
            .and_then(Value::as_str)
            .map(String::from),
    })
}

/// Enumerate providers + their model ids from `models.json` (plus defaults
/// from auth-only providers with no models).
pub fn models_catalog(paths: &Paths) -> Result<BTreeMap<String, Vec<String>>, String> {
    let rt = PiRuntimePaths::from_paths(paths);
    let models = crate::pi_runtime::read_object(&rt.models)?;
    let mut out: BTreeMap<String, Vec<String>> = BTreeMap::new();
    if let Some(providers) = models.get("providers").and_then(Value::as_object) {
        for (key, provider) in providers {
            let ids: Vec<String> = provider
                .get("models")
                .and_then(Value::as_array)
                .map(|arr| {
                    arr.iter()
                        .filter_map(|m| {
                            m.get("id")
                                .or_else(|| m.get("model"))
                                .and_then(Value::as_str)
                                .map(String::from)
                        })
                        .collect()
                })
                .unwrap_or_default();
            out.insert(key.clone(), ids);
        }
    }
    Ok(out)
}

/// Validate a (provider, model) pair against the catalog.
pub fn validate_model_settings(
    catalog: &BTreeMap<String, Vec<String>>,
    settings: &PiModelSettings,
) -> Result<(), String> {
    if let Some(provider) = &settings.provider_key {
        let models = catalog
            .get(provider)
            .ok_or_else(|| format!("unknown provider: {provider}"))?;
        if let Some(model) = &settings.model_id
            && !models.is_empty()
            && !models.iter().any(|m| m == model)
        {
            return Err(format!("model {model} not found under provider {provider}"));
        }
    } else if settings.model_id.is_some() {
        return Err("model set without provider".into());
    }
    if let Some(level) = &settings.thinking_level
        && !level.is_empty()
        && !KNOWN_THINKING_LEVELS.contains(&level.as_str())
    {
        // Pi accepts arbitrary levels; only warn-level validation for empty
        // vs known. Unknown values pass through untouched.
        let _ = level;
    }
    Ok(())
}

/// Write the model-settings triple into `settings.json`, preserving all
/// other fields (packages, theme, extensions, …).
pub fn write_model_settings(
    paths: &Paths,
    settings: &PiModelSettings,
) -> Result<Vec<std::path::PathBuf>, String> {
    let rt = PiRuntimePaths::from_paths(paths);
    let mut doc = crate::pi_runtime::read_object(&rt.settings)?;
    if !doc.is_object() {
        return Err("Pi settings.json must be an object".into());
    }
    let obj = doc.as_object_mut().unwrap();

    match &settings.provider_key {
        Some(v) if !v.is_empty() => {
            obj.insert("defaultProvider".into(), Value::String(v.clone()));
        }
        _ => {
            obj.remove("defaultProvider");
        }
    }
    match &settings.model_id {
        Some(v) if !v.is_empty() => {
            obj.insert("defaultModel".into(), Value::String(v.clone()));
        }
        _ => {
            obj.remove("defaultModel");
        }
    }
    match &settings.thinking_level {
        Some(v) if !v.is_empty() => {
            obj.insert("defaultThinkingLevel".into(), Value::String(v.clone()));
        }
        _ => {
            obj.remove("defaultThinkingLevel");
        }
    }

    crate::store::save_json_atomic(&rt.settings, &doc).map_err(|e| e.to_string())?;
    Ok(vec![rt.settings])
}

/// Other Settings: settings.json minus `packages`.
pub fn read_other_settings(paths: &Paths) -> Result<Value, String> {
    let rt = PiRuntimePaths::from_paths(paths);
    let mut doc = crate::pi_runtime::read_object(&rt.settings)?;
    if let Some(obj) = doc.as_object_mut() {
        obj.remove("packages");
    }
    Ok(doc)
}

/// Save Other Settings: merge the edited object back over the on-disk
/// settings.json, always preserving the extension-owned `packages` key.
pub fn write_other_settings(
    paths: &Paths,
    edited: &Value,
) -> Result<Vec<std::path::PathBuf>, String> {
    let rt = PiRuntimePaths::from_paths(paths);
    let current = crate::pi_runtime::read_object(&rt.settings)?;

    // start from edited content; re-attach packages from disk
    let mut merged = edited.clone();
    if let Some(packages) = current.get("packages")
        && let Some(obj) = merged.as_object_mut()
    {
        obj.insert("packages".into(), packages.clone());
    }
    if !merged.is_object() {
        return Err("other settings must be a JSON object".into());
    }
    crate::store::save_json_atomic(&rt.settings, &merged).map_err(|e| e.to_string())?;
    Ok(vec![rt.settings])
}

/// Raw file previews for the runtime tabs.
pub struct PiRuntimePreview {
    pub settings_content: String,
    pub auth_content: String,
    pub models_content: String,
    pub prompt_content: String,
}

pub fn read_runtime_preview(paths: &Paths) -> Result<PiRuntimePreview, String> {
    let rt = PiRuntimePaths::from_paths(paths);
    let read = |p: &std::path::Path| {
        if p.exists() {
            std::fs::read_to_string(p).map_err(|e| e.to_string())
        } else {
            Ok(String::new())
        }
    };
    Ok(PiRuntimePreview {
        settings_content: read(&rt.settings)?,
        auth_content: read(&rt.auth)?,
        models_content: read(&rt.models)?,
        prompt_content: read(&rt.prompt_prompt(paths))?,
    })
}

impl PiRuntimePaths {
    /// AGENTS.md prompt path (never CLAUDE.md).
    fn prompt_prompt(&self, _paths: &Paths) -> std::path::PathBuf {
        self.root.join("AGENTS.md")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() -> (tempfile::TempDir, Paths) {
        let dir = tempfile::tempdir().unwrap();
        let paths = Paths::new(dir.path().join("home"), dir.path().join("data"));
        let rt = PiRuntimePaths::from_paths(&paths);
        std::fs::create_dir_all(&rt.root).unwrap();
        std::fs::write(
            &rt.models,
            r#"{"providers":{"alpha":{"name":"Alpha","baseUrl":"https://a","models":[{"id":"m1"},{"id":"m2"}]},"beta":{"baseUrl":"https://b","models":[]}}}"#,
        )
        .unwrap();
        std::fs::write(
            &rt.settings,
            r#"{"packages":["npm:pi-goal"],"defaultProvider":"alpha","theme":"dark"}"#,
        )
        .unwrap();
        std::fs::write(&rt.auth, r#"{"alpha":{"type":"api_key","key":"k"}}"#).unwrap();
        std::fs::write(rt.root.join("AGENTS.md"), "# Prompt\n").unwrap();
        (dir, paths)
    }

    #[test]
    fn model_settings_roundtrip_preserving_packages() {
        let (_dir, paths) = setup();
        let cur = read_model_settings(&paths).unwrap();
        assert_eq!(cur.provider_key.as_deref(), Some("alpha"));
        assert_eq!(cur.model_id, None);

        let updated = PiModelSettings {
            provider_key: Some("alpha".into()),
            model_id: Some("m2".into()),
            thinking_level: Some("high".into()),
        };
        let files = write_model_settings(&paths, &updated).unwrap();
        assert_eq!(files.len(), 1);

        let back = read_model_settings(&paths).unwrap();
        assert_eq!(back, updated);

        let v: Value = serde_json::from_str(
            &std::fs::read_to_string(&PiRuntimePaths::from_paths(&paths).settings).unwrap(),
        )
        .unwrap();
        assert_eq!(v["packages"][0], "npm:pi-goal", "packages must survive");
        assert_eq!(v["theme"], "dark", "unrelated settings survive");
        assert_eq!(v["defaultModel"], "m2");
        assert_eq!(v["defaultThinkingLevel"], "high");
    }

    #[test]
    fn clearing_settings_removes_keys() {
        let (_dir, paths) = setup();
        write_model_settings(
            &paths,
            &PiModelSettings {
                provider_key: None,
                model_id: None,
                thinking_level: None,
            },
        )
        .unwrap();
        let back = read_model_settings(&paths).unwrap();
        assert_eq!(back, PiModelSettings::default());
        let v: Value = serde_json::from_str(
            &std::fs::read_to_string(&PiRuntimePaths::from_paths(&paths).settings).unwrap(),
        )
        .unwrap();
        assert!(v.get("defaultProvider").is_none());
        assert!(v.get("defaultModel").is_none());
    }

    #[test]
    fn catalog_and_validation() {
        let (_dir, paths) = setup();
        let catalog = models_catalog(&paths).unwrap();
        assert_eq!(
            catalog.get("alpha").unwrap(),
            &vec!["m1".to_string(), "m2".to_string()]
        );
        assert!(catalog.contains_key("beta"));

        assert!(
            validate_model_settings(
                &catalog,
                &PiModelSettings {
                    provider_key: Some("alpha".into()),
                    model_id: Some("m2".into()),
                    thinking_level: None
                }
            )
            .is_ok()
        );
        assert!(
            validate_model_settings(
                &catalog,
                &PiModelSettings {
                    provider_key: Some("nope".into()),
                    model_id: None,
                    thinking_level: None
                }
            )
            .is_err()
        );
        assert!(
            validate_model_settings(
                &catalog,
                &PiModelSettings {
                    provider_key: Some("alpha".into()),
                    model_id: Some("mX".into()),
                    thinking_level: None
                }
            )
            .is_err()
        );
        // beta has zero models declared: any model id passes
        assert!(
            validate_model_settings(
                &catalog,
                &PiModelSettings {
                    provider_key: Some("beta".into()),
                    model_id: Some("anything".into()),
                    thinking_level: None
                }
            )
            .is_ok()
        );
        assert!(
            validate_model_settings(
                &catalog,
                &PiModelSettings {
                    provider_key: None,
                    model_id: Some("m1".into()),
                    thinking_level: None
                }
            )
            .is_err()
        );
    }

    #[test]
    fn other_settings_hides_and_preserves_packages() {
        let (_dir, paths) = setup();
        let other = read_other_settings(&paths).unwrap();
        assert!(other.get("packages").is_none(), "packages must be hidden");
        assert_eq!(other["theme"], "dark");
        assert_eq!(other["defaultProvider"], "alpha");

        // user edits theme; packages must be re-attached on save
        let mut edited = other.clone();
        edited["theme"] = Value::String("light".into());
        let files = write_other_settings(&paths, &edited).unwrap();
        assert_eq!(files.len(), 1);
        let v: Value = serde_json::from_str(
            &std::fs::read_to_string(&PiRuntimePaths::from_paths(&paths).settings).unwrap(),
        )
        .unwrap();
        assert_eq!(v["theme"], "light");
        assert_eq!(
            v["packages"][0], "npm:pi-goal",
            "packages preserved on save"
        );
    }

    #[test]
    fn runtime_preview_reads_all_files() {
        let (_dir, paths) = setup();
        let preview = read_runtime_preview(&paths).unwrap();
        assert!(preview.settings_content.contains("defaultProvider"));
        assert!(preview.auth_content.contains("api_key"));
        assert!(preview.models_content.contains("alpha"));
        assert!(preview.prompt_content.contains("# Prompt"));
    }
}
