#!/bin/bash
# Prueba: Plane sin Docker (Postgres portable + Python), como lo haría la app.
set -euo pipefail
R=/var/tmp/plane-embebido
REPO=$(cd "$(dirname "$0")/../.." && pwd)
API_SRC=$REPO/objetivos/plane/apps/api
PG_URL=https://github.com/theseus-rs/postgresql-binaries/releases/download/16.15.0/postgresql-16.15.0-x86_64-unknown-linux-gnu.tar.gz
PORT_PG=55432
PORT_API=8110
mkdir -p "$R"; cd "$R"
t0=$(date +%s)

echo "== uv"
if [ ! -x "$R/uv" ]; then
  curl -LsSf https://github.com/astral-sh/uv/releases/download/0.12.21/uv-x86_64-unknown-linux-gnu.tar.gz | tar -xz --strip-components=1 -C "$R" uv-x86_64-unknown-linux-gnu/uv
fi

echo "== código de la API"
rm -rf api && mkdir api
(cd "$API_SRC" && tar --exclude=__pycache__ --exclude=./plane/tests --exclude=./logs -cf - manage.py plane templates pyproject.toml uv.lock) | tar -xf - -C api

echo "== python + dependencias"
if [ ! -d venv ]; then
  ./uv venv -q --python 3.12 venv
fi
(cd api && ../uv export -q --frozen --no-dev --no-default-groups --no-hashes --no-emit-project -o ../req.txt 2>/dev/null || ../uv export --frozen --no-default-groups --no-hashes -o ../req.txt)
VIRTUAL_ENV=$R/venv ./uv pip install -q -r req.txt psycopg-binary==3.3.4 fakeredis==2.26.2
du -sh venv

echo "== postgres"
if [ ! -d pg ]; then
  mkdir pg && curl -LsSf "$PG_URL" | tar -xz --strip-components=1 -C pg
fi
du -sh pg
id -u plane >/dev/null 2>&1 || useradd -r -M plane
mkdir -p datos && chown plane datos
if [ ! -f datos/PG_VERSION ]; then
  su plane -s /bin/bash -c "$R/pg/bin/initdb -D $R/datos -U plane --auth=trust -E UTF8 >/dev/null"
fi
su plane -s /bin/bash -c "$R/pg/bin/pg_ctl -D $R/datos -o '-p $PORT_PG -k /tmp -c listen_addresses=127.0.0.1' -l $R/datos/pg.log -w start" >/dev/null || true
su plane -s /bin/bash -c "$R/pg/bin/createdb -h 127.0.0.1 -p $PORT_PG -U plane plane" 2>/dev/null || true

export DJANGO_SETTINGS_MODULE=plane.settings.escritorio
export DATABASE_URL="postgresql://plane@127.0.0.1:$PORT_PG/plane"
export SECRET_KEY=prueba-$(head -c 16 /dev/urandom | od -An -tx1 | tr -d ' \n')
export WEB_URL=http://127.0.0.1:$PORT_API
cd api
echo "== migrate"
t1=$(date +%s)
../venv/bin/python manage.py migrate --noinput 2>&1 | tail -3
echo "migrate: $(( $(date +%s) - t1 )) s"
echo "== preparar"
TOKEN=$(../venv/bin/python manage.py shell < "$REPO/objetivos/servidor/preparar.py" 2>/dev/null | tail -1)
echo "token: ${#TOKEN} caracteres"
echo "== api"
pkill -f "uvicorn plane.asgi" 2>/dev/null || true
nohup ../venv/bin/python -m uvicorn plane.asgi:application --host 127.0.0.1 --port $PORT_API > "$R/api.log" 2>&1 &
for i in $(seq 1 40); do curl -s -o /dev/null "http://127.0.0.1:$PORT_API/" && break; sleep 0.5; done
H="X-Api-Key: $TOKEN"
B=http://127.0.0.1:$PORT_API/api/v1/workspaces/objetivos
echo "proyectos (vacío):"; curl -s -H "$H" "$B/projects/" | head -c 300; echo
P=$(curl -s -H "$H" -H 'Content-Type: application/json' -X POST "$B/projects/" -d '{"name":"Prueba","identifier":"PRU"}' | ../venv/bin/python -c 'import json,sys; print(json.load(sys.stdin)["id"])')
echo "proyecto: $P"
curl -s -H "$H" -H 'Content-Type: application/json' -X POST "$B/projects/$P/issues/" -d '{"name":"Primer objetivo"}' | head -c 200; echo
echo "estados:"; curl -s -H "$H" "$B/projects/$P/states/" | ../venv/bin/python -c 'import json,sys; print([s["name"] for s in json.load(sys.stdin)["results"]])'
echo "issues:"; curl -s -H "$H" "$B/projects/$P/issues/" | ../venv/bin/python -c 'import json,sys; print([i["name"] for i in json.load(sys.stdin)["results"]])'
echo "memoria api (MB): $(ps -o rss= -p $(pgrep -f 'uvicorn plane.asgi' | head -1) | awk '{print int($1/1024)}')"
echo "total: $(( $(date +%s) - t0 )) s"
tail -5 "$R/api.log"
