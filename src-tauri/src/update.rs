//! Actualizaciones desde las releases de GitHub (docs/releases.md).
//!
//! Cada release publica `latest.json` (el formato del actualizador de Tauri más un apartado `binario`).
//! La app lo lee al arrancar y cada pocas horas; si hay una versión más nueva, avisa y el usuario
//! decide cuándo actualizar. Dos formas de instalarla:
//! - **instalador** (Windows, macOS, AppImage): el actualizador oficial de Tauri, que comprueba la
//!   firma del paquete con la clave pública de `tauri.conf.json`.
//! - **binario** (la versión estable de WSL/Linux, sin paquete): descarga el binario, comprueba su
//!   sha256 y lo deja en `~/.local/share/chat-agentes/<versión>`, con el enlace `actual` apuntando a él.
//!   El lanzador `chat-agentes` abre esa versión si existe.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tauri::AppHandle;

pub const REPO: &str = "ramosjr18/IDE-Workspace";
const FEED: &str = "https://github.com/ramosjr18/IDE-Workspace/releases/latest/download/latest.json";

#[derive(Deserialize, Debug)]
struct Feed {
    version: String,
    #[serde(default)]
    notes: String,
    #[serde(default)]
    pub_date: Option<String>,
    /// Binarios sueltos para la instalación «binario»: `linux-x86_64` → url y sha256.
    #[serde(default)]
    binario: HashMap<String, Asset>,
}

#[derive(Deserialize, Clone, Debug)]
struct Asset {
    url: String,
    sha256: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    pub current: String,
    pub version: String,
    pub notes: String,
    pub date: Option<String>,
    /// instalador | binario | manual (no se puede instalar sola: se abre la página de la release).
    pub mode: &'static str,
    pub page: String,
}

/// Cómo se instaló esta copia de la app.
pub fn mode() -> &'static str {
    if cfg!(target_os = "linux") {
        if std::env::var_os("APPIMAGE").is_some() {
            return "instalador";
        }
        // Paquete .deb/.rpm (en /usr): se actualiza con el gestor de paquetes.
        let exe = std::env::current_exe().unwrap_or_default();
        if exe.starts_with("/usr/") {
            return "manual";
        }
        return "binario";
    }
    "instalador"
}

/// `a` es más nueva que `b` (semver sencillo: «1.2.10» > «1.2.9»; la «v» delante se ignora).
pub fn newer(a: &str, b: &str) -> bool {
    let parse = |s: &str| -> Vec<u64> {
        s.trim().trim_start_matches('v').split(['.', '-', '+']).take(3).map(|p| p.parse().unwrap_or(0)).collect()
    };
    parse(a) > parse(b)
}

fn client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .user_agent("chat-de-agentes")
        .timeout(Duration::from_secs(120))
        .connect_timeout(Duration::from_secs(10))
        .build()
        .map_err(|e| e.to_string())
}

/// `CHAT_UPDATE_FEED` apunta a otro latest.json (pruebas sin publicar nada).
fn feed_url() -> String {
    std::env::var("CHAT_UPDATE_FEED").ok().filter(|u| !u.is_empty()).unwrap_or_else(|| FEED.to_string())
}

async fn feed() -> Result<Feed, String> {
    feed_from(&feed_url()).await
}

async fn feed_from(url: &str) -> Result<Feed, String> {
    let r = client()?.get(url).send().await.map_err(|e| format!("No se pudo consultar GitHub: {e}"))?;
    if r.status() == reqwest::StatusCode::NOT_FOUND {
        return Err("Todavía no hay ninguna release publicada".into());
    }
    if !r.status().is_success() {
        return Err(format!("GitHub respondió {}", r.status()));
    }
    let bytes = r.bytes().await.map_err(|e| format!("No se pudo leer latest.json: {e}"))?;
    serde_json::from_slice::<Feed>(&bytes).map_err(|e| format!("latest.json no válido: {e}"))
}

