//! Entorno para lanzar las CLIs: PATH completo y búsqueda de ejecutables.
//!
//! Una app abierta desde el Finder (macOS) o el menú (Linux) no hereda el PATH de la
//! terminal, así que lo pedimos a la shell de login del usuario. Además añadimos las
//! carpetas donde los instaladores oficiales dejan los binarios.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

pub const IS_WIN: bool = cfg!(windows);

pub fn home() -> PathBuf {
    dirs::home_dir().unwrap_or_else(|| PathBuf::from("."))
}

fn path_sep() -> &'static str {
    if IS_WIN { ";" } else { ":" }
}

/// PATH de la shell de login (macOS/Linux). Vacío si no se puede obtener en 5 s.
fn login_shell_path() -> Option<String> {
    if IS_WIN {
        return None;
    }
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".into());
    let mut child = Command::new(shell)
        .args(["-ilc", "printf '__PATH__%s__PATH__' \"$PATH\""])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let start = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if start.elapsed() < Duration::from_secs(5) => std::thread::sleep(Duration::from_millis(50)),
            _ => {
                let _ = child.kill();
                return None;
            }
        }
    }
    let out = child.wait_with_output().ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    let mut parts = text.split("__PATH__");
    parts.next();
    parts.next().map(str::to_string).filter(|p| !p.is_empty())
}

fn npm_global_bin(path: &str) -> Option<PathBuf> {
    let npm = which_in("npm", path)?;
    let mut cmd = Command::new(npm);
    cmd.args(["prefix", "-g"]).env("PATH", path).stdin(Stdio::null()).stderr(Stdio::null());
    no_window(&mut cmd);
    let out = cmd.output().ok()?;
    let prefix = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if prefix.is_empty() {
        return None;
    }
    Some(if IS_WIN { PathBuf::from(prefix) } else { Path::new(&prefix).join("bin") })
}

/// PATH completo que se usa para todas las CLIs (se calcula una vez).
pub fn full_path() -> &'static str {
    static PATH: OnceLock<String> = OnceLock::new();
    PATH.get_or_init(|| {
        let mut dirs: Vec<String> = Vec::new();
        fn push(dirs: &mut Vec<String>, p: String) {
            for d in p.split(path_sep()) {
                if !d.is_empty() && !dirs.iter().any(|x| x == d) {
                    dirs.push(d.to_string());
                }
            }
        }
        if let Some(p) = login_shell_path() {
            push(&mut dirs, p);
        }
        if let Ok(p) = std::env::var("PATH") {
            push(&mut dirs, p);
        }
        let h = home();
        push(&mut dirs, h.join(".local").join("bin").to_string_lossy().into());
        push(&mut dirs, h.join(".opencode").join("bin").to_string_lossy().into());
        push(&mut dirs, h.join(".cargo").join("bin").to_string_lossy().into());
        if IS_WIN {
            if let Ok(local) = std::env::var("LOCALAPPDATA") {
                push(&mut dirs, Path::new(&local).join("agy").join("bin").to_string_lossy().into());
            }
        } else {
            push(&mut dirs, "/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin".into());
        }
        let base = dirs.join(path_sep());
        if let Some(npm_bin) = npm_global_bin(&base) {
            push(&mut dirs, npm_bin.to_string_lossy().into());
        }
        dirs.join(path_sep())
    })
}

tokio::task_local! {
    /// Conversación del chat que está ejecutando el agente: sus procesos hijos la reciben en AGENTES_CONVERSACION.
    static CONVERSATION: String;
}

/// Ejecuta `f` con la conversación `id` activa (los procesos que lance llevarán AGENTES_CONVERSACION).
pub async fn conversation_scope<F: std::future::Future>(id: String, f: F) -> F::Output {
    CONVERSATION.scope(id, f).await
}

/// Proyecto abierto en la app: sus procesos hijos lo reciben en AGENTES_PROYECTO.
static PROJECT: Mutex<Option<PathBuf>> = Mutex::new(None);

/// La app cambia de proyecto (o abre el primero).
pub fn set_project(dir: Option<PathBuf>) {
    *PROJECT.lock().unwrap_or_else(|e| e.into_inner()) = dir;
}

