//! Миры и скриншоты сборки: список, резервные копии миров, открытие файлов.

use std::io::Write;
use std::path::{Path, PathBuf};

use serde::Serialize;
use tauri::{AppHandle, State};
use tauri_plugin_opener::OpenerExt;

use crate::content::checked_name;
use crate::error::{Error, Result};
use crate::modpack::safe_join;
use crate::state::AppState;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct World {
    /// Имя папки в saves — по нему мир и находится
    pub folder: String,
    /// Когда в мир последний раз заходили (время level.dat)
    pub last_played: Option<String>,
    pub size: u64,
    /// Путь к icon.png мира, если есть
    pub icon: Option<String>,
    pub backups: u32,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Backup {
    pub file: String,
    pub world: String,
    pub created_at: String,
    pub size: u64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Screenshot {
    pub file: String,
    /// Полный путь — фронтенд показывает картинку через asset-протокол
    pub path: String,
    pub taken_at: String,
    pub size: u64,
}

fn iso(time: std::io::Result<std::time::SystemTime>) -> Option<String> {
    time.ok().map(|t| chrono::DateTime::<chrono::Utc>::from(t).to_rfc3339())
}

pub(crate) fn dir_size(path: &Path) -> u64 {
    let Ok(entries) = std::fs::read_dir(path) else { return 0 };
    entries
        .flatten()
        .map(|e| match e.file_type() {
            Ok(t) if t.is_dir() => dir_size(&e.path()),
            _ => e.metadata().map_or(0, |m| m.len()),
        })
        .sum()
}

fn backups_dir(state: &AppState, id: &str) -> Result<PathBuf> {
    Ok(state.paths.instance_dir(id)?.join("backups"))
}

/// `Мой мир-2024-05-01_12-30-00.zip` → `Мой мир`
fn backup_world(file: &str) -> Option<&str> {
    let stem = file.strip_suffix(".zip")?;
    // Дата в конце имеет фиксированную длину: -YYYY-MM-DD_HH-MM-SS
    stem.len().checked_sub(20).filter(|&i| stem.is_char_boundary(i)).map(|i| &stem[..i])
}

fn list_backups(state: &AppState, id: &str) -> Result<Vec<Backup>> {
    let Ok(entries) = std::fs::read_dir(backups_dir(state, id)?) else { return Ok(vec![]) };
    let mut out: Vec<Backup> = entries
        .flatten()
        .filter_map(|e| {
            let file = e.file_name().to_string_lossy().into_owned();
            let world = backup_world(&file)?.to_owned();
            let meta = e.metadata().ok()?;
            Some(Backup { created_at: iso(meta.modified())?, size: meta.len(), file, world })
        })
        .collect();
    out.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    Ok(out)
}

#[tauri::command]
pub async fn worlds_list(state: State<'_, AppState>, id: String) -> Result<Vec<World>> {
    let saves = state.paths.game_dir(&id)?.join("saves");
    let backups = list_backups(&state, &id)?;
    let worlds = tokio::task::spawn_blocking(move || {
        let Ok(entries) = std::fs::read_dir(&saves) else { return vec![] };
        let mut out: Vec<World> = entries
            .flatten()
            .filter(|e| e.path().join("level.dat").exists())
            .map(|e| {
                let path = e.path();
                let folder = e.file_name().to_string_lossy().into_owned();
                let icon = path.join("icon.png");
                World {
                    backups: backups.iter().filter(|b| b.world == folder).count() as u32,
                    last_played: iso(std::fs::metadata(path.join("level.dat")).and_then(|m| m.modified())),
                    size: dir_size(&path),
                    icon: icon.exists().then(|| icon.to_string_lossy().into_owned()),
                    folder,
                }
            })
            .collect();
        out.sort_by(|a, b| b.last_played.cmp(&a.last_played));
        out
    })
    .await
    .map_err(|e| Error::msg(e.to_string()))?;
    Ok(worlds)
}

#[tauri::command]
pub fn world_backups(state: State<'_, AppState>, id: String) -> Result<Vec<Backup>> {
    list_backups(&state, &id)
}

/// Архивирует папку мира в `<сборка>/backups`. Мир можно копировать и во время игры:
/// файлы, которые игра держит открытыми, Windows всё равно даёт прочитать.
#[tauri::command]
pub async fn world_backup(
    state: State<'_, AppState>,
    id: String,
    world: String,
    progress: tauri::ipc::Channel<crate::progress::Progress>,
) -> Result<Backup> {
    let world = checked_name(&world)?.to_owned();
    let source = state.paths.game_dir(&id)?.join("saves").join(&world);
    if !source.join("level.dat").exists() {
        return Err(Error::msg("world_not_found"));
    }
    let dir = backups_dir(&state, &id)?;
    std::fs::create_dir_all(&dir)?;
    let file = format!("{world}-{}.zip", chrono::Local::now().format("%Y-%m-%d_%H-%M-%S"));
    let target = dir.join(&file);
    let path = target.clone();
    let result = tokio::task::spawn_blocking(move || {
        let mut tracker = crate::progress::Tracker::new(progress, crate::progress::size_of(&source));
        zip_dirs(&[(source, world)], &path, &mut |n| tracker.add(n))
    })
        .await
        .map_err(|e| Error::msg(e.to_string()))?;
    if let Err(e) = result {
        let _ = std::fs::remove_file(&target);
        return Err(e);
    }
    let meta = std::fs::metadata(&target)?;
    Ok(Backup {
        world: backup_world(&file).unwrap_or_default().to_owned(),
        created_at: iso(meta.modified()).unwrap_or_default(),
        size: meta.len(),
        file,
    })
}

/// Папки целиком, каждая внутри архива под своим именем — чтобы распаковать прямо на место.
pub(crate) fn zip_dirs(sources: &[(PathBuf, String)], target: &Path, add: &mut dyn FnMut(u64)) -> Result<()> {
    let mut zip = zip::ZipWriter::new(std::fs::File::create(target)?);
    let options = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    let mut stack: Vec<(PathBuf, String)> = sources.to_vec();
    while let Some((dir, prefix)) = stack.pop() {
        for entry in std::fs::read_dir(&dir)? {
            let entry = entry?;
            let name = format!("{prefix}/{}", entry.file_name().to_string_lossy());
            if entry.file_type()?.is_dir() {
                zip.add_directory(format!("{name}/"), options)?;
                stack.push((entry.path(), name));
            } else if entry.file_name() != "session.lock" {
                // session.lock игра держит заблокированным, а в копии он не нужен
                zip.start_file(name, options)?;
                let bytes = std::fs::read(entry.path())?;
                zip.write_all(&bytes)?;
                add(bytes.len() as u64);
            }
        }
    }
    zip.finish()?;
    Ok(())
}

/// Распаковывает архив в `dest`, не выпуская файлы за её пределы.
pub(crate) fn unzip_into(archive: &Path, dest: &Path) -> Result<()> {
    let mut zip = zip::ZipArchive::new(std::fs::File::open(archive)?)?;
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i)?;
        let path = safe_join(dest, entry.name())?;
        if entry.is_dir() {
            std::fs::create_dir_all(&path)?;
            continue;
        }
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::io::copy(&mut entry, &mut std::fs::File::create(&path)?)?;
    }
    Ok(())
}

