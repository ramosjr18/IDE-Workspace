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
pub(super) struct LimitsState {
    backoff_file: Option<std::path::PathBuf>,
    backoff: tokio::sync::Mutex<Option<HashMap<String, i64>>>,
    cache: tokio::sync::Mutex<HashMap<String, CachedProbe>>,
}
impl Default for LimitsState {
    fn default() -> Self {
        Self {
            backoff_file: backoff_path(),
            backoff: Default::default(),
            cache: Default::default(),
        }
    }
}
impl LimitsState {
    pub(super) async fn collect(&self, r: &mut UsageRecord) {
        if !["codex", "claude", "opencode"].contains(&r.id.as_str()) {
            return;
        }
        let id = r.id.clone();
        self.collect_with(r, async {
            match id.as_str() {
                "codex" => codex().await,
                "claude" => claude(self, &HttpTransport)
                    .await
                    .unwrap_or_else(Probe::failed),
                "opencode" => opencode(self, &HttpTransport)
                    .await
                    .unwrap_or_else(Probe::failed),
                _ => Probe::default(),
            }
        })
        .await;
    }
    async fn collect_with(
        &self,
        r: &mut UsageRecord,
        work: impl std::future::Future<Output = Probe>,
    ) {
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
        let deadline = self.deadline(&r.id).await;
        let probe = if deadline > now {
            Probe::failed(Failure::Http(429))
        } else {
            work.await
        };
        let next = (now + 300).max(self.deadline(&r.id).await);
        self.cache.lock().await.insert(
            r.id.clone(),
            CachedProbe {
                next,
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
// This type intentionally has neither Debug nor Serialize.
struct Secret(String);
impl<'de> serde::Deserialize<'de> for Secret {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        <String as serde::Deserialize>::deserialize(d).map(Self)
    }
}
#[derive(Clone, Copy)]
enum Endpoint {
    Claude,
    OpenCode,
    Copilot,
}
impl Endpoint {
    fn url(self) -> &'static str {
        match self {
            Self::Claude => "https://api.anthropic.com/api/oauth/usage",
            Self::OpenCode => "https://opencode.ai/api/go/status",
            Self::Copilot => "https://api.github.com/copilot_internal/user",
        }
    }
}
struct HttpResponse {
    status: u16,
    retry_after: Option<String>,
    body: Value,
}
trait Transport: Sync {
    fn get<'a>(
        &'a self,
        endpoint: Endpoint,
        token: &'a Secret,
        headers: Vec<(&'static str, String)>,
    ) -> futures::future::BoxFuture<'a, Result<HttpResponse, Failure>>;
}
struct HttpTransport;
impl Transport for HttpTransport {
    fn get<'a>(
        &'a self,
        endpoint: Endpoint,
        token: &'a Secret,
        headers: Vec<(&'static str, String)>,
    ) -> futures::future::BoxFuture<'a, Result<HttpResponse, Failure>> {
        Box::pin(async move {
            let client = reqwest::Client::builder()
                .https_only(true)
                .redirect(reqwest::redirect::Policy::none())
                .timeout(Duration::from_secs(10))
                .build()
                .map_err(|_| Failure::Unavailable)?;
            let mut auth = reqwest::header::HeaderValue::from_str(&format!("Bearer {}", token.0))
                .map_err(|_| Failure::NoSession)?;
            auth.set_sensitive(true);
            let mut request = client
                .get(endpoint.url())
                .header(reqwest::header::AUTHORIZATION, auth)
                .header("Accept", "application/json")
                .header("User-Agent", "ide-workspace-usage/1");
            for (name, value) in headers {
                request = request.header(name, value);
            }
            let mut response = request.send().await.map_err(|_| Failure::Unavailable)?;
            let status = response.status().as_u16();
            let retry_after = response
                .headers()
                .get("Retry-After")
                .and_then(|v| v.to_str().ok())
                .map(str::to_owned);
            if status != 200 {
                return Ok(HttpResponse {
                    status,
                    retry_after,
                    body: Value::Null,
                });
            }
            let mut bytes = Vec::new();
            while let Some(chunk) = response.chunk().await.map_err(|_| Failure::Unavailable)? {
                if bytes.len() + chunk.len() > 1024 * 1024 {
                    return Err(Failure::InvalidResponse);
                }
                bytes.extend_from_slice(&chunk);
            }
            let body = serde_json::from_slice(&bytes).map_err(|_| Failure::InvalidResponse)?;
            Ok(HttpResponse {
                status,
                retry_after,
                body,
            })
        })
    }
}
fn retry_deadline(value: Option<&str>, now: i64) -> i64 {
    let until = value
        .and_then(|s| s.trim().parse::<i64>().ok())
        .map(|n| now.saturating_add(n.max(0)))
        .or_else(|| {
            value
                .and_then(|s| httpdate::parse_http_date(s).ok())
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs().min(i64::MAX as u64) as i64)
        })
        .unwrap_or(now.saturating_add(300));
    until.max(now.saturating_add(300))
}
fn backoff_path() -> Option<std::path::PathBuf> {
    dirs::data_dir().map(|p| p.join("usage-backoff.json"))
}
impl LimitsState {
    async fn deadline(&self, id: &str) -> i64 {
        let mut saved = self.backoff.lock().await;
        if saved.is_none() {
            let path = self.backoff_file.clone();
            *saved = Some(
                tauri::async_runtime::spawn_blocking(move || {
                    path.and_then(|p| std::fs::read(p).ok())
                        .and_then(|b| serde_json::from_slice::<HashMap<String, i64>>(&b).ok())
                        .unwrap_or_default()
                })
                .await
                .unwrap_or_default(),
            );
        }
        *saved.as_ref().unwrap().get(id).unwrap_or(&0)
    }
    async fn save_backoff(&self, id: &str, until: i64) -> Result<(), Failure> {
        let mut saved = self.backoff.lock().await;
        saved
            .get_or_insert_with(HashMap::new)
            .insert(id.into(), until);
        let values = saved.clone().unwrap_or_default();
        let path = self.backoff_file.clone();
        tauri::async_runtime::spawn_blocking(move || {
            let path = path.ok_or(Failure::Unavailable)?;
            std::fs::create_dir_all(path.parent().ok_or(Failure::Unavailable)?)
                .map_err(|_| Failure::Unavailable)?;
            // Contains only agent IDs and numeric deadlines; never credential or response data.
            let bytes = serde_json::to_vec(&values).map_err(|_| Failure::Unavailable)?;
            std::fs::write(path, bytes).map_err(|_| Failure::Unavailable)
        })
        .await
        .map_err(|_| Failure::Unavailable)?
    }
    async fn request(
        &self,
        id: &str,
        transport: &dyn Transport,
        endpoint: Endpoint,
        token: &Secret,
        headers: Vec<(&'static str, String)>,
    ) -> Result<Value, Failure> {
        let response = transport.get(endpoint, token, headers).await?;
        if response.status == 429 {
            let until = retry_deadline(response.retry_after.as_deref(), Utc::now().timestamp());
            self.save_backoff(id, until).await?;
        }
        if response.status != 200 {
            return Err(Failure::Http(response.status));
        }
        Ok(response.body)
    }
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct ClaudeOauth {
    access_token: Secret,
    expires_at: Option<i64>,
    subscription_type: Option<String>,
}
#[derive(serde::Deserialize)]
struct ClaudeCredentials {
    #[serde(rename = "claudeAiOauth")]
    oauth: ClaudeOauth,
}
async fn claude_credentials() -> Result<ClaudeOauth, Failure> {
    let file = super::local::claude_dir().join(".credentials.json");
    let bytes = tauri::async_runtime::spawn_blocking(move || std::fs::read(file).ok())
        .await
        .ok()
        .flatten();
    if let Some(bytes) = bytes {
        return serde_json::from_slice::<ClaudeCredentials>(&bytes)
            .map(|v| v.oauth)
            .map_err(|_| Failure::NoSession);
    }
    #[cfg(target_os = "macos")]
    {
        let mut cmd = Command::new("security");
        cmd.args([
            "find-generic-password",
            "-s",
            "Claude Code-credentials",
            "-w",
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true);
        if let Ok(Ok(output)) = tokio::time::timeout(Duration::from_secs(10), cmd.output()).await {
            if output.status.success() {
                return serde_json::from_slice::<ClaudeCredentials>(&output.stdout)
                    .map(|v| v.oauth)
                    .map_err(|_| Failure::NoSession);
            }
        }
    }
    Err(Failure::NoSession)
}
fn claude_limits(v: &Value) -> Result<Probe, Failure> {
    if !v.is_object() {
        return Err(Failure::InvalidResponse);
    }
    let mut probe = Probe::default();
    for (key, label) in [("five_hour", "Sesión (5 h)"), ("seven_day", "Semanal")] {
        let w = &v[key];
        let Some(n) = w["utilization"].as_f64() else {
            continue;
        };
        probe.limits.push(UsageLimit {
            label: label.into(),
            percent: (n / 100.0).clamp(0.0, 1.0),
            resets_at: reset(&w["resets_at"]),
        });
    }
    if probe.limits.is_empty() {
        return Err(Failure::InvalidResponse);
    }
    Ok(probe)
}
async fn claude(state: &LimitsState, transport: &dyn Transport) -> Result<Probe, Failure> {
    let mut credentials = claude_credentials().await?;
    if credentials
        .expires_at
        .is_some_and(|at| at <= Utc::now().timestamp_millis() + 300_000)
    {
        drop(credentials);
        let mut cmd = command("claude").map_err(|_| Failure::Expired)?;
        cmd.args(["-p", "ok"]);
        let status = tokio::time::timeout(Duration::from_secs(60), cmd.status()).await;
        if !matches!(status, Ok(Ok(s)) if s.success()) {
            return Err(Failure::Expired);
        }
        credentials = claude_credentials().await?;
        if credentials
            .expires_at
            .is_some_and(|at| at <= Utc::now().timestamp_millis())
        {
            return Err(Failure::Expired);
        }
    }
    if credentials.access_token.0.is_empty() {
        return Err(Failure::NoSession);
    }
    let response = state
        .request(
            "claude",
            transport,
            Endpoint::Claude,
            &credentials.access_token,
            vec![("anthropic-beta", "oauth-2025-04-20".into())],
        )
        .await?;
    let mut probe = claude_limits(&response)?;
    probe.plan = credentials
        .subscription_type
        .as_ref()
        .and_then(|s| plan(&Value::String(s.clone())));
    Ok(probe)
}

#[cfg(test)]
#[test]
fn claude_response_and_retry_after() {
    let v: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/usage/claude-limits.json"
    ))
    .unwrap();
    let probe = claude_limits(&v).ok().unwrap();
    assert_eq!(probe.limits.len(), 2);
    assert_eq!(probe.limits[0].percent, 0.11);
    assert_eq!(probe.limits[1].percent, 0.47);
    assert_eq!(retry_deadline(Some("900"), 1000), 1900);
    assert_eq!(
        retry_deadline(Some("Thu, 01 Jan 1970 00:30:00 GMT"), 1000),
        1800
    );
    assert_eq!(retry_deadline(Some("bad"), 1000), 1300);
    assert_eq!(retry_deadline(Some("-2"), 1000), 1300);
}

#[cfg(test)]
mod network_tests {
    use super::*;
    struct Fake {
        status: u16,
        body: Value,
    }
    impl Transport for Fake {
        fn get<'a>(
            &'a self,
            endpoint: Endpoint,
            token: &'a Secret,
            headers: Vec<(&'static str, String)>,
        ) -> futures::future::BoxFuture<'a, Result<HttpResponse, Failure>> {
            Box::pin(async move {
                assert_eq!(endpoint.url(), "https://api.anthropic.com/api/oauth/usage");
                assert!(!token.0.is_empty());
                assert_eq!(headers[0].0, "anthropic-beta");
                Ok(HttpResponse {
                    status: self.status,
                    retry_after: Some("900".into()),
                    body: self.body.clone(),
                })
            })
        }
    }
    #[tokio::test]
    async fn simulated_http_and_persistent_backoff() {
        let path =
            std::env::temp_dir().join(format!("usage-backoff-{}.json", uuid::Uuid::new_v4()));
        let state = LimitsState {
            backoff_file: Some(path.clone()),
            ..Default::default()
        };
        let token = Secret("test-only".into());
        let headers = || vec![("anthropic-beta", "oauth-2025-04-20".into())];
        let fake = Fake {
            status: 200,
            body: serde_json::from_str(include_str!(
                "../../tests/fixtures/usage/claude-limits.json"
            ))
            .unwrap(),
        };
        let response = state
            .request("claude", &fake, Endpoint::Claude, &token, headers())
            .await
            .ok()
            .unwrap();
        assert_eq!(
            claude_limits(&response).ok().unwrap().limits[0].percent,
            0.11
        );
        let fake = Fake {
            status: 429,
            body: json!({"error":"must never be displayed"}),
        };
        assert!(matches!(
            state
                .request("claude", &fake, Endpoint::Claude, &token, headers())
                .await,
            Err(Failure::Http(429))
        ));
        let restarted = LimitsState {
            backoff_file: Some(path.clone()),
            ..Default::default()
        };
        assert!(restarted.deadline("claude").await >= Utc::now().timestamp() + 899);
        let disk: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert!(disk["claude"].is_i64());
        assert_eq!(disk.as_object().unwrap().len(), 1);
        let mut r = UsageRecord::empty("claude", "Claude");
        restarted
            .collect_with(&mut r, async { panic!("backoff must prevent requests") })
            .await;
        assert!(matches!(r.status, UsageStatus::Unavailable));
        std::fs::remove_file(path).unwrap();
    }
    #[tokio::test]
    async fn refresh_cannot_bypass_five_minute_interval() {
        let state = LimitsState {
            backoff_file: None,
            ..Default::default()
        };
        let mut r = UsageRecord::empty("codex", "Codex");
        state
            .collect_with(&mut r, async {
                codex_limits(&json!({"primary":{"usedPercent":1}}))
            })
            .await;
        state
            .collect_with(&mut r, async { panic!("second attempt within 5 minutes") })
            .await;
        assert_eq!(r.limits[0].percent, 0.01);
    }
}
struct OpenCodeAccount {
    token: Secret,
    base: String,
    org: String,
    expires: i64,
    active: bool,
}
#[derive(serde::Deserialize)]
struct OpenCodeCredential {
    #[serde(rename = "type")]
    kind: String,
    access: Option<Secret>,
    expires: Option<i64>,
    metadata: Option<OpenCodeMetadata>,
}
#[derive(serde::Deserialize, Default)]
struct OpenCodeMetadata {
    server: Option<String>,
    #[serde(rename = "orgID")]
    org_id: Option<String>,
}
fn valid_opencode_base(base: &str) -> bool {
    reqwest::Url::parse(base).ok().is_some_and(|u| {
        u.scheme() == "https"
            && u.host_str() == Some("opencode.ai")
            && u.port_or_known_default() == Some(443)
            && u.username().is_empty()
            && u.password().is_none()
            && u.path() == "/"
            && u.query().is_none()
            && u.fragment().is_none()
    })
}
fn opencode_account_from_db(path: &std::path::Path) -> Option<OpenCodeAccount> {
    let conn = super::local::open_db(path).ok()?;
    let (active_id, org) = conn
        .query_row(
            "SELECT active_account_id, active_org_id FROM account_state LIMIT 1",
            [],
            |r| {
                Ok((
                    r.get::<_, Option<String>>(0)?.unwrap_or_default(),
                    r.get::<_, Option<String>>(1)?.unwrap_or_default(),
                ))
            },
        )
        .unwrap_or_default();
    let mut accounts = vec![];
    if let Ok(mut stmt) = conn.prepare("SELECT id, url, access_token, token_expiry FROM account") {
        if let Ok(rows) = stmt.query_map([], |r| {
            Ok(OpenCodeAccount {
                active: r.get::<_, String>(0)? == active_id,
                base: r.get::<_, Option<String>>(1)?.unwrap_or_default(),
                token: Secret(r.get::<_, Option<String>>(2)?.unwrap_or_default()),
                expires: r.get::<_, Option<i64>>(3)?.unwrap_or(0),
                org: org.clone(),
            })
        }) {
            accounts.extend(rows.flatten().filter(|a| !a.token.0.is_empty()));
        }
    }
    if let Ok(mut stmt) = conn.prepare("SELECT value, active FROM credential") {
        if let Ok(rows) = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, bool>(1)?)))
        {
            for (raw, active) in rows.flatten() {
                let Ok(credential) = serde_json::from_str::<OpenCodeCredential>(&raw) else {
                    continue;
                };
                if credential.kind != "oauth" {
                    continue;
                }
                let Some(token) = credential.access.filter(|t| !t.0.is_empty()) else {
                    continue;
                };
                let meta = credential.metadata.unwrap_or_default();
                accounts.push(OpenCodeAccount {
                    token,
                    base: meta.server.unwrap_or_default(),
                    org: meta.org_id.unwrap_or_else(|| org.clone()),
                    expires: credential.expires.unwrap_or(0),
                    active,
                });
            }
        }
    }
    accounts.sort_by(|a, b| b.active.cmp(&a.active).then(b.expires.cmp(&a.expires)));
    accounts.into_iter().next()
}
#[derive(serde::Deserialize)]
struct GoAuth {
    key: Option<Secret>,
    access: Option<Secret>,
}
#[derive(serde::Deserialize)]
struct OpenCodeAuth {
    #[serde(rename = "opencode-go")]
    go: Option<GoAuth>,
}
fn opencode_account() -> Option<OpenCodeAccount> {
    if let Some(account) = opencode_account_from_db(&super::local::opencode_db()) {
        return Some(account);
    }
    let api_key = std::env::var("OPENCODE_APIKEY")
        .ok()
        .filter(|s| !s.is_empty())
        .map(Secret);
    let token = api_key.or_else(|| {
        let path = super::local::opencode_db().parent()?.join("auth.json");
        let raw = std::fs::read(path).ok()?;
        let auth: OpenCodeAuth = serde_json::from_slice(&raw).ok()?;
        let go = auth.go?;
        go.key.or(go.access)
    })?;
    Some(OpenCodeAccount {
        token,
        base: "https://opencode.ai".into(),
        org: String::new(),
        expires: 0,
        active: true,
    })
}
fn opencode_limits(v: &Value) -> Result<Probe, Failure> {
    if v.is_null() {
        return Ok(Probe::default());
    }
    if !v.is_object() {
        return Err(Failure::InvalidResponse);
    }
    let mut probe = Probe {
        plan: plan(&v["product"]),
        ..Probe::default()
    };
    for (key, label) in [
        ("fiveHour", "Sesión (5 h)"),
        ("week", "Semanal"),
        ("month", "Mensual"),
    ] {
        let m = &v["access"]["meters"][key];
        let number = |v: &Value| {
            v.as_f64()
                .or_else(|| v.as_str().and_then(|s| s.parse::<f64>().ok()))
                .filter(|n| n.is_finite())
        };
        let Some(limit) = number(&m["limitMicroCents"]).filter(|n| *n > 0.0) else {
            continue;
        };
        let Some(used) = number(&m["usedMicroCents"]) else {
            continue;
        };
        probe.limits.push(UsageLimit {
            label: label.into(),
            percent: (used / limit).clamp(0.0, 1.0),
            resets_at: reset(&m["resetsAt"]),
        });
    }
    Ok(probe)
}
async fn opencode(state: &LimitsState, transport: &dyn Transport) -> Result<Probe, Failure> {
    let account = tauri::async_runtime::spawn_blocking(opencode_account)
        .await
        .map_err(|_| Failure::Unavailable)?
        .ok_or(Failure::NoSession)?;
    if !valid_opencode_base(&account.base) {
        return Err(Failure::Unavailable);
    }
    if account.expires > 0 && account.expires <= Utc::now().timestamp_millis() {
        return Err(Failure::Expired);
    }
    let headers = if account.org.is_empty() {
        vec![]
    } else {
        vec![("x-org-id", account.org)]
    };
    let v = state
        .request(
            "opencode",
            transport,
            Endpoint::OpenCode,
            &account.token,
            headers,
        )
        .await?;
    opencode_limits(&v)
}

