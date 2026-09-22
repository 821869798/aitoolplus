//! Claude Desktop 3P (third-party) gateway profile writer.
//!
//! Schema (from ai-toolbox `coding/claude_desktop/AGENTS.md`):
//! - `claude_desktop_config.json`: live config carrying the applied profile
//! - `configLibrary/<PROFILE_ID>.json`: persisted profiles
//! - `configLibrary/_meta.json`: `{ appliedId: "<profile id>" }`
//! - Applied state reads ONLY `_meta.appliedId` + profile's
//!   `inferenceGatewayBaseUrl` / `inferenceModels` (not `deploymentMode`)
//! - Official restore: clear appliedId and remove 3P-managed keys from the
//!   live config, leaving Claude Desktop on official endpoints.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::paths::Paths;
use crate::providers::ProviderRecord;

pub struct ClaudeDesktopPaths {
    pub root: PathBuf,
    pub live_config: PathBuf,
    pub config_library: PathBuf,
    pub meta: PathBuf,
}

impl ClaudeDesktopPaths {
    pub fn from_paths(paths: &Paths) -> Self {
        let root = claude_desktop_root(paths);
        let library = root.join("configLibrary");
        Self {
            live_config: root.join("claude_desktop_config.json"),
            config_library: library.clone(),
            meta: library.join("_meta.json"),
            root,
        }
    }
}

/// Windows: `%APPDATA%\Claude`; macOS: `~/Library/Application Support/Claude`;
/// Linux: `~/.config/Claude`.
pub fn claude_desktop_root(paths: &Paths) -> PathBuf {
    if let Some(dir) = paths.tool_roots.get(&crate::tools::ToolId::ClaudeDesktop) {
        return dir.clone();
    }
    if let Ok(env) = std::env::var("CLAUDE_DESKTOP_ROOT")
        && !env.is_empty()
    {
        return PathBuf::from(env);
    }
    #[cfg(target_os = "windows")]
    {
        if let Some(appdata) = std::env::var_os("APPDATA") {
            let p = PathBuf::from(appdata).join("Claude");
            if p.is_absolute() {
                return p;
            }
        }
        paths.home.join("AppData").join("Roaming").join("Claude")
    }
    #[cfg(target_os = "macos")]
    {
        return paths
            .home
            .join("Library")
            .join("Application Support")
            .join("Claude");
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        return paths.home.join(".config").join("Claude");
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct LibraryMeta {
    #[serde(default, rename = "appliedId", skip_serializing_if = "Option::is_none")]
    pub applied_id: Option<String>,
}

/// Keys this app manages on the live config + profiles.
pub const GATEWAY_KEYS: [&str; 3] = [
    "inferenceGatewayBaseUrl",
    "inferenceModels",
    "deploymentMode",
];

fn read_json_object(path: &PathBuf) -> Result<Value, String> {
    if !path.exists() {
        return Ok(Value::Object(Default::default()));
    }
    let raw = std::fs::read_to_string(path).map_err(|e| format!("read {}: {e}", path.display()))?;
    if raw.trim().is_empty() {
        return Ok(Value::Object(Default::default()));
    }
    serde_json::from_str(&raw).map_err(|e| format!("parse {}: {e}", path.display()))
}

fn write_json_atomic(path: &PathBuf, value: &Value) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let tmp = path.with_extension("tmp");
    let text = serde_json::to_string_pretty(value).map_err(|e| e.to_string())?;
    std::fs::write(&tmp, &text).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, path).map_err(|e| e.to_string())?;
    Ok(())
}

