//! Varias cuentas por CLI: carpetas de sesión independientes, reparto por cuota y relevo.
//! Diseño: docs/plan-cuentas.md
//!
//! Una cuenta es la carpeta donde una CLI guarda su sesión más la variable que la elige
//! (`CLAUDE_CONFIG_DIR`, `CODEX_HOME`…). La «principal» es la de siempre (`~/.claude`, `~/.codex`…).
//! La cuenta activa de una tarea se fija con [`scope`] / [`scope_sync`]: los procesos que se lanzan
//! dentro reciben su entorno y los colectores de uso leen su carpeta.

pub mod exhaustion;

use crate::store::data_dir;
use serde::Serialize;
use std::cell::RefCell;
use std::path::{Path, PathBuf};

/// Por encima de este uso (límite más alto) no se lanza nada nuevo en una cuenta.
pub const THRESHOLD: f64 = 0.90;

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Account {
    /// `codex` (principal) o `codex:cuenta-2`.
    pub id: String,
    pub cli: String,
    /// `None` = principal.
    pub name: Option<String>,
    #[serde(skip)]
    pub dir: Option<PathBuf>,
}

impl Account {
    pub fn principal(cli: &str) -> Self {
        Account { id: cli.to_string(), cli: cli.to_string(), name: None, dir: None }
    }

    pub fn label(&self) -> String {
        self.name.clone().unwrap_or_else(|| "principal".into())
    }

    /// Variables que eligen esta cuenta. La principal no añade nada.
    pub fn env(&self) -> Vec<(String, String)> {
        let Some(dir) = &self.dir else { return Vec::new() };
        let d = |p: &Path| p.to_string_lossy().to_string();
        match self.cli.as_str() {
            "claude" => vec![("CLAUDE_CONFIG_DIR".into(), d(dir))],
            "codex" => vec![("CODEX_HOME".into(), d(dir))],
            // Solo los datos (auth.json y opencode.db): XDG_CONFIG_HOME cambiaría también la config de git, mise…
            "opencode" => vec![("XDG_DATA_HOME".into(), d(&dir.join("data")))],
            "gemini" => vec![("GEMINI_CLI_HOME".into(), d(dir))],
            _ => Vec::new(),
        }
    }

    /// Carpeta de sesión de esta cuenta para los colectores de uso (`None`: la de siempre).
    pub fn claude_dir(&self) -> Option<PathBuf> {
        self.dir.clone().filter(|_| self.cli == "claude")
    }
    pub fn codex_dir(&self) -> Option<PathBuf> {
        self.dir.clone().filter(|_| self.cli == "codex")
    }
    pub fn opencode_data(&self) -> Option<PathBuf> {
        self.dir.as_ref().filter(|_| self.cli == "opencode").map(|d| d.join("data"))
    }
}

/// CLIs que admiten cuentas extra. Antigravity y Copilot: su carpeta de sesión aún no está verificada.
pub fn supports_accounts(cli: &str) -> bool {
    matches!(cli, "claude" | "codex" | "opencode" | "gemini")
}

fn root() -> PathBuf {
    data_dir().join("cuentas")
}

fn valid_name(name: &str) -> bool {
    !name.is_empty() && name.len() <= 40 && name != "principal" && name.chars().all(|c| c.is_ascii_alphanumeric() || "-_".contains(c))
}

/// Cuentas de una CLI: la principal primero y luego las extra por nombre.
pub fn list(cli: &str) -> Vec<Account> {
    list_in(&root(), cli)
}

fn list_in(root: &Path, cli: &str) -> Vec<Account> {
    let mut out = vec![Account::principal(cli)];
    if !supports_accounts(cli) {
        return out;
    }
    let mut extra: Vec<Account> = std::fs::read_dir(root.join(cli))
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.path().is_dir())
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().to_string();
            valid_name(&name).then(|| Account { id: format!("{cli}:{name}"), cli: cli.to_string(), name: Some(name), dir: Some(e.path()) })
        })
        .collect();
    extra.sort_by(|a, b| a.name.cmp(&b.name));
    out.extend(extra);
    out
}

pub fn get(id: &str) -> Option<Account> {
    let cli = id.split(':').next()?;
    list(cli).into_iter().find(|a| a.id == id)
}

