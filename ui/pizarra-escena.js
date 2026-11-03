// Módulo de escena de la pizarra del tablero del consejo (IDE-4): decide qué esqueletos añadir y qué se guarda.
(function (root) {
  // Devuelve el id estable del elemento de pizarra para una tarjeta del consejo.
  function cardElementId(cardId) {
    return "card-" + cardId;
  }

  // Normaliza el autor (agente de miembro o author de tarjeta) para usarlo como clave de columna.
  function columnKey(author) {
    return author == null ? "" : author;
  }

  // Color de fondo del esqueleto según el kind de la tarjeta (comentario y cualquier otro: gris).
  function cardColor(kind) {
    return {
      propuesta: "#a5d8ff",
      riesgo: "#ffc9c9",
      apoyo: "#b2f2bb",
      objecion: "#ffd8a8",
      pregunta: "#d0bfff",
      comentario: "#e9ecef",
    }[kind] || "#e9ecef";
  }

  // Calcula la altura del esqueleto de tarjeta según la longitud del texto, con máximo 400.
  function cardHeight(text) {
    return Math.min(400, 60 + 20 * Math.ceil(text.length / 45));
  }

  // Lista las claves de columna candidatas: agentes de members en orden, luego judge, user y autores sueltos.
  function columnCandidates(council, cards) {
    const keys = [];
    const seen = new Set();
    const add = (k) => {
      if (!seen.has(k)) {
        seen.add(k);
        keys.push(k);
      }
    };
    const members = (council && council.members) || [];
    for (const m of members) add(columnKey(m.agent));
    add("judge");
    add("user");
    for (const c of cards) add(columnKey(c.author));
    return keys;
  }

  // Calcula los esqueletos de Excalidraw que faltan en la pizarra para las tarjetas del consejo.
  function missingSkeletons(council, existingElements) {
    // Sin consejo o sin tarjetas no hay nada que añadir.
    const cards = (council && council.cards) || [];
    if (cards.length === 0) return [];
    // Una tarjeta ya está en la pizarra si existe un elemento con su id (aunque esté borrado).
    const present = new Set();
    for (const el of existingElements || []) {
      if (el && el.id) present.add(el.id);
    }
    // Índice de columna por clave, contando solo las columnas que tienen tarjetas (sin huecos).
    const colIndex = new Map();
    let next = 0;
    for (const key of columnCandidates(council, cards)) {
      if (cards.some((c) => columnKey(c.author) === key)) colIndex.set(key, next++);
    }
    // Recorre las tarjetas en el orden del consejo y calcula su x, y, altura y texto.
    const layout = [];
    const byId = new Map();
    const colY = new Map();
    for (const c of cards) {
      const key = columnKey(c.author);
      const col = colIndex.get(key);
      if (!colY.has(key)) colY.set(key, 0);
      const text = "[" + c.id + "] " + (c.text || "");
      const height = cardHeight(text);
      const rec = {
        card: c,
        id: cardElementId(c.id),
        x: col * 340,
        y: colY.get(key),
        text,
        height,
        isNew: !present.has(cardElementId(c.id)),
      };
      layout.push(rec);
      byId.set(c.id, rec);
      colY.set(key, colY.get(key) + height + 24);
    }
    // esqueletos de las tarjetas nuevas, en el orden en que aparecen en el consejo.
    const out = [];
    for (const rec of layout) {
      if (!rec.isNew) continue;
      out.push({
        type: "rectangle",
        id: rec.id,
        x: rec.x,
        y: rec.y,
        width: 300,
        height: rec.height,
        backgroundColor: cardColor(rec.card.kind),
        fillStyle: "solid",
        roundness: { type: 3 },
        customData: { cardId: rec.card.id },
        label: { text: rec.text, fontSize: 16, textAlign: "left", verticalAlign: "top" },
      });
    }
    // Flechas de respuesta: solo si la tarjeta y su destino son nuevas en esta misma llamada.
    for (const rec of layout) {
      if (!rec.isNew || !rec.card.replyTo) continue;
      const target = byId.get(rec.card.replyTo);
      if (!target || !target.isNew) continue;
      out.push({
        type: "arrow",
        id: "reply-" + rec.card.id + "-" + rec.card.replyTo,
        x: 0,
        y: 0,
        start: { id: rec.id },
        end: { id: cardElementId(rec.card.replyTo) },
        customData: { replyTo: rec.card.replyTo },
      });
    }
    return out;
  }

  // Prepara el objeto que se guarda con council_board_save: elementos tal cual y solo 4 claves de appState.
  function boardToSave(elements, appState) {
    const st = {};
    if (appState) {
      st.viewBackgroundColor = appState.viewBackgroundColor;
      st.scrollX = appState.scrollX;
      st.scrollY = appState.scrollY;
      st.zoom = appState.zoom;
    }
    return { elements, appState: st };
  }

  // API pública del módulo, disponible vía module.exports en Node o window.PizarraEscena en el navegador.
  const api = { cardElementId, missingSkeletons, boardToSave };
  if (typeof module !== "undefined" && module.exports) module.exports = api;
  else root.PizarraEscena = api;
})(typeof window !== "undefined" ? window : globalThis);
