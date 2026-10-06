#Requires -Version 5.1
<#
registrar-actualizacion.ps1 - Crea tarea programada SEMANAL de auto-update
(sin admin, solo para el usuario actual).

El instalador (instalar.ps1) ya registra esta tarea automaticamente en TODOS
los equipos; este script es solo para registro manual o re-registro.

Copia actualizar.ps1 a %LOCALAPPDATA%\hermes\bin\hermes-autoupdate.ps1 para
que la tarea siga funcionando aunque se borre la carpeta del repo.
#>
$scriptPath = Split-Path -Parent $MyInvocation.MyCommand.Path
if ($scriptPath.StartsWith('\\?\')) { $scriptPath = $scriptPath.Substring(4) }
$actualizar = Join-Path $scriptPath "actualizar.ps1"

if (-not (Test-Path $actualizar)) { Write-Host "No se encontro actualizar.ps1 junto a este script"; exit 1 }

$binDir = Join-Path $env:LOCALAPPDATA "hermes\bin"
if (-not (Test-Path $binDir)) { New-Item -ItemType Directory -Path $binDir -Force | Out-Null }
$dest = Join-Path $binDir "hermes-autoupdate.ps1"
Copy-Item $actualizar $dest -Force

# Comilla-dentro-de-comilla PS (")") para schtasks /TR con rutas con espacios
$tr = 'powershell.exe -NoProfile -ExecutionPolicy Bypass -File "{0}" -Silent' -f $dest
schtasks /Create /F /SC WEEKLY /D MON /ST 09:00 /TN "Hermes AutoUpdate" /TR $tr

if ($LASTEXITCODE -eq 0) {
  Write-Host ""
  Write-Host "[OK] Tarea 'Hermes AutoUpdate' creada: cada lunes 09:00" -ForegroundColor Green
  Write-Host "     Log: %LOCALAPPDATA%\hermes\logs\hermes-autoupdate.log"
  Write-Host "     Eliminar: schtasks /Delete /TN `"Hermes AutoUpdate`" /F"
} else {
  Write-Host "[!] schtasks fallo (exit $LASTEXITCODE)" -ForegroundColor Yellow
}
