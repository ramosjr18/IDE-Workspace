# Plan: cualquier agente dibuja en la pizarra

Estado: **decidido el 04/11/2026** (§4). Implementa Claude.

## Lo que se pidió

> ¿Solo el consejo puede usar la pizarra? Mi idea no era solo esa, sino que
> si estoy con cualquier CLI y le pido que me haga X diagrama, o algo visual,
> o un plan, o lo que sea, debe cambiar la vista a la pizarra y mostrármelo.

## 1. Cómo llega un dibujo a la pizarra

Las CLIs no saben dibujar en Excalidraw, pero todas escriben **Mermaid** con
soltura: diagramas de flujo, de secuencia, de clases, de estados, de
entidades, Gantt, mapas mentales…

Excalidraw tiene su conversor oficial,
`@excalidraw/mermaid-to-excalidraw`:
- **Diagramas de flujo, de secuencia y de clases:** se convierten en formas
  editables.
- **El resto de tipos:** entran como imagen, porque el conversor no los
  pasa a formas.

Hay dos vías, que se complementan:

- **A. Explícita: `agentes pizarra`.** El agente ejecuta
  `agentes pizarra --title "Arquitectura" < diagrama.mmd`. Admite Mermaid y,
  para quien lo sepa hacer, el JSON de esqueletos de Excalidraw. Va por el
  mismo canal con token que `agentes run`, así que solo pueden usarlo los
  procesos que lanza la app. La nota que reciben todos los agentes del chat
  explica cuándo usarlo: «si el usuario pide un diagrama, un esquema o un
  plan visual».
- **B. Automática: bloques ```mermaid en la respuesta.** Si la respuesta de
  cualquier agente trae un bloque Mermaid, se manda a la pizarra aunque la
  CLI no haya hecho caso de la nota. En el chat el bloque queda con el
  botón «Ver en pizarra».

## 2. Qué pasa en la pantalla

- El panel derecho **cambia solo a la pestaña Pizarra** (y se muestra si
  estaba oculto). La vista se coloca sobre el diagrama nuevo.
- Cada diagrama tiene **su propia pizarra**, con título (§4.2). Se puede
  editar como cualquier otra y se guarda sola.
- En el chat, la respuesta muestra «🖼 Diagrama en la pizarra · Ver».

## 3. Piezas

| Pieza | Dónde |
|---|---|
| `agentes pizarra [--title] [--format mermaid\|excalidraw] < archivo` | `panes.rs` (cliente y servidor) → evento `board-add` |
| Conversión de Mermaid a elementos | `pizarra/entry.jsx` + `@excalidraw/mermaid-to-excalidraw` en el bundle |
| Recibir `board-add`, cambiar de pestaña, añadir el marco y encuadrar | `ui/index.html` |
| Detectar ```mermaid en las respuestas | `ui/index.html` (render Markdown) |
| La nota para los agentes | `workflows.rs` (`LAUNCHER_NOTE`) |

## 4. Decisiones (04/11/2026)

1. **Las dos vías:** `agentes pizarra` y los bloques ```mermaid de las
   respuestas.
2. **Una pizarra por diagrama.** La pestaña Pizarra tiene un selector:
   pizarra libre del proyecto, la del consejo abierto si hay uno, y los
   diagramas, del más nuevo al más antiguo.
   - Se guardan en `data_dir()/boards/<sha1 del repo>/<id>.json`, con su
     título y su código fuente.
   - El id sale del hash del código: el mismo diagrama no se duplica.
   - Si el diagrama llega con otro proyecto abierto, se convierte al abrirlo.
3. **La vista cambia siempre:** el panel derecho pasa a Pizarra (y se muestra
   si estaba oculto) y abre el diagrama nuevo.
4. **Conversor:** `@excalidraw/mermaid-to-excalidraw@2.2.2`, que va con
   `mermaid`. La 3.x cambia a `pele` y tiene menos de dos semanas.

## 5. Criterios de aceptación

- Pedir a Claude, Codex u OpenCode desde el chat «hazme un diagrama de la
  arquitectura» hace que el panel derecho muestre la pizarra con el
  diagrama, en formas editables si es de flujo, de secuencia o de clases.
- `agentes pizarra` fuera de la app se rechaza (token).
- Un Mermaid inválido no rompe nada: aparece como bloque de código y con el
  error del conversor.
- Los diagramas quedan guardados en la pizarra del proyecto al reiniciar.
