//! Oh My Pi adapter bridging the trait to `oh_my_pi` runtime logic.

use crate::adapters::{AppliedReport, ApplyCtx, ApplyError, ToolAdapter};
use crate::oh_my_pi::{OmpRuntimePaths, apply_provider as omp_apply, import_runtime as omp_import};
use crate::paths::Paths;
use crate::providers::ProviderRecord;
use crate::tools::ToolId;
use serde_json::Value;

pub struct OhMyPiAdapter;

impl ToolAdapter for OhMyPiAdapter {
    fn tool(&self) -> ToolId {
        ToolId::OhMyPi
    }

    fn apply(&self, ctx: &ApplyCtx) -> Result<AppliedReport, ApplyError> {
        let paths = OmpRuntimePaths::from_paths(ctx.paths);
        // New records created in the UI don't have an "omp:" key yet; derive
        // one from the provider name so first-apply persists a stable id.
        let provider = ensure_omp_key(ctx.provider);
        let mut files = omp_apply(&paths, &provider).map_err(ApplyError::Message)?;
        if let Ok(common) = serde_json::from_str::<Value>(ctx.common_config)
            && common.as_object().is_some_and(|object| !object.is_empty())
        {
            crate::oh_my_pi::merge_other_settings(&paths, &common).map_err(ApplyError::Message)?;
            files.push(paths.config.clone());
        }
        Ok(AppliedReport { files })
    }

    fn read_current(&self, paths: &Paths) -> Result<Value, ApplyError> {
        let omp = OmpRuntimePaths::from_paths(paths);
        let models = std::fs::read_to_string(&omp.models).unwrap_or_default();
        let config = std::fs::read_to_string(&omp.config).unwrap_or_default();
        Ok(serde_json::json!({
            "modelsYml": models,
            "configYml": config,
        }))
    }

    fn prompt_file(&self, paths: &Paths) -> Option<std::path::PathBuf> {
        Some(OmpRuntimePaths::from_paths(paths).prompt)
    }
}

/// Records created in-app carry a uuid id; give them an "omp:<name>" id on
/// first apply so later imports can reconcile them with the runtime file.
fn ensure_omp_key(record: &ProviderRecord) -> ProviderRecord {
    if record.id.starts_with("omp:") {
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
    out.id = format!("omp:{key}");
    out
}

/// Import runtime providers into a section (used at startup).
pub fn import_into(providers: &mut Vec<ProviderRecord>, paths: &Paths) -> usize {
    let omp = OmpRuntimePaths::from_paths(paths);
    omp_import(&omp, providers).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::MergeStrategy;
    use serde_json::json;

    #[test]
    fn apply_via_trait_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let paths = Paths::new(dir.path().join("home"), dir.path().join("data"));
        std::fs::create_dir_all(paths.tool_root(ToolId::OhMyPi)).unwrap();

        let mut p = ProviderRecord::new("Alpha Gateway", "custom");
        p.set_settings(&json!({"name": "Alpha Gateway", "baseUrl": "https://a/v1"}));

        let ctx = ApplyCtx {
            paths: &paths,
            common_config: "{}",
            provider: &p,
            strategy: MergeStrategy::default(),
            provider_optional: false,
        };
        let report = OhMyPiAdapter.apply(&ctx).unwrap();
        assert_eq!(report.files.len(), 1);
        let out = std::fs::read_to_string(&report.files[0]).unwrap();
        assert!(out.contains("alpha-gateway"), "key missing: {out}");
        assert!(out.contains("baseUrl: https://a/v1"));
    }
}
