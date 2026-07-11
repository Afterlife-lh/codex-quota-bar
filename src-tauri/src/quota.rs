use crate::{
    models::{CredentialStatus, QuotaSnapshot, QuotaWindow, WindowKind},
    settings::AppSettings,
};
use reqwest::StatusCode;
use serde::Deserialize;
use std::{
    fs,
    path::PathBuf,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use thiserror::Error;

const USAGE_URL: &str = "https://chatgpt.com/backend-api/wham/usage";

#[derive(Debug, Deserialize)]
struct AuthFile {
    auth_mode: Option<String>,
    tokens: Option<AuthTokens>,
    last_refresh: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AuthTokens {
    access_token: Option<String>,
    account_id: Option<String>,
}

struct Credentials {
    access_token: String,
    account_id: Option<String>,
    stale: bool,
}

#[derive(Debug, Deserialize)]
struct UsageResponse {
    rate_limit: Option<RateLimit>,
}
#[derive(Debug, Deserialize)]
struct RateLimit {
    primary_window: Option<RateLimitWindow>,
    secondary_window: Option<RateLimitWindow>,
}
#[derive(Debug, Deserialize)]
struct RateLimitWindow {
    used_percent: Option<f64>,
    limit_window_seconds: Option<i64>,
    reset_at: Option<i64>,
}

#[derive(Debug, Error)]
enum QueryError {
    #[error("未找到 Codex ChatGPT 登录信息")]
    NotFound,
    #[error("Codex 登录信息无法读取: {0}")]
    Parse(String),
    #[error("Codex 登录已失效，请在 Codex 中重新登录")]
    Unauthorized,
    #[error("额度服务请求过于频繁，请稍后再试")]
    RateLimited,
    #[error("网络请求失败: {0}")]
    Network(String),
    #[error("额度服务返回 HTTP {0}")]
    Http(u16),
    #[error("额度响应格式暂不受支持: {0}")]
    Response(String),
}

pub struct QuotaService {
    client: reqwest::Client,
}

impl QuotaService {
    pub fn new() -> Result<Self, String> {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(15))
            .build()
            .map_err(|e| format!("无法创建网络客户端: {e}"))?;
        Ok(Self { client })
    }

    pub async fn refresh(&self, settings: &AppSettings, previous: &QuotaSnapshot) -> QuotaSnapshot {
        let credentials = match read_credentials(settings) {
            Ok(value) => value,
            Err(error) => return merge_failure(previous, error),
        };
        match self.query(&credentials).await {
            Ok(windows) => QuotaSnapshot {
                windows,
                queried_at: Some(now_millis()),
                cached: false,
                credential_status: CredentialStatus::Valid,
                error: credentials
                    .stale
                    .then(|| "登录信息较旧；若查询失败，请在 Codex 中重新登录".to_string()),
            },
            Err(error) => merge_failure(previous, error),
        }
    }

    async fn query(&self, credentials: &Credentials) -> Result<Vec<QuotaWindow>, QueryError> {
        let mut request = self
            .client
            .get(USAGE_URL)
            // The private usage endpoint can be served by CDN/backend nodes with
            // different snapshots. A unique query plus explicit no-cache headers
            // prevents consecutive refreshes from alternating between stale data.
            .query(&[("_cq", now_millis().to_string())])
            .bearer_auth(&credentials.access_token)
            .header("User-Agent", "codex-cli")
            .header("Accept", "application/json")
            .header("Cache-Control", "no-cache, no-store")
            .header("Pragma", "no-cache");
        if let Some(account_id) = credentials.account_id.as_deref() {
            request = request.header("ChatGPT-Account-Id", account_id);
        }
        let response = request
            .send()
            .await
            .map_err(|e| QueryError::Network(safe_network_error(&e)))?;
        match response.status() {
            StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => {
                return Err(QueryError::Unauthorized)
            }
            StatusCode::TOO_MANY_REQUESTS => return Err(QueryError::RateLimited),
            status if !status.is_success() => return Err(QueryError::Http(status.as_u16())),
            _ => {}
        }
        let body: UsageResponse = response
            .json()
            .await
            .map_err(|e| QueryError::Response(e.to_string()))?;
        let rate_limit = body
            .rate_limit
            .ok_or_else(|| QueryError::Response("缺少 rate_limit".into()))?;
        let mut windows = [rate_limit.primary_window, rate_limit.secondary_window]
            .into_iter()
            .flatten()
            .filter_map(to_quota_window)
            .collect::<Vec<_>>();
        windows.sort_by_key(|window| match window.kind {
            WindowKind::FiveHour => 0,
            WindowKind::SevenDay => 1,
            WindowKind::ThirtyDay => 2,
            WindowKind::Unknown => 3,
        });
        if windows.is_empty() {
            return Err(QueryError::Response("没有可显示的额度窗口".into()));
        }
        Ok(windows)
    }
}

pub fn auth_path(settings: &AppSettings) -> PathBuf {
    settings.codex_home_path().join("auth.json")
}

fn read_credentials(settings: &AppSettings) -> Result<Credentials, QueryError> {
    let path = auth_path(settings);
    let raw = fs::read_to_string(&path).map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            QueryError::NotFound
        } else {
            QueryError::Parse(error.to_string())
        }
    })?;
    let auth: AuthFile =
        serde_json::from_str(&raw).map_err(|e| QueryError::Parse(e.to_string()))?;
    if auth.auth_mode.as_deref() != Some("chatgpt") {
        return Err(QueryError::NotFound);
    }
    let tokens = auth
        .tokens
        .ok_or_else(|| QueryError::Parse("缺少 tokens".into()))?;
    let access_token = tokens
        .access_token
        .filter(|token| !token.trim().is_empty())
        .ok_or_else(|| QueryError::Parse("缺少 access_token".into()))?;
    let stale = auth
        .last_refresh
        .as_deref()
        .and_then(|value| chrono::DateTime::parse_from_rfc3339(value).ok())
        .is_some_and(|date| {
            chrono::Utc::now()
                .signed_duration_since(date.with_timezone(&chrono::Utc))
                .num_days()
                > 8
        });
    Ok(Credentials {
        access_token,
        account_id: tokens.account_id.filter(|id| !id.trim().is_empty()),
        stale,
    })
}

