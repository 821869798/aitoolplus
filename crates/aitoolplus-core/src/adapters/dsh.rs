//! DSH adapter bridging the trait to `dsh` runtime logic.

use crate::adapters::{AppliedReport, ApplyCtx, ApplyError, ToolAdapter};
use crate::dsh::{DshRuntimePaths, apply_provider as dsh_apply, import_runtime as dsh_import};
use crate::paths::Paths;
use crate::providers::ProviderRecord;
use crate::tools::ToolId;
use serde_json::Value;

pub struct DshAdapter;

impl ToolAdapter for DshAdapter {
    fn tool(&self) -> ToolId {
        ToolId::Dsh
    }

    fn apply(&self, ctx: &ApplyCtx) -> Result<AppliedReport, ApplyError> {
        let paths = DshRuntimePaths::from_paths(ctx.paths);
        let provider = ensure_key(ctx.provider);
        let mut files = dsh_apply(&paths, &provider).map_err(ApplyError::Message)?;
        if let Ok(common) = serde_json::from_str::<Value>(ctx.common_config)
            && common.as_object().is_some_and(|object| !object.is_empty())
        {
            crate::dsh::merge_other_settings(&paths, &common).map_err(ApplyError::Message)?;
            if !files.contains(&paths.settings) {
                files.push(paths.settings.clone());
            }
        }
        Ok(AppliedReport { files })
    }

    fn read_current(&self, paths: &Paths) -> Result<Value, ApplyError> {
        let d = DshRuntimePaths::from_paths(paths);
        let settings = std::fs::read_to_string(&d.settings).unwrap_or_default();
        let creds = if d.credentials.exists() {
            "<managed separately>".to_string()
        } else {
            String::new()
        };
        Ok(serde_json::json!({ "settingsYaml": settings, "credentials": creds }))
    }

    fn prompt_file(&self, paths: &Paths) -> Option<std::path::PathBuf> {
        Some(DshRuntimePaths::from_paths(paths).prompt)
    }
}

fn ensure_key(record: &ProviderRecord) -> ProviderRecord {
    if record.id.starts_with("dsh:") {
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
    out.id = format!("dsh:{key}");
    out
}

pub fn import_into(providers: &mut Vec<ProviderRecord>, paths: &Paths) -> usize {
    let d = DshRuntimePaths::from_paths(paths);
    dsh_import(&d, providers).unwrap_or(0)
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
            .insert(ToolId::Dsh, dir.path().join(".dsh"));
        let d = DshRuntimePaths::from_paths(&paths);
        std::fs::create_dir_all(&d.root).unwrap();

        let mut p = ProviderRecord::new("DeepSeek Main", "custom");
        p.set_settings(&json!({
            "name": "DeepSeek Main",
            "baseUrl": "https://api.deepseek.com",
            "apiKeyEnv": "DEEPSEEK_API_KEY",
            "apiKey": "sk-1",
            "defaultModel": "deepseek-chat"
        }));

        let ctx = ApplyCtx {
            paths: &paths,
            common_config: "{}",
            provider: &p,
            strategy: MergeStrategy::default(),
            provider_optional: false,
        };
        let report = DshAdapter.apply(&ctx).unwrap();
        assert_eq!(report.files.len(), 2);

        let mut imported = vec![];
        assert_eq!(import_into(&mut imported, &paths), 1);
        assert_eq!(imported[0].id, "dsh:deepseek-main");
        assert!(imported[0].is_applied);
    }
}
