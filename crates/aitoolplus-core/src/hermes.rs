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
    #[cfg(target_os = "windows")]
    {
        if let Ok(local) = std::env::var("LOCALAPPDATA") {
            if !local.trim().is_empty() {
                let local_dir = PathBuf::from(local).join("hermes");
                if local_dir.exists() || !paths.home.join(".hermes").exists() {
                    return local_dir;
                }
            }
        }
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

fn slug_key(raw: &str) -> String {
    let key: String = raw
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' {
                c
            } else {
                '-'
            }
        })
        .collect();
    let trimmed = key.trim_matches('-').to_lowercase();
    if trimmed.is_empty() {
        raw.trim().to_lowercase()
    } else {
        trimmed
    }
}

fn normalize_provider_json_for_hermes(incoming: &mut Value) {
    const KEY_ALIASES: &[(&str, &str)] = &[
        ("baseUrl", "base_url"),
        ("apiKey", "api_key"),
        ("apiMode", "api_mode"),
        ("maxTokens", "max_tokens"),
        ("contextLength", "context_length"),
    ];
    let Some(obj) = incoming.as_object_mut() else {
        return;
    };
    for (from, to) in KEY_ALIASES {
        if let Some(val) = obj.remove(*from) {
            obj.entry((*to).to_string()).or_insert(val);
        }
    }
}