/// ¿Hay una versión más nueva? `None` si esta es la última.
pub async fn check(app: &AppHandle) -> Result<Option<UpdateInfo>, String> {
    let current = app.package_info().version.to_string();
    let f = feed().await?;
    if !newer(&f.version, &current) {
        return Ok(None);
    }
    let version = f.version.trim_start_matches('v').to_string();
    Ok(Some(UpdateInfo {
        page: format!("https://github.com/{REPO}/releases/tag/v{version}"),
        current,
        version,
        notes: f.notes,
        date: f.pub_date,
        mode: mode(),
    }))
}

/// Descarga e instala la versión nueva. Al acabar hay que reiniciar la app ([`restart`]).
pub async fn install(app: &AppHandle) -> Result<String, String> {
    match mode() {
        "binario" => install_binary(app).await,
        "instalador" => install_with_tauri(app).await,
        _ => Err(format!("Esta copia se instaló con un paquete del sistema: descarga la nueva desde https://github.com/{REPO}/releases/latest")),
    }
}

async fn install_with_tauri(app: &AppHandle) -> Result<String, String> {
    use tauri_plugin_updater::UpdaterExt;
    let update = app
        .updater()
        .map_err(|e| e.to_string())?
        .check()
        .await
        .map_err(|e| format!("No se pudo comprobar la actualización: {e}"))?
        .ok_or("Ya tienes la última versión")?;
    let version = update.version.clone();
    // Comprueba la firma del paquete antes de instalarlo.
    update.download_and_install(|_, _| {}, || {}).await.map_err(|e| format!("No se pudo instalar: {e}"))?;
    Ok(version)
}

/// Carpeta de las versiones instaladas por la propia app (sin root).
pub fn versions_dir() -> PathBuf {
    dirs::data_local_dir().unwrap_or_else(|| crate::env::home().join(".local/share")).join("chat-agentes")
}

fn platform() -> &'static str {
    if cfg!(target_arch = "aarch64") { "linux-aarch64" } else { "linux-x86_64" }
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes).iter().map(|b| format!("{b:02x}")).collect()
}

async fn install_binary(app: &AppHandle) -> Result<String, String> {
    install_binary_from(&feed_url(), &app.package_info().version.to_string(), versions_dir()).await
}

async fn install_binary_from(feed_url: &str, current: &str, dir: PathBuf) -> Result<String, String> {
    let f = feed_from(feed_url).await?;
    if !newer(&f.version, current) {
        return Err("Ya tienes la última versión".into());
    }
    let asset = f.binario.get(platform()).cloned().ok_or_else(|| format!("La release {} no trae binario para {}", f.version, platform()))?;
    let bytes = client()?
        .get(&asset.url)
        .send()
        .await
        .and_then(|r| r.error_for_status())
        .map_err(|e| format!("No se pudo descargar: {e}"))?
        .bytes()
        .await
        .map_err(|e| format!("No se pudo descargar: {e}"))?;
    if !sha256_hex(&bytes).eq_ignore_ascii_case(asset.sha256.trim()) {
        return Err("La descarga no coincide con su sha256: no se instala".into());
    }
    let version = f.version.trim_start_matches('v').to_string();
    tauri::async_runtime::spawn_blocking(move || unpack(&dir, &version, &bytes).map(|_| version))
        .await
        .map_err(|e| e.to_string())?
}

