//! Tool identifiers, one per managed CLI.

use serde::{Deserialize, Serialize};

/// The managed coding CLIs, mirroring ai-toolbox's module list (first-party
/// subset; extend by pushing a new variant + adapter).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum ToolId {
    ClaudeCode,
    Codex,
    GeminiCli,
    Grok,
    Kimi,
    OpenCode,
    OpenClaw,
    Pi,
    OhMyPi,
    ClaudeDesktop,
    Hermes,
    Dsh,
}

impl ToolId {
    /// Stable lowercase key used in store JSON and URLs.
    pub const fn key(self) -> &'static str {
        match self {
            ToolId::ClaudeCode => "claude_code",
            ToolId::Codex => "codex",
            ToolId::GeminiCli => "gemini_cli",
            ToolId::Grok => "grok",
            ToolId::Kimi => "kimi",
            ToolId::OpenCode => "opencode",
            ToolId::OpenClaw => "openclaw",
            ToolId::Pi => "pi",
            ToolId::OhMyPi => "oh_my_pi",
            ToolId::ClaudeDesktop => "claude_desktop",
            ToolId::Hermes => "hermes",
            ToolId::Dsh => "dsh",
        }
    }

    /// English display name.
    pub const fn name_en(self) -> &'static str {
        match self {
            ToolId::ClaudeCode => "Claude Code",
            ToolId::Codex => "Codex",
            ToolId::GeminiCli => "Gemini CLI",
            ToolId::Grok => "Grok",
            ToolId::Kimi => "Kimi",
            ToolId::OpenCode => "OpenCode",
            ToolId::OpenClaw => "OpenClaw",
            ToolId::Pi => "Pi",
            ToolId::OhMyPi => "Oh My Pi",
            ToolId::ClaudeDesktop => "Claude Desktop",
            ToolId::Hermes => "Hermes",
            ToolId::Dsh => "DSH",
        }
    }

    /// Chinese display name.
    pub const fn name_zh(self) -> &'static str {
        match self {
            ToolId::ClaudeCode => "Claude Code",
            ToolId::Codex => "Codex",
            ToolId::GeminiCli => "Gemini CLI",
            ToolId::Grok => "Grok",
            ToolId::Kimi => "Kimi",
            ToolId::OpenCode => "OpenCode",
            ToolId::OpenClaw => "OpenClaw",
            ToolId::Pi => "Pi",
            ToolId::OhMyPi => "Oh My Pi",
            ToolId::ClaudeDesktop => "Claude Desktop",
            ToolId::Hermes => "Hermes",
            ToolId::Dsh => "DSH",
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        Some(match key {
            "claude_code" | "claudecode" | "claude" => ToolId::ClaudeCode,
            "codex" => ToolId::Codex,
            "gemini_cli" | "gemini" | "geminicli" => ToolId::GeminiCli,
            "grok" => ToolId::Grok,
            "kimi" => ToolId::Kimi,
            "opencode" | "open_code" => ToolId::OpenCode,
            "openclaw" | "open_claw" => ToolId::OpenClaw,
            "pi" => ToolId::Pi,
            "oh_my_pi" | "omp" | "ohmypi" => ToolId::OhMyPi,
            "claude_desktop" | "claudedesktop" => ToolId::ClaudeDesktop,
            "hermes" => ToolId::Hermes,
            "dsh" | "deepseek_harness" => ToolId::Dsh,
            _ => return None,
        })
    }

    pub const ALL: [ToolId; 11] = [
        ToolId::ClaudeCode,
        ToolId::Codex,
        ToolId::Grok,
        ToolId::Kimi,
        ToolId::OpenCode,
        ToolId::OpenClaw,
        ToolId::Pi,
        ToolId::OhMyPi,
        ToolId::ClaudeDesktop,
        ToolId::Hermes,
        ToolId::Dsh,
    ];
}

impl std::fmt::Display for ToolId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.key())
    }
}