/// Variables de entorno para los procesos hijos.
pub fn child_env() -> HashMap<String, String> {
    let mut env: HashMap<String, String> = std::env::vars().filter(|(k, _)| !k.eq_ignore_ascii_case("PATH")).collect();
    let mut path = full_path().to_string();
    // Los procesos que lanza la app (agentes del chat, terminales, paneles) pueden usar `agentes run`.
    if let Some(l) = LAUNCHER.get() {
        path = format!("{}{}{}", l.shim_dir.display(), path_sep(), path);
        env.insert("AGENTES_ADDR".into(), l.addr.clone());
        env.insert("AGENTES_TOKEN".into(), l.token.clone());
    }
    env.insert("PATH".into(), path);
    // Cuenta activa de la tarea (varias cuentas por CLI): CLAUDE_CONFIG_DIR, CODEX_HOME…
    for (k, v) in crate::accounts::active_env() {
        env.insert(k, v);
    }
    // Proyecto de la app: `agentes pizarra` / `consejo` lo usan si el agente hace `cd` fuera de él.
    if let Some(p) = PROJECT.lock().unwrap_or_else(|e| e.into_inner()).as_ref() {
        env.insert("AGENTES_PROYECTO".into(), p.to_string_lossy().to_string());
    }
    // Conversación del chat que lanza el proceso: `agentes run` / `agentes consejo` la mandan a la app para avisarla al terminar.
    if let Ok(id) = CONVERSATION.try_with(|c| c.clone()) {
        if !id.is_empty() {
            env.insert("AGENTES_CONVERSACION".into(), id);
        }
    }
    env
}

/// Datos del lanzador de agentes (`agentes run`): dónde escucha la app y el token de esta sesión.
pub struct Launcher {
    pub addr: String,
    pub token: String,
    pub shim_dir: PathBuf,
}

static LAUNCHER: OnceLock<Launcher> = OnceLock::new();

pub fn set_launcher(l: Launcher) {
    let _ = LAUNCHER.set(l);
}

pub fn launcher_ready() -> bool {
    LAUNCHER.get().is_some()
}

fn which_in(cmd: &str, path: &str) -> Option<PathBuf> {
    let exts: Vec<String> = if IS_WIN {
        std::env::var("PATHEXT")
            .unwrap_or_else(|_| ".EXE;.CMD;.BAT".into())
            .split(';')
            .map(|e| e.to_lowercase())
            .collect()
    } else {
        vec![String::new()]
    };
    // En Windows, si `cmd` ya trae extensión (p. ej. "powershell.exe"), pruébala como está
    // en cada carpeta del PATH antes de añadir extensiones de PATHEXT (evita "powershell.exe.exe").
    if IS_WIN && Path::new(cmd).extension().is_some() {
        for dir in path.split(path_sep()) {
            let file = Path::new(dir).join(cmd);
            if file.is_file() {
                return Some(file);
            }
        }
    }
    for dir in path.split(path_sep()) {
        for ext in &exts {
            let file = Path::new(dir).join(format!("{cmd}{ext}"));
            if file.is_file() {
                return Some(file);
            }
        }
    }
    None
}

/// Busca un ejecutable en el PATH completo (en Windows también prueba .exe, .cmd…).
pub fn which(cmd: &str) -> Option<PathBuf> {
    which_in(cmd, full_path())
}

/// En Windows, que no aparezca una consola por cada proceso.
pub fn no_window(cmd: &mut Command) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000);
    }
    #[cfg(not(windows))]
    let _ = cmd;
}

/// Quita los códigos de color ANSI de la salida de una CLI.
pub fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\x1b' && chars.peek() == Some(&'[') {
            chars.next();
            while let Some(&n) = chars.peek() {
                chars.next();
                if n.is_ascii_alphabetic() {
                    break;
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// Shell interactiva del usuario para la pestaña Terminal: `$SHELL -l` en macOS/Linux; PowerShell (o cmd) en Windows.
pub fn interactive_shell() -> (PathBuf, Vec<String>) {
    if IS_WIN {
        for sh in ["pwsh", "powershell"] {
            if let Some(p) = which(sh) {
                return (p, vec!["-NoLogo".into()]);
            }
        }
        return (PathBuf::from(std::env::var("COMSPEC").unwrap_or_else(|_| "cmd.exe".into())), Vec::new());
    }
    let shell = std::env::var("SHELL").ok().filter(|s| Path::new(s).is_file()).unwrap_or_else(|| "/bin/bash".into());
    (PathBuf::from(shell), vec!["-l".into()])
}
