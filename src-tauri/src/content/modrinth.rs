//! Клиент Modrinth API v2: поиск, версии проектов, поиск файлов по хешу и обновления.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_json::json;

use super::Source;
use crate::error::{Error, Result};

const API: &str = "https://api.modrinth.com/v2";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchHit {
    #[serde(default)]
    pub source: Source,
    /// Страница проекта на сайте источника
    #[serde(default)]
    pub page_url: Option<String>,
    #[serde(alias = "project_id")]
    pub project_id: String,
    pub slug: String,
    pub title: String,
    pub description: String,
    pub author: String,
    #[serde(alias = "icon_url")]
    pub icon_url: Option<String>,
    pub downloads: u64,
    pub follows: u64,
    #[serde(default, alias = "display_categories")]
    pub display_categories: Vec<String>,
    #[serde(alias = "date_modified")]
    pub date_modified: String,
    /// Версии игры, для которых есть файлы (у Modrinth — поле `versions`)
    #[serde(default, alias = "versions")]
    pub game_versions: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchResult {
    pub hits: Vec<SearchHit>,
    #[serde(alias = "total_hits")]
    pub total_hits: u64,
    /// Найденное, но только для других версий игры — чтобы мод не «пропадал» из поиска
    #[serde(default)]
    pub other: Vec<SearchHit>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Version {
    pub id: String,
    pub project_id: String,
    pub version_number: String,
    pub version_type: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub game_versions: Vec<String>,
    #[serde(default)]
    pub loaders: Vec<String>,
    #[serde(default)]
    pub date_published: String,
    pub files: Vec<VersionFile>,
    #[serde(default)]
    pub dependencies: Vec<Dependency>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct VersionFile {
    pub url: String,
    pub filename: String,
    #[serde(default)]
    pub primary: bool,
    pub hashes: HashMap<String, String>,
    pub size: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Dependency {
    pub version_id: Option<String>,
    pub project_id: Option<String>,
    pub dependency_type: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Project {
    pub id: String,
    #[serde(default)]
    pub slug: String,
    pub title: String,
    pub icon_url: Option<String>,
    /// required / optional / unsupported — нужен ли мод на сервере
    #[serde(default)]
    pub server_side: Option<String>,
}

impl Version {
    /// Основной файл версии (у некоторых модов есть ещё sources и т. п.)
    pub fn primary_file(&self) -> Option<&VersionFile> {
        self.files.iter().find(|f| f.primary).or(self.files.first())
    }
}

pub struct SearchQuery<'a> {
    pub query: &'a str,
    pub project_type: &'a str,
    pub game_version: Option<&'a str>,
    /// Загрузчики через «или»: для Quilt подходят и моды Fabric
    pub loaders: &'a [&'a str],
    pub sort: &'a str,
    pub offset: u32,
    pub limit: u32,
    /// Категории через «и»: каждая выбранная должна быть у проекта
    pub categories: &'a [String],
    /// Дополнительные фасеты (окружение и т. п.)
    pub extra: &'a [Vec<String>],
}

/// Фасеты Modrinth для окружения: client — только клиент, server — только сервер, both — нужен и там, и там.
pub fn environment_facets(env: Option<&str>) -> Vec<Vec<String>> {
    let f = |a: &str, b: &str| vec![vec![a.to_owned()], vec![b.to_owned()]];
    match env {
        Some("client") => f("client_side!=unsupported", "server_side:unsupported"),
        Some("server") => f("server_side!=unsupported", "client_side:unsupported"),
        Some("both") => f("client_side!=unsupported", "server_side!=unsupported"),
        _ => vec![],
    }
}


pub async fn search(http: &reqwest::Client, q: &SearchQuery<'_>) -> Result<SearchResult> {
    let mut facets = vec![vec![format!("project_type:{}", q.project_type)]];
    if let Some(v) = q.game_version {
        facets.push(vec![format!("versions:{v}")]);
    }
    if !q.loaders.is_empty() {
        facets.push(q.loaders.iter().map(|l| format!("categories:{l}")).collect());
    }
    for c in q.categories {
        facets.push(vec![format!("categories:{c}")]);
    }
    facets.extend(q.extra.iter().cloned());
    let index = match q.sort {
        "downloads" | "follows" | "newest" | "updated" => q.sort,
        _ => "relevance",
    };
    let response = http
        .get(format!("{API}/search"))
        .query(&[
            ("query", q.query.to_owned()),
            ("facets", serde_json::to_string(&facets)?),
            ("index", index.to_owned()),
            ("offset", q.offset.to_string()),
            ("limit", q.limit.to_string()),
        ])
        .send()
        .await?
        .error_for_status()?;
    let mut result: SearchResult = response.json().await?;
    for hit in &mut result.hits {
        hit.page_url = Some(format!("https://modrinth.com/{}/{}", q.project_type, hit.slug));
    }
    Ok(result)
}

/// Версии проекта, подходящие под версию игры и загрузчики, новые сверху.
/// Пустая версия игры — любая: так можно поставить ресурспак, сделанный для старой версии.
pub async fn project_versions(
    http: &reqwest::Client,
    project: &str,
    loaders: &[&str],
    game_version: &str,
) -> Result<Vec<Version>> {
    let mut query = vec![];
    if !game_version.is_empty() {
        query.push(("game_versions", serde_json::to_string(&[game_version])?));
    }
    if !loaders.is_empty() {
        query.push(("loaders", serde_json::to_string(loaders)?));
    }
    let response = http.get(format!("{API}/project/{project}/version")).query(&query).send().await?;
    if response.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(vec![]);
    }
    Ok(response.error_for_status()?.json().await?)
}

/// Лучшая версия: последний релиз, а если релизов нет — последняя бета или альфа.
pub fn best(versions: Vec<Version>) -> Option<Version> {
    let release = versions.iter().position(|v| v.version_type == "release");
    let index = release.unwrap_or(0);
    versions.into_iter().nth(index)
}

pub async fn version(http: &reqwest::Client, id: &str) -> Result<Version> {
    Ok(http.get(format!("{API}/version/{id}")).send().await?.error_for_status()?.json().await?)
}

pub async fn project(http: &reqwest::Client, id: &str) -> Result<Project> {
    Ok(http.get(format!("{API}/project/{id}")).send().await?.error_for_status()?.json().await?)
}

pub async fn projects(http: &reqwest::Client, ids: &[String]) -> Result<Vec<Project>> {
    if ids.is_empty() {
        return Ok(vec![]);
    }
    let response = http
        .get(format!("{API}/projects"))
        .query(&[("ids", serde_json::to_string(ids)?)])
        .send()
        .await?
        .error_for_status()?;
    Ok(response.json().await?)
}

/// Версии по SHA1 файлов. Файлов, которых нет на Modrinth, в ответе нет.
pub async fn versions_by_hash(http: &reqwest::Client, hashes: &[String]) -> Result<HashMap<String, Version>> {
    if hashes.is_empty() {
        return Ok(HashMap::new());
    }
    let response = http
        .post(format!("{API}/version_files"))
        .json(&json!({ "hashes": hashes, "algorithm": "sha1" }))
        .send()
        .await?
        .error_for_status()?;
    Ok(response.json().await?)
}

/// Последние подходящие версии для файлов по их SHA1.
pub async fn latest_by_hash(
    http: &reqwest::Client,
    hashes: &[String],
    loaders: &[&str],
    game_version: &str,
) -> Result<HashMap<String, Version>> {
    if hashes.is_empty() {
        return Ok(HashMap::new());
    }
    let response = http
        .post(format!("{API}/version_files/update"))
        .json(&json!({
            "hashes": hashes,
            "algorithm": "sha1",
            "loaders": loaders,
            "game_versions": [game_version],
        }))
        .send()
        .await?
        .error_for_status()?;
    Ok(response.json().await?)
}

pub fn not_found(what: &str) -> Error {
    Error::msg(format!("no_compatible_version:{what}"))
}
