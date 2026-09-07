//! Oh My Pi extension management (`omp plugin`), separate from Pi packages.

use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;

use crate::paths::Paths;
use crate::pi_extensions::{PiExtensionKind, PiExtensionScope, PiExtensionSummary};
use crate::tools::ToolId;

#[derive(Debug, Clone)]
pub struct OmpExtensionList {
    pub extensions_path: PathBuf,
    pub packages_path: PathBuf,
    pub cli_path: Option<String>,
    pub cli_version: Option<String>,
    pub extensions: Vec<PiExtensionSummary>,
}

fn root(paths: &Paths) -> PathBuf {
    paths.tool_root(ToolId::OhMyPi)
}

pub fn extensions_path(paths: &Paths) -> PathBuf {
    root(paths).join("extensions")
}

pub fn packages_path(paths: &Paths) -> PathBuf {
    root(paths)
        .parent()
        .map(|parent| parent.join("plugins").join("node_modules"))
        .unwrap_or_else(|| root(paths).join("plugins").join("node_modules"))
}

fn resolve_omp() -> Option<PathBuf> {
    if let Ok(path) = std::env::var("AITOOLPLUS_CLI_OMP")
        && PathBuf::from(&path).is_file()
    {
        return Some(PathBuf::from(path));
    }
    // Direct filesystem check for common global install locations
    if let Some(home) = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME")) {
        let home = PathBuf::from(home);
        let candidates = [
            home.join(".bun").join("bin").join("omp.exe"),
            home.join("AppData")
                .join("Roaming")
                .join("npm")
                .join("omp.cmd"),
            home.join(".local").join("bin").join("omp"),
        ];
        if let Some(path) = candidates.into_iter().find(|path| path.is_file()) {
            return Some(path);
        }
    }
    if let Ok(output) = Command::new("where").arg("omp").output()
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
    None
}

fn run_with_program(program: &Path, paths: &Paths, args: &[&str]) -> Result<String, String> {
    let label = program.display().to_string();
    let mut command = Command::new(program);
    command.args(args).env("PI_CODING_AGENT_DIR", root(paths));
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000);
    }
    let output = command
        .output()
        .map_err(|error| format!("OMP command failed: {error}\nomp_cli={label}"))?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        Err(format!("{stderr}\nomp_cli={label}"))
    }
}

fn run(paths: &Paths, args: &[&str]) -> Result<String, String> {
    let program = resolve_omp().ok_or("未找到 omp CLI")?;
    run_with_program(&program, paths, args)
}

fn parse_cli_list(raw: &str) -> Vec<PiExtensionSummary> {
    let Ok(value) = serde_json::from_str::<Value>(raw) else {
        return vec![];
    };
    let mut result = vec![];
    for entry in value
        .get("npm")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let Some(name) = entry.get("name").and_then(Value::as_str) else {
            continue;
        };
        result.push(PiExtensionSummary {
            id: format!("npm:{name}"),
            source: name.into(),
            scope: PiExtensionScope::User,
            kind: PiExtensionKind::Package,
            path: entry.get("path").and_then(Value::as_str).map(String::from),
            built_in: false,
            current_version: entry
                .get("version")
                .and_then(Value::as_str)
                .map(String::from),
            latest_version: None,
            update_available: false,
        });
    }
    for entry in value
        .get("marketplace")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let Some(id) = entry.get("id").and_then(Value::as_str) else {
            continue;
        };
        let first = entry
            .get("entries")
            .and_then(Value::as_array)
            .and_then(|entries| entries.first());
        result.push(PiExtensionSummary {
            id: format!("marketplace:{id}"),
            source: id.into(),
            scope: if entry.get("scope").and_then(Value::as_str) == Some("project") {
                PiExtensionScope::Project
            } else {
                PiExtensionScope::User
            },
            kind: PiExtensionKind::Package,
            path: first
                .and_then(|value| value.get("installPath"))
                .and_then(Value::as_str)
                .map(String::from),
            built_in: false,
            current_version: first
                .and_then(|value| value.get("version"))
                .and_then(Value::as_str)
                .map(String::from),
            latest_version: None,
            update_available: false,
        });
    }
    result
}

