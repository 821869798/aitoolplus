//! Hermes Agent runtime adapter.
//!
//! Schema (from ai-toolbox `coding/hermes/AGENTS.md`):
//! - root: DB custom > `HERMES_HOME` env > platform default `~/.hermes`
//! - single `config.yaml` is the ONLY source of truth:
//!   - `custom_providers.<id>`: name/base_url/api_key/model/… credentials inline
//!   - top-level `model`: default model selection
//!   - `memory.enabled` + other settings
//! - prompt: `<root>/SOUL.md`
//! - memory: `<root>/memories/MEMORY.md` + `<root>/memories/USER.md`

use std::path::PathBuf;

use serde_json::Value;

use crate::paths::Paths;
use crate::providers::ProviderRecord;
use crate::tools::ToolId;

pub struct HermesRuntimePaths {
    pub root: PathBuf,
    pub config: PathBuf,
    pub prompt: PathBuf,
    pub memory_dir: PathBuf,
}

impl HermesRuntimePaths {
    pub fn from_paths(paths: &Paths) -> Self {
        let root = hermes_root(paths);
        Self {
            config: root.join("config.yaml"),
            prompt: root.join("SOUL.md"),
            memory_dir: root.join("memories"),
            root,
        }
    }
}

pub fn hermes_root(paths: &Paths) -> PathBuf {
    if let Some(dir) = paths.tool_roots.get(&ToolId::Hermes) {
        return dir.clone();
    }
    if let Ok(env) = std::env::var("HERMES_HOME")
        && !env.is_empty()
    {
        return PathBuf::from(env);
    }
    paths.home.join(".hermes")
}

fn read_config(paths: &HermesRuntimePaths) -> Result<serde_yaml::Value, String> {
    if !paths.config.exists() {
        return Ok(serde_yaml::Value::Mapping(Default::default()));
    }
    let raw = std::fs::read_to_string(&paths.config)
        .map_err(|e| format!("read {}: {e}", paths.config.display()))?;
    if raw.trim().is_empty() {
        return Ok(serde_yaml::Value::Mapping(Default::default()));
    }
    serde_yaml::from_str(&raw).map_err(|e| format!("parse config.yaml: {e}"))
}

fn write_config_atomic(paths: &HermesRuntimePaths, doc: &serde_yaml::Value) -> Result<(), String> {
    let text = serde_yaml::to_string(doc).map_err(|e| e.to_string())?;
    if let Some(parent) = paths.config.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let tmp = paths.config.with_extension("tmp");
    std::fs::write(&tmp, text).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, &paths.config).map_err(|e| e.to_string())?;
    Ok(())
}

/// Import `custom_providers` entries into UI records.
pub fn import_runtime(
    paths: &HermesRuntimePaths,
    providers: &mut Vec<ProviderRecord>,
) -> Result<usize, String> {
    let doc = read_config(paths)?;
    let Some(map) = doc.get("custom_providers").and_then(|v| v.as_mapping()) else {
        return Ok(0);
    };

    let default_provider = doc
        .get("model")
        .and_then(|v| v.get("provider"))
        .and_then(|v| v.as_str())
        .map(str::to_string);
    let default_model = doc
        .get("model")
        .and_then(|v| v.get("model"))
        .and_then(|v| v.as_str())
        .map(str::to_string);

    let mut added = 0;
    let mut next = providers.iter().map(|p| p.sort_index).max().unwrap_or(-1);
    for (key, value) in map {
        let Some(key) = key.as_str() else { continue };
        let json: Value =
            serde_json::to_value(value).map_err(|e| format!("convert provider: {e}"))?;
        let name = json
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or(key)
            .to_string();

        let stable_id = format!("hermes:{key}");
        let is_applied = default_provider.as_deref() == Some(key);
        if let Some(record) = providers.iter_mut().find(|p| p.id == stable_id) {
            record.name = name.clone();
            record.settings_config = serde_json::to_string_pretty(&json).unwrap_or_default();
            record.is_applied = is_applied;
            record.touch();
            continue;
        }
        next += 1;
        let mut record = ProviderRecord::new(name, "custom");
        record.id = stable_id;
        record.sort_index = next;
        record.is_applied = is_applied;
        record.settings_config = serde_json::to_string_pretty(&json).unwrap_or_default();
        providers.push(record);
        added += 1;
    }
    let _ = default_model;
    Ok(added)
}