/// Deja el binario en `<dir>/<versión>/` y mueve el enlace `<dir>/actual` a esa carpeta.
fn unpack(dir: &Path, version: &str, tar_gz: &[u8]) -> Result<(), String> {
    if version.is_empty() || !version.chars().all(|c| c.is_ascii_alphanumeric() || ".-+".contains(c)) {
        return Err(format!("Versión no válida: {version}"));
    }
    let dest = dir.join(version);
    let tmp = dir.join(format!(".{version}.descarga"));
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).map_err(|e| e.to_string())?;
    let archive = tmp.join("paquete.tar.gz");
    std::fs::write(&archive, tar_gz).map_err(|e| e.to_string())?;
    let ok = std::process::Command::new("tar")
        .arg("-xzf")
        .arg(&archive)
        .arg("-C")
        .arg(&tmp)
        .status()
        .map_err(|e| e.to_string())?
        .success();
    let _ = std::fs::remove_file(&archive);
    let exe = tmp.join("chat-agentes");
    if !ok || !exe.is_file() {
        let _ = std::fs::remove_dir_all(&tmp);
        return Err("El paquete descargado no trae el binario chat-agentes".into());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&exe, std::fs::Permissions::from_mode(0o755));
    }
    let _ = std::fs::remove_dir_all(&dest);
    std::fs::rename(&tmp, &dest).map_err(|e| e.to_string())?;
    std::fs::write(dest.join("VERSION"), format!("{version} {}\n", chrono::Local::now().to_rfc3339())).map_err(|e| e.to_string())?;
    #[cfg(unix)]
    {
        // Enlace nuevo con otro nombre y rename: el cambio es atómico.
        let link = dir.join("actual");
        let new_link = dir.join(".actual.nuevo");
        let _ = std::fs::remove_file(&new_link);
        std::os::unix::fs::symlink(&dest, &new_link).map_err(|e| e.to_string())?;
        std::fs::rename(&new_link, &link).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Cierra la app y la abre con la versión nueva.
pub fn restart(app: &AppHandle) {
    if mode() == "binario" {
        let exe = versions_dir().join("actual").join("chat-agentes");
        if exe.is_file() {
            // Se abre cuando esta ya se ha cerrado: con las dos a la vez, GTK pasaría el control a la vieja.
            let mut cmd = std::process::Command::new("/bin/sh");
            cmd.args(["-c", "sleep 1; exec \"$0\""]).arg(&exe).stdin(std::process::Stdio::null());
            #[cfg(unix)]
            {
                use std::os::unix::process::CommandExt;
                cmd.process_group(0);
            }
            if let Some(home) = dirs::home_dir() {
                cmd.current_dir(home);
            }
            let _ = cmd.spawn();
        }
        app.exit(0);
        return;
    }
    app.restart();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn comparar_versiones() {
        assert!(newer("1.1.0", "1.0.0"));
        assert!(newer("v1.2.10", "1.2.9"));
        assert!(newer("2.0.0", "1.99.99"));
        assert!(!newer("1.0.0", "1.0.0"));
        assert!(!newer("1.0.0", "1.0.1"));
        assert!(!newer("basura", "1.0.0"));
    }

    #[test]
    fn feed_con_binario() {
        let f: Feed = serde_json::from_str(
            r#"{"version":"1.1.0","notes":"Mejoras","pub_date":"2026-11-11T00:00:00Z",
                "platforms":{"windows-x86_64":{"signature":"s","url":"u"}},
                "binario":{"linux-x86_64":{"url":"https://x/y.tar.gz","sha256":"ab"}}}"#,
        )
        .unwrap();
        assert_eq!(f.version, "1.1.0");
        assert_eq!(f.binario["linux-x86_64"].sha256, "ab");
        // Un latest.json sin binario (solo instaladores) también vale.
        let f: Feed = serde_json::from_str(r#"{"version":"1.1.0","platforms":{}}"#).unwrap();
        assert!(f.binario.is_empty());
        assert_eq!(sha256_hex(b"abc"), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
    }

    /// Servidor HTTP mínimo: sirve `files` (ruta → contenido) a las peticiones que lleguen.
    async fn serve(files: Vec<(&'static str, Vec<u8>)>) -> String {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            while let Ok((mut sock, _)) = listener.accept().await {
                let files = files.clone();
                tokio::spawn(async move {
                    let mut buf = vec![0u8; 4096];
                    let n = sock.read(&mut buf).await.unwrap_or(0);
                    let req = String::from_utf8_lossy(&buf[..n]).to_string();
                    let path = req.split_whitespace().nth(1).unwrap_or("/").to_string();
                    let (status, body) = match files.iter().find(|(p, _)| *p == path) {
                        Some((_, b)) => ("200 OK", b.clone()),
                        None => ("404 Not Found", Vec::new()),
                    };
                    let head = format!("HTTP/1.1 {status}
Content-Length: {}
Connection: close

", body.len());
                    let _ = sock.write_all(head.as_bytes()).await;
                    let _ = sock.write_all(&body).await;
                });
            }
        });
        format!("http://{addr}")
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn actualiza_el_binario_desde_un_feed() {
        let root = std::env::temp_dir().join(format!("feed-{}", uuid::Uuid::new_v4().simple()));
        let src = root.join("src");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(src.join("chat-agentes"), "#!/bin/sh
echo nueva
").unwrap();
        let tgz = root.join("p.tar.gz");
        assert!(std::process::Command::new("tar").arg("-czf").arg(&tgz).arg("-C").arg(&src).arg("chat-agentes").status().unwrap().success());
        let bytes = std::fs::read(&tgz).unwrap();
        let sha = sha256_hex(&bytes);
        // El feed necesita la URL del servidor: primero se levanta para saberla.
        let base = serve(vec![("/p.tar.gz", bytes.clone())]).await;
        let json = |sha: &str| format!(r#"{{"version":"1.1.0","notes":"n","platforms":{{}},"binario":{{"{}":{{"url":"{base}/p.tar.gz","sha256":"{sha}"}}}}}}"#, platform());
        let feed_ok = serve(vec![("/latest.json", json(&sha).into_bytes())]).await;
        let feed_bad = serve(vec![("/latest.json", json("00").into_bytes())]).await;
        let dir = root.join("versiones");
        // sha256 que no cuadra: no se instala nada.
        assert!(install_binary_from(&format!("{feed_bad}/latest.json"), "1.0.0", dir.clone()).await.unwrap_err().contains("sha256"));
        assert!(!dir.join("actual").exists());
        // Ya en la última: nada que hacer.
        assert!(install_binary_from(&format!("{feed_ok}/latest.json"), "1.1.0", dir.clone()).await.is_err());
        // Más vieja: se instala y `actual` apunta a la nueva.
        assert_eq!(install_binary_from(&format!("{feed_ok}/latest.json"), "1.0.0", dir.clone()).await.unwrap(), "1.1.0");
        assert!(dir.join("actual/chat-agentes").is_file());
        assert!(feed_from(&format!("{feed_ok}/no-existe.json")).await.unwrap_err().contains("ninguna release"));
        let _ = std::fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[test]
    fn desempaqueta_y_mueve_actual() {
        let root = std::env::temp_dir().join(format!("versiones-{}", uuid::Uuid::new_v4().simple()));
        let src = root.join("src");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(src.join("chat-agentes"), "#!/bin/sh\n").unwrap();
        let tgz = root.join("p.tar.gz");
        assert!(std::process::Command::new("tar").arg("-czf").arg(&tgz).arg("-C").arg(&src).arg("chat-agentes").status().unwrap().success());
        let bytes = std::fs::read(&tgz).unwrap();
        let dir = root.join("chat-agentes");
        unpack(&dir, "1.1.0", &bytes).unwrap();
        assert!(dir.join("actual/chat-agentes").is_file());
        assert_eq!(std::fs::read_link(dir.join("actual")).unwrap(), dir.join("1.1.0"));
        unpack(&dir, "1.2.0", &bytes).unwrap();
        assert_eq!(std::fs::read_link(dir.join("actual")).unwrap(), dir.join("1.2.0"));
        assert!(unpack(&dir, "../x", &bytes).is_err());
        assert!(unpack(&dir, "1.3.0", b"no es un tar").is_err());
        assert_eq!(std::fs::read_link(dir.join("actual")).unwrap(), dir.join("1.2.0"));
        let _ = std::fs::remove_dir_all(root);
    }
}
