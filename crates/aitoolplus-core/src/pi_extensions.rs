//! Pi extension management (mirrors ai-toolbox `coding/pi/extensions.rs`).
//!
//! Semantics:
//! - `pi list` is the source of truth for package extensions; prefer
//!   `--no-approve` so non-interactive ops don't stall on project-trust
//!   prompts, and fall back once without the flag when the CLI doesn't
//!   know it (`Unknown option --no-approve`)
//! - local `.ts` files and `<dir>/index.ts` under `<root>/extensions` merge
//!   into the same list; `pi-deck-*` / `ai-toolbox-*` are protected
//! - unpinned `npm:` packages get a registry `dist-tags.latest` lookup for
//!   update-available hints; failures degrade silently
//! - errors must carry `pi_cli=<path>` so multi-`pi` PATH issues are visible
//! - `settings.json.packages` is owned by this chain: Other-Settings views
//!   hide it, saves preserve it

use std::path::{Path, PathBuf};
use std::process::Command;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::paths::Paths;
use crate::tools::ToolId;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PiExtensionKind {
    Package,
    LocalFile,
    LocalDirectory,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PiExtensionScope {
    User,
    Project,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PiExtensionSummary {
    pub id: String,
    pub source: String,
    pub scope: PiExtensionScope,
    pub kind: PiExtensionKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    pub built_in: bool,
    #[serde(
        rename = "currentVersion",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub current_version: Option<String>,
    #[serde(
        rename = "latestVersion",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub latest_version: Option<String>,
    pub update_available: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PiExtensionListResult {
    pub extensions: Vec<PiExtensionSummary>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cli_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cli_version: Option<String>,
}

/// Where the `pi` binary might live when GUI PATH misses it.
pub fn resolve_pi_program() -> Option<PathBuf> {
    if let Ok(path) = std::env::var("AITOOLPLUS_CLI_PI")
        && PathBuf::from(&path).is_file()
    {
        return Some(PathBuf::from(path));
    }
    // Direct filesystem check for common global install locations (much faster than `where.exe`)
    if let Some(home) = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME")) {
        let home = PathBuf::from(home);
        let bun = home.join(".bun").join("bin").join("pi.exe");
        if bun.is_file() {
            return Some(bun);
        }
        let npm_global = home
            .join("AppData")
            .join("Roaming")
            .join("npm")
            .join("pi.cmd");
        if npm_global.is_file() {
            return Some(npm_global);
        }
    }
    if let Ok(found) = Command::new("where").arg("pi").output() {
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
    None
}

fn pi_root(paths: &Paths) -> PathBuf {
    paths.tool_root(ToolId::Pi)
}

pub fn extensions_path(paths: &Paths) -> PathBuf {
    pi_root(paths).join("extensions")
}

/// Run a `pi` subcommand against the runtime root using a resolved program binary.
fn run_pi_with_program(
    program: &Path,
    paths: &Paths,
    args: &[&str],
) -> Result<(String, Vec<String>), String> {
    let cli_label = program.display().to_string();
    let root = pi_root(paths);

    let run = |args: &[&str]| -> Result<String, String> {
        let mut cmd = Command::new(program);
        cmd.args(args)
            .env("PI_CODING_AGENT_DIR", &root)
            .env("NPM_CONFIG_LEGACY_PEER_DEPS", "true");
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
        }
        let output = cmd
            .output()
            .map_err(|e| format!("Failed to run Pi command: {e}\npi_cli={cli_label}"))?;
        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        if output.status.success() {
            Ok(stdout)
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            let msg = if stderr.is_empty() {
                stdout.trim().to_string()
            } else {
                stderr
            };
            Err(format!("{msg}\npi_cli={cli_label}"))
        }
    };

    let used: Vec<String> = args.iter().map(|s| s.to_string()).collect();
    match run(args) {
        Ok(out) => Ok((out, used)),
        Err(err) if args.contains(&"--no-approve") && is_unknown_no_approve_error(&err) => {
            // Old pi builds don't know --no-approve; retry without it.
            let fallback: Vec<&str> = args
                .iter()
                .copied()
                .filter(|a| *a != "--no-approve" && *a != "-na")
                .collect();
            let used: Vec<String> = fallback.iter().map(|s| s.to_string()).collect();
            Ok((run(&fallback)?, used))
        }
        Err(err) => Err(err),
    }
}

/// Run a `pi` subcommand against the runtime root. Returns (stdout, args).
fn run_pi(paths: &Paths, args: &[&str]) -> Result<(String, Vec<String>), String> {
    let program = resolve_pi_program().ok_or_else(|| {
        "未找到 pi CLI。请先安装：npm i -g @mariozechner/pi-coding-agent".to_string()
    })?;
    run_pi_with_program(&program, paths, args)
}

fn is_unknown_no_approve_error(message: &str) -> bool {
    let lower = message.to_ascii_lowercase();
    let unknown = lower.contains("unknown option")
        || lower.contains("unknown argument")
        || lower.contains("unrecognized option");
    let no_approve = lower.contains("--no-approve")
        || lower.contains("-no-approve")
        || lower.contains("'-na'")
        || lower.contains("\"-na\"")
        || lower
            .split_whitespace()
            .any(|t| t.trim_matches(|c| c == '\'' || c == '"' || c == ',' || c == '.') == "-na");
    unknown && no_approve
}

fn is_cli_package_source(source: &str) -> bool {
    let lower = source.trim().to_ascii_lowercase();
    ["npm:", "file:", "github:", "git:", "http:", "https:"]
        .iter()
        .any(|p| lower.starts_with(p))
}

pub fn is_protected_local_extension(source: &str) -> bool {
    let s = source.trim();
    s.starts_with("pi-deck-") || s.starts_with("ai-toolbox-")
}

/// Parse `pi list` output (User packages: / Project packages: sections).
fn parse_list_output(raw: &str) -> Vec<PiExtensionSummary> {
    let mut result = Vec::new();
    let mut scope = PiExtensionScope::Unknown;
    let mut pending: Option<usize> = None;

    for line in raw.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if trimmed.eq_ignore_ascii_case("User packages:") {
            scope = PiExtensionScope::User;
            pending = None;
            continue;
        }
        if trimmed.eq_ignore_ascii_case("Project packages:") {
            scope = PiExtensionScope::Project;
            pending = None;
            continue;
        }
        if is_cli_package_source(trimmed) {
            result.push(PiExtensionSummary {
                id: format!("{}:{}", scope_id(scope), trimmed),
                source: trimmed.to_string(),
                scope,
                kind: PiExtensionKind::Package,
                path: None,
                built_in: false,
                current_version: None,
                latest_version: None,
                update_available: false,
            });
            pending = Some(result.len() - 1);
            continue;
        }
        if let Some(idx) = pending
            && result[idx].path.is_none()
        {
            // the line after a package source is its install path
            let candidate = Path::new(trimmed);
            if candidate.is_absolute() || trimmed.contains("node_modules") {
                result[idx].path = Some(trimmed.to_string());
            }
        }
    }
    result
}

fn scope_id(scope: PiExtensionScope) -> &'static str {
    match scope {
        PiExtensionScope::User => "user",
        PiExtensionScope::Project => "project",
        PiExtensionScope::Unknown => "unknown",
    }
}