/// Apply a provider: upsert `custom_providers.<id>` and set top-level
/// `model: { provider, model }`, preserving everything else.
pub fn apply_provider(
    paths: &HermesRuntimePaths,
    provider: &ProviderRecord,
) -> Result<Vec<PathBuf>, String> {
    let key = provider
        .id
        .strip_prefix("hermes:")
        .ok_or("hermes provider record must carry key")?;

    let mut doc = read_config(paths)?;
    if !matches!(doc, serde_yaml::Value::Mapping(_)) {
        return Err("config.yaml root must be a mapping".into());
    }

    let incoming: Value = serde_json::from_str(&provider.settings_config)
        .map_err(|e| format!("invalid provider JSON: {e}"))?;
    let yaml_value = serde_yaml::to_value(&incoming).map_err(|e| e.to_string())?;

    if let Some(map) = doc.as_mapping_mut() {
        let custom = map
            .entry(serde_yaml::Value::String("custom_providers".into()))
            .or_insert(serde_yaml::Value::Mapping(Default::default()));
        if let Some(cm) = custom.as_mapping_mut() {
            cm.insert(serde_yaml::Value::String(key.into()), yaml_value);
        }

        // Default model from the record's model field.
        if let Some(model) = incoming.get("model").and_then(Value::as_str) {
            let mut model_map = serde_yaml::Value::Mapping(Default::default());
            if let Some(m) = model_map.as_mapping_mut() {
                m.insert(
                    serde_yaml::Value::String("provider".into()),
                    serde_yaml::Value::String(key.into()),
                );
                m.insert(
                    serde_yaml::Value::String("model".into()),
                    serde_yaml::Value::String(model.into()),
                );
            }
            map.insert(serde_yaml::Value::String("model".into()), model_map);
        }
    }

    write_config_atomic(paths, &doc)?;
    Ok(vec![paths.config.clone()])
}

/// Merge top-level common/other settings into config.yaml, preserving the
/// runtime provider and unknown sections.
pub fn merge_other_settings(
    paths: &HermesRuntimePaths,
    settings: &serde_json::Value,
) -> Result<(), String> {
    if !settings.is_object() {
        return Err("Hermes common config must be a JSON object".into());
    }
    let mut current = read_config(paths)?;
    let overlay = serde_yaml::to_value(settings).map_err(|error| error.to_string())?;
    merge_yaml(&mut current, &overlay);
    write_config_atomic(paths, &current)
}

fn merge_yaml(base: &mut serde_yaml::Value, overlay: &serde_yaml::Value) {
    match (base, overlay) {
        (serde_yaml::Value::Mapping(base), serde_yaml::Value::Mapping(overlay)) => {
            for (key, value) in overlay {
                match base.get_mut(key) {
                    Some(current) => merge_yaml(current, value),
                    None => {
                        base.insert(key.clone(), value.clone());
                    }
                }
            }
        }
        (base, overlay) => *base = overlay.clone(),
    }
}

/// Memory files: read/write MEMORY.md and USER.md content.
pub fn read_memory(paths: &HermesRuntimePaths, which: &str) -> Result<String, String> {
    let file = match which {
        "memory" => paths.memory_dir.join("MEMORY.md"),
        "user" => paths.memory_dir.join("USER.md"),
        _ => return Err(format!("unknown memory file: {which}")),
    };
    if !file.exists() {
        return Ok(String::new());
    }
    std::fs::read_to_string(&file).map_err(|e| e.to_string())
}

