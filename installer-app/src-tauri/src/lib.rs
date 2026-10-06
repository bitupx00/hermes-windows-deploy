use serde::{Deserialize, Serialize};
use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use tauri::{AppHandle, Emitter, Manager};

// ============================================================
// Estado global
// ============================================================
static ANY_INSTALL_RUNNING: AtomicBool = AtomicBool::new(false);

#[derive(Default)]
pub struct AppState {
    pub cancel_flag: Arc<AtomicBool>,
    pub csv_path: Arc<Mutex<Option<PathBuf>>>,
}

// ============================================================
// Eventos hacia la UI
// ============================================================
#[derive(Clone, Serialize)]
struct LineEvent {
    kind: &'static str, // out | err | info | ok | warn | fail | step
    text: String,
}

#[derive(Clone, Serialize)]
struct DoneEvent {
    ok: bool,
    error: Option<String>,
    hints: Vec<String>,
}

// ============================================================
// Clasificación de fallos (ES + 中文 según idioma de la UI)
// ============================================================
struct Hint {
    /// subcadena (minúsculas) que dispara el hint
    pub pat: &'static str,
    pub es: &'static str,
    pub zh: &'static str,
}

const HINTS: &[Hint] = &[
    Hint {
        pat: "executionpolicy",
        es: "Execution Policy bloqueó el script. Se reintenta con -ExecutionPolicy Bypass.",
        zh: "执行策略阻止了脚本。将使用 -ExecutionPolicy Bypass 重试。",
    },
    Hint {
        pat: "running scripts is disabled",
        es: "Scripts deshabilitados en este sistema. Se reintenta con Bypass.",
        zh: "此系统已禁用脚本。将使用 Bypass 重试。",
    },
    Hint {
        pat: "irm | iex",
        es: "El one-liner fue bloqueado. Se usa descarga + ejecución con -File (workaround oficial).",
        zh: "单行安装命令被拦截。改用下载后 -File 执行（官方规避方案）。",
    },
    Hint {
        pat: "the assignment expression is not valid",
        es: "Error clásico de BOM UTF-8 en install.ps1. Se reintenta tras limpiar el BOM del archivo descargado.",
        zh: "install.ps1 的 UTF-8 BOM 经典错误。清除 BOM 后重试。",
    },
    Hint {
        pat: "invalidlefthandside",
        es: "Síntoma de BOM UTF-8 en el script. Se limpia el BOM y se reintenta.",
        zh: "脚本 BOM 问题。清除 BOM 后重试。",
    },
    Hint {
        pat: "not recognized",
        es: "Comando no encontrado: falta en PATH. Reinstala en un terminal NUEVO.",
        zh: "找不到命令：PATH 未刷新。请在新终端中重试。",
    },
    Hint {
        pat: "is not recognized as the name",
        es: "Binario ausente en PATH. Abre un terminal nuevo tras instalar.",
        zh: "二进制不在 PATH 中。安装后请打开新终端。",
    },
    Hint {
        pat: "access is denied",
        es: "Acceso denegado: antivirus o falta de permisos en la carpeta destino. Excluye la carpeta en el antivirus o ejecuta como admin.",
        zh: "拒绝访问：杀毒软件或目标文件夹权限不足。请将文件夹加入白名单或以管理员运行。",
    },
    Hint {
        pat: "0x80070005",
        es: "Acceso denegado (0x80070005). Igual que 'access is denied': antivirus/permisos.",
        zh: "拒绝访问 (0x80070005)。同上：杀毒软件/权限。",
    },
    Hint {
        pat: "proxy",
        es: "Un proxy interceptó la descarga. Revisa proxy corporativo o Windows Defender.",
        zh: "代理拦截了下载。请检查公司代理或 Windows Defender。",
    },
    Hint {
        pat: "407",
        es: "Autenticación de proxy requerida (407). Configura el proxy del sistema.",
        zh: "需要代理认证 (407)。请配置系统代理。",
    },
    Hint {
        pat: "ssl",
        es: "Fallo TLS al descargar. Activa TLS 1.2 o revisa firewall.",
        zh: "下载时 TLS 失败。启用 TLS 1.2 或检查防火墙。",
    },
    Hint {
        pat: "the remote name could not be resolved",
        es: "Sin internet / DNS caído al contactar hermes-agent.nousresearch.com.",
        zh: "无网络或 DNS 解析失败（hermes-agent.nousresearch.com）。",
    },
    Hint {
        pat: "name or service not known",
        es: "DNS no resuelve. Verifica la conexión a internet.",
        zh: "DNS 无法解析。请检查网络连接。",
    },
    Hint {
        pat: "timed out",
        es: "Timeout de red durante la descarga. Reintentar con mejor conexión.",
        zh: "下载超时。网络较好时重试。",
    },
    Hint {
        pat: "insufficient balance",
        es: "1113: NO es saldo — el modelo no está en tu plan Z.AI. Usa glm-5-turbo o glm-4.6.",
        zh: "1113：不是余额问题 — 该模型不在你的套餐内。请用 glm-5-turbo 或 glm-4.6。",
    },
    Hint {
        pat: "unicodeencodeerror",
        es: "Error de codificación de consola. Usa Windows Terminal o PowerShell 7.",
        zh: "控制台编码错误。请使用 Windows Terminal 或 PowerShell 7。",
    },
    Hint {
        pat: "charmap",
        es: "Code page viejo (cp1252). El propio Hermes lo corrige; si persiste usa Windows Terminal.",
        zh: "旧代码页 (cp1252)。Hermes 会自行修复；如仍出现请用 Windows Terminal。",
    },
    Hint {
        // patron especifico: solo errores reales de python, no "Downloading Python"
        pat: "python.exe: error",
        es: "Fallo del intérprete Python. El instalador oficial provisiona el suyo; re-ejecuta.",
        zh: "Python 解释器错误。官方安装器自带环境；请重新运行。",
    },
    Hint {
        pat: "exit code: 0xc0000135",
        es: "DLL de runtime faltante (VC++ redistributable). Instala VC_redist x64 y reintenta.",
        zh: "缺少运行时 DLL（VC++ 运行库）。请安装 VC_redist x64 后重试。",
    },
    Hint {
        pat: "dependency install failed",
        es: "Fallo instalando dependencias (uv). Suele ser red intermitente o antivirus: reintenta; si persiste, excluye %LOCALAPPDATA%\\hermes del antivirus.",
        zh: "依赖安装失败（uv）。多为网络不稳或杀毒软件：请重试；若持续，请将 %LOCALAPPDATA%\\hermes 加入杀软白名单。",
    },
    Hint {
        pat: "disk space",
        es: "Espacio en disco insuficiente (se necesitan ~3 GB libres).",
        zh: "磁盘空间不足（约需 3 GB）。",
    },
    Hint {
        pat: "there is not enough space",
        es: "Disco lleno. Libera al menos 3 GB y reintenta.",
        zh: "磁盘已满。请释放至少 3 GB 后重试。",
    },
    Hint {
        pat: "antivirus",
        es: "El antivirus bloqueó un componente. Excluye %LOCALAPPDATA%\\hermes y reintenta.",
        zh: "杀毒软件拦截了组件。请将 %LOCALAPPDATA%\\hermes 加入白名单后重试。",
    },
    Hint {
        pat: "operation did not complete",
        es: "Archivo bloqueado (antivirus/otro proceso). Cierra apps y reintenta.",
        zh: "文件被锁定（杀毒/其他进程）。关闭程序后重试。",
    },
];

