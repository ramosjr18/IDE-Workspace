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
- Si abres la app desde el Finder o el menú, se abre en el último proyecto que usaste.
- Para cambiar de proyecto sin cerrar la app: **☰ → Proyecto**, pulsa la carpeta y elige otra.

## Uso

La cabecera muestra el proyecto y el agente activos («mi-repo · Claude»). Todo lo demás está en el menú **☰** de la izquierda:

- **Proyecto**: la carpeta donde trabajan los agentes. Al pulsarla puedes elegir otra.
- **Agente**: elige el agente. Debajo se ve su estado (**Listo**, **Sin sesión** o **No instalado**) con el botón para arreglarlo. **Agentes** muestra todos con su estado, como un panel de cuentas.
- **Conversaciones**: «Nueva conversación», «Convocar consejo» y la lista de conversaciones del proyecto.
- **Vista**: mostrar u ocultar el panel de archivos.

El menú se cierra al elegir algo, al pulsar fuera o con Esc.

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

- **Tema** y **resultado**: ADR (`docs/adr/`), política (`docs/politicas/`) o nota de decisión (`docs/decisiones/`).
- **Participantes**, cada uno con un rol: arquitecto, abogado del diablo, seguridad, coste y operación, usuario y producto, o libre.
- **Moderador**: resume y redacta; mejor uno que no participe.
- **Contexto**: la conversación actual, que puedes editar antes de empezar.

Fases, en el tablero de la pestaña **Consejo** (se abre ampliado):

1. **Divergir**: cada agente da su propuesta sin ver la de los demás.
2. **Contrastar**: cada uno lee al resto y responde con apoyos, objeciones o preguntas, citando la tarjeta (C1, C2…).
3. **Converger**: el moderador resume acuerdos y puntos abiertos y redacta el documento.
4. **Evaluar** (si está activado): ver «Evaluador independiente».
5. **Revisión**: editas el documento si quieres y eliges **Aprobar y guardar**, **Otra ronda** o **Rehacer síntesis**. También puedes añadir tus comentarios, que cuentan en la siguiente ronda.

### Evaluador independiente

Es el primer agente propio del IDE. Después de cada síntesis, un evaluador revisa con ojos nuevos si lo convergido es correcto: si responde al tema, recoge las objeciones importantes, no contradice el código y se puede aplicar.

- Corre sobre una **CLI elegida al azar** entre las disponibles. Nunca es el moderador y, si se puede, tampoco un participante. Si la CLI elegida falla (sin cupo, sin sesión…), prueba con la siguiente.
- Si le convence, **recomienda aprobar** y pasas a revisar.
- Si no, deja sus **objeciones como tarjetas** (columna «Evaluador») y **manda al consejo a otra ronda**: los participantes las responden, el moderador rehace la síntesis y vuelve a evaluar.
- Puede pedir como máximo las **rondas extra** que elijas al convocar (de 0 a 3; por defecto, 2). Si se agotan y sigue sin convencerle, te avisa y decides tú.

Se activa o desactiva en el diálogo de convocar, que también calcula cuántas llamadas puede suponer.

Al aprobar, el documento se guarda numerado en el repo (por ejemplo `docs/adr/0004-politica-de-versionado.md`) y en el chat aparece un resumen con los acuerdos y los puntos abiertos.

Durante el consejo los agentes trabajan en **modo solo lectura**: leen el repo para fundamentar su opinión, pero no lo modifican (Claude `--permission-mode plan`, Codex `--sandbox read-only`, OpenCode `--agent plan`, Antigravity `--mode plan`, Copilot sin herramientas de escritura ni shell). Cada participante hace dos llamadas y el moderador una; el diálogo lo indica antes de empezar.

## Panel de archivos (como VS Code)

A la derecha hay un panel con tres pestañas: Worktree, Archivo y Consejo (ver arriba). Se oculta desde **☰ → Vista** y se ensancha arrastrando su borde.

- **Worktree**: el explorador del proyecto, con los colores y letras de git de VS Code (**M** modificado, **U** sin seguimiento, **A** añadido, **D** borrado) y la rama actual. Respeta `.gitignore`. El botón de filtro muestra solo los archivos con cambios.
- **Archivo**: el editor de VS Code (Monaco). Tiene resaltado de sintaxis, minimapa y búsqueda (⌘F / Ctrl+F), y se puede editar y guardar (⌘S / Ctrl+S). En el margen, barras como las de VS Code: verde para líneas añadidas, azul para modificadas y un triángulo rojo donde se borró algo. El botón de comparar abre la vista de cambios respecto al último commit.

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
| Pasos en vivo | `src-tauri/src/stream.rs` | Traduce la salida JSON de cada CLI a pasos comunes (comando, edición, lectura, texto…). |
| Proyecto | `src-tauri/src/project.rs` | Carpeta donde trabajan los agentes y comando `agentes`. |
| Panel de archivos | `src-tauri/src/workspace.rs` | Árbol del proyecto, estado git, líneas cambiadas, guardar y vigilancia de la carpeta. |
| Terminal | `src-tauri/src/term.rs` | Terminal real (PTY) para instalar agentes e iniciar sesión. |
| Entorno | `src-tauri/src/env.rs` | Encuentra las CLIs aunque la app se abra desde el Finder o el menú, sin el PATH de la terminal. |

Puedes cambiar de agente a mitad de conversación. El nuevo recibe todo lo anterior, con el nombre del agente que escribió cada respuesta (por ejemplo, «[Codex]: …») y un resumen de lo que hizo (comandos que ejecutó y archivos que editó). Se incluyen los mensajes más recientes hasta unos 30 000 caracteres (20 000 en agy, que recibe el texto como argumento). Si un agente falla, ese intercambio no se guarda.

## Desarrollo

Requisitos: [Rust](https://rustup.rs) y Node.js 18 o superior. En Linux, además, `libwebkit2gtk-4.1-dev`, `libappindicator3-dev`, `librsvg2-dev` y `patchelf`.

```sh
npm install
npm run dev      # abre la app en modo desarrollo (copia Monaco y xterm a ui/vendor)
npm run build    # genera el instalador de este sistema
cd src-tauri && cargo test                           # tests
cd src-tauri && cargo test -- --ignored --nocapture  # tests con las CLIs reales
```

## Notas

- Cada mensaje lanza la CLI de nuevo, con la conversación como contexto.
- Los agentes se ejecutan sin pedir permisos (`--dangerously-skip-permissions`, `--yolo`…). Pueden modificar archivos del proyecto.
- Cada respuesta tiene un tiempo máximo de 10 minutos.
- Cada agente usa su configuración habitual. Por ejemplo, Claude lee tu `~/.claude/CLAUDE.md` global, así que sus instrucciones también se aplican dentro de la app.
