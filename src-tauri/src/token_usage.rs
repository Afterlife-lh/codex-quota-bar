use chrono::{DateTime, Duration, Local};
use rusqlite::{Connection, OpenFlags};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const CACHE_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct TokenUsageSnapshot {
    pub today_tokens: u64,
    pub week_tokens: u64,
    pub month_tokens: u64,
    pub total_tokens: u64,
    pub daily_trend: Vec<DailyTokenUsage>,
    pub conversations: Vec<ConversationUsage>,
    pub projects: Vec<ProjectUsage>,
    pub scanned_at: i64,
    pub source_files: usize,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DailyTokenUsage {
    pub date: String,
    pub label: String,
    pub tokens: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversationUsage {
    pub id: String,
    pub title: String,
    pub project: String,
    pub project_path: String,
    pub total_tokens: u64,
    pub updated_at: i64,
    pub archived: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectUsage {
    pub name: String,
    pub path: String,
    pub total_tokens: u64,
    pub conversation_count: usize,
    pub updated_at: i64,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct UsageCache {
    version: u32,
    files: HashMap<String, CachedRollout>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct CachedRollout {
    length: u64,
    modified_ms: u64,
    offset: u64,
    session_id: Option<String>,
    cwd: Option<String>,
    forked_from_id: Option<String>,
    last_total: Option<u64>,
    last_event_ms: i64,
    daily_tokens: BTreeMap<String, u64>,
}

pub fn scan(codex_home: &Path, cache_path: &Path) -> Result<TokenUsageSnapshot, String> {
    let mut cache = load_cache(cache_path);
    let candidates = rollout_files(codex_home);
    let live_paths = candidates
        .iter()
        .map(|path| path.to_string_lossy().to_string())
        .collect::<HashSet<_>>();
    cache.files.retain(|path, _| live_paths.contains(path));

    for path in &candidates {
        let key = path.to_string_lossy().to_string();
        let metadata = match fs::metadata(path) {
            Ok(value) => value,
            Err(_) => continue,
        };
        let length = metadata.len();
        let modified_ms = modified_millis(&metadata);
        let entry = cache.files.entry(key).or_default();
        if entry.offset > length || (entry.length == length && entry.modified_ms != modified_ms) {
            *entry = CachedRollout::default();
        }
        if entry.length != length || entry.modified_ms != modified_ms {
            scan_rollout(path, entry)?;
            entry.length = length;
            entry.modified_ms = modified_ms;
        }
    }

    save_cache(cache_path, &cache)?;
    let canonical = canonical_sessions(&cache.files);
    let (today_tokens, week_tokens, month_tokens) = period_totals(&canonical);
    let daily_trend = seven_day_trend(&canonical);
    let (conversations, database_error) = load_conversations(codex_home, &canonical);
    let total_tokens = conversations.iter().map(|item| item.total_tokens).sum();
    let projects = aggregate_projects(&conversations);

    Ok(TokenUsageSnapshot {
        today_tokens,
        week_tokens,
        month_tokens,
        total_tokens,
        daily_trend,
        conversations,
        projects,
        scanned_at: now_millis(),
        source_files: candidates.len(),
        error: database_error,
    })
}

fn rollout_files(codex_home: &Path) -> Vec<PathBuf> {
    let mut result = Vec::new();
    for directory in ["sessions", "archived_sessions"] {
        collect_jsonl(&codex_home.join(directory), &mut result);
    }
    result.sort();
    result
}

fn collect_jsonl(directory: &Path, result: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(directory) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_jsonl(&path, result);
        } else if path
            .extension()
            .is_some_and(|value| value.eq_ignore_ascii_case("jsonl"))
        {
            result.push(path);
        }
    }
}

fn scan_rollout(path: &Path, entry: &mut CachedRollout) -> Result<(), String> {
    let file = open_shared(path).map_err(|error| format!("无法读取 Codex 会话记录: {error}"))?;
    let mut reader = BufReader::new(file);
    reader
        .seek(SeekFrom::Start(entry.offset))
        .map_err(|error| format!("无法定位 Codex 会话记录: {error}"))?;
    let mut committed_offset = entry.offset;
    loop {
        let mut line = Vec::new();
        let read = reader
            .read_until(b'\n', &mut line)
            .map_err(|error| format!("无法扫描 Codex 会话记录: {error}"))?;
        if read == 0 {
            break;
        }
        if !line.ends_with(b"\n") {
            break;
        }
        committed_offset += read as u64;
        if let Ok(value) = serde_json::from_slice::<Value>(&line) {
            apply_record(entry, &value);
        }
    }
    entry.offset = committed_offset;
    Ok(())
}

#[cfg(windows)]
fn open_shared(path: &Path) -> std::io::Result<File> {
    use std::os::windows::fs::OpenOptionsExt;
    OpenOptions::new()
        .read(true)
        .share_mode(0x0000_0001 | 0x0000_0002 | 0x0000_0004)
        .open(path)
}

#[cfg(not(windows))]
fn open_shared(path: &Path) -> std::io::Result<File> {
    OpenOptions::new().read(true).open(path)
}

fn apply_record(entry: &mut CachedRollout, value: &Value) {
    match value.get("type").and_then(Value::as_str) {
        Some("session_meta") => {
            let Some(payload) = value.get("payload") else {
                return;
            };
            if entry.session_id.is_none() {
                entry.session_id = text(payload, "session_id").or_else(|| text(payload, "id"));
                entry.cwd = text(payload, "cwd");
                entry.forked_from_id = text(payload, "forked_from_id");
            }
        }
        Some("event_msg")
            if value.pointer("/payload/type").and_then(Value::as_str) == Some("token_count") =>
        {
            let Some(total) = value
                .pointer("/payload/info/total_token_usage/total_tokens")
                .and_then(Value::as_u64)
            else {
                return;
            };
            let delta = match entry.last_total {
                Some(previous) if total >= previous => total - previous,
                Some(_) => 0,
                None if entry.forked_from_id.is_some() => 0,
                None => total,
            };
            entry.last_total = Some(total);
            let Some(timestamp) = value.get("timestamp").and_then(Value::as_str) else {
                return;
            };
            let Ok(timestamp) = DateTime::parse_from_rfc3339(timestamp) else {
                return;
            };
            let local = timestamp.with_timezone(&Local);
            entry.last_event_ms = local.timestamp_millis();
            if delta > 0 {
                *entry
                    .daily_tokens
                    .entry(local.date_naive().format("%Y-%m-%d").to_string())
                    .or_default() += delta;
            }
        }
        _ => {}
    }
}

fn canonical_sessions<'a>(files: &'a HashMap<String, CachedRollout>) -> Vec<&'a CachedRollout> {
    let mut sessions = HashMap::<String, &CachedRollout>::new();
    for rollout in files.values() {
        let Some(id) = rollout.session_id.as_ref() else {
            continue;
        };
        let replace = sessions.get(id).is_none_or(|existing| {
            rollout.last_total.unwrap_or(0) > existing.last_total.unwrap_or(0)
                || (rollout.last_total == existing.last_total
                    && rollout.last_event_ms > existing.last_event_ms)
        });
        if replace {
            sessions.insert(id.clone(), rollout);
        }
    }
    sessions.into_values().collect()
}

fn period_totals(sessions: &[&CachedRollout]) -> (u64, u64, u64) {
    let today = Local::now().date_naive();
    let week_start = today - Duration::days(6);
    let month_start = today - Duration::days(29);
    let mut day = 0_u64;
    let mut week = 0_u64;
    let mut month = 0_u64;
    for session in sessions {
        if session.forked_from_id.is_some() {
            continue;
        }
        for (key, tokens) in &session.daily_tokens {
            let Ok(date) = chrono::NaiveDate::parse_from_str(key, "%Y-%m-%d") else {
                continue;
            };
            if date == today {
                day = day.saturating_add(*tokens);
            }
            if date >= week_start && date <= today {
                week = week.saturating_add(*tokens);
            }
            if date >= month_start && date <= today {
                month = month.saturating_add(*tokens);
            }
        }
    }
    (day, week, month)
}

fn seven_day_trend(sessions: &[&CachedRollout]) -> Vec<DailyTokenUsage> {
    let today = Local::now().date_naive();
    (0..7)
        .rev()
        .map(|days_ago| {
            let date = today - Duration::days(days_ago);
            let key = date.format("%Y-%m-%d").to_string();
            let tokens = sessions
                .iter()
                .filter(|session| session.forked_from_id.is_none())
                .map(|session| session.daily_tokens.get(&key).copied().unwrap_or(0))
                .sum();
            DailyTokenUsage {
                date: key,
                label: date.format("%m/%d").to_string(),
                tokens,
            }
        })
        .collect()
}

fn load_conversations(
    codex_home: &Path,
    sessions: &[&CachedRollout],
) -> (Vec<ConversationUsage>, Option<String>) {
    let Some(database) = latest_state_database(codex_home) else {
        return (
            fallback_conversations(sessions),
            Some("未找到 Codex 状态数据库；会话名称暂以本地记录 ID 显示。".into()),
        );
    };
    let session_names = load_session_names(&codex_home.join("session_index.jsonl"));
    match query_conversations(&database, &session_names) {
        Ok(rows) => (rows, None),
        Err(error) => (
            fallback_conversations(sessions),
            Some(format!("Codex 状态数据库暂不可用：{error}")),
        ),
    }
}

fn latest_state_database(codex_home: &Path) -> Option<PathBuf> {
    fs::read_dir(codex_home)
        .ok()?
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|value| value.to_str())
                .is_some_and(|name| name.starts_with("state_") && name.ends_with(".sqlite"))
        })
        .max_by_key(|path| fs::metadata(path).and_then(|value| value.modified()).ok())
}

