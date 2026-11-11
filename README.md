# Chat de agentes

App de escritorio (macOS, Windows y Linux) para chatear con los agentes CLI: Claude, Codex, OpenCode, Antigravity (agy), Copilot y Gemini. Hecha con [Tauri](https://tauri.app) y Rust: el instalador pesa unos 4 MB.

Usa las CLIs y las sesiones que ya tengas en tu ordenador. Si un agente no está instalado o no tiene la sesión iniciada, la propia app te deja instalarlo e iniciar sesión.

## Instalar

- **macOS**: abre el `.dmg` y arrastra «Chat de agentes» a Aplicaciones. La app no está firmada: si macOS la bloquea, ábrela con clic derecho → Abrir.
- **Windows**: ejecuta el instalador `.msi` o `.exe`.
- **Linux**: `.deb`, `.rpm` o `.AppImage`.

Los instaladores se generan en GitHub Actions (pestaña Actions → Build → Run workflow) o en local con `npm run build`. Cada sistema tiene que compilar el suyo.

**Versión estable en WSL** (una copia fija para trabajar a diario, que no cambia con el repo): cómo abrirla, actualizarla y volver atrás en [docs/version-estable-wsl.md](docs/version-estable-wsl.md).

## Abrir en un proyecto

Como `claude` o `code .`: abre la app desde la carpeta de un repo y los agentes trabajarán ahí.

```sh
cd ~/workspace/mi-repo
agentes            # abre la app en la carpeta actual
agentes ../otro    # o en otra carpeta
```

El comando `agentes` se instala desde la app: **☰ → Agentes → Instalar comando «agentes»**. En macOS y Linux se instala en `~/.local/bin`; en Windows, en una carpeta que ya está en el PATH.

- Cada carpeta abre su propia ventana, con el nombre del proyecto en el título.
- Cada proyecto tiene sus propias conversaciones.
- Si abres la app desde el Finder o el menú (sin carpeta), sale la pantalla de inicio: **Abrir carpeta…** y la lista de proyectos recientes. Hasta que eliges una, el chat está desactivado.
- Para cambiar de proyecto sin cerrar la app: **☰ → Proyecto**, pulsa la carpeta y elige otra.

## Uso

La cabecera muestra el proyecto y el agente activos («mi-repo · Claude»). Todo lo demás está en el menú **☰** de la izquierda:

- **Proyecto**: la carpeta donde trabajan los agentes. Al pulsarla puedes elegir otra.
- **Agente**: elige el agente. Debajo se ve su estado (**Listo**, **Sin sesión** o **No instalado**) con el botón para arreglarlo. **Agentes** muestra todos con su estado, como un panel de cuentas.
- **Conversaciones**: «Nueva conversación», «Convocar consejo» y la lista de conversaciones del proyecto.
- **Vista**: mostrar u ocultar el panel de archivos.

El menú se cierra al elegir algo, al pulsar fuera o con Esc.

El panel del chat tiene dos pestañas:

- **Chat**: la conversación con el agente.
- **Terminal**: tu shell (`$SHELL`; PowerShell en Windows) abierta en la carpeta del proyecto. Sigue viva al cambiar de pestaña y se abre de nuevo al cambiar de proyecto.

Las respuestas de los agentes se ven en Markdown: tablas, listas y bloques de código con colores y botón para copiar. Los enlaces web se abren en el navegador y las rutas del proyecto, en el panel de archivos.

**Cada agente sigue su propia sesión.** En una conversación, cada CLI reanuda su sesión (`--resume` y equivalentes) en vez de empezar de cero: recuerda lo que leyó y aprovecha la caché, y un mensaje de seguimiento a Claude cuesta unas 20 veces menos. Si cambias de agente, al volver recibe solo lo que se habló sin él. Detalles en `docs/plan-reanudar-sesiones.md`.

**Instalar** e **Iniciar sesión** abren un terminal dentro de la app con el instalador oficial o el login de cada CLI. Los enlaces del terminal se abren en el navegador con un clic.

| Agente | Instalación | Inicio de sesión |
| --- | --- | --- |
| Claude | instalador oficial (`claude.ai/install.sh` o `.ps1`) | `claude auth login` |
| Codex | `npm install -g @openai/codex` | `codex login` |
| OpenCode | `npm install -g opencode-ai` | `opencode auth login` (tiene modelos gratuitos sin sesión) |
| Antigravity | instalador oficial (`antigravity.google/cli`) | `agy` |
| Gemini | `npm install -g @google/gemini-cli` | `gemini` |
| Copilot | `npm install -g @github/copilot` | `copilot login` (también vale la sesión de `gh`) |

Codex, OpenCode, Gemini y Copilot se instalan con npm, así que necesitan Node.js.

### Comandos «/»

Al escribir `/` al principio del mensaje se abre la lista de comandos, que se filtra según escribes. ↑/↓ para moverte, Tab o Enter para elegir, Esc para cerrar. Cada comando vale solo para ese mensaje: no cambia la forma de trabajar elegida en el menú. En la conversación se guarda lo que escribiste, con el comando resaltado.

| Comando | Qué hace |
| --- | --- |
| `/equipo <tarea>` | Reparte la tarea entre varios agentes en worktrees y los lanza ya, sin pedir aprobación del plan |
| `/consejo <tema>` | Convoca un consejo de agentes (abre el diálogo) |
| `/diagrama <qué dibujar>` | Dibuja un diagrama en Mermaid y lo abre en la pizarra |
| `/pizarra [pregunta]` | El agente lee la pizarra abierta y la tiene en cuenta |
| `/objetivo <objetivo>` | Fija el objetivo de la sesión y el agente lo divide en una checklist (pestaña **Objetivo**) |
| `/maqueta <tema>` | Maqueta HTML en `docs/maquetas/` |
| `/directo <mensaje>` | Solo este mensaje, sin forma de trabajar |

También salen tus agentes propios activos (`/<id> <mensaje>`) y tus plantillas de consejo (`/consejo-<plantilla>`). Un `/texto` que no sea un comando se envía como mensaje normal.

### Objetivo de la sesión

Cada conversación puede tener un objetivo y una checklist, en la pestaña **Objetivo** del panel derecho. Es opcional: sin objetivo, la pestaña dice «Sin objetivo» y no molesta.

- Escribe el objetivo en una línea, añade puntos, márcalos o quítalos. Cada punto puede llevar un ticket (`IDE-23` o una URL, que se abre en el navegador).
- `/objetivo <texto>` en el chat: el agente guarda el objetivo, lo divide en pasos y te enseña la checklist sin empezar todavía.
- Los agentes la leen y la mantienen al día con `agentes tareas` (`--objetivo "…"`, `--añadir "…" [--ticket X]`, `--hecha <n>`, `--pendiente <n>`, `--quitar <n>`): añaden lo que descubren y marcan lo que terminan. La pestaña se refresca sola.
- Cada cambio, tuyo o de un agente, se aplica sobre lo guardado, así que no se pisan aunque trabajen a la vez. Se guarda en `~/.ide-workspace-chat/objetivos/<conversación>.json` y se conserva al reabrir la conversación.

## Uso de los agentes

La pestaña **Uso** del panel derecho muestra en una sola vista una tarjeta por cada CLI instalada:

- **Límites**: el porcentaje gastado de cada ventana (sesión de 5 h, semana…) y cuándo se reinicia.
- **Tokens por día**: los últimos 7 días.
- **Por modelo**: los modelos más usados en la semana.

Todo se calcula en la app (Rust), en macOS, Windows y Linux: las estadísticas salen de los archivos locales de cada CLI y los límites, de los mismos servicios que usan las CLIs. La app nunca modifica ni renueva los tokens de sesión: si uno caduca, te pide abrir la CLI. Los detalles están en [`docs/plan-pestanas-y-uso.md`](docs/plan-pestanas-y-uso.md).

## Varias cuentas por CLI

Puedes tener varias cuentas de la misma CLI (por ejemplo, dos de Claude Team o dos de Codex) y usarlas a la vez. En **☰ → Agentes**, escribe un nombre y pulsa **+ Cuenta**: se abre el inicio de sesión de esa CLI en una carpeta propia. Las cuentas no comparten nada con la principal: ni sesión, ni historial, ni configuración.

- **Cuenta auto** (por defecto, en el chat y en `agentes run`): se usa la cuenta con más cuota libre. Por encima del 90 % de su límite, una cuenta no recibe trabajo nuevo.
- **Relevo:** si una cuenta se queda sin cuota a mitad de una respuesta o de un carril, se sigue con la siguiente cuenta con saldo, en el mismo worktree y el mismo panel. El agente nuevo recibe un brief de continuación con lo que ya está hecho. Si ninguna cuenta de esa CLI tiene cuota, se pasa a otra CLI.
- **Elegir una cuenta concreta:** en el menú, debajo del agente, o con `agentes run --account <nombre>`.
- La pestaña **Uso** muestra una tarjeta por cuenta.

Funciona con Claude (`CLAUDE_CONFIG_DIR`), Codex (`CODEX_HOME`), OpenCode (`XDG_DATA_HOME`) y Gemini (`GEMINI_CLI_HOME`). Antigravity y Copilot, de momento, solo con su cuenta principal. Diseño completo: [`docs/plan-cuentas.md`](docs/plan-cuentas.md).

## Ver lo que hace cada agente

Mientras un agente trabaja, su respuesta en el chat muestra en vivo cada paso, como en Claude Code:

- **Comandos** (`$ npm test`), con su salida (pulsa para desplegarla) y el código de salida si fallan.
- **Archivos** que lee o edita. Al pulsarlos se abren en el panel de archivos.
- **Búsquedas**, consultas web, subagentes, lista de tareas y lo que va pensando, si la CLI lo cuenta.
- Sus mensajes intermedios y la respuesta final.

El botón **Enviar** pasa a ser **Detener** mientras el agente trabaja. Los pasos se guardan con la conversación, así que se ven también al volver a abrirla.

Cada CLI informa de sus pasos en su propio formato (Claude `stream-json`, Codex `--json`, OpenCode `--format json`, Antigravity `stream-json`, Copilot `--output-format json`); la app los traduce a uno común (`src-tauri/src/stream.rs`). Gemini solo muestra el texto de la respuesta.

**Avisos al terminar.** Si el agente del chat lanza otros agentes (`agentes run`) o convoca un consejo (`agentes consejo`), no se queda esperando: termina su turno y puedes seguir escribiendo. Cuando terminan, la app manda a esa conversación un aviso con el código de salida y las últimas líneas del log, y el agente continúa solo. Si terminan varios mientras el chat está ocupado, llegan juntos en un solo aviso al acabar el turno. Si la conversación no está abierta, el aviso espera a que la abras. Se desactiva en ☰ → **Avisar al agente cuando terminen sus agentes**. `agentes wait` sigue existiendo para scripts.

## Consejo de agentes

Para decisiones con peso (una política, un ADR, una arquitectura), en lugar de preguntar a un solo agente puedes convocar un **consejo**: varios agentes opinan, se critican entre ellos y un moderador redacta la propuesta. Después vuelves al chat de un solo agente con la decisión como contexto.

Se convoca escribiendo `/consejo tema` en el chat, desde **☰ → Convocar consejo** o con **+** en la pestaña **Consejo**. Eliges:

- **Plantilla** (opcional): rellena el resto de campos con una combinación guardada. Ver «Roles y plantillas».
- **Tema** y **resultado**: ADR (`docs/adr/`), política (`docs/politicas/`) o nota de decisión (`docs/decisiones/`).
- **Participantes**, cada uno con un rol (arquitecto, abogado del diablo, seguridad, coste y operación, usuario y producto, libre, o los que hayas creado).
- **Moderador**: resume y redacta; mejor uno que no participe.
- **Contexto**: la conversación actual, que puedes editar antes de empezar.

También puedes pedírselo al agente del chat: «lanza un consejo sobre si conviene X». El agente lo convoca con `agentes consejo --tema "…"` (opcional: `--participantes claude,codex:abogado`, `--moderador`, `--resultado adr|politica|nota`, `--contexto archivo`; sin participantes usa hasta tres CLIs listas) y se abre en la pestaña **Consejo** igual que si lo hubieras convocado tú. Los participantes trabajan en solo lectura: no crean ramas ni escriben en el repo, y el documento solo se guarda cuando lo apruebas. El agente lee el resultado con `agentes consejo --estado <id>` para contestarte. Las formas de trabajar le piden que no simule un consejo lanzando agentes con `agentes run`.

Fases, en el tablero de la pestaña **Consejo** (se abre ampliado):

1. **Divergir**: cada agente da su propuesta sin ver la de los demás.
2. **Contrastar**: cada uno lee al resto y responde con apoyos, objeciones o preguntas, citando la tarjeta (C1, C2…).
3. **Converger**: el moderador resume acuerdos y puntos abiertos y redacta el documento.
4. **Evaluar** (si está activado): ver «Evaluador independiente».
5. **Revisión**: editas el documento si quieres y eliges **Aprobar y guardar**, **Otra ronda** o **Rehacer síntesis**. También puedes añadir tus comentarios, que cuentan en la siguiente ronda.

### Vista de pizarra

- La pizarra (Excalidraw) tiene su propia pestaña, **Pizarra**, en el panel derecho, con un selector arriba:
  - **Pizarra libre:** la tuya, una por proyecto.
  - **Consejo:** el tablero del consejo abierto, con las tarjetas de los participantes.
  - **Diagramas:** uno por pizarra, del más nuevo al más antiguo, con un botón para borrarlos.
- **Cualquier agente puede dibujar.** Pídele un diagrama, un esquema o un plan visual: lo escribe en Mermaid y lo manda con `agentes pizarra`. Cualquier bloque ` ```mermaid ` de una respuesta también llega, aunque la CLI no use el comando, y en el chat lleva el botón **Ver en pizarra**. El panel derecho cambia solo a la pestaña Pizarra y abre el diagrama. Los diagramas de flujo, de secuencia y de clases se convierten en formas editables; el resto de tipos entra como imagen.
- **Los agentes también la leen.** Antes de escribir un plan o de responder sobre algo dibujado, el agente consulta la pizarra:
  - `agentes pizarra --listar`: diagramas del proyecto (id y título).
  - `agentes pizarra --leer <id|título|proyecto>`: el código original del diagrama y la pizarra tal como está ahora, en texto (marcos, formas, textos, flechas con sus etiquetas…), con los cambios que hiciste a mano. `proyecto` es tu pizarra libre. No necesita la app abierta.
  - `agentes pizarra --imagen <id|título|proyecto> <ruta.png>`: la pizarra en PNG, para las CLIs que ven imágenes. Necesita la app abierta en ese proyecto.
  - Si tienes la pestaña Pizarra abierta al escribir en el chat, el agente sabe qué diagrama estás mirando.
- Cada tarjeta es un rectángulo coloreado por tipo (propuesta, riesgo, apoyo, objeción, pregunta, comentario), con columnas por agente, y las respuestas se unen con flechas.
- Se pueden mover, agrupar, conectar y dibujar libremente. Se guarda solo, por consejo.
- Las tarjetas nuevas aparecen en la pizarra mientras el consejo trabaja, sin mover lo que ya se ha colocado. Si borras una tarjeta de la pizarra, no vuelve a aparecer (en las columnas sigue estando).
- La pizarra se guarda aparte del consejo, en `~/.ide-workspace-chat/councils/boards/<id>.json` (o bajo `CHAT_DATA_DIR`).

### Coste y tiempo

- Cada llamada a un agente durante el consejo se anota con su duración y, si la CLI lo informa, tokens, coste en dólares y peticiones (Copilot). También las que fallan.
- El tablero muestra bajo las fases una línea «Uso del consejo» con el total; al pasar el ratón por encima se ve el desglose por agente. Cada columna muestra el uso de su agente y la de síntesis el del moderador.
- Qué informa cada CLI: Claude da tokens y coste; Codex da tokens; Copilot da peticiones premium; Antigravity, tokens si los incluye en su resultado; OpenCode solo en versiones que emiten el evento `step_finish` (las recientes no lo hacen). Gemini no da nada. Si una CLI no informa de un dato, no se muestra (no aparece como 0).

### Roles y plantillas

- Los roles no están fijos en el código: se editan en **☰ → Roles y plantillas** (crear, renombrar, cambiar la instrucción y quitar). Por defecto son los seis de siempre.
- Al convocar, el campo **Plantilla** rellena resultado, participantes con sus roles y moderador. Si algún agente de la plantilla no está disponible, lo avisa.
- **Guardar como plantilla**, en el mismo diálogo, guarda la selección actual con un nombre. Las plantillas se quitan desde **☰ → Roles y plantillas**.
- No se puede quitar un rol que use una plantilla: la app lo indica al guardar.
- Todo se guarda en `~/.ide-workspace-chat/council-config.json` (o bajo `CHAT_DATA_DIR`). Si el archivo falta o no es válido, se usan los roles por defecto.

### Evaluador independiente

Es el primer agente propio del IDE. Después de cada síntesis, un evaluador revisa con ojos nuevos si lo convergido es correcto: si responde al tema, recoge las objeciones importantes, no contradice el código y se puede aplicar.

- Corre sobre una **CLI elegida al azar** entre las disponibles. Nunca es el moderador y, si se puede, tampoco un participante. Si la CLI elegida falla (sin cupo, sin sesión…), prueba con la siguiente.
- Si le convence, **recomienda aprobar** y pasas a revisar.
- Si no, deja sus **objeciones como tarjetas** (columna «Evaluador») y **manda al consejo a otra ronda**: los participantes las responden, el moderador rehace la síntesis y vuelve a evaluar.
- Puede pedir como máximo las **rondas extra** que elijas al convocar (de 0 a 3; por defecto, 2). Si se agotan y sigue sin convencerle, te avisa y decides tú.

Se activa o desactiva en el diálogo de convocar, que también calcula cuántas llamadas puede suponer.

Al aprobar, el documento se guarda numerado en el repo (por ejemplo `docs/adr/0004-politica-de-versionado.md`) y en el chat aparece un resumen con los acuerdos y los puntos abiertos.

Durante el consejo los agentes trabajan en **modo solo lectura**: leen el repo para fundamentar su opinión, pero no lo modifican (Claude `--permission-mode plan`, Codex `--sandbox read-only`, OpenCode `--agent plan`, Antigravity `--mode plan`, Copilot sin herramientas de escritura ni shell). Cada participante hace dos llamadas y el moderador una; con el evaluador activado hay al menos una llamada más, y hasta participantes + 2 por cada ronda extra. El diálogo lo indica antes de empezar.

Las instrucciones del evaluador, sus CLIs y si está activo se editan como cualquier agente propio (ver «Agentes propios»).

## Agentes propios

Además de los agentes CLI, puedes definir tus propios agentes en archivos Markdown.

- Los agentes propios se definen en archivos `.md`: una cabecera entre líneas `---` con `id`, `nombre`, `permisos` (lectura | escritura), `cuando` (manual | tras-sintesis | antes-de-commit | al-guardar), `clis` (separadas por comas; vacío = cualquiera disponible), `seleccion` (azar | fija) y `activo` (si | no), y debajo las instrucciones.
- Se buscan en la carpeta de datos de la app (`~/.ide-workspace-chat/agents/`, o bajo `CHAT_DATA_DIR`) y en el proyecto (`.ide-agents/`). Si el mismo `id` está en los dos, gana el del proyecto.
- Se gestionan en **☰ → Agentes propios**: crear, editar, activar o desactivar y quitar.
- El evaluador independiente del consejo es uno de ellos: `evaluador.md` se crea solo la primera vez, con sus instrucciones de siempre. Se puede editar (instrucciones, CLIs, selección) o desactivar, pero no quitar. Lo que tiene que responder (el JSON con el veredicto) sigue en el código, porque la app lo interpreta.
- De momento, los de «antes de un commit» y «al guardar un archivo» se guardan pero no se ejecutan solos.

```md
---
id: revisor
nombre: Revisor de seguridad
permisos: lectura
cuando: manual
clis: claude, codex
seleccion: azar
activo: si
---
Revisa los cambios buscando secretos, permisos excesivos y entradas sin validar.
```

## Maquetas

Una maqueta es una página HTML de ejemplo que genera un agente, para ver cómo sería una propuesta.

- `/maqueta tema` en el chat pide al agente elegido una maqueta HTML en `docs/maquetas/<tema>.html`: un único archivo autocontenido, sin recursos externos.
- En un consejo aprobado, el botón «Maquetar esta decisión» la pide basándose en el documento aprobado.
- En la pestaña **Archivos**, los `.html` tienen un botón de vista previa. Los de `docs/maquetas/` se abren ya en vista previa. Se refresca sola cuando el agente edita el archivo y también con los cambios sin guardar.
- La maqueta se muestra aislada (iframe con sandbox): puede ejecutar su JavaScript, pero no acceder a la app ni a tus archivos.

## Panel de archivos (como VS Code)

A la derecha hay un panel con las pestañas principales **Archivos** y **Consejo** (ver arriba). Se oculta desde **☰ → Vista** y se ensancha arrastrando su borde.

- **Archivos**: árbol a la izquierda y editor a la derecha; al pulsar un archivo se abre sin cambiar de pestaña; el borde entre los dos se arrastra y el botón de la barra del editor oculta el árbol (se recuerda).
  - **Árbol**: el explorador del proyecto, con los colores y letras de git de VS Code (**M** modificado, **U** sin seguimiento, **A** añadido, **D** borrado) y la rama actual. Respeta `.gitignore`. El botón de filtro muestra solo los archivos con cambios.
  - **Editor**: el editor de VS Code (Monaco). Tiene resaltado de sintaxis, minimapa y búsqueda (⌘F / Ctrl+F), y se puede editar y guardar (⌘S / Ctrl+S). En el margen, barras como las de VS Code: verde para líneas añadidas, azul para modificadas y un triángulo rojo donde se borró algo. El botón de comparar abre la vista de cambios respecto al último commit. Los `.html` se pueden ver como página (ver «Maquetas»).

Mientras un agente trabaja, el panel muestra el archivo que está editando (botón del ojo, «Seguir»). Si cambia el archivo abierto, se recarga sin perder la posición; si tenías cambios sin guardar, avisa en lugar de sobrescribirlos.

## Atajos

- **Enter**: enviar. **Shift+Enter**: salto de línea.
- **↑ / ↓**: recorrer los mensajes que ya enviaste, como en un terminal. Solo actúa si el cursor está en la primera línea (↑) o en la última (↓).

## Cómo está hecho

| Capa | Archivo | Qué hace |
| --- | --- | --- |
| Chat | `ui/index.html` | La ventana: chat, menú ☰, panel de archivos y terminal. |
| Caché propia | `src-tauri/src/store.rs` | Guarda cada conversación en disco (`~/.ide-workspace-chat/conversations`, o `CHAT_DATA_DIR`) con qué agente escribió cada respuesta. No depende de ninguna CLI. |
| Adaptadores | `src-tauri/src/agents.rs` | Convierte la conversación guardada en el contexto del CLI elegido y lo ejecuta. Aquí se cambian comandos y modelos. |
| Consejo | `src-tauri/src/council.rs` | Fases del consejo, prompts, tarjetas, síntesis y documento final. |
| Pizarra | `pizarra/entry.jsx`, `ui/pizarra-escena.js` | Excalidraw empaquetado con esbuild (`scripts/pizarra.mjs`) y el paso de tarjetas a elementos de la pizarra. |
| Roles | `src-tauri/src/roles.rs` | Roles del consejo y plantillas, editables desde la app (`council-config.json`). |
| Agentes propios | `src-tauri/src/own_agents.rs` | Agentes definidos en archivos `.md` (app y proyecto), entre ellos el evaluador. |
| Pasos en vivo | `src-tauri/src/stream.rs` | Traduce la salida JSON de cada CLI a pasos comunes (comando, edición, lectura, texto…). |
| Proyecto | `src-tauri/src/project.rs` | Carpeta donde trabajan los agentes y comando `agentes`. |
| Panel de archivos | `src-tauri/src/workspace.rs` | Árbol del proyecto, estado git, líneas cambiadas, guardar y vigilancia de la carpeta. |
| Terminal | `src-tauri/src/term.rs` | Terminal real (PTY) para instalar agentes e iniciar sesión. |
| Entorno | `src-tauri/src/env.rs` | Encuentra las CLIs aunque la app se abra desde el Finder o el menú, sin el PATH de la terminal. |

Puedes cambiar de agente a mitad de conversación. El nuevo recibe todo lo anterior, con el nombre del agente que escribió cada respuesta (por ejemplo, «[Codex]: …») y un resumen de lo que hizo (comandos que ejecutó y archivos que editó). Se incluyen los mensajes más recientes hasta unos 30 000 caracteres (20 000 en agy, que recibe el texto como argumento). Si un agente falla, ese intercambio no se guarda.

## Desarrollo

Requisitos: [Rust](https://rustup.rs) y Node.js 18 o superior. En Linux, además, `libwebkit2gtk-4.1-dev`, `libappindicator3-dev`, `librsvg2-dev` y `patchelf`. En Windows, además, hacen falta las Visual Studio Build Tools con la carga de trabajo «Desarrollo para el escritorio con C++» (requisito de Rust/Tauri en Windows) y WebView2 (ya viene en Windows 10/11).

```sh
npm install
npm run dev      # abre la app en modo desarrollo (copia Monaco y xterm a ui/vendor)
npm run build    # genera el instalador de este sistema
npm run test:ui  # tests de la parte web (node --test)
cd src-tauri && cargo test                           # tests
cd src-tauri && cargo test -- --ignored --nocapture  # tests con las CLIs reales
```

En Windows PowerShell, usa `cd src-tauri; cargo test` (con `;` en lugar de `&&`).

`npm run vendor` (lo lanzan `dev` y `build`) también empaqueta la pizarra en `ui/vendor/pizarra`.

## Notas

- Cada mensaje lanza la CLI de nuevo, con la conversación como contexto.
- Los agentes se ejecutan sin pedir permisos (`--dangerously-skip-permissions`, `--yolo`…). Pueden modificar archivos del proyecto.
- Cada respuesta tiene un tiempo máximo de 10 minutos.
- Cada agente usa su configuración habitual. Por ejemplo, Claude lee tu `~/.claude/CLAUDE.md` global, así que sus instrucciones también se aplican dentro de la app.

## Licencia

[AGPL-3.0-only](LICENSE). La pestaña Objetivo reutiliza código de [Plane](objetivos/vista/plane/README.md)
(Plane Software, Inc. y colaboradores, AGPL-3.0) y `@makeplane/propel`; sus archivos conservan su
copyright. Las versiones publicadas: [releases](docs/releases.md).
