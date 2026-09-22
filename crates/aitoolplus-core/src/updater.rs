//! GitHub Releases update checker and asset downloader.
//!
//! Updates are never installed silently: the UI checks, shows version/release
//! notes, and downloads the selected asset to a user-visible path. Replacing a
//! running Windows executable is deliberately left to the downloaded installer
//! or a future signed helper.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

pub const DEFAULT_RELEASES_API: &str =
    "https://api.github.com/repos/821869798/aitoolplus/releases/latest";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum UpdateMirror {
    #[default]
    Official,
    GhProxyNet,
    MirrorGhProxy,
    GhProxyCom,
    Custom,
}

impl UpdateMirror {
    pub const ALL: &'static [UpdateMirror] = &[
        UpdateMirror::Official,
        UpdateMirror::GhProxyNet,
        UpdateMirror::MirrorGhProxy,
        UpdateMirror::GhProxyCom,
        UpdateMirror::Custom,
    ];

    pub fn id(&self) -> &'static str {
        match self {
            Self::Official => "official",
            Self::GhProxyNet => "ghproxy_net",
            Self::MirrorGhProxy => "mirror_ghproxy",
            Self::GhProxyCom => "ghproxy_com",
            Self::Custom => "custom",
        }
    }

    pub fn from_id(id: &str) -> Self {
        match id {
            "ghproxy_net" => Self::GhProxyNet,
            "mirror_ghproxy" => Self::MirrorGhProxy,
            "ghproxy_com" => Self::GhProxyCom,
            "custom" => Self::Custom,
            _ => Self::Official,
        }
    }

    pub fn display_name(&self, is_zh: bool) -> &'static str {
        match self {
            Self::Official => {
                if is_zh {
                    "GitHub 官方直连"
                } else {
                    "GitHub Official (Direct)"
                }
            }
            Self::GhProxyNet => {
                if is_zh {
                    "国内高速镜像 (ghproxy.net)"
                } else {
                    "China Mirror (ghproxy.net)"
                }
            }
            Self::MirrorGhProxy => {
                if is_zh {
                    "国内备用镜像 (mirror.ghproxy.com)"
                } else {
                    "China Mirror (mirror.ghproxy.com)"
                }
            }
            Self::GhProxyCom => {
                if is_zh {
                    "国内备用镜像 (gh-proxy.com)"
                } else {
                    "China Mirror (gh-proxy.com)"
                }
            }
            Self::Custom => {
                if is_zh {
                    "自定义 CDN / 代理前缀"
                } else {
                    "Custom CDN / Mirror Prefix"
                }
            }
        }
    }

    pub fn apply_url(&self, original_url: &str, custom_prefix: &str) -> String {
        let trimmed = original_url.trim();
        if trimmed.is_empty() {
            return String::new();
        }
        match self {
            Self::Official => trimmed.to_string(),
            Self::GhProxyNet => {
                if trimmed.starts_with("https://ghproxy.net/") {
                    trimmed.to_string()
                } else {
                    format!("https://ghproxy.net/{}", trimmed)
                }
            }
            Self::MirrorGhProxy => {
                if trimmed.starts_with("https://mirror.ghproxy.com/") {
                    trimmed.to_string()
                } else {
                    format!("https://mirror.ghproxy.com/{}", trimmed)
                }
            }
            Self::GhProxyCom => {
                if trimmed.starts_with("https://gh-proxy.com/") {
                    trimmed.to_string()
                } else {
                    format!("https://gh-proxy.com/{}", trimmed)
                }
            }
            Self::Custom => {
                let prefix = custom_prefix.trim();
                if prefix.is_empty() {
                    trimmed.to_string()
                } else if prefix.contains("{}") {
                    prefix.replace("{}", trimmed)
                } else {
                    let clean_prefix = prefix.trim_end_matches('/');
                    format!("{clean_prefix}/{trimmed}")
                }
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadProgress {
    pub downloaded: u64,
    pub total: u64,
    pub percentage: f32,
    pub speed_bps: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateAsset {
    pub name: String,
    pub download_url: String,
    pub size: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    pub current_version: String,
    pub latest_version: String,
    pub update_available: bool,
    pub release_url: String,
    pub release_notes: String,
    pub published_at: Option<String>,
    pub assets: Vec<UpdateAsset>,
}

#[derive(Debug, Deserialize)]
struct GithubRelease {
    tag_name: String,
    #[serde(default)]
    html_url: String,
    #[serde(default)]
    body: String,
    #[serde(default)]
    published_at: Option<String>,
    #[serde(default)]
    prerelease: bool,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    assets: Vec<GithubAsset>,
}

#[derive(Debug, Deserialize)]
struct GithubAsset {
    name: String,
    browser_download_url: String,
    #[serde(default)]
    size: u64,
}

#[derive(Debug, Deserialize)]
struct TauriPlatformInfo {
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    #[allow(dead_code)]
    pub signature: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TauriLatestRelease {
    version: String,
    #[serde(default)]
    notes: Option<String>,
    #[serde(default)]
    pub_date: Option<String>,
    #[serde(default)]
    platforms: std::collections::HashMap<String, TauriPlatformInfo>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum ReleasePayload {
    Github(GithubRelease),
    Tauri(TauriLatestRelease),
}

pub fn check_latest(current_version: &str) -> Result<UpdateInfo, String> {
    let api = std::env::var("AITOOLPLUS_UPDATE_API")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_RELEASES_API.into());
    check_latest_at(&api, current_version)
}

pub fn check_latest_at(api: &str, current_version: &str) -> Result<UpdateInfo, String> {
    let response = ureq::AgentBuilder::new()
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .get(api)
        .set("Accept", "application/vnd.github+json, application/json")
        .set("User-Agent", "AIToolPlus-Updater")
        .call()
        .map_err(|error| format!("update check failed: {error}"))?;

    let payload: ReleasePayload = response
        .into_json()
        .map_err(|error| format!("update response parse failed: {error}"))?;

    let current = normalize_version(current_version);

    match payload {
        ReleasePayload::Github(release) => {
            if release.draft || release.prerelease {
                return Err("latest release is draft/prerelease".into());
            }
            let latest = normalize_version(&release.tag_name);
            let assets = release
                .assets
                .into_iter()
                .filter(|asset| {
                    asset.browser_download_url.starts_with("https://")
                        || asset.browser_download_url.starts_with("http://")
                })
                .map(|asset| UpdateAsset {
                    name: asset.name,
                    download_url: asset.browser_download_url,
                    size: asset.size,
                })
                .collect();
            Ok(UpdateInfo {
                current_version: current.clone(),
                latest_version: latest.clone(),
                update_available: compare_versions(&latest, &current).is_gt(),
                release_url: release.html_url,
                release_notes: release.body.chars().take(20_000).collect(),
                published_at: release.published_at,
                assets,
            })
        }
        ReleasePayload::Tauri(tauri_rel) => {
            let latest = normalize_version(&tauri_rel.version);
            let mut assets = Vec::new();
            for (platform_key, info) in tauri_rel.platforms {
                if let Some(download_url) = info.url {
                    let asset_name = if platform_key.contains("windows") {
                        "aitoolplus-setup.exe".to_string()
                    } else if platform_key.contains("darwin") || platform_key.contains("macos") {
                        "aitoolplus.dmg".to_string()
                    } else {
                        "aitoolplus.AppImage".to_string()
                    };
                    assets.push(UpdateAsset {
                        name: asset_name,
                        download_url,
                        size: 0,
                    });
                }
            }
            Ok(UpdateInfo {
                current_version: current.clone(),
                latest_version: latest.clone(),
                update_available: compare_versions(&latest, &current).is_gt(),
                release_url: format!("https://github.com/821869798/aitoolplus/releases/tag/v{latest}"),
                release_notes: tauri_rel.notes.unwrap_or_default().chars().take(20_000).collect(),
                published_at: tauri_rel.pub_date,
                assets,
            })
        }
    }
}

pub fn best_asset(info: &UpdateInfo) -> Option<&UpdateAsset> {
    let platform = if cfg!(windows) {
        ["windows", "win", ".exe", ".msi"]
    } else if cfg!(target_os = "macos") {
        ["macos", "darwin", ".dmg", ".app"]
    } else {
        ["linux", "appimage", ".deb", ".rpm"]
    };
    info.assets.iter().find(|asset| {
        let name = asset.name.to_ascii_lowercase();
        platform.iter().any(|part| name.contains(part))
    })
}

pub fn download(asset: &UpdateAsset, output: &Path) -> Result<PathBuf, String> {
    download_with_progress(&asset.download_url, asset.size, output, |_| true)
}

/// Download asset with progress callback and moving-average speed calculation.
///
/// `on_progress` is called with current `DownloadProgress`. Returning `false` from
/// `on_progress` aborts the download and removes the temporary file.
pub fn download_with_progress<F>(
    download_url: &str,
    expected_size: u64,
    output: &Path,
    mut on_progress: F,
) -> Result<PathBuf, String>
where
    F: FnMut(DownloadProgress) -> bool,
{
    if !download_url.starts_with("https://") && !download_url.starts_with("http://") {
        return Err("update asset URL must use http or https".into());
    }
    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }

    let agent = ureq::AgentBuilder::new()
        .timeout_connect(std::time::Duration::from_secs(15))
        .timeout_read(std::time::Duration::from_secs(180))
        .build();

    let response = agent
        .get(download_url)
        .set("User-Agent", "AIToolPlus-Updater")
        .call()
        .map_err(|error| format!("update download failed: {error}"))?;

    let content_len = response
        .header("content-length")
        .and_then(|h| h.parse::<u64>().ok())
        .filter(|&len| len > 0)
        .unwrap_or(expected_size);

    let temp = output.with_extension("download.tmp");
    let mut file = std::fs::File::create(&temp).map_err(|e| e.to_string())?;
    let mut input = response.into_reader();

    let mut buffer = [0u8; 32768];
    let mut downloaded = 0u64;
    let mut last_instant = std::time::Instant::now();
    let mut last_bytes = 0u64;
    let mut speed_bps = 0.0;

    // Initial progress report
    let _ = on_progress(DownloadProgress {
        downloaded: 0,
        total: content_len,
        percentage: 0.0,
        speed_bps: 0,
    });

    loop {
        let bytes_read = match input.read(&mut buffer) {
            Ok(0) => break,
            Ok(n) => n,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(e) => {
                let _ = std::fs::remove_file(&temp);
                return Err(format!("download read error: {e}"));
            }
        };

        if let Err(e) = file.write_all(&buffer[..bytes_read]) {
            let _ = std::fs::remove_file(&temp);
            return Err(format!("download write error: {e}"));
        }

        downloaded = downloaded.saturating_add(bytes_read as u64);

        let now = std::time::Instant::now();
        let elapsed = now.duration_since(last_instant);
        if elapsed >= std::time::Duration::from_millis(200) {
            let delta_bytes = downloaded.saturating_sub(last_bytes);
            let instant_speed = delta_bytes as f64 / elapsed.as_secs_f64();
            if speed_bps == 0.0 {
                speed_bps = instant_speed;
            } else {
                speed_bps = speed_bps * 0.7 + instant_speed * 0.3;
            }
            last_bytes = downloaded;
            last_instant = now;
        }

        let percentage = if content_len > 0 {
            ((downloaded as f64 / content_len as f64) * 100.0).clamp(0.0, 100.0) as f32
        } else {
            0.0
        };

        let cont = on_progress(DownloadProgress {
            downloaded,
            total: content_len,
            percentage,
            speed_bps: speed_bps as u64,
        });

        if !cont {
            let _ = std::fs::remove_file(&temp);
            return Err("download cancelled by user".into());
        }
    }

    file.flush().map_err(|e| e.to_string())?;

    if content_len > 0 && downloaded != content_len {
        let _ = std::fs::remove_file(&temp);
        return Err(format!(
            "download size mismatch: expected {content_len}, got {downloaded}"
        ));
    }

    // Final progress report
    let _ = on_progress(DownloadProgress {
        downloaded,
        total: downloaded,
        percentage: 100.0,
        speed_bps: 0,
    });

    std::fs::rename(&temp, output).map_err(|e| e.to_string())?;
    Ok(output.to_path_buf())
}

/// Verify the SHA-256 checksum of a downloaded update asset.
pub fn verify_asset_sha256(file: &Path, expected_hex: &str) -> Result<bool, String> {
    use sha2::{Digest, Sha256};
    let mut f = std::fs::File::open(file).map_err(|e| e.to_string())?;
    let mut hasher = Sha256::new();
    std::io::copy(&mut f, &mut hasher).map_err(|e| e.to_string())?;
    let actual_bytes = hasher.finalize();
    let mut actual_hex = String::with_capacity(64);
    for b in actual_bytes {
        use std::fmt::Write;
        let _ = write!(actual_hex, "{:02x}", b);
    }
    Ok(actual_hex.eq_ignore_ascii_case(expected_hex.trim()))
}

/// Launch the downloaded installer or perform in-place replacement and restart.
/// Exits the current process upon successful launch.
pub fn install_update_and_restart(downloaded_asset: &Path) -> Result<(), String> {
    if !downloaded_asset.is_file() {
        return Err(format!(
            "update asset not found: {}",
            downloaded_asset.display()
        ));
    }
    let filename = downloaded_asset
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();

    let current_exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let current_pid = std::process::id();

    #[cfg(target_os = "windows")]
    {
        if filename.contains("setup")
            || filename.contains("installer")
            || filename.ends_with(".msi")
        {
            std::process::Command::new(downloaded_asset)
                .spawn()
                .map_err(|e| format!("failed to launch installer: {e}"))?;
            std::process::exit(0);
        } else if filename.ends_with(".exe") {
            let temp_script = downloaded_asset
                .parent()
                .unwrap_or(Path::new("."))
                .join("run_update.ps1");
            let script_content = format!(
                "Wait-Process -Id {pid} -Timeout 15 -ErrorAction SilentlyContinue\r\n\
                 Copy-Item -Force \"{src}\" \"{dst}\"\r\n\
                 Start-Process \"{dst}\"\r\n\
                 Remove-Item -Force \"$PSCommandPath\" -ErrorAction SilentlyContinue\r\n",
                pid = current_pid,
                src = downloaded_asset.to_string_lossy().replace('"', "`\""),
                dst = current_exe.to_string_lossy().replace('"', "`\"")
            );
            std::fs::write(&temp_script, script_content).map_err(|e| e.to_string())?;

            std::process::Command::new("powershell.exe")
                .args([
                    "-NoProfile",
                    "-WindowStyle",
                    "Hidden",
                    "-ExecutionPolicy",
                    "Bypass",
                    "-File",
                    &temp_script.to_string_lossy(),
                ])
                .spawn()
                .map_err(|e| format!("failed to spawn updater script: {e}"))?;

            std::process::exit(0);
        } else {
            Err("unsupported update asset format for auto-install".into())
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        std::process::Command::new(downloaded_asset)
            .spawn()
            .map_err(|e| format!("failed to launch update asset: {e}"))?;
        std::process::exit(0);
    }
}

fn normalize_version(value: &str) -> String {
    value
        .trim()
        .trim_start_matches('v')
        .split_once('-')
        .map(|(stable, _)| stable)
        .unwrap_or(value.trim().trim_start_matches('v'))
        .to_string()
}

fn compare_versions(left: &str, right: &str) -> std::cmp::Ordering {
    let parse = |value: &str| -> Vec<u64> {
        value
            .split('.')
            .map(|part| part.parse::<u64>().unwrap_or(0))
            .collect()
    };
    let left = parse(left);
    let right = parse(right);
    for index in 0..left.len().max(right.len()) {
        let order = left
            .get(index)
            .copied()
            .unwrap_or(0)
            .cmp(&right.get(index).copied().unwrap_or(0));
        if !order.is_eq() {
            return order;
        }
    }
    std::cmp::Ordering::Equal
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader};
    use std::net::TcpListener;

    #[test]
    fn version_compare_and_asset_selection() {
        assert!(compare_versions("1.2.0", "1.1.9").is_gt());
        assert!(compare_versions("1.0", "1.0.0").is_eq());
        assert_eq!(normalize_version("v2.3.4-beta"), "2.3.4");
        let info = UpdateInfo {
            current_version: "1.0.0".into(),
            latest_version: "2.0.0".into(),
            update_available: true,
            release_url: String::new(),
            release_notes: String::new(),
            published_at: None,
            assets: vec![
                UpdateAsset {
                    name: "aitoolplus-linux.AppImage".into(),
                    download_url: "https://example.com/linux".into(),
                    size: 1,
                },
                UpdateAsset {
                    name: "aitoolplus-windows.exe".into(),
                    download_url: "https://example.com/windows".into(),
                    size: 1,
                },
            ],
        };
        let best = best_asset(&info).unwrap();
        if cfg!(windows) {
            assert!(best.name.contains("windows"));
        }
    }

    #[test]
    fn update_check_against_mock_release_api() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            loop {
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                if line == "\r\n" || line.is_empty() {
                    break;
                }
            }
            let body = serde_json::json!({
                "tag_name": "v1.2.3",
                "html_url": "https://example.com/release",
                "body": "notes",
                "published_at": "2026-01-01T00:00:00Z",
                "prerelease": false,
                "draft": false,
                "assets": [{
                    "name": "aitoolplus-windows.exe",
                    "browser_download_url": "https://example.com/aitoolplus.exe",
                    "size": 42
                }]
            })
            .to_string();
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            stream.write_all(response.as_bytes()).unwrap();
        });
        let info = check_latest_at(&format!("http://{address}/latest"), "1.0.0").unwrap();
        server.join().unwrap();
        assert!(info.update_available);
        assert_eq!(info.latest_version, "1.2.3");
        assert_eq!(info.assets.len(), 1);
    }

    #[test]
    fn verify_asset_sha256_test() {
        let temp_dir = tempfile::tempdir().unwrap();
        let file = temp_dir.path().join("test-asset.bin");
        std::fs::write(&file, b"hello world\n").unwrap();
        let expected = "a948904f2f0f479b8f8197694b30184b0d2ed1c1cd2a1ec0fb85d299a192a447";
        assert!(verify_asset_sha256(&file, expected).unwrap());
        assert!(
            !verify_asset_sha256(
                &file,
                "0000000000000000000000000000000000000000000000000000000000000000"
            )
            .unwrap()
        );
    }

    #[test]
    fn update_mirror_url_transformations() {
        let orig = "https://github.com/aitoolplus/aitoolplus/releases/download/v0.1.0/aitoolplus-setup.exe";
        assert_eq!(UpdateMirror::Official.apply_url(orig, ""), orig);
        assert_eq!(
            UpdateMirror::GhProxyNet.apply_url(orig, ""),
            format!("https://ghproxy.net/{orig}")
        );
        assert_eq!(
            UpdateMirror::MirrorGhProxy.apply_url(orig, ""),
            format!("https://mirror.ghproxy.com/{orig}")
        );
        assert_eq!(
            UpdateMirror::Custom.apply_url(orig, "https://cdn.example.com"),
            format!("https://cdn.example.com/{orig}")
        );
        assert_eq!(
            UpdateMirror::Custom.apply_url(orig, "https://cdn.example.com/proxy?url={}"),
            format!("https://cdn.example.com/proxy?url={orig}")
        );
    }

    #[test]
    fn update_check_against_tauri_latest_json() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            loop {
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                if line == "\r\n" || line.is_empty() {
                    break;
                }
            }
            let body = serde_json::json!({
                "version": "1.3.0",
                "notes": "New UI improvements and bugfixes",
                "pub_date": "2026-09-22T12:00:00Z",
                "platforms": {
                    "windows-x86_64": {
                        "signature": "sig123",
                        "url": "https://example.com/aitoolplus-setup.exe"
                    }
                }
            })
            .to_string();
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            stream.write_all(response.as_bytes()).unwrap();
        });
        let info = check_latest_at(&format!("http://{address}/latest.json"), "1.0.0").unwrap();
        server.join().unwrap();
        assert!(info.update_available);
        assert_eq!(info.latest_version, "1.3.0");
        assert_eq!(info.assets.len(), 1);
        assert_eq!(info.assets[0].name, "aitoolplus-setup.exe");
    }

    #[test]
    fn download_with_progress_local_server() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            loop {
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                if line == "\r\n" || line.is_empty() {
                    break;
                }
            }
            let data = vec![42u8; 1024];
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                data.len()
            );
            stream.write_all(response.as_bytes()).unwrap();
            stream.write_all(&data).unwrap();
        });

        let temp_dir = tempfile::tempdir().unwrap();
        let out_file = temp_dir.path().join("downloaded.bin");

        let mut progress_count = 0;
        let downloaded_path = download_with_progress(
            &format!("http://{address}/download"),
            1024,
            &out_file,
            |p| {
                progress_count += 1;
                assert!(p.percentage <= 100.0);
                true
            },
        )
        .unwrap();

        server.join().unwrap();
        assert_eq!(downloaded_path, out_file);
        assert!(out_file.is_file());
        assert_eq!(std::fs::metadata(&out_file).unwrap().len(), 1024);
        assert!(progress_count >= 1);
    }
}

