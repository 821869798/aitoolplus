//! Local agent CLI check / install / update.
//!
//! Windows behavior follows CC-Switch `commands/misc.rs`:
//! probe with System32 `where.exe $PATH:name` (never `cmd /C name`, which can
//! hit an App Execution Alias), skip `WindowsApps` stubs, then run the real
//! executable's `--version`. Install and update command lines are the same
//! strings CC-Switch uses. WSL-only installs are not probed.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

#[derive(Debug, Clone)]
pub struct ToolStatus {
    pub id: String,
    pub name: String,
    pub version: Option<String>,
    pub latest_version: Option<String>,
    /// mise / nvm / volta / pnpm / scoop / homebrew / npm，用来决定升级写回哪里。
    pub source: String,
    pub error: Option<String>,
    pub installed_but_broken: bool,
}

#[derive(Clone, Copy)]
enum Action {
    Install,
    Update,
}

struct ToolSpec {
    id: &'static str,
    name: &'static str,
    npm: Option<&'static str>,
    /// Official self-update, chained before the npm install fallback.
    update: Option<&'static str>,
}

const TOOLS: &[ToolSpec] = &[
    ToolSpec {
        id: "claude",
        name: "Claude Code",
        npm: Some("npm i -g @anthropic-ai/claude-code@latest"),
        update: Some("claude update"),
    },
    ToolSpec {
        id: "codex",
        name: "Codex",
        npm: Some("npm i -g @openai/codex@latest"),
        update: Some("codex update"),
    },
    ToolSpec {
        id: "agy",
        name: "Antigravity CLI",
        npm: Some("npm i -g @google/antigravity-cli@latest"),
        update: None,
    },
    ToolSpec {
        id: "grok",
        name: "Grok Build",
        npm: Some("npm i -g @xai-official/grok@latest"),
        update: Some("grok update"),
    },
    ToolSpec {
        id: "opencode",
        name: "OpenCode",
        npm: Some("npm i -g opencode-ai@latest"),
        update: Some("opencode upgrade"),
    },
    ToolSpec {
        id: "openclaw",
        name: "OpenClaw",
        npm: Some("npm i -g openclaw@latest"),
        update: Some("openclaw update --yes"),
    },
    ToolSpec {
        id: "hermes",
        name: "Hermes",
        npm: None,
        update: Some("hermes update"),
    },
    ToolSpec {
        id: "pi",
        name: "Pi",
        npm: Some("npm i -g @earendil-works/pi-coding-agent@latest"),
        update: None,
    },
];

const HERMES_INSTALL_PS: &str =
    "irm https://raw.githubusercontent.com/NousResearch/hermes-agent/main/scripts/install.ps1 | iex";
const AGY_INSTALL_PS: &str = "irm https://antigravity.google/cli/install.ps1 | iex";
const GROK_INSTALL_PS: &str = "irm https://x.ai/cli/install.ps1 | iex";

/// 一次探测的结果。最新版本和升级命令只看它，不再各写一套判断。
enum Source {
    Mise(String),
    Brew(String),
    Scoop(String),
    Volta(String),
    Bun(String),
    Pnpm(String),
    Npm(String),
    /// 程序自带 update/upgrade。
    Official,
    /// 官方安装脚本（Antigravity CLI、原生 Grok）。
    Script,
    Unknown,
}

