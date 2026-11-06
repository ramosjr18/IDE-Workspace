# Versión estable en WSL

Una copia fija de la app para trabajar a diario. No cambia aunque se modifique el repo: solo cuando se instala otra versión.

- Vive en `/opt/chat-agentes/<commit>`. `/opt/chat-agentes/actual` apunta a la que se abre.
- Corre en WSL (Ubuntu-24.04, usuario `daniel`) y abre carpetas de Windows por `/mnt/c/...`.
- Sigue el tema claro u oscuro de Windows al abrirse.

## Abrirla

Cualquiera de estas:

- **Acceso directo** `Chat de agentes` del escritorio de Windows.
- **Desde Windows** (PowerShell, Ejecutar o Win+R):

      wsl -d Ubuntu-24.04 -u daniel --cd ~ -- /usr/local/bin/chat-agentes

- **Desde una terminal de WSL**: `chat-agentes`.

Si el acceso directo se pierde, se vuelve a crear con PowerShell:

    $s = (New-Object -ComObject WScript.Shell).CreateShortcut("$([Environment]::GetFolderPath('Desktop'))\Chat de agentes.lnk")
    $s.TargetPath = "C:\Windows\System32\wsl.exe"
    $s.Arguments = "-d Ubuntu-24.04 -u daniel --cd ~ -- /usr/local/bin/chat-agentes"
    $s.Save()

Para saber qué versión está abierta: `cat /opt/chat-agentes/actual/VERSION` (en WSL).

## Actualizarla

Desde Windows (PowerShell). Se ejecuta como root, porque el usuario `daniel` de WSL no tiene sudo:

    wsl -d Ubuntu-24.04 -u root --cd /mnt/c/Users/daniel.ramos/workspace/IDE-Workspace -- bash scripts/wsl/instalar-estable.sh          # la última de main
    wsl -d Ubuntu-24.04 -u root --cd /mnt/c/Users/daniel.ramos/workspace/IDE-Workspace -- bash scripts/wsl/instalar-estable.sh 1a2b3c4  # un commit concreto

Tarda unos 3 minutos. Compila una copia fija del commit (lo que haya sin commitear no entra), la deja en `/opt/chat-agentes/<commit>` y cambia `actual`. Después hay que cerrar la app y volver a abrirla.

Conversaciones, pizarras, consejos y configuración no se pierden: están en `~/.ide-workspace-chat`, fuera de la app.

## Volver a una versión anterior

Las versiones instaladas se quedan en `/opt/chat-agentes/`. Desde Windows:

    wsl -d Ubuntu-24.04 -u root -- ls /opt/chat-agentes/
    wsl -d Ubuntu-24.04 -u root -- ln -sfn /opt/chat-agentes/<commit> /opt/chat-agentes/actual

## Cómo está montada

- `/usr/local/bin/chat-agentes` (copia de `scripts/wsl/chat-agentes`): pone `WEBKIT_DISABLE_DMABUF_RENDERER=1`, que evita la ventana en blanco en WSLg, y el tema oscuro de GTK si Windows lo usa. Después abre `/opt/chat-agentes/actual/chat-agentes`.
- Las CLIs (claude, codex, copilot, gh, az…), git y sus credenciales están configuradas en WSL para el usuario `daniel` y comparten sesión con las de Windows.
