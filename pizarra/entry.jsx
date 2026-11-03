// ./ruta.js tiene que ir primero: la ruta de assets debe existir antes de cargar Excalidraw.
import "./ruta.js";

// Dependencias para montar el componente de React en un elemento del DOM.
import React from "react";
import { createRoot } from "react-dom/client";
// Componente de la pizarra del consejo (IDE-4) y su CSS (lleva las @font-face).
import { Excalidraw } from "@excalidraw/excalidraw";
import "@excalidraw/excalidraw/index.css";

// Pizarra: fachada única para montar la vista del tablero del consejo.
window.Pizarra = {
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
      // unmount libera la vista y su raíz de React.
      unmount() {
        root.unmount();
      },
    };
  },
};
