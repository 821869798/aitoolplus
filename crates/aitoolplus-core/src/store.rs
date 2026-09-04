//! Atomic, fault-tolerant JSON storage for the app database (`store.json`).

use std::fs;
use std::path::{Path, PathBuf};

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::paths::Paths;
use crate::tools::ToolId;

pub const SCHEMA_VERSION: u32 = 1;

/// The whole app database. Every section is versioned through
/// `schema_version`; loaders migrate forward.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Store {
    #[serde(default)]
    pub schema_version: u32,
    #[serde(default)]
    pub tools: std::collections::BTreeMap<String, ToolStore>,
    #[serde(default)]
    pub mcp: crate::mcp::McpStore,
    #[serde(default)]
    pub skills: crate::skills::SkillsStore,
    #[serde(default)]
    pub opencode_addons: crate::opencode_addons::OpenCodeAddonStore,
}

/// Per-tool section: providers, common config, prompts.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ToolStore {
    #[serde(default)]
    pub providers: Vec<crate::providers::ProviderRecord>,
    /// A JSON string layer merged under every provider config.
    #[serde(default)]
    pub common_config: String,
    #[serde(default)]
    pub prompts: Vec<crate::prompt::PromptRecord>,
}

impl Store {
    pub fn new() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            ..Default::default()
        }
    }

    /// Per-tool section, creating an empty one on first access.
    pub fn tool_mut(&mut self, tool: ToolId) -> &mut ToolStore {
        self.tools.entry(tool.key().to_string()).or_default()
    }

    /// Read-only per-tool section.
    pub fn tool(&self, tool: ToolId) -> ToolStore {
        self.tools.get(tool.key()).cloned().unwrap_or_default()
    }
}

pub struct StoreHandle {
    path: PathBuf,
    store: Store,
}

#[derive(thiserror::Error, Debug)]
pub enum StoreError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
}

impl StoreHandle {
    /// Load (or initialize) the store at `paths.store_file()`.
    pub fn open(paths: &Paths) -> Result<Self, StoreError> {
        let path = paths.store_file();
        let store = load_or_default(&path)?;
        Ok(Self { path, store })
    }

    pub fn store(&self) -> &Store {
        &self.store
    }

    pub fn store_mut(&mut self) -> &mut Store {
        &mut self.store
    }

    /// Mutate + persist atomically.
    pub fn update<F>(&mut self, f: F) -> Result<(), StoreError>
    where
        F: FnOnce(&mut Store),
    {
        f(&mut self.store);
        self.save()
    }

    pub fn save(&self) -> Result<(), StoreError> {
        save_json_atomic(&self.path, &self.store)
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Serialize the entire store for backup export.
    pub fn to_backup_json(&self) -> Result<String, StoreError> {
        let mut store = self.store.clone();
        store.schema_version = SCHEMA_VERSION;
        serde_json::to_string_pretty(&store).map_err(StoreError::Json)
    }

    /// Replace store contents from a backup, then persist.
    pub fn restore_backup(&mut self, json: &str) -> Result<(), StoreError> {
        let store: Store = serde_json::from_str(json)?;
        self.store = store;
        self.save()
    }
}

/// Load a JSON file; a missing file yields default; a corrupt file is
/// quarantined (renamed to `<name>.corrupt-<ts>`) and default is returned,
/// mirroring ai-toolbox's fault-tolerant adapter philosophy.
pub fn load_or_default<T: Default + DeserializeOwned>(path: &Path) -> Result<T, StoreError> {
    if !path.exists() {
        return Ok(T::default());
    }
    let raw = fs::read_to_string(path)?;
    match serde_json::from_str::<T>(&raw) {
        Ok(v) => Ok(v),
        Err(e) => {
            let ts = chrono::Utc::now().format("%Y%m%d%H%M%S");
            let quarantine = path.with_extension(format!(
                "{}.corrupt-{ts}",
                path.extension().and_then(|e| e.to_str()).unwrap_or("json")
            ));
            let _ = fs::rename(path, &quarantine);
            tracing::warn!(
                "store {} was corrupt ({e}); quarantined to {} and reset",
                path.display(),
                quarantine.display()
            );
            Ok(T::default())
        }
    }
}

/// Write JSON atomically: serialize to `path.tmp`, fsync, rename over.
pub fn save_json_atomic<T: Serialize>(path: &Path, value: &T) -> Result<(), StoreError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("json.tmp");
    let data = serde_json::to_string_pretty(value)?;
    {
        use std::io::Write;
        let mut f = fs::File::create(&tmp)?;
        f.write_all(data.as_bytes())?;
        f.write_all(b"\n")?;
        f.sync_all()?;
    }
    fs::rename(&tmp, path)?;
    Ok(())
}

/// Serialize any `serde` value to a pretty JSON string, never panicking.
pub fn to_pretty(value: &Value) -> String {
    serde_json::to_string_pretty(value).unwrap_or_else(|_| "{}".into())
}

/// Parse a JSON string to a Value, mapping errors to readable messages.
pub fn parse_json(s: &str) -> Result<Value, String> {
    serde_json::from_str::<Value>(s).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Serialize, Deserialize, Default)]
    struct Doc {
        #[serde(default)]
        v: u32,
        #[serde(default)]
        items: Vec<String>,
    }

    #[test]
    fn round_trip_atomic_write() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("store.json");
        let doc = Doc {
            v: 7,
            items: vec!["a".into()],
        };
        save_json_atomic(&p, &doc).unwrap();
        assert!(p.exists());
        assert!(!p.with_extension("json.tmp").exists());
        let back: Doc = load_or_default(&p).unwrap();
        assert_eq!(back.v, 7);
    }

    #[test]
    fn missing_file_yields_default() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("nope.json");
        let doc: Doc = load_or_default(&p).unwrap();
        assert!(doc.items.is_empty());
    }

    #[test]
    fn corrupt_file_is_quarantined_and_reset() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("store.json");
        fs::write(&p, "{ not json !!").unwrap();
        let doc: Doc = load_or_default(&p).unwrap();
        assert_eq!(doc.v, 0);
        let has_quarantine = fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .any(|e| e.file_name().to_string_lossy().contains("corrupt"));
        assert!(has_quarantine);
    }

    #[test]
    fn parse_and_format_json_helpers() {
        let v = parse_json("{\"a\":1}").unwrap();
        assert_eq!(v["a"], 1);
        assert!(parse_json("bad").is_err());
        assert!(to_pretty(&v).contains("\"a\""));
    }

    #[test]
    fn store_tool_sections() {
        let mut s = Store::new();
        assert_eq!(s.schema_version, SCHEMA_VERSION);
        assert!(s.tool(ToolId::Codex).providers.is_empty());
        s.tool_mut(ToolId::Codex).common_config = "{\"k\":1}".into();
        assert_eq!(s.tool(ToolId::Codex).common_config, "{\"k\":1}");
        let json = serde_json::to_value(&s).unwrap();
        assert!(json["tools"].get("codex").is_some());
    }
}