impl Source {
    fn label(&self) -> &'static str {
        match self {
            Self::Mise(_) => "mise",
            Self::Brew(_) => "homebrew",
            Self::Scoop(_) => "scoop",
            Self::Volta(_) => "volta",
            Self::Bun(_) => "bun",
            Self::Pnpm(_) => "pnpm",
            Self::Npm(_) => "npm",
            Self::Official | Self::Script => "官方",
            Self::Unknown => "未知",
        }
    }

    fn latest(&self, tool: &ToolSpec) -> Option<String> {
        match self {
            Self::Mise(package) => mise_latest(package),
            Self::Brew(formula) => brew_latest(formula),
            Self::Scoop(app) => scoop_latest(app),
            Self::Volta(_) | Self::Bun(_) | Self::Pnpm(_) | Self::Npm(_) => {
                fetch_latest(tool, None)
            }
            Self::Official if tool.id == "hermes" => github_latest("NousResearch/hermes-agent"),
            Self::Official | Self::Script | Self::Unknown => None,
        }
    }

    fn upgrade(&self, tool: &ToolSpec, bin: &str) -> Option<String> {
        let package = npm_package(tool.id);
        match self {
            Self::Mise(name) => Some(format!("mise upgrade {name}")),
            Self::Brew(formula) => sibling_bin(bin, "brew", &["", "exe"])
                .map(|brew| format!("{} upgrade {formula}", quote_batch(&brew))),
            Self::Scoop(app) => Some(format!("scoop update {app}")),
            Self::Volta(volta) => package.map(|pkg| format!("{} install {pkg}", quote_batch(volta))),
            Self::Bun(bun) => {
                package.map(|pkg| format!("{} add -g {pkg}@latest", quote_batch(bun)))
            }
            Self::Pnpm(pnpm) => {
                package.map(|pkg| format!("{} add -g {pkg}@latest", quote_batch(pnpm)))
            }
            Self::Npm(npm) => package.map(|pkg| format!("{} i -g {pkg}@latest", quote_batch(npm))),
            Self::Official => {
                let update = format!("{} {}", quote_batch(bin), official_args(tool.id)?);
                Some(match sibling_bin(bin, "npm", &["cmd", "exe", ""]) {
                    Some(npm) if package.is_some() => {
                        format!("{update} || call {} i -g {}@latest", quote_batch(&npm), package.unwrap())
                    }
                    _ => update,
                })
            }
            Self::Script if tool.id == "grok" => Some(format!(
                "{} update || {}",
                quote_batch(bin),
                powershell_encoded(GROK_INSTALL_PS)
            )),
            Self::Script if tool.id == "agy" => Some(format!(
                "{} || call npm i -g @google/antigravity-cli@latest",
                powershell_encoded(AGY_INSTALL_PS)
            )),
            Self::Script | Self::Unknown => None,
        }
    }
}

fn classify(id: &str, bin: &Path, real: &Path) -> Source {
    let bin_text = bin.to_string_lossy();
    let real_text = real.to_string_lossy();
    if id == "hermes" {
        return Source::Official;
    }
    if path_has(bin, "/mise/") || path_has(real, "/mise/") {
        return Source::Mise(mise_package(id, &bin_text, &real_text));
    }
    if id == "grok" && [bin, real].iter().any(|path| {
        let text = normalized(path);
        text.contains("/.grok/bin/") || text.contains("/.grok/downloads/grok-")
    }) {
        return Source::Script;
    }
    if let Some(formula) = brew_formula(&real_text) {
        if sibling_bin(&bin_text, "brew", &["", "exe"]).is_some() {
            return Source::Brew(formula);
        }
    }
    if let Some(app) = scoop_app(bin, Some(real)) {
        return Source::Scoop(app);
    }
    if let Some(bin) = sibling_bin(&bin_text, "volta", &["exe", "cmd", ""]) {
        return Source::Volta(bin);
    }
    if let Some(bin) = sibling_bin(&bin_text, "bun", &["exe", "cmd", ""]) {
        return Source::Bun(bin);
    }
    if let Some(bin) = sibling_bin(&bin_text, "pnpm", &["cmd", "exe", ""]) {
        return Source::Pnpm(bin);
    }
    if matches!(id, "claude" | "openclaw") {
        return Source::Official;
    }
    if let Some(bin) = sibling_bin(&bin_text, "npm", &["cmd", "exe", ""]) {
        return Source::Npm(bin);
    }
    if id == "agy" {
        return Source::Script;
    }
    Source::Unknown
}

fn path_has(path: &Path, marker: &str) -> bool {
    normalized(path).contains(marker)
}

fn normalized(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/").to_ascii_lowercase()
}

pub fn tool_ids() -> Vec<&'static str> {
    TOOLS.iter().map(|tool| tool.id).collect()
}

pub fn check_all() -> Vec<ToolStatus> {
    TOOLS.iter().map(check_one).collect()
}

pub fn check_one_id(id: &str) -> Option<ToolStatus> {
    TOOLS.iter().find(|tool| tool.id == id).map(check_one)
}

pub fn manual_command(id: &str) -> Option<&'static str> {
    TOOLS.iter().find(|tool| tool.id == id).and_then(|tool| tool.npm)
}

