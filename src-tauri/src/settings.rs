//! Общие настройки лаунчера: значения по умолчанию для сборок, поведение при запуске игры
//! и расположение папки с данными.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, State};

use tauri::ipc::Channel;

use crate::error::{Error, Result};
use crate::paths::Paths;
use crate::progress::{size_of, Progress, Tracker};
use crate::state::AppState;

/// Что делает окно лаунчера, когда игра запустилась.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OnLaunch {
    #[default]
    Keep,
    /// Свернуться и развернуться обратно, когда игра закроется
    Minimize,
    /// Закрыть лаунчер; игра продолжит работать
    Close,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    /// Память для новых сборок, МБ; None — 2 ГБ для ванили и 4 ГБ с модами
    pub default_memory_mb: Option<u32>,
    /// Java для сборок, у которых своя не указана; None — автоматически от Mojang
    pub default_java_path: Option<String>,
    /// Аргументы JVM для всех сборок; аргументы сборки идут после и побеждают
    pub default_jvm_args: String,
    pub on_launch: OnLaunch,
    /// Не показывать в Discord, во что идёт игра
    pub discord_disabled: bool,
    /// Не показывать новости Minecraft на главной
    pub hide_news: bool,
    /// Свой ключ CurseForge API; пусто — вшитый в лаунчер, если он есть
    pub curseforge_key: Option<String>,
    /// Первый запуск пройден
    pub onboarded: bool,
    /// Принятая версия условий использования; 0 — не принимались
    pub terms_version: u32,
    /// Копировать миры, в которые играли, после выхода из игры
    pub auto_backup: bool,
    /// Сколько последних копий мира хранить; 0 — 5
    pub backup_keep: u32,
}

