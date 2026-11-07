//! Сервер из самого модпака: у CurseForge — серверный архив, который приложил автор,
//! у Modrinth — файлы из `.mrpack`, помеченные как нужные на сервере, и `server-overrides/`.

use std::io::Read;
use std::path::Path;

use super::mrpack::Index;
use super::{extract_overrides, fetch_pack, safe_join};
use crate::content::{curseforge, Source};
use crate::download::{download_all, Download};
use crate::error::{Error, Result};
use crate::instances::PackOrigin;
use crate::state::AppState;

/// Можно ли поднять сервер из модпака. Да, если сборка скачана как модпак: в `.mrpack` есть
/// пометки env.server, а у CurseForge без серверного архива сервер собирается из файлов сборки.
#[tauri::command]
pub async fn server_pack_available(state: tauri::State<'_, AppState>, id: String) -> Result<bool> {
    let instance = crate::instances::load(&state.paths, &id)?;
    Ok(instance.pack.is_some())
}

async fn cf_file(state: &AppState, origin: &PackOrigin) -> Result<curseforge::File> {
    let mod_id: u64 = origin.project_id.parse().map_err(|_| Error::msg("pack_not_linked"))?;
    let file_id: u64 = origin.version_id.as_deref().and_then(|v| v.parse().ok()).ok_or_else(|| Error::msg("pack_not_linked"))?;
    curseforge::get_file(&state.http, mod_id, file_id).await
}

/// Прогресс установки: `files(done, total)` — файлы `.mrpack`, `bytes(done, total)` — скачивание архива.
pub struct Steps<'a> {
    pub files: &'a (dyn Fn(usize, usize) + Sync),
    pub bytes: &'a (dyn Fn(u64, u64) + Sync),
}

/// Кладёт файлы серверной версии модпака в папку сервера.
/// `false` — серверного архива нет ни на CurseForge, ни на GitHub сборки: сервер надо собрать из файлов сборки.
pub async fn install(state: &AppState, origin: &PackOrigin, dir: &Path, steps: &Steps<'_>) -> Result<bool> {
    match origin.source {
        Source::Curseforge => curseforge_pack(state, origin, dir, steps).await,
        Source::Modrinth => modrinth_pack(state, origin, dir, steps.files).await.map(|()| true),
    }
}

async fn curseforge_pack(state: &AppState, origin: &PackOrigin, dir: &Path, steps: &Steps<'_>) -> Result<bool> {
    let file = cf_file(state, origin).await?;
    let Some(server_id) = file.server_pack_file_id else {
        // Многие авторы (например, TerraFirmaGreg) кладут серверные файлы в релизы на GitHub
        let Some(url) = github_server_zip(state, &file).await else { return Ok(false) };
        unpack_from(state, &url, dir, steps).await?;
        return Ok(true);
    };
    let server = curseforge::get_file(&state.http, file.mod_id, server_id).await?;
    let Some(url) = server.download_url else {
        // Автор запретил скачивание из программ — дадим ссылку на страницу файла
        let page = curseforge::get_mod(&state.http, file.mod_id).await.ok().and_then(|m| m.links).and_then(|l| l.website_url);
        let link = curseforge::manual_url(page.as_deref(), server_id).unwrap_or_default();
        return Err(Error::msg(format!("server_pack_manual:{link}")));
    };
    unpack_from(state, &url, dir, steps).await?;
    Ok(true)
}

async fn unpack_from(state: &AppState, url: &str, dir: &Path, steps: &Steps<'_>) -> Result<()> {
    let archive = download_big(state, url, steps.bytes).await?;
    let (from, to) = (archive.clone(), dir.to_path_buf());
    let result = tokio::task::spawn_blocking(move || unpack_server_zip(&from, &to)).await.map_err(|e| Error::msg(e.to_string()))?;
    let _ = std::fs::remove_file(&archive);
    result
}