/// Crea la carpeta de una cuenta nueva (vacía: no se comparte nada con la principal).
pub fn add(cli: &str, name: &str) -> Result<Account, String> {
    add_in(&root(), cli, name)
}

fn add_in(root: &Path, cli: &str, name: &str) -> Result<Account, String> {
    if !supports_accounts(cli) {
        return Err(format!("«{cli}» todavía no admite varias cuentas"));
    }
    if !valid_name(name) {
        return Err("Nombre no válido: letras, números, «-» y «_» (máx. 40)".into());
    }
    let dir = root.join(cli).join(name);
    if dir.exists() {
        return Err(format!("Ya existe la cuenta «{name}»"));
    }
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    if cli == "opencode" {
        std::fs::create_dir_all(dir.join("data")).map_err(|e| e.to_string())?;
    }
    Ok(Account { id: format!("{cli}:{name}"), cli: cli.into(), name: Some(name.into()), dir: Some(dir) })
}

/// Borra una cuenta extra (su sesión se pierde). La principal no se puede borrar.
pub fn remove(id: &str) -> Result<(), String> {
    remove_in(&root(), id)
}

fn remove_in(root: &Path, id: &str) -> Result<(), String> {
    let cli = id.split(':').next().unwrap_or_default();
    let acc = list_in(root, cli).into_iter().find(|a| a.id == id).ok_or("Cuenta no encontrada")?;
    let dir = acc.dir.ok_or("La cuenta principal no se puede borrar")?;
    // Defensa: solo dentro de data_dir()/cuentas.
    if !dir.starts_with(root) {
        return Err("Ruta de cuenta inesperada".into());
    }
    std::fs::remove_dir_all(dir).map_err(|e| e.to_string())
}

// ---------- Cuenta activa de la tarea en curso ----------

tokio::task_local! {
    static ACTIVE: Account;
}
thread_local! {
    static ACTIVE_THREAD: RefCell<Option<Account>> = const { RefCell::new(None) };
}

pub fn active() -> Option<Account> {
    ACTIVE_THREAD.with(|a| a.borrow().clone()).or_else(|| ACTIVE.try_with(|a| a.clone()).ok())
}

/// Entorno de la cuenta activa (vacío si no hay o es la principal).
pub fn active_env() -> Vec<(String, String)> {
    active().map(|a| a.env()).unwrap_or_default()
}

pub async fn scope<F: std::future::Future>(acc: Account, f: F) -> F::Output {
    ACTIVE.scope(acc, f).await
}

/// Ejecuta `f` en un hilo aparte (spawn_blocking) conservando la cuenta activa.
pub async fn blocking<R: Send + 'static>(f: impl FnOnce() -> R + Send + 'static) -> Option<R> {
    let acc = active();
    tauri::async_runtime::spawn_blocking(move || match acc {
        Some(a) => scope_sync(a, f),
        None => f(),
    })
    .await
    .ok()
}

pub fn scope_sync<R>(acc: Account, f: impl FnOnce() -> R) -> R {
    let prev = ACTIVE_THREAD.with(|a| a.replace(Some(acc)));
    let out = f();
    ACTIVE_THREAD.with(|a| *a.borrow_mut() = prev);
    out
}

// ---------- Elegir cuenta por cuota ----------

/// Uso de una cuenta para decidir: el límite más alto (0..1) y si tiene sesión.
#[derive(Clone, Debug)]
pub struct Load {
    pub account: String,
    pub worst: f64,
    pub usable: bool,
}

/// La cuenta de `cli` con más margen, sin las de `exclude` y por debajo del umbral.
pub fn pick(cli: &str, loads: &[Load], exclude: &[String]) -> Option<Account> {
    pick_in(&root(), cli, loads, exclude)
}

fn pick_in(root: &Path, cli: &str, loads: &[Load], exclude: &[String]) -> Option<Account> {
    let mut candidates: Vec<(f64, Account)> = list_in(root, cli)
        .into_iter()
        .filter(|a| !exclude.contains(&a.id))
        .filter_map(|a| {
            // Sin datos de uso de la cuenta: se intenta (mejor que no lanzar), detrás de las conocidas.
            let worst = loads.iter().find(|l| l.account == a.id).map(|l| if l.usable { l.worst } else { f64::INFINITY }).unwrap_or(0.5);
            (worst < THRESHOLD).then_some((worst, a))
        })
        .collect();
    candidates.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    candidates.into_iter().next().map(|(_, a)| a)
}

