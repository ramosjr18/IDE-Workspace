//! Usage is independent of the current project. Credentials never enter this contract.
use serde::Serialize;
use std::time::{Duration, Instant};
use tauri::State;
mod limits;
mod local;

#[derive(Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum UsageStatus {
    Ok,
    NoSession,
    Expired,
    Unavailable,
    NoData,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageLimit {
    pub label: String,
    pub percent: f64,
    pub resets_at: Option<String>,
}
#[derive(Clone, Default, Serialize)]
pub struct Today {
    pub tokens: u64,
    pub prompts: Option<u64>,
    pub sessions: Option<u64>,
}
#[derive(Clone, Serialize)]
pub struct Day {
    pub date: String,
    pub tokens: u64,
}
#[derive(Clone, Serialize)]
pub struct Model {
    pub model: String,
    pub tokens: u64,
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageRecord {
    /// `codex` (cuenta principal) o `codex:cuenta-2`.
    pub id: String,
    /// Id de la CLI (el mismo que list_agents).
    pub cli: String,
    /// Nombre de la cuenta; `None` = principal.
    pub account: Option<String>,
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
        Self {
            id: id.into(),
            cli: id.into(),
            account: None,
            name: name.into(),
            plan: None,
            status: UsageStatus::NoData,
            status_text: Some("Sin datos de uso".into()),
            help: None,
            limits: vec![],
            today: Today::default(),
            days: vec![],
            models: vec![],
            updated_at: chrono::Utc::now().to_rfc3339(),
        }
    }
}

#[derive(Default)]
pub struct UsageState {
    limits: limits::LimitsState,
    cache: tokio::sync::Mutex<Option<(Instant, Vec<UsageRecord>)>>,
}

async fn collect(state: &UsageState) -> Vec<UsageRecord> {
    // PATH discovery may launch the login shell; keep it off the main thread.
    // Una entrada por cuenta de cada CLI instalada (la principal y las de data_dir()/cuentas).
    let records: Vec<(crate::accounts::Account, UsageRecord)> = tauri::async_runtime::spawn_blocking(|| {
        crate::agents::AGENTS
            .iter()
            .filter(|a| crate::env::which(a.cmd).is_some())
            .flat_map(|a| {
                crate::accounts::list(a.id).into_iter().map(move |acc| {
                    let mut r = UsageRecord::empty(a.id, a.name);
                    if let Some(n) = &acc.name {
                        r.id = acc.id.clone();
                        r.account = Some(n.clone());
                        r.name = format!("{} · {n}", a.name);
                    }
                    (acc, r)
                })
            })
            .collect()
    })
    .await
    .unwrap_or_default();
    futures::future::join_all(records.into_iter().map(|(acc, record)| async move {
        crate::accounts::scope(acc, async move {
            let fallback = record.clone();
            let mut record = crate::accounts::blocking(move || {
                let mut record = record;
                local::collect(&mut record);
                record
            })
            .await
            .unwrap_or(fallback);
            state.limits.collect(&mut record).await;
            record
        })
        .await
    }))
    .await
}

/// Carga de cada cuenta (límite más alto y si tiene sesión) para elegir cuenta por cuota.
pub async fn loads(state: &UsageState) -> Vec<crate::accounts::Load> {
    state
        .read(false, collect(state))
        .await
        .iter()
        .map(|r| crate::accounts::Load {
            account: r.id.clone(),
            worst: r.limits.iter().map(|l| l.percent).fold(0.0, f64::max),
            usable: !matches!(r.status, UsageStatus::NoSession | UsageStatus::Expired),
        })
        .collect()
}

#[tauri::command]
pub async fn get_usage(
    force: bool,
    state: State<'_, UsageState>,
) -> Result<Vec<UsageRecord>, String> {
    Ok(state.read(force, collect(&state)).await)
}
impl UsageState {
    async fn read(
        &self,
        force: bool,
        work: impl std::future::Future<Output = Vec<UsageRecord>>,
    ) -> Vec<UsageRecord> {
        let mut cache = self.cache.lock().await;
        if !force {
            if let Some((at, records)) = cache.as_ref() {
                if at.elapsed() < Duration::from_secs(60) {
                    return records.clone();
                }
            }
        }
        let records = work.await;
        *cache = Some((Instant::now(), records.clone()));
        records
    }
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
        assert_eq!(value.as_object().unwrap().len(), 13);
        assert_eq!(value["cli"], "claude");
        assert!(value["account"].is_null());
    }
}

#[cfg(test)]
#[tokio::test]
async fn cache_force_and_expiry() {
    let state = UsageState::default();
    state
        .read(false, async {
            vec![UsageRecord::empty("claude", "Claude")]
        })
        .await;
    let cached = state
        .read(false, async { panic!("fresh cache must skip collection") })
        .await;
    assert_eq!(cached[0].id, "claude");
    let forced = state
        .read(true, async { vec![UsageRecord::empty("codex", "Codex")] })
        .await;
    assert_eq!(forced[0].id, "codex");
    state.cache.lock().await.as_mut().unwrap().0 = Instant::now() - Duration::from_secs(61);
    assert!(state.read(false, async { vec![] }).await.is_empty());
}

/// Opt-in acceptance gate: reads real CLI data and may run Claude to renew its own session.
#[cfg(test)]
#[tokio::test]
#[ignore]
async fn real_usage_dump() {
    let records = collect(&UsageState::default()).await;
    println!("{}", serde_json::to_string_pretty(&records).unwrap());
    for id in ["claude", "codex", "opencode"] {
        let r = records
            .iter()
            .find(|r| r.id == id)
            .expect("required CLI must be installed");
        assert!(
            r.today.tokens > 0 || !r.days.is_empty(),
            "{id}: missing local usage"
        );
        if id != "opencode" {
            assert!(!r.limits.is_empty(), "{id}: missing limits");
        }
    }
}
