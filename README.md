# Hermes Agent — Instalador Windows 10/11 (GLM + Telegram + WhatsApp)

Despliegue rapido de **Hermes Agent** (Nous Research) en cualquier PC Windows 10/11,
con **API key GLM (Z.AI) distinta por equipo** y gateway de **Telegram / WhatsApp**.

Incluye **dos formas de instalar**:

| Forma | Para quien |
|---|---|
| **GUI Rust+Tauri** (`installer-app/`) | Usuarios finales: interfaz intuitiva ES/中文, ejecuta el instalador oficial, detecta y explica fallos en vivo, chat GLM integrado para pedir ayuda, autoverificacion final |
| **Scripts** (`instalar.bat` + `scripts/*.ps1`) | Deployment tecnico masivo / sin GUI |

✅ Usa el **instalador oficial** de Nous Research (sin admin, sin WSL)
✅ Provisiona solo: Python 3.14, Node.js, git, ripgrep, FFmpeg
✅ Credenciales por equipo desde un CSV local (NUNCA se sube a GitHub)
✅ Gateway como tarea programada al inicio de Windows (sin admin)
✅ Auto-update semanal opcional (hermes update + reinicio del gateway)
✅ 100% PowerShell nativo + binario Rust — no requiere compilar nada en el equipo destino

---

## GUI Rust + Tauri (`installer-app/`)

Instalador de escritorio (binario unico, ~7 MB) con:

- **Bilingue 中文/ES** con un click (detecta idioma del SO al arrancar).
- **Ejecuta el PS1 con deteccion de fallos**: cada linea del log se clasifica en vivo
  y los errores conocidos de Win10/11 muestran pista 💡 con solucion (BOM #27397,
  ExecutionPolicy, PATH, acceso denegado 0x80070005, proxy/TLS/DNS, charmap,
  disco, error 1113 de Z.AI…).
- **Chat GLM integrado**: para preguntarle al modelo (mismo provider que Hermes)
  sobre cualquier error del instalador. Implementa el endpoint correcto del Coding
  Plan (`https://api.z.ai/api/coding/paas/v4`), fallback a `reasoning_content`
  cuando `content` llega vacio, y diagnostico de 1113/1311.
- **Autoverificacion** al terminar: binario, `hermes --version`, `hermes doctor`,
  `hermes gateway status`.
- **Cancelacion** segura y bloqueo de instalaciones dobles.
- Credenciales via variables de entorno al proceso PS1 (no visibles en `process list`).

### Compilar (desarrollo, Linux)

```bash
cd installer-app/src-tauri
cargo build --release
```

### Compilar para Windows (release)

Push de un tag `v*` dispara `.github/workflows/build-windows.yml`:
compila en `windows-latest`, corre los 7 tests del clasificador de fallos,
hace smoke-test del binario y publica el `.exe` en GitHub Releases.

## Uso rapido (2 minutos)

1. Clona o copia esta carpeta al equipo (ej. en un pendrive o `git clone`).
2. Crea `config\equipos.csv` copiando `config\equipos.ejemplo.csv` y llena la fila
   con el `hostname` de ese equipo (`hostname` en cmd te lo dice).
3. Doble click a **`instalar.bat`** (o boton derecho → ejecutar). Al terminar,
   **abre un terminal NUEVO** y escribe `hermes`.

```cmd
instalar.bat
```

Modo manual (equipo que no esta en el CSV): el script pregunta las credenciales una a una.

## Que instala cada script

| Script | Funcion |
|---|---|
| `instalar.bat` | Entrada por doble-click; llama a `scripts\instalar.ps1` |
| `scripts\instalar.ps1` | Instala/actualiza Hermes (oficial), escribe `.env` + `config.yaml`, instala gateway, corre `hermes doctor` |
| `scripts\actualizar.ps1` | `hermes update` + `hermes pm install` + reinicio del gateway |
| `scripts\registrar-actualizacion.ps1` | Crea tarea programada semanal de auto-update (lunes 09:00) |
| `scripts\estado.ps1` | Diagnostico: version, doctor, estado del gateway, rutas |
| `config\equipos.csv` | ⚠️ LOCAL: credenciales reales por equipo (ignorado por git) |
| `config\equipos.ejemplo.csv` | Plantilla de columnas para el CSV |

## Formato del CSV por equipo

```csv
hostname,modelo,glm_api_key,glm_base_url,telegram_bot_token,telegram_allowed_users,telegram_home_channel
DESKTOP-ABC123,glm-5-turbo,zai-xxxx,,123456:AAxxxx,usuario_brad,
```

- `hostname`: salida del comando `hostname` en ese Windows (mayusculas/minusculas no importan, se compara exacto — respeta el nombre tal cual).
- `glm_base_url`: dejalo vacio para usar la URL default de Z.AI.
- `telegram_home_channel`: opcional (chat id fijo); vacio = se define con `/sethome` desde Telegram.
- Cada equipo puede tener su **propio bot de Telegram** (token distinto) — recomendado para no cruzar conversaciones.

## WhatsApp

WhatsApp se empareja escaneando un QR la primera vez (no se puede automatizar por
seguridad de WhatsApp). Despues de instalar:

```
hermes gateway setup
```

Elegi WhatsApp, escanea el QR con el telefono → queda emparejado para siempre
(la sesion se guarda local). El reinicio del gateway NO pide QR de nuevo.

## Actualizacion

- **Manual:** `powershell -File scripts\actualizar.ps1` (o re-ejecutar `instalar.bat` — detecta que ya esta instalado y solo actualiza).
- **Automatico:** ejecuta una vez `scripts\registrar-actualizacion.ps1` → queda tarea semanal que actualiza y reinicia el gateway sola, con log en `%LOCALAPPDATA%\hermes\logs\hermes-autoupdate.log`.

## Seguridad

- ⚠️ `config\equipos.csv` contiene API keys y tokens → esta en `.gitignore`, jamas lo commitees.
- `TELEGRAM_ALLOWED_USERS` limita quien puede hablarle al bot. Dejalo siempre lleno.
- Las keys se guardan en `%LOCALAPPDATA%\hermes\.hermes\.env` (perfil del usuario, no admin).

## Desinstalar

```
hermes gateway stop
hermes uninstall
```

## Notas tecnicas

- Instalacion nativa en `%LOCALAPPDATA%\hermes\` — no requiere permisos de administrador.
- El gateway al inicio de Windows usa tarea programada (schtasks) — sin admin.
- `.env` y `config.yaml` se escriben en UTF-8 **sin BOM** (el BOM rompe el parser de Hermes).
- Si `hermes` no se reconoce en un terminal viejo: abri un terminal nuevo (el PATH se agrega al perfil de usuario).
- Fuente oficial: https://hermes-agent.nousresearch.com/docs/user-guide/windows-native
