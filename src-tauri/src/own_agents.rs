//! Agentes propios: los define el usuario en archivos `<id>.md` editables. Cada archivo es una
//! cabecera `---` … `---` con `clave: valor` por línea, seguida de las instrucciones (texto libre).
//! Se buscan en la carpeta de datos de la app (`data_dir()/agents/`) y en el proyecto
//! (`<proyecto>/.ide-agents/`); si el mismo id está en los dos, gana el del proyecto.
//!   `permisos`: `lectura` o `escritura` (si puede editar archivos).
//!   `cuando`: `manual`, `tras-sintesis`, `antes-de-commit` o `al-guardar` (TRIGGERS).
//!   `clis`: ids de agentes CLI separados por comas (vacío = cualquiera disponible).
//!   `seleccion`: `fija` (se usa la primera CLI de la lista) o `azar` (una al azar y, si falla, la siguiente).
//!   `activo`: `si` o `no`.

use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

/// Agente propio, tal como se describe en un archivo `<id>.md` (sin el `source`, que no se escribe).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct OwnAgent {
    /// Identificador: solo minúsculas, números y guiones. Es también el nombre del archivo.
    pub id: String,
    /// Nombre que se muestra al usuario.
    pub name: String,
    /// `escritura` permite editar archivos; `lectura` solo puede leerlos.
    pub write: bool,
    /// Cuándo se invoca: uno de los valores de TRIGGERS.
    pub trigger: String,
    /// CLIs que pueden ejecutarlo (vacío = cualquiera disponible).
    pub clis: Vec<String>,
    /// `azar` elige una CLI al azar; `fija` usa la primera de la lista.
    pub random: bool,
    /// `no` = desactivado, se ignora aunque exista el archivo.
    pub enabled: bool,
    /// Instrucciones del agente (el texto libre tras la cabecera).
    pub instructions: String,
    /// Carpeta de la que se leyó: "app" o "proyecto" (no se escribe en el archivo).
    #[serde(default)]
    pub source: String,
}

/// Valores válidos para `cuando`.
pub const TRIGGERS: &[&str] = &["manual", "tras-sintesis", "antes-de-commit", "al-guardar"];

/// Evaluador predefinido de la app: juzga con ojos nuevos lo que el consejo ha convergido.
pub fn default_evaluator() -> OwnAgent {
    OwnAgent {
        id: "evaluador".into(),
        name: "Evaluador independiente".into(),
        write: false,
        trigger: "tras-sintesis".into(),
        clis: vec![],
        random: true,
        enabled: true,
        instructions: "Actúas como EVALUADOR INDEPENDIENTE de un consejo de asistentes de IA. No has participado en el debate. Tu trabajo es decidir si lo que el consejo ha convergido es correcto y está bien fundamentado, o si necesita otra ronda.\n\nComprueba: que la decisión responde al tema; que recoge con justicia las objeciones importantes; que no contradice el código o el contexto; que no deja abiertos puntos que impidan aplicarla; y que el documento es coherente. Sé exigente pero no pidas otra ronda por detalles menores.".into(),
        source: String::new(),
    }
}

/// Lee y valida la definición de un agente propio (cabecera + instrucciones).
pub fn parse(text: &str) -> Result<OwnAgent, String> {
    // Quita el BOM y normaliza finales \r\n.
    let text = text.strip_prefix('\u{feff}').unwrap_or(text).replace("\r\n", "\n");
    let lines: Vec<&str> = text.split('\n').collect();
    if !lines.first().map_or(false, |l| l.trim() == "---") {
        return Err("Falta la cabecera (---) al principio".into());
    }
    let Some(sep) = lines.iter().skip(1).position(|l| l.trim() == "---") else {
        return Err("La cabecera no se cierra con ---".into());
    };
    let sep = sep + 1; // índice de la segunda línea ---
    let mut id = String::new();
    let mut name = String::new();
    let mut write = false;
    let mut trigger = "manual".into();
    let mut clis: Vec<String> = vec![];
    let mut random = false;
    let mut enabled = true;
    for line in &lines[1..sep] {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once(':') else { continue };
        let (key, value) = (key.trim(), value.trim());
        match key {
            "id" => id = value.into(),
            "nombre" => name = value.into(),
            "permisos" => match value {
                "lectura" => write = false,
                "escritura" => write = true,
                other => return Err(format!("Permisos no válidos: {other}")),
            },
            "cuando" => {
                if TRIGGERS.contains(&value) {
                    trigger = value.into();
                } else {
                    return Err(format!("«cuando» no válido: {value}"));
                }
            }
            "clis" => {
                clis = value.split(',').map(|c| c.trim().to_string()).filter(|c| !c.is_empty()).collect();
            }
            "seleccion" => match value {
                "fija" => random = false,
                "azar" => random = true,
                other => return Err(format!("Selección no válida: {other}")),
            },
            "activo" => match value {
                "si" => enabled = true,
                "no" => enabled = false,
                _ => return Err("«activo» debe ser si o no".into()),
            },
            _ => {} // claves desconocidas se ignoran
        }
    }
    let valid = !id.is_empty()
        && id.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
    if !valid {
        return Err("El id solo puede tener minúsculas, números y guiones".into());
    }
    if name.is_empty() {
        return Err("Falta el nombre".into());
    }
    let instructions = lines[sep + 1..].join("\n").trim().to_string();
    if instructions.is_empty() {
        return Err("Faltan las instrucciones".into());
    }
    Ok(OwnAgent { id, name, write, trigger, clis, random, enabled, instructions, source: String::new() })
}

