//! Chat de escritorio para agentes CLI. Capas:
//!   ui/index.html → chat (la ventana)
//!   store.rs      → caché propia de conversaciones, compartida por todos los agentes
//!   agents.rs     → adaptadores: cómo cada CLI recibe esa conversación y responde
//!   project.rs    → carpeta del proyecto donde trabajan los agentes (como `claude` o `code .`)
//!   council.rs    → consejo: varios agentes deliberan y convergen en un ADR o política
//!   term.rs       → terminales (PTY): instalar agentes, pestaña Terminal y paneles de la vista Agentes
//!   panes.rs      → vista Agentes: `agentes run`, worktrees y paneles en mosaico
//!   workflows.rs  → formas de trabajar (instrucciones que se añaden al agente del chat)
//!   workspace.rs  → panel derecho: árbol de archivos y archivo que se está editando

mod accounts;
mod agents;
mod council;
mod env;
mod own_agents;
mod panes;
mod project;
mod roles;
mod store;
mod stream;
mod term;
mod workflows;
mod workspace;
mod usage;

/// Modo cliente del ejecutable: `agentes run | status | wait | kill` (lo usa el `agentes` interno).
pub fn cli_main(args: Vec<String>) -> i32 {
    panes::cli_main(args)
}

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
/// `None` hasta que se elige una carpeta en la pantalla de inicio.
struct CurrentProject(Mutex<Option<PathBuf>>);

const NO_PROJECT: &str = "Sin proyecto";

impl CurrentProject {
    fn get(&self) -> Result<PathBuf, String> {
        self.0.lock().unwrap().clone().ok_or_else(|| NO_PROJECT.into())
    }
    fn key(&self) -> Option<String> {
        self.0.lock().unwrap().as_ref().map(|p| p.to_string_lossy().to_string())
    }
}

fn set_title(window: &WebviewWindow, project: Option<&project::Project>) {
    let _ = window.set_title(&match project {
        Some(p) => format!("{} — Chat de agentes", p.name),
        None => "Chat de agentes".into(),
    });
}

#[tauri::command]
fn get_project(current: State<'_, CurrentProject>) -> Option<project::Project> {
    current.get().ok().map(|p| project::Project::from_path(&p))
}

#[tauri::command]
fn recent_projects() -> Vec<project::Project> {
    project::recent()
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
    *current.0.lock().unwrap() = Some(dir.clone());
    project::remember(&dir);
    let _ = watcher.watch(app, dir.clone());
    let p = project::Project::from_path(&dir);
    set_title(&window, Some(&p));
    Ok(p)
}

