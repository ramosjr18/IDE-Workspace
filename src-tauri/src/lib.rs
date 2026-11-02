//! Chat de escritorio para agentes CLI. Capas:
//!   ui/index.html → chat (la ventana)
//!   store.rs      → caché propia de conversaciones, compartida por todos los agentes
//!   agents.rs     → adaptadores: cómo cada CLI recibe esa conversación y responde
//!   project.rs    → carpeta del proyecto donde trabajan los agentes (como `claude` o `code .`)
//!   council.rs    → consejo: varios agentes deliberan y convergen en un ADR o política
//!   term.rs       → terminal para instalar agentes e iniciar sesión
//!   workspace.rs  → panel derecho: árbol de archivos y archivo que se está editando

mod agents;
mod council;
mod env;
mod project;
mod store;
mod stream;
mod term;
mod workspace;
mod usage;

use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter, Manager, State, WebviewWindow};

#[derive(Default)]
struct Busy(Mutex<HashSet<String>>);

/// Ejecuciones en curso, para poder detenerlas: id de la petición → aviso de cancelación.
#[derive(Default)]
struct Running(Mutex<HashMap<String, tokio::sync::oneshot::Sender<()>>>);

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct StepEvent<'a> {
    request_id: &'a str,
    step: &'a stream::Step,
}

/// Los pasos se guardan con la salida recortada, para que el historial no ocupe demasiado.
fn steps_for_storage(steps: Vec<stream::Step>) -> Option<Vec<stream::Step>> {
    if steps.is_empty() {
        return None;
    }
    Some(
        steps
            .into_iter()
            .map(|mut s| {
                if let Some(o) = &s.output {
                    let n = o.chars().count();
                    if n > 2000 {
                        s.output = Some(format!("…{}", o.chars().skip(n - 2000).collect::<String>()));
                    }
                }
                s
            })
            .collect(),
    )
}

/// Detiene la respuesta en curso.
#[tauri::command]
fn cancel_chat(running: State<'_, Running>, request_id: String) {
    if let Some(tx) = running.0.lock().unwrap().remove(&request_id) {
        let _ = tx.send(());
    }
}

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
    app: AppHandle,
    busy: State<'_, Busy>,
    running: State<'_, Running>,
    current: State<'_, CurrentProject>,
    request_id: String,
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
    let (tx, rx) = tokio::sync::oneshot::channel();
    running.0.lock().unwrap().insert(request_id.clone(), tx);
    let on_step = |step: &stream::Step| {
        let _ = app.emit("agent-step", StepEvent { request_id: &request_id, step });
    };
    let result = agents::run(ag, &agents::build_prompt(ag, &previous, &message), &cwd, on_step, rx).await;
    running.0.lock().unwrap().remove(&request_id);
    busy.0.lock().unwrap().remove(&key);
    let result = result?;
    // Solo se guarda el intercambio si el agente respondió bien.
    let conv = store::append(
        conversation_id.as_deref(),
        &cwd.to_string_lossy(),
        vec![
            store::Message { role: "user".into(), text: message, agent: None, agent_name: None, ts: String::new(), steps: None },
            store::Message {
                role: "assistant".into(),
                text: result.text,
                agent: Some(ag.id.into()),
                agent_name: Some(ag.name.into()),
                ts: String::new(),
                steps: steps_for_storage(result.steps),
            },
        ],
    )?;
    Ok(ChatReply { conversation_id: conv.id.clone(), title: conv.title.clone(), message: conv.messages.last().cloned().unwrap() })
}

// ---------- Consejo de agentes ----------

#[derive(Serialize)]
struct RoleInfo {
    id: &'static str,
    name: &'static str,
    description: &'static str,
}

#[tauri::command]
fn council_roles() -> Vec<RoleInfo> {
    council::ROLES.iter().map(|(id, name, description)| RoleInfo { id, name, description }).collect()
}

/// Contexto inicial del consejo: la conversación actual en texto (lo más reciente, hasta ~12 000 caracteres).
#[tauri::command]
fn council_context(conversation_id: Option<String>) -> String {
    let Some(conv) = conversation_id.as_deref().and_then(store::get) else { return String::new() };
    let mut lines: Vec<String> = Vec::new();
    let mut total = 0;
    for m in conv.messages.iter().rev() {
        let who = if m.role == "user" { "Usuario" } else { m.agent_name.as_deref().unwrap_or("Asistente") };
        let line = format!("[{who}]: {}", m.text.trim());
        total += line.chars().count();
        if total > 12_000 && !lines.is_empty() {
            break;
        }
        lines.push(line);
    }
    lines.reverse();
    lines.join("\n\n")
}

#[tauri::command]
fn council_list(current: State<'_, CurrentProject>) -> Vec<council::Summary> {
    council::list(&current.get().to_string_lossy())
}

#[tauri::command]
fn council_get(id: String) -> Result<council::Council, String> {
    council::get(&id).ok_or_else(|| "Consejo no encontrado".into())
}