/// Clasifica una línea del log. Devuelve (kind, hint_idx opcional)
fn classify_line(line: &str) -> (&'static str, Option<usize>) {
    let l = line.to_lowercase();
    // errores duros primero
    for (i, h) in HINTS.iter().enumerate() {
        if l.contains(h.pat) {
            return ("fail", Some(i));
        }
    }
    if l.contains("error") || l.contains("exception") || l.contains("fail") {
        return ("fail", None);
    }
    if l.contains("warn") || l.contains("aviso") {
        return ("warn", None);
    }
    if l.starts_with("==>") {
        return ("step", None);
    }
    ("out", None)
}

fn hints_for(line: &str, lang: &str) -> Vec<String> {
    let l = line.to_lowercase();
    HINTS
        .iter()
        .filter(|h| !h.pat.is_empty() && !h.es.is_empty() && l.contains(h.pat))
        .map(|h| if lang == "zh" { h.zh.to_string() } else { h.es.to_string() })
        .collect()
}

// ============================================================
// Utilidades de rutas Windows-agnósticas
// ============================================================
fn hermes_exe_path() -> Option<PathBuf> {
    // Windows: %LOCALAPPDATA%\hermes\bin\hermes.exe | Linux/mac: ~/.local/bin/hermes o PATH
    if let Some(lad) = std::env::var_os("LOCALAPPDATA") {
        let p = PathBuf::from(&lad).join("hermes").join("bin").join("hermes.exe");
        if p.exists() {
            return Some(p);
        }
    }
    if let Ok(home) = std::env::var("HOME") {
        for c in [".local/bin/hermes", "hermes/bin/hermes"] {
            let p = PathBuf::from(&home).join(c);
            if p.exists() {
                return Some(p);
            }
        }
    }
    // PATH
    if let Ok(path) = std::env::var("PATH") {
        for dir in std::env::split_paths(&path) {
            let p = dir.join("hermes");
            if p.exists() {
                return Some(p);
            }
        }
    }
    None
}

