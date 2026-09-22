//! Portable local backup bundles and automatic backup rotation.
//!
//! Bundle format: ZIP with a versioned `manifest.json`, the app store and
//! settings, selected CLI config files, the central skills repository, and
//! optional custom files/directories. Restore paths are portable namespaces:
//! `appdata/...` and `home/...`; absolute paths are never trusted from ZIP
//! entry names, preventing traversal attacks.

use std::fs::{self, File};
use std::io::{Read, Seek, Write};
use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::adapters::adapter_for;
use crate::paths::Paths;
use crate::settings::{AppSettings, BackupType};
use crate::tools::ToolId;

pub const BACKUP_SCHEMA: &str = "aitoolplus.backup.v1";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupManifest {
    pub schema: String,
    pub app_version: String,
    pub created_at: String,
    pub entries: Vec<BackupManifestEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupManifestEntry {
    pub archive_path: String,
    pub restore_target: String,
    pub kind: String,
}

#[derive(Debug, Clone, Default)]
pub struct BackupReport {
    pub output: PathBuf,
    pub file_count: usize,
    pub skipped: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ConflictStrategy {
    #[default]
    Overwrite,
    Skip,
    SaveCopy,
}

#[derive(Debug, Clone, Default)]
pub struct RestoreOptions {
    pub allow_custom_absolute: bool,
    pub conflict_strategy: ConflictStrategy,
}

#[derive(Debug, Clone, Default)]
pub struct RestoreReport {
    pub restored: usize,
    pub overwritten: usize,
    pub skipped: Vec<String>,
    pub copies: Vec<PathBuf>,
}

/// All known configuration files that influence runtime behavior. Sessions
/// and package caches are intentionally excluded; skills content is included
/// separately from the central repo.
pub fn cli_config_files(paths: &Paths) -> Vec<PathBuf> {
    let mut files = vec![];
    for tool in ToolId::ALL {
        files.extend(paths.config_files(tool));
        if let Some(prompt) = adapter_for(tool).prompt_file(paths) {
            files.push(prompt);
        }
        if let Some(mcp) = crate::mcp::mcp_config_path(paths, tool) {
            files.push(mcp);
        }
    }
    // Runtime-owned auth/catalog/state files not all exposed by config_files.
    files.extend([
        paths.tool_root(ToolId::GeminiCli).join("oauth_creds.json"),
        paths.tool_root(ToolId::Pi).join("auth.json"),
        paths.tool_root(ToolId::Pi).join("models.json"),
        paths
            .tool_root(ToolId::ClaudeCode)
            .join("plugins")
            .join("installed_plugins.json"),
        paths
            .tool_root(ToolId::ClaudeCode)
            .join("plugins")
            .join("known_marketplaces.json"),
        paths.tool_root(ToolId::Dsh).join(".credentials.yaml"),
        crate::opencode_addons::config_path(
            paths,
            crate::opencode_addons::AddonKind::OhMyOpenAgent,
        ),
        crate::opencode_addons::config_path(
            paths,
            crate::opencode_addons::AddonKind::OhMyOpenCodeSlim,
        ),
    ]);
    files.sort();
    files.dedup();
    files
}

/// Create a local ZIP backup at `output`.
pub fn create_backup(
    paths: &Paths,
    settings: &AppSettings,
    output: &Path,
) -> Result<BackupReport, String> {
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let temp = output.with_extension("zip.tmp");
    let file = File::create(&temp).map_err(|e| e.to_string())?;
    let mut zip = zip::ZipWriter::new(file);
    let mut manifest = BackupManifest {
        schema: BACKUP_SCHEMA.into(),
        app_version: env!("CARGO_PKG_VERSION").into(),
        created_at: chrono::Utc::now().to_rfc3339(),
        entries: vec![],
    };
    let mut report = BackupReport {
        output: output.to_path_buf(),
        ..Default::default()
    };

    add_file_if_exists(
        &mut zip,
        &paths.store_file(),
        "appdata/store.json",
        "appdata/store.json",
        &mut manifest,
        &mut report,
    )?;
    add_file_if_exists(
        &mut zip,
        &paths.settings_file(),
        "appdata/settings.json",
        "appdata/settings.json",
        &mut manifest,
        &mut report,
    )?;
    add_file_if_exists(
        &mut zip,
        &paths.app_data.join("antigravity_accounts.json"),
        "appdata/antigravity_accounts.json",
        "appdata/antigravity_accounts.json",
        &mut manifest,
        &mut report,
    )?;

    if settings.backup_cli_config_files_enabled {
        for file in cli_config_files(paths) {
            if is_filtered(paths, settings, &file) {
                report.skipped.push(format!("filtered: {}", file.display()));
                continue;
            }
            let Some((archive, target)) = portable_target(paths, &file) else {
                report.skipped.push(file.display().to_string());
                continue;
            };
            add_file_if_exists(
                &mut zip,
                &file,
                &archive,
                &target,
                &mut manifest,
                &mut report,
            )?;
        }
    }

    // Central skill source contents (the DB alone is insufficient to restore).
    let skills = paths.central_skills_dir();
    if skills.is_dir() {
        add_directory(
            &mut zip,
            &skills,
            "appdata/skills",
            "appdata/skills",
            &mut manifest,
            &mut report,
        )?;
    }

    for custom in &settings.backup_custom_entries {
        let source = PathBuf::from(&custom.source_path);
        if !source.exists()
            || source == paths.sync_file()
            || source.file_name() == Some(std::ffi::OsStr::new("sync.json"))
        {
            report.skipped.push(custom.source_path.clone());
            continue;
        }
        let archive_root = format!("custom/{}/content", sanitize_segment(&custom.id));
        let restore_target = custom
            .restore_path
            .as_deref()
            .filter(|v| !v.trim().is_empty())
            .map(String::from)
            .unwrap_or_else(|| format!("custom/{}", sanitize_segment(&custom.id)));
        if source.is_dir() {
            add_directory(
                &mut zip,
                &source,
                &archive_root,
                &restore_target,
                &mut manifest,
                &mut report,
            )?;
        } else {
            add_file_if_exists(
                &mut zip,
                &source,
                &archive_root,
                &restore_target,
                &mut manifest,
                &mut report,
            )?;
        }
    }

    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    zip.start_file("manifest.json", options)
        .map_err(|e| e.to_string())?;
    zip.write_all(
        serde_json::to_string_pretty(&manifest)
            .map_err(|e| e.to_string())?
            .as_bytes(),
    )
    .map_err(|e| e.to_string())?;
    zip.finish().map_err(|e| e.to_string())?;
    fs::rename(&temp, output).map_err(|e| e.to_string())?;
    Ok(report)
}

fn add_file_if_exists<W: Write + Seek>(
    zip: &mut zip::ZipWriter<W>,
    source: &Path,
    archive: &str,
    target: &str,
    manifest: &mut BackupManifest,
    report: &mut BackupReport,
) -> Result<(), String> {
    if !source.is_file() {
        return Ok(());
    }
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    zip.start_file(archive.replace('\\', "/"), options)
        .map_err(|e| e.to_string())?;
    let mut file = File::open(source).map_err(|e| e.to_string())?;
    std::io::copy(&mut file, zip).map_err(|e| e.to_string())?;
    manifest.entries.push(BackupManifestEntry {
        archive_path: archive.replace('\\', "/"),
        restore_target: target.replace('\\', "/"),
        kind: "file".into(),
    });
    report.file_count += 1;
    Ok(())
}

fn add_directory<W: Write + Seek>(
    zip: &mut zip::ZipWriter<W>,
    source: &Path,
    archive_root: &str,
    restore_root: &str,
    manifest: &mut BackupManifest,
    report: &mut BackupReport,
) -> Result<(), String> {
    for file in walk_files(source)? {
        let relative = file.strip_prefix(source).map_err(|e| e.to_string())?;
        let relative = slash(relative);
        add_file_if_exists(
            zip,
            &file,
            &format!("{archive_root}/{relative}"),
            &format!("{restore_root}/{relative}"),
            manifest,
            report,
        )?;
    }
    Ok(())
}

fn walk_files(root: &Path) -> Result<Vec<PathBuf>, String> {
    let mut files = vec![];
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in fs::read_dir(&dir).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            let path = entry.path();
            let kind = entry.file_type().map_err(|e| e.to_string())?;
            if kind.is_dir() {
                stack.push(path);
            } else if kind.is_file() {
                files.push(path);
            }
        }
    }
    files.sort();
    Ok(files)
}

