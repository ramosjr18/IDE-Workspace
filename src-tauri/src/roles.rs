//! Roles del consejo de agentes y plantillas por tipo de documento (ADR, política, nota).
//! Todo es editable desde la app y se guarda en `data_dir()/council-config.json`;
//! si el archivo falta o es inválido, se cae en la configuración por defecto.

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

/// Rol que puede asumir un miembro del consejo.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Role {
    /// Identificador interno (minúsculas, dígitos y guiones).
    pub id: String,
    /// Nombre visible en el selector de roles.
    pub name: String,
    /// Instrucción que se da al agente que asume este rol.
    pub description: String,
}

/// Agente de una plantilla con el rol que asume.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TemplateMember {
    /// Id del agente.
    pub agent: String,
    /// Id del rol que asume en este consejo.
    pub role: String,
}

/// Plantilla de un consejo: miembros, moderador y documento que se redacta.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Template {
    /// Identificador de la plantilla.
    pub id: String,
    /// Nombre visible en el selector de plantillas.
    pub name: String,
    /// Documento que se redacta: adr | politica | nota.
    pub output: String,
    /// Miembros que participan, con su rol.
    pub members: Vec<TemplateMember>,
    /// Agente que modera y redacta la propuesta final.
    pub moderator: String,
}

/// Configuración editable del consejo: roles disponibles y plantillas.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Config {
    /// Roles que puede asumir cualquier miembro.
    pub roles: Vec<Role>,
    /// Plantillas por tipo de consejo (las antiguas no tienen este campo).
    #[serde(default)]
    pub templates: Vec<Template>,
}

pub fn default_roles() -> Vec<Role> {
    vec![
        Role { id: "arquitecto".into(), name: "Arquitecto".into(), description: "Piensa en la estructura a largo plazo: simplicidad, mantenibilidad, coherencia con lo que ya existe en el repo.".into() },
        Role { id: "abogado".into(), name: "Abogado del diablo".into(), description: "Busca activamente los puntos débiles, los supuestos no demostrados y las alternativas que nadie ha considerado. No des la razón por cortesía.".into() },
        Role { id: "seguridad".into(), name: "Seguridad".into(), description: "Evalúa riesgos de seguridad, privacidad, cumplimiento y qué puede salir mal en producción.".into() },
        Role { id: "coste".into(), name: "Coste y operación".into(), description: "Evalúa el coste, el esfuerzo de implantación, la operación diaria y la carga para el equipo.".into() },
        Role { id: "usuario".into(), name: "Usuario y producto".into(), description: "Defiende la experiencia de quien usa el producto y el valor que aporta la decisión.".into() },
        Role { id: "libre".into(), name: "Libre".into(), description: "Da tu opinión sincera y razonada desde el ángulo que te parezca más importante.".into() },
    ]
}

pub fn default_config() -> Config {
    Config { roles: default_roles(), templates: vec![] }
}

fn validate(cfg: &Config) -> Result<(), String> {
    if cfg.roles.is_empty() {
        return Err("Tiene que haber al menos un rol".into());
    }
    for r in &cfg.roles {
        if r.id.is_empty() || !r.id.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-') {
            return Err("Cada rol necesita un id".into());
        }
    }
    for r in &cfg.roles {
        if r.name.trim().is_empty() {
            return Err("Cada rol necesita un nombre".into());
        }
    }
    let mut seen: Vec<&str> = vec![];
    for r in &cfg.roles {
        if seen.contains(&r.id.as_str()) {
            return Err(format!("Hay dos roles con el id «{}»", r.id));
        }
        seen.push(&r.id);
    }
    for t in &cfg.templates {
        if t.name.trim().is_empty() {
            return Err("Cada plantilla necesita un nombre".into());
        }
    }
    seen.clear();
    for t in &cfg.templates {
        if t.id.is_empty() {
            return Err("Cada plantilla necesita un id".into());
        }
        if seen.contains(&t.id.as_str()) {
            return Err(format!("Hay dos plantillas con el id «{}»", t.id));
        }
        seen.push(&t.id);
    }
    for t in &cfg.templates {
        for m in &t.members {
            if !cfg.roles.iter().any(|r| r.id == m.role) {
                return Err(format!("La plantilla «{}» usa el rol «{}», que no existe", t.name, m.role));
            }
        }
    }
    Ok(())
}

