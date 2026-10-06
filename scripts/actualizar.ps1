#Requires -Version 5.1
<#
actualizar.ps1 - Actualiza Hermes Agent a la ultima version y reinicia el gateway.
Uso:
  powershell -NoProfile -ExecutionPolicy Bypass -File actualizar.ps1
  actualizar.ps1 -Silent   (para tarea programada: log sin preguntas)
#>
param([switch]$Silent)
$ErrorActionPreference = "Continue"

$hermesExe = Join-Path $env:LOCALAPPDATA "hermes\bin\hermes.exe"
if (-not (Test-Path $hermesExe)) {
  $cmd = Get-Command hermes -ErrorAction SilentlyContinue
  if ($cmd) { $hermesExe = $cmd.Source } else { Write-Host "hermes no encontrado"; exit 1 }
}

$logLine = "[{0}] {1}" -f (Get-Date -Format "yyyy-MM-dd HH:mm:ss"), $env:COMPUTERNAME

function Invoke-Hermes([string]$sub) {
  $salida = & $hermesExe $sub 2>&1
  if ($LASTEXITCODE -ne 0) { throw ("exit $LASTEXITCODE: " + ($salida | Out-String)) }
}

try {
  Invoke-Hermes "update"
  $logLine += " | update OK"
} catch {
  $logLine += " | update FALLO: $($_.Exception.Message)"
}

# Re-provisionar dependencias por si la nueva version las necesita
try {
  Invoke-Hermes "pm install"
  $logLine += " | pm OK"
} catch {
  $logLine += " | pm FALLO: $($_.Exception.Message)"
}

# Reiniciar gateway para que tome la version nueva
try {
  Invoke-Hermes "gateway restart"
  $logLine += " | gateway OK"
} catch {
  $logLine += " | gateway FALLO: $($_.Exception.Message)"
}

if ($Silent) {
  $logDir = Join-Path $env:LOCALAPPDATA "hermes\logs"
  if (-not (Test-Path $logDir)) { New-Item -ItemType Directory -Path $logDir -Force | Out-Null }
  Add-Content -Path (Join-Path $logDir "hermes-autoupdate.log") -Value $logLine -Encoding UTF8
} else {
  Write-Host $logLine
  Write-Host ""
  Write-Host "Listo. Verifica con: hermes --version"
}
