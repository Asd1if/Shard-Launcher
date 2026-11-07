//! Клиент CurseForge API v1. Ключ вшивается при сборке (см. build.rs): без него CurseForge выключен.

use std::collections::HashMap;

use serde::Deserialize;
use serde_json::json;

use super::modrinth::{SearchHit, SearchResult};
use super::{ContentKind, Meta, Source};
use crate::error::{Error, Result};
use crate::instances::ModLoader;

const API: &str = "https://api.curseforge.com/v1";
const MINECRAFT: u32 = 432;

/// Ключ API из console.curseforge.com, вшитый при сборке.
const API_KEY: Option<&str> = option_env!("SHARD_CURSEFORGE_API_KEY");

/// Свой ключ пользователя из настроек — главнее вшитого.
static USER_KEY: std::sync::RwLock<Option<String>> = std::sync::RwLock::new(None);

pub fn set_user_key(key: Option<String>) {
    *USER_KEY.write().expect("lock") = key.map(|k| k.trim().to_owned()).filter(|k| !k.is_empty());
}

pub fn enabled() -> bool {
    key().is_some()
}

pub fn built_in() -> bool {
    API_KEY.map(str::trim).is_some_and(|k| !k.is_empty())
}

fn key() -> Option<String> {
    USER_KEY.read().expect("lock").clone().or_else(|| API_KEY.map(str::trim).filter(|k| !k.is_empty()).map(str::to_owned))
}

/// Проверка ключа: CurseForge отвечает 403 на чужой или отозванный.
pub async fn check_key(http: &reqwest::Client, key: &str) -> Result<bool> {
    let response = http.get(format!("{API}/games/{MINECRAFT}")).header("x-api-key", key.trim()).send().await?;
    Ok(response.status().is_success())
}

pub(crate) fn class_id(kind: ContentKind) -> u32 {
    match kind {
        ContentKind::Mod => 6,
        ContentKind::Resourcepack => 12,
        ContentKind::Shader => 6552,
    }
}

/// Номер загрузчика для поиска. Моды Fabric работают и в Quilt, а отдельных модов под Quilt
/// на CurseForge почти нет — поэтому для Quilt ищем моды Fabric.
fn loader_type(loader: ModLoader) -> u32 {
    match loader {
        ModLoader::Forge => 1,
        ModLoader::Fabric | ModLoader::Quilt => 4,
        ModLoader::Neoforge => 6,
    }
}

