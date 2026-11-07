//! Новости Minecraft: заметки к последним релизам и снапшотам Java Edition из
//! публичной ленты Mojang (без ключей).

use serde::{Deserialize, Serialize};
use tauri::State;

use crate::error::Result;
use crate::state::AppState;

const BASE: &str = "https://launchercontent.mojang.com";

#[derive(Deserialize)]
struct Feed {
    entries: Vec<Entry>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Entry {
    title: String,
    version: String,
    #[serde(rename = "type")]
    kind: String,
    image: Option<Image>,
    content_path: String,
    date: String,
    #[serde(default)]
    short_text: String,
}

#[derive(Deserialize)]
struct Image {
    url: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct News {
    pub title: String,
    pub version: String,
    /// release / snapshot
    pub kind: String,
    pub image: Option<String>,
    pub date: String,
    pub text: String,
    /// Путь к полному тексту для mc_news_body
    pub path: String,
}

#[tauri::command]
pub async fn mc_news(state: State<'_, AppState>) -> Result<Vec<News>> {
    let feed: Feed = state.http.get(format!("{BASE}/v2/javaPatchNotes.json")).send().await?.error_for_status()?.json().await?;
    let mut entries = feed.entries;
    entries.sort_by(|a, b| b.date.cmp(&a.date));
    Ok(entries
        .into_iter()
        .take(6)
        .map(|e| News {
            path: e.content_path,
            image: e.image.map(|i| format!("{BASE}{}", i.url)),
            title: e.title,
            version: e.version,
            kind: e.kind,
            date: e.date,
            text: e.short_text,
        })
        .collect())
}

#[derive(Deserialize)]
struct Body {
    body: String,
}

/// Полный текст заметки (HTML, на фронте проходит через DOMPurify).
#[tauri::command]
pub async fn mc_news_body(state: State<'_, AppState>, path: String) -> Result<String> {
    // Только файлы ленты, никаких произвольных адресов
    if !path.starts_with("javaPatchNotes/") || path.contains("..") || !path.ends_with(".json") {
        return Err(crate::error::Error::msg("bad_path"));
    }
    let body: Body = state.http.get(format!("{BASE}/v2/{path}")).send().await?.error_for_status()?.json().await?;
    Ok(body.body)
}
