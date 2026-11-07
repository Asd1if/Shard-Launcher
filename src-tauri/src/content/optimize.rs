//! Оптимизация в один клик: ставит проверенный набор модов на FPS и память под
//! загрузчик и версию сборки. Уже стоящие и несовместимые с версией пропускаем.

use tauri::State;

use super::*;

/// Группы взаимозаменяемых модов: из группы ставится первый, что есть под версию.
/// Если в сборке уже стоит любой мод группы — группу пропускаем (Sodium и Embeddium вместе не живут).
fn groups(loader: ModLoader) -> Vec<&'static [&'static str]> {
    match loader {
        ModLoader::Fabric | ModLoader::Quilt => vec![
            &["sodium"],
            &["lithium"],
            &["ferrite-core"],
            &["entityculling"],
            &["immediatelyfast"],
            &["modernfix"],
            &["dynamic-fps"],
        ],
        ModLoader::Neoforge => vec![
            &["sodium", "embeddium"],
            &["ferrite-core"],
            &["entityculling"],
            &["immediatelyfast"],
            &["modernfix"],
            &["dynamic-fps"],
        ],
        ModLoader::Forge => vec![
            &["embeddium", "rubidium"],
            &["ferrite-core"],
            &["entityculling"],
            &["modernfix"],
            &["dynamic-fps"],
        ],
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OptimizeResult {
    pub installed: Vec<Installed>,
    /// Уже стояли
    pub present: u32,
    /// Нет под эту версию
    pub unavailable: u32,
}

#[tauri::command]
pub async fn content_optimize(state: State<'_, AppState>, id: String) -> Result<OptimizeResult> {
    let mut instance = instances::load(&state.paths, &id)?;
    let loader = instance.loader.ok_or_else(|| Error::msg("content_needs_loader"))?;
    let groups = groups(loader);
    let slugs: Vec<String> = groups.iter().flat_map(|g| g.iter().map(|s| (*s).to_owned())).collect();
    let ids: HashMap<String, String> = modrinth::projects(&state.http, &slugs).await?.into_iter().map(|p| (p.slug, p.id)).collect();
    let mut projects = installed_project_ids(&state, &id).await?;
    let loaders = modrinth_loaders(ContentKind::Mod, Some(loader));
    let mut result = OptimizeResult { installed: vec![], present: 0, unavailable: 0 };

    for group in groups {
        if group.iter().filter_map(|s| ids.get(*s)).any(|pid| projects.contains(pid)) {
            result.present += 1;
            continue;
        }
        let mut done = false;
        for slug in group {
            let versions = modrinth::project_versions(&state.http, slug, &loaders, &instance.mc_version).await.unwrap_or_default();
            if let Some(version) = modrinth::best(versions) {
                if install_version(&state, &instance, ContentKind::Mod, version, false, &mut projects, &mut result.installed).await.is_ok() {
                    done = true;
                    break;
                }
            }
        }
        if !done {
            result.unavailable += 1;
        }
    }
    refresh_mod_count(&state.paths, &mut instance)?;
    Ok(result)
}
