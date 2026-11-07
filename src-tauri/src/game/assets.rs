//! Ассеты: индекс + объекты по хешам. Для очень старых версий — раскладка в «виртуальную» папку.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::download::{download_all, download_one, Download};
use crate::error::Result;

use super::version::AssetIndexRef;

const RESOURCES_URL: &str = "https://resources.download.minecraft.net";

#[derive(Debug, Deserialize)]
struct AssetIndex {
    objects: HashMap<String, AssetObject>,
    #[serde(default, rename = "virtual")]
    is_virtual: bool,
    #[serde(default)]
    map_to_resources: bool,
}

#[derive(Debug, Deserialize)]
struct AssetObject {
    hash: String,
    size: u64,
}

/// Возвращает папку для плейсхолдера ${game_assets} (нужна только старым версиям).
pub async fn ensure(
    client: &reqwest::Client,
    assets_dir: &Path,
    game_dir: &Path,
    index_ref: &AssetIndexRef,
    progress: impl FnMut(usize, usize),
) -> Result<PathBuf> {
    let index_path = assets_dir.join("indexes").join(format!("{}.json", index_ref.id));
    download_one(
        client,
        &Download {
            url: index_ref.url.clone(),
            path: index_path.clone(),
            sha1: Some(index_ref.sha1.clone()),
            size: Some(index_ref.size),
        },
    )
    .await?;
    let index: AssetIndex = serde_json::from_slice(&tokio::fs::read(&index_path).await?)?;

    let object_path = |hash: &str| assets_dir.join("objects").join(&hash[..2]).join(hash);
    let downloads = index
        .objects
        .values()
        .map(|o| Download {
            url: format!("{RESOURCES_URL}/{}/{}", &o.hash[..2], o.hash),
            path: object_path(&o.hash),
            sha1: Some(o.hash.clone()),
            size: Some(o.size),
        })
        .collect();
    download_all(client, downloads, progress).await?;

    let virtual_dir = assets_dir.join("virtual").join(&index_ref.id);
    // До 1.7.2 игра читает ассеты по обычным путям, а не по хешам
    if index.is_virtual || index.map_to_resources {
        let target = if index.map_to_resources { game_dir.join("resources") } else { virtual_dir.clone() };
        for (name, object) in &index.objects {
            let dest = target.join(name);
            if dest.metadata().is_ok_and(|m| m.len() == object.size) {
                continue;
            }
            if let Some(parent) = dest.parent() {
                tokio::fs::create_dir_all(parent).await?;
            }
            tokio::fs::copy(object_path(&object.hash), &dest).await?;
        }
    }
    Ok(virtual_dir)
}
