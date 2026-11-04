# Plan: reanudar la sesión de cada CLI en el chat

Estado: **implementado el 04/11/2026** (decisiones en §6, resultados en §8).

## Lo que se pidió

> Reanudar la sesión de cada CLI para gastar menos tokens. Esto es lo más
> urgente.

## 1. El problema

Hoy **cada mensaje del chat lanza la CLI de cero**, aunque no cambies de
agente (`agents::run` sin `--resume`). En cada mensaje se paga esto:

- **Las instrucciones internas y las herramientas de la CLI**, sin la caché
  de prompts de una sesión. En Claude Code son decenas de miles de tokens.
- **El historial pegado como texto** (`build_prompt`): hasta 30.000
  caracteres, unos 8.000 tokens.
- **Volver a leer** los archivos y las salidas de comandos que el agente ya
  había visto, porque no los recuerda. Es lo que más gasta en conversaciones
  largas sobre código.

Efecto colateral: cada mensaje deja una sesión suelta en el historial de la
CLI (`claude --resume` muestra una por mensaje).

## 2. Qué ofrece cada CLI (comprobado con `--help` el 04/11/2026)

Hay que fijarse en si se puede **elegir el id de sesión de antemano**. Así la
app no tiene que sacarlo de la salida de la CLI.

| CLI | Sesión nueva con id propio | Reanudar | Notas |
|---|---|---|---|
| Claude | `--session-id <uuid>` | `--resume <id>` | También sale en el evento `system/init` del `stream-json` |
| Codex | No | `codex exec resume <id> -` | El id sale en el evento `thread.started` de `--json` |
| OpenCode | `-s <id>`: la crea si no existe | `-s <id>` | |
| Copilot | `--session-id <id>` | `--resume <id>` | |
| Gemini | `--session-id <uuid>` | `--resume <?>` | La ayuda solo habla de `latest` o de un número: hay que probar si acepta el uuid |
| agy | No | `--conversation <id>` | Hay que ver si el id sale en su `stream-json` |

## 3. Diseño

### 3.1 Qué se guarda

En la conversación (`store.rs`) se añade:

```rust
/// Clave: "<agente>@<cuenta>", p. ej. "claude@principal", "codex@team-2".
pub sessions: BTreeMap<String, CliSession>,

pub struct CliSession {
    pub id: String,
    /// Mensajes de la conversación que esta sesión ya ha visto (índice del siguiente).
    pub seen: usize,
    /// Forma de trabajar con la que se abrió (ver §3.4).
    pub workflow: String,
}
```

La clave lleva la cuenta porque cada sesión vive en la carpeta de su cuenta:
`claude@team-2` no puede reanudar una sesión de `claude@principal`.

### 3.2 Un mensaje, paso a paso

1. **Si ya hay sesión** para `agente@cuenta`, se reanuda. El prompt lleva
   solo:
   - los mensajes que esa sesión no vio (`messages[seen..]`), es decir, los
     de otros agentes y los del usuario desde su último turno, con el mismo
     formato `[Quién]: texto` de hoy;
   - el mensaje nuevo.

   Si no hay nada nuevo de otros agentes, el prompt es solo el mensaje del
   usuario, igual que en la CLI.
2. **Si no hay sesión**, se crea una:
   - con id propio donde la CLI lo admita, o con el que saque de la salida
     si no;
   - con el prompt de hoy: la cabecera, el historial como texto, la nota de
     la app y la forma de trabajar.
3. **Al terminar bien**, se guarda `seen = messages.len()` (incluida la
   respuesta nueva).

### 3.3 Cuándo no se puede reanudar

En todos estos casos, se abre sesión nueva con el historial como texto, igual
que hoy:

- **Reanudar falla**: la CLI borró la sesión, caducó o no la encuentra. Se
  detecta por el error de la CLI. Se reintenta una vez sin `--resume` y se
  sustituye la sesión guardada.
- **Relevo de cuenta** (`plan-cuentas.md` §5): la cuenta nueva no ve la
  sesión de la anterior.
- **CLIs que no lo permitan** o sin verificar (Gemini y agy hasta probarlas).
- **El agente pasó por el modo solo lectura del consejo**: el consejo no usa
  estas sesiones (§6.3).

### 3.4 La nota de la app y la forma de trabajar

Hoy van en cada mensaje: unos 100 tokens la nota y unos 700 «Equipo en
worktrees». Dentro de una sesión, el agente ya las tiene:

- van **en el primer mensaje de la sesión**;
- **si cambias de forma de trabajar** a mitad de conversación, se mandan en
  el siguiente mensaje con un aviso («A partir de ahora…»).

### 3.5 Lo que no cambia

- La conversación de la app sigue siendo la fuente de verdad: se ve igual,
  se guarda igual y se puede abrir con cualquier agente.
