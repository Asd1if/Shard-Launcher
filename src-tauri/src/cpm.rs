//! Модели Customizable Player Models (CPM): библиотека `.cpmmodel` и установка в сборки.
//!
//! Мод CPM берёт модели из `<папка игры>/player_models/`, выбирают их в игре через
//! меню жестов → Models. Библиотека — просто папка `<root>/cpm/` с файлами моделей.

use std::path::PathBuf;

use serde::Serialize;
use tauri::{AppHandle, State};
use tauri_plugin_opener::OpenerExt;

use crate::content::checked_name;
use crate::error::{Error, Result};
use crate::paths::Paths;
use crate::state::AppState;

const EXT: &str = "cpmmodel";

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CpmModel {
    /// Имя файла — оно же имя в папке player_models сборки
    pub id: String,
    pub name: String,
    pub size: u64,
    pub added_at: String,
}

fn library(paths: &Paths) -> PathBuf {
    paths.root.join("cpm")
}

fn model_file(paths: &Paths, id: &str) -> Result<PathBuf> {
    let id = checked_name(id)?;
    if !id.to_lowercase().ends_with(&format!(".{EXT}")) {
        return Err(Error::msg("cpm_not_model"));
    }
    Ok(library(paths).join(id))
}

fn is_model(path: &std::path::Path) -> bool {
    path.is_file() && path.extension().is_some_and(|e| e.eq_ignore_ascii_case(EXT))
}

#[tauri::command]
pub fn cpm_list(state: State<'_, AppState>) -> Vec<CpmModel> {
    let Ok(entries) = std::fs::read_dir(library(&state.paths)) else { return vec![] };
    let mut list: Vec<CpmModel> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| is_model(p))
        .filter_map(|p| {
            let meta = p.metadata().ok()?;
            let id = p.file_name()?.to_string_lossy().into_owned();
            let added: chrono::DateTime<chrono::Utc> = meta.modified().ok()?.into();
            Some(CpmModel { name: p.file_stem()?.to_string_lossy().into_owned(), id, size: meta.len(), added_at: added.to_rfc3339() })
        })
        .collect();
    list.sort_by(|a, b| b.added_at.cmp(&a.added_at));
    list
}

/// Копирует файл `.cpmmodel` в библиотеку. Файл с тем же именем заменяется.
#[tauri::command]
pub fn cpm_add(state: State<'_, AppState>, path: String) -> Result<String> {
    let source = PathBuf::from(path);
    if !is_model(&source) {
        return Err(Error::msg("cpm_not_model"));
    }
    let id = source.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let target = model_file(&state.paths, &id)?;
    std::fs::create_dir_all(library(&state.paths))?;
    std::fs::copy(source, target)?;
    Ok(id)
}

#[tauri::command]
pub fn cpm_remove(state: State<'_, AppState>, id: String) -> Result<()> {
    let file = model_file(&state.paths, &id)?;
    if file.exists() {
        std::fs::remove_file(file)?;
    }
    Ok(())
}

fn installed_file(state: &AppState, instance: &str, id: &str) -> Result<PathBuf> {
    Ok(state.paths.game_dir(instance)?.join("player_models").join(checked_name(id)?))
}

/// В каких сборках стоит модель: [id сборки, имя файла].
#[tauri::command]
pub fn cpm_installed(state: State<'_, AppState>) -> Result<Vec<(String, String)>> {
    let mut out = vec![];
    for instance in crate::instances::list(&state.paths)? {
        let Ok(entries) = std::fs::read_dir(state.paths.game_dir(&instance.id)?.join("player_models")) else { continue };
        for p in entries.flatten().map(|e| e.path()).filter(|p| is_model(p)) {
            out.push((instance.id.clone(), p.file_name().unwrap_or_default().to_string_lossy().into_owned()));
        }
    }
    Ok(out)
}

#[tauri::command]
pub fn cpm_install(state: State<'_, AppState>, id: String, instance_id: String) -> Result<()> {
    let from = model_file(&state.paths, &id)?;
    let to = installed_file(&state, &instance_id, &id)?;
    if let Some(dir) = to.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::copy(from, to)?;
    Ok(())
}

#[tauri::command]
pub fn cpm_uninstall(state: State<'_, AppState>, id: String, instance_id: String) -> Result<()> {
    let file = installed_file(&state, &instance_id, &id)?;
    if file.exists() {
        std::fs::remove_file(file)?;
    }
    Ok(())
}

/// Открывает папку библиотеки в проводнике.
#[tauri::command]
pub fn cpm_open(app: AppHandle, state: State<'_, AppState>) -> Result<()> {
    let dir = library(&state.paths);
    std::fs::create_dir_all(&dir)?;
    app.opener().open_path(dir.to_string_lossy(), None::<&str>).map_err(|e| Error::msg(e.to_string()))
}
