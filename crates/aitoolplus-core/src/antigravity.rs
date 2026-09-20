//! Antigravity (Google / Gemini Code Assist) account management.
//!
//! Provides account authentication, token refresh, quota monitoring,
//! and seamless credential switching for Antigravity.
//!
//! STRICT CONSTRAINT: Zero reverse proxy, zero proxy server, zero proxy pool.
//! Pure client-side account management, quota inspection, and credential switching.

use std::path::{Path, PathBuf};
use std::time::Duration;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tracing::{info, warn};

// Google OAuth configuration
pub const CLIENT_ID: &str = "1071006060591-tmhssin2h21lcre235vtolojh4g403ep.apps.googleusercontent.com";
pub const CLIENT_SECRET: &str = "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";
pub const TOKEN_URL: &str = "https://oauth2.googleapis.com/token";
pub const USERINFO_URL: &str = "https://www.googleapis.com/oauth2/v2/userinfo";
pub const AUTH_URL: &str = "https://accounts.google.com/o/oauth2/v2/auth";

pub const NATIVE_USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/133.0.0.0 Safari/537.36";

pub const QUOTA_MODELS_ENDPOINTS: [&str; 3] = [
    "https://daily-cloudcode-pa.sandbox.googleapis.com/v1internal:fetchAvailableModels",
    "https://daily-cloudcode-pa.googleapis.com/v1internal:fetchAvailableModels",
    "https://cloudcode-pa.googleapis.com/v1internal:fetchAvailableModels",
];

pub const QUOTA_SUMMARY_ENDPOINTS: [&str; 3] = [
    "https://daily-cloudcode-pa.sandbox.googleapis.com/v1internal:retrieveUserQuotaSummary",
    "https://daily-cloudcode-pa.googleapis.com/v1internal:retrieveUserQuotaSummary",
    "https://cloudcode-pa.googleapis.com/v1internal:retrieveUserQuotaSummary",
];

pub const LOAD_PROJECT_ENDPOINTS: [&str; 3] = [
    "https://daily-cloudcode-pa.sandbox.googleapis.com/v1internal:loadCodeAssist",
    "https://daily-cloudcode-pa.googleapis.com/v1internal:loadCodeAssist",
    "https://cloudcode-pa.googleapis.com/v1internal:loadCodeAssist",
];

/// A single model quota item.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ModelQuotaInfo {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    pub percentage: i32, // 0 - 100
    pub reset_time: String,
    #[serde(default)]
    pub supports_thinking: bool,
    #[serde(default)]
    pub recommended: bool,
}

/// Grouped quota item (5h or weekly bucket).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct QuotaGroupInfo {
    pub display_name: String,
    pub window: String, // "5h" or "weekly"
    pub remaining_fraction: f64, // 0.0 - 1.0
    pub reset_time: String,
}

/// Overall quota data for an Antigravity account.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AntigravityQuota {
    pub models: Vec<ModelQuotaInfo>,
    #[serde(default)]
    pub quota_groups: Vec<QuotaGroupInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub window_5h: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub window_weekly: Option<f64>,
    pub last_updated: i64,
    #[serde(default)]
    pub is_forbidden: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub forbidden_reason: Option<String>,
}

impl Default for AntigravityQuota {
    fn default() -> Self {
        Self {
            models: Vec::new(),
            quota_groups: Vec::new(),
            window_5h: None,
            window_weekly: None,
            last_updated: Utc::now().timestamp(),
            is_forbidden: false,
            forbidden_reason: None,
        }
    }
}

/// An Antigravity / Google account.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AntigravityAccount {
    pub id: String,
    pub email: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub picture: Option<String>,
    pub access_token: String,
    pub refresh_token: String,
    pub expiry_timestamp: i64, // in seconds
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tier: Option<String>, // "FREE", "PRO", "ULTRA", "ENTERPRISE"
    pub is_active: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quota: Option<AntigravityQuota>,
    pub created_at: i64,
    pub last_used: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub custom_label: Option<String>,
}

impl AntigravityAccount {
    pub fn new(id: String, email: String, access_token: String, refresh_token: String, expiry_timestamp: i64) -> Self {
        let now = Utc::now().timestamp();
        Self {
            id,
            email,
            name: None,
            picture: None,
            access_token,
            refresh_token,
            expiry_timestamp,
            project_id: None,
            tier: None,
            is_active: false,
            quota: None,
            created_at: now,
            last_used: now,
            custom_label: None,
        }
    }

    /// Check if token is expired or will expire within 5 minutes.
    pub fn is_token_expiring(&self) -> bool {
        let now = Utc::now().timestamp();
        self.expiry_timestamp <= now + 300
    }
}

/// Persistent store of Antigravity accounts.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct AntigravityStore {
    pub accounts: Vec<AntigravityAccount>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active_account_id: Option<String>,
}

impl AntigravityStore {
    pub fn get_account(&self, id: &str) -> Option<&AntigravityAccount> {
        self.accounts.iter().find(|a| a.id == id)
    }

    pub fn get_account_mut(&mut self, id: &str) -> Option<&mut AntigravityAccount> {
        self.accounts.iter_mut().find(|a| a.id == id)
    }

    pub fn get_active_account(&self) -> Option<&AntigravityAccount> {
        self.accounts.iter().find(|a| a.is_active)
    }

    pub fn remove_account(&mut self, id: &str) -> bool {
        let initial_len = self.accounts.len();
        self.accounts.retain(|a| a.id != id);
        if self.active_account_id.as_deref() == Some(id) {
            self.active_account_id = self.accounts.first().map(|a| a.id.clone());
        }
        self.accounts.len() < initial_len
    }
}