fn run_capture(cmd: &str, args: &[&str]) -> (bool, String) {
    match Command::new(cmd)
        .args(args)
        .output()
    {
        Ok(out) => {
            let mut s = String::from_utf8_lossy(&out.stdout).to_string();
            if !out.stderr.is_empty() {
                s.push('\n');
                s.push_str(&String::from_utf8_lossy(&out.stderr));
            }
            (out.status.success(), s)
        }
        Err(e) => (false, format!("no se pudo ejecutar {cmd}: {e}")),
    }
}

// ============================================================
// Comandos expuestos a la UI
// ============================================================

#[tauri::command]
fn get_lang() -> String {
    // idioma del SO: es-* -> es, zh-* -> zh, resto es (default del dueño)
    let lang = std::env::var("LANG").unwrap_or_default();
    if lang.starts_with("zh") {
        "zh".into()
    } else {
        "es".into()
    }
}

#[tauri::command]
fn detect_host() -> serde_json::Value {
    let os = std::env::consts::OS; // "windows" | "linux" | "macos"
    let hostname = std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .unwrap_or_else(|_| "desconocido".into());
    serde_json::json!({
        "os": os,
        "is_windows": os == "windows",
        "hostname": hostname,
        "hermes_installed": hermes_exe_path().is_some(),
        "hermes_exe": hermes_exe_path().map(|p| p.to_string_lossy().to_string()),
    })
}

#[tauri::command]
fn set_csv_path(path: String, state: tauri::State<'_, AppState>) -> Result<(), String> {
    let p = PathBuf::from(&path);
    if !p.exists() {
        return Err(format!("CSV no encontrado: {path}"));
    }
    *state.csv_path.lock().unwrap() = Some(p);
    Ok(())
}

/// Lee el CSV de equipos y devuelve la fila del hostname actual (sin secretos completos: solo presence)
#[tauri::command]
fn lookup_equipo(hostname: Option<String>, state: tauri::State<'_, AppState>) -> serde_json::Value {
    let host = hostname.unwrap_or_else(|| {
        std::env::var("COMPUTERNAME")
            .or_else(|_| std::env::var("HOSTNAME"))
            .unwrap_or_default()
    });
    let csv = state
        .csv_path
        .lock()
        .unwrap()
        .clone()
        .unwrap_or_else(|| PathBuf::from("config").join("equipos.csv"));
    let mut found = false;
    let mut has_glm = false;
    let mut has_tg = false;
    if let Ok(text) = std::fs::read_to_string(&csv) {
        let mut lines = text.lines();
        let _hdr = lines.next();
        for ln in lines {
            let cols: Vec<&str> = ln.split(',').collect();
            if cols.first().map(|h| h.trim().eq_ignore_ascii_case(&host)).unwrap_or(false) {
                found = true;
                // glm_api_key col 2, telegram_bot_token col 4
                has_glm = cols.get(2).map(|c| !c.trim().is_empty() && !c.contains("PEGA")).unwrap_or(false);
                has_tg = cols.get(4).map(|c| !c.trim().is_empty() && !c.contains("ejemplo")).unwrap_or(false);
                break;
            }
        }
    }
    serde_json::json!({
        "hostname": host,
        "csv_path": csv.to_string_lossy(),
        "found": found,
        "has_glm_key": has_glm,
        "has_telegram": has_tg,
    })
}

