#!/bin/bash
# Compila la versión release de un commit y la instala como versión estable en WSL:
# /opt/chat-agentes/<commit>, con el enlace /opt/chat-agentes/actual apuntando a ella.
# Uso (en WSL, como root, desde el repo): bash scripts/wsl/instalar-estable.sh [commit]   (por defecto main)
# Necesita Rust (rustup) y Node en root. Las versiones anteriores se quedan para poder volver.
set -e
# El repo es de daniel y esto corre como root: git no se fía de él sin safe.directory.
git() { command git -c safe.directory='*' "$@"; }
repo=$(git rev-parse --show-toplevel)
commit=$(git -C "$repo" rev-parse --short "${1:-main}")
src=/root/build/$commit
rm -rf "$src" && mkdir -p "$src"
# Copia fija de ese commit: lo que haya sin commitear en el repo no entra.
git -C "$repo" archive --format=tar "$commit" | tar -xf - -C "$src"
cd "$src"
npm ci --no-audit --no-fund >/dev/null 2>&1
source /root/.cargo/env
export CARGO_TARGET_DIR=/root/target-prod
npx tauri build --no-bundle 2>&1 | tail -4
dest=/opt/chat-agentes/$commit
mkdir -p "$dest"
install -m 755 "$CARGO_TARGET_DIR/release/ide-workspace-chat" "$dest/chat-agentes"
echo "$commit $(date -Iseconds)" > "$dest/VERSION"
ln -sfn "$dest" /opt/chat-agentes/actual
install -m 755 "$repo/scripts/wsl/chat-agentes" /usr/local/bin/chat-agentes
echo "Instalada la versión estable $commit. Cierra la app y vuelve a abrirla."
