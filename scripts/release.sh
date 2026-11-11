#!/bin/bash
# Releases de Chat de agentes (docs/releases.md).
#
#   scripts/release.sh subir <X.Y.Z>             pone la versión en package.json, Cargo.toml y tauri.conf.json
#                                                (luego: commit y push a main)
#   scripts/release.sh publicar <X.Y.Z> "notas"  lanza el workflow Release en GitHub (compila, firma y publica)
#   scripts/release.sh secretos                  sube a GitHub la clave de firma (~/.tauri/chat-agentes.key*)
set -euo pipefail

HERE="$(cd "$(dirname "$0")/.." && pwd)"
REPO="ramosjr18/IDE-Workspace"
cmd="${1:-}"; ver="${2:-}"

need_version() {
  [[ "$ver" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || { echo "falta la versión X.Y.Z"; exit 1; }
}

subir() {
  need_version
  # Cargo.toml: solo la primera línea «version = …» (la del paquete); Cargo.lock: la entrada del paquete.
  sed -i -E "0,/^version = \"[^\"]+\"/s//version = \"$ver\"/" "$HERE/src-tauri/Cargo.toml"
  if [ -f "$HERE/src-tauri/Cargo.lock" ]; then
    sed -i -E "/^name = \"ide-workspace-chat\"$/{n;s/^version = \"[^\"]+\"/version = \"$ver\"/}" "$HERE/src-tauri/Cargo.lock"
  fi
  # Los JSON con node, para no tocar más que su versión.
  (cd "$HERE" && VER="$ver" node -e '
    const fs = require("fs");
    for (const f of ["package.json", "src-tauri/tauri.conf.json", "package-lock.json"]) {
      if (!fs.existsSync(f)) continue;
      const j = JSON.parse(fs.readFileSync(f, "utf8"));
      j.version = process.env.VER;
      if (j.packages && j.packages[""]) j.packages[""].version = process.env.VER;
      fs.writeFileSync(f, JSON.stringify(j, null, 2) + String.fromCharCode(10));
    }')
  grep -m1 -E '^version = ' "$HERE/src-tauri/Cargo.toml"
  grep -m1 '"version"' "$HERE/package.json" "$HERE/src-tauri/tauri.conf.json"
  echo "Versión $ver. Ahora: git commit -am \"Versión $ver\" && git push, y scripts/release.sh publicar $ver \"notas\""
}

publicar() {
  need_version
  local notas="${3:-Mejoras varias.}"
  gh workflow run release.yml -R "$REPO" --ref main -f version="$ver" -f notas="$notas"
  echo "Lanzado. Síguelo con: gh run watch -R $REPO \$(gh run list -R $REPO -w release.yml -L 1 --json databaseId -q '.[0].databaseId')"
}

secretos() {
  local key="$HOME/.tauri/chat-agentes.key"
  [ -f "$key" ] && [ -f "$key.password" ] || { echo "no encuentro $key y $key.password"; exit 1; }
  gh secret set TAURI_SIGNING_PRIVATE_KEY -R "$REPO" < "$key"
  gh secret set TAURI_SIGNING_PRIVATE_KEY_PASSWORD -R "$REPO" < "$key.password"
  echo "Secretos de firma subidos a $REPO"
}

case "$cmd" in
  subir) subir ;;
  publicar) publicar "$@" ;;
  secretos) secretos ;;
  *) echo "uso: $0 {subir <X.Y.Z> | publicar <X.Y.Z> \"notas\" | secretos}"; exit 1 ;;
esac
