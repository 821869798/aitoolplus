//! Claude Desktop adapter bridging the trait to profile logic.

use crate::adapters::{AppliedReport, ApplyCtx, ApplyError, ToolAdapter};
use crate::claude_desktop::{ClaudeDesktopPaths, apply_profile, import_runtime, read_state};
use crate::paths::Paths;
use crate::providers::ProviderRecord;
use crate::tools::ToolId;
use serde_json::Value;

pub struct ClaudeDesktopAdapter;

impl ToolAdapter for ClaudeDesktopAdapter {
    fn tool(&self) -> ToolId {
        ToolId::ClaudeDesktop
    }

    fn apply(&self, ctx: &ApplyCtx) -> Result<AppliedReport, ApplyError> {
        let paths = ClaudeDesktopPaths::from_paths(ctx.paths);
        let provider = ensure_profile_key(ctx.provider);
        apply_profile(&paths, &provider)
            .map(|files| AppliedReport { files })
            .map_err(ApplyError::Message)
    }

    fn read_current(&self, paths: &Paths) -> Result<Value, ApplyError> {
        let paths = ClaudeDesktopPaths::from_paths(paths);
        read_state(&paths).map_err(ApplyError::Message)
    }

    fn prompt_file(&self, _paths: &Paths) -> Option<std::path::PathBuf> {
        None
    }
}

fn ensure_profile_key(record: &ProviderRecord) -> ProviderRecord {
    if record.id.starts_with("cd:") {
        return record.clone();
    }
    let mut out = record.clone();
    let key: String = record
        .name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' {
                c
            } else {
                '-'
            }
        })
        .collect();
    let mut key = key.trim_matches('-').to_lowercase();
    if key.is_empty() {
        key = format!("profile-{}", chrono::Local::now().format("%H%M%S%3f"));
    }
    out.id = format!("cd:{key}");
    out
}

/// Import existing `configLibrary` profiles at startup.
pub fn import_into(providers: &mut Vec<ProviderRecord>, paths: &Paths) -> usize {
    let cd = ClaudeDesktopPaths::from_paths(paths);
    import_runtime(&cd, providers).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::MergeStrategy;
    use serde_json::json;

    #[test]
    fn apply_via_trait_writes_live_and_meta() {
        let dir = tempfile::tempdir().unwrap();
        let mut paths = Paths::new(dir.path().join("home"), dir.path().join("data"));
        paths
            .tool_roots
            .insert(ToolId::ClaudeDesktop, dir.path().join("Claude"));
        let cd = ClaudeDesktopPaths::from_paths(&paths);
        std::fs::create_dir_all(&cd.config_library).unwrap();

        let mut p = ProviderRecord::new("My GW", "custom");
        p.set_settings(&json!({
            "inferenceGatewayBaseUrl": "https://gw.example.com",
            "inferenceModels": ["m1"]
        }));

        let ctx = ApplyCtx {
            paths: &paths,
            common_config: "{}",
            provider: &p,
            strategy: MergeStrategy::default(),
            provider_optional: false,
        };
        let report = ClaudeDesktopAdapter.apply(&ctx).unwrap();
        assert_eq!(report.files.len(), 2);

        let mut imported = vec![];
        assert_eq!(import_into(&mut imported, &paths), 1);
        assert_eq!(imported[0].id, "cd:my-gw");
        assert!(imported[0].is_applied);
    }
}