/// Серверные архивы весят сотни мегабайт — пишем сразу на диск и сообщаем, сколько скачано.
async fn download_big(state: &AppState, url: &str, bytes: &(dyn Fn(u64, u64) + Sync)) -> Result<std::path::PathBuf> {
    use tokio::io::AsyncWriteExt;
    let mut response = state.http.get(url).send().await?.error_for_status()?;
    let total = response.content_length().unwrap_or(0);
    let dir = state.paths.root.join("tmp");
    tokio::fs::create_dir_all(&dir).await?;
    let path = dir.join(format!("server-{}.zip", uuid::Uuid::new_v4()));
    let mut file = tokio::fs::File::create(&path).await?;
    let mut done = 0u64;
    let result: Result<()> = async {
        while let Some(chunk) = response.chunk().await? {
            file.write_all(&chunk).await?;
            done += chunk.len() as u64;
            bytes(done, total);
        }
        file.flush().await?;
        Ok(())
    }
    .await;
    drop(file);
    if result.is_err() {
        let _ = tokio::fs::remove_file(&path).await;
    }
    result.map(|()| path)
}

#[derive(serde::Deserialize)]
struct Release {
    tag_name: String,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    assets: Vec<Asset>,
}

#[derive(serde::Deserialize)]
struct Asset {
    name: String,
    browser_download_url: String,
}

/// `owner/repo` из ссылки на GitHub.
fn github_repo(url: &str) -> Option<String> {
    let rest = url.trim().trim_end_matches('/').split("github.com/").nth(1)?;
    let mut parts = rest.split('/');
    let (owner, repo) = (parts.next()?, parts.next()?.trim_end_matches(".git"));
    (!owner.is_empty() && !repo.is_empty()).then(|| format!("{owner}/{repo}"))
}

/// Номер версии из названия файла: «TerraFirmaGreg-Modern-0.13.17» → «0.13.17».
fn version_of(name: &str) -> Option<String> {
    let name = name.trim_end_matches(".zip");
    name.split(|c: char| c == '-' || c == '_' || c == ' ')
        .map(|t| t.trim_start_matches(['v', 'V']))
        .filter(|t| t.contains('.') && t.chars().next().is_some_and(|c| c.is_ascii_digit()))
        .last()
        .map(str::to_owned)
}

/// Серверный zip из релиза той же версии на GitHub, если репозиторий указан на странице модпака.
async fn github_server_zip(state: &AppState, file: &curseforge::File) -> Option<String> {
    let repo = curseforge::get_mod(&state.http, file.mod_id).await.ok()?.links?.source_url.as_deref().and_then(github_repo)?;
    let version = version_of(&file.display_name).or_else(|| version_of(&file.file_name))?;
    let releases: Vec<Release> = state
        .http
        .get(format!("https://api.github.com/repos/{repo}/releases?per_page=50"))
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .ok()?
        .error_for_status()
        .ok()?
        .json()
        .await
        .ok()?;
    pick_server_asset(&releases, &version)
}

fn pick_server_asset(releases: &[Release], version: &str) -> Option<String> {
    let same = |s: &str| s.trim_start_matches(['v', 'V']) == version;
    let release = releases.iter().find(|r| same(&r.tag_name) || r.name.as_deref().is_some_and(same))?;
    release
        .assets
        .iter()
        .find(|a| {
            let n = a.name.to_lowercase();
            n.ends_with(".zip") && n.contains("server")
        })
        .map(|a| a.browser_download_url.clone())
}

/// Распаковывает серверный архив. Часто всё лежит в одной папке внутри — её пропускаем.
fn unpack_server_zip(archive: &Path, dir: &Path) -> Result<()> {
    let mut zip = zip::ZipArchive::new(std::fs::File::open(archive)?)?;
    let names: Vec<String> = (0..zip.len()).filter_map(|i| zip.by_index(i).ok().map(|e| e.name().replace('\\', "/"))).collect();
    let root = common_root(&names);
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i)?;
        let name = entry.name().replace('\\', "/");
        let relative = root.as_deref().and_then(|r| name.strip_prefix(r)).unwrap_or(&name);
        if relative.is_empty() {
            continue;
        }
        let target = safe_join(dir, relative)?;
        if entry.is_dir() {
            std::fs::create_dir_all(&target)?;
            continue;
        }
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut bytes = Vec::with_capacity(entry.size() as usize);
        entry.read_to_end(&mut bytes)?;
        std::fs::write(target, bytes)?;
    }
    Ok(())
}

/// «Pack-1.2/» — если все записи архива лежат в одной папке.
fn common_root(names: &[String]) -> Option<String> {
    let first = names.first()?.split('/').next()?.to_owned();
    let prefix = format!("{first}/");
    names.iter().all(|n| n.starts_with(&prefix) || *n == first).then_some(prefix)
}

