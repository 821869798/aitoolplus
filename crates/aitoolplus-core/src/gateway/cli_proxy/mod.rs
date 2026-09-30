pub mod claude;
pub mod codex;
pub mod manifest;

use std::path::PathBuf;
use crate::gateway::types::{GatewayCliKey, GatewayCliTakeoverStatus, GatewayProxyMode};
use crate::paths::Paths;
use crate::tools::ToolId;
use manifest::{backup_file, restore_file, CliProxyManifest, CliProxyManifestFile};

pub fn cli_gateway_endpoint(cli: GatewayCliKey, port: u16) -> String {
    let base = format!("http://127.0.0.1:{port}");
    match cli {
        GatewayCliKey::Claude => format!("{base}/anthropic"),
        GatewayCliKey::Codex => format!("{base}/openai/v1"),
        GatewayCliKey::Grok => format!("{base}/grok/v1"),
        GatewayCliKey::Kimi => format!("{base}/kimi/v1"),
    }
}

pub fn get_cli_target_files(paths: &Paths, cli: GatewayCliKey) -> Vec<PathBuf> {
    match cli {
        GatewayCliKey::Claude => vec![paths.tool_root(ToolId::ClaudeCode).join("settings.json")],
        GatewayCliKey::Codex => vec![
            paths.tool_root(ToolId::Codex).join("config.toml"),
            paths.tool_root(ToolId::Codex).join("auth.json"),
        ],
        GatewayCliKey::Grok => vec![paths.tool_root(ToolId::Grok).join("config.toml")],
        GatewayCliKey::Kimi => vec![paths.tool_root(ToolId::Kimi).join("config.toml")],
    }
}

