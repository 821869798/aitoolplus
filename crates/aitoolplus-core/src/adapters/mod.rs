//! Per-tool adapters that translate a provider record into the real config
//! files on disk. Each adapter knows its files, merge semantics and starter
//! settings, mirroring ai-toolbox's `coding/<tool>/adapter.rs` modules.

use std::fs;
use std::path::Path;

use serde_json::Value;

use crate::config::{MergeStrategy, merged_config};
use crate::paths::Paths;
use crate::providers::{ProviderRecord, default_settings_for};
use crate::tools::ToolId;

pub mod claude_code;
pub mod claude_desktop;
pub mod codex;
pub mod dsh;
pub mod gemini_cli;
pub mod grok;
pub mod hermes;
pub mod kimi;
pub mod oh_my_pi;
pub mod openclaw;
pub mod opencode;
pub mod pi;

/// Everything an adapter needs to perform an apply.
pub struct ApplyCtx<'a> {
    pub paths: &'a Paths,
    pub common_config: &'a str,
    pub provider: &'a ProviderRecord,
    pub strategy: MergeStrategy,
    /// When true, apply without a provider (just persist common config).
    pub provider_optional: bool,
}

/// What an apply did: files touched, for display and backup pruning.
#[derive(Debug, Default, Clone)]
pub struct AppliedReport {
    pub files: Vec<std::path::PathBuf>,
}

#[derive(thiserror::Error, Debug)]
pub enum ApplyError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
    #[error("no provider selected")]
    NoProvider,
    #[error("{0}")]
    Message(String),
}

/// The trait every tool adapter implements.
pub trait ToolAdapter: Send + Sync {
    fn tool(&self) -> ToolId;

    /// Apply provider + common config to the real config files.
    fn apply(&self, ctx: &ApplyCtx) -> Result<AppliedReport, ApplyError>;

    /// Read the current primary config file as JSON for display
    /// (missing file -> empty object).
    fn read_current(&self, paths: &Paths) -> Result<Value, ApplyError> {
        let path = paths.primary_config(self.tool());
        if !path.exists() {
            return Ok(Value::Object(Default::default()));
        }
        let raw = fs::read_to_string(&path)?;
        Ok(serde_json::from_str(&raw).unwrap_or(Value::Object(Default::default())))
    }

    /// Starter settings for the "new provider" dialog.
    fn default_settings(&self) -> Value {
        default_settings_for(self.tool())
    }

    /// Runtime files to preview in the tool's Runtime tab (label, path).
    fn runtime_files(&self, paths: &Paths) -> Vec<(String, std::path::PathBuf)> {
        let tool = self.tool();
        paths
            .config_files(tool)
            .into_iter()
            .map(|p| {
                let label = p
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("config")
                    .to_string();
                (label, p)
            })
            .collect()
    }

    /// Prompt file path for this tool, if it supports a global prompt file.
    fn prompt_file(&self, _paths: &Paths) -> Option<std::path::PathBuf> {
        None
    }
}

pub struct AgentsAdapter;
impl ToolAdapter for AgentsAdapter {
    fn tool(&self) -> ToolId {
        ToolId::Agents
    }
    fn apply(&self, _ctx: &ApplyCtx) -> Result<AppliedReport, ApplyError> {
        Ok(AppliedReport::default())
    }
}

/// Registry: fetch the adapter for a tool.
pub fn adapter_for(tool: ToolId) -> Box<dyn ToolAdapter> {
    match tool {
        ToolId::ClaudeCode => Box::new(claude_code::ClaudeCodeAdapter),
        ToolId::Agents => Box::new(AgentsAdapter),
        ToolId::Codex => Box::new(codex::CodexAdapter),
        ToolId::GeminiCli => Box::new(gemini_cli::GeminiCliAdapter),
        ToolId::Grok => Box::new(grok::GrokAdapter),
        ToolId::Kimi => Box::new(kimi::KimiAdapter),
        ToolId::OpenCode => Box::new(opencode::OpenCodeAdapter),
        ToolId::OpenClaw => Box::new(openclaw::OpenClawAdapter),
        ToolId::Pi => Box::new(pi::PiAdapter),
        ToolId::OhMyPi => Box::new(oh_my_pi::OhMyPiAdapter),
        ToolId::ClaudeDesktop => Box::new(claude_desktop::ClaudeDesktopAdapter),
        ToolId::Hermes => Box::new(hermes::HermesAdapter),
        ToolId::Dsh => Box::new(dsh::DshAdapter),
    }
}

