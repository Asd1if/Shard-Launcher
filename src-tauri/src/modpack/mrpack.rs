//! Формат Modrinth `.mrpack`: `modrinth.index.json` + `overrides/` и `client-overrides/`.
//! Спецификация: https://support.modrinth.com/en/articles/8802351-modrinth-modpack-format-mrpack

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use tauri::ipc::Channel;

use super::{
    cleanup_on_error, create_instance, extract_overrides, linkable, safe_join, save_pack_files, write_zip, ExportArgs, ExportResult,
    ImportResult, PackFile, PackFormat, PackInfo, PackProgress,
};
use crate::content::{self, modrinth};
use crate::download::{download_all, Download};
use crate::error::{Error, Result};
use crate::instances::{Instance, ModLoader};
use crate::state::AppState;

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Index {
    pub(super) format_version: u32,
    pub(super) game: String,
    pub(super) version_id: String,
    pub(super) name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) summary: Option<String>,
    pub(super) files: Vec<IndexFile>,
    pub(super) dependencies: HashMap<String, String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct IndexFile {
    pub(super) path: String,
    pub(super) hashes: HashMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) env: Option<Env>,
    pub(super) downloads: Vec<String>,
    pub(super) file_size: u64,
}

#[derive(Debug, Serialize, Deserialize)]
pub(super) struct Env {
    pub(super) client: String,
    pub(super) server: String,
}

const LOADER_KEYS: [(&str, ModLoader); 4] = [
    ("fabric-loader", ModLoader::Fabric),
    ("quilt-loader", ModLoader::Quilt),
    ("forge", ModLoader::Forge),
    ("neoforge", ModLoader::Neoforge),
];

fn loader_key(loader: ModLoader) -> &'static str {
    LOADER_KEYS.iter().find(|(_, l)| *l == loader).map(|(k, _)| *k).expect("every loader has a key")
}

pub(super) async fn import(
    state: &AppState,
    mut archive: zip::ZipArchive<std::fs::File>,
    raw: &[u8],
    progress: &Channel<PackProgress>,
    target: Option<Instance>,
) -> Result<ImportResult> {
    let index: Index = serde_json::from_slice(raw).map_err(|e| Error::msg(format!("pack_bad_manifest: {e}")))?;
    if index.game != "minecraft" {
        return Err(Error::msg("pack_unknown_format"));
    }
    let mc_version =
        index.dependencies.get("minecraft").cloned().ok_or_else(|| Error::msg("pack_bad_manifest: no minecraft"))?;
    let (loader, loader_version) = LOADER_KEYS
        .iter()
        .find_map(|(key, loader)| index.dependencies.get(*key).map(|v| (Some(*loader), Some(v.clone()))))
        .unwrap_or((None, None));
    let info = PackInfo { name: index.name.clone(), mc_version, loader, loader_version };

    let fresh = target.is_none();
    let instance = create_instance(state, &info, target).await?;
    let result = async {
        let game_dir = state.paths.game_dir(&instance.id)?;
        let mut downloads = vec![];
        let mut written = vec![];
        for f in &index.files {
            // Файлы только для сервера игроку не нужны
            if f.env.as_ref().is_some_and(|e| e.client == "unsupported") {
                continue;
            }
            let url = f.downloads.first().ok_or_else(|| Error::msg(format!("no download for {}", f.path)))?;
            let sha1 = f.hashes.get("sha1").cloned().ok_or_else(|| Error::msg(format!("no sha1 for {}", f.path)))?;
            written.push(f.path.replace('\\', "/"));
            downloads.push(Download {
                url: url.clone(),
                path: safe_join(&game_dir, &f.path)?,
                sha1: Some(sha1),
                size: Some(f.file_size),
            });
        }
        download_all(&state.http, downloads, |done, total| {
            let _ = progress.send(PackProgress::Files { done, total });
        })
        .await?;

        let _ = progress.send(PackProgress::Overrides);
        let game_dir_blocking = game_dir.clone();
        let overrides = tokio::task::spawn_blocking(move || -> Result<Vec<String>> {
            let mut files = extract_overrides(&mut archive, "overrides", &game_dir_blocking)?;
            // client-overrides накладываются поверх общих
            files.extend(extract_overrides(&mut archive, "client-overrides", &game_dir_blocking)?);
            Ok(files)
        })
        .await
        .map_err(|e| Error::msg(e.to_string()))??;
        written.extend(overrides);
        save_pack_files(&state.paths, &instance.id, &written)
    }
    .await;
    cleanup_on_error(&state.paths, &instance, result, fresh).await?;

    let instance = finish_import(state, instance).await;
    Ok(ImportResult { instance, format: PackFormat::Modrinth, manual: vec![] })
}

/// Опознаёт поставленный контент (названия и иконки во вкладке «Контент») и пересчитывает моды.
pub(super) async fn finish_import(state: &AppState, mut instance: Instance) -> Instance {
    let _ = content::list(state, &instance.id).await;
    let _ = content::refresh_mod_count(&state.paths, &mut instance);
    instance
}

/// `https://modrinth.com/modpack/<slug>[/…]` → slug
pub(super) fn modrinth_project_slug(url: &str) -> Option<String> {
    let rest = url
        .trim()
        .trim_start_matches("https://")
        .trim_start_matches("http://")
        .trim_start_matches("www.")
        .strip_prefix("modrinth.com/modpack/")?;
    let slug = rest.split(['/', '?', '#']).next()?;
    (!slug.is_empty()).then(|| slug.to_owned())
}

