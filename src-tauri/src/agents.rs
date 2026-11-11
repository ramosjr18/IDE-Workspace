//! Adaptadores: cómo se ejecuta cada CLI y cómo recibe la conversación compartida.

use crate::env::{self, child_env, strip_ansi, which};
use crate::store::Message;
use crate::stream::{self, Format, Parser, Step};
use serde::Serialize;
use std::path::Path;
use std::process::Stdio;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;

const RUN_TIMEOUT: Duration = Duration::from_secs(10 * 60);
const MAX_CONTEXT_CHARS: usize = 30_000;

#[derive(Clone, Copy, PartialEq)]
pub enum StatusCheck {
    /// `claude auth status` devuelve JSON con `loggedIn`.
    ClaudeAuth,
    /// `codex login status` sale con código 0 si hay sesión.
    CodexLogin,
    /// No necesita sesión (OpenCode tiene modelos gratuitos).
    AlwaysReady,
    /// agy no tiene «auth status»: si no hay sesión, `agy models` se queda esperando el login.
    AgyModels,
    /// Gemini: archivo de credenciales o variable GEMINI_API_KEY.
    GeminiCreds,
    /// Copilot: usuario guardado por `copilot login`, token en el entorno o sesión de `gh`.
    CopilotAuth,
}

pub struct Agent {
    pub id: &'static str,
    pub name: &'static str,
    pub cmd: &'static str,
    /// Argumentos fijos. Si `prompt_arg` es true, el mensaje va tras `-p`; si no, por stdin
    /// (así en Windows nunca pasa por la línea de comandos de cmd.exe).
    pub args: &'static [&'static str],
    pub prompt_arg: bool,
    /// Formato de la salida (con las opciones de `args`), para mostrar sus pasos en vivo.
    pub format: Format,
    /// Opciones para trabajar en modo solo lectura (consejo): puede leer el repo, no modificarlo.
    pub readonly_args: &'static [&'static str],
    /// Límite de contexto en caracteres (agy lo recibe como argumento: Windows limita a ~32 000).
    pub max_context: usize,
    pub install_unix: &'static str,
    pub install_win: &'static str,
    pub login: &'static str,
    pub status: StatusCheck,
    /// Cómo reanuda su sesión (docs/plan-reanudar-sesiones.md §2).
    pub session: SessionMode,
}

/// Cómo guarda y reanuda su sesión cada CLI. Comprobado el 04/11/2026.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum SessionMode {
    /// No se reanuda: cada mensaje lleva el historial como texto (Gemini, sin probar).
    None,
    /// La app elige el id: `--session-id <uuid>` al crearla y `--resume <id>` al seguir (Claude, Copilot).
    Own,
    /// La CLI dice el id en su salida y se sigue con `<opción> <id>` (OpenCode `-s`, agy `--conversation`).
    Flag(&'static str),
    /// Codex: `exec [opciones] resume <id> -`; el id sale en `thread.started`.
    Codex,
}

/// Sesión de una ejecución.
#[derive(Clone, PartialEq, Debug)]
pub enum Session {
    /// Sin sesión que guardar (consejo, pruebas).
    None,
    /// Sesión nueva, para reanudarla en el siguiente mensaje.
    New,
    /// Seguir esta sesión.
    Resume(String),
}

