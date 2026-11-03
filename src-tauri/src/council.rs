//! Consejo de agentes: varios agentes deliberan sobre un tema (política, ADR…) y convergen
//! en una propuesta antes de volver al chat de un solo agente.
//!
//! Fases: divergir (cada uno opina sin ver a los demás) → contrastar (cada uno lee al resto y
//! comenta) → converger (un moderador resume y redacta el ADR) → tú apruebas, pides otra ronda
//! o comentas. Durante el consejo los agentes trabajan en modo solo lectura.

use crate::agents::{self, Agent};
use crate::store::{data_dir, now, write_atomic};
use crate::stream::Step;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};

/// Roles predefinidos: id, nombre visible e instrucción para el agente.
pub const ROLES: &[(&str, &str, &str)] = &[
    ("arquitecto", "Arquitecto", "Piensa en la estructura a largo plazo: simplicidad, mantenibilidad, coherencia con lo que ya existe en el repo."),
    ("abogado", "Abogado del diablo", "Busca activamente los puntos débiles, los supuestos no demostrados y las alternativas que nadie ha considerado. No des la razón por cortesía."),
    ("seguridad", "Seguridad", "Evalúa riesgos de seguridad, privacidad, cumplimiento y qué puede salir mal en producción."),
    ("coste", "Coste y operación", "Evalúa el coste, el esfuerzo de implantación, la operación diaria y la carga para el equipo."),
    ("usuario", "Usuario y producto", "Defiende la experiencia de quien usa el producto y el valor que aporta la decisión."),
    ("libre", "Libre", "Da tu opinión sincera y razonada desde el ángulo que te parezca más importante."),
];

