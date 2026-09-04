//! Claude Code plugin management (mirrors ai-toolbox
//! `coding/claude_code/plugin_state.rs` + `plugin_cli.rs`).
//!
//! Storage layout (all under the Claude root dir):
//! - `plugins/installed_plugins.json`: `{"plugins": {"<name>@<marketplace>": [{"scope": "user", "install_path": "...", "version": "..."}]}}`
//! - `plugins/known_marketplaces.json`: `{"<name>": {"source": ..., "autoUpdateEnabled": bool, ...}}`
//! - `settings.json.enabledPlugins`: `{"<name>@<marketplace>": bool}`
//! - plugin manifest: `<install>/.claude-plugin/plugin.json`
//!
//! CLI mutations run `claude plugin <sub> <args>` with CLAUDE_CONFIG_DIR set
//! (user scope); marketplace auto-update flags survive CLI rewrites.

use std::path::{Path, PathBuf};
use std::process::Command;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::paths::Paths;
use crate::tools::ToolId;

// ---------------------------------------------------------------------------
// Types (upstream parity)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KnownMarketplace {
    pub name: String,
    pub source: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub install_location: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_updated: Option<String>,
    #[serde(default)]
    pub auto_update_enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(default)]
    pub plugin_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MarketplacePlugin {
    pub marketplace_name: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub homepage: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repository: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    pub source: Value,
    pub plugin_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstalledPlugin {
    pub plugin_id: String,
    pub name: String,
    pub marketplace_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub install_path: Option<String>,
    #[serde(default)]
    pub user_scope_installed: bool,
    #[serde(default)]
    pub user_scope_enabled: bool,
    #[serde(default)]
    pub install_scopes: Vec<String>,
    #[serde(default)]
    pub has_skills: bool,
    #[serde(default)]
    pub has_agents: bool,
    #[serde(default)]
    pub has_hooks: bool,
    #[serde(default)]
    pub has_mcp_servers: bool,
    #[serde(default)]
    pub has_lsp_servers: bool,
}

#[derive(Debug, Deserialize, Default)]
struct InstalledPluginsFile {
    #[serde(default)]
    plugins: std::collections::HashMap<String, Vec<InstalledEntry>>,
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct InstalledEntry {
    #[serde(default)]
    scope: Option<String>,
    #[serde(default)]
    install_path: Option<String>,
    #[serde(default)]
    version: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
#[allow(dead_code)] // owner kept for manifest round-trip fidelity
struct MarketplaceManifest {
    #[serde(default)]
    owner: Option<Value>,
    #[serde(default)]
    metadata: Option<Value>,
    #[serde(default)]
    plugins: Vec<MarketplacePluginEntry>,
}

#[derive(Debug, Deserialize, Default)]
struct MarketplacePluginEntry {
    name: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    version: Option<String>,
    #[serde(default)]
    homepage: Option<String>,
    #[serde(default)]
    repository: Option<String>,
    #[serde(default)]
    category: Option<String>,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default)]
    source: Value,
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)] // homepage/repository kept for manifest round-trip fidelity
struct PluginManifest {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    version: Option<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    homepage: Option<String>,
    #[serde(default)]
    repository: Option<String>,
    #[serde(default)]
    hooks: Option<Value>,
    #[serde(default)]
    mcp_servers: Option<Value>,
    #[serde(default)]
    lsp_servers: Option<Value>,
    #[serde(default)]
    agents: Option<Value>,
}

// ---------------------------------------------------------------------------
// Paths
// ---------------------------------------------------------------------------

pub fn plugins_root(paths: &Paths) -> PathBuf {
    paths.tool_root(ToolId::ClaudeCode).join("plugins")
}

pub fn installed_plugins_path(paths: &Paths) -> PathBuf {
    plugins_root(paths).join("installed_plugins.json")
}

pub fn known_marketplaces_path(paths: &Paths) -> PathBuf {
    plugins_root(paths).join("known_marketplaces.json")
}

// ---------------------------------------------------------------------------
// Read state
// ---------------------------------------------------------------------------

fn read_json_or_default<T: for<'de> Deserialize<'de> + Default>(path: &Path) -> Result<T, String> {
    if !path.exists() {
        return Ok(T::default());
    }
    let raw = std::fs::read_to_string(path)
        .map_err(|e| format!("Failed to read {}: {e}", path.display()))?;
    serde_json::from_str(&raw).map_err(|e| format!("Failed to parse {}: {e}", path.display()))
}

