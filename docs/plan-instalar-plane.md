# Plan: Plane instalado y configurado con la app, sin hacer nada

Objetivo: **descargar la app y que la pestaña Objetivo funcione sin hacer nada más**. Nada de Docker
ni de instalar Postgres a mano: la app trae su propio Plane, crea su base de datos y lo configura,
como las apps que crean su propia instancia de base de datos al instalarse.

Contexto: `docs/plan-plane-objetivo.md` (integración de la vista) y `docs/releases.md` (releases).

## Prueba hecha (11/11/2026, WSL)

Plane **sin Docker**, solo con un Postgres portable y Python:

| Paso | Resultado |
|---|---|
| Postgres 16 portable (`theseus-rs/postgresql-binaries`, 11 MB comprimido) | `initdb` + arranque en `127.0.0.1:55432`, como usuario normal |
| Dependencias de Plane (`uv.lock`, sin grupo de producción) + `psycopg-binary` + `fakeredis` | 228 MB instalado |
| `manage.py migrate` desde cero | **91 s** (solo la primera vez) |
| `preparar.py` (usuario, espacio `objetivos`, clave de API) | OK |
| API con `uvicorn plane.asgi` en `127.0.0.1:8110` | Proyectos, estados, crear proyecto y crear objetivo: OK (las tareas de actividad se ejecutan en línea) |
| Memoria de la API | 163 MB |
| Todo comprimido (dependencias + Postgres + código) | **76 MB** (+ ~25 MB del Python portable) |

## Prueba hecha (11/11/2026, Windows 11 nativo, sin WSL)

| Paso | Resultado |
|---|---|
| uv, Python 3.12 portable, Postgres 16 portable (`x86_64-pc-windows-msvc`) | Se ejecutan con Smart App Control activo |
| Dependencias (`uv pip install --target site`) | 154 MB, unos segundos (con caché) |
| `initdb` + Postgres en `127.0.0.1:55433` | OK, sin servicio de Windows ni administrador |
| `manage.py migrate` desde cero | **122 s** (solo la primera vez) |
| `preparar.py` | OK, con `manage.py shell -c` |
| API: crear proyecto, crear objetivo, listar estados y objetivos | OK |
| Memoria de la API | 167 MB |

Lo que hay que hacer distinto en Windows:

- **Sin entorno virtual.** Smart App Control bloquea el `python.exe` que crea `venv` (un lanzador
  sin reputación, error 4551); el Python portable sí se ejecuta. El paquete trae Python y una carpeta
  `site` con las dependencias, y la app lo lanza con `PYTHONPATH=site`. Igual en todas las plataformas.
- **`preparar.py` con `-c`**, no por la entrada estándar: en Windows `manage.py shell` abre la consola
  interactiva y rompe los bloques.
- La app lanza Postgres y uvicorn desde Rust, sin ventana (`CREATE_NO_WINDOW`), y espera solo a
  `pg_ctl` (no a sus hijos).

Lo que cambia en nuestra copia de Plane (`objetivos/plane`):

- `plane/settings/escritorio.py`: un solo proceso. Caché en memoria, tareas de Celery en el momento
  (`CELERY_TASK_ALWAYS_EAGER`, como en los tests de Plane), sin cola ni worker, solo `127.0.0.1`.
- `plane/settings/redis.py`: con `REDIS_URL=memory://`, un Redis en memoria del proceso (`fakeredis`).

Sin Redis, RabbitMQ ni MinIO. Lo que se pierde: subir archivos adjuntos (MinIO) y las tareas
periódicas (beat): limpiezas y correos, que en un equipo local no hacen falta.

## Cómo funcionará

### En cada release (CI)

Un job por plataforma (Linux x86_64, Windows x86_64, macOS arm64 e Intel) construye
`objetivos-plane-<versión>-<plataforma>.tar.gz` con:

