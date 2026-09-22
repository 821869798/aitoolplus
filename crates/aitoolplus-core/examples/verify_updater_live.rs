use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;

use sha2::{Digest, Sha256};

use aitoolplus_core::updater::{
    best_asset, check_latest_at, download_with_progress, is_scoop_install_path,
    verify_asset_sha256, UpdateMirror,
};

fn main() {
    println!("============================================================");
    println!("  AIToolPlus In-App Updater Live E2E Verification");
    println!("============================================================");

    // 1. Locate or create actual installer payload
    let dist_installer = Path::new("target/dist/aitoolplus-setup.exe");
    let payload_bytes = if dist_installer.is_file() {
        println!("[SETUP] Reading official release installer from target/dist/aitoolplus-setup.exe...");
        std::fs::read(dist_installer).expect("read installer")
    } else {
        println!("[SETUP] target/dist/aitoolplus-setup.exe not found, generating 3 MB dummy payload...");
        vec![0x42u8; 3 * 1024 * 1024]
    };

    let total_installer_size = payload_bytes.len();
    let mut hasher = Sha256::new();
    hasher.update(&payload_bytes);
    let expected_sha256 = format!("{:x}", hasher.finalize());

    println!(
        "[SETUP] Installer payload size: {} bytes ({:.2} MB)",
        total_installer_size,
        total_installer_size as f64 / (1024.0 * 1024.0)
    );
    println!("[SETUP] Expected SHA-256: {}", expected_sha256);

    // 2. Start local HTTP mock file server
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind listener");
    let server_addr = listener.local_addr().expect("local addr");
    println!("[SERVER] Started local HTTP file server on http://{}", server_addr);

    let running = Arc::new(AtomicBool::new(true));
    let running_clone = running.clone();
    let payload_for_server = Arc::new(payload_bytes.clone());

    let server_handle = thread::spawn(move || {
        listener.set_nonblocking(true).unwrap();
        while running_clone.load(Ordering::Relaxed) {
            match listener.accept() {
                Ok((mut stream, _)) => {
                    let mut reader = BufReader::new(stream.try_clone().unwrap());
                    let mut request_line = String::new();
                    let _ = reader.read_line(&mut request_line);

                    // Drain remaining headers
                    loop {
                        let mut line = String::new();
                        if reader.read_line(&mut line).unwrap_or(0) == 0 || line == "\r\n" || line.is_empty() {
                            break;
                        }
                    }

                    if request_line.contains("/api/releases/latest") {
                        let body = serde_json::json!({
                            "tag_name": "v0.2.0",
                            "html_url": "https://github.com/821869798/aitoolplus/releases/tag/v0.2.0",
                            "body": "## AIToolPlus v0.2.0\n- In-app auto updater\n- High speed CDN mirrors\n- Bug fixes and UI refinements",
                            "published_at": "2026-09-22T20:00:00Z",
                            "assets": [
                                {
                                    "name": "aitoolplus-setup.exe",
                                    "browser_download_url": format!("http://{server_addr}/downloads/aitoolplus-setup.exe"),
                                    "size": payload_for_server.len() as u64
                                }
                            ]
                        }).to_string();

                        let resp = format!(
                            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                            body.len()
                        );
                        let _ = stream.write_all(resp.as_bytes());
                    } else if request_line.contains("/downloads/aitoolplus-setup.exe") {
                        let header = format!(
                            "HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                            payload_for_server.len()
                        );
                        let _ = stream.write_all(header.as_bytes());
                        // Send in 64 KB chunks to simulate real streaming and allow EMA speed calculation
                        for chunk in payload_for_server.chunks(64 * 1024) {
                            let _ = stream.write_all(chunk);
                            thread::sleep(std::time::Duration::from_millis(5));
                        }
                    } else if request_line.contains("/404") {
                        let resp = "HTTP/1.1 404 Not Found\r\nContent-Length: 23\r\nConnection: close\r\n\r\n{\"message\":\"Not Found\"}";
                        let _ = stream.write_all(resp.as_bytes());
                    } else {
                        let resp = "HTTP/1.1 400 Bad Request\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
                        let _ = stream.write_all(resp.as_bytes());
                    }
                }
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(std::time::Duration::from_millis(10));
                }
                Err(e) => {
                    eprintln!("[SERVER ERROR] accept error: {e}");
                    break;
                }
            }
        }
    });

    // 3. Test 1: Check for update from current v0.1.0 -> v0.2.0
    println!("\n[TEST 1] Checking latest release from server (current = v0.1.0)...");
    let api_url = format!("http://{server_addr}/api/releases/latest");
    let info = check_latest_at(&api_url, "0.1.0").expect("check_latest_at failed");
    assert!(info.update_available, "Expected update to be available");
    assert_eq!(info.current_version, "0.1.0");
    assert_eq!(info.latest_version, "0.2.0");
    assert_eq!(info.assets.len(), 1);
    println!("  -> Update Available: {}", info.update_available);
    println!("  -> Current Version:  v{}", info.current_version);
    println!("  -> Latest Version:   v{}", info.latest_version);
    println!("  -> Release Notes:    \n{}", info.release_notes.lines().map(|l| format!("     | {l}")).collect::<Vec<_>>().join("\n"));
    println!("[PASS] Test 1: Detected new release v0.2.0 successfully!");

    // 4. Test 2: Check when current version already equals latest (v0.2.0)
    println!("\n[TEST 2] Checking latest release when already on v0.2.0...");
    let info_up_to_date = check_latest_at(&api_url, "0.2.0").expect("check_latest_at up to date");
    assert!(!info_up_to_date.update_available, "Expected no update when version is equal");
    println!("  -> Update Available: {}", info_up_to_date.update_available);
    println!("[PASS] Test 2: Correctly reported already up to date!");

    // 5. Test 3: Check 404 response (repo with no releases yet)
    println!("\n[TEST 3] Checking behavior when server returns HTTP 404 (no releases yet)...");
    let not_found_url = format!("http://{server_addr}/404");
    let info_404 = check_latest_at(&not_found_url, "0.1.0").expect("check 404 must be handled gracefully");
    assert!(!info_404.update_available);
    println!("  -> HTTP 404 handled gracefully, update_available = {}", info_404.update_available);
    println!("[PASS] Test 3: HTTP 404 cleanly converted to 'up-to-date' without crashing!");

    // 6. Test 4: Download installer with live progress tracking
    println!("\n[TEST 4] Downloading installer from local file server with progress tracking...");
    let asset = best_asset(&info).expect("asset found");
    let temp_dir = tempfile::tempdir().expect("tempdir");
    let download_target = temp_dir.path().join(&asset.name);

    let mut last_percentage: i32 = -1;
    let downloaded_path = download_with_progress(
        &asset.download_url,
        asset.size,
        &download_target,
        |prog| {
            let pct = prog.percentage.round() as i32;
            if pct % 20 == 0 && pct != last_percentage {
                last_percentage = pct;
                println!(
                    "  [PROGRESS] {:.1}% ({:.2} MB / {:.2} MB) - Speed: {:.2} MB/s",
                    prog.percentage,
                    prog.downloaded as f64 / (1024.0 * 1024.0),
                    prog.total as f64 / (1024.0 * 1024.0),
                    prog.speed_bps as f64 / (1024.0 * 1024.0)
                );
            }
            true
        },
    ).expect("download_with_progress failed");

    assert_eq!(downloaded_path, download_target);
    assert!(download_target.is_file(), "Downloaded file must exist on disk");
    let downloaded_bytes = std::fs::read(&download_target).expect("read downloaded file");
    assert_eq!(
        downloaded_bytes.len(),
        total_installer_size,
        "Downloaded file length must exactly match expected size"
    );
    assert_eq!(
        downloaded_bytes,
        payload_bytes,
        "Downloaded bytes must byte-for-byte match source payload"
    );
    println!("[PASS] Test 4: Installer download completed and byte stream matches 100%!");

    // 7. Test 5: Verify SHA-256 Checksum
    println!("\n[TEST 5] Verifying SHA-256 integrity hash...");
    let sha_ok = verify_asset_sha256(&download_target, &expected_sha256).expect("verify sha256");
    assert!(sha_ok, "SHA-256 verification must pass for valid file");
    let sha_bad = verify_asset_sha256(&download_target, "deadbeef1234567890").expect("verify bad sha256");
    assert!(!sha_bad, "SHA-256 verification must fail for mismatched hash");
    println!("  -> Correct hash matches: true");
    println!("  -> Tampered hash rejects: true");
    println!("[PASS] Test 5: SHA-256 verification verified successfully!");

    // 8. Test 6: Verify Mirror Rewriting Rules
    println!("\n[TEST 6] Verifying CDN mirror transformations...");
    let orig = "https://github.com/821869798/aitoolplus/releases/download/v0.2.0/aitoolplus-setup.exe";
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
        UpdateMirror::Custom.apply_url(orig, "https://cdn.mycustom.io/"),
        format!("https://cdn.mycustom.io/{orig}")
    );
    println!("[PASS] Test 6: All mirror URL rewriting rules passed!");

    // 9. Test 7: Verify Scoop Installation Detection
    println!("\n[TEST 7] Verifying Scoop installation detection...");
    assert!(is_scoop_install_path(r"C:\Users\test\scoop\apps\aitoolplus\0.1.0\aitoolplus.exe", &[]));
    assert!(!is_scoop_install_path(r"C:\Users\test\AppData\Local\Programs\AIToolPlus\aitoolplus.exe", &[]));
    println!("[PASS] Test 7: Scoop installation detection verified!");

    // 10. Clean shutdown
    running.store(false, Ordering::Relaxed);
    let _ = server_handle.join();

    println!("\n============================================================");
    println!("  ALL 7 LIVE IN-APP UPDATER TESTS PASSED SUCCESSFULLY!       ");
    println!("============================================================");
}
