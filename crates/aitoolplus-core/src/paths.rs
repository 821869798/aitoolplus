//! Home-directory and CLI config-path resolution.
//!
//! `Paths` is a plain value that can be constructed against a temp dir in
//! tests. Production uses [`Paths::system`], which honours `AITOOLPLUS_HOME`
//! as a test/override hook, then `USERPROFILE`/`HOME`.

use std::path::{Path, PathBuf};

use crate::ToolId;

#[derive(Debug, Clone)]
pub struct Paths {
    /// User home directory (`~`).
    pub home: PathBuf,
    /// App data dir (store, backups, skills central repo, logs).
    pub app_data: PathBuf,
    /// Per-tool root override, e.g. `AITOOLPLUS_CLAUDE_ROOT`.
    pub tool_roots: std::collections::HashMap<ToolId, PathBuf>,
}

impl Paths {
    /// Resolve against the real system environment.
    pub fn system() -> Self {
        let home = std::env::var("AITOOLPLUS_HOME")
            .ok()
            .filter(|s| !s.is_empty())
            .map(PathBuf::from)
            .or_else(home_dir)
            .unwrap_or_else(|| PathBuf::from("."));

        let app_data = std::env::var("AITOOLPLUS_APPDATA")
            .ok()
            .filter(|s| !s.is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| default_app_data(&home));

        let mut tool_roots = std::collections::HashMap::new();
        for tool in ToolId::ALL {
            let var = format!("AITOOLPLUS_{}_ROOT", tool.key().to_uppercase());
            if let Ok(v) = std::env::var(&var)
                && !v.is_empty()
            {
                tool_roots.insert(tool, PathBuf::from(v));
            }
        }

        Self {
            home,
            app_data,
            tool_roots,
        }
    }

    /// Explicit construction (tests).
    pub fn new(home: impl Into<PathBuf>, app_data: impl Into<PathBuf>) -> Self {
        Self {
            home: home.into(),
            app_data: app_data.into(),
            tool_roots: Default::default(),
        }
    }

    pub fn with_tool_root(mut self, tool: ToolId, root: impl Into<PathBuf>) -> Self {
        self.tool_roots.insert(tool, root.into());
        self
    }

    /// The tool's config root dir: override > platform default.
    pub fn tool_root(&self, tool: ToolId) -> PathBuf {
        if let Some(root) = self.tool_roots.get(&tool) {
            return root.clone();
        }
        let home = &self.home;
        match tool {
            ToolId::ClaudeCode => home.join(".claude"),
            ToolId::Codex => home.join(".codex"),
            ToolId::GeminiCli => home.join(".gemini"),
            ToolId::Grok => home.join(".grok"),
            ToolId::Kimi => home.join(".kimi-code"),
            // opencode follows the XDG convention on all platforms
            ToolId::OpenCode => home.join(".config").join("opencode"),
            ToolId::OpenClaw => home.join(".openclaw"),
            ToolId::Pi => home.join(".pi").join("agent"),
            ToolId::OhMyPi => home.join(".config").join("oh-my-pi"),
            ToolId::ClaudeDesktop => crate::claude_desktop::claude_desktop_root(self),
            ToolId::Hermes => crate::hermes::hermes_root(self),
            ToolId::Dsh => crate::dsh::dsh_root(self),
        }
    }

    /// Main config file(s) per tool.
    pub fn config_files(&self, tool: ToolId) -> Vec<PathBuf> {
        match tool {
            ToolId::ClaudeCode => vec![self.tool_root(tool).join("settings.json")],
            ToolId::Codex => vec![
                self.tool_root(tool).join("config.toml"),
                self.tool_root(tool).join("auth.json"),
            ],
            ToolId::GeminiCli => vec![
                self.tool_root(tool).join(".env"),
                self.tool_root(tool).join("settings.json"),
            ],
            ToolId::Grok => vec![self.tool_root(tool).join("config.toml")],
            ToolId::Kimi => vec![self.tool_root(tool).join("config.toml")],
            ToolId::OpenCode => vec![self.tool_root(tool).join("opencode.json")],
            ToolId::OpenClaw => vec![self.tool_root(tool).join("openclaw.json")],
            ToolId::Pi => vec![self.tool_root(tool).join("settings.json")],
            ToolId::OhMyPi => vec![
                self.tool_root(tool).join("models.yml"),
                self.tool_root(tool).join("config.yml"),
            ],
            ToolId::ClaudeDesktop => vec![
                crate::claude_desktop::claude_desktop_root(self).join("claude_desktop_config.json"),
            ],
            ToolId::Hermes => vec![self.tool_root(tool).join("config.yaml")],
            ToolId::Dsh => vec![self.tool_root(tool).join("settings.yaml")],
        }
    }

