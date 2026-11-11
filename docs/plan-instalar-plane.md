# Plan: instalar y configurar Plane automáticamente

Estado: **pendiente de decidir** (ver «Decisión»). Contexto: `docs/plan-plane-objetivo.md` (integración) y
`docs/releases.md` (releases y actualizaciones).

## Cómo está hoy

`scripts/wsl/instalar-estable.sh` y las releases instalan **solo la app**: el binario y el lanzador. La
pestaña Objetivo enseña Plane si encuentra `~/.ide-workspace-chat/plane.json`; si no, la lista vieja
(la de IDE-29).

En el equipo de desarrollo, Plane se montó a mano en tres pasos:

1. **Servidor** (`objetivos/servidor`): `.env` copiado de `.env.example` con sus contraseñas, y
   `docker compose -p objetivos up -d --build`. Construye la imagen `objetivos-plane-api:local` desde
   `objetivos/plane/apps/api`. La primera vez tarda bastante.
2. **Preparar**: `docker compose -p objetivos exec -T api python manage.py shell < preparar.py`.
   Crea el usuario local, el espacio de trabajo `objetivos` y la clave de API (se guarda en
   `.plane-token`). Se puede repetir sin duplicar nada.
3. **Conectar la app**: `~/.ide-workspace-chat/plane.json` con `{url, token, workspace}` y permisos 600.
   La clave nunca llega a la página: la usa Rust (`src-tauri/src/plane.rs`).

Consecuencias:

- En otro equipo, o al instalar una release, la pestaña Objetivo sale con la lista vieja.
- Si Docker Desktop no arranca tras reiniciar Windows, la pestaña no conecta.
- El paso 1 necesita `objetivos/plane` (la copia completa de Plane), que solo existe en el disco del
  equipo de desarrollo: fuera de él no se puede construir la imagen.

## Propuesta: que lo haga la propia app

Así vale igual para `instalar-estable.sh` que para las releases.

### Al abrir la pestaña Objetivo sin Plane configurado

Un aviso con el botón **«Instalar Plane»** que:

1. Comprueba Docker (`docker version`). Si no está o no responde: lo dice y enlaza cómo instalarlo,
   en vez de fallar. En WSL, Docker Desktop con la integración de WSL activada (hoy está desactivada:
   el servidor se maneja desde Windows).
2. Crea la carpeta del servidor en los datos de la app (`~/.ide-workspace-chat/plane/`) con el
   `compose.yml` y un `.env` con **contraseñas aleatorias** (Postgres, RabbitMQ, MinIO, `SECRET_KEY`).
3. `docker compose -p objetivos up -d` con la imagen ya construida (ver «Decisión»).
4. Espera a que la API responda y ejecuta `preparar.py` dentro del contenedor.
5. Escribe `plane.json` (600) con la URL `http://127.0.0.1:8100`, la clave y el espacio de trabajo.
6. Recarga la pestaña: ya sale Plane.

El progreso se ve en la pestaña (paso a paso) y se puede reintentar: cada paso es repetible.

### Al abrir la app

Si Plane está configurado pero parado (la API no responde), lo arranca con `docker compose up -d`
(sin `--build`). Opción en el menú para pararlo y para desinstalarlo (con aviso: los datos están en
los volúmenes de Docker).

### Lo que se mantiene

- Nada sale del equipo: puertos solo en `127.0.0.1`, sin telemetría (`docs/plan-plane-objetivo.md`).
- La clave de API solo en `plane.json` (600) y en Rust; nunca en la página ni en git.

## Decisión

¿De dónde sale la imagen de Plane en un equipo que no tiene `objetivos/plane`?

| Opción | Cómo | A favor | En contra |
|---|---|---|---|
| **A. Imagen publicada** (recomendada) | El workflow Release construye `objetivos-plane-api` desde nuestra copia de Plane y la publica en GitHub Container Registry (`ghcr.io/ramosjr18/…`), con la versión de la app. | Instalar es descargar la imagen: rápido y igual en todos los equipos. | Hay que subir nuestra copia de Plane a algún sitio para que el workflow la construya (repo o submódulo), y la imagen es pública (AGPL: con su código disponible). |
| B. Código de Plane en el repo | Meter `objetivos/plane/apps/api` (y lo que necesite) en este repo y construir la imagen en cada equipo. | Sin registro de imágenes. | Mucho código ajeno en el repo; la primera instalación tarda bastante en cada equipo. |
| C. Solo el equipo de desarrollo | Automatizar los tres pasos con la copia local de `objetivos/plane`. | Lo más rápido de hacer. | No vale para nadie más ni para otro equipo. |

En cualquier caso hay que conservar `LICENSE.txt`, `COPYRIGHT.txt` y las cabeceras de Plane, y la
telemetría desactivada (`docs/plan-plane-objetivo.md`).

## Pasos cuando se decida

1. (A) Job en `release.yml` que construye y publica la imagen; `compose.yml` con `image:` en vez de `build:`.
2. `src-tauri/src/plane.rs`: comandos `plane_install` (los pasos de arriba, con eventos de progreso),
   `plane_start`, `plane_stop`.
3. Pestaña Objetivo: aviso «Instalar Plane» con el progreso, y errores claros (sin Docker, puerto ocupado…).
4. Arranque automático al abrir la app.
5. Tests: generación del `.env` (contraseñas nuevas, no pisa uno existente), `plane.json` con 600,
   y que la clave no salga en ningún evento ni log.