/// Orden en que se prueban otras CLIs cuando la del carril no tiene ninguna cuenta con cuota.
pub const FALLBACK: [&str; 6] = ["claude", "codex", "opencode", "gemini", "copilot", "agy"];

/// Argumentos por defecto para lanzar una CLI sin interacción con un brief. `None` en el brief:
/// va por la entrada estándar; `Some(i)`: se pone el brief en esa posición.
pub fn preset(cli: &str) -> Option<(Vec<&'static str>, Option<usize>)> {
    Some(match cli {
        "codex" => (vec!["exec", "--approve-for-me", "-"], None),
        "claude" => (vec!["-p", "--dangerously-skip-permissions"], None),
        "opencode" => (vec!["run", "--auto", "{brief}"], Some(2)),
        "agy" => (vec!["-p", "{brief}", "--dangerously-skip-permissions"], Some(1)),
        "gemini" => (vec!["--yolo", "-p", "{brief}"], Some(2)),
        "copilot" => (vec!["-p", "{brief}", "--allow-all-tools"], Some(1)),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn with_root<R>(f: impl FnOnce(&Path) -> R) -> R {
        let dir = std::env::temp_dir().join(format!("cuentas-{}", uuid::Uuid::new_v4().simple()));
        std::fs::create_dir_all(&dir).unwrap();
        let out = f(&dir);
        let _ = std::fs::remove_dir_all(dir);
        out
    }

    #[test]
    fn alta_baja_y_entorno() {
        with_root(|r| {
            assert_eq!(list_in(r, "codex").len(), 1);
            let a = add_in(r, "codex", "cuenta-2").unwrap();
            assert_eq!(a.id, "codex:cuenta-2");
            assert!(add_in(r, "codex", "cuenta-2").is_err());
            assert!(add_in(r, "codex", "../x").is_err());
            assert!(add_in(r, "codex", "principal").is_err());
            assert!(add_in(r, "agy", "dos").is_err());
            let env = a.env();
            assert_eq!(env[0].0, "CODEX_HOME");
            assert!(env[0].1.ends_with("codex/cuenta-2"));
            assert_eq!(list_in(r, "codex").iter().map(|a| a.id.as_str()).collect::<Vec<_>>(), ["codex", "codex:cuenta-2"]);
            assert!(remove_in(r, "codex").is_err());
            remove_in(r, "codex:cuenta-2").unwrap();
            assert_eq!(list_in(r, "codex").len(), 1);
            let o = add_in(r, "opencode", "b").unwrap();
            assert!(o.env()[0].1.ends_with("opencode/b/data"));
            assert!(Account::principal("claude").env().is_empty());
        });
    }

    #[test]
    fn eleccion_por_cuota() {
        with_root(|r| {
            add_in(r, "codex", "dos").unwrap();
            add_in(r, "codex", "tres").unwrap();
            let load = |id: &str, worst: f64| Load { account: id.into(), worst, usable: true };
            let loads = vec![load("codex", 0.96), load("codex:dos", 0.40), load("codex:tres", 0.10)];
            assert_eq!(pick_in(r, "codex", &loads, &[]).unwrap().id, "codex:tres");
            assert_eq!(pick_in(r, "codex", &loads, &["codex:tres".into()]).unwrap().id, "codex:dos");
            // Todas por encima del umbral o excluidas
            assert!(pick_in(r, "codex", &loads, &["codex:tres".into(), "codex:dos".into()]).is_none());
            // Sin sesión no cuenta
            let loads = vec![Load { account: "codex".into(), worst: 0.0, usable: false }, load("codex:dos", 0.95), load("codex:tres", 0.92)];
            assert!(pick_in(r, "codex", &loads, &[]).is_none());
        });
    }

    #[tokio::test]
    async fn ambito_de_cuenta() {
        assert!(active().is_none());
        let acc = Account { id: "codex:x".into(), cli: "codex".into(), name: Some("x".into()), dir: Some("/tmp/x".into()) };
        let got = scope(acc.clone(), async { active() }).await;
        assert_eq!(got, Some(acc.clone()));
        assert_eq!(scope_sync(acc.clone(), active_env)[0].1, "/tmp/x");
        assert!(active().is_none());
    }
}
