# Plan: pestañas en el panel del chat y pestaña «Uso»

Estado: **decidido el 02/11/2026**: estadísticas locales y límites por red, todo
en Rust y para cualquier sistema (como Codenotch), pestañas Chat y Terminal, y
pantalla de inicio sin carpeta. Lo implementan Codex (backend de uso) y Claude
(el resto, y lo que Codex deje a medias).

## Lo que se pidió

> En el panel izquierdo, el del chat, quiero pestañas también. Una pestaña de
> terminal, y chat por ahora […]. Y en el panel de la derecha, al lado de
> Consejo, quiero una pestaña de usage tal y como está esto [widget de uso de
> Omarchy], pero en una sola vista y de todos los CLI instalados.

## 1. Pestañas en el panel izquierdo: Chat · Terminal

**Estructura.** Una barra de pestañas igual que la del panel derecho (`.tab`,
`role="tablist"`), pero genérica: cada pestaña registra `{id, título, montar(),
mostrar(), ocultar()}`. Así, añadir otras más adelante no obliga a tocar el
chat ni el terminal.

**Chat.** Es el panel de ahora, sin cambios. Es la pestaña por defecto.

**Terminal.** Una shell interactiva de verdad (`$SHELL -l`; en Windows,
`pwsh`/`cmd`) abierta en la carpeta del proyecto.

- Reutiliza `term.rs`, que ya abre un PTY real, y el xterm.js que ya está en
  `ui/vendor`. Solo falta un comando `term_shell(cols, rows)` que lance la
  shell con `cwd = proyecto actual`, en vez de `home()` como los
  instaladores (`lib.rs:381`).
- El PTY sigue vivo al cambiar de pestaña: ocultar no mata la shell.
- Una sola terminal por proyecto en esta fase. Varias terminales con «+» se
  dejan para después.
- Al cambiar de proyecto, la terminal anterior se cierra (con aviso si tiene
  un proceso en marcha) y se abre una nueva en la carpeta nueva.
- Si sale la shell (`exit`), aparece «Proceso terminado · Abrir otra».
- La terminal tiene el mismo entorno que los agentes (`child_env()`).

**Otras pestañas (sugerencias, fuera de alcance por ahora).** La estructura
genérica tiene que admitirlas sin rehacerse:

- **Varios chats a la vez**: una pestaña por conversación o agente, para
  tener Claude y Codex trabajando en paralelo.
- **Varias terminales**: servidor de desarrollo, tests, logs… («+» en la barra).
- **Vista previa**: un `localhost` del proyecto dentro de la app.
- **Cambios**: el `git diff` del proyecto con los archivos que ha tocado cada
  agente en la conversación.
- **Tareas**: la lista de tareas (`todo`) del agente activo, siempre a la
  vista.

## 2. Pestaña «Uso» en el panel derecho

Va al lado de Consejo: `Worktree · Archivo · Consejo · Uso`. Es una sola vista
con una tarjeta por CLI instalada (Claude, Codex, OpenCode, Antigravity,
Gemini, Copilot), en vez del selector de un agente cada vez que tiene el
widget.

Cada tarjeta lleva:

- Cabecera con el logo, el nombre y el plan («Team», «Go», «Pro»…), o el
  aviso de estado («Sin sesión · `claude auth login`»).
- **Límites**: una barra por ventana (sesión de 5 h, semanal, mensual), con el
  % y «se reinicia en 4 h 40 min».
- **Hoy**: los tokens, y los prompts y sesiones si la CLI los registra.
- **7 días**: un minigráfico de barras, compacto para que quepan varias
  tarjetas.
- **Por modelo**: los 3 modelos con más uso, y el resto plegado.
- Si una CLI está instalada pero no deja datos de uso: «Sin datos de uso»,
  sin inventar cifras.

**Refresco:** al abrir la pestaña, cada 60 s mientras está visible y con un
botón ↻. Con la pestaña oculta no se consulta nada.

### De dónde salen los datos: todo en Rust, en cualquier sistema

Decidido el 02/11/2026: **la app no depende de Omarchy** (que solo existe en
Linux). Todo se recoge en Rust con el mismo enfoque que **Codenotch**, que en
Windows también es Tauri 2: estadísticas desde los archivos locales de cada
CLI y límites desde los endpoints que usan las propias CLIs. Los colectores de
Omarchy (`/usr/share/omarchy/bin/omarchy-agent-usage-*`) sirven solo como
**referencia de los formatos**, no como dependencia.

