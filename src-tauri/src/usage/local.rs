//! Read-only local statistics. Invalid and partially written JSONL lines are skipped.
use super::{Day, Model, Today, UsageRecord, UsageStatus};
use chrono::{DateTime, Local, NaiveDate, TimeDelta, Utc};
use serde_json::Value;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs::{self, File};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

pub(super) fn claude_dir() -> PathBuf {
    std::env::var_os("CLAUDE_CONFIG_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| crate::env::home().join(".claude"))
}

pub(super) fn recent_files(root: &Path) -> Vec<PathBuf> {
    fn walk(root: &Path, out: &mut Vec<PathBuf>) {
        let Ok(entries) = fs::read_dir(root) else {
            return;
        };
        for entry in entries.flatten() {
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            if kind.is_dir() {
                walk(&entry.path(), out);
            } else if kind.is_file() && entry.path().extension().is_some_and(|e| e == "jsonl") {
                if entry
                    .metadata()
                    .and_then(|m| m.modified())
                    .ok()
                    .is_some_and(|at| {
                        SystemTime::now().duration_since(at).unwrap_or_default()
                            <= Duration::from_secs(8 * 86400)
                    })
                {
                    out.push(entry.path());
                }
            }
        }
    }
    let mut out = vec![];
    walk(root, &mut out);
    out.sort();
    out
}
fn lines(path: &Path, mut f: impl FnMut(usize, Value)) {
    let Ok(file) = File::open(path) else { return };
    for (i, line) in BufReader::new(file).lines().enumerate() {
        if let Ok(line) = line {
            if let Ok(v) = serde_json::from_str(&line) {
                f(i, v);
            }
        }
    }
}
fn day(v: &Value) -> Option<NaiveDate> {
    if let Some(s) = v.as_str() {
        return DateTime::parse_from_rfc3339(s)
            .ok()
            .map(|t| t.with_timezone(&Local).date_naive());
    }
    let n = v.as_i64()?;
    let t = if n > 10_000_000_000 {
        DateTime::<Utc>::from_timestamp_millis(n)
    } else {
        DateTime::<Utc>::from_timestamp(n, 0)
    }?;
    Some(t.with_timezone(&Local).date_naive())
}
fn num(v: &Value) -> u64 {
    v.as_u64().unwrap_or(0)
}

struct Stats {
    today: NaiveDate,
    days: BTreeMap<NaiveDate, u64>,
    models: HashMap<String, u64>,
    sessions: HashSet<String>,
    prompts: u64,
}
impl Stats {
    fn new(today: NaiveDate) -> Self {
        Self {
            today,
            days: BTreeMap::new(),
            models: HashMap::new(),
            sessions: HashSet::new(),
            prompts: 0,
        }
    }
    fn add(&mut self, date: Option<NaiveDate>, tokens: u64, model: &str, session: &str) {
        let Some(date) = date else { return };
        if date > self.today || date < self.today - TimeDelta::days(6) || tokens == 0 {
            return;
        }
        *self.days.entry(date).or_default() += tokens;
        *self.models.entry(model.into()).or_default() += tokens;
        if date == self.today {
            self.prompts += 1;
            self.sessions.insert(session.into());
        }
    }
    fn apply(self, r: &mut UsageRecord) {
        if self.days.is_empty() {
            return;
        }
        r.today = Today {
            tokens: *self.days.get(&self.today).unwrap_or(&0),
            prompts: Some(self.prompts),
            sessions: Some(self.sessions.len() as u64),
        };
        r.days = (0..7)
            .rev()
            .map(|offset| {
                let date = self.today - TimeDelta::days(offset);
                Day {
                    date: date.to_string(),
                    tokens: *self.days.get(&date).unwrap_or(&0),
                }
            })
            .collect();
        r.models = self
            .models
            .into_iter()
            .map(|(model, tokens)| Model { model, tokens })
            .collect();
        r.models
            .sort_by(|a, b| b.tokens.cmp(&a.tokens).then(a.model.cmp(&b.model)));
        r.status = UsageStatus::Ok;
        r.status_text = None;
    }
}

pub(super) fn collect(r: &mut UsageRecord) {
    if r.id == "opencode" {
        opencode(&opencode_db(), Local::now().date_naive()).apply(r);
    }
    if r.id == "codex" {
        codex(
            &recent_files(&codex_dir().join("sessions")),
            Local::now().date_naive(),
        )
        .apply(r);
    }
    if r.id == "claude" {
        claude(
            &recent_files(&claude_dir().join("projects")),
            Local::now().date_naive(),
        )
        .apply(r);
    }
}
fn claude(files: &[PathBuf], today: NaiveDate) -> Stats {
    let mut stats = Stats::new(today);
    let mut seen = HashSet::new();
    for path in files {
        lines(path, |i, v| {
            if v["type"] != "assistant" {
                return;
            }
            let m = &v["message"];
            let Some(u) = m.get("usage").filter(|u| u.is_object()) else {
                return;
            };
            let key = if let Some(id) = m["id"].as_str() {
                (
                    id.to_owned(),
                    v["requestId"].as_str().unwrap_or("").to_owned(),
                )
            } else {
                (path.to_string_lossy().into_owned(), i.to_string())
            };
            if !seen.insert(key) {
                return;
            }
            let tokens = [
                "input_tokens",
                "output_tokens",
                "cache_creation_input_tokens",
                "cache_read_input_tokens",
            ]
            .iter()
            .map(|k| num(&u[*k]))
            .sum();
            stats.add(
                day(&v["timestamp"]),
                tokens,
                m["model"].as_str().unwrap_or("claude"),
                v["sessionId"]
                    .as_str()
                    .unwrap_or(path.to_str().unwrap_or("session")),
            );
        });
    }
    stats
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn claude_fixture() {
        let path =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/usage/claude.jsonl");
        let stats = claude(&[path], NaiveDate::from_ymd_opt(2026, 11, 2).unwrap());
        let mut r = UsageRecord::empty("claude", "Claude");
        stats.apply(&mut r);
        assert_eq!(r.today.tokens, 145);
        assert_eq!(r.today.sessions, Some(1));
        assert_eq!(r.today.prompts, Some(2));
        assert_eq!(r.models[0].tokens, 145);
        assert_eq!(r.days.len(), 7);
        assert_eq!(r.days.iter().map(|d| d.tokens).sum::<u64>(), 155);
    }
}
pub(super) fn codex_dir() -> PathBuf {
    std::env::var_os("CODEX_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| crate::env::home().join(".codex"))
}
fn codex(files: &[PathBuf], today: NaiveDate) -> Stats {
    let mut stats = Stats::new(today);
    for path in files {
        if !path
            .file_name()
            .is_some_and(|n| n.to_string_lossy().starts_with("rollout-"))
        {
            continue;
        }
        let mut model = "codex".to_string();
        let mut previous = 0;
        let mut last_without_total = None;
        lines(path, |_, v| {
            let p = &v["payload"];
            if v["type"] == "turn_context" {
                if let Some(m) = p["model"].as_str().or(p["model_slug"].as_str()) {
                    model = m.into();
                }
            }
            let p = if v["type"] == "response_item" {
                p.get("payload").unwrap_or(p)
            } else {
                p
            };
            if p["type"] != "token_count" {
                return;
            }
            let info = &p["info"];
            let usage_total = |u: &Value| {
                u["total_tokens"]
                    .as_u64()
                    .unwrap_or_else(|| num(&u["input_tokens"]) + num(&u["output_tokens"]))
            };
            let tokens = if info["total_token_usage"].is_object() {
                let total = usage_total(&info["total_token_usage"]);
                let delta = if total >= previous {
                    total - previous
                } else {
                    usage_total(&info["last_token_usage"])
                };
                previous = total;
                delta
            } else {
                let fingerprint = (v["timestamp"].clone(), info["last_token_usage"].clone());
                if last_without_total.as_ref() == Some(&fingerprint) {
                    return;
                }
                last_without_total = Some(fingerprint);
                usage_total(&info["last_token_usage"])
            };
            stats.add(
                day(&v["timestamp"]),
                tokens,
                &model,
                path.to_str().unwrap_or("session"),
            );
        });
    }
    stats
}

#[cfg(test)]
#[test]
fn codex_fixture() {
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/usage/rollout-codex.jsonl");
    let mut r = UsageRecord::empty("codex", "Codex");
    codex(&[path], NaiveDate::from_ymd_opt(2026, 11, 2).unwrap()).apply(&mut r);
    assert_eq!(r.today.tokens, 200);
    assert_eq!(r.today.prompts, Some(3));
    assert_eq!(r.models[0].tokens, 150);
    assert_eq!(r.models[1].tokens, 50);
}
pub(super) fn opencode_db() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| crate::env::home().join(".local/share"))
        .join("opencode/opencode.db")
}
pub(super) fn open_db(path: &Path) -> rusqlite::Result<rusqlite::Connection> {
    let conn =
        rusqlite::Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    conn.busy_timeout(Duration::from_millis(50))?;
    Ok(conn)
}
fn opencode(path: &Path, today: NaiveDate) -> Stats {
    let mut stats = Stats::new(today);
    let Ok(conn) = open_db(path) else {
        return stats;
    };
    let mut seen = HashSet::new();
    let start = today - TimeDelta::days(6);
    let Some(start_ms) = start
        .and_hms_opt(0, 0, 0)
        .and_then(|d| d.and_local_timezone(Local).earliest())
        .map(|d| d.timestamp_millis())
    else {
        return stats;
    };
    for (table, role, model) in [
        ("session_message", "type = 'assistant'", "$.model.id"),
        (
            "message",
            "CASE WHEN json_valid(data) THEN json_extract(data, '$.role') END = 'assistant'",
            "$.modelID",
        ),
    ] {
        let extract =
            |key: &str| format!("CASE WHEN json_valid(data) THEN json_extract(data, '{key}') END");
        let columns = [
            "$.time.created",
            model,
            "$.tokens.input",
            "$.tokens.output",
            "$.tokens.reasoning",
            "$.tokens.cache.read",
            "$.tokens.cache.write",
        ]
        .map(extract);
        let sql = format!("SELECT id, session_id, COALESCE({}, time_created), {}, {}, {}, {}, {}, {} FROM {table} WHERE {role} AND COALESCE({}, time_created) >= ?1",
            columns[0],columns[1],columns[2],columns[3],columns[4],columns[5],columns[6],columns[0]);
        let Ok(mut stmt) = conn.prepare(&sql) else {
            continue;
        };
        let Ok(rows) = stmt.query_map([start_ms], |row| {
            let mut tokens = 0u64;
            for i in 4..9 {
                tokens =
                    tokens.saturating_add(row.get::<_, Option<i64>>(i)?.unwrap_or(0).max(0) as u64);
            }
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, Option<String>>(3)?
                    .unwrap_or_else(|| "opencode".into()),
                tokens,
            ))
        }) else {
            continue;
        };
        for (id, session, at, model, tokens) in rows.flatten() {
            if !seen.insert(id) {
                continue;
            }
            let model = model
                .trim_end_matches('/')
                .rsplit('/')
                .next()
                .unwrap_or("opencode");
            stats.add(day(&Value::from(at)), tokens, model, &session);
        }
    }
    stats
}