// ---------------------------------------------------------------------------
// Network / API Helpers
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: String,
    expires_in: i64,
    #[serde(default)]
    refresh_token: Option<String>,
    #[serde(default)]
    #[allow(dead_code)]
    id_token: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct GoogleUserInfo {
    pub email: String,
    pub name: Option<String>,
    pub picture: Option<String>,
}

#[derive(Debug, Deserialize)]
struct QuotaModelResponse {
    #[serde(default)]
    models: std::collections::HashMap<String, ModelInfoJson>,
}

#[derive(Debug, Deserialize)]
struct ModelInfoJson {
    #[serde(rename = "quotaInfo")]
    quota_info: Option<QuotaInfoJson>,
    #[serde(rename = "displayName")]
    display_name: Option<String>,
    #[serde(rename = "supportsThinking")]
    supports_thinking: Option<bool>,
    recommended: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct QuotaInfoJson {
    #[serde(rename = "remainingFraction")]
    remaining_fraction: Option<f64>,
    #[serde(rename = "resetTime")]
    reset_time: Option<String>,
}

#[derive(Debug, Deserialize)]
struct QuotaSummaryResponse {
    #[serde(default)]
    groups: Vec<QuotaSummaryGroupJson>,
}

#[derive(Debug, Deserialize)]
struct QuotaSummaryGroupJson {
    #[serde(rename = "displayName")]
    display_name: Option<String>,
    #[serde(default)]
    buckets: Vec<QuotaSummaryBucketJson>,
}

#[derive(Debug, Deserialize)]
struct QuotaSummaryBucketJson {
    window: Option<String>,
    #[serde(rename = "remainingFraction")]
    remaining_fraction: Option<f64>,
    #[serde(rename = "resetTime")]
    reset_time: Option<String>,
    #[serde(rename = "displayName")]
    display_name: Option<String>,
}

#[derive(Debug, Deserialize)]
struct LoadProjectResponse {
    #[serde(rename = "cloudaicompanionProject")]
    project_id: Option<String>,
    #[serde(rename = "currentTier")]
    current_tier: Option<TierJson>,
    #[serde(rename = "paidTier")]
    paid_tier: Option<TierJson>,
    #[serde(rename = "allowedTiers")]
    allowed_tiers: Option<Vec<TierJson>>,
}

#[derive(Debug, Deserialize)]
struct TierJson {
    id: Option<String>,
    name: Option<String>,
}

/// Refresh an access token using a refresh token.
pub fn refresh_access_token(refresh_token: &str) -> Result<(String, i64), String> {
    info!("Refreshing Antigravity access token...");
    let resp = ureq::post(TOKEN_URL)
        .set("User-Agent", NATIVE_USER_AGENT)
        .timeout(Duration::from_secs(15))
        .send_form(&[
            ("client_id", CLIENT_ID),
            ("client_secret", CLIENT_SECRET),
            ("refresh_token", refresh_token),
            ("grant_type", "refresh_token"),
        ])
        .map_err(|e| format!("Token refresh request failed: {}", e))?;

    let token_res: TokenResponse = resp
        .into_json()
        .map_err(|e| format!("Failed to parse token refresh response: {}", e))?;

    Ok((token_res.access_token, token_res.expires_in))
}

/// Exchange an authorization code for an access token and refresh token.
pub fn exchange_auth_code(code: &str, redirect_uri: &str) -> Result<(String, String, i64), String> {
    info!("Exchanging authorization code for Antigravity tokens...");
    let resp = ureq::post(TOKEN_URL)
        .set("User-Agent", NATIVE_USER_AGENT)
        .timeout(Duration::from_secs(15))
        .send_form(&[
            ("client_id", CLIENT_ID),
            ("client_secret", CLIENT_SECRET),
            ("code", code),
            ("redirect_uri", redirect_uri),
            ("grant_type", "authorization_code"),
        ])
        .map_err(|e| format!("Authorization code exchange failed: {}", e))?;

    let token_res: TokenResponse = resp
        .into_json()
        .map_err(|e| format!("Failed to parse token response: {}", e))?;

    let refresh_token = token_res
        .refresh_token
        .ok_or_else(|| "Google did not return a refresh_token. Make sure prompt=consent is used.".to_string())?;

    Ok((token_res.access_token, refresh_token, token_res.expires_in))
}

/// Fetch Google user profile info.
pub fn fetch_user_info(access_token: &str) -> Result<GoogleUserInfo, String> {
    let resp = ureq::get(USERINFO_URL)
        .set("Authorization", &format!("Bearer {}", access_token))
        .set("User-Agent", NATIVE_USER_AGENT)
        .timeout(Duration::from_secs(10))
        .call()
        .map_err(|e| format!("User info request failed: {}", e))?;

    let user_info: GoogleUserInfo = resp
        .into_json()
        .map_err(|e| format!("Failed to parse user info: {}", e))?;

    Ok(user_info)
}

/// Normalize raw tier string into "FREE", "PRO", "ULTRA", "ENTERPRISE".
pub fn normalize_subscription_tier(tier: &str) -> String {
    let lower = tier.trim().to_lowercase();
    if lower.contains("ultra") || lower.contains("helium") {
        "ULTRA".to_string()
    } else if lower.contains("free") || lower.contains("starter") {
        "FREE".to_string()
    } else if lower.contains("pro") || lower.contains("premium") || lower.contains("advanced") {
        "PRO".to_string()
    } else if lower.contains("enterprise") || lower.contains("corp") {
        "ENTERPRISE".to_string()
    } else {
        "FREE".to_string()
    }
}

/// Fetch project ID and subscription tier from loadCodeAssist API.
pub fn fetch_project_and_tier(access_token: &str) -> (Option<String>, Option<String>) {
    let body = serde_json::json!({
        "metadata": {
            "ideType": "ANTIGRAVITY"
        }
    });

    for endpoint in LOAD_PROJECT_ENDPOINTS {
        let resp = ureq::post(endpoint)
            .set("Authorization", &format!("Bearer {}", access_token))
            .set("Content-Type", "application/json")
            .set("User-Agent", NATIVE_USER_AGENT)
            .timeout(Duration::from_secs(10))
            .send_json(&body);

        if let Ok(res) = resp {
            if let Ok(data) = res.into_json::<LoadProjectResponse>() {
                let project_id = data.project_id;
                let raw_tier = data
                    .paid_tier
                    .as_ref()
                    .and_then(|t| t.id.clone().or_else(|| t.name.clone()))
                    .or_else(|| {
                        data.current_tier
                            .as_ref()
                            .and_then(|t| t.id.clone().or_else(|| t.name.clone()))
                    })
                    .or_else(|| {
                        data.allowed_tiers.as_ref().and_then(|allowed| {
                            allowed
                                .iter()
                                .find(|t| t.id.as_deref() == Some("free-tier"))
                                .and_then(|t| t.id.clone().or_else(|| t.name.clone()))
                        })
                    })
                    .unwrap_or_else(|| "free-tier".to_string());

                let tier = Some(normalize_subscription_tier(&raw_tier));
                return (project_id, tier);
            }
        }
    }

    (None, Some("FREE".to_string()))
}

/// Fetch detailed model quota and summary groups.
pub fn fetch_quota(access_token: &str) -> Result<AntigravityQuota, String> {
    let mut quota = AntigravityQuota::default();
    let body = serde_json::json!({});

    // 1. Fetch available models
    let mut models_fetched = false;
    for endpoint in QUOTA_MODELS_ENDPOINTS {
        let resp = ureq::post(endpoint)
            .set("Authorization", &format!("Bearer {}", access_token))
            .set("Content-Type", "application/json")
            .set("User-Agent", NATIVE_USER_AGENT)
            .timeout(Duration::from_secs(10))
            .send_json(&body);

        match resp {
            Ok(res) => {
                if let Ok(data) = res.into_json::<QuotaModelResponse>() {
                    let mut models = Vec::new();
                    for (name, info) in data.models {
                        if let Some(q_info) = info.quota_info {
                            let fraction = q_info.remaining_fraction.unwrap_or(1.0);
                            let percentage = (fraction * 100.0).round() as i32;
                            let reset_time = q_info.reset_time.unwrap_or_default();
                            models.push(ModelQuotaInfo {
                                name,
                                display_name: info.display_name,
                                percentage,
                                reset_time,
                                supports_thinking: info.supports_thinking.unwrap_or(false),
                                recommended: info.recommended.unwrap_or(false),
                            });
                        }
                    }
                    // Sort models: priority to Gemini 2.5 Pro, Flash, Claude
                    models.sort_by(|a, b| {
                        let score = |m: &ModelQuotaInfo| -> i32 {
                            let n = m.name.to_lowercase();
                            if n.contains("gemini-2.5-pro") {
                                100
                            } else if n.contains("gemini-2.5-flash") || n.contains("gemini-3.6-flash") {
                                90
                            } else if n.contains("claude-3-5-sonnet") || n.contains("claude-3-7-sonnet") || n.contains("claude-sonnet") {
                                80
                            } else if n.contains("gemini") {
                                50
                            } else if n.contains("claude") {
                                40
                            } else {
                                10
                            }
                        };
                        score(b).cmp(&score(a)).then_with(|| a.name.cmp(&b.name))
                    });

                    quota.models = models;
                    models_fetched = true;
                    break;
                }
            }
            Err(ureq::Error::Status(403, resp)) => {
                quota.is_forbidden = true;
                quota.forbidden_reason = resp.into_string().ok();
                return Ok(quota);
            }
            Err(_) => continue,
        }
    }

    // 2. Fetch quota summary (5h and weekly buckets)
    for endpoint in QUOTA_SUMMARY_ENDPOINTS {
        let resp = ureq::post(endpoint)
            .set("Authorization", &format!("Bearer {}", access_token))
            .set("Content-Type", "application/json")
            .set("User-Agent", NATIVE_USER_AGENT)
            .timeout(Duration::from_secs(10))
            .send_json(&body);

        if let Ok(res) = resp {
            if let Ok(data) = res.into_json::<QuotaSummaryResponse>() {
                for group in data.groups {
                    let group_name = group.display_name.unwrap_or_else(|| "General".to_string());
                    for bucket in group.buckets {
                        let window = bucket.window.unwrap_or_default();
                        let fraction = bucket.remaining_fraction.unwrap_or(1.0);
                        let reset_time = bucket.reset_time.unwrap_or_default();
                        let display_name = bucket.display_name.unwrap_or_else(|| group_name.clone());

                        if window == "5h" && quota.window_5h.is_none() {
                            quota.window_5h = Some(fraction);
                        } else if window == "weekly" && quota.window_weekly.is_none() {
                            quota.window_weekly = Some(fraction);
                        }

                        quota.quota_groups.push(QuotaGroupInfo {
                            display_name,
                            window,
                            remaining_fraction: fraction,
                            reset_time,
                        });
                    }
                }
                break;
            }
        }
    }

    if !models_fetched && quota.models.is_empty() {
        warn!("Failed to fetch models from all endpoints");
    }

    quota.last_updated = Utc::now().timestamp();
    Ok(quota)
}

// ---------------------------------------------------------------------------
// Credential Switcher & System Keyring Integration
// ---------------------------------------------------------------------------

#[cfg(target_os = "windows")]
mod win_cred {
    use std::os::windows::ffi::OsStrExt;
    use std::ptr;

