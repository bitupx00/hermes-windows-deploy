#Requires -Version 5.1
<#
registrar-actualizacion.ps1 - Crea tarea programada SEMANAL de auto-update
(sin admin, solo para el usuario actual).
#>
$scriptPath = Split-Path -Parent $MyInvocation.MyCommand.Path
$actualizar = Join-Path $scriptPath "actualizar.ps1"

if (-not (Test-Path $actualizar)) { Write-Host "No se encontro actualizar.ps1 junto a este script"; exit 1 }

# Comillas escapadas \" — requisito de schtasks /TR cuando la ruta tiene espacios
$tr = "powershell.exe -NoProfile -ExecutionPolicy Bypass -File \`"$actualizar\`" -Silent"
schtasks /Create /F /SC WEEKLY /D MON /ST 09:00 /TN "Hermes AutoUpdate" /TR $tr

if ($LASTEXITCODE -eq 0) {
  Write-Host ""
  Write-Host "[OK] Tarea 'Hermes AutoUpdate' creada: cada lunes 09:00" -ForegroundColor Green
  Write-Host "     Log: %LOCALAPPDATA%\hermes\logs\hermes-autoupdate.log"
  Write-Host "     Eliminar: schtasks /Delete /TN `"Hermes AutoUpdate`" /F"
} else {
  Write-Host "[!] schtasks fallo (exit $LASTEXITCODE)" -ForegroundColor Yellow
}