/// Inspect a backup archive and parse its manifest without extracting files.
pub fn inspect_backup(archive: &Path) -> Result<BackupManifest, String> {
    let file = File::open(archive).map_err(|e| e.to_string())?;
    let mut zip = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;
    let mut entry = zip.by_name("manifest.json").map_err(|e| e.to_string())?;
    let mut raw = String::new();
    entry.read_to_string(&mut raw).map_err(|e| e.to_string())?;
    let manifest: BackupManifest = serde_json::from_str(&raw).map_err(|e| e.to_string())?;
    if manifest.schema != BACKUP_SCHEMA {
        return Err(format!("unsupported backup schema: {}", manifest.schema));
    }
    Ok(manifest)
}

/// Restore a bundle with custom options (conflict strategy and custom absolute targets).
pub fn restore_backup_with_options(
    paths: &Paths,
    archive: &Path,
    options: &RestoreOptions,
) -> Result<RestoreReport, String> {
    let file = File::open(archive).map_err(|e| e.to_string())?;
    let mut zip = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;
    let manifest: BackupManifest = {
        let mut entry = zip.by_name("manifest.json").map_err(|e| e.to_string())?;
        let mut raw = String::new();
        entry.read_to_string(&mut raw).map_err(|e| e.to_string())?;
        serde_json::from_str(&raw).map_err(|e| e.to_string())?
    };
    if manifest.schema != BACKUP_SCHEMA {
        return Err(format!("unsupported backup schema: {}", manifest.schema));
    }

    let mut report = RestoreReport::default();
    for item in manifest.entries {
        let Some(target) =
            resolve_restore_target(paths, &item.restore_target, options.allow_custom_absolute)
        else {
            report.skipped.push(item.restore_target);
            continue;
        };
        let mut input = match zip.by_name(&item.archive_path) {
            Ok(file) => file,
            Err(error) => {
                report
                    .skipped
                    .push(format!("{}: {error}", item.archive_path));
                continue;
            }
        };

        let final_target = if target.exists() {
            match options.conflict_strategy {
                ConflictStrategy::Skip => {
                    report
                        .skipped
                        .push(format!("skipped existing: {}", target.display()));
                    continue;
                }
                ConflictStrategy::SaveCopy => {
                    let parent = target.parent().unwrap_or(Path::new(""));
                    let stem = target
                        .file_stem()
                        .and_then(|s| s.to_str())
                        .unwrap_or("file");
                    let ext = target.extension().and_then(|s| s.to_str());
                    let copy_name = if let Some(e) = ext {
                        format!("{stem}.restored.{e}")
                    } else {
                        format!("{stem}.restored")
                    };
                    let copy_path = parent.join(copy_name);
                    report.copies.push(copy_path.clone());
                    copy_path
                }
                ConflictStrategy::Overwrite => {
                    report.overwritten += 1;
                    target
                }
            }
        } else {
            target
        };

        if let Some(parent) = final_target.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let temp = final_target.with_extension("restore.tmp");
        let mut output = File::create(&temp).map_err(|e| e.to_string())?;
        std::io::copy(&mut input, &mut output).map_err(|e| e.to_string())?;
        output.sync_all().map_err(|e| e.to_string())?;
        fs::rename(&temp, &final_target).map_err(|e| e.to_string())?;
        report.restored += 1;
    }
    Ok(report)
}