/// Все версии модпака, новые сверху.
pub(super) async fn pack_versions(http: &reqwest::Client, project: &str) -> Result<Vec<modrinth::Version>> {
    let response = http.get(format!("https://api.modrinth.com/v2/project/{project}/version")).send().await?;
    if response.status() == reqwest::StatusCode::NOT_FOUND {
        return Err(Error::msg("pack_not_found"));
    }
    Ok(response.error_for_status()?.json().await?)
}

/// Версия модпака: выбранная или последняя (релиз важнее беты).
pub(super) async fn pack_version(http: &reqwest::Client, slug: &str, version_id: Option<&str>) -> Result<modrinth::Version> {
    match version_id {
        Some(id) => modrinth::version(http, id).await.map_err(|_| Error::msg("pack_not_found")),
        None => modrinth::best(pack_versions(http, slug).await?).ok_or_else(|| Error::msg("pack_not_found")),
    }
}

pub(super) async fn export(
    state: &AppState,
    instance: &Instance,
    loader_version: Option<String>,
    args: &ExportArgs,
    files: Vec<PackFile>,
    progress: &Channel<PackProgress>,
) -> Result<ExportResult> {
    // SHA1 всех файлов контента — по ним Modrinth отдаёт ссылки и SHA512
    let candidates: Vec<(usize, std::path::PathBuf)> =
        files.iter().enumerate().filter(|(_, f)| linkable(f)).map(|(i, f)| (i, f.path.clone())).collect();
    let hashed = tokio::task::spawn_blocking(move || {
        candidates
            .into_iter()
            .filter_map(|(i, path)| content::sha1_file(&path).ok().map(|h| (i, h)))
            .collect::<Vec<_>>()
    })
    .await
    .map_err(|e| Error::msg(e.to_string()))?;
    let hashes: Vec<String> = hashed.iter().map(|(_, h)| h.clone()).collect();
    // Без интернета экспорт всё равно возможен — просто всё ляжет в архив
    let found = modrinth::versions_by_hash(&state.http, &hashes).await.unwrap_or_default();

    let mut linked: HashMap<usize, IndexFile> = HashMap::new();
    for (i, sha1) in hashed {
        let Some(version) = found.get(&sha1) else { continue };
        let Some(vf) = version.files.iter().find(|f| f.hashes.get("sha1") == Some(&sha1)) else { continue };
        let Some(sha512) = vf.hashes.get("sha512") else { continue };
        let relative = &files[i].relative;
        let server = if relative.starts_with("mods/") { "required" } else { "unsupported" };
        linked.insert(
            i,
            IndexFile {
                path: relative.clone(),
                hashes: HashMap::from([("sha1".to_owned(), sha1.clone()), ("sha512".to_owned(), sha512.clone())]),
                env: Some(Env { client: "required".into(), server: server.into() }),
                downloads: vec![vf.url.clone()],
                file_size: vf.size,
            },
        );
    }

    let mut dependencies = HashMap::from([("minecraft".to_owned(), instance.mc_version.clone())]);
    if let (Some(loader), Some(version)) = (instance.loader, loader_version) {
        dependencies.insert(loader_key(loader).to_owned(), version);
    }
    let summary = args.summary.trim();
    let mut index_files: Vec<IndexFile> = vec![];
    let mut overrides: Vec<&PackFile> = vec![];
    for (i, f) in files.iter().enumerate() {
        match linked.remove(&i) {
            Some(entry) => index_files.push(entry),
            None => overrides.push(f),
        }
    }
    let index = Index {
        format_version: 1,
        game: "minecraft".into(),
        version_id: args.version.trim().to_owned(),
        name: args.name.trim().to_owned(),
        summary: (!summary.is_empty()).then(|| summary.to_owned()),
        files: index_files,
        dependencies,
    };
    let linked_count = index.files.len();
    let bundled = overrides.len();
    let manifest = serde_json::to_vec_pretty(&index)?;
    let target = std::path::PathBuf::from(&args.path);
    let progress = progress.clone();
    let overrides: Vec<PackFile> =
        overrides.into_iter().map(|f| PackFile { relative: f.relative.clone(), path: f.path.clone() }).collect();
    let size = tokio::task::spawn_blocking(move || {
        let refs: Vec<&PackFile> = overrides.iter().collect();
        write_zip(&target, "modrinth.index.json", &manifest, &refs, &progress)
    })
    .await
    .map_err(|e| Error::msg(e.to_string()))??;
    Ok(ExportResult { linked: linked_count, bundled, size })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_project_links() {
        assert_eq!(modrinth_project_slug("https://modrinth.com/modpack/fabulously-optimized").as_deref(), Some("fabulously-optimized"));
        assert_eq!(modrinth_project_slug("modrinth.com/modpack/cobblemon-fabric/versions?g=1.21").as_deref(), Some("cobblemon-fabric"));
        assert_eq!(modrinth_project_slug("https://modrinth.com/mod/sodium"), None);
        assert_eq!(modrinth_project_slug("https://cdn.modrinth.com/data/x/versions/y/pack.mrpack"), None);
    }

    #[test]
    fn parses_index() {
        let raw = r#"{
            "formatVersion": 1, "game": "minecraft", "versionId": "1.0", "name": "Pack",
            "files": [{"path": "mods/a.jar", "hashes": {"sha1": "aa", "sha512": "bb"},
                       "env": {"client": "required", "server": "required"},
                       "downloads": ["https://cdn.modrinth.com/a.jar"], "fileSize": 10}],
            "dependencies": {"minecraft": "1.21.1", "fabric-loader": "0.16.10"}
        }"#;
        let index: Index = serde_json::from_str(raw).unwrap();
        assert_eq!(index.files.len(), 1);
        assert_eq!(index.dependencies["fabric-loader"], "0.16.10");
    }
}
