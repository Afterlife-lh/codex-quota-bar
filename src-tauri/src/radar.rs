use serde::Serialize;
use serde_json::Value;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const PUBLIC_RADAR_URL: &str = "https://codexradar.com/current.json";

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct RadarSnapshot {
    pub models: Vec<RadarModel>,
    pub quota_rows: Vec<RadarQuotaRow>,
    pub status: Option<String>,
    pub signal: Option<String>,
    pub batch: Option<String>,
    pub quota_batch: Option<String>,
    pub updated_at: Option<String>,
    pub fetched_at: Option<i64>,
    pub source: String,
    pub attribution: String,
    pub site_url: String,
    pub cached: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RadarModel {
    pub id: String,
    pub label: String,
    pub score: Option<f64>,
    pub status: Option<String>,
    pub passed: Option<i64>,
    pub valid_tasks: Option<i64>,
    pub invalid_tasks: Option<i64>,
    pub wall_time: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RadarQuotaRow {
    pub tier: String,
    pub five_hour: Option<f64>,
    pub seven_day: Option<f64>,
    pub basis: Option<String>,
}

pub struct RadarService {
    client: reqwest::Client,
}

impl RadarService {
    pub fn new() -> Result<Self, String> {
        Ok(Self {
            client: reqwest::Client::builder()
                .timeout(Duration::from_secs(12))
                .build()
                .map_err(|error| error.to_string())?,
        })
    }

    pub async fn refresh(&self, previous: &RadarSnapshot) -> RadarSnapshot {
        let result = self
            .client
            .get(PUBLIC_RADAR_URL)
            .header("Accept", "application/json")
            .header("User-Agent", "codex-quota-bar/0.5")
            .header("Cache-Control", "no-cache")
            .send()
            .await
            .and_then(|response| response.error_for_status())
            .map_err(|error| safe_error(&error));
        match result {
            Ok(response) => match response.json::<Value>().await {
                Ok(value) => parse_summary(&value, now_millis()).unwrap_or_else(|error| {
                    merge_failure(previous, format!("Radar 数据格式错误：{error}"))
                }),
                Err(error) => merge_failure(previous, format!("Radar 响应无法解析：{error}")),
            },
            Err(error) => merge_failure(previous, error),
        }
    }
}

fn parse_model(id: String, label: String, value: &Value) -> RadarModel {
    RadarModel {
        id,
        label,
        score: value.get("score").and_then(Value::as_f64),
        status: text(value, "status"),
        passed: value.get("passed").and_then(Value::as_i64),
        valid_tasks: value
            .get("valid_tasks")
            .or_else(|| value.get("tasks"))
            .and_then(Value::as_i64),
        invalid_tasks: value
            .get("invalid_tasks")
            .or_else(|| value.get("invalid"))
            .and_then(Value::as_i64),
        wall_time: text(value, "wall_time_human"),
    }
}

fn parse_summary(root: &Value, fetched_at: i64) -> Result<RadarSnapshot, String> {
    let model_iq = root
        .get("model_iq")
        .and_then(Value::as_object)
        .ok_or_else(|| "缺少 model_iq".to_string())?;
    let mut models = Vec::new();
    if let Some(latest) = model_iq.get("latest") {
        let model = text(latest, "model").unwrap_or_else(|| "Latest".into());
        let effort = text(latest, "reasoning_effort").unwrap_or_default();
        let label = format!("{} {}", model, effort).trim().to_string();
        models.push(parse_model("latest".into(), label, latest));
    }
    if let Some(comparisons) = model_iq.get("comparisons").and_then(Value::as_object) {
        let mut entries = comparisons.iter().collect::<Vec<_>>();
        entries.sort_by_key(|(key, _)| *key);
        for (key, comparison) in entries {
            let Some(latest) = comparison.get("latest") else {
                continue;
            };
            let label = text(comparison, "label").unwrap_or_else(|| key.replace('_', " "));
            models.push(parse_model(key.clone(), label, latest));
        }
    }
    models.truncate(6);

    let quota = model_iq.get("quota_radar");
    let quota_rows = quota
        .and_then(|value| value.get("rows"))
        .and_then(Value::as_array)
        .map(|rows| {
            rows.iter()
                .filter_map(|row| {
                    Some(RadarQuotaRow {
                        tier: text(row, "tier")?,
                        five_hour: row.get("five_h").and_then(Value::as_f64),
                        seven_day: row.get("seven_d").and_then(Value::as_f64),
                        basis: text(row, "basis"),
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    let requirements = root.pointer("/api_access/requirements");
    Ok(RadarSnapshot {
        models,
        quota_rows,
        status: text(root, "status"),
        signal: root
            .pointer("/prediction/expected_window")
            .and_then(Value::as_str)
            .or_else(|| root.pointer("/window/message").and_then(Value::as_str))
            .or_else(|| root.pointer("/prediction/summary").and_then(Value::as_str))
            .map(str::to_string),
        batch: model_iq.get("latest").and_then(|v| text(v, "date")),
        quota_batch: quota.and_then(|v| text(v, "date")),
        updated_at: quota
            .and_then(|v| text(v, "updated_at"))
            .or_else(|| text(root, "monitored_at")),
        fetched_at: Some(fetched_at),
        source: "public_summary".into(),
        attribution: requirements
            .and_then(|v| text(v, "attribution_text"))
            .unwrap_or_else(|| "数据来自 Codex 雷达 codexradar.com".into()),
        site_url: requirements
            .and_then(|v| text(v, "site"))
            .filter(|url| url == "https://codexradar.com")
            .unwrap_or_else(|| "https://codexradar.com".into()),
        cached: false,
        error: None,
    })
}

fn text(value: &Value, key: &str) -> Option<String> {
    value
        .get(key)?
        .as_str()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

fn merge_failure(previous: &RadarSnapshot, error: String) -> RadarSnapshot {
    let mut snapshot = previous.clone();
    snapshot.cached = !snapshot.models.is_empty() || !snapshot.quota_rows.is_empty();
    snapshot.error = Some(error);
    snapshot
}

fn safe_error(error: &reqwest::Error) -> String {
    if error.is_timeout() {
        "Codex Radar 请求超时".into()
    } else {
        "无法连接 Codex Radar".into()
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

    #[test]
    fn parses_dynamic_models_and_quota_rows() {
        let value: Value = serde_json::from_str(r#"{
          "status":"community_confirmed","window":{"message":"等待下一轮"},
          "api_access":{"requirements":{"attribution_text":"数据来自 Codex 雷达 codexradar.com","site":"https://codexradar.com"}},
          "model_iq":{"latest":{"date":"2026-07-11-pm","model":"gpt-5.6-sol","reasoning_effort":"max","score":135,"passed":9,"valid_tasks":10},
          "comparisons":{"future_model":{"label":"Future high","latest":{"score":105,"passed":7,"tasks":10}}},
          "quota_radar":{"date":"2026-07-11-pm","rows":[{"tier":"20x Pro","five_h":328.15,"seven_d":1968.9,"basis":"measured"}]}}
        }"#).unwrap();
        let snapshot = parse_summary(&value, 42).unwrap();
        assert_eq!(snapshot.models.len(), 2);
        assert_eq!(snapshot.models[1].label, "Future high");
        assert_eq!(snapshot.quota_rows[0].five_hour, Some(328.15));
        assert_eq!(snapshot.fetched_at, Some(42));
    }
}
