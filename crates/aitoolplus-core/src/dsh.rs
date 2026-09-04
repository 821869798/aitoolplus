//! DeepSeek Harness (dsh) runtime adapter.
//!
//! Schema (from ai-toolbox `coding/dsh/AGENTS.md`):
//! - root: DB custom > `DSH_HOME` env > platform default `~/.dsh`
//! - `settings.yaml`: namespaced stack config
//!   - `llm-pi-ai.providers.<route>`: provider dict (key = route id)
//!   - `agent-default-model`: `{ provider, model, reasoningEffort }`
//!   - unknown sections preserved
//! - `.credentials.yaml`: versioned layout `version: 1` + `refs: { REF: secret }`
//!   + `records:` (dsh login flow owned, never written by us)
//! - prompt: `<root>/AGENTS.md`
//! - providers store `apiKeyEnv` refs; secrets live in `refs:`

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::paths::Paths;
use crate::providers::ProviderRecord;
use crate::tools::ToolId;

pub struct DshRuntimePaths {
    pub root: PathBuf,
    pub settings: PathBuf,
    pub credentials: PathBuf,
    pub prompt: PathBuf,
}

impl DshRuntimePaths {
    pub fn from_paths(paths: &Paths) -> Self {
        let root = dsh_root(paths);
        Self {
            settings: root.join("settings.yaml"),
            credentials: root.join(".credentials.yaml"),
            prompt: root.join("AGENTS.md"),
            root,
        }
    }
}

/// Root resolution: env override > platform default. (DB custom handled by
/// caller; `runtime_location`-style sources are out of scope for v1.)
pub fn dsh_root(paths: &Paths) -> PathBuf {
    if let Some(dir) = paths.tool_roots.get(&ToolId::Dsh) {
        return dir.clone();
    }
    if let Ok(env) = std::env::var("DSH_HOME")
        && !env.is_empty()
    {
        return PathBuf::from(env);
    }
    paths.home.join(".dsh")
}

/// A parsed settings.yaml managed section.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct DshSettings {
    /// provider route id -> provider object
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub providers: Option<serde_yaml::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_model: Option<serde_yaml::Value>,
}

/// Read raw settings.yaml text (missing -> empty).
pub fn read_settings(paths: &DshRuntimePaths) -> Result<String, String> {
    if !paths.settings.exists() {
        return Ok(String::new());
    }
    std::fs::read_to_string(&paths.settings)
        .map_err(|e| format!("read {}: {e}", paths.settings.display()))
}

/// Import providers from `llm-pi-ai.providers.<route>` into UI records.
pub fn import_runtime(
    paths: &DshRuntimePaths,
    providers: &mut Vec<ProviderRecord>,
) -> Result<usize, String> {
    let raw = read_settings(paths)?;
    if raw.trim().is_empty() {
        return Ok(0);
    }
    let doc: serde_yaml::Value =
        serde_yaml::from_str(&raw).map_err(|e| format!("parse settings.yaml: {e}"))?;

    let Some(provider_map) = doc
        .get("llm-pi-ai")
        .and_then(|v| v.get("providers"))
        .and_then(|v| v.as_mapping())
    else {
        return Ok(0);
    };

    let default_route = doc
        .get("agent-default-model")
        .and_then(|v| v.get("provider"))
        .and_then(|v| v.as_str())
        .map(str::to_string);

    let mut added = 0;
    let mut next = providers.iter().map(|p| p.sort_index).max().unwrap_or(-1);
    for (key, value) in provider_map {
        let Some(key) = key.as_str() else { continue };
        let json = yaml_to_json(value)?;
        let name = json
            .get("name")
            .or_else(|| json.get("displayName"))
            .and_then(Value::as_str)
            .unwrap_or(key)
            .to_string();

        let stable_id = format!("dsh:{key}");
        if let Some(record) = providers.iter_mut().find(|p| p.id == stable_id) {
            record.name = name;
            record.settings_config = serde_json::to_string_pretty(&json).unwrap_or_default();
            record.is_applied = default_route.as_deref() == Some(key);
            record.touch();
            continue;
        }
        next += 1;
        let mut record = ProviderRecord::new(name, "custom");
        record.id = stable_id;
        record.sort_index = next;
        record.is_applied = default_route.as_deref() == Some(key);
        record.settings_config = serde_json::to_string_pretty(&json).unwrap_or_default();
        providers.push(record);
        added += 1;
    }
    Ok(added)
}