    #[repr(C)]
    struct FILETIME {
        dw_low_date_time: u32,
        dw_high_date_time: u32,
    }

    #[repr(C)]
    struct CREDENTIALW {
        flags: u32,
        cred_type: u32,
        target_name: *const u16,
        comment: *const u16,
        last_written: FILETIME,
        credential_blob_size: u32,
        credential_blob: *const u8,
        persist: u32,
        attribute_count: u32,
        attributes: *const std::ffi::c_void,
        target_alias: *const u16,
        user_name: *const u16,
    }

    #[repr(C)]
    struct CREDENTIALW_READ {
        flags: u32,
        cred_type: u32,
        target_name: *const u16,
        comment: *const u16,
        last_written: FILETIME,
        credential_blob_size: u32,
        credential_blob: *mut u8,
        persist: u32,
        attribute_count: u32,
        attributes: *const std::ffi::c_void,
        target_alias: *const u16,
        user_name: *const u16,
    }

    #[link(name = "advapi32")]
    unsafe extern "system" {
        fn CredWriteW(credential: *const CREDENTIALW, flags: u32) -> i32;
        fn CredDeleteW(target_name: *const u16, type_: u32, flags: u32) -> i32;
        fn CredReadW(
            target_name: *const u16,
            type_: u32,
            flags: u32,
            credential: *mut *mut CREDENTIALW_READ,
        ) -> i32;
        fn CredFree(buffer: *mut std::ffi::c_void);
    }