/// Верхние папки архива — какие миры в нём лежат.
pub(crate) fn zip_roots(archive: &Path) -> Result<Vec<String>> {
    let zip = zip::ZipArchive::new(std::fs::File::open(archive)?)?;
    let mut roots: Vec<String> = zip.file_names().filter_map(|n| n.split(['/', '\\']).next()).filter(|n| !n.is_empty()).map(str::to_owned).collect();
    roots.sort();
    roots.dedup();
    Ok(roots)
}

/// Оставляет у мира `keep` последних копий, лишние удаляет.
fn prune(state: &AppState, id: &str, world: &str, keep: usize) -> Result<()> {
    let dir = backups_dir(state, id)?;
    for old in list_backups(state, id)?.into_iter().filter(|b| b.world == world).skip(keep) {
        let _ = std::fs::remove_file(dir.join(old.file));
    }
    Ok(())
}

/// Копия мира без прогресса — для автобэкапа и страховки перед восстановлением.
fn backup_quiet(state: &AppState, id: &str, world: &str) -> Result<()> {
    let source = state.paths.game_dir(id)?.join("saves").join(world);
    let dir = backups_dir(state, id)?;
    std::fs::create_dir_all(&dir)?;
    let target = dir.join(format!("{world}-{}.zip", chrono::Local::now().format("%Y-%m-%d_%H-%M-%S")));
    if let Err(e) = zip_dirs(&[(source, world.to_owned())], &target, &mut |_| {}) {
        let _ = std::fs::remove_file(&target);
        return Err(e);
    }
    Ok(())
}

/// После выхода из игры: копии миров, в которые заходили с момента `since`.
pub async fn auto_backup(state: &AppState, id: &str, since: std::time::SystemTime, keep: usize) {
    let Ok(saves) = state.paths.game_dir(id).map(|d| d.join("saves")) else { return };
    let Ok(entries) = std::fs::read_dir(&saves) else { return };
    let played: Vec<String> = entries
        .flatten()
        .filter(|e| std::fs::metadata(e.path().join("level.dat")).and_then(|m| m.modified()).is_ok_and(|t| t >= since))
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    for world in played {
        if let Err(e) = backup_quiet(state, id, &world) {
            eprintln!("auto backup {world}: {e}");
            continue;
        }
        let _ = prune(state, id, &world, keep.max(1));
    }
}