fn query_conversations(
    database: &Path,
    session_names: &HashMap<String, String>,
) -> Result<Vec<ConversationUsage>, String> {
    let connection = Connection::open_with_flags(
        database,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|error| error.to_string())?;
    connection
        .busy_timeout(std::time::Duration::from_millis(800))
        .ok();
    let mut statement = connection
        .prepare(
            "SELECT id, COALESCE(NULLIF(name, ''), NULLIF(title, ''), NULLIF(first_user_message, ''), id), \
             cwd, MAX(tokens_used, 0), COALESCE(updated_at_ms, updated_at * 1000), archived \
             FROM threads WHERE (agent_role IS NULL OR agent_role = '') AND tokens_used > 0 \
             ORDER BY tokens_used DESC, updated_at DESC",
        )
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map([], |row| {
            let path = normalize_path(&row.get::<_, String>(2)?);
            Ok(ConversationUsage {
                id: row.get(0)?,
                title: row.get(1)?,
                project: project_name(&path),
                project_path: path,
                total_tokens: row.get::<_, i64>(3)?.max(0) as u64,
                updated_at: row.get(4)?,
                archived: row.get::<_, i64>(5)? != 0,
            })
        })
        .map_err(|error| error.to_string())?;
    let mut conversations = rows
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;
    for conversation in &mut conversations {
        if let Some(name) = session_names.get(&conversation.id) {
            conversation.title = name.clone();
        }
    }
    Ok(conversations)
}