/// Scan local `.ts` / `<dir>/index.ts` extensions under `<root>/extensions`.
pub fn scan_local_extensions(extensions_path: &Path) -> Result<Vec<PiExtensionSummary>, String> {
    let entries = match std::fs::read_dir(extensions_path) {
        Ok(e) => e,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(vec![]),
        Err(e) => {
            return Err(format!(
                "Failed to read Pi extensions directory {}: {e}",
                extensions_path.display()
            ));
        }
    };

    let mut result = vec![];
    for entry in entries {
        let entry = entry.map_err(|e| e.to_string())?;
        let file_name = entry.file_name();
        let Some(file_name) = file_name.to_str() else {
            continue;
        };
        if file_name.starts_with('.') || file_name == "node_modules" || file_name.ends_with(".d.ts")
        {
            continue;
        }
        let path = entry.path();
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        let (source, kind) = if file_type.is_file() && file_name.ends_with(".ts") {
            (file_name.to_string(), PiExtensionKind::LocalFile)
        } else if file_type.is_dir() && path.join("index.ts").is_file() {
            (file_name.to_string(), PiExtensionKind::LocalDirectory)
        } else {
            continue;
        };
        let built_in = is_protected_local_extension(&source);
        result.push(PiExtensionSummary {
            id: format!("local:{source}"),
            source,
            scope: PiExtensionScope::User,
            kind,
            path: Some(path.to_string_lossy().to_string()),
            built_in,
            current_version: None,
            latest_version: None,
            update_available: false,
        });
    }
    result.sort_by(|a, b| a.source.cmp(&b.source));
    Ok(result)
}

