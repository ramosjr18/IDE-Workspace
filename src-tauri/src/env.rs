//! Entorno para lanzar las CLIs: PATH completo y búsqueda de ejecutables.
//!
//! Una app abierta desde el Finder (macOS) o el menú (Linux) no hereda el PATH de la
//! terminal, así que lo pedimos a la shell de login del usuario. Además añadimos las
//! carpetas donde los instaladores oficiales dejan los binarios.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::OnceLock;
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

/// Variables de entorno para los procesos hijos.
pub fn child_env() -> HashMap<String, String> {
    let mut env: HashMap<String, String> = std::env::vars().filter(|(k, _)| !k.eq_ignore_ascii_case("PATH")).collect();
    env.insert("PATH".into(), full_path().into());
    env
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