fn load_session_names(path: &Path) -> HashMap<String, String> {
    let Ok(file) = open_shared(path) else {
        return HashMap::new();
    };
    BufReader::new(file)
        .lines()
        .map_while(Result::ok)
        .filter_map(|line| serde_json::from_str::<Value>(&line).ok())
        .filter_map(|value| {
            Some((
                text(&value, "id")?,
                text(&value, "thread_name")?.trim().to_string(),
            ))
        })
        .filter(|(_, name)| !name.is_empty())
        .collect()
}

fn fallback_conversations(sessions: &[&CachedRollout]) -> Vec<ConversationUsage> {
    let mut rows = sessions
        .iter()
        .filter(|session| session.forked_from_id.is_none())
        .filter_map(|session| {
            let id = session.session_id.clone()?;
            let path = normalize_path(session.cwd.as_deref().unwrap_or("未知项目"));
            Some(ConversationUsage {
                title: format!("会话 {}", id.chars().take(8).collect::<String>()),
                project: project_name(&path),
                project_path: path,
                total_tokens: session.last_total.unwrap_or(0),
                updated_at: session.last_event_ms,
                archived: false,
                id,
            })
        })
        .collect::<Vec<_>>();
    rows.sort_by_key(|row| std::cmp::Reverse(row.total_tokens));
    rows
}

fn aggregate_projects(conversations: &[ConversationUsage]) -> Vec<ProjectUsage> {
    let mut grouped = HashMap::<String, ProjectUsage>::new();
    for conversation in conversations {
        let item = grouped
            .entry(conversation.project_path.to_ascii_lowercase())
            .or_insert_with(|| ProjectUsage {
                name: conversation.project.clone(),
                path: conversation.project_path.clone(),
                total_tokens: 0,
                conversation_count: 0,
                updated_at: 0,
            });
        item.total_tokens = item.total_tokens.saturating_add(conversation.total_tokens);
        item.conversation_count += 1;
        item.updated_at = item.updated_at.max(conversation.updated_at);
    }
    let mut rows = grouped.into_values().collect::<Vec<_>>();
    rows.sort_by_key(|row| std::cmp::Reverse(row.total_tokens));
    rows
}

fn project_name(path: &str) -> String {
    Path::new(path)
        .file_name()
        .and_then(|value| value.to_str())
        .filter(|value| !value.is_empty())
        .unwrap_or(path)
        .to_string()
}

fn normalize_path(path: &str) -> String {
    path.strip_prefix(r"\\?\").unwrap_or(path).to_string()
}

fn load_cache(path: &Path) -> UsageCache {
    fs::read(path)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<UsageCache>(&bytes).ok())
        .filter(|cache| cache.version == CACHE_VERSION)
        .unwrap_or_else(|| UsageCache {
            version: CACHE_VERSION,
            files: HashMap::new(),
        })
}

