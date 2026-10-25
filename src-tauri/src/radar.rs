use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const PUBLIC_ROUND_URL: &str = "https://codexradar.com/degradation-codex-latest.json";
const PUBLIC_RATINGS_URL: &str = "https://codexradar.com/api/model-ratings";

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct RadarSnapshot {
    pub models: Vec<RadarModel>,
    pub score_max: Option<f64>,
    pub status: Option<String>,
    pub batch: Option<String>,
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
    pub model: String,
    pub model_label: String,
    pub effort: String,
    pub score: Option<f64>,
    pub score_max: f64,
    pub status: String,
    pub items_scored: u64,
    pub items_expected: u64,
    pub wall_time: Option<String>,
    pub cost_usd: Option<f64>,
    pub cost_kind: String,
    pub cost_known: usize,
    pub cost_total: usize,
    pub community_score: Option<f64>,
    pub community_votes: Option<i64>,
}

pub struct RadarService;

impl RadarService {
    pub fn new() -> Result<Self, String> {
        Ok(Self)
    }

    pub async fn refresh(&self, previous: &RadarSnapshot) -> RadarSnapshot {
        let (round, ratings) = tokio::join!(
            self.fetch_json(PUBLIC_ROUND_URL),
            self.fetch_json(PUBLIC_RATINGS_URL)
        );
        match round.and_then(|value| parse_round(&value, now_millis())) {
            Ok(mut snapshot) => {
                if let Ok(value) = ratings {
                    apply_ratings(&mut snapshot, &value);
                }
                snapshot
            }
            Err(error) => merge_failure(previous, error),
        }
    }

    async fn fetch_json(&self, url: &str) -> Result<Value, String> {
        crate::network::client(Duration::from_secs(12))?
            .get(url)
            .header("Accept", "application/json")
            .header(
                "User-Agent",
                concat!("codex-quota-bar/", env!("CARGO_PKG_VERSION")),
            )
            .header("Cache-Control", "no-cache")
            .send()
            .await
            .and_then(|response| response.error_for_status())
            .map_err(|error| safe_error(&error))?
            .json::<Value>()
            .await
            .map_err(|_| "Codex Radar 响应无法解析".to_string())
    }
}