/// Restore a bundle to current home/app-data paths. Custom absolute restore
/// paths must be enabled explicitly by the caller.
pub fn restore_backup(
    paths: &Paths,
    archive: &Path,
    allow_custom_absolute: bool,
) -> Result<RestoreReport, String> {
    restore_backup_with_options(
        paths,
        archive,
        &RestoreOptions {
            allow_custom_absolute,
            conflict_strategy: ConflictStrategy::Overwrite,
        },
    )
}

fn is_filtered(paths: &Paths, settings: &AppSettings, file: &Path) -> bool {
    let portable = portable_target(paths, file)
        .map(|(_, target)| target)
        .unwrap_or_else(|| slash(file));
    settings.backup_file_filter_rules.iter().any(|rule| {
        let rule_path = rule
            .file_path
            .replace('\\', "/")
            .trim_start_matches("~/")
            .to_string();
        portable == rule.file_path.replace('\\', "/")
            || portable.ends_with(&rule_path)
            || slash(file).ends_with(&rule_path)
    })
}

fn portable_target(paths: &Paths, file: &Path) -> Option<(String, String)> {
    if let Ok(relative) = file.strip_prefix(&paths.home) {
        let relative = slash(relative);
        return Some((format!("home/{relative}"), format!("home/{relative}")));
    }
    if let Ok(relative) = file.strip_prefix(&paths.app_data) {
        let relative = slash(relative);
        return Some((format!("appdata/{relative}"), format!("appdata/{relative}")));
    }
    None
}

