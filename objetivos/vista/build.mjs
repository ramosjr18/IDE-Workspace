// Empaqueta la pestaña Objetivo en ui/vendor/objetivos/objetivos.js (como la pizarra con Excalidraw).
// 1) Tailwind compila src/estilos.css → dist/objetivos.css.  2) esbuild empaqueta src/entry.tsx con
// ese CSS dentro, y resuelve los @plane/... al código de nuestra copia de Plane (objetivos/plane).
import { build } from "esbuild";
import { execFileSync } from "node:child_process";
import { mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const plane = resolve(here, "../plane/packages");
const out = resolve(here, "../../ui/vendor/objetivos");

mkdirSync(join(here, "dist"), { recursive: true });
execFileSync(process.execPath, [join(here, "node_modules/@tailwindcss/cli/dist/index.mjs"), "-i", "src/estilos.css", "-o", "dist/objetivos.css", "--minify"], {
  cwd: here,
  stdio: "inherit",
});

// Dentro de un Shadow DOM, `:host:where([data-theme=dark])` no casa nunca (el anfitrión no admite
// selectores compuestos): los temas de Plane se aplican también a nuestro contenedor .plane-root.
const cssFile = join(here, "dist/objetivos.css");
writeFileSync(cssFile, readFileSync(cssFile, "utf8").replaceAll(":is(:root,:host):where(", ":is(:root,:host,.plane-root):where("));

rmSync(out, { recursive: true, force: true });
mkdirSync(out, { recursive: true });
await build({
  entryPoints: [join(here, "src/entry.tsx")],
  bundle: true,
  format: "iife",
  minify: true,
  jsx: "automatic",
  tsconfig: join(here, "tsconfig.json"),
  define: { "process.env.NODE_ENV": '"production"' },
  loader: { ".css": "text" },
  alias: {
    // Las piezas de Plane están en ../plane: al buscar `react` desde ahí se llegaría antes al React 18
    // de la raíz de la app (el de Excalidraw) que al nuestro. Todo usa el React 19 de esta carpeta.
    react: join(here, "node_modules/react"),
    "react-dom": join(here, "node_modules/react-dom"),
    "@plane/utils": join(here, "src/plane-utils.ts"),
    "@plane/blocks/layout": join(plane, "blocks/src/layout/index.ts"),
    "@plane/blocks/icons": join(plane, "blocks/src/icons/index.ts"),
  },
  // Las piezas de Plane están fuera de esta carpeta: que sus imports se resuelvan con nuestros node_modules.
  nodePaths: [join(here, "node_modules")],
  outfile: join(out, "objetivos.js"),
  logLevel: "warning",
});
console.log("objetivos lista en ui/vendor/objetivos");
