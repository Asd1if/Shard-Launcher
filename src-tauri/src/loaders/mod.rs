//! Загрузчики модов: Fabric, Quilt, Forge, NeoForge.
//!
//! Каждый загрузчик даёт «профиль» — частичный version JSON с `inheritsFrom`. Он хранится в
//! `versions/<загрузчик>-<mc>-<версия>/` и при запуске накладывается на ванильный JSON:
//! свой mainClass, дополнительные аргументы и библиотеки. Ядро остаётся ванильным.

mod fabric;
pub(crate) mod forge;

use std::collections::HashSet;

use serde::Serialize;
use serde_json::{json, Value};
use tauri::State;

use crate::download::write_atomic;
use crate::error::{Error, Result};
use crate::game::{version::VersionJson, Stage};
use crate::instances::ModLoader;
use crate::state::AppState;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LoaderVersion {
    pub version: String,
    pub stable: bool,
    pub recommended: bool,
}

/// Версии загрузчика для данной версии Minecraft, новые сверху.
pub async fn versions(state: &AppState, loader: ModLoader, mc: &str) -> Result<Vec<LoaderVersion>> {
    match loader {
        ModLoader::Fabric | ModLoader::Quilt => fabric::versions(&state.http, loader, mc).await,
        ModLoader::Forge | ModLoader::Neoforge => forge::versions(&state.http, loader, mc).await,
    }
}

#[tauri::command]
pub async fn loader_versions(
    state: State<'_, AppState>,
    loader: ModLoader,
    mc_version: String,
) -> Result<Vec<LoaderVersion>> {
    versions(&state, loader, &mc_version).await
}

/// Рекомендованная версия, иначе последняя стабильная, иначе просто последняя.
pub fn pick_default(list: &[LoaderVersion]) -> Option<&LoaderVersion> {
    list.iter()
        .find(|v| v.recommended)
        .or_else(|| list.iter().find(|v| v.stable))
        .or(list.first())
}

fn loader_name(loader: ModLoader) -> &'static str {
    match loader {
        ModLoader::Fabric => "fabric",
        ModLoader::Quilt => "quilt",
        ModLoader::Forge => "forge",
        ModLoader::Neoforge => "neoforge",
    }
}

/// Ставит загрузчик, если его ещё нет, и возвращает итоговый version JSON для запуска.
pub async fn apply(
    state: &AppState,
    loader: ModLoader,
    loader_version: &str,
    vanilla: &VersionJson,
    progress: &mut (dyn FnMut(Stage, usize, usize) + Send),
) -> Result<VersionJson> {
    let mc = &vanilla.id;
    let id = format!("{}-{mc}-{loader_version}", loader_name(loader));
    let file = state.paths.version_dir(&id).join(format!("{id}.json"));

    // Профиль пишется последним шагом установки, поэтому его наличие и значит «установлено»
    if !file.exists() {
        let profile = match loader {
            ModLoader::Fabric | ModLoader::Quilt => fabric::profile(&state.http, loader, mc, loader_version).await?,
            ModLoader::Forge | ModLoader::Neoforge => {
                forge::install(state, loader, loader_version, vanilla, progress).await?
            }
        };
        write_atomic(&file, &serde_json::to_vec_pretty(&profile)?).await?;
    }

    let profile: Value = serde_json::from_slice(&tokio::fs::read(&file).await?)?;
    let base: Value =
        serde_json::from_slice(&tokio::fs::read(state.paths.version_dir(mc).join(format!("{mc}.json"))).await?)?;
    serde_json::from_value(merge(base, profile)).map_err(|e| Error::msg(format!("loader profile {id}: {e}")))
}

