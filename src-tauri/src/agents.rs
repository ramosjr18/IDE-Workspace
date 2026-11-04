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
    cmd
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
            _ = &mut deadline => { let _ = child.kill().await; return (-1, String::from_utf8_lossy(&out).into()); }
        }
        let text = String::from_utf8_lossy(&out);
        if stop_on.iter().any(|s| text.contains(s)) {
            let _ = child.kill().await;
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
            std::env::var("GEMINI_API_KEY").is_ok() || env::home().join(".gemini").join("oauth_creds.json").exists()
        }
        StatusCheck::CopilotAuth => {
            let saved = std::fs::read_to_string(env::home().join(".copilot").join("config.json"))
                .map(|t| t.contains("\"logged_in_users\"") && !t.contains("\"logged_in_users\": []"))
                .unwrap_or(false);
            let token = ["COPILOT_GITHUB_TOKEN", "GH_TOKEN", "GITHUB_TOKEN"].iter().any(|k| std::env::var(k).is_ok_and(|v| !v.is_empty()));
            saved || token || exec("gh", &["auth", "token"], short, &[]).await.0 == 0
        }
    };
    if ready { "ready" } else { "login" }
}

/// Convierte la conversación guardada en el prompt para el agente elegido. Cada respuesta
/// lleva el nombre del agente que la escribió, para que el siguiente sepa quién dijo qué.
pub fn build_prompt(agent: &Agent, messages: &[Message], message: &str) -> String {
    let mut lines: Vec<String> = Vec::new();
    let mut total = 0;
    for m in messages.iter().rev() {
        let who = if m.role == "user" { "Usuario" } else { m.agent_name.as_deref().or(m.agent.as_deref()).unwrap_or("Asistente") };
        let mut text = m.text.clone();
        if let Some(sum) = m.steps.as_deref().and_then(stream::summary) {
            text = format!("{sum}\n{text}");
        }
        let line = format!("[{who}]: {text}");
        let len = line.chars().count();
        if total + len > agent.max_context {
            break;
        }
        total += len;
        lines.push(line);
    }
    if lines.is_empty() {
        return message.to_string();
    }
    lines.reverse();
    format!(
        "Estás en un chat compartido por varios asistentes de IA. El usuario puede cambiar de asistente en cualquier momento, así que algunas respuestas anteriores pueden ser de otro asistente: cada una lleva su nombre. Tú eres {}. Continúa la conversación con naturalidad.\n\n--- Conversación hasta ahora ---\n{}\n--- Fin de la conversación ---\n\nNuevo mensaje del usuario:\n{}",
        agent.name,
        lines.join("\n\n"),
        message
    )
}

/// Resultado de una ejecución: respuesta final y pasos que se mostraron.
pub struct RunResult {
    pub text: String,
    pub steps: Vec<Step>,
    /// Uso que informó la CLI durante la ejecución (tokens, coste, peticiones).
    pub usage: crate::stream::Usage,
}

/// Ejecuta el agente con el prompt en la carpeta `cwd`. Cada paso (comando, archivo editado,
/// texto…) se pasa a `on_step` en cuanto ocurre. Si llega `cancel`, se detiene el proceso.
pub async fn run(
    agent: &Agent,
    prompt: &str,
    cwd: &Path,
    on_step: impl FnMut(&Step),
    cancel: tokio::sync::oneshot::Receiver<()>,
) -> Result<RunResult, String> {
    run_with(agent, agent.args, prompt, cwd, on_step, cancel).await
}

/// Igual que `run`, pero en modo solo lectura (para el consejo).
pub async fn run_readonly(
    agent: &Agent,
    prompt: &str,
    cwd: &Path,
    on_step: impl FnMut(&Step),
    cancel: tokio::sync::oneshot::Receiver<()>,
) -> Result<RunResult, String> {
    run_with(agent, agent.readonly_args, prompt, cwd, on_step, cancel).await
}

async fn run_with(
    agent: &Agent,
    args: &[&str],
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
    cmd.args(args).current_dir(cwd).env("PWD", cwd).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());
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
                    let _ = child.kill().await;
                    return Err("Detenido.".into());
                }
                cancel_live = false;
            }
            _ = &mut deadline => {
                let _ = child.kill().await;
                return Err("Tiempo agotado (10 min)".into());
            }
        }
    }
    let status = child.wait().await.map_err(|e| e.to_string())?;
    let err = strip_ansi(&err_task.await.unwrap_or_default());
    let text = parser.final_text();
    let usage = parser.usage();
    let steps = parser.finish();
    if status.success() {
        return Ok(RunResult { text, steps, usage });
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
