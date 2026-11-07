//! Параллельная загрузка файлов с проверкой SHA1 и повторами.

use std::path::{Path, PathBuf};
use std::time::Duration;

use futures_util::{stream, StreamExt};
use sha1::{Digest, Sha1};

use crate::error::{Error, Result};

const PARALLEL: usize = 16;
const ATTEMPTS: u32 = 3;

#[derive(Debug, Clone)]
pub struct Download {
    pub url: String,
    pub path: PathBuf,
    pub sha1: Option<String>,
    pub size: Option<u64>,
}

/// Файл считается скачанным, если он есть и совпадает по размеру.
/// SHA1 проверяется при загрузке, а не при каждом запуске — иначе тысячи ассетов читались бы заново.
fn is_present(d: &Download) -> bool {
    match std::fs::metadata(&d.path) {
        Ok(meta) => d.size.is_none_or(|size| meta.len() == size),
        Err(_) => false,
    }
}

/// Качает всё, чего нет на диске. `progress(done, total)` вызывается после каждого файла.
pub async fn download_all(
    client: &reqwest::Client,
    items: Vec<Download>,
    mut progress: impl FnMut(usize, usize),
) -> Result<()> {
    let mut todo: Vec<Download> = items.into_iter().filter(|d| !is_present(d)).collect();
    // Один и тот же файл может встретиться дважды (например, одна библиотека в двух списках)
    todo.sort_by(|a, b| a.path.cmp(&b.path));
    todo.dedup_by(|a, b| a.path == b.path);

    let total = todo.len();
    progress(0, total);
    let mut results = stream::iter(todo)
        .map(|d| {
            let client = client.clone();
            async move { fetch(&client, &d).await }
        })
        .buffer_unordered(PARALLEL);

    let mut done = 0;
    while let Some(result) = results.next().await {
        result?;
        done += 1;
        progress(done, total);
    }
    Ok(())
}

pub async fn download_one(client: &reqwest::Client, item: &Download) -> Result<()> {
    if is_present(item) {
        return Ok(());
    }
    fetch(client, item).await
}

async fn fetch(client: &reqwest::Client, d: &Download) -> Result<()> {
    let mut last_error = None;
    for attempt in 0..ATTEMPTS {
        match try_fetch(client, d).await {
            Ok(()) => return Ok(()),
            Err(e) => last_error = Some(e),
        }
        tokio::time::sleep(Duration::from_millis(500 * u64::from(attempt + 1))).await;
    }
    Err(last_error.expect("at least one attempt"))
}

async fn try_fetch(client: &reqwest::Client, d: &Download) -> Result<()> {
    let bytes = client.get(&d.url).send().await?.error_for_status()?.bytes().await?;
    if let Some(expected) = &d.sha1 {
        let actual = hex::encode(Sha1::digest(&bytes));
        if !actual.eq_ignore_ascii_case(expected) {
            return Err(Error::msg(format!("checksum mismatch: {}", d.url)));
        }
    }
    write_atomic(&d.path, &bytes).await
}

/// Пишем во временный файл и переименовываем, чтобы прерванная загрузка не оставила обрезанный файл.
pub async fn write_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".part");
    tokio::fs::write(&tmp, bytes).await?;
    tokio::fs::rename(&tmp, path).await?;
    Ok(())
}
