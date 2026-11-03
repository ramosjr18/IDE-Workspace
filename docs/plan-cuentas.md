# Plan: varias cuentas por CLI, en simultáneo y con relevo

Estado: **implementado el 03/11/2026** (decisiones en §6). El carril de OpenCode
(`beam-free`) terminó sin escribir código, así que la detección también la
hizo Claude.

Pendiente de verificar:
- **macOS:** si una cuenta extra de Claude guarda su token en el Llavero en
  vez de en `.credentials.json`. Hasta comprobarlo, los límites de esa cuenta
  salen sin datos.
- **Antigravity y Copilot:** solo cuenta principal, porque su carpeta de
  sesión no está verificada.
- **Gemini:** `GEMINI_CLI_HOME` aparece en su código, pero no se ha probado
  iniciando sesión con una segunda cuenta.

## Lo que se pidió

> Podemos trabajar con múltiples cuentas de CLIs: de Claude con Teams tengo
> varias, a lo mejor tengo 2 de OpenCode y de Codex, y así con todos.
> Inicialmente pensé en contenedores, pero no sé si es lo más óptimo.
> […] Pero simultáneo: a lo mejor pongo las 2 cuentas de un proveedor a
> trabajar de una vez. O una cuenta se agotó, y cosas así.

## 1. La idea: una cuenta es una carpeta, no un contenedor

Cada CLI guarda su sesión en una carpeta y casi todas dejan cambiarla con una
variable de entorno. La app ya lanza todas las CLIs con el entorno que ella
decide (`child_env`), así que:

- una **cuenta** es la carpeta de sesión de una CLI más su variable;
- como la variable es de cada proceso, **dos cuentas del mismo proveedor
  funcionan a la vez** sin pisarse.

| CLI | Variable | Estado |
|---|---|---|
| Claude | `CLAUDE_CONFIG_DIR` | Conocida |
| Codex | `CODEX_HOME` | Conocida |
| OpenCode | `XDG_DATA_HOME` + `XDG_CONFIG_HOME` (`auth.json`, `opencode.db`) | Conocida |
| Gemini | `GEMINI_CLI_HOME` | Aparece en su código; hay que probarla |
| Antigravity (agy) | Candidatas: `ANTIGRAVITY_GEMINI_DIR`, `ANTIGRAVITY_APP_DATA_DIR` | Hay que probarlas |
| Copilot | Su sesión, o la de `gh` (`GH_CONFIG_DIR`) | Hay que probarla |

**Contenedores, no:**
- No aíslan cuentas mejor que una carpeta aparte.
- En macOS y Windows obligan a tener Docker.
- Complican los worktrees, `node_modules` y `mise`, y hay que instalar cada
  CLI dentro de la imagen.

Sí tendrían sentido para otra cosa: encerrar a un agente que corre sin
permisos dentro de su worktree. Eso sería otra forma de trabajar, más
adelante.

## 2. Cuentas en la app

- **Dónde viven:** `data_dir()/cuentas/<cli>/<nombre>/`. La cuenta
  «principal» de cada CLI es la de siempre (`~/.claude`, `~/.codex`…) y no se
  mueve.
- **Añadir:** ☰ → Agentes → «+ Cuenta» en cada CLI. Se elige un nombre
  («team-2») y se abre el login oficial de la CLI en la terminal de la app,
  con la variable ya puesta.
- **Nada compartido** (decidido): cada cuenta es una carpeta independiente y
  se configura por separado. Consecuencia: en una cuenta nueva de Claude no
  se aplican el `CLAUDE.md` global ni las skills de `~/.claude`, salvo que se
  copien a mano.
- **Quitar:** borra la carpeta de esa cuenta. Antes pregunta, porque su
  sesión se pierde.
- **En macOS**, Claude guarda el token en el Llavero, no en un archivo. Antes
  de dar la función por buena hay que comprobar que otro `CLAUDE_CONFIG_DIR`
  usa otra entrada del Llavero y no pisa la principal.

## 3. Uso por cuenta

La pestaña Uso muestra una tarjeta por cuenta: «Codex · principal», «Codex ·
cuenta-2». Los colectores ya respetan `CLAUDE_CONFIG_DIR` y `CODEX_HOME`, y
`limits.rs` lee la credencial de la carpeta de cada cuenta. Se mantienen
todas las reglas de seguridad de la fase 3 de `plan-pestanas-y-uso.md`.

