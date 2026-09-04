//! Oh My Pi (OMP) runtime adapter.
//!
//! Schema (from ai-toolbox `coding/oh_my_pi/AGENTS.md`):
//! - root: `~/.config/oh-my-pi` style platform config dir
//! - `models.yml` (YAML): provider catalog, map of provider id -> settings
//! - `config.yml` (YAML): dotted camelCase key settings (e.g. `ui.theme`)
//! - `mcp.json`: derived MCP file (owned by the global MCP module)
//! - prompt: `<root>/AGENTS.md`
//! - auth DB remains owned by OMP itself; we never write credentials here

use std::path::PathBuf;

use serde_json::Value;

use crate::paths::Paths;
use crate::providers::ProviderRecord;
use crate::tools::ToolId;

pub struct OmpRuntimePaths {
    pub root: PathBuf,
    pub models: PathBuf,
    pub config: PathBuf,
    pub mcp: PathBuf,
    pub prompt: PathBuf,
}

impl OmpRuntimePaths {
    pub fn from_paths(paths: &Paths) -> Self {
        let root = omp_root(paths);
        Self {
            models: root.join("models.yml"),
            config: root.join("config.yml"),
            mcp: root.join("mcp.json"),
            prompt: root.join("AGENTS.md"),
            root,
        }
    }
}

pub fn omp_root(paths: &Paths) -> PathBuf {
    if let Some(dir) = paths.tool_roots.get(&ToolId::OhMyPi) {
        return dir.clone();
    }
    if let Ok(env) = std::env::var("OH_MY_PI_HOME")
        && !env.is_empty()
    {
        return PathBuf::from(env);
    }
    paths.home.join(".config").join("oh-my-pi")
}

fn read_yaml(path: &PathBuf) -> Result<serde_yaml::Value, String> {
    if !path.exists() {
        return Ok(serde_yaml::Value::Mapping(Default::default()));
    }
    let raw = std::fs::read_to_string(path).map_err(|e| format!("read {}: {e}", path.display()))?;
    if raw.trim().is_empty() {
        return Ok(serde_yaml::Value::Mapping(Default::default()));
    }
    serde_yaml::from_str(&raw).map_err(|e| format!("parse {}: {e}", path.display()))
}

fn write_yaml_atomic(path: &PathBuf, doc: &serde_yaml::Value) -> Result<(), String> {
    let text = serde_yaml::to_string(doc).map_err(|e| e.to_string())?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, text).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, path).map_err(|e| e.to_string())?;
    Ok(())
}

/// Import providers from `models.yml`.
pub fn import_runtime(
    paths: &OmpRuntimePaths,
    providers: &mut Vec<ProviderRecord>,
) -> Result<usize, String> {
    let doc = read_yaml(&paths.models)?;
    let Some(map) = doc.as_mapping() else {
        return Ok(0);
    };

    let mut added = 0;
    let mut next = providers.iter().map(|p| p.sort_index).max().unwrap_or(-1);
    for (key, value) in map {
        let Some(key) = key.as_str() else { continue };
        let json: Value =
            serde_json::to_value(value).map_err(|e| format!("convert provider: {e}"))?;
        let name = json
            .get("name")
            .and_then(Value::as_str)
            .or(json.get("label").and_then(Value::as_str))
            .unwrap_or(key)
            .to_string();

        let stable_id = format!("omp:{key}");
        if let Some(record) = providers.iter_mut().find(|p| p.id == stable_id) {
            record.name = name;
            record.settings_config = serde_json::to_string_pretty(&json).unwrap_or_default();
            record.touch();
            continue;
        }
        next += 1;
        let mut record = ProviderRecord::new(name, "custom");
        record.id = stable_id;
        record.sort_index = next;
        record.settings_config = serde_json::to_string_pretty(&json).unwrap_or_default();
        providers.push(record);
        added += 1;
    }
    Ok(added)
}

/// Apply: upsert `models.yml.<key>` preserving other entries.
pub fn apply_provider(
    paths: &OmpRuntimePaths,
    provider: &ProviderRecord,
) -> Result<Vec<PathBuf>, String> {
    let key = provider
        .id
        .strip_prefix("omp:")
        .ok_or("omp provider record must carry key")?;

    let mut doc = read_yaml(&paths.models)?;
    if !matches!(doc, serde_yaml::Value::Mapping(_)) {
        return Err("models.yml root must be a mapping".into());
    }
    let incoming: Value = serde_json::from_str(&provider.settings_config)
        .map_err(|e| format!("invalid provider JSON: {e}"))?;
    let yaml_value = serde_yaml::to_value(&incoming).map_err(|e| e.to_string())?;

    if let Some(map) = doc.as_mapping_mut() {
        map.insert(serde_yaml::Value::String(key.into()), yaml_value);
    }
    write_yaml_atomic(&paths.models, &doc)?;
    Ok(vec![paths.models.clone()])
}