#[cfg(test)]
#[test]
fn opencode_read_only_wal_and_migration() {
    let path = std::env::temp_dir().join(format!("usage-{}.db", uuid::Uuid::new_v4()));
    let writer = rusqlite::Connection::open(&path).unwrap();
    writer.execute_batch("PRAGMA journal_mode=WAL; CREATE TABLE session_message(id TEXT, session_id TEXT, type TEXT, time_created INTEGER, data TEXT); CREATE TABLE message(id TEXT, session_id TEXT, time_created INTEGER, data TEXT);").unwrap();
    let created = chrono::DateTime::parse_from_rfc3339("2026-11-02T12:00:00Z")
        .unwrap()
        .timestamp_millis();
    let data = include_str!("../../tests/fixtures/usage/opencode-message.json");
    writer
        .execute(
            "INSERT INTO session_message VALUES ('m1','s','assistant',?1,?2)",
            rusqlite::params![created, data],
        )
        .unwrap();
    writer
        .execute(
            "INSERT INTO message VALUES ('m1','s',?1,?2), ('m2','s',?1,?2), ('bad','s',?1,'{')",
            rusqlite::params![created, data],
        )
        .unwrap();
    writer
        .execute("INSERT INTO message VALUES ('old','s',0,?1)", [data])
        .unwrap();
    let conn = open_db(&path).unwrap();
    assert!(conn.execute("DELETE FROM message", []).is_err());
    let mut r = UsageRecord::empty("opencode", "OpenCode");
    opencode(&path, NaiveDate::from_ymd_opt(2026, 11, 2).unwrap()).apply(&mut r);
    assert_eq!(r.today.tokens, 220);
    assert_eq!(r.today.prompts, Some(2));
    assert_eq!(r.today.sessions, Some(1));
    assert_eq!(r.models[0].model, "test");
    drop(conn);
    drop(writer);
    let _ = fs::remove_file(path);
}