/// Lee `base/council-config.json`; ante cualquier problema usa la configuración por defecto.
fn load_from(base: &Path) -> Config {
    let cfg: Option<Config> = fs::read_to_string(base.join("council-config.json"))
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .filter(|cfg: &Config| validate(cfg).is_ok());
    cfg.unwrap_or_else(default_config)
}

/// Guarda la configuración en `base/council-config.json`, creando antes la carpeta.
fn save_to(base: &Path, cfg: &Config) -> Result<(), String> {
    validate(cfg)?;
    fs::create_dir_all(base).map_err(|e| e.to_string())?;
    let json = serde_json::to_string_pretty(cfg).map_err(|e| e.to_string())?;
    crate::store::write_atomic(&base.join("council-config.json"), &json)
}

pub fn load() -> Config {
    load_from(&crate::store::data_dir())
}

pub fn save(cfg: &Config) -> Result<(), String> {
    save_to(&crate::store::data_dir(), cfg)
}

/// Nombre e instrucción de un rol; si no existe, el del rol «libre».
pub fn role_info(id: &str) -> (String, String) {
    let cfg = load();
    if let Some(r) = cfg.roles.iter().find(|r| r.id == id) {
        return (r.name.clone(), r.description.clone());
    }
    let desc = default_roles().into_iter().find(|r| r.id == "libre").map(|r| r.description).unwrap_or_default();
    ("Libre".into(), desc)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("roles-test-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn sin_archivo_devuelve_default() {
        let base = tmp("sin-archivo");
        let cfg = load_from(&base);
        assert_eq!(cfg, default_config(), "sin archivo cae en la configuración por defecto");
        assert_eq!(cfg.roles.len(), 6, "seis roles por defecto");
        assert!(cfg.templates.is_empty());
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn guardar_y_leer_conserva() {
        let base = tmp("guardar-leer");
        let mut cfg = default_config();
        cfg.roles.push(Role { id: "pruebas".into(), name: "Pruebas".into(), description: "Examina las pruebas".into() });
        cfg.templates.push(Template {
            id: "adr".into(),
            name: "ADR clásico".into(),
            output: "adr".into(),
            members: vec![TemplateMember { agent: "claude".into(), role: "arquitecto".into() }, TemplateMember { agent: "codex".into(), role: "abogado".into() }],
            moderator: "codex".into(),
        });
        save_to(&base, &cfg).unwrap();
        assert_eq!(load_from(&base), cfg, "lo guardado se lee igual");
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn save_to_rechaza_config_invalida() {
        let base = tmp("invalida");
        assert_eq!(save_to(&base, &Config { roles: vec![], templates: vec![] }).unwrap_err(), "Tiene que haber al menos un rol");
        let upper = Config { roles: vec![Role { id: "Arquitecto".into(), name: "Arquitecto".into(), description: "x".into() }], templates: vec![] };
        assert_eq!(save_to(&base, &upper).unwrap_err(), "Cada rol necesita un id");
        let spaces = Config { roles: vec![Role { id: "con espacios".into(), name: "Espacio".into(), description: "x".into() }], templates: vec![] };
        assert_eq!(save_to(&base, &spaces).unwrap_err(), "Cada rol necesita un id");
        let no_name = Config { roles: vec![Role { id: "valido".into(), name: "   ".into(), description: "x".into() }], templates: vec![] };
        assert_eq!(save_to(&base, &no_name).unwrap_err(), "Cada rol necesita un nombre");
        let dup = Config {
            roles: vec![Role { id: "libre".into(), name: "Libre".into(), description: "x".into() }, Role { id: "libre".into(), name: "Otra".into(), description: "y".into() }],
            templates: vec![],
        };
        assert_eq!(save_to(&base, &dup).unwrap_err(), "Hay dos roles con el id «libre»");
        let no_role = Config {
            roles: default_roles(),
            templates: vec![Template { id: "adr".into(), name: "ADR".into(), output: "adr".into(), members: vec![TemplateMember { agent: "claude".into(), role: "fantasma".into() }], moderator: "claude".into() }],
        };
        assert_eq!(save_to(&base, &no_role).unwrap_err(), "La plantilla «ADR» usa el rol «fantasma», que no existe");
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn json_invalido_devuelve_default() {
        let base = tmp("json-invalido");
        fs::create_dir_all(&base).unwrap();
        fs::write(base.join("council-config.json"), "esto no es json").unwrap();
        let cfg = load_from(&base);
        assert_eq!(cfg, default_config(), "JSON inválido cae en la configuración por defecto");
        assert_eq!(cfg.roles.len(), 6);
        let _ = fs::remove_dir_all(&base);
    }
}