/// Merge a complete JSON object into config.yml, preserving unknown fields.
pub fn merge_other_settings(paths: &OmpRuntimePaths, settings: &Value) -> Result<(), String> {
    if !settings.is_object() {
        return Err("OMP common config must be a JSON object".into());
    }
    let mut current = read_yaml(&paths.config)?;
    let overlay = serde_yaml::to_value(settings).map_err(|error| error.to_string())?;
    merge_yaml(&mut current, &overlay);
    write_yaml_atomic(&paths.config, &current)
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

/// Read one `config.yml` dotted key (e.g. `ui.theme`).
pub fn config_get(paths: &OmpRuntimePaths, dotted_key: &str) -> Result<Option<Value>, String> {
    let doc = read_yaml(&paths.config)?;
    let mut current = &doc;
    for part in dotted_key.split('.') {
        match current.get(part) {
            Some(v) => current = v,
            None => return Ok(None),
        }
    }
    Ok(serde_json::to_value(current).ok())
}

/// Write one `config.yml` dotted key, creating intermediate mappings.
pub fn config_set(paths: &OmpRuntimePaths, dotted_key: &str, value: &Value) -> Result<(), String> {
    let mut doc = read_yaml(&paths.config)?;
    if !matches!(doc, serde_yaml::Value::Mapping(_)) {
        return Err("config.yml root must be a mapping".into());
    }
    let parts: Vec<&str> = dotted_key.split('.').collect();
    let yaml_value = serde_yaml::to_value(value).map_err(|e| e.to_string())?;

    let mut current = &mut doc;
    for part in &parts[..parts.len() - 1] {
        if !matches!(current, serde_yaml::Value::Mapping(_)) {
            return Err(format!("config.yml key {part} is not a mapping"));
        }
        let map = current.as_mapping_mut().unwrap();
        let entry = map
            .entry(serde_yaml::Value::String((*part).into()))
            .or_insert(serde_yaml::Value::Mapping(Default::default()));
        current = entry;
    }
    let last = parts[parts.len() - 1];
    if let Some(map) = current.as_mapping_mut() {
        map.insert(serde_yaml::Value::String(last.into()), yaml_value);
    }
    write_yaml_atomic(&paths.config, &doc)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() -> (tempfile::TempDir, OmpRuntimePaths) {
        let dir = tempfile::tempdir().unwrap();
        let paths = OmpRuntimePaths {
            root: dir.path().join("omp"),
            models: dir.path().join("omp").join("models.yml"),
            config: dir.path().join("omp").join("config.yml"),
            mcp: dir.path().join("omp").join("mcp.json"),
            prompt: dir.path().join("omp").join("AGENTS.md"),
        };
        std::fs::create_dir_all(&paths.root).unwrap();
        (dir, paths)
    }

    #[test]
    fn models_roundtrip_preserves_others() {
        let (_dir, paths) = setup();
        std::fs::write(
            &paths.models,
            "other-provider:\n  name: Keep\nalpha:\n  name: Alpha\n  baseUrl: https://a/v1\n",
        )
        .unwrap();

        let mut providers = vec![];
        assert_eq!(import_runtime(&paths, &mut providers).unwrap(), 2);
        let alpha = providers.iter().find(|p| p.id == "omp:alpha").unwrap();
        assert_eq!(alpha.name, "Alpha");

        let mut rec = alpha.clone();
        rec.set_settings(&serde_json::json!({"name": "Alpha", "baseUrl": "https://b/v2"}));
        apply_provider(&paths, &rec).unwrap();

        let out = std::fs::read_to_string(&paths.models).unwrap();
        assert!(out.contains("other-provider:"), "other entry lost");
        assert!(out.contains("name: Keep"));
        assert!(out.contains("baseUrl: https://b/v2"));
    }

    #[test]
    fn config_dotted_keys() {
        let (_dir, paths) = setup();
        std::fs::write(&paths.config, "ui:\n  theme: dark\n").unwrap();

        assert_eq!(config_get(&paths, "ui.theme").unwrap().unwrap(), "dark");
        config_set(&paths, "ui.theme", &serde_json::json!("light")).unwrap();
        assert_eq!(config_get(&paths, "ui.theme").unwrap().unwrap(), "light");

        // create nested keys that don't exist yet
        config_set(&paths, "models.default", &serde_json::json!("alpha")).unwrap();
        assert_eq!(
            config_get(&paths, "models.default").unwrap().unwrap(),
            "alpha"
        );
        // original key survives
        assert_eq!(config_get(&paths, "ui.theme").unwrap().unwrap(), "light");

        assert_eq!(config_get(&paths, "nope.missing").unwrap(), None);
    }

    #[test]
    fn common_settings_merge_preserves_existing() {
        let (_dir, paths) = setup();
        std::fs::write(&paths.config, "ui:\n  theme: dark\nkeep: true\n").unwrap();
        merge_other_settings(
            &paths,
            &serde_json::json!({"ui":{"theme":"light","density":"compact"}}),
        )
        .unwrap();
        let value = read_yaml(&paths.config).unwrap();
        assert_eq!(value["ui"]["theme"].as_str(), Some("light"));
        assert_eq!(value["ui"]["density"].as_str(), Some("compact"));
        assert_eq!(value["keep"].as_bool(), Some(true));
    }

    #[test]
    fn empty_files_import_zero() {
        let (_dir, paths) = setup();
        let mut providers = vec![];
        assert_eq!(import_runtime(&paths, &mut providers).unwrap(), 0);
    }
}
