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

pub const NATIVE_USER_AGENT: &str = "vscode/1.96.2 (Antigravity/4.3.0)";

// Quota API endpoints (fallback order: Sandbox → Daily → Prod, matching Antigravity-Manager)
pub const QUOTA_MODELS_ENDPOINTS: [&str; 3] = [
    "https://daily-cloudcode-pa.sandbox.googleapis.com/v1internal:fetchAvailableModels",
    "https://daily-cloudcode-pa.googleapis.com/v1internal:fetchAvailableModels",
    "https://cloudcode-pa.googleapis.com/v1internal:fetchAvailableModels",
];

// Quota Summary API endpoints (weekly + 5h grouped quota, fallback order: Sandbox → Daily → Prod)
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

static AGENT: std::sync::LazyLock<ureq::Agent> = std::sync::LazyLock::new(|| {
    ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(10))
        .build()
});

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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bucket_id: Option<String>,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subscription_tier: Option<String>,
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
            subscription_tier: None,
        }
    }
}

/// Device fingerprint profile for Antigravity isolation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct DeviceProfile {
    pub machine_id: String,
    pub mac_machine_id: String,
    pub dev_device_id: String,
    pub sqm_id: String,
}

impl DeviceProfile {
    pub fn generate_random() -> Self {
        let u1 = uuid::Uuid::new_v4();
        let u2 = uuid::Uuid::new_v4();
        let u3 = uuid::Uuid::new_v4();
        let u4 = uuid::Uuid::new_v4();
        Self {
            machine_id: format!("auth0|user_{}", u1.simple()),
            mac_machine_id: u2.to_string(),
            dev_device_id: u3.to_string(),
            sqm_id: format!("{{{}}}", u4.to_string().to_uppercase()),
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
    #[serde(default)]
    pub disabled: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disabled_reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub device_profile: Option<DeviceProfile>,
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
            disabled: false,
            disabled_reason: None,
            device_profile: None,
        }
    }

    /// Check if token is expired or will expire within 5 minutes.
    pub fn is_token_expiring(&self) -> bool {
        let now = Utc::now().timestamp();
        self.expiry_timestamp <= now + 300
    }