#[cfg(test)]
mod opencode_tests {
    use super::*;
    #[test]
    fn response_and_destinations() {
        let v: Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/usage/opencode-limits.json"
        ))
        .unwrap();
        let probe = opencode_limits(&v).ok().unwrap();
        assert_eq!(probe.plan.as_deref(), Some("go"));
        assert_eq!(probe.limits.len(), 3);
        assert_eq!(probe.limits[0].percent, 0.125);
        assert_eq!(probe.limits[2].percent, 1.0);
        assert!(opencode_limits(&Value::Null)
            .ok()
            .unwrap()
            .limits
            .is_empty());
        assert!(valid_opencode_base("https://opencode.ai"));
        for base in [
            "http://opencode.ai",
            "https://opencode.ai.evil.test",
            "https://opencode.ai@evil.test",
            "https://opencode.ai:444",
            "https://opencode.ai/path",
            "https://evil.test",
            "https://opencode.ai?redirect=evil",
        ] {
            assert!(!valid_opencode_base(base));
        }
    }
    #[test]
    fn account_selection_is_read_only() {
        let path = std::env::temp_dir().join(format!("usage-account-{}.db", uuid::Uuid::new_v4()));
        let conn = rusqlite::Connection::open(&path).unwrap();
        conn.execute_batch("CREATE TABLE account_state(active_account_id TEXT, active_org_id TEXT); INSERT INTO account_state VALUES ('active','org'); CREATE TABLE account(id TEXT, url TEXT, access_token TEXT, token_expiry INTEGER); INSERT INTO account VALUES ('other','https://evil.test','inactive',9000000000000), ('active','https://opencode.ai','active-token',8000000000000);").unwrap();
        let a = opencode_account_from_db(&path).unwrap();
        assert_eq!(a.token.0, "active-token");
        assert_eq!(a.org, "org");
        assert!(valid_opencode_base(&a.base));
        conn.execute_batch(
            "DROP TABLE account; CREATE TABLE credential(value TEXT, active INTEGER);",
        )
        .unwrap();
        conn.execute("INSERT INTO credential VALUES (?1,1)", [r#"{"type":"oauth","access":"legacy-token","expires":8000000000000,"metadata":{"server":"https://opencode.ai","orgID":"legacy-org"}}"#]).unwrap();
        let a = opencode_account_from_db(&path).unwrap();
        assert_eq!(a.token.0, "legacy-token");
        assert_eq!(a.org, "legacy-org");
        drop(conn);
        std::fs::remove_file(path).unwrap();
    }
}