fn to_quota_window(window: RateLimitWindow) -> Option<QuotaWindow> {
    let used = window.used_percent?.clamp(0.0, 100.0);
    let seconds = window.limit_window_seconds.unwrap_or_default();
    let (kind, label) = match seconds {
        18_000 => (WindowKind::FiveHour, "5 小时"),
        604_800 => (WindowKind::SevenDay, "7 天"),
        2_592_000 => (WindowKind::ThirtyDay, "30 天"),
        _ => (WindowKind::Unknown, "其他窗口"),
    };
    Some(QuotaWindow {
        kind,
        label: label.to_string(),
        used_percent: used,
        remaining_percent: 100.0 - used,
        reset_at: window
            .reset_at
            .map(|timestamp| timestamp.saturating_mul(1000)),
    })
}

fn merge_failure(previous: &QuotaSnapshot, error: QueryError) -> QuotaSnapshot {
    let status = match &error {
        QueryError::NotFound => CredentialStatus::NotFound,
        QueryError::Parse(_) => CredentialStatus::ParseError,
        QueryError::Unauthorized => CredentialStatus::Expired,
        _ => CredentialStatus::Valid,
    };
    QuotaSnapshot {
        windows: previous.windows.clone(),
        queried_at: previous.queried_at,
        cached: !previous.windows.is_empty(),
        credential_status: status,
        error: Some(error.to_string()),
    }
}