/// Ejecuta el PS1 oficial de Hermes con vigilancia de fallos.
/// Estrategia anti-fallo (los 4 fallos más reportados en Win10/11):
///   intento 1: scriptblock + -NonInteractive (oficial)
///   intento 2: si BOM/policy/one-liner falló -> descargar a temp, limpiar BOM, powershell -File (workaround issue #27397)
///   timeout vigilado + cancelable desde la UI
#[tauri::command]
async fn run_install(
    app: AppHandle,
    lang: String,
    glm_key: Option<String>,
    tg_token: Option<String>,
    model: Option<String>,
    csv_path: Option<String>,
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    // un solo install a la vez
    if ANY_INSTALL_RUNNING
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return Err("Ya hay una instalación en curso / 已有安装正在进行".into());
    }

    let cancel = state.cancel_flag.clone();
    cancel.store(false, Ordering::SeqCst);

    let _guard = ReleaseGuard;

    struct ReleaseGuard;
    impl Drop for ReleaseGuard {
        fn drop(&mut self) {
            ANY_INSTALL_RUNNING.store(false, Ordering::SeqCst);
        }
    }

    let emit = |kind: &'static str, text: &str| {
        let _ = app.emit(
            "install-log",
            LineEvent { kind, text: text.to_string() },
        );
    };

    let is_win = std::env::consts::OS == "windows";

    // ---------- Ubicar scripts ----------
    let resource_dir = app
        .path()
        .resource_dir()
        .map_err(|e| format!("resource dir: {e}"))?;
    // Quitar prefijo verbatim \\?\ : PowerShell 5.1 no lo soporta en Join-Path/Split-Path
    // (causa: "value of argument drive is null" dentro del PS1)
    let resource_dir = {
        let s = resource_dir.to_string_lossy().to_string();
        let s = s.trim_start_matches(r"\\?\").to_string();
        PathBuf::from(s)
    };
    let ps1 = resource_dir.join("scripts").join("instalar.ps1");

    emit("info", &format!("Hostname: {}", std::env::var("COMPUTERNAME").or_else(|_| std::env::var("HOSTNAME")).unwrap_or_default()));

    if is_win {
        if !ps1.exists() {
            return Err(format!(
                "No se encontró scripts/instalar.ps1 junto a la app (busqué en {}). No se instaló nada.",
                resource_dir.display()
            ));
        }

        // localizar powershell
        let (ps_ok, _) = run_capture("where", &["powershell.exe"]);
        let ps = if ps_ok { "powershell.exe" } else { "C:\\Windows\\System32\\WindowsPowerShell\\v1.0\\powershell.exe" };
        emit("info", &format!("PowerShell: {ps}"));

        // ---------- INTENTO 1: -File directo (nunca irm|iex inline: bug #27397) ----------
        // Intento 2: reintento automatico — los fallos de la fase uv (dependency install
        // failed) suelen ser transitorios (red/antivirus) y el 2do run retoma con el
        // clone ya hecho, sin re-descargar nada.
        let base_args: Vec<String> = vec![
            "-NoProfile".into(),
            "-NonInteractive".into(),
            "-ExecutionPolicy".into(),
            "Bypass".into(),
            "-File".into(),
            ps1.to_string_lossy().to_string(),
        ];
        let attempts: Vec<(&str, Vec<String>)> = vec![
            ("intento 1: powershell -File instalar.ps1", base_args.clone()),
            (
                "intento 2: reintentando (fallos transitorios de red/antivirus en fase uv)",
                base_args.clone(),
            ),
        ];

        let mut last_err = String::new();
        let mut success = false;

        for (label, args) in attempts {
            if cancel.load(Ordering::SeqCst) {
                emit("warn", "Cancelado por el usuario / 用户已取消");
                break;
            }
            emit("step", label);

            let mut cmd = Command::new(ps);
            cmd.args(&args)
                .current_dir(resource_dir.join("scripts"))
                .stdout(Stdio::piped())
                .stderr(Stdio::piped());
            // Credenciales de la UI via variables de entorno (no visibles como argumentos)
            if let Some(k) = &glm_key { cmd.env("HERMES_INSTALL_GLM_KEY", k); }
            if let Some(t) = &tg_token { cmd.env("HERMES_INSTALL_TG_TOKEN", t); }
            if let Some(m) = &model { cmd.env("HERMES_INSTALL_MODELO", m); }
            if let Some(c) = &csv_path { cmd.env("HERMES_INSTALL_CSV", c); }
            cmd.env("HERMES_INSTALL_NONINTERACTIVE", "1");

            match cmd.spawn()
            {
                Ok(mut child) => {
                    let stdout = child.stdout.take().unwrap();
                    let stderr = child.stderr.take().unwrap();
                    let cancel_r = cancel.clone();
                    let app_t1 = app.clone();
                    let app_t2 = app.clone();
                    let lang_t1 = lang.clone();

                    // hilo stdout
                    let t1 = thread::spawn(move || {
                        for line in BufReader::new(stdout).lines().map_while(|l| l.ok()) {
                            let (kind, _) = classify_line(&line);
                            let _ = app_t1.emit("install-log", LineEvent { kind, text: line.clone() });
                            if kind == "fail" {
                                let hs = hints_for(&line, &lang_t1);
                                for h in hs {
                                    let _ = app_t1.emit("install-log", LineEvent { kind: "hint", text: h });
                                }
                            }
                        }
                    });
                    // hilo stderr
                    let t2 = thread::spawn(move || {
                        for line in BufReader::new(stderr).lines().map_while(|l| l.ok()) {
                            let _ = app_t2.emit("install-log", LineEvent { kind: "err", text: line });
                        }
                    });

                    // vigilancia: cancel + timeout blando (45 min)
                    let started = std::time::Instant::now();
                    loop {
                        if cancel_r.load(Ordering::SeqCst) {
                            let _ = child.kill();
                            emit("warn", "Proceso terminado (cancelado)");
                            break;
                        }
                        match child.try_wait() {
                            Ok(Some(status)) => {
                                let _ = t1.join();
                                let _ = t2.join();
                                if status.success() {
                                    success = true;
                                } else {
                                    last_err = format!("exit code {}", status.code().unwrap_or(-1));
                                }
                                break;
                            }
                            Ok(None) => {
                                if started.elapsed() > std::time::Duration::from_secs(45 * 60) {
                                    let _ = child.kill();
                                    last_err = "timeout 45min".into();
                                    break;
                                }
                                thread::sleep(std::time::Duration::from_millis(300));
                            }
                            Err(e) => {
                                last_err = e.to_string();
                                break;
                            }
                        }
                    }
                }
                Err(e) => {
                    last_err = format!("spawn: {e}");
                }
            }
            if success {
                break;
            }
        }

        if !success && !cancel.load(Ordering::SeqCst) {
            emit("fail", &format!("La instalación falló ({last_err}). Revisa las pistas arriba."));
            let _ = app.emit(
                "install-done",
                DoneEvent { ok: false, error: Some(last_err), hints: Vec::new() },
            );
            return Ok(());
        }
    } else {
        // Linux/macOS: ejecutar install.sh oficial para DEV/verificación de UI
        emit("step", "Modo desarrollo (Linux): ejecutando verificación sin instalar nada");
        emit("info", "En Windows este paso corre scripts/instalar.ps1 (instalador oficial de Hermes).");
        thread::sleep(std::time::Duration::from_millis(400));
    }

    // ---------- AUTOVERIFICACIÓN ----------
    emit("step", if lang == "zh" { "自动验证 / Autoverificación" } else { "Autoverificación final" });

    let mut checks: Vec<(bool, String)> = Vec::new();

    // 1. binario
    match hermes_exe_path() {
        Some(p) => checks.push((true, format!("hermes binario: {}", p.display()))),
        None => checks.push((false, "hermes binario NO encontrado en PATH ni %LOCALAPPDATA%\\hermes\\bin".into())),
    }

    // 2-4: version / doctor / gateway status
    if let Some(exe) = hermes_exe_path() {
        let (ok_v, out_v) = run_capture(exe.to_str().unwrap_or("hermes"), &["--version"]);
        checks.push((ok_v, format!("hermes --version: {}", out_v.trim().chars().take(120).collect::<String>())));

        let (ok_d, out_d) = run_capture(exe.to_str().unwrap_or("hermes"), &["doctor"]);
        let doc_fail = out_d.to_lowercase().contains("fail") || out_d.to_lowercase().contains("error");
        checks.push((ok_d && !doc_fail, format!("hermes doctor: {}", if ok_d && !doc_fail { "sin errores" } else { "ver log" })));

        let (ok_g, out_g) = run_capture(exe.to_str().unwrap_or("hermes"), &["gateway", "status"]);
        checks.push((ok_g, format!("hermes gateway status: {}", out_g.trim().chars().take(120).collect::<String>())));
    }

    let all_ok = checks.iter().all(|(ok, _)| *ok);
    for (ok, msg) in &checks {
        emit(if *ok { "ok" } else { "fail" }, msg);
    }

    let _ = app.emit(
        "install-done",
        DoneEvent { ok: all_ok, error: None, hints: Vec::new() },
    );
    Ok(())
}

