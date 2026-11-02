//! Usage is independent of the current project. Credentials never enter this contract.
use serde::Serialize;
use std::time::{Duration, Instant};
use tauri::State;

#[derive(Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum UsageStatus { Ok, NoSession, Expired, Unavailable, NoData }

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageLimit {
    pub label: String,
    pub percent: f64,
    pub resets_at: Option<String>,
}
#[derive(Clone, Default, Serialize)]
pub struct Today { pub tokens: u64, pub prompts: Option<u64>, pub sessions: Option<u64> }
#[derive(Clone, Serialize)]
pub struct Day { pub date: String, pub tokens: u64 }
#[derive(Clone, Serialize)]
pub struct Model { pub model: String, pub tokens: u64 }
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageRecord {
    pub id: String,
    pub name: String,
    pub plan: Option<String>,
    pub status: UsageStatus,
    pub status_text: Option<String>,
    pub help: Option<String>,
    pub limits: Vec<UsageLimit>,
    pub today: Today,
    pub days: Vec<Day>,
    pub models: Vec<Model>,
    pub updated_at: String,
}
impl UsageRecord {
    fn empty(id: &str, name: &str) -> Self {
        Self { id: id.into(), name: name.into(), plan: None, status: UsageStatus::NoData,
            status_text: Some("Sin datos de uso".into()), help: None, limits: vec![],
            today: Today::default(), days: vec![], models: vec![], updated_at: chrono::Utc::now().to_rfc3339() }
    }
}

#[derive(Default)]
pub struct UsageState {
    cache: tokio::sync::Mutex<Option<(Instant, Vec<UsageRecord>)>>,
}

async fn collect() -> Vec<UsageRecord> {
    // PATH discovery may launch the login shell; keep it off the main thread.
    tauri::async_runtime::spawn_blocking(|| crate::agents::AGENTS.iter()
        .filter(|a| crate::env::which(a.cmd).is_some())
        .map(|a| UsageRecord::empty(a.id, a.name)).collect()).await.unwrap_or_default()
}

#[tauri::command]
pub async fn get_usage(force: bool, state: State<'_, UsageState>) -> Result<Vec<UsageRecord>, String> {
    let mut cache = state.cache.lock().await;
    if !force {
        if let Some((at, records)) = cache.as_ref() {
            if at.elapsed() < Duration::from_secs(60) { return Ok(records.clone()); }
        }
    }
    let records = collect().await;
    *cache = Some((Instant::now(), records.clone()));
    Ok(records)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn contract() {
        let value = serde_json::to_value(UsageRecord::empty("claude", "Claude")).unwrap();
        assert_eq!(value["status"], "no_data");
        assert!(value.get("updatedAt").is_some());
        assert!(value["today"]["prompts"].is_null());
        assert_eq!(value.as_object().unwrap().len(), 11);
    }
}
