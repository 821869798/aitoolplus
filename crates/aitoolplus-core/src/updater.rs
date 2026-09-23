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
    #[serde(
        alias = "gh_proxy",
        alias = "ghproxy",
        alias = "ghproxy_com",
        alias = "ghproxy_net",
        alias = "mirror_ghproxy",
        alias = "custom"
    )]
    GhProxy,
    #[serde(alias = "github", alias = "direct")]
    Official,
}

impl UpdateMirror {
    pub const ALL: &'static [UpdateMirror] = &[UpdateMirror::GhProxy, UpdateMirror::Official];

    pub fn id(&self) -> &'static str {
        match self {
            Self::GhProxy => "ghproxy",
            Self::Official => "official",
        }
    }

    pub fn from_id(id: &str) -> Self {
        match id {
            "official" | "github" | "direct" => Self::Official,
            _ => Self::GhProxy,
        }
    }

    pub fn display_name(&self, is_zh: bool) -> &'static str {
        match self {
            Self::GhProxy => {
                if is_zh {
                    "GhProxy 镜像加速 (gh-proxy.com)"
                } else {
                    "GhProxy Mirror (gh-proxy.com)"
                }
            }
            Self::Official => {
                if is_zh {
                    "GitHub 官方 (直连)"
                } else {
                    "GitHub Official (Direct)"
                }
            }
        }
    }

    pub fn apply_url(&self, original_url: &str, _custom_prefix: &str) -> String {
        let trimmed = original_url.trim();
        if trimmed.is_empty() {
            return String::new();
        }
        match self {
            Self::Official => trimmed.to_string(),
            Self::GhProxy => {
                if trimmed.starts_with("https://gh-proxy.com/") {
                    trimmed.to_string()
                } else {
                    format!("https://gh-proxy.com/{trimmed}")
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
    let version = std::env::var("AITOOLPLUS_CURRENT_VERSION")
        .ok()
        .filter(|v| !v.trim().is_empty())
        .unwrap_or_else(|| current_version.to_string());
    let api = std::env::var("AITOOLPLUS_UPDATE_API")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_RELEASES_API.into());
    check_latest_at(&api, &version)
}

pub fn check_latest_at(api: &str, current_version: &str) -> Result<UpdateInfo, String> {
    let res = ureq::AgentBuilder::new()
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .get(api)
        .set("Accept", "application/vnd.github+json, application/json")
        .set("User-Agent", "AIToolPlus-Updater")
        .call();

    let response = match res {
        Ok(resp) => resp,
        Err(ureq::Error::Status(404, _)) => {
            let current = normalize_version(current_version);
            return Ok(UpdateInfo {
                current_version: current.clone(),
                latest_version: current,
                update_available: false,
                release_url: "https://github.com/821869798/aitoolplus/releases".to_string(),
                release_notes: String::new(),
                published_at: None,
                assets: Vec::new(),
            });
        }
        Err(ureq::Error::Status(403, _)) => {
            return Err("GitHub API rate limit exceeded (HTTP 403), please try again later".to_string());
        }
        Err(error) => return Err(format!("update check failed: {error}")),
    };

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

/// Check if the currently running executable was installed via installer (NSIS)
/// or is running as a standalone / portable executable.
pub fn is_installer_installed() -> bool {
    if let Ok(val) = std::env::var("AITOOLPLUS_FORCE_INSTALLER") {
        return val == "1" || val.eq_ignore_ascii_case("true");
    }
    if let Ok(val) = std::env::var("AITOOLPLUS_FORCE_PORTABLE") {
        if val == "1" || val.eq_ignore_ascii_case("true") {
            return false;
        }
    }
    #[cfg(target_os = "windows")]
    {
        if let Ok(exe_path) = std::env::current_exe() {
            if let Some(parent) = exe_path.parent() {
                if parent.join("uninstall.exe").exists() {
                    return true;
                }
                let path_str = parent.to_string_lossy().to_ascii_lowercase();
                if path_str.contains(r"programs\aitoolplus")
                    || path_str.contains(r"program files\aitoolplus")
                {
                    return true;
                }
            }
        }
        false
    }
    #[cfg(not(target_os = "windows"))]
    {
        false
    }
}

pub fn best_asset(info: &UpdateInfo) -> Option<&UpdateAsset> {
    best_asset_for_mode(info, is_installer_installed())
}

pub fn best_asset_for_mode(info: &UpdateInfo, #[allow(unused_variables)] is_installer: bool) -> Option<&UpdateAsset> {
    let valid_assets: Vec<&UpdateAsset> = info
        .assets
        .iter()
        .filter(|asset| {
            let name = asset.name.to_ascii_lowercase();
            !name.ends_with(".sha256")
                && !name.ends_with(".sig")
                && !name.ends_with(".json")
                && !name.ends_with(".blockmap")
        })
        .collect();

    #[cfg(target_os = "windows")]
    {
        if is_installer {
            // Installer mode: prefer setup.exe / installer.exe / .msi
            if let Some(asset) = valid_assets.iter().find(|a| {
                let name = a.name.to_ascii_lowercase();
                (name.contains("setup") || name.contains("installer") || name.ends_with(".msi"))
                    && name.ends_with(".exe")
            }) {
                return Some(*asset);
            }
        } else {
            // Portable mode: prefer .zip containing windows / x86_64
            if let Some(asset) = valid_assets.iter().find(|a| {
                let name = a.name.to_ascii_lowercase();
                name.ends_with(".zip") && (name.contains("win") || name.contains("x86_64"))
            }) {
                return Some(*asset);
            }
            // If no zip, check for standalone non-installer .exe
            if let Some(asset) = valid_assets.iter().find(|a| {
                let name = a.name.to_ascii_lowercase();
                name.ends_with(".exe") && !name.contains("setup") && !name.contains("installer")
            }) {
                return Some(*asset);
            }
        }

        // Fallback: any windows executable or archive
        valid_assets.into_iter().find(|a| {
            let name = a.name.to_ascii_lowercase();
            name.ends_with(".exe") || name.ends_with(".zip") || name.ends_with(".msi")
        })
    }
    #[cfg(target_os = "macos")]
    {
        valid_assets.into_iter().find(|a| {
            let name = a.name.to_ascii_lowercase();
            name.ends_with(".dmg") || name.ends_with(".app.tar.gz") || name.ends_with(".zip")
        })
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        valid_assets.into_iter().find(|a| {
            let name = a.name.to_ascii_lowercase();
            name.ends_with(".appimage") || name.ends_with(".deb") || name.ends_with(".tar.gz")
        })
    }
}

/// Try to fetch the expected SHA-256 checksum from a matching `.sha256` asset in the release.
pub fn fetch_expected_sha256(
    info: &UpdateInfo,
    asset: &UpdateAsset,
    mirror: &UpdateMirror,
    custom_prefix: &str,
) -> Option<String> {
    let sha_asset_name = format!("{}.sha256", asset.name);
    let sha_asset = info.assets.iter().find(|a| a.name == sha_asset_name)?;
    let url = mirror.apply_url(&sha_asset.download_url, custom_prefix);
    let resp = ureq::AgentBuilder::new()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .get(&url)
        .set("User-Agent", "AIToolPlus-Updater")
        .call()
        .ok()?;
    let text = resp.into_string().ok()?;
    text.split_whitespace().next().map(|s| s.trim().to_lowercase())
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

/// Generate the PowerShell update script for in-place replacement and restart.
#[cfg(target_os = "windows")]
pub fn build_update_script_content(
    downloaded_asset: &Path,
    current_exe: &Path,
    current_pid: u32,
) -> Result<String, String> {
    let filename = downloaded_asset
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();

    if filename.ends_with(".zip") {
        let current_dir = current_exe
            .parent()
            .ok_or_else(|| "cannot determine application directory".to_string())?;

        Ok(format!(
            "Wait-Process -Id {pid} -Timeout 15 -ErrorAction SilentlyContinue\r\n\
             Start-Sleep -Milliseconds 600\r\n\
             $tempExtract = Join-Path $env:TEMP ('aitoolplus_extract_' + [System.Guid]::NewGuid().ToString('N'))\r\n\
             New-Item -ItemType Directory -Path $tempExtract -Force | Out-Null\r\n\
             Expand-Archive -Force -Path \"{src}\" -DestinationPath $tempExtract\r\n\
             $extractedExe = Get-ChildItem -Path $tempExtract -Filter \"aitoolplus.exe\" -Recurse | Select-Object -First 1\r\n\
             if ($extractedExe) {{\r\n\
                 $root = $extractedExe.Directory.FullName\r\n\
                 Copy-Item -Path \"$root\\*\" -Destination \"{dst_dir}\" -Recurse -Force\r\n\
             }} else {{\r\n\
                 Copy-Item -Path \"$tempExtract\\*\" -Destination \"{dst_dir}\" -Recurse -Force\r\n\
             }}\r\n\
             Remove-Item -Path $tempExtract -Recurse -Force -ErrorAction SilentlyContinue\r\n\
             Start-Process \"{dst_exe}\"\r\n\
             Remove-Item -Force \"$PSCommandPath\" -ErrorAction SilentlyContinue\r\n",
            pid = current_pid,
            src = downloaded_asset.to_string_lossy().replace('"', "`\""),
            dst_dir = current_dir.to_string_lossy().replace('"', "`\""),
            dst_exe = current_exe.to_string_lossy().replace('"', "`\"")
        ))
    } else if filename.ends_with(".exe") && !filename.contains("setup") && !filename.contains("installer") {
        Ok(format!(
            "Wait-Process -Id {pid} -Timeout 15 -ErrorAction SilentlyContinue\r\n\
             Start-Sleep -Milliseconds 600\r\n\
             Copy-Item -Force \"{src}\" \"{dst}\"\r\n\
             Start-Process \"{dst}\"\r\n\
             Remove-Item -Force \"$PSCommandPath\" -ErrorAction SilentlyContinue\r\n",
            pid = current_pid,
            src = downloaded_asset.to_string_lossy().replace('"', "`\""),
            dst = current_exe.to_string_lossy().replace('"', "`\"")
        ))
    } else {
        Err("unsupported update asset format for script replacement".into())
    }
}

/// Generate the POSIX shell update script for in-place replacement and restart on macOS/Linux.
#[cfg(not(target_os = "windows"))]
pub fn build_update_script_content(
    downloaded_asset: &Path,
    current_exe: &Path,
    current_pid: u32,
) -> Result<String, String> {
    let filename = downloaded_asset
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();

    let current_dir = current_exe
        .parent()
        .ok_or_else(|| "cannot determine application directory".to_string())?;

    if filename.ends_with(".tar.gz") || filename.ends_with(".tgz") {
        Ok(format!(
            "#!/bin/sh\n\
             while kill -0 {pid} 2>/dev/null; do sleep 0.2; done\n\
             tar -xzf \"{src}\" -C \"{dst_dir}\"\n\
             chmod +x \"{dst_exe}\"\n\
             nohup \"{dst_exe}\" >/dev/null 2>&1 &\n\
             rm -f \"$0\"\n",
            pid = current_pid,
            src = downloaded_asset.to_string_lossy(),
            dst_dir = current_dir.to_string_lossy(),
            dst_exe = current_exe.to_string_lossy()
        ))
    } else if filename.ends_with(".zip") {
        Ok(format!(
            "#!/bin/sh\n\
             while kill -0 {pid} 2>/dev/null; do sleep 0.2; done\n\
             unzip -o \"{src}\" -d \"{dst_dir}\"\n\
             chmod +x \"{dst_exe}\"\n\
             nohup \"{dst_exe}\" >/dev/null 2>&1 &\n\
             rm -f \"$0\"\n",
            pid = current_pid,
            src = downloaded_asset.to_string_lossy(),
            dst_dir = current_dir.to_string_lossy(),
            dst_exe = current_exe.to_string_lossy()
        ))
    } else if filename.ends_with(".appimage") || !filename.contains('.') {
        Ok(format!(
            "#!/bin/sh\n\
             while kill -0 {pid} 2>/dev/null; do sleep 0.2; done\n\
             cp -f \"{src}\" \"{dst_exe}\"\n\
             chmod +x \"{dst_exe}\"\n\
             nohup \"{dst_exe}\" >/dev/null 2>&1 &\n\
             rm -f \"$0\"\n",
            pid = current_pid,
            src = downloaded_asset.to_string_lossy(),
            dst_exe = current_exe.to_string_lossy()
        ))
    } else {
        Err("unsupported update asset format for script replacement".into())
    }
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
    #[cfg(target_os = "windows")]
    {
        let filename = downloaded_asset
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();

        if filename.contains("setup")
            || filename.contains("installer")
            || filename.ends_with(".msi")
        {
            std::process::Command::new(downloaded_asset)
                .spawn()
                .map_err(|e| format!("failed to launch installer: {e}"))?;
            std::process::exit(0);
        }

        let current_exe = std::env::current_exe().map_err(|e| e.to_string())?;
        let current_pid = std::process::id();
        let script_content = build_update_script_content(downloaded_asset, &current_exe, current_pid)?;

        let temp_script = downloaded_asset
            .parent()
            .unwrap_or(Path::new("."))
            .join("run_update.ps1");
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
    }
    #[cfg(not(target_os = "windows"))]
    {
        let filename = downloaded_asset
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();

        if filename.ends_with(".dmg") || filename.ends_with(".deb") || filename.ends_with(".rpm") {
            let _ = opener::open(downloaded_asset);
            std::process::exit(0);
        }

        let current_exe = std::env::current_exe().map_err(|e| e.to_string())?;
        let current_pid = std::process::id();
        let script_content = build_update_script_content(downloaded_asset, &current_exe, current_pid)?;

        let temp_script = downloaded_asset
            .parent()
            .unwrap_or(Path::new("."))
            .join("run_update.sh");
        std::fs::write(&temp_script, &script_content).map_err(|e| e.to_string())?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&temp_script, std::fs::Permissions::from_mode(0o755));
        }

        std::process::Command::new("/bin/sh")
            .arg(&temp_script)
            .spawn()
            .map_err(|e| format!("failed to spawn updater script: {e}"))?;

        std::process::exit(0);
    }
}

/// Whether the currently running executable is managed by Scoop.
pub fn is_scoop_install() -> bool {
    #[cfg(target_os = "windows")]
    {
        if let Ok(exe_path) = std::env::current_exe() {
            let roots: Vec<String> = ["SCOOP", "SCOOP_GLOBAL"]
                .into_iter()
                .filter_map(|name| std::env::var_os(name))
                .map(|path| path.to_string_lossy().into_owned())
                .collect();
            return is_scoop_install_path(&exe_path.to_string_lossy(), &roots);
        }
    }
    false
}

/// Pure path check for Scoop-managed executables (case-insensitive).
pub fn is_scoop_install_path(exe_path: &str, roots: &[String]) -> bool {
    let normalize = |path: &str| {
        let normalized = path.replace('/', "\\").to_lowercase();
        let normalized = if let Some(suffix) = normalized.strip_prefix(r"\\?\unc\") {
            format!(r"\\{suffix}")
        } else {
            normalized
                .strip_prefix(r"\\?\")
                .unwrap_or(&normalized)
                .to_string()
        };
        normalized.trim_end_matches('\\').to_string()
    };
    let normalized_exe = normalize(exe_path);
    normalized_exe.contains("\\scoop\\apps\\")
        || roots
            .iter()
            .filter(|root| !root.trim().is_empty())
            .any(|root| normalized_exe.starts_with(&format!("{}\\apps\\", normalize(root))))
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
            UpdateMirror::GhProxy.apply_url(orig, ""),
            format!("https://gh-proxy.com/{orig}")
        );
        // Idempotent when already prefixed
        assert_eq!(
            UpdateMirror::GhProxy.apply_url(&format!("https://gh-proxy.com/{orig}"), ""),
            format!("https://gh-proxy.com/{orig}")
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

    #[test]
    fn update_check_404_handles_gracefully_as_up_to_date() {
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
            let response = "HTTP/1.1 404 Not Found\r\nContent-Length: 23\r\nConnection: close\r\n\r\n{\"message\":\"Not Found\"}";
            stream.write_all(response.as_bytes()).unwrap();
        });

        let info = check_latest_at(&format!("http://{address}/latest"), "0.1.0").unwrap();
        server.join().unwrap();
        assert!(!info.update_available);
        assert_eq!(info.current_version, "0.1.0");
        assert_eq!(info.latest_version, "0.1.0");
        assert!(info.assets.is_empty());
    }

    #[test]
    fn update_check_403_rate_limit_returns_friendly_error() {
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
            let response = "HTTP/1.1 403 Forbidden\r\nContent-Length: 31\r\nConnection: close\r\n\r\n{\"message\":\"API rate limit\"}";
            stream.write_all(response.as_bytes()).unwrap();
        });

        let err = check_latest_at(&format!("http://{address}/latest"), "0.1.0").unwrap_err();
        server.join().unwrap();
        assert!(err.contains("rate limit"));
    }

    #[test]
    fn full_in_app_update_lifecycle_e2e() {
        use sha2::{Digest, Sha256};

        let payload_bytes = vec![0x90; 8192];
        let mut hasher = Sha256::new();
        hasher.update(&payload_bytes);
        let expected_sha256 = format!("{:x}", hasher.finalize());

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();

        let payload_clone = payload_bytes.clone();
        let server = std::thread::spawn(move || {
            // First connection: /api/latest
            let (mut stream1, _) = listener.accept().unwrap();
            let mut reader1 = BufReader::new(stream1.try_clone().unwrap());
            loop {
                let mut line = String::new();
                reader1.read_line(&mut line).unwrap();
                if line == "\r\n" || line.is_empty() {
                    break;
                }
            }
            let release_json = serde_json::json!({
                "tag_name": "v0.2.0",
                "html_url": "https://github.com/821869798/aitoolplus/releases/tag/v0.2.0",
                "body": "### What's New in v0.2.0\n- In-app update support\n- UI bug fixes",
                "published_at": "2026-09-22T19:00:00Z",
                "assets": [
                    {
                        "name": "aitoolplus-setup.exe",
                        "browser_download_url": format!("http://{address}/download/aitoolplus-setup.exe"),
                        "size": payload_clone.len() as u64
                    }
                ]
            }).to_string();
            let resp1 = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{release_json}",
                release_json.len()
            );
            stream1.write_all(resp1.as_bytes()).unwrap();

            // Second connection: /download/aitoolplus-setup.exe
            let (mut stream2, _) = listener.accept().unwrap();
            let mut reader2 = BufReader::new(stream2.try_clone().unwrap());
            loop {
                let mut line = String::new();
                reader2.read_line(&mut line).unwrap();
                if line == "\r\n" || line.is_empty() {
                    break;
                }
            }
            let resp2 = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                payload_clone.len()
            );
            stream2.write_all(resp2.as_bytes()).unwrap();
            stream2.write_all(&payload_clone).unwrap();
        });

        // 1. Check for updates from current v0.1.0
        let update_info = check_latest_at(&format!("http://{address}/api/latest"), "0.1.0").unwrap();
        assert!(update_info.update_available);
        assert_eq!(update_info.latest_version, "0.2.0");
        assert_eq!(update_info.current_version, "0.1.0");
        assert!(update_info.release_notes.contains("In-app update support"));

        // 2. Select best asset
        let asset = best_asset(&update_info).expect("asset found");
        assert_eq!(asset.name, "aitoolplus-setup.exe");
        assert_eq!(asset.size, 8192);

        // 3. Download asset with progress tracking
        let temp_dir = tempfile::tempdir().unwrap();
        let target_file = temp_dir.path().join(&asset.name);

        let mut progress_events = Vec::new();
        let downloaded = download_with_progress(
            &asset.download_url,
            asset.size,
            &target_file,
            |p| {
                progress_events.push(p);
                true
            },
        ).expect("download succeeds");

        server.join().unwrap();

        // 4. Verify downloaded file integrity
        assert_eq!(downloaded, target_file);
        assert!(target_file.is_file());
        let disk_bytes = std::fs::read(&target_file).unwrap();
        assert_eq!(disk_bytes, payload_bytes);

        // 5. Verify SHA-256 calculation
        assert!(verify_asset_sha256(&target_file, &expected_sha256).unwrap());
        assert!(!verify_asset_sha256(&target_file, "wrong_hash").unwrap());

        // 6. Verify progress sequence reached 100%
        assert!(!progress_events.is_empty());
        let last_event = progress_events.last().unwrap();
        assert_eq!(last_event.downloaded, 8192);
        assert_eq!(last_event.total, 8192);
        assert_eq!(last_event.percentage, 100.0);
    }

    #[test]
    fn detects_scoop_install_paths() {
        assert!(is_scoop_install_path(
            r"C:\Users\User\scoop\apps\aitoolplus\0.1.0\aitoolplus.exe",
            &[],
        ));
        assert!(is_scoop_install_path(
            r"D:\Scoop\Apps\aitoolplus\current\aitoolplus.exe",
            &[],
        ));
        assert!(!is_scoop_install_path(
            r"C:\Program Files\AIToolPlus\aitoolplus.exe",
            &[],
        ));
        assert!(!is_scoop_install_path(
            r"C:\Users\User\AppData\Local\Programs\AIToolPlus\aitoolplus.exe",
            &[],
        ));
    }

    #[test]
    fn test_portable_vs_installer_asset_selection() {
        let assets = vec![
            UpdateAsset {
                name: "aitoolplus-setup.exe.sha256".to_string(),
                download_url: "https://example.com/aitoolplus-setup.exe.sha256".to_string(),
                size: 66,
            },
            UpdateAsset {
                name: "aitoolplus-setup.exe".to_string(),
                download_url: "https://example.com/aitoolplus-setup.exe".to_string(),
                size: 5127083,
            },
            UpdateAsset {
                name: "aitoolplus-windows-x86_64.zip".to_string(),
                download_url: "https://example.com/aitoolplus-windows-x86_64.zip".to_string(),
                size: 6564794,
            },
            UpdateAsset {
                name: "aitoolplus-windows-x86_64.zip.sha256".to_string(),
                download_url: "https://example.com/aitoolplus-windows-x86_64.zip.sha256".to_string(),
                size: 66,
            },
            UpdateAsset {
                name: "latest.json".to_string(),
                download_url: "https://example.com/latest.json".to_string(),
                size: 351,
            },
        ];

        let info = UpdateInfo {
            current_version: "0.0.1".to_string(),
            latest_version: "0.1.0".to_string(),
            update_available: true,
            release_url: "https://example.com".to_string(),
            release_notes: "test notes".to_string(),
            published_at: None,
            assets,
        };

        // When running in installer mode, setup.exe should be selected
        let installer_asset = best_asset_for_mode(&info, true).expect("installer asset");
        assert_eq!(installer_asset.name, "aitoolplus-setup.exe");

        // When running in portable mode, zip should be selected
        let portable_asset = best_asset_for_mode(&info, false).expect("portable asset");
        assert_eq!(portable_asset.name, "aitoolplus-windows-x86_64.zip");
    }

    #[test]
    #[cfg(target_os = "windows")]
    fn test_build_update_script_zip() {
        let zip_asset = Path::new(r"C:\Users\User\AppData\Roaming\aitoolplus\updates\aitoolplus-windows-x86_64.zip");
        let current_exe = Path::new(r"D:\Tools\aitoolplus\aitoolplus.exe");
        let script = build_update_script_content(zip_asset, current_exe, 12345).unwrap();

        assert!(script.contains("Wait-Process -Id 12345"));
        assert!(script.contains("Expand-Archive -Force"));
        assert!(script.contains(r"aitoolplus-windows-x86_64.zip"));
        assert!(script.contains(r"D:\Tools\aitoolplus"));
        assert!(script.contains(r"Start-Process"));
        assert!(script.contains("Remove-Item -Force \"$PSCommandPath\""));
    }

    #[test]
    #[cfg(target_os = "windows")]
    fn test_build_update_script_standalone_exe() {
        let exe_asset = Path::new(r"C:\Users\User\AppData\Roaming\aitoolplus\updates\aitoolplus.exe");
        let current_exe = Path::new(r"D:\Tools\aitoolplus\aitoolplus.exe");
        let script = build_update_script_content(exe_asset, current_exe, 12345).unwrap();

        assert!(script.contains("Wait-Process -Id 12345"));
        assert!(script.contains("Copy-Item -Force"));
        assert!(script.contains(r"Start-Process"));
    }

    #[test]
    fn test_live_github_release_detection_and_portable_asset() {
        let info = match check_latest("0.0.1") {
            Ok(info) => info,
            Err(e) => {
                eprintln!("Skipping live test due to network/rate limit: {e}");
                return;
            }
        };

        assert!(info.update_available, "Update should be available from 0.0.1");
        assert!(compare_versions(&info.latest_version, "0.0.1").is_gt(), "latest version should be > 0.0.1");

        // Portable mode check
        let portable_asset = best_asset_for_mode(&info, false).expect("portable asset found");
        assert_eq!(portable_asset.name, "aitoolplus-windows-x86_64.zip");
        assert!(portable_asset.download_url.contains("aitoolplus-windows-x86_64.zip"));

        // Installer mode check
        let installer_asset = best_asset_for_mode(&info, true).expect("installer asset found");
        assert_eq!(installer_asset.name, "aitoolplus-setup.exe");
        assert!(installer_asset.download_url.contains("aitoolplus-setup.exe"));
    }

    #[test]
    fn test_live_download_and_verify_portable_zip() {
        let info = match check_latest("0.0.1") {
            Ok(info) => info,
            Err(e) => {
                eprintln!("Skipping live test: {e}");
                return;
            }
        };

        let asset = best_asset_for_mode(&info, false).expect("portable asset found");
        let mirror = UpdateMirror::GhProxy;
        let download_url = mirror.apply_url(&asset.download_url, "");
        println!("Testing live download from: {download_url}");

        let expected_sha = fetch_expected_sha256(&info, asset, &mirror, "").expect("expected sha fetched");
        println!("Expected SHA-256: {expected_sha}");
        assert_eq!(expected_sha.len(), 64);

        let temp_dir = tempfile::tempdir().unwrap();
        let target_zip = temp_dir.path().join(&asset.name);

        let downloaded_path = download_with_progress(&download_url, asset.size, &target_zip, |p| {
            if (p.percentage as u32) % 25 == 0 {
                println!("Download progress: {:.1}% ({}/{} bytes)", p.percentage, p.downloaded, p.total);
            }
            true
        }).expect("download succeeded");

        assert_eq!(downloaded_path, target_zip);
        assert!(target_zip.is_file());

        let sha_valid = verify_asset_sha256(&target_zip, &expected_sha).expect("sha verification succeeded");
        assert!(sha_valid, "Downloaded zip hash must match expected SHA-256!");
        println!("SHA-256 verification passed!");

        // Now test extraction logic
        let extract_dir = temp_dir.path().join("extracted");
        std::fs::create_dir_all(&extract_dir).unwrap();
        let file = std::fs::File::open(&target_zip).unwrap();
        let mut archive = zip::ZipArchive::new(file).unwrap();
        archive.extract(&extract_dir).unwrap();
        assert!(extract_dir.join("aitoolplus.exe").exists(), "Extracted zip contains aitoolplus.exe!");
        println!("Extraction verified: aitoolplus.exe present!");
    }

    #[test]
    fn test_env_var_version_override_and_best_asset() {
        unsafe {
            std::env::set_var("AITOOLPLUS_CURRENT_VERSION", "0.0.1");
            std::env::set_var("AITOOLPLUS_FORCE_PORTABLE", "1");
            std::env::remove_var("AITOOLPLUS_FORCE_INSTALLER");
        }

        let info = match check_latest("0.1.0") {
            Ok(info) => info,
            Err(e) => {
                eprintln!("Skipping live test: {e}");
                return;
            }
        };

        assert!(info.update_available);
        assert_eq!(info.current_version, "0.0.1");
        assert!(compare_versions(&info.latest_version, "0.0.1").is_gt());

        let asset = best_asset(&info).expect("asset found");
        assert_eq!(asset.name, "aitoolplus-windows-x86_64.zip");

        // Now test forcing installer mode
        unsafe {
            std::env::remove_var("AITOOLPLUS_FORCE_PORTABLE");
            std::env::set_var("AITOOLPLUS_FORCE_INSTALLER", "1");
        }
        let installer_asset = best_asset(&info).expect("installer asset found");
        assert_eq!(installer_asset.name, "aitoolplus-setup.exe");

        // Clean up environment variables
        unsafe {
            std::env::remove_var("AITOOLPLUS_CURRENT_VERSION");
            std::env::remove_var("AITOOLPLUS_FORCE_INSTALLER");
        }
    }
}



