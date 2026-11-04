//! Proyecto: la carpeta donde trabajan los agentes, como cuando lanzas `claude` o `code .`
//! dentro de un repo. Se elige al abrir la app:
//!   1. `agentes <carpeta>` (o la ruta como argumento del ejecutable)
//!   2. si se lanzó desde una terminal, la carpeta actual
//!   3. si se abrió desde el Finder o el menú, ninguno: la app muestra la pantalla de inicio
//!      con «Abrir carpeta…» y los proyectos recientes.

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

const MAX_RECENT: usize = 10;

fn recent_file() -> PathBuf {
    data_dir().join("recent-projects.json")
}

/// Lista guardada, la más reciente primero. Migra el `last-project` de versiones anteriores.
fn recent_paths() -> Vec<PathBuf> {
    if let Some(list) = fs::read_to_string(recent_file()).ok().and_then(|t| serde_json::from_str::<Vec<String>>(&t).ok()) {
        return list.into_iter().map(PathBuf::from).collect();
    }
    let old = data_dir().join("last-project");
    match fs::read_to_string(&old) {
        Ok(t) if !t.trim().is_empty() => vec![PathBuf::from(t.trim())],
        _ => Vec::new(),
    }
}

/// Apunta la carpeta como la más reciente.
pub fn remember(path: &Path) {
    let mut list: Vec<PathBuf> = recent_paths().into_iter().filter(|p| p != path).collect();
    list.insert(0, path.to_path_buf());
    list.truncate(MAX_RECENT);
    let json: Vec<String> = list.iter().map(|p| p.to_string_lossy().to_string()).collect();
    let _ = fs::create_dir_all(data_dir());
    if write_atomic(&recent_file(), &serde_json::to_string(&json).unwrap_or_default()).is_ok() {
        let _ = fs::remove_file(data_dir().join("last-project"));
    }
}

/// Proyectos recientes que todavía existen.
pub fn recent() -> Vec<Project> {
    recent_paths().iter().filter(|p| p.is_dir()).map(|p| Project::from_path(p)).collect()
}

/// `tauri dev` arranca el binario dentro de `src-tauri`: esa carpeta no es un proyecto.
fn is_own_source(dir: &Path) -> bool {
    cfg!(debug_assertions) && dir == Path::new(env!("CARGO_MANIFEST_DIR"))
}

/// Carpeta inicial según cómo se abrió la app. `None`: pantalla de inicio.
pub fn initial() -> Option<PathBuf> {
    initial_from(std::env::args().skip(1), std::env::current_dir().ok())
}

fn initial_from(args: impl Iterator<Item = String>, cwd: Option<PathBuf>) -> Option<PathBuf> {
    // Ruta pasada como argumento (se ignoran opciones como -psn_… de macOS antiguos).
    let mut args = args;
    if let Some(arg) = args.find(|a| !a.starts_with('-')) {
        let p = PathBuf::from(arg);
        if p.is_dir() {
            return Some(p.canonicalize().unwrap_or(p));
        }
    }
    // Lanzada desde una terminal: la carpeta actual. Desde el Finder o el menú es «/» o tu carpeta de usuario.
    let cwd = cwd?;
    let is_root = cwd.parent().is_none();
    (!is_root && cwd != home() && !is_own_source(&cwd)).then_some(cwd)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn carpeta_inicial() {
        let tmp = std::env::temp_dir();
        let none = std::iter::empty::<String>;
        // Argumento con carpeta existente
        assert_eq!(initial_from(vec![tmp.to_string_lossy().to_string()].into_iter(), None), Some(tmp.canonicalize().unwrap()));
        // Desde una terminal en una carpeta cualquiera
        assert_eq!(initial_from(none(), Some(tmp.clone())), Some(tmp.clone()));
        // Desde el menú: «/» o la carpeta de usuario → pantalla de inicio
        assert_eq!(initial_from(none(), Some(PathBuf::from("/"))), None);
        assert_eq!(initial_from(none(), Some(home())), None);
        // `tauri dev`: el propio src-tauri no es un proyecto
        assert_eq!(initial_from(none(), Some(PathBuf::from(env!("CARGO_MANIFEST_DIR")))), None);
    }
}

/// Pizarra libre del proyecto (pestaña Pizarra sin consejo abierto): `data_dir()/boards/<sha1 de la ruta>.json`.
fn board_file(project: &Path) -> PathBuf {
    let hash = sha1_smol::Sha1::from(project.to_string_lossy().as_bytes()).digest().to_string();
    data_dir().join("boards").join(format!("{hash}.json"))
}

pub fn board_get(project: &Path) -> Option<serde_json::Value> {
    fs::read_to_string(board_file(project)).ok().and_then(|s| serde_json::from_str(&s).ok())
}

pub fn board_save(project: &Path, board: &serde_json::Value) -> Result<(), String> {
    if !board.is_object() {
        return Err("La pizarra debe ser un objeto JSON".into());
    }
    let json = serde_json::to_string(board).map_err(|e| e.to_string())?;
    if json.len() > 5_000_000 {
        return Err("La pizarra es demasiado grande".into());
    }
    let file = board_file(project);
    fs::create_dir_all(file.parent().unwrap()).map_err(|e| e.to_string())?;
    write_atomic(&file, &json)
}