/// Engage Gateway takeover for a CLI tool.
pub fn engage_cli_proxy(
    paths: &Paths,
    cli_key: GatewayCliKey,
    port: u16,
    mode: GatewayProxyMode,
    primary_provider_id: Option<String>,
    primary_provider_name: Option<String>,
) -> Result<GatewayCliTakeoverStatus, String> {
    let backup_dir = CliProxyManifest::backup_dir(paths, cli_key);
    let target_files = get_cli_target_files(paths, cli_key);
    let mut manifest_files = Vec::new();

    // 1. Snapshot backup files before changing anything
    for target in &target_files {
        let is_taken_over = match cli_key {
            GatewayCliKey::Claude => claude::check_claude_is_gateway(target),
            GatewayCliKey::Codex => codex::check_codex_is_gateway(target),
            _ => false,
        };
        let file_name = target.file_name().and_then(|n| n.to_str()).unwrap_or("file");
        let backup_dest = backup_dir.join(file_name);
        // Do not backup if target is already taken over (avoids poisoning backup slot with proxy stubs, parity with cc-switch)
        if !is_taken_over {
            let original_size = backup_file(target, &backup_dest)?;
            manifest_files.push(CliProxyManifestFile {
                target_path: target.to_string_lossy().to_string(),
                backup_rel_path: file_name.to_string(),
                original_size,
            });
        }
    }

    let endpoint = cli_gateway_endpoint(cli_key, port);
    let mut aggregate_provider_ids = Vec::new();
    let mut primary_provider_id = primary_provider_id;
    let mut primary_provider_name = primary_provider_name;

    // 2. Patch CLI config
    match cli_key {
        GatewayCliKey::Claude => {
            if let Some(target) = target_files.first() {
                let (resolved_id, primary_provider) = {
                    let store_handle = crate::store::StoreHandle::open(paths).ok();
                    let tool_store = store_handle.as_ref().map(|h| h.store().tool(ToolId::ClaudeCode)).unwrap_or_default();
                    if let Some(pid) = &primary_provider_id {
                        let p = tool_store.providers.into_iter().find(|p| &p.id == pid);
                        (primary_provider_id.clone(), p)
                    } else {
                        let p = tool_store.providers.into_iter().find(|p| p.is_applied && !p.is_disabled);
                        let id = p.as_ref().map(|p| p.id.clone());
                        (id, p)
                    }
                };
                if primary_provider_id.is_none() {
                    primary_provider_id = resolved_id;
                }
                if primary_provider_name.is_none() {
                    primary_provider_name = primary_provider.as_ref().map(|p| p.name.clone());
                }
                claude::patch_claude_settings(target, &endpoint, primary_provider.as_ref())?;
            }
        }
        GatewayCliKey::Codex => {
            let config_path = &target_files[0];
            let auth_path = &target_files[1];
            let codex_root = paths.tool_root(ToolId::Codex);
            let is_aggregate = mode == GatewayProxyMode::Aggregate;

            let (resolved_id, primary_provider) = {
                let store_handle = crate::store::StoreHandle::open(paths).ok();
                let tool_store = store_handle.as_ref().map(|h| h.store().tool(ToolId::Codex)).unwrap_or_default();
                if let Some(pid) = &primary_provider_id {
                    let p = tool_store.providers.iter().find(|p| &p.id == pid).cloned();
                    (primary_provider_id.clone(), p)
                } else {
                    let p = tool_store.providers.iter().find(|p| p.is_applied && !p.is_disabled).cloned();
                    let id = p.as_ref().map(|p| p.id.clone());
                    (id, p)
                }
            };
            if primary_provider_id.is_none() {
                primary_provider_id = resolved_id;
            }
            if primary_provider_name.is_none() {
                primary_provider_name = primary_provider.as_ref().map(|p| p.name.clone());
            }

            if is_aggregate {
                let store_handle = crate::store::StoreHandle::open(paths).ok();
                let tool_store = store_handle.as_ref().map(|h| h.store().tool(ToolId::Codex)).unwrap_or_default();
                let mut providers: Vec<_> = tool_store.providers.into_iter().filter(|p| !p.is_disabled).collect();
                if let Some(pid) = &primary_provider_id {
                    providers.sort_by_key(|p| if &p.id == pid { 0 } else { p.sort_index + 1 });
                }
                aggregate_provider_ids = providers.iter().map(|p| p.id.clone()).collect();
                codex::generate_aggregate_codex_catalog(&codex_root, &providers)?;
            } else {
                codex::cleanup_codex_aggregate_catalog(&codex_root);
            }

            codex::patch_codex_config(config_path, auth_path, &endpoint, is_aggregate)?;
        }
        _ => return Err(format!("Takeover not implemented for {:?}", cli_key)),
    }

    // 3. Write manifest
    let manifest = CliProxyManifest {
        cli_key,
        enabled: true,
        mode,
        primary_provider_id: primary_provider_id.clone().unwrap_or_default(),
        aggregate_provider_ids,
        port,
        files: manifest_files,
        engaged_at: chrono::Utc::now().to_rfc3339(),
    };
    manifest.write(paths)?;

    Ok(GatewayCliTakeoverStatus {
        cli_key,
        enabled: true,
        mode,
        primary_provider_id,
        primary_provider_name,
        provider_priorities: vec![],
        error: None,
    })
}

