# Chat de agentes

App de escritorio (macOS, Windows y Linux) para chatear con los agentes CLI: Claude, Codex, OpenCode, Antigravity (agy) y Gemini. Hecha con [Tauri](https://tauri.app) y Rust: el instalador pesa unos 4 MB.

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
- **Conversaciones**: «Nueva conversación» y la lista de conversaciones del proyecto.
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

Codex, OpenCode y Gemini se instalan con npm, así que necesitan Node.js.

## Panel de archivos (como VS Code)

A la derecha hay un panel con dos pestañas. Se oculta desde **☰ → Vista** y se ensancha arrastrando su borde.

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
| Proyecto | `src-tauri/src/project.rs` | Carpeta donde trabajan los agentes y comando `agentes`. |
| Panel de archivos | `src-tauri/src/workspace.rs` | Árbol del proyecto, estado git, líneas cambiadas, guardar y vigilancia de la carpeta. |
| Terminal | `src-tauri/src/term.rs` | Terminal real (PTY) para instalar agentes e iniciar sesión. |
| Entorno | `src-tauri/src/env.rs` | Encuentra las CLIs aunque la app se abra desde el Finder o el menú, sin el PATH de la terminal. |

Puedes cambiar de agente a mitad de conversación. El nuevo recibe todo lo anterior, con el nombre del agente que escribió cada respuesta (por ejemplo, «[Codex]: …»). Se incluyen los mensajes más recientes hasta unos 30 000 caracteres (20 000 en agy, que recibe el texto como argumento). Si un agente falla, ese intercambio no se guarda.

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