fn parse_round(root: &Value, fetched_at: i64) -> Result<RadarSnapshot, String> {
    if root.get("schema").and_then(Value::as_str) != Some("degradation-codex-plan/3") {
        return Err("Codex Radar 综合分快照版本暂不兼容".into());
    }
    let settings = root
        .get("settings")
        .and_then(Value::as_array)
        .ok_or_else(|| "Codex Radar 缺少模型档位列表".to_string())?;
    let round = root
        .get("round")
        .filter(|value| value.is_object())
        .ok_or_else(|| "Codex Radar 缺少本轮成绩".to_string())?;
    if root.pointer("/taskset/harness").and_then(Value::as_str) != Some("codex")
        || root.pointer("/taskset/version").and_then(Value::as_str)
            != round.get("taskset_version").and_then(Value::as_str)
    {
        return Err("Codex Radar 测试题组与本轮成绩不匹配".into());
    }
    let score_max = finite(root.pointer("/scoring_policy/total_max"))
        .filter(|v| *v > 0.0)
        .unwrap_or(150.0);
    let items_expected = root
        .pointer("/scoring_policy/question_count")
        .and_then(Value::as_u64)
        .filter(|count| *count > 0)
        .unwrap_or(10);
    let tasks = root
        .get("tasks")
        .and_then(Value::as_array)
        .ok_or_else(|| "Codex Radar 缺少测试项目".to_string())?;
    let mut seen = HashSet::new();
    let mut models = Vec::new();
    // This published array also determines the website's model order.
    // Keep it intact; the frontend groups models and orders their efforts.
    for setting in settings {
        let (Some(model), Some(effort)) = (text(setting, "model"), text(setting, "effort")) else {
            continue;
        };
        let model = model.to_ascii_lowercase();
        let effort = effort.to_ascii_lowercase();
        let key = format!("{model}|{effort}");
        if !seen.insert(key.clone()) {
            continue;
        }
        if setting
            .pointer("/availability/reason")
            .and_then(Value::as_str)
            == Some("unsupported_effort")
        {
            continue;
        }
        let availability = setting
            .pointer("/availability/status")
            .and_then(Value::as_str)
            .unwrap_or("pending");
        let totals = round.get("totals").and_then(|value| value.get(&key));
        let items_scored = totals
            .and_then(|value| value.get("items_scored"))
            .and_then(Value::as_u64)
            .unwrap_or(0);
        let complete = totals.is_some_and(|value| {
            value.get("complete").and_then(Value::as_bool) == Some(true)
                && value.get("items_expected").and_then(Value::as_u64) == Some(items_expected)
                && items_scored == items_expected
                && finite(value.get("score_max")) == Some(score_max)
        });
        // Only use the published ten-item total, never the legacy IQ or a
        // partially scored group. Missing results are not zero scores.
        let score = if availability == "available" && complete {
            totals
                .and_then(|value| finite(value.get("score_150")))
                .filter(|value| (0.0..=score_max).contains(value))
        } else {
            None
        };
        let results = round.get("results").and_then(|value| value.get(&key));
        let animation = round.get("animations").and_then(|value| value.get(&key));
        let mut seconds = Vec::new();
        let mut costs = Vec::new();
        for task in tasks {
            let result = task
                .get("id")
                .and_then(Value::as_str)
                .and_then(|id| results.and_then(|value| value.get(id)));
            if result.is_some_and(|value| {
                value.get("status").and_then(Value::as_str) == Some("scored")
                    && finite(value.get("reward"))
                        .is_some_and(|reward| (0.0..=1.0).contains(&reward))
            }) {
                if let Some(value) = result
                    .and_then(|value| finite(value.get("seconds")))
                    .filter(|v| *v >= 0.0)
                {
                    seconds.push(value);
                }
            }
            costs.push(result.and_then(|value| value.get("cost")));
        }
        // The scoreboard includes animation generation, not its separate judge.
        if animation
            .is_some_and(|value| value.get("status").and_then(Value::as_str) == Some("available"))
        {
            if let Some(value) = animation
                .and_then(|value| finite(value.get("seconds")))
                .filter(|v| *v >= 0.0)
            {
                seconds.push(value);
            }
        }
        costs.push(animation.and_then(|value| value.get("cost")));
        let (cost_usd, cost_kind, cost_known) = summarize_costs(&costs);
        models.push(RadarModel {
            id: format!("{model}-{effort}"),
            model_label: friendly_model_name(&model),
            model,
            effort,
            score,
            score_max,
            status: match availability {
                "unavailable" => "unavailable",
                "available" if score.is_some() => "scored",
                "available" if items_scored > 0 => "incomplete",
                "available" => "not_started",
                _ => "pending",
            }
            .into(),
            items_scored,
            items_expected,
            wall_time: (!seconds.is_empty()).then(|| {
                format!(
                    "{:.1}分钟",
                    seconds.iter().sum::<f64>() / seconds.len() as f64 / 60.0
                )
            }),
            cost_usd,
            cost_kind,
            cost_known,
            cost_total: costs.len(),
            community_score: None,
            community_votes: None,
        });
    }
    if models.is_empty() {
        return Err("Codex Radar 暂无可显示的模型档位".into());
    }
    Ok(RadarSnapshot {
        models,
        score_max: Some(score_max),
        status: text(round, "status"),
        batch: text(round, "id"),
        updated_at: text(round, "updated_at"),
        fetched_at: Some(fetched_at),
        source: "degradation_codex".into(),
        attribution: "数据来自 Codex 雷达 codexradar.com".into(),
        site_url: "https://codexradar.com/#stationmaster-round".into(),
        cached: false,
        error: None,
    })
}

