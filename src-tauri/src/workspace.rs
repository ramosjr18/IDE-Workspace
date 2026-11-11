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
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};
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
pub struct Watcher {
    debouncer: Mutex<Option<Debouncer<notify_debouncer_mini::notify::RecommendedWatcher>>>,
    /// Parada del sondeo del proyecto anterior (ver `needs_polling`).
    poll_stop: Mutex<Option<Arc<AtomicBool>>>,
}

/// En WSL, las carpetas de Windows (`/mnt/c/...`) no dan avisos de inotify, ni siquiera de los
/// cambios hechos desde el propio WSL: hay que mirar cada poco.
fn needs_polling(root: &Path) -> bool {
    cfg!(target_os = "linux")
        && root.starts_with("/mnt/")
        && fs::read_to_string("/proc/sys/kernel/osrelease").is_ok_and(|r| r.to_lowercase().contains("microsoft"))
}

/// Huella barata del proyecto para el sondeo: en git, los archivos con cambios (estado, fecha y
/// tamaño) y la rama; sin git, todos los archivos. Ruta → firma.
fn snapshot(root: &Path) -> HashMap<String, String> {
    let stamp = |rel: &str| {
        fs::metadata(root.join(rel))
            .map(|m| format!("{}:{:?}", m.len(), m.modified().unwrap_or(SystemTime::UNIX_EPOCH)))
            .unwrap_or_default()
    };
    let mut snap = HashMap::new();
    // --no-optional-locks: sin reescribir .git/index, que no se toque nada solo por mirar.
    match git(root, &["--no-optional-locks", "status", "--porcelain=v1", "-z", "--untracked-files=all"]) {
        Some(text) => {
            let mut parts = text.split(' ');
            while let Some(entry) = parts.next() {
                if entry.len() < 4 {
                    continue;
                }
                let (code, path) = entry.split_at(3);
                if code.contains('R') || code.contains('C') {
                    parts.next();
                }
                if !skipped(Path::new(path)) {
                    snap.insert(path.to_string(), format!("{code}{}", stamp(path)));
                }
            }
            snap.insert(".git/HEAD".into(), fs::read_to_string(root.join(".git/HEAD")).unwrap_or_default());
            snap.insert(".git/head-commit".into(), git(root, &["rev-parse", "HEAD"]).unwrap_or_default());
        }
        None => {
            let mut files = Vec::new();
            walk(root, root, &mut files);
            for f in files {
                let st = stamp(&f);
                snap.insert(f, st);
            }
        }
    }
    snap
}

/// Rutas que cambian entre dos huellas (las de `.git` aparte, como en inotify). Un archivo que
/// solo sale de `git status` (p. ej. al hacer commit) no ha cambiado en disco: refresca el árbol,
/// pero no se cuenta como archivo cambiado.
fn snapshot_diff(old: &HashMap<String, String>, new: &HashMap<String, String>) -> (Vec<String>, bool) {
    let mut files = Vec::new();
    let mut git_changed = false;
    for k in old.keys().chain(new.keys()) {
        if old.get(k) != new.get(k) {
            if k.starts_with(".git/") {
                git_changed = true;
            } else if !files.contains(k) && new.contains_key(k) {
                files.push(k.clone());
            }
        }
    }
    (files, git_changed)
}

impl Watcher {
    pub fn watch(&self, app: AppHandle, root: PathBuf) -> Result<(), String> {
        if let Some(old) = self.poll_stop.lock().unwrap().take() {
            old.store(true, Ordering::Relaxed);
        }
        if needs_polling(&root) {
            let stop = Arc::new(AtomicBool::new(false));
            *self.poll_stop.lock().unwrap() = Some(stop.clone());
            let (app, root) = (app.clone(), root.clone());
            std::thread::spawn(move || {
                let mut last = snapshot(&root);
                while !stop.load(Ordering::Relaxed) {
                    std::thread::sleep(Duration::from_secs(2));
                    if stop.load(Ordering::Relaxed) {
                        break;
                    }
                    let now = snapshot(&root);
                    let (files, git_changed) = snapshot_diff(&last, &now);
                    if files.is_empty() && !git_changed && last.len() == now.len() {
                        continue;
                    }
                    let _ = app.emit("fs-changed", Changed { files });
                    last = now;
                }
            });
        }
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
        *self.debouncer.lock().unwrap() = Some(debouncer);
        Ok(())
    }
}

