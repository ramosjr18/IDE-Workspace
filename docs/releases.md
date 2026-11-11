# Releases y actualizaciones

Las versiones de Chat de agentes se publican como **releases de GitHub** en
[`ramosjr18/IDE-Workspace`](https://github.com/ramosjr18/IDE-Workspace/releases). La app mira la última
release al arrancar y cada 6 horas. Si hay una más nueva, sale un aviso arriba: **«Hay una versión
nueva: X (tienes la Y)»**, con «Novedades», «Actualizar» y ✕ («ahora no»: no vuelve a avisar de esa
versión, sí de la siguiente). También se puede buscar a mano en el menú: **App → Buscar actualizaciones**.

## Sacar una versión

1. Subir el número de versión (semver) en `package.json`, `package-lock.json`, `src-tauri/Cargo.toml`,
   `src-tauri/Cargo.lock` y `src-tauri/tauri.conf.json`:

   ```bash
   scripts/release.sh subir 1.1.0
   git commit -am "Versión 1.1.0" && git push
   ```

2. Lanzar el workflow **Release** (en GitHub: Actions → Release → Run workflow, o desde aquí):

   ```bash
   scripts/release.sh publicar 1.1.0 "Paneles que se quedan, actualizaciones desde la app"
   ```

   - **preparar**: comprueba que la versión cuadra y crea la release `v1.1.0` como **borrador**, con
     las notas y la lista de commits desde la release anterior.
   - **paquetes**: compila macOS (Apple Silicon e Intel), Windows y Linux; firma los paquetes de
     actualización y los sube al borrador con `latest.json`. En Linux sube además el binario suelto
     `chat-agentes-<versión>-linux-x86_64.tar.gz`.
   - **publicar**: añade a `latest.json` el binario y su sha256 y publica la release como la última.
     Hasta ese momento `releases/latest` sigue siendo la anterior, así que ninguna app ve una release
     a medias. Con `publicar: false` se queda en borrador para revisarla.

## Cómo se actualiza cada instalación

| Instalación | Cómo se detecta | Qué hace «Actualizar» |
|---|---|---|
| Windows (instalador NSIS/MSI) | — | Actualizador de Tauri: descarga el instalador, **comprueba su firma** y lo ejecuta. |
| macOS (.app) | — | Actualizador de Tauri: descarga el `.app.tar.gz`, comprueba la firma y sustituye la app. |
| Linux AppImage | variable `APPIMAGE` | Actualizador de Tauri (firma) y sustituye el AppImage. |
| **WSL / Linux sin paquete** (la estable) | binario fuera de `/usr` | Descarga `chat-agentes-…tar.gz`, **comprueba su sha256** (el de `latest.json`) y lo deja en `~/.local/share/chat-agentes/<versión>`, con el enlace `actual` apuntando a él. No necesita root. |
| Linux .deb/.rpm | binario en `/usr` | No se instala sola: «Descargar» abre la release. |

Después, **«Reiniciar»** cierra la app y la abre con la versión nueva. Si hay agentes trabajando o el
chat está respondiendo, avisa antes: al cerrar la app se cortan (los paneles quedan «Interrumpido»).

En WSL, el lanzador `chat-agentes` (`scripts/wsl/chat-agentes`, en `/usr/local/bin`) abre la versión
instalada más tarde: la de `~/.local/share/chat-agentes/actual` (la que instala la app) o la de
`/opt/chat-agentes/actual` (la que compila `scripts/wsl/instalar-estable.sh` desde el repo, para
probar un commit concreto). Las versiones anteriores se quedan en su carpeta para poder volver.

## `latest.json`

Lo genera `tauri-action` (formato del actualizador de Tauri) y el último paso le añade `binario`:

```json
{
  "version": "1.1.0",
  "notes": "…",
  "pub_date": "2026-11-11T18:00:00Z",
  "platforms": {
    "windows-x86_64": { "signature": "…", "url": "…/Chat.de.agentes_1.1.0_x64-setup.exe" },
    "darwin-aarch64": { "signature": "…", "url": "…" },
    "linux-x86_64":   { "signature": "…", "url": "…AppImage" }
  },
  "binario": {
    "linux-x86_64": { "url": "…/chat-agentes-1.1.0-linux-x86_64.tar.gz", "sha256": "…" }
  }
}
```

La app lo lee de `https://github.com/ramosjr18/IDE-Workspace/releases/latest/download/latest.json`.
Para probar sin publicar nada: `CHAT_UPDATE_FEED=http://localhost:8000/latest.json chat-agentes`
(solo el modo binario; el actualizador de Tauri usa siempre la URL de `tauri.conf.json`).

## Claves de firma

Los paquetes de actualización van firmados (minisign). La **clave pública** está en
`src-tauri/tauri.conf.json` (`plugins.updater.pubkey`); la app rechaza cualquier paquete que no esté
firmado con su clave privada.

- La **privada** y su contraseña están en `~/.tauri/chat-agentes.key` y
  `~/.tauri/chat-agentes.key.password` (fuera del repo) y en los secretos del repo
  `TAURI_SIGNING_PRIVATE_KEY` / `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` (`scripts/release.sh secretos`).
- **Guárdalas en un sitio seguro** (gestor de contraseñas). Si se pierden, las apps instaladas no
  aceptarán más actualizaciones firmadas con otra clave: habría que reinstalarlas a mano.
- `createUpdaterArtifacts` está activado: `tauri build` (con paquetes) necesita la clave privada en el
  entorno. `instalar-estable.sh` compila con `--no-bundle` y no la necesita.

## Pendiente

- Sin certificados de código: Windows avisa de «editor desconocido» (SmartScreen) y macOS pide abrirla
  desde Ajustes → Privacidad la primera vez. Las actualizaciones posteriores ya no lo piden en Windows;
  en macOS, sin notarizar, puede volver a pedirlo. Para quitarlo: certificado de firma de Windows y
  Developer ID + notarización de Apple (como en el Hub).