/// Apply one provider record: upsert `llm-pi-ai.providers.<route>`, write
/// `agent-default-model` from the record, preserve unknown sections.
pub fn apply_provider(
    paths: &DshRuntimePaths,
    provider: &ProviderRecord,
) -> Result<Vec<PathBuf>, String> {
    let route = provider
        .id
        .strip_prefix("dsh:")
        .ok_or("dsh provider record must carry route id")?;

    let raw = read_settings(paths)?;
    let mut doc: serde_yaml::Value = if raw.trim().is_empty() {
        serde_yaml::Value::Mapping(Default::default())
    } else {
        serde_yaml::from_str(&raw).map_err(|e| format!("parse settings.yaml: {e}"))?
    };

    // Upsert provider under llm-pi-ai.providers.<route>.
    let provider_yaml =
        json_to_yaml(&serde_json::from_str(&provider.settings_config).unwrap_or_default())?;
    if !matches!(doc, serde_yaml::Value::Mapping(_)) {
        return Err("settings.yaml root must be a mapping".into());
    }
    if let Some(map) = doc.as_mapping_mut() {
        let llm = map
            .entry(serde_yaml::Value::String("llm-pi-ai".into()))
            .or_insert(serde_yaml::Value::Mapping(Default::default()));
        if let Some(llm_map) = llm.as_mapping_mut() {
            let providers = llm_map
                .entry(serde_yaml::Value::String("providers".into()))
                .or_insert(serde_yaml::Value::Mapping(Default::default()));
            if let Some(pm) = providers.as_mapping_mut() {
                pm.insert(
                    serde_yaml::Value::String(route.into()),
                    provider_yaml.clone(),
                );
            }
        }

        // Default model block: { provider, model, reasoningEffort? }.
        let mut default = serde_yaml::Value::Mapping(Default::default());
        if let Some(m) = default.as_mapping_mut() {
            m.insert(
                serde_yaml::Value::String("provider".into()),
                serde_yaml::Value::String(route.into()),
            );
            if let Some(model) = provider
                .settings()
                .get("defaultModel")
                .and_then(Value::as_str)
            {
                m.insert(
                    serde_yaml::Value::String("model".into()),
                    serde_yaml::Value::String(model.into()),
                );
            }
            if let Some(effort) = provider
                .settings()
                .get("reasoningEffort")
                .and_then(Value::as_str)
                && !effort.is_empty()
            {
                m.insert(
                    serde_yaml::Value::String("reasoningEffort".into()),
                    serde_yaml::Value::String(effort.into()),
                );
            }
        }
        map.insert(
            serde_yaml::Value::String("agent-default-model".into()),
            default,
        );
    }

    write_atomic_text(
        &paths.settings,
        &serde_yaml::to_string(&doc).map_err(|e| e.to_string())?,
    )?;

    // Credentials: write `refs.<REF>` when apiKeyEnv + apiKey supplied.
    let mut files = vec![paths.settings.clone()];
    let settings = provider.settings();
    if let Some(api_key_env) = settings.get("apiKeyEnv").and_then(Value::as_str)
        && let Some(api_key) = settings.get("apiKey").and_then(Value::as_str)
    {
        upsert_credential_ref(paths, api_key_env, api_key)?;
        files.push(paths.credentials.clone());
    }
    Ok(files)
}