/// Накладывает профиль загрузчика на ванильный JSON.
///
/// `id` остаётся ванильным: по нему ищется client.jar, а Forge 1.17+ исключает
/// `${version_name}.jar` из модулей — имя файла и переменная должны совпадать.
pub fn merge(mut base: Value, profile: Value) -> Value {
    let (Some(b), Value::Object(p)) = (base.as_object_mut(), profile) else { return base };

    for key in ["mainClass", "minecraftArguments"] {
        if let Some(v) = p.get(key) {
            b.insert(key.to_owned(), v.clone());
        }
    }

    if let Some(Value::Object(extra)) = p.get("arguments") {
        let args = b.entry("arguments").or_insert_with(|| json!({}));
        for kind in ["game", "jvm"] {
            if let (Some(Value::Array(more)), Some(target)) = (extra.get(kind), args.as_object_mut()) {
                if let Some(list) = target.entry(kind).or_insert_with(|| json!([])).as_array_mut() {
                    list.extend(more.iter().cloned());
                }
            }
        }
    }

    // Библиотеки загрузчика идут первыми и заменяют ванильные с тем же group:artifact
    let mut libs = p.get("libraries").and_then(Value::as_array).cloned().unwrap_or_default();
    let taken: HashSet<String> = libs.iter().filter_map(library_key).collect();
    if let Some(Value::Array(vanilla)) = b.get("libraries") {
        libs.extend(
            vanilla
                .iter()
                .filter(|l| library_key(l).is_none_or(|k| !taken.contains(&k)))
                .cloned(),
        );
    }
    b.insert("libraries".to_owned(), Value::Array(libs));
    base
}

/// group:artifact[:classifier] без версии.
fn library_key(lib: &Value) -> Option<String> {
    let name = lib.get("name")?.as_str()?;
    let name = name.split('@').next()?;
    let parts: Vec<&str> = name.split(':').collect();
    Some(match parts.as_slice() {
        [group, artifact, _version] => format!("{group}:{artifact}"),
        [group, artifact, _version, classifier, ..] => format!("{group}:{artifact}:{classifier}"),
        _ => name.to_owned(),
    })
}

/// Сравнение версий по числам: 47.3.10 новее 47.3.9.
pub(crate) fn version_key(v: &str) -> Vec<u64> {
    v.split(|c: char| !c.is_ascii_digit())
        .filter(|s| !s.is_empty())
        .map(|s| s.parse().unwrap_or(0))
        .collect()
}

pub(crate) fn is_prerelease(v: &str) -> bool {
    let v = v.to_ascii_lowercase();
    ["alpha", "beta", "pre", "rc", "snapshot"].iter().any(|tag| v.contains(tag))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merges_profile_over_vanilla() {
        let vanilla = json!({
            "id": "1.20.1",
            "mainClass": "net.minecraft.client.main.Main",
            "arguments": { "game": ["--username"], "jvm": ["-cp", "${classpath}"] },
            "libraries": [
                { "name": "org.ow2.asm:asm:9.3" },
                { "name": "org.lwjgl:lwjgl:3.3.1" },
                { "name": "org.lwjgl:lwjgl:3.3.1:natives-windows" }
            ]
        });
        let profile = json!({
            "id": "fabric-loader-0.16-1.20.1",
            "inheritsFrom": "1.20.1",
            "mainClass": "net.fabricmc.loader.impl.launch.knot.KnotClient",
            "arguments": { "jvm": ["-DFabricMcEmu= net.minecraft.client.main.Main "] },
            "libraries": [{ "name": "org.ow2.asm:asm:9.8" }]
        });
        let merged = merge(vanilla, profile);
        assert_eq!(merged["id"], "1.20.1");
        assert_eq!(merged["mainClass"], "net.fabricmc.loader.impl.launch.knot.KnotClient");
        assert_eq!(merged["arguments"]["jvm"].as_array().unwrap().len(), 3);
        assert_eq!(merged["arguments"]["game"].as_array().unwrap().len(), 1);
        let names: Vec<&str> =
            merged["libraries"].as_array().unwrap().iter().map(|l| l["name"].as_str().unwrap()).collect();
        assert_eq!(names, ["org.ow2.asm:asm:9.8", "org.lwjgl:lwjgl:3.3.1", "org.lwjgl:lwjgl:3.3.1:natives-windows"]);
    }

    #[test]
    fn compares_versions() {
        assert!(version_key("47.3.10") > version_key("47.3.9"));
        assert!(version_key("14.23.5.2860") > version_key("14.23.5.2859"));
        assert!(is_prerelease("21.0.0-beta"));
        assert!(!is_prerelease("0.17.2"));
    }
}
