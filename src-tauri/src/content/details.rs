//! Страница проекта в лаунчере: описание, скриншоты, версии и зависимости. Плюс установка
//! конкретной версии из списка.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};
use tauri::State;

use super::{curseforge, install_cf_file, install_version, modrinth, ContentKind, InstallResult, Source};
use crate::error::{Error, Result};
use crate::instances;
use crate::state::AppState;

const MODRINTH: &str = "https://api.modrinth.com/v2";
/// Столько версий показываем: у старых популярных модов их тысячи
const MAX_VERSIONS: usize = 300;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Shot {
    url: String,
    full: String,
    title: Option<String>,
}

#[derive(Serialize)]
pub struct Link {
    kind: &'static str,
    url: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VersionInfo {
    id: String,
    name: String,
    game_versions: Vec<String>,
    loaders: Vec<String>,
    date: String,
    /// release, beta или alpha
    channel: &'static str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DepInfo {
    project_id: String,
    title: String,
    icon_url: Option<String>,
    required: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectDetails {
    /// Описание: Markdown у Modrinth, HTML у CurseForge
    body: String,
    html: bool,
    gallery: Vec<Shot>,
    links: Vec<Link>,
    license: Option<String>,
    versions: Vec<VersionInfo>,
    /// Зависимости последней версии
    dependencies: Vec<DepInfo>,
}

#[derive(Deserialize)]
struct MrProject {
    #[serde(default)]
    body: String,
    #[serde(default)]
    gallery: Vec<MrShot>,
    source_url: Option<String>,
    issues_url: Option<String>,
    wiki_url: Option<String>,
    discord_url: Option<String>,
    #[serde(default)]
    donation_urls: Vec<MrDonation>,
    license: Option<MrLicense>,
}

#[derive(Deserialize)]
struct MrShot {
    url: String,
    raw_url: Option<String>,
    title: Option<String>,
    #[serde(default)]
    featured: bool,
    #[serde(default)]
    ordering: i64,
}

#[derive(Deserialize)]
struct MrDonation {
    url: String,
}

#[derive(Deserialize)]
struct MrLicense {
    id: String,
    #[serde(default)]
    name: String,
}

fn channel(kind: &str) -> &'static str {
    match kind {
        "beta" => "beta",
        "alpha" => "alpha",
        _ => "release",
    }
}

fn push_link(links: &mut Vec<Link>, kind: &'static str, url: Option<String>) {
    if let Some(url) = url.filter(|u| u.starts_with("https://") || u.starts_with("http://")) {
        links.push(Link { kind, url });
    }
}

async fn modrinth_details(state: &AppState, id: &str) -> Result<ProjectDetails> {
    let http = &state.http;
    let project = async { Ok::<MrProject, Error>(http.get(format!("{MODRINTH}/project/{id}")).send().await?.error_for_status()?.json().await?) };
    let versions = async {
        Ok::<Vec<modrinth::Version>, Error>(http.get(format!("{MODRINTH}/project/{id}/version")).send().await?.error_for_status()?.json().await?)
    };
    let (project, mut versions) = tokio::try_join!(project, versions)?;
    versions.sort_by(|a, b| b.date_published.cmp(&a.date_published));

    let mut deps: Vec<(String, bool)> = vec![];
    if let Some(latest) = versions.first() {
        for d in &latest.dependencies {
            let required = match d.dependency_type.as_str() {
                "required" => true,
                "optional" => false,
                _ => continue,
            };
            if let Some(p) = &d.project_id {
                if !deps.iter().any(|(id, _)| id == p) {
                    deps.push((p.clone(), required));
                }
            }
        }
    }
    let ids: Vec<String> = deps.iter().map(|(id, _)| id.clone()).collect();
    let found = if ids.is_empty() { vec![] } else { modrinth::projects(http, &ids).await.unwrap_or_default() };
    let dependencies = deps
        .into_iter()
        .filter_map(|(id, required)| {
            found.iter().find(|p| p.id == id).map(|p| DepInfo { project_id: id, title: p.title.clone(), icon_url: p.icon_url.clone(), required })
        })
        .collect();

    let mut gallery = project.gallery;
    gallery.sort_by_key(|s| (!s.featured, s.ordering));
    let mut links = vec![];
    push_link(&mut links, "source", project.source_url);
    push_link(&mut links, "issues", project.issues_url);
    push_link(&mut links, "wiki", project.wiki_url);
    push_link(&mut links, "discord", project.discord_url);
    for d in project.donation_urls {
        push_link(&mut links, "donate", Some(d.url));
    }
    Ok(ProjectDetails {
        body: project.body,
        html: false,
        gallery: gallery.into_iter().map(|s| Shot { full: s.raw_url.unwrap_or_else(|| s.url.clone()), url: s.url, title: s.title.filter(|t| !t.is_empty()) }).collect(),
        links,
        license: project.license.map(|l| if l.name.is_empty() { l.id } else { l.name }),
        versions: versions
            .into_iter()
            .take(MAX_VERSIONS)
            .map(|v| VersionInfo {
                channel: channel(&v.version_type),
                name: if v.name.is_empty() { v.version_number.clone() } else { v.name.clone() },
                id: v.id,
                game_versions: v.game_versions,
                loaders: v.loaders,
                date: v.date_published,
            })
            .collect(),
        dependencies,
    })
}

async fn curseforge_details(state: &AppState, id: &str) -> Result<ProjectDetails> {
    let http = &state.http;
    let mod_id: u64 = id.parse().map_err(|_| Error::msg("bad project id"))?;
    let (info, body, mut files) =
        tokio::try_join!(curseforge::mod_extras(http, mod_id), curseforge::description(http, mod_id), curseforge::files_page(http, mod_id, 0, 50))?;
    files.sort_by(|a, b| b.file_date.cmp(&a.file_date));

    let mut deps: Vec<(u64, bool)> = vec![];
    if let Some(latest) = files.first() {
        for d in &latest.dependencies {
            let required = match d.relation_type {
                3 => true,
                2 => false,
                _ => continue,
            };
            if !deps.iter().any(|(id, _)| *id == d.mod_id) {
                deps.push((d.mod_id, required));
            }
        }
    }
    let ids: Vec<u64> = deps.iter().map(|(id, _)| *id).collect();
    let found = curseforge::get_mods(http, &ids).await.unwrap_or_default();
    let dependencies = deps
        .into_iter()
        .filter_map(|(id, required)| {
            found.iter().find(|m| m.id == id).map(|m| DepInfo {
                project_id: id.to_string(),
                title: m.name.clone(),
                icon_url: m.logo.as_ref().and_then(|l| l.thumbnail_url.clone()),
                required,
            })
        })
        .collect();

    let mut links = vec![];
    if let Some(l) = info.links {
        push_link(&mut links, "source", l.source_url);
        push_link(&mut links, "issues", l.issues_url);
        push_link(&mut links, "wiki", l.wiki_url);
    }
    const LOADERS: [&str; 5] = ["forge", "fabric", "quilt", "neoforge", "liteloader"];
    Ok(ProjectDetails {
        body,
        html: true,
        gallery: info.screenshots.into_iter().map(|s| Shot { full: s.url.clone(), url: s.thumbnail_url.unwrap_or(s.url), title: s.title.filter(|t| !t.is_empty()) }).collect(),
        links,
        license: None,
        versions: files
            .into_iter()
            .map(|f| {
                let (loaders, game_versions): (Vec<String>, Vec<String>) =
                    f.game_versions.iter().cloned().partition(|v| LOADERS.contains(&v.to_ascii_lowercase().as_str()));
                VersionInfo {
                    id: f.id.to_string(),
                    name: f.display_name,
                    game_versions: game_versions.into_iter().filter(|v| v.starts_with(|c: char| c.is_ascii_digit())).collect(),
                    loaders: loaders.into_iter().map(|l| l.to_ascii_lowercase()).collect(),
                    date: f.file_date,
                    channel: match f.release_type {
                        2 => "beta",
                        3 => "alpha",
                        _ => "release",
                    },
                }
            })
            .collect(),
        dependencies,
    })
}

#[tauri::command]
pub async fn content_project(state: State<'_, AppState>, source: Source, project_id: String) -> Result<ProjectDetails> {
    match source {
        Source::Curseforge => curseforge_details(&state, &project_id).await,
        _ => modrinth_details(&state, &project_id).await,
    }
}

/// Ставит выбранную на странице проекта версию (с обязательными зависимостями).
#[tauri::command]
pub async fn content_install_version(
    state: State<'_, AppState>,
    id: String,
    kind: ContentKind,
    source: Source,
    project_id: String,
    version_id: String,
) -> Result<InstallResult> {
    let instance = instances::load(&state.paths, &id)?;
    let mut result = InstallResult::default();
    if source == Source::Curseforge {
        let mod_id: u64 = project_id.parse().map_err(|_| Error::msg("bad project id"))?;
        let file_id: u64 = version_id.parse().map_err(|_| Error::msg("bad file id"))?;
        let (project, file) = tokio::try_join!(curseforge::get_mod(&state.http, mod_id), curseforge::get_file(&state.http, mod_id, file_id))?;
        install_cf_file(&state, &instance, kind, project, file, false, &mut HashSet::new(), &mut result).await?;
    } else {
        let version = modrinth::version(&state.http, &version_id).await?;
        install_version(&state, &instance, kind, version, false, &mut HashSet::new(), &mut result.installed).await?;
    }
    Ok(result)
}
