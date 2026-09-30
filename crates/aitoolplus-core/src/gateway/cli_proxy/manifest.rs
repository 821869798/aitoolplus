use crate::gateway::types::{GatewayCliKey, GatewayProxyMode};
use crate::paths::Paths;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CliProxyManifestFile {
    pub target_path: String,
    pub backup_rel_path: String,
    pub original_size: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CliProxyManifest {
    pub cli_key: GatewayCliKey,
    pub enabled: bool,
    pub mode: GatewayProxyMode,
    pub primary_provider_id: String,
    #[serde(default)]
    pub aggregate_provider_ids: Vec<String>,
    pub port: u16,
    pub files: Vec<CliProxyManifestFile>,
    pub engaged_at: String,
}

impl CliProxyManifest {
    pub fn manifests_dir(paths: &Paths) -> PathBuf {
        paths.gateway_dir().join("manifests")
    }

    pub fn manifest_path(paths: &Paths, cli: GatewayCliKey) -> PathBuf {
        Self::manifests_dir(paths).join(format!("{}.json", cli.as_str()))
    }

    pub fn backup_dir(paths: &Paths, cli: GatewayCliKey) -> PathBuf {
        paths.gateway_dir().join("backups").join(cli.as_str())
    }

    pub fn read(paths: &Paths, cli: GatewayCliKey) -> Option<Self> {
        let p = Self::manifest_path(paths, cli);
        if !p.exists() {
            return None;
        }
        let content = fs::read_to_string(&p).ok()?;
        serde_json::from_str(&content).ok()
    }

    pub fn write(&self, paths: &Paths) -> Result<(), String> {
        let dir = Self::manifests_dir(paths);
        fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let p = Self::manifest_path(paths, self.cli_key);
        let content = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        fs::write(p, content).map_err(|e| e.to_string())
    }

    pub fn remove(paths: &Paths, cli: GatewayCliKey) -> Result<(), String> {
        let p = Self::manifest_path(paths, cli);
        if p.exists() {
            fs::remove_file(p).map_err(|e| e.to_string())?;
        }
        Ok(())
    }
}

/// Create a backup file copy if it does not already exist.
pub fn backup_file(src: &Path, backup_dest: &Path) -> Result<u64, String> {
    if !src.exists() {
        return Ok(0);
    }
    if let Some(parent) = backup_dest.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    // Only copy if backup doesn't already exist to preserve true original
    if !backup_dest.exists() {
        fs::copy(src, backup_dest).map_err(|e| format!("Failed to copy backup: {e}"))?;
    }
    let meta = fs::metadata(src).map_err(|e| e.to_string())?;
    Ok(meta.len())
}

/// Restore a target file from its backup, and delete the backup.
pub fn restore_file(backup_src: &Path, target_dest: &Path) -> Result<(), String> {
    if backup_src.exists() {
        if let Some(parent) = target_dest.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        fs::copy(backup_src, target_dest).map_err(|e| format!("Failed to restore file: {e}"))?;
        let _ = fs::remove_file(backup_src);
    } else if target_dest.exists() {
        let _ = fs::remove_file(target_dest);
    }
    Ok(())
}