pub fn run_action(id: &str, install: bool) -> Result<(), String> {
    let tool = TOOLS
        .iter()
        .find(|tool| tool.id == id)
        .ok_or_else(|| format!("未知工具: {id}"))?;
    let command = if install {
        action_command(tool, Action::Install)
    } else {
        upgrade_command(tool).or_else(|| action_command(tool, Action::Update))
    }
    .ok_or_else(|| format!("不支持的操作: {id}"))?;
    run_batch(&command)
}

/// 升级写回当前 PATH 命中的那一份，而不是盲目 `npm i -g`。
/// Homebrew formula → 同目录 `brew upgrade`；mise/nvm/fnm → 那套 Node 旁边的 npm；
/// Volta / pnpm / bun 用各自的命令。找不到锚点才退回静态命令。
fn upgrade_command(tool: &ToolSpec) -> Option<String> {
    let path = resolve_on_path(tool.id)?;
    let real = std::fs::canonicalize(&path).unwrap_or_else(|_| path.clone());
    classify(tool.id, &path, &real).upgrade(tool, &path.to_string_lossy())
}

fn check_one(tool: &ToolSpec) -> ToolStatus {
    let located = resolve_on_path(tool.id).or_else(|| find_fallback(tool.id));
    let (label, latest_version, probe) = match &located {
        Some(path) => {
            let real = std::fs::canonicalize(path).unwrap_or_else(|_| path.clone());
            let source = classify(tool.id, path, &real);
            let latest = source.latest(tool);
            (source.label().to_string(), latest, run_version(path))
        }
        None => (
            "未安装".to_string(),
            None,
            Probe::Missing("未安装或不在 PATH 中".to_string()),
        ),
    };
    let (version, error, broken) = match probe {
        Probe::Found(version) => (Some(version), None, false),
        Probe::Broken(error) => (None, Some(error), true),
        Probe::Missing(error) => (None, Some(error), false),
    };
    ToolStatus {
        id: tool.id.to_string(),
        name: tool.name.to_string(),
        version,
        latest_version,
        source: label,
        error,
        installed_but_broken: broken,
    }
}

fn find_fallback(id: &str) -> Option<PathBuf> {
    for dir in fallback_dirs() {
        for candidate in executable_candidates(id, &dir) {
            if candidate.exists() && !is_windows_apps_alias(&candidate) {
                return Some(candidate);
            }
        }
    }
    None
}

enum Probe {
    Found(String),
    Broken(String),
    Missing(String),
}

fn resolve_on_path(id: &str) -> Option<PathBuf> {
    let path = effective_path();
    let where_exe = std::env::var_os("SystemRoot")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\Windows"))
        .join("System32")
        .join("where.exe");
    let mut command = Command::new(where_exe);
    command
        .arg(format!("$PATH:{id}"))
        .env("PATH", &path)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    hide_window(&mut command);
    let output = output_timeout(command, Duration::from_secs(8)).ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    text.lines().map(str::trim).find_map(|line| {
        let path = PathBuf::from(line);
        if path.exists() && !is_windows_apps_alias(&path) {
            Some(path)
        } else {
            None
        }
    })
}

fn run_version(path: &Path) -> Probe {
    let mut command = Command::new(path);
    let parent = path.parent().unwrap_or(Path::new("."));
    let path_env = format!("{};{}", parent.display(), effective_path());
    command
        .arg("--version")
        .env("PATH", path_env)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    hide_window(&mut command);
    let output = match output_timeout(command, Duration::from_secs(8)) {
        Ok(output) => output,
        Err(error) => return Probe::Broken(error),
    };
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if output.status.success() {
        let raw = if stdout.is_empty() { stderr } else { stdout };
        if raw.is_empty() {
            Probe::Broken("没有版本输出".to_string())
        } else {
            Probe::Found(extract_version(&raw))
        }
    } else {
        let detail = if stderr.is_empty() { stdout } else { stderr };
        Probe::Broken(last_lines(detail.trim(), 4))
    }
}

fn executable_candidates(id: &str, dir: &Path) -> Vec<PathBuf> {
    [".cmd", ".exe", ".bat", ""]
        .into_iter()
        .map(|ext| dir.join(format!("{id}{ext}")))
        .collect()
}