pub const AGENTS: &[Agent] = &[
    Agent {
        id: "claude",
        name: "Claude",
        cmd: "claude",
        args: &["-p", "--dangerously-skip-permissions", "--output-format", "stream-json", "--verbose"],
        prompt_arg: false,
        format: Format::Claude,
        readonly_args: &["-p", "--permission-mode", "plan", "--output-format", "stream-json", "--verbose"],
        max_context: MAX_CONTEXT_CHARS,
        install_unix: "curl -fsSL https://claude.ai/install.sh | bash",
        install_win: "irm https://claude.ai/install.ps1 | iex",
        login: "claude auth login",
        status: StatusCheck::ClaudeAuth,
        session: SessionMode::Own,
    },
    Agent {
        id: "codex",
        name: "Codex",
        cmd: "codex",
        args: &["exec", "--approve-for-me", "--skip-git-repo-check", "--json", "-"],
        prompt_arg: false,
        format: Format::Codex,
        readonly_args: &["exec", "--sandbox", "read-only", "--skip-git-repo-check", "--json", "-"],
        max_context: MAX_CONTEXT_CHARS,
        install_unix: "npm install -g @openai/codex",
        install_win: "npm install -g @openai/codex",
        login: "codex login",
        status: StatusCheck::CodexLogin,
        session: SessionMode::Codex,
    },
    Agent {
        id: "opencode",
        name: "OpenCode",
        cmd: "opencode",
        args: &["run", "--auto", "--format", "json"],
        prompt_arg: false,
        format: Format::OpenCode,
        readonly_args: &["run", "--agent", "plan", "--format", "json"],
        max_context: MAX_CONTEXT_CHARS,
        install_unix: "npm install -g opencode-ai",
        install_win: "npm install -g opencode-ai",
        login: "opencode auth login",
        status: StatusCheck::AlwaysReady,
        session: SessionMode::Flag("-s"),
    },
    Agent {
        id: "agy",
        name: "Antigravity (agy)",
        cmd: "agy",
        args: &["--dangerously-skip-permissions", "--print-timeout", "30m", "--output-format", "stream-json"],
        prompt_arg: true,
        format: Format::Agy,
        readonly_args: &["--mode", "plan", "--print-timeout", "30m", "--output-format", "stream-json"],
        max_context: 20_000,
        install_unix: "curl -fsSL https://antigravity.google/cli | bash",
        install_win: "irm https://antigravity.google/cli/install.ps1 | iex",
        login: "agy",
        status: StatusCheck::AgyModels,
        session: SessionMode::Flag("--conversation"),
    },
    Agent {
        id: "gemini",
        name: "Gemini",
        cmd: "gemini",
        args: &["--yolo", "--skip-trust"],
        prompt_arg: false,
        format: Format::Plain,
        readonly_args: &["--skip-trust"],
        max_context: MAX_CONTEXT_CHARS,
        install_unix: "npm install -g @google/gemini-cli",
        install_win: "npm install -g @google/gemini-cli",
        login: "gemini",
        status: StatusCheck::GeminiCreds,
        session: SessionMode::None,
    },
    Agent {
        id: "copilot",
        name: "Copilot",
        cmd: "copilot",
        args: &["--yolo", "--no-ask-user", "--output-format", "json"],
        prompt_arg: false,
        format: Format::Copilot,
        readonly_args: &["--allow-all-tools", "--deny-tool", "write", "--deny-tool", "shell", "--no-ask-user", "--output-format", "json"],
        max_context: MAX_CONTEXT_CHARS,
        install_unix: "npm install -g @github/copilot",
        install_win: "npm install -g @github/copilot",
        login: "copilot login",
        status: StatusCheck::CopilotAuth,
        session: SessionMode::Own,
    },
];

pub fn get(id: &str) -> Option<&'static Agent> {
    AGENTS.iter().find(|a| a.id == id)
}

#[derive(Serialize)]
pub struct AgentInfo {
    pub id: &'static str,
    pub name: &'static str,
    /// "ready" | "login" | "missing"
    pub status: &'static str,
}

fn command(file: &Path) -> Command {
    let mut cmd = Command::new(file);
    cmd.env_clear().envs(child_env()).kill_on_drop(true);
    #[cfg(windows)]
    cmd.creation_flags(0x0800_0000);
    // Grupo de procesos propio, para poder matar también a sus hijos (ver `kill_tree`).
    #[cfg(unix)]
    cmd.process_group(0);
    cmd
}