/// Import all `configLibrary/*.json` profiles (excluding `_meta.json`).
pub fn import_runtime(
    paths: &ClaudeDesktopPaths,
    providers: &mut Vec<ProviderRecord>,
) -> Result<usize, String> {
    std::fs::create_dir_all(&paths.config_library).map_err(|e| e.to_string())?;
    let meta: LibraryMeta = read_json_object(&paths.meta)
        .and_then(|v| serde_json::from_value(v).map_err(|e| e.to_string()))?;

    let mut added = 0;
    let mut next = providers.iter().map(|p| p.sort_index).max().unwrap_or(-1);
    for entry in std::fs::read_dir(&paths.config_library)
        .map_err(|e| e.to_string())?
        .filter_map(|e| e.ok())
    {
        let path = entry.path();
        let Some(name) = path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        if name == "_meta" {
            continue;
        }
        let profile = read_json_object(&path)?;
        let display = profile
            .get("profileName")
            .and_then(Value::as_str)
            .unwrap_or(name)
            .to_string();

        let stable_id = format!("cd:{name}");
        let is_applied = meta.applied_id.as_deref() == Some(name);
        if let Some(record) = providers.iter_mut().find(|p| p.id == stable_id) {
            record.name = display;
            record.settings_config = serde_json::to_string_pretty(&profile).unwrap_or_default();
            record.is_applied = is_applied;
            record.touch();
            continue;
        }
        next += 1;
        let mut record = ProviderRecord::new(display, "custom");
        record.id = stable_id;
        record.sort_index = next;
        record.is_applied = is_applied;
        record.settings_config = serde_json::to_string_pretty(&profile).unwrap_or_default();
        providers.push(record);
        added += 1;
    }
    Ok(added)
}

/// Save a profile into `configLibrary/<id>.json`.
pub fn save_profile(paths: &ClaudeDesktopPaths, provider: &ProviderRecord) -> Result<(), String> {
    let profile_id = provider
        .id
        .strip_prefix("cd:")
        .unwrap_or(&provider.id)
        .to_string();
    let mut profile: Value = serde_json::from_str(&provider.settings_config)
        .map_err(|e| format!("invalid profile JSON: {e}"))?;
    if let Some(obj) = profile.as_object_mut() {
        obj.insert("profileName".into(), Value::String(provider.name.clone()));
    }
    write_json_atomic(
        &paths.config_library.join(format!("{profile_id}.json")),
        &profile,
    )
}

/// Apply a profile: write the live config's managed keys + `_meta.appliedId`.
/// Non-managed sections of the live config are preserved.
pub fn apply_profile(
    paths: &ClaudeDesktopPaths,
    provider: &ProviderRecord,
) -> Result<Vec<PathBuf>, String> {
    let profile_id = provider
        .id
        .strip_prefix("cd:")
        .unwrap_or(&provider.id)
        .to_string();
    save_profile(paths, provider)?;

    let profile: Value = serde_json::from_str(&provider.settings_config)
        .map_err(|e| format!("invalid profile JSON: {e}"))?;

    // Live config: strip previous 3P keys, then write the new ones.
    let mut live = read_json_object(&paths.live_config)?;
    if let Some(obj) = live.as_object_mut() {
        for key in GATEWAY_KEYS {
            obj.remove(key);
        }
        for (k, v) in profile.as_object().into_iter().flatten() {
            if GATEWAY_KEYS.contains(&k.as_str()) {
                obj.insert(k.clone(), v.clone());
            }
        }
    }
    write_json_atomic(&paths.live_config, &live)?;

    // Meta: mark applied.
    let meta = serde_json::json!({ "appliedId": profile_id });
    write_json_atomic(&paths.meta, &meta)?;

    Ok(vec![paths.live_config.clone(), paths.meta.clone()])
}

/// Official restore: clear `_meta.appliedId` and remove managed keys from
/// the live config so Claude Desktop talks to official endpoints again.
pub fn restore_official(paths: &ClaudeDesktopPaths) -> Result<Vec<PathBuf>, String> {
    let mut meta = read_json_object(&paths.meta)?;
    if let Some(obj) = meta.as_object_mut() {
        obj.remove("appliedId");
    }
    write_json_atomic(&paths.meta, &meta)?;

    let mut live = read_json_object(&paths.live_config)?;
    if let Some(obj) = live.as_object_mut() {
        for key in GATEWAY_KEYS {
            obj.remove(key);
        }
    }
    write_json_atomic(&paths.live_config, &live)?;
    Ok(vec![paths.meta.clone(), paths.live_config.clone()])
}