fn role_info(id: &str) -> (&'static str, &'static str) {
    ROLES.iter().find(|r| r.0 == id).map(|r| (r.1, r.2)).unwrap_or(("Libre", ROLES[5].2))
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Member {
    pub agent: String,
    pub agent_name: String,
    pub role: String,
    pub role_name: String,
    /// pending | running | done | error
    #[serde(default)]
    pub status: String,
    /// Último paso visible (comando, archivo…) o error.
    #[serde(default)]
    pub activity: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Card {
    /// C1, C2… para poder citarlas.
    pub id: String,
    /// 0 = divergir; 1, 2… = rondas de contraste.
    pub round: u32,
    /// Agente que la escribió, o "user".
    pub author: String,
    pub author_name: String,
    /// propuesta | riesgo | apoyo | objecion | pregunta | comentario
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reply_to: Option<String>,
    pub text: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct Synthesis {
    pub agreements: Vec<String>,
    pub open: Vec<String>,
    /// Borrador del documento final (Markdown).
    pub document: String,
}

/// Evaluador: un agente propio del IDE que, tras cada síntesis, juzga con ojos nuevos si lo
/// convergido se sostiene. Corre sobre una CLI elegida al azar (distinta del moderador) y, si no
/// le convence, manda al consejo a otra ronda con sus objeciones, hasta un máximo de rondas.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct Judge {
    pub enabled: bool,
    /// CLIs entre las que se elige al azar (las disponibles cuando se convocó).
    pub pool: Vec<String>,
    /// Rondas que puede pedir el evaluador por su cuenta.
    pub max_rounds: u32,
    /// Rondas que ya ha pedido.
    #[serde(default)]
    pub used_rounds: u32,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Verdict {
    pub round: u32,
    pub agent: String,
    pub agent_name: String,
    /// aprobar | otra_ronda | sin_veredicto
    pub decision: String,
    #[serde(default)]
    pub confidence: Option<u32>,
    pub reasons: Vec<String>,
    /// Preguntas u objeciones que el evaluador deja al consejo.
    pub questions: Vec<String>,
    /// El evaluador quería otra ronda, pero ya no quedaban rondas automáticas.
    #[serde(default)]
    pub capped: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Council {
    pub id: String,
    pub project: String,
    #[serde(default)]
    pub conversation_id: Option<String>,
    pub topic: String,
    /// adr | politica | nota
    pub output: String,
    pub context: String,
    pub members: Vec<Member>,
    pub moderator: String,
    /// diverge | contrast | converge | review | approved | error | cancelled
    pub phase: String,
    pub round: u32,
    pub cards: Vec<Card>,
    #[serde(default)]
    pub synthesis: Option<Synthesis>,
    #[serde(default)]
    pub document_path: Option<String>,
    #[serde(default)]
    pub error: Option<String>,
    #[serde(default)]
    pub judge: Judge,
    #[serde(default)]
    pub verdicts: Vec<Verdict>,
    pub created_at: String,
    pub updated_at: String,
}

fn dir() -> PathBuf {
    let d = data_dir().join("councils");
    let _ = fs::create_dir_all(&d);
    d
}

fn valid_id(id: &str) -> bool {
    id.len() == 36 && id.chars().all(|c| c.is_ascii_hexdigit() || c == '-')
}

pub fn get(id: &str) -> Option<Council> {
    if !valid_id(id) {
        return None;
    }
    serde_json::from_str(&fs::read_to_string(dir().join(format!("{id}.json"))).ok()?).ok()
}

pub fn save(c: &mut Council) -> Result<(), String> {
    c.updated_at = now();
    let json = serde_json::to_string_pretty(c).map_err(|e| e.to_string())?;
    write_atomic(&dir().join(format!("{}.json", c.id)), &json)
}

/// Pizarra (Excalidraw) del tablero del consejo: aparte del Council porque, mientras el consejo
/// está en marcha, la tarea en segundo plano sobrescribe su copia del Council con cada cambio.
fn boards_dir(base: &Path) -> PathBuf {
    let d = base.join("boards");
    let _ = fs::create_dir_all(&d);
    d
}

fn read_board(base: &Path, id: &str) -> Option<Value> {
    if !valid_id(id) {
        return None;
    }
    fs::read_to_string(boards_dir(base).join(format!("{id}.json")))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
}

fn write_board(base: &Path, id: &str, board: &Value) -> Result<(), String> {
    if !valid_id(id) {
        return Err("Consejo no válido".into());
    }
    if !board.is_object() {
        return Err("La pizarra debe ser un objeto JSON".into());
    }
    let json = serde_json::to_string(board).map_err(|e| e.to_string())?;
    if json.len() > 5_000_000 {
        return Err("La pizarra es demasiado grande".into());
    }
    write_atomic(&boards_dir(base).join(format!("{id}.json")), &json)
}

pub fn board_get(id: &str) -> Option<Value> {
    read_board(&dir(), id)
}

pub fn board_save(id: &str, board: &Value) -> Result<(), String> {
    get(id).ok_or("Consejo no encontrado")?;
    write_board(&dir(), id, board)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Summary {
    pub id: String,
    pub topic: String,
    pub phase: String,
    pub updated_at: String,
}

pub fn list(project: &str) -> Vec<Summary> {
    let mut out: Vec<Summary> = fs::read_dir(dir())
        .map(|rd| {
            rd.filter_map(|e| e.ok())
                .filter_map(|e| e.file_name().to_str()?.strip_suffix(".json").map(str::to_string))
                .filter_map(|id| get(&id))
                .filter(|c| c.project == project)
                .map(|c| Summary { id: c.id, topic: c.topic, phase: c.phase, updated_at: c.updated_at })
                .collect()
        })
        .unwrap_or_default();
    out.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    out
}

pub struct NewMember {
    pub agent: String,
    pub role: String,
}

pub fn create(
    project: &Path,
    conversation_id: Option<String>,
    topic: String,
    output: String,
    context: String,
    members: Vec<NewMember>,
    moderator: String,
    judge: Judge,
) -> Result<Council, String> {
    if topic.trim().is_empty() {
        return Err("Escribe el tema del consejo".into());
    }
    if members.len() < 2 {
        return Err("Elige al menos dos agentes".into());
    }
    let members = members
        .into_iter()
        .map(|m| {
            let a = agents::get(&m.agent).ok_or_else(|| format!("Agente desconocido: {}", m.agent))?;
            let (role_name, _) = role_info(&m.role);
            Ok(Member { agent: a.id.into(), agent_name: a.name.into(), role: m.role, role_name: role_name.into(), status: "pending".into(), activity: String::new() })
        })
        .collect::<Result<Vec<_>, String>>()?;
    agents::get(&moderator).ok_or("Moderador desconocido")?;
    let ts = now();
    let mut c = Council {
        id: uuid::Uuid::new_v4().to_string(),
        project: project.to_string_lossy().to_string(),
        conversation_id,
        topic: topic.trim().into(),
        output,
        context,
        members,
        moderator,
        phase: "diverge".into(),
        round: 0,
        cards: vec![],
        synthesis: None,
        document_path: None,
        error: None,
        judge: Judge { used_rounds: 0, ..judge },
        verdicts: vec![],
        created_at: ts.clone(),
        updated_at: ts,
    };
    save(&mut c)?;
    Ok(c)
}

fn output_name(output: &str) -> &'static str {
    match output {
        "politica" => "una política",
        "nota" => "una nota de decisión",
        _ => "un ADR (Architecture Decision Record)",
    }
}

fn header(c: &Council, agent: &Agent, role: &str) -> String {
    let (role_name, role_text) = role_info(role);
    format!(
        "Participas en un consejo de varios asistentes de IA (Claude, Codex, Copilot…) que deliberan para decidir sobre un tema y redactar {}. Tú eres {} con el rol «{role_name}»: {role_text}\n\
         Estás en MODO SOLO LECTURA: puedes leer archivos del repositorio para fundamentar tu opinión, pero no modifiques nada.\n\n\
         TEMA: {}\n\nCONTEXTO (conversación previa y notas):\n{}\n",
        output_name(&c.output),
        agent.name,
        c.topic,
        if c.context.trim().is_empty() { "(sin contexto adicional)" } else { c.context.trim() },
    )
}

fn cards_text(cards: &[&Card]) -> String {
    cards
        .iter()
        .map(|k| {
            let reply = k.reply_to.as_deref().map(|r| format!(" → responde a {r}")).unwrap_or_default();
            format!("[{}] {} · {}{reply}: {}", k.id, k.author_name, k.kind, k.text)
        })
        .collect::<Vec<_>>()
        .join("\n")
}

const JSON_RULES: &str = "Responde SOLO con un objeto JSON válido, sin texto antes ni después, con esta forma:";

pub fn diverge_prompt(c: &Council, agent: &Agent, role: &str) -> String {
    format!(
        "{}\nFASE 1 · DIVERGIR. Todavía no has visto las opiniones de los demás. Da tu propuesta independiente: qué decidirías y por qué, y los riesgos o contras principales.\n\n\
         {JSON_RULES}\n{{\"tarjetas\": [{{\"tipo\": \"propuesta\", \"texto\": \"…\"}}, {{\"tipo\": \"riesgo\", \"texto\": \"…\"}}]}}\n\
         Tipos posibles: propuesta, riesgo. Entre 1 y 4 tarjetas, cada una de 1 a 4 frases, concretas.",
        header(c, agent, role)
    )
}

pub fn contrast_prompt(c: &Council, agent: &Agent, role: &str) -> String {
    let others: Vec<&Card> = c.cards.iter().filter(|k| k.author != agent.id).collect();
    let mine: Vec<&Card> = c.cards.iter().filter(|k| k.author == agent.id).collect();
    format!(
        "{}\nFASE 2 · CONTRASTAR (ronda {}). Estas son las aportaciones de los demás (y del usuario, si las hay):\n{}\n\nTus aportaciones anteriores:\n{}\n\n\
         Lee las de los demás y reacciona: apoya lo que te convenza, objeta lo que no, pregunta lo que falte. Puedes cambiar de opinión si te convencen.\n\n\
         {JSON_RULES}\n{{\"tarjetas\": [{{\"tipo\": \"objecion\", \"responde_a\": \"C2\", \"texto\": \"…\"}}]}}\n\
         Tipos posibles: apoyo, objecion, pregunta, comentario. «responde_a» es el id de la tarjeta a la que respondes. Entre 1 y 4 tarjetas, de 1 a 3 frases cada una.",
        header(c, agent, role),
        c.round,
        cards_text(&others),
        if mine.is_empty() { "(ninguna)".into() } else { cards_text(&mine) },
    )
}

pub fn converge_prompt(c: &Council, agent: &Agent) -> String {
    let all: Vec<&Card> = c.cards.iter().collect();
    format!(
        "Eres {} y moderas un consejo de asistentes de IA que ha deliberado sobre un tema. No defiendas una postura propia: resume con justicia y redacta la propuesta final.\n\
         Estás en MODO SOLO LECTURA: no modifiques archivos.\n\nTEMA: {}\n\nCONTEXTO:\n{}\n\nAPORTACIONES DEL CONSEJO:\n{}\n\n\
         Redacta {} en Markdown, en español, con la decisión que mejor recoja los acuerdos. Empieza con una línea «# » seguida del título real de la decisión (no escribas la palabra «Título»). Si es un ADR, después usa estas secciones «## »: Estado (Propuesto), Contexto, Decisión, Alternativas consideradas, Consecuencias, Puntos abiertos.\n\n\
         Responde exactamente con este formato, sin nada más:\n=== ACUERDOS ===\n- …\n=== ABIERTOS ===\n- …\n=== DOCUMENTO ===\n(el documento en Markdown)",
        agent.name,
        c.topic,
        if c.context.trim().is_empty() { "(sin contexto adicional)" } else { c.context.trim() },
        cards_text(&all),
        output_name(&c.output),
    )
}

pub fn judge_prompt(c: &Council, agent: &Agent) -> String {
    let all: Vec<&Card> = c.cards.iter().collect();
    let s = c.synthesis.clone().unwrap_or_default();
    format!(
        "Eres {} y actúas como EVALUADOR INDEPENDIENTE de un consejo de asistentes de IA. No has participado en el debate. Tu trabajo es decidir si lo que el consejo ha convergido es correcto y está bien fundamentado, o si necesita otra ronda.\n\
         Estás en MODO SOLO LECTURA: puedes leer el repositorio para comprobar afirmaciones, pero no modifiques nada.\n\n\
         TEMA: {}\n\nCONTEXTO:\n{}\n\nAPORTACIONES DEL CONSEJO:\n{}\n\nSÍNTESIS DEL MODERADOR\nAcuerdos:\n- {}\nPuntos abiertos:\n- {}\n\nDOCUMENTO PROPUESTO:\n{}\n\n\
         Comprueba: que la decisión responde al tema; que recoge con justicia las objeciones importantes; que no contradice el código o el contexto; que no deja abiertos puntos que impidan aplicarla; y que el documento es coherente. Sé exigente pero no pidas otra ronda por detalles menores.\n\n\
         {JSON_RULES}\n{{\"veredicto\": \"aprobar\", \"confianza\": 80, \"motivos\": [\"…\"], \"preguntas\": [\"…\"]}}\n\
         «veredicto» es «aprobar» u «otra_ronda». «confianza» va de 0 a 100. «motivos»: de 1 a 4 frases que justifican el veredicto. «preguntas»: solo si pides otra ronda, de 1 a 4 objeciones o preguntas concretas que el consejo debe resolver.",
        agent.name,
        c.topic,
        if c.context.trim().is_empty() { "(sin contexto adicional)" } else { c.context.trim() },
        cards_text(&all),
        if s.agreements.is_empty() { "(ninguno)".into() } else { s.agreements.join("\n- ") },
        if s.open.is_empty() { "(ninguno)".into() } else { s.open.join("\n- ") },
        s.document,
    )
}

/// Interpreta la respuesta del evaluador.
pub fn parse_verdict(text: &str) -> (String, Option<u32>, Vec<String>, Vec<String>) {
    let list = |v: &Value, keys: &[&str]| -> Vec<String> {
        keys.iter()
            .find_map(|k| v.get(*k).and_then(Value::as_array))
            .map(|a| a.iter().filter_map(Value::as_str).map(|t| t.trim().to_string()).filter(|t| !t.is_empty()).collect())
            .unwrap_or_default()
    };
    if let Some(v) = extract_json(text) {
        let raw = v.get("veredicto").or_else(|| v.get("verdict")).and_then(Value::as_str).unwrap_or("").to_lowercase();
        let decision = if raw.contains("aprob") || raw.contains("approve") {
            "aprobar"
        } else if raw.contains("ronda") || raw.contains("round") || raw.contains("rechaz") {
            "otra_ronda"
        } else {
            "sin_veredicto"
        };
        let confidence = v.get("confianza").or_else(|| v.get("confidence")).and_then(Value::as_u64).map(|n| n.min(100) as u32);
        return (decision.into(), confidence, list(&v, &["motivos", "reasons"]), list(&v, &["preguntas", "questions"]));
    }
    let t: String = text.trim().chars().take(600).collect();
    ("sin_veredicto".into(), None, if t.is_empty() { vec![] } else { vec![t] }, vec![])
}

fn rotate<T>(mut v: Vec<T>, seed: u64) -> Vec<T> {
    if !v.is_empty() {
        let n = (seed % v.len() as u64) as usize;
        v.rotate_left(n);
    }
    v
}

/// CLIs candidatas para el evaluador, en orden aleatorio: nunca el moderador; primero quienes no
/// participaron y después los participantes. Si la primera falla (sin cupo, sin sesión…), se usa la siguiente.
pub fn judge_candidates(c: &Council, seed: u64) -> Vec<&'static Agent> {
    let not_mod: Vec<&String> = c.judge.pool.iter().filter(|a| **a != c.moderator).collect();
    let (outsiders, members): (Vec<&String>, Vec<&String>) = not_mod.into_iter().partition(|a| !c.members.iter().any(|m| &m.agent == *a));
    let mut order = rotate(outsiders, seed);
    order.extend(rotate(members, seed / 7));
    let mut out: Vec<&'static Agent> = order.into_iter().filter_map(|a| agents::get(a)).collect();
    if out.is_empty() {
        out.extend(agents::get(&c.moderator));
    }
    out
}

/// Saca el JSON de una respuesta aunque venga entre ``` o con texto alrededor.
fn extract_json(text: &str) -> Option<Value> {
    let start = text.find('{')?;
    let end = text.rfind('}')?;
    serde_json::from_str(&text[start..=end]).ok()
}

/// Convierte la respuesta de un agente en tarjetas. Si no es JSON, toda la respuesta es una tarjeta.
pub fn parse_cards(text: &str, default_kind: &str, known: &[String]) -> Vec<(String, Option<String>, String)> {
    let normalize = |k: &str| -> String {
        let k = k.to_lowercase().replace('ó', "o").replace('í', "i");
        match k.as_str() {
            "propuesta" | "riesgo" | "apoyo" | "objecion" | "pregunta" | "comentario" => k,
            _ => default_kind.into(),
        }
    };
    if let Some(v) = extract_json(text) {
        let items = v.get("tarjetas").or_else(|| v.get("cards")).and_then(Value::as_array).cloned().unwrap_or_default();
        let cards: Vec<_> = items
            .iter()
            .filter_map(|i| {
                let texto = i.get("texto").or_else(|| i.get("text")).and_then(Value::as_str)?.trim().to_string();
                if texto.is_empty() {
                    return None;
                }
                let kind = normalize(i.get("tipo").or_else(|| i.get("kind")).and_then(Value::as_str).unwrap_or(default_kind));
                let reply = i
                    .get("responde_a")
                    .or_else(|| i.get("reply_to"))
                    .and_then(Value::as_str)
                    .map(|r| r.trim().trim_matches(['[', ']']).to_uppercase())
                    .filter(|r| known.contains(r));
                Some((kind, reply, texto))
            })
            .collect();
        if !cards.is_empty() {
            return cards;
        }
    }
    let t = text.trim();
    if t.is_empty() { vec![] } else { vec![(default_kind.into(), None, t.into())] }
}

pub fn parse_synthesis(text: &str) -> Synthesis {
    let section = |name: &str| -> Option<String> {
        let marker = format!("=== {name} ===");
        let start = text.find(&marker)? + marker.len();
        let rest = &text[start..];
        let end = rest.find("\n=== ").unwrap_or(rest.len());
        Some(rest[..end].trim().to_string())
    };
    let bullets = |s: Option<String>| -> Vec<String> {
        s.unwrap_or_default()
            .lines()
            .map(|l| l.trim().trim_start_matches(['-', '*', '•']).trim().to_string())
            .filter(|l| !l.is_empty() && l != "…" && l.to_lowercase() != "ninguno")
            .collect()
    };
    match section("DOCUMENTO") {
        Some(doc) => Synthesis { agreements: bullets(section("ACUERDOS")), open: bullets(section("ABIERTOS")), document: doc },
        None => Synthesis { agreements: vec![], open: vec![], document: text.trim().to_string() },
    }
}

/// Añade las tarjetas de un agente al consejo, numerándolas.
pub fn add_cards(c: &mut Council, author: &str, author_name: &str, parsed: Vec<(String, Option<String>, String)>) {
    for (kind, reply_to, text) in parsed {
        let id = format!("C{}", c.cards.len() + 1);
        c.cards.push(Card { id, round: c.round, author: author.into(), author_name: author_name.into(), kind, reply_to, text });
    }
}

/// Texto breve del último paso de un agente, para mostrar en su columna mientras trabaja.
pub fn activity(step: &Step) -> String {
    let d: String = step.detail.lines().next().unwrap_or("").chars().take(80).collect();
    match step.kind.as_str() {
        "command" => format!("$ {d}"),
        "read" => format!("Leyendo {d}"),
        "search" => format!("Buscando {d}"),
        "web" => format!("Consultando {d}"),
        "edit" => format!("Intenta editar {d}"),
        "text" | "thinking" => "Escribiendo…".into(),
        _ => d,
    }
}

/// Guarda el documento aprobado en el repo: docs/adr/0001-tema.md, docs/politicas/… o docs/decisiones/…
pub fn write_document(c: &Council, content: &str) -> Result<String, String> {
    let sub = match c.output.as_str() {
        "politica" => "docs/politicas",
        "nota" => "docs/decisiones",
        _ => "docs/adr",
    };
    let dir = Path::new(&c.project).join(sub);
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let next = fs::read_dir(&dir)
        .map(|rd| {
            rd.filter_map(|e| e.ok())
                .filter_map(|e| e.file_name().to_str()?.split('-').next()?.parse::<u32>().ok())
                .max()
                .unwrap_or(0)
        })
        .unwrap_or(0)
        + 1;
    let slug: String = c
        .topic
        .to_lowercase()
        .chars()
        .map(|ch| match ch {
            'á' | 'à' | 'ä' => 'a',
            'é' | 'è' | 'ë' => 'e',
            'í' | 'ì' | 'ï' => 'i',
            'ó' | 'ò' | 'ö' => 'o',
            'ú' | 'ù' | 'ü' => 'u',
            'ñ' => 'n',
            c if c.is_ascii_alphanumeric() => c,
            _ => '-',
        })
        .collect::<String>()
        .split('-')
        .filter(|s| !s.is_empty())
        .take(8)
        .collect::<Vec<_>>()
        .join("-");
    let rel = format!("{sub}/{next:04}-{}.md", if slug.is_empty() { "decision" } else { &slug });
    let mut text = content.trim().to_string();
    text.push('\n');
    fs::write(Path::new(&c.project).join(&rel), text).map_err(|e| e.to_string())?;
    Ok(rel)
}

// ---------------------------------------------------------------------------------------------
// Ejecución: las fases se ejecutan en segundo plano y la ventana recibe cada cambio
// («council-update» con el consejo entero, «council-activity» con lo que hace cada agente).

use std::collections::HashMap;
use std::sync::{Arc, Mutex as StdMutex};
use tauri::{AppHandle, Emitter};
use tokio::sync::{oneshot, Mutex};

/// Consejos en marcha: id → avisos de cancelación de los agentes que están trabajando.
#[derive(Default)]
pub struct Runs(pub StdMutex<HashMap<String, Vec<oneshot::Sender<()>>>>);

impl Runs {
    fn begin(&self, id: &str) -> Result<(), String> {
        let mut map = self.0.lock().unwrap();
        if map.contains_key(id) {
            return Err("Este consejo ya está trabajando".into());
        }
        map.insert(id.into(), vec![]);
        Ok(())
    }
    /// Receptor de cancelación para un agente; None si el consejo ya se detuvo.
    fn cancel_rx(&self, id: &str) -> Option<oneshot::Receiver<()>> {
        let (tx, rx) = oneshot::channel();
        self.0.lock().unwrap().get_mut(id)?.push(tx);
        Some(rx)
    }
    fn is_running(&self, id: &str) -> bool {
        self.0.lock().unwrap().contains_key(id)
    }
    fn end(&self, id: &str) {
        self.0.lock().unwrap().remove(id);
    }
    pub fn cancel(&self, id: &str) {
        if let Some(txs) = self.0.lock().unwrap().remove(id) {
            for tx in txs {
                let _ = tx.send(());
            }
        }
    }
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct ActivityEvent<'a> {
    council_id: &'a str,
    agent: &'a str,
    activity: String,
}

fn emit(app: &AppHandle, c: &Council) {
    let _ = app.emit("council-update", c);
}

async fn set_phase(app: &AppHandle, shared: &Mutex<Council>, phase: &str) {
    let mut c = shared.lock().await;
    c.phase = phase.into();
    let _ = save(&mut c);
    emit(app, &c);
}

/// Ejecuta una fase de agentes en paralelo (divergir o contrastar).
async fn member_phase(app: &AppHandle, runs: &Runs, shared: Arc<Mutex<Council>>, contrast: bool) -> usize {
    let (cwd, members) = {
        let mut c = shared.lock().await;
        for m in &mut c.members {
            m.status = "running".into();
            m.activity = "Pensando…".into();
        }
        let _ = save(&mut c);
        emit(app, &c);
        (PathBuf::from(&c.project), c.members.clone())
    };
    let tasks = members.into_iter().map(|m| {
        let shared = shared.clone();
        let cwd = cwd.clone();
        async move {
            let Some(agent) = agents::get(&m.agent) else { return false };
            let (prompt, council_id) = {
                let c = shared.lock().await;
                let p = if contrast { contrast_prompt(&c, agent, &m.role) } else { diverge_prompt(&c, agent, &m.role) };
                (p, c.id.clone())
            };
            let Some(cancel) = runs.cancel_rx(&council_id) else { return false };
            let on_step = |step: &Step| {
                let _ = app.emit("council-activity", ActivityEvent { council_id: &council_id, agent: agent.id, activity: activity(step) });
            };
            let result = agents::run_readonly(agent, &prompt, &cwd, on_step, cancel).await;
            let mut c = shared.lock().await;
            let ok = match result {
                Ok(r) => {
                    let known: Vec<String> = c.cards.iter().map(|k| k.id.clone()).collect();
                    let parsed = parse_cards(&r.text, if contrast { "comentario" } else { "propuesta" }, &known);
                    add_cards(&mut c, agent.id, agent.name, parsed);
                    true
                }
                Err(e) => {
                    if let Some(mm) = c.members.iter_mut().find(|x| x.agent == agent.id) {
                        mm.activity = e.lines().take(3).collect::<Vec<_>>().join(" ").chars().take(240).collect();
                    }
                    false
                }
            };
            if let Some(mm) = c.members.iter_mut().find(|x| x.agent == agent.id) {
                mm.status = if ok { "done".into() } else { "error".into() };
                if ok {
                    mm.activity.clear();
                }
            }
            let _ = save(&mut c);
            emit(app, &c);
            ok
        }
    });
    futures::future::join_all(tasks).await.into_iter().filter(|ok| *ok).count()
}

async fn converge(app: &AppHandle, runs: &Runs, shared: Arc<Mutex<Council>>) -> Result<(), String> {
    set_phase(app, &shared, "converge").await;
    let (prompt, cwd, moderator, id) = {
        let c = shared.lock().await;
        let agent = agents::get(&c.moderator).ok_or("Moderador desconocido")?;
        (converge_prompt(&c, agent), PathBuf::from(&c.project), agent, c.id.clone())
    };
    let cancel = runs.cancel_rx(&id).ok_or("Detenido.")?;
    let on_step = |step: &Step| {
        let _ = app.emit("council-activity", ActivityEvent { council_id: &id, agent: "moderator", activity: activity(step) });
    };
    let r = agents::run_readonly(moderator, &prompt, &cwd, on_step, cancel).await?;
    let mut c = shared.lock().await;
    c.synthesis = Some(parse_synthesis(&r.text));
    c.phase = "review".into();
    c.error = None;
    let _ = save(&mut c);
    emit(app, &c);
    Ok(())
}

fn random_seed() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos() as u64).unwrap_or(0)
}

/// El evaluador juzga la síntesis. Devuelve true si pide otra ronda (y aún quedan).
async fn judge(app: &AppHandle, runs: &Runs, shared: Arc<Mutex<Council>>) -> Result<bool, String> {
    set_phase(app, &shared, "judge").await;
    let (candidates, cwd, id) = {
        let c = shared.lock().await;
        (judge_candidates(&c, random_seed()), PathBuf::from(&c.project), c.id.clone())
    };
    let mut failures: Vec<String> = vec![];
    let mut answer = None;
    for agent in candidates {
        let prompt = {
            let mut c = shared.lock().await;
            let round = c.round;
            // Se anota ya quién evalúa, para que el tablero lo muestre mientras trabaja.
            if c.verdicts.last().is_some_and(|v| v.decision == "evaluando") {
                c.verdicts.pop();
            }
            c.verdicts.push(Verdict { round, agent: agent.id.into(), agent_name: agent.name.into(), decision: "evaluando".into(), confidence: None, reasons: vec![], questions: vec![], capped: false });
            let _ = save(&mut c);
            emit(app, &c);
            judge_prompt(&c, agent)
        };
        let cancel = runs.cancel_rx(&id).ok_or("Detenido.")?;
        let on_step = |step: &Step| {
            let _ = app.emit("council-activity", ActivityEvent { council_id: &id, agent: "judge", activity: activity(step) });
        };
        match agents::run_readonly(agent, &prompt, &cwd, on_step, cancel).await {
            Ok(r) => {
                answer = Some((agent, r.text));
                break;
            }
            Err(e) if !runs.is_running(&id) => return Err(e),
            Err(e) => {
                let low = e.to_lowercase();
                let why = if low.contains("resource_exhausted") || low.contains("quota") || low.contains("rate limit") || low.contains("429") {
                    "sin cupo".to_string()
                } else if low.contains("login") || low.contains("auth") || low.contains("ineligible") {
                    "sin sesión válida".to_string()
                } else if low.contains("no está instalado") {
                    "no instalado".to_string()
                } else {
                    "falló".to_string()
                };
                failures.push(format!("{}: {why}", agent.name));
            }
        }
    }
    let mut c = shared.lock().await;
    let Some((agent, text)) = answer else {
        c.verdicts.pop();
        return Err(format!("Ninguna CLI pudo hacer de evaluador. {}", failures.join(" · ")));
    };
    let (decision, confidence, mut reasons, questions) = parse_verdict(&text);
    if !failures.is_empty() {
        reasons.push(format!("(Antes se intentó con otras CLIs: {})", failures.join(" · ")));
    }
    let wants_round = decision == "otra_ronda";
    let can_round = c.judge.used_rounds < c.judge.max_rounds;
    let round = c.round;
    if let Some(v) = c.verdicts.last_mut() {
        *v = Verdict { round, agent: agent.id.into(), agent_name: agent.name.into(), decision, confidence, reasons, questions: questions.clone(), capped: wants_round && !can_round };
    }
    let again = wants_round && can_round;
    if again {
        c.judge.used_rounds += 1;
        // Sus objeciones van al tablero para que el consejo las responda en la siguiente ronda.
        let name = format!("Evaluador · {}", agent.name);
        let parsed = questions.into_iter().map(|q| ("pregunta".to_string(), None, q)).collect();
        add_cards(&mut c, "judge", &name, parsed);
    } else {
        c.phase = "review".into();
    }
    let _ = save(&mut c);
    emit(app, &c);
    Ok(again)
}

async fn fail(app: &AppHandle, shared: &Mutex<Council>, cancelled: bool, msg: String) {
    let mut c = shared.lock().await;
    c.phase = if cancelled { "cancelled".into() } else { "error".into() };
    c.error = Some(msg);
    if c.verdicts.last().is_some_and(|v| v.decision == "evaluando") {
        c.verdicts.pop();
    }
    for m in &mut c.members {
        if m.status == "running" {
            m.status = "pending".into();
            m.activity.clear();
        }
    }
    let _ = save(&mut c);
    emit(app, &c);
}

/// Qué hacer: el consejo completo (divergir + contrastar + converger), otra ronda, o solo la síntesis.
#[derive(Clone, Copy, PartialEq)]
pub enum Plan {
    Full,
    AnotherRound,
    SynthesisOnly,
}

pub fn launch(app: AppHandle, runs: Arc<Runs>, council: Council, plan: Plan) -> Result<(), String> {
    runs.begin(&council.id)?;
    let id = council.id.clone();
    let shared = Arc::new(Mutex::new(council));
    tauri::async_runtime::spawn(async move {
        let outcome: Result<(), String> = async {
            if plan == Plan::Full {
                set_phase(&app, &shared, "diverge").await;
                if member_phase(&app, &runs, shared.clone(), false).await == 0 {
                    return Err("Ningún agente pudo dar su opinión. Revisa en la columna de cada uno qué ha fallado.".into());
                }
            }
            if plan != Plan::SynthesisOnly {
                {
                    let mut c = shared.lock().await;
                    c.round += 1;
                }
                set_phase(&app, &shared, "contrast").await;
                if !runs.is_running(&id) {
                    return Err("Detenido.".into());
                }
                member_phase(&app, &runs, shared.clone(), true).await;
            }
            if !runs.is_running(&id) {
                return Err("Detenido.".into());
            }
            converge(&app, &runs, shared.clone()).await?;
            // Evaluador: si no le convence, otra ronda de contraste y nueva síntesis, hasta su tope.
            loop {
                let enabled = shared.lock().await.judge.enabled;
                if !enabled || !runs.is_running(&id) {
                    break;
                }
                if !judge(&app, &runs, shared.clone()).await? {
                    break;
                }
                {
                    let mut c = shared.lock().await;
                    c.round += 1;
                }
                set_phase(&app, &shared, "contrast").await;
                member_phase(&app, &runs, shared.clone(), true).await;
                if !runs.is_running(&id) {
                    return Err("Detenido.".into());
                }
                converge(&app, &runs, shared.clone()).await?;
            }
            Ok(())
        }
        .await;
        let cancelled = !runs.is_running(&id);
        runs.end(&id);
        if let Err(e) = outcome {
            fail(&app, &shared, cancelled, e).await;
        }
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tarjetas_desde_json_con_texto_alrededor() {
        let known = vec!["C1".to_string(), "C2".to_string()];
        let t = "Aquí va:\n```json\n{\"tarjetas\":[{\"tipo\":\"Objeción\",\"responde_a\":\"[c2]\",\"texto\":\"No escala.\"},{\"tipo\":\"apoyo\",\"responde_a\":\"C9\",\"texto\":\"Bien.\"}]}\n```";
        let cards = parse_cards(t, "comentario", &known);
        assert_eq!(cards[0], ("objecion".into(), Some("C2".into()), "No escala.".into()));
        assert_eq!(cards[1].1, None, "una referencia inexistente se descarta");
    }

    #[test]
    fn tarjetas_sin_json() {
        let cards = parse_cards("Propongo usar /v1 en la URL.", "propuesta", &[]);
        assert_eq!(cards, vec![("propuesta".into(), None, "Propongo usar /v1 en la URL.".into())]);
    }

    #[test]
    fn sintesis_por_secciones() {
        let s = parse_synthesis("=== ACUERDOS ===\n- Versión en la URL\n=== ABIERTOS ===\n- ¿Dos o tres versiones?\n=== DOCUMENTO ===\n# ADR 1\nTexto");
        assert_eq!(s.agreements, vec!["Versión en la URL"]);
        assert_eq!(s.open, vec!["¿Dos o tres versiones?"]);
        assert_eq!(s.document, "# ADR 1\nTexto");
    }

    #[test]
    fn veredicto_del_evaluador() {
        let (d, conf, motivos, preg) = parse_verdict("```json\n{\"veredicto\":\"otra_ronda\",\"confianza\":40,\"motivos\":[\"Falta X\"],\"preguntas\":[\"¿Y si Y?\"]}\n```");
        assert_eq!((d.as_str(), conf, motivos, preg), ("otra_ronda", Some(40), vec!["Falta X".to_string()], vec!["¿Y si Y?".to_string()]));
        assert_eq!(parse_verdict("{\"veredicto\":\"Aprobar\"}").0, "aprobar");
        assert_eq!(parse_verdict("no sé").0, "sin_veredicto");
    }

    #[test]
    fn evaluador_al_azar_y_nunca_el_moderador() {
        let member = |a: &str| Member { agent: a.into(), agent_name: a.into(), role: "libre".into(), role_name: "Libre".into(), status: String::new(), activity: String::new() };
        let mut c = Council {
            id: "x".into(), project: String::new(), conversation_id: None, topic: String::new(), output: "adr".into(), context: String::new(),
            members: vec![member("copilot"), member("opencode")], moderator: "codex".into(), phase: "review".into(), round: 1, cards: vec![],
            synthesis: None, document_path: None, error: None,
            judge: Judge { enabled: true, pool: vec!["claude".into(), "codex".into(), "copilot".into(), "opencode".into(), "agy".into()], max_rounds: 2, used_rounds: 0 },
            verdicts: vec![], created_at: String::new(), updated_at: String::new(),
        };
        let firsts: std::collections::HashSet<&str> = (0..20u64).map(|i| judge_candidates(&c, i)[0].id).collect();
        assert_eq!(firsts, ["claude", "agy"].into_iter().collect(), "primero quien no participó ni modera, y varía");
        let all: Vec<&str> = judge_candidates(&c, 3).iter().map(|a| a.id).collect();
        assert_eq!(all.len(), 4, "todos menos el moderador, como reserva");
        assert!(!all.contains(&"codex"));
        c.judge.pool = vec!["codex".into(), "copilot".into()];
        assert_eq!(judge_candidates(&c, 0)[0].id, "copilot", "si solo quedan participantes, uno de ellos, nunca el moderador");
    }

    #[test]
    fn documento_numerado_en_docs_adr() {
        let root = std::env::temp_dir().join(format!("consejo-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("docs/adr")).unwrap();
        fs::write(root.join("docs/adr/0003-otra.md"), "x").unwrap();
        let c = Council {
            id: "x".into(),
            project: root.to_string_lossy().into(),
            conversation_id: None,
            topic: "Política de versionado de la API".into(),
            output: "adr".into(),
            context: String::new(),
            members: vec![],
            moderator: "claude".into(),
            phase: "review".into(),
            round: 1,
            cards: vec![],
            synthesis: None,
            document_path: None,
            error: None,
            judge: Judge::default(),
            verdicts: vec![],
            created_at: String::new(),
            updated_at: String::new(),
        };
        let rel = write_document(&c, "# ADR").unwrap();
        assert_eq!(rel, "docs/adr/0004-politica-de-versionado-de-la-api.md");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn pizarra_guardar_y_leer() {
        let base = std::env::temp_dir().join(format!("pizarra-test-{}-guardar_y_leer", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        let board = serde_json::json!({"type": "excalidraw", "elements": [{"id": "e1"}]});
        write_board(&base, "00000000-0000-0000-0000-000000000000", &board).unwrap();
        assert_eq!(read_board(&base, "00000000-0000-0000-0000-000000000000").unwrap(), board);
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn pizarra_leer_id_inexistente() {
        let base = std::env::temp_dir().join(format!("pizarra-test-{}-inexistente", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        assert_eq!(read_board(&base, "00000000-0000-0000-0000-000000000000"), None);
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn pizarra_id_invalido() {
        let base = std::env::temp_dir().join(format!("pizarra-test-{}-id_invalido", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        assert_eq!(write_board(&base, "../x", &serde_json::json!({"a": 1})).unwrap_err(), "Consejo no válido");
        assert_eq!(read_board(&base, "../x"), None);
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn pizarra_debe_ser_objeto() {
        let base = std::env::temp_dir().join(format!("pizarra-test-{}-no_objeto", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        assert_eq!(
            write_board(&base, "00000000-0000-0000-0000-000000000000", &serde_json::json!([1, 2])).unwrap_err(),
            "La pizarra debe ser un objeto JSON"
        );
        let _ = fs::remove_dir_all(&base);
    }
}

/// Consejo real con CLIs instaladas, sin ventana:
/// `AGENTES_COUNCIL_DIR=<repo> cargo test consejo_real -- --ignored --nocapture`
#[cfg(test)]
#[tokio::test(flavor = "multi_thread")]
#[ignore]
async fn consejo_real() {
    let dir = PathBuf::from(std::env::var("AGENTES_COUNCIL_DIR").expect("define AGENTES_COUNCIL_DIR"));
    std::env::set_var("CHAT_DATA_DIR", std::env::temp_dir().join("consejo-real-data"));
    let before = std::process::Command::new("git").args(["status", "--porcelain"]).current_dir(&dir).output().unwrap().stdout;
    let mut c = create(
        &dir,
        None,
        "¿El programa de src/main.rs debería leer el nombre a saludar desde los argumentos de línea de comandos o desde una variable de entorno?".into(),
        "adr".into(),
        String::new(),
        vec![NewMember { agent: "copilot".into(), role: "arquitecto".into() }, NewMember { agent: "opencode".into(), role: "abogado".into() }],
        "codex".into(),
        Judge::default(),
    )
    .unwrap();
    let never = || tokio::sync::oneshot::channel::<()>().1;
    for contrast in [false, true] {
        if contrast {
            c.round = 1;
        }
        let members = c.members.clone();
        for m in members {
            let agent = agents::get(&m.agent).unwrap();
            let prompt = if contrast { contrast_prompt(&c, agent, &m.role) } else { diverge_prompt(&c, agent, &m.role) };
            let r = agents::run_readonly(agent, &prompt, &dir, |s| println!("    [{}] {}", agent.id, activity(s)), never()).await;
            match r {
                Ok(r) => {
                    let known: Vec<String> = c.cards.iter().map(|k| k.id.clone()).collect();
                    let parsed = parse_cards(&r.text, if contrast { "comentario" } else { "propuesta" }, &known);
                    println!("  {} → {} tarjetas (¿JSON? {})", agent.id, parsed.len(), extract_json(&r.text).is_some());
                    add_cards(&mut c, agent.id, agent.name, parsed);
                }
                Err(e) => println!("  {} ERROR: {e}", agent.id),
            }
        }
    }
    for k in &c.cards {
        println!("[{}] r{} {} · {}{}: {}", k.id, k.round, k.author_name, k.kind, k.reply_to.as_deref().map(|r| format!(" → {r}")).unwrap_or_default(), k.text);
    }
    let moderator = agents::get("codex").unwrap();
    let r = agents::run_readonly(moderator, &converge_prompt(&c, moderator), &dir, |_| {}, never()).await.unwrap();
    let s = parse_synthesis(&r.text);
    println!("ACUERDOS: {:?}\nABIERTOS: {:?}\nDOCUMENTO:\n{}", s.agreements, s.open, s.document.chars().take(1200).collect::<String>());
    let after = std::process::Command::new("git").args(["status", "--porcelain"]).current_dir(&dir).output().unwrap().stdout;
    assert_eq!(before, after, "en modo solo lectura no debe cambiar ningún archivo");
    assert!(!s.document.is_empty());
}