/// Restore a CLI tool back to direct mode (parity with ai-toolbox and cc-switch).
pub fn restore_cli_direct(
    paths: &Paths,
    cli_key: GatewayCliKey,
) -> Result<GatewayCliTakeoverStatus, String> {
    let backup_dir = CliProxyManifest::backup_dir(paths, cli_key);
    let maybe_manifest = CliProxyManifest::read(paths, cli_key);
    let mut restored_successfully = false;

    match cli_key {
        GatewayCliKey::Claude => {
            let claude_settings_path = paths.tool_root(ToolId::ClaudeCode).join("settings.json");
            let backup_path = backup_dir.join("settings.json");
            let backup_content = if backup_path.exists() {
                let content = std::fs::read_to_string(&backup_path).unwrap_or_default();
                if !content.contains("127.0.0.1") && !content.contains("aitoolplus-gateway") {
                    Some(content)
                } else {
                    None
                }
            } else {
                None
            };

            if let Some(clean_content) = backup_content {
                // Parity with ai-toolbox restore_claude_settings: JSON pointer restoration
                if claude::restore_claude_settings(&claude_settings_path, Some(&clean_content)).is_ok() {
                    restored_successfully = true;
                }
            }

            if !restored_successfully && restore_cli_from_ssot(paths, GatewayCliKey::Claude).is_err() {
                // Cleanup any lingering proxy placeholders
                let _ = claude::cleanup_claude_takeover_placeholders(&claude_settings_path);
            }
        }
        GatewayCliKey::Codex => {
            if let Some(manifest) = &maybe_manifest {
                for f in &manifest.files {
                    let target_path = PathBuf::from(&f.target_path);
                    let backup_path = backup_dir.join(&f.backup_rel_path);
                    let backup_clean = if backup_path.exists() {
                        let content = std::fs::read_to_string(&backup_path).unwrap_or_default();
                        !content.contains("127.0.0.1") && !content.contains("aitoolplus-gateway")
                    } else {
                        false
                    };
                    if backup_clean {
                        let _ = restore_file(&backup_path, &target_path);
                        restored_successfully = true;
                    }
                }
            }
            if !restored_successfully {
                for target in get_cli_target_files(paths, cli_key) {
                    let file_name = target.file_name().and_then(|n| n.to_str()).unwrap_or("file");
                    let backup_path = backup_dir.join(file_name);
                    let backup_clean = if backup_path.exists() {
                        let content = std::fs::read_to_string(&backup_path).unwrap_or_default();
                        !content.contains("127.0.0.1") && !content.contains("aitoolplus-gateway")
                    } else {
                        false
                    };
                    if backup_clean {
                        let _ = restore_file(&backup_path, &target);
                        restored_successfully = true;
                    }
                }
            }
            if !restored_successfully {
                let _ = restore_cli_from_ssot(paths, GatewayCliKey::Codex);
            }
            codex::cleanup_codex_aggregate_catalog(&paths.tool_root(ToolId::Codex));
        }
        _ => {}
    }

    // Parity with ai-toolbox & cc-switch:
    // Drop original snapshots after a successful restore so the next engage re-backs up
    // the post-direct runtime files instead of reusing a stale first-engage .bak.
    clear_gateway_backups(paths, cli_key);
    let _ = CliProxyManifest::remove(paths, cli_key);

    Ok(GatewayCliTakeoverStatus {
        cli_key,
        enabled: false,
        mode: GatewayProxyMode::Single,
        primary_provider_id: None,
        primary_provider_name: None,
        provider_priorities: vec![],
        error: None,
    })
}

/// Clear all backup snapshots for a CLI tool (parity with ai-toolbox and cc-switch).
pub fn clear_gateway_backups(paths: &Paths, cli_key: GatewayCliKey) {
    let backup_dir = CliProxyManifest::backup_dir(paths, cli_key);
    if backup_dir.exists() {
        if let Ok(entries) = std::fs::read_dir(&backup_dir) {
            for entry in entries.flatten() {
                let _ = std::fs::remove_file(entry.path());
            }
        }
        let _ = std::fs::remove_dir(&backup_dir);
    }
}

/// Cleanup any lingering takeover placeholders or local gateway URLs from live CLI configs (parity with cc-switch).
pub fn cleanup_takeover_placeholders_in_live(paths: &Paths, cli_key: GatewayCliKey) {
    match cli_key {
        GatewayCliKey::Claude => {
            let settings_path = paths.tool_root(ToolId::ClaudeCode).join("settings.json");
            let _ = claude::cleanup_claude_takeover_placeholders(&settings_path);
        }
        GatewayCliKey::Codex => {
            let config_path = paths.tool_root(ToolId::Codex).join("config.toml");
            let auth_path = paths.tool_root(ToolId::Codex).join("auth.json");
            codex::unpatch_codex_config(&config_path, &auth_path);
        }
        _ => {}
    }
}

