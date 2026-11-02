//! Chat de escritorio para agentes CLI. Capas:
//!   ui/index.html → chat (la ventana)
//!   store.rs      → caché propia de conversaciones, compartida por todos los agentes
//!   agents.rs     → adaptadores: cómo cada CLI recibe esa conversación y responde
//!   project.rs    → carpeta del proyecto donde trabajan los agentes (como `claude` o `code .`)
//!   term.rs       → terminal para instalar agentes e iniciar sesión
//!   workspace.rs  → panel derecho: árbol de archivos y archivo que se está editando

mod agents;
mod env;
mod project;
mod store;
mod term;
mod workspace;

use serde::Serialize;
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Mutex;
use tauri::{AppHandle, Manager, State, WebviewWindow};

#[derive(Default)]
struct Busy(Mutex<HashSet<String>>);

/// Carpeta del proyecto de esta ventana.
struct CurrentProject(Mutex<PathBuf>);

impl CurrentProject {
    fn get(&self) -> PathBuf {
        self.0.lock().unwrap().clone()
    }
}

fn set_title(window: &WebviewWindow, project: &project::Project) {
    let _ = window.set_title(&format!("{} — Chat de agentes", project.name));
}

#[tauri::command]
fn get_project(current: State<'_, CurrentProject>) -> project::Project {
    project::Project::from_path(&current.get())
}

/// Cambia de proyecto (desde «Abrir carpeta…»).
#[tauri::command]
fn open_project(
    app: AppHandle,
    window: WebviewWindow,
    current: State<'_, CurrentProject>,
    watcher: State<'_, workspace::Watcher>,
    path: String,
) -> Result<project::Project, String> {
    let dir = PathBuf::from(&path);
    if !dir.is_dir() {
        return Err(format!("La carpeta no existe: {path}"));
    }
    let dir = dir.canonicalize().unwrap_or(dir);
    *current.0.lock().unwrap() = dir.clone();
    project::remember(&dir);
    let _ = watcher.watch(app, dir.clone());
    let p = project::Project::from_path(&dir);
    set_title(&window, &p);
    Ok(p)
}

