use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const PUBLIC_RADAR_URL: &str = "https://codexradar.com/current.json";
const PUBLIC_RATINGS_URL: &str = "https://codexradar.com/api/model-ratings";

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
    pub community_score: Option<f64>,
    pub community_votes: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RadarQuotaRow {
    pub tier: String,
    pub five_hour: Option<f64>,
    pub seven_day: Option<f64>,
    pub basis: Option<String>,
}

pub struct RadarService;

impl RadarService {
    pub fn new() -> Result<Self, String> {
        Ok(Self)
    }

    pub async fn refresh(&self, previous: &RadarSnapshot) -> RadarSnapshot {
        let summary = self.fetch_json(PUBLIC_RADAR_URL);
        let ratings = self.fetch_json(PUBLIC_RATINGS_URL);
        let (result, ratings) = tokio::join!(summary, ratings);
        match result {
            Ok(value) => match parse_summary(&value, now_millis()) {
                Ok(mut snapshot) => {
                    if let Ok(value) = ratings {
                        apply_ratings(&mut snapshot, &value);
                    }
                    snapshot
                }
                Err(error) => merge_failure(previous, format!("Radar 数据格式错误：{error}")),
            },
            Err(error) => merge_failure(previous, error),
        }
    }

    async fn fetch_json(&self, url: &str) -> Result<Value, String> {
        crate::network::client(Duration::from_secs(12))?
            .get(url)
            .header("Accept", "application/json")
            .header("User-Agent", "codex-quota-bar/0.6")
            .header("Cache-Control", "no-cache")
            .send()
            .await
            .and_then(|response| response.error_for_status())
            .map_err(|error| safe_error(&error))?
            .json::<Value>()
            .await
            .map_err(|error| format!("Radar 响应无法解析：{error}"))
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
        community_score: None,
        community_votes: None,
    }
}

fn model_id(value: &Value) -> Option<String> {
    let model = text(value, "model")?.to_ascii_lowercase();
    let effort = text(value, "reasoning_effort")?.to_ascii_lowercase();
    Some(format!("{model}-{effort}"))
}

fn model_sort_key(model: &RadarModel) -> (u8, u8, String) {
    let id = model.id.to_ascii_lowercase();
    let family = if id.contains("-sol-") {
        0
    } else if id.contains("-terra-") {
        1
    } else if id.contains("-luna-") {
        2
    } else {
        3
    };
    let effort = if id.ends_with("-max") {
        0
    } else if id.ends_with("-xhigh") {
        1
    } else if id.ends_with("-high") {
        2
    } else if id.ends_with("-medium") {
        3
    } else if id.ends_with("-low") {
        4
    } else {
        5
    };
    (family, effort, id)
}

#[derive(Debug, Deserialize)]
struct RatingModel {
    id: String,
    average: Option<f64>,
    count: Option<i64>,
}

fn apply_ratings(snapshot: &mut RadarSnapshot, root: &Value) {
    let ratings: HashMap<String, RatingModel> = root
        .get("models")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|value| serde_json::from_value::<RatingModel>(value.clone()).ok())
        .map(|rating| (rating.id.to_ascii_lowercase(), rating))
        .collect();
    for model in &mut snapshot.models {
        if let Some(rating) = ratings.get(&model.id.to_ascii_lowercase()) {
            model.community_score = rating.average;
            model.community_votes = rating.count;
        }
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
        let id = model_id(latest).unwrap_or_else(|| "latest".into());
        models.push(parse_model(id, label, latest));
    }
    if let Some(comparisons) = model_iq.get("comparisons").and_then(Value::as_object) {
        let mut entries = comparisons.iter().collect::<Vec<_>>();
        entries.sort_by_key(|(key, _)| *key);
        for (key, comparison) in entries {
            let Some(latest) = comparison.get("latest") else {
                continue;
            };
            let label = text(comparison, "label").unwrap_or_else(|| key.replace('_', " "));
            let id = model_id(latest)
                .or_else(|| model_id(comparison))
                .unwrap_or_else(|| key.replace('_', "-"));
            models.push(parse_model(id, label, latest));
        }
    }
    models.sort_by_key(model_sort_key);
    models.dedup_by(|left, right| left.id.eq_ignore_ascii_case(&right.id));

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

    #[test]
    fn joins_community_scores_by_stable_model_identity() {
        let mut snapshot = RadarSnapshot {
            models: vec![parse_model(
                "gpt-5.6-sol-max".into(),
                "GPT-5.6 Sol max".into(),
                &serde_json::json!({"score": 135}),
            )],
            ..RadarSnapshot::default()
        };
        apply_ratings(
            &mut snapshot,
            &serde_json::json!({"models":[{"id":"gpt-5.6-sol-max","average":8.5,"count":115}]}),
        );
        assert_eq!(snapshot.models[0].community_score, Some(8.5));
        assert_eq!(snapshot.models[0].community_votes, Some(115));
    }

    #[test]
    fn sorts_known_families_and_efforts_in_site_order() {
        let mut models = [
            ("gpt-5.6-luna-medium", "Luna medium"),
            ("gpt-5.6-sol-low", "Sol low"),
            ("gpt-5.6-terra-xhigh", "Terra xhigh"),
            ("gpt-5.6-sol-max", "Sol max"),
        ]
        .into_iter()
        .map(|(id, label)| parse_model(id.into(), label.into(), &serde_json::json!({})))
        .collect::<Vec<_>>();
        models.sort_by_key(model_sort_key);
        assert_eq!(
            models
                .iter()
                .map(|model| model.id.as_str())
                .collect::<Vec<_>>(),
            [
                "gpt-5.6-sol-max",
                "gpt-5.6-sol-low",
                "gpt-5.6-terra-xhigh",
                "gpt-5.6-luna-medium"
            ]
        );
    }
}