#[tauri::command]
fn cancel_install(state: tauri::State<'_, AppState>) {
    state.cancel_flag.store(true, Ordering::SeqCst);
}

/// Prueba la API key GLM/Z.AI contra el endpoint del Coding Plan (regla zai-coding-plan).
#[tauri::command]
async fn test_glm_key(api_key: String, model: Option<String>) -> Result<serde_json::Value, String> {
    let model = model.unwrap_or_else(|| "glm-5-turbo".into());
    let url = "https://api.z.ai/api/coding/paas/v4/chat/completions";
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| e.to_string())?;
    let body = serde_json::json!({
        "model": model,
        "messages": [{"role": "user", "content": "ping"}],
        "max_tokens": 256
    });
    let resp = client
        .post(url)
        .header("Authorization", format!("Bearer {api_key}"))
        .json(&body)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    let status = resp.status().as_u16();
    let text = resp.text().await.unwrap_or_default();
    let mut hint = String::new();
    if text.contains("1113") {
        hint = "1113: el modelo no está en tu plan (NO es saldo). Prueba glm-5-turbo / glm-4.6.".into();
    } else if text.contains("1311") {
        hint = "1311: tu suscripción no incluye este modelo.".into();
    }
    Ok(serde_json::json!({ "status": status, "ok": status == 200, "hint": hint, "body_head": text.chars().take(200).collect::<String>() }))
}