/// Восстанавливает мир из копии. Текущий мир сначала тоже сохраняется — откат можно отменить.
#[tauri::command]
pub async fn world_restore(state: State<'_, AppState>, id: String, file: String) -> Result<()> {
    if state.running.lock().await.contains_key(&id) {
        return Err(Error::msg("instance_running"));
    }
    let file = checked_name(&file)?.to_owned();
    let archive = backups_dir(&state, &id)?.join(&file);
    let world = backup_world(&file).ok_or_else(|| Error::msg("file_not_found"))?.to_owned();
    if !archive.is_file() || zip_roots(&archive)? != [world.clone()] {
        return Err(Error::msg("file_not_found"));
    }
    let saves = state.paths.game_dir(&id)?.join("saves");
    let current = saves.join(&world);
    if current.join("level.dat").exists() {
        backup_quiet(&state, &id, &world)?;
    }
    tokio::task::spawn_blocking(move || -> Result<()> {
        if current.exists() {
            std::fs::remove_dir_all(&current)?;
        }
        std::fs::create_dir_all(&saves)?;
        unzip_into(&archive, &saves)
    })
    .await
    .map_err(|e| Error::msg(e.to_string()))?
}

#[tauri::command]
pub fn world_backup_delete(state: State<'_, AppState>, id: String, file: String) -> Result<()> {
    let file = checked_name(&file)?.to_owned();
    std::fs::remove_file(backups_dir(&state, &id)?.join(file))?;
    Ok(())
}

#[tauri::command]
pub async fn screenshots_list(state: State<'_, AppState>, id: String) -> Result<Vec<Screenshot>> {
    let dir = state.paths.game_dir(&id)?.join("screenshots");
    let Ok(entries) = std::fs::read_dir(dir) else { return Ok(vec![]) };
    let mut out: Vec<Screenshot> = entries
        .flatten()
        .filter_map(|e| {
            let file = e.file_name().to_string_lossy().into_owned();
            if !file.to_ascii_lowercase().ends_with(".png") {
                return None;
            }
            let meta = e.metadata().ok()?;
            Some(Screenshot {
                path: e.path().to_string_lossy().into_owned(),
                taken_at: iso(meta.modified())?,
                size: meta.len(),
                file,
            })
        })
        .collect();
    out.sort_by(|a, b| b.taken_at.cmp(&a.taken_at));
    Ok(out)
}

/// Открывает файл или папку внутри сборки в программе по умолчанию. `relative` — путь от папки
/// игры (`saves/Мир`, `screenshots/x.png`) или от папки сборки, если начинается с `@/`.
#[tauri::command]
pub fn instance_open_path(app: AppHandle, state: State<'_, AppState>, id: String, relative: String, reveal: bool) -> Result<()> {
    let path = match relative.strip_prefix("@/") {
        Some(rest) => safe_join(&state.paths.instance_dir(&id)?, rest)?,
        None => safe_join(&state.paths.game_dir(&id)?, &relative)?,
    };
    if !path.exists() {
        if reveal {
            std::fs::create_dir_all(&path)?;
        } else {
            return Err(Error::msg("file_not_found"));
        }
    }
    let opener = app.opener();
    let result = if reveal && path.is_file() {
        opener.reveal_item_in_dir(&path)
    } else {
        opener.open_path(path.to_string_lossy(), None::<&str>)
    };
    result.map_err(|e| Error::msg(e.to_string()))
}

/// Открывает файл по полному пути — для отчёта о краше, который лежит в папке игры.
#[tauri::command]
pub fn open_crash_report(app: AppHandle, state: State<'_, AppState>, path: String) -> Result<()> {
    let path = PathBuf::from(path);
    // Только файлы внутри папки данных лаунчера
    if !path.starts_with(state.paths.instances()) || !path.is_file() {
        return Err(Error::msg("file_not_found"));
    }
    app.opener().open_path(path.to_string_lossy(), None::<&str>).map_err(|e| Error::msg(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backup_names() {
        assert_eq!(backup_world("Мой мир-2024-05-01_12-30-00.zip"), Some("Мой мир"));
        assert_eq!(backup_world("New World-2024-05-01_12-30-00.zip"), Some("New World"));
        assert_eq!(backup_world("x.zip"), None);
    }

    #[test]
    fn zips_world_folder() {
        let base = std::env::temp_dir().join(format!("shard-world-{}", uuid::Uuid::new_v4()));
        let world = base.join("World");
        std::fs::create_dir_all(world.join("region")).unwrap();
        std::fs::write(world.join("level.dat"), b"x").unwrap();
        std::fs::write(world.join("session.lock"), b"x").unwrap();
        std::fs::write(world.join("region/r.0.0.mca"), b"y").unwrap();
        let target = base.join("b.zip");
        zip_dirs(&[(world.clone(), "World".to_owned())], &target, &mut |_| {}).unwrap();
        let mut archive = zip::ZipArchive::new(std::fs::File::open(&target).unwrap()).unwrap();
        assert!(archive.by_name("World/level.dat").is_ok());
        assert!(archive.by_name("World/region/r.0.0.mca").is_ok());
        assert!(archive.by_name("World/session.lock").is_err());
        drop(archive);
        std::fs::remove_dir_all(base).unwrap();
    }
}
