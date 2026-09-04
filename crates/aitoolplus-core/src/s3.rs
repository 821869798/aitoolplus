//! S3-compatible remote backup transport with AWS Signature Version 4 (SigV4).
//!
//! Compatible with AWS S3, Cloudflare R2, MinIO, Ceph, Aliyun OSS, Tencent COS, etc.
//! Supports path-style and virtual-host style endpoints, automatic SigV4 signing,
//! connection testing, upload, list, download, delete, and remote pruning.

use std::fmt::Write as FmtWrite;
use std::fs::{self, File};
use std::path::{Path, PathBuf};

use hmac::{Hmac, Mac};
use sha2::{Digest, Sha256};
use url::Url;

use crate::security::unprotect_secret;
use crate::settings::S3Config;
pub use crate::webdav::RemoteBackup;

type HmacSha256 = Hmac<Sha256>;

fn sha256_hex(data: &[u8]) -> String {
    let hash = Sha256::digest(data);
    let mut hex = String::with_capacity(64);
    for b in hash {
        let _ = write!(hex, "{:02x}", b);
    }
    hex
}

fn hmac_sha256(key: &[u8], data: &[u8]) -> Vec<u8> {
    let mut mac = HmacSha256::new_from_slice(key).expect("HMAC supports any key size");
    mac.update(data);
    mac.finalize().into_bytes().to_vec()
}

fn derive_signing_key(secret: &str, date: &str, region: &str, service: &str) -> Vec<u8> {
    let k_secret = format!("AWS4{secret}");
    let k_date = hmac_sha256(k_secret.as_bytes(), date.as_bytes());
    let k_region = hmac_sha256(&k_date, region.as_bytes());
    let k_service = hmac_sha256(&k_region, service.as_bytes());
    hmac_sha256(&k_service, b"aws4_request")
}

pub struct S3UrlParts {
    pub url: String,
    pub host: String,
    pub canonical_uri: String,
}

pub fn resolve_s3_url(config: &S3Config, key: Option<&str>) -> Result<S3UrlParts, String> {
    let raw_endpoint = config.endpoint.trim().trim_end_matches('/');
    if !(raw_endpoint.starts_with("http://") || raw_endpoint.starts_with("https://")) {
        return Err("S3 endpoint must start with http:// or https://".into());
    }
    let bucket = config.bucket.trim();
    if bucket.is_empty() {
        return Err("S3 bucket name cannot be empty".into());
    }
    let parsed = Url::parse(raw_endpoint).map_err(|e| format!("invalid endpoint URL: {e}"))?;
    let scheme = parsed.scheme();
    let original_host = parsed
        .host_str()
        .ok_or_else(|| "missing host in S3 endpoint".to_string())?;
    let port = parsed.port();
    let base_path = parsed.path().trim_end_matches('/');

    let full_key = match (config.prefix.trim().trim_matches('/'), key) {
        ("", Some(k)) => k.to_string(),
        (p, Some(k)) => format!("{p}/{k}"),
        (p, None) => p.to_string(),
    };

    if config.path_style {
        let host = if let Some(p) = port {
            format!("{original_host}:{p}")
        } else {
            original_host.to_string()
        };
        let canonical_uri = if full_key.is_empty() {
            format!("{base_path}/{bucket}")
        } else {
            format!("{base_path}/{bucket}/{full_key}")
        };
        let canonical_uri = if !canonical_uri.starts_with('/') {
            format!("/{canonical_uri}")
        } else {
            canonical_uri
        };
        let url = format!("{scheme}://{host}{canonical_uri}");
        Ok(S3UrlParts {
            url,
            host,
            canonical_uri,
        })
    } else {
        let host_prefix = format!("{bucket}.{original_host}");
        let host = if let Some(p) = port {
            format!("{host_prefix}:{p}")
        } else {
            host_prefix
        };
        let canonical_uri = if full_key.is_empty() {
            if base_path.is_empty() {
                "/".to_string()
            } else {
                base_path.to_string()
            }
        } else {
            format!("{base_path}/{full_key}")
        };
        let canonical_uri = if !canonical_uri.starts_with('/') {
            format!("/{canonical_uri}")
        } else {
            canonical_uri
        };
        let url = format!("{scheme}://{host}{canonical_uri}");
        Ok(S3UrlParts {
            url,
            host,
            canonical_uri,
        })
    }
}

