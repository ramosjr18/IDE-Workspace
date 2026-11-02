//! Caché propia: las conversaciones en disco, independientes del agente que respondió.
//! Cada conversación es un JSON en ~/.ide-workspace-chat/conversations (o en CHAT_DATA_DIR).

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Message {
    pub role: String,
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_name: Option<String>,
    #[serde(default)]
    pub ts: String,
    /// Pasos que hizo el agente (comandos, archivos…), para volver a mostrarlos.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub steps: Option<Vec<crate::stream::Step>>,
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Conversation {
    pub id: String,
    pub title: String,
    pub created_at: String,
    pub updated_at: String,
    /// Carpeta del proyecto. Las conversaciones antiguas sin proyecto cuentan como la carpeta de usuario.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project: Option<String>,
    pub messages: Vec<Message>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Summary {
    pub id: String,
    pub title: String,
    pub updated_at: String,
}

pub fn data_dir() -> PathBuf {
    std::env::var("CHAT_DATA_DIR").map(PathBuf::from).unwrap_or_else(|_| crate::env::home().join(".ide-workspace-chat"))
}

fn conv_dir() -> PathBuf {
    let dir = data_dir().join("conversations");
    let _ = fs::create_dir_all(&dir);
    dir
}

fn valid_id(id: &str) -> bool {
    id.len() == 36 && id.chars().all(|c| c.is_ascii_hexdigit() || c == '-')
}

pub fn now() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

/// Escribe en un archivo temporal y lo renombra, para no dejar nunca un JSON a medias.
pub fn write_atomic(path: &PathBuf, json: &str) -> Result<(), String> {
    let tmp = path.with_extension(format!("{}.tmp", std::process::id()));
    fs::write(&tmp, json).map_err(|e| e.to_string())?;
    fs::rename(&tmp, path).map_err(|e| e.to_string())
}

pub fn get(id: &str) -> Option<Conversation> {
    if !valid_id(id) {
        return None;
    }
    let text = fs::read_to_string(conv_dir().join(format!("{id}.json"))).ok()?;
    serde_json::from_str(&text).ok()
}

fn save(conv: &Conversation) -> Result<(), String> {
    let json = serde_json::to_string_pretty(conv).map_err(|e| e.to_string())?;
    write_atomic(&conv_dir().join(format!("{}.json", conv.id)), &json)
}

/// Añade mensajes a la conversación `id`; si no existe (o es None), crea una nueva en `project`.
pub fn append(id: Option<&str>, project: &str, messages: Vec<Message>) -> Result<Conversation, String> {
    let ts = now();
    let mut conv = id.and_then(get).unwrap_or_else(|| Conversation {
        id: uuid::Uuid::new_v4().to_string(),
        title: "Conversación nueva".into(),
        created_at: ts.clone(),
        updated_at: ts.clone(),
        project: Some(project.to_string()),
        messages: vec![],
    });
    for mut m in messages {
        m.ts = ts.clone();
        conv.messages.push(m);
    }
    if let Some(first) = conv.messages.iter().find(|m| m.role == "user") {
        conv.title = first.text.split_whitespace().collect::<Vec<_>>().join(" ").chars().take(60).collect();
    }
    conv.updated_at = ts;
    save(&conv)?;
    Ok(conv)
}

fn project_of(c: &Conversation) -> String {
    c.project.clone().unwrap_or_else(|| crate::env::home().to_string_lossy().to_string())
}

/// Conversaciones de un proyecto, de la más reciente a la más antigua.
pub fn list(project: &str) -> Vec<Summary> {
    let mut out: Vec<Summary> = fs::read_dir(conv_dir())
        .map(|rd| {
            rd.filter_map(|e| e.ok())
                .filter_map(|e| e.file_name().to_str()?.strip_suffix(".json").map(str::to_string))
                .filter_map(|id| get(&id))
                .filter(|c| !c.messages.is_empty() && project_of(c) == project)
                .map(|c| Summary { id: c.id, title: c.title, updated_at: c.updated_at })
                .collect()
        })
        .unwrap_or_default();
    out.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    out
}