/// Chat GLM integrado (misma llamada que test_glm_key, streaming por eventos "chat-delta").
#[tauri::command]
async fn chat_send(
    app: AppHandle,
    api_key: String,
    model: Option<String>,
    history: Vec<ChatMsg>,
) -> Result<(), String> {
    let model = model.unwrap_or_else(|| "glm-5-turbo".into());
    let url = "https://api.z.ai/api/coding/paas/v4/chat/completions";
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .build()
        .map_err(|e| e.to_string())?;

    let msgs: Vec<serde_json::Value> = history
        .iter()
        .map(|m| serde_json::json!({"role": m.role, "content": m.content}))
        .collect();

    let resp = client
        .post(url)
        .header("Authorization", format!("Bearer {api_key}"))
        .json(&serde_json::json!({
            "model": model,
            "messages": msgs,
            "stream": false,
            "max_tokens": 4096
        }))
        .send()
        .await
        .map_err(|e| e.to_string())?;

    let status = resp.status().as_u16();
    let text = resp.text().await.unwrap_or_default();

    if status != 200 {
        let mut hint = String::new();
        if text.contains("1113") {
            hint = "1113: modelo fuera del plan (no es saldo). Usa glm-5-turbo o glm-4.6.".into();
        }
        let _ = app.emit(
            "chat-error",
            serde_json::json!({ "status": status, "hint": hint, "body": text.chars().take(300).collect::<String>() }),
        );
        return Ok(());
    }

    // extraer choices[0].message.content; si vino vacío (trampa Z.AI: el modelo
    // gastó el presupuesto en reasoning_content), usar el razonamiento como respuesta
    let parsed: serde_json::Value = serde_json::from_str(&text).unwrap_or(serde_json::Value::Null);
    let mut content = parsed
        .pointer("/choices/0/message/content")
        .and_then(|c| c.as_str())
        .unwrap_or("")
        .to_string();
    if content.trim().is_empty() {
        if let Some(r) = parsed
            .pointer("/choices/0/message/reasoning_content")
            .and_then(|c| c.as_str())
        {
            content = r.to_string();
        }
    }
    let finish = parsed
        .pointer("/choices/0/finish_reason")
        .and_then(|c| c.as_str())
        .unwrap_or("");
    let _ = app.emit(
        "chat-delta",
        serde_json::json!({ "content": content, "finish_reason": finish }),
    );
    Ok(())
}

