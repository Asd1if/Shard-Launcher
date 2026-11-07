//! Починка сборки: по отчёту о краше — план действий (поставить зависимость, отключить мод,
//! добавить память, вернуть Java по умолчанию), который игрок видит и подтверждает.
//! Отсюда же — проверка недостающих зависимостей у установленных модов.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tauri::State;

use crate::content::{self, modrinth, ContentKind, Source};
use crate::crash::{CrashKind, CrashReport};
use crate::error::Result;
use crate::instances;
use crate::modmeta::{self, ModMeta};
use crate::state::AppState;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum FixAction {
    /// Поставить мод по его id (как он указан в зависимостях)
    Install { id: String, for_mods: Vec<String> },
    /// Отключить мод: файл получит `.disabled`
    Disable { file: String, name: String },
    Memory { from: u32, to: u32 },
    /// Вернуть Java, которую подбирает лаунчер
    Java,
}

#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FixResult {
    /// Что получилось — понятными словами для тоста
    pub done: Vec<String>,
    pub failed: Vec<String>,
}

/// Включённые моды сборки и что они о себе пишут.
fn scan(mods_dir: &Path) -> Vec<(PathBuf, ModMeta)> {
    std::fs::read_dir(mods_dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "jar"))
        .filter_map(|p| modmeta::read(&p).map(|m| (p, m)))
        .collect()
}

fn file_name(path: &Path) -> String {
    path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
}

fn installs(mods: &[(PathBuf, ModMeta)]) -> Vec<FixAction> {
    let metas: Vec<ModMeta> = mods.iter().map(|(_, m)| m.clone()).collect();
    modmeta::missing(&metas).into_iter().map(|(id, for_mods)| FixAction::Install { id, for_mods }).collect()
}

/// Моды, на которые показывает отчёт (по id или названию).
fn culprits(mods: &[(PathBuf, ModMeta)], names: &[String]) -> Vec<FixAction> {
    let wanted: Vec<String> = names.iter().map(|n| n.to_lowercase()).collect();
    mods.iter()
        .filter(|(_, m)| wanted.contains(&m.id.to_lowercase()) || wanted.contains(&m.name.to_lowercase()))
        .map(|(p, m)| FixAction::Disable { file: file_name(p), name: m.name.clone() })
        .collect()
}

/// Одинаковые моды: оставляем самый свежий файл, остальные отключаем.
fn duplicates(mods: &[(PathBuf, ModMeta)]) -> Vec<FixAction> {
    let mut out = vec![];
    let mut ids: Vec<&str> = mods.iter().map(|(_, m)| m.id.as_str()).collect();
    ids.sort_unstable();
    ids.dedup();
    for id in ids {
        let mut same: Vec<&(PathBuf, ModMeta)> = mods.iter().filter(|(_, m)| m.id == id).collect();
        if same.len() < 2 {
            continue;
        }
        same.sort_by_key(|(p, _)| std::fs::metadata(p).and_then(|m| m.modified()).ok());
        same.pop();
        out.extend(same.into_iter().map(|(p, m)| FixAction::Disable { file: file_name(p), name: m.name.clone() }));
    }
    out
}

#[tauri::command]
pub fn crash_fix_plan(state: State<'_, AppState>, id: String, report: CrashReport) -> Result<Vec<FixAction>> {
    let instance = instances::load(&state.paths, &id)?;
    let mods = scan(&state.paths.game_dir(&id)?.join("mods"));
    let mut plan = match report.kind {
        CrashKind::MissingDependency => installs(&mods),
        CrashKind::Duplicate => duplicates(&mods),
        CrashKind::Incompatible => [installs(&mods), culprits(&mods, &report.mods)].concat(),
        CrashKind::Mixin | CrashKind::ModCrash => culprits(&mods, &report.mods),
        CrashKind::Memory => {
            let total = crate::launch::system_info().total_memory_mb as u32;
            // Оставляем системе 2 ГБ; шаг — 2 ГБ
            let limit = total.saturating_sub(2048) / 256 * 256;
            let to = (instance.memory_mb + 2048).min(limit);
            if to > instance.memory_mb {
                vec![FixAction::Memory { from: instance.memory_mb, to }]
            } else {
                vec![]
            }
        }
        CrashKind::Java if instance.settings.java_path.is_some() => vec![FixAction::Java],
        _ => vec![],
    };
    plan.dedup();
    Ok(plan)
}