    /// Resolve account subscription tier ("ULTRA", "PRO", "ENTERPRISE", or "FREE").
    pub fn get_tier(&self) -> &str {
        if let Some(t) = &self.tier {
            let s = t.trim();
            if !s.is_empty() {
                return s;
            }
        }
        if let Some(q) = &self.quota {
            if let Some(t) = &q.subscription_tier {
                let s = t.trim();
                if !s.is_empty() {
                    return s;
                }
            }
        }
        "FREE"
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
#[allow(dead_code)]
struct QuotaSummaryBucketJson {
    #[serde(rename = "bucketId")]
    bucket_id: Option<String>,
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
    let resp = AGENT.post(TOKEN_URL)
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
    let resp = AGENT.post(TOKEN_URL)
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
    let resp = AGENT.get(USERINFO_URL)
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
        let resp = AGENT.post(endpoint)
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
/// If `project_id` is provided, it tries with and without project_id.
/// If `existing_quota` is provided and the API is rate-limited or transiently fails,
/// the existing model list and quota groups are preserved as fallback.
pub fn fetch_quota(
    access_token: &str,
    project_id: Option<&str>,
    existing_quota: Option<&AntigravityQuota>,
) -> Result<AntigravityQuota, String> {
    let mut quota = AntigravityQuota::default();
    let mut models_fetched = false;
    let mut last_403_reason = None;

    // 1. Fetch available models
    for endpoint in QUOTA_MODELS_ENDPOINTS {
        let bodies = if let Some(pid) = project_id.filter(|p| !p.trim().is_empty()) {
            vec![
                serde_json::json!({ "project": pid }),
                serde_json::json!({}),
            ]
        } else {
            vec![serde_json::json!({})]
        };

        let mut endpoint_succeeded = false;

        for body in &bodies {
            let resp = AGENT.post(endpoint)
                .set("Authorization", &format!("Bearer {}", access_token))
                .set("Content-Type", "application/json")
                .set("User-Agent", NATIVE_USER_AGENT)
                .timeout(Duration::from_secs(10))
                .send_json(body);

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
                        // Sort models: priority to Gemini 3.x / 2.5 Pro, Flash, Claude
                        models.sort_by(|a, b| {
                            let score = |m: &ModelQuotaInfo| -> i32 {
                                let n = m.name.to_lowercase();
                                if n.contains("gemini-3.1-pro") || n.contains("gemini-3-pro") || n.contains("gemini-2.5-pro") {
                                    100
                                } else if n.contains("gemini-3.8-flash") || n.contains("gemini-3-flash") || n.contains("gemini-2.5-flash") {
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
                        endpoint_succeeded = true;
                        last_403_reason = None;
                        break;
                    }
                }
                Err(ureq::Error::Status(403, resp)) => {
                    // Record reason and continue trying without project_id or next endpoint
                    last_403_reason = resp.into_string().ok();
                    continue;
                }
                Err(_) => continue,
            }
        }

        if endpoint_succeeded {
            break;
        }
    }

    // If all model endpoints failed with 403, mark as forbidden if it is a real TOS violation
    if !models_fetched && last_403_reason.is_some() {
        if let Some(reason) = &last_403_reason {
            if reason.contains("TOS_VIOLATION") || reason.contains("violation of Terms of Service") {
                quota.is_forbidden = true;
                quota.forbidden_reason = last_403_reason;
                return Ok(quota);
            }
        }
    }

    // 2. Fetch quota summary (5h and weekly buckets)
    let summary_bodies = if let Some(pid) = project_id.filter(|p| !p.trim().is_empty()) {
        vec![
            serde_json::json!({ "project": pid }),
            serde_json::json!({}),
        ]
    } else {
        vec![serde_json::json!({})]
    };

    let mut summary_fetched = false;
    for endpoint in QUOTA_SUMMARY_ENDPOINTS {
        for body in &summary_bodies {
            let resp = AGENT.post(endpoint)
                .set("Authorization", &format!("Bearer {}", access_token))
                .set("Content-Type", "application/json")
                .set("User-Agent", NATIVE_USER_AGENT)
                .timeout(Duration::from_secs(10))
                .send_json(body);

            if let Ok(res) = resp {
                if let Ok(data) = res.into_json::<QuotaSummaryResponse>() {
                    for group in &data.groups {
                        let group_name = group.display_name.clone().unwrap_or_else(|| "General".to_string());
                        let is_gemini_group = group_name.to_lowercase().contains("gemini")
                            || (!group_name.to_lowercase().contains("claude")
                                && !group_name.to_lowercase().contains("gpt")
                                && !group_name.to_lowercase().contains("3p"));

                        for bucket in &group.buckets {
                            let window = bucket.window.clone().unwrap_or_default();
                            let fraction = bucket.remaining_fraction.unwrap_or(1.0);
                            let reset_time = bucket.reset_time.clone().unwrap_or_default();
                            let bucket_id = bucket.bucket_id.clone();
                            let display_name = group_name.clone();

                            let win_lower = window.to_lowercase();
                            let bid_lower = bucket_id.as_deref().unwrap_or("").to_lowercase();
                            let is_5h = win_lower.contains("5h") || bid_lower.contains("5h") || win_lower.contains("hour");
                            let is_weekly = win_lower.contains("week") || bid_lower.contains("week") || win_lower.contains("7d") || bid_lower.contains("7d");

                            if is_gemini_group {
                                if is_5h && quota.window_5h.is_none() {
                                    quota.window_5h = Some(fraction);
                                } else if is_weekly && quota.window_weekly.is_none() {
                                    quota.window_weekly = Some(fraction);
                                }
                            }

                            quota.quota_groups.push(QuotaGroupInfo {
                                display_name,
                                window,
                                remaining_fraction: fraction,
                                reset_time,
                                bucket_id,
                            });
                        }
                    }

                    // [FIX #3426 / Antigravity-Manager Parity] Fuse real bucket quotas into models so UI doesn't show fake 100%
                    for model in quota.models.iter_mut() {
                        let name_lower = model.name.to_lowercase();
                        let is_claude_or_gpt = name_lower.starts_with("claude") || name_lower.starts_with("gpt");
                        let is_gemini = name_lower.starts_with("gemini");

                        for group in &data.groups {
                            let gname = group.display_name.as_deref().unwrap_or("").to_lowercase();
                            let matches_group = if is_claude_or_gpt {
                                gname.contains("claude") || gname.contains("gpt") || gname.contains("3p")
                            } else if is_gemini {
                                gname.contains("gemini")
                                    || (!gname.contains("claude") && !gname.contains("gpt") && !gname.contains("3p"))
                            } else {
                                false
                            };

                            if matches_group {
                                let bucket_5h = group.buckets.iter().find(|b| {
                                    let win = b.window.as_deref().unwrap_or("").to_lowercase();
                                    let bid = b.bucket_id.as_deref().unwrap_or("").to_lowercase();
                                    win.contains("5h") || bid.contains("5h") || win.contains("hour") || bid.contains("hour")
                                });
                                let bucket_weekly = group.buckets.iter().find(|b| {
                                    let win = b.window.as_deref().unwrap_or("").to_lowercase();
                                    let bid = b.bucket_id.as_deref().unwrap_or("").to_lowercase();
                                    win.contains("week") || bid.contains("week") || win.contains("7d") || bid.contains("7d")
                                });

                                let chosen_bucket = match (bucket_5h, bucket_weekly) {
                                    (Some(h), Some(w)) => {
                                        let w_frac = w.remaining_fraction.unwrap_or(1.0);
                                        let h_frac = h.remaining_fraction.unwrap_or(1.0);
                                        if w_frac <= 0.001 {
                                            Some(w)
                                        } else if h_frac <= w_frac {
                                            Some(h)
                                        } else {
                                            Some(w)
                                        }
                                    }
                                    (Some(h), None) => Some(h),
                                    (None, Some(w)) => Some(w),
                                    _ => group.buckets.first(),
                                };

                                if let Some(b) = chosen_bucket {
                                    let fraction = b.remaining_fraction.unwrap_or(1.0);
                                    model.percentage = (fraction * 100.0).round() as i32;
                                    if let Some(ref rt) = b.reset_time {
                                        if !rt.is_empty() {
                                            model.reset_time = rt.clone();
                                        }
                                    }
                                    break;
                                }
                            }
                        }
                    }

                    summary_fetched = true;
                    break;
                }
            }
        }
        if summary_fetched {
            break;
        }
    }

    // Fallback: If models or quota_groups could not be fetched (e.g. rate limit 429),
    // retain existing cached quota data instead of wiping them out
    if let Some(existing) = existing_quota {
        if quota.models.is_empty() && !existing.models.is_empty() {
            quota.models = existing.models.clone();
        }
        if quota.quota_groups.is_empty() && !existing.quota_groups.is_empty() {
            quota.quota_groups = existing.quota_groups.clone();
            if quota.window_5h.is_none() {
                quota.window_5h = existing.window_5h;
            }
            if quota.window_weekly.is_none() {
                quota.window_weekly = existing.window_weekly;
            }
        }
        if quota.subscription_tier.is_none() {
            quota.subscription_tier = existing.subscription_tier.clone();
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
    let mut store: AntigravityStore = match std::fs::read_to_string(&path) {
        Ok(s) => serde_json::from_str(&s).unwrap_or_default(),
        Err(_) => AntigravityStore::default(),
    };
    // Auto-heal false 403s caused by previous sandbox domain endpoint bug
    for acc in &mut store.accounts {
        if let Some(quota) = &mut acc.quota {
            if quota.is_forbidden {
                if let Some(reason) = &quota.forbidden_reason {
                    if reason.contains("The caller does not have permission") && !reason.contains("TOS_VIOLATION") {
                        quota.is_forbidden = false;
                        quota.forbidden_reason = None;
                    }
                }
            }
        }
    }
    store
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
    let quota = fetch_quota(&access_token, project_id.as_deref(), None).ok();

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

#[derive(Debug, Clone)]
pub struct ImportedOAuthState {
    pub refresh_token: String,
    pub is_gcp_tos: bool,
    pub project_id: Option<String>,
}

/// Protobuf Varint reader
pub fn read_varint(data: &[u8], offset: usize) -> Result<(u64, usize), String> {
    let mut result = 0u64;
    let mut shift = 0;
    let mut pos = offset;

    loop {
        if pos >= data.len() {
            return Err("incomplete_data".to_string());
        }
        let byte = data[pos];
        result |= ((byte & 0x7F) as u64) << shift;
        pos += 1;
        if byte & 0x80 == 0 {
            break;
        }
        shift += 7;
    }

    Ok((result, pos))
}

/// Skip Protobuf Field
pub fn skip_field(data: &[u8], offset: usize, wire_type: u8) -> Result<usize, String> {
    match wire_type {
        0 => {
            let (_, new_offset) = read_varint(data, offset)?;
            Ok(new_offset)
        }
        1 => Ok(offset + 8),
        2 => {
            let (length, content_offset) = read_varint(data, offset)?;
            Ok(content_offset + length as usize)
        }
        5 => Ok(offset + 4),
        _ => Err(format!("unknown_wire_type: {}", wire_type)),
    }
}

/// Find first instance of specified Protobuf field content (Length-Delimited only)
pub fn find_field(data: &[u8], target_field: u32) -> Result<Option<Vec<u8>>, String> {
    let mut offset = 0;
    while offset < data.len() {
        let (tag, new_offset) = match read_varint(data, offset) {
            Ok(v) => v,
            Err(_) => break,
        };
        let wire_type = (tag & 7) as u8;
        let field_num = (tag >> 3) as u32;

        if field_num == target_field && wire_type == 2 {
            let (length, content_offset) = read_varint(data, new_offset)?;
            let length = length as usize;
            if content_offset + length <= data.len() {
                return Ok(Some(data[content_offset..content_offset + length].to_vec()));
            }
        }
        offset = skip_field(data, new_offset, wire_type)?;
    }
    Ok(None)
}

/// Find all instances of specified Protobuf field content (Length-Delimited only)
pub fn find_all_fields(data: &[u8], target_field: u32) -> Vec<Vec<u8>> {
    let mut results = Vec::new();
    let mut offset = 0;
    while offset < data.len() {
        let (tag, new_offset) = match read_varint(data, offset) {
            Ok(v) => v,
            Err(_) => break,
        };
        let wire_type = (tag & 7) as u8;
        let field_num = (tag >> 3) as u32;

        if field_num == target_field && wire_type == 2 {
            if let Ok((length, content_offset)) = read_varint(data, new_offset) {
                let length = length as usize;
                if content_offset + length <= data.len() {
                    results.push(data[content_offset..content_offset + length].to_vec());
                }
            }
        }
        if let Ok(next) = skip_field(data, new_offset, wire_type) {
            offset = next;
        } else {
            break;
        }
    }
    results
}

/// Find varint field
pub fn find_varint_field(data: &[u8], target_field: u32) -> Result<Option<u64>, String> {
    let mut offset = 0;
    while offset < data.len() {
        let (tag, new_offset) = read_varint(data, offset)?;
        let wire_type = (tag & 7) as u8;
        let field_num = (tag >> 3) as u32;

        if field_num == target_field && wire_type == 0 {
            let (value, _) = read_varint(data, new_offset)?;
            return Ok(Some(value));
        }
        offset = skip_field(data, new_offset, wire_type)?;
    }
    Ok(None)
}

/// Decode a unified state entry payload matching target sentinel key
pub fn decode_unified_state_payload(outer_b64: &str, target_sentinel_key: &str) -> Result<Vec<u8>, String> {
    use base64::{engine::general_purpose, Engine as _};

    let outer_blob = general_purpose::STANDARD
        .decode(outer_b64.trim())
        .map_err(|e| format!("Outer Base64 decoding failed: {}", e))?;

    // Check all data entries in Topic (Field 1)
    let data_entries = find_all_fields(&outer_blob, 1);
    for entry in data_entries {
        if let Ok(Some(key_bytes)) = find_field(&entry, 1) {
            if let Ok(key_str) = std::str::from_utf8(&key_bytes) {
                if key_str == target_sentinel_key {
                    if let Ok(Some(row_blob)) = find_field(&entry, 2) {
                        if let Ok(Some(encoded_payload)) = find_field(&row_blob, 1) {
                            if let Ok(encoded_str) = std::str::from_utf8(&encoded_payload) {
                                if let Ok(decoded) = general_purpose::STANDARD.decode(encoded_str.trim()) {
                                    return Ok(decoded);
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // Fallback: Legacy nested format (Outer F1 -> Inner F1 sentinel key, Inner F2 payload)
    if let Ok(Some(inner_blob)) = find_field(&outer_blob, 1) {
        if let Ok(Some(key_bytes)) = find_field(&inner_blob, 1) {
            if let Ok(key_str) = std::str::from_utf8(&key_bytes) {
                if key_str == target_sentinel_key {
                    if let Ok(Some(payload)) = find_field(&inner_blob, 2) {
                        if let Ok(decoded) = general_purpose::STANDARD.decode(&payload) {
                            return Ok(decoded);
                        }
                        return Ok(payload);
                    }
                }
            }
        }
    }

    Err(format!("Sentinel key '{}' not found in unified state entry", target_sentinel_key))
}

/// Extract enterprise GCP project ID from SQLite database connection if configured
pub fn extract_enterprise_project_id_from_conn(conn: &rusqlite::Connection) -> Result<Option<String>, String> {
    let entry_b64: Option<String> = conn
        .query_row(
            "SELECT value FROM ItemTable WHERE key = ?",
            ["antigravityUnifiedStateSync.enterprisePreferences"],
            |row| row.get(0),
        )
        .ok();

    let Some(entry_b64) = entry_b64 else {
        return Ok(None);
    };

    let payload = match decode_unified_state_payload(&entry_b64, "enterpriseGcpProjectId") {
        Ok(p) => p,
        Err(_) => return Ok(None),
    };

    let Some(project_bytes) = find_field(&payload, 3).map_err(|e| e.to_string())? else {
        return Ok(None);
    };

    let project_id = String::from_utf8(project_bytes)
        .map_err(|_| "enterpriseGcpProjectId is not UTF-8 encoded".to_string())?;
    let trimmed = project_id.trim();
    if trimmed.is_empty() {
        Ok(None)
    } else {
        Ok(Some(trimmed.to_string()))
    }
}

/// Extract OAuth credentials and tokens from a SQLite state database (state.vscdb)
pub fn extract_oauth_state_from_file(db_path: &Path) -> Result<ImportedOAuthState, String> {
    use base64::{engine::general_purpose, Engine as _};

    if !db_path.exists() {
        return Err(format!("Database file not found: {:?}", db_path));
    }

    let conn = rusqlite::Connection::open(db_path)
        .map_err(|e| format!("Failed to open SQLite database {:?}: {}", db_path, e))?;

    // 1. Try new format (antigravityUnifiedStateSync.oauthToken)
    let new_format_data: Option<String> = conn
        .query_row(
            "SELECT value FROM ItemTable WHERE key = ?",
            ["antigravityUnifiedStateSync.oauthToken"],
            |row| row.get(0),
        )
        .ok();

    if let Some(outer_b64) = new_format_data {
        if let Ok(oauth_info_blob) = decode_unified_state_payload(&outer_b64, "oauthTokenInfoSentinelKey") {
            let refresh_bytes = find_field(&oauth_info_blob, 3)?
                .ok_or_else(|| "Refresh Token not found in OAuthInfo (Field 3)".to_string())?;
            let refresh_token = String::from_utf8(refresh_bytes)
                .map_err(|_| "Refresh Token is not valid UTF-8".to_string())?;
            let is_gcp_tos = find_varint_field(&oauth_info_blob, 6)?.unwrap_or(1) != 0;
            let project_id = extract_enterprise_project_id_from_conn(&conn)?;

            return Ok(ImportedOAuthState {
                refresh_token,
                is_gcp_tos,
                project_id,
            });
        }
    }

    // 2. Try old format (jetskiStateSync.agentManagerInitState)
    let old_data: Option<String> = conn
        .query_row(
            "SELECT value FROM ItemTable WHERE key = ?",
            ["jetskiStateSync.agentManagerInitState"],
            |row| row.get(0),
        )
        .ok();

    if let Some(current_data) = old_data {
        let blob = general_purpose::STANDARD
            .decode(&current_data)
            .map_err(|e| format!("Base64 decoding failed: {}", e))?;

        let oauth_data = find_field(&blob, 6)?
            .ok_or_else(|| "OAuth data not found in Field 6".to_string())?;

        let refresh_bytes = find_field(&oauth_data, 3)?
            .ok_or_else(|| "Refresh Token not found in Field 3".to_string())?;

        let refresh_token = String::from_utf8(refresh_bytes)
            .map_err(|_| "Refresh Token is not valid UTF-8".to_string())?;

        return Ok(ImportedOAuthState {
            refresh_token,
            is_gcp_tos: true,
            project_id: extract_enterprise_project_id_from_conn(&conn)?,
        });
    }

    Err("Login credentials not found in database".to_string())
}

/// Get all candidate state database paths for Antigravity IDE and Antigravity
pub fn get_all_candidate_db_paths(target_ide: Option<&str>) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    let folder_names: &[&str] = if target_ide == Some("ide") {
        &["Antigravity IDE", "Antigravity"]
    } else if target_ide == Some("code") {
        &["Antigravity", "Antigravity IDE"]
    } else {
        &["Antigravity IDE", "Antigravity"]
    };

    #[cfg(target_os = "windows")]
    {
        if let Ok(appdata) = std::env::var("APPDATA") {
            for folder_name in folder_names {
                paths.push(
                    PathBuf::from(&appdata)
                        .join(folder_name)
                        .join("User")
                        .join("globalStorage")
                        .join("state.vscdb"),
                );
            }
        }
    }

    #[cfg(target_os = "macos")]
    {
        if let Some(home) = dirs::home_dir() {
            for folder_name in folder_names {
                paths.push(home.join(format!(
                    "Library/Application Support/{}/User/globalStorage/state.vscdb",
                    folder_name
                )));
            }
        }
    }

    #[cfg(target_os = "linux")]
    {
        if let Some(home) = crate::paths::home_dir() {
            for folder_name in folder_names {
                paths.push(home.join(format!(
                    ".config/{}/User/globalStorage/state.vscdb",
                    folder_name
                )));
            }
        }
    }

    paths
}

/// Extract refresh token from database file
pub fn extract_refresh_token_from_file(db_path: &Path) -> Result<String, String> {
    extract_oauth_state_from_file(db_path).map(|s| s.refresh_token)
}

/// Get active refresh token from local system (Keyring or candidate DBs)
pub fn get_refresh_token_from_db(target_ide: Option<&str>) -> Result<String, String> {
    for db_path in get_all_candidate_db_paths(target_ide) {
        if db_path.exists() {
            if let Ok(token) = extract_refresh_token_from_file(&db_path) {
                if !token.is_empty() {
                    return Ok(token);
                }
            }
        }
    }
    Err("Login state data not found in any database path".to_string())
}

/// Parse and extract refresh tokens from arbitrary text (supports JSON arrays, objects, or raw token streams)
pub fn extract_tokens_from_text(input: &str) -> Vec<String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Vec::new();
    }

    // 1. Try parsing JSON
    if (trimmed.starts_with('[') && trimmed.ends_with(']'))
        || (trimmed.starts_with('{') && trimmed.ends_with('}'))
    {
        if let Ok(val) = serde_json::from_str::<serde_json::Value>(trimmed) {
            let mut tokens = Vec::new();
            if let Some(arr) = val.as_array() {
                for item in arr {
                    if let Some(s) = item.as_str() {
                        if s.starts_with("1//") {
                            tokens.push(s.to_string());
                        }
                    } else if let Some(obj) = item.as_object() {
                        if let Some(rt) = obj.get("refresh_token").and_then(|v| v.as_str()) {
                            if rt.starts_with("1//") {
                                tokens.push(rt.to_string());
                            }
                        }
                    }
                }
            } else if let Some(obj) = val.as_object() {
                if let Some(rt) = obj.get("refresh_token").and_then(|v| v.as_str()) {
                    if rt.starts_with("1//") {
                        tokens.push(rt.to_string());
                    }
                }
            }
            if !tokens.is_empty() {
                let mut unique = Vec::new();
                let mut set = std::collections::HashSet::new();
                for t in tokens {
                    if set.insert(t.clone()) {
                        unique.push(t);
                    }
                }
                return unique;
            }
        }
    }

    // 2. Regex / pattern scan for 1//[a-zA-Z0-9_\-]+
    let mut tokens = Vec::new();
    let mut set = std::collections::HashSet::new();
    let bytes = trimmed.as_bytes();
    let mut i = 0;
    while i + 3 <= bytes.len() {
        if &bytes[i..i + 3] == b"1//" {
            let start = i;
            let mut end = i + 3;
            while end < bytes.len() {
                let b = bytes[end];
                if b.is_ascii_alphanumeric() || b == b'_' || b == b'-' {
                    end += 1;
                } else {
                    break;
                }
            }
            if end - start >= 20 {
                if let Ok(token_str) = std::str::from_utf8(&bytes[start..end]) {
                    let s = token_str.to_string();
                    if set.insert(s.clone()) {
                        tokens.push(s);
                    }
                }
            }
            i = end;
        } else {
            i += 1;
        }
    }

    tokens
}

/// Import account from custom database path
pub fn import_from_custom_db_path(path: &Path) -> Result<AntigravityAccount, String> {
    let oauth_state = extract_oauth_state_from_file(path)?;
    let mut account = build_account_from_refresh_token(
        &oauth_state.refresh_token,
        None,
        Some("自定义 DB 导入".to_string()),
    )?;
    if oauth_state.project_id.is_some() && account.project_id.is_none() {
        account.project_id = oauth_state.project_id;
    }
    Ok(account)
}

/// Import accounts from V1 / legacy backup directory (~/.antigravity-agent or ~/.antigravity)
pub fn import_from_v1_backup(home_dir: &Path) -> Result<Vec<AntigravityAccount>, String> {
    use base64::{engine::general_purpose, Engine as _};

    let mut imported = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let dirs_to_check = [
        home_dir.join(".antigravity-agent"),
        home_dir.join(".antigravity"),
    ];

    for base_dir in dirs_to_check {
        if !base_dir.exists() {
            continue;
        }

        for index_name in &["antigravity_accounts.json", "accounts.json"] {
            let index_path = base_dir.join(index_name);
            if !index_path.exists() {
                continue;
            }

            let Ok(content) = std::fs::read_to_string(&index_path) else { continue };
            let Ok(val) = serde_json::from_str::<serde_json::Value>(&content) else { continue };

            let accounts_map = if let Some(map) = val.as_object() {
                if let Some(accs) = map.get("accounts").and_then(|v| v.as_object()) {
                    accs
                } else {
                    map
                }
            } else {
                continue;
            };

            for (_id, acc_info) in accounts_map {
                if !acc_info.is_object() {
                    continue;
                }
                let target_file = acc_info.get("backup_file").or_else(|| acc_info.get("data_file")).and_then(|v| v.as_str());
                let Some(file_str) = target_file else { continue };
                let mut target_path = PathBuf::from(file_str);
                if !target_path.exists() {
                    target_path = base_dir.join(target_path.file_name().unwrap_or_default());
                }
                if !target_path.exists() {
                    target_path = base_dir.join("backups").join(target_path.file_name().unwrap_or_default());
                }
                if !target_path.exists() {
                    target_path = base_dir.join("accounts").join(target_path.file_name().unwrap_or_default());
                }
                if !target_path.exists() {
                    continue;
                }

                let Ok(file_content) = std::fs::read_to_string(&target_path) else { continue };
                let Ok(data_val) = serde_json::from_str::<serde_json::Value>(&file_content) else { continue };

                let mut rt_opt = None;
                if let Some(token_data) = data_val.get("token") {
                    if let Some(rt) = token_data.get("refresh_token").and_then(|v| v.as_str()) {
                        rt_opt = Some(rt.to_string());
                    }
                }
                if rt_opt.is_none() {
                    if let Some(state_b64) = data_val.get("jetskiStateSync.agentManagerInitState").and_then(|v| v.as_str()) {
                        if let Ok(blob) = general_purpose::STANDARD.decode(state_b64) {
                            if let Ok(Some(oauth_data)) = find_field(&blob, 6) {
                                if let Ok(Some(refresh_bytes)) = find_field(&oauth_data, 3) {
                                    if let Ok(rt) = String::from_utf8(refresh_bytes) {
                                        rt_opt = Some(rt);
                                    }
                                }
                            }
                        }
                    }
                }

                if let Some(rt) = rt_opt {
                    if !rt.is_empty() && seen.insert(rt.clone()) {
                        if let Ok(acc) = build_account_from_refresh_token(&rt, None, Some("旧版数据导入".to_string())) {
                            imported.push(acc);
                        }
                    }
                }
            }
        }
    }

    if imported.is_empty() {
        return Err("未找到旧版数据备份文件".to_string());
    }

    Ok(imported)
}

/// Scan all local sources (candidate IDE state databases, System Keyring, CLI directories, and legacy backups)
pub fn import_all_local_accounts(home_dir: &Path) -> Result<Vec<AntigravityAccount>, String> {
    let mut imported = Vec::new();
    let mut seen_tokens = std::collections::HashSet::new();

    // 1. Candidate DB paths (Antigravity IDE & Antigravity state.vscdb)
    for db_path in get_all_candidate_db_paths(None) {
        if db_path.exists() {
            if let Ok(oauth_state) = extract_oauth_state_from_file(&db_path) {
                let rt = oauth_state.refresh_token;
                if !rt.is_empty() && seen_tokens.insert(rt.clone()) {
                    let label = if db_path.to_string_lossy().contains("Antigravity IDE") {
                        "Antigravity IDE"
                    } else {
                        "Antigravity DB"
                    };
                    if let Ok(mut acc) = build_account_from_refresh_token(&rt, None, Some(label.to_string())) {
                        if oauth_state.project_id.is_some() && acc.project_id.is_none() {
                            acc.project_id = oauth_state.project_id;
                        }
                        imported.push(acc);
                    }
                }
            }
        }
    }

    // 2. Keyring
    if let Ok(rt) = read_from_system_keyring() {
        if !rt.is_empty() && seen_tokens.insert(rt.clone()) {
            if let Ok(acc) = build_account_from_refresh_token(&rt, None, Some("系统凭据导入".to_string())) {
                imported.push(acc);
            }
        }
    }

    // 3. ~/.gemini/oauth_creds.json
    if let Ok(rt) = read_file_credentials(home_dir) {
        if !rt.is_empty() && seen_tokens.insert(rt.clone()) {
            if let Ok(acc) = build_account_from_refresh_token(&rt, None, Some("文件凭据导入".to_string())) {
                imported.push(acc);
            }
        }
    }

    // 4. Antigravity Manager directory (~/.antigravity_tools)
    if let Ok(mgr_accs) = import_from_antigravity_manager(home_dir) {
        for acc in mgr_accs {
            if seen_tokens.insert(acc.refresh_token.clone()) {
                imported.push(acc);
            }
        }
    }

    // 5. V1 backup (~/.antigravity-agent)
    if let Ok(v1_accs) = import_from_v1_backup(home_dir) {
        for acc in v1_accs {
            if seen_tokens.insert(acc.refresh_token.clone()) {
                imported.push(acc);
            }
        }
    }

    if imported.is_empty() {
        return Err("未在本地系统 Keyring、IDE 数据库或历史文件中找到已登录账号".to_string());
    }

    Ok(imported)
}

/// Import active account from local machine (Antigravity IDE state.vscdb, Windows Credential Manager, or ~/.gemini/oauth_creds.json).
pub fn import_from_local_system(home_dir: &Path) -> Result<AntigravityAccount, String> {
    // 1. Try local candidate databases (Antigravity IDE & Antigravity state.vscdb)
    let refresh_token = get_refresh_token_from_db(None)
        // 2. Try keyring
        .or_else(|_| read_from_system_keyring())
        // 3. Try ~/.gemini/oauth_creds.json
        .or_else(|_| read_file_credentials(home_dir))?;

    let mut account = build_account_from_refresh_token(&refresh_token, None, Some("本机导入".to_string()))?;
    account.is_active = true;
    Ok(account)
}

/// Check if auto refresh is due based on last refresh time and configured interval (minutes).
pub fn is_auto_refresh_due(last_refresh_time: Option<&str>, interval_minutes: u32) -> bool {
    if interval_minutes == 0 {
        return false;
    }
    let Some(last_str) = last_refresh_time else {
        return true;
    };
    let Ok(last_dt) = chrono::DateTime::parse_from_rfc3339(last_str) else {
        return true;
    };
    let now = chrono::Utc::now();
    let elapsed = now.signed_duration_since(last_dt);
    elapsed.num_minutes() >= interval_minutes as i64
}

/// Refresh all account quotas concurrently, matching Antigravity-Manager's refresh_all_quotas_logic.
/// Returns the count of successfully refreshed accounts.
pub fn refresh_all_quotas(store: &mut AntigravityStore) -> usize {
    if store.accounts.is_empty() {
        return 0;
    }
    let mut success_count = 0;
    std::thread::scope(|s| {
        let mut handles = Vec::new();
        for acc in &mut store.accounts {
            handles.push(s.spawn(move || {
                // Skip accounts that are already known to be forbidden due to TOS violation (Antigravity-Manager parity)
                if let Some(quota) = &acc.quota {
                    if quota.is_forbidden {
                        if let Some(reason) = &quota.forbidden_reason {
                            if reason.contains("TOS_VIOLATION") || reason.contains("violation of Terms of Service") {
                                return false;
                            }
                        }
                    }
                }
                if let Ok(()) = ensure_fresh_token(acc) {
                    if acc.project_id.is_none() || acc.tier.is_none() {
                        let (pid, tier) = fetch_project_and_tier(&acc.access_token);
                        if pid.is_some() { acc.project_id = pid; }
                        if tier.is_some() { acc.tier = tier; }
                    }
                    if let Ok(q) = fetch_quota(&acc.access_token, acc.project_id.as_deref(), acc.quota.as_ref()) {
                        acc.quota = Some(q);
                        return true;
                    }
                }
                false
            }));
        }
        for handle in handles {
            if let Ok(true) = handle.join() {
                success_count += 1;
            }
        }
    });
    success_count
}

/// Auto-sync current active account from local machine (Antigravity-Manager parity).
/// If local machine has an active account, marks it as active in the store (and imports it if not present).
/// Returns true if the active account or store changed.
pub fn auto_sync_active_account(home_dir: &Path, store: &mut AntigravityStore) -> bool {
    let local_acc = match import_from_local_system(home_dir) {
        Ok(a) => a,
        Err(_) => return false,
    };

    if let Some(pos) = store.accounts.iter().position(|a| a.email.eq_ignore_ascii_case(&local_acc.email)) {
        let was_active = store.accounts[pos].is_active;
        if !was_active {
            for a in &mut store.accounts {
                a.is_active = false;
            }
            store.accounts[pos].is_active = true;
            store.active_account_id = Some(store.accounts[pos].id.clone());
            return true;
        }
        false
    } else {
        // New account discovered from system, add it as active
        for a in &mut store.accounts {
            a.is_active = false;
        }
        let mut new_acc = local_acc;
        new_acc.is_active = true;
        let id = new_acc.id.clone();
        store.accounts.push(new_acc);
        store.active_account_id = Some(id);
        true
    }
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
        acc.custom_label = detail.get("custom_label").and_then(|v| v.as_str()).map(String::from);

        // Resolve subscription tier from detail.tier or detail.quota.subscription_tier
        let raw_tier = detail.get("tier").and_then(|v| v.as_str())
            .or_else(|| detail.get("quota").and_then(|q| q.get("subscription_tier")).and_then(|v| v.as_str()));
        if let Some(t) = raw_tier {
            acc.tier = Some(normalize_subscription_tier(t));
        }

        // Parse quota if present
        if let Some(quota_val) = detail.get("quota") {
            let mut quota = AntigravityQuota::default();
            quota.is_forbidden = quota_val.get("is_forbidden").and_then(|v| v.as_bool()).unwrap_or(false);
            quota.forbidden_reason = quota_val.get("forbidden_reason").and_then(|v| v.as_str()).map(String::from);
            if let Some(lu) = quota_val.get("last_updated").and_then(|v| v.as_i64()) {
                quota.last_updated = lu;
            }
            if let Some(st) = quota_val.get("subscription_tier").and_then(|v| v.as_str()) {
                let norm = normalize_subscription_tier(st);
                quota.subscription_tier = Some(norm.clone());
                if acc.tier.is_none() {
                    acc.tier = Some(norm);
                }
            }
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
            if let Some(groups) = quota_val.get("quota_groups").and_then(|v| v.as_array()) {
                for g in groups {
                    let g_name = g.get("display_name").and_then(|v| v.as_str()).unwrap_or("General").to_string();
                    if let Some(buckets) = g.get("buckets").and_then(|v| v.as_array()) {
                        for b in buckets {
                            let window = b.get("window").and_then(|v| v.as_str()).unwrap_or("").to_string();
                            let fraction = b.get("remaining_fraction").and_then(|v| v.as_f64()).unwrap_or(1.0);
                            let reset_time = b.get("reset_time").and_then(|v| v.as_str()).unwrap_or("").to_string();
                            let bucket_id = b.get("bucket_id").or_else(|| b.get("bucketId")).and_then(|v| v.as_str()).map(|s| s.to_string());
                            let display_name = g_name.clone();
                            quota.quota_groups.push(QuotaGroupInfo {
                                display_name,
                                window,
                                remaining_fraction: fraction,
                                reset_time,
                                bucket_id,
                            });
                        }
                    }
                }
            }
            acc.quota = Some(quota);
        }

        accounts.push(acc);
    }
    Ok(accounts)
}

/// Toggle account disabled status.
pub fn toggle_account_disabled(
    app_data: &Path,
    store: &mut AntigravityStore,
    account_id: &str,
) -> Result<bool, String> {
    let account = store.get_account_mut(account_id)
        .ok_or_else(|| format!("Account {} not found", account_id))?;
    account.disabled = !account.disabled;
    let new_state = account.disabled;
    save_store(app_data, store)?;
    Ok(new_state)
}

/// Update custom label for an account.
pub fn update_account_label(
    app_data: &Path,
    store: &mut AntigravityStore,
    account_id: &str,
    label: Option<String>,
) -> Result<(), String> {
    let account = store.get_account_mut(account_id)
        .ok_or_else(|| format!("Account {} not found", account_id))?;
    account.custom_label = label;
    save_store(app_data, store)?;
    Ok(())
}

/// Update device profile for an account.
pub fn update_account_device_profile(
    app_data: &Path,
    store: &mut AntigravityStore,
    account_id: &str,
    profile: DeviceProfile,
) -> Result<(), String> {
    let account = store.get_account_mut(account_id)
        .ok_or_else(|| format!("Account {} not found", account_id))?;
    account.device_profile = Some(profile);
    save_store(app_data, store)?;
    Ok(())
}

/// Switch active account: updates Windows Credential Manager, ~/.gemini/oauth_creds.json, and store.
pub fn switch_account(
    home_dir: &Path,
    app_data: &Path,
    store: &mut AntigravityStore,
    account_id: &str,
) -> Result<(), String> {
    switch_account_target(home_dir, app_data, store, account_id, None)
}

/// Switch active account targeting classic, IDE, or CLI.
pub fn switch_account_target(
    home_dir: &Path,
    app_data: &Path,
    store: &mut AntigravityStore,
    account_id: &str,
    target: Option<&str>,
) -> Result<(), String> {
    let account = store.get_account_mut(account_id)
        .ok_or_else(|| format!("Account {} not found", account_id))?;

    if account.disabled {
        return Err("Cannot switch to a disabled account".to_string());
    }

    // Ensure valid token before switching
    ensure_fresh_token(account)?;

    // 1. Write to system keyring
    write_to_system_keyring(account)?;

    // 2. Write to ~/.gemini files
    write_file_credentials(home_dir, account)?;

    // 3. If target is IDE or classic and device profile exists, write to storage.json
    if let Some(ref profile) = account.device_profile {
        let _ = write_device_profile_to_storage(target, profile);
    }

    // 4. Update store state
    for acc in &mut store.accounts {
        acc.is_active = acc.id == account_id;
        if acc.is_active {
            acc.last_used = Utc::now().timestamp();
        }
    }
    store.active_account_id = Some(account_id.to_string());

    save_store(app_data, store)?;
    info!("Successfully switched active Antigravity account to {} (target: {:?})", account_id, target);
    Ok(())
}

/// Write device profile to VS Code / Antigravity IDE storage.json if present.
pub fn write_device_profile_to_storage(_target: Option<&str>, _profile: &DeviceProfile) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        let target = _target;
        let profile = _profile;
        let appdata = match std::env::var("APPDATA") {
            Ok(v) => v,
            Err(_) => return Ok(()),
        };
        let folder = if target == Some("ide") { "Antigravity IDE" } else { "Antigravity" };
        let storage_path = PathBuf::from(&appdata)
            .join(folder)
            .join("User")
            .join("globalStorage")
            .join("storage.json");

        if storage_path.exists() {
            if let Ok(content) = std::fs::read_to_string(&storage_path) {
                if let Ok(mut val) = serde_json::from_str::<serde_json::Value>(&content) {
                    if let Some(map) = val.as_object_mut() {
                        map.insert("telemetry.machineId".to_string(), serde_json::Value::String(profile.machine_id.clone()));
                        map.insert("telemetry.macMachineId".to_string(), serde_json::Value::String(profile.mac_machine_id.clone()));
                        map.insert("telemetry.devDeviceId".to_string(), serde_json::Value::String(profile.dev_device_id.clone()));
                        map.insert("telemetry.sqmId".to_string(), serde_json::Value::String(profile.sqm_id.clone()));
                        map.insert("storage.serviceMachineId".to_string(), serde_json::Value::String(profile.dev_device_id.clone()));
                        if let Ok(pretty) = serde_json::to_string_pretty(&val) {
                            let _ = std::fs::write(&storage_path, pretty);
                        }
                    }
                }
            }
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Ephemeral OAuth Loopback Server
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// Ephemeral OAuth Loopback Server
// ---------------------------------------------------------------------------

pub struct OAuthServerSession {
    pub auth_url: String,
    pub state: String,
    pub port: u16,
    pub redirect_uri: String,
    cancelled: std::sync::Arc<std::sync::atomic::AtomicBool>,
    listener_v4: Option<std::net::TcpListener>,
    listener_v6: Option<std::net::TcpListener>,
}

impl OAuthServerSession {
    pub fn start() -> Result<Self, String> {
        let mut ipv4_listener = None;
        let mut ipv6_listener = None;
        let port: u16;

        // Try dual-stack binding on the same port, matching Antigravity-Manager
        match std::net::TcpListener::bind("[::1]:0") {
            Ok(l6) => {
                let p = l6.local_addr().map_err(|e| e.to_string())?.port();
                port = p;
                ipv6_listener = Some(l6);
                if let Ok(l4) = std::net::TcpListener::bind(format!("127.0.0.1:{}", p)) {
                    ipv4_listener = Some(l4);
                }
            }
            Err(_) => {
                let l4 = std::net::TcpListener::bind("127.0.0.1:0")
                    .map_err(|e| format!("Failed to bind local loopback server: {}", e))?;
                let p = l4.local_addr().map_err(|e| e.to_string())?.port();
                port = p;
                ipv4_listener = Some(l4);
                if let Ok(l6) = std::net::TcpListener::bind(format!("[::1]:{}", p)) {
                    ipv6_listener = Some(l6);
                }
            }
        }

        let has_ipv4 = ipv4_listener.is_some();
        let has_ipv6 = ipv6_listener.is_some();

        let redirect_uri = if has_ipv4 && has_ipv6 {
            format!("http://localhost:{}/oauth-callback", port)
        } else if has_ipv4 {
            format!("http://127.0.0.1:{}/oauth-callback", port)
        } else {
            format!("http://[::1]:{}/oauth-callback", port)
        };

        let state = uuid::Uuid::new_v4().to_string();

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
            redirect_uri,
            cancelled: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
            listener_v4: ipv4_listener,
            listener_v6: ipv6_listener,
        })
    }

    /// Signal cancellation to stop waiting and release listeners
    pub fn cancel(&self) {
        self.cancelled.store(true, std::sync::atomic::Ordering::SeqCst);
    }

    /// Check if cancelled
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(std::sync::atomic::Ordering::SeqCst)
    }

    /// Wait synchronously for the browser callback (with a timeout).
    /// Robust against empty browser preconnects, favicon requests, and query errors.
    pub fn wait_for_code(&self, timeout: Duration) -> Result<String, String> {
        use std::io::{Read, Write};

        let start = std::time::Instant::now();
        if let Some(ref l) = self.listener_v4 {
            let _ = l.set_nonblocking(true);
        }
        if let Some(ref l) = self.listener_v6 {
            let _ = l.set_nonblocking(true);
        }

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

        loop {
            if self.is_cancelled() {
                return Err("OAuth authorization cancelled.".to_string());
            }
            if start.elapsed() > timeout {
                return Err("OAuth authorization timed out. Please try again.".to_string());
            }

            let mut accepted_stream = None;

            if let Some(ref l4) = self.listener_v4 {
                match l4.accept() {
                    Ok((stream, _)) => accepted_stream = Some(stream),
                    Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
                    Err(e) => warn!("IPv4 accept error: {}", e),
                }
            }

            if accepted_stream.is_none() {
                if let Some(ref l6) = self.listener_v6 {
                    match l6.accept() {
                        Ok((stream, _)) => accepted_stream = Some(stream),
                        Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
                        Err(e) => warn!("IPv6 accept error: {}", e),
                    }
                }
            }

            let mut stream = match accepted_stream {
                Some(s) => s,
                None => {
                    std::thread::sleep(Duration::from_millis(100));
                    continue;
                }
            };

            // Set a short read timeout so reading from connected socket does not block forever
            let _ = stream.set_read_timeout(Some(Duration::from_secs(3)));
            let mut buffer = [0u8; 4096];
            let bytes_read = stream.read(&mut buffer).unwrap_or(0);

            // Ignore TCP handshake / preconnect with 0 bytes
            if bytes_read == 0 {
                continue;
            }

            let request = String::from_utf8_lossy(&buffer[..bytes_read]);

            // Handle favicon requests or other static probes
            if request.contains("/favicon.ico") {
                let _ = stream.write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n");
                let _ = stream.flush();
                continue;
            }

            let query_params = request
                .lines()
                .next()
                .and_then(|line| {
                    let parts: Vec<&str> = line.split_whitespace().collect();
                    if parts.len() >= 2 { Some(parts[1]) } else { None }
                })
                .and_then(|path| {
                    url::Url::parse(&format!("http://localhost{}", path)).ok()
                })
                .map(|url| {
                    let mut code = None;
                    let mut state = None;
                    let mut error = None;
                    let mut error_desc = None;
                    for (k, v) in url.query_pairs() {
                        if k == "code" {
                            code = Some(v.to_string());
                        } else if k == "state" {
                            state = Some(v.to_string());
                        } else if k == "error" {
                            error = Some(v.to_string());
                        } else if k == "error_description" {
                            error_desc = Some(v.to_string());
                        }
                    }
                    (code, state, error, error_desc)
                });

            let (code, rec_state, error, error_desc) = match query_params {
                Some((c, s, e, ed)) => (c, s, e, ed),
                None => (None, None, None, None),
            };

            // If Google returned an OAuth error directly (e.g. user cancelled)
            if let Some(err) = error {
                let _ = stream.write_all(fail_html.as_bytes());
                let _ = stream.flush();
                let desc = error_desc.unwrap_or(err);
                return Err(format!("Google 授权错误: {}", desc));
            }

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
            }

            // Connection without code or error (e.g. OPTIONS / preflight): respond and continue listening
            let _ = stream.write_all(b"HTTP/1.1 204 No Content\r\nContent-Length: 0\r\n\r\n");
            let _ = stream.flush();
        }
    }
}

/// Extract OAuth authorization code from manual input (can be raw code or full callback URL)
pub fn extract_oauth_code(input: &str) -> String {
    let trimmed = input.trim();
    if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        if let Ok(parsed) = url::Url::parse(trimmed) {
            for (k, v) in parsed.query_pairs() {
                if k == "code" {
                    return v.to_string();
                }
            }
        }
    }
    trimmed.to_string()
}

/// Open an URL in the default web browser.
pub fn open_browser(url: &str) {
    if let Err(e) = opener::open(url) {
        tracing::error!("Failed to open browser with opener: {}", e);
    }
}

// ---------------------------------------------------------------------------
// Antigravity Session Management (CLI + IDE/App)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AntigravitySessionMeta {
    pub session_id: String,
    pub source: String, // "cli" or "app"
    pub title: String,
    pub preview: String,
    pub project_dir: Option<String>,
    pub last_active_at: Option<i64>,
    pub step_count: usize,
    pub source_path: String,
    pub resume_command: Option<String>,
}

impl From<AntigravitySessionMeta> for crate::session::SessionMeta {
    fn from(s: AntigravitySessionMeta) -> Self {
        Self {
            provider_id: format!("antigravity:{}", s.source),
            session_id: s.session_id,
            title: Some(s.title),
            summary: Some(s.preview),
            project_dir: s.project_dir,
            created_at: s.last_active_at,
            last_active_at: s.last_active_at,
            source_path: s.source_path,
            resume_command: s.resume_command,
        }
    }
}

/// Strip formatting and tags from Antigravity user input / prompts.
pub fn clean_antigravity_snippet(raw: &str) -> String {
    let mut text = raw.trim();
    if let Some(start) = text.find("<USER_REQUEST>") {
        if let Some(end) = text[start..].find("</USER_REQUEST>") {
            text = text[start + "<USER_REQUEST>".len()..start + end].trim();
        }
    }
    let lines: Vec<&str> = text
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('<'))
        .collect();
    if lines.is_empty() {
        text.chars().take(100).collect()
    } else {
        lines.join(" ").chars().take(100).collect()
    }
}

/// Scan Antigravity sessions from both ~/.gemini/antigravity-cli and ~/.gemini/antigravity-ide.
pub fn scan_antigravity_sessions(home: &Path, limit: usize) -> Vec<AntigravitySessionMeta> {
    let mut sessions = vec![];
    let gemini_dir = home.join(".gemini");
    if !gemini_dir.is_dir() {
        return sessions;
    }

    // 1. Scan Antigravity CLI sessions
    let cli_dir = gemini_dir.join("antigravity-cli");
    if cli_dir.is_dir() {
        scan_cli_sessions(&cli_dir, &mut sessions);
    }

    // 2. Scan Antigravity IDE / App sessions
    let ide_dir = gemini_dir.join("antigravity-ide");
    if ide_dir.is_dir() {
        scan_ide_sessions(&ide_dir, &mut sessions);
    }

    // Sort descending by last_active_at
    sessions.sort_by_key(|s| std::cmp::Reverse(s.last_active_at));
    sessions.truncate(limit);
    sessions
}

fn scan_cli_sessions(cli_dir: &Path, out: &mut Vec<AntigravitySessionMeta>) {
    use std::collections::HashMap;
    use std::io::BufRead;

    // A map from conversationId -> (timestamp, workspace, display_prompt)
    let mut history_map: HashMap<String, (Option<i64>, Option<String>, String)> = HashMap::new();
    let history_file = cli_dir.join("history.jsonl");
    if let Ok(file) = std::fs::File::open(&history_file) {
        let reader = std::io::BufReader::new(file);
        for line in reader.lines().map_while(Result::ok) {
            let Ok(val) = serde_json::from_str::<serde_json::Value>(&line) else {
                continue;
            };
            let cid = val.get("conversationId").and_then(|v| v.as_str()).unwrap_or("").trim();
            if cid.is_empty() {
                continue;
            }
            let ts = val.get("timestamp").and_then(|v| v.as_i64());
            let ws = val.get("workspace").and_then(|v| v.as_str()).map(String::from);
            let display = val.get("display").and_then(|v| v.as_str()).unwrap_or("").trim();
            if let Some(entry) = history_map.get_mut(cid) {
                if ts.is_some() && (entry.0.is_none() || ts > entry.0) {
                    entry.0 = ts;
                }
                if ws.is_some() {
                    entry.1 = ws;
                }
                if !display.is_empty() {
                    entry.2 = display.to_string();
                }
            } else {
                history_map.insert(cid.to_string(), (ts, ws, display.to_string()));
            }
        }
    }

    // Scan brain directory
    let brain_dir = cli_dir.join("brain");
    if let Ok(entries) = std::fs::read_dir(&brain_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let cid = entry.file_name().to_string_lossy().to_string();
            // Check if valid session directory
            let transcript_path = path.join(".system_generated").join("logs").join("transcript.jsonl");
            let source_path = if transcript_path.is_file() {
                transcript_path
            } else {
                path.clone()
            };

            let (mut ts, ws, prompt) = history_map
                .remove(&cid)
                .unwrap_or_else(|| (None, None, String::new()));

            let mut title = if !prompt.is_empty() {
                clean_antigravity_snippet(&prompt)
            } else {
                String::new()
            };

            // If prompt was empty, inspect the first line of transcript
            if title.is_empty() && source_path.is_file() {
                if let Ok(first_line) = std::fs::read_to_string(&source_path) {
                    if let Some(line) = first_line.lines().next() {
                        if let Ok(val) = serde_json::from_str::<serde_json::Value>(line) {
                            if let Some(content) = val.get("content").and_then(|c| c.as_str()) {
                                title = clean_antigravity_snippet(content);
                            }
                            if ts.is_none() {
                                if let Some(created_at) = val.get("created_at").and_then(|c| c.as_str()) {
                                    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(created_at) {
                                        ts = Some(dt.timestamp_millis());
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // Fallback timestamp: directory mtime
            if ts.is_none() {
                if let Ok(meta) = entry.metadata() {
                    if let Ok(mtime) = meta.modified() {
                        let duration = mtime
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap_or_default();
                        ts = Some(duration.as_millis() as i64);
                    }
                }
            }

            if title.is_empty() {
                title = format!("CLI Session {}", if cid.len() >= 8 { &cid[..8] } else { &cid });
            }

            let preview = title.clone();
            out.push(AntigravitySessionMeta {
                session_id: cid.clone(),
                source: "cli".into(),
                title,
                preview,
                project_dir: ws,
                last_active_at: ts,
                step_count: 0,
                source_path: source_path.to_string_lossy().to_string(),
                resume_command: Some(format!("agy resume {cid}")),
            });
        }
    }

    // Any remaining items in history_map without brain folder
    for (cid, (ts, ws, display)) in history_map {
        let title = if !display.is_empty() {
            clean_antigravity_snippet(&display)
        } else {
            format!("CLI Session {}", if cid.len() >= 8 { &cid[..8] } else { &cid })
        };
        let preview = title.clone();
        let db_path = cli_dir.join("conversations").join(format!("{cid}.db"));
        out.push(AntigravitySessionMeta {
            session_id: cid.clone(),
            source: "cli".into(),
            title,
            preview,
            project_dir: ws,
            last_active_at: ts,
            step_count: 0,
            source_path: db_path.to_string_lossy().to_string(),
            resume_command: Some(format!("agy resume {cid}")),
        });
    }
}

fn scan_ide_sessions(ide_dir: &Path, out: &mut Vec<AntigravitySessionMeta>) {
    let conv_dir = ide_dir.join("conversations");
    let brain_dir = ide_dir.join("brain");

    if let Ok(entries) = std::fs::read_dir(&conv_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("pb") {
                continue;
            }
            let cid = entry
                .path()
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_default();
            if cid.is_empty() {
                continue;
            }

            let mut ts = None;
            if let Ok(meta) = entry.metadata() {
                if let Ok(mtime) = meta.modified() {
                    let duration = mtime
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default();
                    ts = Some(duration.as_millis() as i64);
                }
            }

            // Check if there is overview or logs in brain/<cid>
            let brain_session = brain_dir.join(&cid);
            let overview_path = brain_session.join(".system_generated").join("logs").join("overview.txt");
            let transcript_path = brain_session.join(".system_generated").join("logs").join("transcript.jsonl");
            let task_path = brain_session.join("task.md");
            let plan_path = brain_session.join("implementation_plan.md");

            let mut title = String::new();
            let mut ws = None;
            let mut source_path = path.clone();

            if overview_path.is_file() {
                source_path = overview_path.clone();
                if let Ok(file_content) = std::fs::read_to_string(&overview_path) {
                    if let Some(first_line) = file_content.lines().next() {
                        if let Ok(val) = serde_json::from_str::<serde_json::Value>(first_line) {
                            if let Some(c) = val.get("content").and_then(|c| c.as_str()) {
                                title = clean_antigravity_snippet(c);
                                if let Some(idx) = c.find("Active Document:") {
                                    let doc_line = c[idx + 16..].lines().next().unwrap_or("").trim();
                                    if let Some(dir) = std::path::Path::new(doc_line).parent() {
                                        ws = Some(dir.to_string_lossy().to_string());
                                    }
                                }
                            }
                        }
                    }
                }
            } else if transcript_path.is_file() {
                source_path = transcript_path.clone();
                if let Ok(file_content) = std::fs::read_to_string(&transcript_path) {
                    if let Some(first_line) = file_content.lines().next() {
                        if let Ok(val) = serde_json::from_str::<serde_json::Value>(first_line) {
                            if let Some(c) = val.get("content").and_then(|c| c.as_str()) {
                                title = clean_antigravity_snippet(c);
                            }
                        }
                    }
                }
            } else if task_path.is_file() {
                if let Ok(content) = std::fs::read_to_string(&task_path) {
                    title = content
                        .lines()
                        .map(str::trim)
                        .find(|l| !l.is_empty() && !l.starts_with("<!--"))
                        .map(|l| l.trim_start_matches('#').trim().to_string())
                        .unwrap_or_default();
                }
            } else if plan_path.is_file() {
                if let Ok(content) = std::fs::read_to_string(&plan_path) {
                    title = content
                        .lines()
                        .map(str::trim)
                        .find(|l| !l.is_empty() && !l.starts_with("<!--"))
                        .map(|l| l.trim_start_matches('#').trim().to_string())
                        .unwrap_or_default();
                }
            }

            if title.is_empty() {
                title = format!("App Session {}", if cid.len() >= 8 { &cid[..8] } else { &cid });
            }

            let preview = title.clone();
            out.push(AntigravitySessionMeta {
                session_id: cid,
                source: "app".into(),
                title,
                preview,
                project_dir: ws,
                last_active_at: ts,
                step_count: 0,
                source_path: source_path.to_string_lossy().to_string(),
                resume_command: None,
            });
        }
    }
}

pub fn delete_antigravity_session(home: &Path, session: &AntigravitySessionMeta) -> Result<(), String> {
    let gemini_dir = home.join(".gemini");
    if session.source == "cli" {
        let cli_dir = gemini_dir.join("antigravity-cli");
        let brain_path = cli_dir.join("brain").join(&session.session_id);
        if brain_path.exists() {
            let _ = std::fs::remove_dir_all(&brain_path);
        }
        let conv_path = cli_dir.join("conversations").join(format!("{}.db", session.session_id));
        if conv_path.exists() {
            let _ = std::fs::remove_file(&conv_path);
        }
        Ok(())
    } else {
        let ide_dir = gemini_dir.join("antigravity-ide");
        let brain_path = ide_dir.join("brain").join(&session.session_id);
        if brain_path.exists() {
            let _ = std::fs::remove_dir_all(&brain_path);
        }
        let conv_path = ide_dir.join("conversations").join(format!("{}.pb", session.session_id));
        if conv_path.exists() {
            let _ = std::fs::remove_file(&conv_path);
        }
        let annot_path = ide_dir.join("annotations").join(format!("{}.pbtxt", session.session_id));
        if annot_path.exists() {
            let _ = std::fs::remove_file(&annot_path);
        }
        Ok(())
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

    #[test]
    fn test_import_from_antigravity_manager_preserves_quota() {
        let temp = tempfile::tempdir().unwrap();
        let base = temp.path().join(".antigravity_tools");
        std::fs::create_dir_all(base.join("accounts")).unwrap();

        let accounts_index = serde_json::json!({
            "version": "2.0",
            "current_account_id": "acc-1",
            "accounts": [
                { "id": "acc-1", "email": "pro@example.com" }
            ]
        });
        std::fs::write(base.join("accounts.json"), accounts_index.to_string()).unwrap();

        let detail = serde_json::json!({
            "email": "pro@example.com",
            "token": {
                "access_token": "at",
                "refresh_token": "rt",
                "expiry_timestamp": 1800000000,
                "project_id": "p-1"
            },
            "quota": {
                "is_forbidden": false,
                "subscription_tier": "PRO",
                "models": [
                    {
                        "name": "gemini-3.8-flash",
                        "percentage": 95,
                        "reset_time": "2026-09-21T07:23:40Z"
                    }
                ],
                "quota_groups": [
                    {
                        "display_name": "Gemini Models",
                        "buckets": [
                            {
                                "window": "5h",
                                "remaining_fraction": 0.95,
                                "reset_time": "2026-09-21T07:23:40Z"
                            }
                        ]
                    }
                ]
            }
        });
        std::fs::write(base.join("accounts").join("acc-1.json"), detail.to_string()).unwrap();

        let imported = import_from_antigravity_manager(temp.path()).unwrap();
        assert_eq!(imported.len(), 1);
        let acc = &imported[0];
        assert_eq!(acc.email, "pro@example.com");
        assert_eq!(acc.tier.as_deref(), Some("PRO"));
        let q = acc.quota.as_ref().unwrap();
        assert!(!q.is_forbidden);
        assert_eq!(q.models.len(), 1);
        assert_eq!(q.models[0].percentage, 95);
        assert_eq!(q.models[0].reset_time, "2026-09-21T07:23:40Z");
        assert_eq!(q.quota_groups.len(), 1);
        assert_eq!(q.quota_groups[0].window, "5h");
        assert_eq!(q.quota_groups[0].reset_time, "2026-09-21T07:23:40Z");
    }

    #[test]
    fn test_scan_antigravity_sessions_mock() {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path();
        let cli_dir = home.join(".gemini").join("antigravity-cli");
        let ide_dir = home.join(".gemini").join("antigravity-ide");
        std::fs::create_dir_all(cli_dir.join("brain").join("cli-sess-1").join(".system_generated").join("logs")).unwrap();
        std::fs::create_dir_all(ide_dir.join("conversations")).unwrap();

        // Write history.jsonl
        let history_line = r#"{"conversationId":"cli-sess-1","display":"CLI prompt test","timestamp":1700000000000,"workspace":"/test/project"}"#;
        std::fs::write(cli_dir.join("history.jsonl"), format!("{history_line}\n")).unwrap();

        // Write transcript.jsonl
        let transcript = r#"{"type":"USER_INPUT","content":"<USER_REQUEST>\nCLI prompt test\n</USER_REQUEST>"}"#;
        std::fs::write(
            cli_dir.join("brain").join("cli-sess-1").join(".system_generated").join("logs").join("transcript.jsonl"),
            transcript,
        ).unwrap();

        // Write IDE .pb
        std::fs::write(ide_dir.join("conversations").join("ide-sess-1.pb"), b"mock").unwrap();

        let sessions = scan_antigravity_sessions(home, 10);
        assert_eq!(sessions.len(), 2);
        let cli_sess = sessions.iter().find(|s| s.source == "cli").unwrap();
        assert_eq!(cli_sess.session_id, "cli-sess-1");
        assert_eq!(cli_sess.title, "CLI prompt test");
        assert_eq!(cli_sess.project_dir.as_deref(), Some("/test/project"));
        assert_eq!(cli_sess.resume_command.as_deref(), Some("agy resume cli-sess-1"));

        let ide_sess = sessions.iter().find(|s| s.source == "app").unwrap();
        assert_eq!(ide_sess.session_id, "ide-sess-1");
        assert!(ide_sess.title.contains("ide-sess"));
    }

    #[test]
    fn test_is_auto_refresh_due() {
        assert!(is_auto_refresh_due(None, 15));
        assert!(!is_auto_refresh_due(None, 0));

        let now = chrono::Utc::now();
        let recent = (now - chrono::Duration::minutes(5)).to_rfc3339();
        assert!(!is_auto_refresh_due(Some(&recent), 15));
        assert!(is_auto_refresh_due(Some(&recent), 5));

        let old = (now - chrono::Duration::minutes(20)).to_rfc3339();
        assert!(is_auto_refresh_due(Some(&old), 15));
    }

    #[test]
    fn test_extract_tokens_from_text() {
        // Single token
        let t1 = "1//04abcdefghijklmnopqrstuvwxyz_1234567890";
        assert_eq!(extract_tokens_from_text(t1), vec![t1]);

        // JSON array of strings
        let json_arr = r#"["1//04abcdefghijklmnopqrstuvwxyz_1234567890", "1//04secondtoken1234567890abcdefghijklm"]"#;
        assert_eq!(
            extract_tokens_from_text(json_arr),
            vec!["1//04abcdefghijklmnopqrstuvwxyz_1234567890", "1//04secondtoken1234567890abcdefghijklm"]
        );

        // JSON array of objects with refresh_token
        let json_objs = r#"[{"refresh_token": "1//04abcdefghijklmnopqrstuvwxyz_1234567890"}, {"refresh_token": "1//04secondtoken1234567890abcdefghijklm"}]"#;
        assert_eq!(
            extract_tokens_from_text(json_objs),
            vec!["1//04abcdefghijklmnopqrstuvwxyz_1234567890", "1//04secondtoken1234567890abcdefghijklm"]
        );

        // Mixed text with duplicates
        let mixed = "Here is token: 1//04abcdefghijklmnopqrstuvwxyz_1234567890 and again 1//04abcdefghijklmnopqrstuvwxyz_1234567890 and another 1//04secondtoken1234567890abcdefghijklm end";
        assert_eq!(
            extract_tokens_from_text(mixed),
            vec!["1//04abcdefghijklmnopqrstuvwxyz_1234567890", "1//04secondtoken1234567890abcdefghijklm"]
        );
    }

    #[test]
    fn test_extract_oauth_state_from_real_db_if_present() {
        for db_path in get_all_candidate_db_paths(None) {
            if db_path.exists() {
                if let Ok(_state) = extract_oauth_state_from_file(&db_path) {
                    return;
                }
            }
        }
    }

    #[test]
    fn test_print_auth_url() {
        let session = OAuthServerSession::start().unwrap();
        println!("AUTH_URL: {}", session.auth_url);
    }

    #[test]
    fn test_import_from_antigravity_manager_live() {
        if let Ok(home_str) = std::env::var("USERPROFILE") {
            let home = PathBuf::from(home_str);
            if let Ok(accs) = import_from_antigravity_manager(&home) {
                println!("Successfully imported {} accounts from Antigravity Manager!", accs.len());
                for a in &accs {
                    println!("  Account: {} (tier: {:?}, active: {})", a.email, a.tier, a.is_active);
                }
                assert!(!accs.is_empty());
            }

            if let Ok(all_accs) = import_all_local_accounts(&home) {
                println!("Successfully imported {} total local accounts!", all_accs.len());
                assert!(!all_accs.is_empty());
            }
        }
    }
}