- Python 3.12 portable (`python-build-standalone`) y las dependencias de Plane ya instaladas;
- el código de la API de Plane (con los dos cambios de arriba y sin telemetría);
- Postgres 16 portable.

Se sube a la release y su sha256 va en `latest.json`, como el binario de la app.

### Al abrir la app (en segundo plano, sin pedir nada)

1. **Primera vez**: descarga el paquete de su plataforma (comprobando el sha256) y lo descomprime en
   la carpeta de datos de la app (`~/.ide-workspace-chat/plane/<versión>`).
2. **Base de datos**: si no existe, `initdb` en `~/.ide-workspace-chat/plane/datos`, solo para el
   usuario y en `127.0.0.1`. Arranca Postgres en un puerto libre.
3. **Migraciones**: `manage.py migrate` (la primera vez, ~1,5 min; en cada versión nueva de Plane,
   solo las nuevas).
4. **Preparar** (la primera vez): usuario local, espacio `objetivos`, clave de API, y `plane.json` (600).
   `SECRET_KEY` aleatoria guardada junto a los datos.
5. **API**: `uvicorn` en `127.0.0.1` y un puerto libre.
6. Al cerrar la app: para la API y Postgres (`pg_ctl stop -m fast`).

Mientras tanto, la pestaña Objetivo enseña el progreso («Descargando Objetivos… 40 %», «Preparando la
base de datos…»). Si algo falla, lo dice con un botón «Reintentar»: cada paso se puede repetir.

### Actualizaciones

Una release nueva de la app trae, si cambia, un Plane nuevo: se descarga, se migra la base de datos
(los datos se conservan) y se borra la versión anterior.

## Decisiones tomadas

- **Sin Docker.** Docker no está en todos los equipos y no se puede instalar sin pedir permiso.
- **El código de la API de Plane pasa al repo** (`objetivos/plane-api/`, ~5 MB sin tests): el CI lo
  necesita para construir el paquete. AGPL-3.0 con su LICENSE, COPYRIGHT y cabeceras, como la vista.
- **Todo sale de nuestras releases** (también Postgres): una sola fuente, con sha256.
- El servidor con Docker (`objetivos/servidor`) se queda solo para desarrollo, o se retira.

## Fases

| # | Fase | Estado |
|---|---|---|
| 0 | Pruebas en WSL y Windows (arriba) | **Hecho** |
| 1 | Código de la API en el repo (`objetivos/plane-api/`) + `escritorio.py` + `redis.py` | Pendiente: hecho solo en la copia local (ver abajo) |
| 2 | Script que construye el paquete de una plataforma (`scripts/plane-paquete.sh`), probado en WSL | Pendiente |
| 3 | `src-tauri/src/plane_local.rs`: descargar, `initdb`, arrancar, migrar, preparar, parar; eventos de progreso; tests | Pendiente |
| 4 | Pestaña Objetivo: progreso y errores, botón «Reintentar» | Pendiente |
| 5 | Job en `release.yml` para las cuatro plataformas; probar en macOS | Pendiente |
| 6 | Restaurar una base de datos ya migrada del paquete en vez de migrar desde cero (91–122 s → segundos) | Pendiente |

Por decidir al llegar a la fase 3: ¿se pasan a la base de datos nueva los objetivos de prueba del Plane
con Docker (proyecto «IDE Workspace», 7 objetivos) o se empieza de cero?

## Cómo retomarlo

### Lo que solo está en este equipo

`objetivos/plane` (Plane v1.5.0-rc1, sin remote) está en `.gitignore`. Sus commits locales:

