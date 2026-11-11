// Compila la pestaña Objetivo (objetivos/vista → ui/vendor/objetivos/objetivos.js) dentro de `npm run vendor`.
// La vista tiene su propio package.json (React 19 y @makeplane/propel; la app usa React 18 por Excalidraw):
// sus dependencias se instalan la primera vez o cuando cambia su package-lock.json.
import { execSync } from "node:child_process";
import { existsSync, readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";

const vista = "objetivos/vista";
const lock = join(vista, "package-lock.json");
const stamp = join(vista, "node_modules/.lock-instalado");
const want = readFileSync(lock, "utf8");
if (!existsSync(stamp) || readFileSync(stamp, "utf8") !== want) {
  execSync("npm ci --no-audit --no-fund", { cwd: vista, stdio: "inherit" });
  writeFileSync(stamp, want);
}
execSync(`"${process.execPath}" build.mjs`, { cwd: vista, stdio: "inherit" });
