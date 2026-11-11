# Plane en la pestaña Objetivo

Estado al 11/11/2026, para retomarlo. Tickets: IDE-29 (vista Objetivo) e IDE-30 (trackers en el Objetivo: Jira y Plane).

## Decisión

- La pestaña **Objetivo** actual (IDE-29: objetivo en una línea y una checklist) no convence. Ahí irá **Plane** (https://github.com/makeplane/plane), un gestor de proyectos open source y autoalojable.
- Trabajamos con **nuestra propia copia de Plane** y la adaptamos a nuestro uso. No se sube nada a Plane.
- **Nada debe salir del equipo donde corre.** Ni telemetría ni consultas a servidores de Plane.
- Se respeta la autoría: se mantienen `LICENSE.txt`, `COPYRIGHT.txt`, los avisos de copyright de las cabeceras y el `README.md`, que dice de dónde viene el código. No nos queremos apropiar de algo ajeno.

## Vía elegida: opción 3 reutilizando el código de Plane

**Decisión del usuario (11/11/2026)**, que sustituye a la B. La pestaña Objetivo es nuestra, pero con la interfaz de Plane, reutilizando su propio código, no rehaciéndolo a mano:

- **Plane solo como servidor** (API, base de datos, procesos en segundo plano). Su web no se usa.
- **La pestaña Objetivo es un paquete de React** empaquetado con esbuild en `ui/vendor/`, igual que la pizarra con Excalidraw (`pizarra/entry.jsx`, `scripts/pizarra.mjs`).
- **Se ve igual que Plane desde el principio** porque usa sus estilos y sus piezas: `packages/tailwind-config` (Tailwind 4) y `packages/blocks` (tarjetas, diálogos, migas de pan, barras de progreso, selector de emojis, estructura de páginas…).
- **Las vistas se copian de `apps/web` poco a poco:** primero la lista de issues, luego el tablero kanban, el detalle del issue, los ciclos…
- **La capa de datos es nuestra,** en vez de sus stores de MobX. Habla con la API de Plane (`/api/v1/`, `X-Api-Key`) y sabe de conversaciones y agentes. Es la integración profunda que buscamos.
- **El editor de Plane** (`packages/editor`, Tiptap 3) para las descripciones y las páginas.
- Cualquier vista se puede adaptar después a nuestro gusto.

Por qué no rehacer la interfaz a mano: son unos 1.700 componentes (`.tsx`) de React 19, React Router 8, MobX y Tailwind 4. Rehacerlos en JS sin framework sería reescribir Plane, nunca quedaría igual y habría que perseguir cada cambio suyo.

### A tener en cuenta

- **React 19:** la app ya incluye React 18 para Excalidraw. Cada paquete lleva su propia copia de React, así que no hay conflicto. Se podrían unificar si Excalidraw admite React 19 (sin comprobar).
- **Licencia:** el código de Plane (AGPL-3.0) pasa a estar **dentro de la app**, así que la app entera queda sujeta a la AGPL. Si se reparten los instaladores (GitHub Actions), hay que ofrecer su código fuente bajo AGPL. Si el repo es público, eso ya se cumple en la práctica. Nuestro repo no tiene `LICENSE`: habrá que añadir la AGPL-3.0. Con esta vía, la app no puede tener una licencia cerrada.
- Plane sigue necesitando correr como servicio (Postgres, Redis, RabbitMQ, almacenamiento). Las formas 6, 8, 9 y 10 de abajo siguen siendo útiles. Las que tratan de cómo mostrar su web (1, 2, 5 y 7) ya no hacen falta.

### Primer paso

Una prueba pequeña: **la lista de issues de un proyecto**, con los estilos de Plane, dentro de la pestaña Objetivo, leyendo de un Plane levantado en local con Docker en WSL.

### Primera prueba hecha (11/11/2026)

**Servidor** (`objetivos/servidor/`):
- `compose.yml`: base de datos (Postgres), Redis, RabbitMQ, MinIO y la API, el worker y el beat de Plane, **construidos desde nuestra copia** (imagen `objetivos-plane-api:local`, sin telemetría). Sin la web de Plane. Los puertos solo escuchan en `127.0.0.1`: API en `8100` y MinIO en `9100`.
- `.env`: claves aleatorias. Fuera de git; se crea a partir de `.env.example`.
- `preparar.py`: crea el usuario local, el espacio de trabajo `objetivos` y una clave de API. Se ejecuta con `docker compose -p objetivos exec -T api python manage.py shell < preparar.py` y se puede repetir. La clave queda en `.plane-token`, fuera de git.
- Arrancar: `docker compose -p objetivos up -d --build`. Parar: `docker compose -p objetivos down` (los datos se quedan en los volúmenes).
- Ahora corre en el Docker Desktop de Windows. La integración de Docker Desktop con la distro de WSL está apagada; hará falta encenderla para que la app lo arranque y lo pare.
- Comprobado: ningún contenedor tiene conexiones hacia fuera del equipo, y la tarea de telemetría no aparece en los logs.
- Datos de prueba: proyecto «IDE Workspace» (`IDE`) con 7 issues.

**Vista** (`objetivos/vista/`), con su propio `package.json` (React 19, `@makeplane/propel` 0.9.7, Tailwind 4) para no mezclarse con el React 18 de Excalidraw:
- `build.mjs` → `ui/vendor/objetivos/objetivos.js`. Tailwind compila los estilos de Plane y esbuild resuelve `@plane/blocks/...` al código de `objetivos/plane/packages`. `@plane/utils` se sustituye por `src/plane-utils.ts` (solo `cn`).
- Va **dentro de un Shadow DOM**, para que el reset de Tailwind no toque el resto de la app. Los `@property` de Tailwind se registran en el documento, porque dentro del Shadow DOM no funcionan. El tema oscuro de Plane (`:host:where([data-theme=dark])`) no casa en un Shadow DOM, así que `build.mjs` lo aplica también al contenedor `.plane-root`.
- `build.mjs` obliga a usar el React 19 de `objetivos/vista/node_modules`. Si no, el código de `objetivos/plane/packages` encuentra antes el React 18 de la raíz de la app y la vista no pinta nada (React error #525).
- Para probarla sin la app: un servidor local que sirve el paquete y reenvía `/api/v1/` a Plane con la clave, más Edge en modo headless (`--screenshot`).
- `src/api.ts`: nuestra capa de datos (proyectos, estados, issues, cambiar el estado).
- `src/lista.tsx`: la lista de issues agrupada por estado, con el marcado y las clases de la lista de Plane y sus piezas reales (`Row`, `StateGroupIcon`, `PriorityIcon`). Se puede cambiar el estado de un issue desde la lista.

**App:**
- `src-tauri/src/plane.rs`: comandos `plane_info` y `plane_request`. La clave está en `~/.ide-workspace-chat/plane.json` (permisos 600) y nunca llega a la página. Solo se permiten rutas `/api/v1/`.
- `ui/index.html`: si Plane está configurado, la pestaña Objetivo carga la vista la primera vez que se abre y sigue el tema claro u oscuro. Si no lo está, se ve la vista de IDE-29.

Falta: la fuente Inter (ahora usa la del sistema), el detalle del issue, crear issues, el tablero kanban… y que el paquete de la vista se genere al instalar la versión estable (`objetivos/plane` no está en git).

## Por qué Plane no puede ser un paquete, como Excalidraw

- **Excalidraw** es un paquete de npm (`@excalidraw/excalidraw` 0.18.1 en `package.json`). `pizarra/entry.jsx` lo monta y `npm run vendor` (`scripts/pizarra.mjs`) lo empaqueta con esbuild en `ui/vendor/pizarra/`. Se puede hacer así porque Excalidraw está pensado como librería.
- **Plane** es una aplicación completa:
  - `apps/`: `api` (Django), `web` (la app principal), `admin`, `space` (la parte pública), `live` (edición colaborativa en tiempo real) y `proxy`.
  - Servicios, según `docker-compose.yml`: Postgres 15, Valkey (equivalente a Redis), RabbitMQ, almacenamiento de archivos (MinIO) y procesos en segundo plano (`worker`, `beat-worker`, `migrator`).
  - `packages/` (`editor`, `blocks`, `types`, `services`, `utils`…): **todos privados**, Plane no los publica en npm.
- Por eso Plane tiene que correr **como servicio aparte** (Docker en WSL). La app lo muestra en la pestaña Objetivo y habla con su API. No cabe dentro de la app (Tauri, unos 4 MB).
- «Como módulo» sí tiene sentido en git: más adelante, un **submódulo** `objetivos/plane` para que nuestro repo guarde qué versión de Plane usamos.

### Opciones que se valoraron

| Opción | Qué es | Coste |
| --- | --- | --- |
| A. Nuestra copia de Plane como servicio más un adaptador en la app | Plane autoalojado y adaptado (por ejemplo, objetivos y checklists de las sesiones, o qué agente hizo cada cosa). La pestaña Objetivo se sincroniza con él por su API. | Medio: hay que mantener una copia de un proyecto grande |
| **B. Mostrar Plane dentro de la app** (la elegida) | A, más la vista de Plane en la pestaña | Bajo una vez hecho A |
| C. Copiar trozos de Plane en la app | Su modelo de datos o su interfaz | Alto y no compensa: Plane está hecho en React y la app no |

**Decisión del usuario (11/11/2026): opción B, directamente.** *Sustituida ese mismo día por la opción 3 reutilizando el código de Plane (ver «Vía elegida»).* Todo muy integrado en la app y controlado desde dentro:

- La pestaña Objetivo **es** Plane: su interfaz se muestra dentro de la app, sin ir al navegador.
- La app **controla Plane**: lo arranca y lo para (los contenedores de Docker en WSL) y muestra si está funcionando, sin que el usuario tenga que hacerlo a mano.
- Lo que la app y los agentes necesiten de Plane (`agentes tareas`, `/objetivo`, el estado de los tickets) pasa por su API, que es la parte de la opción A que B incluye.
- Plane sigue siendo un servicio aparte por dentro, pero el usuario no lo ve como algo separado.

## Más formas de integrarlo

La opción B sigue en pie. Estas son otras formas, que se pueden combinar entre sí, incluidas las que modifican el código de Plane.

### Lo que hay en el código y condiciona la integración

- **Web:** React Router 7 y MobX. Editor Tiptap 3 y edición colaborativa con Hocuspocus (`apps/live`).
- **Sesión:** cookie `session-id`, solo HTTP. Django tiene activado `XFrameOptionsMiddleware`. Si la página no es https, las cookies se marcan como no seguras.
- **API pública:** `/api/v1/` (`apps/api/plane/api`) con clave en la cabecera `X-Api-Key`.
- **Webhooks:** existen (`bgtasks/webhook_task.py`), pero **bloquean las IPs privadas**, `localhost` incluido (`utils/url_security.py`), salvo los hosts marcados como de confianza.
- **Base de datos:** depende de Postgres (unos 24 archivos usan funciones propias de Postgres), así que SQLite no sirve.
- **Imagen «todo en uno»** (`deployments/aio`): mete las aplicaciones en un contenedor, pero sigue necesitando Postgres, Redis, RabbitMQ y S3 aparte.

### Cómo mostrarlo en la app

| Forma | Cómo | Pros | Contras |
| --- | --- | --- | --- |
| **1. Iframe en la pestaña** (la B tal cual) | `<iframe src="http://localhost:…">` | Lo más rápido | La app (`tauri://localhost`) y Plane son orígenes distintos: las cookies de sesión no llegan al iframe salvo con `SameSite=None; Secure`, que exige https. Además hay que quitar `X-Frame-Options`. Hay que **tocar la configuración de Plane** y servirlo por https. |
| **2. Webview nativa de Tauri** | Una segunda webview en la ventana, colocada sobre la zona de la pestaña, que carga Plane directamente (Tauri 2, función de varias webviews) | Plane funciona como si fuera la ventana principal: cookies normales, sin restricciones de frames. La app le puede inyectar un script para comunicarse con él. | Hay que mover y redimensionar la webview a mano al cambiar de pestaña o de tamaño. Esa función de Tauri todavía es «inestable». |
| **3. Interfaz propia sobre la API de Plane** (Plane sin su interfaz) | Plane solo como servidor; la pestaña la pintamos nosotros, en JS como el resto de la app, usando `/api/v1/` | Integración total, con el mismo aspecto que la app y lo justo que necesitamos | Se pierde la interfaz de Plane (tableros, ciclos, editor…). Hay que hacer y mantener la nuestra. |
| **4. Trozos de Plane como librería** | Empaquetar piezas de su código con esbuild, como hicimos con Excalidraw | Su editor (`packages/editor`, Tiptap) es bastante independiente y se podría aprovechar | Las vistas de issues están en `apps/web`, atadas a sus stores de MobX y a su router: sacarlas cuesta mucho. Solo es viable para el editor. |

### Modificaciones a Plane para que encaje

| Forma | Qué se cambia | Para qué |
| --- | --- | --- |
| **5. Modo integrado en su web** | Un parámetro (por ejemplo `?embed=1`) que quita la barra lateral, la cabecera y el selector de espacios de trabajo, y deja solo el proyecto o la vista del proyecto abierto. Que use el tema claro u oscuro de la app. | Que no parezca otra aplicación metida dentro |
| **6. Entrada automática** | Un endpoint nuevo en su API que, con un secreto local que solo conoce la app, abre sesión sin pedir usuario ni contraseña. Un único usuario local. | No tener que iniciar sesión en Plane nunca |
| **7. Comunicación en los dos sentidos** | Desde la app hacia Plane: un script inyectado (forma 2) o `postMessage` (forma 1). Desde Plane hacia la app: abrir un archivo del proyecto, mandar un issue al chat o lanzar un agente. | Que Plane y el chat funcionen como una sola cosa |
| **8. Avisos de Plane a la app** | Webhooks hacia el servidor local de la app (marcándolo como host de confianza) o una señal de Django que llame a la app directamente | Por ejemplo: pasas un issue a «En curso» asignado a un agente y la app lanza ese agente; cuando termina, el issue se marca solo, igual que los avisos de IDE-25 |
| **9. Modelo de datos propio** | Sin tocar código: carpeta del proyecto = proyecto de Plane, conversación = ciclo o módulo, checklist = sub-issues, agente = usuario «bot» asignado. Tocando código: campos o modelos nuevos en Django (conversación, sesión de la CLI, agente) con sus endpoints. | Que Plane sepa de conversaciones y de agentes |
| **10. Plane más ligero** | RabbitMQ fuera (Celery con Redis como cola), archivos en disco local en vez de S3/MinIO (Plane sube por URLs firmadas de S3, así que hay que cambiar código), y fuera `space`/`admin`/`live` si no se usan. Incluso sin Docker: Django y Node directamente en WSL con un Postgres local. | Que arranque rápido, consuma menos y la app lo controle sin dificultad |

### Cómo usarían Plane los agentes

- **`agentes tareas` hablando con la API de Plane** (la idea actual): mismo comando, pero los datos viven en Plane.
- **Servidor MCP de Plane:** creo que Plane tiene uno oficial, pero **está sin comprobar**. Los agentes que hablan MCP (Claude, Codex…) lo usarían directamente.
- **Descartada:** que la app lea y escriba directamente en la base de datos de Plane. Se saltaría su lógica (permisos, historial, avisos) y se rompería con cada cambio de esquema.

### Combinación que parece más completa

Partir de la B y sumarle: **2** (webview nativa) + **5** (modo integrado) + **6** (entrada automática) + **7** y **8** (comunicación en los dos sentidos) + **9** (modelo de datos, primero sin tocar código). Dejar la **10** para cuando el consumo moleste, y la **3** como alternativa si la interfaz de Plane acaba sobrando.

## Licencia: AGPL-3.0

Lo confirma `LICENSE.txt`. Plane se puede modificar como queramos. Si alguna vez se diera acceso a nuestra versión modificada a otras personas por la red, habría que publicar su código. Para uso interno no hay ninguna obligación más.

## Dónde está y cómo se preparó

- Carpeta: `objetivos/plane/`. Está en el `.gitignore` de este repo (comentario «Plane (IDE-30)») hasta que pase a ser submódulo.
- Es un **repo de git independiente**, sin remoto, sin ramas ni etiquetas de Plane y sin su historial:
  - `1b96587 Plane v1.5.0-rc1: punto de partida`: el código tal cual estaba en `v1.5.0-rc1` (commit original `ea63ca8ab`).
  - `de2689d Nada sale del equipo…`: los cambios de abajo.
- Se clonó en modo parcial (`--filter=blob:none`). Quitar solo el remoto lo habría dejado roto, porque le faltaban objetos del historial y ya no podría descargarlos. Por eso se rehízo con un único commit inicial. Si algún día hace falta el original, se vuelve a clonar.
- **Saltos de línea:** el git de Windows tiene `core.autocrlf=true` y había convertido todo a CRLF. Así fallarían los scripts y entrypoints en Docker sobre WSL. Se normalizó todo a LF, y este repo tiene su propia configuración (`core.autocrlf=false`, `core.eol=lf`) para que no vuelva a pasar.
- Ocupa unos 131 MB de código y unos 46 MB de `.git`.

## Cambios hechos en nuestra copia (`de2689d`)

### Archivos de su proyecto, quitados

`.github/` (workflows de CI, plantillas de issues y de PR), `CODEOWNERS`, `CONTRIBUTING.md` y `SECURITY.md`.

### Telemetría y salidas automáticas, desactivadas

| Qué hacía | Dónde | Cambio |
| --- | --- | --- |
| Cada 5 minutos mandaba métricas a `telemetry.plane.so`: el id de la instancia y el número de usuarios, espacios de trabajo, proyectos, issues, ciclos, módulos y páginas. **Activada por defecto.** | `apps/api/plane/license/bgtasks/telemetry_metrics.py`, `apps/api/plane/celery.py`, `apps/api/plane/utils/otlp_endpoints.py` | La tarea ya no se programa y, aunque alguien la ejecute, no envía nada. Sin colector por defecto (`localhost`). |
| La instalación dejaba la telemetría activada | `apps/api/plane/license/api/views/admin.py`, `apps/admin/components/instance/setup-form.tsx` | Apagada por defecto |
| Al registrar la instancia consultaba la API de GitHub por la última versión | `apps/api/plane/license/management/commands/register_instance.py` | No consulta nada: usa la versión actual |
| La ventana de novedades cargaba una página de `sites.plane.so` | `apps/api/Dockerfile.api`, `apps/api/Dockerfile.dev` (`INSTANCE_CHANGELOG_URL`) | Vacía: Plane muestra su mensaje alternativo |

Estos cambios solo se han comprobado compilando los archivos Python. Plane todavía no se ha arrancado.

### Salidas que solo existen si se configuran (apagadas por defecto)

- Imágenes de Unsplash (necesita `UNSPLASH_ACCESS_KEY`).
- Inicio de sesión con GitHub, GitLab o Google.
- Funciones de IA (necesita una clave de OpenAI).
- Correo por SMTP.
- Certificados de Let's Encrypt (solo si se pone un dominio público en el proxy Caddy).

`apps/api/templates/base.html` carga Bootstrap y jQuery de CDNs, pero no lo usa ninguna vista.

## Pendiente

### Para que no cargue nada de internet

- **Editor:** las notas destacadas («callout») usan por defecto un emoji que es una imagen de `cdn.jsdelivr.net` (`packages/editor/src/extensions/callout/utils.ts`). Hay que servir los emojis en local.
- **Datos de ejemplo:** `apps/api/plane/seeds/data/*.json` llevan imágenes de jsdelivr y Unsplash. Desactivarlos o cambiarlos por imágenes locales.
- **Enlaces de ayuda y documentación** a `plane.so`, `docs.plane.so`, `go.plane.so`, `forum.plane.so` y `status.plane.so` en la interfaz. Solo se abren si haces clic; se cambian al adaptar la interfaz.
- **Instalar y compilar** descarga paquetes (pnpm, pip) e imágenes de Docker. Es inevitable la primera vez; después funciona sin conexión.
- Al levantarlo, **vigilar desde WSL qué conexiones abren los contenedores** y comprobar que nada sale fuera.

### Siguientes pasos

1. Levantar Plane en local con Docker en WSL (solo como servidor) y hacer la prueba de la lista de issues con sus estilos dentro de la pestaña Objetivo. Después, que la app lo arranque y lo pare sola.
2. Convertir `objetivos/plane` en submódulo (hace falta un repo propio donde guardarlo; crearlo es decisión del usuario).
3. Actualizar IDE-30 en Jira con este enfoque (opción 3 reutilizando su código): Plane autoalojado solo como servidor, la pestaña Objetivo hecha con su interfaz y nuestra capa de datos, y `agentes tareas` conectado a su API.
4. Añadir `LICENSE` (AGPL-3.0) al repo de la app.
5. Decidir qué se queda de IDE-29 (ver abajo).

## Estado de IDE-29 (vista Objetivo actual)

- Integrado en `main` (merge `bedf7c4`). **No está instalado en la versión estable**, que sigue en `6da0e72`.
- Qué incluye:
  - `src-tauri/src/objetivo.rs`: objetivo y checklist por conversación en `~/.ide-workspace-chat/objetivos/<conversación>.json`.
  - `agentes tareas`: `--objetivo`, `--añadir … [--ticket X]`, `--hecha n`, `--pendiente n` y `--quitar n`.
  - El comando `/objetivo <texto>` y la pestaña Objetivo.
- En la revisión se arregló que la pestaña podía borrar lo que un agente añadía a la vez: ahora cada cambio se aplica como una operación sobre lo guardado, con bloqueo (`objetivo::apply`).
- Comprobado: tests (Rust 92, JS 33) y prueba de punta a punta de `agentes tareas` (6 agentes añadiendo a la vez sin perder ninguno).
- En Jira está **En curso**, con un comentario de lo hecho.
- La pestaña se sustituirá por Plane. `agentes tareas` y `/objetivo` pueden quedarse y pasar a leer y escribir en Plane, para que los agentes sigan marcando lo que terminan.
