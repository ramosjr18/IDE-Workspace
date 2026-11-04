// Pruebas del módulo de escena de la pizarra del consejo (IDE-4).
const test = require("node:test");
const assert = require("node:assert/strict");
const escena = require("../ui/pizarra-escena.js");

// Consejo de ejemplo: dos agentes y tarjetas de claude, codex, judge y user, alguna con replyTo.
// Todos los textos son cortos (el texto de etiqueta mide 45 o menos), así que cada altura es 80.
const consejo = {
  members: [{ agent: "claude" }, { agent: "codex" }],
  cards: [
    { id: "C1", round: 0, author: "claude", authorName: "Claude", kind: "propuesta", text: "Primera propuesta del agente Claude" },
    { id: "C2", round: 0, author: "codex", authorName: "Codex", kind: "riesgo", replyTo: "C1", text: "Veo un riesgo en el paso 3" },
    { id: "C3", round: 0, author: "claude", authorName: "Claude", kind: "apoyo", text: "Apoyo a la propuesta anterior" },
    { id: "J1", round: 0, author: "judge", authorName: "Juez", kind: "objecion", text: "Objeción: falta evidencia" },
    { id: "U1", round: 0, author: "user", authorName: "Usuario", kind: "pregunta", text: "¿Puedo editar la pizarra?" },
  ],
};

// Busca un elemento del resultado por su id.
const porId = (elementos, id) => elementos.find((e) => e.id === id);

// Crea un consejo mínimo con una tarjeta suelta.
const consejoDe = (tarjeta) => ({ members: [], cards: [tarjeta] });

test("cardElementId es estable y prefijo card-", () => {
  assert.equal(escena.cardElementId("C1"), "card-C1");
  assert.equal(escena.cardElementId("U1"), "card-U1");
});

test("con la pizarra vacía añade todas las tarjetas con las x de sus columnas y las y apiladas", () => {
  const esqueletos = escena.missingSkeletons(consejo, []);
  // Las tarjetas van en el orden del consejo y las flechas al final del array.
  assert.deepEqual(
    esqueletos.map((e) => e.id),
    ["card-C1", "card-C2", "card-C3", "card-J1", "card-U1", "reply-C2-C1"]
  );
  // Columnas: claude 0, codex 340, judge 680, user 1020.
  assert.deepEqual(
    esqueletos.filter((e) => e.type === "rectangle").map((e) => e.x),
    [0, 340, 0, 680, 1020]
  );
  // C3 apila bajo C1: 80 + 24 de separación; las demás son las primeras de su columna.
  assert.deepEqual(
    esqueletos.filter((e) => e.type === "rectangle").map((e) => e.y),
    [0, 0, 104, 0, 0]
  );
  // La flecha de respuesta va al final con sus extremos enlazados y su geometría.
  // C2 (codex, x 340) está a la derecha de C1 (claude, x 0): sale del borde izquierdo de C2
  // y llega al borde derecho de C1, con el segundo punto relativo al inicio.
  assert.deepEqual(esqueletos[5], {
    type: "arrow",
    id: "reply-C2-C1",
    x: 340,
    y: 40,
    points: [[0, 0], [-40, 0]],
    start: { id: "card-C2" },
    end: { id: "card-C1" },
    customData: { replyTo: "C1" },
  });
  // Forma del esqueleto de tarjeta tal como la pide la pizarra.
  assert.deepEqual(porId(esqueletos, "card-C1"), {
    type: "rectangle",
    id: "card-C1",
    x: 0,
    y: 0,
    width: 300,
    height: 80,
    backgroundColor: "#a5d8ff",
    fillStyle: "solid",
    roundness: { type: 3 },
    customData: { cardId: "C1" },
    label: { text: "[C1] Primera propuesta del agente Claude", fontSize: 16, textAlign: "left", verticalAlign: "top" },
  });
});

test("no vuelve a añadir tarjetas que ya están en la pizarra, ni siquiera borradas", () => {
  const ya = [{ id: "card-C1" }, { id: "card-C2", isDeleted: true }];
  const esqueletos = escena.missingSkeletons(consejo, ya);
  assert.deepEqual(
    esqueletos.map((e) => e.id),
    ["card-C3", "card-J1", "card-U1"]
  );
});

test("la y de una tarjeta nueva cuenta las tarjetas anteriores de su columna", () => {
  // C1 ya está en la pizarra pero C3 sigue apilándose en la y 104.
  const conC1 = escena.missingSkeletons(consejo, [{ id: "card-C1" }]);
  assert.equal(porId(conC1, "card-C3").y, 104);
  assert.ok(!porId(conC1, "card-C1"));
  // Si C1 y C3 ya están, ninguna tarjeta de claude sale.
  const conAmbas = escena.missingSkeletons(consejo, [{ id: "card-C1" }, { id: "card-C3" }]);
  assert.deepEqual(
    conAmbas.map((e) => e.id),
    ["card-C2", "card-J1", "card-U1"]
  );
  assert.equal(porId(conAmbas, "card-C2").y, 0);
});

test("la flecha solo sale cuando la tarjeta y su destino son nuevas", () => {
  // Destino ya en la pizarra: C2 se añade pero sin flecha.
  const conC1 = escena.missingSkeletons(consejo, [{ id: "card-C1" }]);
  assert.ok(porId(conC1, "card-C2"));
  assert.ok(!porId(conC1, "reply-C2-C1"));
  // Destino que no existe: tampoco hay flecha.
  const conC4 = {
    ...consejo,
    cards: [
      ...consejo.cards,
      { id: "C4", round: 1, author: "codex", authorName: "Codex", kind: "riesgo", replyTo: "CX", text: "Riesgo de otro hilo" },
    ],
  };
  const esqueletos = escena.missingSkeletons(conC4, []);
  assert.ok(porId(esqueletos, "card-C4"));
  assert.ok(!porId(esqueletos, "reply-C4-CX"));
});

