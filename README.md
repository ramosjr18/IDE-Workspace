# Chat de agentes

App de escritorio (macOS, Windows y Linux) para chatear con los agentes CLI: Claude, Codex, OpenCode, Antigravity (agy), Copilot y Gemini. Hecha con [Tauri](https://tauri.app) y Rust: el instalador pesa unos 4 MB.

Usa las CLIs y las sesiones que ya tengas en tu ordenador. Si un agente no está instalado o no tiene la sesión iniciada, la propia app te deja instalarlo e iniciar sesión.

## Instalar

- **macOS**: abre el `.dmg` y arrastra «Chat de agentes» a Aplicaciones. La app no está firmada: si macOS la bloquea, ábrela con clic derecho → Abrir.
- **Windows**: ejecuta el instalador `.msi` o `.exe`.
- **Linux**: `.deb`, `.rpm` o `.AppImage`.

Los instaladores se generan en GitHub Actions (pestaña Actions → Build → Run workflow) o en local con `npm run build`. Cada sistema tiene que compilar el suyo.

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

## Consejo de agentes

Para decisiones con peso (una política, un ADR, una arquitectura), en lugar de preguntar a un solo agente puedes convocar un **consejo**: varios agentes opinan, se critican entre ellos y un moderador redacta la propuesta. Después vuelves al chat de un solo agente con la decisión como contexto.

Se convoca escribiendo `/consejo tema` en el chat, desde **☰ → Convocar consejo** o con **+** en la pestaña **Consejo**. Eliges:

- **Plantilla** (opcional): rellena el resto de campos con una combinación guardada. Ver «Roles y plantillas».
- **Tema** y **resultado**: ADR (`docs/adr/`), política (`docs/politicas/`) o nota de decisión (`docs/decisiones/`).
- **Participantes**, cada uno con un rol (arquitecto, abogado del diablo, seguridad, coste y operación, usuario y producto, libre, o los que hayas creado).
- **Moderador**: resume y redacta; mejor uno que no participe.
- **Contexto**: la conversación actual, que puedes editar antes de empezar.

Fases, en el tablero de la pestaña **Consejo** (se abre ampliado):

1. **Divergir**: cada agente da su propuesta sin ver la de los demás.
2. **Contrastar**: cada uno lee al resto y responde con apoyos, objeciones o preguntas, citando la tarjeta (C1, C2…).
3. **Converger**: el moderador resume acuerdos y puntos abiertos y redacta el documento.
4. **Evaluar** (si está activado): ver «Evaluador independiente».
5. **Revisión**: editas el documento si quieres y eliges **Aprobar y guardar**, **Otra ronda** o **Rehacer síntesis**. También puedes añadir tus comentarios, que cuentan en la siguiente ronda.

### Vista de pizarra

- La pizarra (Excalidraw) tiene su propia pestaña, **Pizarra**, en el panel derecho. Con un consejo abierto muestra su tablero, con las tarjetas de los participantes; sin consejo, una pizarra libre del proyecto, que se guarda aparte. Se puede ampliar a toda la ventana.
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

## Maquetas

Una maqueta es una página HTML de ejemplo que genera un agente, para ver cómo sería una propuesta.

- `/maqueta tema` en el chat pide al agente elegido una maqueta HTML en `docs/maquetas/<tema>.html`: un único archivo autocontenido, sin recursos externos.
- En un consejo aprobado, el botón «Maquetar esta decisión» la pide basándose en el documento aprobado.
- En la pestaña **Archivo**, los `.html` tienen un botón de vista previa. Los de `docs/maquetas/` se abren ya en vista previa. Se refresca sola cuando el agente edita el archivo y también con los cambios sin guardar.
- La maqueta se muestra aislada (iframe con sandbox): puede ejecutar su JavaScript, pero no acceder a la app ni a tus archivos.

## Panel de archivos (como VS Code)

A la derecha hay un panel con tres pestañas: Worktree, Archivo y Consejo (ver arriba). Se oculta desde **☰ → Vista** y se ensancha arrastrando su borde.

- **Worktree**: el explorador del proyecto, con los colores y letras de git de VS Code (**M** modificado, **U** sin seguimiento, **A** añadido, **D** borrado) y la rama actual. Respeta `.gitignore`. El botón de filtro muestra solo los archivos con cambios.
- **Archivo**: el editor de VS Code (Monaco). Tiene resaltado de sintaxis, minimapa y búsqueda (⌘F / Ctrl+F), y se puede editar y guardar (⌘S / Ctrl+S). En el margen, barras como las de VS Code: verde para líneas añadidas, azul para modificadas y un triángulo rojo donde se borró algo. El botón de comparar abre la vista de cambios respecto al último commit. Los `.html` se pueden ver como página (ver «Maquetas»).

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
