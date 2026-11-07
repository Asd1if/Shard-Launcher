//! Сборки: у каждой своя папка с instance.json и папкой игры.

use serde::{Deserialize, Serialize};
use tauri::State;
use tauri_plugin_opener::OpenerExt;

use crate::error::{Error, Result};
use crate::paths::Paths;
use crate::state::AppState;
use crate::versions::VersionType;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ModLoader {
    Fabric,
    Quilt,
    Forge,
    Neoforge,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Instance {
    pub id: String,
    pub name: String,
    /// Ядро всегда ванильное — это версия Minecraft от Mojang
    pub mc_version: String,
    pub version_type: VersionType,
    /// None — чистая ванилла без загрузчика модов
    pub loader: Option<ModLoader>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub loader_version: Option<String>,
    #[serde(default)]
    pub mod_count: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_played: Option<String>,
    pub created_at: String,
    /// Максимум памяти для игры (-Xmx), МБ
    #[serde(default = "default_memory")]
    pub memory_mb: u32,
    #[serde(default)]
    pub settings: InstanceSettings,
    /// `preset:<имя>` из встроенного набора или небольшая картинка data URL; None — узор из названия
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    /// Папка в списке сборок; None — без группы
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub favorite: bool,
    /// Модпак, из которого поставлена сборка
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pack: Option<PackOrigin>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PackOrigin {
    pub source: crate::content::Source,
    pub project_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version_name: Option<String>,
    /// Иконка модпака — её можно вернуть после смены иконки сборки
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon_url: Option<String>,
}

fn default_memory() -> u32 {
    2048
}

/// Тонкие настройки запуска. Пустое значение — поведение по умолчанию.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct InstanceSettings {
    /// Своя Java вместо автоматической от Mojang
    pub java_path: Option<String>,
    /// Начальная память (-Xms), МБ
    pub min_memory_mb: Option<u32>,
    /// Дополнительные аргументы JVM, через пробел; кавычки группируют
    pub jvm_args: String,
    /// Дополнительные аргументы игры
    pub game_args: String,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub fullscreen: bool,
    /// Переменные окружения, по одной KEY=VALUE на строку
    pub env: String,
    /// Команда перед запуском; если она завершится с ошибкой, игра не запустится
    pub pre_launch: String,
    /// Команда после выхода из игры
    pub post_exit: String,
    /// Адрес сервера для кнопки «Зайти на сервер»: `host` или `host:port`
    pub server: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstanceUpdate {
    pub name: String,
    pub memory_mb: u32,
    pub settings: InstanceSettings,
    /// Смена версии загрузчика; для ванильной сборки игнорируется
    #[serde(default)]
    pub loader_version: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewInstance {
    pub name: String,
    pub mc_version: String,
    pub version_type: VersionType,
    pub loader: Option<ModLoader>,
    /// None — рекомендованная версия, определится при первом запуске
    #[serde(default)]
    pub loader_version: Option<String>,
}

pub fn load(paths: &Paths, id: &str) -> Result<Instance> {
    let file = paths.instance_dir(id)?.join("instance.json");
    Ok(serde_json::from_slice(&std::fs::read(file)?)?)
}

pub fn save(paths: &Paths, instance: &Instance) -> Result<()> {
    let dir = paths.instance_dir(&instance.id)?;
    std::fs::create_dir_all(&dir)?;
    std::fs::write(dir.join("instance.json"), serde_json::to_vec_pretty(instance)?)?;
    Ok(())
}

pub fn list(paths: &Paths) -> Result<Vec<Instance>> {
    let dir = paths.instances();
    if !dir.exists() {
        return Ok(vec![]);
    }
    let mut out = vec![];
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        // Битую или чужую папку пропускаем, а не роняем весь список
        let read = std::fs::read(entry.path().join("instance.json")).ok();
        if let Some(instance) = read.and_then(|b| serde_json::from_slice::<Instance>(&b).ok()) {
            out.push(instance);
        }
    }
    out.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    Ok(out)
}

#[tauri::command]
pub fn instances_list(state: State<'_, AppState>) -> Result<Vec<Instance>> {
    list(&state.paths)
}

#[tauri::command]
pub fn instance_create(state: State<'_, AppState>, new: NewInstance) -> Result<Instance> {
    let name = new.name.trim();
    if name.is_empty() {
        return Err(Error::msg("instance name is empty"));
    }
    let instance = Instance {
        id: uuid::Uuid::new_v4().to_string(),
        name: name.to_owned(),
        mc_version: new.mc_version,
        version_type: new.version_type,
        loader: new.loader,
        loader_version: new.loader.and(new.loader_version),
        mod_count: 0,
        last_played: None,
        created_at: chrono::Utc::now().to_rfc3339(),
        memory_mb: crate::settings::new_instance_memory(&state.paths, new.loader.is_some()),
        settings: InstanceSettings::default(),
        icon: None,
        group: None,
        favorite: false,
        pack: None,
    };
    state.paths.new_instance_dir(&instance.id, &instance.name);
    save(&state.paths, &instance)?;
    std::fs::create_dir_all(state.paths.game_dir(&instance.id)?)?;
    Ok(instance)
}

#[tauri::command]
pub fn instance_update(state: State<'_, AppState>, id: String, update: InstanceUpdate) -> Result<Instance> {
    let mut instance = load(&state.paths, &id)?;
    let name = update.name.trim();
    if name.is_empty() {
        return Err(Error::msg("instance name is empty"));
    }
    if update.memory_mb < 512 {
        return Err(Error::msg("memory must be at least 512 MB"));
    }
    let mut settings = update.settings;
    // Пустой путь к Java — значит «автоматически»
    settings.java_path = settings.java_path.map(|p| p.trim().to_owned()).filter(|p| !p.is_empty());
    settings.server = settings.server.trim().to_owned();
    if settings.server.chars().any(char::is_whitespace) {
        return Err(Error::msg("server_invalid"));
    }
    if let Some(min) = settings.min_memory_mb {
        if min > update.memory_mb {
            return Err(Error::msg("minimum memory is larger than maximum"));
        }
    }
    if instance.name != name {
        // Папку запущенной игры не трогаем: Windows не даст переименовать открытые файлы
        let running = state.running.try_lock().map_or(true, |r| r.contains_key(&id));
        if !running {
            state.paths.rename_instance_dir(&id, name);
        }
    }
    instance.name = name.to_owned();
    instance.memory_mb = update.memory_mb;
    instance.settings = settings;
    if instance.loader.is_some() && update.loader_version.is_some() {
        instance.loader_version = update.loader_version;
    }
    save(&state.paths, &instance)?;
    Ok(instance)
}

/// Иконка в data URL. Обычные картинки фронтенд уменьшает сам, а анимированные GIF и WebP
/// модпаков приходят как есть — они бывают и по полмегабайта.
pub(crate) const MAX_ICON: usize = 4 * 1024 * 1024;

pub fn check_icon(icon: &str) -> Result<()> {
    let preset = icon
        .strip_prefix("preset:")
        .is_some_and(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_lowercase() || c == '-'));
    let image = ["png", "jpeg", "webp", "gif"].iter().any(|t| icon.starts_with(&format!("data:image/{t};base64,")));
    if !(preset || image) || icon.len() > MAX_ICON {
        return Err(Error::msg("invalid instance icon"));
    }
    Ok(())
}

#[tauri::command]
pub fn instance_set_icon(state: State<'_, AppState>, id: String, icon: Option<String>) -> Result<Instance> {
    if let Some(icon) = &icon {
        check_icon(icon)?;
    }
    let mut instance = load(&state.paths, &id)?;
    instance.icon = icon;
    save(&state.paths, &instance)?;
    Ok(instance)
}

/// Группа и избранное — то, что меняется прямо из списка сборок.
#[tauri::command]
pub fn instance_set_meta(state: State<'_, AppState>, id: String, group: Option<String>, favorite: bool) -> Result<Instance> {
    let mut instance = load(&state.paths, &id)?;
    instance.group = group.map(|g| g.trim().chars().take(40).collect::<String>()).filter(|g| !g.is_empty());
    instance.favorite = favorite;
    save(&state.paths, &instance)?;
    Ok(instance)
}

/// Копия сборки со всеми модами, настройками и мирами под новым именем.
#[tauri::command]
pub async fn instance_clone(
    state: State<'_, AppState>,
    id: String,
    name: String,
    progress: tauri::ipc::Channel<crate::progress::Progress>,
) -> Result<Instance> {
    let source = load(&state.paths, &id)?;
    let name = name.trim();
    if name.is_empty() {
        return Err(Error::msg("instance name is empty"));
    }
    let mut copy = source.clone();
    copy.id = uuid::Uuid::new_v4().to_string();
    copy.name = name.to_owned();
    copy.created_at = chrono::Utc::now().to_rfc3339();
    copy.last_played = None;
    copy.favorite = false;
    let (from, to) = (state.paths.instance_dir(&id)?, state.paths.new_instance_dir(&copy.id, &copy.name));
    let result = tokio::task::spawn_blocking(move || {
        let skipped = crate::progress::size_of(&from.join("backups"));
        let mut tracker = crate::progress::Tracker::new(progress, crate::progress::size_of(&from).saturating_sub(skipped));
        copy_dir(&from, &to, &mut tracker)
    })
        .await
        .map_err(|e| Error::msg(e.to_string()))?;
    if let Err(e) = result.and_then(|_| save(&state.paths, &copy)) {
        let _ = std::fs::remove_dir_all(state.paths.instance_dir(&copy.id)?);
        return Err(e);
    }
    Ok(copy)
}

fn copy_dir(from: &std::path::Path, to: &std::path::Path, tracker: &mut crate::progress::Tracker) -> Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let name = entry.file_name();
        // Резервные копии миров и логи в копию не нужны
        if name == "backups" {
            continue;
        }
        let target = to.join(&name);
        if entry.file_type()?.is_dir() {
            copy_dir(&entry.path(), &target, tracker)?;
        } else if name != "session.lock" {
            tracker.add(std::fs::copy(entry.path(), target)?);
        }
    }
    Ok(())
}

#[tauri::command]
pub async fn instance_delete(state: State<'_, AppState>, id: String, progress: tauri::ipc::Channel<crate::progress::Progress>) -> Result<()> {
    if state.running.lock().await.contains_key(&id) {
        return Err(Error::msg("stop the game before deleting the instance"));
    }
    let dir = state.paths.instance_dir(&id)?;
    tokio::task::spawn_blocking(move || crate::progress::remove_dir(&dir, progress))
        .await
        .map_err(|e| Error::msg(e.to_string()))??;
    Ok(())
}

#[tauri::command]
pub fn instance_open_dir(app: tauri::AppHandle, state: State<'_, AppState>, id: String) -> Result<()> {
    let dir = state.paths.game_dir(&id)?;
    std::fs::create_dir_all(&dir)?;
    app.opener()
        .open_path(dir.to_string_lossy(), None::<&str>)
        .map_err(|e| Error::msg(e.to_string()))
}