/// Mata el proceso y todo lo que haya lanzado. Las CLIs de npm (Copilot) son un lanzador de node
/// que arranca el binario de verdad: matar solo el lanzador deja vivo al binario, que sigue
/// trabajando y bloquea la sesión («SessionInUseError» en el siguiente mensaje).
async fn kill_tree(child: &mut tokio::process::Child) {
    if let Some(pid) = child.id() {
        #[cfg(unix)]
        let _ = Command::new("kill").args(["-s", "KILL", "--", &format!("-{pid}")]).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).status().await;
        #[cfg(windows)]
        {
            let mut tk = Command::new("taskkill");
            tk.args(["/T", "/F", "/PID", &pid.to_string()]).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).creation_flags(0x0800_0000);
            let _ = tk.status().await;
        }
    }
    let _ = child.kill().await;
}

/// Ejecuta un comando corto. Devuelve (código, salida). `stop_on` corta en cuanto aparece ese texto.
async fn exec(cmd_name: &str, args: &[&str], timeout: Duration, stop_on: &[&str]) -> (i32, String) {
    let Some(file) = which(cmd_name) else { return (-1, String::new()) };
    let mut cmd = command(&file);
    cmd.args(args).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    let Ok(mut child) = cmd.spawn() else { return (-1, String::new()) };
    let mut stdout = child.stdout.take().unwrap();
    let mut stderr = child.stderr.take().unwrap();
    let mut out = Vec::new();
    let mut buf_o = [0u8; 4096];
    let mut buf_e = [0u8; 4096];
    let deadline = tokio::time::sleep(timeout);
    tokio::pin!(deadline);
    let (mut o_done, mut e_done) = (false, false);
    while !(o_done && e_done) {
        tokio::select! {
            r = stdout.read(&mut buf_o), if !o_done => match r { Ok(0) | Err(_) => o_done = true, Ok(n) => out.extend_from_slice(&buf_o[..n]) },
            r = stderr.read(&mut buf_e), if !e_done => match r { Ok(0) | Err(_) => e_done = true, Ok(n) => out.extend_from_slice(&buf_e[..n]) },
            _ = &mut deadline => { kill_tree(&mut child).await; return (-1, String::from_utf8_lossy(&out).into()); }
        }
        let text = String::from_utf8_lossy(&out);
        if stop_on.iter().any(|s| text.contains(s)) {
            kill_tree(&mut child).await;
            return (1, text.into());
        }
    }
    let code = child.wait().await.ok().and_then(|s| s.code()).unwrap_or(-1);
    (code, String::from_utf8_lossy(&out).into())
}

pub async fn status(agent: &Agent) -> &'static str {
    if which(agent.cmd).is_none() {
        return "missing";
    }
    let short = Duration::from_secs(15);
    let ready = match agent.status {
        StatusCheck::AlwaysReady => true,
        StatusCheck::ClaudeAuth => {
            let (_, out) = exec("claude", &["auth", "status"], short, &[]).await;
            serde_json::from_str::<serde_json::Value>(out.trim())
                .ok()
                .and_then(|v| v.get("loggedIn").and_then(|b| b.as_bool()))
                .unwrap_or(false)
        }
        StatusCheck::CodexLogin => exec("codex", &["login", "status"], short, &[]).await.0 == 0,
        StatusCheck::AgyModels => {
            let (code, out) =
                exec("agy", &["models"], Duration::from_secs(20), &["Waiting for authentication", "authorization code"]).await;
            code == 0 && !out.contains("authentication")
        }
        StatusCheck::GeminiCreds => {
            // Con una cuenta extra, GEMINI_CLI_HOME hace de carpeta de usuario.
            let home = crate::accounts::active().and_then(|a| a.dir.filter(|_| a.cli == "gemini")).unwrap_or_else(env::home);
            std::env::var("GEMINI_API_KEY").is_ok() || home.join(".gemini").join("oauth_creds.json").exists()
        }
        StatusCheck::CopilotAuth => {
            let saved = std::fs::read_to_string(env::home().join(".copilot").join("config.json"))
                .ok()
                .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok())
                .and_then(|v| v["logged_in_users"].as_array().map(|a| !a.is_empty()))
                .unwrap_or(false);
            let token = ["COPILOT_GITHUB_TOKEN", "GH_TOKEN", "GITHUB_TOKEN"].iter().any(|k| std::env::var(k).is_ok_and(|v| !v.is_empty()));
            saved || token || exec("gh", &["auth", "token"], short, &[]).await.0 == 0
        }
    };
    if ready { "ready" } else { "login" }
}