/// Merge top-level common/other settings into settings.yaml. Provider and
/// credential namespaces are preserved unless explicitly overlaid.
pub fn merge_other_settings(
    paths: &DshRuntimePaths,
    settings: &serde_json::Value,
) -> Result<(), String> {
    if !settings.is_object() {
        return Err("DSH common config must be a JSON object".into());
    }
    let raw = read_settings(paths)?;
    let mut current: serde_yaml::Value = if raw.trim().is_empty() {
        serde_yaml::Value::Mapping(Default::default())
    } else {
        serde_yaml::from_str(&raw).map_err(|error| error.to_string())?
    };
    let overlay = serde_yaml::to_value(settings).map_err(|error| error.to_string())?;
    merge_yaml(&mut current, &overlay);
    write_atomic_text(
        &paths.settings,
        &serde_yaml::to_string(&current).map_err(|error| error.to_string())?,
    )
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

/// Versioned credentials document: always emits `version: 1`, preserves
/// `records:`, migrates flat layouts into `refs:`.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct CredentialsDocument {
    #[serde(default)]
    pub version: Option<u32>,
    #[serde(default)]
    pub refs: serde_yaml::Value,
    #[serde(default)]
    pub records: serde_yaml::Value,
}

fn upsert_credential_ref(
    paths: &DshRuntimePaths,
    env_name: &str,
    secret: &str,
) -> Result<(), String> {
    let raw = if paths.credentials.exists() {
        std::fs::read_to_string(&paths.credentials)
            .map_err(|e| format!("read {}: {e}", paths.credentials.display()))?
    } else {
        String::new()
    };

    let mut doc: serde_yaml::Value = if raw.trim().is_empty() {
        flat_versioned_doc()
    } else {
        let parsed: serde_yaml::Value =
            serde_yaml::from_str(&raw).map_err(|e| format!("parse credentials: {e}"))?;
        normalize_credentials(parsed)
    };

    // Ensure refs mapping exists and set the entry.
    if let Some(map) = doc.as_mapping_mut() {
        if map
            .get(serde_yaml::Value::String("version".into()))
            .is_none()
        {
            map.insert(
                serde_yaml::Value::String("version".into()),
                serde_yaml::Value::Number(1.into()),
            );
        }
        let refs = map
            .entry(serde_yaml::Value::String("refs".into()))
            .or_insert_with(|| serde_yaml::Value::Mapping(Default::default()));
        if let Some(refs_map) = refs.as_mapping_mut() {
            refs_map.insert(
                serde_yaml::Value::String(env_name.into()),
                serde_yaml::Value::String(secret.into()),
            );
        }
    }

    write_atomic_text(
        &paths.credentials,
        &serde_yaml::to_string(&doc).map_err(|e| e.to_string())?,
    )?;
    restrict_permissions(&paths.credentials);
    Ok(())
}

/// Old flat docs (`REF: secret` at top level) migrate into `refs:`; any
/// `records:` content is preserved untouched.
fn normalize_credentials(parsed: serde_yaml::Value) -> serde_yaml::Value {
    if !matches!(parsed, serde_yaml::Value::Mapping(_)) {
        return flat_versioned_doc();
    }
    let mut out = serde_yaml::Value::Mapping(Default::default());
    let mut refs = serde_yaml::Value::Mapping(Default::default());
    let mut records = serde_yaml::Value::Mapping(Default::default());
    if let Some(map) = parsed.as_mapping() {
        for (k, v) in map {
            match k.as_str() {
                Some("version") => {
                    if let Some(o) = out.as_mapping_mut() {
                        o.insert(k.clone(), v.clone());
                    }
                }
                Some("refs") => refs = v.clone(),
                Some("records") => records = v.clone(),
                _ => {
                    if let Some(rm) = refs.as_mapping_mut() {
                        rm.insert(k.clone(), v.clone());
                    }
                }
            }
        }
    }
    if let Some(o) = out.as_mapping_mut() {
        o.insert(
            serde_yaml::Value::String("version".into()),
            serde_yaml::Value::Number(1.into()),
        );
        o.insert(serde_yaml::Value::String("refs".into()), refs);
        o.insert(serde_yaml::Value::String("records".into()), records);
    }
    out
}

fn flat_versioned_doc() -> serde_yaml::Value {
    let mut doc = serde_yaml::Value::Mapping(Default::default());
    if let Some(m) = doc.as_mapping_mut() {
        m.insert(
            serde_yaml::Value::String("version".into()),
            serde_yaml::Value::Number(1.into()),
        );
        m.insert(
            serde_yaml::Value::String("refs".into()),
            serde_yaml::Value::Mapping(Default::default()),
        );
    }
    doc
}

