//! Контент сборки: моды, ресурспаки и шейдеры.
//!
//! Источник правды — файлы в папке игры. Сведения о них (проект, версия, название, иконка)
//! лежат в `instances/<id>/content.json` по SHA1, так что переименование или отключение
//! файла (`.disabled`) их не теряет. Файлы, поставленные вручную, опознаются по хешу через Modrinth.

pub mod curseforge;
pub mod details;
pub mod modrinth;
pub mod optimize;

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use serde::{Deserialize, Serialize};
use sha1::{Digest, Sha1};
use tauri::{AppHandle, Manager, State};

use crate::download::{download_one, Download};
use crate::error::{Error, Result};
use crate::instances::{self, Instance, ModLoader};
use crate::paths::Paths;
use crate::state::AppState;

const DISABLED: &str = ".disabled";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ContentKind {
    Mod,
    Resourcepack,
    Shader,
}

impl Meta {
    /// Ключ проекта без пересечений между источниками
    fn project_key(&self) -> String {
        project_key(self.source, &self.project_id)
    }
}

pub(crate) fn project_key(source: Source, id: &str) -> String {
    match source {
        Source::Modrinth => id.to_owned(),
        Source::Curseforge => format!("cf:{id}"),
    }
}

impl ContentKind {
    pub(crate) const ALL: [ContentKind; 3] = [ContentKind::Mod, ContentKind::Resourcepack, ContentKind::Shader];

    pub(crate) fn folder(self) -> &'static str {
        match self {
            ContentKind::Mod => "mods",
            ContentKind::Resourcepack => "resourcepacks",
            ContentKind::Shader => "shaderpacks",
        }
    }

    fn extension(self) -> &'static str {
        match self {
            ContentKind::Mod => ".jar",
            _ => ".zip",
        }
    }

    fn modrinth_type(self) -> &'static str {
        match self {
            ContentKind::Mod => "mod",
            ContentKind::Resourcepack => "resourcepack",
            ContentKind::Shader => "shader",
        }
    }
}

/// Загрузчики Modrinth, под которые искать версии для сборки.
pub(crate) fn modrinth_loaders(kind: ContentKind, loader: Option<ModLoader>) -> Vec<&'static str> {
    match kind {
        ContentKind::Mod => match loader {
            Some(ModLoader::Fabric) => vec!["fabric"],
            // Quilt запускает и моды Fabric
            Some(ModLoader::Quilt) => vec!["quilt", "fabric"],
            Some(ModLoader::Forge) => vec!["forge"],
            Some(ModLoader::Neoforge) => vec!["neoforge"],
            None => vec![],
        },
        ContentKind::Resourcepack => vec!["minecraft"],
        // У шейдеров «загрузчик» — это Iris, OptiFine и т. п.; фильтровать по нему не нужно
        ContentKind::Shader => vec![],
    }
}