#[tauri::command]
async fn get_tree(current: State<'_, CurrentProject>) -> Result<workspace::Tree, String> {
    let root = current.get()?;
    tauri::async_runtime::spawn_blocking(move || workspace::tree(&root)).await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn read_file(current: State<'_, CurrentProject>, path: String) -> Result<workspace::FileView, String> {
    let root = current.get()?;
    tauri::async_runtime::spawn_blocking(move || workspace::read(&root, &path, workspace::is_git(&root)))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn read_head(current: State<'_, CurrentProject>, path: String) -> Result<String, String> {
    let root = current.get()?;
    tauri::async_runtime::spawn_blocking(move || workspace::head(&root, &path)).await.map_err(|e| e.to_string())?
}

#[tauri::command]
fn write_file(current: State<'_, CurrentProject>, path: String, content: String) -> Result<(), String> {
    workspace::write(&current.get()?, &path, &content)
}

/// Guarda en disco lo que la ventana «descarga» (exportar la pizarra a PNG o SVG…): la ruta la elige el usuario en el diálogo «Guardar».
#[tauri::command]
fn save_download(path: String, data: Vec<u8>) -> Result<(), String> {
    std::fs::write(&path, data).map_err(|e| format!("No se pudo guardar {path}: {e}"))
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
    workflow: Option<String>,
    account: Option<String>,
) -> Result<ChatReply, String> {
    let ag = agents::get(&agent).ok_or_else(|| format!("Agente desconocido: {agent}"))?;
    if message.trim().is_empty() {
        return Err("Mensaje vacío".into());
    }
    let cwd = current.get()?;
    if !cwd.is_dir() {
        return Err(format!("La carpeta del proyecto ya no existe: {}", cwd.display()));
    }
    let (previous, sessions) =
        conversation_id.as_deref().and_then(store::get).map(|c| (c.messages, c.sessions)).unwrap_or_default();
    let key = conversation_id.clone().unwrap_or_default();
    let flow = workflow.as_deref().and_then(workflows::get);
    let flow_id = flow.as_ref().map(|w| w.id.clone()).unwrap_or_else(|| "directo".into());
    if !key.is_empty() && !busy.0.lock().unwrap().insert(key.clone()) {
        return Err("Esta conversación ya está esperando una respuesta".into());
    }
    let on_step = |step: &stream::Step| {
        let _ = app.emit("agent-step", StepEvent { request_id: &request_id, step });
    };
    // Cuenta: la indicada, o «auto» (la de más cuota libre). Si se agota, la siguiente con saldo;
    // sin ninguna en esta CLI, otra CLI con cuota (docs/plan-cuentas.md §6).
    let fixed = account.as_deref().filter(|a| *a != "auto").and_then(accounts::get);
    let loads = if fixed.is_some() { Vec::new() } else { usage::loads(&app.state::<usage::UsageState>()).await };
    let mut tried: Vec<String> = Vec::new();
    let mut relays: Vec<String> = Vec::new();
    let mut used = ag;
    let mut outcome: Result<agents::RunResult, String> = Err("Sin cuentas disponibles".into());
    let mut session_of: Option<(String, String)> = None;
    for _ in 0..8 {
        let next = match &fixed {
            Some(a) if tried.is_empty() => Some((ag, a.clone())),
            Some(_) => None,
            None => std::iter::once(ag.id)
                .chain(accounts::FALLBACK.iter().copied().filter(|c| *c != ag.id))
                .filter_map(agents::get)
                .find_map(|cand| accounts::pick(cand.id, &loads, &tried).map(|a| (cand, a))),
        };
        // Ninguna por debajo del umbral: la principal de la CLI elegida (que dé su propio error si no puede).
        let Some((cand, acc)) = next.or_else(|| tried.is_empty().then(|| (ag, accounts::Account::principal(ag.id)))) else { break };
        tried.push(acc.id.clone());
        // Sesión de esta CLI y cuenta en la conversación: se reanuda si la CLI la sigue teniendo
        // (docs/plan-reanudar-sesiones.md §3). Si no, sesión nueva con el historial como texto.
        let mut resume = sessions.get(&acc.id).filter(|_| cand.session != agents::SessionMode::None).cloned();
        if cand.session == agents::SessionMode::Flag("-s") {
            if let Some(s) = &resume {
                if !accounts::scope(acc.clone(), agents::opencode_has_session(&s.id, &cwd)).await {
                    resume = None;
                }
            }
        }
        loop {
            let (tx, rx) = tokio::sync::oneshot::channel();
            running.0.lock().unwrap().insert(request_id.clone(), tx);
            let (session, prompt) = match &resume {
                Some(s) => {
                    let unseen = &previous[s.seen.min(previous.len())..];
                    let mut prompt = agents::build_resume_prompt(cand, unseen, &message);
                    // La forma de trabajar ya la tiene la sesión; solo se repite si ha cambiado.
                    if s.workflow != flow_id {
                        let change = match flow.as_ref().filter(|w| !w.instructions.is_empty()) {
                            Some(w) => format!("A partir de ahora, sigue esta forma de trabajar:\n\n{}", w.instructions),
                            None => "A partir de ahora, deja la forma de trabajar anterior: responde y trabaja tú directamente.".into(),
                        };
                        prompt = format!("{change}\n\n{prompt}");
                    }
                    (agents::Session::Resume(s.id.clone()), prompt)
                }
                None => {
                    // Forma de trabajar y nota del lanzador: van en el prompt, no se guardan en la conversación.
                    let mut prompt = agents::build_prompt(cand, &previous, &message);
                    if let Some(w) = flow.as_ref().filter(|w| !w.instructions.is_empty()) {
                        prompt = format!("{}\n\n{prompt}", w.instructions);
                    }
                    if env::launcher_ready() {
                        prompt = format!("{}\n\n{prompt}", workflows::LAUNCHER_NOTE);
                    }
                    (agents::Session::New, prompt)
                }
            };
            outcome = accounts::scope(acc.clone(), agents::run_session(cand, &session, &prompt, &cwd, &on_step, rx)).await;
            // La CLI ya no tiene la sesión: se repite una vez con sesión nueva. agy no falla: abre
            // otra conversación y lo dice con un id distinto.
            let lost = match (&resume, &outcome) {
                (Some(_), Err(e)) => agents::session_lost(cand, e),
                (Some(s), Ok(r)) if cand.id == "agy" => r.session_id.as_deref().is_some_and(|id| id != s.id),
                _ => false,
            };
            if !lost {
                break;
            }
            if let Some(id) = conversation_id.as_deref() {
                let _ = store::forget_session(id, &acc.id);
            }
            resume = None;
        }
        session_of = outcome.as_ref().ok().and_then(|r| r.session_id.clone()).map(|id| (acc.id.clone(), id));
        used = cand;
        // El aviso de límite puede llegar como error o como respuesta corta (p. ej. Claude en stream-json).
        let failed_text = match &outcome {
            Err(e) => Some(e.as_str()),
            Ok(r) if r.text.chars().count() < 600 => Some(r.text.as_str()),
            Ok(_) => None,
        };
        match failed_text.and_then(|t| accounts::exhaustion::detect(cand.id, t, 1)) {
            Some(x) if fixed.is_none() => {
                relays.push(format!("{} ({}): {}", cand.name, acc.label(), x.reason));
                outcome = Err(x.reason);
            }
            _ => break,
        }
    }
    running.0.lock().unwrap().remove(&request_id);
    busy.0.lock().unwrap().remove(&key);
    let mut result = outcome?;
    let account_used = tried.last().and_then(|id| accounts::get(id)).and_then(|a| a.name);
    if !relays.is_empty() {
        result.text = format!("*Relevo de cuenta: {}.*\n\n{}", relays.join(" · "), result.text);
    }
    // Solo se guarda el intercambio si el agente respondió bien.
    let conv = store::append(
        conversation_id.as_deref(),
        &cwd.to_string_lossy(),
        vec![
            store::Message { role: "user".into(), text: message, agent: None, agent_name: None, ts: String::new(), steps: None },
            store::Message {
                role: "assistant".into(),
                text: result.text,
                agent: Some(used.id.into()),
                agent_name: Some(match &account_used {
                    Some(n) => format!("{} · {n}", used.name),
                    None => used.name.into(),
                }),
                ts: String::new(),
                steps: steps_for_storage(result.steps),
            },
        ],
    )?;
    if let Some((acc_id, sid)) = session_of {
        store::set_session(&conv.id, &acc_id, sid, flow_id)?;
    }
    Ok(ChatReply { conversation_id: conv.id.clone(), title: conv.title.clone(), message: conv.messages.last().cloned().unwrap() })
}

// ---------- Consejo de agentes ----------

/// Roles disponibles en el consejo, tal como están configurados.
#[tauri::command]
fn council_roles() -> Vec<roles::Role> {
    roles::load().roles
}

/// Roles y plantillas del consejo, para editarlos desde la app.
#[tauri::command]
fn council_config_get() -> roles::Config {
    roles::load()
}

/// Guarda la configuración y devuelve la que queda vigente.
#[tauri::command]
fn council_config_save(config: roles::Config) -> Result<roles::Config, String> {
    roles::save(&config)?;
    Ok(roles::load())
}

/// Agentes propios de la app y del proyecto, tal como están configurados.
#[tauri::command]
fn own_agents_list(current: State<'_, CurrentProject>) -> Vec<own_agents::OwnAgent> {
    let project = current.get().ok();
    own_agents::load_all(project.as_deref())
}

/// Guarda el agente propio y devuelve la lista actualizada.
#[tauri::command]
fn own_agent_save(current: State<'_, CurrentProject>, agent: own_agents::OwnAgent) -> Result<Vec<own_agents::OwnAgent>, String> {
    let project = current.get().ok();
    own_agents::save(project.as_deref(), &agent)?;
    Ok(own_agents::load_all(project.as_deref()))
}

/// Borra el agente propio y devuelve la lista actualizada.
#[tauri::command]
fn own_agent_delete(current: State<'_, CurrentProject>, agent: own_agents::OwnAgent) -> Result<Vec<own_agents::OwnAgent>, String> {
    let project = current.get().ok();
    own_agents::delete(project.as_deref(), &agent)?;
    Ok(own_agents::load_all(project.as_deref()))
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
        let line_len = line.chars().count();
        if lines.is_empty() {
            if line_len > 12_000 {
                let truncated: String = line.chars().skip(line_len.saturating_sub(12_000)).collect();
                lines.push(truncated);
                break;
            } else {
                total += line_len;
                lines.push(line);
            }
        } else {
            if total + line_len > 12_000 {
                break;
            }
            total += line_len;
            lines.push(line);
        }
    }
    lines.reverse();
    lines.join("\n\n")
}

#[tauri::command]
fn council_list(current: State<'_, CurrentProject>) -> Vec<council::Summary> {
    current.key().map(|k| council::list(&k)).unwrap_or_default()
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
    let judge = council::Judge { enabled: judge_enabled, pool: judge_pool, max_rounds: judge_max_rounds.min(3), used_rounds: 0 };
    let c = council::create(&current.get()?, conversation_id, topic, output, context, members, moderator, judge)?;
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

/// Lee el layout de la pizarra (Excalidraw) del tablero del consejo; None si no hay pizarra guardada.
#[tauri::command]
fn council_board_get(id: String) -> Option<serde_json::Value> {
    council::board_get(&id)
}

/// Guarda el layout de la pizarra del tablero del consejo.
#[tauri::command]
fn council_board_save(id: String, board: serde_json::Value) -> Result<(), String> {
    council::board_save(&id, &board)
}

/// Pizarra libre del proyecto (sin consejo).
#[tauri::command]
fn project_board_get(current: State<'_, CurrentProject>) -> Result<Option<serde_json::Value>, String> {
    Ok(project::board_get(&current.get()?))
}

#[tauri::command]
fn project_board_save(current: State<'_, CurrentProject>, board: serde_json::Value) -> Result<(), String> {
    project::board_save(&current.get()?, &board)
}

/// Diagramas del proyecto (una pizarra por diagrama), del más nuevo al más antiguo.
#[tauri::command]
fn diagrams_list(current: State<'_, CurrentProject>) -> Vec<project::Diagram> {
    current.get().map(|d| project::diagrams_list(&panes::repo_root(&d))).unwrap_or_default()
}

#[tauri::command]
fn diagram_get(current: State<'_, CurrentProject>, id: String) -> Result<Option<serde_json::Value>, String> {
    Ok(project::diagram_get(&panes::repo_root(&current.get()?), &id))
}

#[tauri::command]
fn diagram_save(current: State<'_, CurrentProject>, id: String, board: serde_json::Value) -> Result<(), String> {
    project::diagram_save(&panes::repo_root(&current.get()?), &id, &board)
}

/// Desde el chat: un bloque ```mermaid de una respuesta se convierte en su pizarra.
#[tauri::command]
fn diagram_create(current: State<'_, CurrentProject>, title: String, format: String, source: String) -> Result<project::Diagram, String> {
    project::diagram_create(&panes::repo_root(&current.get()?), &title, &format, &source)
}

#[tauri::command]
fn diagram_delete(current: State<'_, CurrentProject>, id: String) -> Result<(), String> {
    project::diagram_delete(&panes::repo_root(&current.get()?), &id)
}

#[tauri::command]
fn list_conversations(current: State<'_, CurrentProject>) -> Vec<store::Summary> {
    current.key().map(|k| store::list(&k)).unwrap_or_default()
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
    account: Option<String>,
) -> Result<u32, String> {
    let ag = agents::get(&agent).ok_or("Agente desconocido")?;
    if action != "install" && action != "login" {
        return Err("Acción no válida".into());
    }
    let (command, shell, args) = agents::terminal_command(ag, &action);
    let file = env::which(&shell).unwrap_or_else(|| PathBuf::from(&shell));
    let mut job = term::Job::new(command, file, args, env::home());
    // Iniciar sesión en una cuenta extra: la CLI guarda la sesión en la carpeta de esa cuenta.
    if let Some(acc) = account.as_deref().and_then(accounts::get) {
        job.env = acc.env();
    }
    terms.start(app, job, cols, rows)
}

#[derive(Serialize)]
struct CliAccounts {
    cli: String,
    supported: bool,
    accounts: Vec<accounts::Account>,
}

/// Cuentas de cada CLI (la principal siempre está).
#[tauri::command]
fn accounts_list() -> Vec<CliAccounts> {
    agents::AGENTS
        .iter()
        .map(|a| CliAccounts { cli: a.id.into(), supported: accounts::supports_accounts(a.id), accounts: accounts::list(a.id) })
        .collect()
}

#[tauri::command]
fn account_add(usage: State<'_, usage::UsageState>, cli: String, name: String) -> Result<accounts::Account, String> {
    let acc = accounts::add(&cli, name.trim())?;
    usage.invalidate();
    Ok(acc)
}

#[tauri::command]
fn account_remove(usage: State<'_, usage::UsageState>, id: String) -> Result<(), String> {
    accounts::remove(&id)?;
    usage.invalidate();
    Ok(())
}

/// Shell interactiva de la pestaña Terminal, en la carpeta del proyecto (o en la de usuario si no hay).
#[tauri::command]
fn term_shell(app: AppHandle, terms: State<'_, term::Terminals>, current: State<'_, CurrentProject>, cols: u16, rows: u16) -> Result<u32, String> {
    let cwd = current.get().unwrap_or_else(|_| env::home());
    let (file, args) = env::interactive_shell();
    let command = file.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    terms.start(app, term::Job::new(command, file, args, cwd), cols, rows)
}

#[tauri::command]
fn term_replay(terms: State<'_, term::Terminals>, id: u32) -> String {
    terms.replay(id)
}

#[tauri::command]
fn list_workflows() -> Vec<workflows::Workflow> {
    workflows::list()
}

/// Paneles de la vista Agentes del proyecto actual.
#[tauri::command]
fn panes_list(current: State<'_, CurrentProject>, panes: State<'_, Arc<panes::Panes>>) -> Vec<panes::Pane> {
    match current.get() {
        Ok(dir) => panes.list(&panes::repo_root(&dir).to_string_lossy()),
        Err(_) => Vec::new(),
    }
}

/// «+ Lanzar» desde la vista Agentes.
#[tauri::command]
async fn pane_launch(
    app: AppHandle,
    current: State<'_, CurrentProject>,
    cli: String,
    name: Option<String>,
    worktree: Option<String>,
    branch: Option<String>,
    args: Vec<String>,
    brief: Option<String>,
    account: Option<String>,
) -> Result<panes::Pane, String> {
    let cwd = current.get()?.to_string_lossy().to_string();
    let req = panes::LaunchRequest { cwd, cli, name, worktree, branch, args, stdin: brief, account };
    let loads = usage::loads(&app.state::<usage::UsageState>()).await;
    tauri::async_runtime::spawn_blocking(move || panes::launch(&app, req, &loads)).await.map_err(|e| e.to_string())?
}

#[tauri::command]
fn pane_kill(terms: State<'_, term::Terminals>, panes: State<'_, Arc<panes::Panes>>, id: u32) {
    panes.stop(&terms, id)
}

#[tauri::command]
fn pane_close(app: AppHandle, terms: State<'_, term::Terminals>, panes: State<'_, Arc<panes::Panes>>, id: u32) {
    panes.stop(&terms, id);
    if let Some(t) = panes.term_of(id) {
        terms.forget(t);
    }
    panes.remove(id);
    let _ = app.emit("panes-changed", ());
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
    if let Some(dir) = &initial {
        project::remember(dir);
    }
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .manage(Busy::default())
        .manage(usage::UsageState::default())
        .manage(Running::default())
        .manage(Arc::new(council::Runs::default()))
        .manage(CurrentProject(Mutex::new(initial.clone())))
        .manage(term::Terminals::default())
        .manage(Arc::new(panes::Panes::default()))
        .manage(workspace::Watcher::default())
        .setup(move |app| {
            if let Some(window) = app.get_webview_window("main") {
                set_title(&window, initial.as_deref().map(project::Project::from_path).as_ref());
            }
            if let Some(dir) = initial.clone() {
                let _ = app.state::<workspace::Watcher>().watch(app.handle().clone(), dir);
            }
            panes::start_server(app.handle().clone());
            #[cfg(debug_assertions)]
            workspace::reload_on_ui_change(app.handle().clone());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_project,
            recent_projects,
            open_project,
            install_cli,
            get_tree,
            read_file,
            read_head,
            write_file,
            save_download,
            list_agents,
            usage::get_usage,
            chat,
            cancel_chat,
            council_roles,
            council_config_get,
            council_config_save,
            own_agents_list,
            own_agent_save,
            own_agent_delete,
            council_context,
            council_list,
            council_get,
            council_start,
            council_round,
            council_comment,
            council_cancel,
            council_approve,
            council_board_get,
            council_board_save,
            project_board_get,
            project_board_save,
            diagrams_list,
            diagram_get,
            diagram_save,
            diagram_create,
            diagram_delete,
            list_conversations,
            get_conversation,
            term_start,
            term_shell,
            term_replay,
            list_workflows,
            accounts_list,
            account_add,
            account_remove,
            panes_list,
            pane_launch,
            pane_kill,
            pane_close,
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
        // Sesiones de CLI: `seen` = mensajes que había al guardarla.
        store::set_session(&a.id, "claude", "S1".into(), "directo".into()).unwrap();
        store::append(Some(&a.id), "/proyecto/a", vec![msg("user", "otra", None), msg("assistant", "vale", Some("Codex"))]).unwrap();
        let c = store::get(&a.id).unwrap();
        assert_eq!(c.sessions["claude"], store::CliSession { id: "S1".into(), seen: 1, workflow: "directo".into() });
        assert_eq!(c.messages[c.sessions["claude"].seen..].len(), 2, "Claude no ha visto lo de Codex");
        store::forget_session(&a.id, "claude").unwrap();
        assert!(store::get(&a.id).unwrap().sessions.is_empty());
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

    /// Claude en vivo: dos mensajes reanudando la sesión frente a dos sesiones nuevas con el historial.
    /// `AGENTES_LIVE_DIR=~/workspace/pablo cargo test sesion_reanudada_en_vivo -- --ignored --nocapture`
    #[tokio::test]
    #[ignore]
    async fn sesion_reanudada_en_vivo() {
        let dir = std::path::PathBuf::from(std::env::var("AGENTES_LIVE_DIR").expect("define AGENTES_LIVE_DIR"));
        let claude = agents::get("claude").unwrap();
        let never = || tokio::sync::oneshot::channel::<()>().1;
        let first = "Lee README.md y dime en una frase de qué trata.";
        let second = "¿Cuál era el título exacto (la primera línea) de ese archivo? No lo vuelvas a abrir.";
        let reads = |r: &agents::RunResult| r.steps.iter().filter(|s| s.kind != "text").count();

        // Reanudando
        let r1 = agents::run_session(claude, &agents::Session::New, first, &dir, |_| {}, never()).await.unwrap();
        let sid = r1.session_id.clone().expect("id de sesión");
        let p2 = agents::build_resume_prompt(claude, &[], second);
        let r2 = agents::run_session(claude, &agents::Session::Resume(sid.clone()), &p2, &dir, |_| {}, never()).await.unwrap();
        assert_eq!(r2.session_id.as_deref(), Some(sid.as_str()));
        println!("REANUDANDO  1: {:?}\n            2: {:?} pasos={} «{}»", r1.usage, r2.usage, reads(&r2), r2.text);

        // Como antes: sesión nueva con el historial como texto
        let history = vec![
            store::Message { role: "user".into(), text: first.into(), agent: None, agent_name: None, ts: String::new(), steps: None },
            store::Message { role: "assistant".into(), text: r1.text.clone(), agent: Some("claude".into()), agent_name: Some("Claude".into()), ts: String::new(), steps: Some(r1.steps.clone()) },
        ];
        let r3 = agents::run_session(claude, &agents::Session::None, &agents::build_prompt(claude, &history, second), &dir, |_| {}, never()).await.unwrap();
        println!("COMO ANTES  2: {:?} pasos={} «{}»", r3.usage, reads(&r3), r3.text);
    }
}
