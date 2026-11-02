//! Panel derecho: árbol de archivos del proyecto (worktree), contenido de un archivo con
//! las líneas cambiadas respecto a git, y vigilancia de la carpeta para saber qué archivo
//! se está editando (evento "fs-changed").

use crate::env::{child_env, no_window, which};
use notify_debouncer_mini::notify::RecursiveMode;
use notify_debouncer_mini::{new_debouncer, DebounceEventResult, Debouncer};
use serde::Serialize;
use std::collections::HashMap;
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::time::Duration;
use tauri::{AppHandle, Emitter};

/// Carpetas que nunca se muestran ni se vigilan.
const SKIP_DIRS: &[&str] = &[".git", "node_modules", "target", ".next", "dist", "build", ".venv", "venv", "__pycache__", ".DS_Store"];
const MAX_FILES: usize = 20_000;
const MAX_FILE_BYTES: u64 = 2 * 1024 * 1024;

fn git(dir: &Path, args: &[&str]) -> Option<String> {
    let file = which("git")?;
    let mut cmd = Command::new(file);
    cmd.args(args).current_dir(dir).env_clear().envs(child_env()).stdin(Stdio::null()).stderr(Stdio::null());
    no_window(&mut cmd);
    let out = cmd.output().ok()?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).to_string())
}