fn summarize_costs(costs: &[Option<&Value>]) -> (Option<f64>, String, usize) {
    let mut sum = 0.0;
    let mut known = 0;
    let mut kinds = HashSet::new();
    for cost in costs.iter().flatten() {
        let kind = cost
            .get("kind")
            .and_then(Value::as_str)
            .unwrap_or("unknown");
        if !["actual", "estimated"].contains(&kind)
            || cost.get("includes_subagents").and_then(Value::as_bool) != Some(true)
        {
            continue;
        }
        if let Some(value) = finite(cost.get("usd")).filter(|v| *v >= 0.0) {
            sum += value;
            known += 1;
            kinds.insert(kind);
        }
    }
    let kind = if kinds.len() > 1 {
        "mixed"
    } else {
        kinds.into_iter().next().unwrap_or("unknown")
    };
    ((known > 0).then_some(sum), kind.into(), known)
}

fn friendly_model_name(model: &str) -> String {
    let mut parts = model.split('-');
    let first = parts.next().unwrap_or(model);
    let mut label = match first {
        "gpt" => format!("GPT-{}", parts.next().unwrap_or_default()),
        "deepseek" => "DeepSeek".into(),
        other => title_word(other),
    };
    for part in parts {
        label.push(' ');
        label.push_str(&title_word(part));
    }
    label
}
fn title_word(word: &str) -> String {
    let mut chars = word.chars();
    chars
        .next()
        .map(|first| first.to_uppercase().to_string() + chars.as_str())
        .unwrap_or_default()
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
        if let Some(rating) = ratings.get(&model.id) {
            model.community_score = rating
                .average
                .filter(|v| v.is_finite() && (0.0..=10.0).contains(v));
            model.community_votes = rating.count;
        }
    }
}
fn finite(value: Option<&Value>) -> Option<f64> {
    value.and_then(Value::as_f64).filter(|v| v.is_finite())
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
    snapshot.cached = !snapshot.models.is_empty();
    snapshot.error = Some(error);
    snapshot
}
fn safe_error(error: &reqwest::Error) -> String {
    if error.is_timeout() {
        "Codex Radar 请求超时".into()
    } else {
        "无法读取 Codex Radar 综合分".into()
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
    use serde_json::json;

    fn fixture() -> Value {
        json!({
            "schema":"degradation-codex-plan/3", "taskset":{"harness":"codex","version":"round-v3"},
            "scoring_policy":{"question_count":10,"total_max":150},
            "settings":[
                {"model":"gpt-6-astra","effort":"ultra","availability":{"status":"available"}},
                {"model":"gpt-6.1-sol","effort":"max","availability":{"status":"available"}},
                {"model":"gpt-6-luna","effort":"max","availability":{"status":"available"}}
            ], "tasks":[{"id":"a"},{"id":"b"}],
            "round":{"id":"test-round","taskset_version":"round-v3","status":"running","updated_at":"2026-10-25T09:00:00+08:00",
                "totals":{"gpt-6-astra|ultra":{"complete":true,"items_scored":10,"items_expected":10,"score_max":150,"score_150":148.0285714285714},
                    "gpt-6.1-sol|max":{"complete":false,"items_scored":9,"items_expected":10,"score_max":150,"score_150":null}},
                "results":{"gpt-6-astra|ultra":{
                    "a":{"status":"scored","reward":1,"seconds":60,"cost":{"usd":2,"kind":"estimated","includes_subagents":true}},
                    "b":{"status":"scored","reward":0.8,"seconds":120,"cost":{"usd":3,"kind":"actual","includes_subagents":true}}
                }},
                "animations":{"gpt-6-astra|ultra":{"status":"available","seconds":180,"cost":{"usd":4,"kind":"estimated","includes_subagents":true},
                    "judge":{"seconds":9999,"cost":{"usd":999,"kind":"estimated","includes_subagents":true}}}}
            }
        })
    }
    #[test]
    fn uses_published_composite_score_and_keeps_site_family_order() {
        let snapshot = parse_round(&fixture(), 42).unwrap();
        assert_eq!(
            snapshot
                .models
                .iter()
                .map(|model| model.model.as_str())
                .collect::<Vec<_>>(),
            ["gpt-6-astra", "gpt-6.1-sol", "gpt-6-luna"]
        );
        assert_eq!(snapshot.models[0].model_label, "GPT-6 Astra");
        assert_eq!(snapshot.models[0].score, Some(148.0285714285714));
        assert_eq!(snapshot.models[1].score, None);
        assert_eq!(snapshot.models[2].score, None);
        assert_eq!(snapshot.fetched_at, Some(42));
    }
    #[test]
    fn matches_round_cost_and_average_time_without_the_separate_judge() {
        let model = parse_round(&fixture(), 42).unwrap().models.remove(0);
        assert_eq!(model.cost_usd, Some(9.0));
        assert_eq!(model.cost_kind, "mixed");
        assert_eq!(model.cost_known, 3);
        assert_eq!(model.wall_time.as_deref(), Some("2.0分钟"));
    }
    #[test]
    fn never_turns_partial_scores_or_unknown_costs_into_zero() {
        let mut value = fixture();
        value["round"]["totals"]["gpt-6.1-sol|max"]["score_150"] = json!(120);
        let models = parse_round(&value, 42).unwrap().models;
        assert_eq!(models[1].score, None);
        assert_eq!(models[1].status, "incomplete");
        assert_eq!(models[2].cost_usd, None);
        assert_eq!(models[2].wall_time, None);
    }
    #[test]
    fn preserves_a_legitimate_zero_score() {
        let mut value = fixture();
        value["round"]["totals"]["gpt-6-astra|ultra"]["score_150"] = json!(0);
        assert_eq!(parse_round(&value, 42).unwrap().models[0].score, Some(0.0));
    }
    #[test]
    fn supports_new_models_and_omits_explicitly_unsupported_efforts() {
        let mut value = fixture();
        value["settings"] = json!([
            {"model":"gpt-7.2-future","effort":"new-mode","availability":{"status":"pending"}},
            {"model":"gpt-6-luna","effort":"ultra","availability":{"status":"unavailable","reason":"unsupported_effort"}},
            {"model":"gpt-6-luna","effort":"max","availability":{"status":"unavailable","reason":"service_unavailable"}}
        ]);
        let models = parse_round(&value, 42).unwrap().models;
        assert_eq!(models.len(), 2);
        assert_eq!(models[0].model_label, "GPT-7.2 Future");
        assert_eq!(models[0].effort, "new-mode");
        assert_eq!(models[1].effort, "max");
        assert_eq!(models[1].status, "unavailable");
    }
    #[test]
    fn joins_community_ratings_by_exact_model_and_effort() {
        let mut snapshot = parse_round(&fixture(), 42).unwrap();
        apply_ratings(
            &mut snapshot,
            &json!({"models":[{"id":"gpt-6-astra-ultra","average":8.5,"count":115}]}),
        );
        assert_eq!(snapshot.models[0].community_score, Some(8.5));
        assert_eq!(snapshot.models[1].community_score, None);
    }
    #[test]
    fn rejects_old_iq_sources_and_keeps_the_last_successful_round() {
        assert!(parse_round(&json!({"points":[{"iq":140}]}), 42).is_err());
        let mut value = fixture();
        value["round"]["taskset_version"] = json!("other-round");
        assert!(parse_round(&value, 42).is_err());
        let snapshot = parse_round(&fixture(), 42).unwrap();
        let cached = merge_failure(&snapshot, "offline".into());
        assert!(cached.cached);
        assert_eq!(cached.models[0].score, snapshot.models[0].score);
    }
    #[test]
    #[ignore = "explicit verification against a downloaded public website snapshot"]
    fn verifies_current_public_snapshot_fixture() {
        let path = std::env::var("CQB_RADAR_FIXTURE")
            .expect("set CQB_RADAR_FIXTURE to a downloaded public snapshot");
        let value: Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        let mut snapshot = parse_round(&value, 42).unwrap();
        assert_eq!(
            snapshot.models.len(),
            value["settings"].as_array().unwrap().len()
        );
        for model in &snapshot.models {
            let key = format!("{}|{}", model.model, model.effort);
            assert_eq!(
                model.score,
                value["round"]["totals"][key]["score_150"].as_f64()
            );
        }
        if let Ok(path) = std::env::var("CQB_RADAR_RATINGS_FIXTURE") {
            let ratings: Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
            apply_ratings(&mut snapshot, &ratings);
        }
        if let Ok(path) = std::env::var("CQB_RADAR_OUTPUT") {
            std::fs::write(path, serde_json::to_vec_pretty(&snapshot).unwrap()).unwrap();
        }
    }
}