/// Generate SigV4 authorization headers.
pub fn sign_request(
    config: &S3Config,
    method: &str,
    canonical_uri: &str,
    query_string: &str,
    host: &str,
    payload: &[u8],
    now: chrono::DateTime<chrono::Utc>,
) -> (String, String, String) {
    let date_stamp = now.format("%Y%m%d").to_string();
    let amz_date = now.format("%Y%m%dT%H%M%SZ").to_string();
    let payload_hash = sha256_hex(payload);

    let region = if config.region.trim().is_empty() {
        "us-east-1"
    } else {
        config.region.trim()
    };

    let canonical_headers =
        format!("host:{host}\nx-amz-content-sha256:{payload_hash}\nx-amz-date:{amz_date}\n");
    let signed_headers = "host;x-amz-content-sha256;x-amz-date";

    let canonical_request = format!(
        "{method}\n{canonical_uri}\n{query_string}\n{canonical_headers}\n{signed_headers}\n{payload_hash}"
    );
    let canonical_hash = sha256_hex(canonical_request.as_bytes());

    let credential_scope = format!("{date_stamp}/{region}/s3/aws4_request");
    let string_to_sign =
        format!("AWS4-HMAC-SHA256\n{amz_date}\n{credential_scope}\n{canonical_hash}");

    let secret = unprotect_secret(&config.secret_access_key);
    let signing_key = derive_signing_key(&secret, &date_stamp, region, "s3");
    let signature = {
        let sig_bytes = hmac_sha256(&signing_key, string_to_sign.as_bytes());
        let mut hex = String::with_capacity(64);
        for b in sig_bytes {
            let _ = write!(hex, "{:02x}", b);
        }
        hex
    };

    let access_key = config.access_key_id.trim();
    let auth_header = format!(
        "AWS4-HMAC-SHA256 Credential={access_key}/{credential_scope}, SignedHeaders={signed_headers}, Signature={signature}"
    );

    (auth_header, amz_date, payload_hash)
}

fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout(std::time::Duration::from_secs(30))
        .build()
}

fn safe_filename(value: &str) -> bool {
    !value.is_empty()
        && value.ends_with(".zip")
        && !value.contains('/')
        && !value.contains('\\')
        && !value.contains("..")
}

/// Test S3 credentials and bucket access with ListObjectsV2 (max-keys=1).
pub fn test_connection(config: &S3Config) -> Result<(), String> {
    let parts = resolve_s3_url(config, None)?;
    let query = "list-type=2&max-keys=1";
    let now = chrono::Utc::now();
    let (auth, amz_date, payload_hash) = sign_request(
        config,
        "GET",
        &parts.canonical_uri,
        query,
        &parts.host,
        b"",
        now,
    );

    let full_url = format!("{}?{query}", parts.url);
    let response = agent()
        .get(&full_url)
        .set("Host", &parts.host)
        .set("Authorization", &auth)
        .set("x-amz-date", &amz_date)
        .set("x-amz-content-sha256", &payload_hash)
        .call()
        .map_err(|e| format!("S3 connection failed: {e}"))?;

    if (200..300).contains(&response.status()) {
        Ok(())
    } else {
        Err(format!("S3 returned HTTP {}", response.status()))
    }
}