fn skipped(rel: &Path) -> bool {
    rel.components().any(|c| matches!(c, Component::Normal(n) if SKIP_DIRS.iter().any(|s| n == *s)))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Tree {
    pub is_git: bool,
    pub branch: Option<String>,
    /// Rutas relativas con «/» como separador.
    pub files: Vec<String>,
    /// Estado git por ruta: "M" modificado, "A" nuevo, "D" borrado, "R" renombrado, "?" sin seguimiento.
    pub status: HashMap<String, String>,
    pub truncated: bool,
}

fn walk(root: &Path, dir: &Path, out: &mut Vec<String>) {
    let Ok(rd) = fs::read_dir(dir) else { return };
    let mut entries: Vec<_> = rd.filter_map(|e| e.ok()).collect();
    entries.sort_by_key(|e| e.file_name());
    for e in entries {
        if out.len() >= MAX_FILES {
            return;
        }
        let path = e.path();
        let rel = path.strip_prefix(root).unwrap_or(&path);
        if skipped(rel) {
            continue;
        }
        match e.file_type() {
            Ok(t) if t.is_dir() => walk(root, &path, out),
            Ok(t) if t.is_file() => out.push(rel.to_string_lossy().replace('\\', "/")),
            _ => {}
        }
    }
}

pub fn tree(root: &Path) -> Tree {
    // En un repo git se usan los archivos del repo (respetando .gitignore); si no, se recorre la carpeta.
    let listed = git(root, &["ls-files", "--cached", "--others", "--exclude-standard", "-z"]);
    let is_git = listed.is_some();
    let mut files: Vec<String> = match listed {
        Some(text) => text.split('\0').filter(|f| !f.is_empty() && !skipped(Path::new(f))).map(str::to_string).collect(),
        None => {
            let mut out = Vec::new();
            walk(root, root, &mut out);
            out
        }
    };
    let mut status = HashMap::new();
    if is_git {
        if let Some(text) = git(root, &["status", "--porcelain=v1", "-z", "--untracked-files=all"]) {
            let mut parts = text.split('\0');
            while let Some(entry) = parts.next() {
                if entry.len() < 4 {
                    continue;
                }
                let (code, path) = entry.split_at(3);
                let code = code.trim();
                let letter = if code == "??" {
                    "?"
                } else if code.contains('D') {
                    "D"
                } else if code.contains('R') {
                    parts.next(); // ruta original del renombrado
                    "R"
                } else if code.contains('A') {
                    "A"
                } else {
                    "M"
                };
                status.insert(path.to_string(), letter.to_string());
                // Los borrados no aparecen en ls-files: se añaden para verlos en el árbol.
                if letter == "D" && !files.iter().any(|f| f == path) {
                    files.push(path.to_string());
                }
            }
        }
    }
    files.sort();
    files.dedup();
    let truncated = files.len() >= MAX_FILES;
    files.truncate(MAX_FILES);
    let branch = if is_git { git(root, &["branch", "--show-current"]).map(|b| b.trim().to_string()).filter(|b| !b.is_empty()) } else { None };
    Tree { is_git, branch, files, status, truncated }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileView {
    pub path: String,
    pub content: Option<String>,
    /// Motivo por el que no se muestra el contenido (binario, demasiado grande, borrado…).
    pub notice: Option<String>,
    /// Marcas del margen como en VS Code, respecto al último commit (líneas desde 1):
    /// añadidas, modificadas, y líneas tras las que se borró algo (0 = al principio).
    pub added: Vec<u32>,
    pub modified: Vec<u32>,
    pub deleted: Vec<u32>,
    /// Número de líneas borradas.
    pub deleted_count: u32,
    /// El archivo está en git (se puede comparar con el último commit).
    pub tracked: bool,
}

/// Resuelve una ruta relativa dentro del proyecto, sin dejar salir de él.
fn inside(root: &Path, rel: &str) -> Result<PathBuf, String> {
    let rel_path = Path::new(rel);
    if rel_path.is_absolute() || rel_path.components().any(|c| matches!(c, Component::ParentDir)) {
        return Err("Ruta fuera del proyecto".into());
    }
    Ok(root.join(rel_path))
}

#[derive(Default)]
struct Marks {
    added: Vec<u32>,
    modified: Vec<u32>,
    deleted: Vec<u32>,
    deleted_count: u32,
}

/// Marcas del margen según `git diff -U0 HEAD`, como el «dirty diff» de VS Code.
fn diff_marks(root: &Path, rel: &str) -> Marks {
    let mut m = Marks::default();
    let Some(diff) = git(root, &["diff", "--no-color", "-U0", "HEAD", "--", rel]) else { return m };
    for line in diff.lines().filter(|l| l.starts_with("@@")) {
        // @@ -a,b +c,d @@  (si falta «,n», n = 1)
        let mut parts = line.split_whitespace().skip(1);
        let parse = |s: Option<&str>| -> (u32, u32) {
            let s = s.unwrap_or("").trim_start_matches(['-', '+']);
            let mut it = s.split(',');
            let start = it.next().and_then(|n| n.parse().ok()).unwrap_or(0);
            let count = it.next().and_then(|n| n.parse().ok()).unwrap_or(1);
            (start, count)
        };
        let (_, old_count) = parse(parts.next());
        let (new_start, new_count) = parse(parts.next());
        if new_count == 0 {
            m.deleted.push(new_start);
            m.deleted_count += old_count;
        } else if old_count == 0 {
            m.added.extend(new_start..new_start + new_count);
        } else {
            let shared = old_count.min(new_count);
            m.modified.extend(new_start..new_start + shared);
            m.added.extend(new_start + shared..new_start + new_count);
            if old_count > new_count {
                m.deleted.push(new_start + new_count - 1);
                m.deleted_count += old_count - new_count;
            }
        }
    }
    m
}

/// Contenido del archivo en el último commit (para la vista de cambios).
pub fn head(root: &Path, rel: &str) -> Result<String, String> {
    inside(root, rel)?;
    Ok(git(root, &["show", &format!("HEAD:{rel}")]).unwrap_or_default())
}

/// Guarda el archivo (desde el editor).
pub fn write(root: &Path, rel: &str, content: &str) -> Result<(), String> {
    let file = inside(root, rel)?;
    fs::write(&file, content).map_err(|e| format!("No se pudo guardar {rel}: {e}"))
}

pub fn is_git(root: &Path) -> bool {
    git(root, &["rev-parse", "--is-inside-work-tree"]).is_some()
}

pub fn read(root: &Path, rel: &str, is_git: bool) -> Result<FileView, String> {
    let file = inside(root, rel)?;
    let mut view = FileView {
        path: rel.to_string(),
        content: None,
        notice: None,
        added: vec![],
        modified: vec![],
        deleted: vec![],
        deleted_count: 0,
        tracked: false,
    };
    let Ok(meta) = fs::metadata(&file) else {
        view.notice = Some("Este archivo se ha borrado.".into());
        return Ok(view);
    };
    if meta.len() > MAX_FILE_BYTES {
        view.notice = Some(format!("Archivo demasiado grande para mostrarlo ({} KB).", meta.len() / 1024));
        return Ok(view);
    }
    let bytes = fs::read(&file).map_err(|e| e.to_string())?;
    if bytes.iter().take(8000).any(|&b| b == 0) {
        view.notice = Some("Archivo binario.".into());
        return Ok(view);
    }
    view.content = Some(String::from_utf8_lossy(&bytes).to_string());
    if is_git {
        let tracked = git(root, &["ls-files", "--error-unmatch", "--", rel]).is_some();
        view.tracked = tracked;
        if tracked {
            let m = diff_marks(root, rel);
            view.added = m.added;
            view.modified = m.modified;
            view.deleted = m.deleted;
            view.deleted_count = m.deleted_count;
        } else {
            // Archivo nuevo: todas sus líneas son nuevas.
            let n = view.content.as_deref().map(|c| c.lines().count()).unwrap_or(0) as u32;
            view.added = (1..=n).collect();
        }
    }
    Ok(view)
}

#[derive(Serialize, Clone)]
struct Changed {
    /// Archivos cambiados (rutas relativas), del más reciente al más antiguo.
    files: Vec<String>,
}

/// Vigila la carpeta del proyecto. Se sustituye al cambiar de proyecto.
#[derive(Default)]
pub struct Watcher(Mutex<Option<Debouncer<notify_debouncer_mini::notify::RecommendedWatcher>>>);

impl Watcher {
    pub fn watch(&self, app: AppHandle, root: PathBuf) -> Result<(), String> {
        let base = root.clone();
        let mut debouncer = new_debouncer(Duration::from_millis(300), move |res: DebounceEventResult| {
            let Ok(events) = res else { return };
            let mut files: Vec<String> = Vec::new();
            for e in events.iter().rev() {
                let Ok(rel) = e.path.strip_prefix(&base) else { continue };
                if rel.as_os_str().is_empty() || skipped(rel) || e.path.is_dir() {
                    continue;
                }
                let rel = rel.to_string_lossy().replace('\\', "/");
                if !files.contains(&rel) {
                    files.push(rel);
                }
            }
            // Cambios dentro de .git (commit, checkout…) también refrescan el árbol.
            let git_changed = events.iter().any(|e| e.path.strip_prefix(&base).is_ok_and(|r| r.starts_with(".git")));
            if !files.is_empty() || git_changed {
                let _ = app.emit("fs-changed", Changed { files });
            }
        })
        .map_err(|e| e.to_string())?;
        debouncer.watcher().watch(&root, RecursiveMode::Recursive).map_err(|e| e.to_string())?;
        *self.0.lock().unwrap() = Some(debouncer);
        Ok(())
    }
}