/// Import `custom_providers` entries (standard YAML sequence or legacy mapping) into UI records.
pub fn import_runtime(
    paths: &HermesRuntimePaths,
    providers: &mut Vec<ProviderRecord>,
) -> Result<usize, String> {
    let doc = read_config(paths)?;
    let Some(raw_custom) = doc.get("custom_providers") else {
        return Ok(0);
    };

    let entries: Vec<(String, Value)> = if let Some(seq) = raw_custom.as_sequence() {
        seq.iter()
            .filter_map(|item| {
                let json = serde_json::to_value(item).ok()?;
                let name = json
                    .get("name")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|s| !s.is_empty())?
                    .to_string();
                Some((slug_key(&name), json))
            })
            .collect()
    } else if let Some(map) = raw_custom.as_mapping() {
        map.iter()
            .filter_map(|(k, v)| {
                let key = k.as_str()?;
                let mut json = serde_json::to_value(v).ok()?;
                if let Some(obj) = json.as_object_mut() {
                    obj.entry("name")
                        .or_insert_with(|| Value::String(key.to_string()));
                }
                Some((slug_key(key), json))
            })
            .collect()
    } else {
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
    for (key, json) in entries {
        let name = json
            .get("name")
            .and_then(Value::as_str)
            .filter(|s| !s.trim().is_empty())
            .unwrap_or(&key)
            .to_string();

        let stable_id = format!("hermes:{key}");
        let is_applied = default_provider
            .as_deref()
            .is_some_and(|dp| dp == name || dp == key || slug_key(dp) == key);
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

/// Apply a provider: upsert into `custom_providers` YAML sequence (`[{ name, ... }]`)
/// and set top-level `model: { provider, model }`, preserving everything else.
pub fn apply_provider(
    paths: &HermesRuntimePaths,
    provider: &ProviderRecord,
) -> Result<Vec<PathBuf>, String> {
    let key = provider
        .id
        .strip_prefix("hermes:")
        .unwrap_or(&provider.id);

    let mut doc = read_config(paths)?;
    if !matches!(doc, serde_yaml::Value::Mapping(_)) {
        return Err("config.yaml root must be a mapping".into());
    }

    let mut incoming: Value = serde_json::from_str(&provider.settings_config)
        .map_err(|e| format!("invalid provider JSON: {e}"))?;
    normalize_provider_json_for_hermes(&mut incoming);

    let entry_name = incoming
        .get("name")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(String::from)
        .unwrap_or_else(|| {
            if !provider.name.trim().is_empty() {
                provider.name.trim().to_string()
            } else {
                key.to_string()
            }
        });
    if let Some(obj) = incoming.as_object_mut() {
        obj.insert("name".into(), Value::String(entry_name.clone()));
    }

    let mut yaml_value = serde_yaml::to_value(&incoming).map_err(|e| e.to_string())?;
    let entry_slug = slug_key(&entry_name);
    let key_slug = slug_key(key);

    if let Some(map) = doc.as_mapping_mut() {
        let mut seq: Vec<serde_yaml::Value> = match map.get(&serde_yaml::Value::String("custom_providers".into())) {
            Some(serde_yaml::Value::Sequence(seq)) => seq.clone(),
            Some(serde_yaml::Value::Mapping(m)) => m
                .iter()
                .map(|(k, v)| {
                    let mut item = v.clone();
                    if let (Some(ks), serde_yaml::Value::Mapping(im)) = (k.as_str(), &mut item) {
                        im.entry(serde_yaml::Value::String("name".into()))
                            .or_insert_with(|| serde_yaml::Value::String(ks.to_string()));
                    }
                    item
                })
                .collect(),
            _ => Vec::new(),
        };

        let matches_entry = |item: &serde_yaml::Value| {
            item.get("name")
                .and_then(|v| v.as_str())
                .is_some_and(|n| {
                    let n_slug = slug_key(n);
                    n == entry_name || n == key || n_slug == entry_slug || n_slug == key_slug
                })
        };
        if let Some(existing) = seq.iter_mut().find(|item| matches_entry(item)) {
            if let (Some(existing_map), serde_yaml::Value::Mapping(new_map)) =
                (existing.as_mapping(), &mut yaml_value)
            {
                for (k, v) in existing_map {
                    new_map.entry(k.clone()).or_insert_with(|| v.clone());
                }
            }
            *existing = yaml_value;
        } else {
            seq.push(yaml_value);
        }

        map.insert(
            serde_yaml::Value::String("custom_providers".into()),
            serde_yaml::Value::Sequence(seq),
        );

        // Default model from the record's model field.
        if let Some(model) = incoming
            .get("model")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            let mut model_map = serde_yaml::Value::Mapping(Default::default());
            if let Some(m) = model_map.as_mapping_mut() {
                m.insert(
                    serde_yaml::Value::String("provider".into()),
                    serde_yaml::Value::String(entry_name),
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

/// Remove a provider from Hermes config.yaml custom_providers.
pub fn remove_provider(
    paths: &HermesRuntimePaths,
    provider_id: &str,
) -> Result<Vec<PathBuf>, String> {
    let key = provider_id
        .strip_prefix("hermes:")
        .unwrap_or(provider_id);
    let key_slug = slug_key(key);

    let mut doc = read_config(paths)?;
    let mut changed = false;
    let mut removed_names: Vec<String> = vec![key.to_string()];

    if let Some(map) = doc.as_mapping_mut() {
        let custom_key = serde_yaml::Value::String("custom_providers".into());
        match map.get_mut(&custom_key) {
            Some(serde_yaml::Value::Sequence(seq)) => {
                let before = seq.len();
                seq.retain(|item| {
                    let matched = item
                        .get("name")
                        .and_then(|v| v.as_str())
                        .is_some_and(|n| {
                            let match_flag = n == key || slug_key(n) == key_slug;
                            if match_flag {
                                removed_names.push(n.to_string());
                            }
                            match_flag
                        });
                    !matched
                });
                if seq.len() != before {
                    changed = true;
                }
            }
            Some(serde_yaml::Value::Mapping(custom)) => {
                if custom.remove(&serde_yaml::Value::String(key.into())).is_some() {
                    changed = true;
                }
            }
            _ => {}
        }
        if let Some(model) = map
            .get_mut(&serde_yaml::Value::String("model".into()))
            .and_then(|v| v.as_mapping_mut())
        {
            if model
                .get(&serde_yaml::Value::String("provider".into()))
                .and_then(|v| v.as_str())
                .is_some_and(|p| p == key || slug_key(p) == key_slug || removed_names.iter().any(|rn| rn == p))
            {
                map.remove(&serde_yaml::Value::String("model".into()));
                changed = true;
            }
        }
    }

    if changed {
        write_config_atomic(paths, &doc)?;
        Ok(vec![paths.config.clone()])
    } else {
        Ok(vec![])
    }
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
            "agent:\n  name: hermes\nmodel:\n  provider: Alpha\n  model: m1\ncustom_providers:\n  - name: Alpha\n    base_url: https://a/v1\n    api_key: k1\nunknown:\n  keep: true\n",
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
        let parsed: serde_yaml::Value = serde_yaml::from_str(&out).unwrap();
        assert!(parsed["custom_providers"].is_sequence(), "custom_providers must be a YAML sequence: {out}");
        assert_eq!(parsed["custom_providers"][0]["name"].as_str(), Some("Alpha"));
        assert_eq!(parsed["unknown"]["keep"].as_bool(), Some(true));
        assert_eq!(parsed["model"]["provider"].as_str(), Some("Alpha"));
        assert_eq!(parsed["model"]["model"].as_str(), Some("m2"));
        assert_eq!(parsed["custom_providers"][0]["base_url"].as_str(), Some("https://a/v1"));
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

    #[test]
    fn test_remove_provider_cleans_custom_providers_and_model() {
        let (_dir, paths) = setup();
        std::fs::write(
            &paths.config,
            "custom_providers:\n  - name: Remove\n    base_url: https://r\n  - name: Keep\n    base_url: https://k\nmodel:\n  provider: Remove\n  model: m1\n",
        ).unwrap();

        let removed = remove_provider(&paths, "hermes:remove").unwrap();
        assert_eq!(removed.len(), 1);

        let content = std::fs::read_to_string(&paths.config).unwrap();
        assert!(!content.contains("Remove"));
        assert!(content.contains("Keep"));
        assert!(!content.contains("provider: Remove"));
    }
}