    pub fn write_keyring(payload_json: &str) -> Result<(), String> {
        let target = "gemini:antigravity";
        let user = "antigravity";
        let secret = payload_json.as_bytes();

        let target_wide: Vec<u16> = std::ffi::OsStr::new(target)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();

        let user_wide: Vec<u16> = std::ffi::OsStr::new(user)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();

        let cred = CREDENTIALW {
            flags: 0,
            cred_type: 1, // CRED_TYPE_GENERIC
            target_name: target_wide.as_ptr(),
            comment: ptr::null(),
            last_written: FILETIME {
                dw_low_date_time: 0,
                dw_high_date_time: 0,
            },
            credential_blob_size: secret.len() as u32,
            credential_blob: secret.as_ptr(),
            persist: 2, // CRED_PERSIST_LOCAL_MACHINE
            attribute_count: 0,
            attributes: ptr::null(),
            target_alias: ptr::null(),
            user_name: user_wide.as_ptr(),
        };

        unsafe {
            let _ = CredDeleteW(target_wide.as_ptr(), 1, 0);
            let res = CredWriteW(&cred, 0);
            if res == 0 {
                let err = std::io::Error::last_os_error();
                return Err(format!("Windows CredWriteW failed: {}", err));
            }
        }
        Ok(())
    }