## 4. Elegir cuenta y repartir

- **Chat:** el selector de agente pasa a «Claude · team-2». Con «auto», se
  usa la cuenta con más cuota libre.
- **`agentes run --account <nombre>|auto`** y un desplegable de cuenta en
  «+ Lanzar». Cada panel muestra su cuenta.
- **Reparto en un lote:** con `auto`, cada carril nuevo va a la cuenta con
  más margen; si están igual, se alternan.
- **Puerta antes de lanzar:** no se lanza en una cuenta por encima del umbral
  (§6), para no arrancar un carril que muere en el primer turno.

## 5. Relevo cuando una cuenta se agota a mitad

1. **Detección:** cada CLI tiene su lista de mensajes de agotamiento («usage
   limit», «quota reached», «Individual quota reached», «monthly usage limit»,
   HTTP 429…), sacados de **errores reales** guardados como ejemplos de test.
   Se buscan en la salida del panel, junto con el código de salida. Además,
   la pestaña Uso marca la cuenta al 100 %.
2. **Relevo:** el mismo carril se lanza **en el mismo worktree** con otra
   cuenta. La sesión de la cuenta agotada no se puede continuar, porque vive
   en su carpeta. El agente nuevo recibe un **brief de continuación**:
   - el brief original;
   - `git log` desde la rama base;
   - `git diff`, incluido lo que quedó sin commitear;
   - el final de `agente.log`;
   - «continúa desde aquí; no rehagas lo que ya está».
3. Se mantiene el **mismo id de panel**: el panel muestra «relevado: cuenta-1
   → cuenta-2» y `agentes wait` sigue esperando al carril, no al proceso
   muerto.
4. **Sin cuentas con cuota** en esa CLI: se pasa a otra CLI (§6) o se para y
   se avisa.

## 6. Decisiones (cerradas el 03/11/2026)

1. **Relevo automático, mirando el trabajo y no la cuenta.** Las palabras del
   usuario: «No lo mires por cuenta sino por el proyecto. Si una se queda sin
   cuota, pasa a la siguiente cuenta con saldo disponible.» La cuenta es
   intercambiable y todas sirven para todos los proyectos.
2. **Sin cuota en ninguna cuenta de la CLI: se pasa a otra CLI** con cuota,
   con el mismo brief de continuación.
3. **Umbral: 90 %** del límite más alto (sesión o semanal). Por encima no se
   lanza nada nuevo en esa cuenta.
4. **Nada compartido entre cuentas** (§2).
5. **Chat: cuenta «auto»** (la de más cuota libre). Si se agota a mitad de
   una respuesta, se repite con la siguiente.
6. **Quién:** OpenCode (`beam-free`) hace `accounts/exhaustion.rs`, la
   detección de agotamiento con ejemplos reales. Claude hace todo lo demás y
   la revisión.

## 8. Contrato del carril de OpenCode

```rust
// src-tauri/src/accounts/exhaustion.rs
pub struct Exhaustion { pub reason: String, pub resets_in: Option<std::time::Duration> }
/// `cli`: id del agente (claude, codex, opencode, agy, gemini, copilot).
/// `output`: salida del proceso sin colores (el final de agente.log).
pub fn detect(cli: &str, output: &str, exit_code: i32) -> Option<Exhaustion>
```

- Solo se consideran las **últimas 40 líneas** y solo si `exit_code != 0`.
  Así no se confunde con un agente que escribe código que menciona «rate
  limit».
- **Falsos positivos que tienen que dar `None`**: el aviso de privacidad de
  OpenCode («This Go model trains on request data…»), un error de
  compilación y un 429 transitorio de un servidor web de prueba.

## 7. Criterios de aceptación

- Dos cuentas de Codex trabajan a la vez en dos paneles, cada una con su
  sesión, y la pestaña Uso muestra las dos por separado.
- Al agotarse una cuenta (simulado con un falso CLI que imprime el error
  real y sale con código 1), el carril sigue en el mismo worktree con otra
  cuenta y el panel lo indica.
- Una cuenta por encima del umbral no recibe carriles nuevos.
- Añadir una cuenta no toca la sesión de la principal (comprobado
  comparando `~/.claude/.credentials.json` antes y después).
- Tests: la detección de agotamiento de cada CLI, con sus ejemplos reales, y
  el reparto `auto`.
