//! WebDAV backup transport: connection test, upload, list, download, delete.
//!
//! Uses ordinary WebDAV verbs (PROPFIND/MKCOL/PUT/GET/DELETE) and Basic
//! authentication. Filenames are generated locally and restricted to safe
//! backup names, so no remote path injection is accepted.

use std::io::Write;
use std::path::{Path, PathBuf};

use base64::Engine;

use crate::settings::WebDavConfig;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteBackup {
    pub name: String,
    pub href: String,
}

fn auth_header(config: &WebDavConfig) -> Option<String> {
    if config.username.is_empty() && config.password.is_empty() {
        return None;
    }
    let credentials = base64::engine::general_purpose::STANDARD
        .encode(format!("{}:{}", config.username, config.password));
    Some(format!("Basic {credentials}"))
}

fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout(std::time::Duration::from_secs(20))
        .build()
}

fn request(agent: &ureq::Agent, method: &str, url: &str, config: &WebDavConfig) -> ureq::Request {
    let mut request = agent.request(method, url);
    if let Some(auth) = auth_header(config) {
        request = request.set("Authorization", &auth);
    }
    request
}

fn base_url(config: &WebDavConfig) -> Result<String, String> {
    let base = config.url.trim().trim_end_matches('/');
    if !(base.starts_with("http://") || base.starts_with("https://")) {
        return Err("WebDAV URL must start with http:// or https://".into());
    }
    Ok(base.to_string())
}

fn directory_url(config: &WebDavConfig) -> Result<String, String> {
    let base = base_url(config)?;
    let directory = safe_segments(&config.remote_directory)?;
    if directory.is_empty() {
        Ok(base)
    } else {
        Ok(format!("{base}/{directory}"))
    }
}

fn backup_url(config: &WebDavConfig, filename: &str) -> Result<String, String> {
    if !safe_filename(filename) {
        return Err("unsafe WebDAV backup filename".into());
    }
    Ok(format!("{}/{}", directory_url(config)?, filename))
}

fn safe_segments(value: &str) -> Result<String, String> {
    let normalized = value.trim().trim_matches('/').replace('\\', "/");
    if normalized
        .split('/')
        .any(|segment| segment.is_empty() || segment == "." || segment == "..")
    {
        return Err("unsafe WebDAV remote directory".into());
    }
    Ok(normalized)
}

fn safe_filename(value: &str) -> bool {
    !value.is_empty()
        && value.ends_with(".zip")
        && !value.contains('/')
        && !value.contains('\\')
        && !value.contains("..")
}

/// Test WebDAV credentials with a depth-0 PROPFIND.
pub fn test_connection(config: &WebDavConfig) -> Result<(), String> {
    let agent = agent();
    let response = request(&agent, "PROPFIND", &base_url(config)?, config)
        .set("Depth", "0")
        .send_string("")
        .map_err(map_error)?;
    if (200..300).contains(&response.status()) || response.status() == 207 {
        Ok(())
    } else {
        Err(format!("WebDAV returned HTTP {}", response.status()))
    }
}

/// Ensure the configured remote directory exists. MKCOL 405 means it
/// already exists and is accepted.
pub fn ensure_directory(config: &WebDavConfig) -> Result<(), String> {
    let directory = config.remote_directory.trim().trim_matches('/');
    if directory.is_empty() {
        return Ok(());
    }
    let agent = agent();
    let mut current = base_url(config)?;
    for segment in safe_segments(directory)?.split('/') {
        current.push('/');
        current.push_str(segment);
        match request(&agent, "MKCOL", &current, config).call() {
            Ok(response) if (200..300).contains(&response.status()) => {}
            Err(ureq::Error::Status(405, _)) => {}
            Err(error) => return Err(map_error(error)),
            Ok(response) => return Err(format!("MKCOL returned HTTP {}", response.status())),
        }
    }
    Ok(())
}