    pub fn read_keyring() -> Result<String, String> {
        let target = "gemini:antigravity";
        let target_wide: Vec<u16> = std::ffi::OsStr::new(target)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();

        let mut cred_ptr: *mut CREDENTIALW_READ = ptr::null_mut();
        unsafe {
            let res = CredReadW(target_wide.as_ptr(), 1, 0, &mut cred_ptr);
            if res == 0 || cred_ptr.is_null() {
                return Err("No credential found in Windows Credential Manager".to_string());
            }

            let cred = &*cred_ptr;
            let blob = std::slice::from_raw_parts(
                cred.credential_blob,
                cred.credential_blob_size as usize,
            );
            let payload_str = String::from_utf8_lossy(blob).to_string();
            CredFree(cred_ptr as *mut std::ffi::c_void);
            Ok(payload_str)
        }
    }
}

/// Write credentials to system keyring (Windows Credential Manager / macOS Keychain / Linux Secret Service).
pub fn write_to_system_keyring(account: &AntigravityAccount) -> Result<(), String> {
    let expiry_datetime = DateTime::from_timestamp(account.expiry_timestamp, 0)
        .unwrap_or_else(Utc::now);
    let expiry_str = expiry_datetime.to_rfc3339_opts(chrono::SecondsFormat::Micros, true);

    #[derive(Serialize)]
    struct KeyringTokenDetails {
        access_token: String,
        token_type: String,
        refresh_token: String,
        expiry: String,
    }

    #[derive(Serialize)]
    struct KeyringPayload {
        token: KeyringTokenDetails,
        auth_method: String,
    }

    let payload = KeyringPayload {
        token: KeyringTokenDetails {
            access_token: account.access_token.clone(),
            token_type: "Bearer".to_string(),
            refresh_token: account.refresh_token.clone(),
            expiry: expiry_str,
        },
        auth_method: "consumer".to_string(),
    };

    let payload_json = serde_json::to_string(&payload)
        .map_err(|e| format!("Failed to serialize keyring payload: {}", e))?;

    #[cfg(target_os = "windows")]
    {
        win_cred::write_keyring(&payload_json)?;
    }

    #[cfg(target_os = "macos")]
    {
        use std::process::Command;
        use base64::{engine::general_purpose::STANDARD, Engine as _};
        let encoded_payload = STANDARD.encode(&payload_json);
        let full_keyring_value = format!("go-keyring-base64:{}", encoded_payload);
        let _ = Command::new("security")
            .args(["delete-generic-password", "-s", "gemini", "-a", "antigravity"])
            .output();
        let _ = Command::new("security")
            .args(["add-generic-password", "-s", "gemini", "-a", "antigravity", "-w", &full_keyring_value, "-A"])
            .output();
    }

    #[cfg(target_os = "linux")]
    {
        use std::process::Command;
        use std::io::Write;
        let mut cmd = Command::new("secret-tool");
        cmd.args(["store", "--label=gemini", "service", "gemini", "username", "antigravity"]);
        cmd.stdin(std::process::Stdio::piped());
        if let Ok(mut child) = cmd.spawn() {
            if let Some(mut stdin) = child.stdin.take() {
                let _ = stdin.write_all(payload_json.as_bytes());
            }
            let _ = child.wait();
        }
    }

    Ok(())
}

/// Read refresh token from system keyring.
pub fn read_from_system_keyring() -> Result<String, String> {
    #[cfg(target_os = "windows")]
    {
        let payload_str = win_cred::read_keyring()?;
        extract_refresh_token_from_payload(&payload_str)
    }

    #[cfg(target_os = "macos")]
    {
        use std::process::Command;
        use base64::{engine::general_purpose::STANDARD, Engine as _};
        let output = Command::new("security")
            .args(["find-generic-password", "-s", "gemini", "-a", "antigravity", "-w"])
            .output()
            .map_err(|e| format!("Security command failed: {}", e))?;
        if !output.status.success() {
            return Err("No credential in macOS Keychain".to_string());
        }
        let secret_str = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let payload_str = if secret_str.starts_with("go-keyring-base64:") {
            let b64_part = &secret_str["go-keyring-base64:".len()..];
            let decoded = STANDARD.decode(b64_part).map_err(|e| e.to_string())?;
            String::from_utf8(decoded).map_err(|e| e.to_string())?
        } else {
            secret_str
        };
        extract_refresh_token_from_payload(&payload_str)
    }

    #[cfg(target_os = "linux")]
    {
        use std::process::Command;
        let output = Command::new("secret-tool")
            .args(["lookup", "service", "gemini", "username", "antigravity"])
            .output()
            .map_err(|e| format!("secret-tool failed: {}", e))?;
        if !output.status.success() {
            return Err("No credential in Linux secret-tool".to_string());
        }
        let payload_str = String::from_utf8_lossy(&output.stdout).trim().to_string();
        extract_refresh_token_from_payload(&payload_str)
    }

    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    {
        Err("Keyring not supported on this operating system".to_string())
    }
}

fn extract_refresh_token_from_payload(payload_str: &str) -> Result<String, String> {
    let json: serde_json::Value = serde_json::from_str(payload_str)
        .map_err(|e| format!("Failed to parse keyring JSON: {}", e))?;

    json.get("token")
        .and_then(|t| t.get("refresh_token"))
        .and_then(|v| v.as_str())
        .or_else(|| json.get("refresh_token").and_then(|v| v.as_str()))
        .map(|s| s.to_string())
        .ok_or_else(|| "Refresh token not found in keyring payload".to_string())
}

/// Write file credentials: `~/.gemini/oauth_creds.json` and `~/.gemini/google_accounts.json`.
pub fn write_file_credentials(home_dir: &Path, account: &AntigravityAccount) -> Result<(), String> {
    let gemini_dir = home_dir.join(".gemini");
    if !gemini_dir.exists() {
        std::fs::create_dir_all(&gemini_dir)
            .map_err(|e| format!("Failed to create .gemini directory: {}", e))?;
    }

    let expiry_ms = if account.expiry_timestamp > 10_000_000_000 {
        account.expiry_timestamp
    } else {
        account.expiry_timestamp * 1000
    };

    #[derive(Serialize)]
    struct OAuthCredsFile {
        access_token: String,
        refresh_token: String,
        token_type: String,
        expiry_date: i64,
        scope: String,
    }

    let creds = OAuthCredsFile {
        access_token: account.access_token.clone(),
        refresh_token: account.refresh_token.clone(),
        token_type: "Bearer".to_string(),
        expiry_date: expiry_ms,
        scope: "https://www.googleapis.com/auth/userinfo.email openid https://www.googleapis.com/auth/cloud-platform https://www.googleapis.com/auth/userinfo.profile".to_string(),
    };

    let creds_path = gemini_dir.join("oauth_creds.json");
    let json_str = serde_json::to_string_pretty(&creds)
        .map_err(|e| format!("Failed to serialize oauth_creds: {}", e))?;
    std::fs::write(&creds_path, json_str)
        .map_err(|e| format!("Failed to write oauth_creds.json: {}", e))?;

    #[derive(Serialize)]
    struct GoogleAccountsFile {
        active: String,
        old: Vec<String>,
    }

    let accounts_info = GoogleAccountsFile {
        active: account.email.clone(),
        old: vec![],
    };

    let accounts_path = gemini_dir.join("google_accounts.json");
    if let Ok(accounts_str) = serde_json::to_string_pretty(&accounts_info) {
        let _ = std::fs::write(&accounts_path, accounts_str);
    }

    info!("Synced credentials to ~/.gemini/oauth_creds.json for: {}", account.email);
    Ok(())
}

/// Read refresh token from `~/.gemini/oauth_creds.json`.
pub fn read_file_credentials(home_dir: &Path) -> Result<String, String> {
    let creds_path = home_dir.join(".gemini").join("oauth_creds.json");
    if !creds_path.exists() {
        return Err(format!("File not found: {}", creds_path.display()));
    }

    let content = std::fs::read_to_string(&creds_path)
        .map_err(|e| format!("Failed to read oauth_creds.json: {}", e))?;

    let json: serde_json::Value = serde_json::from_str(&content)
        .map_err(|e| format!("Failed to parse oauth_creds.json: {}", e))?;

    json.get("refresh_token")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| "Refresh token not found in oauth_creds.json".to_string())
}

// ---------------------------------------------------------------------------
// Store & Account Lifecycle
// ---------------------------------------------------------------------------

pub fn store_path(app_data: &Path) -> PathBuf {
    app_data.join("antigravity_accounts.json")
}

pub fn load_store(app_data: &Path) -> AntigravityStore {
    let path = store_path(app_data);
    if !path.exists() {
        return AntigravityStore::default();
    }
    match std::fs::read_to_string(&path) {
        Ok(s) => serde_json::from_str(&s).unwrap_or_default(),
        Err(_) => AntigravityStore::default(),
    }
}

