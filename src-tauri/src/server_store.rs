//! Магазин сервера: плагины для Paper и Purpur, серверные моды для Fabric, Quilt, Forge и
//! NeoForge, ресурспак сервера (его игроки скачивают при входе).
//!
//! Ищет на Modrinth под ядро и версию сервера, ставит файл с обязательными зависимостями
//! в `plugins/` или `mods/`. Ресурспак прописывается в server.properties ссылкой и SHA1.
//! Что откуда поставлено — в `shard-store.json` рядом с сервером.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tauri::State;

use crate::content::modrinth::{self, SearchResult};
use crate::content::{modrinth_loaders, ContentKind};
use crate::error::{Error, Result};
use crate::server::{self, Core, ServerConfig};
use crate::state::AppState;

const INDEX: &str = "shard-store.json";
const SEARCH: &str = "https://api.modrinth.com/v2/search";
/// Ключ ресурспака сервера в индексе: файла у него нет, только ссылка в server.properties
const PACK_KEY: &str = "@resource-pack";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StoreKind {
    /// Плагины или моды — смотря какое ядро
    Addon,
    Resourcepack,
}

/// Что ставить на сервер с этим ядром.
struct Target {
    project_type: &'static str,
    folder: &'static str,
    loaders: Vec<&'static str>,
}

fn addon_target(config: &ServerConfig) -> Result<Target> {
    Ok(match config.core {
        Core::Paper => Target { project_type: "plugin", folder: "plugins", loaders: vec!["paper", "spigot", "bukkit"] },
        Core::Purpur => Target { project_type: "plugin", folder: "plugins", loaders: vec!["purpur", "paper", "spigot", "bukkit"] },
        Core::Vanilla => return Err(Error::msg("server_store_vanilla")),
        core => Target { project_type: "mod", folder: "mods", loaders: modrinth_loaders(ContentKind::Mod, core.loader()) },
    })
}