fn fallback_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Ok(appdata) = std::env::var("APPDATA") {
        dirs.push(PathBuf::from(appdata).join("npm"));
    }
    if let Ok(local) = std::env::var("LOCALAPPDATA") {
        let local = PathBuf::from(local);
        dirs.push(local.join("pnpm"));
        dirs.push(local.join("Volta").join("bin"));
        dirs.push(local.join("Programs").join("claude"));
        dirs.push(
            local
                .join("Programs")
                .join("OpenAI")
                .join("Codex")
                .join("bin"),
        );
    }
    if let Ok(home) = std::env::var("USERPROFILE") {
        dirs.push(PathBuf::from(home).join(".local").join("bin"));
    }
    dirs
}

fn is_windows_apps_alias(path: &Path) -> bool {
    path.to_string_lossy()
        .to_ascii_lowercase()
        .contains("\\windowsapps\\")
}

fn parent_dir(path: &str) -> String {
    match path.rfind('\\').max(path.rfind('/')) {
        Some(index) if index > 0 => path[..index].to_string(),
        _ => String::new(),
    }
}

fn mise_package(id: &str, bin_path: &str, real: &str) -> String {
    for path in [real, bin_path] {
        let text = path.replace('\\', "/");
        let lower = text.to_ascii_lowercase();
        let marker = "/mise/installs/";
        if let Some(start) = lower.find(marker) {
            let after = &text[start + marker.len()..];
            if let Some(name) = after.split('/').next() {
                if !name.is_empty() {
                    return name.to_string();
                }
            }
        }
    }
    match id {
        "claude" => "claude-code".to_string(),
        "agy" => "agy".to_string(),
        other => other.to_string(),
    }
}

fn brew_formula(real: &str) -> Option<String> {
    let normalized = real.replace('\\', "/");
    let mut parts = normalized.split('/');
    while let Some(part) = parts.next() {
        if part.eq_ignore_ascii_case("Cellar") {
            return parts.next().filter(|part| !part.is_empty()).map(str::to_string);
        }
    }
    None
}

fn sibling_bin(bin_path: &str, name: &str, extensions: &[&str]) -> Option<String> {
    let dir = parent_dir(bin_path);
    if dir.is_empty() {
        return None;
    }
    let dir = PathBuf::from(dir);
    for extension in extensions {
        let candidate = if extension.is_empty() {
            dir.join(name)
        } else {
            dir.join(format!("{name}.{extension}"))
        };
        if candidate.is_file() {
            return Some(candidate.to_string_lossy().into_owned());
        }
    }
    None
}

fn quote_batch(path: &str) -> String {
    let escaped = path.replace('%', "%%%%");
    let needs_quote = path
        .chars()
        .any(|ch| matches!(ch, ' ' | '&' | '(' | ')' | '^' | ';' | '<' | '>' | '|' | ','));
    if needs_quote {
        format!("\"{escaped}\"")
    } else {
        escaped
    }
}

fn official_args(id: &str) -> Option<&'static str> {
    match id {
        "claude" | "codex" | "grok" | "hermes" => Some("update"),
        "openclaw" => Some("update --yes"),
        "opencode" => Some("upgrade"),
        _ => None,
    }
}

/// 写回探测到的那一份安装。规则与 CC-Switch `anchored_command_from_paths` 一致。
fn npm_package(id: &str) -> Option<&'static str> {
    match id {
        "claude" => Some("@anthropic-ai/claude-code"),
        "codex" => Some("@openai/codex"),
        "agy" => Some("@google/antigravity-cli"),
        "grok" => Some("@xai-official/grok"),
        "opencode" => Some("opencode-ai"),
        "openclaw" => Some("openclaw"),
        "pi" => Some("@earendil-works/pi-coding-agent"),
        _ => None,
    }
}