pub fn save_store(app_data: &Path, store: &AntigravityStore) -> Result<(), String> {
    let path = store_path(app_data);
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let s = serde_json::to_string_pretty(store)
        .map_err(|e| format!("Failed to serialize antigravity store: {}", e))?;
    std::fs::write(&path, s)
        .map_err(|e| format!("Failed to write antigravity store: {}", e))?;
    Ok(())
}

/// Ensure that the account's token is fresh, refreshing if needed.
pub fn ensure_fresh_token(account: &mut AntigravityAccount) -> Result<(), String> {
    if account.is_token_expiring() {
        info!("Token expiring for {}, refreshing...", account.email);
        let (new_access, expires_in) = refresh_access_token(&account.refresh_token)?;
        account.access_token = new_access;
        account.expiry_timestamp = Utc::now().timestamp() + expires_in;
    }
    Ok(())
}

/// Create or update an account from a refresh token.
/// Refreshes token, queries user info, project & tier, and quota.
pub fn build_account_from_refresh_token(
    refresh_token: &str,
    existing_id: Option<String>,
    label: Option<String>,
) -> Result<AntigravityAccount, String> {
    let (access_token, expires_in) = refresh_access_token(refresh_token)?;
    let expiry_timestamp = Utc::now().timestamp() + expires_in;

    let user_info = fetch_user_info(&access_token)?;
    let (project_id, tier) = fetch_project_and_tier(&access_token);
    let quota = fetch_quota(&access_token).ok();

    let id = existing_id.unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    let mut account = AntigravityAccount::new(id, user_info.email, access_token, refresh_token.to_string(), expiry_timestamp);
    account.name = user_info.name;
    account.picture = user_info.picture;
    account.project_id = project_id;
    account.tier = tier;
    account.quota = quota;
    account.custom_label = label;

    Ok(account)
}

/// Import active account from local machine (Windows Credential Manager or ~/.gemini/oauth_creds.json).
pub fn import_from_local_system(home_dir: &Path) -> Result<AntigravityAccount, String> {
    // 1. Try keyring
    let refresh_token = read_from_system_keyring().or_else(|_| {
        // 2. Try ~/.gemini/oauth_creds.json
        read_file_credentials(home_dir)
    })?;

    let mut account = build_account_from_refresh_token(&refresh_token, None, Some("本机导入".to_string()))?;
    account.is_active = true;
    Ok(account)
}

/// Import all accounts from Antigravity Manager (~/.antigravity_tools).
pub fn import_from_antigravity_manager(home_dir: &Path) -> Result<Vec<AntigravityAccount>, String> {
    let base = home_dir.join(".antigravity_tools");
    let index_file = base.join("accounts.json");
    if !index_file.exists() {
        return Err("未找到 Antigravity Manager 数据目录 (~/.antigravity_tools/accounts.json)".into());
    }
    let content = std::fs::read_to_string(&index_file).map_err(|e| e.to_string())?;
    let index: serde_json::Value = serde_json::from_str(&content).map_err(|e| e.to_string())?;
    let current_id = index.get("current_account_id").and_then(|v| v.as_str());

    let mut accounts = Vec::new();
    let accounts_arr = index.get("accounts").and_then(|v| v.as_array()).ok_or("accounts 列表格式错误")?;
    for a in accounts_arr {
        let Some(acc_id) = a.get("id").and_then(|v| v.as_str()) else { continue };
        let detail_path = base.join("accounts").join(format!("{acc_id}.json"));
        if !detail_path.exists() { continue; }
        let detail_raw = match std::fs::read_to_string(&detail_path) {
            Ok(c) => c,
            Err(_) => continue,
        };
        let detail: serde_json::Value = match serde_json::from_str(&detail_raw) {
            Ok(v) => v,
            Err(_) => continue,
        };
        let email = detail.get("email").and_then(|v| v.as_str()).unwrap_or("").to_string();
        if email.is_empty() { continue; }
        let name = detail.get("name").and_then(|v| v.as_str()).map(String::from);
        let token_obj = detail.get("token");
        let access_token = token_obj.and_then(|t| t.get("access_token")).and_then(|v| v.as_str()).unwrap_or("").to_string();
        let refresh_token = token_obj.and_then(|t| t.get("refresh_token")).and_then(|v| v.as_str()).unwrap_or("").to_string();
        if refresh_token.is_empty() { continue; }
        let expiry = token_obj.and_then(|t| t.get("expiry_timestamp")).and_then(|v| v.as_i64()).unwrap_or_else(|| Utc::now().timestamp() + 3600);
        let project_id = token_obj.and_then(|t| t.get("project_id")).and_then(|v| v.as_str()).map(String::from);

        let mut acc = AntigravityAccount::new(
            acc_id.to_string(),
            email,
            access_token,
            refresh_token,
            expiry,
        );
        acc.name = name;
        acc.project_id = project_id;
        acc.is_active = current_id == Some(acc_id);
        acc.custom_label = Some("从 Antigravity Manager 导入".to_string());

        // Parse quota if present
        if let Some(quota_val) = detail.get("quota") {
            let mut quota = AntigravityQuota::default();
            if let Some(models) = quota_val.get("models").and_then(|v| v.as_array()) {
                for m in models {
                    if let Some(name) = m.get("name").and_then(|v| v.as_str()) {
                        let pct = m.get("percentage").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
                        let display_name = m.get("display_name").and_then(|v| v.as_str()).map(String::from);
                        let reset_time = m.get("reset_time").and_then(|v| v.as_str()).unwrap_or("").to_string();
                        let supports_thinking = m.get("supports_thinking").and_then(|v| v.as_bool()).unwrap_or(false);
                        let recommended = m.get("recommended").and_then(|v| v.as_bool()).unwrap_or(false);
                        quota.models.push(ModelQuotaInfo {
                            name: name.to_string(),
                            display_name,
                            percentage: pct,
                            reset_time,
                            supports_thinking,
                            recommended,
                        });
                    }
                }
            }
            if let Some(w5) = quota_val.get("window_5h").and_then(|v| v.as_f64()) {
                quota.window_5h = Some(w5);
            }
            if let Some(ww) = quota_val.get("window_weekly").and_then(|v| v.as_f64()) {
                quota.window_weekly = Some(ww);
            }
            acc.quota = Some(quota);
        }

        accounts.push(acc);
    }
    Ok(accounts)
}