#[derive(serde::Deserialize)]
struct MemberArg {
    agent: String,
    role: String,
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
fn council_start(
    app: AppHandle,
    current: State<'_, CurrentProject>,
    runs: State<'_, Arc<council::Runs>>,
    conversation_id: Option<String>,
    topic: String,
    output: String,
    context: String,
    members: Vec<MemberArg>,
    moderator: String,
    judge_enabled: bool,
    judge_pool: Vec<String>,
    judge_max_rounds: u32,
) -> Result<council::Council, String> {
    let members = members.into_iter().map(|m| council::NewMember { agent: m.agent, role: m.role }).collect();
    let judge = council::Judge { enabled: judge_enabled, pool: judge_pool, max_rounds: judge_max_rounds.min(5), used_rounds: 0 };
    let c = council::create(&current.get(), conversation_id, topic, output, context, members, moderator, judge)?;
    council::launch(app, runs.inner().clone(), c.clone(), council::Plan::Full)?;
    Ok(c)
}

/// Otra ronda de contraste (y nueva síntesis), o solo rehacer la síntesis.
#[tauri::command]
fn council_round(app: AppHandle, runs: State<'_, Arc<council::Runs>>, id: String, synthesis_only: bool) -> Result<(), String> {
    let c = council::get(&id).ok_or("Consejo no encontrado")?;
    let plan = if synthesis_only { council::Plan::SynthesisOnly } else { council::Plan::AnotherRound };
    council::launch(app, runs.inner().clone(), c, plan)
}

/// Comentario del usuario en el tablero (opcionalmente en respuesta a una tarjeta).
#[tauri::command]
fn council_comment(app: AppHandle, id: String, text: String, kind: String, reply_to: Option<String>) -> Result<council::Council, String> {
    let mut c = council::get(&id).ok_or("Consejo no encontrado")?;
    if text.trim().is_empty() {
        return Err("Escribe el comentario".into());
    }
    let reply_to = reply_to.filter(|r| c.cards.iter().any(|k| &k.id == r));
    council::add_cards(&mut c, "user", "Tú", vec![(kind, reply_to, text.trim().to_string())]);
    council::save(&mut c)?;
    let _ = app.emit("council-update", &c);
    Ok(c)
}

#[tauri::command]
fn council_cancel(runs: State<'_, Arc<council::Runs>>, id: String) {
    runs.cancel(&id)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Approved {
    council: council::Council,
    conversation_id: String,
}

/// Aprueba el documento (con los cambios que hayas hecho), lo guarda en el repo y deja un
/// resumen en el chat para que el siguiente agente lo tenga como contexto.
#[tauri::command]
fn council_approve(app: AppHandle, id: String, document: String) -> Result<Approved, String> {
    let mut c = council::get(&id).ok_or("Consejo no encontrado")?;
    if document.trim().is_empty() {
        return Err("El documento está vacío".into());
    }
    let rel = council::write_document(&c, &document)?;
    if let Some(s) = c.synthesis.as_mut() {
        s.document = document.clone();
    }
    c.document_path = Some(rel.clone());
    c.phase = "approved".into();
    council::save(&mut c)?;
    let _ = app.emit("council-update", &c);
    let s = c.synthesis.clone().unwrap_or_default();
    let mut summary = format!(
        "Consejo sobre «{}» aprobado ({}). Documento: {rel}",
        c.topic,
        c.members.iter().map(|m| format!("{} · {}", m.agent_name, m.role_name)).collect::<Vec<_>>().join(", ")
    );
    if !s.agreements.is_empty() {
        summary.push_str(&format!("\n\nAcuerdos:\n- {}", s.agreements.join("\n- ")));
    }
    if !s.open.is_empty() {
        summary.push_str(&format!("\n\nPuntos abiertos:\n- {}", s.open.join("\n- ")));
    }
    let conv = store::append(
        c.conversation_id.as_deref(),
        &c.project,
        vec![store::Message {
            role: "assistant".into(),
            text: summary,
            agent: Some("consejo".into()),
            agent_name: Some("Consejo".into()),
            ts: String::new(),
            steps: None,
        }],
    )?;
    Ok(Approved { council: c, conversation_id: conv.id })
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
        .manage(usage::UsageState::default())
        .manage(Running::default())
        .manage(Arc::new(council::Runs::default()))
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
            usage::get_usage,
            chat,
            cancel_chat,
            council_roles,
            council_context,
            council_list,
            council_get,
            council_start,
            council_round,
            council_comment,
            council_cancel,
            council_approve,
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
            steps: None,
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
        let never = || tokio::sync::oneshot::channel::<()>().1;
        let r1 = agents::run(codex, &agents::build_prompt(codex, &[], first), &dir, |s| println!("  paso: {s:?}"), never()).await.unwrap().text;
        println!("Codex: {r1}");
        let history = vec![msg("user", first, None), msg("assistant", &r1, Some("Codex"))];
        let q = "¿Cuál es mi número de la suerte y qué asistente respondió antes? Una frase.";
        let r2 = agents::run(claude, &agents::build_prompt(claude, &history, q), &dir, |s| println!("  paso: {s:?}"), never()).await.unwrap().text;
        println!("Claude: {r2}");
        assert!(r2.contains("42") && r2.contains("Codex"));
    }
}