fn restrict_permissions(path: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(md) = std::fs::metadata(path) {
            let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
            let _ = md;
        }
    }
    #[cfg(not(unix))]
    {
        let _ = path;
    }
}

fn write_atomic_text(path: &Path, contents: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, contents).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, path).map_err(|e| e.to_string())?;
    Ok(())
}

// YAML <-> JSON conversions through serde_yaml::Value <-> serde_json::Value.
fn yaml_to_json(y: &serde_yaml::Value) -> Result<Value, String> {
    serde_json::to_value(y).map_err(|e| e.to_string())
}

fn json_to_yaml(j: &Value) -> Result<serde_yaml::Value, String> {
    serde_yaml::to_value(j).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() -> (tempfile::TempDir, DshRuntimePaths) {
        let dir = tempfile::tempdir().unwrap();
        let paths = DshRuntimePaths {
            root: dir.path().join(".dsh"),
            settings: dir.path().join(".dsh").join("settings.yaml"),
            credentials: dir.path().join(".dsh").join(".credentials.yaml"),
            prompt: dir.path().join(".dsh").join("AGENTS.md"),
        };
        std::fs::create_dir_all(&paths.root).unwrap();
        (dir, paths)
    }

    #[test]
    fn import_and_apply_roundtrip_preserves_unknown_sections() {
        let (_dir, paths) = setup();
        std::fs::write(
            &paths.settings,
            "unknownTop: keep-me\nllm-pi-ai:\n  providers:\n    deepseek:\n      name: DeepSeek\n      baseUrl: https://api.deepseek.com\nagent-default-model:\n  provider: deepseek\n  model: deepseek-chat\n",
        )
        .unwrap();

        let mut providers = vec![];
        assert_eq!(import_runtime(&paths, &mut providers).unwrap(), 1);
        let p = &providers[0];
        assert_eq!(p.id, "dsh:deepseek");
        assert_eq!(p.name, "DeepSeek");
        assert_eq!(p.settings()["baseUrl"], "https://api.deepseek.com");
        assert!(p.is_applied);

        // Apply an updated record with a new default model.
        let mut rec = p.clone();
        rec.set_settings(&serde_json::json!({
            "name": "DeepSeek",
            "baseUrl": "https://api.deepseek.com",
            "apiKeyEnv": "DEEPSEEK_API_KEY",
            "apiKey": "sk-1",
            "defaultModel": "deepseek-reasoner",
            "reasoningEffort": "high"
        }));
        let files = apply_provider(&paths, &rec).unwrap();
        assert_eq!(files.len(), 2);

        let out = std::fs::read_to_string(&paths.settings).unwrap();
        assert!(out.contains("unknownTop: keep-me"), "unknown section lost");
        assert!(out.contains("provider: deepseek"));
        assert!(out.contains("model: deepseek-reasoner"));
        assert!(out.contains("reasoningEffort: high"));
        assert!(out.contains("baseUrl: https://api.deepseek.com"));

        let creds = std::fs::read_to_string(&paths.credentials).unwrap();
        assert!(creds.contains("version: 1"));
        assert!(creds.contains("DEEPSEEK_API_KEY: sk-1"));
    }

    #[test]
    fn flat_credentials_migrate_to_versioned() {
        let (_dir, paths) = setup();
        std::fs::write(&paths.credentials, "OLD_KEY: old-secret\n").unwrap();

        let mut rec = ProviderRecord::new("X", "custom");
        rec.id = "dsh:x".into();
        rec.set_settings(&serde_json::json!({
            "apiKeyEnv": "NEW_KEY", "apiKey": "s1", "defaultModel": "m"
        }));
        apply_provider(&paths, &rec).unwrap();

        let creds = std::fs::read_to_string(&paths.credentials).unwrap();
        assert!(creds.contains("version: 1"), "no version stamp: {creds}");
        assert!(
            creds.contains("OLD_KEY: old-secret"),
            "migration lost old entry"
        );
        assert!(creds.contains("NEW_KEY: s1"));
    }

    #[test]
    fn missing_settings_imports_zero() {
        let (_dir, paths) = setup();
        let mut providers = vec![];
        assert_eq!(import_runtime(&paths, &mut providers).unwrap(), 0);
    }
}
