#Requires -Version 5.1
<#
instalar.ps1 - Instalador Hermes Agent para Windows 10/11 (fleet)

Que hace:
  1. Carga credenciales del equipo desde config\equipos.csv (o las pide).
  2. Instala/actualiza Hermes Agent con el instalador OFICIAL de Nous Research.
     (provisiona Python 3.14, Node, git, ripgrep, FFmpeg el solo, sin admin)
  3. Escribe .env (GLM_API_KEY, TELEGRAM_*) y config.yaml (provider zai + modelo).
  4. Instala e inicia el gateway como tarea programada (sin admin).
  5. Corre hermes doctor y muestra el resumen.

Uso:
  powershell -NoProfile -ExecutionPolicy Bypass -File instalar.ps1
  instalar.ps1 -EquiposCsv "C:\ruta\equipos.csv" -Modelo glm-5-turbo -SkipGateway
#>
param(
  [string]$EquiposCsv = "",
  [string]$Modelo = "",
  [switch]$SkipGateway,
  [switch]$SkipAutoUpdate
)

# La GUI (Rust/Tauri) pasa credenciales por variables de entorno para no
# exponerlas en la lista de procesos. Tienen prioridad sobre CSV y prompts.
$EnvGlmKey   = $env:HERMES_INSTALL_GLM_KEY
$EnvTgToken  = $env:HERMES_INSTALL_TG_TOKEN
$EnvCsv      = $env:HERMES_INSTALL_CSV
if (-not $Modelo -and $env:HERMES_INSTALL_MODELO) { $Modelo = $env:HERMES_INSTALL_MODELO }
if (-not $EquiposCsv -and $EnvCsv) { $EquiposCsv = $EnvCsv }
if (-not $Modelo) { $Modelo = "glm-5-turbo" }

