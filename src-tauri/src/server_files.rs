//! Файловый менеджер сервера: список папок, просмотр и правка текстовых файлов, удаление.
//! Все пути — относительно папки сервера, выйти за неё нельзя.

use std::path::{Component, Path, PathBuf};

use serde::Serialize;
use tauri::State;

use crate::error::{Error, Result};
use crate::server::server_dir;
use crate::state::AppState;

/// Больше этого текстовый редактор не открывает
const MAX_TEXT: u64 = 2 * 1024 * 1024;
/// Служебные файлы лаунчера — их правит сам Shard
const HIDDEN: [&str; 2] = ["shard-server.json", "shard-store.json"];

/// Путь внутри папки сервера; `..`, абсолютные пути и диски запрещены.
pub(crate) fn resolve(root: &Path, rel: &str) -> Result<PathBuf> {
    let mut out = root.to_path_buf();
    for part in Path::new(rel.trim_matches(['/', '\\'])).components() {
        match part {
            Component::Normal(p) => out.push(p),
            Component::CurDir => {}
            _ => return Err(Error::msg("bad_path")),
        }
    }
    // Ссылки (junction, symlink) не должны уводить наружу
    if let (Ok(real), Ok(base)) = (out.canonicalize(), root.canonicalize()) {
        if !real.starts_with(base) {
            return Err(Error::msg("bad_path"));
        }
    }
    Ok(out)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileEntry {
    name: String,
    dir: bool,
    size: u64,
    /// Время изменения, секунды Unix
    modified: u64,
}

#[tauri::command]
pub fn server_files(state: State<'_, AppState>, id: String, path: String) -> Result<Vec<FileEntry>> {
    let root = server_dir(&state, &id)?;
    let dir = resolve(&root, &path)?;
    let mut out: Vec<FileEntry> = std::fs::read_dir(&dir)?
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().to_str()?.to_owned();
            if path.trim_matches(['/', '\\']).is_empty() && HIDDEN.contains(&name.as_str()) {
                return None;
            }
            let meta = e.metadata().ok()?;
            Some(FileEntry {
                dir: meta.is_dir(),
                size: if meta.is_dir() { 0 } else { meta.len() },
                modified: meta.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map_or(0, |d| d.as_secs()),
                name,
            })
        })
        .collect();
    out.sort_by(|a, b| b.dir.cmp(&a.dir).then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase())));
    Ok(out)
}

#[tauri::command]
pub fn server_file_read(state: State<'_, AppState>, id: String, path: String) -> Result<String> {
    let file = resolve(&server_dir(&state, &id)?, &path)?;
    if std::fs::metadata(&file)?.len() > MAX_TEXT {
        return Err(Error::msg("file_too_large"));
    }
    let bytes = std::fs::read(&file)?;
    // Нулевой байт — почти наверняка двоичный файл
    if bytes.iter().take(8192).any(|&b| b == 0) {
        return Err(Error::msg("file_binary"));
    }
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

#[tauri::command]
pub fn server_file_write(state: State<'_, AppState>, id: String, path: String, content: String) -> Result<()> {
    let root = server_dir(&state, &id)?;
    let file = resolve(&root, &path)?;
    if file == root || file.is_dir() {
        return Err(Error::msg("bad_path"));
    }
    let tmp = file.with_extension("shard-tmp");
    std::fs::write(&tmp, content)?;
    std::fs::rename(&tmp, &file)?;
    Ok(())
}

#[tauri::command]
pub async fn server_file_delete(state: State<'_, AppState>, id: String, path: String) -> Result<()> {
    let root = server_dir(&state, &id)?;
    let target = resolve(&root, &path)?;
    if target == root {
        return Err(Error::msg("bad_path"));
    }
    if crate::server::is_running(&id).await {
        return Err(Error::msg("server_running"));
    }
    if target.is_dir() {
        std::fs::remove_dir_all(&target)?;
    } else {
        std::fs::remove_file(&target)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_escape() {
        let root = std::env::temp_dir();
        assert!(resolve(&root, "../x").is_err());
        assert!(resolve(&root, "a/../../x").is_err());
        assert!(resolve(&root, "C:/Windows").is_err());
        assert!(resolve(&root, "config/paper.yml").is_ok());
    }
}