/// Parse `npm:name` / `npm:@scope/name` (+ optional `@version` pin).
pub fn parse_npm_package_source(source: &str) -> Option<(String, Option<String>)> {
    let trimmed = source.trim();
    let without_prefix = trimmed.strip_prefix("npm:")?;
    if without_prefix.is_empty() {
        return None;
    }
    if let Some(rest) = without_prefix.strip_prefix('@') {
        let (name_part, version_part) = match rest.rsplit_once('@') {
            Some((name, version)) if name.contains('/') => (name, Some(version)),
            _ => (rest, None),
        };
        if name_part.is_empty() || !name_part.contains('/') {
            return None;
        }
        let package = format!("@{name_part}");
        let pinned = version_part
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .map(String::from);
        return Some((package, pinned));
    }
    let (name_part, version_part) = match without_prefix.rsplit_once('@') {
        Some((name, version)) if !name.is_empty() => (name, Some(version)),
        _ => (without_prefix, None),
    };
    if name_part.is_empty() || name_part.contains('/') {
        return None;
    }
    let pinned = version_part
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(String::from);
    Some((name_part.to_string(), pinned))
}

/// Loose semver-ish comparison: is `latest` newer than `current`?
pub fn is_version_newer(latest: &str, current: &str) -> bool {
    let parse = |v: &str| -> Vec<u64> {
        v.trim()
            .trim_start_matches('v')
            .split(|c: char| !c.is_ascii_digit())
            .filter(|p| !p.is_empty())
            .filter_map(|p| p.parse::<u64>().ok())
            .collect()
    };
    let l = parse(latest);
    let c = parse(current);
    if l.is_empty() || c.is_empty() {
        return latest.trim() != current.trim() && !latest.trim().is_empty();
    }
    for i in 0..l.len().max(c.len()) {
        let a = l.get(i).copied().unwrap_or(0);
        let b = c.get(i).copied().unwrap_or(0);
        if a > b {
            return true;
        }
        if a < b {
            return false;
        }
    }
    false
}

/// npm registry `dist-tags.latest` lookup for unpinned packages.
pub fn fetch_npm_latest_version(package: &str) -> Result<String, String> {
    let url = format!("https://registry.npmjs.org/-/package/{package}/dist-tags");
    let agent = ureq::AgentBuilder::new()
        .timeout(std::time::Duration::from_secs(8))
        .build();
    let response = agent
        .get(&url)
        .call()
        .map_err(|e| format!("npm registry lookup failed: {e}"))?;
    let body: Value = response
        .into_json()
        .map_err(|e| format!("npm registry parse failed: {e}"))?;
    body.get("latest")
        .and_then(Value::as_str)
        .map(String::from)
        .ok_or_else(|| "no latest tag".into())
}

