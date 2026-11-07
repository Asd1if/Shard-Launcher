//! Резервные копии мира сервера: обычный мир, Незер и Энд (Paper хранит их отдельными папками).
//! Лежат в `<сервер>/backups`, восстанавливаются одной кнопкой при остановленном сервере.

use std::path::{Path, PathBuf};

use serde::Serialize;
use tauri::State;

use crate::error::{Error, Result};
use crate::server::{self, server_dir};
use crate::state::AppState;
use crate::worlds::{unzip_into, zip_dirs, zip_roots};

/// Сколько копий хранить, если делать их при каждом запуске
pub const KEEP: usize = 10;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerBackup {
    file: String,
    created_at: String,
    size: u64,
}

fn backups_dir(dir: &Path) -> PathBuf {
    dir.join("backups")
}

/// Папки мира: level-name и её _nether/_the_end, если есть.
fn world_dirs(dir: &Path) -> Vec<(PathBuf, String)> {
    let level = server::read_properties(dir).into_iter().find(|(k, _)| k == "level-name").map_or_else(|| "world".to_owned(), |(_, v)| v);
    // level-name может быть путём — берём только простые имена
    if level.is_empty() || level.contains(['/', '\\', '.']) {
        return vec![];
    }
    [level.clone(), format!("{level}_nether"), format!("{level}_the_end")]
        .into_iter()
        .map(|name| (dir.join(&name), name))
        .filter(|(path, _)| path.is_dir())
        .collect()
}

fn list(dir: &Path) -> Vec<ServerBackup> {
    let Ok(entries) = std::fs::read_dir(backups_dir(dir)) else { return vec![] };
    let mut out: Vec<ServerBackup> = entries
        .flatten()
        .filter_map(|e| {
            let file = e.file_name().to_str()?.to_owned();
            if !file.ends_with(".zip") {
                return None;
            }
            let meta = e.metadata().ok()?;
            Some(ServerBackup { created_at: chrono::DateTime::<chrono::Utc>::from(meta.modified().ok()?).to_rfc3339(), size: meta.len(), file })
        })
        .collect();
    out.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    out
}

/// Копия мира; `keep` — сколько последних копий оставить.
pub fn make(dir: &Path, keep: Option<usize>) -> Result<String> {
    let sources = world_dirs(dir);
    if sources.is_empty() {
        return Err(Error::msg("world_not_found"));
    }
    let backups = backups_dir(dir);
    std::fs::create_dir_all(&backups)?;
    let file = format!("world-{}.zip", chrono::Local::now().format("%Y-%m-%d_%H-%M-%S"));
    let target = backups.join(&file);
    if let Err(e) = zip_dirs(&sources, &target, &mut |_| {}) {
        let _ = std::fs::remove_file(&target);
        return Err(e);
    }
    if let Some(keep) = keep {
        for old in list(dir).into_iter().skip(keep.max(1)) {
            let _ = std::fs::remove_file(backups.join(old.file));
        }
    }
    Ok(file)
}

fn checked(file: &str) -> Result<&str> {
    if file.ends_with(".zip") && !file.contains(['/', '\\']) && !file.starts_with('.') {
        Ok(file)
    } else {
        Err(Error::msg("file_not_found"))
    }
}

#[tauri::command]
pub fn server_backups(state: State<'_, AppState>, id: String) -> Result<Vec<ServerBackup>> {
    Ok(list(&server_dir(&state, &id)?))
}

/// Копия во время работы тоже возможна: перед ней сервер сохраняет мир и не пишет, пока копируем.
#[tauri::command]
pub async fn server_backup(state: State<'_, AppState>, id: String) -> Result<String> {
    let dir = server_dir(&state, &id)?;
    let running = server::is_running(&id).await;
    if running {
        server::server_command(id.clone(), "save-off".to_owned()).await?;
        server::server_command(id.clone(), "save-all flush".to_owned()).await?;
        tokio::time::sleep(std::time::Duration::from_secs(3)).await;
    }
    let result = tokio::task::spawn_blocking(move || make(&dir, None)).await.map_err(|e| Error::msg(e.to_string()))?;
    if running {
        let _ = server::server_command(id, "save-on".to_owned()).await;
    }
    result
}

#[tauri::command]
pub async fn server_backup_restore(state: State<'_, AppState>, id: String, file: String) -> Result<()> {
    if server::is_running(&id).await {
        return Err(Error::msg("server_running"));
    }
    let dir = server_dir(&state, &id)?;
    let archive = backups_dir(&dir).join(checked(&file)?);
    let roots = zip_roots(&archive)?;
    if roots.is_empty() || roots.iter().any(|r| r.contains(['/', '\\', '.']) || r == "backups" || r == "plugins" || r == "mods") {
        return Err(Error::msg("file_not_found"));
    }
    tokio::task::spawn_blocking(move || -> Result<()> {
        // Текущий мир — в копию, чтобы откат можно было отменить
        if !world_dirs(&dir).is_empty() {
            make(&dir, None)?;
        }
        for root in &roots {
            let path = dir.join(root);
            if path.exists() {
                std::fs::remove_dir_all(&path)?;
            }
        }
        unzip_into(&archive, &dir)
    })
    .await
    .map_err(|e| Error::msg(e.to_string()))?
}

#[tauri::command]
pub fn server_backup_delete(state: State<'_, AppState>, id: String, file: String) -> Result<()> {
    let dir = server_dir(&state, &id)?;
    std::fs::remove_file(backups_dir(&dir).join(checked(&file)?))?;
    Ok(())
}
