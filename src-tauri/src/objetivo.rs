//! Objetivo de la sesión (IDE-29): un objetivo y una checklist por conversación del chat, en
//! `data_dir()/objetivos/<conversación>.json`. Lo editan el usuario (pestaña Objetivo) y los agentes (`agentes tareas`).

use crate::store::{data_dir, now, write_atomic};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;

/// Leer, cambiar y guardar va de una vez: la pestaña y varios agentes pueden tocar la misma checklist.
static LOCK: Mutex<()> = Mutex::new(());

/// Un punto de la checklist. `ticket`: clave (IDE-23) o URL de un ticket, opcional.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Item {
    pub id: u32,
    pub text: String,
    #[serde(default)]
    pub done: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ticket: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Objetivo {
    pub conversation: String,
    #[serde(default)]
    pub goal: String,
    #[serde(default)]
    pub items: Vec<Item>,
    #[serde(default)]
    pub updated_at: String,
}

impl Objetivo {
    /// Añade un punto a la checklist y devuelve su `id` (el mayor actual + 1).
    pub fn add(&mut self, text: &str, ticket: Option<&str>) -> Result<u32, String> {
        let text = text.trim();
        if text.is_empty() {
            return Err("Escribe el texto del punto".into());
        }
        if self.items.len() >= 200 {
            return Err("Demasiados puntos".into());
        }
        let id = self.items.iter().map(|i| i.id).max().unwrap_or(0) + 1;
        let ticket = ticket.map(|t| t.trim().to_string()).filter(|t| !t.is_empty());
        self.items.push(Item { id, text: text.into(), done: false, ticket });
        Ok(id)
    }

    /// Marca o desmarca un punto por `id`; error si no existe.
    pub fn set_done(&mut self, id: u32, done: bool) -> Result<(), String> {
        let item = self.items.iter_mut().find(|i| i.id == id).ok_or_else(|| format!("No hay ningún punto {id}"))?;
        item.done = done;
        Ok(())
    }

    /// Quita el punto con ese `id`; error si no existe.
    pub fn remove(&mut self, id: u32) -> Result<(), String> {
        let pos = match self.items.iter().position(|i| i.id == id) {
            Some(pos) => pos,
            None => return Err(format!("No hay ningún punto {id}")),
        };
        self.items.remove(pos);
        Ok(())
    }

    /// Guarda el objetivo recortado (sin espacios en los extremos).
    pub fn set_goal(&mut self, goal: &str) {
        self.goal = goal.trim().to_string();
    }

    /// Texto plano con el objetivo y la checklist, tal como lo leen los agentes.
    pub fn describe(&self) -> String {
        let mut lineas: Vec<String> = Vec::new();
        if self.goal.is_empty() {
            lineas.push("Sin objetivo".into());
        } else {
            lineas.push(format!("Objetivo: {}", self.goal));
        }
        if self.items.is_empty() {
            lineas.push("(sin puntos)".into());
        } else {
            for item in &self.items {
                let marca = if item.done { "[x]" } else { "[ ]" };
                let ticket = match &item.ticket {
                    Some(ticket) => format!(" ({ticket})"),
                    None => String::new(),
                };
                lineas.push(format!("{} {}. {}{}", marca, item.id, item.text, ticket));
            }
        }
        let hechos = self.items.iter().filter(|i| i.done).count();
        lineas.push(format!("Hechos: {} de {}", hechos, self.items.len()));
        lineas.join("\n")
    }
}

/// Carpeta `data_dir()/objetivos`; la crea si no existe.
fn dir() -> PathBuf {
    let dir = data_dir().join("objetivos");
    let _ = fs::create_dir_all(&dir);
    dir
}

/// Solo acepta conversaciones con uuid válido (evita rutas raras).
fn valid(conversation: &str) -> bool {
    uuid::Uuid::parse_str(conversation).is_ok()
}

/// Carga el objetivo de esa conversación (uno nuevo si aún no tiene archivo).
pub fn load(conversation: &str) -> Result<Objetivo, String> {
    if !valid(conversation) {
        return Err("Conversación no válida".into());
    }
    let path = dir().join(format!("{conversation}.json"));
    match fs::read_to_string(&path).ok().and_then(|json| serde_json::from_str::<Objetivo>(&json).ok()) {
        Some(mut o) => {
            o.conversation = conversation.into();
            Ok(o)
        }
        None => Ok(Objetivo { conversation: conversation.into(), ..Default::default() }),
    }
}

