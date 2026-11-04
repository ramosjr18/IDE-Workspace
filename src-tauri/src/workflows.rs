//! Formas de trabajar: instrucciones que se añaden al mensaje del agente del chat. Dos de serie
//! («Directo» y «Equipo en worktrees») y las que haya en `data_dir()/workflows/*.md`, con una cabecera
//! `name:` / `description:` y debajo las instrucciones.

use crate::store::data_dir;
use serde::Serialize;

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct Workflow {
    pub id: String,
    pub name: String,
    pub description: String,
    #[serde(skip)]
    pub instructions: String,
}

/// Nota que reciben siempre los agentes del chat: si delegan en otro proceso, que se vea en la vista Agentes.
pub const LAUNCHER_NOTE: &str = "[Nota de la app «Chat de agentes»] Si lanzas otros agentes CLI (codex, opencode, claude, agy, gemini…) para que trabajen en paralelo, hazlo con `agentes run` en vez de `nohup`/`&`: así cada uno sale en su propia terminal en la vista «Agentes» del usuario. `agentes run --help` explica el uso; `agentes status` y `agentes wait <id>` dicen quién terminó y con qué código. Si el usuario pide un diagrama, un esquema, un plan visual o algo que se entienda mejor dibujado, escríbelo en Mermaid y envíalo con `agentes pizarra --title \"<título>\" diagrama.mmd` (o por la entrada estándar): se le abre en su pizarra. Un bloque ```mermaid en tu respuesta también llega a la pizarra.";

const TEAM: &str = r#"[Forma de trabajar: Equipo en worktrees]
Trabajas como orquestador: no implementas tú, repartes el trabajo entre otros agentes CLI y lo supervisas. Sigue estos pasos:

1. Plan. Si no hay un plan escrito y decidido por el usuario, escríbelo (en docs/) y pide su decisión antes de lanzar nada. Commitéalo: los worktrees salen de HEAD y un plan sin commitear no lo ven.
2. Carriles. Divide el trabajo en zonas de código disjuntas (p. ej. backend ↔ frontend), una por agente, para que no se pisen.
3. Brief por carril, por escrito, siempre en este orden: el problema con las palabras del usuario · su zona exclusiva y qué está prohibido tocar, con el porqué · el contrato con los demás carriles cerrado por adelantado (rutas, códigos de error, nombres de campo) · la línea base de tests medida · una puerta que se cierra con ficheros reales, no con «compila» · y «si ves que lo correcto es otra cosa, párate y escríbelo en ultimo-mensaje.md en vez de improvisar». Pide un commit por paso.
4. Cuota. Antes de lanzar, comprueba que cada CLI responde a una tarea real corta (no basta con «di OK»). Si una no tiene cuota, pasa su brief a otra.
5. Lanzar. Un worktree por agente con `agentes run --cli <cli> --worktree <agente>-<tema> --brief <brief.md> -- <argumentos>`. Ejemplos: `agentes run --cli codex --worktree codex-api --brief brief.md -- exec --approve-for-me -` · `agentes run --cli opencode --worktree oc-ui -- run --auto "$(cat brief.md)"` · `agentes run --cli claude --worktree claude-docs -- -p "$(cat brief.md)" --dangerously-skip-permissions`.
6. Supervisar. `agentes status` y `agentes wait <id>`; la salida de cada uno está en <worktree>/agente.log. Si uno se queda sin cuota a medias, mira si dejó trabajo sin commitear: suele estar escrito.
7. Verificar, no creer. Corre tú mismo build y tests en cada worktree y comprueba en el código cada afirmación de su informe. Devuelve correcciones concretas al mismo agente.
8. Mergear en el orden del plan (quien define un contrato antes que quien lo consume), quitar ultimo-mensaje.md antes de mergear, y limpiar (`git worktree remove`, borrar ramas).
9. Informa corto: qué hizo cada uno, qué se corrigió y qué queda."#;

fn builtin() -> Vec<Workflow> {
    vec![
        Workflow { id: "directo".into(), name: "Directo".into(), description: "El agente responde y trabaja él solo.".into(), instructions: String::new() },
        Workflow {
            id: "equipo".into(),
            name: "Equipo en worktrees".into(),
            description: "El agente orquesta: reparte el trabajo en carriles, lanza otros agentes en worktrees y los supervisa.".into(),
            instructions: TEAM.into(),
        },
    ]
}

/// Lee un archivo de forma de trabajar: cabecera `name:` / `description:` (opcional, hasta la primera línea vacía o `---`).
fn parse(id: &str, text: &str) -> Workflow {
    let mut name = id.to_string();
    let mut description = String::new();
    let mut body_start = 0;
    let mut offset = 0;
    let text = text.strip_prefix("---\n").unwrap_or(text);
    for line in text.split_inclusive('\n') {
        let l = line.trim();
        offset += line.len();
        if let Some(v) = l.strip_prefix("name:") {
            name = v.trim().to_string();
        } else if let Some(v) = l.strip_prefix("description:") {
            description = v.trim().to_string();
        } else {
            if l.is_empty() || l == "---" {
                body_start = offset;
            }
            break;
        }
        body_start = offset;
    }
    Workflow { id: id.to_string(), name, description, instructions: text[body_start..].trim().to_string() }
}

pub fn list() -> Vec<Workflow> {
    let mut all = builtin();
    if let Ok(entries) = std::fs::read_dir(data_dir().join("workflows")) {
        let mut extra: Vec<Workflow> = entries
            .flatten()
            .filter(|e| e.path().extension().is_some_and(|x| x == "md"))
            .filter_map(|e| {
                let id = e.path().file_stem()?.to_string_lossy().to_string();
                let text = std::fs::read_to_string(e.path()).ok()?;
                Some(parse(&id, &text))
            })
            .filter(|w| !all.iter().any(|b| b.id == w.id) && !w.instructions.is_empty())
            .collect();
        extra.sort_by(|a, b| a.name.cmp(&b.name));
        all.extend(extra);
    }
    all
}

pub fn get(id: &str) -> Option<Workflow> {
    list().into_iter().find(|w| w.id == id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lee_cabecera() {
        let w = parse("revision", "name: Revisión doble\ndescription: Dos agentes revisan\n\nHaz esto.\nY esto.");
        assert_eq!(w.name, "Revisión doble");
        assert_eq!(w.description, "Dos agentes revisan");
        assert_eq!(w.instructions, "Haz esto.\nY esto.");
        let w = parse("simple", "Solo instrucciones.");
        assert_eq!((w.name.as_str(), w.instructions.as_str()), ("simple", "Solo instrucciones."));
        let w = parse("fm", "---\nname: Con front matter\n---\nCuerpo");
        assert_eq!((w.name.as_str(), w.instructions.as_str()), ("Con front matter", "Cuerpo"));
    }

    #[test]
    fn de_serie() {
        let l = list();
        assert_eq!(l[0].id, "directo");
        assert!(l[0].instructions.is_empty());
        assert!(l[1].instructions.contains("agentes run"));
    }
}
