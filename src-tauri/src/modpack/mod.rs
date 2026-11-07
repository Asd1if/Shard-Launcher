//! Сборки-модпаки: импорт и экспорт `.mrpack` (Modrinth) и zip CurseForge.
//!
//! Тип архива определяется по содержимому: `modrinth.index.json` или `manifest.json`.
//! При экспорте файлы, найденные на Modrinth или CurseForge, уходят ссылками, остальное — в `overrides/`.

mod curseforge;
mod mrpack;
pub mod server_pack;

use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};
use tauri::ipc::Channel;
use tauri::State;

use crate::content::{self, modrinth, ManualDownload, Source};
use crate::error::{Error, Result};
use crate::instances::{self, Instance, InstanceSettings, ModLoader, PackOrigin};
use crate::paths::Paths;
use crate::state::AppState;
use crate::versions::VersionType;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PackFormat {
    Modrinth,
    Curseforge,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase", tag = "stage")]
pub enum PackProgress {
    /// Скачивание архива по ссылке
    Fetch,
    Files { done: usize, total: usize },
    Overrides,
    /// Поиск файлов на Modrinth и CurseForge при экспорте
    Identify,
    Write { done: usize, total: usize },
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportResult {
    pub instance: Instance,
    pub format: PackFormat,
    /// Файлы CurseForge, которые нужно скачать вручную
    pub manual: Vec<ManualDownload>,
}

/// Что получилось прочитать из манифеста — общее для обоих форматов.
struct PackInfo {
    name: String,
    mc_version: String,
    loader: Option<ModLoader>,
    loader_version: Option<String>,
}

/// Путь из архива или манифеста внутри папки игры. Абсолютные пути и `..` отклоняются,
/// иначе чужой архив мог бы записать файл куда угодно.
pub(crate) fn safe_join(base: &Path, relative: &str) -> Result<PathBuf> {
    let relative = relative.replace('\\', "/");
    let path = Path::new(&relative);
    let mut out = base.to_path_buf();
    let mut any = false;
    for part in path.components() {
        match part {
            Component::Normal(p) => {
                out.push(p);
                any = true;
            }
            Component::CurDir => {}
            _ => return Err(Error::msg(format!("unsafe path in pack: {relative}"))),
        }
    }
    if !any {
        return Err(Error::msg(format!("unsafe path in pack: {relative}")));
    }
    Ok(out)
}

/// Распаковывает в папку игры всё, что лежит в архиве под `prefix/`. Возвращает пути распакованных файлов.
fn extract_overrides(archive: &mut zip::ZipArchive<std::fs::File>, prefix: &str, game_dir: &Path) -> Result<Vec<String>> {
    let prefix = format!("{}/", prefix.trim_end_matches('/'));
    let mut written = vec![];
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i)?;
        let name = entry.name().replace('\\', "/");
        let Some(relative) = name.strip_prefix(&prefix) else { continue };
        if relative.is_empty() {
            continue;
        }
        let target = safe_join(game_dir, relative)?;
        if entry.is_dir() {
            std::fs::create_dir_all(&target)?;
            continue;
        }
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut file = std::fs::File::create(&target)?;
        std::io::copy(&mut entry, &mut file)?;
        written.push(relative.to_owned());
    }
    Ok(written)
}

/// Файлы, которые поставил модпак, — при смене версии убираются только они, а не моды игрока.
fn pack_files_path(paths: &Paths, id: &str) -> Result<PathBuf> {
    Ok(paths.instance_dir(id)?.join("pack-files.json"))
}

pub(super) fn save_pack_files(paths: &Paths, id: &str, files: &[String]) -> Result<()> {
    std::fs::write(pack_files_path(paths, id)?, serde_json::to_vec(files)?)?;
    Ok(())
}

