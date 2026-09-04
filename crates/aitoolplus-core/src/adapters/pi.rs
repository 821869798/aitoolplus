//! Pi adapter: `~/.pi/agent/settings.json` + prompt via `AGENTS.md`.

use crate::adapters::{AppliedReport, ApplyCtx, ApplyError, ToolAdapter};
use crate::paths::Paths;
use crate::tools::ToolId;

pub struct PiAdapter;

impl ToolAdapter for PiAdapter {
    fn tool(&self) -> ToolId {
        ToolId::Pi
    }

    fn apply(&self, ctx: &ApplyCtx) -> Result<AppliedReport, ApplyError> {
        crate::pi_runtime::apply_provider(ctx.paths, ctx.provider)
            .map(|files| AppliedReport { files })
            .map_err(ApplyError::Message)
    }

    fn prompt_file(&self, paths: &Paths) -> Option<std::path::PathBuf> {
        Some(paths.tool_root(ToolId::Pi).join("AGENTS.md"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::MergeStrategy;
    use crate::providers::ProviderRecord;
    use serde_json::Value;
    use serde_json::json;

    #[test]
    fn apply_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let paths = Paths::new(dir.path().join("home"), dir.path().join("appdata"));
        let mut p = ProviderRecord::new("PiX", "custom");
        p.id = "pi:pix".into();
        p.set_settings(&json!({"name":"PiX", "baseUrl": "https://x.example/v1", "apiKey": "sk-1", "models":[{"id":"m1"}]}));
        let ctx = ApplyCtx {
            paths: &paths,
            common_config: "{}",
            provider: &p,
            strategy: MergeStrategy::default(),
            provider_optional: false,
        };
        let _report = PiAdapter.apply(&ctx).unwrap();
        let v: Value = serde_json::from_str(
            &std::fs::read_to_string(paths.tool_root(ToolId::Pi).join("models.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(v["providers"]["pix"]["baseUrl"], "https://x.example/v1");
    }
}
