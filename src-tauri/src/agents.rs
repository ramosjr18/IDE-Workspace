//! Adaptadores: cómo se ejecuta cada CLI y cómo recibe la conversación compartida.

use crate::env::{self, child_env, strip_ansi, which};
use crate::store::Message;
use serde::Serialize;
use std::path::Path;
use std::process::Stdio;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
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
}

pub struct Agent {
    pub id: &'static str,
    pub name: &'static str,
    pub cmd: &'static str,
    /// Argumentos fijos. Si `prompt_arg` es true, el mensaje va tras `-p`; si no, por stdin
    /// (así en Windows nunca pasa por la línea de comandos de cmd.exe).
    pub args: &'static [&'static str],
    pub prompt_arg: bool,
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
        args: &["-p", "--dangerously-skip-permissions"],
        prompt_arg: false,
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
        args: &["exec", "--approve-for-me", "--skip-git-repo-check", "-"],
        prompt_arg: false,
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
        args: &["run", "--auto"],
        prompt_arg: false,
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
        args: &["--dangerously-skip-permissions", "--print-timeout", "30m"],
        prompt_arg: true,
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
        max_context: MAX_CONTEXT_CHARS,
        install_unix: "npm install -g @google/gemini-cli",
        install_win: "npm install -g @google/gemini-cli",
        login: "gemini",
        status: StatusCheck::GeminiCreds,
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
        let line = format!("[{who}]: {}", m.text);
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

/// Ejecuta el agente con el prompt en la carpeta `cwd` y devuelve su respuesta.
pub async fn run(agent: &Agent, prompt: &str, cwd: &Path) -> Result<String, String> {
    let file = which(agent.cmd).ok_or_else(|| format!("{} no está instalado", agent.cmd))?;
    let mut cmd = command(&file);
    if agent.prompt_arg {
        cmd.arg("-p").arg(prompt);
    }
    cmd.args(agent.args).current_dir(cwd).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = cmd.spawn().map_err(|e| format!("No se pudo ejecutar {}: {e}", agent.cmd))?;
    let mut stdin = child.stdin.take().unwrap();
    let input = if agent.prompt_arg { String::new() } else { prompt.to_string() };
    tokio::spawn(async move {
        let _ = stdin.write_all(input.as_bytes()).await;
        let _ = stdin.shutdown().await;
    });
    let output = tokio::time::timeout(RUN_TIMEOUT, child.wait_with_output())
        .await
        .map_err(|_| "Tiempo agotado (10 min)".to_string())?
        .map_err(|e| e.to_string())?;
    let out = strip_ansi(&String::from_utf8_lossy(&output.stdout));
    if output.status.success() {
        return Ok(out.trim().to_string());
    }
    let err = strip_ansi(&String::from_utf8_lossy(&output.stderr));
    let detail = if err.trim().is_empty() { out } else { err };
    let tail: String = detail.chars().rev().take(4000).collect::<Vec<_>>().into_iter().rev().collect();
    Err(format!("{} terminó con código {}\n{}", agent.cmd, output.status.code().unwrap_or(-1), tail.trim()))
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