#[tauri::command]
async fn get_tree(current: State<'_, CurrentProject>) -> Result<workspace::Tree, String> {
    let root = current.get();
    tauri::async_runtime::spawn_blocking(move || workspace::tree(&root)).await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn read_file(current: State<'_, CurrentProject>, path: String) -> Result<workspace::FileView, String> {
    let root = current.get();
    tauri::async_runtime::spawn_blocking(move || workspace::read(&root, &path, workspace::is_git(&root)))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn read_head(current: State<'_, CurrentProject>, path: String) -> Result<String, String> {
    let root = current.get();
    tauri::async_runtime::spawn_blocking(move || workspace::head(&root, &path)).await.map_err(|e| e.to_string())?
}

#[tauri::command]
fn write_file(current: State<'_, CurrentProject>, path: String, content: String) -> Result<(), String> {
    workspace::write(&current.get(), &path, &content)
}

#[tauri::command]
fn install_cli() -> Result<String, String> {
    project::install_cli()
}

#[tauri::command]
async fn list_agents() -> Vec<agents::AgentInfo> {
    let checks = agents::AGENTS.iter().map(|a| async move {
        agents::AgentInfo { id: a.id, name: a.name, status: agents::status(a).await }
    });
    futures::future::join_all(checks).await
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ChatReply {
    conversation_id: String,
    title: String,
    message: store::Message,
}

#[tauri::command]
async fn chat(
    busy: State<'_, Busy>,
    current: State<'_, CurrentProject>,
    conversation_id: Option<String>,
    agent: String,
    message: String,
) -> Result<ChatReply, String> {
    let ag = agents::get(&agent).ok_or_else(|| format!("Agente desconocido: {agent}"))?;
    if message.trim().is_empty() {
        return Err("Mensaje vacío".into());
    }
    let cwd = current.get();
    if !cwd.is_dir() {
        return Err(format!("La carpeta del proyecto ya no existe: {}", cwd.display()));
    }
    let previous = conversation_id.as_deref().and_then(store::get).map(|c| c.messages).unwrap_or_default();
    let key = conversation_id.clone().unwrap_or_default();
    if !key.is_empty() && !busy.0.lock().unwrap().insert(key.clone()) {
        return Err("Esta conversación ya está esperando una respuesta".into());
    }
    let result = agents::run(ag, &agents::build_prompt(ag, &previous, &message), &cwd).await;
    busy.0.lock().unwrap().remove(&key);
    let output = result?;
    // Solo se guarda el intercambio si el agente respondió bien.
    let conv = store::append(
        conversation_id.as_deref(),
        &cwd.to_string_lossy(),
        vec![
            store::Message { role: "user".into(), text: message, agent: None, agent_name: None, ts: String::new() },
            store::Message {
                role: "assistant".into(),
                text: output,
                agent: Some(ag.id.into()),
                agent_name: Some(ag.name.into()),
                ts: String::new(),
            },
        ],
    )?;
    Ok(ChatReply { conversation_id: conv.id.clone(), title: conv.title.clone(), message: conv.messages.last().cloned().unwrap() })
}

#[tauri::command]
fn list_conversations(current: State<'_, CurrentProject>) -> Vec<store::Summary> {
    store::list(&current.get().to_string_lossy())
}

#[tauri::command]
fn get_conversation(id: String) -> Result<store::Conversation, String> {
    store::get(&id).ok_or_else(|| "Conversación no encontrada".into())
}

/// Abre un terminal para instalar un agente o iniciar sesión (action = install | login).
#[tauri::command]
fn term_start(
    app: AppHandle,
    terms: State<'_, term::Terminals>,
    agent: String,
    action: String,
    cols: u16,
    rows: u16,
) -> Result<u32, String> {
    let ag = agents::get(&agent).ok_or("Agente desconocido")?;
    if action != "install" && action != "login" {
        return Err("Acción no válida".into());
    }
    let (command, shell, args) = agents::terminal_command(ag, &action);
    let file = env::which(&shell).unwrap_or_else(|| PathBuf::from(&shell));
    terms.start(app, term::Job { command, file, args, cwd: env::home() }, cols, rows)
}

#[tauri::command]
fn term_write(terms: State<'_, term::Terminals>, id: u32, data: String) {
    terms.write(id, &data)
}

#[tauri::command]
fn term_resize(terms: State<'_, term::Terminals>, id: u32, cols: u16, rows: u16) {
    terms.resize(id, cols, rows)
}

#[tauri::command]
fn term_kill(terms: State<'_, term::Terminals>, id: u32) {
    terms.kill(id)
}

pub fn run() {
    // Calcula el PATH al arrancar, para que la primera comprobación de agentes sea rápida.
    std::thread::spawn(|| {
        env::full_path();
    });
    let initial = project::initial();
    project::remember(&initial);
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(Busy::default())
        .manage(CurrentProject(Mutex::new(initial.clone())))
        .manage(term::Terminals::default())
        .manage(workspace::Watcher::default())
        .setup(move |app| {
            if let Some(window) = app.get_webview_window("main") {
                set_title(&window, &project::Project::from_path(&initial));
            }
            let _ = app.state::<workspace::Watcher>().watch(app.handle().clone(), initial.clone());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_project,
            open_project,
            install_cli,
            get_tree,
            read_file,
            read_head,
            write_file,
            list_agents,
            chat,
            list_conversations,
            get_conversation,
            term_start,
            term_write,
            term_resize,
            term_kill
        ])
        .run(tauri::generate_context!())
        .expect("error al arrancar la app");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn msg(role: &str, text: &str, agent: Option<&str>) -> store::Message {
        store::Message {
            role: role.into(),
            text: text.into(),
            agent: agent.map(str::to_lowercase),
            agent_name: agent.map(str::to_string),
            ts: String::new(),
        }
    }

    #[test]
    fn prompt_marca_quien_dijo_que() {
        let claude = agents::get("claude").unwrap();
        assert_eq!(agents::build_prompt(claude, &[], "hola"), "hola");
        let p = agents::build_prompt(claude, &[msg("user", "me llamo Ana", None), msg("assistant", "hola Ana", Some("Codex"))], "¿quién soy?");
        assert!(p.contains("[Usuario]: me llamo Ana"));
        assert!(p.contains("[Codex]: hola Ana"));
        assert!(p.contains("Tú eres Claude"));
        assert!(p.ends_with("¿quién soy?"));
    }

    #[test]
    fn prompt_respeta_el_limite() {
        let agy = agents::get("agy").unwrap();
        let big = "x".repeat(15_000);
        let p = agents::build_prompt(agy, &[msg("user", &big, None), msg("assistant", &big, Some("Codex"))], "sigue");
        assert_eq!(p.matches(&big).count(), 1, "solo cabe el mensaje más reciente");
    }

    #[test]
    fn conversaciones_separadas_por_proyecto() {
        let tmp = std::env::temp_dir().join(format!("agentes-test-{}", std::process::id()));
        std::env::set_var("CHAT_DATA_DIR", &tmp);
        let a = store::append(None, "/proyecto/a", vec![msg("user", "en A", None)]).unwrap();
        store::append(None, "/proyecto/b", vec![msg("user", "en B", None)]).unwrap();
        let list_a = store::list("/proyecto/a");
        assert_eq!(list_a.len(), 1);
        assert_eq!(list_a[0].id, a.id);
        assert_eq!(store::list("/proyecto/b")[0].title, "en B");
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn panel_lee_arbol_y_lineas_cambiadas() {
        let root = std::env::temp_dir().join(format!("agentes-ws-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::create_dir_all(root.join("node_modules/x")).unwrap();
        std::fs::write(root.join("src/a.txt"), "uno\ndos\ntres\n").unwrap();
        std::fs::write(root.join("node_modules/x/b.js"), "x").unwrap();
        let sh = |args: &[&str]| {
            std::process::Command::new("git").args(args).current_dir(&root).output().unwrap();
        };
        sh(&["init", "-q"]);
        sh(&["add", "."]);
        sh(&["-c", "user.name=t", "-c", "user.email=t@t", "commit", "-qm", "init"]);
        std::fs::write(root.join("src/a.txt"), "uno\nDOS\ntres\ncuatro\n").unwrap();
        std::fs::write(root.join("nuevo.md"), "hola\n").unwrap();

        let t = workspace::tree(&root);
        assert!(t.is_git);
        assert!(t.files.contains(&"src/a.txt".to_string()));
        assert!(!t.files.iter().any(|f| f.starts_with("node_modules")));
        assert_eq!(t.status.get("src/a.txt").map(String::as_str), Some("M"));
        assert_eq!(t.status.get("nuevo.md").map(String::as_str), Some("?"));

        let f = workspace::read(&root, "src/a.txt", true).unwrap();
        assert_eq!(f.modified, vec![2]);
        assert_eq!(f.added, vec![4]);
        assert!(f.tracked);
        assert_eq!(workspace::head(&root, "src/a.txt").unwrap(), "uno\ndos\ntres\n");
        let n = workspace::read(&root, "nuevo.md", true).unwrap();
        assert_eq!(n.added, vec![1]);
        assert!(!n.tracked);
        // Borrar una línea deja una marca de borrado
        std::fs::write(root.join("src/a.txt"), "uno\ntres\n").unwrap();
        let d = workspace::read(&root, "src/a.txt", true).unwrap();
        assert_eq!((d.deleted.clone(), d.deleted_count), (vec![1], 1));
        workspace::write(&root, "src/a.txt", "guardado\n").unwrap();
        assert_eq!(std::fs::read_to_string(root.join("src/a.txt")).unwrap(), "guardado\n");
        assert!(workspace::write(&root, "../fuera.txt", "x").is_err());
        assert!(workspace::read(&root, "../fuera.txt", true).is_err());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn quita_colores_ansi() {
        assert_eq!(env::strip_ansi("\x1b[0m\x1b[1;32mok\x1b[0m"), "ok");
    }

    // Llaman a las CLIs reales: `cargo test -- --ignored --nocapture`
    #[tokio::test]
    #[ignore]
    async fn estados_reales() {
        for a in agents::AGENTS {
            println!("{}: {}", a.id, agents::status(a).await);
        }
    }

    #[tokio::test]
    #[ignore]
    async fn memoria_compartida_entre_agentes() {
        let dir = std::env::temp_dir();
        let codex = agents::get("codex").unwrap();
        let claude = agents::get("claude").unwrap();
        let first = "Mi número de la suerte es el 42. Responde solo: anotado.";
        let r1 = agents::run(codex, &agents::build_prompt(codex, &[], first), &dir).await.unwrap();
        println!("Codex: {r1}");
        let history = vec![msg("user", first, None), msg("assistant", &r1, Some("Codex"))];
        let q = "¿Cuál es mi número de la suerte y qué asistente respondió antes? Una frase.";
        let r2 = agents::run(claude, &agents::build_prompt(claude, &history, q), &dir).await.unwrap();
        println!("Claude: {r2}");
        assert!(r2.contains("42") && r2.contains("Codex"));
    }
}