pub fn write_memory(paths: &HermesRuntimePaths, which: &str, content: &str) -> Result<(), String> {
    let file = match which {
        "memory" => paths.memory_dir.join("MEMORY.md"),
        "user" => paths.memory_dir.join("USER.md"),
        _ => return Err(format!("unknown memory file: {which}")),
    };
    std::fs::create_dir_all(&paths.memory_dir).map_err(|e| e.to_string())?;
    std::fs::write(&file, content).map_err(|e| e.to_string())
}

/// The `memory.enabled` switch.
pub fn memory_enabled(paths: &HermesRuntimePaths) -> Result<bool, String> {
    let doc = read_config(paths)?;
    Ok(doc
        .get("memory")
        .and_then(|v| v.get("enabled"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false))
}

pub fn set_memory_enabled(paths: &HermesRuntimePaths, on: bool) -> Result<(), String> {
    let mut doc = read_config(paths)?;
    if let Some(map) = doc.as_mapping_mut() {
        let mut mem = serde_yaml::Value::Mapping(Default::default());
        if let Some(existing) = map.get(serde_yaml::Value::String("memory".into())) {
            mem = existing.clone();
        }
        if let Some(m) = mem.as_mapping_mut() {
            m.insert(
                serde_yaml::Value::String("enabled".into()),
                serde_yaml::Value::Bool(on),
            );
        }
        map.insert(serde_yaml::Value::String("memory".into()), mem);
    }
    write_config_atomic(paths, &doc)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() -> (tempfile::TempDir, HermesRuntimePaths) {
        let dir = tempfile::tempdir().unwrap();
        let paths = HermesRuntimePaths {
            root: dir.path().join(".hermes"),
            config: dir.path().join(".hermes").join("config.yaml"),
            prompt: dir.path().join(".hermes").join("SOUL.md"),
            memory_dir: dir.path().join(".hermes").join("memories"),
        };
        std::fs::create_dir_all(&paths.root).unwrap();
        (dir, paths)
    }

    #[test]
    fn import_and_apply_roundtrip() {
        let (_dir, paths) = setup();
        std::fs::write(
            &paths.config,
            "agent:\n  name: hermes\nmodel:\n  provider: alpha\n  model: m1\ncustom_providers:\n  alpha:\n    name: Alpha\n    base_url: https://a/v1\n    api_key: k1\nunknown:\n  keep: true\n",
        )
        .unwrap();

        let mut providers = vec![];
        assert_eq!(import_runtime(&paths, &mut providers).unwrap(), 1);
        let p = &providers[0];
        assert_eq!(p.id, "hermes:alpha");
        assert_eq!(p.name, "Alpha");
        assert!(p.is_applied);

        let mut rec = p.clone();
        rec.set_settings(&serde_json::json!({
            "name": "Alpha", "base_url": "https://a/v1", "api_key": "k1", "model": "m2"
        }));
        let files = apply_provider(&paths, &rec).unwrap();
        assert_eq!(files.len(), 1);

        let out = std::fs::read_to_string(&paths.config).unwrap();
        assert!(out.contains("unknown:"), "unknown section lost");
        assert!(out.contains("keep: true"));
        assert!(out.contains("provider: alpha"));
        assert!(out.contains("model: m2"));
        assert!(out.contains("base_url: https://a/v1"));
    }

    #[test]
    fn memory_roundtrip() {
        let (_dir, paths) = setup();
        assert_eq!(read_memory(&paths, "memory").unwrap(), "");
        write_memory(&paths, "memory", "# notes\n").unwrap();
        assert_eq!(read_memory(&paths, "memory").unwrap(), "# notes\n");

        write_memory(&paths, "user", "# user\n").unwrap();
        assert!(read_memory(&paths, "user").unwrap().contains("user"));

        assert!(!memory_enabled(&paths).unwrap());
        set_memory_enabled(&paths, true).unwrap();
        assert!(memory_enabled(&paths).unwrap());
        // existing memory content preserved after toggle
        assert!(read_memory(&paths, "memory").unwrap().contains("notes"));
    }

    #[test]
    fn missing_config_imports_zero() {
        let (_dir, paths) = setup();
        let mut providers = vec![];
        assert_eq!(import_runtime(&paths, &mut providers).unwrap(), 0);
    }
}