fn read_entry(archive: &mut zip::ZipArchive<std::fs::File>, name: &str) -> Result<Option<Vec<u8>>> {
    match archive.by_name(name) {
        Ok(mut entry) => {
            let mut bytes = Vec::with_capacity(entry.size() as usize);
            entry.read_to_end(&mut bytes)?;
            Ok(Some(bytes))
        }
        Err(zip::result::ZipError::FileNotFound) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

/// Создаёт пустую сборку под манифест, а при смене версии модпака — обновляет существующую.
/// Тип версии берём из списка Mojang, если он доступен.
async fn create_instance(state: &AppState, info: &PackInfo, target: Option<Instance>) -> Result<Instance> {
    let version_type = match crate::versions::manifest(state, false).await {
        Ok(m) => m.versions.into_iter().find(|v| v.id == info.mc_version).map_or(VersionType::Release, |v| v.kind),
        Err(_) => VersionType::Release,
    };
    if let Some(mut instance) = target {
        instance.mc_version = info.mc_version.clone();
        instance.version_type = version_type;
        instance.loader = info.loader;
        instance.loader_version = info.loader.and(info.loader_version.clone());
        instances::save(&state.paths, &instance)?;
        return Ok(instance);
    }
    let instance = Instance {
        id: uuid::Uuid::new_v4().to_string(),
        name: unique_name(&state.paths, &info.name),
        mc_version: info.mc_version.clone(),
        version_type,
        loader: info.loader,
        loader_version: info.loader.and(info.loader_version.clone()),
        mod_count: 0,
        last_played: None,
        created_at: chrono::Utc::now().to_rfc3339(),
        memory_mb: crate::settings::new_instance_memory(&state.paths, info.loader.is_some()),
        settings: InstanceSettings::default(),
        icon: None,
        group: None,
        favorite: false,
        pack: None,
    };
    state.paths.new_instance_dir(&instance.id, &instance.name);
    instances::save(&state.paths, &instance)?;
    std::fs::create_dir_all(state.paths.game_dir(&instance.id)?)?;
    Ok(instance)
}

/// «Имя», «Имя (2)», … — чтобы повторный импорт не путался с первым.
fn unique_name(paths: &Paths, name: &str) -> String {
    let name = if name.trim().is_empty() { "Modpack" } else { name.trim() };
    let taken: Vec<String> = instances::list(paths).unwrap_or_default().into_iter().map(|i| i.name).collect();
    if !taken.iter().any(|n| n == name) {
        return name.to_owned();
    }
    (2..).map(|n| format!("{name} ({n})")).find(|n| !taken.contains(n)).expect("infinite range")
}

/// Скачивает архив во временную папку.
async fn fetch_pack(state: &AppState, url: &str) -> Result<PathBuf> {
    let bytes = state.http.get(url).send().await?.error_for_status()?.bytes().await?;
    let dir = state.paths.root.join("tmp");
    std::fs::create_dir_all(&dir)?;
    let path = dir.join(format!("import-{}.zip", uuid::Uuid::new_v4()));
    std::fs::write(&path, &bytes)?;
    Ok(path)
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportArgs {
    /// Путь к файлу на диске
    pub path: Option<String>,
    /// Или ссылка: прямая на архив либо страница сборки на Modrinth
    pub url: Option<String>,
    /// Или проект-модпак CurseForge
    #[serde(default)]
    pub curseforge_id: Option<u64>,
    /// Выбранная версия: id версии Modrinth или id файла CurseForge; None — последняя
    #[serde(default)]
    pub version_id: Option<String>,
}

#[tauri::command]
pub async fn modpack_import(
    state: State<'_, AppState>,
    args: ImportArgs,
    progress: Channel<PackProgress>,
) -> Result<ImportResult> {
    run_import(&state, args, &progress).await
}

/// Откуда взять архив и что известно о модпаке заранее.
async fn resolve(
    state: &AppState,
    args: &ImportArgs,
    progress: &Channel<PackProgress>,
) -> Result<(PathBuf, bool, Option<PackOrigin>)> {
    if let Some(path) = &args.path {
        return Ok((PathBuf::from(path), false, None));
    }
    let _ = progress.send(PackProgress::Fetch);
    if let Some(url) = &args.url {
        let url = url.trim();
        let Some(slug) = mrpack::modrinth_project_slug(url) else {
            return Ok((fetch_pack(state, url).await?, true, None));
        };
        let version = mrpack::pack_version(&state.http, &slug, args.version_id.as_deref()).await?;
        let file = version.primary_file().ok_or_else(|| Error::msg("pack_not_found"))?;
        let path = fetch_pack(state, &file.url).await?;
        let icon_url = modrinth::project(&state.http, &version.project_id).await.ok().and_then(|p| p.icon_url);
        let origin = PackOrigin {
            source: Source::Modrinth,
            project_id: version.project_id.clone(),
            version_id: Some(version.id.clone()),
            version_name: Some(version.version_number.clone()),
            icon_url,
        };
        return Ok((path, true, Some(origin)));
    }
    if let Some(id) = args.curseforge_id {
        let file_id = args.version_id.as_deref().and_then(|v| v.parse().ok());
        let file = curseforge::pack_file(state, id, file_id).await?;
        // Автор запретил стороннюю загрузку — архив придётся скачать на сайте и импортировать файлом
        let url = file.download_url.clone().ok_or_else(|| Error::msg("pack_manual_download"))?;
        let path = fetch_pack(state, &url).await?;
        let icon_url = content::curseforge::get_mod(&state.http, id).await.ok().and_then(|m| m.logo?.thumbnail_url);
        let origin = PackOrigin {
            source: Source::Curseforge,
            project_id: id.to_string(),
            version_id: Some(file.id.to_string()),
            version_name: Some(file.display_name.clone()),
            icon_url,
        };
        return Ok((path, true, Some(origin)));
    }
    Err(Error::msg("nothing to import"))
}

/// Модпак по самому архиву: по SHA1 на Modrinth, по отпечатку на CurseForge.
/// Так и у импортированного файлом модпака будет оригинальная иконка.
async fn identify_pack(state: &AppState, path: &Path) -> Option<PackOrigin> {
    let file = path.to_path_buf();
    let (sha1, print) = tokio::task::spawn_blocking(move || {
        let bytes = std::fs::read(&file).ok()?;
        let sha1 = content::sha1_file(&file).ok()?;
        Some((sha1, content::curseforge::fingerprint(&bytes)))
    })
    .await
    .ok()??;
    let by_hash = modrinth::versions_by_hash(&state.http, std::slice::from_ref(&sha1)).await.ok();
    if let Some(version) = by_hash.and_then(|mut m| m.remove(&sha1)) {
        let icon_url = modrinth::project(&state.http, &version.project_id).await.ok().and_then(|p| p.icon_url);
        return Some(PackOrigin {
            source: Source::Modrinth,
            project_id: version.project_id,
            version_id: Some(version.id),
            version_name: Some(version.version_number),
            icon_url,
        });
    }
    if !content::curseforge::enabled() {
        return None;
    }
    let meta = content::curseforge::identify(&state.http, &[print]).await.ok()?.remove(&print)?;
    Some(PackOrigin {
        source: Source::Curseforge,
        project_id: meta.project_id,
        version_id: Some(meta.version_id),
        version_name: Some(meta.version_number),
        icon_url: meta.icon_url,
    })
}

async fn run_import(state: &AppState, args: ImportArgs, progress: &Channel<PackProgress>) -> Result<ImportResult> {
    let (path, temporary, origin) = resolve(state, &args, progress).await?;
    let mut result = import(state, &path, progress, None).await;
    if let Ok(imported) = &mut result {
        let origin = match origin {
            Some(o) => Some(o),
            None => identify_pack(state, &path).await,
        };
        // Иконка модпака — с его страницы на Modrinth или CurseForge; без неё сборка просто получит узор
        if let Some(url) = origin.as_ref().and_then(|o| o.icon_url.clone()) {
            imported.instance.icon = fetch_icon(state, &url).await;
        }
        imported.instance.pack = origin;
        let _ = instances::save(&state.paths, &imported.instance);
    }
    if temporary {
        let _ = std::fs::remove_file(&path);
    }
    result
}

/// Возвращает сборке оригинальную иконку модпака.
#[tauri::command]
pub async fn instance_original_icon(state: State<'_, AppState>, id: String) -> Result<Instance> {
    let mut instance = instances::load(&state.paths, &id)?;
    let url = instance.pack.as_ref().and_then(|p| p.icon_url.clone()).ok_or_else(|| Error::msg("pack_no_icon"))?;
    instance.icon = Some(fetch_icon(&state, &url).await.ok_or_else(|| Error::msg("pack_no_icon"))?);
    instances::save(&state.paths, &instance)?;
    Ok(instance)
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PackVersion {
    pub id: String,
    pub name: String,
    pub game_versions: Vec<String>,
    pub loaders: Vec<String>,
    pub date: String,
    /// release, beta или alpha
    pub kind: String,
    /// Автор CurseForge запретил стороннюю загрузку этого файла
    pub manual: bool,
}

/// Все версии модпака для выбора перед установкой.
#[tauri::command]
pub async fn modpack_versions(state: State<'_, AppState>, source: Source, project_id: String) -> Result<Vec<PackVersion>> {
    if source == Source::Curseforge {
        let id: u64 = project_id.parse().map_err(|_| Error::msg("pack_not_found"))?;
        let files = curseforge::pack_files(&state, id).await?;
        return Ok(files.into_iter().map(cf_version).collect());
    }
    let versions = mrpack::pack_versions(&state.http, &project_id).await?;
    Ok(versions
        .into_iter()
        .map(|v| PackVersion {
            name: if v.name.is_empty() { v.version_number.clone() } else { v.name.clone() },
            id: v.id,
            game_versions: v.game_versions,
            loaders: v.loaders,
            date: v.date_published,
            kind: v.version_type,
            manual: false,
        })
        .collect())
}

fn cf_version(f: content::curseforge::File) -> PackVersion {
    // В gameVersions CurseForge вперемешку версии игры, загрузчики и «Client»/«Server»
    let (game, other): (Vec<String>, Vec<String>) =
        f.game_versions.iter().cloned().partition(|v| v.starts_with(|c: char| c.is_ascii_digit()));
    let loaders = other.into_iter().map(|l| l.to_lowercase()).filter(|l| l != "client" && l != "server").collect();
    let kind = match f.release_type {
        1 => "release",
        2 => "beta",
        _ => "alpha",
    };
    PackVersion {
        id: f.id.to_string(),
        name: f.display_name,
        game_versions: game,
        loaders,
        date: f.file_date,
        kind: kind.to_owned(),
        manual: f.download_url.is_none(),
    }
}

/// Скачивает иконку в data URL. Только с CDN Modrinth и CurseForge и только небольшие картинки.
async fn fetch_icon(state: &AppState, url: &str) -> Option<String> {
    use base64::Engine;
    let parsed = reqwest::Url::parse(url).ok()?;
    let host = parsed.host_str()?;
    if !matches!(host, "cdn.modrinth.com" | "media.forgecdn.net") {
        return None;
    }
    let response = state.http.get(parsed).send().await.ok()?.error_for_status().ok()?;
    let mime = response.headers().get(reqwest::header::CONTENT_TYPE)?.to_str().ok()?.split(';').next()?.trim().to_owned();
    let bytes = response.bytes().await.ok()?.to_vec();
    // Анимированные логотипы CurseForge бывают по 4–5 МБ — ужимаем, сохраняя анимацию
    let (mime, bytes) = if bytes.len() * 4 / 3 > instances::MAX_ICON / 2 {
        tokio::task::spawn_blocking(move || crate::icon_shrink::shrink(&mime, &bytes)).await.ok()?.ok()?
    } else {
        (mime, bytes)
    };
    let icon = format!("data:{mime};base64,{}", base64::engine::general_purpose::STANDARD.encode(&bytes));
    instances::check_icon(&icon).ok().map(|_| icon)
}

async fn import(
    state: &AppState,
    path: &Path,
    progress: &Channel<PackProgress>,
    target: Option<Instance>,
) -> Result<ImportResult> {
    let mut archive = zip::ZipArchive::new(std::fs::File::open(path)?).map_err(|_| Error::msg("pack_unknown_format"))?;
    if let Some(index) = read_entry(&mut archive, "modrinth.index.json")? {
        return mrpack::import(state, archive, &index, progress, target).await;
    }
    if let Some(manifest) = read_entry(&mut archive, "manifest.json")? {
        return curseforge::import(state, archive, &manifest, progress, target).await;
    }
    Err(Error::msg("pack_unknown_format"))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PackSearchArgs {
    pub query: String,
    pub sort: String,
    pub offset: u32,
    #[serde(default)]
    pub source: Source,
    #[serde(default)]
    pub categories: Vec<String>,
    #[serde(default)]
    pub game_version: Option<String>,
    #[serde(default)]
    pub loaders: Vec<String>,
}

/// Поиск модпаков на Modrinth или CurseForge.
#[tauri::command]
pub async fn modpack_search(state: State<'_, AppState>, args: PackSearchArgs) -> Result<modrinth::SearchResult> {
    let query = args.query.trim();
    if args.source == Source::Curseforge {
        return content::curseforge::search_filtered(
            &state.http,
            query,
            content::curseforge::MODPACK_CLASS,
            args.game_version.as_deref().filter(|v| !v.is_empty()),
            match args.loaders.first().map(String::as_str) {
                Some("fabric") => Some(crate::instances::ModLoader::Fabric),
                Some("quilt") => Some(crate::instances::ModLoader::Quilt),
                Some("forge") => Some(crate::instances::ModLoader::Forge),
                Some("neoforge") => Some(crate::instances::ModLoader::Neoforge),
                _ => None,
            },
            &args.sort,
            args.offset,
            &args.categories,
        )
        .await;
    }
    modrinth::search(
        &state.http,
        &modrinth::SearchQuery {
            query,
            project_type: "modpack",
            game_version: args.game_version.as_deref().filter(|v| !v.is_empty()),
            loaders: &args.loaders.iter().map(String::as_str).collect::<Vec<_>>(),
            sort: &args.sort,
            offset: args.offset,
            limit: 20,
            categories: &args.categories,
            extra: &[],
        },
    )
    .await
}

/// Если что-то пошло не так посреди импорта, не оставляем полусобранную сборку.
/// Существующую сборку при смене версии не трогаем — её файлы вернёт `modpack_change_version`.
async fn cleanup_on_error<T>(paths: &Paths, instance: &Instance, result: Result<T>, fresh: bool) -> Result<T> {
    if result.is_err() && fresh {
        if let Ok(dir) = paths.instance_dir(&instance.id) {
            let _ = std::fs::remove_dir_all(dir);
        }
    }
    result
}

// ---------- смена версии модпака ----------

/// Переносит файлы прошлой версии модпака в `backups/pack-<дата>`, сохраняя пути.
/// Если список файлов не записан (сборка из старой версии Shard), убираем всю папку mods:
/// иначе старые и новые версии модов окажутся рядом и игра не запустится.
fn stash_pack_files(paths: &Paths, id: &str) -> Result<(PathBuf, Vec<String>)> {
    let game_dir = paths.game_dir(id)?;
    let files: Vec<String> = match std::fs::read(pack_files_path(paths, id)?) {
        Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_default(),
        Err(_) => std::fs::read_dir(game_dir.join("mods"))
            .map(|d| d.flatten().filter(|e| e.path().is_file()).map(|e| format!("mods/{}", e.file_name().to_string_lossy())).collect())
            .unwrap_or_default(),
    };
    let stash = paths.instance_dir(id)?.join("backups").join(format!("pack-{}", chrono::Local::now().format("%Y-%m-%d_%H-%M-%S")));
    let mut moved = vec![];
    for rel in files {
        let Ok(from) = safe_join(&game_dir, &rel) else { continue };
        // Выключенный игроком мод тоже убираем
        let from = if from.exists() { from } else { PathBuf::from(format!("{}.disabled", from.display())) };
        if !from.is_file() {
            continue;
        }
        let rel = from.strip_prefix(&game_dir).map(|p| p.to_string_lossy().replace('\\', "/")).unwrap_or(rel);
        let to = safe_join(&stash, &rel)?;
        if let Some(parent) = to.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::rename(&from, &to)?;
        moved.push(rel);
    }
    Ok((stash, moved))
}

/// Возвращает файлы прошлой версии на место, если новая не встала.
fn restore_pack_files(paths: &Paths, id: &str, stash: &Path, moved: &[String]) {
    let Ok(game_dir) = paths.game_dir(id) else { return };
    for rel in moved {
        if let (Ok(from), Ok(to)) = (safe_join(stash, rel), safe_join(&game_dir, rel)) {
            let _ = std::fs::rename(from, to);
        }
    }
    let _ = std::fs::remove_dir_all(stash);
}

/// Ставит другую версию модпака в ту же сборку: обновление или откат. Миры, настройки
/// и свои моды игрока остаются; файлы прошлой версии уходят в резервную копию сборки.
#[tauri::command]
pub async fn modpack_change_version(
    state: State<'_, AppState>,
    id: String,
    version_id: String,
    progress: Channel<PackProgress>,
) -> Result<ImportResult> {
    if state.running.lock().await.contains_key(&id) {
        return Err(Error::msg("stop the game before changing the modpack version"));
    }
    let instance = instances::load(&state.paths, &id)?;
    let origin = instance.pack.clone().ok_or_else(|| Error::msg("pack_not_linked"))?;
    let args = match origin.source {
        Source::Modrinth => ImportArgs {
            url: Some(format!("https://modrinth.com/modpack/{}", origin.project_id)),
            version_id: Some(version_id),
            ..Default::default()
        },
        Source::Curseforge => ImportArgs {
            curseforge_id: Some(origin.project_id.parse().map_err(|_| Error::msg("pack_not_found"))?),
            version_id: Some(version_id),
            ..Default::default()
        },
    };
    let (path, _, new_origin) = resolve(&state, &args, &progress).await?;
    crate::snapshots::auto(&state.paths, &id, "pack");

    let (stash, moved) = stash_pack_files(&state.paths, &id)?;
    let result = import(&state, &path, &progress, Some(instance.clone())).await;
    let _ = std::fs::remove_file(&path);
    match result {
        Ok(mut imported) => {
            if let Some(o) = new_origin {
                imported.instance.pack = Some(PackOrigin { icon_url: o.icon_url.or(origin.icon_url), ..o });
            }
            imported.instance.name = instance.name;
            imported.instance.icon = instance.icon;
            instances::save(&state.paths, &imported.instance)?;
            Ok(imported)
        }
        Err(e) => {
            restore_pack_files(&state.paths, &id, &stash, &moved);
            let _ = instances::save(&state.paths, &instance);
            Err(e)
        }
    }
}

// ---------- экспорт ----------

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportEntry {
    /// Имя файла или папки в корне папки игры
    pub name: String,
    pub is_dir: bool,
    pub size: u64,
    /// Включать по умолчанию: настройки и контент — да, миры, логи и кеши — нет
    pub suggested: bool,
}

/// Что обычно не кладут в сборку: личные данные игрока, логи и кеши.
const SKIP_BY_DEFAULT: &[&str] = &[
    "saves",
    "logs",
    "crash-reports",
    "screenshots",
    "downloads",
    ".cache",
    ".fabric",
    ".quilt",
    ".mixin.out",
    "usercache.json",
    "usernamecache.json",
    "servers.dat",
    "servers.dat_old",
    "command_history.txt",
    "realms_persistence.json",
    "patchouli_books",
    "journeymap",
    "xaero",
];

fn dir_size(path: &Path) -> u64 {
    let Ok(entries) = std::fs::read_dir(path) else { return 0 };
    entries
        .flatten()
        .map(|e| match e.metadata() {
            Ok(m) if m.is_dir() => dir_size(&e.path()),
            Ok(m) => m.len(),
            Err(_) => 0,
        })
        .sum()
}

#[tauri::command]
pub async fn modpack_export_entries(state: State<'_, AppState>, id: String) -> Result<Vec<ExportEntry>> {
    run_entries(&state, &id).await
}

async fn run_entries(state: &AppState, id: &str) -> Result<Vec<ExportEntry>> {
    let game_dir = state.paths.game_dir(id)?;
    tokio::task::spawn_blocking(move || -> Result<Vec<ExportEntry>> {
        let mut out = vec![];
        let Ok(entries) = std::fs::read_dir(&game_dir) else { return Ok(out) };
        for entry in entries.flatten() {
            let Ok(meta) = entry.metadata() else { continue };
            let name = entry.file_name().to_string_lossy().into_owned();
            let size = if meta.is_dir() { dir_size(&entry.path()) } else { meta.len() };
            let suggested = !SKIP_BY_DEFAULT.contains(&name.as_str()) && !name.ends_with(".log");
            out.push(ExportEntry { name, is_dir: meta.is_dir(), size, suggested });
        }
        out.sort_by(|a, b| b.is_dir.cmp(&a.is_dir).then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase())));
        Ok(out)
    })
    .await
    .map_err(|e| Error::msg(e.to_string()))?
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportArgs {
    pub format: PackFormat,
    /// Куда сохранить архив
    pub path: String,
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub summary: String,
    /// Выбранные файлы и папки из корня папки игры
    pub include: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportResult {
    /// Файлов, ушедших ссылками на Modrinth или CurseForge
    pub linked: usize,
    /// Файлов, положенных в архив целиком
    pub bundled: usize,
    pub size: u64,
}

/// Файл папки игры, попадающий в сборку.
pub(crate) struct PackFile {
    /// Путь относительно папки игры, через `/`
    pub relative: String,
    pub path: PathBuf,
}

/// Все файлы выбранных записей, рекурсивно.
fn collect_files(game_dir: &Path, include: &[String]) -> Result<Vec<PackFile>> {
    fn walk(dir: &Path, relative: &str, out: &mut Vec<PackFile>) {
        let Ok(entries) = std::fs::read_dir(dir) else { return };
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            let rel = format!("{relative}/{name}");
            match entry.file_type() {
                Ok(t) if t.is_dir() => walk(&entry.path(), &rel, out),
                Ok(t) if t.is_file() => out.push(PackFile { relative: rel, path: entry.path() }),
                _ => {}
            }
        }
    }
    let mut out = vec![];
    for name in include {
        let path = safe_join(game_dir, name)?;
        if path.is_dir() {
            walk(&path, name, &mut out);
        } else if path.is_file() {
            out.push(PackFile { relative: name.clone(), path });
        }
    }
    out.sort_by(|a, b| a.relative.cmp(&b.relative));
    Ok(out)
}

/// Версия загрузчика для манифеста: из сборки, а если её ещё не выбирали — рекомендованная.
async fn resolved_loader_version(state: &AppState, instance: &Instance) -> Result<Option<String>> {
    let Some(loader) = instance.loader else { return Ok(None) };
    if let Some(v) = &instance.loader_version {
        return Ok(Some(v.clone()));
    }
    let list = crate::loaders::versions(state, loader, &instance.mc_version).await?;
    crate::loaders::pick_default(&list)
        .map(|v| Some(v.version.clone()))
        .ok_or_else(|| Error::msg("loader_unavailable"))
}

/// Пишет zip: сначала манифест, потом файлы под `overrides/`.
fn write_zip(
    target: &Path,
    manifest_name: &str,
    manifest: &[u8],
    overrides: &[&PackFile],
    progress: &Channel<PackProgress>,
) -> Result<u64> {
    let tmp = target.with_extension("part");
    let file = std::fs::File::create(&tmp)?;
    let mut zip = zip::ZipWriter::new(file);
    let options = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    zip.start_file(manifest_name, options)?;
    zip.write_all(manifest)?;
    let total = overrides.len();
    for (i, f) in overrides.iter().enumerate() {
        zip.start_file(format!("overrides/{}", f.relative), options)?;
        let mut source = std::fs::File::open(&f.path)?;
        std::io::copy(&mut source, &mut zip)?;
        if i % 16 == 0 || i + 1 == total {
            let _ = progress.send(PackProgress::Write { done: i + 1, total });
        }
    }
    zip.finish()?;
    std::fs::rename(&tmp, target)?;
    Ok(std::fs::metadata(target)?.len())
}

#[tauri::command]
pub async fn modpack_export(
    state: State<'_, AppState>,
    id: String,
    args: ExportArgs,
    progress: Channel<PackProgress>,
) -> Result<ExportResult> {
    run_export(&state, &id, args, &progress).await
}

async fn run_export(state: &AppState, id: &str, args: ExportArgs, progress: &Channel<PackProgress>) -> Result<ExportResult> {
    let instance = instances::load(&state.paths, id)?;
    let game_dir = state.paths.game_dir(id)?;
    let files = {
        let game_dir = game_dir.clone();
        let include = args.include.clone();
        tokio::task::spawn_blocking(move || collect_files(&game_dir, &include))
            .await
            .map_err(|e| Error::msg(e.to_string()))??
    };
    let loader_version = resolved_loader_version(state, &instance).await?;
    let _ = progress.send(PackProgress::Identify);
    match args.format {
        PackFormat::Modrinth => mrpack::export(state, &instance, loader_version, &args, files, progress).await,
        PackFormat::Curseforge => curseforge::export(state, &instance, loader_version, &args, files, progress).await,
    }
}

/// Файлы контента (mods/, resourcepacks/, shaderpacks/), которые можно отдать ссылкой.
/// Отключённые файлы остаются в архиве как есть, чтобы не включиться у игрока сами.
pub(crate) fn linkable(file: &PackFile) -> bool {
    let Some((folder, rest)) = file.relative.split_once('/') else { return false };
    !rest.contains('/')
        && !rest.ends_with(".disabled")
        && content::ContentKind::ALL.iter().any(|k| k.folder() == folder)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Ходит в Modrinth: cargo test modpack_roundtrip -- --ignored --nocapture
    /// Импорт сборки по ссылке, экспорт в .mrpack и CurseForge zip, повторный импорт своего .mrpack.
    #[tokio::test]
    #[ignore]
    async fn modpack_roundtrip() {
        let root = std::env::temp_dir().join("shard-modpack-test");
        let _ = std::fs::remove_dir_all(&root);
        let state = AppState::new(root.clone());
        let progress: Channel<PackProgress> = Channel::new(|_| Ok(()));

        let url = std::env::var("SHARD_TEST_PACK").unwrap_or("https://modrinth.com/modpack/fabulously-optimized".into());
        let imported = run_import(&state, ImportArgs { path: None, url: Some(url), ..Default::default() }, &progress).await.unwrap();
        let instance = imported.instance;
        println!("imported {} {} {:?} {:?}, mods: {}", instance.name, instance.mc_version, instance.loader, instance.loader_version, instance.mod_count);
        assert!(instance.mod_count > 0);
        // Иконка модпака приходит со страницы проекта на Modrinth
        assert!(instance.icon.as_deref().is_some_and(|i| i.starts_with("data:image/")), "no pack icon");

        let include: Vec<String> = run_entries(&state, &instance.id).await.unwrap().into_iter().filter(|e| e.suggested).map(|e| e.name).collect();
        println!("include: {include:?}");
        let target = root.join("out.mrpack");
        let args = |format, path: &Path| ExportArgs {
            format,
            path: path.to_string_lossy().into_owned(),
            name: "Roundtrip".into(),
            version: "1.0".into(),
            author: "test".into(),
            summary: String::new(),
            include: include.clone(),
        };
        let exported = run_export(&state, &instance.id, args(PackFormat::Modrinth, &target), &progress).await.unwrap();
        println!("mrpack: linked {}, bundled {}, {} bytes", exported.linked, exported.bundled, exported.size);
        assert!(exported.linked > 0);

        let cf = root.join("out.zip");
        let exported = run_export(&state, &instance.id, args(PackFormat::Curseforge, &cf), &progress).await.unwrap();
        println!("curseforge: linked {}, bundled {}, {} bytes", exported.linked, exported.bundled, exported.size);

        let again = run_import(&state, ImportArgs { path: Some(target.to_string_lossy().into_owned()), url: None, ..Default::default() }, &progress).await.unwrap();
        println!("re-imported {} mods: {}", again.instance.name, again.instance.mod_count);
        assert_eq!(again.instance.mod_count, instance.mod_count);
        assert_eq!(again.instance.loader_version, instance.loader_version);

        // Без ключа CurseForge всё лежит в overrides, и такой архив импортируется тоже без ключа
        if !crate::content::curseforge::enabled() {
            let cf_again = run_import(&state, ImportArgs { path: Some(cf.to_string_lossy().into_owned()), url: None, ..Default::default() }, &progress).await.unwrap();
            assert_eq!(cf_again.format, PackFormat::Curseforge);
            assert_eq!(cf_again.instance.mod_count, instance.mod_count);
            assert_eq!(cf_again.instance.loader_version, instance.loader_version);
        }
    }

    #[test]
    fn safe_join_rejects_escapes() {
        let base = Path::new("/game");
        assert!(safe_join(base, "mods/a.jar").is_ok());
        assert!(safe_join(base, "config\\x.toml").is_ok());
        assert!(safe_join(base, "../a.jar").is_err());
        assert!(safe_join(base, "mods/../../a.jar").is_err());
        assert!(safe_join(base, "/etc/passwd").is_err());
        assert!(safe_join(base, "").is_err());
        #[cfg(windows)]
        assert!(safe_join(base, "C:/Windows/a.dll").is_err());
    }

    #[test]
    fn linkable_only_top_level_content() {
        let f = |r: &str| PackFile { relative: r.into(), path: PathBuf::new() };
        assert!(linkable(&f("mods/sodium.jar")));
        assert!(linkable(&f("shaderpacks/bsl.zip")));
        assert!(!linkable(&f("mods/sodium.jar.disabled")));
        assert!(!linkable(&f("mods/sub/x.jar")));
        assert!(!linkable(&f("config/sodium.json")));
        assert!(!linkable(&f("options.txt")));
    }
}