    /// The primary config file (the one shown in the generic config editor).
    pub fn primary_config(&self, tool: ToolId) -> PathBuf {
        self.config_files(tool)
            .first()
            .cloned()
            .unwrap_or_else(|| self.tool_root(tool).join("config.json"))
    }

    /// MCP config target per tool.
    pub fn mcp_target(&self, tool: ToolId) -> Option<PathBuf> {
        Some(match tool {
            ToolId::ClaudeCode => self.home.join(".claude.json"),
            ToolId::Codex => self.tool_root(tool).join("config.toml"),
            ToolId::GeminiCli => self.tool_root(tool).join("settings.json"),
            ToolId::OpenCode => self.tool_root(tool).join("opencode.json"),
            _ => return None,
        })
    }

    /// Skills install dir per tool.
    pub fn skills_dir(&self, tool: ToolId) -> Option<PathBuf> {
        Some(match tool {
            ToolId::ClaudeCode => self.tool_root(tool).join("skills"),
            ToolId::Codex => self.tool_root(tool).join("skills"),
            ToolId::Pi => self.tool_root(tool).join("skills"),
            _ => return None,
        })
    }

    /// Claude Code global MCP lives in `~/.claude.json`, not `~/.claude/`.
    pub fn claude_global_json(&self) -> PathBuf {
        self.home.join(".claude.json")
    }

    pub fn backups_dir(&self, tool: ToolId) -> PathBuf {
        self.app_data.join("backups").join(tool.key())
    }

    /// Central skill repository managed by this app.
    pub fn central_skills_dir(&self) -> PathBuf {
        self.app_data.join("skills")
    }

    pub fn store_file(&self) -> PathBuf {
        self.app_data.join("store.json")
    }

    pub fn settings_file(&self) -> PathBuf {
        self.app_data.join("settings.json")
    }
}

/// The user home dir on this OS.
pub fn home_dir() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("USERPROFILE") {
        let p = PathBuf::from(dir);
        if p.is_absolute() {
            return Some(p);
        }
    }
    if let Some(dir) = std::env::var_os("HOME") {
        let p = PathBuf::from(dir);
        if p.is_absolute() {
            return Some(p);
        }
    }
    None
}

fn default_app_data(home: &Path) -> PathBuf {
    if let Some(dir) = std::env::var_os("APPDATA") {
        let p = PathBuf::from(dir);
        if p.is_absolute() {
            return p.join("aitoolplus");
        }
    }
    home.join(".local").join("share").join("aitoolplus")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_roots_follow_platform_layout() {
        let paths = Paths::new("/home/dev", "/home/dev/.local/share/aitoolplus");
        assert_eq!(
            paths.tool_root(ToolId::ClaudeCode),
            PathBuf::from("/home/dev/.claude")
        );
        assert_eq!(
            paths.tool_root(ToolId::Pi),
            PathBuf::from("/home/dev/.pi/agent")
        );
        assert_eq!(
            paths.primary_config(ToolId::Codex),
            PathBuf::from("/home/dev/.codex/config.toml")
        );
    }

    #[test]
    fn env_override_wins() {
        let paths =
            Paths::new("/home/dev", "/data").with_tool_root(ToolId::ClaudeCode, "/custom/claude");
        assert_eq!(
            paths.tool_root(ToolId::ClaudeCode),
            PathBuf::from("/custom/claude")
        );
    }

    #[test]
    fn tool_ids_roundtrip() {
        for tool in ToolId::ALL {
            assert_eq!(ToolId::from_key(tool.key()), Some(tool));
        }
    }
}
