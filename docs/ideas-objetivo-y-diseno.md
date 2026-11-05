1. Vista «Objetivo» (To-Do de la sesión)
- Por sesión, no por proyecto: cada conversación puede tener su objetivo, o
  ninguno. Si entras a explorar sin un objetivo claro, la vista simplemente
  dice «Sin objetivo» y no molesta.
- Si llegas con un objetivo, lo escribes y el agente lo divide en una
  checklist, o la haces tú. Cada punto puede ser una tarea suelta o enlazar
  un ticket de Jira (IDE-22, por ejemplo).
- El agente la va actualizando mientras trabaja: marca lo hecho y añade lo
  que descubre. Usaría un comando agentes tareas, igual que agentes pizarra.
  Tú también la marcas a mano.
- Encaja con IDE-25: cuando un agente lanzado en paralelo termine, el aviso
  puede marcar su punto de la lista.

Antes de crear el ticket necesito que me digas cuánta integración con Jira
quieres:
- Solo enlaces: el punto lleva la clave del ticket y se abre en el
  navegador. Es sencillo y no necesita credenciales.
- Leer el estado: el punto muestra si el ticket está «En curso» o «Listo».
  La app necesitaría un token de API de Jira.
- Además, que al cerrar la checklist se puedan pasar los tickets a «Listo»,
  como he hecho yo hoy. Es lo más completo y también necesita token.

2. Vista de diseño de frontend
Aquí hay una diferencia importante con Excalidraw:
- Excalidraw es una librería que funciona entera en el navegador, por eso
  pudimos empaquetarla dentro de la app.
- Penpot es una aplicación completa con su propio servidor, base de datos y
  caché. No se puede meter dentro de la app igual: habría que levantar un
  servidor (en la Spark, por ejemplo) y mostrarlo en una vista, o abrirlo
  aparte. Eso ya está en el ticket IDE-10, que pide investigarlo primero.
- «OpenDesign» no lo conozco con seguridad y no quiero inventarme cómo
  funciona. ¿Me pasas el enlace? Así miro si es una librería que se puede
  empaquetar como Excalidraw o una aplicación con servidor como Penpot.

Notas sobre Open Design (https://github.com/nexu-io/open-design), revisado el 04/11/2026:
- Qué es: «espacio de diseño con agentes de IA» (prototipos, webs, apps,
  dashboards, presentaciones, imágenes, vídeo); se presenta como alternativa
  a Figma. Apache-2.0, ~105 000 estrellas, muy activo.
- Cómo funciona: aplicación web + daemon local (Node 24, pnpm) o
  `docker compose`; se abre en http://127.0.0.1:7456. Tiene app de
  escritorio y guía para WSL2.
- Usa las mismas CLIs de agentes que este IDE (Claude Code, Codex, OpenCode,
  Copilot…), buscándolas en el PATH.
- Integración: no se empaqueta como Excalidraw. Hay que levantar su servidor
  (WSL o Spark) y mostrarlo en una pestaña (iframe: hay que permitir su URL
  en la CSP de la app) o abrirlo aparte. Se solapa con el chat de agentes:
  usarlo solo para diseño. Investigar junto con Penpot (IDE-10).

Requisitos del usuario (04/11/2026) para la herramienta de diseño:
- Tiene que ir DENTRO de la app (es código abierto: se puede modificar lo que haga falta).
- Los diseños tienen que quedar en el código del proyecto, como archivos, para que en
  equipo otra persona los vea al hacer pull, sin publicar nada ni montar un servidor
  compartido.

Comparación con esos requisitos:
- Penpot: guarda los diseños en su base de datos (PostgreSQL) en el servidor; exporta
  .penpot, pero el diseño vivo está en la base de datos. Necesita servidor Clojure (JVM),
  PostgreSQL y Redis. Cumplir «diseños en el repo» exigiría reescribir su almacenamiento.
- Open Design: tiene proyectos «folder-backed» (trabaja en una carpeta que se le indica,
  editándola en su sitio) y está pensado para que un orquestador externo le prepare la
  carpeta y se ocupe de git (docs/orchestrator-workspaces.md de su repo). Sus resultados
  son archivos. Necesita un daemon Node 24 (SQLite solo para su estado) y su interfaz web.

Propuesta: Open Design.
1. La app arranca y para el daemon de Open Design como proceso acompañante (sidecar).
2. Pestaña «Diseño» con su interfaz dentro de la app.
3. Cada proyecto apunta a una carpeta del repo (p. ej. design/): lo diseñado se commitea
   y el equipo lo ve con git pull.
4. Fork propio para quitar o adaptar lo que se solape con el IDE (su chat de agentes).
Primero: probarlo en WSL. Después: épica con tickets (sidecar, pestaña, carpeta en el
repo, adaptaciones del fork). Sustituye la investigación de Penpot de IDE-10.

Una duda más: dices «panel de la izquierda», pero ahí está el chat. ¿Te
refieres a una pestaña nueva en el panel derecho (junto a Archivos, Consejo
y Pizarra), o a otra pestaña en el de la izquierda, junto a Chat y Terminal?
