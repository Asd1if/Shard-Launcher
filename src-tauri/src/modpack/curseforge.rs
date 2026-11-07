//! Формат CurseForge: `manifest.json` (projectID/fileID) + папка overrides.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use tauri::ipc::Channel;

use super::{
    cleanup_on_error, create_instance, extract_overrides, linkable, save_pack_files, mrpack::finish_import, write_zip, ExportArgs,
    ExportResult, ImportResult, PackFile, PackFormat, PackInfo, PackProgress,
};
use crate::content::{self, curseforge, ManualDownload, Meta, Source};
use crate::download::{download_all, Download};
use crate::error::{Error, Result};
use crate::instances::{Instance, ModLoader};
use crate::state::AppState;

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Manifest {
    minecraft: Minecraft,
    #[serde(default = "manifest_type")]
    manifest_type: String,
    #[serde(default = "one")]
    manifest_version: u32,
    #[serde(default)]
    name: String,
    #[serde(default)]
    version: String,
    #[serde(default)]
    author: String,
    #[serde(default)]
    files: Vec<ManifestFile>,
    #[serde(default = "overrides")]
    overrides: String,
}

fn manifest_type() -> String {
    "minecraftModpack".into()
}
fn one() -> u32 {
    1
}
fn overrides() -> String {
    "overrides".into()
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Minecraft {
    version: String,
    #[serde(default)]
    mod_loaders: Vec<LoaderEntry>,
}

#[derive(Debug, Serialize, Deserialize)]
struct LoaderEntry {
    id: String,
    #[serde(default)]
    primary: bool,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ManifestFile {
    #[serde(rename = "projectID")]
    project_id: u64,
    #[serde(rename = "fileID")]
    file_id: u64,
    #[serde(default = "yes")]
    required: bool,
}

fn yes() -> bool {
    true
}

const LOADER_PREFIXES: [(&str, ModLoader); 4] = [
    ("neoforge-", ModLoader::Neoforge),
    ("forge-", ModLoader::Forge),
    ("fabric-", ModLoader::Fabric),
    ("quilt-", ModLoader::Quilt),
];

/// `forge-47.2.0` → (Forge, 47.2.0)
fn parse_loader(id: &str) -> Option<(ModLoader, String)> {
    LOADER_PREFIXES.iter().find_map(|(prefix, loader)| id.strip_prefix(prefix).map(|v| (*loader, v.to_owned())))
}

fn loader_id(loader: ModLoader, version: &str) -> String {
    let prefix = LOADER_PREFIXES.iter().find(|(_, l)| *l == loader).map(|(p, _)| *p).expect("every loader has a prefix");
    format!("{prefix}{version}")
}

/// Ссылка на архив последней версии модпака CurseForge.
/// Файл модпака: выбранный или последний.
pub(super) async fn pack_file(state: &AppState, id: u64, file_id: Option<u64>) -> Result<curseforge::File> {
    match file_id {
        Some(file_id) => curseforge::get_file(&state.http, id, file_id).await,
        None => curseforge::latest_file(&state.http, id).await?.ok_or_else(|| Error::msg("pack_not_found")),
    }
}

/// Все файлы модпака, новые сверху (до 200 — у старых сборок их бывают сотни).
pub(super) async fn pack_files(state: &AppState, id: u64) -> Result<Vec<curseforge::File>> {
    let mut all = vec![];
    for page in 0..4 {
        let files = curseforge::files_page(&state.http, id, page * 50, 50).await?;
        let last = files.len() < 50;
        all.extend(files);
        if last {
            break;
        }
    }
    all.sort_by(|a, b| b.file_date.cmp(&a.file_date));
    Ok(all)
}

pub(super) async fn import(
    state: &AppState,
    mut archive: zip::ZipArchive<std::fs::File>,
    raw: &[u8],
    progress: &Channel<PackProgress>,
    target: Option<Instance>,
) -> Result<ImportResult> {
    let manifest: Manifest = serde_json::from_slice(raw).map_err(|e| Error::msg(format!("pack_bad_manifest: {e}")))?;
    if manifest.manifest_type != "minecraftModpack" {
        return Err(Error::msg("pack_unknown_format"));
    }
    let files: Vec<&ManifestFile> = manifest.files.iter().filter(|f| f.required).collect();
    // Без ключа ссылки projectID/fileID не во что превратить — лучше сказать сразу, чем собрать пустую сборку
    if !files.is_empty() && !curseforge::enabled() {
        return Err(Error::msg("curseforge_no_key"));
    }
    let primary = manifest.minecraft.mod_loaders.iter().find(|l| l.primary).or(manifest.minecraft.mod_loaders.first());
    let (loader, loader_version) = primary.and_then(|l| parse_loader(&l.id)).map_or((None, None), |(l, v)| (Some(l), Some(v)));
    let info = PackInfo { name: manifest.name.clone(), mc_version: manifest.minecraft.version.clone(), loader, loader_version };

    let fresh = target.is_none();
    let instance = create_instance(state, &info, target).await?;
    let result = async {
        let game_dir = state.paths.game_dir(&instance.id)?;
        let ids: Vec<u64> = files.iter().map(|f| f.file_id).collect();
        let cf_files = curseforge::get_files(&state.http, &ids).await?;
        let mod_ids: Vec<u64> = cf_files.iter().map(|f| f.mod_id).collect();
        let mods: HashMap<u64, curseforge::Mod> =
            curseforge::get_mods(&state.http, &mod_ids).await?.into_iter().map(|m| (m.id, m)).collect();

        let mut downloads = vec![];
        let mut written = vec![];
        let mut known = vec![];
        let mut manual = vec![];
        for file in cf_files {
            let project = mods.get(&file.mod_id);
            let kind = curseforge::kind_for_class(project.and_then(|p| p.class_id));
            let file_name = content::checked_name(&file.file_name)?.to_owned();
            let meta = Meta {
                source: Source::Curseforge,
                project_id: file.mod_id.to_string(),
                version_id: file.id.to_string(),
                title: project.map_or_else(|| file.display_name.clone(), |p| p.name.clone()),
                version_number: file.display_name.clone(),
                icon_url: project.and_then(|p| p.logo.as_ref()).and_then(|l| l.thumbnail_url.clone()),
            };
            match &file.download_url {
                Some(url) => {
                    written.push(format!("{}/{file_name}", kind.folder()));
                    downloads.push(Download {
                        url: url.clone(),
                        path: game_dir.join(kind.folder()).join(&file_name),
                        sha1: file.sha1(),
                        size: Some(file.file_length),
                    });
                    if let Some(sha1) = file.sha1() {
                        known.push((sha1, meta));
                    }
                }
                None => {
                    let page = project.and_then(|p| p.links.as_ref()).and_then(|l| l.website_url.as_deref());
                    let Some(url) = curseforge::manual_url(page, file.id) else { continue };
                    manual.push(ManualDownload { kind, file_name, url, sha1: file.sha1(), size: file.file_length, meta });
                }
            }
        }
        download_all(&state.http, downloads, |done, total| {
            let _ = progress.send(PackProgress::Files { done, total });
        })
        .await?;
        for (sha1, meta) in known {
            content::remember(&state.paths, &instance.id, &sha1, meta)?;
        }

        let _ = progress.send(PackProgress::Overrides);
        let prefix = manifest.overrides.clone();
        let game_dir_blocking = game_dir.clone();
        let overrides = tokio::task::spawn_blocking(move || extract_overrides(&mut archive, &prefix, &game_dir_blocking))
            .await
            .map_err(|e| Error::msg(e.to_string()))??;
        written.extend(overrides);
        // Файлы для ручной загрузки тоже принадлежат модпаку
        written.extend(manual.iter().map(|m: &ManualDownload| format!("{}/{}", m.kind.folder(), m.file_name)));
        save_pack_files(&state.paths, &instance.id, &written)?;
        Ok(manual)
    }
    .await;
    let manual = cleanup_on_error(&state.paths, &instance, result, fresh).await?;

    let instance = finish_import(state, instance).await;
    Ok(ImportResult { instance, format: PackFormat::Curseforge, manual })
}

pub(super) async fn export(
    state: &AppState,
    instance: &Instance,
    loader_version: Option<String>,
    args: &ExportArgs,
    files: Vec<PackFile>,
    progress: &Channel<PackProgress>,
) -> Result<ExportResult> {
    let candidates: Vec<(usize, std::path::PathBuf)> =
        files.iter().enumerate().filter(|(_, f)| linkable(f)).map(|(i, f)| (i, f.path.clone())).collect();
    let prints = tokio::task::spawn_blocking(move || {
        candidates
            .into_iter()
            .filter_map(|(i, path)| std::fs::read(path).ok().map(|b| (i, curseforge::fingerprint(&b))))
            .collect::<Vec<_>>()
    })
    .await
    .map_err(|e| Error::msg(e.to_string()))?;
    let values: Vec<u32> = prints.iter().map(|(_, p)| *p).collect();
    // Без ключа или интернета всё ляжет в overrides — сборка останется рабочей, просто тяжелее
    let found = if curseforge::enabled() { curseforge::identify(&state.http, &values).await.unwrap_or_default() } else { HashMap::new() };

    let mut linked: HashMap<usize, ManifestFile> = HashMap::new();
    for (i, print) in prints {
        let Some(meta) = found.get(&print) else { continue };
        let (Ok(project_id), Ok(file_id)) = (meta.project_id.parse(), meta.version_id.parse()) else { continue };
        linked.insert(i, ManifestFile { project_id, file_id, required: true });
    }

    let mut manifest_files = vec![];
    let mut overrides = vec![];
    for (i, f) in files.iter().enumerate() {
        match linked.remove(&i) {
            Some(entry) => manifest_files.push(entry),
            None => overrides.push(PackFile { relative: f.relative.clone(), path: f.path.clone() }),
        }
    }
    let mod_loaders = match (instance.loader, loader_version) {
        (Some(loader), Some(version)) => vec![LoaderEntry { id: loader_id(loader, &version), primary: true }],
        _ => vec![],
    };
    let manifest = Manifest {
        minecraft: Minecraft { version: instance.mc_version.clone(), mod_loaders },
        manifest_type: manifest_type(),
        manifest_version: 1,
        name: args.name.trim().to_owned(),
        version: args.version.trim().to_owned(),
        author: args.author.trim().to_owned(),
        files: manifest_files,
        overrides: "overrides".into(),
    };
    let linked_count = manifest.files.len();
    let bundled = overrides.len();
    let bytes = serde_json::to_vec_pretty(&manifest)?;
    let target = std::path::PathBuf::from(&args.path);
    let progress = progress.clone();
    let size = tokio::task::spawn_blocking(move || {
        let refs: Vec<&PackFile> = overrides.iter().collect();
        write_zip(&target, "manifest.json", &bytes, &refs, &progress)
    })
    .await
    .map_err(|e| Error::msg(e.to_string()))??;
    Ok(ExportResult { linked: linked_count, bundled, size })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_loader_ids() {
        assert_eq!(parse_loader("forge-47.2.0"), Some((ModLoader::Forge, "47.2.0".into())));
        assert_eq!(parse_loader("neoforge-21.1.77"), Some((ModLoader::Neoforge, "21.1.77".into())));
        assert_eq!(parse_loader("fabric-0.16.10"), Some((ModLoader::Fabric, "0.16.10".into())));
        assert_eq!(parse_loader("liteloader-1"), None);
        assert_eq!(loader_id(ModLoader::Quilt, "0.27.1"), "quilt-0.27.1");
    }

    #[test]
    fn parses_manifest() {
        let raw = r#"{
            "minecraft": {"version": "1.20.1", "modLoaders": [{"id": "forge-47.2.0", "primary": true}]},
            "manifestType": "minecraftModpack", "manifestVersion": 1,
            "name": "Pack", "version": "1.0", "author": "me",
            "files": [{"projectID": 238222, "fileID": 4712, "required": true}],
            "overrides": "overrides"
        }"#;
        let m: Manifest = serde_json::from_str(raw).unwrap();
        assert_eq!(m.files[0].project_id, 238222);
        assert_eq!(m.minecraft.mod_loaders[0].id, "forge-47.2.0");
    }
}