/// Solo en desarrollo (`npm run dev`): recarga la ventana al cambiar algo en `ui/` (sin `ui/vendor`).
/// `tauri dev` sirve `ui/` como archivos estáticos y no recarga solo.
#[cfg(debug_assertions)]
pub fn reload_on_ui_change(app: AppHandle) {
    use tauri::Manager;
    let ui = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("ui");
    let Ok(ui) = ui.canonicalize() else { return };
    let vendor = ui.join("vendor");
    // Solo cuenta que cambie la fecha de modificación: abrir o leer los archivos (lo hace la propia
    // recarga) también genera eventos y provocaría un bucle de recargas.
    fn newest(dir: &std::path::Path, skip: &std::path::Path) -> Option<std::time::SystemTime> {
        let mut best = None;
        for e in fs::read_dir(dir).ok()?.flatten() {
            let p = e.path();
            if p.starts_with(skip) {
                continue;
            }
            let t = if p.is_dir() { newest(&p, skip) } else { e.metadata().ok().and_then(|m| m.modified().ok()) };
            best = best.max(t);
        }
        best
    }
    let last = std::sync::Mutex::new(newest(&ui, &vendor));
    let (handle, root, skip) = (app.clone(), ui.clone(), vendor.clone());
    let debouncer = new_debouncer(Duration::from_millis(250), move |res: DebounceEventResult| {
        let Ok(events) = res else { return };
        if !events.iter().any(|e| !e.path.starts_with(&skip)) {
            return;
        }
        let now = newest(&root, &skip);
        let mut last = last.lock().unwrap();
        if now > *last {
            *last = now;
            for w in handle.webview_windows().values() {
                let _ = w.eval("location.reload()");
            }
        }
    });
    if let Ok(mut d) = debouncer {
        if d.watcher().watch(&ui, RecursiveMode::Recursive).is_ok() {
            // Vive mientras viva la app.
            std::mem::forget(d);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snap(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
    }

    #[test]
    fn diff_de_huellas() {
        let antes = snap(&[("a.txt", " M1"), ("b.txt", "??1"), (".git/HEAD", "main")]);
        // Sin cambios: nada.
        assert_eq!(snapshot_diff(&antes, &antes), (vec![], false));
        // Editado otra vez, nuevo y commit de b (sale de status): solo cuentan los que siguen en disco.
        let despues = snap(&[("a.txt", " M2"), ("c.txt", "??1"), (".git/HEAD", "main")]);
        let (mut files, git) = snapshot_diff(&antes, &despues);
        files.sort();
        assert_eq!(files, vec!["a.txt", "c.txt"]);
        assert!(!git);
        // Cambio de rama.
        let rama = snap(&[("a.txt", " M1"), ("b.txt", "??1"), (".git/HEAD", "otra")]);
        assert_eq!(snapshot_diff(&antes, &rama), (vec![], true));
    }

    #[test]
    fn huella_de_un_repo_ve_las_ediciones() {
        let dir = std::env::temp_dir().join(format!("huella-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        if git(&dir, &["init", "-q"]).is_none() {
            return; // sin git en esta máquina
        }
        fs::write(dir.join("a.txt"), "uno").unwrap();
        let s1 = snapshot(&dir);
        assert!(s1.contains_key("a.txt"));
        std::thread::sleep(Duration::from_millis(20));
        fs::write(dir.join("a.txt"), "uno y dos").unwrap();
        let s2 = snapshot(&dir);
        assert_eq!(snapshot_diff(&s1, &s2).0, vec!["a.txt"]);
        let _ = fs::remove_dir_all(&dir);
    }
}