- Los pasos en vivo, el botón de parar, el relevo de cuenta y los diagramas
  a la pizarra.
- `agentes run` (vista Agentes): cada carril ya es una sesión larga en su
  terminal.

## 4. Piezas

| Pieza | Dónde |
|---|---|
| `CliSession` y `sessions` en `Conversation` (con `serde(default)`, las conversaciones viejas siguen abriéndose) | `store.rs` |
| Por CLI: argumentos para sesión nueva y para reanudar, y cómo se saca el id | `agents.rs` (`Agent` gana `session: SessionMode`) |
| Sacar el id de sesión de la salida (Codex `thread.started`, Claude `init`, agy) | `stream.rs` (`Parser` lo expone) |
| `build_prompt` en dos modos: completo y solo lo no visto | `agents.rs` |
| Elegir sesión, reintentar sin ella y guardar `seen` | `lib.rs` (`chat`) |
| Detectar «sesión no encontrada» por CLI | `agents.rs` (como `exhaustion.rs`, con mensajes reales) |

## 5. Cómo se mide que funciona

- Usar la pestaña Uso y el `usage` que ya devuelve cada ejecución
  (`stream::Usage`).
- Prueba: tres mensajes seguidos a Claude en `~/workspace/pablo`, antes y
  después.
- Criterio: del segundo mensaje en adelante, en Claude, la mayor parte de la
  entrada sale de la caché (`cache_read`), y los tokens nuevos de entrada
  bajan claramente.
- Se apuntan las cifras reales en `docs/gasto-de-tokens.md`.

## 6. Decisiones (04/11/2026)

1. **Cambio de agente a mitad:** el que vuelve reanuda su sesión y recibe
   solo lo que no vio.
2. **Nota de la app y forma de trabajar:** en el primer mensaje de la sesión
   y cuando cambie la forma de trabajar.
3. **Alcance:** las 6 CLIs. Gemini y agy, si las pruebas confirman cómo
   reanudar; si no, siguen como hoy. El consejo queda fuera.
4. **Quién:** Claude (una sola zona de código: no se reparte en carriles).

## 7. Criterios de aceptación

- En una conversación solo con Claude, el segundo mensaje recuerda un archivo
  que leyó en el primero sin volver a leerlo (se ve en los pasos), y las
  cifras de §5 bajan.
- Claude → Codex → Claude: Claude reanuda su sesión y recibe solo lo que dijo
  Codex más el mensaje nuevo.
- Si se borra la sesión de la CLI a mano, el siguiente mensaje funciona igual
  (vuelve al historial como texto) y guarda una sesión nueva.
- Un relevo de cuenta a mitad sigue funcionando.
- Las conversaciones guardadas antes del cambio se abren y continúan.
- Tests: `build_prompt` en sus dos modos, el cálculo de `seen`, la
  serialización de las conversaciones viejas, y que cada CLI construye bien
  sus argumentos de sesión.

## 8. Resultados (04/11/2026)

**Claude, medido en `~/workspace/pablo`.** El coste es el que da la propia
CLI (`total_cost_usd`). Al reanudar, esa cifra es el acumulado de la sesión,
así que el de cada mensaje sale de la diferencia.

| Mensaje | Reanudando | Como antes (sesión nueva + historial) |
|---|---|---|
| 1.º («lee README.md») | 0,20 $ | 0,20 $ |
| 2.º | **0,009 $** (37.000 tokens leídos de caché, 101 nuevos) | 0,18 $ (otra vez unos 24.000 tokens nuevos en caché) |
| 3.º | **0,009 $** | ~0,18 $ |

- Cada mensaje de seguimiento cuesta **unas 20 veces menos**.
- Al reanudar, Claude dio el título exacto del README sin volver a abrirlo.
  Como antes, contestó «no lo sé, solo tengo el resumen»: para saberlo
  habría tenido que volver a leer el archivo, y eso cuesta más.

**Qué se ha comprobado de cada CLI:**

| CLI | Estado |
|---|---|
| Claude | Probado de punta a punta: sesión propia, reanudar, sesión perdida |
| Copilot | Probado: `--session-id`, `--resume`, sesión perdida |
| agy | Probado: reanuda con `--conversation`. Si no la encuentra, abre otra sin avisar; la app lo detecta porque cambia el id, y repite el mensaje con sesión nueva |
| Codex | Probados el id de `thread.started` y el error de sesión perdida. El reanudar no se ha podido probar: sin cuota hasta el 05/11 |
| OpenCode | Probado el id `ses_…`. Si la sesión no existe, `-s` crea una vacía sin avisar, así que la app la busca antes con `opencode session list`. El reanudar no se ha podido probar: sin cuota de Go ni de modelos gratis |
| Gemini | Sigue como antes: no hay sesión iniciada en esta máquina para probarlo |

Pendiente: probar Codex y OpenCode cuando vuelva su cuota.
