//! Hermes adapter bridging the trait to `hermes` runtime logic.

use crate::adapters::{AppliedReport, ApplyCtx, ApplyError, ToolAdapter};
use crate::hermes::{
    HermesRuntimePaths, apply_provider as hermes_apply, import_runtime as hermes_import,
};
use crate::paths::Paths;
use crate::providers::ProviderRecord;
use crate::tools::ToolId;
use serde_json::Value;

pub struct HermesAdapter;

impl ToolAdapter for HermesAdapter {
    fn tool(&self) -> ToolId {
        ToolId::Hermes
    }

    fn apply(&self, ctx: &ApplyCtx) -> Result<AppliedReport, ApplyError> {
        let paths = HermesRuntimePaths::from_paths(ctx.paths);
        let provider = ensure_key(ctx.provider);
        let mut files = hermes_apply(&paths, &provider).map_err(ApplyError::Message)?;
        if let Ok(common) = serde_json::from_str::<Value>(ctx.common_config)
            && common.as_object().is_some_and(|object| !object.is_empty())
        {
            crate::hermes::merge_other_settings(&paths, &common).map_err(ApplyError::Message)?;
            if !files.contains(&paths.config) {
                files.push(paths.config.clone());
            }
        }
        Ok(AppliedReport { files })
    }

    fn read_current(&self, paths: &Paths) -> Result<Value, ApplyError> {
        let h = HermesRuntimePaths::from_paths(paths);
        let raw = std::fs::read_to_string(&h.config).unwrap_or_default();
        Ok(serde_json::json!({ "configYaml": raw }))
    }

    fn prompt_file(&self, paths: &Paths) -> Option<std::path::PathBuf> {
        Some(HermesRuntimePaths::from_paths(paths).prompt)
    }
}

fn ensure_key(record: &ProviderRecord) -> ProviderRecord {
    if record.id.starts_with("hermes:") {
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
        key = format!("p-{}", chrono::Local::now().format("%H%M%S%3f"));
    }
    out.id = format!("hermes:{key}");
    out
}

pub fn import_into(providers: &mut Vec<ProviderRecord>, paths: &Paths) -> usize {
    let h = HermesRuntimePaths::from_paths(paths);
    hermes_import(&h, providers).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::MergeStrategy;
    use serde_json::json;

    #[test]
    fn apply_via_trait_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let mut paths = Paths::new(dir.path().join("home"), dir.path().join("data"));
        paths
            .tool_roots
            .insert(ToolId::Hermes, dir.path().join(".hermes"));
        let h = HermesRuntimePaths::from_paths(&paths);
        std::fs::create_dir_all(&h.root).unwrap();

        let mut p = ProviderRecord::new("Alpha One", "custom");
        p.set_settings(&json!({
            "name": "Alpha One", "base_url": "https://a/v1", "api_key": "k", "model": "m1"
        }));

        let ctx = ApplyCtx {
            paths: &paths,
            common_config: "{}",
            provider: &p,
            strategy: MergeStrategy::default(),
            provider_optional: false,
        };
        let report = HermesAdapter.apply(&ctx).unwrap();
        assert_eq!(report.files.len(), 1);

        let mut imported = vec![];
        assert_eq!(import_into(&mut imported, &paths), 1);
        assert_eq!(imported[0].id, "hermes:alpha-one");
        assert!(imported[0].is_applied);
        assert_eq!(imported[0].settings()["model"], "m1");
    }
}