#[tauri::command]
pub async fn crash_fix_apply(state: State<'_, AppState>, id: String, actions: Vec<FixAction>) -> Result<FixResult> {
    let mods_dir = state.paths.game_dir(&id)?.join("mods");
    let mut result = FixResult::default();
    for action in actions {
        match action {
            FixAction::Install { id: mod_id, .. } => match install_by_mod_id(state.clone(), &id, &mod_id).await {
                Ok(title) => result.done.push(title),
                Err(_) => result.failed.push(mod_id),
            },
            FixAction::Disable { file, name } => {
                let from = mods_dir.join(&file);
                match std::fs::rename(&from, mods_dir.join(format!("{file}.disabled"))) {
                    Ok(()) => result.done.push(name),
                    Err(_) => result.failed.push(name),
                }
            }
            FixAction::Memory { to, .. } => {
                let mut instance = instances::load(&state.paths, &id)?;
                instance.memory_mb = to;
                instances::save(&state.paths, &instance)?;
                result.done.push(format!("{} MB", to));
            }
            FixAction::Java => {
                let mut instance = instances::load(&state.paths, &id)?;
                instance.settings.java_path = None;
                instances::save(&state.paths, &instance)?;
                result.done.push("Java".to_owned());
            }
        }
    }
    Ok(result)
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MissingDep {
    pub id: String,
    pub for_mods: Vec<String>,
}

/// Обязательные зависимости, которых не хватает включённым модам сборки.
#[tauri::command]
pub async fn content_missing_deps(state: State<'_, AppState>, id: String) -> Result<Vec<MissingDep>> {
    let dir = state.paths.game_dir(&id)?.join("mods");
    let mods = tokio::task::spawn_blocking(move || scan(&dir)).await.map_err(|e| crate::error::Error::msg(e.to_string()))?;
    let metas: Vec<ModMeta> = mods.into_iter().map(|(_, m)| m).collect();
    Ok(modmeta::missing(&metas).into_iter().map(|(id, for_mods)| MissingDep { id, for_mods }).collect())
}

#[tauri::command]
pub async fn content_install_deps(state: State<'_, AppState>, id: String, ids: Vec<String>) -> Result<FixResult> {
    let actions = ids.into_iter().map(|id| FixAction::Install { id, for_mods: vec![] }).collect();
    crash_fix_apply(state, id, actions).await
}

/// Мод по id из зависимостей. На Modrinth адрес проекта почти всегда совпадает с id мода
/// (`fabric-api`, `geckolib`); если нет — ищем и берём проект с таким же адресом.
async fn install_by_mod_id(state: State<'_, AppState>, instance_id: &str, mod_id: &str) -> Result<String> {
    let dashed = mod_id.replace('_', "-");
    let mut candidates = vec![mod_id.to_owned()];
    if dashed != mod_id {
        candidates.push(dashed.clone());
    }
    for slug in &candidates {
        if let Ok(r) = content::content_install(state.clone(), instance_id.to_owned(), slug.clone(), ContentKind::Mod, Some(Source::Modrinth), None, None).await {
            if let Some(first) = r.installed.first() {
                return Ok(first.title.clone());
            }
        }
    }
    let instance = instances::load(&state.paths, instance_id)?;
    let loaders = content::modrinth_loaders(ContentKind::Mod, instance.loader);
    let found = modrinth::search(
        &state.http,
        &modrinth::SearchQuery {
            query: mod_id,
            project_type: "mod",
            game_version: Some(&instance.mc_version),
            loaders: &loaders,
            sort: "relevance",
            offset: 0,
            limit: 10,
            categories: &[],
            extra: &[],
        },
    )
    .await?;
    let normalize = |s: &str| s.to_lowercase().replace(['-', '_', ' '], "");
    let hit = found
        .hits
        .into_iter()
        .find(|h| candidates.contains(&h.slug) || normalize(&h.slug) == normalize(mod_id) || normalize(&h.title) == normalize(mod_id))
        .ok_or_else(|| modrinth::not_found(mod_id))?;
    let r = content::content_install(state.clone(), instance_id.to_owned(), hit.project_id, ContentKind::Mod, Some(Source::Modrinth), None, None).await?;
    Ok(r.installed.first().map_or(hit.title, |i| i.title.clone()))
}
