# Prueba: Plane sin Docker en Windows (Postgres portable + Python), como lo haría la app.
$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
$R = Join-Path $env:LOCALAPPDATA 'plane-prueba'
$Repo = (Resolve-Path "$PSScriptRoot\..\..").Path
$ApiSrc = "$Repo\objetivos\plane\apps\api"
$PgUrl = 'https://github.com/theseus-rs/postgresql-binaries/releases/download/16.15.0/postgresql-16.15.0-x86_64-pc-windows-msvc.zip'
$PortPg = 55433
$PortApi = 8111
New-Item -ItemType Directory -Force $R | Out-Null
Set-Location $R
$t0 = Get-Date
function Paso($m) { Write-Output "== $m  ($([int]((Get-Date) - $t0).TotalSeconds) s)" }

Paso 'uv'
if (-not (Test-Path "$R\uv.exe")) {
  Invoke-WebRequest 'https://github.com/astral-sh/uv/releases/download/0.12.21/uv-x86_64-pc-windows-msvc.zip' -OutFile uv.zip
  Expand-Archive uv.zip -DestinationPath . -Force
  Remove-Item uv.zip
}
& .\uv.exe --version

Paso 'código de la API'
if (Test-Path api) { Remove-Item -Recurse -Force api }
New-Item -ItemType Directory api | Out-Null
Copy-Item "$ApiSrc\manage.py", "$ApiSrc\pyproject.toml", "$ApiSrc\uv.lock" api\
Copy-Item -Recurse "$ApiSrc\plane", "$ApiSrc\templates" api\
Remove-Item -Recurse -Force api\plane\tests -ErrorAction SilentlyContinue
Get-ChildItem api -Recurse -Directory -Filter __pycache__ | Remove-Item -Recurse -Force

Paso 'python + dependencias'
$env:UV_PYTHON_INSTALL_DIR = "$R\python"
& .\uv.exe python install -q 3.12
$basePy = (Get-ChildItem "$R\python" -Recurse -Filter python.exe | Select-Object -First 1).FullName
Push-Location api
& ..\uv.exe export -q --frozen --no-default-groups --no-hashes --no-emit-project -o ..\req.txt
Pop-Location
# Sin entorno virtual: Smart App Control bloquea su lanzador python.exe (sin reputación).
& .\uv.exe pip install -q --python $basePy --target "$R\site" -r req.txt psycopg-binary==3.3.4 fakeredis==2.26.2
if ($LASTEXITCODE) { throw "pip install falló" }
Write-Output ("site: {0:N0} MB" -f ((Get-ChildItem site -Recurse -File | Measure-Object Length -Sum).Sum / 1MB))

Paso 'postgres'
if (-not (Test-Path pg\bin\postgres.exe)) {
  Invoke-WebRequest $PgUrl -OutFile pg.zip
  Expand-Archive pg.zip -DestinationPath pgtmp -Force
  Remove-Item pg.zip
  $inner = Get-ChildItem pgtmp -Directory | Select-Object -First 1
  Move-Item $inner.FullName pg
  Remove-Item -Recurse -Force pgtmp
}
if (-not (Test-Path datos\PG_VERSION)) {
  & .\pg\bin\initdb.exe -D datos -U plane --auth=trust -E UTF8 | Out-Null
  if ($LASTEXITCODE) { throw "initdb falló" }
}
# Sin tuberías: el postgres que queda en marcha heredaría la salida y PowerShell esperaría para siempre.
$st = Start-Process "$R\pg\bin\pg_ctl.exe" -ArgumentList "-D", "$R\datos", "-o", "`"-p $PortPg -c listen_addresses=127.0.0.1`"", "-l", "$R\datos\pg.log", "-w", "start" -PassThru -WindowStyle Hidden
# -Wait esperaría también al postgres hijo (que no termina): solo esperamos a pg_ctl.
$st.WaitForExit(60000) | Out-Null
if (-not (& .\pg\bin\pg_isready.exe -h 127.0.0.1 -p $PortPg)) { throw "postgres no arranca" }
& .\pg\bin\createdb.exe -h 127.0.0.1 -p $PortPg -U plane plane 2>$null

$env:DJANGO_SETTINGS_MODULE = 'plane.settings.escritorio'
$env:DATABASE_URL = "postgresql://plane@127.0.0.1:$PortPg/plane"
$env:SECRET_KEY = 'prueba-' + [guid]::NewGuid().ToString('N')
$env:WEB_URL = "http://127.0.0.1:$PortApi"
$env:PYTHONUTF8 = '1'
$py = $basePy
$env:PYTHONPATH = "$R\site"
Set-Location api

Paso 'migrate'
& $py manage.py migrate --noinput 2>&1 | Select-Object -Last 2
if ($LASTEXITCODE) { throw "migrate falló" }

Paso 'preparar'
# Con -c: en Windows, por stdin, manage.py shell abre la consola interactiva y se come los bloques.
$prep = Get-Content "$Repo\objetivos\servidor\preparar.py" -Raw
$token = (& $py manage.py shell -c $prep 2>$null | Select-Object -Last 1).Trim()
Write-Output "token: $($token.Length) caracteres"

Paso 'api'
$api = Start-Process -FilePath $py -ArgumentList '-m', 'uvicorn', 'plane.asgi:application', '--host', '127.0.0.1', '--port', $PortApi -WorkingDirectory "$R\api" -RedirectStandardError "$R\api.err" -RedirectStandardOutput "$R\api.out" -PassThru -WindowStyle Hidden
foreach ($i in 1..60) { try { Invoke-WebRequest "http://127.0.0.1:$PortApi/" -UseBasicParsing -TimeoutSec 2 | Out-Null; break } catch { if ($_.Exception.Response) { break }; Start-Sleep -Milliseconds 500 } }
$H = @{ 'X-Api-Key' = $token }
$B = "http://127.0.0.1:$PortApi/api/v1/workspaces/objetivos"
$p = Invoke-RestMethod -Method Post "$B/projects/" -Headers $H -ContentType 'application/json' -Body '{"name":"Prueba Windows","identifier":"WIN"}'
Write-Output "proyecto: $($p.id)"
$i = Invoke-RestMethod -Method Post "$B/projects/$($p.id)/issues/" -Headers $H -ContentType 'application/json' -Body '{"name":"Primer objetivo en Windows"}'
Write-Output "objetivo: $($i.name)"
Write-Output ("estados: " + ((Invoke-RestMethod "$B/projects/$($p.id)/states/" -Headers $H).results.name -join ', '))
Write-Output ("objetivos: " + ((Invoke-RestMethod "$B/projects/$($p.id)/issues/" -Headers $H).results.name -join ', '))
Write-Output ("memoria api: {0:N0} MB" -f ((Get-Process -Id $api.Id).WorkingSet64 / 1MB))
Paso 'fin'
Get-Content "$R\api.err" -Tail 5