pub fn upload(config: &WebDavConfig, local_file: &Path) -> Result<RemoteBackup, String> {
    let filename = local_file
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or("backup has no filename")?;
    if !safe_filename(filename) {
        return Err("backup filename must be a safe .zip name".into());
    }
    ensure_directory(config)?;
    let bytes = std::fs::read(local_file).map_err(|e| e.to_string())?;
    let agent = agent();
    request(&agent, "PUT", &backup_url(config, filename)?, config)
        .set("Content-Type", "application/zip")
        .send_bytes(&bytes)
        .map_err(map_error)?;
    Ok(RemoteBackup {
        name: filename.into(),
        href: backup_url(config, filename)?,
    })
}

pub fn list(config: &WebDavConfig) -> Result<Vec<RemoteBackup>, String> {
    let agent = agent();
    let response = request(&agent, "PROPFIND", &directory_url(config)?, config)
        .set("Depth", "1")
        .send_string("")
        .map_err(map_error)?;
    let body = response.into_string().map_err(|e| e.to_string())?;
    let mut backups = vec![];
    for href in xml_elements(&body, "href") {
        let decoded = percent_decode(&href);
        let name = decoded
            .trim_end_matches('/')
            .rsplit('/')
            .next()
            .unwrap_or("")
            .to_string();
        if safe_filename(&name) {
            backups.push(RemoteBackup { name, href });
        }
    }
    backups.sort_by(|a, b| b.name.cmp(&a.name));
    backups.dedup_by(|a, b| a.name == b.name);
    Ok(backups)
}

pub fn download(config: &WebDavConfig, filename: &str, output: &Path) -> Result<PathBuf, String> {
    let agent = agent();
    let response = request(&agent, "GET", &backup_url(config, filename)?, config)
        .call()
        .map_err(map_error)?;
    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let temp = output.with_extension("download.tmp");
    let mut input = response.into_reader();
    let mut file = std::fs::File::create(&temp).map_err(|e| e.to_string())?;
    std::io::copy(&mut input, &mut file).map_err(|e| e.to_string())?;
    file.flush().map_err(|e| e.to_string())?;
    std::fs::rename(&temp, output).map_err(|e| e.to_string())?;
    Ok(output.to_path_buf())
}

pub fn delete(config: &WebDavConfig, filename: &str) -> Result<(), String> {
    let agent = agent();
    request(&agent, "DELETE", &backup_url(config, filename)?, config)
        .call()
        .map_err(map_error)?;
    Ok(())
}

fn map_error(error: ureq::Error) -> String {
    match error {
        ureq::Error::Status(401 | 403, _) => "WebDAV authentication failed".into(),
        ureq::Error::Status(status, response) => format!(
            "WebDAV HTTP {status}: {}",
            response.into_string().unwrap_or_default()
        ),
        ureq::Error::Transport(error) => format!("WebDAV transport error: {error}"),
    }
}

fn xml_elements(xml: &str, local_name: &str) -> Vec<String> {
    let mut values = vec![];
    let lower = xml.to_ascii_lowercase();
    let mut cursor = 0;
    while let Some(open_start) = lower[cursor..].find('<') {
        let open_start = cursor + open_start;
        let Some(open_end_rel) = lower[open_start..].find('>') else {
            break;
        };
        let open_end = open_start + open_end_rel;
        let tag = &lower[open_start + 1..open_end];
        let tag_name = tag
            .split_whitespace()
            .next()
            .unwrap_or("")
            .trim_start_matches('/');
        if tag_name.rsplit(':').next() == Some(local_name) && !tag.starts_with('/') {
            let close_suffix = format!("</{tag_name}>");
            if let Some(close_rel) = lower[open_end + 1..].find(&close_suffix) {
                let close = open_end + 1 + close_rel;
                values.push(html_unescape(&xml[open_end + 1..close]));
                cursor = close + close_suffix.len();
                continue;
            }
        }
        cursor = open_end + 1;
    }
    values
}

fn percent_decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%'
            && index + 2 < bytes.len()
            && let (Some(high), Some(low)) = (hex(bytes[index + 1]), hex(bytes[index + 2]))
        {
            out.push(high * 16 + low);
            index += 3;
        } else {
            out.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8_lossy(&out).to_string()
}

