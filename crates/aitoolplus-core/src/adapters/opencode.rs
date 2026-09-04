//! OpenCode adapter: `opencode.json` / `opencode.jsonc` single-file config.
//!
//! Ported from ai-toolbox `coding/open_code/AGENTS.md` semantics:
//! - Config file priority: explicit path (common_config.config_path) >
//!   `OPENCODE_CONFIG` env > default `~/.config/opencode/opencode.json`;
//!   `.jsonc` wins over `.json` when both exist (JSONC = JSON with comments)
//! - prompt file is `AGENTS.md` **beside the current config file** — never a
//!   hardcoded `~/.config/opencode/AGENTS.md`
//! - model values use `provider_id/model_id`; splitting only on the FIRST
//!   `/` (model ids may themselves contain slashes)
//! - writing must preserve unknown top-level fields (agents, tools, …)

use serde_json::{Map, Value};

use crate::adapters::{
    AppliedReport, ApplyCtx, ApplyError, ToolAdapter, backup_file, merged_payload, write_atomic,
};
use crate::paths::Paths;
use crate::tools::ToolId;

pub struct OpenCodeAdapter;

/// Resolve the effective config file path (upstream priority).
pub fn resolve_config_file(paths: &Paths) -> std::path::PathBuf {
    let root = paths.tool_root(ToolId::OpenCode);
    if let Ok(env) = std::env::var("OPENCODE_CONFIG")
        && !env.trim().is_empty()
    {
        let p = std::path::PathBuf::from(env);
        if p.is_absolute() {
            return p;
        }
    }
    let jsonc = root.join("opencode.jsonc");
    if jsonc.exists() {
        return jsonc;
    }
    root.join("opencode.json")
}

/// Parse JSONC (strip line/block comments outside strings).
fn parse_jsonc(raw: &str) -> Result<Value, String> {
    if raw.trim().is_empty() {
        return Ok(Value::Object(Map::new()));
    }
    let cleaned = strip_jsonc_comments(raw);
    serde_json::from_str(&cleaned).map_err(|e| format!("failed to parse config: {e}"))
}

fn strip_jsonc_comments(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut in_string = false;
    let mut chars = input.chars().peekable();
    while let Some(c) = chars.next() {
        if in_string {
            out.push(c);
            if c == '\\' {
                if let Some(&n) = chars.peek() {
                    out.push(n);
                    chars.next();
                }
            } else if c == '"' {
                in_string = false;
            }
            continue;
        }
        match c {
            '"' => {
                in_string = true;
                out.push(c);
            }
            '/' if chars.peek() == Some(&'/') => {
                for c2 in chars.by_ref() {
                    if c2 == '\n' {
                        out.push('\n');
                        break;
                    }
                }
            }
            '/' if chars.peek() == Some(&'*') => {
                chars.next(); // consume '*'
                // consume until the closing "*/" appears
                let mut prev: Option<char> = None;
                for c2 in chars.by_ref() {
                    if prev == Some('*') && c2 == '/' {
                        break;
                    }
                    prev = Some(c2);
                }
            }
            _ => out.push(c),
        }
    }
    out
}

/// Split an OpenCode model value on the FIRST slash only:
/// `zenmux/openai/gpt-5.5` -> provider `zenmux`, model `openai/gpt-5.5`.
pub fn split_model_value(value: &str) -> Option<(String, String)> {
    let idx = value.find('/')?;
    let provider = value[..idx].to_string();
    let model = value[idx + 1..].to_string();
    if provider.is_empty() || model.is_empty() {
        return None;
    }
    Some((provider, model))
}

impl ToolAdapter for OpenCodeAdapter {
    fn tool(&self) -> ToolId {
        ToolId::OpenCode
    }

    fn apply(&self, ctx: &ApplyCtx) -> Result<AppliedReport, ApplyError> {
        let path = resolve_config_file(ctx.paths);
        if path.exists() {
            backup_file(ctx.paths, ToolId::OpenCode, &path);
        }
        let payload = merged_payload(ctx);

        // deep-merge into the existing config, preserving unknown fields
        let current: Value = if path.exists() {
            parse_jsonc(&std::fs::read_to_string(&path).unwrap_or_default())
                .unwrap_or(Value::Object(Map::new()))
        } else {
            Value::Object(Map::new())
        };
        let mut merged = current;
        deep_merge(&mut merged, &payload);
        write_atomic(&path, &serde_json::to_string_pretty(&merged).unwrap())?;
        Ok(AppliedReport { files: vec![path] })
    }

    fn read_current(&self, paths: &Paths) -> Result<Value, ApplyError> {
        let path = resolve_config_file(paths);
        if !path.exists() {
            return Ok(Value::Object(Map::new()));
        }
        let raw = std::fs::read_to_string(&path)?;
        parse_jsonc(&raw).map_err(ApplyError::Message)
    }