/// Upload a backup archive to S3.
pub fn upload(config: &S3Config, path: &Path) -> Result<RemoteBackup, String> {
    let filename = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| "invalid backup filename".to_string())?;
    if !safe_filename(filename) {
        return Err("unsafe S3 backup filename".into());
    }

    let bytes = fs::read(path).map_err(|e| format!("cannot read backup file: {e}"))?;
    let parts = resolve_s3_url(config, Some(filename))?;
    let now = chrono::Utc::now();
    let (auth, amz_date, payload_hash) = sign_request(
        config,
        "PUT",
        &parts.canonical_uri,
        "",
        &parts.host,
        &bytes,
        now,
    );

    let response = agent()
        .put(&parts.url)
        .set("Host", &parts.host)
        .set("Authorization", &auth)
        .set("x-amz-date", &amz_date)
        .set("x-amz-content-sha256", &payload_hash)
        .set("Content-Type", "application/zip")
        .send_bytes(&bytes)
        .map_err(|e| format!("S3 upload failed: {e}"))?;

    if (200..300).contains(&response.status()) {
        Ok(RemoteBackup {
            name: filename.to_string(),
            href: parts.url,
        })
    } else {
        Err(format!("S3 upload returned HTTP {}", response.status()))
    }
}

/// List available backup archives in S3 prefix.
pub fn list(config: &S3Config) -> Result<Vec<RemoteBackup>, String> {
    let parts = resolve_s3_url(config, None)?;
    let prefix = config.prefix.trim().trim_matches('/');
    let query = if prefix.is_empty() {
        "list-type=2".to_string()
    } else {
        format!("list-type=2&prefix={prefix}")
    };

    let now = chrono::Utc::now();
    let (auth, amz_date, payload_hash) = sign_request(
        config,
        "GET",
        &parts.canonical_uri,
        &query,
        &parts.host,
        b"",
        now,
    );

    let full_url = format!("{}?{query}", parts.url);
    let response = agent()
        .get(&full_url)
        .set("Host", &parts.host)
        .set("Authorization", &auth)
        .set("x-amz-date", &amz_date)
        .set("x-amz-content-sha256", &payload_hash)
        .call()
        .map_err(|e| format!("S3 list failed: {e}"))?;

    let body = response
        .into_string()
        .map_err(|e| format!("cannot read S3 list response: {e}"))?;

    let mut backups = parse_s3_list_keys(&body);
    backups.sort_by(|a, b| b.name.cmp(&a.name));
    Ok(backups)
}

/// Parse S3 ListBucketResult XML and extract .zip keys.
pub fn parse_s3_list_keys(xml: &str) -> Vec<RemoteBackup> {
    let mut results = vec![];
    let mut remaining = xml;
    while let Some(start) = remaining.find("<Key>") {
        let after = &remaining[start + 5..];
        if let Some(end) = after.find("</Key>") {
            let key = &after[..end];
            if key.ends_with(".zip") {
                let filename = key.rsplit('/').next().unwrap_or(key).to_string();
                results.push(RemoteBackup {
                    name: filename,
                    href: key.to_string(),
                });
            }
            remaining = &after[end + 6..];
        } else {
            break;
        }
    }
    results
}

/// Download a backup archive from S3.
pub fn download(config: &S3Config, filename: &str, destination: &Path) -> Result<PathBuf, String> {
    if !safe_filename(filename) {
        return Err("unsafe S3 backup filename".into());
    }
    let parts = resolve_s3_url(config, Some(filename))?;
    let now = chrono::Utc::now();
    let (auth, amz_date, payload_hash) = sign_request(
        config,
        "GET",
        &parts.canonical_uri,
        "",
        &parts.host,
        b"",
        now,
    );

    let response = agent()
        .get(&parts.url)
        .set("Host", &parts.host)
        .set("Authorization", &auth)
        .set("x-amz-date", &amz_date)
        .set("x-amz-content-sha256", &payload_hash)
        .call()
        .map_err(|e| format!("S3 download failed: {e}"))?;

    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let temp = destination.with_extension("download.tmp");
    let mut input = response.into_reader();
    let mut output = File::create(&temp).map_err(|e| e.to_string())?;
    std::io::copy(&mut input, &mut output).map_err(|e| e.to_string())?;
    output.sync_all().map_err(|e| e.to_string())?;
    fs::rename(&temp, destination).map_err(|e| e.to_string())?;
    Ok(destination.to_path_buf())
}