Módulo `src-tauri/src/usage/`: `mod.rs` (contrato y comando), `local.rs`
(estadísticas de los archivos locales, fase 2), `limits.rs` (límites por red,
fase 3) y un archivo por agente si crece.

### Contrato con el frontend (cerrado)

Comando `get_usage(force: bool) -> Vec<UsageRecord>`. Devuelve una entrada
por cada agente **instalado** (Claude, Codex, OpenCode, Antigravity, Gemini,
Copilot, en ese orden). JSON en camelCase:

```jsonc
{
  "id": "claude",            // el mismo id que list_agents
  "name": "Claude",
  "plan": "Team",            // o null
  "status": "ok",            // ok | no_session | expired | unavailable | no_data
  "statusText": null,        // texto para el usuario si status != ok
  "help": null,              // p. ej. "Ejecuta `claude auth login`"
  "limits": [ { "label": "Sesión (5 h)", "percent": 0.10, "resetsAt": "2026-11-03T01:40:00Z" } ],
  "today": { "tokens": 80500000, "prompts": 120, "sessions": 6 },   // prompts y sessions pueden ser null
  "days": [ { "date": "2026-10-27", "tokens": 141900000 } ],        // 7 días, terminando hoy
  "models": [ { "model": "claude-opus-5-5", "tokens": 501900000 } ],// 7 días, de mayor a menor
  "updatedAt": "2026-11-02T21:00:00Z"
}
```

`percent` va de 0 a 1. Si no hay límites, `limits` es `[]`, sin inventar
cifras. Sin `force`, el backend devuelve la caché si tiene menos de 60 s.

### Fase 2: estadísticas locales (sin red)

| Agente | Archivos (solo lectura) |
|---|---|
| Claude | `~/.claude/projects/**/*.jsonl` (o `$CLAUDE_CONFIG_DIR`): `message.usage` y `message.model` de las líneas `assistant`, sin duplicar por `message.id` + `requestId` |
| Codex | `~/.codex/sessions/AAAA/MM/DD/rollout-*.jsonl` (o `$CODEX_HOME`): eventos `token_count` |
| OpenCode | `~/.local/share/opencode/opencode.db` (en cada sistema, su ruta de datos): `session_message`, más las filas `message` de v1 |
| Gemini, Copilot, Antigravity | Lo que registre cada una (`~/.gemini/tmp`, `~/.copilot`, `~/.gemini/antigravity-cli`). Si no hay tokens, `status: "no_data"` |

Las rutas se resuelven con `dirs` en cada sistema. Solo se leen los últimos 7
días, por la fecha de modificación de cada archivo, para que no tarde con
historiales grandes.

### Fase 3: límites por red

Ver §5.

## 3. Pantalla de inicio sin carpeta

Hoy la app siempre tiene un proyecto. Desde el menú usa el último; desde una
terminal, la carpeta actual. Por eso en desarrollo abría `src-tauri`.

**Nuevo comportamiento:**

- Con `agentes <carpeta>` o la ruta como argumento, se abre esa carpeta, como
  ahora.
- Lanzada desde una terminal fuera de `$HOME` y de `/`, se abre la carpeta
  actual, como ahora, **salvo** si es la carpeta `src-tauri` del propio repo
  de la app (comparando con `CARGO_MANIFEST_DIR` en builds de depuración).
- En cualquier otro caso, incluido abrirla desde el menú, **no hay proyecto**:
  - aparece una pantalla de inicio en el panel del chat con «Abrir carpeta…»
    y la lista de **proyectos recientes** (los últimos 10, sin los que ya no
    existen);
  - el campo de mensaje y «Enviar» están desactivados, y el menú ☰ solo ofrece
    Proyecto y Agentes;
  - el panel derecho muestra Uso, que no depende del proyecto, y en las demás
    pestañas «Abre una carpeta»;
  - la pestaña Terminal abre en `$HOME`.
- `last-project` pasa a ser `recent-projects.json` (una lista). Si existe el
  archivo viejo, se migra.

**Backend:** `CurrentProject` guarda `Option<PathBuf>`. Todos los comandos
que necesitan carpeta devuelven el error `"Sin proyecto"` si no hay ninguna,
en vez de usar `home()`.

## 4. Criterios de aceptación

- El panel izquierdo tiene las pestañas Chat y Terminal. La terminal abre en
  la carpeta del proyecto, sobrevive al cambio de pestaña y se reabre al
  cambiar de proyecto.