    fn prompt_file(&self, paths: &Paths) -> Option<std::path::PathBuf> {
        // AGENTS.md lives beside the current config file — not a hardcoded path.
        let config = resolve_config_file(paths);
        Some(
            config
                .parent()
                .map(|d| d.join("AGENTS.md"))
                .unwrap_or_else(|| paths.tool_root(ToolId::OpenCode).join("AGENTS.md")),
        )
    }
}

fn deep_merge(base: &mut Value, patch: &Value) {
    match (base, patch) {
        (Value::Object(b), Value::Object(p)) => {
            for (k, v) in p {
                match b.get_mut(k) {
                    Some(existing) => deep_merge(existing, v),
                    None => {
                        b.insert(k.clone(), v.clone());
                    }
                }
            }
        }
        (b, p) => *b = p.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::MergeStrategy;
    use crate::providers::ProviderRecord;
    use serde_json::json;

    fn setup() -> (tempfile::TempDir, Paths) {
        let dir = tempfile::tempdir().unwrap();
        let paths = Paths::new(dir.path().join("home"), dir.path().join("data"));
        (dir, paths)
    }

    fn ctx<'a>(paths: &'a Paths, p: &'a ProviderRecord) -> ApplyCtx<'a> {
        ApplyCtx {
            paths,
            common_config: "{}",
            provider: p,
            strategy: MergeStrategy::default(),
            provider_optional: false,
        }
    }

    #[test]
    fn jsonc_takes_precedence_and_comments_parse() {
        let _guard = ENV_LOCK.lock().unwrap();
        let (_dir, paths) = setup();
        let root = paths.tool_root(ToolId::OpenCode);
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("opencode.json"), r#"{"model":"plain"}"#).unwrap();
        std::fs::write(
            root.join("opencode.jsonc"),
            "// leading comment\n{\n  /* block */\n  \"model\": \"anthropic/claude-sonnet-4\"\n}\n",
        )
        .unwrap();

        assert!(resolve_config_file(&paths).ends_with("opencode.jsonc"));

        let v = OpenCodeAdapter.read_current(&paths).unwrap();
        assert_eq!(v["model"], "anthropic/claude-sonnet-4");
    }

    #[test]
    fn apply_preserves_unknown_top_level_fields() {
        let (_dir, paths) = setup();
        let root = paths.tool_root(ToolId::OpenCode);
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(
            root.join("opencode.json"),
            r#"{"agents":{"coder":{"mode":"primary"}},"tools":{"web":true}}"#,
        )
        .unwrap();

        let mut p = ProviderRecord::new("Custom", "custom");
        p.set_settings(&json!({
            "provider": "anthropic",
            "apiKey": "sk-1",
            "model": "anthropic/claude-sonnet-4",
            "small_model": "anthropic/claude-haiku"
        }));
        OpenCodeAdapter.apply(&ctx(&paths, &p)).unwrap();

        let v: Value =
            serde_json::from_str(&std::fs::read_to_string(root.join("opencode.json")).unwrap())
                .unwrap();
        // provider payload landed
        assert_eq!(v["apiKey"], "sk-1");
        assert_eq!(v["model"], "anthropic/claude-sonnet-4");
        assert_eq!(v["small_model"], "anthropic/claude-haiku");
        // unknown sections survive
        assert_eq!(v["agents"]["coder"]["mode"], "primary");
        assert_eq!(v["tools"]["web"], true);
    }

    #[test]
    fn model_values_split_on_first_slash_only() {
        assert_eq!(
            split_model_value("zenmux/openai/gpt-5.5"),
            Some(("zenmux".into(), "openai/gpt-5.5".into()))
        );
        assert_eq!(
            split_model_value("anthropic/claude-sonnet-4"),
            Some(("anthropic".into(), "claude-sonnet-4".into()))
        );
        assert_eq!(split_model_value("noprovider"), None);
        assert_eq!(split_model_value("/leading"), None);
        assert_eq!(split_model_value("trailing/"), None);
    }

    #[test]
    fn prompt_sits_beside_current_config_file() {
        let (_dir, paths) = setup();
        let prompt = OpenCodeAdapter.prompt_file(&paths).unwrap();
        assert!(prompt.ends_with("opencode\\AGENTS.md") || prompt.ends_with("opencode/AGENTS.md"));
        assert!(!prompt.to_string_lossy().contains(".json"));
    }

    /// Env mutation is process-global; serialize this test so it never races
    /// with the other tests in this binary. (Edition 2024 makes env mutation
    /// unsafe because it can race with other threads.)
    static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn opencode_config_env_override_wins() {
        let _guard = ENV_LOCK.lock().unwrap();
        let (_dir, paths) = setup();
        let tmp = std::env::temp_dir().join("aitoolplus-test-opencode.json");
        std::fs::write(&tmp, "{}").unwrap();
        unsafe {
            std::env::set_var("OPENCODE_CONFIG", &tmp);
        }
        let resolved = resolve_config_file(&paths);
        unsafe {
            std::env::remove_var("OPENCODE_CONFIG");
        }
        assert_eq!(resolved, tmp);
    }
}