/// Read the effective live state for the Runtime tab.
pub fn read_state(paths: &ClaudeDesktopPaths) -> Result<Value, String> {
    let meta = read_json_object(&paths.meta)?;
    let live = read_json_object(&paths.live_config)?;
    let applied_id = meta.get("appliedId").and_then(Value::as_str);
    let profile = match applied_id {
        Some(id) => read_json_object(&paths.config_library.join(format!("{id}.json"))).ok(),
        None => None,
    };
    Ok(serde_json::json!({
        "appliedId": applied_id,
        "liveGatewayBaseUrl": live.get("inferenceGatewayBaseUrl"),
        "liveInferenceModels": live.get("inferenceModels"),
        "profileGatewayBaseUrl": profile.as_ref().and_then(|p| p.get("inferenceGatewayBaseUrl")),
        "profileInferenceModels": profile.as_ref().and_then(|p| p.get("inferenceModels")),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() -> (tempfile::TempDir, ClaudeDesktopPaths) {
        let dir = tempfile::tempdir().unwrap();
        let paths = ClaudeDesktopPaths {
            root: dir.path().join("Claude"),
            live_config: dir.path().join("Claude").join("claude_desktop_config.json"),
            config_library: dir.path().join("Claude").join("configLibrary"),
            meta: dir
                .path()
                .join("Claude")
                .join("configLibrary")
                .join("_meta.json"),
        };
        std::fs::create_dir_all(&paths.config_library).unwrap();
        (dir, paths)
    }

    #[test]
    fn profile_lifecycle_with_meta_and_restore() {
        let (_dir, paths) = setup();
        // existing live config with unrelated content
        std::fs::write(
            &paths.live_config,
            r#"{"mcpServers": {"fs": {"command": "npx"}}, "theme": "dark"}"#,
        )
        .unwrap();

        let mut p = ProviderRecord::new("My Gateway", "custom");
        p.id = "cd:my-gateway".into();
        p.set_settings(&serde_json::json!({
            "inferenceGatewayBaseUrl": "https://gw.example.com",
            "inferenceModels": ["m1", "m2"],
            "profileName": "My Gateway"
        }));

        let files = apply_profile(&paths, &p).unwrap();
        assert_eq!(files.len(), 2);

        // import sees exactly one applied profile
        let mut providers = vec![];
        assert_eq!(import_runtime(&paths, &mut providers).unwrap(), 1);
        assert_eq!(providers[0].id, "cd:my-gateway");
        assert!(providers[0].is_applied);

        // live config: managed keys written, unrelated kept
        let live: Value =
            serde_json::from_str(&std::fs::read_to_string(&paths.live_config).unwrap()).unwrap();
        assert_eq!(live["inferenceGatewayBaseUrl"], "https://gw.example.com");
        assert_eq!(live["inferenceModels"][0], "m1");
        assert_eq!(live["mcpServers"]["fs"]["command"], "npx");
        assert_eq!(live["theme"], "dark");

        // state view mirrors upstream semantics
        let state = read_state(&paths).unwrap();
        assert_eq!(state["appliedId"], "my-gateway");
        assert_eq!(state["liveGatewayBaseUrl"], "https://gw.example.com");

        // official restore clears everything managed
        restore_official(&paths).unwrap();
        let live: Value =
            serde_json::from_str(&std::fs::read_to_string(&paths.live_config).unwrap()).unwrap();
        assert!(live.get("inferenceGatewayBaseUrl").is_none());
        assert!(live.get("inferenceModels").is_none());
        assert!(live.get("deploymentMode").is_none());
        // unrelated content still present
        assert_eq!(live["mcpServers"]["fs"]["command"], "npx");
        let meta: Value =
            serde_json::from_str(&std::fs::read_to_string(&paths.meta).unwrap()).unwrap();
        assert!(meta.get("appliedId").is_none());
        // profile file itself survives restore
        assert!(paths.config_library.join("my-gateway.json").exists());
    }
}
