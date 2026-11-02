# Plan: vista «Agentes» (terminales en mosaico de un lote multiagente)

Estado: **decidido el 02/11/2026**:
- opción A: la app lanza a los agentes y el orquestador usa `agentes run`;
- la pestaña va **al final del panel derecho**
  (`Worktree · Archivo · Consejo · Uso · Agentes`) y se puede ampliar a toda
  la ventana, como el Consejo;
- cuando esté probado, se actualiza `~/.claude/CLAUDE.md`.

## Lo que se pidió

> Yo en Claude tengo un workflow multiagéntico con diferentes CLIs y diferentes
> worktrees. Quiero traer eso aquí. Entre la pestaña de Chat y Terminal, o al
> final. Quiero que sea una vista de terminales. Si el CLI seleccionado ya usa
> un workflow como este (Claude, por ejemplo), lanza los agentes en sus
> worktrees, pero la ejecución se ve así [mosaico de terminales, estilo tmux] en
> la vista nueva. Dependerá de los agentes que se lancen: se deberán ir
> acomodando según el caso.

El workflow de hoy (en `~/.claude/CLAUDE.md`): Claude escribe un brief por
carril, crea un worktree por agente (`git worktree add .claude/worktrees/<agente>-<tema>`)
y lanza en segundo plano `codex exec …`, `opencode run …` o `agy -p …`, con
salida a un log. Después supervisa, revisa y mergea. Hoy esa ejecución no se
ve: solo hay logs en ficheros.

## El problema de fondo: cómo se entera la app de los agentes

Cuando el orquestador (Claude en el chat) lanza `codex exec` con su
herramienta Bash, el proceso es hijo de Claude y no de la app. La app no lo
ve ni puede mostrar su terminal. Hay tres formas de resolverlo:

| | Cómo | A favor | En contra |
|---|---|---|---|
| **A. La app lanza** (recomendada) | La app trae un comando `agentes run`. El orquestador lo usa en lugar de `nohup … &` y la app abre el agente en un PTY suyo, dentro de un panel del mosaico | Terminal real e interactiva: puedes escribirle al agente o pararlo. Sabes cuándo acaba y con qué código. Funciona igual con cualquier orquestador (Claude, Codex…) y en cualquier sistema | El orquestador tiene que usar `agentes run` (se le dice en su prompt de sistema; ver abajo) |
| B. Leer los logs | La app vigila los logs (`*.log`, `codex.log`) de `.claude/worktrees/*/` y los muestra con `tail -f` en cada panel | No cambia nada del workflow | Solo lectura: no puedes interactuar, no hay estado fiable de «terminado» y depende de cómo nombre el log cada uno |
| C. Lanzamiento manual | Un diálogo «Lanzar lote»: eliges CLI, worktree y brief por carril, y la app los lanza | Sin orquestador | No es «el CLI seleccionado lanza a los agentes»; es otra cosa |

**Propuesta: A como base, con C como acceso manual sobre el mismo mecanismo.**
B queda descartada porque no da terminales de verdad.

## Diseño (opción A)

### `agentes run`: el lanzador

El comando `agentes` ya se instala desde la app. Gana un subcomando:

```sh
agentes run --cli codex --worktree codex-uso --branch codex/uso \
  [--name "Backend uso"] [--wait] -- exec --approve-for-me - < brief.md
```

- `--worktree <nombre>`: carpeta `.claude/worktrees/<nombre>` del proyecto.
  Si no existe, la crea con `git worktree add … -b <branch> HEAD` y enlaza
  `node_modules`/`.venv`, igual que ahora a mano.
- Lo que va después de `--` son los argumentos de la CLI, que se ejecuta en
  ese worktree. Si se le pasa algo por stdin, el panel lo guarda en
  `<worktree>/brief.md` y se lo da a la CLI por stdin.
- Sin `--wait`: vuelve enseguida con el id del panel. Con `--wait`: espera a
  que termine y devuelve su código de salida. La salida queda además en
  `<worktree>/agente.log`, para que el orquestador la lea al supervisar.
