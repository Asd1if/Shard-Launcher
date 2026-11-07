//! Fabric и Quilt: meta API сразу отдаёт готовый профиль, установка — это одна загрузка JSON.

use serde::Deserialize;
use serde_json::Value;

use super::{is_prerelease, LoaderVersion};
use crate::error::Result;
use crate::instances::ModLoader;

fn meta(loader: ModLoader) -> &'static str {
    match loader {
        ModLoader::Quilt => "https://meta.quiltmc.org/v3",
        _ => "https://meta.fabricmc.net/v2",
    }
}

#[derive(Deserialize)]
struct Entry {
    loader: Loader,
}

#[derive(Deserialize)]
struct Loader {
    version: String,
    /// У Quilt этого поля нет — там стабильность видна по суффиксу -beta
    stable: Option<bool>,
}

/// В id версий Minecraft бывают пробелы ("1.14 Pre-Release 1")
fn encode(mc: &str) -> String {
    mc.replace(' ', "%20")
}

pub async fn versions(http: &reqwest::Client, loader: ModLoader, mc: &str) -> Result<Vec<LoaderVersion>> {
    let url = format!("{}/versions/loader/{}", meta(loader), encode(mc));
    let response = http.get(url).send().await?;
    // Для неподдерживаемой версии Minecraft meta отвечает 400 или пустым списком
    if response.status().is_client_error() {
        return Ok(vec![]);
    }
    let list: Vec<Entry> = response.error_for_status()?.json().await?;
    Ok(list
        .into_iter()
        .map(|e| LoaderVersion {
            stable: e.loader.stable.unwrap_or_else(|| !is_prerelease(&e.loader.version)),
            version: e.loader.version,
            recommended: false,
        })
        .collect())
}

pub async fn profile(http: &reqwest::Client, loader: ModLoader, mc: &str, version: &str) -> Result<Value> {
    let url = format!("{}/versions/loader/{}/{version}/profile/json", meta(loader), encode(mc));
    Ok(http.get(url).send().await?.error_for_status()?.json().await?)
}
