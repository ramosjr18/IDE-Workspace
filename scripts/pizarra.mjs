// Empaqueta la vista de pizarra del tablero del consejo (IDE-4) con esbuild y copia sus fuentes.
import { build } from "esbuild";
import { cpSync, rmSync, mkdirSync } from "node:fs";
import { dirname, join } from "node:path";
import { createRequire } from "node:module";

// Borra y vuelve a crear ui/vendor/pizarra para que el bundle quede limpio.
rmSync("ui/vendor/pizarra", { recursive: true, force: true });
mkdirSync("ui/vendor/pizarra", { recursive: true });

// Empaqueta entry.jsx como IIFE de producción: genera pizarra.js, pizarra.css y fuentes .woff2 al lado.
await build({
  entryPoints: ["pizarra/entry.jsx"],
  bundle: true,
  format: "iife",
  minify: true,
  conditions: ["production"],
  define: {
    "process.env.NODE_ENV": '"production"',
    "import.meta.env": '{"MODE":"production","DEV":false,"PROD":true}',
  },
  loader: { ".woff2": "file" },
  outfile: "ui/vendor/pizarra/pizarra.js",
  logLevel: "warning",
});

// Localiza el paquete de Excalidraw. 0.18.1 no exporta ./package.json, así que se resuelve la entrada ".".
const distProd = dirname(createRequire(import.meta.url).resolve("@excalidraw/excalidraw"));
// Copia la carpeta de fuentes de dibujo (<paquete>/dist/prod/fonts) junto al bundle.
cpSync(join(distProd, "fonts"), "ui/vendor/pizarra/fonts", { recursive: true });

// Aviso final del empaquetado.
console.log("pizarra lista en ui/vendor/pizarra");
