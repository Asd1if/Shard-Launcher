//! Серверы: ядро (ванильное, Paper, Purpur, Fabric, Quilt, Forge, NeoForge), плагины и моды,
//! запуск с консолью, server.properties и адреса, по которым могут зайти друзья.
//!
//! Сервер не привязан к сборке: всё лежит в `<данные>/servers/<id>/`, настройки — в
//! `shard-server.json`. Сборка может быть лишь источником модов, мира или серверного архива.

use std::collections::{BTreeSet, HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{LazyLock, OnceLock};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::ipc::Channel;
use tauri::{AppHandle, Emitter, State};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::{oneshot, Mutex};

use crate::download::{download_one, Download};
use crate::error::{Error, Result};
use crate::instances::{self, ModLoader};
use crate::share::{self, Share};
use crate::state::AppState;

const CONFIG: &str = "shard-server.json";
const LOG_LINES: usize = 2000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Core {
    Vanilla,
    Paper,
    Purpur,
    Fabric,
    Quilt,
    Forge,
    Neoforge,
}

impl Core {
    pub(crate) fn loader(self) -> Option<ModLoader> {
        match self {
            Core::Fabric => Some(ModLoader::Fabric),
            Core::Quilt => Some(ModLoader::Quilt),
            Core::Forge => Some(ModLoader::Forge),
            Core::Neoforge => Some(ModLoader::Neoforge),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerConfig {
    #[serde(default)]
    pub name: String,
    pub core: Core,
    pub mc_version: String,
    pub loader_version: Option<String>,
    pub memory_mb: u32,
    #[serde(default)]
    pub jvm_args: String,
    /// Открывать порт на роутере (UPnP) и показывать адрес для интернета
    #[serde(default)]
    pub share: bool,
    /// Открывать туннель playit.gg при запуске — постоянный адрес без белого IP
    #[serde(default)]
    pub tunnel: bool,
    /// Аргументы после java: `-jar server.jar nogui` или `@libraries/…/win_args.txt nogui`
    pub launch: Vec<String>,
    pub java_component: String,
    /// Моды, которые скопировал лаунчер — их можно заменить при синхронизации
    #[serde(default)]
    pub copied_mods: Vec<String>,
    /// Версия модпака, с которой взят серверный набор (для сборок с Modrinth и CurseForge)
    #[serde(default)]
    pub from_pack: Option<String>,
    /// Сборка, из которой брали моды, — для повторной синхронизации
    #[serde(default)]
    pub source: Option<String>,
    pub created_at: String,
    /// Перезапускать, если сервер упал
    #[serde(default)]
    pub auto_restart: bool,
    /// Запускать вместе с лаунчером
    #[serde(default)]
    pub autostart: bool,
    /// Плановый перезапуск каждые N часов, 0 — выключен
    #[serde(default)]
    pub restart_hours: u32,
    /// Копия мира перед каждым запуском (хранятся последние 10)
    #[serde(default)]
    pub backup_on_start: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Stopped,
    Starting,
    Running,
    Stopping,
}

/// Запущенный сервер: ввод консоли, статус, игроки, адреса.
struct Live {
    stdin: tokio::process::ChildStdin,
    status: Status,
    players: BTreeSet<String>,
    share: Option<Share>,
    /// Адрес туннеля playit.gg или код ошибки
    tunnel: Option<std::result::Result<String, String>>,
    kill: Option<oneshot::Sender<()>>,
    /// Номер запуска: таймеры прошлого запуска не трогают новый
    run: u64,
    pid: Option<u32>,
    started: Instant,
    /// Остановлен для планового перезапуска — после выхода запустить снова
    restart: bool,
}

static RUNS: AtomicU64 = AtomicU64::new(1);

// ---------- автозапуск и перезапуск ----------

/// Очередь запусков: перезапуск после падения, плановый, автозапуск с лаунчером. Запускает
/// отдельная задача, чтобы server_start не вызывал сам себя.
static STARTS: OnceLock<tokio::sync::mpsc::UnboundedSender<(String, Duration)>> = OnceLock::new();

fn request_start(id: String, delay: Duration) {
    if let Some(tx) = STARTS.get() {
        let _ = tx.send((id, delay));
    }
}

/// Запускает очередь и серверы с автозапуском. Вызывается один раз при старте лаунчера.
pub fn spawn_supervisor(app: AppHandle) {
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<(String, Duration)>();
    if STARTS.set(tx).is_err() {
        return;
    }
    let starter = app.clone();
    tauri::async_runtime::spawn(async move {
        while let Some((id, delay)) = rx.recv().await {
            let app = starter.clone();
            tauri::async_runtime::spawn(async move {
                tokio::time::sleep(delay).await;
                let state = tauri::Manager::state::<AppState>(&app);
                if let Err(e) = server_start(app.clone(), state, id.clone()).await {
                    push_log(&id, format!("[Shard] {e}"));
                }
            });
        }
    });
    let state = tauri::Manager::state::<AppState>(&app);
    for entry in server_list(state) {
        if entry.config.autostart {
            request_start(entry.id, Duration::from_secs(2));
        }
    }
}

// ---------- нагрузка ----------

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Stats {
    /// Процент от всех ядер
    cpu: f32,
    memory_mb: u64,
    players: usize,
    uptime_secs: u64,
}

static SYS: LazyLock<std::sync::Mutex<sysinfo::System>> = LazyLock::new(|| std::sync::Mutex::new(sysinfo::System::new()));

#[tauri::command]
pub async fn server_stats(id: String) -> Option<Stats> {
    let (pid, players, uptime) = {
        let live = LIVE.lock().await;
        let l = live.get(&id)?;
        (l.pid?, l.players.len(), l.started.elapsed().as_secs())
    };
    let mut sys = SYS.lock().expect("lock");
    let pid = sysinfo::Pid::from_u32(pid);
    sys.refresh_process(pid);
    let process = sys.process(pid)?;
    let cores = std::thread::available_parallelism().map_or(1, |n| n.get()) as f32;
    Some(Stats { cpu: process.cpu_usage() / cores, memory_mb: process.memory() / 1024 / 1024, players, uptime_secs: uptime })
}

static LIVE: LazyLock<Mutex<HashMap<String, Live>>> = LazyLock::new(Default::default);
/// Хвост консоли остаётся и после остановки, чтобы было видно, почему сервер упал
static LOGS: LazyLock<std::sync::Mutex<HashMap<String, VecDeque<String>>>> = LazyLock::new(Default::default);

/// Кто сейчас на сервере (по строкам «joined/left the game» в консоли).
pub async fn online_players(id: &str) -> Vec<String> {
    LIVE.lock().await.get(id).map(|l| l.players.iter().cloned().collect()).unwrap_or_default()
}

async fn is_run(id: &str, run: u64) -> bool {
    LIVE.lock().await.get(id).is_some_and(|l| l.run == run)
}

pub async fn is_running(id: &str) -> bool {
    LIVE.lock().await.contains_key(id)
}

fn servers_root(paths: &crate::paths::Paths) -> PathBuf {
    paths.root.join("servers")
}

pub(crate) fn server_dir(state: &AppState, id: &str) -> Result<PathBuf> {
    // id приходит из фронтенда — не даём выйти за пределы папки серверов
    if id.is_empty() || !id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
        return Err(Error::msg("server_missing"));
    }
    Ok(servers_root(&state.paths).join(id))
}

/// Серверы старых версий жили внутри сборки (`<сборка>/server`) — переносим их в `servers/`.
pub fn migrate(paths: &crate::paths::Paths) {
    let Ok(entries) = std::fs::read_dir(paths.instances()) else { return };
    for entry in entries.flatten() {
        let old = entry.path().join("server");
        let Some(mut config) = load_config(&old) else { continue };
        let Some(meta) = std::fs::read(entry.path().join("instance.json")).ok().and_then(|b| serde_json::from_slice::<Value>(&b).ok()) else {
            continue;
        };
        let Some(id) = meta.get("id").and_then(Value::as_str).map(str::to_owned) else { continue };
        let target = servers_root(paths).join(&id);
        if target.exists() || std::fs::create_dir_all(servers_root(paths)).is_err() || std::fs::rename(&old, &target).is_err() {
            continue;
        }
        if config.name.is_empty() {
            config.name = meta.get("name").and_then(Value::as_str).unwrap_or("Server").to_owned();
        }
        config.source.get_or_insert(id);
        let _ = save_config(&target, &config);
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerEntry {
    id: String,
    config: ServerConfig,
}

#[tauri::command]
pub fn server_list(state: State<'_, AppState>) -> Vec<ServerEntry> {
    let mut list: Vec<ServerEntry> = std::fs::read_dir(servers_root(&state.paths))
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| Some(ServerEntry { config: load_config(&e.path())?, id: e.file_name().to_str()?.to_owned() }))
        .collect();
    list.sort_by(|a, b| a.config.created_at.cmp(&b.config.created_at));
    list
}

#[tauri::command]
pub fn server_rename(state: State<'_, AppState>, id: String, name: String) -> Result<ServerConfig> {
    let dir = server_dir(&state, &id)?;
    let mut config = load_config(&dir).ok_or_else(|| Error::msg("server_missing"))?;
    let name = name.trim();
    if !name.is_empty() {
        config.name = name.chars().take(64).collect();
        save_config(&dir, &config)?;
    }
    Ok(config)
}

/// Открывает папку сервера или папку внутри неё (`plugins`, `mods`).
#[tauri::command]
pub fn server_open_folder(app: AppHandle, state: State<'_, AppState>, id: String, sub: Option<String>) -> Result<()> {
    use tauri_plugin_opener::OpenerExt;
    let mut path = server_dir(&state, &id)?;
    if let Some(sub) = sub.filter(|s| !s.is_empty()) {
        path = crate::server_files::resolve(&path, &sub)?;
    }
    std::fs::create_dir_all(&path)?;
    app.opener().open_path(path.to_string_lossy(), None::<&str>).map_err(|e| Error::msg(e.to_string()))
}

pub(crate) fn load_config(dir: &Path) -> Option<ServerConfig> {
    serde_json::from_slice(&std::fs::read(dir.join(CONFIG)).ok()?).ok()
}

fn save_config(dir: &Path, config: &ServerConfig) -> Result<()> {
    std::fs::write(dir.join(CONFIG), serde_json::to_vec_pretty(config)?)?;
    Ok(())
}

// ---------- server.properties ----------

/// Пары ключ-значение без комментариев.
pub(crate) fn read_properties(dir: &Path) -> Vec<(String, String)> {
    std::fs::read_to_string(dir.join("server.properties"))
        .unwrap_or_default()
        .lines()
        .filter(|l| !l.trim_start().starts_with('#'))
        .filter_map(|l| l.split_once('='))
        .map(|(k, v)| (k.trim().to_owned(), v.trim().to_owned()))
        .collect()
}

/// Меняет значения, сохраняя порядок строк и комментарии; новых ключей дописывает в конец.
pub(crate) fn write_properties(dir: &Path, values: &HashMap<String, String>) -> Result<()> {
    let path = dir.join("server.properties");
    let text = std::fs::read_to_string(&path).unwrap_or_default();
    let mut rest = values.clone();
    let mut out: Vec<String> = text
        .lines()
        .map(|line| match line.split_once('=') {
            Some((k, _)) if !line.trim_start().starts_with('#') => match rest.remove(k.trim()) {
                Some(v) => format!("{}={v}", k.trim()),
                None => line.to_owned(),
            },
            _ => line.to_owned(),
        })
        .collect();
    let mut added: Vec<_> = rest.into_iter().collect();
    added.sort();
    out.extend(added.into_iter().map(|(k, v)| format!("{k}={v}")));
    std::fs::write(path, out.join("\n") + "\n")?;
    Ok(())
}

fn port_of(dir: &Path) -> u16 {
    read_properties(dir)
        .into_iter()
        .find(|(k, _)| k == "server-port")
        .and_then(|(_, v)| v.parse().ok())
        .unwrap_or(25565)
}

// ---------- информация ----------

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerInfo {
    config: Option<ServerConfig>,
    status: Status,
    players: Vec<String>,
    port: u16,
    share: Option<Share>,
    tunnel: Option<String>,
    tunnel_error: Option<String>,
    /// Адрес в локальной сети — работает и без UPnP
    lan: Option<String>,
}

#[tauri::command]
pub async fn server_info(state: State<'_, AppState>, id: String) -> Result<ServerInfo> {
    let dir = server_dir(&state, &id)?;
    let port = port_of(&dir);
    let live = LIVE.lock().await;
    let running = live.get(&id);
    Ok(ServerInfo {
        config: load_config(&dir),
        status: running.map_or(Status::Stopped, |l| l.status),
        players: running.map(|l| l.players.iter().cloned().collect()).unwrap_or_default(),
        port,
        share: running.and_then(|l| l.share.clone()),
        tunnel: running.and_then(|l| l.tunnel.clone()).and_then(|t| t.ok()),
        tunnel_error: running.and_then(|l| l.tunnel.clone()).and_then(|t| t.err()),
        lan: share::lan_ip().map(|ip| format!("{ip}:{port}")),
    })
}

/// Добавляет сервер в список «Сетевая игра» сборки. `address` — какой адрес вписать
/// (локальный, для друзей или туннель); по умолчанию localhost с портом сервера.
#[tauri::command]
pub fn server_add_to_instance(state: State<'_, AppState>, id: String, instance_id: String, address: Option<String>) -> Result<bool> {
    let dir = server_dir(&state, &id)?;
    let config = load_config(&dir).ok_or_else(|| Error::msg("server_missing"))?;
    let port = port_of(&dir);
    let address = address
        .map(|a| a.trim().to_owned())
        .filter(|a| !a.is_empty() && !a.contains(char::is_whitespace))
        .unwrap_or_else(|| if port == 25565 { "localhost".to_owned() } else { format!("localhost:{port}") });
    let name = if config.name.trim().is_empty() { "Shard".to_owned() } else { config.name.clone() };
    crate::nbt::add_server(&state.paths.game_dir(&instance_id)?.join("servers.dat"), &name, &address)
}

#[tauri::command]
pub fn server_logs(id: String) -> Vec<String> {
    LOGS.lock().expect("lock").get(&id).map(|l| l.iter().cloned().collect()).unwrap_or_default()
}

/// Версии Minecraft, для которых есть ядро. Для ванили, Fabric и прочих — None: подходят все.
#[tauri::command]
pub async fn server_core_versions(state: State<'_, AppState>, core: Core) -> Result<Option<Vec<String>>> {
    let http = &state.http;
    Ok(match core {
        Core::Paper => Some(paper_versions(http).await?),
        Core::Purpur => {
            let v: Value = http.get("https://api.purpurmc.org/v2/purpur").send().await?.error_for_status()?.json().await?;
            let mut list: Vec<String> = v["versions"].as_array().into_iter().flatten().filter_map(|x| x.as_str().map(str::to_owned)).collect();
            list.reverse();
            Some(list)
        }
        _ => None,
    })
}

// ---------- создание ----------

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateArgs {
    /// Название сервера (по умолчанию — ядро и версия)
    #[serde(default)]
    pub name: String,
    pub core: Core,
    pub mc_version: String,
    pub loader_version: Option<String>,
    /// Сборка, из которой взять моды, конфиги и мир; сервер к ней не привязывается
    #[serde(default)]
    pub source: Option<String>,
    /// Скопировать моды и конфиги из сборки-источника
    #[serde(default)]
    pub copy_mods: bool,
    /// Папка мира из saves сборки-источника
    pub world: Option<String>,
    /// Пользователь принял EULA Minecraft
    pub eula: bool,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateProgress {
    stage: &'static str,
    done: usize,
    total: usize,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModsResult {
    copied: usize,
    /// Моды только для клиента — на сервер их не кладём
    skipped: Vec<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateResult {
    id: String,
    config: ServerConfig,
    mods: Option<ModsResult>,
}

/// Папка нового сервера, а при пересборке — существующего.
async fn target_dir(state: &AppState, id: Option<String>) -> Result<(String, PathBuf)> {
    let id = match id {
        Some(id) => {
            if is_running(&id).await {
                return Err(Error::msg("server_running"));
            }
            id
        }
        None => uuid::Uuid::new_v4().to_string(),
    };
    let dir = server_dir(state, &id)?;
    tokio::fs::create_dir_all(&dir).await?;
    Ok((id, dir))
}

/// Создаёт сервер или пересобирает существующий (`id`).
#[tauri::command]
pub async fn server_create(
    state: State<'_, AppState>,
    id: Option<String>,
    args: CreateArgs,
    progress: Channel<CreateProgress>,
) -> Result<CreateResult> {
    if !args.eula {
        return Err(Error::msg("server_eula"));
    }
    if args.core.loader().is_some() && args.loader_version.as_deref().is_none_or(str::is_empty) {
        return Err(Error::msg("server_no_loader_version"));
    }
    let source = match &args.source {
        Some(s) if !s.is_empty() => Some(instances::load(&state.paths, s)?),
        _ => None,
    };
    let (id, dir) = target_dir(&state, id).await?;
    let step = |stage: &'static str, done: usize, total: usize| {
        let _ = progress.send(CreateProgress { stage, done, total });
    };
    let loader_version = args.loader_version.clone().filter(|_| args.core.loader().is_some());
    let name = if args.name.trim().is_empty() {
        load_config(&dir).map(|c| c.name).filter(|n| !n.is_empty()).unwrap_or_else(|| format!("{:?} {}", args.core, args.mc_version))
    } else {
        args.name.trim().chars().take(64).collect()
    };
    let mut config = prepare(&state, &name, &dir, args.core, &args.mc_version, loader_version, &step).await?;

    let mut mods = None;
    if let Some(instance) = &source {
        if let Some(world) = args.world.as_deref().filter(|w| !w.is_empty()) {
            step("world", 0, 1);
            copy_world(&state, &instance.id, world, &dir)?;
        }
        // Моды нужны, только если ядро с тем же загрузчиком, что и сборка
        let same_loader = args.core.loader().is_some_and(|l| instance.loader == Some(l) || (l == ModLoader::Quilt && instance.loader == Some(ModLoader::Fabric)));
        if args.copy_mods && same_loader {
            step("mods", 0, 1);
            config.source = Some(instance.id.clone());
            mods = Some(sync_mods(&state, &instance.id, &dir, &mut config).await?);
        }
    }
    save_config(&dir, &config)?;
    Ok(CreateResult { id, config, mods })
}

/// Общее для любого сервера: Java, ядро, EULA, server.properties и настройки.
async fn prepare(
    state: &AppState,
    name: &str,
    dir: &Path,
    core: Core,
    mc: &str,
    loader_version: Option<String>,
    step: &(dyn Fn(&'static str, usize, usize) + Sync),
) -> Result<ServerConfig> {
    let vanilla = crate::launch::version_json(state, mc).await?;
    let java_component = vanilla
        .java_version
        .as_ref()
        .map(|j| j.component.clone())
        .unwrap_or_else(|| crate::game::java::LEGACY_COMPONENT.to_owned());
    step("java", 0, 1);
    let java = crate::game::java::ensure(&state.http, &state.paths.runtimes(), &java_component, |d, t| step("java", d, t)).await?;

    step("core", 0, 1);
    let launch = install_core(state, dir, core, &vanilla, loader_version.as_deref(), &java, step).await?;

    std::fs::write(dir.join("eula.txt"), "# Принято в Shard: https://aka.ms/MinecraftEULA\neula=true\n")?;
    if !dir.join("server.properties").exists() {
        let motd = format!("{name} \u{2014} Shard").replace(['\n', '\r'], " ");
        write_properties(dir, &HashMap::from([("motd".to_owned(), motd), ("server-port".to_owned(), "25565".to_owned())]))?;
    }

    let previous = load_config(dir);
    Ok(ServerConfig {
        name: name.to_owned(),
        core,
        mc_version: mc.to_owned(),
        loader_version,
        memory_mb: previous.as_ref().map_or_else(|| default_memory(core), |c| c.memory_mb),
        jvm_args: previous.as_ref().map(|c| c.jvm_args.clone()).unwrap_or_default(),
        share: previous.as_ref().is_some_and(|c| c.share),
        tunnel: previous.as_ref().is_some_and(|c| c.tunnel),
        auto_restart: previous.as_ref().is_some_and(|c| c.auto_restart),
        autostart: previous.as_ref().is_some_and(|c| c.autostart),
        restart_hours: previous.as_ref().map_or(0, |c| c.restart_hours),
        backup_on_start: previous.as_ref().is_some_and(|c| c.backup_on_start),
        launch,
        java_component,
        copied_mods: previous.as_ref().map(|c| c.copied_mods.clone()).unwrap_or_default(),
        from_pack: None,
        source: previous.and_then(|c| c.source),
        created_at: chrono::Utc::now().to_rfc3339(),
    })
}

/// Сервер из серверной версии модпака сборки: файлы автора сборки плюс ядро той же версии.
#[tauri::command]
pub async fn server_create_from_pack(
    state: State<'_, AppState>,
    id: Option<String>,
    instance_id: String,
    name: Option<String>,
    eula: bool,
    progress: Channel<CreateProgress>,
) -> Result<CreateResult> {
    if !eula {
        return Err(Error::msg("server_eula"));
    }
    let instance = instances::load(&state.paths, &instance_id)?;
    let origin = instance.pack.clone().ok_or_else(|| Error::msg("pack_not_linked"))?;
    let (id, dir) = target_dir(&state, id).await?;
    let step = |stage: &'static str, done: usize, total: usize| {
        let _ = progress.send(CreateProgress { stage, done, total });
    };

    step("pack", 0, 1);
    let steps = crate::modpack::server_pack::Steps {
        files: &|d, t| step("pack", d, t),
        bytes: &|d, t| step("packDownload", d as usize, t as usize),
    };
    let official = crate::modpack::server_pack::install(&state, &origin, &dir, &steps).await?;

    let core = instance.loader.map_or(Core::Vanilla, |l| match l {
        ModLoader::Fabric => Core::Fabric,
        ModLoader::Quilt => Core::Quilt,
        ModLoader::Forge => Core::Forge,
        ModLoader::Neoforge => Core::Neoforge,
    });
    // «Рекомендованная» версия загрузчика у сборки — узнаём, какая именно
    let loader_version = match (instance.loader, instance.loader_version.clone()) {
        (Some(_), Some(v)) => Some(v),
        (Some(loader), None) => {
            let list = crate::loaders::versions(&state, loader, &instance.mc_version).await?;
            Some(crate::loaders::pick_default(&list).ok_or_else(|| Error::msg("loader_unavailable"))?.version.clone())
        }
        (None, _) => None,
    };
    let name = name.map(|n| n.trim().chars().take(64).collect::<String>()).filter(|n| !n.is_empty()).unwrap_or_else(|| instance.name.clone());
    let mut config = prepare(&state, &name, &dir, core, &instance.mc_version, loader_version, &step).await?;
    config.from_pack = Some(origin.version_name.clone().unwrap_or_else(|| instance.name.clone()));
    let mods = if official {
        config.copied_mods.clear();
        None
    } else {
        // Серверного архива нет — берём моды и файлы самой сборки
        step("mods", 0, 1);
        config.source = Some(instance.id.clone());
        let result = sync_mods(&state, &instance.id, &dir, &mut config).await?;
        copy_pack_folders(&state.paths.game_dir(&instance.id)?, &dir)?;
        Some(result)
    };
    save_config(&dir, &config)?;
    Ok(CreateResult { id, config, mods })
}

/// Иконка сервера в списке серверов игры: PNG 64×64, его рисует фронтенд.
#[tauri::command]
pub fn server_set_icon(state: State<'_, AppState>, id: String, png: String) -> Result<()> {
    use base64::Engine;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(png.strip_prefix("data:image/png;base64,").unwrap_or(&png))
        .map_err(|_| Error::msg("server_icon_bad"))?;
    // Сигнатура PNG и ширина/высота из IHDR: игра принимает только 64×64
    let size = |at: usize| bytes.get(at..at + 4).map(|b| u32::from_be_bytes([b[0], b[1], b[2], b[3]]));
    if !bytes.starts_with(b"\x89PNG\r\n\x1a\n") || size(16) != Some(64) || size(20) != Some(64) || bytes.len() > 256 * 1024 {
        return Err(Error::msg("server_icon_bad"));
    }
    let dir = server_dir(&state, &id)?;
    std::fs::create_dir_all(&dir)?;
    std::fs::write(dir.join("server-icon.png"), bytes)?;
    Ok(())
}

fn default_memory(core: Core) -> u32 {
    if core.loader().is_some() { 4096 } else { 2048 }
}

/// Ставит ядро и возвращает аргументы запуска после `java`.
async fn install_core(
    state: &AppState,
    dir: &Path,
    core: Core,
    vanilla: &crate::game::version::VersionJson,
    loader_version: Option<&str>,
    java: &Path,
    step: &(dyn Fn(&'static str, usize, usize) + Sync),
) -> Result<Vec<String>> {
    let http = &state.http;
    let mc = vanilla.id.as_str();
    let jar = |name: &str| vec!["-jar".to_owned(), name.to_owned(), "nogui".to_owned()];
    let vanilla_server = || async {
        let server = vanilla.downloads.server.as_ref().ok_or_else(|| Error::msg("server_unavailable"))?;
        download_one(
            http,
            &Download { url: server.url.clone(), path: dir.join("server.jar"), sha1: server.sha1.clone(), size: server.size },
        )
        .await
    };

    match core {
        Core::Vanilla => {
            vanilla_server().await?;
            Ok(jar("server.jar"))
        }
        Core::Paper => {
            let url = paper_download(http, mc).await?;
            fetch_fresh(http, &url, &dir.join("server.jar")).await?;
            Ok(jar("server.jar"))
        }
        Core::Purpur => {
            let url = format!("https://api.purpurmc.org/v2/purpur/{mc}/latest/download");
            fetch_fresh(http, &url, &dir.join("server.jar")).await?;
            Ok(jar("server.jar"))
        }
        Core::Fabric => {
            // Лаунчер Fabric сам качает ванильный сервер, но мы уже знаем его хеш — кладём сразу
            vanilla_server().await?;
            let installers: Value = http.get("https://meta.fabricmc.net/v2/versions/installer").send().await?.error_for_status()?.json().await?;
            let installer = installers
                .as_array()
                .and_then(|l| l.iter().find(|i| i["stable"] == true).or(l.first()))
                .and_then(|i| i["version"].as_str())
                .ok_or_else(|| Error::msg("server_core_unavailable"))?;
            let loader = loader_version.expect("checked");
            let url = format!("https://meta.fabricmc.net/v2/versions/loader/{mc}/{loader}/{installer}/server/jar");
            fetch_fresh(http, &url, &dir.join("fabric-server-launch.jar")).await?;
            Ok(jar("fabric-server-launch.jar"))
        }
        Core::Quilt => {
            let base = "https://maven.quiltmc.org/repository/release/org/quiltmc/quilt-installer";
            let xml = http.get(format!("{base}/maven-metadata.xml")).send().await?.error_for_status()?.text().await?;
            let version = xml
                .split("<release>")
                .nth(1)
                .and_then(|s| s.split_once("</release>"))
                .map(|(v, _)| v.trim().to_owned())
                .ok_or_else(|| Error::msg("server_core_unavailable"))?;
            let installer = dir.join("quilt-installer.jar");
            fetch_fresh(http, &format!("{base}/{version}/quilt-installer-{version}.jar"), &installer).await?;
            step("install", 0, 1);
            let loader = loader_version.expect("checked");
            let install_dir = format!("--install-dir={}", dir.display());
            run_installer(java, dir, &["-jar", "quilt-installer.jar", "install", "server", mc, loader, &install_dir, "--download-server"]).await?;
            let _ = std::fs::remove_file(installer);
            Ok(jar("quilt-server-launch.jar"))
        }
        Core::Forge | Core::Neoforge => {
            let loader = core.loader().expect("modded");
            let url = crate::loaders::forge::installer_url(loader, mc, loader_version.expect("checked"));
            let installer = dir.join("installer.jar");
            fetch_fresh(http, &url, &installer).await?;
            step("install", 0, 1);
            run_installer(java, dir, &["-jar", "installer.jar", "--installServer"]).await?;
            let _ = std::fs::remove_file(&installer);
            let _ = std::fs::remove_file(dir.join("installer.jar.log"));
            forge_launch(dir).ok_or_else(|| Error::msg("server_install_failed"))
        }
    }
}

/// Новый Forge кладёт аргументы в `libraries/…/win_args.txt`, старый — запускаемый jar в корень.
fn forge_launch(dir: &Path) -> Option<Vec<String>> {
    let name = if cfg!(windows) { "win_args.txt" } else { "unix_args.txt" };
    let mut stack = vec![dir.join("libraries")];
    let mut found = vec![];
    while let Some(d) = stack.pop() {
        for entry in std::fs::read_dir(&d).into_iter().flatten().flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if entry.file_name() == name {
                found.push(path);
            }
        }
    }
    // Если в папке остались аргументы от прошлой версии — берём самые свежие
    found.sort_by_key(|p| std::fs::metadata(p).and_then(|m| m.modified()).ok());
    if let Some(args) = found.pop() {
        let rel = args.strip_prefix(dir).ok()?.to_string_lossy().replace('\\', "/");
        return Some(vec![format!("@{rel}"), "nogui".to_owned()]);
    }
    let jar = std::fs::read_dir(dir).ok()?.flatten().map(|e| e.file_name().to_string_lossy().into_owned()).find(|n| {
        let n = n.to_ascii_lowercase();
        n.ends_with(".jar") && (n.starts_with("forge-") || n.starts_with("minecraftforge")) && !n.contains("installer")
    })?;
    Some(vec!["-jar".to_owned(), jar, "nogui".to_owned()])
}

async fn run_installer(java: &Path, dir: &Path, args: &[&str]) -> Result<()> {
    let mut command = tokio::process::Command::new(java);
    command.args(args).current_dir(dir).stdin(Stdio::null());
    #[cfg(windows)]
    if !java.file_name().is_some_and(|n| n.eq_ignore_ascii_case("javaw.exe")) {
        command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    let output = command.output().await?;
    if !output.status.success() {
        let text = String::from_utf8_lossy(&output.stdout);
        let tail: Vec<&str> = text.lines().rev().take(8).collect::<Vec<_>>().into_iter().rev().collect();
        return Err(Error::msg(format!("server_install_failed:{}", tail.join("\n"))));
    }
    Ok(())
}

/// Ядра Paper и Purpur обновляются под тем же именем — качаем всегда заново.
async fn fetch_fresh(http: &reqwest::Client, url: &str, path: &Path) -> Result<()> {
    let bytes = http.get(url).send().await?.error_for_status()?.bytes().await?;
    crate::download::write_atomic(path, &bytes).await
}

async fn paper_versions(http: &reqwest::Client) -> Result<Vec<String>> {
    // Новый API Fill; старый v2 — запасной
    if let Ok(r) = http.get("https://fill.papermc.io/v3/projects/paper").send().await {
        if let Ok(v) = r.error_for_status()?.json::<Value>().await {
            let mut list: Vec<String> = v["versions"]
                .as_object()
                .into_iter()
                .flat_map(|groups| groups.values())
                .flat_map(|g| g.as_array().cloned().unwrap_or_default())
                .filter_map(|x| x.as_str().map(str::to_owned))
                .collect();
            if !list.is_empty() {
                list.sort_by(|a, b| version_key(b).cmp(&version_key(a)));
                return Ok(list);
            }
        }
    }
    let v: Value = http.get("https://api.papermc.io/v2/projects/paper").send().await?.error_for_status()?.json().await?;
    let mut list: Vec<String> = v["versions"].as_array().into_iter().flatten().filter_map(|x| x.as_str().map(str::to_owned)).collect();
    list.reverse();
    Ok(list)
}

async fn paper_download(http: &reqwest::Client, mc: &str) -> Result<String> {
    let v3 = http.get(format!("https://fill.papermc.io/v3/projects/paper/versions/{mc}/builds")).send().await;
    if let Ok(builds) = v3.and_then(|r| r.error_for_status()) {
        if let Ok(Value::Array(list)) = builds.json::<Value>().await {
            let best = list
                .iter()
                .filter(|b| b["channel"].as_str().is_none_or(|c| c.eq_ignore_ascii_case("stable")))
                .max_by_key(|b| b["id"].as_u64())
                .or_else(|| list.iter().max_by_key(|b| b["id"].as_u64()));
            if let Some(url) = best.and_then(|b| b["downloads"]["server:default"]["url"].as_str()) {
                return Ok(url.to_owned());
            }
        }
    }
    let v: Value = http
        .get(format!("https://api.papermc.io/v2/projects/paper/versions/{mc}/builds"))
        .send()
        .await?
        .error_for_status()
        .map_err(|_| Error::msg("server_core_unavailable"))?
        .json()
        .await?;
    let build = v["builds"].as_array().and_then(|b| b.last()).ok_or_else(|| Error::msg("server_core_unavailable"))?;
    let number = build["build"].as_u64().unwrap_or_default();
    let name = build["downloads"]["application"]["name"].as_str().ok_or_else(|| Error::msg("server_core_unavailable"))?;
    Ok(format!("https://api.papermc.io/v2/projects/paper/versions/{mc}/builds/{number}/downloads/{name}"))
}

/// «1.21.4» → [1, 21, 4] для сортировки; хвосты вроде «-pre1» не учитываем.
fn version_key(v: &str) -> Vec<u32> {
    v.split(['.', '-']).map_while(|p| p.parse().ok()).collect()
}

fn copy_world(state: &AppState, id: &str, world: &str, dir: &Path) -> Result<()> {
    if world.is_empty() || world.contains(['/', '\\']) || world.starts_with('.') {
        return Err(Error::msg("world_not_found"));
    }
    let source = state.paths.game_dir(id)?.join("saves").join(world);
    if !source.join("level.dat").exists() {
        return Err(Error::msg("world_not_found"));
    }
    let target = dir.join("world");
    if target.exists() {
        // Прежний мир сервера не теряем
        let backup = dir.join(format!("world-{}", chrono::Local::now().format("%Y-%m-%d_%H-%M-%S")));
        std::fs::rename(&target, backup)?;
    }
    copy_tree(&source, &target)?;
    write_properties(dir, &HashMap::from([("level-name".to_owned(), "world".to_owned())]))
}

fn copy_tree(from: &Path, to: &Path) -> Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)?.flatten() {
        let target = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_tree(&entry.path(), &target)?;
        } else if entry.file_name() != "session.lock" {
            std::fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

// ---------- моды ----------

/// Мод только для клиента по его описанию в jar: Fabric/Quilt `environment: client`,
/// Forge/NeoForge `clientSideOnly=true`.
fn client_only_by_manifest(jar: &Path) -> bool {
    let Ok(file) = std::fs::File::open(jar) else { return false };
    let Ok(mut zip) = zip::ZipArchive::new(file) else { return false };
    let mut read = |name: &str| -> Option<String> {
        let mut entry = zip.by_name(name).ok()?;
        let mut text = String::new();
        std::io::Read::read_to_string(&mut entry, &mut text).ok()?;
        Some(text)
    };
    if let Some(json) = read("fabric.mod.json").and_then(|t| serde_json::from_str::<Value>(&t).ok()) {
        return json["environment"] == "client";
    }
    if let Some(json) = read("quilt.mod.json").and_then(|t| serde_json::from_str::<Value>(&t).ok()) {
        return json["minecraft"]["environment"] == "client";
    }
    for toml in ["META-INF/neoforge.mods.toml", "META-INF/mods.toml"] {
        if let Some(text) = read(toml) {
            return text.lines().any(|l| {
                let l = l.split('#').next().unwrap_or_default().replace(' ', "");
                l == "clientSideOnly=true"
            });
        }
    }
    false
}

/// Кладёт на сервер моды сборки, кроме клиентских. Моды, которые сервер поставил сам
/// (не из сборки), не трогает.
async fn sync_mods(state: &AppState, id: &str, dir: &Path, config: &mut ServerConfig) -> Result<ModsResult> {
    let source = state.paths.game_dir(id)?.join("mods");
    let target = dir.join("mods");
    tokio::fs::create_dir_all(&target).await?;
    for old in config.copied_mods.drain(..) {
        let _ = std::fs::remove_file(target.join(old));
    }

    let jars: Vec<PathBuf> = std::fs::read_dir(&source)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("jar")))
        .collect();

    // Modrinth знает, нужен ли мод на сервере, — спрашиваем по хешам
    let hashes: HashMap<String, PathBuf> = jars
        .iter()
        .filter_map(|p| Some((hex::encode(<sha1::Sha1 as sha1::Digest>::digest(std::fs::read(p).ok()?)), p.clone())))
        .collect();
    let mut client_only: BTreeSet<PathBuf> = BTreeSet::new();
    if let Ok(versions) = crate::content::modrinth::versions_by_hash(&state.http, &hashes.keys().cloned().collect::<Vec<_>>()).await {
        let ids: Vec<String> = versions.values().map(|v| v.project_id.clone()).collect::<BTreeSet<_>>().into_iter().collect();
        let projects = crate::content::modrinth::projects(&state.http, &ids).await.unwrap_or_default();
        let unsupported: BTreeSet<&str> =
            projects.iter().filter(|p| p.server_side.as_deref() == Some("unsupported")).map(|p| p.id.as_str()).collect();
        for (hash, version) in &versions {
            if unsupported.contains(version.project_id.as_str()) {
                client_only.extend(hashes.get(hash).cloned());
            }
        }
    }

    let mut skipped = vec![];
    for jar in jars {
        let name = jar.file_name().expect("file").to_string_lossy().into_owned();
        if client_only.contains(&jar) || client_only_by_manifest(&jar) {
            skipped.push(name);
            continue;
        }
        std::fs::copy(&jar, target.join(&name))?;
        config.copied_mods.push(name);
    }

    // Конфиги модов: кладём только те, которых на сервере ещё нет
    let config_src = state.paths.game_dir(id)?.join("config");
    if config_src.is_dir() {
        copy_missing(&config_src, &dir.join("config"))?;
    }
    skipped.sort();
    Ok(ModsResult { copied: config.copied_mods.len(), skipped })
}

/// Папки модпака, которые нужны серверу: скрипты, датапаки и конфиги по умолчанию.
const PACK_FOLDERS: &[&str] = &["defaultconfigs", "kubejs", "scripts", "global_packs", "datapacks", "openloader", "paxi"];

fn copy_pack_folders(game: &Path, dir: &Path) -> Result<()> {
    for name in PACK_FOLDERS {
        let from = game.join(name);
        if from.is_dir() {
            copy_missing(&from, &dir.join(name))?;
        }
    }
    Ok(())
}

fn copy_missing(from: &Path, to: &Path) -> Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)?.flatten() {
        let target = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_missing(&entry.path(), &target)?;
        } else if !target.exists() {
            std::fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

#[tauri::command]
pub async fn server_sync_mods(state: State<'_, AppState>, id: String) -> Result<ModsResult> {
    if is_running(&id).await {
        return Err(Error::msg("server_running"));
    }
    let dir = server_dir(&state, &id)?;
    let mut config = load_config(&dir).ok_or_else(|| Error::msg("server_missing"))?;
    let source = config.source.clone().ok_or_else(|| Error::msg("server_no_source"))?;
    let result = sync_mods(&state, &source, &dir, &mut config).await?;
    save_config(&dir, &config)?;
    Ok(result)
}

// ---------- настройки ----------

#[tauri::command]
pub fn server_properties(state: State<'_, AppState>, id: String) -> Result<Vec<(String, String)>> {
    Ok(read_properties(&server_dir(&state, &id)?))
}

#[tauri::command]
pub async fn server_set_properties(state: State<'_, AppState>, id: String, values: HashMap<String, String>) -> Result<()> {
    if values.iter().any(|(k, v)| k.contains(['=', '\n', '#']) || k.trim().is_empty() || v.contains('\n')) {
        return Err(Error::msg("server_property_invalid"));
    }
    if let Some(port) = values.get("server-port") {
        if port.parse::<u16>().map_or(true, |p| p < 1024) {
            return Err(Error::msg("server_port_invalid"));
        }
    }
    write_properties(&server_dir(&state, &id)?, &values)
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigUpdate {
    pub memory_mb: u32,
    pub jvm_args: String,
    pub share: bool,
    #[serde(default)]
    pub auto_restart: Option<bool>,
    #[serde(default)]
    pub autostart: Option<bool>,
    #[serde(default)]
    pub restart_hours: Option<u32>,
    #[serde(default)]
    pub backup_on_start: Option<bool>,
}

#[tauri::command]
pub fn server_set_config(state: State<'_, AppState>, id: String, update: ConfigUpdate) -> Result<ServerConfig> {
    if update.memory_mb < 512 {
        return Err(Error::msg("memory must be at least 512 MB"));
    }
    let dir = server_dir(&state, &id)?;
    let mut config = load_config(&dir).ok_or_else(|| Error::msg("server_missing"))?;
    config.memory_mb = update.memory_mb;
    config.jvm_args = update.jvm_args.trim().to_owned();
    config.share = update.share;
    if let Some(v) = update.auto_restart {
        config.auto_restart = v;
    }
    if let Some(v) = update.autostart {
        config.autostart = v;
    }
    if let Some(v) = update.backup_on_start {
        config.backup_on_start = v;
    }
    if let Some(v) = update.restart_hours {
        config.restart_hours = v.min(168);
    }
    save_config(&dir, &config)?;
    Ok(config)
}

#[tauri::command]
pub async fn server_delete(state: State<'_, AppState>, id: String, progress: Channel<crate::progress::Progress>) -> Result<()> {
    if is_running(&id).await {
        return Err(Error::msg("server_running"));
    }
    let dir = server_dir(&state, &id)?;
    tokio::task::spawn_blocking(move || crate::progress::remove_dir(&dir, progress))
        .await
        .map_err(|e| Error::msg(e.to_string()))??;
    LOGS.lock().expect("lock").remove(&id);
    Ok(())
}

// ---------- запуск ----------

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct LogEvent {
    instance_id: String,
    line: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct StatusEvent {
    instance_id: String,
    status: Status,
    code: Option<i32>,
}

fn push_log(id: &str, line: String) {
    let mut logs = LOGS.lock().expect("lock");
    let log = logs.entry(id.to_owned()).or_default();
    if log.len() >= LOG_LINES {
        log.pop_front();
    }
    log.push_back(line);
}

async fn set_status(app: &AppHandle, id: &str, status: Status) {
    if let Some(live) = LIVE.lock().await.get_mut(id) {
        live.status = status;
    }
    let _ = app.emit("server-status", StatusEvent { instance_id: id.to_owned(), status, code: None });
}

/// Ник из строки «[Server thread/INFO]: Steve joined the game».
fn player_event(line: &str) -> Option<(bool, String)> {
    let text = line.rsplit_once("]: ").map_or(line, |(_, t)| t).trim();
    if let Some(name) = text.strip_suffix(" joined the game") {
        return Some((true, name.to_owned()));
    }
    text.strip_suffix(" left the game").map(|name| (false, name.to_owned()))
}

#[tauri::command]
pub async fn server_start(app: AppHandle, state: State<'_, AppState>, id: String) -> Result<()> {
    let dir = server_dir(&state, &id)?;
    let config = load_config(&dir).ok_or_else(|| Error::msg("server_missing"))?;
    if LIVE.lock().await.contains_key(&id) {
        return Err(Error::msg("server_running"));
    }
    let port = port_of(&dir);
    if std::net::TcpListener::bind(("0.0.0.0", port)).is_err() {
        return Err(Error::msg(format!("server_port_busy:{port}")));
    }
    let java = crate::game::java::ensure(&state.http, &state.paths.runtimes(), &config.java_component, |_, _| {}).await?;
    if config.backup_on_start {
        let dir = dir.clone();
        let made = tokio::task::spawn_blocking(move || crate::server_backup::make(&dir, Some(crate::server_backup::KEEP))).await;
        if let Ok(Err(e)) = made {
            // Мира ещё нет при первом запуске — это не ошибка
            if !e.to_string().contains("world_not_found") {
                push_log(&id, format!("[Shard] backup failed: {e}"));
            }
        }
    }

    let mut command = tokio::process::Command::new(&java);
    command
        .arg(format!("-Xmx{}M", config.memory_mb))
        .arg(format!("-Xms{}M", config.memory_mb.min(1024)))
        .args(config.jvm_args.split_whitespace())
        .args(&config.launch)
        .current_dir(&dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    if !java.file_name().is_some_and(|n| n.eq_ignore_ascii_case("javaw.exe")) {
        command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    let mut child = command.spawn()?;
    let stdin = child.stdin.take().expect("piped");
    let (kill_tx, kill_rx) = oneshot::channel();
    let run = RUNS.fetch_add(1, Ordering::Relaxed);
    {
        // После перезапуска старый лог остаётся — видно, почему сервер упал
        let mut logs = LOGS.lock().expect("lock");
        let log = logs.entry(id.clone()).or_default();
        if !log.is_empty() {
            log.push_back("[Shard] ----------".to_owned());
        }
    }
    LIVE.lock().await.insert(
        id.clone(),
        Live {
            stdin,
            status: Status::Starting,
            players: BTreeSet::new(),
            share: None,
            tunnel: None,
            kill: Some(kill_tx),
            run,
            pid: child.id(),
            started: Instant::now(),
            restart: false,
        },
    );
    if config.restart_hours > 0 {
        let (app, id) = (app.clone(), id.clone());
        let hours = config.restart_hours;
        tauri::async_runtime::spawn(async move {
            tokio::time::sleep(Duration::from_secs(u64::from(hours) * 3600 - 60)).await;
            if !is_run(&id, run).await {
                return;
            }
            let _ = server_command(id.clone(), "say Server restarts in 1 minute".to_owned()).await;
            tokio::time::sleep(Duration::from_secs(60)).await;
            if !is_run(&id, run).await {
                return;
            }
            if let Some(live) = LIVE.lock().await.get_mut(&id) {
                live.restart = true;
            }
            push_log(&id, "[Shard] scheduled restart".to_owned());
            let _ = server_stop(app, id, None).await;
        });
    }
    let _ = app.emit("server-status", StatusEvent { instance_id: id.clone(), status: Status::Starting, code: None });

    for (stream, stderr) in [(child.stdout.take().map(|s| Box::new(s) as Box<dyn tokio::io::AsyncRead + Unpin + Send>), false), (child.stderr.take().map(|s| Box::new(s) as _), true)] {
        let Some(stream) = stream else { continue };
        let (app, id) = (app.clone(), id.clone());
        tauri::async_runtime::spawn(async move {
            let mut lines = BufReader::new(stream).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                if !stderr && line.contains("]: Done (") {
                    set_status(&app, &id, Status::Running).await;
                }
                if let Some((joined, name)) = player_event(&line) {
                    if let Some(live) = LIVE.lock().await.get_mut(&id) {
                        if joined { live.players.insert(name) } else { live.players.remove(&name) };
                    }
                    let _ = app.emit("server-players", id.clone());
                }
                push_log(&id, line.clone());
                let _ = app.emit("server-log", LogEvent { instance_id: id.clone(), line });
            }
        });
    }

    // Адреса для друзей: UPnP и внешний IP — пока сервер грузится
    if config.share {
        let (app, id, http) = (app.clone(), id.clone(), state.http.clone());
        tauri::async_runtime::spawn(async move {
            let share = share::open(&http, port).await;
            if let Some(live) = LIVE.lock().await.get_mut(&id) {
                live.share = Some(share);
            }
            let _ = app.emit("server-players", id);
        });
    }

    if config.tunnel {
        spawn_tunnel(app.clone(), id.clone(), port);
    }

    let id_done = id.clone();
    tauri::async_runtime::spawn(async move {
        let code = tokio::select! {
            status = child.wait() => status.ok().and_then(|s| s.code()),
            _ = kill_rx => {
                let _ = child.kill().await;
                None
            }
        };
        let (live, others) = {
            let mut all = LIVE.lock().await;
            let live = all.remove(&id_done);
            (live, all.values().any(|l| l.tunnel.is_some()))
        };
        if let Some(l) = &live {
            // Упал сам (не «stop» и не кнопка) и проработал хотя бы минуту — иначе это цикл падений
            let crashed = l.status != Status::Stopping && code != Some(0) && l.kill.is_some();
            let auto = crashed && l.started.elapsed() > Duration::from_secs(60) && {
                let state = tauri::Manager::state::<AppState>(&app);
                server_dir(&state, &id_done).ok().and_then(|d| load_config(&d)).is_some_and(|c| c.auto_restart)
            };
            if l.restart || auto {
                push_log(&id_done, format!("[Shard] restarting{}", if auto { " after crash" } else { "" }));
                request_start(id_done.clone(), Duration::from_secs(5));
            }
        }
        if live.as_ref().is_some_and(|l| l.tunnel.is_some()) && !others {
            crate::tunnel::close().await;
        }
        if let Some(mapped) = live.and_then(|l| l.share).filter(|s| s.upnp) {
            share::close(mapped.port).await;
        }
        let _ = app.emit("server-status", StatusEvent { instance_id: id_done, status: Status::Stopped, code });
    });
    Ok(())
}

#[tauri::command]
pub async fn server_command(id: String, command: String) -> Result<()> {
    let command = command.trim();
    if command.is_empty() {
        return Ok(());
    }
    let mut live = LIVE.lock().await;
    let server = live.get_mut(&id).ok_or_else(|| Error::msg("server_not_running"))?;
    server.stdin.write_all(format!("{}\n", command.trim_start_matches('/')).as_bytes()).await?;
    server.stdin.flush().await?;
    push_log(&id, format!("> {command}"));
    Ok(())
}

/// Мягкая остановка командой stop; если сервер не закрылся за минуту — завершаем процесс.
#[tauri::command]
pub async fn server_stop(app: AppHandle, id: String, force: Option<bool>) -> Result<()> {
    let mut live = LIVE.lock().await;
    let server = live.get_mut(&id).ok_or_else(|| Error::msg("server_not_running"))?;
    if force.unwrap_or(false) {
        if let Some(kill) = server.kill.take() {
            let _ = kill.send(());
        }
        return Ok(());
    }
    server.status = Status::Stopping;
    let run = server.run;
    let _ = server.stdin.write_all(b"stop\n").await;
    let _ = server.stdin.flush().await;
    drop(live);
    let _ = app.emit("server-status", StatusEvent { instance_id: id.clone(), status: Status::Stopping, code: None });
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_secs(60)).await;
        if let Some(kill) = LIVE.lock().await.get_mut(&id).filter(|l| l.run == run).and_then(|l| l.kill.take()) {
            let _ = kill.send(());
        }
    });
    Ok(())
}

/// Открывает туннель в фоне, пока сервер загружается; адрес появится в server_info.
fn spawn_tunnel(app: AppHandle, id: String, port: u16) {
    tauri::async_runtime::spawn(async move {
        let state = tauri::Manager::state::<AppState>(&app);
        let result = crate::tunnel::open(&state, port).await.map_err(|e| e.to_string());
        if let Some(live) = LIVE.lock().await.get_mut(&id) {
            live.tunnel = Some(result);
        }
        let _ = app.emit("server-players", id);
    });
}

/// Включает или выключает туннель playit.gg для сервера; у запущенного — сразу.
#[tauri::command]
pub async fn server_set_tunnel(app: AppHandle, state: State<'_, AppState>, id: String, on: bool) -> Result<ServerConfig> {
    let dir = server_dir(&state, &id)?;
    let mut config = load_config(&dir).ok_or_else(|| Error::msg("server_missing"))?;
    config.tunnel = on;
    save_config(&dir, &config)?;
    if is_running(&id).await {
        if on {
            spawn_tunnel(app, id, port_of(&dir));
        } else {
            let others = {
                let mut all = LIVE.lock().await;
                if let Some(live) = all.get_mut(&id) {
                    live.tunnel = None;
                }
                all.values().any(|l| l.tunnel.is_some())
            };
            if !others {
                crate::tunnel::close().await;
            }
            let _ = app.emit("server-players", id);
        }
    }
    Ok(config)
}

/// Повторно открыть доступ из интернета (например, после смены роутера или IP).
#[tauri::command]
pub async fn server_reshare(app: AppHandle, state: State<'_, AppState>, id: String) -> Result<Share> {
    let port = port_of(&server_dir(&state, &id)?);
    if !is_running(&id).await {
        return Err(Error::msg("server_not_running"));
    }
    let share = share::open(&state.http, port).await;
    if let Some(live) = LIVE.lock().await.get_mut(&id) {
        live.share = Some(share.clone());
    }
    let _ = app.emit("server-players", id);
    Ok(share)
}

/// Останавливает все серверы при выходе из лаунчера, чтобы мир сохранился.
pub async fn stop_all() {
    let mut live = LIVE.lock().await;
    for server in live.values_mut() {
        let _ = server.stdin.write_all(b"stop\n").await;
        let _ = server.stdin.flush().await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_player_events() {
        assert_eq!(player_event("[12:00:01] [Server thread/INFO]: Steve joined the game"), Some((true, "Steve".into())));
        assert_eq!(player_event("[12:00:09] [Server thread/INFO]: Alex left the game"), Some((false, "Alex".into())));
        assert_eq!(player_event("[12:00:09] [Server thread/INFO]: Done (3.2s)!"), None);
    }

    #[test]
    fn keeps_properties_layout() {
        let dir = std::env::temp_dir().join(format!("shard-props-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("server.properties"), "#Minecraft server properties\nmotd=Old\nserver-port=25565\n").unwrap();
        write_properties(&dir, &HashMap::from([("motd".into(), "New".into()), ("pvp".into(), "false".into())])).unwrap();
        let text = std::fs::read_to_string(dir.join("server.properties")).unwrap();
        assert_eq!(text, "#Minecraft server properties\nmotd=New\nserver-port=25565\npvp=false\n");
        assert_eq!(port_of(&dir), 25565);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn finds_forge_launch_args() {
        let dir = std::env::temp_dir().join(format!("shard-forge-{}", uuid::Uuid::new_v4()));
        let args = dir.join("libraries/net/neoforged/neoforge/21.1.77");
        std::fs::create_dir_all(&args).unwrap();
        let name = if cfg!(windows) { "win_args.txt" } else { "unix_args.txt" };
        std::fs::write(args.join(name), "-cp x").unwrap();
        assert_eq!(
            forge_launch(&dir).unwrap(),
            vec![format!("@libraries/net/neoforged/neoforge/21.1.77/{name}"), "nogui".to_owned()]
        );
        std::fs::remove_dir_all(dir).unwrap();
    }
}