fn save_cache(path: &Path, cache: &UsageCache) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| format!("无法创建 Token 缓存目录: {error}"))?;
    }
    let temporary = path.with_extension("json.tmp");
    let bytes =
        serde_json::to_vec(cache).map_err(|error| format!("无法生成 Token 缓存: {error}"))?;
    fs::write(&temporary, bytes).map_err(|error| format!("无法写入 Token 缓存: {error}"))?;
    if path.exists() {
        fs::remove_file(path).map_err(|error| format!("无法更新 Token 缓存: {error}"))?;
    }
    fs::rename(temporary, path).map_err(|error| format!("无法提交 Token 缓存: {error}"))
}

fn text(value: &Value, key: &str) -> Option<String> {
    value.get(key)?.as_str().map(str::to_string)
}

fn modified_millis(metadata: &fs::Metadata) -> u64 {
    metadata
        .modified()
        .ok()
        .and_then(|value| value.duration_since(UNIX_EPOCH).ok())
        .map(|value| value.as_millis() as u64)
        .unwrap_or(0)
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

    fn token_record(timestamp: &str, total: u64) -> Value {
        serde_json::json!({
            "timestamp": timestamp,
            "type": "event_msg",
            "payload": {"type": "token_count", "info": {"total_token_usage": {"total_tokens": total}}}
        })
    }

    #[test]
    fn counts_only_positive_cumulative_deltas() {
        let mut rollout = CachedRollout::default();
        apply_record(&mut rollout, &token_record("2026-08-05T01:00:00Z", 100));
        apply_record(&mut rollout, &token_record("2026-08-05T01:01:00Z", 160));
        apply_record(&mut rollout, &token_record("2026-08-05T01:02:00Z", 20));
        assert_eq!(rollout.daily_tokens.values().sum::<u64>(), 160);
        assert_eq!(rollout.last_total, Some(20));
    }

    #[test]
    fn forked_rollout_uses_first_total_as_inherited_baseline() {
        let mut rollout = CachedRollout {
            forked_from_id: Some("parent".into()),
            ..CachedRollout::default()
        };
        apply_record(&mut rollout, &token_record("2026-08-05T01:00:00Z", 1_000));
        apply_record(&mut rollout, &token_record("2026-08-05T01:01:00Z", 1_120));
        assert_eq!(rollout.daily_tokens.values().sum::<u64>(), 120);
    }

    #[test]
    fn groups_conversations_by_normalized_project() {
        let conversations = vec![
            ConversationUsage {
                id: "1".into(),
                title: "A".into(),
                project: "demo".into(),
                project_path: r"C:\demo".into(),
                total_tokens: 10,
                updated_at: 1,
                archived: false,
            },
            ConversationUsage {
                id: "2".into(),
                title: "B".into(),
                project: "demo".into(),
                project_path: r"c:\DEMO".into(),
                total_tokens: 15,
                updated_at: 2,
                archived: false,
            },
        ];
        let projects = aggregate_projects(&conversations);
        assert_eq!(projects.len(), 1);
        assert_eq!(projects[0].total_tokens, 25);
        assert_eq!(projects[0].conversation_count, 2);
    }

    #[test]
    fn trend_always_contains_seven_natural_days_and_ignores_forks() {
        let today = Local::now().date_naive();
        let yesterday = today - Duration::days(1);
        let mut parent = CachedRollout::default();
        parent
            .daily_tokens
            .insert(today.format("%Y-%m-%d").to_string(), 120);
        parent
            .daily_tokens
            .insert(yesterday.format("%Y-%m-%d").to_string(), 80);
        let mut fork = CachedRollout {
            forked_from_id: Some("parent".into()),
            ..CachedRollout::default()
        };
        fork.daily_tokens
            .insert(today.format("%Y-%m-%d").to_string(), 999);

        let trend = seven_day_trend(&[&parent, &fork]);
        assert_eq!(trend.len(), 7);
        assert_eq!(trend[5].tokens, 80);
        assert_eq!(trend[6].tokens, 120);
        assert_eq!(trend[6].date, today.format("%Y-%m-%d").to_string());
    }

    #[test]
    fn session_index_uses_latest_generated_name_for_each_thread() {
        let path = std::env::temp_dir().join(format!(
            "codex-quota-session-index-{}.jsonl",
            std::process::id()
        ));
        fs::write(
            &path,
            concat!(
                "{\"id\":\"thread-a\",\"thread_name\":\"旧名称\"}\n",
                "{\"id\":\"thread-b\",\"thread_name\":\"另一个对话\"}\n",
                "{\"id\":\"thread-a\",\"thread_name\":\"Codex 最新名称\"}\n"
            ),
        )
        .unwrap();

        let names = load_session_names(&path);
        let _ = fs::remove_file(path);
        assert_eq!(
            names.get("thread-a").map(String::as_str),
            Some("Codex 最新名称")
        );
        assert_eq!(
            names.get("thread-b").map(String::as_str),
            Some("另一个对话")
        );
    }
}