$ErrorActionPreference = "Stop"
try { [Net.ServicePointManager]::SecurityProtocol = [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12 } catch {}

function Write-Paso([string]$m) { Write-Host ""; Write-Host "==> $m" -ForegroundColor Cyan }
function Write-Ok([string]$m)   { Write-Host "    [OK] $m" -ForegroundColor Green }
function Write-Av([string]$m)   { Write-Host "    [!]  $m" -ForegroundColor Yellow }

$pc = $env:COMPUTERNAME
# Resolucion robusta de rutas: $PSScriptRoot con fallback, y SIN prefijo verbatim \\?\
# (PowerShell 5.1 falla con Join-Path/Split-Path sobre rutas \\?\ — bug real en ATENCC-LUCAS)
$scriptRoot = $PSScriptRoot
if (-not $scriptRoot) { $scriptRoot = Split-Path -Parent $MyInvocation.MyCommand.Path }
if ($scriptRoot -and $scriptRoot.StartsWith('\\?\')) { $scriptRoot = $scriptRoot.Substring(4) }
$repoRoot = $null
if ($scriptRoot) { $repoRoot = Split-Path -Parent $scriptRoot }
if ($repoRoot -and $repoRoot.StartsWith('\\?\')) { $repoRoot = $repoRoot.Substring(4) }

# ---------------------------------------------------------------
# 1. Credenciales por equipo
# ---------------------------------------------------------------
Write-Paso "Cargando credenciales para equipo '$pc'"

$csvPath = $EquiposCsv
if (-not $csvPath -and $repoRoot) { $csvPath = Join-Path $repoRoot "config\equipos.csv" }

$row = $null
if ($EnvGlmKey) {
  # Modo GUI: credenciales directas, sin CSV ni prompts
  Write-Ok "Credenciales recibidas de la GUI (variables de entorno)"
  $GlmKey     = $EnvGlmKey
  $GlmBaseUrl = $env:HERMES_INSTALL_GLM_BASE_URL
  $TgToken    = $EnvTgToken
  $TgUsers    = $env:HERMES_INSTALL_TG_USERS
  $TgHome     = $env:HERMES_INSTALL_TG_HOME
} elseif ($csvPath -and (Test-Path $csvPath)) {
  $rows = ConvertFrom-Csv (Get-Content $csvPath -Encoding UTF8 | Out-String)
  foreach ($r in $rows) { if ($r.hostname -eq $pc) { $row = $r; break } }
}

if ($row) {
  Write-Ok "Equipo encontrado en $csvPath"
  if ($row.modelo)               { $Modelo = $row.modelo }
  $GlmKey     = $row.glm_api_key
  $GlmBaseUrl = $row.glm_base_url
  $TgToken    = $row.telegram_bot_token
  $TgUsers    = $row.telegram_allowed_users
  $TgHome     = $row.telegram_home_channel
} elseif (-not $EnvGlmKey) {
  $interactive = [Environment]::UserInteractive -and -not $env:HERMES_INSTALL_NONINTERACTIVE
  if (-not $interactive) {
    Write-Host ""
    Write-Host "ERROR: sin GLM_API_KEY (no hay CSV, ni variables de entorno, y el modo es no-interactivo). Abortando." -ForegroundColor Red
    exit 1
  }
  Write-Av "Equipo '$pc' no esta en $csvPath - modo manual"
  $Modelo     = Read-Host "Modelo GLM (Enter = $Modelo)"
  if (-not $Modelo) { $Modelo = "glm-5-turbo" }
  $GlmKey     = Read-Host "GLM_API_KEY (obligatorio)"
  $GlmBaseUrl = Read-Host "GLM_BASE_URL (Enter = default Z.AI)"
  $TgToken    = Read-Host "TELEGRAM_BOT_TOKEN (Enter = omitir Telegram)"
  $TgUsers    = Read-Host "TELEGRAM_ALLOWED_USERS (ej: brad_usuario)"
  $TgHome     = Read-Host "TELEGRAM_HOME_CHANNEL (opcional, chat id)"
}

if (-not $GlmKey) {
  Write-Host ""
  Write-Host "ERROR: GLM_API_KEY es obligatoria. Abortando." -ForegroundColor Red
  exit 1
}

# ---------------------------------------------------------------
# 2. Instalar / actualizar Hermes Agent (instalador oficial)
# ---------------------------------------------------------------
$hermesExe = Join-Path $env:LOCALAPPDATA "hermes\bin\hermes.exe"
$yaInstalado = (Get-Command hermes -ErrorAction SilentlyContinue) -or (Test-Path $hermesExe)

if (-not $yaInstalado) {
  Write-Paso "Instalando Hermes Agent (instalador oficial Nous Research, sin admin)"
  $code = (Invoke-WebRequest -UseBasicParsing -Uri "https://hermes-agent.nousresearch.com/install.ps1").Content
  & ([scriptblock]::Create($code)) -NonInteractive
  if ($null -ne $LASTEXITCODE -and $LASTEXITCODE -ne 0) { throw "Instalador oficial fallo (exit $LASTEXITCODE)" }
  Write-Ok "Hermes Agent instalado en %LOCALAPPDATA%\hermes"
} else {
  Write-Paso "Hermes ya esta instalado - actualizando a la ultima version"
  try {
    & $hermesExe update
    Write-Ok "hermes update completado"
  } catch {
    Write-Av "hermes update fallo ($($_.Exception.Message)) - re-ejecutando instalador oficial"
    $code = (Invoke-WebRequest -UseBasicParsing -Uri "https://hermes-agent.nousresearch.com/install.ps1").Content
    & ([scriptblock]::Create($code)) -NonInteractive
  }
}
# Resolver binario (el PATH de esta sesion puede no verlo todavia)
if (-not (Test-Path $hermesExe)) {
  $cmd = Get-Command hermes -ErrorAction SilentlyContinue
  if ($cmd) { $hermesExe = $cmd.Source } else { throw "No se encontro el binario hermes despues de instalar" }
}
Write-Ok "Binario: $hermesExe"

# ---------------------------------------------------------------
# 3. Escribir .env con credenciales de este equipo (UTF-8 sin BOM)
# ---------------------------------------------------------------
Write-Paso "Configurando credenciales (.env)"

$envPath = ""
try { $envPath = (& $hermesExe config env-path | Out-String).Trim() } catch {}
if (-not $envPath -or -not (Split-Path $envPath -Parent -ErrorAction SilentlyContinue)) {
  $envPath = Join-Path $env:LOCALAPPDATA "hermes\.hermes\.env"
}
$envDir = Split-Path $envPath -Parent
if (-not (Test-Path $envDir)) { New-Item -ItemType Directory -Path $envDir -Force | Out-Null }

function Set-EnvKeys([string]$path, [hashtable]$kv) {
  $enc = New-Object System.Text.UTF8Encoding($false)   # sin BOM (BOM rompe config hermes)
  $lines = @()
  if (Test-Path $path) { $lines = [System.IO.File]::ReadAllLines($path) }
  foreach ($k in $kv.Keys) {
    $v = [string]$kv[$k]
    if (-not $v) { continue }
    $found = $false
    for ($i = 0; $i -lt $lines.Count; $i++) {
      if ($lines[$i] -match ("^" + [regex]::Escape($k) + "=")) { $lines[$i] = "$k=$v"; $found = $true }
    }
    if (-not $found) { $lines += "$k=$v" }
  }
  [System.IO.File]::WriteAllText($path, (($lines -join "`r`n") + "`r`n"), $enc)
}

$kv = @{
  GLM_API_KEY = $GlmKey
}
if ($GlmBaseUrl) { $kv["GLM_BASE_URL"] = $GlmBaseUrl }
if ($TgToken) {
  $kv["TELEGRAM_BOT_TOKEN"]     = $TgToken
  if ($TgUsers) { $kv["TELEGRAM_ALLOWED_USERS"] = $TgUsers }
  if ($TgHome)  { $kv["TELEGRAM_HOME_CHANNEL"]  = $TgHome }
}
Set-EnvKeys $envPath $kv
Write-Ok ".env escrito: $envPath"

# ---------------------------------------------------------------
# 4. config.yaml -> provider zai + modelo
# ---------------------------------------------------------------
Write-Paso "Configurando modelo: $Modelo (provider zai)"

$cfgPath = ""
try { $cfgPath = (& $hermesExe config path | Out-String).Trim() } catch {}
if (-not $cfgPath) { $cfgPath = Join-Path $env:LOCALAPPDATA "hermes\.hermes\config.yaml" }

if (Test-Path $cfgPath) {
  & $hermesExe config set model.provider zai | Out-Null
  & $hermesExe config set model.default $Modelo | Out-Null
  Write-Ok "config.yaml actualizado: $cfgPath"
} else {
  $enc = New-Object System.Text.UTF8Encoding($false)
  $cfg = "model:`r`n  default: $Modelo`r`n  provider: zai`r`n"
  $cfgDir = Split-Path $cfgPath -Parent
  if (-not (Test-Path $cfgDir)) { New-Item -ItemType Directory -Path $cfgDir -Force | Out-Null }
  [System.IO.File]::WriteAllText($cfgPath, $cfg, $enc)
  Write-Ok "config.yaml creado: $cfgPath"
}

# ---------------------------------------------------------------
# 5. Gateway (Telegram / WhatsApp) como tarea programada
# ---------------------------------------------------------------
if (-not $SkipGateway) {
  Write-Paso "Instalando gateway al inicio de Windows (sin admin)"
  try {
    & $hermesExe gateway install
    & $hermesExe gateway start
    Write-Ok "Gateway instalado e iniciado"
  } catch {
    Write-Av "Gateway no se pudo iniciar automaticamente: $($_.Exception.Message)"
    Write-Av "Puedes iniciarlo despues con: hermes gateway start"
  }
} else {
  Write-Av "-SkipGateway: gateway NO configurado en esta corrida"
}

# ---------------------------------------------------------------
# 5.5 Auto-update semanal (TODOS los equipos, automatico)
#     Copia actualizar.ps1 a %LOCALAPPDATA%\hermes\bin\ (independiente
#     del repo) y crea la tarea programada. Se salta con -SkipAutoUpdate.
# ---------------------------------------------------------------
if (-not $SkipAutoUpdate) {
  Write-Paso "Registrando auto-update semanal (lunes 09:00)"
  try {
    $binDir  = Join-Path $env:LOCALAPPDATA "hermes\bin"
    if (-not (Test-Path $binDir)) { New-Item -ItemType Directory -Path $binDir -Force | Out-Null }
    $dest = Join-Path $binDir "hermes-autoupdate.ps1"
    $src  = Join-Path $scriptRoot "actualizar.ps1"
    if (Test-Path $src) {
      Copy-Item $src $dest -Force
      # Comillas escapadas \" — requisito de schtasks /TR con rutas con espacios
      $tr = "powershell.exe -NoProfile -ExecutionPolicy Bypass -File `\"$dest`\" -Silent"
      schtasks /Create /F /SC WEEKLY /D MON /ST 09:00 /TN "Hermes AutoUpdate" /TR $tr | Out-Null
      if ($LASTEXITCODE -eq 0) {
        Write-Ok "Tarea 'Hermes AutoUpdate' creada (lunes 09:00)"
        Write-Ok "Log: %LOCALAPPDATA%\hermes\logs\hermes-autoupdate.log"
      } else {
        Write-Av "No se pudo crear la tarea (exit $LASTEXITCODE) - el equipo se actualiza solo al re-ejecutar el instalador"
      }
    } else {
      Write-Av "actualizar.ps1 no encontrado junto al instalador - auto-update omitido"
    }
  } catch {
    Write-Av "auto-update no registrado: $($_.Exception.Message)"
  }
}

# ---------------------------------------------------------------
# 6. Verificacion
# ---------------------------------------------------------------
Write-Paso "Verificacion (hermes doctor)"
try { & $hermesExe doctor } catch { Write-Av "doctor reporto problemas - revisa arriba" }

Write-Host ""
Write-Host "================================================" -ForegroundColor Cyan
Write-Host "  INSTALACION COMPLETA - $pc" -ForegroundColor Cyan
Write-Host "================================================" -ForegroundColor Cyan
Write-Host "  Modelo   : $Modelo (zai)"
Write-Host "  Telegram : $(if ($TgToken) { 'configurado' } else { 'sin token (omitido)' })"
Write-Host "  WhatsApp : requiere emparejar una vez -> hermes gateway setup"
Write-Host "  Prueba   : abre un NUEVO terminal y ejecuta: hermes"
Write-Host "  Estado   : hermes gateway status | hermes doctor"
Write-Host "================================================" -ForegroundColor Cyan