- `agentes status` lista los paneles (id, CLI, worktree, estado, código de
  salida). Así el orquestador sabe sin adivinar quién terminó y quién sigue.

**Cómo llega a la app.** La app escucha en un socket local: un socket Unix
`0600` en `data_dir()`, y en Windows, una *named pipe*. Solo acepta peticiones
con un **token por sesión**, que la app pasa en el entorno
(`AGENTES_SOCKET`, `AGENTES_TOKEN`) a los procesos que lanza ella: los agentes
del chat y la terminal. Un proceso cualquiera del equipo no puede abrir
paneles. Si `agentes run` se ejecuta fuera de la app, sin token, no hace nada
y lo dice.

**Cómo se entera el orquestador.** Al lanzar el agente del chat, la app añade
a su prompt una nota corta: «Para lanzar subagentes en worktrees usa
`agentes run …` (ver `agentes run --help`); no uses `nohup`/`&`». Tu
`CLAUDE.md` global también se puede actualizar en ese sentido; eso lo decides
tú.

### La vista: pestaña «Agentes»

Va al final del panel derecho: `Worktree · Archivo · Consejo · Uso · Agentes`.
Así el chat (el orquestador) sigue a la vista a la izquierda mientras miras
el mosaico. Con «Ampliar» ocupa toda la ventana, como el tablero del Consejo.

- **Mosaico automático** según el número de paneles:
  - 1: ocupa todo;
  - 2: lado a lado;
  - 3: uno grande a la izquierda y dos apilados;
  - 4: rejilla 2×2;
  - de 5 en adelante: rejilla de ⌈√n⌉ columnas.

  Se reajusta solo cuando un panel entra o sale. Los bordes se pueden
  arrastrar.
- **Cabecera de cada panel**: CLI y nombre, worktree y rama, estado
  (en marcha, terminado ✓, error con código) y tiempo transcurrido. Botones:
  maximizar (también con doble clic), detener, abrir el worktree en el panel
  derecho y cerrar.
- Cada panel es una terminal de verdad (xterm + PTY, como la pestaña
  Terminal): puedes escribirle al agente.
- Un panel terminado se queda visible hasta que lo cierras, para que puedas
  revisar su salida.
- **Contador en la pestaña**: «Agentes 2/3» (en marcha/total).
- Botón **«+ Lanzar»** (opción C): un diálogo con CLI, worktree, rama y brief,
  que usa el mismo mecanismo.
- Paneles por proyecto: al cambiar de proyecto, los del anterior siguen en
  marcha y vuelven a verse al volver a él.

### Lo que no se puede mostrar

Los subagentes internos de Claude Code (su herramienta `Agent`) no son
procesos aparte: viven dentro de Claude y no tienen terminal. Se siguen viendo
como pasos en el chat, como ahora. La vista Agentes es para los **procesos**
que lanza el orquestador (`codex`, `opencode`, `agy`, otro `claude`…).

### Al cerrar la app

Si hay agentes en marcha, la app pregunta: «Hay 2 agentes trabajando. ¿Los
detengo?». En esta fase no se pueden dejar vivos sin la app: sin ella no
habría dónde verlos.

## Fases

1. Vista Agentes con mosaico, paneles PTY y «+ Lanzar» manual. Ya da valor
   sin orquestador.
2. Socket con token, `agentes run`/`status` y el aviso en el prompt del
   orquestador.
3. Opcional: comprobar la cuota antes de lanzar (por ejemplo «Codex al 92 %
   semanal») con los datos de la pestaña Uso.

## Criterios de aceptación

- Desde el chat con Claude, pedirle «lanza a codex en un worktree con este
  brief» abre un panel en la vista Agentes con Codex trabajando en
  `.claude/worktrees/<x>`, y Claude ve su estado con `agentes status`.
- Con 1, 2, 3, 4 y 6 paneles, el mosaico se recoloca solo. Maximizar y
  restaurar funcionan.
- Un `agentes run` lanzado desde una terminal fuera de la app se rechaza.
- Al cerrar la app con agentes en marcha, pregunta antes.