async fn modrinth_pack(state: &AppState, origin: &PackOrigin, dir: &Path, step: &(dyn Fn(usize, usize) + Sync)) -> Result<()> {
    let version = super::mrpack::pack_version(&state.http, &origin.project_id, origin.version_id.as_deref()).await?;
    let file = version.primary_file().ok_or_else(|| Error::msg("pack_not_found"))?;
    let archive = fetch_pack(state, &file.url).await?;
    let result = async {
        let mut zip = zip::ZipArchive::new(std::fs::File::open(&archive)?)?;
        let index: Index = {
            let mut entry = zip.by_name("modrinth.index.json").map_err(|_| Error::msg("pack_bad_manifest"))?;
            let mut text = String::new();
            entry.read_to_string(&mut text)?;
            serde_json::from_str(&text).map_err(|_| Error::msg("pack_bad_manifest"))?
        };
        let mut downloads = vec![];
        for f in &index.files {
            // Без пометки файл нужен везде; unsupported — только для клиента
            if f.env.as_ref().is_some_and(|e| e.server == "unsupported") {
                continue;
            }
            let Some(url) = f.downloads.first() else { continue };
            downloads.push(Download { url: url.clone(), path: safe_join(dir, &f.path)?, sha1: f.hashes.get("sha1").cloned(), size: Some(f.file_size) });
        }
        download_all(&state.http, downloads, |d, t| step(d, t)).await?;
        extract_overrides(&mut zip, "overrides", dir)?;
        extract_overrides(&mut zip, "server-overrides", dir)?;
        Ok(())
    }
    .await;
    let _ = std::fs::remove_file(&archive);
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_server_pack_on_github() {
        assert_eq!(github_repo("https://github.com/TerraFirmaGreg-Team/Modpack-Modern").as_deref(), Some("TerraFirmaGreg-Team/Modpack-Modern"));
        assert_eq!(github_repo("https://example.com/x"), None);
        assert_eq!(version_of("TerraFirmaGreg-Modern-0.13.17").as_deref(), Some("0.13.17"));
        assert_eq!(version_of("All the Mods 9-1.1.1").as_deref(), Some("1.1.1"));
        let asset = |name: &str| Asset { name: name.into(), browser_download_url: format!("https://x/{name}") };
        let releases = vec![
            Release { tag_name: "0.13.16".into(), name: None, assets: vec![asset("p-0.13.16-serverpack.zip")] },
            Release {
                tag_name: "v0.13.17".into(),
                name: None,
                assets: vec![asset("p-0.13.17-curseforge.zip"), asset("p-0.13.17-docker.yml"), asset("p-0.13.17-serverpack.zip")],
            },
        ];
        assert_eq!(pick_server_asset(&releases, "0.13.17").as_deref(), Some("https://x/p-0.13.17-serverpack.zip"));
        assert_eq!(pick_server_asset(&releases, "0.13.18"), None);
    }

    #[test]
    fn finds_common_root_folder() {
        let names = |l: &[&str]| l.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(common_root(&names(&["Pack/", "Pack/mods/a.jar", "Pack/config/x.toml"])), Some("Pack/".into()));
        assert_eq!(common_root(&names(&["mods/a.jar", "config/x.toml"])), None);
    }
}

#[cfg(test)]
mod live {
    use crate::content::curseforge;

    /// `cargo test --lib server_pack::live -- --ignored --nocapture` — нужен ключ CurseForge
    #[tokio::test]
    #[ignore]
    async fn curseforge_reports_server_packs() {
        let http = reqwest::Client::builder().user_agent("Shard-test").build().unwrap();
        // All the Mods 9
        let files = curseforge::files_page(&http, 715572, 0, 5).await.unwrap();
        let with_server: Vec<_> = files.iter().filter_map(|f| f.server_pack_file_id.map(|s| (f.display_name.clone(), s))).collect();
        println!("{with_server:?}");
        assert!(!with_server.is_empty());
        let (_, server_id) = &with_server[0];
        let server = curseforge::get_file(&http, 715572, *server_id).await.unwrap();
        println!("{} url={:?}", server.file_name, server.download_url);
    }
}