/// Una línea del historial: «[Quién]: texto», con el resumen de pasos si los hubo.
fn history_line(m: &Message) -> String {
    let who = if m.role == "user" { "Usuario" } else { m.agent_name.as_deref().or(m.agent.as_deref()).unwrap_or("Asistente") };
    let mut text = m.text.clone();
    if let Some(sum) = m.steps.as_deref().and_then(stream::summary) {
        text = format!("{sum}\n{text}");
    }
    format!("[{who}]: {text}")
}

/// Las líneas más recientes que caben en `budget` caracteres, en orden.
fn recent_lines(messages: &[Message], budget: usize) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    let mut total = 0;
    for m in messages.iter().rev() {
        let line = history_line(m);
        let len = line.chars().count();
        if total + len > budget {
            break;
        }
        total += len;
        lines.push(line);
    }
    lines.reverse();
    lines
}

/// Convierte la conversación guardada en el prompt para el agente elegido. Cada respuesta
/// lleva el nombre del agente que la escribió, para que el siguiente sepa quién dijo qué.
/// Es el prompt de una sesión nueva: con el historial entero (lo que quepa).
pub fn build_prompt(agent: &Agent, messages: &[Message], message: &str) -> String {
    let header = format!(
        "Estás en un chat compartido por varios asistentes de IA. El usuario puede cambiar de asistente en cualquier momento, así que algunas respuestas anteriores pueden ser de otro asistente: cada una lleva su nombre. Tú eres {}. Continúa la conversación con naturalidad.\n\n--- Conversación hasta ahora ---\n",
        agent.name
    );
    let footer = "\n--- Fin de la conversación ---\n\nNuevo mensaje del usuario:\n";
    // Presupuesto para el historial: deja sitio para el mensaje nuevo y la cabecera.
    let budget = agent.max_context.saturating_sub(message.chars().count() + header.chars().count() + footer.chars().count());
    let lines = recent_lines(messages, budget);
    if lines.is_empty() {
        return message.to_string();
    }
    format!("{header}{}{footer}{}", lines.join("\n\n"), message)
}

/// Prompt de una sesión reanudada: solo lo que esta sesión no vio (lo que se habló con otros
/// asistentes desde su último turno) y el mensaje nuevo. Sin nada pendiente, el mensaje tal cual.
pub fn build_resume_prompt(agent: &Agent, unseen: &[Message], message: &str) -> String {
    let header = "Mientras no estabas, el usuario siguió la conversación con otros asistentes (cada respuesta lleva su nombre):\n\n--- Lo que te perdiste ---\n";
    let footer = "\n--- Fin ---\n\nNuevo mensaje del usuario:\n";
    let budget = agent.max_context.saturating_sub(message.chars().count() + header.chars().count() + footer.chars().count());
    let lines = recent_lines(unseen, budget);
    if lines.is_empty() {
        return message.to_string();
    }
    format!("{header}{}{footer}{}", lines.join("\n\n"), message)
}

/// Resultado de una ejecución: respuesta final y pasos que se mostraron.
pub struct RunResult {
    pub text: String,
    pub steps: Vec<Step>,
    /// Uso que informó la CLI durante la ejecución (tokens, coste, peticiones).
    pub usage: crate::stream::Usage,
    /// Sesión de la CLI en la que quedó esta respuesta, para reanudarla.
    pub session_id: Option<String>,
}

#[cfg(test)]
/// Ejecuta el agente con el prompt en la carpeta `cwd`. Cada paso (comando, archivo editado,
/// texto…) se pasa a `on_step` en cuanto ocurre. Si llega `cancel`, se detiene el proceso.
pub async fn run(
    agent: &Agent,
    prompt: &str,
    cwd: &Path,
    on_step: impl FnMut(&Step),
    cancel: tokio::sync::oneshot::Receiver<()>,
) -> Result<RunResult, String> {
    run_with(agent, agent.args, &Session::None, prompt, cwd, on_step, cancel).await
}

