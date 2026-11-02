//! Vista Agentes: cada agente que lanza el orquestador (con `agentes run`) o el usuario (con «+ Lanzar»)
//! corre en un PTY de la app, en su worktree, y se ve como un panel del mosaico.
//!
//! `agentes run` habla con la app por TCP en 127.0.0.1 (puerto aleatorio) y solo se atiende con el
//! token de esta sesión, que la app pasa en el entorno a los procesos que lanza ella.

use crate::env::{self, which};
use crate::store::data_dir;
use crate::term::{Job, Terminals};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter, Manager};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::Notify;

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Pane {
    pub id: u32,
    #[serde(skip)]
    key: String,
    /// Carpeta raíz del repo (la del proyecto), para agrupar los paneles por proyecto.
    pub project: String,
    pub name: String,
    pub cli: String,
    pub command: String,
    pub dir: String,
    pub worktree: Option<String>,
    pub branch: Option<String>,
    /// running | done | error
    pub status: String,
    pub exit_code: Option<i32>,
    pub started_at: String,
    pub ended_at: Option<String>,
    pub log: String,
}

#[derive(Default)]
pub struct Panes {
    list: Mutex<Vec<Pane>>,
    changed: Notify,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct LaunchRequest {
    /// Carpeta desde la que se pide (la del orquestador o la del proyecto).
    pub cwd: String,
    pub cli: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub worktree: Option<String>,
    #[serde(default)]
    pub branch: Option<String>,
    #[serde(default)]
    pub args: Vec<String>,
    /// Texto para la entrada estándar de la CLI (el brief).
    #[serde(default)]
    pub stdin: Option<String>,
}

fn now() -> String {
    chrono::Utc::now().to_rfc3339()
}

fn git(dir: &Path, args: &[&str]) -> Result<String, String> {
    let out = Command::new("git").args(args).current_dir(dir).envs(env::child_env()).output().map_err(|e| format!("git: {e}"))?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
}

/// Raíz del repo principal aunque `dir` sea un worktree. Si no es un repo git, la propia carpeta.
pub fn repo_root(dir: &Path) -> PathBuf {
    match git(dir, &["rev-parse", "--path-format=absolute", "--git-common-dir"]) {
        Ok(common) => {
            let common = PathBuf::from(common);
            if common.file_name().is_some_and(|n| n == ".git") {
                common.parent().map(Path::to_path_buf).unwrap_or_else(|| dir.to_path_buf())
            } else {
                dir.to_path_buf()
            }
        }
        Err(_) => dir.to_path_buf(),
    }
}

/// Nombre seguro para una carpeta de worktree: letras, números, punto, guion y guion bajo.
fn valid_name(name: &str) -> bool {
    !name.is_empty() && name.len() <= 80 && !name.starts_with('.') && name.chars().all(|c| c.is_ascii_alphanumeric() || "._-".contains(c))
}

/// Añade líneas al `info/exclude` del repo común (el de los worktrees enlazados no se lee).
fn ensure_excluded(root: &Path, lines: &[&str]) {
    let file = root.join(".git").join("info").join("exclude");
    let current = std::fs::read_to_string(&file).unwrap_or_default();
    let missing: Vec<&str> = lines.iter().copied().filter(|l| !current.lines().any(|c| c.trim() == *l)).collect();
    if missing.is_empty() {
        return;
    }
    let _ = std::fs::create_dir_all(file.parent().unwrap());
    let mut text = current;
    if !text.is_empty() && !text.ends_with('\n') {
        text.push('\n');
    }
    text.push_str("# Chat de agentes (agentes run)\n");
    for l in missing {
        text.push_str(l);
        text.push('\n');
    }
    let _ = std::fs::write(&file, text);
}

/// `.claude/worktrees/<nombre>` del repo; lo crea si no existe (rama nueva desde HEAD o la rama indicada).
pub fn ensure_worktree(root: &Path, name: &str, branch: Option<&str>) -> Result<(PathBuf, String), String> {
    if !valid_name(name) {
        return Err(format!("Nombre de worktree no válido: «{name}» (solo letras, números, «.», «-» y «_»)"));
    }
    if !root.join(".git").exists() {
        return Err("La carpeta del proyecto no es un repositorio git: no se pueden crear worktrees".into());
    }
    let path = root.join(".claude").join("worktrees").join(name);
    ensure_excluded(root, &[".claude/worktrees/", "agente.log", "node_modules", ".venv"]);
    if path.exists() {
        let b = git(&path, &["branch", "--show-current"]).unwrap_or_default();
        return Ok((path, b));
    }
    let branch = branch.map(str::to_string).unwrap_or_else(|| format!("agentes/{name}"));
    git(root, &["check-ref-format", "--branch", &branch]).map_err(|_| format!("Nombre de rama no válido: «{branch}»"))?;
    let path_s = path.to_string_lossy().to_string();
    let exists = git(root, &["rev-parse", "--verify", "--quiet", &format!("refs/heads/{branch}")]).is_ok();
    if exists {
        git(root, &["worktree", "add", &path_s, &branch])?;
    } else {
        git(root, &["worktree", "add", &path_s, "-b", &branch, "HEAD"])?;
    }
    // Dependencias del repo principal, para que los tests corran en el worktree.
    #[cfg(unix)]
    for dep in ["node_modules", ".venv"] {
        let src = root.join(dep);
        if src.exists() && !path.join(dep).exists() {
            let _ = std::os::unix::fs::symlink(&src, path.join(dep));
        }
    }
    Ok((path, branch))
}

#[cfg(windows)]
fn quote_cmd(s: &str) -> String {
    if s.is_empty() || s.contains([' ', '\t', '"', '&', '|', '<', '>', '^']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

impl Panes {
    pub fn list(&self, project: &str) -> Vec<Pane> {
        self.list.lock().unwrap().iter().filter(|p| p.project == project).cloned().collect()
    }

    pub fn get(&self, id: u32) -> Option<Pane> {
        self.list.lock().unwrap().iter().find(|p| p.id == id).cloned()
    }

    fn finish(&self, key: &str, code: i32) {
        if let Some(p) = self.list.lock().unwrap().iter_mut().find(|p| p.key == key) {
            p.status = if code == 0 { "done".into() } else { "error".into() };
            p.exit_code = Some(code);
            p.ended_at = Some(now());
        }
        self.changed.notify_waiters();
    }

    pub fn remove(&self, id: u32) {
        self.list.lock().unwrap().retain(|p| p.id != id);
    }

    /// Espera a que el panel termine y devuelve cómo acabó.
    pub async fn wait(&self, id: u32) -> Option<Pane> {
        loop {
            let notified = self.changed.notified();
            let p = self.get(id)?;
            if p.status != "running" {
                return Some(p);
            }
            notified.await;
        }
    }
}

/// Lanza una CLI en un panel nuevo.
pub fn launch(app: &AppHandle, req: LaunchRequest) -> Result<Pane, String> {
    let panes = app.state::<Arc<Panes>>().inner().clone();
    let cwd = PathBuf::from(&req.cwd);
    if !cwd.is_dir() {
        return Err(format!("La carpeta no existe: {}", req.cwd));
    }
    let root = repo_root(&cwd);
    let cli = req.cli.trim().to_string();
    if cli.is_empty() || cli.contains(['/', '\\']) && !Path::new(&cli).is_file() {
        return Err("Indica la CLI con --cli (p. ej. codex, opencode, claude, agy, gemini)".into());
    }
    let cli_path = if Path::new(&cli).is_file() { PathBuf::from(&cli) } else { which(&cli).ok_or_else(|| format!("No encuentro «{cli}» en el PATH"))? };
    let cli_name = cli_path.file_stem().map(|n| n.to_string_lossy().to_string()).unwrap_or(cli.clone());

    let (dir, branch, worktree) = match req.worktree.as_deref().filter(|w| !w.is_empty()) {
        Some(w) => {
            let (dir, branch) = ensure_worktree(&root, w, req.branch.as_deref().filter(|b| !b.is_empty()))?;
            (dir, Some(branch).filter(|b| !b.is_empty()), Some(w.to_string()))
        }
        None => (cwd.clone(), git(&cwd, &["branch", "--show-current"]).ok().filter(|b| !b.is_empty()), None),
    };

    let key = uuid::Uuid::new_v4().to_string();
    let log = if worktree.is_some() {
        dir.join("agente.log")
    } else {
        let d = data_dir().join("logs");
        let _ = std::fs::create_dir_all(&d);
        d.join(format!("{key}.log"))
    };
    let shown = std::iter::once(cli_name.clone()).chain(req.args.iter().cloned()).collect::<Vec<_>>().join(" ");
    let mut job_env = Vec::new();
    let (file, args) = match req.stdin.as_deref().filter(|s| !s.is_empty()) {
        Some(text) => {
            let briefs = data_dir().join("briefs");
            let _ = std::fs::create_dir_all(&briefs);
            let brief = briefs.join(format!("{key}.md"));
            std::fs::write(&brief, text).map_err(|e| e.to_string())?;
            job_env.push(("AGENTES_BRIEF".to_string(), brief.to_string_lossy().to_string()));
            #[cfg(unix)]
            {
                let mut a = vec!["-c".to_string(), "exec \"$0\" \"$@\" < \"$AGENTES_BRIEF\"".to_string(), cli_path.to_string_lossy().to_string()];
                a.extend(req.args.iter().cloned());
                (PathBuf::from("/bin/sh"), a)
            }
            #[cfg(windows)]
            {
                let line = std::iter::once(cli_path.to_string_lossy().to_string()).chain(req.args.iter().cloned()).map(|s| quote_cmd(&s)).collect::<Vec<_>>().join(" ");
                let comspec = std::env::var("COMSPEC").unwrap_or_else(|_| "cmd.exe".into());
                (PathBuf::from(comspec), vec!["/D".into(), "/S".into(), "/C".into(), format!("\"{line} < {}\"", quote_cmd(&brief.to_string_lossy()))])
            }
        }
        None => (cli_path.clone(), req.args.clone()),
    };

    let name = req.name.clone().filter(|n| !n.trim().is_empty()).unwrap_or_else(|| match &worktree {
        Some(w) => format!("{cli_name} · {w}"),
        None => cli_name.clone(),
    });
    let mut job = Job::new(format!("{shown}{}", if req.stdin.is_some() { " < brief" } else { "" }), file, args, dir.clone());
    job.env = job_env;
    job.log = Some(log.clone());
    let (p2, k2, app2) = (panes.clone(), key.clone(), app.clone());
    job.on_exit = Some(Box::new(move |code| {
        p2.finish(&k2, code);
        let _ = app2.emit("panes-changed", ());
    }));
    let terms = app.state::<Terminals>();
    // Registrar antes de arrancar: si el proceso termina enseguida, `finish` lo encuentra.
    let mut pane = Pane {
        id: 0,
        key: key.clone(),
        project: root.to_string_lossy().to_string(),
        name,
        cli: cli_name,
        command: shown,
        dir: dir.to_string_lossy().to_string(),
        worktree,
        branch,
        status: "running".into(),
        exit_code: None,
        started_at: now(),
        ended_at: None,
        log: log.to_string_lossy().to_string(),
    };
    panes.list.lock().unwrap().push(pane.clone());
    match terms.start(app.clone(), job, 120, 32) {
        Ok(id) => {
            if let Some(p) = panes.list.lock().unwrap().iter_mut().find(|p| p.key == key) {
                p.id = id;
                pane = p.clone();
            }
            let _ = app.emit("panes-changed", ());
            Ok(pane)
        }
        Err(e) => {
            panes.list.lock().unwrap().retain(|p| p.key != key);
            Err(e)
        }
    }
}

// ---------- Servidor local para `agentes run` ----------

#[derive(Deserialize)]
#[serde(tag = "cmd", rename_all = "camelCase")]
enum Request {
    Run {
        #[serde(flatten)]
        req: LaunchRequest,
        #[serde(default)]
        wait: bool,
    },
    Status {
        cwd: String,
    },
    Wait {
        id: u32,
    },
    Kill {
        id: u32,
    },
}

#[derive(Deserialize)]
struct Envelope {
    token: String,
    #[serde(flatten)]
    request: Request,
}

#[derive(Serialize, Default)]
#[serde(rename_all = "camelCase")]
struct Reply {
    ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pane: Option<Pane>,
    #[serde(skip_serializing_if = "Option::is_none")]
    panes: Option<Vec<Pane>>,
}

fn err(e: impl Into<String>) -> Reply {
    Reply { ok: false, error: Some(e.into()), ..Default::default() }
}

/// Compara sin cortar al primer carácter distinto.
fn same_token(a: &str, b: &str) -> bool {
    a.len() == b.len() && a.bytes().zip(b.bytes()).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

async fn handle(app: &AppHandle, token: &str, line: &str) -> Reply {
    let env: Envelope = match serde_json::from_str(line) {
        Ok(e) => e,
        Err(e) => return err(format!("Petición no válida: {e}")),
    };
    if !same_token(&env.token, token) {
        return err("Token no válido");
    }
    let panes = app.state::<Arc<Panes>>().inner().clone();
    match env.request {
        Request::Run { req, wait } => {
            let app2 = app.clone();
            let launched = tauri::async_runtime::spawn_blocking(move || launch(&app2, req)).await.map_err(|e| e.to_string()).and_then(|r| r);
            match launched {
                Ok(p) if wait => Reply { ok: true, pane: panes.wait(p.id).await, ..Default::default() },
                Ok(p) => Reply { ok: true, pane: Some(p), ..Default::default() },
                Err(e) => err(e),
            }
        }
        Request::Status { cwd } => {
            let root = repo_root(Path::new(&cwd));
            Reply { ok: true, panes: Some(panes.list(&root.to_string_lossy())), ..Default::default() }
        }
        Request::Wait { id } => match panes.wait(id).await {
            Some(p) => Reply { ok: true, pane: Some(p), ..Default::default() },
            None => err(format!("No hay ningún panel {id}")),
        },
        Request::Kill { id } => {
            app.state::<Terminals>().kill(id);
            Reply { ok: true, pane: panes.get(id), ..Default::default() }
        }
    }
}

/// Arranca el servidor y deja el `agentes` interno en el PATH de los procesos hijos.
pub fn start_server(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let listener = match tokio::net::TcpListener::bind("127.0.0.1:0").await {
            Ok(l) => l,
            Err(e) => {
                eprintln!("agentes run no disponible: {e}");
                return;
            }
        };
        let addr = listener.local_addr().map(|a| a.to_string()).unwrap_or_default();
        let token = format!("{}{}", uuid::Uuid::new_v4().simple(), uuid::Uuid::new_v4().simple());
        match write_shim() {
            Ok(shim_dir) => env::set_launcher(env::Launcher { addr, token: token.clone(), shim_dir }),
            Err(e) => {
                eprintln!("agentes run no disponible: {e}");
                return;
            }
        }
        let token = Arc::new(token);
        loop {
            let Ok((stream, peer)) = listener.accept().await else { continue };
            if !peer.ip().is_loopback() {
                continue;
            }
            let (app, token) = (app.clone(), token.clone());
            tauri::async_runtime::spawn(async move {
                let (read, mut write) = stream.into_split();
                let mut lines = BufReader::new(read).lines();
                if let Ok(Some(line)) = lines.next_line().await {
                    let reply = handle(&app, &token, &line).await;
                    let mut out = serde_json::to_string(&reply).unwrap_or_default();
                    out.push('\n');
                    let _ = write.write_all(out.as_bytes()).await;
                }
            });
        }
    });
}

/// `agentes` interno (en `data_dir()/bin`): reenvía al ejecutable de la app en modo cliente.
fn write_shim() -> Result<PathBuf, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let dir = data_dir().join("bin");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    if env::IS_WIN {
        std::fs::write(dir.join("agentes.cmd"), format!("@echo off\r\n\"{}\" --agentes-cli %*\r\n", exe.display())).map_err(|e| e.to_string())?;
    } else {
        let file = dir.join("agentes");
        std::fs::write(&file, format!("#!/bin/sh\nexec \"{}\" --agentes-cli \"$@\"\n", exe.display())).map_err(|e| e.to_string())?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o755)).map_err(|e| e.to_string())?;
        }
    }
    Ok(dir)
}