fn open(state: &AppState, id: &str) -> Result<(PathBuf, ServerConfig)> {
    let dir = server::server_dir(state, id)?;
    let config = server::load_config(&dir).ok_or_else(|| Error::msg("server_missing"))?;
    Ok((dir, config))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Entry {
    project_id: String,
    title: String,
    version_number: String,
    icon_url: Option<String>,
}

fn load_index(dir: &Path) -> HashMap<String, Entry> {
    std::fs::read(dir.join(INDEX)).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
}

fn save_index(dir: &Path, index: &HashMap<String, Entry>) -> Result<()> {
    std::fs::write(dir.join(INDEX), serde_json::to_vec_pretty(index)?)?;
    Ok(())
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn server_store_search(
    state: State<'_, AppState>,
    id: String,
    kind: StoreKind,
    query: String,
    sort: String,
    offset: u32,
    categories: Option<Vec<String>>,
    any_version: Option<bool>,
    game_version: Option<String>,
) -> Result<SearchResult> {
    let (_, config) = open(&state, &id)?;
    let (project_type, loaders) = match kind {
        StoreKind::Addon => {
            let t = addon_target(&config)?;
            (t.project_type, t.loaders)
        }
        StoreKind::Resourcepack => ("resourcepack", vec![]),
    };
    let mut facets = vec![vec![format!("project_type:{project_type}")]];
    if let Some(v) = game_version.filter(|v| !v.is_empty()) {
        facets.push(vec![format!("versions:{v}")]);
    } else if !any_version.unwrap_or(false) {
        facets.push(vec![format!("versions:{}", config.mc_version)]);
    }
    if !loaders.is_empty() {
        facets.push(loaders.iter().map(|l| format!("categories:{l}")).collect());
    }
    // Моды только для клиента серверу не нужны
    if project_type == "mod" {
        facets.push(vec!["server_side!=unsupported".to_owned()]);
    }
    for c in categories.unwrap_or_default() {
        facets.push(vec![format!("categories:{c}")]);
    }
    let index = match sort.as_str() {
        "downloads" | "follows" | "newest" | "updated" => sort.as_str(),
        _ => "relevance",
    };
    let mut result: SearchResult = state
        .http
        .get(SEARCH)
        .query(&[
            ("query", query.trim().to_owned()),
            ("facets", serde_json::to_string(&facets)?),
            ("index", index.to_owned()),
            ("offset", offset.to_string()),
            ("limit", "20".to_owned()),
        ])
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    for hit in &mut result.hits {
        hit.page_url = Some(format!("https://modrinth.com/{project_type}/{}", hit.slug));
    }
    Ok(result)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StoreItem {
    kind: StoreKind,
    file: String,
    project_id: Option<String>,
    title: String,
    version_number: Option<String>,
    icon_url: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StoreList {
    /// plugins или mods; None — у ядра их нет (ванильный сервер)
    folder: Option<&'static str>,
    items: Vec<StoreItem>,
}

#[tauri::command]
pub fn server_store_list(state: State<'_, AppState>, id: String) -> Result<StoreList> {
    let (dir, config) = open(&state, &id)?;
    let index = load_index(&dir);
    let target = addon_target(&config).ok();
    let mut items: Vec<StoreItem> = match &target {
        Some(target) => std::fs::read_dir(dir.join(target.folder))
            .into_iter()
            .flatten()
            .flatten()
            .filter(|e| e.path().is_file())
            .filter_map(|e| e.file_name().to_str().map(str::to_owned))
            .filter(|name| name.to_ascii_lowercase().ends_with(".jar"))
            .map(|file| match index.get(&file) {
                Some(e) => StoreItem {
                    kind: StoreKind::Addon,
                    project_id: Some(e.project_id.clone()),
                    title: e.title.clone(),
                    version_number: Some(e.version_number.clone()),
                    icon_url: e.icon_url.clone(),
                    file,
                },
                None => StoreItem {
                    kind: StoreKind::Addon,
                    project_id: None,
                    title: file.trim_end_matches(".jar").to_owned(),
                    version_number: None,
                    icon_url: None,
                    file,
                },
            })
            .collect(),
        None => vec![],
    };
    items.sort_by_key(|i| i.title.to_lowercase());

    // Ресурспак сервера: из магазина или вписанный вручную ссылкой
    let props = server::read_properties(&dir);
    let url = props.iter().find(|(k, _)| k == "resource-pack").map(|(_, v)| v.clone()).unwrap_or_default();
    if !url.is_empty() {
        let known = index.get(PACK_KEY);
        items.push(StoreItem {
            kind: StoreKind::Resourcepack,
            file: PACK_KEY.to_owned(),
            project_id: known.map(|e| e.project_id.clone()),
            title: known.map_or_else(|| url.rsplit('/').next().unwrap_or(&url).to_owned(), |e| e.title.clone()),
            version_number: known.map(|e| e.version_number.clone()),
            icon_url: known.and_then(|e| e.icon_url.clone()),
        });
    }
    Ok(StoreList { folder: target.map(|t| t.folder), items })
}

/// Ставит проект (и обязательные зависимости). Возвращает названия поставленного.
#[tauri::command]
pub async fn server_store_install(state: State<'_, AppState>, id: String, kind: Option<StoreKind>, project_id: String, any_version: Option<bool>) -> Result<Vec<String>> {
    let (dir, config) = open(&state, &id)?;
    let mut index = load_index(&dir);
    let mut installed = vec![];
    match kind.unwrap_or(StoreKind::Addon) {
        StoreKind::Addon => {
            let target = addon_target(&config)?;
            install(&state, &dir, &config, &target, &project_id, &mut index, &mut installed, 0).await?;
        }
        StoreKind::Resourcepack => {
            let version = if any_version.unwrap_or(false) { "" } else { config.mc_version.as_str() };
            let mut versions = modrinth::project_versions(&state.http, &project_id, &["minecraft"], version).await?;
            if versions.is_empty() && !version.is_empty() {
                // Старый ресурспак обычно работает и на новой версии
                versions = modrinth::project_versions(&state.http, &project_id, &["minecraft"], "").await?;
            }
            let v = modrinth::best(versions).ok_or_else(|| modrinth::not_found(&project_id))?;
            let file = v.primary_file().ok_or_else(|| modrinth::not_found(&project_id))?;
            let sha1 = file.hashes.get("sha1").cloned().unwrap_or_default();
            server::write_properties(
                &dir,
                &HashMap::from([("resource-pack".to_owned(), file.url.replace(':', "\\:")), ("resource-pack-sha1".to_owned(), sha1)]),
            )?;
            let project = modrinth::project(&state.http, &v.project_id).await?;
            installed.push(project.title.clone());
            index.insert(
                PACK_KEY.to_owned(),
                Entry { project_id: v.project_id.clone(), title: project.title, version_number: v.version_number.clone(), icon_url: project.icon_url },
            );
        }
    }
    save_index(&dir, &index)?;
    Ok(installed)
}

#[allow(clippy::too_many_arguments)]
async fn install(
    state: &AppState,
    dir: &Path,
    config: &ServerConfig,
    target: &Target,
    project_id: &str,
    index: &mut HashMap<String, Entry>,
    installed: &mut Vec<String>,
    depth: u8,
) -> Result<()> {
    let versions = modrinth::project_versions(&state.http, project_id, &target.loaders, &config.mc_version).await?;
    let version = modrinth::best(versions).ok_or_else(|| modrinth::not_found(project_id))?;
    let folder = dir.join(target.folder);
    if index.iter().any(|(file, e)| e.project_id == version.project_id && folder.join(file).exists()) {
        return Ok(());
    }
    let file = version.primary_file().ok_or_else(|| modrinth::not_found(project_id))?;
    let name = Path::new(&file.filename).file_name().and_then(|n| n.to_str()).ok_or_else(|| Error::msg("bad file name"))?.to_owned();
    let bytes = state.http.get(&file.url).send().await?.error_for_status()?.bytes().await?;
    std::fs::create_dir_all(&folder)?;
    crate::download::write_atomic(&folder.join(&name), &bytes).await?;
    let project = modrinth::project(&state.http, &version.project_id).await?;
    installed.push(project.title.clone());
    index.insert(
        name,
        Entry { project_id: version.project_id.clone(), title: project.title, version_number: version.version_number.clone(), icon_url: project.icon_url },
    );
    if depth < 4 {
        for dep in version.dependencies.iter().filter(|d| d.dependency_type == "required") {
            if let Some(dep_id) = &dep.project_id {
                Box::pin(install(state, dir, config, target, dep_id, index, installed, depth + 1)).await?;
            }
        }
    }
    Ok(())
}

#[tauri::command]
pub async fn server_store_remove(state: State<'_, AppState>, id: String, file: String) -> Result<()> {
    let (dir, config) = open(&state, &id)?;
    let mut index = load_index(&dir);
    if file == PACK_KEY {
        server::write_properties(&dir, &HashMap::from([("resource-pack".to_owned(), String::new()), ("resource-pack-sha1".to_owned(), String::new())]))?;
    } else {
        if server::is_running(&id).await {
            return Err(Error::msg("server_running"));
        }
        if file.contains(['/', '\\']) || file.starts_with('.') {
            return Err(Error::msg("file_not_found"));
        }
        let target = addon_target(&config)?;
        std::fs::remove_file(dir.join(target.folder).join(&file))?;
    }
    if index.remove(&file).is_some() {
        save_index(&dir, &index)?;
    }
    Ok(())
}