/// Update the live backup snapshot from a provider's settings (parity with cc-switch update_live_backup_from_provider).
///
/// When the gateway has taken over live CLI files, modifying or switching an applied provider
/// must update the backup files on disk so that subsequent restores (or app exit) will restore
/// the latest provider settings rather than resurrecting stale configurations.
pub fn update_live_backup_from_provider(
    paths: &Paths,
    cli_key: GatewayCliKey,
    provider: &crate::providers::ProviderRecord,
    common_config: &str,
) -> Result<(), String> {
    let backup_dir = CliProxyManifest::backup_dir(paths, cli_key);
    std::fs::create_dir_all(&backup_dir).map_err(|e| format!("Failed to create backup dir: {e}"))?;

    match cli_key {
        GatewayCliKey::Claude => {
            let backup_file = backup_dir.join("settings.json");
            let current_backup_val = if backup_file.exists() {
                std::fs::read_to_string(&backup_file)
                    .ok()
                    .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
            } else {
                let live_path = paths.tool_root(ToolId::ClaudeCode).join("settings.json");
                if live_path.exists() {
                    std::fs::read_to_string(&live_path)
                        .ok()
                        .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
                } else {
                    None
                }
            };

            let next_common: serde_json::Value = serde_json::from_str(common_config)
                .unwrap_or(serde_json::Value::Object(serde_json::Map::new()));
            let provider_config: serde_json::Value = serde_json::from_str(&provider.settings_config)
                .unwrap_or(serde_json::Value::Object(serde_json::Map::new()));

            let effective = crate::adapters::claude_code::merge_settings_for_provider(
                current_backup_val.as_ref(),
                None,
                &next_common,
                None,
                None,
                &provider_config,
                &crate::adapters::claude_code::KNOWN_ENV_FIELDS,
            )?;
            let content = serde_json::to_string_pretty(&effective)
                .map_err(|e| format!("Failed to serialize backup settings: {e}"))?;
            std::fs::write(&backup_file, content)
                .map_err(|e| format!("Failed to write live backup: {e}"))?;
            tracing::info!("Updated Claude live backup from provider {}", provider.name);
        }
        GatewayCliKey::Codex => {
            let backup_config = backup_dir.join("config.toml");
            let backup_auth = backup_dir.join("auth.json");
            if let Ok(settings) = serde_json::from_str::<serde_json::Value>(&provider.settings_config) {
                if let Some(cfg_str) = settings.get("config").and_then(|v| v.as_str()) {
                    let _ = std::fs::write(&backup_config, cfg_str);
                }
                if let Some(auth_val) = settings.get("auth") {
                    let _ = std::fs::write(&backup_auth, serde_json::to_string_pretty(auth_val).unwrap_or_default());
                }
            }
            tracing::info!("Updated Codex live backup from provider {}", provider.name);
        }
        _ => {}
    }

    // Keep manifest primary_provider_id in sync
    if let Some(mut manifest) = CliProxyManifest::read(paths, cli_key) {
        manifest.primary_provider_id = provider.id.clone();
        for target in get_cli_target_files(paths, cli_key) {
            let file_name = target.file_name().and_then(|n| n.to_str()).unwrap_or("file");
            if !manifest.files.iter().any(|f| f.backup_rel_path == file_name) {
                let backup_file = backup_dir.join(file_name);
                let size = std::fs::metadata(&backup_file).map(|m| m.len()).unwrap_or(0);
                manifest.files.push(CliProxyManifestFile {
                    target_path: target.to_string_lossy().to_string(),
                    backup_rel_path: file_name.to_string(),
                    original_size: size,
                });
            }
        }
        let _ = manifest.write(paths);
    }

    Ok(())
}