// ---------- Cliente: `agentes run | status | wait | kill` ----------

const HELP: &str = "Lanza agentes en paneles de la vista «Agentes» de Chat de agentes.

Uso:
  agentes run --cli <cli> [--worktree <nombre>] [--branch <rama>] [--name <título>]
              [--brief <archivo>] [--wait] [-- <argumentos de la CLI>]
  agentes status            paneles de este proyecto (id, estado, worktree, log)
  agentes wait <id>         espera a que termine el panel y sale con su código
  agentes kill <id>         detiene el panel

--worktree crea (o reutiliza) .claude/worktrees/<nombre> en una rama nueva desde HEAD
(agentes/<nombre> si no se indica --branch) y ejecuta la CLI ahí.
--brief <archivo> (o «-» entre los argumentos con el brief en la entrada estándar) se pasa
a la CLI por su entrada estándar. La salida queda en <worktree>/agente.log.

Ejemplos:
  agentes run --cli codex --worktree codex-api --brief brief.md -- exec --approve-for-me -
  agentes run --cli opencode --worktree oc-ui -- run --auto \"$(cat brief.md)\"
  agentes run --cli claude --worktree claude-docs --wait -- -p \"Revisa docs/ y …\"

Solo funciona desde procesos lanzados por la app (agentes del chat, Terminal, paneles).";