/// Local extension list: `pi list` packages + local extensions + installed
/// versions. Invokes external processes, so call from background thread/task.
pub fn list_extensions(paths: &Paths) -> Result<PiExtensionListResult, String> {
    let program = resolve_pi_program();
    let cli_path = program.as_ref().map(|p| p.display().to_string());
    let cli_version = program
        .as_ref()
        .and_then(|p| run_pi_with_program(p, paths, &["--version"]).ok())
        .map(|(out, _)| out.lines().next().unwrap_or("").trim().to_string())
        .filter(|v| !v.is_empty());

    let prog = program.ok_or_else(|| {
        "未找到 pi CLI。请先安装：npm i -g @mariozechner/pi-coding-agent".to_string()
    })?;

    let mut extensions = match run_pi_with_program(&prog, paths, &["list", "--no-approve"]) {
        Ok((out, _)) => parse_list_output(&out),
        Err(e) => return Err(e),
    };
    extensions.retain(|e| e.kind == PiExtensionKind::Package);

    // enrich current versions from installed package.json
    extensions = extensions
        .into_iter()
        .map(|mut e| {
            if let Some(path) = e.path.as_deref()
                && let Ok(pkg) = std::fs::read_to_string(Path::new(path).join("package.json"))
                && let Ok(v) = serde_json::from_str::<Value>(&pkg)
                && let Some(version) = v.get("version").and_then(Value::as_str)
            {
                e.current_version = Some(version.to_string());
            }
            e
        })
        .collect();

    // local extensions merge in
    let mut locals = scan_local_extensions(&extensions_path(paths))?;
    extensions.append(&mut locals);

    Ok(PiExtensionListResult {
        extensions,
        cli_path,
        cli_version,
    })
}

/// Network phase: enrich an already-fast local list with npm latest
/// versions. Call this from a background executor, never from GPUI render().
pub fn check_extension_updates(result: &mut PiExtensionListResult) -> usize {
    let mut available = 0;
    for extension in &mut result.extensions {
        if extension.kind == PiExtensionKind::Package
            && let Some((package, None)) = parse_npm_package_source(&extension.source)
            && let Ok(latest) = fetch_npm_latest_version(&package)
        {
            if let Some(current) = extension.current_version.as_deref()
                && is_version_newer(&latest, current)
            {
                extension.update_available = true;
                available += 1;
            }
            extension.latest_version = Some(latest);
        }
    }
    available
}

/// `pi install <source>` for a package extension.
pub fn install_extension(paths: &Paths, source: &str) -> Result<Vec<String>, String> {
    let (out, args) = run_pi(paths, &["install", source, "--no-approve"])?;
    let _ = out;
    Ok(args)
}

/// `pi remove <source>`.
pub fn remove_extension(paths: &Paths, source: &str) -> Result<Vec<String>, String> {
    let (out, args) = run_pi(paths, &["remove", source, "--no-approve"])?;
    let _ = out;
    Ok(args)
}

/// `pi update <source>` (or all when source is None).
pub fn update_extension(paths: &Paths, source: Option<&str>) -> Result<Vec<String>, String> {
    let mut args = vec!["update".to_string()];
    if let Some(src) = source {
        args.push(src.to_string());
    }
    args.push("--no-approve".into());
    let refs: Vec<&str> = args.iter().map(String::as_str).collect();
    let (out, used) = run_pi(paths, &refs)?;
    let _ = out;
    Ok(used)
}