/// Настройки из settings.json. Ключа CurseForge здесь нет — он в хранилище учётных данных, см. `load_full`.
pub fn load(paths: &Paths) -> Settings {
    std::fs::read(paths.root.join("settings.json"))
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

// Ключ CurseForge хранится в системном хранилище учётных данных, а не открытым текстом в settings.json
fn cf_entry() -> Option<keyring::Entry> {
    keyring::Entry::new(crate::auth::KEYRING_SERVICE, "curseforge-key").ok()
}

fn cf_key_read() -> Option<String> {
    cf_entry()?.get_password().ok().filter(|k| !k.is_empty())
}

/// Сохраняет ключ в хранилище. false — хранилище недоступно, ключ остаётся в файле.
fn cf_key_write(key: Option<&str>) -> bool {
    let Some(entry) = cf_entry() else { return false };
    match key {
        Some(k) => entry.set_password(k).is_ok(),
        None => matches!(entry.delete_credential(), Ok(()) | Err(keyring::Error::NoEntry)),
    }
}

fn save_file(paths: &Paths, settings: &Settings) -> Result<()> {
    let mut on_disk = settings.clone();
    if cf_key_write(settings.curseforge_key.as_deref()) {
        on_disk.curseforge_key = None;
    }
    std::fs::write(paths.root.join("settings.json"), serde_json::to_vec_pretty(&on_disk)?)?;
    Ok(())
}

/// Настройки вместе с ключом CurseForge. Старый ключ из settings.json переносится в хранилище.
pub fn load_full(paths: &Paths) -> Settings {
    let mut settings = load(paths);
    if settings.curseforge_key.is_some() {
        let _ = save_file(paths, &settings);
    } else {
        settings.curseforge_key = cf_key_read();
    }
    settings
}

/// Память новой сборки: из общих настроек, иначе 4 ГБ с загрузчиком модов и 2 ГБ без.
pub fn new_instance_memory(paths: &Paths, modded: bool) -> u32 {
    load(paths).default_memory_mb.unwrap_or(if modded { 4096 } else { 2048 })
}

#[tauri::command]
pub async fn settings_get(state: State<'_, AppState>) -> Result<Settings> {
    Ok(load_full(&state.paths))
}

#[tauri::command]
pub async fn settings_set(state: State<'_, AppState>, settings: Settings) -> Result<Settings> {
    let mut settings = settings;
    if settings.default_memory_mb.is_some_and(|m| m < 512) {
        return Err(Error::msg("memory must be at least 512 MB"));
    }
    settings.default_java_path = settings.default_java_path.map(|p| p.trim().to_owned()).filter(|p| !p.is_empty());
    settings.curseforge_key = settings.curseforge_key.map(|k| k.trim().to_owned()).filter(|k| !k.is_empty());
    crate::content::curseforge::set_user_key(settings.curseforge_key.clone());
    save_file(&state.paths, &settings)?;
    tauri::async_runtime::spawn(crate::discord::set_disabled(settings.discord_disabled));
    Ok(settings)
}

#[tauri::command]
pub async fn curseforge_check_key(state: State<'_, AppState>, key: String) -> Result<bool> {
    crate::content::curseforge::check_key(&state.http, &key).await
}

// ---------- своя картинка фона ----------

fn backgrounds_dir(paths: &Paths) -> PathBuf {
    paths.root.join("backgrounds")
}

/// Текущая своя картинка фона главного экрана.
#[tauri::command]
pub fn background_get(state: State<'_, AppState>) -> Option<String> {
    std::fs::read_dir(backgrounds_dir(&state.paths))
        .ok()?
        .flatten()
        .find(|e| e.path().is_file())
        .map(|e| e.path().to_string_lossy().into_owned())
}

/// Копирует картинку в папку лаунчера (исходник можно удалить). None — убрать свою картинку.
#[tauri::command]
pub fn background_set(state: State<'_, AppState>, path: Option<String>) -> Result<Option<String>> {
    let dir = backgrounds_dir(&state.paths);
    let Some(path) = path else {
        let _ = std::fs::remove_dir_all(&dir);
        return Ok(None);
    };
    let source = PathBuf::from(path);
    let ext = source.extension().map(|e| e.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
    if !["png", "jpg", "jpeg", "webp", "gif"].contains(&ext.as_str()) {
        return Err(Error::msg("icon_not_image"));
    }
    if std::fs::metadata(&source)?.len() > 25 * 1024 * 1024 {
        return Err(Error::msg("background_too_large"));
    }
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir)?;
    // Новое имя каждый раз — иначе WebView покажет старую картинку из кеша
    let target = dir.join(format!("{}.{ext}", uuid::Uuid::new_v4()));
    std::fs::copy(&source, &target)?;
    Ok(Some(target.to_string_lossy().into_owned()))
}

/// Средняя яркость картинки фона, 0 — чёрная, 1 — белая. По ней подбирается цвет текста.
#[tauri::command]
pub async fn background_luma(path: String) -> Result<f32> {
    tauri::async_runtime::spawn_blocking(move || {
        let img = image::open(&path).map_err(|e| Error::msg(e.to_string()))?.thumbnail(64, 64).to_rgba8();
        let (mut sum, mut weight) = (0f32, 0f32);
        for p in img.pixels() {
            let a = p[3] as f32 / 255.0;
            sum += a * (0.2126 * p[0] as f32 + 0.7152 * p[1] as f32 + 0.0722 * p[2] as f32) / 255.0;
            weight += a;
        }
        Ok(if weight > 0.0 { sum / weight } else { 0.0 })
    })
    .await
    .map_err(|e| Error::msg(e.to_string()))?
}

// ---------- папка с данными ----------

/// Файл с путём к своей папке данных. Лежит в стандартной папке приложения, а не в самих данных:
/// иначе после переноса его не найти.
const LOCATION_FILE: &str = "data-location.txt";

/// Где хранить данные: портативная версия → `data` рядом с exe, иначе своя папка из настроек,
/// иначе стандартная папка приложения.
pub fn data_root(default: &Path) -> PathBuf {
    if let Some(root) = portable_root() {
        return root;
    }
    std::fs::read_to_string(default.join(LOCATION_FILE))
        .ok()
        .map(|s| PathBuf::from(s.trim()))
        .filter(|p| p.is_absolute())
        .unwrap_or_else(|| default.to_path_buf())
}

/// Портативная версия: если рядом с exe лежит `portable.txt`, все данные хранятся в папке `data`
/// рядом с ним, а не в %APPDATA%. Так Shard можно носить на флешке или держать в любой папке.
fn portable_root() -> Option<PathBuf> {
    let dir = std::env::current_exe().ok()?.parent()?.to_path_buf();
    dir.join("portable.txt").exists().then(|| dir.join("data"))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DataDir {
    path: String,
    /// В портативной версии папка всегда рядом с exe и не меняется
    portable: bool,
    is_default: bool,
}

fn default_dir(app: &AppHandle) -> Result<PathBuf> {
    app.path().app_data_dir().map_err(|e| Error::msg(e.to_string()))
}

#[tauri::command]
pub fn settings_data_dir(app: AppHandle, state: State<'_, AppState>) -> Result<DataDir> {
    let default = default_dir(&app)?;
    Ok(DataDir {
        path: state.paths.root.to_string_lossy().into_owned(),
        portable: portable_root().is_some(),
        is_default: state.paths.root == default,
    })
}

/// Переносит данные в другую папку и перезапускает лаунчер. Если там уже есть данные Shard,
/// просто переключается на них, ничего не трогая.
#[tauri::command]
pub async fn settings_move_data(
    app: AppHandle,
    state: State<'_, AppState>,
    path: Option<String>,
    progress: Channel<Progress>,
) -> Result<()> {
    if portable_root().is_some() {
        return Err(Error::msg("data_dir_portable"));
    }
    if !state.running.lock().await.is_empty() {
        return Err(Error::msg("data_dir_game_running"));
    }
    let default = default_dir(&app)?;
    let target = path.map(PathBuf::from).unwrap_or_else(|| default.clone());
    if !target.is_absolute() {
        return Err(Error::msg("data_dir_invalid"));
    }
    let from = state.paths.root.clone();
    if target == from {
        return Ok(());
    }
    if target.starts_with(&from) || from.starts_with(&target) && target != default {
        return Err(Error::msg("data_dir_nested"));
    }

    let existing = target.join("instances").exists() || target.join("accounts.json").exists();
    if !existing {
        let (from, to) = (from.clone(), target.clone());
        tokio::task::spawn_blocking(move || move_contents(&from, &to, progress))
            .await
            .map_err(|e| Error::msg(e.to_string()))??;
    }

    std::fs::create_dir_all(&default)?;
    let location = default.join(LOCATION_FILE);
    if target == default {
        let _ = std::fs::remove_file(location);
    } else {
        std::fs::write(location, target.to_string_lossy().as_bytes())?;
    }
    app.restart();
}

/// Переносит всё содержимое папки. Файл с расположением остаётся на месте.
fn move_contents(from: &Path, to: &Path, progress: Channel<Progress>) -> Result<()> {
    std::fs::create_dir_all(to)?;
    if std::fs::read_dir(to)?.flatten().any(|e| !is_service(&e.file_name().to_string_lossy())) {
        return Err(Error::msg("data_dir_not_empty"));
    }
    let entries: Vec<_> = std::fs::read_dir(from)?
        .flatten()
        .filter(|e| !is_service(&e.file_name().to_string_lossy()))
        .map(|e| (e.path(), size_of(&e.path())))
        .collect();
    let mut tracker = Tracker::new(progress, entries.iter().map(|(_, size)| size).sum());
    for (path, size) in entries {
        let target = to.join(path.file_name().expect("entry has a name"));
        // rename не работает между дисками — тогда копируем и удаляем
        if std::fs::rename(&path, &target).is_ok() {
            tracker.add(size);
        } else {
            copy_recursive(&path, &target, &mut tracker)?;
            remove_any(&path)?;
        }
    }
    Ok(())
}

/// Файлы самого приложения, а не данные лаунчера: расположение, размер окна, кеш WebView2.
fn is_service(name: &str) -> bool {
    name == LOCATION_FILE || name == "EBWebView" || name.starts_with(".window-state")
}

fn copy_recursive(from: &Path, to: &Path, tracker: &mut Tracker) -> Result<()> {
    if from.is_dir() {
        std::fs::create_dir_all(to)?;
        for entry in std::fs::read_dir(from)? {
            let entry = entry?;
            copy_recursive(&entry.path(), &to.join(entry.file_name()), tracker)?;
        }
    } else {
        tracker.add(std::fs::copy(from, to)?);
    }
    Ok(())
}

fn remove_any(path: &Path) -> Result<()> {
    if path.is_dir() { std::fs::remove_dir_all(path)? } else { std::fs::remove_file(path)? }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn moves_folder_contents() {
        let base = std::env::temp_dir().join(format!("shard-move-{}", uuid::Uuid::new_v4()));
        let (from, to) = (base.join("a"), base.join("b"));
        std::fs::create_dir_all(from.join("instances/x")).unwrap();
        std::fs::write(from.join("instances/x/instance.json"), "{}").unwrap();
        std::fs::write(from.join(LOCATION_FILE), "keep").unwrap();
        move_contents(&from, &to, Channel::new(|_| Ok(()))).unwrap();
        assert!(to.join("instances/x/instance.json").exists());
        assert!(from.join(LOCATION_FILE).exists());
        assert!(!to.join(LOCATION_FILE).exists());
        std::fs::remove_dir_all(base).unwrap();
    }
}