#[derive(Deserialize)]
struct ChatMsg {
    role: String,
    content: String,
}

// ============================================================
// Tests: clasificador de fallos (los errores reales reportados en Win10/11)
// ============================================================
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clasifica_bom_oficial() {
        let (kind, hint) = classify_line("The assignment expression is not valid");
        assert_eq!(kind, "fail");
        assert!(hint.is_some(), "debe dar hint de BOM");
    }

    #[test]
    fn clasifica_executionpolicy() {
        let (kind, hint) = classify_line("running scripts is disabled on this system");
        assert_eq!(kind, "fail");
        assert!(hint.is_some());
    }

    #[test]
    fn clasifica_no_reconocido() {
        let (kind, _) = classify_line("hermes : the term 'hermes' is not recognized as the name");
        assert_eq!(kind, "fail");
    }

    #[test]
    fn clasifica_acceso_denegado() {
        let (kind, _) = classify_line("At line:1 char:1 ... Access is denied");
        assert_eq!(kind, "fail");
    }

    #[test]
    fn clasifica_codigo_zai() {
        let hs = hints_for("{\"error\":{\"code\":\"1113\",\"message\":\"Insufficient balance\"}}", "es");
        assert!(!hs.is_empty(), "1113 debe generar hint");
        assert!(hs[0].contains("1113"));
    }

    #[test]
    fn no_falla_linea_normal() {
        let (kind, hint) = classify_line("==> Configurando modelo: glm-5-turbo");
        assert_eq!(kind, "step");
        assert!(hint.is_none());
    }

    // ---- Regresion con log REAL de ATENCC-LUCAS (v1.0.1) ----

    #[test]
    fn regresion_descargando_python_no_es_fallo() {
        // v1.0.1 marcaba esto como fallo por el patron "python" generico
        let (kind, hint) = classify_line("-> Downloading Python 3.14");
        assert_ne!(kind, "fail", "'Downloading Python' es progreso, no error");
        assert!(hint.is_none());
    }

    #[test]
    fn regresion_instalado_python_no_es_fallo() {
        let (kind, _) = classify_line("Installed Python 3.14.7 in 1m 09s");
        assert_ne!(kind, "fail");
    }

    #[test]
    fn regresion_hash_verified_no_es_fallo() {
        let (kind, _) = classify_line("-> Installing dependencies (hash-verified via uv.lock)");
        assert_ne!(kind, "fail", "'hash-verified' es progreso, no error");
    }

    #[test]
    fn regresion_dependency_install_failed_si_es_fallo() {
        let (kind, hint) = classify_line("[X] dependency install failed");
        assert_eq!(kind, "fail");
        assert!(hint.is_some(), "debe dar hint de uv/antivirus");
    }

    #[test]
    fn hints_en_chino() {
        let hs = hints_for("UnicodeEncodeError: 'charmap' codec", "zh");
        assert!(!hs.is_empty());
        assert!(hs[0].contains("Terminal") || hs[0].contains("终端") || hs[0].contains("代码页"));
    }
}

// ============================================================
// Boot
// ============================================================
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![
            get_lang,
            detect_host,
            set_csv_path,
            lookup_equipo,
            run_install,
            cancel_install,
            test_glm_key,
            chat_send
        ])
        .run(tauri::generate_context!())
        .expect("error mientras corrida el instalador hermes");
}