/// Como `run`, creando o reanudando la sesión de la CLI.
pub async fn run_session(
    agent: &Agent,
    session: &Session,
    prompt: &str,
    cwd: &Path,
    on_step: impl FnMut(&Step),
    cancel: tokio::sync::oneshot::Receiver<()>,
) -> Result<RunResult, String> {
    run_with(agent, agent.args, session, prompt, cwd, on_step, cancel).await
}

/// Argumentos de la CLI para esa sesión. `own_id`: el id que pone la app en una sesión nueva (modo `Own`).
fn session_args(agent: &Agent, base: &[&str], session: &Session, own_id: &str) -> Vec<String> {
    let mut args: Vec<String> = base.iter().map(|a| a.to_string()).collect();
    match (agent.session, session) {
        (SessionMode::None, _) | (_, Session::None) => {}
        (SessionMode::Own, Session::New) => args.extend(["--session-id".into(), own_id.into()]),
        (SessionMode::Own, Session::Resume(id)) => args.extend(["--resume".into(), id.clone()]),
        (SessionMode::Flag(flag), Session::Resume(id)) => args.extend([flag.into(), id.clone()]),
        (SessionMode::Codex, Session::Resume(id)) => {
            // `exec … -` → `exec … resume <id> -` (las opciones van antes de `resume`).
            if args.last().map(String::as_str) == Some("-") {
                args.pop();
            }
            args.extend(["resume".into(), id.clone(), "-".into()]);
        }
        (SessionMode::Flag(_) | SessionMode::Codex, Session::New) => {}
    }
    args
}

/// La CLI no encontró la sesión que se le pidió reanudar (borrada o de otra carpeta). Textos reales.
pub fn session_lost(agent: &Agent, error: &str) -> bool {
    let e = error.to_lowercase();
    let marks: &[&str] = match agent.id {
        // «No conversation found with session ID: …»
        "claude" => &["no conversation found with session id"],
        // «thread/resume failed: no rollout found for thread id …»
        "codex" => &["no rollout found for thread id", "thread/resume failed"],
        // «Error: No session, task, or name matched '…'.»
        "copilot" => &["no session, task, or name matched"],
        _ => &[],
    };
    marks.iter().any(|m| e.contains(m))
}

/// OpenCode crea una sesión vacía si `-s` apunta a una que no existe, sin avisar: se comprueba antes.
pub async fn opencode_has_session(id: &str, cwd: &Path) -> bool {
    let Some(file) = which("opencode") else { return false };
    let mut cmd = command(&file);
    cmd.args(["session", "list", "--format", "json", "-n", "100000"])
        .current_dir(cwd)
        .env("PWD", cwd)
        .stdin(Stdio::null())
        .stderr(Stdio::null());
    match tokio::time::timeout(Duration::from_secs(20), cmd.output()).await {
        Ok(Ok(out)) => serde_json::from_slice::<Vec<serde_json::Value>>(&out.stdout)
            .map(|l| l.iter().any(|s| s["id"] == id))
            .unwrap_or(false),
        _ => false,
    }
}

/// Igual que `run`, pero en modo solo lectura (para el consejo).
pub async fn run_readonly(
    agent: &Agent,
    prompt: &str,
    cwd: &Path,
    on_step: impl FnMut(&Step),
    cancel: tokio::sync::oneshot::Receiver<()>,
) -> Result<RunResult, String> {
    run_with(agent, agent.readonly_args, &Session::None, prompt, cwd, on_step, cancel).await
}