fn action_command(tool: &ToolSpec, action: Action) -> Option<String> {
    if tool.id == "hermes" {
        let install = powershell_encoded(HERMES_INSTALL_PS);
        return Some(match action {
            Action::Install => install,
            Action::Update => format!("hermes update || {install}"),
        });
    }
    if tool.id == "agy" {
        let install = powershell_encoded(AGY_INSTALL_PS);
        let npm = "npm i -g @google/antigravity-cli@latest";
        return Some(format!("{install} || call {npm}"));
    }
    if tool.id == "grok" && matches!(action, Action::Install) {
        let npm = tool.npm?;
        return Some(format!(
            "{} || call {npm}",
            powershell_encoded(GROK_INSTALL_PS)
        ));
    }
    let npm = tool.npm?;
    Some(match action {
        Action::Install => npm.to_string(),
        Action::Update => match tool.update {
            Some(update) => format!("{update} || call {npm}"),
            None => npm.to_string(),
        },
    })
}

fn powershell_encoded(script: &str) -> String {
    use base64::Engine;
    let mut bytes = Vec::new();
    for unit in script.encode_utf16() {
        bytes.extend_from_slice(&unit.to_le_bytes());
    }
    format!(
        "powershell -NoProfile -ExecutionPolicy Bypass -EncodedCommand {}",
        base64::engine::general_purpose::STANDARD.encode(bytes)
    )
}

struct TempFile(PathBuf);
impl Drop for TempFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

fn run_batch(command: &str) -> Result<(), String> {
    #[cfg(windows)]
    let (mut process, _cleanup) = {
        let script =
            format!("@echo off\r\ncall {command}\r\nif errorlevel 1 exit /b %errorlevel%\r\n");
        let file = std::env::temp_dir().join(format!("aitoolplus-tool-{}.bat", std::process::id()));
        std::fs::write(&file, &script).map_err(|e| format!("写入安装脚本失败: {e}"))?;
        let mut process = Command::new("cmd");
        process.arg("/C").arg(&file);
        (process, TempFile(file))
    };
    #[cfg(not(windows))]
    let (mut process, _cleanup) = {
        let mut process = Command::new("sh");
        process.arg("-c").arg(command);
        (process, ())
    };
    process
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    hide_window(&mut process);
    let output = output_timeout(process, Duration::from_secs(600))?;
    if output.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let raw = if stderr.trim().is_empty() {
        stdout.trim()
    } else {
        stderr.trim()
    };
    let detail = last_lines(raw, 8);
    Err(if detail.is_empty() {
        format!("命令失败 ({:?})", output.status.code())
    } else {
        detail
    })
}

fn scoop_app(path: &Path, real: Option<&Path>) -> Option<String> {
    for candidate in real.into_iter().chain(std::iter::once(path)) {
        let text = candidate.to_string_lossy().replace('\\', "/");
        let lower = text.to_ascii_lowercase();
        let marker = "/scoop/apps/";
        if let Some(start) = lower.find(marker) {
            let name = text[start + marker.len()..].split('/').next()?;
            if !name.is_empty() && !name.eq_ignore_ascii_case("current") {
                return Some(name.to_string());
            }
        }
    }
    None
}

fn brew_latest(formula: &str) -> Option<String> {
    let mut command = Command::new("brew");
    command
        .args(["info", "--json=v2", formula])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    hide_window(&mut command);
    let output = output_timeout(command, Duration::from_secs(12)).ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let value: serde_json::Value = serde_json::from_str(&text).ok()?;
    value
        .pointer("/formulae/0/versions/stable")
        .or_else(|| value.pointer("/casks/0/version"))
        .and_then(|v| v.as_str())
        .map(str::to_string)
}

fn scoop_latest(app: &str) -> Option<String> {
    let mut command = Command::new("scoop");
    command
        .args(["info", app])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    hide_window(&mut command);
    let output = output_timeout(command, Duration::from_secs(12)).ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    text.lines().find_map(|line| {
        let line = line.trim();
        let rest = line.strip_prefix("Version")?.trim().trim_start_matches(':').trim();
        if rest.is_empty() { None } else { Some(extract_version(rest)) }
    })
}

fn mise_latest(package: &str) -> Option<String> {
    let mut command = Command::new("mise");
    command
        .args(["latest", package])
        .env("PATH", effective_path())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    hide_window(&mut command);
    let output = output_timeout(command, Duration::from_secs(8)).ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let version = text.trim();
    if version.is_empty() {
        None
    } else {
        Some(extract_version(version))
    }
}