fn print_pane(p: &Pane) {
    let state = match p.status.as_str() {
        "running" => "en marcha".to_string(),
        "done" => "terminado (0)".to_string(),
        _ => format!("error ({})", p.exit_code.unwrap_or(-1)),
    };
    println!("{}\t{}\t{}\t{}\t{}", p.id, state, p.name, p.worktree.as_deref().unwrap_or("-"), p.log);
}

fn request(json: serde_json::Value) -> Result<Reply2, String> {
    use std::io::{BufRead, Write};
    let addr = std::env::var("AGENTES_ADDR").map_err(|_| "agentes solo funciona dentro de Chat de agentes (falta AGENTES_ADDR)".to_string())?;
    let token = std::env::var("AGENTES_TOKEN").map_err(|_| "agentes solo funciona dentro de Chat de agentes (falta AGENTES_TOKEN)".to_string())?;
    let mut json = json;
    json["token"] = serde_json::Value::String(token);
    let mut stream = std::net::TcpStream::connect(&addr).map_err(|e| format!("No se pudo hablar con la app ({addr}): {e}. ¿Sigue abierta?"))?;
    let mut line = serde_json::to_string(&json).unwrap();
    line.push('\n');
    stream.write_all(line.as_bytes()).map_err(|e| e.to_string())?;
    let mut reply = String::new();
    std::io::BufReader::new(stream).read_line(&mut reply).map_err(|e| e.to_string())?;
    let r: Reply2 = serde_json::from_str(&reply).map_err(|e| format!("Respuesta no válida: {e}"))?;
    if !r.ok {
        return Err(r.error.unwrap_or_else(|| "Error".into()));
    }
    Ok(r)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Reply2 {
    ok: bool,
    error: Option<String>,
    pane: Option<PaneOut>,
    panes: Option<Vec<PaneOut>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PaneOut {
    id: u32,
    name: String,
    status: String,
    exit_code: Option<i32>,
    worktree: Option<String>,
    log: String,
}

impl From<PaneOut> for Pane {
    fn from(p: PaneOut) -> Self {
        Pane {
            id: p.id,
            key: String::new(),
            project: String::new(),
            name: p.name,
            cli: String::new(),
            command: String::new(),
            dir: String::new(),
            worktree: p.worktree,
            branch: None,
            status: p.status,
            exit_code: p.exit_code,
            started_at: String::new(),
            ended_at: None,
            log: p.log,
        }
    }
}

fn exit_code(p: &Pane) -> i32 {
    match p.status.as_str() {
        "running" | "done" => 0,
        _ => p.exit_code.unwrap_or(1).max(1),
    }
}

/// Punto de entrada del modo cliente (`<app> --agentes-cli …`). Devuelve el código de salida.
pub fn cli_main(args: Vec<String>) -> i32 {
    match cli(args) {
        Ok(code) => code,
        Err(e) => {
            eprintln!("agentes: {e}");
            2
        }
    }
}

fn cli(args: Vec<String>) -> Result<i32, String> {
    let cwd = std::env::current_dir().map_err(|e| e.to_string())?.to_string_lossy().to_string();
    let mut it = args.into_iter();
    let sub = it.next().unwrap_or_default();
    match sub.as_str() {
        "run" => {
            let mut v = serde_json::json!({ "cmd": "run", "cwd": cwd, "args": [] });
            let mut cli_args: Vec<String> = Vec::new();
            let mut wait = false;
            let mut brief: Option<String> = None;
            fn val(it: &mut impl Iterator<Item = String>, name: &str) -> Result<String, String> {
                it.next().ok_or_else(|| format!("Falta el valor de {name}"))
            }
            while let Some(a) = it.next() {
                match a.as_str() {
                    "--" => {
                        cli_args.extend(it.by_ref());
                        break;
                    }
                    "--cli" => v["cli"] = val(&mut it, "--cli")?.into(),
                    "--worktree" => v["worktree"] = val(&mut it, "--worktree")?.into(),
                    "--branch" => v["branch"] = val(&mut it, "--branch")?.into(),
                    "--name" => v["name"] = val(&mut it, "--name")?.into(),
                    "--brief" => {
                        let f = val(&mut it, "--brief")?;
                        brief = Some(std::fs::read_to_string(&f).map_err(|e| format!("No se pudo leer {f}: {e}"))?);
                    }
                    "--wait" => wait = true,
                    "-h" | "--help" => {
                        println!("{HELP}");
                        return Ok(0);
                    }
                    other => return Err(format!("Opción desconocida: {other} (los argumentos de la CLI van después de --)")),
                }
            }
            if v.get("cli").is_none() {
                return Err("Falta --cli".into());
            }
            // «-» entre los argumentos: el brief llega por la entrada estándar.
            if brief.is_none() && cli_args.iter().any(|a| a == "-") {
                use std::io::{IsTerminal, Read};
                let mut s = String::new();
                if !std::io::stdin().is_terminal() {
                    std::io::stdin().read_to_string(&mut s).map_err(|e| e.to_string())?;
                }
                brief = Some(s).filter(|s| !s.is_empty());
            }
            v["args"] = cli_args.into();
            v["wait"] = wait.into();
            if let Some(b) = brief {
                v["stdin"] = b.into();
            }
            let r = request(v)?;
            let p: Pane = r.pane.ok_or("Sin respuesta")?.into();
            print_pane(&p);
            Ok(if wait { exit_code(&p) } else { 0 })
        }
        "status" => {
            let r = request(serde_json::json!({ "cmd": "status", "cwd": cwd }))?;
            let list = r.panes.unwrap_or_default();
            if list.is_empty() {
                println!("No hay paneles en este proyecto.");
            }
            println!("id\testado\tnombre\tworktree\tlog");
            for p in list {
                print_pane(&p.into());
            }
            Ok(0)
        }
        "wait" | "kill" => {
            let id: u32 = it.next().and_then(|s| s.parse().ok()).ok_or(format!("Uso: agentes {sub} <id>"))?;
            let r = request(serde_json::json!({ "cmd": sub, "id": id }))?;
            let p: Pane = r.pane.ok_or("Sin respuesta")?.into();
            print_pane(&p);
            Ok(if sub == "wait" { exit_code(&p) } else { 0 })
        }
        _ => {
            println!("{HELP}");
            Ok(if sub.is_empty() || sub == "help" || sub == "--help" || sub == "-h" { 0 } else { 2 })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nombres_de_worktree() {
        assert!(valid_name("codex-uso"));
        assert!(valid_name("oc_ui.2"));
        assert!(!valid_name("../fuera"));
        assert!(!valid_name("a/b"));
        assert!(!valid_name(".git"));
        assert!(!valid_name(""));
    }

    #[test]
    fn token_constante() {
        assert!(same_token("abc", "abc"));
        assert!(!same_token("abc", "abd"));
        assert!(!same_token("abc", "abcd"));
    }

    #[test]
    fn crea_worktree_y_excluye() {
        let root = std::env::temp_dir().join(format!("agentes-wt-{}", uuid::Uuid::new_v4().simple()));
        std::fs::create_dir_all(&root).unwrap();
        for args in [vec!["init", "-q", "-b", "main"], vec!["-c", "user.email=a@b", "-c", "user.name=t", "commit", "-q", "--allow-empty", "-m", "x"]] {
            assert!(Command::new("git").args(&args).current_dir(&root).status().unwrap().success());
        }
        let (dir, branch) = ensure_worktree(&root, "codex-x", None).unwrap();
        assert!(dir.ends_with(".claude/worktrees/codex-x"));
        assert_eq!(branch, "agentes/codex-x");
        assert_eq!(repo_root(&dir), root.canonicalize().unwrap_or(root.clone()));
        let exclude = std::fs::read_to_string(root.join(".git/info/exclude")).unwrap();
        assert!(exclude.contains(".claude/worktrees/") && exclude.contains("agente.log"));
        // Reutiliza el existente
        assert_eq!(ensure_worktree(&root, "codex-x", None).unwrap().0, dir);
        assert!(ensure_worktree(&root, "../malo", None).is_err());
        let _ = std::fs::remove_dir_all(&root);
    }
}
