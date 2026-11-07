//! Модели Figura (аватары): своя библиотека вне аккаунта и установка в сборки.
//!
//! Аватар Figura — папка с `avatar.json`, моделями Blockbench (`*.bbmodel`), текстурами
//! и скриптами Lua. В игре мод Figura ищет их в `<папка игры>/figura/avatars/`.
//! Библиотека лежит в `<root>/figura/<id>/`, рядом `figura.json` с названиями.

use std::io::Read;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, State};
use tauri_plugin_opener::OpenerExt;

use crate::content::checked_name;
use crate::error::{Error, Result};
use crate::modpack::safe_join;
use crate::paths::Paths;
use crate::state::AppState;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Avatar {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub authors: Vec<String>,
    /// Цвет аватара из avatar.json (`#rrggbb`), Figura рисует им значок
    #[serde(default)]
    pub color: Option<String>,
    /// Имена моделей Blockbench без расширения
    #[serde(default)]
    pub models: Vec<String>,
    #[serde(default)]
    pub scripts: u32,
    #[serde(default)]
    pub size: u64,
    pub added_at: String,
    /// Имя папки в `figura/avatars` сборки
    pub folder: String,
}

/// Файлы аватара для 3D-превью.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AvatarFiles {
    /// Путь внутри аватара → JSON модели Blockbench
    pub models: Vec<(String, String)>,
    /// Путь внутри аватара → PNG в data URL (текстуры, которые не вшиты в модель)
    pub textures: Vec<(String, String)>,
}

/// Поля avatar.json, которые нужны лаунчеру. Остальное Figura читает сама.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct AvatarJson {
    name: Option<String>,
    description: Option<String>,
    authors: Option<serde_json::Value>,
    author: Option<String>,
    color: Option<String>,
}

fn library(paths: &Paths) -> PathBuf {
    paths.root.join("figura")
}

fn index_file(paths: &Paths) -> PathBuf {
    library(paths).join("figura.json")
}

fn load_index(paths: &Paths) -> Vec<Avatar> {
    std::fs::read(index_file(paths)).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
}

fn save_index(paths: &Paths, list: &[Avatar]) -> Result<()> {
    std::fs::create_dir_all(library(paths))?;
    std::fs::write(index_file(paths), serde_json::to_vec_pretty(list)?)?;
    Ok(())
}

fn avatar_dir(paths: &Paths, id: &str) -> Result<PathBuf> {
    if id.is_empty() || !id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
        return Err(Error::msg("invalid avatar id"));
    }
    Ok(library(paths).join(id))
}

fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for e in entries.flatten() {
        let path = e.path();
        if path.is_dir() {
            walk(&path, out);
        } else {
            out.push(path);
        }
    }
}

/// Читает avatar.json и считает модели и скрипты — сведения для карточки в библиотеке.
fn describe(dir: &Path, id: String, folder: String) -> Avatar {
    let json: AvatarJson = std::fs::read(dir.join("avatar.json"))
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default();
    let mut files = vec![];
    walk(dir, &mut files);
    let ext = |p: &PathBuf, e: &str| p.extension().is_some_and(|x| x.eq_ignore_ascii_case(e));
    let mut models: Vec<String> = files
        .iter()
        .filter(|p| ext(p, "bbmodel"))
        .filter_map(|p| Some(p.file_stem()?.to_string_lossy().into_owned()))
        .collect();
    models.sort();
    let authors = match json.authors {
        Some(serde_json::Value::Array(list)) => list.into_iter().filter_map(|v| v.as_str().map(str::to_owned)).collect(),
        Some(serde_json::Value::String(s)) => vec![s],
        _ => json.author.into_iter().collect(),
    };
    Avatar {
        id,
        name: json.name.filter(|n| !n.trim().is_empty()).unwrap_or_else(|| folder.clone()),
        description: json.description.unwrap_or_default(),
        authors,
        color: json.color,
        models,
        scripts: files.iter().filter(|p| ext(p, "lua")).count() as u32,
        size: files.iter().filter_map(|p| p.metadata().ok()).map(|m| m.len()).sum(),
        added_at: chrono::Utc::now().to_rfc3339(),
        folder,
    }
}