/// Reconstruct live config from the Single Source of Truth (store.json's current provider)
pub fn restore_cli_from_ssot(paths: &Paths, cli_key: GatewayCliKey) -> Result<(), String> {
    let tool_id = match cli_key {
        GatewayCliKey::Claude => ToolId::ClaudeCode,
        GatewayCliKey::Codex => ToolId::Codex,
        _ => return Ok(()),
    };
    let store_handle = crate::store::StoreHandle::open(paths).ok();
    let tool_store = store_handle.as_ref().map(|h| h.store().tool(tool_id)).unwrap_or_default();

    let provider = tool_store.providers.iter()
        .find(|p| p.is_applied && !p.is_disabled)
        .or_else(|| tool_store.providers.iter().find(|p| !p.is_disabled));

    match cli_key {
        GatewayCliKey::Claude => {
            let claude_settings_path = paths.tool_root(ToolId::ClaudeCode).join("settings.json");
            if let Some(p) = provider {
                let (resolved_url, _) = p.resolve_credentials(ToolId::ClaudeCode);
                if !resolved_url.contains("127.0.0.1") && !resolved_url.contains("localhost") {
                    let adapter = crate::adapters::adapter_for(ToolId::ClaudeCode);
                    let ctx = crate::adapters::ApplyCtx {
                        paths,
                        common_config: &tool_store.common_config,
                        provider: p,
                        strategy: crate::config::MergeStrategy::default(),
                        provider_optional: false,
                    };
                    if adapter.apply(&ctx).is_ok() {
                        return Ok(());
                    }
                }
            }
            // Cleanup proxy fields from settings.json if present
            if claude_settings_path.exists() {
                if let Ok(content) = std::fs::read_to_string(&claude_settings_path) {
                    if let Ok(mut val) = serde_json::from_str::<serde_json::Value>(&content) {
                        if let Some(env) = val.get_mut("env").and_then(serde_json::Value::as_object_mut) {
                            env.remove("ANTHROPIC_BASE_URL");
                            env.remove("ANTHROPIC_AUTH_TOKEN");
                        }
                        let _ = std::fs::write(&claude_settings_path, serde_json::to_string_pretty(&val).unwrap_or_default());
                    }
                }
            }
        }
        GatewayCliKey::Codex => {
            let config_path = paths.tool_root(ToolId::Codex).join("config.toml");
            let auth_path = paths.tool_root(ToolId::Codex).join("auth.json");
            if let Some(p) = provider {
                let adapter = crate::adapters::adapter_for(ToolId::Codex);
                let ctx = crate::adapters::ApplyCtx {
                    paths,
                    common_config: &tool_store.common_config,
                    provider: p,
                    strategy: crate::config::MergeStrategy::default(),
                    provider_optional: false,
                };
                let _ = adapter.apply(&ctx);
            } else {
                codex::unpatch_codex_config(&config_path, &auth_path);
            }
        }
        _ => {}
    }
    Ok(())
}

/// Get live takeover status for a CLI tool.
pub fn get_cli_takeover_status(paths: &Paths, cli_key: GatewayCliKey) -> GatewayCliTakeoverStatus {
    let manifest = CliProxyManifest::read(paths, cli_key);
    let target_files = get_cli_target_files(paths, cli_key);

    let is_on_disk_taken_over = match cli_key {
        GatewayCliKey::Claude => {
            target_files.first().map(|p| claude::check_claude_is_gateway(p)).unwrap_or(false)
        }
        GatewayCliKey::Codex => {
            target_files.first().map(|p| codex::check_codex_is_gateway(p)).unwrap_or(false)
        }
        _ => false,
    };

    if let Some(m) = manifest {
        GatewayCliTakeoverStatus {
            cli_key,
            enabled: m.enabled && is_on_disk_taken_over,
            mode: m.mode,
            primary_provider_id: if m.primary_provider_id.is_empty() { None } else { Some(m.primary_provider_id) },
            primary_provider_name: None,
            provider_priorities: vec![],
            error: None,
        }
    } else {
        GatewayCliTakeoverStatus {
            cli_key,
            enabled: is_on_disk_taken_over,
            mode: GatewayProxyMode::Single,
            primary_provider_id: None,
            primary_provider_name: None,
            provider_priorities: vec![],
            error: None,
        }
    }
}

