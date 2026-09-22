//! Launch managed CLIs after applying a provider.

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::paths::Paths;
use crate::tools::ToolId;

pub fn command_name(tool: ToolId) -> Option<&'static str> {
    Some(match tool {
        ToolId::ClaudeCode => "claude",
        ToolId::Codex => "codex",
        ToolId::GeminiCli => "gemini",
        ToolId::Grok => "grok",
        ToolId::Kimi => "kimi",
        ToolId::OpenCode => "opencode",
        ToolId::OpenClaw => "openclaw",
        ToolId::Pi => "pi",
        ToolId::OhMyPi => "omp",
        ToolId::Hermes => "hermes",
        ToolId::Dsh => "dsh",
        ToolId::ClaudeDesktop | ToolId::Agents => return None,
    })
}

pub fn resolve_command(tool: ToolId) -> Option<PathBuf> {
    let command = command_name(tool)?;
    if let Ok(path) = std::env::var(format!("AITOOLPLUS_CLI_{}", command.to_ascii_uppercase()))
        && PathBuf::from(&path).is_file()
    {
        return Some(PathBuf::from(path));
    }
    let locator = if cfg!(windows) { "where" } else { "which" };
    let output = Command::new(locator).arg(command).output().ok()?;
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .next()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(PathBuf::from)
}

pub fn launch(
    paths: &Paths,
    tool: ToolId,
    working_directory: Option<&Path>,
    claude_full_access: bool,
) -> Result<u32, String> {
    let command =
        resolve_command(tool).ok_or_else(|| format!("{} CLI not found", tool.name_en()))?;
    let cwd = working_directory.unwrap_or(&paths.home);
    if !cwd.is_dir() {
        return Err(format!(
            "working directory does not exist: {}",
            cwd.display()
        ));
    }
    let mut arguments: Vec<String> = vec![];
    if tool == ToolId::ClaudeCode && claude_full_access {
        arguments.push("--dangerously-skip-permissions".into());
    }

    #[cfg(windows)]
    {
        let mut shell = Command::new("cmd");
        shell
            .args(["/d", "/c", "start", "", "/d", &cwd.to_string_lossy()])
            .arg(&command)
            .args(&arguments);
        let child = shell
            .spawn()
            .map_err(|error| format!("launch failed: {error}"))?;
        Ok(child.id())
    }
    #[cfg(not(windows))]
    {
        let child = Command::new(&command)
            .args(&arguments)
            .current_dir(cwd)
            .spawn()
            .map_err(|error| format!("launch failed: {error}"))?;
        Ok(child.id())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commands_cover_all_cli_tools() {
        for tool in ToolId::ALL {
            if tool == ToolId::ClaudeDesktop {
                assert!(command_name(tool).is_none());
            } else {
                assert!(command_name(tool).is_some(), "missing command for {tool}");
            }
        }
    }
}