/// `settings.json.packages` preservation: the Other-Settings view hides it.
pub fn hide_packages_from_other_settings(settings: &mut Value) {
    if let Some(obj) = settings.as_object_mut() {
        obj.remove("packages");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_pi_list_output() {
        let raw = "User packages:\n  npm:pi-subagents\n    C:\\Users\\u\\.pi\\agent\\npm\\node_modules\\pi-subagents\n  npm:pi-goal\n    C:\\Users\\u\\.pi\\agent\\npm\\node_modules\\pi-goal\nProject packages:\n  npm:local-tool\n";
        let parsed = parse_list_output(raw);
        assert_eq!(parsed.len(), 3);
        assert_eq!(parsed[0].source, "npm:pi-subagents");
        assert_eq!(parsed[0].scope, PiExtensionScope::User);
        assert!(parsed[0].path.as_deref().unwrap().contains("node_modules"));
        assert_eq!(parsed[2].scope, PiExtensionScope::Project);
    }

    #[test]
    fn local_scan_finds_ts_and_dirs() {
        let dir = tempfile::tempdir().unwrap();
        let ext = dir.path().join("extensions");
        std::fs::create_dir_all(&ext).unwrap();
        std::fs::write(ext.join("my-tool.ts"), "// ext").unwrap();
        std::fs::create_dir_all(ext.join("my-pack")).unwrap();
        std::fs::write(ext.join("my-pack").join("index.ts"), "// ext").unwrap();
        std::fs::create_dir_all(ext.join("node_modules")).unwrap();
        std::fs::write(ext.join("skip.d.ts"), "skip").unwrap();

        let found = scan_local_extensions(&ext).unwrap();
        assert_eq!(found.len(), 2);
        assert!(
            found
                .iter()
                .any(|e| e.source == "my-tool.ts" && e.kind == PiExtensionKind::LocalFile)
        );
        assert!(
            found
                .iter()
                .any(|e| e.source == "my-pack" && e.kind == PiExtensionKind::LocalDirectory)
        );
        assert!(
            !found
                .iter()
                .any(|e| e.source.contains("node_modules") || e.source.contains(".d.ts"))
        );
    }

    #[test]
    fn protected_prefixes() {
        assert!(is_protected_local_extension("pi-deck-mcp"));
        assert!(is_protected_local_extension("ai-toolbox-something"));
        assert!(!is_protected_local_extension("my-ext"));
    }

    #[test]
    fn npm_source_parsing() {
        assert_eq!(
            parse_npm_package_source("npm:pi-subagents"),
            Some(("pi-subagents".into(), None))
        );
        assert_eq!(
            parse_npm_package_source("npm:pi-subagents@1.2.3"),
            Some(("pi-subagents".into(), Some("1.2.3".into())))
        );
        assert_eq!(
            parse_npm_package_source("npm:@scope/name"),
            Some(("@scope/name".into(), None))
        );
        assert_eq!(
            parse_npm_package_source("npm:@scope/name@2.0.0"),
            Some(("@scope/name".into(), Some("2.0.0".into())))
        );
        // pinned versions must not trigger latest lookups
        let (_, pinned) = parse_npm_package_source("npm:pi-goal@0.9.1").unwrap();
        assert_eq!(pinned, Some("0.9.1".into()));
        assert!(parse_npm_package_source("github:foo/bar").is_none());
        assert!(parse_npm_package_source("npm:").is_none());
    }

    #[test]
    fn version_compare() {
        assert!(is_version_newer("1.3.0", "1.2.9"));
        assert!(!is_version_newer("1.2.9", "1.3.0"));
        assert!(is_version_newer("0.80.6", "0.80.5"));
        assert!(!is_version_newer("0.80.6", "0.80.6"));
        assert!(is_version_newer("2.0", "1.9.9"));
        assert!(is_version_newer("v1.1", "1.0"));
    }

    #[test]
    fn no_approve_error_detection() {
        assert!(is_unknown_no_approve_error(
            "Error: Unknown option --no-approve for \"list\""
        ));
        assert!(is_unknown_no_approve_error("unknown argument '-na'"));
        assert!(!is_unknown_no_approve_error("some other error"));
    }

    #[test]
    fn other_settings_hides_packages() {
        let mut settings = serde_json::json!({
            "packages": ["npm:pi-goal"],
            "defaultProvider": "anthropic",
            "theme": "dark"
        });
        hide_packages_from_other_settings(&mut settings);
        assert!(settings.get("packages").is_none());
        assert_eq!(settings["defaultProvider"], "anthropic");
        assert_eq!(settings["theme"], "dark");
    }
}