async fn run_with(
    agent: &Agent,
    args: &[&str],
    session: &Session,
    prompt: &str,
    cwd: &Path,
    mut on_step: impl FnMut(&Step),
    mut cancel: tokio::sync::oneshot::Receiver<()>,
) -> Result<RunResult, String> {
    let file = which(agent.cmd).ok_or_else(|| format!("{} no está instalado", agent.cmd))?;
    let mut cmd = command(&file);
    if agent.prompt_arg {
        cmd.arg("-p").arg(prompt);
    }
    // Algunas CLIs (OpenCode) toman la carpeta de PWD y no de la carpeta del proceso.
    let own_id = uuid::Uuid::new_v4().to_string();
    cmd.args(session_args(agent, args, session, &own_id)).current_dir(cwd).env("PWD", cwd).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = cmd.spawn().map_err(|e| format!("No se pudo ejecutar {}: {e}", agent.cmd))?;
    let mut stdin = child.stdin.take().unwrap();
    let input = if agent.prompt_arg { String::new() } else { prompt.to_string() };
    tokio::spawn(async move {
        let _ = stdin.write_all(input.as_bytes()).await;
        let _ = stdin.shutdown().await;
    });
    // stderr se guarda aparte para el mensaje de error.
    let mut stderr = child.stderr.take().unwrap();
    let err_task = tokio::spawn(async move {
        let mut buf = Vec::new();
        let _ = stderr.read_to_end(&mut buf).await;
        String::from_utf8_lossy(&buf).to_string()
    });

    let mut parser = Parser::new(agent.format, cwd);
    let mut lines = BufReader::new(child.stdout.take().unwrap()).lines();
    let deadline = tokio::time::sleep(RUN_TIMEOUT);
    tokio::pin!(deadline);
    // Solo se detiene con una orden explícita; si el emisor desaparece sin enviarla, se sigue.
    let mut cancel_live = true;
    loop {
        tokio::select! {
            line = lines.next_line() => match line {
                Ok(Some(line)) => {
                    for step in parser.feed(&strip_ansi(&line)) {
                        on_step(&step);
                    }
                }
                _ => break,
            },
            r = &mut cancel, if cancel_live => {
                if r.is_ok() {
                    kill_tree(&mut child).await;
                    return Err("Detenido.".into());
                }
                cancel_live = false;
            }
            _ = &mut deadline => {
                kill_tree(&mut child).await;
                return Err("Tiempo agotado (10 min)".into());
            }
        }
    }
    let status = child.wait().await.map_err(|e| e.to_string())?;
    let err = strip_ansi(&err_task.await.unwrap_or_default());
    let text = parser.final_text();
    let usage = parser.usage();
    let session_id = match (agent.session, session) {
        (SessionMode::None, _) | (_, Session::None) => None,
        (SessionMode::Own, Session::New) => Some(own_id),
        // Si la CLI no repite el id al reanudar, sigue siendo el mismo.
        (_, Session::Resume(id)) => parser.session_id().or_else(|| Some(id.clone())),
        _ => parser.session_id(),
    };
    let steps = parser.finish();
    if status.success() {
        return Ok(RunResult { text, steps, usage, session_id });
    }
    let detail = if err.trim().is_empty() {
        steps.iter().rev().find(|s| s.kind == "error" || s.kind == "text").map(|s| s.detail.clone()).unwrap_or_default()
    } else {
        err
    };
    let tail: String = detail.chars().rev().take(4000).collect::<Vec<_>>().into_iter().rev().collect();
    Err(format!("{} terminó con código {}\n{}", agent.cmd, status.code().unwrap_or(-1), tail.trim()))
}

