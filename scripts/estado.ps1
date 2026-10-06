#Requires -Version 5.1
<#
estado.ps1 - Diagnostico rapido del equipo: version, doctor, gateway, rutas.
#>
$ErrorActionPreference = "Continue"

$hermesExe = Join-Path $env:LOCALAPPDATA "hermes\bin\hermes.exe"
if (-not (Test-Path $hermesExe)) {
  $cmd = Get-Command hermes -ErrorAction SilentlyContinue
  if ($cmd) { $hermesExe = $cmd.Source }
}

Write-Host "== Equipo: $env:COMPUTERNAME ==" -ForegroundColor Cyan
Write-Host ""

if (-not ($hermesExe -and (Test-Path $hermesExe))) {
  Write-Host "[!] Hermes NO esta instalado. Ejecuta instalar.bat" -ForegroundColor Yellow
  exit 1
}

Write-Host "-- Version --"
& $hermesExe --version
Write-Host ""

Write-Host "-- Doctor --"
& $hermesExe doctor
Write-Host ""

Write-Host "-- Gateway --"
& $hermesExe gateway status
Write-Host ""

Write-Host "-- Rutas (sin mostrar secretos) --"
try { Write-Host ("config : " + (& $hermesExe config path   | Out-String).Trim()) } catch {}
try { Write-Host (".env   : " + (& $hermesExe config env-path | Out-String).Trim()) } catch {}