/// Apply flow shared by all adapters: merge common+provider, write with
/// backup. Adapters mostly implement `write_config` below via their `apply`.
pub fn merged_payload(ctx: &ApplyCtx) -> Value {
    if ctx.provider_optional {
        merged_config(ctx.common_config, "{}", ctx.strategy)
    } else {
        merged_config(
            ctx.common_config,
            &ctx.provider.settings_config,
            ctx.strategy,
        )
    }
}

/// Backup `path` into the app backups dir before overwriting, keeping the
/// newest `KEEP` copies.
const BACKUP_KEEP: usize = 20;

pub fn backup_file(paths: &Paths, tool: ToolId, path: &Path) {
    let Some(filename) = path.file_name().and_then(|n| n.to_str()) else {
        return;
    };
    let dir = paths.backups_dir(tool);
    if fs::create_dir_all(&dir).is_err() {
        return;
    }
    let ts = chrono::Local::now().format("%Y%m%d%H%M%S%3f");
    let dest = dir.join(format!("{ts}-{filename}"));
    let _ = fs::copy(path, &dest);
    prune_backups(&dir, BACKUP_KEEP);
}

fn prune_backups(dir: &Path, keep: usize) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let mut files: Vec<_> = entries
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().map(|t| t.is_file()).unwrap_or(false))
        .map(|e| (e.file_name().to_string_lossy().to_string(), e.path()))
        .collect();
    if files.len() <= keep {
        return;
    }
    files.sort();
    let excess = files.len() - keep;
    for (_, path) in files.into_iter().take(excess) {
        let _ = fs::remove_file(path);
    }
}

/// Write a text file atomically (tmp + rename), creating parents.
pub fn write_atomic(path: &Path, contents: &str) -> Result<(), ApplyError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("aitoolplus.tmp");
    fs::write(&tmp, contents)?;
    fs::rename(&tmp, path)?;
    Ok(())
}