/// Switch active account: updates Windows Credential Manager, ~/.gemini/oauth_creds.json, and store.
pub fn switch_account(
    home_dir: &Path,
    app_data: &Path,
    store: &mut AntigravityStore,
    account_id: &str,
) -> Result<(), String> {
    let account = store.get_account_mut(account_id)
        .ok_or_else(|| format!("Account {} not found", account_id))?;

    // Ensure valid token before switching
    ensure_fresh_token(account)?;

    // 1. Write to system keyring
    write_to_system_keyring(account)?;

    // 2. Write to ~/.gemini files
    write_file_credentials(home_dir, account)?;

    // 3. Update store state
    for acc in &mut store.accounts {
        acc.is_active = acc.id == account_id;
        if acc.is_active {
            acc.last_used = Utc::now().timestamp();
        }
    }
    store.active_account_id = Some(account_id.to_string());

    save_store(app_data, store)?;
    info!("Successfully switched active Antigravity account to {}", account_id);
    Ok(())
}

// ---------------------------------------------------------------------------
// Ephemeral OAuth Loopback Server
// ---------------------------------------------------------------------------

pub struct OAuthServerSession {
    pub auth_url: String,
    pub state: String,
    pub port: u16,
    listener: std::net::TcpListener,
}

impl OAuthServerSession {
    pub fn start() -> Result<Self, String> {
        let listener = std::net::TcpListener::bind("127.0.0.1:0")
            .map_err(|e| format!("Failed to bind local loopback server: {}", e))?;
        let port = listener.local_addr().map_err(|e| e.to_string())?.port();
        listener.set_nonblocking(false).map_err(|e| e.to_string())?;

        let state = uuid::Uuid::new_v4().to_string();
        let redirect_uri = format!("http://127.0.0.1:{}/oauth-callback", port);

        let scopes = [
            "openid",
            "https://www.googleapis.com/auth/cloud-platform",
            "https://www.googleapis.com/auth/userinfo.email",
            "https://www.googleapis.com/auth/userinfo.profile",
            "https://www.googleapis.com/auth/cclog",
            "https://www.googleapis.com/auth/experimentsandconfigs",
        ].join(" ");

        let params = [
            ("client_id", CLIENT_ID),
            ("redirect_uri", &redirect_uri),
            ("response_type", "code"),
            ("scope", &scopes),
            ("access_type", "offline"),
            ("prompt", "consent"),
            ("include_granted_scopes", "true"),
            ("state", &state),
        ];

        let auth_url = url::Url::parse_with_params(AUTH_URL, &params)
            .map_err(|e| format!("Failed to build auth URL: {}", e))?
            .to_string();

        Ok(Self {
            auth_url,
            state,
            port,
            listener,
        })
    }

    /// Wait synchronously for the browser callback (with a timeout).
    pub fn wait_for_code(self, timeout: Duration) -> Result<String, String> {
        use std::io::{Read, Write};

        let listener = self.listener;
        // In Windows, set_read_timeout on accepted stream or timeout loop
        let start = std::time::Instant::now();
        listener.set_nonblocking(true).map_err(|e| e.to_string())?;

        loop {
            if start.elapsed() > timeout {
                return Err("OAuth authorization timed out. Please try again.".to_string());
            }

            match listener.accept() {
                Ok((mut stream, _)) => {
                    let mut buffer = [0u8; 4096];
                    let bytes_read = stream.read(&mut buffer).unwrap_or(0);
                    let request = String::from_utf8_lossy(&buffer[..bytes_read]);

                    let query_params = request
                        .lines()
                        .next()
                        .and_then(|line| {
                            let parts: Vec<&str> = line.split_whitespace().collect();
                            if parts.len() >= 2 { Some(parts[1]) } else { None }
                        })
                        .and_then(|path| {
                            url::Url::parse(&format!("http://127.0.0.1{}", path)).ok()
                        })
                        .map(|url| {
                            let mut code = None;
                            let mut state = None;
                            for (k, v) in url.query_pairs() {
                                if k == "code" {
                                    code = Some(v.to_string());
                                } else if k == "state" {
                                    state = Some(v.to_string());
                                }
                            }
                            (code, state)
                        });

                    let (code, rec_state) = match query_params {
                        Some((c, s)) => (c, s),
                        None => (None, None),
                    };

                    let success_html = "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\n\r\n\
                        <html>\
                        <body style='font-family: -apple-system, BlinkMacSystemFont, \"Segoe UI\", Roboto, sans-serif; text-align: center; padding: 60px; background: #0f172a; color: #f8fafc;'>\
                        <h1 style='color: #10b981;'>授权成功！</h1>\
                        <p style='font-size: 16px; color: #94a3b8;'>Google 账号已成功授权，你可以关闭本页面返回 AI ToolPlus。</p>\
                        <script>setTimeout(function() { window.close(); }, 3000);</script>\
                        </body>\
                        </html>";

                    let fail_html = "HTTP/1.1 400 Bad Request\r\nContent-Type: text/html; charset=utf-8\r\n\r\n\
                        <html>\
                        <body style='font-family: -apple-system, BlinkMacSystemFont, \"Segoe UI\", Roboto, sans-serif; text-align: center; padding: 60px; background: #0f172a; color: #f8fafc;'>\
                        <h1 style='color: #ef4444;'>授权失败</h1>\
                        <p style='font-size: 16px; color: #94a3b8;'>状态校验不匹配或未获取到授权码，请返回重试。</p>\
                        </body>\
                        </html>";

                    if let Some(code) = code {
                        if rec_state.as_deref() == Some(&self.state) {
                            let _ = stream.write_all(success_html.as_bytes());
                            let _ = stream.flush();
                            return Ok(code);
                        } else {
                            let _ = stream.write_all(fail_html.as_bytes());
                            let _ = stream.flush();
                            return Err("State mismatch (CSRF protection)".to_string());
                        }
                    } else {
                        let _ = stream.write_all(fail_html.as_bytes());
                        let _ = stream.flush();
                        return Err("No authorization code in callback query".to_string());
                    }
                }
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(100));
                }
                Err(e) => {
                    return Err(format!("Listener accept error: {}", e));
                }
            }
        }
    }
}

