// ./ruta.js tiene que ir primero: la ruta de assets debe existir antes de cargar Excalidraw.
import "./ruta.js";

// Dependencias para montar el componente de React en un elemento del DOM.
import React from "react";
import { createRoot } from "react-dom/client";
// Componente de la pizarra del consejo (IDE-4) y su CSS (lleva las @font-face).
import { Excalidraw, convertToExcalidrawElements } from "@excalidraw/excalidraw";
import "@excalidraw/excalidraw/index.css";

// Pizarra: fachada única para montar la vista del tablero del consejo.
window.Pizarra = {
  // convert convierte esqueletos en elementos de Excalidraw, conservando sus ids (regenerateIds: false).
  convert(skeletons) {
    return convertToExcalidrawElements(skeletons, { regenerateIds: false });
  },
  // fontsReady espera a que Excalidraw registre Excalifont en document.fonts (lo hace al montarse, no al cargar el bundle): sin ella convert mide el texto con una reserva y las tarjetas se cortan.
  async fontsReady() {
    // Sondea document.fonts cada 50 ms como máximo 3 s; si Excalifont no aparece, sigue igualmente.
    const inicio = Date.now();
    while (Date.now() - inicio < 3000) {
      if ([...document.fonts].some((f) => f.family === "Excalifont")) break;
      await new Promise((res) => setTimeout(res, 50));
    }
    // Carga la fuente ya registrada para que las medidas de texto la usen; si falla, no rompe.
    try { await document.fonts.load("16px Excalifont"); } catch (err) {}
  },
  // mount dibuja el tablero en el elemento dado y devuelve controles para actualizarlo o liberarlo.
  mount(el, scene, options) {
    scene = scene || {};
    options = options || {};
    // apiRef guarda la API de Excalidraw cuando el componente se monta (para updateScene).
    let apiRef = null;
    // theme es "light" o "dark", por defecto "light".
    const theme = (options && options.theme) || "light";
    // root es el raíz de React sobre el elemento donde se dibuja.
    const root = createRoot(el);
    // Renderiza el componente con la escena inicial y el tema pedido.
    root.render(
      React.createElement(Excalidraw, {
        initialData: {
          elements: scene.elements || [],
          appState: { ...(scene.appState || {}), theme },
        },
        theme: theme,
        onChange: (elements, appState) =>
          options.onChange && options.onChange(elements, appState),
        excalidrawAPI: (api) => {
          apiRef = api;
        },
      })
    );
    // Controles que devuelve a quien monta la vista.
    return {
      // update repinta la escena con los elementos dados.
      update(elements) {
        if (apiRef) apiRef.updateScene({ elements });
      },
      // getElements devuelve los elementos actuales de la escena, incluidos los borrados.
      getElements() {
        return apiRef ? apiRef.getSceneElementsIncludingDeleted() : [];
      },
      // scrollToContent centra la vista y ajusta el zoom al contenido actual (fitToContent).
      scrollToContent() {
        if (apiRef) apiRef.scrollToContent(undefined, { fitToContent: true });
      },
      // unmount libera la vista y su raíz de React.
      unmount() {
        root.unmount();
      },
    };
  },
};