/// Как загрузчик подписан в списке gameVersions файла
fn loader_tags(loader: ModLoader) -> &'static [&'static str] {
    match loader {
        ModLoader::Forge => &["forge"],
        ModLoader::Fabric => &["fabric"],
        ModLoader::Quilt => &["quilt", "fabric"],
        ModLoader::Neoforge => &["neoforge"],
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Mod {
    pub id: u64,
    #[serde(default)]
    pub class_id: Option<u32>,
    pub name: String,
    pub slug: String,
    #[serde(default)]
    pub summary: String,
    #[serde(default)]
    pub download_count: f64,
    #[serde(default)]
    pub thumbs_up_count: u64,
    pub logo: Option<Logo>,
    #[serde(default)]
    pub authors: Vec<Author>,
    pub links: Option<Links>,
    #[serde(default)]
    pub date_modified: String,
    #[serde(default)]
    pub latest_files_indexes: Vec<FileIndex>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Logo {
    pub thumbnail_url: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Author {
    pub name: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Links {
    pub website_url: Option<String>,
    /// Репозиторий сборки — там часто лежат серверные файлы
    #[serde(default)]
    pub source_url: Option<String>,
    #[serde(default)]
    pub issues_url: Option<String>,
    #[serde(default)]
    pub wiki_url: Option<String>,
}

/// Поля проекта для страницы в лаунчере: скриншоты и ссылки.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModExtras {
    #[serde(default)]
    pub screenshots: Vec<Screenshot>,
    pub links: Option<Links>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Screenshot {
    pub url: String,
    pub thumbnail_url: Option<String>,
    pub title: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileIndex {
    pub game_version: String,
    pub file_id: u64,
    pub filename: String,
    pub release_type: u8,
    pub mod_loader: Option<u32>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct File {
    pub id: u64,
    #[serde(default)]
    pub mod_id: u64,
    pub display_name: String,
    pub file_name: String,
    pub release_type: u8,
    pub file_date: String,
    pub file_length: u64,
    /// null — автор запретил скачивание из сторонних программ
    pub download_url: Option<String>,
    #[serde(default)]
    pub hashes: Vec<Hash>,
    #[serde(default)]
    pub dependencies: Vec<FileDependency>,
    #[serde(default)]
    pub game_versions: Vec<String>,
    /// Готовый серверный архив, который автор сборки приложил к этой версии
    #[serde(default)]
    pub server_pack_file_id: Option<u64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Hash {
    pub value: String,
    pub algo: u8,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileDependency {
    pub mod_id: u64,
    pub relation_type: u8,
}

impl File {
    pub fn sha1(&self) -> Option<String> {
        self.hashes.iter().find(|h| h.algo == 1).map(|h| h.value.clone())
    }

    /// Обязательные зависимости (FileRelationType 3)
    pub fn required(&self) -> impl Iterator<Item = u64> + '_ {
        self.dependencies.iter().filter(|d| d.relation_type == 3).map(|d| d.mod_id)
    }
}

#[derive(Deserialize)]
struct Data<T> {
    data: T,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Pagination {
    total_count: u64,
}

#[derive(Deserialize)]
struct SearchResponse {
    data: Vec<Mod>,
    pagination: Pagination,
}

fn request(http: &reqwest::Client, method: reqwest::Method, path: &str) -> Result<reqwest::RequestBuilder> {
    let key = key().ok_or_else(|| Error::msg("curseforge_no_key"))?;
    Ok(http.request(method, format!("{API}{path}")).header("x-api-key", key).header("Accept", "application/json"))
}

async fn send<T: serde::de::DeserializeOwned>(builder: reqwest::RequestBuilder) -> Result<T> {
    let response = builder.send().await?;
    if response.status() == reqwest::StatusCode::FORBIDDEN {
        return Err(Error::msg("curseforge_bad_key"));
    }
    Ok(response.error_for_status()?.json().await?)
}

/// `loader` — оставить в versions только версии игры с файлами под этот загрузчик.
pub fn hit_for(m: Mod, loader: Option<ModLoader>) -> SearchHit {
    let mut game_versions: Vec<String> = vec![];
    for f in &m.latest_files_indexes {
        let fits = match (loader, f.mod_loader) {
            (Some(l), Some(ml)) => ml == loader_type(l),
            _ => true,
        };
        if fits && !game_versions.contains(&f.game_version) {
            game_versions.push(f.game_version.clone());
        }
    }
    SearchHit {
        game_versions,
        source: Source::Curseforge,
        page_url: m.links.and_then(|l| l.website_url),
        project_id: m.id.to_string(),
        slug: m.slug,
        title: m.name,
        description: m.summary,
        author: m.authors.into_iter().next().map(|a| a.name).unwrap_or_default(),
        icon_url: m.logo.and_then(|l| l.thumbnail_url),
        downloads: m.download_count as u64,
        follows: m.thumbs_up_count,
        display_categories: vec![],
        date_modified: m.date_modified,
    }
}

/// classId модпаков на CurseForge
pub const MODPACK_CLASS: u32 = 4471;

#[cfg_attr(not(test), allow(dead_code))]
pub async fn search(
    http: &reqwest::Client,
    query: &str,
    kind: ContentKind,
    game_version: Option<&str>,
    loader: Option<ModLoader>,
    sort: &str,
    offset: u32,
) -> Result<SearchResult> {
    // Фильтр по загрузчику нужен только модам
    let loader = if kind == ContentKind::Mod { loader } else { None };
    search_filtered(http, query, class_id(kind), game_version, loader, sort, offset, &[]).await
}

/// Поиск с фильтром по категориям (id категорий CurseForge).
#[allow(clippy::too_many_arguments)]
pub async fn search_filtered(
    http: &reqwest::Client,
    query: &str,
    class: u32,
    game_version: Option<&str>,
    loader: Option<ModLoader>,
    sort: &str,
    offset: u32,
    categories: &[String],
) -> Result<SearchResult> {
    // ModsSearchSortField: 2 Popularity, 3 LastUpdated, 6 TotalDownloads, 11 ReleasedDate
    let sort_field = match sort {
        "downloads" => 6,
        "updated" => 3,
        "newest" => 11,
        _ => 2,
    };
    let mut params = vec![
        ("gameId", MINECRAFT.to_string()),
        ("classId", class.to_string()),
        ("searchFilter", query.to_owned()),
        ("sortField", sort_field.to_string()),
        ("sortOrder", "desc".to_owned()),
        ("index", offset.to_string()),
        ("pageSize", "20".to_owned()),
    ];
    let category_ids: Vec<u64> = categories.iter().filter_map(|c| c.parse().ok()).take(10).collect();
    if !category_ids.is_empty() {
        params.push(("categoryIds", serde_json::to_string(&category_ids)?));
    }
    if let Some(v) = game_version {
        params.push(("gameVersion", v.to_owned()));
        // Фильтр по загрузчику работает только вместе с версией игры
        if let Some(loader) = loader {
            params.push(("modLoaderType", loader_type(loader).to_string()));
        }
    }
    let response: SearchResponse = send(request(http, reqwest::Method::GET, "/mods/search")?.query(&params)).await?;
    Ok(SearchResult {
        total_hits: response.pagination.total_count,
        hits: response.data.into_iter().map(|m| hit_for(m, loader)).collect(),
        other: vec![],
    })
}

/// Категории класса (моды, ресурспаки, …): id и название, без самих классов.
pub async fn categories(http: &reqwest::Client, class: u32) -> Result<Vec<(u64, String)>> {
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Cat {
        id: u64,
        name: String,
        #[serde(default)]
        is_class: Option<bool>,
    }
    let params = [("gameId", MINECRAFT.to_string()), ("classId", class.to_string())];
    let list: Data<Vec<Cat>> = send(request(http, reqwest::Method::GET, "/categories")?.query(&params)).await?;
    let mut out: Vec<(u64, String)> = list.data.into_iter().filter(|c| c.is_class != Some(true)).map(|c| (c.id, c.name)).collect();
    out.sort_by(|a, b| a.1.cmp(&b.1));
    Ok(out)
}

pub async fn get_mod(http: &reqwest::Client, id: u64) -> Result<Mod> {
    Ok(send::<Data<Mod>>(request(http, reqwest::Method::GET, &format!("/mods/{id}"))?).await?.data)
}

pub async fn mod_extras(http: &reqwest::Client, id: u64) -> Result<ModExtras> {
    Ok(send::<Data<ModExtras>>(request(http, reqwest::Method::GET, &format!("/mods/{id}"))?).await?.data)
}

/// Описание проекта (HTML).
pub async fn description(http: &reqwest::Client, id: u64) -> Result<String> {
    Ok(send::<Data<String>>(request(http, reqwest::Method::GET, &format!("/mods/{id}/description"))?).await?.data)
}

pub async fn get_mods(http: &reqwest::Client, ids: &[u64]) -> Result<Vec<Mod>> {
    if ids.is_empty() {
        return Ok(vec![]);
    }
    let body = json!({ "modIds": ids, "filterPcOnly": true });
    Ok(send::<Data<Vec<Mod>>>(request(http, reqwest::Method::POST, "/mods")?.json(&body)).await?.data)
}

pub async fn get_file(http: &reqwest::Client, mod_id: u64, file_id: u64) -> Result<File> {
    Ok(send::<Data<File>>(request(http, reqwest::Method::GET, &format!("/mods/{mod_id}/files/{file_id}"))?)
        .await?
        .data)
}

/// Файлы по их id одним запросом (для манифестов сборок).
pub async fn get_files(http: &reqwest::Client, ids: &[u64]) -> Result<Vec<File>> {
    if ids.is_empty() {
        return Ok(vec![]);
    }
    let body = json!({ "fileIds": ids });
    Ok(send::<Data<Vec<File>>>(request(http, reqwest::Method::POST, "/mods/files")?.json(&body)).await?.data)
}

/// Тип контента по classId проекта: в сборках CurseForge встречаются и ресурспаки, и шейдеры.
pub fn kind_for_class(class_id: Option<u32>) -> ContentKind {
    match class_id {
        Some(12) => ContentKind::Resourcepack,
        Some(6552) => ContentKind::Shader,
        _ => ContentKind::Mod,
    }
}

/// Подходящие файлы проекта, новые сверху. Загрузчик проверяем по списку gameVersions файла —
/// так надёжнее, чем фильтр API.
pub async fn files(
    http: &reqwest::Client,
    mod_id: u64,
    kind: ContentKind,
    game_version: &str,
    loader: Option<ModLoader>,
) -> Result<Vec<File>> {
    // Пустая версия игры — любая
    let mut params = vec![("pageSize", "50".to_owned())];
    if !game_version.is_empty() {
        params.push(("gameVersion", game_version.to_owned()));
    }
    let mut list: Vec<File> =
        send::<Data<Vec<File>>>(request(http, reqwest::Method::GET, &format!("/mods/{mod_id}/files"))?.query(&params))
            .await?
            .data;
    if let (ContentKind::Mod, Some(loader)) = (kind, loader) {
        let tags = loader_tags(loader);
        list.retain(|f| f.game_versions.iter().any(|v| tags.contains(&v.to_ascii_lowercase().as_str())));
    }
    if !game_version.is_empty() {
        list.retain(|f| f.game_versions.iter().any(|v| v == game_version));
    }
    list.sort_by(|a, b| b.file_date.cmp(&a.file_date));
    Ok(list)
}

/// Последний файл проекта без фильтров (для модпаков: у них один файл на версию).
pub async fn latest_file(http: &reqwest::Client, mod_id: u64) -> Result<Option<File>> {
    let params = [("pageSize", "50")];
    let mut list: Vec<File> =
        send::<Data<Vec<File>>>(request(http, reqwest::Method::GET, &format!("/mods/{mod_id}/files"))?.query(&params))
            .await?
            .data;
    list.sort_by(|a, b| b.file_date.cmp(&a.file_date));
    Ok(best(list))
}

/// Страница списка файлов проекта.
pub async fn files_page(http: &reqwest::Client, mod_id: u64, index: u32, size: u32) -> Result<Vec<File>> {
    let params = [("index", index.to_string()), ("pageSize", size.to_string())];
    Ok(send::<Data<Vec<File>>>(request(http, reqwest::Method::GET, &format!("/mods/{mod_id}/files"))?.query(&params))
        .await?
        .data)
}

/// Последний релиз, иначе последняя бета или альфа.
pub fn best(files: Vec<File>) -> Option<File> {
    let index = files.iter().position(|f| f.release_type == 1).unwrap_or(0);
    files.into_iter().nth(index)
}

/// Страница ручной загрузки файла на сайте CurseForge.
pub fn manual_url(page_url: Option<&str>, file_id: u64) -> Option<String> {
    page_url.map(|p| format!("{}/download/{file_id}", p.trim_end_matches('/')))
}

/// Отпечаток файла для CurseForge: MurmurHash2 (seed 1) по байтам без пробельных символов.
pub fn fingerprint(bytes: &[u8]) -> u32 {
    let data: Vec<u8> = bytes.iter().copied().filter(|b| !matches!(b, 9 | 10 | 13 | 32)).collect();
    murmur2(&data, 1)
}

fn murmur2(data: &[u8], seed: u32) -> u32 {
    const M: u32 = 0x5bd1_e995;
    let mut h = seed ^ data.len() as u32;
    let mut chunks = data.chunks_exact(4);
    for c in &mut chunks {
        let mut k = u32::from_le_bytes([c[0], c[1], c[2], c[3]]);
        k = k.wrapping_mul(M);
        k ^= k >> 24;
        k = k.wrapping_mul(M);
        h = h.wrapping_mul(M) ^ k;
    }
    let rest = chunks.remainder();
    if !rest.is_empty() {
        if rest.len() >= 3 {
            h ^= u32::from(rest[2]) << 16;
        }
        if rest.len() >= 2 {
            h ^= u32::from(rest[1]) << 8;
        }
        h ^= u32::from(rest[0]);
        h = h.wrapping_mul(M);
    }
    h ^= h >> 13;
    h = h.wrapping_mul(M);
    h ^ (h >> 15)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct FingerprintMatches {
    exact_matches: Vec<FingerprintMatch>,
}

#[derive(Deserialize)]
struct FingerprintMatch {
    id: u64,
    file: FingerprintFile,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct FingerprintFile {
    id: u64,
    display_name: String,
    file_fingerprint: u32,
}

/// Опознаёт файлы по отпечаткам: отпечаток → сведения о проекте и файле.
pub async fn identify(http: &reqwest::Client, fingerprints: &[u32]) -> Result<HashMap<u32, Meta>> {
    if fingerprints.is_empty() {
        return Ok(HashMap::new());
    }
    let body = json!({ "fingerprints": fingerprints });
    let matches: Data<FingerprintMatches> =
        send(request(http, reqwest::Method::POST, &format!("/fingerprints/{MINECRAFT}"))?.json(&body)).await?;
    let ids: Vec<u64> = matches.data.exact_matches.iter().map(|m| m.id).collect();
    let mods: HashMap<u64, Mod> = get_mods(http, &ids).await?.into_iter().map(|m| (m.id, m)).collect();
    Ok(matches
        .data
        .exact_matches
        .into_iter()
        .map(|m| {
            let project = mods.get(&m.id);
            let meta = Meta {
                source: Source::Curseforge,
                project_id: m.id.to_string(),
                version_id: m.file.id.to_string(),
                title: project.map_or_else(|| m.file.display_name.clone(), |p| p.name.clone()),
                version_number: m.file.display_name,
                icon_url: project.and_then(|p| p.logo.as_ref()).and_then(|l| l.thumbnail_url.clone()),
            };
            (m.file.file_fingerprint, meta)
        })
        .collect())
}

/// Новейший подходящий файл из latestFilesIndexes проекта (релизы важнее бет).
pub fn latest_for(m: &Mod, kind: ContentKind, game_version: &str, loader: Option<ModLoader>) -> Option<FileIndex> {
    let fits = |f: &&FileIndex| {
        f.game_version == game_version
            && match (kind, loader) {
                (ContentKind::Mod, Some(l)) => f.mod_loader.is_none_or(|ml| ml == loader_type(l) || (l == ModLoader::Quilt && ml == 5)),
                _ => true,
            }
    };
    let candidates: Vec<&FileIndex> = m.latest_files_indexes.iter().filter(fits).collect();
    candidates
        .iter()
        .filter(|f| f.release_type == 1)
        .max_by_key(|f| f.file_id)
        .or_else(|| candidates.iter().max_by_key(|f| f.file_id))
        .map(|f| (*f).clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Эталонная проверка SMHasher: хеши ключей 0..255 с seed 256-i, затем хеш от их конкатенации.
    #[test]
    fn murmur2_matches_smhasher() {
        let key: Vec<u8> = (0..=255u8).collect();
        let mut hashes = Vec::with_capacity(1024);
        for i in 0..256 {
            hashes.extend_from_slice(&murmur2(&key[..i], 256 - i as u32).to_le_bytes());
        }
        assert_eq!(murmur2(&hashes, 0), 0x2786_4C1E);
    }

    #[test]
    fn fingerprint_skips_whitespace() {
        assert_eq!(fingerprint(b"a b\tc\r\nd"), fingerprint(b"abcd"));
        assert_ne!(fingerprint(b"abcd"), fingerprint(b"abce"));
    }

    #[test]
    fn parses_file() {
        let file: File = serde_json::from_str(
            r#"{"id": 4712345, "gameId": 432, "modId": 238222, "isAvailable": true,
                "displayName": "JEI 15.2.0.27", "fileName": "jei-1.20.1-forge-15.2.0.27.jar",
                "releaseType": 1, "fileStatus": 4,
                "hashes": [{"value": "da39a3ee5e6b4b0d3255bfef95601890afd80709", "algo": 1}, {"value": "x", "algo": 2}],
                "fileDate": "2023-08-01T00:00:00Z", "fileLength": 1200000, "downloadCount": 5,
                "downloadUrl": null,
                "gameVersions": ["1.20.1", "Forge", "Client"],
                "dependencies": [{"modId": 1, "relationType": 3}, {"modId": 2, "relationType": 2}],
                "fileFingerprint": 123}"#,
        )
        .unwrap();
        assert_eq!(file.sha1().as_deref(), Some("da39a3ee5e6b4b0d3255bfef95601890afd80709"));
        assert!(file.download_url.is_none());
        assert_eq!(file.required().collect::<Vec<_>>(), vec![1]);
    }

    #[test]
    fn picks_latest_release_for_loader() {
        let m: Mod = serde_json::from_str(
            r#"{"id": 1, "name": "X", "slug": "x", "latestFilesIndexes": [
                {"gameVersion": "1.20.1", "fileId": 10, "filename": "a.jar", "releaseType": 1, "modLoader": 1},
                {"gameVersion": "1.20.1", "fileId": 12, "filename": "b.jar", "releaseType": 2, "modLoader": 1},
                {"gameVersion": "1.20.1", "fileId": 15, "filename": "c.jar", "releaseType": 1, "modLoader": 4},
                {"gameVersion": "1.21.1", "fileId": 20, "filename": "d.jar", "releaseType": 1, "modLoader": 1}
            ]}"#,
        )
        .unwrap();
        let f = latest_for(&m, ContentKind::Mod, "1.20.1", Some(ModLoader::Forge)).unwrap();
        assert_eq!(f.file_id, 10);
        let f = latest_for(&m, ContentKind::Mod, "1.20.1", Some(ModLoader::Quilt)).unwrap();
        assert_eq!(f.file_id, 15);
    }
}
