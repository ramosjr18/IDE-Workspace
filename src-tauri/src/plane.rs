//! Pestaña Objetivo con Plane (docs/plan-plane-objetivo.md): la página pide a la API de Plane por
//! aquí. La clave de la API se queda en Rust y nunca llega a la página ni a los agentes.
//! Configuración en `data_dir()/plane.json`: `{ "url": "http://127.0.0.1:8100", "token": "plane_api_…", "workspace": "objetivos" }`.

use crate::store::data_dir;
use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Deserialize)]
struct Config {
    url: String,
    token: String,
    #[serde(default = "default_workspace")]
    workspace: String,
}

fn default_workspace() -> String {
    "objetivos".into()
}

fn config() -> Result<Config, String> {
    let path = data_dir().join("plane.json");
    let text = std::fs::read_to_string(&path).map_err(|_| format!("Plane no está configurado (falta {})", path.display()))?;
    let c: Config = serde_json::from_str(&text).map_err(|e| format!("{} no es válido: {e}", path.display()))?;
    if c.url.trim().is_empty() || c.token.trim().is_empty() {
        return Err(format!("{}: faltan la URL o la clave", path.display()));
    }
    Ok(c)
}

#[derive(Serialize)]
pub struct Info {
    pub configured: bool,
    pub workspace: String,
    pub error: Option<String>,
}

/// Si Plane está configurado y con qué espacio de trabajo (sin la clave).
pub fn info() -> Info {
    match config() {
        Ok(c) => Info { configured: true, workspace: c.workspace, error: None },
        Err(e) => Info { configured: false, workspace: String::new(), error: Some(e) },
    }
}

#[derive(Serialize)]
pub struct Response {
    pub status: u16,
    pub body: serde_json::Value,
}

/// Solo rutas de la API pública, sin saltos de carpeta ni otra máquina.
fn valid_path(path: &str) -> bool {
    path.starts_with("/api/v1/") && !path.contains("..") && !path.contains("://") && !path.contains('\\')
}

/// Petición a la API de Plane con la clave de la configuración.
pub async fn request(method: &str, path: &str, body: Option<serde_json::Value>) -> Result<Response, String> {
    if !valid_path(path) {
        return Err(format!("Ruta no permitida: {path}"));
    }
    let method = match method {
        "GET" => reqwest::Method::GET,
        "POST" => reqwest::Method::POST,
        "PATCH" => reqwest::Method::PATCH,
        "DELETE" => reqwest::Method::DELETE,
        other => return Err(format!("Método no permitido: {other}")),
    };
    let c = config()?;
    let url = format!("{}{}", c.url.trim_end_matches('/'), path);
    let client = reqwest::Client::builder().timeout(Duration::from_secs(30)).build().map_err(|e| e.to_string())?;
    let mut req = client.request(method, &url).header("X-Api-Key", c.token.trim());
    if let Some(b) = body {
        req = req.header("Content-Type", "application/json").body(b.to_string());
    }
    let res = req.send().await.map_err(|e| format!("No se pudo hablar con Plane ({}): {e}", c.url))?;
    let status = res.status().as_u16();
    let text = res.text().await.map_err(|e| e.to_string())?;
    let body = serde_json::from_str(&text).unwrap_or(serde_json::Value::String(text));
    Ok(Response { status, body })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn solo_rutas_de_la_api_publica() {
        assert!(valid_path("/api/v1/workspaces/objetivos/projects/"));
        assert!(!valid_path("/api/instances/"));
        assert!(!valid_path("/api/v1/../admin/"));
        assert!(!valid_path("http://otra/api/v1/"));
        assert!(!valid_path("/api/v1/x?u=http://otra"));
    }
}
