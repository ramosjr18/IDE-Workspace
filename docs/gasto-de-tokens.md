# Gasto de tokens: la app frente a usar la CLI directamente

Pregunta (02/11/2026): «¿Mi IDE hace que de alguna manera se gasten más tokens
que de costumbre en cualquier CLI?»

**Respuesta corta: sí.** Es por diseño, no por un fallo: cada mensaje arranca
la CLI de cero y le reenvía la conversación como texto.

## Por qué gasta más

### 1. Cada mensaje es una sesión nueva

`agents.rs` lanza la CLI en modo no interactivo (`claude -p`, `codex exec`,
`opencode run`, `agy`, `gemini`, `copilot -p`) **sin reanudar sesión**: no usa
`--resume`/`resume`/`-c`. En cada mensaje:

- **Se vuelven a cargar las instrucciones internas y las herramientas** de la
  CLI. En Claude Code son decenas de miles de tokens. En una sesión normal de
  la CLI también se envían en cada turno, pero dentro de la misma sesión y con
  la caché de prompts, que las abarata mucho.
- **El historial va como texto** dentro del mensaje (`build_prompt`): hasta
  30.000 caracteres (unos 8.000 tokens), y 20.000 para agy. En la CLI, el
  historial forma parte de la sesión y también se beneficia de la caché.

### 2. El agente pierde lo que ya había leído

Del turno anterior solo le llega el **texto** de las respuestas, más un
resumen de pasos (`stream::summary`). No le llegan el contenido de los
archivos que leyó ni las salidas de los comandos. Para seguir, **vuelve a
leerlos**. Suele ser lo que más gasta en conversaciones largas sobre código.

### 3. Lo que suma la app (poco)

| Qué | Cuánto | Cuándo |
|---|---|---|
| Nota de `agentes run` | ~100 tokens | En cada mensaje del chat |
| Forma de trabajar «Equipo en worktrees» | ~700 tokens | En cada mensaje, si está elegida |
| Consejo de agentes | Un agente por participante y ronda, más el moderador y el evaluador | Por diseño: varios agentes deliberan |
| Pestaña Uso | 0 tokens: solo consulta los límites | La excepción: si caduca la sesión de Claude, ejecuta `claude -p "ok"` para que la CLI la renueve (una petición mínima) |

## Cómo medirlo

La pestaña **Uso** muestra los tokens de hoy y por modelo de cada CLI. Para
comparar:

1. Haz la misma tarea dos veces, una en la app y otra en la CLI.
2. Mira la diferencia de «hoy» antes y después de cada una.

## Mejora propuesta: reanudar la sesión de cada CLI

En vez de reenviar el historial como texto, guardar con cada conversación el
id de sesión de cada CLI y reanudarla:

| CLI | Cómo reanudar |
|---|---|
| Claude | `claude -p --resume <session_id>` (el id sale en el `stream-json`) |
| Codex | `codex exec resume <id>` (o `--last`) |
| OpenCode | `opencode run -s <id>` / `-c` |
| agy, Gemini, Copilot | Hay que comprobar qué ofrece cada una; si no se puede, se sigue como ahora |

**Lo que se gana:**
- La caché de prompts vuelve a funcionar.
- El agente conserva los archivos leídos y las salidas de los comandos.
- No hace falta reenviar el historial.

**Lo que hay que resolver:**
- Las conversaciones de la app son **compartidas entre agentes**: puedes
  cambiar de Claude a Codex a mitad.
  - La sesión de Codex no sabe lo que pasó en la de Claude.
  - Propuesta: reanudar la sesión del agente que ya estaba y pasarle como
    texto **solo** los mensajes que no vio (los de otros agentes desde su
    último turno).
- Las sesiones caducan o se borran en la CLI. Si reanudar falla, se vuelve al
  modo de ahora.

Estado: **propuesta, sin decidir.** Es un cambio de diseño en `agents.rs` y
`store.rs`.
