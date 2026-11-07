//! Запуск сборки: подготовка файлов, старт процесса, поток логов и остановка.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::process::Stdio;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};
use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncRead, BufReader};
use tokio::sync::oneshot;

use crate::download::{download_one, Download};
use crate::error::{Error, Result};
use crate::game::args::{split_args, LaunchOptions};
use crate::game::{self, version::VersionJson, LaunchPlan, Player, Stage};
use crate::state::AppState;
use crate::accounts::AccountKind;
use crate::crash::{self, CrashReport};
use crate::instances::Instance;
use crate::{accounts, auth, instances, loaders, versions};

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct Progress {
    instance_id: String,
    stage: Stage,
    done: usize,
    total: usize,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct LogLine {
    instance_id: String,
    line: String,
    stderr: bool,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct Started {
    instance_id: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct Exited {
    instance_id: String,
    code: Option<i32>,
    /// Разбор причины, если игра упала сама, а не её остановили
    crash: Option<CrashReport>,
}

/// Последние строки вывода игры — для разбора вылета, если лог-файл не записался.
type OutputTail = Arc<std::sync::Mutex<VecDeque<String>>>;
const TAIL_LINES: usize = 2000;

/// version JSON берём с диска, а если его нет — по ссылке из манифеста Mojang.
pub(crate) async fn version_json(state: &AppState, id: &str) -> Result<VersionJson> {
    if let Some(json) = game::read_version_json(&state.paths, id)? {
        return Ok(json);
    }
    let manifest = versions::manifest(state, false).await?;
    let entry = manifest
        .versions
        .iter()
        .find(|v| v.id == id)
        .ok_or_else(|| Error::msg(format!("Minecraft {id} not found in Mojang manifest")))?;
    let path = state.paths.version_dir(id).join(format!("{id}.json"));
    download_one(
        &state.http,
        &Download { url: entry.url.clone(), path, sha1: Some(entry.sha1.clone()), size: None },
    )
    .await?;
    game::read_version_json(&state.paths, id)?.ok_or_else(|| Error::msg("version JSON missing after download"))
}

#[tauri::command]
pub async fn launch_instance(app: AppHandle, state: State<'_, AppState>, id: String, join: Option<bool>) -> Result<()> {
    {
        let mut running = state.running.lock().await;
        if running.contains_key(&id) {
            return Err(Error::msg("this instance is already running"));
        }
        running.insert(id.clone(), None);
    }

    let result = start(&app, &state, &id, join.unwrap_or(false)).await;
    if result.is_err() {
        state.running.lock().await.remove(&id);
    }
    result
}

async fn start(app: &AppHandle, state: &AppState, id: &str, join: bool) -> Result<()> {
    let mut instance = instances::load(&state.paths, id)?;
    let accounts = accounts::load(&state.paths)?;
    let account = accounts.current().ok_or_else(|| Error::msg("no account selected"))?;
    let player = match account.kind {
        AccountKind::Offline => Player {
            name: account.name.clone(),
            uuid: account.uuid.clone(),
            access_token: "0".to_owned(),
            user_type: "legacy",
        },
        AccountKind::Microsoft => {
            let (token, profile) = auth::minecraft_token(state, &account.uuid).await?;
            accounts::rename(&state.paths, &account.id, &profile.name)?;
            Player { name: profile.name, uuid: profile.id, access_token: token, user_type: "msa" }
        }
    };

    // Не шлём событие на каждый из тысяч ассетов — только при смене процента
    let mut last = (None, usize::MAX);
    let emit_id = id.to_owned();
    let mut progress = |stage: Stage, done: usize, total: usize| {
        let percent = if total == 0 { 100 } else { done * 100 / total };
        let key = Some(stage as u8);
        if last != (key, percent) {
            last = (key, percent);
            let _ = app.emit("launch-progress", Progress { instance_id: emit_id.clone(), stage, done, total });
        }
    };

    let mut version = version_json(state, &instance.mc_version).await?;
    if let Some(loader) = instance.loader {
        // Версия загрузчика не выбрана (старая сборка) — берём рекомендованную и запоминаем
        let loader_version = match instance.loader_version.clone() {
            Some(v) => v,
            None => {
                let list = loaders::versions(state, loader, &instance.mc_version).await?;
                let v = loaders::pick_default(&list).ok_or_else(|| Error::msg("loader_unavailable"))?.version.clone();
                instance.loader_version = Some(v.clone());
                instances::save(&state.paths, &instance)?;
                v
            }
        };
        version = loaders::apply(state, loader, &loader_version, &version, &mut progress).await?;
    }

    // Офлайн-аккаунт со скином: CustomSkinLoader покажет его в игре. Без сети или без мода
    // под эту версию просто играем со стандартным скином
    if let (AccountKind::Offline, Some(skin)) = (account.kind, account.skin.as_deref()) {
        if let Err(e) = crate::skins::prepare_offline(state, id, &account.name, skin).await {
            eprintln!("offline skin: {e}");
        }
    }

    let settings = &instance.settings;
    let global = crate::settings::load(&state.paths);
    // Общие аргументы JVM первыми: аргументы сборки идут после и побеждают
    let mut extra_jvm = split_args(&global.default_jvm_args);
    extra_jvm.extend(split_args(&settings.jvm_args));
    let server = (join && !settings.server.is_empty()).then(|| settings.server.clone());
    let options = LaunchOptions {
        memory_mb: instance.memory_mb,
        min_memory_mb: settings.min_memory_mb,
        extra_jvm,
        server,
        extra_game: split_args(&settings.game_args),
        resolution: settings.width.zip(settings.height),
        fullscreen: settings.fullscreen,
    };
    let java_override = settings.java_path.as_ref().or(global.default_java_path.as_ref()).map(PathBuf::from);
    let plan = game::prepare(
        &state.http,
        &state.paths,
        &version,
        state.paths.game_dir(id)?,
        &options,
        java_override,
        &player,
        &mut progress,
    )
    .await?;

    let env = hook_env(state, &instance, &plan)?;
    let mut user_env = parse_env(&settings.env);
    user_env.extend(env.iter().cloned());

    if !settings.pre_launch.trim().is_empty() {
        let code = run_hook(app, id, "pre-launch", &settings.pre_launch, &plan.game_dir, &user_env).await?;
        if code != Some(0) {
            return Err(Error::msg(format!("pre_launch_failed:{}", code.map_or("?".to_owned(), |c| c.to_string()))));
        }
    }

    let mut command = tokio::process::Command::new(&plan.java);
    command
        .args(&plan.args)
        .envs(user_env.iter().map(|(k, v)| (k.as_str(), v.as_str())))
        .current_dir(&plan.game_dir)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    // javaw.exe окна консоли и так не открывает. Скрывать окно нужно только для java.exe,
    // а лишний скрытый запуск антивирусы считают подозрительным.
    #[cfg(windows)]
    if !plan.java.file_name().is_some_and(|n| n.eq_ignore_ascii_case("javaw.exe")) {
        command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    let mut child = command.spawn()?;

    let started_at = std::time::SystemTime::now();
    let session_start = chrono::Local::now();
    let tail: OutputTail = Arc::default();
    pipe_logs(app.clone(), id.to_owned(), child.stdout.take(), false, tail.clone());
    pipe_logs(app.clone(), id.to_owned(), child.stderr.take(), true, tail.clone());

    let (stop_tx, stop_rx) = oneshot::channel::<()>();
    state.running.lock().await.insert(id.to_owned(), Some(stop_tx));

    instance.last_played = Some(chrono::Utc::now().to_rfc3339());
    instances::save(&state.paths, &instance)?;
    let _ = app.emit("game-started", Started { instance_id: id.to_owned() });
    if !global.discord_disabled {
        let loader = instance.loader.map(|l| format!(" · {}", serde_json::to_value(l).ok().and_then(|v| v.as_str().map(capitalize)).unwrap_or_default())).unwrap_or_default();
        let activity = crate::discord::Activity {
            details: instance.name.clone(),
            state: format!("Minecraft {}{loader}", instance.mc_version),
            start: chrono::Utc::now().timestamp(),
        };
        tauri::async_runtime::spawn(crate::discord::set_game(Some(activity)));
    }

    let app = app.clone();
    let id = id.to_owned();
    let post_exit = settings.post_exit.trim().to_owned();
    let game_dir = plan.game_dir.clone();
    tauri::async_runtime::spawn(async move {
        let (code, stopped) = tokio::select! {
            status = child.wait() => (status.ok().and_then(|s| s.code()), false),
            _ = stop_rx => {
                let _ = child.kill().await;
                (None, true)
            }
        };
        // Код 0 — обычный выход; Fabric при ошибке модов тоже выходит с ненулевым кодом
        let crash = match code {
            Some(c) if c != 0 && !stopped => {
                // Дать потокам вывода дочитать последние строки
                tokio::time::sleep(std::time::Duration::from_millis(300)).await;
                let lines: Vec<String> = tail.lock().map(|t| t.iter().cloned().collect()).unwrap_or_default();
                let dir = game_dir.clone();
                tokio::task::spawn_blocking(move || crash::analyze(&dir, started_at, &lines)).await.ok()
            }
            _ => None,
        };
        {
            let state = app.state::<AppState>();
            if let Err(e) = crate::playtime::record(&state.paths, &id, session_start, chrono::Local::now()) {
                eprintln!("playtime: {e}");
            }
        }
        if !post_exit.is_empty() {
            let _ = run_hook(&app, &id, "post-exit", &post_exit, &game_dir, &user_env).await;
        }
        {
            let state = app.state::<AppState>();
            let global = crate::settings::load(&state.paths);
            if global.auto_backup {
                let keep = if global.backup_keep == 0 { 5 } else { global.backup_keep as usize };
                crate::worlds::auto_backup(&state, &id, started_at, keep).await;
            }
        }
        let nothing_running = {
            let state = app.state::<AppState>();
            let mut running = state.running.lock().await;
            running.remove(&id);
            running.is_empty()
        };
        if nothing_running {
            crate::discord::set_game(None).await;
        }
        let _ = app.emit("game-exited", Exited { instance_id: id, code, crash });
    });
    Ok(())
}

fn pipe_logs(
    app: AppHandle,
    instance_id: String,
    stream: Option<impl AsyncRead + Unpin + Send + 'static>,
    stderr: bool,
    tail: OutputTail,
) {
    let Some(stream) = stream else { return };
    tauri::async_runtime::spawn(async move {
        let mut reader = BufReader::new(stream);
        let mut buf = Vec::new();
        while let Some(line) = next_line_lossy(&mut reader, &mut buf).await {
            if let Ok(mut tail) = tail.lock() {
                if tail.len() == TAIL_LINES {
                    tail.pop_front();
                }
                tail.push_back(line.clone());
            }
            let _ = app.emit("game-log", LogLine { instance_id: instance_id.clone(), line, stderr });
        }
    });
}

/// Следующая строка вывода. Битые байты (чужая кодировка консоли) заменяются, а не обрывают
/// чтение: иначе канал переполнится и игра зависнет на записи в лог.
pub(crate) async fn next_line_lossy(reader: &mut (impl AsyncBufRead + Unpin), buf: &mut Vec<u8>) -> Option<String> {
    buf.clear();
    match reader.read_until(b'\n', buf).await {
        Ok(0) | Err(_) => None,
        Ok(_) => Some(String::from_utf8_lossy(buf).trim_end_matches(['\r', '\n']).to_owned()),
    }
}

#[tauri::command]
pub async fn stop_instance(state: State<'_, AppState>, id: String) -> Result<()> {
    let mut running = state.running.lock().await;
    if let Some(Some(stop)) = running.get_mut(&id).map(Option::take) {
        let _ = stop.send(());
    }
    Ok(())
}

/// Какие сборки сейчас готовятся или запущены — фронтенд спрашивает это после перезагрузки страницы.
#[tauri::command]
pub async fn running_instances(state: State<'_, AppState>) -> Result<Vec<String>> {
    Ok(state.running.lock().await.keys().cloned().collect())
}

fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    c.next().map(|f| f.to_uppercase().chain(c).collect()).unwrap_or_default()
}

/// Переменные для команд до/после запуска — те же имена, что у Prism Launcher.
fn hook_env(state: &AppState, instance: &Instance, plan: &LaunchPlan) -> Result<Vec<(String, String)>> {
    let dir = state.paths.instance_dir(&instance.id)?;
    Ok(vec![
        ("INST_ID".into(), instance.id.clone()),
        ("INST_NAME".into(), instance.name.clone()),
        ("INST_DIR".into(), dir.to_string_lossy().into_owned()),
        ("INST_MC_DIR".into(), plan.game_dir.to_string_lossy().into_owned()),
        ("INST_MC_VER".into(), instance.mc_version.clone()),
        ("INST_JAVA".into(), plan.java.to_string_lossy().into_owned()),
    ])
}

/// Строки KEY=VALUE; пустые строки и строки с # пропускаются.
fn parse_env(text: &str) -> Vec<(String, String)> {
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .filter_map(|l| l.split_once('='))
        .map(|(k, v)| (k.trim().to_owned(), v.trim().to_owned()))
        .filter(|(k, _)| !k.is_empty())
        .collect()
}

/// Выполняет команду через системную оболочку, её вывод попадает в консоль сборки.
async fn run_hook(
    app: &AppHandle,
    instance_id: &str,
    label: &str,
    script: &str,
    dir: &Path,
    env: &[(String, String)],
) -> Result<Option<i32>> {
    let mut command = if cfg!(windows) {
        let mut c = tokio::process::Command::new("cmd");
        c.arg("/C").arg(script);
        c
    } else {
        let mut c = tokio::process::Command::new("sh");
        c.arg("-c").arg(script);
        c
    };
    command
        .current_dir(dir)
        .envs(env.iter().map(|(k, v)| (k.as_str(), v.as_str())))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    command.creation_flags(0x0800_0000);

    let emit = |line: String, stderr: bool| {
        let _ = app.emit("game-log", LogLine { instance_id: instance_id.to_owned(), line, stderr });
    };
    emit(format!("[{label}] {script}"), false);
    let output = command.output().await?;
    for line in String::from_utf8_lossy(&output.stdout).lines() {
        emit(format!("[{label}] {line}"), false);
    }
    for line in String::from_utf8_lossy(&output.stderr).lines() {
        emit(format!("[{label}] {line}"), true);
    }
    Ok(output.status.code())
}

#[tauri::command]
pub async fn java_detect(state: State<'_, AppState>) -> Result<Vec<game::java::JavaInfo>> {
    Ok(game::java::detect(&state.paths.runtimes()).await)
}

#[tauri::command]
pub async fn java_probe(path: String) -> Result<game::java::JavaInfo> {
    game::java::probe(Path::new(&path)).await
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemInfo {
    pub total_memory_mb: u64,
}

#[tauri::command]
pub fn system_info() -> SystemInfo {
    let mut sys = sysinfo::System::new();
    sys.refresh_memory();
    SystemInfo { total_memory_mb: sys.total_memory() / 1024 / 1024 }
}
