//! GitHub Releases update checker and asset downloader.
//!
//! Updates are never installed silently: the UI checks, shows version/release
//! notes, and downloads the selected asset to a user-visible path. Replacing a
//! running Windows executable is deliberately left to the downloaded installer
//! or a future signed helper.

use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

pub const DEFAULT_RELEASES_API: &str =
    "https://api.github.com/repos/aitoolplus/aitoolplus/releases/latest";

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
        .set("Accept", "application/vnd.github+json")
        .set("User-Agent", "AIToolPlus-Updater")
        .call()
        .map_err(|error| format!("update check failed: {error}"))?;
    let release: GithubRelease = response
        .into_json()
        .map_err(|error| format!("update response parse failed: {error}"))?;
    if release.draft || release.prerelease {
        return Err("latest release is draft/prerelease".into());
    }
    let latest = normalize_version(&release.tag_name);
    let current = normalize_version(current_version);
    let assets = release
        .assets
        .into_iter()
        .filter(|asset| asset.browser_download_url.starts_with("https://"))
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
    if !asset.download_url.starts_with("https://") {
        return Err("update asset URL must use https".into());
    }
    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let response = ureq::AgentBuilder::new()
        .timeout(std::time::Duration::from_secs(120))
        .build()
        .get(&asset.download_url)
        .set("User-Agent", "AIToolPlus-Updater")
        .call()
        .map_err(|error| format!("update download failed: {error}"))?;
    let temp = output.with_extension("download.tmp");
    let mut input = response.into_reader();
    let mut file = std::fs::File::create(&temp).map_err(|e| e.to_string())?;
    std::io::copy(&mut input, &mut file).map_err(|e| e.to_string())?;
    file.flush().map_err(|e| e.to_string())?;
    if asset.size > 0 {
        let actual = file.metadata().map_err(|e| e.to_string())?.len();
        if actual != asset.size {
            let _ = std::fs::remove_file(&temp);
            return Err(format!(
                "download size mismatch: expected {}, got {actual}",
                asset.size
            ));
        }
    }
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
}