/// Open an URL in the default web browser.
pub fn open_browser(url: &str) {
    #[cfg(target_os = "windows")]
    {
        let _ = std::process::Command::new("cmd")
            .args(["/c", "start", "", url])
            .spawn();
    }
    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new("open").arg(url).spawn();
    }
    #[cfg(target_os = "linux")]
    {
        let _ = std::process::Command::new("xdg-open").arg(url).spawn();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tier_normalization() {
        assert_eq!(normalize_subscription_tier("free-tier"), "FREE");
        assert_eq!(normalize_subscription_tier("Antigravity Starter Quota"), "FREE");
        assert_eq!(normalize_subscription_tier("g1-pro-tier"), "PRO");
        assert_eq!(normalize_subscription_tier("Google AI Pro"), "PRO");
        assert_eq!(normalize_subscription_tier("g1-ultra-tier"), "ULTRA");
        assert_eq!(normalize_subscription_tier("Google AI Ultra"), "ULTRA");
        assert_eq!(normalize_subscription_tier("helium"), "ULTRA");
        assert_eq!(normalize_subscription_tier("enterprise-tier"), "ENTERPRISE");
    }

    #[test]
    fn test_account_is_token_expiring() {
        let now = Utc::now().timestamp();
        let mut acc = AntigravityAccount::new(
            "1".into(),
            "test@example.com".into(),
            "token".into(),
            "refresh".into(),
            now + 100, // Expires in 100s (< 300s)
        );
        assert!(acc.is_token_expiring());

        acc.expiry_timestamp = now + 3600; // Expires in 1 hour (> 300s)
        assert!(!acc.is_token_expiring());
    }

    #[test]
    fn test_store_serialization_roundtrip() {
        let temp = tempfile::tempdir().unwrap();
        let mut store = AntigravityStore::default();
        let acc = AntigravityAccount::new(
            "acc-1".into(),
            "alice@example.com".into(),
            "access".into(),
            "refresh".into(),
            Utc::now().timestamp() + 3600,
        );
        store.accounts.push(acc);
        store.active_account_id = Some("acc-1".into());

        save_store(temp.path(), &store).unwrap();
        let loaded = load_store(temp.path());
        assert_eq!(loaded.accounts.len(), 1);
        assert_eq!(loaded.accounts[0].email, "alice@example.com");
        assert_eq!(loaded.active_account_id.as_deref(), Some("acc-1"));
    }

    #[test]
    fn test_extract_refresh_token_from_payload() {
        let payload1 = r#"{"token":{"access_token":"at","refresh_token":"rt-123","token_type":"Bearer","expiry":"2026-09-19T20:00:00Z"},"auth_method":"consumer"}"#;
        assert_eq!(extract_refresh_token_from_payload(payload1).unwrap(), "rt-123");

        let payload2 = r#"{"refresh_token":"rt-456"}"#;
        assert_eq!(extract_refresh_token_from_payload(payload2).unwrap(), "rt-456");

        let payload3 = r#"{"other":"none"}"#;
        assert!(extract_refresh_token_from_payload(payload3).is_err());
    }

    #[test]
    fn test_file_credentials_roundtrip() {
        let temp = tempfile::tempdir().unwrap();
        let acc = AntigravityAccount::new(
            "acc-1".into(),
            "bob@example.com".into(),
            "access-123".into(),
            "refresh-xyz".into(),
            Utc::now().timestamp() + 3600,
        );

        write_file_credentials(temp.path(), &acc).unwrap();
        let read_rt = read_file_credentials(temp.path()).unwrap();
        assert_eq!(read_rt, "refresh-xyz");

        // Verify google_accounts.json was also written
        let accounts_json_path = temp.path().join(".gemini").join("google_accounts.json");
        assert!(accounts_json_path.exists());
        let content = std::fs::read_to_string(accounts_json_path).unwrap();
        assert!(content.contains("bob@example.com"));
    }

    #[test]
    fn test_store_account_management() {
        let mut store = AntigravityStore::default();
        let acc1 = AntigravityAccount::new("id-1".into(), "user1@gmail.com".into(), "a1".into(), "r1".into(), 1000);
        let mut acc2 = AntigravityAccount::new("id-2".into(), "user2@gmail.com".into(), "a2".into(), "r2".into(), 1000);
        acc2.is_active = true;

        store.accounts.push(acc1);
        store.accounts.push(acc2);
        store.active_account_id = Some("id-2".into());

        assert_eq!(store.get_active_account().unwrap().id, "id-2");
        assert_eq!(store.get_account("id-1").unwrap().email, "user1@gmail.com");

        assert!(store.remove_account("id-2"));
        assert_eq!(store.accounts.len(), 1);
        assert_eq!(store.active_account_id.as_deref(), Some("id-1"));
    }
}