pub fn is_suspicious_premature_reset(
    previous: &QuotaSnapshot,
    candidate: &QuotaSnapshot,
    now_millis: i64,
) -> bool {
    if candidate.cached || candidate.credential_status != CredentialStatus::Valid {
        return false;
    }
    previous.windows.iter().any(|old| {
        let reset_is_still_in_future = old
            .reset_at
            .is_some_and(|reset| reset > now_millis.saturating_add(120_000));
        let Some(new) = candidate.windows.iter().find(|item| item.kind == old.kind) else {
            return false;
        };
        // Remaining quota should not materially increase before the current
        // window resets. Require a second matching response before accepting
        // such a change, regardless of whether it affects 5h, 7d, or 30d.
        reset_is_still_in_future && new.remaining_percent - old.remaining_percent >= 10.0
    })
}

pub fn same_quota_values(left: &QuotaSnapshot, right: &QuotaSnapshot) -> bool {
    left.windows.len() == right.windows.len()
        && left.windows.iter().all(|item| {
            right.windows.iter().any(|other| {
                item.kind == other.kind
                    && (item.remaining_percent - other.remaining_percent).abs() <= 2.0
            })
        })
}

fn safe_network_error(error: &reqwest::Error) -> String {
    if error.is_timeout() {
        "请求超时".into()
    } else if error.is_connect() {
        "无法连接额度服务".into()
    } else {
        "传输错误".into()
    }
}

fn now_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

#[cfg(test)]
mod tests {
    use super::*;
    fn parsed(seconds: i64, used: f64) -> QuotaWindow {
        to_quota_window(RateLimitWindow {
            used_percent: Some(used),
            limit_window_seconds: Some(seconds),
            reset_at: Some(100),
        })
        .unwrap()
    }
    #[test]
    fn maps_known_windows_and_remaining_percent() {
        let five = parsed(18_000, 18.0);
        assert_eq!(five.kind, WindowKind::FiveHour);
        assert_eq!(five.remaining_percent, 82.0);
        assert_eq!(five.reset_at, Some(100_000));
        assert_eq!(parsed(604_800, 36.0).kind, WindowKind::SevenDay);
        assert_eq!(parsed(2_592_000, 15.0).kind, WindowKind::ThirtyDay);
    }
    #[test]
    fn clamps_invalid_percentages() {
        assert_eq!(parsed(18_000, -5.0).remaining_percent, 100.0);
        assert_eq!(parsed(18_000, 120.0).remaining_percent, 0.0);
    }
    #[test]
    fn ignores_window_without_utilization() {
        assert!(to_quota_window(RateLimitWindow {
            used_percent: None,
            limit_window_seconds: Some(18_000),
            reset_at: None
        })
        .is_none());
    }

    fn snapshot(five_remaining: f64, reset_at: i64) -> QuotaSnapshot {
        QuotaSnapshot {
            windows: vec![QuotaWindow {
                kind: WindowKind::FiveHour,
                label: "5 hours".into(),
                used_percent: 100.0 - five_remaining,
                remaining_percent: five_remaining,
                reset_at: Some(reset_at),
            }],
            queried_at: Some(1),
            cached: false,
            credential_status: CredentialStatus::Valid,
            error: None,
        }
    }

    #[test]
    fn detects_large_premature_five_hour_reset() {
        let now = 1_000_000;
        assert!(is_suspicious_premature_reset(
            &snapshot(0.0, now + 600_000),
            &snapshot(91.0, now + 18_000_000),
            now
        ));
        assert!(!is_suspicious_premature_reset(
            &snapshot(0.0, now + 60_000),
            &snapshot(91.0, now + 18_000_000),
            now
        ));
    }

    #[test]
    fn detects_premature_jump_even_when_quota_is_not_exhausted() {
        let now = 1_000_000;
        assert!(is_suspicious_premature_reset(
            &snapshot(70.0, now + 600_000),
            &snapshot(98.0, now + 1_200_000),
            now
        ));
        assert!(!is_suspicious_premature_reset(
            &snapshot(70.0, now + 600_000),
            &snapshot(69.0, now + 600_000),
            now
        ));
    }

    #[test]
    fn confirms_two_consistent_candidate_values() {
        assert!(same_quota_values(&snapshot(91.0, 10), &snapshot(90.2, 20)));
        assert!(!same_quota_values(&snapshot(91.0, 10), &snapshot(0.0, 20)));
    }
}