fn scan_local(directory: &Path) -> Result<Vec<PiExtensionSummary>, String> {
    if !directory.is_dir() {
        return Ok(vec![]);
    }
    let canonical_root = directory
        .canonicalize()
        .map_err(|error| error.to_string())?;
    let mut result = vec![];
    for entry in std::fs::read_dir(directory).map_err(|error| error.to_string())? {
        let entry = entry.map_err(|error| error.to_string())?;
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with('.') || name == "node_modules" || name.ends_with(".d.ts") {
            continue;
        }
        let kind = if path.is_file() && name.ends_with(".ts") {
            Some(PiExtensionKind::LocalFile)
        } else if path.is_dir() && path.join("index.ts").is_file() {
            Some(PiExtensionKind::LocalDirectory)
        } else {
            None
        };
        if let Some(kind) = kind {
            let canonical = path.canonicalize().map_err(|error| error.to_string())?;
            if !canonical.starts_with(&canonical_root) {
                continue;
            }
            result.push(PiExtensionSummary {
                id: format!("local:{name}"),
                source: name.clone(),
                scope: PiExtensionScope::User,
                kind,
                path: Some(path.display().to_string()),
                built_in: name.starts_with("pi-deck-") || name.starts_with("ai-toolbox-"),
                current_version: None,
                latest_version: None,
                update_available: false,
            });
        }
    }
    Ok(result)
}

pub fn list(paths: &Paths) -> Result<OmpExtensionList, String> {
    let cli = resolve_omp();
    let raw = match &cli {
        Some(prog) => run_with_program(prog, paths, &["plugin", "list", "--json"]),
        None => Ok("{}".into()),
    }?;
    let mut extensions = parse_cli_list(&raw);
    let mut local = scan_local(&extensions_path(paths))?;
    let known: std::collections::HashSet<String> = extensions
        .iter()
        .map(|extension| extension.source.clone())
        .collect();
    local.retain(|extension| !known.contains(&extension.source));
    extensions.append(&mut local);
    extensions.sort_by(|a, b| a.source.cmp(&b.source));
    let cli_version = cli
        .as_ref()
        .and_then(|prog| run_with_program(prog, paths, &["--version"]).ok())
        .map(|value| value.trim().into());
    Ok(OmpExtensionList {
        extensions_path: extensions_path(paths),
        packages_path: packages_path(paths),
        cli_path: cli.map(|path| path.display().to_string()),
        cli_version,
        extensions,
    })
}

pub fn install(paths: &Paths, source: &str) -> Result<(), String> {
    run(paths, &["plugin", "install", source]).map(|_| ())
}

pub fn uninstall(
    paths: &Paths,
    source: &str,
    kind: PiExtensionKind,
    path: Option<&str>,
) -> Result<(), String> {
    if kind == PiExtensionKind::Package {
        return run(paths, &["plugin", "uninstall", source]).map(|_| ());
    }
    if source.starts_with("pi-deck-") || source.starts_with("ai-toolbox-") {
        return Err("built-in OMP extension cannot be deleted".into());
    }
    let root = extensions_path(paths)
        .canonicalize()
        .map_err(|error| error.to_string())?;
    let target = PathBuf::from(path.unwrap_or(source));
    let target = if target.is_absolute() {
        target
    } else {
        root.join(target)
    };
    let target = target.canonicalize().map_err(|error| error.to_string())?;
    if !target.starts_with(&root) {
        return Err("OMP extension is outside extensions directory".into());
    }
    if target.is_dir() {
        std::fs::remove_dir_all(target).map_err(|error| error.to_string())
    } else {
        std::fs::remove_file(target).map_err(|error| error.to_string())
    }
}

pub fn update(paths: &Paths, source: Option<&str>) -> Result<(), String> {
    match source {
        Some(source) => run(paths, &["plugin", "upgrade", source]).map(|_| ()),
        None => run(paths, &["plugin", "upgrade"]).map(|_| ()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_npm_and_marketplace() {
        let raw = r#"{"npm":[{"name":"context-mode","version":"1.2.3","path":"/pkg"}],"marketplace":[{"id":"exa","scope":"project","entries":[{"installPath":"/exa","version":"0.9.1"}]}]}"#;
        let extensions = parse_cli_list(raw);
        assert_eq!(extensions.len(), 2);
        assert_eq!(extensions[0].id, "npm:context-mode");
        assert_eq!(extensions[1].scope, PiExtensionScope::Project);
    }

    #[test]
    fn scans_local_and_protects_builtins() {
        let directory = tempfile::tempdir().unwrap();
        std::fs::write(directory.path().join("single.ts"), "").unwrap();
        std::fs::create_dir(directory.path().join("ai-toolbox-core")).unwrap();
        std::fs::write(
            directory.path().join("ai-toolbox-core").join("index.ts"),
            "",
        )
        .unwrap();
        let extensions = scan_local(directory.path()).unwrap();
        assert_eq!(extensions.len(), 2);
        assert!(extensions.iter().any(|extension| extension.built_in));
    }
}