test("la flecha de respuesta va del borde del origen al borde del destino según su posición", () => {
  // Consejo con las cuatro situaciones: destino a la izquierda, a la derecha y misma columna encima y debajo.
  // Todas las alturas son 80 y las columnas son claude 0 y codex 340.
  const consejoFlechas = {
    members: [{ agent: "claude" }, { agent: "codex" }],
    cards: [
      { id: "D1", round: 0, author: "codex", authorName: "Codex", kind: "riesgo", text: "riesgo en el paso 3" },
      { id: "D2", round: 0, author: "claude", authorName: "Claude", kind: "propuesta", replyTo: "D1", text: "respuesta al riesgo" },
      { id: "D3", round: 0, author: "claude", authorName: "Claude", kind: "apoyo", replyTo: "D4", text: "apoyo a la propuesta" },
      { id: "D4", round: 0, author: "claude", authorName: "Claude", kind: "comentario", text: "comentario aparte" },
      { id: "D5", round: 0, author: "claude", authorName: "Claude", kind: "pregunta", replyTo: "D3", text: "pregunta previa" },
      { id: "D6", round: 0, author: "codex", authorName: "Codex", kind: "riesgo", replyTo: "D2", text: "riesgo cruzado" },
    ],
  };
  const esqueletos = escena.missingSkeletons(consejoFlechas, []);
  // Posiciones de las tarjetas: D1 (340, 0), D2 (0, 0), D3 (0, 104), D4 (0, 208), D5 (0, 312) y D6 (340, 104).
  assert.deepEqual(
    esqueletos.filter((e) => e.type === "rectangle").map((e) => [e.id, e.x, e.y]),
    [["card-D1", 340, 0], ["card-D2", 0, 0], ["card-D3", 0, 104], ["card-D4", 0, 208], ["card-D5", 0, 312], ["card-D6", 340, 104]]
  );
  // Origen a la izquierda del destino (D2 → D1): del borde derecho de D2 al borde izquierdo de D1.
  assert.deepEqual(porId(esqueletos, "reply-D2-D1"), {
    type: "arrow",
    id: "reply-D2-D1",
    x: 300,
    y: 40,
    points: [[0, 0], [40, 0]],
    start: { id: "card-D2" },
    end: { id: "card-D1" },
    customData: { replyTo: "D1" },
  });
  // Misma columna con el origen encima del destino (D3 → D4): del borde inferior de D3 al superior de D4.
  assert.deepEqual(porId(esqueletos, "reply-D3-D4"), {
    type: "arrow",
    id: "reply-D3-D4",
    x: 150,
    y: 184,
    points: [[0, 0], [0, 24]],
    start: { id: "card-D3" },
    end: { id: "card-D4" },
    customData: { replyTo: "D4" },
  });
  // Misma columna con el origen debajo del destino (D5 → D3): del borde superior de D5 al inferior de D3.
  assert.deepEqual(porId(esqueletos, "reply-D5-D3"), {
    type: "arrow",
    id: "reply-D5-D3",
    x: 150,
    y: 312,
    points: [[0, 0], [0, -128]],
    start: { id: "card-D5" },
    end: { id: "card-D3" },
    customData: { replyTo: "D3" },
  });
  // Origen a la derecha del destino (D6 → D2): del borde izquierdo de D6 al borde derecho de D2.
  assert.deepEqual(porId(esqueletos, "reply-D6-D2"), {
    type: "arrow",
    id: "reply-D6-D2",
    x: 340,
    y: 144,
    points: [[0, 0], [-40, -104]],
    start: { id: "card-D6" },
    end: { id: "card-D2" },
    customData: { replyTo: "D2" },
  });
});

test("el color depende del kind y la altura no pasa de 400", () => {
  const colores = {
    propuesta: "#a5d8ff",
    riesgo: "#ffc9c9",
    apoyo: "#b2f2bb",
    objecion: "#ffd8a8",
    pregunta: "#d0bfff",
    comentario: "#e9ecef",
    otra: "#e9ecef",
  };
  for (const [kind, color] of Object.entries(colores)) {
    const esqueletos = escena.missingSkeletons(consejoDe({ id: "T1", author: "user", kind, text: "texto" }), []);
    assert.equal(esqueletos[0].backgroundColor, color, `kind ${kind}`);
  }
  // Un texto larguísimo da 420 y se recorta al máximo 400.
  const larga = escena.missingSkeletons(consejoDe({ id: "T2", author: "user", kind: "comentario", text: "x".repeat(800) }), []);
  assert.equal(larga[0].height, 400);
});

test("missingSkeletons sin consejo o sin tarjetas devuelve []", () => {
  assert.deepEqual(escena.missingSkeletons(null, []), []);
  assert.deepEqual(escena.missingSkeletons({ members: [], cards: [] }, []), []);
});

test("boardToSave copia solo 4 claves de appState y conserva los elementos borrados", () => {
  const elementos = [{ id: "card-C1", isDeleted: true }, { id: "card-C2" }];
  const board = escena.boardToSave(elementos, {
    viewBackgroundColor: "#ffffff",
    scrollX: 1.5,
    scrollY: -2,
    zoom: { value: 0.9 },
    theme: "dark",
    gridSize: null,
  });
  assert.deepEqual(board.elements, elementos);
  assert.deepEqual(board.appState, { viewBackgroundColor: "#ffffff", scrollX: 1.5, scrollY: -2, zoom: { value: 0.9 } });
  // Con appState null queda un objeto vacío.
  assert.deepEqual(escena.boardToSave(elementos, null), { elements: elementos, appState: {} });
});
