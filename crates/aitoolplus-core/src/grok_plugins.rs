//! Grok native plugin management (`grok plugin`).

use std::path::PathBuf;
use std::process::Command;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::paths::Paths;
use crate::tools::ToolId;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GrokPlugin {
    pub plugin_id: String,
    pub name: String,
    pub marketplace_name: String,
    pub installed: bool,
    pub enabled: bool,
    pub install_available: bool,
    pub source: Option<String>,
    pub description: Option<String>,
    pub version: Option<String>,
    pub capabilities: Vec<String>,
}

fn resolve_grok() -> Option<PathBuf> {
    if let Ok(path) = std::env::var("AITOOLPLUS_CLI_GROK")
        && PathBuf::from(&path).is_file()
    {
        return Some(PathBuf::from(path));
    }
    if let Ok(output) = Command::new("where").arg("grok").output()
        && let Some(path) = String::from_utf8_lossy(&output.stdout)
            .lines()
            .next()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(PathBuf::from)
        && path.is_file()
    {
        return Some(path);
    }
    let home = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME"))?;
    let home = PathBuf::from(home);
    [
        home.join(".bun").join("bin").join("grok.exe"),
        home.join("AppData")
            .join("Roaming")
            .join("npm")
            .join("grok.cmd"),
        home.join(".local").join("bin").join("grok"),
    ]
    .into_iter()
    .find(|path| path.is_file())
}

fn run(paths: &Paths, args: &[&str]) -> Result<String, String> {
    let program = resolve_grok().ok_or("未找到 grok CLI")?;
    let label = program.display().to_string();
    let mut command = Command::new(&program);
    command
        .args(args)
        .env("GROK_HOME", paths.tool_root(ToolId::Grok));
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000);
    }
    let output = command
        .output()
        .map_err(|error| format!("Grok plugin command failed: {error}; grok_cli={label}"))?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
    }
}

fn capabilities(value: &Value) -> Vec<String> {
    let mut result = vec![];
    if value
        .get("skill_count")
        .and_then(Value::as_u64)
        .unwrap_or(0)
        > 0
    {
        result.push("skills".into());
    }
    for (field, label) in [
        ("has_hooks", "hooks"),
        ("has_agents", "agents"),
        ("has_mcp", "mcp"),
        ("has_apps", "apps"),
    ] {
        if value.get(field).and_then(Value::as_bool).unwrap_or(false) {
            result.push(label.into());
        }
    }
    result
}

fn parse(values: &[Value], available: bool) -> Vec<GrokPlugin> {
    values
        .iter()
        .filter_map(|value| {
            let id = value
                .get("plugin_id")
                .or_else(|| value.get("id"))
                .and_then(Value::as_str)?;
            let (name, marketplace) = id.rsplit_once('@').unwrap_or((id, ""));
            Some(GrokPlugin {
                plugin_id: id.into(),
                name: value
                    .get("display_name")
                    .or_else(|| value.get("name"))
                    .and_then(Value::as_str)
                    .unwrap_or(name)
                    .into(),
                marketplace_name: value
                    .get("marketplace_name")
                    .and_then(Value::as_str)
                    .unwrap_or(marketplace)
                    .into(),
                installed: value
                    .get("installed")
                    .and_then(Value::as_bool)
                    .unwrap_or(!available),
                enabled: value
                    .get("enabled")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
                install_available: value
                    .get("install_available")
                    .and_then(Value::as_bool)
                    .unwrap_or(available),
                source: value
                    .get("install_source")
                    .or_else(|| value.get("source_path"))
                    .and_then(Value::as_str)
                    .map(String::from),
                description: value
                    .get("description")
                    .and_then(Value::as_str)
                    .map(String::from),
                version: value
                    .get("active_version")
                    .or_else(|| value.get("version"))
                    .and_then(Value::as_str)
                    .map(String::from),
                capabilities: capabilities(value),
            })
        })
        .collect()
}

pub fn list_installed(paths: &Paths) -> Result<Vec<GrokPlugin>, String> {
    let value: Value = serde_json::from_str(&run(paths, &["plugin", "list", "--json"])?)
        .map_err(|error| format!("invalid Grok plugin JSON: {error}"))?;
    let array = value.as_array().cloned().unwrap_or_default();
    Ok(parse(&array, false))
}

pub fn list_available(paths: &Paths) -> Result<Vec<GrokPlugin>, String> {
    let value: Value =
        serde_json::from_str(&run(paths, &["plugin", "list", "--json", "--available"])?)
            .map_err(|error| format!("invalid Grok available plugin JSON: {error}"))?;
    let array = value.as_array().cloned().unwrap_or_default();
    Ok(parse(&array, true))
}

pub fn install(paths: &Paths, plugin: &GrokPlugin) -> Result<(), String> {
    let source = plugin.source.as_deref().unwrap_or(&plugin.plugin_id);
    run(paths, &["plugin", "install", source, "--trust"]).map(|_| ())
}

pub fn enable(paths: &Paths, id: &str, enabled: bool) -> Result<(), String> {
    run(
        paths,
        &["plugin", if enabled { "enable" } else { "disable" }, id],
    )
    .map(|_| ())
}

pub fn uninstall(paths: &Paths, id: &str) -> Result<(), String> {
    run(paths, &["plugin", "uninstall", id, "--confirm"]).map(|_| ())
}

pub fn update(paths: &Paths, id: &str) -> Result<(), String> {
    run(paths, &["plugin", "update", id]).map(|_| ())
}

pub fn validate(paths: &Paths, target: &str) -> Result<String, String> {
    run(paths, &["plugin", "validate", target])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_installed_and_available_shapes() {
        let installed = serde_json::json!([{
            "plugin_id":"tool@market","display_name":"Tool","enabled":true,
            "active_version":"1.0.0","skill_count":2,"has_mcp":true
        }]);
        let parsed = parse(installed.as_array().unwrap(), false);
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].name, "Tool");
        assert!(parsed[0].enabled);
        assert!(parsed[0].capabilities.contains(&"skills".to_string()));
        assert!(parsed[0].capabilities.contains(&"mcp".to_string()));

        let available = serde_json::json!([{
            "id":"other@market","name":"other","install_source":"github:x/y",
            "install_available":true,"has_apps":true
        }]);
        let parsed = parse(available.as_array().unwrap(), true);
        assert!(parsed[0].install_available);
        assert_eq!(parsed[0].source.as_deref(), Some("github:x/y"));
        assert!(parsed[0].capabilities.contains(&"apps".to_string()));
    }
}