fn hex(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        b'A'..=b'F' => Some(value - b'A' + 10),
        _ => None,
    }
}

fn html_unescape(value: &str) -> String {
    value
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> WebDavConfig {
        WebDavConfig {
            url: "https://dav.example.com/root/".into(),
            username: "u".into(),
            password: "p".into(),
            remote_directory: "AITool/Backups".into(),
        }
    }

    #[test]
    fn urls_are_safe_and_normalized() {
        assert_eq!(
            directory_url(&config()).unwrap(),
            "https://dav.example.com/root/AITool/Backups"
        );
        assert_eq!(
            backup_url(&config(), "aitoolplus-auto-20260101.zip").unwrap(),
            "https://dav.example.com/root/AITool/Backups/aitoolplus-auto-20260101.zip"
        );
        assert!(backup_url(&config(), "../bad.zip").is_err());
        let mut invalid = config();
        invalid.remote_directory = "../bad".into();
        assert!(directory_url(&invalid).is_err());
    }

    #[test]
    fn parses_multistatus_hrefs() {
        let xml = r#"<?xml version="1.0"?><d:multistatus xmlns:d="DAV:"><d:response><d:href>/root/AITool/Backups/</d:href></d:response><d:response><d:href>/root/AITool/Backups/aitoolplus-auto-1.zip</d:href></d:response><d:response><d:href>/root/AITool/Backups/manual%20backup.zip</d:href></d:response></d:multistatus>"#;
        let hrefs = xml_elements(xml, "href");
        assert_eq!(hrefs.len(), 3);
        assert_eq!(
            percent_decode(&hrefs[2]),
            "/root/AITool/Backups/manual backup.zip"
        );
        let names: Vec<String> = hrefs
            .into_iter()
            .map(|href| percent_decode(&href))
            .filter_map(|href| href.rsplit('/').next().map(String::from))
            .filter(|name| safe_filename(name))
            .collect();
        assert_eq!(names, vec!["aitoolplus-auto-1.zip", "manual backup.zip"]);
    }

    #[test]
    fn basic_auth_header() {
        assert_eq!(auth_header(&config()).as_deref(), Some("Basic dTpw"));
    }

    #[test]
    fn end_to_end_against_mock_webdav() {
        use std::io::{BufRead, BufReader, Read};
        use std::net::TcpListener;
        use std::sync::{Arc, Mutex};

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let requests = Arc::new(Mutex::new(Vec::<String>::new()));
        let seen = requests.clone();
        let server = std::thread::spawn(move || {
            for _ in 0..6 {
                let (stream, _) = listener.accept().unwrap();
                let mut reader = BufReader::new(stream);
                let mut first = String::new();
                reader.read_line(&mut first).unwrap();
                let request_line = first.trim().to_string();
                seen.lock().unwrap().push(request_line.clone());
                let mut length = 0usize;
                loop {
                    let mut line = String::new();
                    reader.read_line(&mut line).unwrap();
                    if line == "\r\n" || line.is_empty() {
                        break;
                    }
                    if let Some(value) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                        length = value.trim().parse().unwrap_or(0);
                    }
                }
                if length > 0 {
                    let mut body = vec![0; length];
                    reader.read_exact(&mut body).unwrap();
                }
                let method = request_line.split_whitespace().next().unwrap_or("");
                let (status, content_type, body) = match method {
                    "PROPFIND" if request_line.contains("/backups ") => (
                        "207 Multi-Status",
                        "application/xml",
                        "<d:multistatus xmlns:d=\"DAV:\"><d:response><d:href>/root/backups/aitoolplus-auto-1.zip</d:href></d:response></d:multistatus>".as_bytes().to_vec(),
                    ),
                    "PROPFIND" => (
                        "207 Multi-Status",
                        "application/xml",
                        b"<d:multistatus xmlns:d=\"DAV:\"/>".to_vec(),
                    ),
                    "MKCOL" => ("201 Created", "text/plain", Vec::new()),
                    "PUT" => ("201 Created", "text/plain", Vec::new()),
                    "GET" => ("200 OK", "application/zip", b"mock-zip".to_vec()),
                    "DELETE" => ("204 No Content", "text/plain", Vec::new()),
                    _ => ("500 Error", "text/plain", Vec::new()),
                };
                let response = format!(
                    "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                let stream = reader.get_mut();
                stream.write_all(response.as_bytes()).unwrap();
                stream.write_all(&body).unwrap();
                stream.flush().unwrap();
            }
        });

        let config = WebDavConfig {
            url: format!("http://{address}/root"),
            username: "user".into(),
            password: "pass".into(),
            remote_directory: "backups".into(),
        };
        test_connection(&config).unwrap();
        let directory = tempfile::tempdir().unwrap();
        let local = directory.path().join("aitoolplus-auto-1.zip");
        std::fs::write(&local, b"zip-data").unwrap();
        let uploaded = upload(&config, &local).unwrap();
        assert_eq!(uploaded.name, "aitoolplus-auto-1.zip");
        let listed = list(&config).unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].name, "aitoolplus-auto-1.zip");
        let downloaded = directory.path().join("download.zip");
        download(&config, "aitoolplus-auto-1.zip", &downloaded).unwrap();
        assert_eq!(std::fs::read(&downloaded).unwrap(), b"mock-zip");
        delete(&config, "aitoolplus-auto-1.zip").unwrap();
        server.join().unwrap();

        let requests = requests.lock().unwrap();
        assert!(requests.iter().any(|request| request.starts_with("MKCOL ")));
        assert!(requests.iter().any(|request| request.starts_with("PUT ")));
        assert!(requests.iter().any(|request| request.starts_with("GET ")));
        assert!(
            requests
                .iter()
                .any(|request| request.starts_with("DELETE "))
        );
    }

    #[test]
    #[ignore]
    fn real_jianguoyun_sync_e2e() {
        let (url, username, password) = if let Ok(home) =
            std::env::var("USERPROFILE").or_else(|_| std::env::var("HOME"))
        {
            let settings_path = PathBuf::from(home).join(".cc-switch").join("settings.json");
            if let Ok(content) = std::fs::read_to_string(&settings_path) {
                if let Ok(val) = serde_json::from_str::<serde_json::Value>(&content) {
                    let w = &val["webdavSync"];
                    (
                        w["baseUrl"].as_str().unwrap_or_default().to_string(),
                        w["username"].as_str().unwrap_or_default().to_string(),
                        w["password"].as_str().unwrap_or_default().to_string(),
                    )
                } else {
                    return;
                }
            } else {
                return;
            }
        } else {
            return;
        };

        if url.is_empty() || username.is_empty() || password.is_empty() {
            return;
        }

        let config = WebDavConfig {
            url,
            username,
            password,
            remote_directory: "aitoolplus-test".into(),
        };

        // 1. Test Connection
        println!("1. Testing connection...");
        test_connection(&config).expect("test_connection failed");
        println!("Connection OK!");

        // 2. Ensure directory
        println!("2. Ensuring remote directory...");
        ensure_directory(&config).expect("ensure_directory failed");
        println!("Directory OK!");

        // 3. Create dummy app data & backup
        println!("3. Creating full local backup with Antigravity accounts...");
        let temp_dir = tempfile::tempdir().unwrap();
        let home = temp_dir.path().join("home");
        let app_data = temp_dir.path().join("appdata");
        std::fs::create_dir_all(&home).unwrap();
        std::fs::create_dir_all(&app_data).unwrap();

        let paths = crate::paths::Paths::new(&home, &app_data);
        let mut settings = crate::settings::AppSettings::default();
        settings.backup_cli_config_files_enabled = true;

        // Write store, settings, antigravity_accounts, and some CLI config files
        std::fs::write(paths.store_file(), r#"{"schema_version":1,"test_store":true}"#).unwrap();
        std::fs::write(paths.settings_file(), r#"{"theme_mode":"Dark","test_settings":true}"#).unwrap();
        std::fs::write(
            paths.app_data.join("antigravity_accounts.json"),
            r#"{"accounts":[{"email":"test@gmail.com","refresh_token":"rt_xyz_test"}]}"#,
        ).unwrap();

        let claude_dir = paths.tool_root(crate::tools::ToolId::ClaudeCode);
        std::fs::create_dir_all(&claude_dir).unwrap();
        std::fs::write(claude_dir.join("settings.json"), r#"{"claude_test":true}"#).unwrap();

        let gemini_dir = paths.tool_root(crate::tools::ToolId::GeminiCli);
        std::fs::create_dir_all(&gemini_dir).unwrap();
        std::fs::write(gemini_dir.join("oauth_creds.json"), r#"{"access_token":"at_test"}"#).unwrap();

        let local_backup_file = temp_dir.path().join("aitoolplus-test-backup.zip");
        let backup_report = crate::backup::create_backup(&paths, &settings, &local_backup_file)
            .expect("create_backup failed");
        println!("Backup created with {} files", backup_report.file_count);

        // 4. Upload to Jianguoyun
        println!("4. Uploading to Jianguoyun...");
        let remote_backup = upload(&config, &local_backup_file).expect("upload failed");
        println!("Uploaded: {} ({})", remote_backup.name, remote_backup.href);

        // 5. List remote backups
        println!("5. Listing remote backups...");
        let remote_list = list(&config).expect("list failed");
        println!("Found {} remote backups", remote_list.len());
        for b in &remote_list {
            println!(" - {} ({})", b.name, b.href);
        }
        assert!(remote_list.iter().any(|b| b.name == "aitoolplus-test-backup.zip"));

        // 6. Download from Jianguoyun
        println!("6. Downloading from Jianguoyun...");
        let downloaded_file = temp_dir.path().join("downloaded.zip");
        download(&config, "aitoolplus-test-backup.zip", &downloaded_file)
            .expect("download failed");
        assert!(downloaded_file.exists());
        println!("Downloaded successfully!");

        // 7. Inspect downloaded backup
        println!("7. Inspecting manifest...");
        let manifest = crate::backup::inspect_backup(&downloaded_file)
            .expect("inspect_backup failed");
        println!("Manifest has {} entries", manifest.entries.len());
        assert!(manifest.entries.iter().any(|e| e.restore_target == "appdata/antigravity_accounts.json"));
        assert!(manifest.entries.iter().any(|e| e.restore_target == "appdata/store.json"));
        assert!(manifest.entries.iter().any(|e| e.restore_target == "home/.gemini/oauth_creds.json"));

        // 8. Restore to new target
        println!("8. Restoring to clean target directory...");
        let restore_home = temp_dir.path().join("restore_home");
        let restore_appdata = temp_dir.path().join("restore_appdata");
        let restore_paths = crate::paths::Paths::new(&restore_home, &restore_appdata);

        let restore_report = crate::backup::restore_backup(&restore_paths, &downloaded_file, false)
            .expect("restore_backup failed");
        println!("Restored {} files", restore_report.restored);
        assert!(restore_report.restored >= 4);

        // Verify restored contents
        let restored_ag = std::fs::read_to_string(restore_paths.app_data.join("antigravity_accounts.json")).unwrap();
        assert!(restored_ag.contains("test@gmail.com"));
        assert!(restored_ag.contains("rt_xyz_test"));

        let restored_store = std::fs::read_to_string(restore_paths.store_file()).unwrap();
        assert!(restored_store.contains("test_store"));

        let restored_oauth = std::fs::read_to_string(restore_paths.home.join(".gemini").join("oauth_creds.json")).unwrap();
        assert!(restored_oauth.contains("at_test"));

        println!("All restored contents match perfectly!");

        // 9. Cleanup on Jianguoyun
        println!("9. Cleaning up test backup on Jianguoyun...");
        delete(&config, "aitoolplus-test-backup.zip").expect("delete failed");
        println!("Deleted!");

        // 10. Verify deletion
        let remote_list_after = list(&config).expect("list after delete failed");
        assert!(!remote_list_after.iter().any(|b| b.name == "aitoolplus-test-backup.zip"));
        println!("Cleanup verified!");
    }
}
