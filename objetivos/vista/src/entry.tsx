// Pestaña Objetivo: monta la interfaz de Plane dentro de un Shadow DOM, para que su CSS (con el
// reset de Tailwind) no toque el resto de la app. La app llama a window.ObjetivosVista.mount().
import { createRoot, type Root } from "react-dom/client";
import { Plane, type Request } from "./api";
import { Lista } from "./lista";
// CSS compilado por Tailwind (build.mjs) e incrustado como texto.
import css from "../dist/objetivos.css";

type Options = { request: Request; workspace: string; theme: "light" | "dark" };

let root: Root | null = null;
let container: HTMLElement | null = null;

// Tailwind 4 declara @property, que no funciona dentro de un Shadow DOM: se registran en el documento.
function registerProperties() {
  if (document.getElementById("objetivos-properties")) return;
  const props = css.match(/@property[^{]+\{[^}]*\}/g) ?? [];
  const style = Object.assign(document.createElement("style"), { id: "objetivos-properties", textContent: props.join("\n") });
  document.head.appendChild(style);
}

function mount(el: HTMLElement, opts: Options) {
  registerProperties();
  const shadow = el.shadowRoot ?? el.attachShadow({ mode: "open" });
  shadow.innerHTML = "";
  shadow.appendChild(Object.assign(document.createElement("style"), { textContent: css }));
  const app = document.createElement("div");
  app.className = "plane-root bg-surface-1 text-primary";
  app.style.cssText = "height:100%;width:100%";
  container = app;
  shadow.appendChild(app);
  setTheme(opts.theme);
  root = createRoot(app);
  root.render(<Lista plane={new Plane(opts.request, opts.workspace)} />);
}

// Los tokens de Plane cambian de tema con data-theme, aquí en el contenedor .plane-root (ver build.mjs).
function setTheme(theme: "light" | "dark") {
  container?.setAttribute("data-theme", theme);
}

function unmount() {
  root?.unmount();
  root = null;
}

(window as any).ObjetivosVista = { mount, unmount, setTheme };