- La pestaña Uso muestra una tarjeta por cada CLI instalada. En esta máquina,
  los tokens de hoy y de los 7 días de Claude y Codex se parecen a los del
  widget de Omarchy (±5 %: cada uno deduplica a su manera).
- Al abrir la app sin argumentos, desde el menú o con `npm run dev`, sale la
  pantalla de inicio. Al elegir una carpeta, todo funciona como hoy.
- Los límites de Claude, Codex y OpenCode salen **sin Omarchy** y coinciden
  con los del widget de Omarchy en esta máquina.
- Hay tests en `cargo test` para cada analizador (con ejemplos guardados) y
  para que el token no se filtre. Si falta un archivo o una CLI, no hay
  errores: el estado es `no_data` o `no_session`.
- El README documenta las pestañas y de dónde sale cada dato.

## 5. Fase 3: límites por red (diseño de seguridad, como Codenotch)

**El riesgo:** para leer los límites hay que usar el token de sesión que cada
CLI guarda en tu equipo. Un fallo aquí podría filtrarlo, cerrar la sesión de
la CLI o mandar el token a un servidor equivocado.

**Reglas comunes (`usage/limits.rs`):**

1. **Nunca se renueva un token desde la app.** Si el token de Claude ha
   caducado o caduca en menos de 5 minutos, se ejecuta la propia CLI
   (`claude -p "ok"`, oculta, con 60 s de límite) para que lo renueve ella,
   como hace Codenotch. La app no lee el `refreshToken`. Las demás CLIs no se
   renuevan: aparece «Sesión caducada · abre `<cli>`».
2. **El token solo vive en memoria**, dentro de la función que hace la
   petición. No va al frontend, no se guarda en disco y no aparece en logs ni
   en mensajes de error (solo el código HTTP, nunca el cuerpo). El tipo que lo
   guarda no implementa `Debug` ni `Serialize`.
3. **Lista cerrada de destinos:** `api.anthropic.com`, `opencode.ai` y
   `api.github.com`. Solo HTTPS, sin seguir redirecciones, con 10 s de límite.
   Si una credencial apunta a otro host, no se envía nada.
4. **Frecuencia:** como mucho un intento por agente cada 5 minutos, también al
   pulsar ↻ (como Codenotch). Ante un 429 se respeta `Retry-After`, y el plazo
   se guarda en `data_dir()/usage-backoff.json` (solo los plazos, nunca el
   token) para que sobreviva a reiniciar la app.
5. **Archivos de otras CLIs, solo para leer:** SQLite abierto con
   `SQLITE_OPEN_READ_ONLY` (tolerante al WAL, con `busy_timeout` de 50 ms) y
   JSON leído sin bloquearlo.

**Por agente:**

| Agente | Credencial (solo lectura) | Petición |
|---|---|---|
| Claude | `~/.claude/.credentials.json` → `claudeAiOauth.accessToken`, `expiresAt`, `subscriptionType`. macOS: `security find-generic-password -s "Claude Code-credentials" -w`. Windows: el mismo JSON en `%USERPROFILE%\.claude` | `GET https://api.anthropic.com/api/oauth/usage` con `anthropic-beta: oauth-2025-04-20` → `five_hour`, `seven_day` |
| Codex | **No se toca su token.** Se lanza `codex -s read-only -a on-request app-server` (oculto, 20 s) y se habla JSON-RPC: `initialize` → `initialized` → `account/rateLimits/read`. Si falla: la última instantánea `rate_limits` de los `rollout-*.jsonl` locales | — |
| OpenCode (Go) | `opencode.db` → `account` + `account_state`, o la fila `credential`; o bien la clave `opencode-go` de `auth.json` / `OPENCODE_APIKEY` | `GET https://opencode.ai/api/go/status` con el org en su cabecera (formato: colector de Omarchy) |
| Copilot | `GH_TOKEN`/`GITHUB_TOKEN` → `oauth_token` de `hosts.yml` de gh → `gh auth token` | `GET https://api.github.com/copilot_internal/user` |
| Antigravity, Gemini | — | Sin límites en esta fase (Antigravity solo da `/usage` en su TUI) |

**Tests:** el análisis de cada respuesta se prueba con ejemplos guardados en
`src-tauri/tests/fixtures/usage/`. La red va detrás de un *trait* para
simularla. Además, un test comprueba que ningún error ni ningún
`UsageRecord` serializado contiene el token de un ejemplo.

## Fuera de alcance

- Varias terminales a la vez y dividir la terminal.
- Las pestañas sugeridas en §1.
