//! Автообновление лаунчера из релизов GitHub. Репозиторий задаётся при сборке
//! (`update-repo.txt` или SHARD_UPDATE_REPO, вида `owner/name`); без него проверка выключена.
//!
//! Установленный лаунчер (рядом лежит uninstall.exe) обновляется установщиком `*-setup.exe`.
//! Портативный и просто скопированный exe подменяют себя файлом из `*portable*.zip`.

use std::io::Read;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, State};

use crate::error::{Error, Result};
use crate::state::AppState;

const REPO: Option<&str> = option_env!("SHARD_UPDATE_REPO");

#[derive(Deserialize)]
struct Release {
    tag_name: String,
    #[serde(default)]
    body: String,
    html_url: String,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
    assets: Vec<Asset>,
}

#[derive(Deserialize, Clone)]
struct Asset {
    name: String,
    browser_download_url: String,
    size: u64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    pub current: String,
    pub version: String,
    /// Описание релиза (Markdown)
    pub notes: String,
    pub url: String,
    pub size: u64,
}

fn repo() -> Option<&'static str> {
    REPO.map(str::trim).filter(|r| r.split('/').count() == 2 && !r.contains(' '))
}

/// «v1.2.3» → [1, 2, 3]; лишнее после цифр (-beta) отбрасываем.
fn parse_version(v: &str) -> Vec<u64> {
    v.trim_start_matches(['v', 'V'])
        .split('.')
        .map(|p| p.chars().take_while(char::is_ascii_digit).collect::<String>().parse().unwrap_or(0))
        .collect()
}

fn newer(latest: &str, current: &str) -> bool {
    let (mut a, mut b) = (parse_version(latest), parse_version(current));
    let len = a.len().max(b.len());
    a.resize(len, 0);
    b.resize(len, 0);
    a > b
}

fn exe() -> Result<PathBuf> {
    Ok(std::env::current_exe()?)
}

/// Лаунчер поставлен установщиком — тогда обновляем им же.
fn installed() -> bool {
    exe().ok().and_then(|p| p.parent().map(|d| d.join("uninstall.exe").is_file())).unwrap_or(false)
}

fn pick(assets: &[Asset]) -> Option<Asset> {
    let name = |a: &&Asset| a.name.to_lowercase();
    if installed() {
        assets.iter().find(|a| name(a).ends_with("-setup.exe")).cloned()
    } else {
        assets.iter().find(|a| name(a).ends_with(".zip") && name(a).contains("portable")).cloned()
    }
}

async fn latest(state: &AppState) -> Result<Option<(Release, Asset)>> {
    let Some(repo) = repo() else { return Ok(None) };
    let release: Release = state
        .http
        .get(format!("https://api.github.com/repos/{repo}/releases/latest"))
        .header("Accept", "application/vnd.github+json")
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    if release.draft || release.prerelease || !newer(&release.tag_name, env!("CARGO_PKG_VERSION")) {
        return Ok(None);
    }
    let Some(asset) = pick(&release.assets) else { return Ok(None) };
    Ok(Some((release, asset)))
}

/// Есть ли новая версия. None — обновлений нет или источник не задан.
#[tauri::command]
pub async fn update_check(state: State<'_, AppState>) -> Result<Option<UpdateInfo>> {
    Ok(latest(&state).await?.map(|(r, a)| UpdateInfo {
        current: env!("CARGO_PKG_VERSION").to_owned(),
        version: r.tag_name.trim_start_matches(['v', 'V']).to_owned(),
        notes: r.body,
        url: r.html_url,
        size: a.size,
    }))
}

#[tauri::command]
pub fn update_available() -> bool {
    repo().is_some()
}

/// Скачивает и ставит обновление, затем закрывает лаунчер (новая версия запускается сама).
#[tauri::command]
pub async fn update_install(app: AppHandle, state: State<'_, AppState>, progress: tauri::ipc::Channel<crate::progress::Progress>) -> Result<()> {
    if !state.running.lock().await.is_empty() {
        return Err(Error::msg("update_game_running"));
    }
    let (_, asset) = latest(&state).await?.ok_or_else(|| Error::msg("update_none"))?;
    let mut response = state.http.get(&asset.browser_download_url).send().await?.error_for_status()?;
    let total = response.content_length().unwrap_or(asset.size);
    let mut tracker = crate::progress::Tracker::new(progress, total);
    let mut bytes = Vec::with_capacity(total as usize);
    while let Some(chunk) = response.chunk().await? {
        tracker.add(chunk.len() as u64);
        bytes.extend_from_slice(&chunk);
    }

    if installed() {
        let setup = std::env::temp_dir().join(&asset.name);
        std::fs::write(&setup, &bytes)?;
        // Установщик сам найдёт старую версию и заменит её
        std::process::Command::new(setup).spawn()?;
    } else {
        let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes))?;
        let index = (0..zip.len())
            .find(|&i| zip.by_index(i).map(|f| f.name().to_lowercase().ends_with(".exe")).unwrap_or(false))
            .ok_or_else(|| Error::msg("update_bad_asset"))?;
        let mut new_exe = Vec::new();
        zip.by_index(index)?.read_to_end(&mut new_exe)?;
        let current = exe()?;
        let old = current.with_extension("exe.old");
        let _ = std::fs::remove_file(&old);
        // Запущенный exe нельзя перезаписать, но можно переименовать
        std::fs::rename(&current, &old)?;
        if let Err(e) = std::fs::write(&current, &new_exe) {
            let _ = std::fs::rename(&old, &current);
            return Err(e.into());
        }
        std::process::Command::new(&current).spawn()?;
    }
    app.exit(0);
    Ok(())
}

/// Убирает старый exe, оставшийся после самообновления.
pub fn cleanup() {
    if let Ok(current) = exe() {
        let _ = std::fs::remove_file(current.with_extension("exe.old"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compares_versions() {
        assert!(newer("v0.2.0", "0.1.9"));
        assert!(newer("1.0", "0.9.9"));
        assert!(!newer("v0.1.0", "0.1.0"));
        assert!(!newer("0.1.0-beta", "0.1.0"));
        assert!(newer("0.10.0", "0.9.0"));
    }
}