/// Папка с avatar.json: сам корень или единственная вложенная папка (так часто пакуют zip).
fn avatar_root(dir: &Path) -> Option<PathBuf> {
    if dir.join("avatar.json").is_file() {
        return Some(dir.to_path_buf());
    }
    let subdirs: Vec<PathBuf> = std::fs::read_dir(dir).ok()?.flatten().map(|e| e.path()).filter(|p| p.is_dir()).collect();
    subdirs.into_iter().find(|p| p.join("avatar.json").is_file())
}

fn copy_dir(from: &Path, to: &Path) -> Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let target = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir(&entry.path(), &target)?;
        } else {
            std::fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

fn unzip(path: &Path, to: &Path) -> Result<()> {
    let mut archive = zip::ZipArchive::new(std::fs::File::open(path)?).map_err(|_| Error::msg("figura_not_avatar"))?;
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i)?;
        let name = entry.name().replace('\\', "/");
        if name.ends_with('/') {
            continue;
        }
        let target = safe_join(to, &name)?;
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut bytes = Vec::with_capacity((entry.size() as usize).min(1 << 24));
        entry.read_to_end(&mut bytes)?;
        std::fs::write(target, bytes)?;
    }
    Ok(())
}

/// Название папки аватара в игре: из исходной папки или архива, без недопустимых символов.
fn folder_name(source: &Path, root: &Path, staging: &Path) -> String {
    let name = if root != staging { root.file_name() } else { source.file_stem() };
    let name: String = name
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
        .chars()
        .map(|c| if c.is_alphanumeric() || " _-.()".contains(c) { c } else { '_' })
        .collect();
    let name = name.trim().trim_matches('.').to_owned();
    if name.is_empty() { "avatar".to_owned() } else { name }
}

#[tauri::command]
pub fn figura_list(state: State<'_, AppState>) -> Vec<Avatar> {
    let mut list = load_index(&state.paths);
    list.sort_by(|a, b| b.added_at.cmp(&a.added_at));
    list
}

/// Добавляет аватар из папки или zip-архива в библиотеку.
#[tauri::command]
pub async fn figura_add(state: State<'_, AppState>, path: String) -> Result<Avatar> {
    let source = PathBuf::from(&path);
    let id = uuid::Uuid::new_v4().to_string();
    let target = avatar_dir(&state.paths, &id)?;
    let staging = library(&state.paths).join(format!("tmp-{id}"));
    let (src, stage, dest) = (source.clone(), staging.clone(), target.clone());
    let result = tokio::task::spawn_blocking(move || -> Result<String> {
        if src.is_dir() {
            copy_dir(&src, &stage)?;
        } else {
            unzip(&src, &stage)?;
        }
        let root = avatar_root(&stage).ok_or_else(|| Error::msg("figura_not_avatar"))?;
        let folder = folder_name(&src, &root, &stage);
        std::fs::rename(&root, &dest)?;
        Ok(folder)
    })
    .await
    .map_err(|e| Error::msg(e.to_string()))?;
    let _ = std::fs::remove_dir_all(&staging);
    let folder = match result {
        Ok(f) => f,
        Err(e) => {
            let _ = std::fs::remove_dir_all(&target);
            return Err(e);
        }
    };
    let avatar = describe(&target, id, folder);
    let mut list = load_index(&state.paths);
    list.push(avatar.clone());
    save_index(&state.paths, &list)?;
    Ok(avatar)
}

#[tauri::command]
pub fn figura_remove(state: State<'_, AppState>, id: String) -> Result<()> {
    let dir = avatar_dir(&state.paths, &id)?;
    if dir.exists() {
        std::fs::remove_dir_all(dir)?;
    }
    let list: Vec<Avatar> = load_index(&state.paths).into_iter().filter(|a| a.id != id).collect();
    save_index(&state.paths, &list)
}

#[tauri::command]
pub fn figura_rename(state: State<'_, AppState>, id: String, name: String) -> Result<Avatar> {
    let name = name.trim();
    if name.is_empty() {
        return Err(Error::msg("name is empty"));
    }
    let mut list = load_index(&state.paths);
    let avatar = list.iter_mut().find(|a| a.id == id).ok_or_else(|| Error::msg("avatar not found"))?;
    avatar.name = name.to_owned();
    let out = avatar.clone();
    save_index(&state.paths, &list)?;
    Ok(out)
}