fn resolve_restore_target(
    paths: &Paths,
    target: &str,
    allow_custom_absolute: bool,
) -> Option<PathBuf> {
    let normalized = Path::new(target);
    if unsafe_relative(normalized) {
        return None;
    }
    // Never restore sync.json (remote sync credentials and transport configuration)
    if target == "appdata/sync.json" || target.ends_with("/sync.json") {
        return None;
    }
    if let Some(rest) = target.strip_prefix("home/") {
        return Some(paths.home.join(rest));
    }
    if let Some(rest) = target.strip_prefix("appdata/") {
        return Some(paths.app_data.join(rest));
    }
    if let Some(rest) = target.strip_prefix("custom/") {
        return Some(paths.app_data.join("restored-custom").join(rest));
    }
    if allow_custom_absolute && normalized.is_absolute() {
        return Some(normalized.to_path_buf());
    }
    None
}

fn unsafe_relative(path: &Path) -> bool {
    path.components().any(|component| {
        matches!(
            component,
            Component::ParentDir | Component::RootDir | Component::Prefix(_)
        )
    }) && !path.is_absolute()
}

fn slash(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn sanitize_segment(value: &str) -> String {
    value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || character == '-' || character == '_' {
                character
            } else {
                '_'
            }
        })
        .collect()
}

/// Create an automatic backup if the configured interval is due; prune old
/// bundles to `auto_backup_max_keep` (0 = unlimited).
pub fn run_auto_backup_if_due(
    paths: &Paths,
    settings: &mut AppSettings,
) -> Result<Option<BackupReport>, String> {
    if !settings.auto_backup_enabled
        || (settings.backup_interval_hours == 0 && settings.auto_backup_interval_days == 0)
    {
        return Ok(None);
    }
    let interval_hours = if settings.backup_interval_hours > 0 {
        settings.backup_interval_hours
    } else {
        settings.auto_backup_interval_days.max(1) * 24
    };

    let directory = settings
        .local_backup_path
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| paths.local_snapshots_dir());
    fs::create_dir_all(&directory).map_err(|e| e.to_string())?;

    // CC-Switch Parity: stable filesystem-based timing check
    // Directly inspect the snapshot directory for the latest backup file's modification time.
    let latest_mtime = fs::read_dir(&directory).ok().and_then(|entries| {
        entries
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.path().extension().map(|ext| ext == "zip").unwrap_or(false)
            })
            .filter_map(|e| e.metadata().ok().and_then(|m| m.modified().ok()))
            .max()
    });

    let interval_secs = u64::from(interval_hours) * 3600;
    let due = match latest_mtime {
        None => true,
        Some(mtime) => {
            mtime.elapsed().unwrap_or_default() >= std::time::Duration::from_secs(interval_secs)
        }
    };
    if !due {
        return Ok(None);
    }

    let now = chrono::Utc::now();
    let output = directory.join(format!(
        "aitoolplus-auto-{}.zip",
        now.format("%Y%m%d-%H%M%S")
    ));
    let report = create_backup(paths, settings, &output)?;
    let keep_count = if settings.backup_retain_count > 0 {
        settings.backup_retain_count
    } else {
        settings.auto_backup_max_keep as usize
    };
    if settings.backup_type == BackupType::Webdav && !settings.webdav.url.trim().is_empty() {
        crate::webdav::upload(&settings.webdav, &output)?;
        prune_remote_backups(&settings.webdav, keep_count as u32)?;
    } else if settings.backup_type == BackupType::S3
        && !settings.s3.endpoint.trim().is_empty()
        && !settings.s3.bucket.trim().is_empty()
    {
        crate::s3::upload(&settings.s3, &output)?;
        crate::s3::prune_remote_backups(&settings.s3, keep_count as u32)?;
    }
    settings.last_auto_backup_time = Some(now.to_rfc3339());
    prune_local_backups(&directory, keep_count)?;
    Ok(Some(report))
}

