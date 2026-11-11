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

1. Código de la API en el repo + `escritorio.py` + `redis.py` (hecho en la copia local; falta pasarlo).
2. Script que construye el paquete para una plataforma (`scripts/plane-paquete.sh`), probado en WSL.
3. `src-tauri/src/plane_local.rs`: descargar, `initdb`, arrancar, migrar, preparar, parar; eventos de
   progreso; tests.
4. Pestaña Objetivo: progreso y errores.
5. Job en `release.yml` para las cuatro plataformas (Windows y macOS: probar `initdb` y `uvicorn` allí).
6. Mejora: en vez de migrar desde cero (91 s), restaurar una base de datos ya migrada que venga en el
   paquete (segundos).

## Riesgos

- **Windows**: Plane solo se prueba en Linux (Docker). Postgres y uvicorn funcionan en Windows, pero
  algún módulo de Plane puede dar problemas; se verá en la fase 5.
- **Tamaño**: ~100 MB la primera vez (solo una vez por versión de Plane).
- **Antivirus/SmartScreen**: ejecutar `postgres.exe` y `python.exe` descargados puede dar avisos
  hasta firmar el código.
