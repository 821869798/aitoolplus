//! OpenClaw adapter: single-file `~/.openclaw/openclaw.json`.
//!
//! OpenClaw is a config-FILE module (not a root module): every page section
//! edits the same JSON object, so writes must deep-merge and never drop
//! unknown top-level fields (upstream `open_claw/AGENTS.md`).

use crate::adapters::{
    AppliedReport, ApplyCtx, ApplyError, ToolAdapter, backup_file, merge_write_json, merged_payload,
};
use crate::tools::ToolId;

pub struct OpenClawAdapter;

impl ToolAdapter for OpenClawAdapter {
    fn tool(&self) -> ToolId {
        ToolId::OpenClaw
    }

    fn apply(&self, ctx: &ApplyCtx) -> Result<AppliedReport, ApplyError> {
        let path = ctx.paths.primary_config(ToolId::OpenClaw);
        if path.exists() {
            backup_file(ctx.paths, ToolId::OpenClaw, &path);
        }
        let payload = merged_payload(ctx);
        merge_write_json(&path, &payload)?;
        Ok(AppliedReport { files: vec![path] })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::MergeStrategy;
    use crate::paths::Paths;
    use crate::providers::ProviderRecord;
    use serde_json::json;

    #[test]
    fn writes_openclaw_json_not_config_json() {
        let dir = tempfile::tempdir().unwrap();
        let paths = Paths::new(dir.path().join("home"), dir.path().join("appdata"));
        let mut p = ProviderRecord::new("OC", "custom");
        p.set_settings(&json!({"model": "gpt-4o"}));
        let ctx = ApplyCtx {
            paths: &paths,
            common_config: "{}",
            provider: &p,
            strategy: MergeStrategy::default(),
            provider_optional: false,
        };
        let report = OpenClawAdapter.apply(&ctx).unwrap();
        assert!(
            report.files[0].ends_with("openclaw.json"),
            "wrong file: {:?}",
            report.files[0]
        );
        let v: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&report.files[0]).unwrap()).unwrap();
        assert_eq!(v["model"], "gpt-4o");
    }

    #[test]
    fn apply_preserves_other_sections_of_same_object() {
        let dir = tempfile::tempdir().unwrap();
        let paths = Paths::new(dir.path().join("home"), dir.path().join("appdata"));
        let cfg = paths.home.join(".openclaw").join("openclaw.json");
        std::fs::create_dir_all(cfg.parent().unwrap()).unwrap();
        std::fs::write(
            &cfg,
            r#"{"env":{"CLAW_KEY":"keep"},"tools":{"web":{"enabled":true}},"agents":{"defaults":{"temperature":0.7}}}"#,
        )
        .unwrap();

        let mut p = ProviderRecord::new("OC", "custom");
        p.set_settings(&json!({"model": "claude-sonnet-4"}));
        let ctx = ApplyCtx {
            paths: &paths,
            common_config: "{}",
            provider: &p,
            strategy: MergeStrategy::default(),
            provider_optional: false,
        };
        OpenClawAdapter.apply(&ctx).unwrap();

        let v: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&cfg).unwrap()).unwrap();
        // provider field written
        assert_eq!(v["model"], "claude-sonnet-4");
        // other sections of the same object survive
        assert_eq!(v["env"]["CLAW_KEY"], "keep");
        assert_eq!(v["tools"]["web"]["enabled"], true);
        assert_eq!(v["agents"]["defaults"]["temperature"], 0.7);
    }
}