/// Escribe la definición en el formato del archivo `<id>.md`.
pub fn to_markdown(a: &OwnAgent) -> String {
    format!(
        "---\nid: {}\nnombre: {}\npermisos: {}\ncuando: {}\nclis: {}\nseleccion: {}\nactivo: {}\n---\n{}\n",
        a.id,
        a.name,
        if a.write { "escritura" } else { "lectura" },
        a.trigger,
        a.clis.join(", "),
        if a.random { "azar" } else { "fija" },
        if a.enabled { "si" } else { "no" },
        a.instructions
    )
}

/// Lee los archivos `*.md` de una carpeta (ignora los que no validen, avisando por stderr).
fn load_dir(dir: &Path, source: &str) -> Vec<OwnAgent> {
    let Ok(rd) = fs::read_dir(dir) else { return vec![] };
    let mut out: Vec<OwnAgent> = vec![];
    for e in rd.filter_map(|e| e.ok()) {
        if !e.file_name().to_str().map_or(false, |n| n.ends_with(".md")) {
            continue;
        }
        let Ok(text) = fs::read_to_string(e.path()) else { continue };
        match parse(&text) {
            Ok(mut a) => {
                a.source = source.into();
                out.push(a);
            }
            Err(err) => eprintln!("own_agents {}: {err}", e.path().display()),
        }
    }
    out.sort_by(|a, b| a.id.cmp(&b.id));
    out
}

/// Carpeta de la app donde viven los agentes propios.
pub fn app_dir() -> PathBuf {
    crate::store::data_dir().join("agents")
}

/// Crea la carpeta si falta y, si no existe `evaluador.md`, lo escribe con el evaluador de fábrica.
fn ensure_defaults(dir: &Path) {
    let _ = fs::create_dir_all(dir);
    let file = dir.join("evaluador.md");
    if !file.exists() {
        let _ = fs::write(&file, to_markdown(&default_evaluator()));
    }
}

/// Carga los agentes de la app y del proyecto; los del proyecto sustituyen a los de la app
/// con el mismo id. Devuelve ordenados por id.
fn load_from(app: &Path, project: Option<&Path>) -> Vec<OwnAgent> {
    ensure_defaults(app);
    let mut out = load_dir(app, "app");
    if let Some(p) = project {
        let own = load_dir(&p.join(".ide-agents"), "proyecto");
        let replaced: HashSet<String> = own.iter().map(|a| a.id.clone()).collect();
        out.retain(|a| !replaced.contains(&a.id));
        out.extend(own);
    }
    out.sort_by(|a, b| a.id.cmp(&b.id));
    out
}

/// Todos los agentes propios (de la app y, si hay proyecto, del proyecto).
pub fn load_all(project: Option<&Path>) -> Vec<OwnAgent> {
    load_from(&app_dir(), project)
}

/// Un agente propio por id (de los que load_from devuelve), o None si no existe.
pub fn get(project: Option<&Path>, id: &str) -> Option<OwnAgent> {
    load_from(&app_dir(), project).into_iter().find(|a| a.id == id)
}

