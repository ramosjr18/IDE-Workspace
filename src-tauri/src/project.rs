//! Proyecto: la carpeta donde trabajan los agentes, como cuando lanzas `claude` o `code .`
//! dentro de un repo. Se elige al abrir la app:
//!   1. `agentes <carpeta>` (o la ruta como argumento del ejecutable)
//!   2. si se lanzó desde una terminal, la carpeta actual
//!   3. si se abrió desde el Finder o el menú, el último proyecto usado (o tu carpeta de usuario)

use crate::env::{home, IS_WIN};
use crate::store::{data_dir, write_atomic};
use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Serialize, Clone)]
pub struct Project {
    pub path: String,
    pub name: String,
}

impl Project {
    pub fn from_path(path: &Path) -> Self {
        let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| path.display().to_string());
        Project { path: path.to_string_lossy().to_string(), name }
    }
}

fn last_file() -> PathBuf {
    data_dir().join("last-project")
}

pub fn remember(path: &Path) {
    let _ = fs::create_dir_all(data_dir());
    let _ = write_atomic(&last_file(), &path.to_string_lossy());
}

fn last() -> Option<PathBuf> {
    let p = PathBuf::from(fs::read_to_string(last_file()).ok()?.trim());
    p.is_dir().then_some(p)
}

/// Carpeta inicial según cómo se abrió la app.
pub fn initial() -> PathBuf {
    // Ruta pasada como argumento (se ignoran opciones como -psn_… de macOS antiguos).
    if let Some(arg) = std::env::args().skip(1).find(|a| !a.starts_with('-')) {
        let p = PathBuf::from(arg);
        if p.is_dir() {
            return p.canonicalize().unwrap_or(p);
        }
    }
    // Lanzada desde una terminal: la carpeta actual. Desde el Finder la carpeta actual es «/».
    if let Ok(cwd) = std::env::current_dir() {
        let is_root = cwd.parent().is_none();
        if !is_root && cwd != home() {
            return cwd;
        }
    }
    last().unwrap_or_else(home)
}

/// Instala el comando `agentes` para abrir la app en una carpeta desde la terminal.
/// Devuelve un texto explicando dónde se instaló y cómo usarlo.
pub fn install_cli() -> Result<String, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let exe_s = exe.to_string_lossy().to_string();
    if IS_WIN {
        // %LOCALAPPDATA%\Microsoft\WindowsApps ya está en el PATH de cada usuario.
        let dir = PathBuf::from(std::env::var("LOCALAPPDATA").map_err(|e| e.to_string())?).join("Microsoft").join("WindowsApps");
        let file = dir.join("agentes.cmd");
        let script = format!(
            "@echo off\r\nrem Abre «Chat de agentes» en la carpeta indicada (por defecto, la actual)\r\nif \"%~1\"==\"\" (set \"dir=%CD%\") else (set \"dir=%~f1\")\r\nstart \"\" \"{exe_s}\" \"%dir%\"\r\n"
        );
        fs::write(&file, script).map_err(|e| e.to_string())?;
        return Ok(format!("Instalado en {}. Abre una terminal nueva y escribe «agentes» dentro de un repo.", file.display()));
    }

    let launch = if cfg!(target_os = "macos") {
        // Ruta del .app: …/Chat de agentes.app/Contents/MacOS/<binario>
        let app = exe.ancestors().nth(3).filter(|p| p.extension().is_some_and(|e| e == "app"));
        match app {
            Some(app) => {
                let app = app.to_string_lossy();
                if app.starts_with("/Volumes/") || app.contains("/AppTranslocation/") {
                    return Err("Primero mueve «Chat de agentes» a la carpeta Aplicaciones y ábrela desde allí.".into());
                }
                format!("exec open -n -a \"{app}\" --args \"$dir\"")
            }
            None => format!("nohup \"{exe_s}\" \"$dir\" >/dev/null 2>&1 &"),
        }
    } else {
        format!("nohup \"{exe_s}\" \"$dir\" >/dev/null 2>&1 &")
    };
    let script = format!(
        "#!/bin/sh\n# Abre «Chat de agentes» en la carpeta indicada (por defecto, la actual)\ndir=$(cd \"${{1:-.}}\" 2>/dev/null && pwd) || {{ echo \"No existe la carpeta: $1\" >&2; exit 1; }}\n{launch}\n"
    );
    let dir = home().join(".local").join("bin");
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let file = dir.join("agentes");
    fs::write(&file, script).map_err(|e| e.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&file, fs::Permissions::from_mode(0o755)).map_err(|e| e.to_string())?;
    }
    let in_path = std::env::var("PATH").unwrap_or_default().split(':').chain(crate::env::full_path().split(':')).any(|d| Path::new(d) == dir);
    let mut msg = format!("Instalado en {}. Escribe «agentes» dentro de un repo (o «agentes ruta/del/repo»).", file.display());
    if !in_path {
        msg.push_str(" Esa carpeta no está en tu PATH: añade export PATH=\"$HOME/.local/bin:$PATH\" a tu ~/.zshrc o ~/.bashrc.");
    }
    Ok(msg)
}