- `1b96587` Plane v1.5.0-rc1: punto de partida.
- `de2689d` Nada sale del equipo: sin telemetría ni consultas a Plane; sin sus archivos de repo.
- `e34aa8f` Un solo proceso sin Redis, RabbitMQ ni MinIO:
  - `apps/api/plane/settings/escritorio.py` (nuevo): `REDIS_URL=memory://` por defecto,
    `from .common import *`, `DEBUG = False`, `ALLOWED_HOSTS = ["127.0.0.1", "localhost"]`, caché
    `LocMemCache`, `CELERY_BROKER_URL = "memory://"`, `CELERY_TASK_ALWAYS_EAGER = True`,
    `CELERY_TASK_EAGER_PROPAGATES = False`, `CELERY_RESULT_BACKEND = None`, registros a la consola
    (WARNING). Con la cabecera de licencia de Plane (AGPL-3.0-only).
  - `apps/api/plane/settings/redis.py`: al principio de `redis_instance()`, si `settings.REDIS_URL`
    empieza por `memory://`, devuelve un `fakeredis.FakeRedis()` compartido (variable global `_memoria`).

Si se pierde la copia, se rehace desde Plane v1.5.0-rc1 con estos dos cambios. En la fase 1 pasan al repo.

### Versiones y receta

- uv **0.12.21** (Plane lo exige exactamente), Python 3.12 (`uv python install 3.12`).
- Postgres **16.15.0** de `theseus-rs/postgresql-binaries` (`x86_64-unknown-linux-gnu`,
  `x86_64-pc-windows-msvc`, y los de macOS).
- Dependencias: en `apps/api`, `uv export --frozen --no-default-groups --no-hashes --no-emit-project`,
  y después `uv pip install --python <python portable> --target site -r req.txt psycopg-binary==3.3.4 fakeredis==2.26.2`.
- Código de la API: `manage.py`, `plane/` (sin `plane/tests`), `templates/`, `pyproject.toml`, `uv.lock`.
- Base de datos: `initdb -D datos -U plane --auth=trust -E UTF8`; arrancar con
  `pg_ctl -D datos -o "-p <puerto> -c listen_addresses=127.0.0.1" -l datos/pg.log -w start`; `createdb plane`.
  En Linux `initdb` no puede ejecutarse como root (la app corre como el usuario, así que no pasa).
- Entorno de Plane: `DJANGO_SETTINGS_MODULE=plane.settings.escritorio`,
  `DATABASE_URL=postgresql://plane@127.0.0.1:<puerto>/plane`, `SECRET_KEY` aleatoria (guardada con los
  datos), `WEB_URL=http://127.0.0.1:<puerto api>`, `PYTHONPATH=site`, `PYTHONUTF8=1`.
- Pasos: `manage.py migrate --noinput`; `manage.py shell -c "<objetivos/servidor/preparar.py>"`
  (la última línea es la clave de API → `plane.json`, permisos 600);
  `python -m uvicorn plane.asgi:application --host 127.0.0.1 --port <puerto api>`.
- Parar: matar uvicorn y `pg_ctl -D datos -m fast stop`.

Scripts de las pruebas: `docs/plane-pruebas/plane-wsl.sh` (como root en WSL, en `/var/tmp/plane-embebido`,
puertos 55432/8110) y `docs/plane-pruebas/plane-windows.ps1` (en `%LOCALAPPDATA%\plane-prueba`, puertos
55433/8111). Las rutas están escritas para este equipo.

### Restos de las pruebas (se pueden borrar)

- WSL: `/var/tmp/plane-embebido` (Postgres parado) y el usuario de sistema `plane`.
- Windows: `%LOCALAPPDATA%\plane-prueba` (Postgres y API parados).
- `uv` deja caché en `%LOCALAPPDATA%\uv` y `~/.cache/uv`.

## Riesgos

- **Windows**: probado (arriba), funciona. Queda por ver lo que no usó la prueba (vistas, ciclos…).
- **macOS**: sin probar; Gatekeeper puede poner en cuarentena los binarios descargados (quitar
  `com.apple.quarantine` al descomprimir).
- **Tamaño**: ~100 MB la primera vez (solo una vez por versión de Plane).
- **Antivirus/SmartScreen**: ejecutar `postgres.exe` y `python.exe` descargados puede dar avisos
  hasta firmar el código.