fn fetch_latest(tool: &ToolSpec, local: Option<&str>) -> Option<String> {
    let package = tool.npm.and_then(npm_package_from_command);
    let mut latest = package.and_then(|package| npm_latest(package));
    if tool.id == "claude" {
        if let (Some(local), Some(stable)) = (local, latest.as_deref()) {
            if compare_semver(local, stable) == Some(std::cmp::Ordering::Greater) {
                if let Some(next) = npm_dist_tag("@anthropic-ai/claude-code", "next") {
                    latest = Some(next);
                }
            }
        }
    }
    if tool.id == "hermes" && latest.is_none() {
        latest = github_latest("NousResearch/hermes-agent");
    }
    if tool.id == "opencode" && latest.is_none() {
        latest = github_latest("anomalyco/opencode");
    }
    latest
}

fn npm_package_from_command(command: &str) -> Option<&str> {
    let marker = "-g ";
    let start = command.find(marker)? + marker.len();
    let rest = command[start..].trim();
    Some(rest.strip_suffix("@latest").unwrap_or(rest))
}

fn npm_latest(package: &str) -> Option<String> {
    npm_dist_tag(package, "latest")
}

fn npm_dist_tag(package: &str, tag: &str) -> Option<String> {
    let encoded = package.replace('/', "%2f");
    let url = format!("https://registry.npmjs.org/{encoded}/{tag}");
    let body = http_get(&url)?;
    let value: serde_json::Value = serde_json::from_str(&body).ok()?;
    value
        .get("version")
        .and_then(|v| v.as_str())
        .map(str::to_string)
}

fn github_latest(repo: &str) -> Option<String> {
    let url = format!("https://api.github.com/repos/{repo}/releases/latest");
    let body = http_get(&url)?;
    let value: serde_json::Value = serde_json::from_str(&body).ok()?;
    value
        .get("tag_name")
        .and_then(|v| v.as_str())
        .map(|tag| tag.trim_start_matches('v').to_string())
}

fn http_get(url: &str) -> Option<String> {
    for attempt in 0..2 {
        match ureq::get(url)
            .timeout(Duration::from_secs(8))
            .set("User-Agent", "aitoolplus")
            .call()
        {
            Ok(response) => {
                let mut body = String::new();
                return response
                    .into_reader()
                    .take(1_000_000)
                    .read_to_string(&mut body)
                    .ok()
                    .map(|_| body);
            }
            Err(ureq::Error::Status(code, _)) if code == 429 || code >= 500 => {
                if attempt == 0 {
                    std::thread::sleep(Duration::from_millis(500));
                    continue;
                }
            }
            Err(_) => return None,
        }
    }
    None
}

pub fn update_available(local: &str, latest: &str) -> bool {
    compare_semver(local, latest) == Some(std::cmp::Ordering::Less)
}

fn compare_semver(left: &str, right: &str) -> Option<std::cmp::Ordering> {
    let (a, a_pre) = parse_semver(left)?;
    let (b, b_pre) = parse_semver(right)?;
    Some(a.cmp(&b).then_with(|| match (a_pre.is_empty(), b_pre.is_empty()) {
        (false, true) => std::cmp::Ordering::Less,
        (true, false) => std::cmp::Ordering::Greater,
        _ => a_pre.cmp(&b_pre),
    }))
}

fn parse_semver(value: &str) -> Option<([u64; 3], String)> {
    let core = value.trim().trim_start_matches('v').split('+').next()?;
    let (core, pre) = core.split_once('-').unwrap_or((core, ""));
    let mut parts = core.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.parse().ok()?;
    let patch = parts.next().unwrap_or("0").parse().ok()?;
    Some(([major, minor, patch], pre.to_string()))
}

fn extract_version(raw: &str) -> String {
    let bytes = raw.as_bytes();
    let mut index = 0;
    while index + 4 < bytes.len() {
        if bytes[index].is_ascii_digit() {
            let start = index;
            while index < bytes.len() && (bytes[index].is_ascii_digit() || bytes[index] == b'.') {
                index += 1;
            }
            let token = &raw[start..index];
            if token.matches('.').count() >= 2 {
                let mut end = index;
                if end < bytes.len() && bytes[end] == b'-' {
                    end += 1;
                    while end < bytes.len() && (bytes[end].is_ascii_alphanumeric() || bytes[end] == b'.') {
                        end += 1;
                    }
                }
                return raw[start..end].to_string();
            }
        } else {
            index += 1;
        }
    }
    raw.lines().next().unwrap_or(raw).trim().to_string()
}