/// Guarda el objetivo en el disco (valida la conversación y sella `updatedAt`).
pub fn store(o: &mut Objetivo) -> Result<(), String> {
    if !valid(&o.conversation) {
        return Err("Conversación no válida".into());
    }
    o.updated_at = now();
    let json = serde_json::to_string_pretty(o).map_err(|e| e.to_string())?;
    write_atomic(&dir().join(format!("{}.json", o.conversation)), &json)
}

/// Aplica una operación (list | goal | add | done | undone | remove) sobre lo guardado y lo guarda.
/// La usan la pestaña Objetivo y `agentes tareas`: cada una manda solo su cambio, así no se pisan.
pub fn apply(conversation: &str, op: &str, text: &str, ticket: Option<&str>, id: u32) -> Result<Objetivo, String> {
    let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut o = load(conversation)?;
    if apply_to(&mut o, op, text, ticket, id)? {
        store(&mut o)?;
    }
    Ok(o)
}

/// La operación sobre el objetivo en memoria. Devuelve si lo ha cambiado.
fn apply_to(o: &mut Objetivo, op: &str, text: &str, ticket: Option<&str>, id: u32) -> Result<bool, String> {
    match op {
        "list" => return Ok(false),
        "goal" => o.set_goal(text),
        "add" => o.add(text, ticket).map(|_| ())?,
        "done" => o.set_done(id, true)?,
        "undone" => o.set_done(id, false)?,
        "remove" => o.remove(id)?,
        other => return Err(format!("Operación desconocida: {other}")),
    }
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_asigna_ids_y_recorta() {
        let mut o = Objetivo::default();
        assert_eq!(o.add("Primero", None).unwrap(), 1);
        assert_eq!(o.add("Segundo", Some("  ")).unwrap(), 2);
        assert_eq!(o.add("Tercero", Some(" IDE-23 ")).unwrap(), 3);
        assert_eq!(o.items[1].ticket, None);
        assert_eq!(o.items[2].ticket, Some("IDE-23".into()));
        assert!(o.remove(2).is_ok());
        assert_eq!(o.add("Cuarto", None).unwrap(), 4);
        assert!(o.add("  ", None).is_err());
    }

    #[test]
    fn ids_inexistentes_dan_error() {
        let mut o = Objetivo::default();
        assert!(o.add("Un punto", None).is_ok());
        assert_eq!(o.set_done(9, true).unwrap_err(), "No hay ningún punto 9");
        assert_eq!(o.remove(9).unwrap_err(), "No hay ningún punto 9");
    }

    /// Pestaña y agente mandan cada uno su operación: los cambios de los dos se conservan.
    #[test]
    fn las_operaciones_no_se_pisan() {
        let mut o = Objetivo::default();
        assert!(apply_to(&mut o, "add", "Paso del agente", None, 0).unwrap());
        assert!(apply_to(&mut o, "add", "Paso del usuario", Some("IDE-29"), 0).unwrap());
        assert!(apply_to(&mut o, "done", "", None, 1).unwrap());
        assert!(apply_to(&mut o, "goal", "  Terminar IDE-29 ", None, 0).unwrap());
        assert!(!apply_to(&mut o, "list", "", None, 0).unwrap());
        assert_eq!(o.goal, "Terminar IDE-29");
        assert_eq!(o.items.len(), 2);
        assert!(o.items[0].done && !o.items[1].done);
        assert!(apply_to(&mut o, "undone", "", None, 1).unwrap());
        assert!(apply_to(&mut o, "remove", "", None, 2).unwrap());
        assert_eq!(o.items.len(), 1);
        assert!(apply_to(&mut o, "volar", "", None, 0).is_err());
        assert!(apply_to(&mut o, "done", "", None, 7).is_err());
    }

    #[test]
    fn describe_resume_todo() {
        let mut o = Objetivo::default();
        assert!(o.describe().contains("Sin objetivo"));
        assert!(o.describe().contains("(sin puntos)"));

        o.set_goal("Doc MCP");
        assert!(o.add("Hacer algo", Some("IDE-23")).is_ok());
        assert!(o.set_done(1, true).is_ok());
        assert!(o.add("Otro punto", None).is_ok());
        let d = o.describe();
        assert!(d.contains("Objetivo: Doc MCP"));
        assert!(d.contains("[x] 1. "));
        assert!(d.contains("(IDE-23)"));
        assert!(d.contains("[ ] 2. "));
        assert!(d.contains("Hechos: 1 de 2"));
    }
}