/// List takeover status for supported CLIs.
pub fn list_all_takeover_statuses(paths: &Paths) -> Vec<GatewayCliTakeoverStatus> {
    vec![
        get_cli_takeover_status(paths, GatewayCliKey::Claude),
        get_cli_takeover_status(paths, GatewayCliKey::Codex),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_claude_takeover_and_restore() {
        let temp = tempdir().unwrap();
        let home = temp.path().to_path_buf();
        let app_data = home.join(".aitoolplus");
        let paths = Paths::new(&home, &app_data);

        let status = engage_cli_proxy(
            &paths,
            GatewayCliKey::Claude,
            15721,
            GatewayProxyMode::Single,
            Some("p1".into()),
            Some("Primary".into()),
        ).unwrap();
        assert!(status.enabled);

        let check = get_cli_takeover_status(&paths, GatewayCliKey::Claude);
        assert!(check.enabled);

        let restored = restore_cli_direct(&paths, GatewayCliKey::Claude).unwrap();
        assert!(!restored.enabled);

        let check_after = get_cli_takeover_status(&paths, GatewayCliKey::Claude);
        assert!(!check_after.enabled);
    }

    #[test]
    fn test_update_live_backup_from_provider_and_restore_preserves_modifications() {
        let temp = tempdir().unwrap();
        let home = temp.path().to_path_buf();
        let app_data = home.join(".aitoolplus");
        let paths = Paths::new(&home, &app_data);

        // 1. Initial settings on disk (e.g. initial setup)
        let claude_dir = paths.tool_root(ToolId::ClaudeCode);
        std::fs::create_dir_all(&claude_dir).unwrap();
        let settings_path = claude_dir.join("settings.json");
        std::fs::write(&settings_path, r#"{"env":{"ANTHROPIC_BASE_URL":"https://initial.example.com","ANTHROPIC_AUTH_TOKEN":"initial-token"}}"#).unwrap();

        // 2. Engage proxy takeover
        let status = engage_cli_proxy(
            &paths,
            GatewayCliKey::Claude,
            15721,
            GatewayProxyMode::Single,
            Some("p1".into()),
            Some("Initial Provider".into()),
        ).unwrap();
        assert!(status.enabled);

        // Verify live settings is now pointing to gateway
        let live_content = std::fs::read_to_string(&settings_path).unwrap();
        assert!(live_content.contains("http://127.0.0.1:15721/anthropic"));
        assert!(live_content.contains("aitoolplus-gateway"));

        // 3. User modifies the provider in aitoolplus while takeover is active
        let mut modified_provider = crate::providers::ProviderRecord::new("anyrouter.top1", "custom");
        modified_provider.id = "p1".into();
        modified_provider.settings_config = r#"{"env":{"ANTHROPIC_BASE_URL":"https://anyrouter.top1","ANTHROPIC_AUTH_TOKEN":"sk-modified-key","ANTHROPIC_MODEL":"opus"}}"#.into();

        // Update live backup from modified provider
        update_live_backup_from_provider(&paths, GatewayCliKey::Claude, &modified_provider, "").unwrap();

        // Verify the backup snapshot now contains the MODIFIED settings, not the initial settings
        let backup_path = CliProxyManifest::backup_dir(&paths, GatewayCliKey::Claude).join("settings.json");
        assert!(backup_path.exists());
        let backup_content = std::fs::read_to_string(&backup_path).unwrap();
        assert!(backup_content.contains("https://anyrouter.top1"));
        assert!(backup_content.contains("sk-modified-key"));
        assert!(!backup_content.contains("https://initial.example.com"));

        // Verify manifest updated
        let manifest = CliProxyManifest::read(&paths, GatewayCliKey::Claude).unwrap();
        assert_eq!(manifest.primary_provider_id, "p1");

        // 4. Restore direct mode (e.g. tray exit or toggle off)
        let restored = restore_cli_direct(&paths, GatewayCliKey::Claude).unwrap();
        assert!(!restored.enabled);

        // 5. Verify the live file restored with the USER'S MODIFICATIONS, NOT the initial settings!
        let restored_live_content = std::fs::read_to_string(&settings_path).unwrap();
        assert!(restored_live_content.contains("https://anyrouter.top1"), "Must restore modified base URL");
        assert!(restored_live_content.contains("sk-modified-key"), "Must restore modified auth token");
        assert!(!restored_live_content.contains("https://initial.example.com"), "Must NOT resurrect initial base URL");
        assert!(!restored_live_content.contains("127.0.0.1"), "Must NOT contain gateway address");
        assert!(!restored_live_content.contains("aitoolplus-gateway"), "Must NOT contain gateway placeholder token");

        // Backup and manifest should be cleaned up
        assert!(!backup_path.exists());
        assert!(CliProxyManifest::read(&paths, GatewayCliKey::Claude).is_none());
    }
}