/// Delete a backup archive from S3.
pub fn delete(config: &S3Config, filename: &str) -> Result<(), String> {
    if !safe_filename(filename) {
        return Err("unsafe S3 backup filename".into());
    }
    let parts = resolve_s3_url(config, Some(filename))?;
    let now = chrono::Utc::now();
    let (auth, amz_date, payload_hash) = sign_request(
        config,
        "DELETE",
        &parts.canonical_uri,
        "",
        &parts.host,
        b"",
        now,
    );

    let response = agent()
        .delete(&parts.url)
        .set("Host", &parts.host)
        .set("Authorization", &auth)
        .set("x-amz-date", &amz_date)
        .set("x-amz-content-sha256", &payload_hash)
        .call()
        .map_err(|e| format!("S3 delete failed: {e}"))?;

    if (200..300).contains(&response.status()) || response.status() == 204 {
        Ok(())
    } else {
        Err(format!("S3 delete returned HTTP {}", response.status()))
    }
}

/// Prune old remote backups beyond `keep` count.
pub fn prune_remote_backups(config: &S3Config, keep: u32) -> Result<(), String> {
    if keep == 0 {
        return Ok(());
    }
    let backups = list(config)?;
    for backup in backups.into_iter().skip(keep as usize) {
        delete(config, &backup.name)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader, Read, Write};
    use std::net::TcpListener;
    use std::sync::{Arc, Mutex};

    #[test]
    fn sigv4_derivation_and_signature() {
        let config = S3Config {
            endpoint: "https://s3.amazonaws.com".into(),
            region: "us-east-1".into(),
            bucket: "mybucket".into(),
            access_key_id: "AKIAIOSFODNN7EXAMPLE".into(),
            secret_access_key: "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY".into(),
            prefix: "aitoolplus".into(),
            path_style: true,
        };
        let now = chrono::DateTime::parse_from_rfc3339("2026-09-04T12:00:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc);
        let (auth, amz_date, payload_hash) = sign_request(
            &config,
            "GET",
            "/mybucket/aitoolplus/backup.zip",
            "",
            "s3.amazonaws.com",
            b"",
            now,
        );
        assert_eq!(amz_date, "20260904T120000Z");
        assert_eq!(
            payload_hash,
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert!(auth.contains("AWS4-HMAC-SHA256"));
        assert!(
            auth.contains("Credential=AKIAIOSFODNN7EXAMPLE/20260904/us-east-1/s3/aws4_request")
        );
        assert!(auth.contains("SignedHeaders=host;x-amz-content-sha256;x-amz-date"));
        assert!(auth.contains("Signature="));
    }

    #[test]
    fn parse_s3_list_keys_extracts_zip_archives() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <ListBucketResult xmlns="http://s3.amazonaws.com/doc/2006-03-01/">
            <Name>mybucket</Name>
            <Prefix>aitoolplus</Prefix>
            <Contents>
                <Key>aitoolplus/aitoolplus-auto-20260901.zip</Key>
                <Size>1024</Size>
            </Contents>
            <Contents>
                <Key>aitoolplus/not-a-zip.txt</Key>
                <Size>50</Size>
            </Contents>
            <Contents>
                <Key>aitoolplus/aitoolplus-auto-20260902.zip</Key>
                <Size>2048</Size>
            </Contents>
        </ListBucketResult>"#;
        let keys = parse_s3_list_keys(xml);
        assert_eq!(keys.len(), 2);
        assert_eq!(keys[0].name, "aitoolplus-auto-20260901.zip");
        assert_eq!(keys[1].name, "aitoolplus-auto-20260902.zip");
    }

    #[test]
    fn end_to_end_against_mock_s3() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let stored_data = Arc::new(Mutex::new(
            std::collections::HashMap::<String, Vec<u8>>::new(),
        ));
        let stored_for_server = stored_data.clone();

        let server = std::thread::spawn(move || {
            for _ in 0..5 {
                let (mut stream, _) = match listener.accept() {
                    Ok(pair) => pair,
                    Err(_) => break,
                };
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut request_line = String::new();
                if reader.read_line(&mut request_line).unwrap() == 0 {
                    continue;
                }
                let mut content_length = 0usize;
                loop {
                    let mut line = String::new();
                    reader.read_line(&mut line).unwrap();
                    if line == "\r\n" || line.is_empty() {
                        break;
                    }
                    if line.to_ascii_lowercase().starts_with("content-length:") {
                        let val = line.split(':').nth(1).unwrap().trim();
                        content_length = val.parse().unwrap_or(0);
                    }
                }
                let mut body = vec![0u8; content_length];
                if content_length > 0 {
                    reader.read_exact(&mut body).unwrap();
                }

                let parts: Vec<&str> = request_line.split_whitespace().collect();
                let method = parts[0];
                let path_and_query = parts[1];

                if method == "PUT" {
                    let key = path_and_query.split('?').next().unwrap().to_string();
                    stored_for_server.lock().unwrap().insert(key, body);
                    let resp = "HTTP/1.1 200 OK\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
                    stream.write_all(resp.as_bytes()).unwrap();
                } else if method == "GET" && path_and_query.contains("list-type=2") {
                    let guard = stored_for_server.lock().unwrap();
                    let mut xml = String::from("<ListBucketResult>");
                    for key in guard.keys() {
                        xml.push_str(&format!("<Contents><Key>{key}</Key></Contents>"));
                    }
                    xml.push_str("</ListBucketResult>");
                    let resp = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: application/xml\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{xml}",
                        xml.len()
                    );
                    stream.write_all(resp.as_bytes()).unwrap();
                } else if method == "GET" {
                    let key = path_and_query.split('?').next().unwrap().to_string();
                    let guard = stored_for_server.lock().unwrap();
                    if let Some(content) = guard.get(&key) {
                        let resp = format!(
                            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                            content.len()
                        );
                        stream.write_all(resp.as_bytes()).unwrap();
                        stream.write_all(content).unwrap();
                    } else {
                        let resp = "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
                        stream.write_all(resp.as_bytes()).unwrap();
                    }
                } else if method == "DELETE" {
                    let key = path_and_query.split('?').next().unwrap().to_string();
                    stored_for_server.lock().unwrap().remove(&key);
                    let resp =
                        "HTTP/1.1 204 No Content\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
                    stream.write_all(resp.as_bytes()).unwrap();
                }
            }
        });

        let config = S3Config {
            endpoint: format!("http://{addr}"),
            region: "us-east-1".into(),
            bucket: "test-bucket".into(),
            access_key_id: "test-key".into(),
            secret_access_key: "test-secret".into(),
            prefix: "aitoolplus".into(),
            path_style: true,
        };

        // 1. Test connection
        assert!(test_connection(&config).is_ok());

        // 2. Upload file
        let temp_dir = tempfile::tempdir().unwrap();
        let test_file = temp_dir.path().join("backup-1.zip");
        fs::write(&test_file, b"PK0304zipcontent").unwrap();
        let uploaded = upload(&config, &test_file).unwrap();
        assert_eq!(uploaded.name, "backup-1.zip");

        // 3. List
        let list_res = list(&config).unwrap();
        assert_eq!(list_res.len(), 1);
        assert_eq!(list_res[0].name, "backup-1.zip");

        // 4. Download
        let restore_path = temp_dir.path().join("restored.zip");
        let downloaded = download(&config, "backup-1.zip", &restore_path).unwrap();
        assert_eq!(fs::read(downloaded).unwrap(), b"PK0304zipcontent");

        // 5. Delete
        assert!(delete(&config, "backup-1.zip").is_ok());

        server.join().unwrap();
    }
}