/// Carpeta donde se guarda un agente: la del proyecto si es de proyecto y hay proyecto.
fn target_dir(project: Option<&Path>, a: &OwnAgent) -> PathBuf {
    if a.source == "proyecto" && project.is_some() {
        project.unwrap().join(".ide-agents")
    } else {
        app_dir()
    }
}

/// Valida la definición y la guarda como `<id>.md` en la carpeta dada.
fn save_to(dir: &Path, a: &OwnAgent) -> Result<(), String> {
    parse(&to_markdown(a))?;
    for c in &a.clis {
        if crate::agents::get(c).is_none() {
            return Err(format!("CLI desconocida: {c}"));
        }
    }
    fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let file = dir.join(format!("{}.md", a.id));
    crate::store::write_atomic(&file, &to_markdown(a))
}

/// Guarda la definición del agente en su carpeta (del proyecto o de la app).
pub fn save(project: Option<&Path>, a: &OwnAgent) -> Result<(), String> {
    save_to(&target_dir(project, a), a)
}

/// Borra el archivo `<id>.md` del agente; el evaluador de la app no se puede borrar.
pub fn delete(project: Option<&Path>, a: &OwnAgent) -> Result<(), String> {
    if a.id == "evaluador" && a.source == "app" {
        return Err("El evaluador no se puede borrar; desactívalo si no lo quieres".into());
    }
    // El id llega de la UI: sin validarlo, «../» permitiría borrar archivos fuera de la carpeta.
    if a.id.is_empty() || !a.id.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-') {
        return Err("El id solo puede tener minúsculas, números y guiones".into());
    }
    let file = target_dir(project, a).join(format!("{}.md", a.id));
    fs::remove_file(&file).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// Carpeta temporal propia de cada test (única por nombre y proceso).
    fn tmp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("agentes-own-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    const EJEMPLO: &str = "---\nid: evaluador\nnombre: Evaluador independiente\npermisos: lectura\ncuando: tras-sintesis\nclis: claude, codex, copilot\nseleccion: azar\nactivo: si\n---\nInstrucciones del agente.\n\nOtra línea.\n";

    #[test]
    fn parse_ejemplo_completo_y_valores_por_defecto() {
        let a = parse(EJEMPLO).unwrap();
        assert_eq!(a.id, "evaluador");
        assert_eq!(a.name, "Evaluador independiente");
        assert!(!a.write);
        assert_eq!(a.trigger, "tras-sintesis");
        assert_eq!(a.clis, vec!["claude", "codex", "copilot"]);
        assert!(a.random);
        assert!(a.enabled);
        assert_eq!(a.instructions, "Instrucciones del agente.\n\nOtra línea.");
        assert_eq!(a.source, "");

        // Solo con las claves mínimas: el resto toma sus valores por defecto.
        let d = parse("---\nid: basico\nnombre: Básico\n---\nHaz tu trabajo.\n").unwrap();
        assert!(!d.write);
        assert_eq!(d.trigger, "manual");
        assert!(d.clis.is_empty());
        assert!(!d.random);
        assert!(d.enabled);

        // Cada error al menos una vez, con su texto exacto.
        assert_eq!(parse("sin cabecera"), Err("Falta la cabecera (---) al principio".into()));
        assert_eq!(parse("---\nid: x\nnombre: X\nInstrucciones.\n"), Err("La cabecera no se cierra con ---".into()));
        assert_eq!(parse("---\nid: MAL_1\nnombre: X\n---\nInstrucciones.\n"), Err("El id solo puede tener minúsculas, números y guiones".into()));
        assert_eq!(parse("---\nid: x\n---\nInstrucciones.\n"), Err("Falta el nombre".into()));
        assert_eq!(parse("---\nid: x\nnombre: X\npermisos: total\n---\nInstrucciones.\n"), Err("Permisos no válidos: total".into()));
        assert_eq!(parse("---\nid: x\nnombre: X\ncuando: ayer\n---\nInstrucciones.\n"), Err("«cuando» no válido: ayer".into()));
        assert_eq!(parse("---\nid: x\nnombre: X\nseleccion: dado\n---\nInstrucciones.\n"), Err("Selección no válida: dado".into()));
        assert_eq!(parse("---\nid: x\nnombre: X\nactivo: quizás\n---\nInstrucciones.\n"), Err("«activo» debe ser si o no".into()));
        assert_eq!(parse("---\nid: x\nnombre: X\n---\n   \n"), Err("Faltan las instrucciones".into()));
    }

    #[test]
    fn round_trip_parse_to_markdown() {
        let a = OwnAgent {
            id: "revisor".into(),
            name: "Revisor".into(),
            write: true,
            trigger: "antes-de-commit".into(),
            clis: vec!["claude".into(), "codex".into()],
            random: false,
            enabled: false,
            instructions: "Revisa el diff.\n\nCuidado con los secretos.".into(),
            source: String::new(),
        };
        assert_eq!(parse(&to_markdown(&a)).unwrap(), a);
        let mut b = a.clone();
        b.source = "proyecto".into();
        assert_eq!(parse(&to_markdown(&b)).unwrap(), a); // source no se escribe en el archivo
    }

    #[test]
    fn parse_acepta_crlf_y_bom() {
        let crlf = EJEMPLO.replace('\n', "\r\n");
        assert_eq!(parse(&format!("\u{feff}{crlf}")).unwrap(), parse(EJEMPLO).unwrap());
    }

    #[test]
    fn load_from_crea_el_evaluador_y_no_sobrescribe() {
        let app = tmp("crea");
        let agents = load_from(&app, None);
        assert_eq!(agents.len(), 1);
        assert_eq!(agents[0].id, "evaluador");
        assert_eq!(agents[0].source, "app");
        assert_eq!(agents[0].trigger, "tras-sintesis");
        assert!(agents[0].random && agents[0].enabled);
        assert_eq!(fs::read_to_string(app.join("evaluador.md")).unwrap(), to_markdown(&default_evaluator()));

        // Si ya existe evaluador.md (aquí con otro nombre) no se sobrescribe.
        let app2 = tmp("no-sobrescribe");
        fs::write(app2.join("evaluador.md"), "---\nid: evaluador\nnombre: Mi evaluador\ncuando: manual\n---\nMío.\n").unwrap();
        let agents2 = load_from(&app2, None);
        assert_eq!(agents2.len(), 1);
        assert_eq!(agents2[0].name, "Mi evaluador");
        assert_eq!(fs::read_to_string(app2.join("evaluador.md")).unwrap(), "---\nid: evaluador\nnombre: Mi evaluador\ncuando: manual\n---\nMío.\n");
        let _ = std::fs::remove_dir_all(&app);
        let _ = std::fs::remove_dir_all(&app2);
    }

    #[test]
    fn el_proyecto_sustituye_a_la_app_y_ignora_invalidos() {
        let app = tmp("app");
        let proyecto = tmp("proyecto");
        fs::write(app.join("evaluador.md"), "---\nid: evaluador\nnombre: Evaluador app\ncuando: manual\n---\nDe la app.\n").unwrap();
        fs::create_dir_all(proyecto.join(".ide-agents")).unwrap();
        fs::write(proyecto.join(".ide-agents/evaluador.md"), "---\nid: evaluador\nnombre: Evaluador proyecto\npermisos: escritura\ncuando: manual\n---\nDel proyecto.\n").unwrap();
        fs::write(proyecto.join(".ide-agents/roto.md"), "sin cabecera\n").unwrap();
        let agents = load_from(&app, Some(&proyecto));
        assert_eq!(agents.len(), 1);
        assert_eq!(agents[0].name, "Evaluador proyecto");
        assert_eq!(agents[0].source, "proyecto");
        assert!(agents[0].write);
        let _ = std::fs::remove_dir_all(&app);
        let _ = std::fs::remove_dir_all(&proyecto);
    }

    #[test]
    fn save_to_rechaza_cli_desconocida() {
        let dir = tmp("save");
        let a = OwnAgent {
            id: "con-cli".into(),
            name: "Con CLI".into(),
            write: false,
            trigger: "manual".into(),
            clis: vec!["nope".into()],
            random: false,
            enabled: true,
            instructions: "Haz algo.".into(),
            source: String::new(),
        };
        assert_eq!(save_to(&dir, &a), Err("CLI desconocida: nope".into()));
        assert!(!dir.join("con-cli.md").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
    #[test]
    fn delete_rechaza_ids_con_rutas() {
        let mut a = default_evaluator();
        a.id = "../fuera".into();
        a.source = "app".into();
        assert_eq!(delete(None, &a).unwrap_err(), "El id solo puede tener minúsculas, números y guiones");
    }
}
