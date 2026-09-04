//! OpenCode companion configs: Oh My OpenAgent and Oh My OpenCode Slim.
//!
//! Both are runtime-file backed profile systems. This module provides the
//! local bridge state, profile CRUD and apply/clear semantics; the app store
//! persists profile records while files remain the runtime authority.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::paths::Paths;
use crate::tools::ToolId;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AddonKind {
    OhMyOpenAgent,
    OhMyOpenCodeSlim,
}

impl AddonKind {
    pub fn key(self) -> &'static str {
        match self {
            Self::OhMyOpenAgent => "oh_my_openagent",
            Self::OhMyOpenCodeSlim => "oh_my_opencode_slim",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddonProfile {
    pub id: String,
    pub name: String,
    pub kind: AddonKind,
    pub config: Value,
    #[serde(default)]
    pub is_applied: bool,
    #[serde(default)]
    pub is_disabled: bool,
    #[serde(default)]
    pub sort_index: i32,
    pub created_at: String,
    pub updated_at: String,
}

impl AddonProfile {
    pub fn new(name: impl Into<String>, kind: AddonKind, config: Value) -> Self {
        let now = chrono::Utc::now().to_rfc3339();
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            name: name.into(),
            kind,
            config,
            is_applied: false,
            is_disabled: false,
            sort_index: 0,
            created_at: now.clone(),
            updated_at: now,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct OpenCodeAddonStore {
    #[serde(default)]
    pub profiles: Vec<AddonProfile>,
    #[serde(default)]
    pub global_configs: std::collections::BTreeMap<String, Value>,
}

pub fn config_path(paths: &Paths, kind: AddonKind) -> PathBuf {
    let directory = paths.tool_root(ToolId::OpenCode);
    match kind {
        AddonKind::OhMyOpenAgent => {
            if std::env::var("AITOOLPLUS_OMO_LEGACY").ok().as_deref() == Some("1") {
                let jsonc = directory.join("oh-my-openagent.jsonc");
                let json = directory.join("oh-my-openagent.json");
                if jsonc.exists() || !json.exists() {
                    jsonc
                } else {
                    json
                }
            } else {
                paths.home.join(".omo").join("omo.jsonc")
            }
        }
        AddonKind::OhMyOpenCodeSlim => directory.join("oh-my-opencode-slim.json"),
    }
}

fn unified_omo() -> bool {
    std::env::var("AITOOLPLUS_OMO_LEGACY").ok().as_deref() != Some("1")
}

pub fn load_local(paths: &Paths, kind: AddonKind) -> Result<Option<AddonProfile>, String> {
    let path = config_path(paths, kind);
    if !path.is_file() {
        return Ok(None);
    }
    let raw = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let document = parse_jsonc(&raw)?;
    let config = if kind == AddonKind::OhMyOpenAgent && unified_omo() {
        document
            .get("opencode")
            .cloned()
            .unwrap_or_else(|| serde_json::json!({}))
    } else {
        document
    };
    Ok(Some(AddonProfile {
        id: "__local__".into(),
        name: "本地配置 / Local Config".into(),
        kind,
        config,
        is_applied: true,
        is_disabled: false,
        sort_index: -1,
        created_at: String::new(),
        updated_at: String::new(),
    }))
}

pub fn list(store: &OpenCodeAddonStore, kind: AddonKind) -> Vec<&AddonProfile> {
    let mut profiles: Vec<_> = store
        .profiles
        .iter()
        .filter(|profile| profile.kind == kind)
        .collect();
    profiles.sort_by_key(|profile| (profile.sort_index, profile.created_at.clone()));
    profiles
}

pub fn upsert(store: &mut OpenCodeAddonStore, mut profile: AddonProfile) {
    profile.updated_at = chrono::Utc::now().to_rfc3339();
    if let Some(existing) = store.profiles.iter_mut().find(|item| item.id == profile.id) {
        *existing = profile;
    } else {
        profile.sort_index = store
            .profiles
            .iter()
            .filter(|item| item.kind == profile.kind)
            .map(|item| item.sort_index)
            .max()
            .unwrap_or(-1)
            + 1;
        store.profiles.push(profile);
    }
}

pub fn delete(store: &mut OpenCodeAddonStore, id: &str) -> bool {
    let before = store.profiles.len();
    store
        .profiles
        .retain(|profile| profile.id != id || profile.id == "__local__");
    before != store.profiles.len()
}

pub fn apply(paths: &Paths, store: &mut OpenCodeAddonStore, id: &str) -> Result<PathBuf, String> {
    let profile = store
        .profiles
        .iter()
        .find(|profile| profile.id == id)
        .cloned()
        .ok_or("add-on profile not found")?;
    if profile.id == "__local__" {
        return Err("local bridge profile must be saved before apply".into());
    }
    let global = store
        .global_configs
        .get(profile.kind.key())
        .cloned()
        .unwrap_or_else(|| serde_json::json!({}));
    let mut output = global;
    deep_merge(&mut output, &profile.config);
    if profile.kind == AddonKind::OhMyOpenCodeSlim {
        normalize_slim(&mut output);
    }
    if profile.kind == AddonKind::OhMyOpenAgent
        && std::env::var("AITOOLPLUS_OMO_DUAL_REASONING_VARIANT")
            .ok()
            .as_deref()
            == Some("1")
    {
        dual_write_reasoning_variant(&mut output);
    }
    let path = config_path(paths, profile.kind);
    if profile.kind == AddonKind::OhMyOpenAgent && unified_omo() {
        let mut document = if path.exists() {
            parse_jsonc(&std::fs::read_to_string(&path).map_err(|e| e.to_string())?)?
        } else {
            serde_json::json!({})
        };
        document["opencode"] = output;
        write_json_atomic(&path, &document)?;
    } else {
        write_json_atomic(&path, &output)?;
    }
    for item in &mut store.profiles {
        if item.kind == profile.kind {
            item.is_applied = item.id == id;
        }
    }
    Ok(path)
}

pub fn clear_applied(
    paths: &Paths,
    store: &mut OpenCodeAddonStore,
    kind: AddonKind,
) -> Result<(), String> {
    let path = config_path(paths, kind);
    if path.exists() {
        if kind == AddonKind::OhMyOpenAgent && unified_omo() {
            let mut document =
                parse_jsonc(&std::fs::read_to_string(&path).map_err(|e| e.to_string())?)?;
            if let Some(object) = document.as_object_mut() {
                object.remove("opencode");
            }
            write_json_atomic(&path, &document)?;
        } else {
            std::fs::remove_file(path).map_err(|e| e.to_string())?;
        }
    }
    for profile in &mut store.profiles {
        if profile.kind == kind {
            profile.is_applied = false;
        }
    }
    Ok(())
}

/// OMOS v2 does not accept `fallback.chains`; remove it from runtime output.
/// Existing agent model arrays and unknown fields are preserved.
fn dual_write_reasoning_variant(config: &mut Value) {
    fn visit(value: &mut Value) {
        match value {
            Value::Object(object) => {
                if let Some(reasoning) = object.get("reasoning").cloned() {
                    object.entry("variant").or_insert(reasoning);
                }
                for value in object.values_mut() {
                    visit(value);
                }
            }
            Value::Array(array) => {
                for value in array {
                    visit(value);
                }
            }
            _ => {}
        }
    }
    visit(config);
}

pub fn normalize_slim(config: &mut Value) {
    if let Some(fallback) = config.get_mut("fallback").and_then(Value::as_object_mut) {
        fallback.remove("chains");
        // legacy council master fields are not accepted by v2
        for key in ["master", "master_timeout", "master_fallback"] {
            fallback.remove(key);
        }
        if fallback.is_empty() {
            config
                .as_object_mut()
                .map(|object| object.remove("fallback"));
        }
    }
    if let Some(council) = config.get_mut("council").and_then(Value::as_object_mut) {
        for key in ["master", "master_timeout", "master_fallback"] {
            council.remove(key);
        }
    }
}

fn deep_merge(base: &mut Value, overlay: &Value) {
    match (base, overlay) {
        (Value::Object(base), Value::Object(overlay)) => {
            for (key, value) in overlay {
                match base.get_mut(key) {
                    Some(current) => deep_merge(current, value),
                    None => {
                        base.insert(key.clone(), value.clone());
                    }
                }
            }
        }
        (base, overlay) => *base = overlay.clone(),
    }
}

fn parse_jsonc(raw: &str) -> Result<Value, String> {
    let mut output = String::with_capacity(raw.len());
    let mut chars = raw.chars().peekable();
    let mut string = false;
    while let Some(character) = chars.next() {
        if string {
            output.push(character);
            if character == '\\' {
                if let Some(next) = chars.next() {
                    output.push(next);
                }
            } else if character == '"' {
                string = false;
            }
            continue;
        }
        match character {
            '"' => {
                string = true;
                output.push(character);
            }
            '/' if chars.peek() == Some(&'/') => {
                chars.next();
                for next in chars.by_ref() {
                    if next == '\n' {
                        output.push('\n');
                        break;
                    }
                }
            }
            '/' if chars.peek() == Some(&'*') => {
                chars.next();
                let mut previous = '\0';
                for next in chars.by_ref() {
                    if previous == '*' && next == '/' {
                        break;
                    }
                    previous = next;
                }
            }
            _ => output.push(character),
        }
    }
    serde_json::from_str(&output).map_err(|e| e.to_string())
}

fn write_json_atomic(path: &Path, value: &Value) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let temp = path.with_extension("tmp");
    std::fs::write(
        &temp,
        serde_json::to_string_pretty(value).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    std::fs::rename(temp, path).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_bridge_and_profile_apply() {
        let directory = tempfile::tempdir().unwrap();
        let paths = Paths::new(directory.path().join("home"), directory.path().join("data"));
        let file = config_path(&paths, AddonKind::OhMyOpenAgent);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(
            &file,
            "// comment\n{\"otherHost\":{\"keep\":true},\"opencode\":{\"agents\":{\"coder\":{\"model\":\"m1\"}}}}",
        )
        .unwrap();
        let local = load_local(&paths, AddonKind::OhMyOpenAgent)
            .unwrap()
            .unwrap();
        assert_eq!(local.id, "__local__");
        assert_eq!(local.config["agents"]["coder"]["model"], "m1");

        let mut store = OpenCodeAddonStore::default();
        let profile = AddonProfile::new(
            "P1",
            AddonKind::OhMyOpenAgent,
            serde_json::json!({"agents":{"coder":{"model":"m2"}}}),
        );
        let id = profile.id.clone();
        upsert(&mut store, profile);
        apply(&paths, &mut store, &id).unwrap();
        let output = parse_jsonc(&std::fs::read_to_string(file).unwrap()).unwrap();
        assert_eq!(output["opencode"]["agents"]["coder"]["model"], "m2");
        assert_eq!(output["otherHost"]["keep"], true);
        assert!(store.profiles[0].is_applied);
    }

    #[test]
    fn slim_runtime_normalization() {
        let mut config = serde_json::json!({
            "agents":{"coder":{"model":["m1","m2"],"advanced":true}},
            "fallback":{"chains":{"coder":["m3"]},"keep":1},
            "council":{"master":"m1","master_timeout":30,"unknown":true}
        });
        normalize_slim(&mut config);
        assert!(config["fallback"].get("chains").is_none());
        assert_eq!(config["fallback"]["keep"], 1);
        assert!(config["council"].get("master").is_none());
        assert_eq!(config["council"]["unknown"], true);
        assert_eq!(config["agents"]["coder"]["model"][1], "m2");
    }
}