fn prune_remote_backups(config: &crate::settings::WebDavConfig, keep: u32) -> Result<(), String> {
    if keep == 0 {
        return Ok(());
    }
    let backups = crate::webdav::list(config)?;
    for backup in backups.into_iter().skip(keep as usize) {
        crate::webdav::delete(config, &backup.name)?;
    }
    Ok(())
}

/// Prune older ZIP backups in `directory` keeping the `keep` newest files.
pub fn prune_local_backups(directory: &Path, keep: usize) -> Result<(), String> {
    if keep == 0 {
        return Ok(());
    }
    let Ok(entries) = fs::read_dir(directory) else {
        return Ok(());
    };
    let mut files: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.is_file() && path.extension().and_then(|e| e.to_str()) == Some("zip")
        })
        .collect();
    // Sort newest first by modified time
    files.sort_by(|a, b| {
        let meta_b = b.metadata().and_then(|m| m.modified()).ok();
        let meta_a = a.metadata().and_then(|m| m.modified()).ok();
        meta_b.cmp(&meta_a).then_with(|| b.cmp(a))
    });
    for file in files.into_iter().skip(keep) {
        let _ = fs::remove_file(file);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() -> (tempfile::TempDir, Paths, AppSettings) {
        let directory = tempfile::tempdir().unwrap();
        let paths = Paths::new(
            directory.path().join("home"),
            directory.path().join("appdata"),
        );
        fs::create_dir_all(&paths.app_data).unwrap();
        fs::create_dir_all(paths.tool_root(ToolId::ClaudeCode)).unwrap();
        fs::write(paths.store_file(), r#"{"schema_version":1}"#).unwrap();
        fs::write(paths.settings_file(), r#"{"language":"System"}"#).unwrap();
        fs::write(
            paths.primary_config(ToolId::ClaudeCode),
            r#"{"env":{"ANTHROPIC_BASE_URL":"https://a.example"}}"#,
        )
        .unwrap();
        let skill = paths.central_skills_dir().join("test-skill");
        fs::create_dir_all(&skill).unwrap();
        fs::write(skill.join("SKILL.md"), "# test\n").unwrap();
        (directory, paths, AppSettings::default())
    }

    #[test]
    fn bundle_roundtrip_restores_store_cli_and_skills() {
        let (directory, paths, settings) = setup();
        let backup = directory.path().join("backup.zip");
        let report = create_backup(&paths, &settings, &backup).unwrap();
        assert!(backup.is_file());
        assert!(report.file_count >= 4);

        fs::remove_file(paths.store_file()).unwrap();
        fs::remove_file(paths.primary_config(ToolId::ClaudeCode)).unwrap();
        fs::remove_dir_all(paths.central_skills_dir()).unwrap();
        let restored = restore_backup(&paths, &backup, false).unwrap();
        assert!(restored.restored >= 4);
        assert!(paths.store_file().is_file());
        assert!(paths.primary_config(ToolId::ClaudeCode).is_file());
        assert!(
            paths
                .central_skills_dir()
                .join("test-skill")
                .join("SKILL.md")
                .is_file()
        );
    }

    #[test]
    fn manifest_rejects_parent_traversal() {
        let paths = Paths::new("/home/test", "/data/app");
        assert!(resolve_restore_target(&paths, "home/../../bad", false).is_none());
        assert!(resolve_restore_target(&paths, "appdata/good.json", false).is_some());
        assert!(resolve_restore_target(&paths, "custom/id/file", false).is_some());
    }

    #[test]
    fn automatic_backup_due_and_rotation() {
        let (directory, paths, mut settings) = setup();
        let output = directory.path().join("auto");
        settings.auto_backup_enabled = true;
        settings.local_backup_path = Some(output.to_string_lossy().to_string());
        settings.auto_backup_interval_days = 1;
        settings.auto_backup_max_keep = 2;
        assert!(
            run_auto_backup_if_due(&paths, &mut settings)
                .unwrap()
                .is_some()
        );
        assert!(settings.last_auto_backup_time.is_some());
        // same day: not due
        assert!(
            run_auto_backup_if_due(&paths, &mut settings)
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn file_filter_excludes_selected_cli_file() {
        let (directory, paths, mut settings) = setup();
        settings
            .backup_file_filter_rules
            .push(crate::settings::BackupFileFilterRule {
                tool: "claude_code".into(),
                file_path: "~/.claude/settings.json".into(),
            });
        let backup = directory.path().join("filtered.zip");
        let report = create_backup(&paths, &settings, &backup).unwrap();
        assert!(
            report
                .skipped
                .iter()
                .any(|item| item.contains("settings.json"))
        );
        let file = File::open(backup).unwrap();
        let mut zip = zip::ZipArchive::new(file).unwrap();
        assert!(zip.by_name("home/.claude/settings.json").is_err());
    }

    #[test]
    fn custom_entry_restores_to_safe_appdata_sandbox_by_default() {
        let (directory, paths, mut settings) = setup();
        let custom = directory.path().join("custom.txt");
        fs::write(&custom, "custom-content").unwrap();
        settings
            .backup_custom_entries
            .push(crate::settings::BackupCustomEntry {
                id: "custom-one".into(),
                source_path: custom.to_string_lossy().to_string(),
                restore_path: None,
            });
        let backup = directory.path().join("custom.zip");
        create_backup(&paths, &settings, &backup).unwrap();
        restore_backup(&paths, &backup, false).unwrap();
        assert!(
            paths
                .app_data
                .join("restored-custom")
                .join("custom-one")
                .is_file()
        );
    }

    #[test]
    fn inspect_backup_reads_manifest() {
        let (directory, paths, settings) = setup();
        let backup = directory.path().join("inspect.zip");
        create_backup(&paths, &settings, &backup).unwrap();
        let manifest = inspect_backup(&backup).unwrap();
        assert_eq!(manifest.schema, BACKUP_SCHEMA);
        assert!(!manifest.entries.is_empty());
    }

    #[test]
    fn conflict_strategies_skip_and_save_copy() {
        let (directory, paths, settings) = setup();
        let backup = directory.path().join("conflict.zip");
        create_backup(&paths, &settings, &backup).unwrap();

        // 1. Modify existing store file
        fs::write(paths.store_file(), r#"{"modified": true}"#).unwrap();

        // 2. Restore with Skip
        let report_skip = restore_backup_with_options(
            &paths,
            &backup,
            &RestoreOptions {
                allow_custom_absolute: false,
                conflict_strategy: ConflictStrategy::Skip,
            },
        )
        .unwrap();
        assert!(report_skip.skipped.iter().any(|s| s.contains("store.json")));
        assert_eq!(
            fs::read_to_string(paths.store_file()).unwrap(),
            r#"{"modified": true}"#
        );

        // 3. Restore with SaveCopy
        let report_copy = restore_backup_with_options(
            &paths,
            &backup,
            &RestoreOptions {
                allow_custom_absolute: false,
                conflict_strategy: ConflictStrategy::SaveCopy,
            },
        )
        .unwrap();
        assert!(!report_copy.copies.is_empty());
        assert!(
            report_copy
                .copies
                .iter()
                .any(|p| p.to_string_lossy().contains("store.restored.json"))
        );
        // Original remains untouched
        assert_eq!(
            fs::read_to_string(paths.store_file()).unwrap(),
            r#"{"modified": true}"#
        );

        // 4. Restore with Overwrite
        let report_overwrite = restore_backup_with_options(
            &paths,
            &backup,
            &RestoreOptions {
                allow_custom_absolute: false,
                conflict_strategy: ConflictStrategy::Overwrite,
            },
        )
        .unwrap();
        assert!(report_overwrite.overwritten > 0);
        assert_eq!(
            fs::read_to_string(paths.store_file()).unwrap(),
            r#"{"schema_version":1}"#
        );
    }

    #[test]
    fn antigravity_accounts_backed_up_and_restored() {
        let (directory, paths, settings) = setup();
        let accounts_file = paths.app_data.join("antigravity_accounts.json");
        fs::write(&accounts_file, r#"{"accounts":[{"email":"user@gmail.com","refresh_token":"rt_123"}]}"#).unwrap();

        let backup = directory.path().join("ag_backup.zip");
        let report = create_backup(&paths, &settings, &backup).unwrap();
        assert!(report.file_count >= 3);

        // Delete local accounts file
        fs::remove_file(&accounts_file).unwrap();
        assert!(!accounts_file.exists());

        // Restore
        let restore_report = restore_backup(&paths, &backup, false).unwrap();
        assert!(restore_report.restored >= 3);
        assert!(accounts_file.exists());
        let content = fs::read_to_string(&accounts_file).unwrap();
        assert!(content.contains("user@gmail.com"));
        assert!(content.contains("rt_123"));
    }

    #[test]
    fn backup_excludes_sync_settings_and_never_overwrites_local_sync() {
        let (directory, paths_a, mut settings_a) = setup();

        // Machine A: configured with WebDAV
        settings_a.backup_type = BackupType::Webdav;
        settings_a.webdav.url = "https://dav.machine-a.com".into();
        settings_a.webdav.username = "user_a".into();
        settings_a.webdav.password = "machine_a_super_secret".into();
        settings_a.save(&paths_a.settings_file()).unwrap();

        assert!(paths_a.sync_file().is_file());
        assert!(paths_a.settings_file().is_file());

        // Create backup on Machine A
        let backup = directory.path().join("backup_a.zip");
        let report = create_backup(&paths_a, &settings_a, &backup).unwrap();
        assert!(report.file_count >= 3);

        // Verify ZIP contents
        let manifest = inspect_backup(&backup).unwrap();
        for entry in &manifest.entries {
            assert!(!entry.archive_path.contains("sync.json"), "sync.json must not be in archive");
            assert!(!entry.restore_target.contains("sync.json"), "sync.json must not be in restore target");
        }

        // Check raw zip file for settings.json content
        let file = File::open(&backup).unwrap();
        let mut zip = zip::ZipArchive::new(file).unwrap();
        let mut settings_entry = zip.by_name("appdata/settings.json").unwrap();
        let mut settings_json = String::new();
        settings_entry.read_to_string(&mut settings_json).unwrap();
        assert!(!settings_json.contains("machine_a_super_secret"));
        assert!(!settings_json.contains("machine-a.com"));
        assert!(!settings_json.contains("webdav"));

        // Machine B: has its own WebDAV config
        let machine_b_dir = directory.path().join("machine_b");
        let paths_b = Paths::new(machine_b_dir.join("home"), machine_b_dir.join("appdata"));
        fs::create_dir_all(&paths_b.app_data).unwrap();

        let mut settings_b = AppSettings::default();
        settings_b.backup_type = BackupType::Webdav;
        settings_b.webdav.url = "https://dav.machine-b.com".into();
        settings_b.webdav.username = "user_b".into();
        settings_b.webdav.password = "machine_b_local_password".into();
        settings_b.save(&paths_b.settings_file()).unwrap();

        // Restore Machine A's backup on Machine B
        let restore_report = restore_backup(&paths_b, &backup, false).unwrap();
        assert!(restore_report.restored >= 2);

        // Machine B's sync credentials must NOT be overwritten!
        let reloaded_b = AppSettings::load(&paths_b.settings_file());
        assert_eq!(reloaded_b.backup_type, BackupType::Webdav);
        assert_eq!(reloaded_b.webdav.url, "https://dav.machine-b.com");
        assert_eq!(reloaded_b.webdav.username, "user_b");
        assert_eq!(reloaded_b.webdav.password, "machine_b_local_password");
    }
}