fn last_lines(text: &str, count: usize) -> String {
    let lines: Vec<&str> = text.lines().collect();
    let start = lines.len().saturating_sub(count);
    lines[start..].join("\n")
}

fn effective_path() -> String {
    let process = std::env::var("PATH").unwrap_or_default();
    let user = registry_path(true);
    let machine = registry_path(false);
    let mut seen = std::collections::HashSet::new();
    let mut parts = Vec::new();
    for source in [&process, &user, &machine] {
        for part in source.split(';') {
            let part = part.trim();
            if part.is_empty() {
                continue;
            }
            let key = part.to_ascii_lowercase();
            if seen.insert(key) {
                parts.push(part.to_string());
            }
        }
    }
    parts.join(";")
}

fn registry_path(user: bool) -> String {
    #[cfg(windows)]
    {
        use windows::core::PCWSTR;
        use windows::Win32::System::Registry::{
            HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, RegGetValueW, RRF_RT_REG_EXPAND_SZ, RRF_RT_REG_SZ,
        };
        let subkey: Vec<u16> = if user {
            "Environment\0"
        } else {
            "SYSTEM\\CurrentControlSet\\Control\\Session Manager\\Environment\0"
        }
        .encode_utf16()
        .collect();
        let name: Vec<u16> = "Path\0".encode_utf16().collect();
        let mut buffer = vec![0u16; 16_384];
        let mut size = (buffer.len() * 2) as u32;
        let root = if user { HKEY_CURRENT_USER } else { HKEY_LOCAL_MACHINE };
        let status = unsafe {
            RegGetValueW(
                root,
                PCWSTR(subkey.as_ptr()),
                PCWSTR(name.as_ptr()),
                RRF_RT_REG_SZ | RRF_RT_REG_EXPAND_SZ,
                None,
                Some(buffer.as_mut_ptr() as _),
                Some(&mut size),
            )
        };
        if status != windows::Win32::Foundation::ERROR_SUCCESS {
            return String::new();
        }
        let chars = (size as usize / 2).saturating_sub(1);
        let raw = String::from_utf16_lossy(&buffer[..chars.min(buffer.len())]);
        expand_percents(&raw)
    }
    #[cfg(not(windows))]
    {
        let _ = user;
        String::new()
    }
}

fn expand_percents(raw: &str) -> String {
    let mut out = String::new();
    let mut rest = raw;
    while let Some(start) = rest.find('%') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        if let Some(end) = after.find('%') {
            let name = &after[..end];
            if let Ok(value) = std::env::var(name) {
                out.push_str(&value);
            }
            rest = &after[end + 1..];
        } else {
            out.push('%');
            out.push_str(after);
            return out;
        }
    }
    out.push_str(rest);
    out
}

fn hide_window(command: &mut Command) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
}

fn reap(mut child: std::process::Child) {
    let _ = child.kill();
    // wait_with_output 会读完管道再回收。只 wait() 时管道写满会把进程卡死。
    let _ = child.wait_with_output();
}

fn output_timeout(mut command: Command, timeout: Duration) -> Result<std::process::Output, String> {
    let mut child = command
        .spawn()
        .map_err(|error| format!("启动进程失败: {error}"))?;
    let started = Instant::now();
    loop {
        if started.elapsed() > timeout {
            reap(child);
            return Err("命令超时".to_string());
        }
        match child.try_wait() {
            Ok(Some(_)) => {
                return child
                    .wait_with_output()
                    .map_err(|error| format!("读取命令输出失败: {error}"));
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(40)),
            Err(error) => {
                reap(child);
                return Err(format!("等待命令失败: {error}"));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_version_from_banner() {
        assert_eq!(extract_version("claude 2.1.156\n"), "2.1.156");
        assert_eq!(extract_version("v1.2.3-beta.1"), "1.2.3-beta.1");
    }

    #[test]
    fn older_local_needs_update() {
        assert!(update_available("1.2.3", "1.2.4"));
        assert!(!update_available("1.2.4", "1.2.3"));
        assert!(update_available("2.0.0-beta.1", "2.0.0"));
    }
}