/// JSON-file merge write: read current (tolerant), deep-merge layer, write.
pub fn merge_write_json(path: &Path, layer: &Value) -> Result<(), ApplyError> {
    let current = if path.exists() {
        fs::read_to_string(path).unwrap_or_default()
    } else {
        String::new()
    };
    let merged = crate::config::merge_into_raw(&current, layer);
    write_atomic(path, &merged)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn merged_payload_without_provider_is_common_only() {
        let p = ProviderRecord::new("t", "custom");
        let ctx = ApplyCtx {
            paths: &Paths::new("/tmp", "/tmp/appdata"),
            common_config: "{\"a\":1}",
            provider: &p,
            strategy: MergeStrategy::default(),
            provider_optional: true,
        };
        let m = merged_payload(&ctx);
        assert_eq!(m["a"], 1);
    }

    #[test]
    fn merged_payload_merges_provider_over_common() {
        let mut p = ProviderRecord::new("t", "custom");
        p.set_settings(&json!({"a": 2}));
        let ctx = ApplyCtx {
            paths: &Paths::new("/tmp", "/tmp/appdata"),
            common_config: "{\"a\":1,\"b\":true}",
            provider: &p,
            strategy: MergeStrategy::default(),
            provider_optional: false,
        };
        let m = merged_payload(&ctx);
        assert_eq!(m["a"], 2);
        assert_eq!(m["b"], true);
    }

    #[test]
    fn merge_write_json_keeps_unrelated_keys() {
        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join("c.json");
        fs::write(&f, r#"{"keep": 5}"#).unwrap();
        merge_write_json(&f, &json!({"new": 6})).unwrap();
        let v: Value = serde_json::from_str(&fs::read_to_string(&f).unwrap()).unwrap();
        assert_eq!(v["keep"], 5);
        assert_eq!(v["new"], 6);
    }

    #[test]
    fn backup_and_prune() {
        let dir = tempfile::tempdir().unwrap();
        let paths = Paths::new(dir.path(), dir.path().join("app"));
        let tool = ToolId::Codex;
        let target = dir.path().join("config.toml");
        fs::write(&target, "x=1").unwrap();
        for i in 0..25 {
            fs::write(&target, format!("x={i}")).unwrap();
            backup_file(&paths, tool, &target);
            std::thread::sleep(std::time::Duration::from_millis(3));
        }
        let count = fs::read_dir(paths.backups_dir(tool))
            .unwrap()
            .filter_map(|e| e.ok())
            .count();
        assert!(count <= BACKUP_KEEP, "too many backups: {count}");
    }

    #[test]
    fn test_apply_updated_provider_takes_effect_on_disk() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().join("home");
        let data = dir.path().join("data");
        let paths = Paths::new(&home, &data);

        // 1. ClaudeCode: initial apply, then update and re-apply
        let mut p_claude = ProviderRecord::new("Claude Initial", "custom");
        p_claude.set_settings(&json!({
            "env": {
                "ANTHROPIC_BASE_URL": "https://init.example.com",
                "ANTHROPIC_AUTH_TOKEN": "init-key"
            }
        }));
        let ctx1 = ApplyCtx {
            paths: &paths,
            common_config: "{}",
            provider: &p_claude,
            strategy: MergeStrategy::default(),
            provider_optional: false,
        };
        adapter_for(ToolId::ClaudeCode).apply(&ctx1).unwrap();
        let claude_cfg = paths.primary_config(ToolId::ClaudeCode);
        let val1: Value = serde_json::from_str(&fs::read_to_string(&claude_cfg).unwrap()).unwrap();
        assert_eq!(val1["env"]["ANTHROPIC_BASE_URL"], "https://init.example.com");

        // Now modify provider: new base URL & key
        p_claude.set_settings(&json!({
            "env": {
                "ANTHROPIC_BASE_URL": "https://updated.example.com",
                "ANTHROPIC_AUTH_TOKEN": "updated-key"
            }
        }));
        let ctx2 = ApplyCtx {
            paths: &paths,
            common_config: "{}",
            provider: &p_claude,
            strategy: MergeStrategy::default(),
            provider_optional: false,
        };
        adapter_for(ToolId::ClaudeCode).apply(&ctx2).unwrap();
        let val2: Value = serde_json::from_str(&fs::read_to_string(&claude_cfg).unwrap()).unwrap();
        assert_eq!(val2["env"]["ANTHROPIC_BASE_URL"], "https://updated.example.com");
        assert_eq!(val2["env"]["ANTHROPIC_AUTH_TOKEN"], "updated-key");

        // 2. Codex: initial apply, then update model and base_url
        let mut p_codex = ProviderRecord::new("Codex Initial", "custom");
        p_codex.settings_config = r#"{"config":"model = \"gpt-4o\"\n[model_providers.custom]\nbase_url = \"https://init-codex.com\"\napi_key = \"k1\""}"#.into();
        let codex_ctx1 = ApplyCtx {
            paths: &paths,
            common_config: "",
            provider: &p_codex,
            strategy: MergeStrategy::default(),
            provider_optional: false,
        };
        adapter_for(ToolId::Codex).apply(&codex_ctx1).unwrap();
        let codex_cfg = paths.tool_root(ToolId::Codex).join("config.toml");
        let codex_content1 = fs::read_to_string(&codex_cfg).unwrap();
        assert!(codex_content1.contains("https://init-codex.com"));

        // Now update Codex provider
        p_codex.settings_config = r#"{"config":"model = \"o3-mini\"\n[model_providers.custom]\nbase_url = \"https://updated-codex.com\"\napi_key = \"k2\""}"#.into();
        let codex_ctx2 = ApplyCtx {
            paths: &paths,
            common_config: "",
            provider: &p_codex,
            strategy: MergeStrategy::default(),
            provider_optional: false,
        };
        adapter_for(ToolId::Codex).apply(&codex_ctx2).unwrap();
        let codex_content2 = fs::read_to_string(&codex_cfg).unwrap();
        assert!(codex_content2.contains("https://updated-codex.com"));
        assert!(codex_content2.contains("o3-mini"));
    }
}
