//! Codex native plugin and marketplace management.
//!
//! Mirrors the layout used by OpenAI Codex CLI and ai-toolbox:
//! - Root: `~/.codex`
//! - Config: `~/.codex/config.toml` (`[features] plugins = true`, `[plugins."<id>"] enabled = bool`)
//! - Cache: `~/.codex/plugins/cache/<marketplace>/<plugin>/<version>`
//! - Manifest: `<cache_dir>/.codex-plugin/plugin.json`

use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use toml_edit::{DocumentMut, Item, Table, value};

use crate::paths::Paths;
use crate::tools::ToolId;

const DEFAULT_PLUGIN_VERSION: &str = "local";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CodexInstalledPlugin {
    pub plugin_id: String,
    pub marketplace_name: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub installed_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active_version: Option<String>,
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub has_skills: bool,
    #[serde(default)]
    pub has_mcp_servers: bool,
    #[serde(default)]
    pub has_apps: bool,
    #[serde(default)]
    pub capabilities: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CodexMarketplacePlugin {
    pub plugin_id: String,
    pub marketplace_name: String,
    pub marketplace_path: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    #[serde(default)]
    pub capabilities: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_path: Option<String>,
    #[serde(default)]
    pub installed: bool,
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub install_available: bool,
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
struct RawPluginManifest {
    #[serde(default)]
    name: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    skills: Option<serde_json::Value>,
    #[serde(default)]
    mcp_servers: Option<serde_json::Value>,
    #[serde(default)]
    apps: Option<serde_json::Value>,
    #[serde(default)]
    interface: Option<RawPluginInterface>,
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct RawPluginInterface {
    #[serde(default)]
    display_name: Option<String>,
    #[serde(default)]
    category: Option<String>,
    #[serde(default)]
    capabilities: Vec<String>,
}

fn codex_root(paths: &Paths) -> PathBuf {
    paths.tool_root(ToolId::Codex)
}

fn codex_config_path(paths: &Paths) -> PathBuf {
    codex_root(paths).join("config.toml")
}

fn codex_cache_root(paths: &Paths) -> PathBuf {
    codex_root(paths).join("plugins").join("cache")
}

fn read_toml_document(config_path: &Path) -> Result<DocumentMut, String> {
    if !config_path.exists() {
        return Ok(DocumentMut::new());
    }
    let content = fs::read_to_string(config_path)
        .map_err(|error| format!("Failed to read {}: {}", config_path.display(), error))?;
    content
        .parse::<DocumentMut>()
        .map_err(|error| format!("Failed to parse {}: {}", config_path.display(), error))
}

fn write_toml_document(config_path: &Path, document: &DocumentMut) -> Result<(), String> {
    if let Some(parent) = config_path.parent() {
        if !parent.exists() {
            fs::create_dir_all(parent)
                .map_err(|error| format!("Failed to create {}: {}", parent.display(), error))?;
        }
    }
    fs::write(config_path, document.to_string())
        .map_err(|error| format!("Failed to write {}: {}", config_path.display(), error))
}

fn ensure_table<'a>(item: &'a mut Item) -> &'a mut Table {
    if !item.is_table() {
        *item = Item::Table(Table::new());
    }
    item.as_table_mut().expect("table ensured")
}

pub fn read_plugin_enabled_map(config_path: &Path) -> HashMap<String, bool> {
    let mut map = HashMap::new();
    if let Ok(document) = read_toml_document(config_path) {
        if let Some(plugins_table) = document.get("plugins").and_then(Item::as_table_like) {
            for (plugin_id, plugin_item) in plugins_table.iter() {
                let enabled = plugin_item
                    .as_table_like()
                    .and_then(|table| table.get("enabled"))
                    .and_then(Item::as_bool)
                    .unwrap_or(true);
                map.insert(plugin_id.to_string(), enabled);
            }
        }
    }
    map
}

fn read_manifest(installed_root: &Path) -> Option<RawPluginManifest> {
    for manifest_subpath in &[".codex-plugin/plugin.json", ".claude-plugin/plugin.json", "plugin.json"] {
        let path = installed_root.join(manifest_subpath);
        if path.is_file() {
            if let Ok(raw) = fs::read_to_string(&path) {
                if let Ok(parsed) = serde_json::from_str::<RawPluginManifest>(&raw) {
                    return Some(parsed);
                }
            }
        }
    }
    None
}

fn active_version(plugin_base_dir: &Path) -> Option<String> {
    let entries = fs::read_dir(plugin_base_dir).ok()?;
    let mut versions = Vec::new();
    for entry in entries.flatten() {
        if let Ok(ft) = entry.file_type() {
            if ft.is_dir() {
                versions.push(entry.file_name().to_string_lossy().to_string());
            }
        }
    }
    versions.sort();
    if versions.iter().any(|v| v == DEFAULT_PLUGIN_VERSION) {
        return Some(DEFAULT_PLUGIN_VERSION.to_string());
    }
    versions.pop()
}

/// Scans local installed plugins in ~/.codex/plugins/cache/<marketplace>/<plugin>/<version>
pub fn list_installed(paths: &Paths) -> Result<Vec<CodexInstalledPlugin>, String> {
    let config_path = codex_config_path(paths);
    let enabled_map = read_plugin_enabled_map(&config_path);
    let cache_root = codex_cache_root(paths);

    let mut installed = Vec::new();
    if let Ok(marketplaces) = fs::read_dir(&cache_root) {
        for m_entry in marketplaces.flatten() {
            if !m_entry.file_type().map(|f| f.is_dir()).unwrap_or(false) {
                continue;
            }
            let marketplace_name = m_entry.file_name().to_string_lossy().to_string();
            if let Ok(plugins) = fs::read_dir(m_entry.path()) {
                for p_entry in plugins.flatten() {
                    if !p_entry.file_type().map(|f| f.is_dir()).unwrap_or(false) {
                        continue;
                    }
                    let plugin_name = p_entry.file_name().to_string_lossy().to_string();
                    let Some(version) = active_version(&p_entry.path()) else {
                        continue;
                    };
                    let installed_root = p_entry.path().join(&version);
                    let plugin_id = format!("{plugin_name}@{marketplace_name}");
                    let enabled = enabled_map.get(&plugin_id).copied().unwrap_or(true);
                    let manifest = read_manifest(&installed_root);

                    let display_name = manifest
                        .as_ref()
                        .and_then(|m| m.interface.as_ref())
                        .and_then(|i| i.display_name.clone());
                    let description = manifest.as_ref().and_then(|m| m.description.clone());
                    let category = manifest
                        .as_ref()
                        .and_then(|m| m.interface.as_ref())
                        .and_then(|i| i.category.clone());
                    let capabilities = manifest
                        .as_ref()
                        .and_then(|m| m.interface.as_ref())
                        .map(|i| i.capabilities.clone())
                        .unwrap_or_default();
                    let has_skills = manifest.as_ref().and_then(|m| m.skills.as_ref()).is_some();
                    let has_mcp_servers =
                        manifest.as_ref().and_then(|m| m.mcp_servers.as_ref()).is_some();
                    let has_apps = manifest.as_ref().and_then(|m| m.apps.as_ref()).is_some();

                    installed.push(CodexInstalledPlugin {
                        plugin_id,
                        marketplace_name: marketplace_name.clone(),
                        name: plugin_name,
                        display_name,
                        description,
                        category,
                        installed_path: Some(installed_root.to_string_lossy().to_string()),
                        active_version: Some(version),
                        enabled,
                        has_skills,
                        has_mcp_servers,
                        has_apps,
                        capabilities,
                    });
                }
            }
        }
    }
    installed.sort_by(|a, b| a.plugin_id.cmp(&b.plugin_id));
    Ok(installed)
}

/// Built-in curated marketplace plugins for Codex
fn curated_marketplace_plugins() -> Vec<CodexMarketplacePlugin> {
    vec![
        CodexMarketplacePlugin {
            plugin_id: "developer-essentials@codex-curated".into(),
            marketplace_name: "codex-curated".into(),
            marketplace_path: "builtin".into(),
            name: "developer-essentials".into(),
            display_name: Some("Developer Essentials".into()),
            description: Some("Comprehensive developer tooling for Codex: code search, syntax analysis, and git workflows.".into()),
            category: Some("development".into()),
            capabilities: vec!["skills".into(), "MCP".into()],
            source_path: None,
            installed: false,
            enabled: false,
            install_available: true,
        },
        CodexMarketplacePlugin {
            plugin_id: "security-audit@codex-curated".into(),
            marketplace_name: "codex-curated".into(),
            marketplace_path: "builtin".into(),
            name: "security-audit".into(),
            display_name: Some("Security & Vulnerability Audit".into()),
            description: Some("Automated static code security analysis, vulnerability detection, and secret leakage prevention.".into()),
            category: Some("security".into()),
            capabilities: vec!["skills".into(), "hooks".into()],
            source_path: None,
            installed: false,
            enabled: false,
            install_available: true,
        },
        CodexMarketplacePlugin {
            plugin_id: "database-studio@codex-curated".into(),
            marketplace_name: "codex-curated".into(),
            marketplace_path: "builtin".into(),
            name: "database-studio".into(),
            display_name: Some("Database Studio".into()),
            description: Some("SQL query builder, database schema introspection, and migration assistant.".into()),
            category: Some("data".into()),
            capabilities: vec!["skills".into(), "MCP".into()],
            source_path: None,
            installed: false,
            enabled: false,
            install_available: true,
        },
        CodexMarketplacePlugin {
            plugin_id: "testing-toolkit@codex-curated".into(),
            marketplace_name: "codex-curated".into(),
            marketplace_path: "builtin".into(),
            name: "testing-toolkit".into(),
            display_name: Some("Testing & QA Suite".into()),
            description: Some("Unit test generation, test coverage verification, and regression test runner.".into()),
            category: Some("testing".into()),
            capabilities: vec!["skills".into()],
            source_path: None,
            installed: false,
            enabled: false,
            install_available: true,
        },
        CodexMarketplacePlugin {
            plugin_id: "api-scaffold@codex-curated".into(),
            marketplace_name: "codex-curated".into(),
            marketplace_path: "builtin".into(),
            name: "api-scaffold".into(),
            display_name: Some("OpenAPI & REST Scaffold".into()),
            description: Some("Instant REST/GraphQL API scaffolding, mock server generation, and client code generation.".into()),
            category: Some("productivity".into()),
            capabilities: vec!["skills".into(), "MCP".into()],
            source_path: None,
            installed: false,
            enabled: false,
            install_available: true,
        },
    ]
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CodexPluginData {
    pub installed_plugins: Vec<CodexInstalledPlugin>,
    pub marketplace_plugins: Vec<CodexMarketplacePlugin>,
}

/// Lists all installed and marketplace plugins for Codex
pub fn list_all(
    paths: &Paths,
) -> Result<CodexPluginData, String> {
    let installed = list_installed(paths)?;
    let installed_ids: BTreeMap<String, bool> =
        installed.iter().map(|p| (p.plugin_id.clone(), p.enabled)).collect();

    let mut marketplace = curated_marketplace_plugins();
    for item in &mut marketplace {
        if let Some(&enabled) = installed_ids.get(&item.plugin_id) {
            item.installed = true;
            item.enabled = enabled;
            item.install_available = false;
        }
    }

    Ok(CodexPluginData {
        installed_plugins: installed,
        marketplace_plugins: marketplace,
    })
}

pub fn set_plugin_enabled(paths: &Paths, plugin_id: &str, enabled: bool) -> Result<(), String> {
    let config_path = codex_config_path(paths);
    let mut document = read_toml_document(&config_path)?;
    let features_table = ensure_table(document.entry("features").or_insert(Item::None));
    features_table["plugins"] = value(true);

    let plugins_table = ensure_table(document.entry("plugins").or_insert(Item::None));
    let plugin_table = ensure_table(plugins_table.entry(plugin_id).or_insert(Item::None));
    plugin_table["enabled"] = value(enabled);
    write_toml_document(&config_path, &document)
}

pub fn set_all_plugins_enabled(paths: &Paths, enabled: bool) -> Result<usize, String> {
    let installed = list_installed(paths)?;
    let config_path = codex_config_path(paths);
    let mut document = read_toml_document(&config_path)?;
    let features_table = ensure_table(document.entry("features").or_insert(Item::None));
    features_table["plugins"] = value(true);

    let plugins_table = ensure_table(document.entry("plugins").or_insert(Item::None));
    for p in &installed {
        let plugin_table = ensure_table(plugins_table.entry(&p.plugin_id).or_insert(Item::None));
        plugin_table["enabled"] = value(enabled);
    }
    write_toml_document(&config_path, &document)?;
    Ok(installed.len())
}

pub fn install_plugin(paths: &Paths, plugin_id: &str) -> Result<(), String> {
    let (plugin_name, marketplace_name) = plugin_id
        .rsplit_once('@')
        .ok_or_else(|| format!("Invalid plugin id `{plugin_id}`"))?;
    let target_dir = codex_cache_root(paths)
        .join(marketplace_name)
        .join(plugin_name)
        .join(DEFAULT_PLUGIN_VERSION);
    fs::create_dir_all(&target_dir)
        .map_err(|e| format!("Failed to create plugin dir: {e}"))?;

    let plugin_manifest = serde_json::json!({
        "name": plugin_name,
        "description": format!("Installed {plugin_name} plugin"),
        "interface": {
            "displayName": plugin_name,
            "capabilities": ["skills"]
        }
    });
    let manifest_dir = target_dir.join(".codex-plugin");
    fs::create_dir_all(&manifest_dir).ok();
    fs::write(
        manifest_dir.join("plugin.json"),
        serde_json::to_string_pretty(&plugin_manifest).unwrap_or_default(),
    )
    .ok();

    set_plugin_enabled(paths, plugin_id, true)
}

pub fn uninstall_plugin(paths: &Paths, plugin_id: &str) -> Result<(), String> {
    let (plugin_name, marketplace_name) = plugin_id
        .rsplit_once('@')
        .ok_or_else(|| format!("Invalid plugin id `{plugin_id}`"))?;
    let plugin_dir = codex_cache_root(paths)
        .join(marketplace_name)
        .join(plugin_name);
    if plugin_dir.exists() {
        fs::remove_dir_all(&plugin_dir)
            .map_err(|e| format!("Failed to remove plugin dir: {e}"))?;
    }

    let config_path = codex_config_path(paths);
    if let Ok(mut document) = read_toml_document(&config_path) {
        if let Some(plugins_table) = document.get_mut("plugins").and_then(Item::as_table_like_mut) {
            plugins_table.remove(plugin_id);
            let _ = write_toml_document(&config_path, &document);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_codex_plugins_lifecycle() {
        let dir = tempfile::tempdir().unwrap();
        let paths = Paths::new(dir.path().join("home"), dir.path().join("data"));

        // 1. Initially empty
        let initial = list_all(&paths).unwrap();
        assert!(initial.installed_plugins.is_empty());
        assert!(!initial.marketplace_plugins.is_empty());

        // 2. Install a plugin
        let test_id = "developer-essentials@codex-curated";
        install_plugin(&paths, test_id).unwrap();

        let after_install = list_all(&paths).unwrap();
        assert_eq!(after_install.installed_plugins.len(), 1);
        let p = &after_install.installed_plugins[0];
        assert_eq!(p.plugin_id, test_id);
        assert!(p.enabled);

        // Marketplace plugin should be marked as installed
        let m_item = after_install
            .marketplace_plugins
            .iter()
            .find(|m| m.plugin_id == test_id)
            .unwrap();
        assert!(m_item.installed);
        assert!(!m_item.install_available);

        // 3. Toggle enabled
        set_plugin_enabled(&paths, test_id, false).unwrap();
        let after_disable = list_all(&paths).unwrap();
        assert!(!after_disable.installed_plugins[0].enabled);

        // 4. Bulk enable / disable
        let count = set_all_plugins_enabled(&paths, true).unwrap();
        assert_eq!(count, 1);
        let after_bulk = list_all(&paths).unwrap();
        assert!(after_bulk.installed_plugins[0].enabled);

        let count2 = set_all_plugins_enabled(&paths, false).unwrap();
        assert_eq!(count2, 1);
        let after_bulk2 = list_all(&paths).unwrap();
        assert!(!after_bulk2.installed_plugins[0].enabled);

        // 5. Uninstall
        uninstall_plugin(&paths, test_id).unwrap();
        let after_uninstall = list_all(&paths).unwrap();
        assert!(after_uninstall.installed_plugins.is_empty());
    }
}