/// Модели и текстуры для 3D-превью. Большие текстуры (>4 МБ) пропускаем.
#[tauri::command]
pub async fn figura_files(state: State<'_, AppState>, id: String) -> Result<AvatarFiles> {
    use base64::Engine;
    let dir = avatar_dir(&state.paths, &id)?;
    tokio::task::spawn_blocking(move || {
        let mut files = vec![];
        walk(&dir, &mut files);
        let mut out = AvatarFiles { models: vec![], textures: vec![] };
        for path in files {
            let rel = path.strip_prefix(&dir).map(|p| p.to_string_lossy().replace('\\', "/")).unwrap_or_default();
            let ext = path.extension().map(|e| e.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
            let Ok(meta) = path.metadata() else { continue };
            if ext == "bbmodel" && meta.len() < 32 * 1024 * 1024 {
                if let Ok(text) = std::fs::read_to_string(&path) {
                    out.models.push((rel, text));
                }
            } else if ext == "png" && meta.len() < 4 * 1024 * 1024 {
                if let Ok(bytes) = std::fs::read(&path) {
                    out.textures.push((rel, format!("data:image/png;base64,{}", base64::engine::general_purpose::STANDARD.encode(bytes))));
                }
            }
        }
        out.models.sort();
        Ok(out)
    })
    .await
    .map_err(|e| Error::msg(e.to_string()))?
}

fn installed_dir(state: &AppState, instance: &str, folder: &str) -> Result<PathBuf> {
    Ok(state.paths.game_dir(instance)?.join("figura").join("avatars").join(checked_name(folder)?))
}

/// В каких сборках стоит аватар (по имени папки в figura/avatars).
#[tauri::command]
pub fn figura_installed(state: State<'_, AppState>) -> Result<Vec<(String, String)>> {
    let mut out = vec![];
    for instance in crate::instances::list(&state.paths)? {
        let dir = state.paths.game_dir(&instance.id)?.join("figura").join("avatars");
        let Ok(entries) = std::fs::read_dir(dir) else { continue };
        for e in entries.flatten().filter(|e| e.path().is_dir()) {
            out.push((instance.id.clone(), e.file_name().to_string_lossy().into_owned()));
        }
    }
    Ok(out)
}

/// Копирует аватар в сборку. Уже стоящую копию заменяет — так обновляют аватар.
#[tauri::command]
pub async fn figura_install(state: State<'_, AppState>, id: String, instance_id: String) -> Result<()> {
    let avatar = load_index(&state.paths).into_iter().find(|a| a.id == id).ok_or_else(|| Error::msg("avatar not found"))?;
    let from = avatar_dir(&state.paths, &id)?;
    let to = installed_dir(&state, &instance_id, &avatar.folder)?;
    tokio::task::spawn_blocking(move || -> Result<()> {
        if to.exists() {
            std::fs::remove_dir_all(&to)?;
        }
        copy_dir(&from, &to)
    })
    .await
    .map_err(|e| Error::msg(e.to_string()))?
}

#[tauri::command]
pub fn figura_uninstall(state: State<'_, AppState>, id: String, instance_id: String) -> Result<()> {
    let avatar = load_index(&state.paths).into_iter().find(|a| a.id == id).ok_or_else(|| Error::msg("avatar not found"))?;
    let dir = installed_dir(&state, &instance_id, &avatar.folder)?;
    if dir.exists() {
        std::fs::remove_dir_all(dir)?;
    }
    Ok(())
}

#[tauri::command]
pub fn figura_open(app: AppHandle, state: State<'_, AppState>, id: String) -> Result<()> {
    let dir = avatar_dir(&state.paths, &id)?;
    app.opener().open_path(dir.to_string_lossy(), None::<&str>).map_err(|e| Error::msg(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_avatar_in_subfolder() {
        let base = std::env::temp_dir().join(format!("shard-figura-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(base.join("Cat")).unwrap();
        std::fs::write(base.join("Cat/avatar.json"), r##"{"name":"Cat","authors":["me"],"color":"#ff8800"}"##).unwrap();
        std::fs::write(base.join("Cat/cat.bbmodel"), "{}").unwrap();
        std::fs::write(base.join("Cat/script.lua"), "").unwrap();
        let root = avatar_root(&base).unwrap();
        assert_eq!(root, base.join("Cat"));
        let a = describe(&root, "x".into(), folder_name(&base, &root, &base));
        assert_eq!(a.name, "Cat");
        assert_eq!(a.folder, "Cat");
        assert_eq!(a.authors, vec!["me"]);
        assert_eq!(a.models, vec!["cat"]);
        assert_eq!(a.scripts, 1);
        std::fs::remove_dir_all(base).unwrap();
    }
}