/// Shell y argumentos para instalar el agente o iniciar sesión en el terminal de la app.
pub fn terminal_command(agent: &Agent, action: &str) -> (String, String, Vec<String>) {
    let command = if action == "install" {
        if env::IS_WIN { agent.install_win } else { agent.install_unix }
    } else {
        agent.login
    };
    if env::IS_WIN {
        (
            command.into(),
            "powershell.exe".into(),
            vec!["-NoProfile".into(), "-ExecutionPolicy".into(), "Bypass".into(), "-Command".into(), command.into()],
        )
    } else {
        (command.into(), "bash".into(), vec!["-lc".into(), command.into()])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Como el lanzador de node de Copilot: al detener, el proceso nieto también tiene que morir.
    #[cfg(unix)]
    #[tokio::test]
    async fn kill_tree_mata_tambien_a_los_nietos() {
        let mut cmd = command(Path::new("/bin/sh"));
        cmd.args(["-c", "sleep 300 & echo $!; wait"]).stdout(Stdio::piped());
        let mut child = cmd.spawn().unwrap();
        let mut lines = BufReader::new(child.stdout.take().unwrap()).lines();
        let nieto = lines.next_line().await.unwrap().unwrap();
        let vivo = || std::fs::read_to_string(format!("/proc/{nieto}/stat")).map(|s| !s.contains(") Z ")).unwrap_or(false);
        assert!(vivo());
        kill_tree(&mut child).await;
        for _ in 0..50 {
            if !vivo() { return; }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        panic!("el nieto {nieto} sigue vivo");
    }

    fn msg(role: &str, text: &str, name: Option<&str>) -> Message {
        Message { role: role.into(), text: text.into(), agent: None, agent_name: name.map(str::to_string), ts: String::new(), steps: None }
    }

    #[test]
    fn argumentos_de_sesion() {
        let a = |id: &str, s: &Session| {
            let ag = get(id).unwrap();
            session_args(ag, ag.args, s, "UUID").join(" ")
        };
        let resume = |id: &str| Session::Resume(id.into());
        assert!(a("claude", &Session::New).ends_with("--verbose --session-id UUID"));
        assert!(a("claude", &resume("abc")).ends_with("--verbose --resume abc"));
        assert_eq!(a("claude", &Session::None), get("claude").unwrap().args.join(" "));
        assert_eq!(a("codex", &Session::New), "exec --approve-for-me --skip-git-repo-check --json -");
        assert_eq!(a("codex", &resume("019a")), "exec --approve-for-me --skip-git-repo-check --json resume 019a -");
        assert!(a("opencode", &resume("ses_x")).ends_with("--format json -s ses_x"));
        assert!(!a("opencode", &Session::New).contains("-s"));
        assert!(a("agy", &resume("c1")).ends_with("--conversation c1"));
        assert!(a("copilot", &Session::New).ends_with("--session-id UUID"));
        assert!(a("copilot", &resume("c2")).ends_with("--resume c2"));
        // Gemini no reanuda (sin probar).
        assert_eq!(a("gemini", &resume("x")), get("gemini").unwrap().args.join(" "));
    }

    #[test]
    fn sesion_perdida() {
        let lost = |id: &str, e: &str| session_lost(get(id).unwrap(), e);
        // Textos reales del 04/11/2026.
        assert!(lost("claude", "claude terminó con código 1\nNo conversation found with session ID: 740c0811-75f9-42fb-bdc8-11bde2193723"));
        assert!(lost("codex", "Error: thread/resume: thread/resume failed: no rollout found for thread id 019a0000-0000-7000-8000-000000000000 (code -32600)"));
        assert!(lost("copilot", "Error: No session, task, or name matched 'bacc8ace-8113-48d8-8e4e-b9b41dc38415'."));
        assert!(!lost("claude", "You've hit your limit · resets 3am"));
        assert!(!lost("codex", "error[E0599]: no method named `foo`"));
    }

    #[test]
    fn prompt_al_reanudar() {
        let claude = get("claude").unwrap();
        // Nada nuevo de otros asistentes: el mensaje tal cual, como en la CLI.
        assert_eq!(build_resume_prompt(claude, &[], "sigue"), "sigue");
        // Solo lo que no vio.
        let unseen = [msg("user", "revisa el login", None), msg("assistant", "hay un fallo en auth.rs", Some("Codex"))];
        let p = build_resume_prompt(claude, &unseen, "arréglalo");
        assert!(p.contains("[Usuario]: revisa el login"));
        assert!(p.contains("[Codex]: hay un fallo en auth.rs"));
        assert!(p.ends_with("arréglalo"));
        assert!(!p.contains("Conversación hasta ahora"));
    }
}