/// Откуда контент: у каждого источника свои id проектов и версий.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Source {
    #[default]
    Modrinth,
    Curseforge,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Meta {
    #[serde(default)]
    pub source: Source,
    pub project_id: String,
    pub version_id: String,
    pub title: String,
    pub version_number: String,
    pub icon_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CachedHash {
    size: u64,
    modified: u64,
    sha1: String,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub(crate) struct Index {
    /// SHA1 → сведения; None — файл проверяли, его нет ни на Modrinth, ни на CurseForge
    #[serde(default)]
    pub(crate) known: HashMap<String, Option<Meta>>,
    /// SHA1 файлов, которые уже искали на CurseForge (чтобы не спрашивать каждый раз)
    #[serde(default)]
    cf_checked: HashSet<String>,
    /// Хеши по пути файла, чтобы не перечитывать сотни модов при каждом открытии
    #[serde(default)]
    hashes: HashMap<String, CachedHash>,
}

fn index_file(paths: &Paths, id: &str) -> Result<PathBuf> {
    Ok(paths.instance_dir(id)?.join("content.json"))
}

pub(crate) fn load_index(paths: &Paths, id: &str) -> Result<Index> {
    match std::fs::read(index_file(paths, id)?) {
        Ok(bytes) => Ok(serde_json::from_slice(&bytes).unwrap_or_default()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Index::default()),
        Err(e) => Err(e.into()),
    }
}

pub(crate) fn save_index(paths: &Paths, id: &str, index: &Index) -> Result<()> {
    std::fs::write(index_file(paths, id)?, serde_json::to_vec(index)?)?;
    Ok(())
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstalledContent {
    pub kind: ContentKind,
    /// Имя файла на диске (с .disabled, если отключён)
    pub file: String,
    pub enabled: bool,
    pub size: u64,
    pub sha1: String,
    pub meta: Option<Meta>,
}

struct DiskFile {
    kind: ContentKind,
    file: String,
    path: PathBuf,
    size: u64,
    modified: u64,
}

fn scan(game_dir: &Path) -> Vec<DiskFile> {
    let mut out = vec![];
    for kind in ContentKind::ALL {
        let Ok(entries) = std::fs::read_dir(game_dir.join(kind.folder())) else { continue };
        for entry in entries.flatten() {
            let Ok(meta) = entry.metadata() else { continue };
            if !meta.is_file() {
                continue;
            }
            let file = entry.file_name().to_string_lossy().into_owned();
            let base = file.strip_suffix(DISABLED).unwrap_or(&file);
            if !base.to_ascii_lowercase().ends_with(kind.extension()) {
                continue;
            }
            let modified = meta
                .modified()
                .ok()
                .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                .map_or(0, |d| d.as_secs());
            out.push(DiskFile { kind, path: entry.path(), file, size: meta.len(), modified });
        }
    }
    out
}

pub(crate) fn sha1_file(path: &Path) -> Result<String> {
    let mut hasher = Sha1::new();
    let mut file = std::fs::File::open(path)?;
    std::io::copy(&mut file, &mut hasher)?;
    Ok(hex::encode(hasher.finalize()))
}

/// Список контента сборки. Новые файлы хешируются и опознаются через Modrinth.
pub async fn list(state: &AppState, id: &str) -> Result<Vec<InstalledContent>> {
    list_with(state, id, false).await
}

/// `quick` — сразу, без хеширования новых файлов и без сети: только то, что уже известно.
/// Так весь список виден мгновенно, а названия новых модов подтягиваются следующим вызовом.
async fn list_with(state: &AppState, id: &str, quick: bool) -> Result<Vec<InstalledContent>> {
    let game_dir = state.paths.game_dir(id)?;
    let mut index = load_index(&state.paths, id)?;

    let files = {
        let game_dir = game_dir.clone();
        let cached = std::mem::take(&mut index.hashes);
        tokio::task::spawn_blocking(move || -> Result<(Vec<(DiskFile, String)>, HashMap<String, CachedHash>)> {
            let mut hashes = HashMap::new();
            let mut out = vec![];
            for f in scan(&game_dir) {
                // Ключ без .disabled: включение и отключение не меняют содержимое
                let key = format!("{}/{}", f.kind.folder(), f.file.strip_suffix(DISABLED).unwrap_or(&f.file));
                let sha1 = match cached.get(&key) {
                    Some(c) if c.size == f.size && c.modified == f.modified => c.sha1.clone(),
                    _ if quick => String::new(),
                    _ => sha1_file(&f.path)?,
                };
                hashes.insert(key, CachedHash { size: f.size, modified: f.modified, sha1: sha1.clone() });
                out.push((f, sha1));
            }
            Ok((out, hashes))
        })
        .await
        .map_err(|e| Error::msg(e.to_string()))??
    };
    let (files, hashes) = files;
    if quick {
        return Ok(collect(files, &index));
    }
    index.hashes = hashes;

    let unknown: Vec<String> = files
        .iter()
        .map(|(_, sha1)| sha1.clone())
        .filter(|sha1| !index.known.contains_key(sha1))
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();
    // Без интернета список всё равно показываем — просто без названий
    if !unknown.is_empty() {
        if let Ok(found) = identify(&state.http, &unknown).await {
            for sha1 in &unknown {
                index.known.insert(sha1.clone(), found.get(sha1).cloned());
            }
        }
    }

    // Что не нашлось на Modrinth — ищем на CurseForge по отпечатку
    if curseforge::enabled() {
        let candidates: Vec<(String, PathBuf)> = files
            .iter()
            .filter(|(_, sha1)| matches!(index.known.get(sha1), Some(None)) && !index.cf_checked.contains(sha1))
            .map(|(f, sha1)| (sha1.clone(), f.path.clone()))
            .collect();
        if !candidates.is_empty() {
            let prints = tokio::task::spawn_blocking(move || {
                candidates
                    .into_iter()
                    .filter_map(|(sha1, path)| std::fs::read(path).ok().map(|b| (sha1, curseforge::fingerprint(&b))))
                    .collect::<Vec<_>>()
            })
            .await
            .map_err(|e| Error::msg(e.to_string()))?;
            let values: Vec<u32> = prints.iter().map(|(_, p)| *p).collect();
            if let Ok(found) = curseforge::identify(&state.http, &values).await {
                for (sha1, print) in prints {
                    if let Some(meta) = found.get(&print) {
                        index.known.insert(sha1.clone(), Some(meta.clone()));
                    }
                    index.cf_checked.insert(sha1);
                }
            }
        }
    }
    save_index(&state.paths, id, &index)?;
    Ok(collect(files, &index))
}

fn collect(files: Vec<(DiskFile, String)>, index: &Index) -> Vec<InstalledContent> {
    let mut out: Vec<InstalledContent> = files
        .into_iter()
        .map(|(f, sha1)| InstalledContent {
            kind: f.kind,
            enabled: !f.file.ends_with(DISABLED),
            size: f.size,
            meta: index.known.get(&sha1).cloned().flatten(),
            sha1,
            file: f.file,
        })
        .collect();
    out.sort_by_cached_key(|c| {
        (c.kind as u8, c.meta.as_ref().map_or_else(|| c.file.to_lowercase(), |m| m.title.to_lowercase()))
    });
    out
}

async fn identify(http: &reqwest::Client, hashes: &[String]) -> Result<HashMap<String, Meta>> {
    let versions = modrinth::versions_by_hash(http, hashes).await?;
    let ids: Vec<String> =
        versions.values().map(|v| v.project_id.clone()).collect::<HashSet<_>>().into_iter().collect();
    let projects: HashMap<String, modrinth::Project> =
        modrinth::projects(http, &ids).await?.into_iter().map(|p| (p.id.clone(), p)).collect();
    Ok(versions
        .into_iter()
        .map(|(sha1, v)| {
            let project = projects.get(&v.project_id);
            let meta = Meta {
                source: Source::Modrinth,
                title: project.map_or_else(|| v.project_id.clone(), |p| p.title.clone()),
                icon_url: project.and_then(|p| p.icon_url.clone()),
                project_id: v.project_id,
                version_id: v.id,
                version_number: v.version_number,
            };
            (sha1, meta)
        })
        .collect())
}

/// Число включённых модов — показывается в списке сборок.
pub(crate) fn refresh_mod_count(paths: &Paths, instance: &mut Instance) -> Result<()> {
    let game_dir = paths.game_dir(&instance.id)?;
    let count = scan(&game_dir).iter().filter(|f| f.kind == ContentKind::Mod && !f.file.ends_with(DISABLED)).count();
    instance.mod_count = count as u32;
    instances::save(paths, instance)
}

/// Имя файла из фронтенда: только имя, без путей.
pub(crate) fn checked_name(file: &str) -> Result<&str> {
    if file.is_empty() || file.contains(['/', '\\']) || file == "." || file == ".." {
        return Err(Error::msg(format!("invalid file name: {file}")));
    }
    Ok(file)
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Installed {
    pub title: String,
    pub version_number: String,
    /// Поставлен как зависимость или как Iris для шейдеров
    pub dependency: bool,
}

/// Ставит версию проекта и рекурсивно — её обязательные зависимости.
async fn install_version(
    state: &AppState,
    instance: &Instance,
    kind: ContentKind,
    version: modrinth::Version,
    dependency: bool,
    installed_projects: &mut HashSet<String>,
    out: &mut Vec<Installed>,
) -> Result<()> {
    installed_projects.insert(project_key(Source::Modrinth, &version.project_id));
    let file = version.primary_file().ok_or_else(|| modrinth::not_found(&version.project_id))?.clone();
    let filename = checked_name(&file.filename)?.to_owned();
    let dir = state.paths.game_dir(&instance.id)?.join(kind.folder());
    download_one(
        &state.http,
        &Download {
            url: file.url.clone(),
            path: dir.join(&filename),
            sha1: file.hashes.get("sha1").cloned(),
            size: Some(file.size),
        },
    )
    .await?;

    let project = modrinth::project(&state.http, &version.project_id).await?;
    if let Some(sha1) = file.hashes.get("sha1") {
        let mut index = load_index(&state.paths, &instance.id)?;
        index.known.insert(
            sha1.clone(),
            Some(Meta {
                source: Source::Modrinth,
                project_id: version.project_id.clone(),
                version_id: version.id.clone(),
                title: project.title.clone(),
                version_number: version.version_number.clone(),
                icon_url: project.icon_url.clone(),
            }),
        );
        save_index(&state.paths, &instance.id, &index)?;
    }
    out.push(Installed { title: project.title, version_number: version.version_number.clone(), dependency });

    let loaders = modrinth_loaders(ContentKind::Mod, instance.loader);
    for dep in version.dependencies.iter().filter(|d| d.dependency_type == "required") {
        if dep.project_id.as_ref().is_some_and(|p| installed_projects.contains(p)) {
            continue;
        }
        let dep_version = match (&dep.version_id, &dep.project_id) {
            (Some(id), _) => modrinth::version(&state.http, id).await?,
            (None, Some(project)) => {
                let list = modrinth::project_versions(&state.http, project, &loaders, &instance.mc_version).await?;
                match modrinth::best(list) {
                    Some(v) => v,
                    None => return Err(modrinth::not_found(project)),
                }
            }
            (None, None) => continue,
        };
        if installed_projects.contains(&dep_version.project_id) {
            continue;
        }
        Box::pin(install_version(state, instance, ContentKind::Mod, dep_version, true, installed_projects, out)).await?;
    }
    Ok(())
}

/// Проекты, которые уже стоят в сборке (по данным content.json).
/// Ставит мод с Modrinth в сборку, если его там ещё нет. true — мод в сборке.
pub(crate) async fn ensure_mod(state: &AppState, id: &str, project: &str, file_hint: &str) -> Result<bool> {
    let mut instance = instances::load(&state.paths, id)?;
    if instance.loader.is_none() {
        return Ok(false);
    }
    // Быстрая проверка без сети: файл мода уже лежит в mods
    let mods = state.paths.game_dir(id)?.join("mods");
    let hint = file_hint.to_lowercase();
    let present = std::fs::read_dir(&mods)
        .into_iter()
        .flatten()
        .flatten()
        .any(|e| e.file_name().to_string_lossy().to_lowercase().contains(&hint));
    if present {
        return Ok(true);
    }
    let loaders = modrinth_loaders(ContentKind::Mod, instance.loader);
    let versions = modrinth::project_versions(&state.http, project, &loaders, &instance.mc_version).await?;
    let Some(version) = modrinth::best(versions) else { return Ok(false) };
    let mut projects = installed_project_ids(state, id).await?;
    if projects.contains(&version.project_id) {
        return Ok(true);
    }
    let mut out = vec![];
    install_version(state, &instance, ContentKind::Mod, version, false, &mut projects, &mut out).await?;
    refresh_mod_count(&state.paths, &mut instance)?;
    Ok(true)
}

async fn installed_project_ids(state: &AppState, id: &str) -> Result<HashSet<String>> {
    Ok(list(state, id).await?.into_iter().filter_map(|c| c.meta.map(|m| m.project_key())).collect())
}

/// Файл CurseForge, который автор запретил качать из сторонних программ:
/// пользователь скачивает его в браузере, а Shard забирает из папки «Загрузки».
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ManualDownload {
    pub kind: ContentKind,
    pub file_name: String,
    pub url: String,
    pub sha1: Option<String>,
    pub size: u64,
    pub meta: Meta,
}

pub(crate) fn remember(paths: &Paths, id: &str, sha1: &str, meta: Meta) -> Result<()> {
    let mut index = load_index(paths, id)?;
    index.known.insert(sha1.to_owned(), Some(meta));
    save_index(paths, id, &index)
}

/// Ставит файл CurseForge и рекурсивно — его обязательные зависимости.
#[allow(clippy::too_many_arguments)]
async fn install_cf_file(
    state: &AppState,
    instance: &Instance,
    kind: ContentKind,
    project: curseforge::Mod,
    file: curseforge::File,
    dependency: bool,
    projects: &mut HashSet<String>,
    out: &mut InstallResult,
) -> Result<()> {
    projects.insert(project_key(Source::Curseforge, &project.id.to_string()));
    let filename = checked_name(&file.file_name)?.to_owned();
    let meta = Meta {
        source: Source::Curseforge,
        project_id: project.id.to_string(),
        version_id: file.id.to_string(),
        title: project.name.clone(),
        version_number: file.display_name.clone(),
        icon_url: project.logo.as_ref().and_then(|l| l.thumbnail_url.clone()),
    };
    match &file.download_url {
        Some(url) => {
            let path = state.paths.game_dir(&instance.id)?.join(kind.folder()).join(&filename);
            download_one(
                &state.http,
                &Download { url: url.clone(), path, sha1: file.sha1(), size: Some(file.file_length) },
            )
            .await?;
            if let Some(sha1) = file.sha1() {
                remember(&state.paths, &instance.id, &sha1, meta)?;
            }
            out.installed.push(Installed { title: project.name.clone(), version_number: file.display_name.clone(), dependency });
        }
        None => {
            let page = project.links.as_ref().and_then(|l| l.website_url.as_deref());
            let url = curseforge::manual_url(page, file.id)
                .ok_or_else(|| Error::msg(format!("no download page for {}", project.name)))?;
            out.manual.push(ManualDownload { kind, file_name: filename, url, sha1: file.sha1(), size: file.file_length, meta });
        }
    }

    for dep_id in file.required().collect::<Vec<_>>() {
        if projects.contains(&project_key(Source::Curseforge, &dep_id.to_string())) {
            continue;
        }
        let list = curseforge::files(&state.http, dep_id, ContentKind::Mod, &instance.mc_version, instance.loader).await?;
        let Some(dep_file) = curseforge::best(list) else { return Err(modrinth::not_found(&dep_id.to_string())) };
        let dep_project = curseforge::get_mod(&state.http, dep_id).await?;
        Box::pin(install_cf_file(state, instance, ContentKind::Mod, dep_project, dep_file, true, projects, out)).await?;
    }
    Ok(())
}

#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallResult {
    pub installed: Vec<Installed>,
    /// Файлы, которые нужно скачать вручную
    pub manual: Vec<ManualDownload>,
}

// ---------- команды ----------

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchArgs {
    pub query: String,
    pub kind: ContentKind,
    /// Сборка, под которую ищем: подставляет версию игры и загрузчик
    pub instance_id: Option<String>,
    pub sort: String,
    pub offset: u32,
    #[serde(default)]
    pub source: Source,
    /// Не фильтровать по версии игры (загрузчик модов всё равно учитывается)
    #[serde(default)]
    pub any_version: bool,
    /// Категории: slug на Modrinth, id на CurseForge
    #[serde(default)]
    pub categories: Vec<String>,
    /// Своя версия игры вместо версии сборки
    #[serde(default)]
    pub game_version: Option<String>,
    /// Свои загрузчики вместо загрузчика сборки (fabric, forge, iris, optifine…)
    #[serde(default)]
    pub loaders: Vec<String>,
    /// client, server или both
    #[serde(default)]
    pub environment: Option<String>,
}

impl SearchArgs {
    /// Версия игры для поиска: выбранная вручную, иначе версия сборки (если не «любая»)
    fn version<'a>(&'a self, instance: Option<&'a instances::Instance>) -> Option<&'a str> {
        match self.game_version.as_deref().filter(|v| !v.is_empty()) {
            Some(v) => Some(v),
            None => instance.filter(|_| !self.any_version).map(|i| i.mc_version.as_str()),
        }
    }
}

/// Загрузчик CurseForge по имени из фильтра.
fn loader_by_name(name: &str) -> Option<ModLoader> {
    match name {
        "fabric" => Some(ModLoader::Fabric),
        "quilt" => Some(ModLoader::Quilt),
        "forge" => Some(ModLoader::Forge),
        "neoforge" => Some(ModLoader::Neoforge),
        _ => None,
    }
}

#[derive(Serialize, Clone)]
pub struct Category {
    id: String,
    name: String,
    /// Группа на Modrinth: categories, features, resolutions, performance impact
    group: Option<String>,
}

/// Категории для фильтра поиска: `project_type` — mod, resourcepack, shader, modpack или plugin.
#[tauri::command]
pub async fn content_categories(state: State<'_, AppState>, source: Source, project_type: String) -> Result<Vec<Category>> {
    if source == Source::Curseforge {
        let class = match project_type.as_str() {
            "resourcepack" => 12,
            "shader" => 6552,
            "modpack" => curseforge::MODPACK_CLASS,
            "plugin" => 5,
            _ => 6,
        };
        return Ok(curseforge::categories(&state.http, class)
            .await?
            .into_iter()
            .map(|(id, name)| Category { id: id.to_string(), name, group: None })
            .collect());
    }
    #[derive(Deserialize)]
    struct Tag {
        name: String,
        project_type: String,
        header: String,
    }
    let tags: Vec<Tag> = state.http.get("https://api.modrinth.com/v2/tag/category").send().await?.error_for_status()?.json().await?;
    // У плагинов на Modrinth категории те же, что у модов
    let wanted = if project_type == "plugin" { "mod" } else { project_type.as_str() };
    Ok(tags
        .into_iter()
        .filter(|t| t.project_type == wanted)
        .map(|t| Category { id: t.name.clone(), name: t.name, group: Some(t.header) })
        .collect())
}

#[tauri::command]
pub async fn content_search(state: State<'_, AppState>, args: SearchArgs) -> Result<modrinth::SearchResult> {
    let instance = match &args.instance_id {
        Some(id) => Some(instances::load(&state.paths, id)?),
        None => None,
    };
    let mut result = search_once(&state, &args, instance.as_ref()).await?;
    // Мод есть, но не для этой версии игры — показываем его отдельно, а не делаем вид, что его нет
    if let Some(instance) = instance.as_ref().filter(|_| !args.any_version && args.game_version.is_none() && args.offset == 0 && !args.query.trim().is_empty()) {
        let wide = SearchArgs {
            any_version: true,
            query: args.query.clone(),
            sort: args.sort.clone(),
            instance_id: None,
            categories: args.categories.clone(),
            loaders: args.loaders.clone(),
            environment: args.environment.clone(),
            ..args
        };
        if let Ok(all) = search_once(&state, &wide, Some(instance)).await {
            result.other = all
                .hits
                .into_iter()
                .filter(|h| !h.game_versions.is_empty() && !h.game_versions.contains(&instance.mc_version))
                .filter(|h| !result.hits.iter().any(|r| r.project_id == h.project_id))
                .take(5)
                .collect();
        }
    }
    Ok(result)
}

async fn search_once(state: &AppState, args: &SearchArgs, instance: Option<&instances::Instance>) -> Result<modrinth::SearchResult> {
    if args.source == Source::Curseforge {
        let loader = match args.loaders.first() {
            Some(l) => loader_by_name(l),
            None => instance.and_then(|i| i.loader),
        }
        .filter(|_| args.kind == ContentKind::Mod);
        return curseforge::search_filtered(
            &state.http,
            args.query.trim(),
            curseforge::class_id(args.kind),
            args.version(instance),
            loader,
            &args.sort,
            args.offset,
            &args.categories,
        )
        .await;
    }
    let loaders: Vec<&str> = if !args.loaders.is_empty() {
        args.loaders.iter().map(String::as_str).collect()
    } else if args.kind == ContentKind::Mod {
        modrinth_loaders(args.kind, instance.and_then(|i| i.loader))
    } else {
        // Для ресурспаков фильтр "minecraft" в поиске не нужен: такой категории у проектов нет
        vec![]
    };
    let extra = modrinth::environment_facets(args.environment.as_deref());
    modrinth::search(
        &state.http,
        &modrinth::SearchQuery {
            query: args.query.trim(),
            project_type: args.kind.modrinth_type(),
            game_version: args.version(instance),
            loaders: &loaders,
            sort: &args.sort,
            offset: args.offset,
            limit: 20,
            categories: &args.categories,
            extra: &extra,
        },
    )
    .await
}

#[tauri::command]
pub async fn content_list(state: State<'_, AppState>, id: String, quick: Option<bool>) -> Result<Vec<InstalledContent>> {
    list_with(&state, &id, quick.unwrap_or(false)).await
}

#[tauri::command]
pub async fn content_install(
    state: State<'_, AppState>,
    id: String,
    project_id: String,
    kind: ContentKind,
    source: Option<Source>,
    any_version: Option<bool>,
    game_version: Option<String>,
) -> Result<InstallResult> {
    let source = source.unwrap_or_default();
    let any_version = any_version.unwrap_or(false);
    let picked = game_version.filter(|v| !v.is_empty());
    let mut instance = instances::load(&state.paths, &id)?;
    if kind == ContentKind::Mod && instance.loader.is_none() {
        return Err(Error::msg("content_needs_loader"));
    }
    if kind == ContentKind::Shader && instance.loader.is_none() {
        return Err(Error::msg("shaders_need_loader"));
    }
    let mut projects = installed_project_ids(&state, &id).await?;
    let mut result = InstallResult::default();

    match source {
        Source::Modrinth => {
            let loaders = modrinth_loaders(kind, instance.loader);
            let game_version = picked.as_deref().unwrap_or(if any_version { "" } else { instance.mc_version.as_str() });
            let versions = modrinth::project_versions(&state.http, &project_id, &loaders, game_version).await?;
            let version = modrinth::best(versions).ok_or_else(|| modrinth::not_found(&project_id))?;
            install_version(&state, &instance, kind, version, false, &mut projects, &mut result.installed).await?;
        }
        Source::Curseforge => {
            let mod_id: u64 = project_id.parse().map_err(|_| Error::msg("bad CurseForge project id"))?;
            let game_version = picked.as_deref().unwrap_or(if any_version { "" } else { instance.mc_version.as_str() });
            let files = curseforge::files(&state.http, mod_id, kind, game_version, instance.loader).await?;
            let file = curseforge::best(files).ok_or_else(|| modrinth::not_found(&project_id))?;
            let project = curseforge::get_mod(&state.http, mod_id).await?;
            install_cf_file(&state, &instance, kind, project, file, false, &mut projects, &mut result).await?;
        }
    }
    let out = &mut result.installed;

    // Шейдерам нужен Iris (на Forge — Oculus). Если его нет и он есть под эту версию — ставим
    if kind == ContentKind::Shader {
        let shader_mod = match instance.loader {
            Some(ModLoader::Forge) => "oculus",
            _ => "iris",
        };
        const SHADER_MODS: [&str; 3] = ["YL57xq9U", "GchcoXML", "iris"]; // Iris, Oculus
        let has_shader_mod = projects.iter().any(|p| SHADER_MODS.contains(&p.as_str()));
        if !has_shader_mod {
            let mod_loaders = modrinth_loaders(ContentKind::Mod, instance.loader);
            let list =
                modrinth::project_versions(&state.http, shader_mod, &mod_loaders, &instance.mc_version).await?;
            if let Some(v) = modrinth::best(list) {
                if !projects.contains(&v.project_id) {
                    install_version(&state, &instance, ContentKind::Mod, v, true, &mut projects, out).await?;
                }
            }
        }
    }

    refresh_mod_count(&state.paths, &mut instance)?;
    Ok(result)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Sources {
    pub curseforge: bool,
    /// В эту сборку лаунчера вшит свой ключ
    pub built_in: bool,
}

#[tauri::command]
pub fn content_sources() -> Sources {
    Sources { curseforge: curseforge::enabled(), built_in: curseforge::built_in() }
}

/// Ищет вручную скачанный файл в «Загрузках» и переносит его в сборку. true — файл забран.
#[tauri::command]
pub async fn content_manual_take(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
    item: ManualDownload,
) -> Result<bool> {
    let name = checked_name(&item.file_name)?.to_owned();
    let downloads = app.path().download_dir().map_err(|e| Error::msg(e.to_string()))?;
    let target_dir = state.paths.game_dir(&id)?.join(item.kind.folder());
    let target = target_dir.join(&name);
    let (kind, size, expected_sha1) = (item.kind, item.size, item.sha1.clone());
    let found = tokio::task::spawn_blocking(move || -> Result<bool> {
        // Браузер мог добавить « (1)» к имени, если такой файл уже был
        let stem = name.rsplit_once('.').map_or(name.as_str(), |(s, _)| s).to_owned();
        let Ok(entries) = std::fs::read_dir(&downloads) else { return Ok(false) };
        for entry in entries.flatten() {
            let file = entry.file_name().to_string_lossy().into_owned();
            if file != name && !(file.starts_with(&stem) && file.ends_with(kind.extension())) {
                continue;
            }
            let path = entry.path();
            if entry.metadata().map(|m| m.len()).ok() != Some(size) {
                continue;
            }
            if let Some(expected) = &expected_sha1 {
                if !sha1_file(&path)?.eq_ignore_ascii_case(expected) {
                    continue;
                }
            }
            std::fs::create_dir_all(&target_dir)?;
            // rename не работает между дисками — тогда копируем и удаляем
            if std::fs::rename(&path, &target).is_err() {
                std::fs::copy(&path, &target)?;
                let _ = std::fs::remove_file(&path);
            }
            return Ok(true);
        }
        Ok(false)
    })
    .await
    .map_err(|e| Error::msg(e.to_string()))??;

    if found {
        if let Some(sha1) = &item.sha1 {
            remember(&state.paths, &id, sha1, item.meta.clone())?;
        }
        let mut instance = instances::load(&state.paths, &id)?;
        refresh_mod_count(&state.paths, &mut instance)?;
    }
    Ok(found)
}

#[tauri::command]
pub fn content_toggle(
    state: State<'_, AppState>,
    id: String,
    kind: ContentKind,
    file: String,
    enabled: bool,
) -> Result<String> {
    let file = checked_name(&file)?;
    let dir = state.paths.game_dir(&id)?.join(kind.folder());
    let base = file.strip_suffix(DISABLED).unwrap_or(file);
    let target = if enabled { base.to_owned() } else { format!("{base}{DISABLED}") };
    if target != file {
        std::fs::rename(dir.join(file), dir.join(&target))?;
    }
    let mut instance = instances::load(&state.paths, &id)?;
    refresh_mod_count(&state.paths, &mut instance)?;
    Ok(target)
}

#[tauri::command]
pub fn content_remove(state: State<'_, AppState>, id: String, kind: ContentKind, file: String) -> Result<()> {
    let file = checked_name(&file)?;
    std::fs::remove_file(state.paths.game_dir(&id)?.join(kind.folder()).join(file))?;
    let mut instance = instances::load(&state.paths, &id)?;
    refresh_mod_count(&state.paths, &mut instance)
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    pub source: Source,
    pub project_id: String,
    pub kind: ContentKind,
    pub file: String,
    pub version_id: String,
    pub version_number: String,
}

#[tauri::command]
pub async fn content_updates(state: State<'_, AppState>, id: String) -> Result<Vec<UpdateInfo>> {
    updates(&state, &id).await
}

async fn updates(state: &AppState, id: &str) -> Result<Vec<UpdateInfo>> {
    let instance = instances::load(&state.paths, id)?;
    let items = list(state, id).await?;
    let mut out = vec![];
    for kind in ContentKind::ALL {
        let mine: Vec<&InstalledContent> = items
            .iter()
            .filter(|c| c.kind == kind && c.meta.as_ref().is_some_and(|m| m.source == Source::Modrinth))
            .collect();
        if mine.is_empty() {
            continue;
        }
        let loaders = match kind {
            ContentKind::Shader => vec!["iris", "optifine", "canvas", "vanilla"],
            _ => modrinth_loaders(kind, instance.loader),
        };
        let hashes: Vec<String> = mine.iter().map(|c| c.sha1.clone()).collect();
        let latest = modrinth::latest_by_hash(&state.http, &hashes, &loaders, &instance.mc_version).await?;
        for c in mine {
            let (Some(meta), Some(v)) = (&c.meta, latest.get(&c.sha1)) else { continue };
            if v.id != meta.version_id {
                out.push(UpdateInfo {
                    source: Source::Modrinth,
                    project_id: meta.project_id.clone(),
                    kind,
                    file: c.file.clone(),
                    version_id: v.id.clone(),
                    version_number: v.version_number.clone(),
                });
            }
        }
    }

    // CurseForge: одним запросом получаем latestFilesIndexes всех проектов
    let cf: Vec<&InstalledContent> =
        items.iter().filter(|c| c.meta.as_ref().is_some_and(|m| m.source == Source::Curseforge)).collect();
    if !cf.is_empty() && curseforge::enabled() {
        let ids: Vec<u64> = cf.iter().filter_map(|c| c.meta.as_ref()?.project_id.parse().ok()).collect();
        let mods: HashMap<String, curseforge::Mod> =
            curseforge::get_mods(&state.http, &ids).await?.into_iter().map(|m| (m.id.to_string(), m)).collect();
        for c in cf {
            let Some(meta) = &c.meta else { continue };
            let Some(m) = mods.get(&meta.project_id) else { continue };
            let Some(latest) = curseforge::latest_for(m, c.kind, &instance.mc_version, instance.loader) else { continue };
            if latest.file_id.to_string() != meta.version_id && latest.file_id > meta.version_id.parse().unwrap_or(0) {
                out.push(UpdateInfo {
                    source: Source::Curseforge,
                    project_id: meta.project_id.clone(),
                    kind: c.kind,
                    file: c.file.clone(),
                    version_id: latest.file_id.to_string(),
                    version_number: latest.filename.clone(),
                });
            }
        }
    }
    Ok(out)
}

#[tauri::command]
pub async fn content_update(
    state: State<'_, AppState>,
    id: String,
    kind: ContentKind,
    file: String,
    version_id: String,
    source: Option<Source>,
    project_id: Option<String>,
) -> Result<()> {
    let instance = instances::load(&state.paths, &id)?;
    let file = checked_name(&file)?.to_owned();
    if source == Some(Source::Curseforge) {
        let mod_id: u64 = project_id.as_deref().and_then(|p| p.parse().ok()).ok_or_else(|| Error::msg("bad project id"))?;
        let file_id: u64 = version_id.parse().map_err(|_| Error::msg("bad file id"))?;
        let cf_file = curseforge::get_file(&state.http, mod_id, file_id).await?;
        if cf_file.download_url.is_none() {
            return Err(Error::msg("curseforge_manual_update"));
        }
        let new_name = cf_file.file_name.clone();
        let project = curseforge::get_mod(&state.http, mod_id).await?;
        let dir = state.paths.game_dir(&id)?.join(kind.folder());
        if file == new_name {
            std::fs::remove_file(dir.join(&file))?;
        }
        let mut result = InstallResult::default();
        install_cf_file(&state, &instance, kind, project, cf_file, false, &mut HashSet::new(), &mut result).await?;
        return finish_update(&dir, &file, &new_name);
    }
    let version = modrinth::version(&state.http, &version_id).await?;
    let new_name = version.primary_file().map(|f| f.filename.clone()).unwrap_or_default();
    let dir = state.paths.game_dir(&id)?.join(kind.folder());
    // Файл с тем же именем загрузчик счёл бы уже скачанным — убираем старый заранее
    if file == new_name {
        std::fs::remove_file(dir.join(&file))?;
    }
    let mut projects = HashSet::new();
    let mut out = vec![];
    install_version(&state, &instance, kind, version, false, &mut projects, &mut out).await?;
    finish_update(&dir, &file, &new_name)
}

/// Убирает старый файл; отключённый мод остаётся отключённым и после обновления.
fn finish_update(dir: &Path, old: &str, new_name: &str) -> Result<()> {
    if old != new_name {
        let _ = std::fs::remove_file(dir.join(old));
    }
    if old.ends_with(DISABLED) {
        std::fs::rename(dir.join(new_name), dir.join(format!("{new_name}{DISABLED}")))?;
    }
    Ok(())
}

// ---------- файлы, перетащенные в окно сборки ----------

/// Чем оказался перетащенный файл.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Dropped {
    Content(ContentKind),
    /// Архив модпака — его ставят отдельной сборкой через импорт
    Pack,
    Unsupported,
}

/// Тип файла по содержимому: у ресурспаков в корне `pack.mcmeta`, у шейдеров папка `shaders/`,
/// у модпаков манифест.
fn classify(path: &Path) -> Dropped {
    let name = path.file_name().map(|n| n.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
    if name.ends_with(".mrpack") {
        return Dropped::Pack;
    }
    if name.ends_with(".jar") {
        return Dropped::Content(ContentKind::Mod);
    }
    if !name.ends_with(".zip") {
        return Dropped::Unsupported;
    }
    let Ok(file) = std::fs::File::open(path) else { return Dropped::Unsupported };
    let Ok(mut archive) = zip::ZipArchive::new(file) else { return Dropped::Unsupported };
    let names: Vec<String> = archive.file_names().map(|n| n.replace('\\', "/")).collect();
    let has = |n: &str| names.iter().any(|x| x == n);
    if has("modrinth.index.json") {
        return Dropped::Pack;
    }
    if has("manifest.json") {
        let mut text = String::new();
        if let Ok(mut entry) = archive.by_name("manifest.json") {
            let _ = std::io::Read::read_to_string(&mut entry, &mut text);
        }
        if text.contains("minecraftModpack") {
            return Dropped::Pack;
        }
    }
    // Шейдеры иногда упакованы с лишней папкой сверху: `Pack/shaders/…`
    if names.iter().any(|n| n.starts_with("shaders/") || n.split('/').nth(1) == Some("shaders")) {
        return Dropped::Content(ContentKind::Shader);
    }
    if has("pack.mcmeta") {
        // Датапак (только data/) ставится в мир, а не в сборку
        let assets = names.iter().any(|n| n.starts_with("assets/"));
        let data = names.iter().any(|n| n.starts_with("data/"));
        return if data && !assets { Dropped::Unsupported } else { Dropped::Content(ContentKind::Resourcepack) };
    }
    // Очень старые моды распространялись zip-архивами
    if has("fabric.mod.json") || has("mcmod.info") || has("META-INF/mods.toml") {
        return Dropped::Content(ContentKind::Mod);
    }
    Dropped::Unsupported
}

#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AddFilesResult {
    /// Добавленные файлы: имя и куда попали
    pub added: Vec<(String, ContentKind)>,
    /// Архивы модпаков — фронтенд предложит их импортировать
    pub packs: Vec<String>,
    /// Файлы, которые не моды, не ресурспаки и не шейдеры
    pub skipped: Vec<String>,
}

/// Копирует перетащенные файлы в нужные папки сборки. Названия и иконки подтянутся
/// при следующем показе списка — по хешу, как у любого файла в папке.
#[tauri::command]
pub async fn content_add_files(state: State<'_, AppState>, id: String, paths: Vec<String>) -> Result<AddFilesResult> {
    let game_dir = state.paths.game_dir(&id)?;
    let result = tokio::task::spawn_blocking(move || -> Result<AddFilesResult> {
        let mut result = AddFilesResult::default();
        for raw in paths {
            let path = PathBuf::from(&raw);
            let Some(name) = path.file_name().map(|n| n.to_string_lossy().into_owned()) else { continue };
            if !path.is_file() {
                result.skipped.push(name);
                continue;
            }
            match classify(&path) {
                Dropped::Pack => result.packs.push(raw),
                Dropped::Unsupported => result.skipped.push(name),
                Dropped::Content(kind) => {
                    let dir = game_dir.join(kind.folder());
                    std::fs::create_dir_all(&dir)?;
                    // Старые моды в zip кладём как есть: загрузчик читает и .zip
                    std::fs::copy(&path, dir.join(checked_name(&name)?))?;
                    result.added.push((name, kind));
                }
            }
        }
        Ok(result)
    })
    .await
    .map_err(|e| Error::msg(e.to_string()))??;

    if !result.added.is_empty() {
        let mut instance = instances::load(&state.paths, &id)?;
        refresh_mod_count(&state.paths, &mut instance)?;
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::instances::InstanceSettings;
    use crate::versions::VersionType;

    /// Ходит в Modrinth: cargo test content_roundtrip -- --ignored --nocapture
    #[tokio::test]
    #[ignore]
    async fn content_roundtrip() {
        let root = std::env::temp_dir().join("shard-content-test");
        let _ = std::fs::remove_dir_all(&root);
        let state = AppState::new(root.clone());
        let mut instance = Instance {
            id: "test".into(),
            name: "test".into(),
            mc_version: "1.21.1".into(),
            version_type: VersionType::Release,
            loader: Some(ModLoader::Fabric),
            loader_version: None,
            mod_count: 0,
            last_played: None,
            created_at: String::new(),
            memory_mb: 2048,
            settings: InstanceSettings::default(),
            icon: None,
            group: None,
            favorite: false,
            pack: None,
        };
        instances::save(&state.paths, &instance).unwrap();

        // Шейдер тянет за собой Iris, а Iris — Sodium
        let shaders = modrinth::search(
            &state.http,
            &modrinth::SearchQuery {
                query: "complementary reimagined",
                project_type: "shader",
                game_version: Some("1.21.1"),
                loaders: &[],
                sort: "relevance",
                offset: 0,
                limit: 1,
                categories: &[],
                extra: &[],
            },
        )
        .await
        .unwrap();
        let shader = &shaders.hits[0];
        println!("shader: {} ({})", shader.title, shader.project_id);
        let mut projects = HashSet::new();
        let mut out = vec![];
        let v = modrinth::best(modrinth::project_versions(&state.http, &shader.project_id, &[], "1.21.1").await.unwrap())
            .unwrap();
        install_version(&state, &instance, ContentKind::Shader, v, false, &mut projects, &mut out).await.unwrap();
        let iris = modrinth::best(
            modrinth::project_versions(&state.http, "iris", &["fabric"], "1.21.1").await.unwrap(),
        )
        .unwrap();
        install_version(&state, &instance, ContentKind::Mod, iris, true, &mut projects, &mut out).await.unwrap();
        for i in &out {
            println!("installed: {} {} dep={}", i.title, i.version_number, i.dependency);
        }
        assert!(out.iter().any(|i| i.title.contains("Sodium")), "Sodium should come as Iris dependency");

        // Файл, положенный вручную, опознаётся по хешу
        let mods = state.paths.game_dir("test").unwrap().join("mods");
        let first = std::fs::read_dir(&mods).unwrap().flatten().next().unwrap().path();
        std::fs::write(index_file(&state.paths, "test").unwrap(), "{}").unwrap();
        std::fs::rename(&first, mods.join("renamed.jar")).unwrap();
        let listed = list(&state, "test").await.unwrap();
        for c in &listed {
            println!("{:?} {} {:?}", c.kind, c.file, c.meta.as_ref().map(|m| &m.title));
        }
        assert!(listed.iter().all(|c| c.meta.is_some()));

        // Отключение и счётчик модов
        refresh_mod_count(&state.paths, &mut instance).unwrap();
        assert_eq!(instance.mod_count, 2);
        std::fs::rename(mods.join("renamed.jar"), mods.join("renamed.jar.disabled")).unwrap();
        let listed = list(&state, "test").await.unwrap();
        assert!(listed.iter().any(|c| !c.enabled && c.meta.is_some()));
        refresh_mod_count(&state.paths, &mut instance).unwrap();
        assert_eq!(instance.mod_count, 1);

        // Старая версия Sodium видна как обновляемая, в том числе отключённая
        let old_sodium = modrinth::project_versions(&state.http, "sodium", &["fabric"], "1.21.1").await.unwrap();
        let old_sodium = old_sodium.into_iter().nth(2).unwrap();
        println!("old sodium: {}", old_sodium.version_number);
        for f in std::fs::read_dir(&mods).unwrap().flatten() {
            if f.file_name().to_string_lossy().starts_with("sodium") {
                std::fs::remove_file(f.path()).unwrap();
            }
        }
        let mut out = vec![];
        install_version(&state, &instance, ContentKind::Mod, old_sodium, false, &mut HashSet::new(), &mut out)
            .await
            .unwrap();
        let found = updates(&state, "test").await.unwrap();
        for u in &found {
            println!("update: {} -> {}", u.file, u.version_number);
        }
        assert!(found.iter().any(|u| u.file.starts_with("sodium")));
        let _ = std::fs::remove_dir_all(&root);
    }

    /// Ходит в CurseForge, нужен ключ в curseforge-api-key.txt:
    /// cargo test curseforge_roundtrip -- --ignored --nocapture
    #[tokio::test]
    #[ignore]
    async fn curseforge_roundtrip() {
        if !curseforge::enabled() {
            println!("no CurseForge key, skipped");
            return;
        }
        let root = std::env::temp_dir().join("shard-cf-test");
        let _ = std::fs::remove_dir_all(&root);
        let state = AppState::new(root.clone());
        let instance = Instance {
            id: "cf".into(),
            name: "cf".into(),
            mc_version: "1.20.1".into(),
            version_type: VersionType::Release,
            loader: Some(ModLoader::Forge),
            loader_version: None,
            mod_count: 0,
            last_played: None,
            created_at: String::new(),
            memory_mb: 2048,
            settings: InstanceSettings::default(),
            icon: None,
            group: None,
            favorite: false,
            pack: None,
        };
        instances::save(&state.paths, &instance).unwrap();

        let found = curseforge::search(&state.http, "jei", ContentKind::Mod, Some("1.20.1"), Some(ModLoader::Forge), "relevance", 0)
            .await
            .unwrap();
        println!("search: {} hits, first {}", found.total_hits, found.hits[0].title);

        // JEI — один из самых скачиваемых модов под Forge 1.20.1
        let jei = found.hits.iter().find(|h| h.slug == "jei").map(|h| h.project_id.clone()).unwrap();
        let mod_id: u64 = jei.parse().unwrap();
        let file = curseforge::best(curseforge::files(&state.http, mod_id, ContentKind::Mod, "1.20.1", Some(ModLoader::Forge)).await.unwrap()).unwrap();
        let project = curseforge::get_mod(&state.http, mod_id).await.unwrap();
        let mut result = InstallResult::default();
        install_cf_file(&state, &instance, ContentKind::Mod, project, file, false, &mut HashSet::new(), &mut result).await.unwrap();
        for i in &result.installed {
            println!("installed: {} {}", i.title, i.version_number);
        }
        for m in &result.manual {
            println!("manual: {} {}", m.file_name, m.url);
        }

        // Файл без записи в content.json опознаётся по отпечатку
        std::fs::write(index_file(&state.paths, "cf").unwrap(), "{}").unwrap();
        let listed = list(&state, "cf").await.unwrap();
        for c in &listed {
            println!("{} {:?}", c.file, c.meta.as_ref().map(|m| (&m.title, m.source)));
        }
        assert!(listed.iter().all(|c| c.meta.is_some()));

        // Эти моды есть и на Modrinth, поэтому список опознал их там; отпечатки CurseForge проверяем напрямую
        let dir = state.paths.game_dir("cf").unwrap().join("mods");
        let prints: Vec<u32> = std::fs::read_dir(&dir)
            .unwrap()
            .map(|e| curseforge::fingerprint(&std::fs::read(e.unwrap().path()).unwrap()))
            .collect();
        let found = curseforge::identify(&state.http, &prints).await.unwrap();
        println!("fingerprints matched: {}/{}", found.len(), prints.len());
        assert_eq!(found.len(), prints.len());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn classifies_dropped_files() {
        use std::io::Write;
        let dir = std::env::temp_dir().join(format!("shard-drop-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let make = |name: &str, entries: &[(&str, &str)]| {
            let path = dir.join(name);
            let mut zip = zip::ZipWriter::new(std::fs::File::create(&path).unwrap());
            for (n, body) in entries {
                zip.start_file(*n, zip::write::SimpleFileOptions::default()).unwrap();
                zip.write_all(body.as_bytes()).unwrap();
            }
            zip.finish().unwrap();
            path
        };
        let rp = make("rp.zip", &[("pack.mcmeta", "{}"), ("assets/minecraft/x.png", "")]);
        let dp = make("dp.zip", &[("pack.mcmeta", "{}"), ("data/x/y.json", "")]);
        let sh = make("sh.zip", &[("Complementary/shaders/final.fsh", "")]);
        let cf = make("cf.zip", &[("manifest.json", r#"{"manifestType":"minecraftModpack"}"#)]);
        let jar = make("m.jar", &[("fabric.mod.json", "{}")]);
        assert_eq!(classify(&rp), Dropped::Content(ContentKind::Resourcepack));
        assert_eq!(classify(&dp), Dropped::Unsupported);
        assert_eq!(classify(&sh), Dropped::Content(ContentKind::Shader));
        assert_eq!(classify(&cf), Dropped::Pack);
        assert_eq!(classify(&jar), Dropped::Content(ContentKind::Mod));
        std::fs::remove_dir_all(dir).unwrap();
    }
}
