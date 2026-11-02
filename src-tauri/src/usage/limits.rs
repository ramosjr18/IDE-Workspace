//! Limit probes never expose credential values, response bodies or subprocess output.
use super::{UsageLimit, UsageRecord, UsageStatus};
use chrono::{DateTime, Utc};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::process::Stdio;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;

#[derive(Clone)]
pub(super) enum Failure {
    NoSession,
    Expired,
    Unavailable,
    InvalidResponse,
    Http(u16),
}
impl Failure {
    fn text(&self) -> String {
        match self {
            Self::NoSession => "Sin sesión".into(),
            Self::Expired => "Sesión caducada".into(),
            Self::Unavailable => "Límites no disponibles".into(),
            Self::InvalidResponse => "Respuesta de uso no válida".into(),
            Self::Http(code) => format!("Límites no disponibles (HTTP {code})"),
        }
    }
}
#[derive(Clone, Default)]
struct Probe {
    limits: Vec<UsageLimit>,
    plan: Option<String>,
    error: Option<Failure>,
}
impl Probe {
    fn failed(error: Failure) -> Self {
        Self {
            error: Some(error),
            ..Self::default()
        }
    }
    fn apply(self, r: &mut UsageRecord) {
        r.limits = self.limits;
        r.plan = self.plan;
        if let Some(error) = self.error {
            r.status = match error {
                Failure::NoSession => UsageStatus::NoSession,
                Failure::Expired | Failure::Http(401) => UsageStatus::Expired,
                _ => UsageStatus::Unavailable,
            };
            r.status_text = Some(error.text());
            r.help = crate::agents::get(&r.id)
                .map(|a| format!("Abre `{}` o ejecuta `{}`", a.cmd, a.login));
        } else if !r.limits.is_empty() {
            r.status = UsageStatus::Ok;
            r.status_text = None;
            r.help = None;
        }
    }
}
struct CachedProbe {
    next: i64,
    probe: Probe,
}
#[derive(Default)]
pub(super) struct LimitsState {
    cache: tokio::sync::Mutex<HashMap<String, CachedProbe>>,
}
impl LimitsState {
    pub(super) async fn collect(&self, r: &mut UsageRecord) {
        if r.id != "codex" {
            return;
        }
        // The outer usage cache serializes collections; individual probes run concurrently.
        let now = Utc::now().timestamp();
        {
            let cache = self.cache.lock().await;
            if let Some(cached) = cache.get(&r.id).filter(|c| c.next > now) {
                let mut probe = cached.probe.clone();
                probe.limits.retain(window_open);
                probe.apply(r);
                return;
            }
        }
        let probe = codex().await;
        self.cache.lock().await.insert(
            r.id.clone(),
            CachedProbe {
                next: now + 300,
                probe: probe.clone(),
            },
        );
        probe.apply(r);
    }
}
fn window_open(limit: &UsageLimit) -> bool {
    limit
        .resets_at
        .as_deref()
        .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
        .is_none_or(|at| at.timestamp() > Utc::now().timestamp())
}
fn reset(v: &Value) -> Option<String> {
    if let Some(n) = v.as_i64() {
        return DateTime::<Utc>::from_timestamp(n, 0).map(|t| t.to_rfc3339());
    }
    DateTime::parse_from_rfc3339(v.as_str()?)
        .ok()
        .map(|t| t.with_timezone(&Utc).to_rfc3339())
}
fn plan(v: &Value) -> Option<String> {
    // Only known plan names can cross the frontend boundary.
    let s = v.as_str()?;
    match s.to_ascii_lowercase().as_str() {
        "free" | "plus" | "pro" | "team" | "business" | "enterprise" | "edu" | "max" | "go"
        | "go-plus" => Some(s.to_ascii_lowercase()),
        _ => None,
    }
}
fn codex_limits(v: &Value) -> Probe {
    let mut probe = Probe {
        plan: plan(v.get("planType").unwrap_or(&v["plan_type"])),
        ..Probe::default()
    };
    for key in ["primary", "secondary"] {
        let w = &v[key];
        let Some(used) = w.get("usedPercent").unwrap_or(&w["used_percent"]).as_f64() else {
            continue;
        };
        let mins = w
            .get("windowDurationMins")
            .unwrap_or(&w["window_minutes"])
            .as_u64()
            .unwrap_or(0);
        let label = match mins {
            10080 => "Semanal".into(),
            n if n > 0 && n % 60 == 0 => format!("Sesión ({} h)", n / 60),
            _ => "Límite".into(),
        };
        probe.limits.push(UsageLimit {
            label,
            percent: (used / 100.0).clamp(0.0, 1.0),
            resets_at: reset(w.get("resetsAt").unwrap_or(&w["resets_at"])),
        });
    }
    probe
}
fn command(name: &str) -> Result<Command, Failure> {
    let file = crate::env::which(name).ok_or(Failure::Unavailable)?;
    let mut cmd = Command::new(file);
    cmd.env_clear()
        .envs(crate::env::child_env())
        .kill_on_drop(true)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(windows)]
    cmd.creation_flags(0x0800_0000);
    Ok(cmd)
}
async fn codex_rpc() -> Result<Probe, Failure> {
    let mut cmd = command("codex")?;
    cmd.args(["-s", "read-only", "-a", "on-request", "app-server"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped());
    let mut child = cmd.spawn().map_err(|_| Failure::Unavailable)?;
    let mut input = child.stdin.take().ok_or(Failure::Unavailable)?;
    let mut lines = BufReader::new(child.stdout.take().ok_or(Failure::Unavailable)?).lines();
    let result = tokio::time::timeout(Duration::from_secs(20), async {
        for (id, method, params) in [
            (
                1,
                "initialize",
                json!({"clientInfo":{"name":"ide-workspace-usage","version":"1"}}),
            ),
            (2, "account/rateLimits/read", json!({})),
        ] {
            input
                .write_all(
                    format!("{}\n", json!({"id":id,"method":method,"params":params})).as_bytes(),
                )
                .await
                .map_err(|_| Failure::Unavailable)?;
            loop {
                let raw = lines
                    .next_line()
                    .await
                    .map_err(|_| Failure::Unavailable)?
                    .ok_or(Failure::Unavailable)?;
                let Ok(message) = serde_json::from_str::<Value>(&raw) else {
                    continue;
                };
                if message["id"].as_i64() != Some(id) {
                    continue;
                }
                if message.get("error").is_some() {
                    return Err(Failure::Unavailable);
                }
                if id == 2 {
                    let probe = codex_limits(&message["result"]["rateLimits"]);
                    return if probe.limits.is_empty() {
                        Err(Failure::InvalidResponse)
                    } else {
                        Ok(probe)
                    };
                }
                break;
            }
            input
                .write_all(b"{\"method\":\"initialized\",\"params\":{}}\n")
                .await
                .map_err(|_| Failure::Unavailable)?;
        }
        Err(Failure::InvalidResponse)
    })
    .await
    .unwrap_or(Err(Failure::Unavailable));
    let _ = child.kill().await;
    let _ = child.wait().await;
    result
}
async fn codex() -> Probe {
    if let Ok(probe) = codex_rpc().await {
        return probe;
    }
    tauri::async_runtime::spawn_blocking(|| {
        let mut latest: Option<(String, Probe)> = None;
        for path in super::local::recent_files(&super::local::codex_dir().join("sessions")) {
            if !path
                .file_name()
                .is_some_and(|n| n.to_string_lossy().starts_with("rollout-"))
            {
                continue;
            }
            super::local::lines(&path, |_, v| {
                let Some(limits) = v["payload"].get("rate_limits") else {
                    return;
                };
                let mut probe = codex_limits(limits);
                probe.limits.retain(window_open);
                if probe.limits.is_empty() {
                    return;
                }
                let at = v["timestamp"].as_str().unwrap_or("").to_string();
                if latest.as_ref().is_none_or(|(prev, _)| at > *prev) {
                    latest = Some((at, probe));
                }
            });
        }
        latest
            .map(|(_, p)| p)
            .unwrap_or_else(|| Probe::failed(Failure::Unavailable))
    })
    .await
    .unwrap_or_else(|_| Probe::failed(Failure::Unavailable))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn codex_response() {
        let v: Value =
            serde_json::from_str(include_str!("../../tests/fixtures/usage/codex-limits.json"))
                .unwrap();
        let probe = codex_limits(&v["rateLimits"]);
        assert_eq!(probe.limits.len(), 2);
        assert_eq!(probe.limits[0].percent, 0.01);
        assert_eq!(probe.limits[1].percent, 0.92);
        assert_eq!(probe.plan.as_deref(), Some("pro"));
        let snake =
            json!({"primary":{"used_percent":7,"window_minutes":300,"resets_at":1900000000}});
        assert_eq!(codex_limits(&snake).limits[0].percent, 0.07);
    }
}