/// Parse `plugin@marketplace` into (plugin, marketplace).
pub fn parse_plugin_id(plugin_id: &str) -> (String, String) {
    match plugin_id.rsplit_once('@') {
        Some((name, marketplace)) => (name.to_string(), marketplace.to_string()),
        None => (plugin_id.to_string(), String::new()),
    }
}

/// List marketplaces from `known_marketplaces.json` (plus manifest metadata
/// from the install location when present).
pub fn list_marketplaces(paths: &Paths) -> Result<Vec<KnownMarketplace>, String> {
    let file = known_marketplaces_path(paths);
    let raw: Value = read_json_or_default(&file)?;
    let Some(entries) = raw.as_object() else {
        return Ok(vec![]);
    };

    let mut out = vec![];
    for (name, entry) in entries {
        let Some(obj) = entry.as_object() else {
            continue;
        };
        let source = obj.get("source").cloned().unwrap_or(Value::Null);
        let install_location = obj
            .get("installLocation")
            .or_else(|| obj.get("install_location"))
            .and_then(Value::as_str)
            .map(String::from);
        let last_updated = obj
            .get("lastUpdated")
            .or_else(|| obj.get("last_updated"))
            .and_then(Value::as_str)
            .map(String::from);
        let auto_update_enabled = obj
            .get("autoUpdateEnabled")
            .or_else(|| obj.get("auto_update_enabled"))
            .and_then(Value::as_bool)
            .unwrap_or(false);

        // pull description/version/plugin_count from the marketplace manifest
        let manifest = install_location
            .as_deref()
            .map(Path::new)
            .map(|dir| dir.join(".claude-plugin").join("marketplace.json"))
            .filter(|m| m.exists())
            .map(|m| read_json_or_default::<MarketplaceManifest>(&m))
            .transpose()?
            .unwrap_or_default();

        let description = manifest
            .metadata
            .as_ref()
            .and_then(|m| m.get("description"))
            .and_then(Value::as_str)
            .map(String::from);
        let version = manifest
            .metadata
            .as_ref()
            .and_then(|m| m.get("version"))
            .and_then(Value::as_str)
            .map(String::from);

        out.push(KnownMarketplace {
            name: name.clone(),
            source,
            install_location,
            last_updated,
            auto_update_enabled,
            description,
            version,
            plugin_count: manifest.plugins.len(),
        });
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(out)
}

/// Enumerate every plugin offered by every known marketplace.
pub fn list_marketplace_plugins(paths: &Paths) -> Result<Vec<MarketplacePlugin>, String> {
    let marketplaces = list_marketplaces(paths)?;
    let mut out = vec![];
    for market in marketplaces {
        let manifest = market
            .install_location
            .as_deref()
            .map(Path::new)
            .map(|dir| dir.join(".claude-plugin").join("marketplace.json"))
            .filter(|m| m.exists())
            .map(|m| read_json_or_default::<MarketplaceManifest>(&m))
            .transpose()?
            .unwrap_or_default();
        for plugin in manifest.plugins {
            let plugin_id = format!("{}@{}", plugin.name, market.name);
            out.push(MarketplacePlugin {
                marketplace_name: market.name.clone(),
                name: plugin.name,
                description: plugin.description,
                version: plugin.version,
                homepage: plugin.homepage,
                repository: plugin.repository,
                category: plugin.category,
                tags: plugin.tags,
                source: plugin.source,
                plugin_id,
            });
        }
    }
    Ok(out)
}

fn has_non_empty_value(value: &Option<Value>) -> bool {
    match value {
        Some(Value::Null) | None => false,
        Some(Value::Array(items)) => !items.is_empty(),
        Some(Value::Object(object)) => !object.is_empty(),
        Some(Value::String(text)) => !text.trim().is_empty(),
        Some(_) => true,
    }
}

/// Full installed-plugin state: installed_plugins.json merged with
/// marketplace metadata, manifests, and settings.json enabledPlugins.
pub fn list_installed_plugins(paths: &Paths) -> Result<Vec<InstalledPlugin>, String> {
    let installed: InstalledPluginsFile = read_json_or_default(&installed_plugins_path(paths))?;
    let marketplace_map: std::collections::HashMap<String, MarketplacePlugin> =
        list_marketplace_plugins(paths)?
            .into_iter()
            .map(|p| (p.plugin_id.clone(), p))
            .collect();

    let settings_path = paths.tool_root(ToolId::ClaudeCode).join("settings.json");
    let settings: Value = read_json_or_default(&settings_path)?;
    let enabled_plugins = settings
        .get("enabledPlugins")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();

    let mut out = vec![];
    for (plugin_id, entries) in installed.plugins {
        let (plugin_name, marketplace_name) = parse_plugin_id(&plugin_id);
        let metadata = marketplace_map.get(&plugin_id);
        let first = entries.first();
        let install_path = first
            .and_then(|e| e.install_path.as_deref())
            .map(PathBuf::from);
        let manifest = install_path
            .as_ref()
            .map(|p| p.join(".claude-plugin").join("plugin.json"))
            .filter(|m| m.exists())
            .map(|m| read_json_or_default::<PluginManifest>(&m))
            .transpose()?
            .unwrap_or_default();

        let install_scopes: Vec<String> = entries.iter().filter_map(|e| e.scope.clone()).collect();
        let user_scope_installed = entries.iter().any(|e| e.scope.as_deref() == Some("user"));
        let user_scope_enabled = enabled_plugins
            .get(&plugin_id)
            .and_then(Value::as_bool)
            .unwrap_or(false);

        let root = install_path.as_deref().unwrap_or_else(|| Path::new(""));
        out.push(InstalledPlugin {
            plugin_id: plugin_id.clone(),
            name: metadata
                .map(|p| p.name.clone())
                .or(manifest.name)
                .unwrap_or(plugin_name),
            marketplace_name,
            description: metadata
                .and_then(|p| p.description.clone())
                .or(manifest.description),
            version: first
                .and_then(|e| e.version.clone())
                .or_else(|| metadata.and_then(|p| p.version.clone()))
                .or(manifest.version),
            install_path: install_path
                .as_ref()
                .map(|p| p.to_string_lossy().to_string()),
            user_scope_installed,
            user_scope_enabled,
            install_scopes,
            has_skills: root.join("skills").is_dir(),
            has_agents: root.join("agents").is_dir() || has_non_empty_value(&manifest.agents),
            has_hooks: root.join("hooks").is_dir() || has_non_empty_value(&manifest.hooks),
            has_mcp_servers: root.join(".mcp.json").exists()
                || has_non_empty_value(&manifest.mcp_servers),
            has_lsp_servers: root.join(".lsp.json").exists()
                || has_non_empty_value(&manifest.lsp_servers),
        });
    }
    out.sort_by(|a, b| a.plugin_id.cmp(&b.plugin_id));
    Ok(out)
}

// ---------------------------------------------------------------------------
// Mutations
// ---------------------------------------------------------------------------

/// Toggle `settings.json.enabledPlugins["<id>"]` directly (enable/disable
/// without reinstalling). Only the one key changes.
pub fn set_plugin_enabled(
    paths: &Paths,
    plugin_id: &str,
    enabled: bool,
) -> Result<Vec<PathBuf>, String> {
    let settings_path = paths.tool_root(ToolId::ClaudeCode).join("settings.json");
    let mut settings: Value = if settings_path.exists() {
        std::fs::read_to_string(&settings_path)
            .ok()
            .and_then(|raw| serde_json::from_str(&raw).ok())
            .unwrap_or_else(|| Value::Object(Map::new()))
    } else {
        Value::Object(Map::new())
    };
    let Some(obj) = settings.as_object_mut() else {
        return Err("settings.json must be an object".into());
    };

    let plugins = obj
        .entry("enabledPlugins".to_string())
        .or_insert_with(|| Value::Object(Map::new()));
    let Some(plugins) = plugins.as_object_mut() else {
        return Err("enabledPlugins must be an object".into());
    };
    plugins.insert(plugin_id.to_string(), Value::Bool(enabled));

    crate::store::save_json_atomic(&settings_path, &settings).map_err(|e| e.to_string())?;
    Ok(vec![settings_path])
}

/// Bulk enable/disable every installed user-scope plugin.
pub fn set_all_plugins_enabled(
    paths: &Paths,
    enabled: bool,
) -> Result<(usize, Vec<PathBuf>), String> {
    let installed = list_installed_plugins(paths)?;
    let ids: Vec<String> = installed
        .iter()
        .filter(|p| p.user_scope_installed)
        .map(|p| p.plugin_id.clone())
        .collect();
    let mut files = vec![];
    let mut count = 0;
    for id in ids {
        files.extend(set_plugin_enabled(paths, &id, enabled)?);
        count += 1;
    }
    Ok((count, files))
}

fn resolve_claude_program() -> Option<PathBuf> {
    if let Ok(path) = std::env::var("AITOOLPLUS_CLI_CLAUDE")
        && PathBuf::from(&path).is_file()
    {
        return Some(PathBuf::from(path));
    }
    if let Ok(found) = Command::new("where").arg("claude").output() {
        let first = String::from_utf8_lossy(&found.stdout)
            .lines()
            .next()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .map(PathBuf::from);
        if let Some(p) = first
            && p.is_file()
        {
            return Some(p);
        }
    }
    if let Some(home) = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME")) {
        let home = PathBuf::from(home);
        for candidate in [
            home.join(".bun").join("bin").join("claude.exe"),
            home.join(".local").join("bin").join("claude"),
            home.join("AppData")
                .join("Roaming")
                .join("npm")
                .join("claude.cmd"),
        ] {
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

/// Run `claude <args>` with CLAUDE_CONFIG_DIR pinned to the Claude root.
fn run_claude(paths: &Paths, args: &[&str]) -> Result<String, String> {
    let program = resolve_claude_program().ok_or_else(|| {
        "未找到 claude CLI。请先安装：npm i -g @anthropic-ai/claude-code".to_string()
    })?;
    let label = program.display().to_string();
    let root = paths.tool_root(ToolId::ClaudeCode);

    let mut cmd = Command::new(&program);
    cmd.args(args).env("CLAUDE_CONFIG_DIR", &root);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000);
    }
    let output = cmd
        .output()
        .map_err(|e| format!("Failed to run Claude command: {e}. attempted_program={label}"))?;
    if output.status.success() {
        return Ok(String::from_utf8_lossy(&output.stdout).to_string());
    }
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let msg = if stderr.is_empty() { stdout } else { stderr };
    Err(if msg.is_empty() {
        "Unknown Claude command failure".into()
    } else {
        msg
    })
}

fn marketplace_auto_update_flags(paths: &Paths) -> std::collections::HashMap<String, bool> {
    let mut flags = std::collections::HashMap::new();
    if let Ok(raw) = read_json_or_default::<Value>(&known_marketplaces_path(paths))
        && let Some(entries) = raw.as_object()
    {
        {
            for (name, entry) in entries {
                if let Some(enabled) = entry
                    .as_object()
                    .and_then(|o| {
                        o.get("autoUpdateEnabled")
                            .or_else(|| o.get("auto_update_enabled"))
                    })
                    .and_then(Value::as_bool)
                {
                    flags.insert(name.clone(), enabled);
                }
            }
        }
    }
    flags
}

fn restore_marketplace_auto_update_flags(
    paths: &Paths,
    flags: &std::collections::HashMap<String, bool>,
) -> Result<(), String> {
    let path = known_marketplaces_path(paths);
    if !path.exists() {
        return Ok(());
    }
    let mut raw: Value = read_json_or_default(&path)?;
    let Some(entries) = raw.as_object_mut() else {
        return Ok(());
    };
    let mut changed = false;
    for (name, enabled) in flags {
        if let Some(obj) = entries.get_mut(name).and_then(Value::as_object_mut) {
            let current = obj
                .get("autoUpdateEnabled")
                .or_else(|| obj.get("auto_update_enabled"))
                .and_then(Value::as_bool);
            if current != Some(*enabled) {
                obj.insert("autoUpdateEnabled".into(), Value::Bool(*enabled));
                changed = true;
            }
        }
    }
    if changed {
        crate::store::save_json_atomic(&path, &raw).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Install a plugin from its marketplace (`claude plugin install name@market`).
pub fn install_plugin(paths: &Paths, plugin_id: &str) -> Result<(), String> {
    // preserve marketplace auto-update flags across the CLI rewrite
    let flags = marketplace_auto_update_flags(paths);
    let result = run_claude(paths, &["plugin", "install", plugin_id, "--scope", "user"]);
    let _ = restore_marketplace_auto_update_flags(paths, &flags);
    result.map(|_| ())
}

/// Uninstall (`claude plugin uninstall name@market`).
pub fn uninstall_plugin(paths: &Paths, plugin_id: &str) -> Result<(), String> {
    let flags = marketplace_auto_update_flags(paths);
    let result = run_claude(paths, &["plugin", "uninstall", plugin_id]);
    let _ = restore_marketplace_auto_update_flags(paths, &flags);
    result.map(|_| ())
}

/// Add a marketplace (`claude plugin marketplace add <source>`).
pub fn add_marketplace(paths: &Paths, source: &str) -> Result<(), String> {
    run_claude(paths, &["plugin", "marketplace", "add", source]).map(|_| ())
}

/// Update one marketplace (or all when name is None).
pub fn update_marketplace(paths: &Paths, name: Option<&str>) -> Result<(), String> {
    let flags = marketplace_auto_update_flags(paths);
    let result = match name {
        Some(n) => run_claude(paths, &["plugin", "marketplace", "update", n]),
        None => run_claude(paths, &["plugin", "marketplace", "update"]),
    };
    let _ = restore_marketplace_auto_update_flags(paths, &flags);
    result.map(|_| ())
}

/// Remove a marketplace.
pub fn remove_marketplace(paths: &Paths, name: &str) -> Result<(), String> {
    run_claude(paths, &["plugin", "marketplace", "remove", name]).map(|_| ())
}

/// Set `autoUpdateEnabled` for one marketplace (direct file edit; the CLI
/// rewrites this file on other commands, so we always restore after them).
pub fn set_marketplace_auto_update(
    paths: &Paths,
    name: &str,
    enabled: bool,
) -> Result<Vec<PathBuf>, String> {
    let path = known_marketplaces_path(paths);
    let mut raw: Value = read_json_or_default(&path)?;
    let Some(entries) = raw.as_object_mut() else {
        return Err("known_marketplaces.json must be an object".into());
    };
    if let Some(obj) = entries.get_mut(name).and_then(Value::as_object_mut) {
        obj.insert("autoUpdateEnabled".into(), Value::Bool(enabled));
    } else {
        let mut obj = Map::new();
        obj.insert("autoUpdateEnabled".into(), Value::Bool(enabled));
        entries.insert(name.to_string(), Value::Object(obj));
    }
    crate::store::save_json_atomic(&path, &raw).map_err(|e| e.to_string())?;
    Ok(vec![path])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() -> (tempfile::TempDir, Paths) {
        let dir = tempfile::tempdir().unwrap();
        let paths = Paths::new(dir.path().join("home"), dir.path().join("data"));
        std::fs::create_dir_all(plugins_root(&paths)).unwrap();
        (dir, paths)
    }

    #[test]
    fn plugin_id_parsing() {
        assert_eq!(
            parse_plugin_id("my-plugin@acme"),
            ("my-plugin".into(), "acme".into())
        );
        assert_eq!(parse_plugin_id("bare"), ("bare".into(), String::new()));
        // rsplit: marketplace is the last @-segment
        assert_eq!(
            parse_plugin_id("weird@name@market"),
            ("weird@name".into(), "market".into())
        );
    }

    #[test]
    fn empty_state_lists_are_empty() {
        let (_dir, paths) = setup();
        assert!(list_marketplaces(&paths).unwrap().is_empty());
        assert!(list_installed_plugins(&paths).unwrap().is_empty());
        assert!(list_marketplace_plugins(&paths).unwrap().is_empty());
    }

    #[test]
    fn full_state_read_and_toggle() {
        let (_dir, paths) = setup();
        // marketplace with one plugin
        let market_dir = dir_marketplace(&paths, "acme");
        let manifest = serde_json::json!({
            "owner": {"name": "ACME"},
            "metadata": {"description": "ACME market", "version": "1.0"},
            "plugins": [{
                "name": "super-tool",
                "description": "Does things",
                "version": "2.1",
                "source": "./super-tool",
                "tags": ["util"],
            }]
        });
        std::fs::create_dir_all(market_dir.join(".claude-plugin")).unwrap();
        std::fs::write(
            market_dir.join(".claude-plugin").join("marketplace.json"),
            serde_json::to_string_pretty(&manifest).unwrap(),
        )
        .unwrap();

        std::fs::write(
            known_marketplaces_path(&paths),
            serde_json::json!({
                "acme": {
                    "source": "./acme",
                    "installLocation": market_dir.to_string_lossy(),
                    "autoUpdateEnabled": true,
                }
            })
            .to_string(),
        )
        .unwrap();

        let markets = list_marketplaces(&paths).unwrap();
        assert_eq!(markets.len(), 1);
        assert_eq!(markets[0].name, "acme");
        assert_eq!(markets[0].plugin_count, 1);
        assert!(markets[0].auto_update_enabled);
        assert_eq!(markets[0].description.as_deref(), Some("ACME market"));

        let plugins = list_marketplace_plugins(&paths).unwrap();
        assert_eq!(plugins.len(), 1);
        assert_eq!(plugins[0].plugin_id, "super-tool@acme");

        // installed with user scope + plugin manifest capabilities
        let install_dir = plugins_root(&paths).join("cache").join("super-tool");
        std::fs::create_dir_all(install_dir.join(".claude-plugin")).unwrap();
        std::fs::write(
            install_dir.join(".claude-plugin").join("plugin.json"),
            serde_json::json!({
                "name": "Super Tool",
                "version": "2.1",
                "mcpServers": {"fs": {"command": "x"}}
            })
            .to_string(),
        )
        .unwrap();
        std::fs::create_dir_all(install_dir.join("skills")).unwrap();
        std::fs::write(
            installed_plugins_path(&paths),
            serde_json::json!({
                "plugins": {
                    "super-tool@acme": [{
                        "scope": "user",
                        "installPath": install_dir.to_string_lossy(),
                        "version": "2.1",
                    }]
                }
            })
            .to_string(),
        )
        .unwrap();

        let installed = list_installed_plugins(&paths).unwrap();
        assert_eq!(installed.len(), 1);
        let p = &installed[0];
        assert_eq!(p.plugin_id, "super-tool@acme");
        assert_eq!(p.name, "super-tool"); // marketplace metadata wins
        assert!(p.user_scope_installed);
        assert!(!p.user_scope_enabled);
        assert!(p.has_skills);
        assert!(p.has_mcp_servers);
        assert!(!p.has_hooks);

        // enable toggles settings.json only
        let files = set_plugin_enabled(&paths, "super-tool@acme", true).unwrap();
        assert_eq!(files.len(), 1);
        let settings: Value = serde_json::from_str(
            &std::fs::read_to_string(paths.tool_root(ToolId::ClaudeCode).join("settings.json"))
                .unwrap(),
        )
        .unwrap();
        assert_eq!(settings["enabledPlugins"]["super-tool@acme"], true);

        // and reflects in the listing
        let installed = list_installed_plugins(&paths).unwrap();
        assert!(installed[0].user_scope_enabled);
    }

    fn dir_marketplace(paths: &Paths, name: &str) -> PathBuf {
        let dir = plugins_root(paths).join("marketplaces").join(name);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn auto_update_flag_set_and_restore() {
        let (_dir, paths) = setup();
        std::fs::write(
            known_marketplaces_path(&paths),
            serde_json::json!({"acme": {"source": "./a", "autoUpdateEnabled": false}}).to_string(),
        )
        .unwrap();

        let flags = marketplace_auto_update_flags(&paths);
        assert_eq!(flags.get("acme"), Some(&false));

        set_marketplace_auto_update(&paths, "acme", true).unwrap();
        assert_eq!(
            marketplace_auto_update_flags(&paths).get("acme"),
            Some(&true)
        );

        // restore semantics used after CLI rewrites
        restore_marketplace_auto_update_flags(
            &paths,
            &std::collections::HashMap::from([("acme".to_string(), false)]),
        )
        .unwrap();
        assert_eq!(
            marketplace_auto_update_flags(&paths).get("acme"),
            Some(&false)
        );
    }

    #[test]
    fn bulk_toggle() {
        let (_dir, paths) = setup();
        std::fs::write(
            installed_plugins_path(&paths),
            serde_json::json!({
                "plugins": {
                    "a@acme": [{"scope": "user"}],
                    "b@acme": [{"scope": "project"}],
                }
            })
            .to_string(),
        )
        .unwrap();
        let (count, _) = set_all_plugins_enabled(&paths, true).unwrap();
        assert_eq!(count, 1, "only user-scope plugins toggle");
        let settings: Value = serde_json::from_str(
            &std::fs::read_to_string(paths.tool_root(ToolId::ClaudeCode).join("settings.json"))
                .unwrap(),
        )
        .unwrap();
        assert_eq!(settings["enabledPlugins"]["a@acme"], true);
        assert!(settings["enabledPlugins"].get("b@acme").is_none());
    }
}
