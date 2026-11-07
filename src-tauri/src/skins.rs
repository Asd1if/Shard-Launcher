//! Скины: локальная библиотека и установка скина и плаща на Microsoft-аккаунт.
//!
//! Библиотека не зависит от аккаунтов: `<root>/skins/<id>.png` и `skins.json`. Выход из аккаунта
//! или его удаление её не трогают. Надеть скин в игре можно только на Microsoft-аккаунт —
//! через Minecraft Services API.

use base64::Engine;
use serde::{Deserialize, Serialize};
use sha1::{Digest, Sha1};
use tauri::State;

use crate::accounts::{self, AccountKind};
use crate::error::{Error, Result};
use crate::paths::Paths;
use crate::state::AppState;

const PROFILE_URL: &str = "https://api.minecraftservices.com/minecraft/profile";
const MOJANG_PROFILE_URL: &str = "https://api.mojang.com/users/profiles/minecraft";
const SESSION_URL: &str = "https://sessionserver.mojang.com/session/minecraft/profile";
/// Текстуры качаем только отсюда: адреса приходят из ответов API
const TEXTURES_HOST: &str = "textures.minecraft.net";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SkinModel {
    /// Steve: руки шириной 4 пикселя
    Classic,
    /// Alex: руки шириной 3 пикселя
    Slim,
}

impl SkinModel {
    fn variant(self) -> &'static str {
        match self {
            SkinModel::Classic => "classic",
            SkinModel::Slim => "slim",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Skin {
    pub id: String,
    pub name: String,
    pub model: SkinModel,
    pub sha1: String,
    pub added_at: String,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct Library {
    skins: Vec<Skin>,
}

fn dir(paths: &Paths) -> std::path::PathBuf {
    paths.root.join("skins")
}

fn library_file(paths: &Paths) -> std::path::PathBuf {
    paths.root.join("skins.json")
}

fn load(paths: &Paths) -> Result<Library> {
    match std::fs::read(library_file(paths)) {
        Ok(bytes) => Ok(serde_json::from_slice(&bytes).unwrap_or_default()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Library::default()),
        Err(e) => Err(e.into()),
    }
}

fn save(paths: &Paths, library: &Library) -> Result<()> {
    std::fs::create_dir_all(&paths.root)?;
    std::fs::write(library_file(paths), serde_json::to_vec_pretty(library)?)?;
    Ok(())
}

/// id скина из фронтенда: только uuid, без путей.
fn skin_path(paths: &Paths, id: &str) -> Result<std::path::PathBuf> {
    if id.is_empty() || !id.chars().all(|c| c.is_ascii_hexdigit() || c == '-') {
        return Err(Error::msg(format!("invalid skin id: {id}")));
    }
    Ok(dir(paths).join(format!("{id}.png")))
}

/// Проверяет, что это PNG скина: 64×64 или старый формат 64×32.
fn check_png(bytes: &[u8]) -> Result<()> {
    const SIGNATURE: &[u8] = b"\x89PNG\r\n\x1a\n";
    if bytes.len() < 24 || &bytes[..8] != SIGNATURE || &bytes[12..16] != b"IHDR" {
        return Err(Error::msg("skin_not_png"));
    }
    let width = u32::from_be_bytes(bytes[16..20].try_into().expect("4 bytes"));
    let height = u32::from_be_bytes(bytes[20..24].try_into().expect("4 bytes"));
    if width != 64 || (height != 64 && height != 32) {
        return Err(Error::msg(format!("skin_bad_size:{width}x{height}")));
    }
    Ok(())
}

/// Добавляет PNG в библиотеку. Такой же файл, добавленный раньше, не дублируется.
fn add(paths: &Paths, bytes: &[u8], name: &str, model: SkinModel) -> Result<Skin> {
    check_png(bytes)?;
    let sha1 = hex::encode(Sha1::digest(bytes));
    let mut library = load(paths)?;
    if let Some(existing) = library.skins.iter().find(|s| s.sha1 == sha1) {
        return Ok(existing.clone());
    }
    let name = name.trim();
    let skin = Skin {
        id: uuid::Uuid::new_v4().to_string(),
        name: if name.is_empty() { "Skin".into() } else { name.chars().take(64).collect() },
        model,
        sha1,
        added_at: chrono::Utc::now().to_rfc3339(),
    };
    std::fs::create_dir_all(dir(paths))?;
    std::fs::write(skin_path(paths, &skin.id)?, bytes)?;
    library.skins.insert(0, skin.clone());
    save(paths, &library)?;
    Ok(skin)
}

fn data_url(bytes: &[u8]) -> String {
    format!("data:image/png;base64,{}", base64::engine::general_purpose::STANDARD.encode(bytes))
}

async fn fetch_texture(http: &reqwest::Client, url: &str) -> Result<Vec<u8>> {
    let parsed = reqwest::Url::parse(url).map_err(|_| Error::msg("bad texture url"))?;
    if parsed.host_str() != Some(TEXTURES_HOST) {
        return Err(Error::msg("bad texture url"));
    }
    // У старых ответов API ссылки на текстуры бывают по http
    let mut parsed = parsed;
    let _ = parsed.set_scheme("https");
    Ok(http.get(parsed).send().await?.error_for_status()?.bytes().await?.to_vec())
}

// ---------- профиль Microsoft-аккаунта ----------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileSkin {
    pub id: String,
    pub state: String,
    pub url: String,
    #[serde(default)]
    pub variant: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileCape {
    pub id: String,
    pub state: String,
    pub url: String,
    #[serde(default)]
    pub alias: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Profile {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub skins: Vec<ProfileSkin>,
    #[serde(default)]
    pub capes: Vec<ProfileCape>,
}

/// Токен Minecraft для аккаунта из списка; оффлайн-аккаунту скин не надеть.
async fn token_for(state: &AppState, account_id: &str) -> Result<String> {
    let list = accounts::load(&state.paths)?;
    let account = list.accounts.iter().find(|a| a.id == account_id).ok_or_else(|| Error::msg("account not found"))?;
    if account.kind != AccountKind::Microsoft {
        return Err(Error::msg("skin_needs_microsoft"));
    }
    Ok(crate::auth::minecraft_token(state, &account.uuid).await?.0)
}

async fn profile_response(response: reqwest::Response) -> Result<Profile> {
    let status = response.status();
    if status == reqwest::StatusCode::UNAUTHORIZED {
        return Err(Error::msg("auth:signed_out"));
    }
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        // Mojang отвечает {"errorMessage": "..."} — показываем его, если он есть
        let message = serde_json::from_str::<serde_json::Value>(&body)
            .ok()
            .and_then(|v| v["errorMessage"].as_str().map(str::to_owned))
            .unwrap_or_else(|| status.to_string());
        return Err(Error::msg(format!("skin_api_failed: {message}")));
    }
    Ok(response.json().await?)
}

// ---------- команды ----------

#[tauri::command]
pub fn skins_list(state: State<'_, AppState>) -> Result<Vec<Skin>> {
    Ok(load(&state.paths)?.skins)
}

/// PNG скина в виде data URL для превью.
#[tauri::command]
pub fn skin_data(state: State<'_, AppState>, id: String) -> Result<String> {
    Ok(data_url(&std::fs::read(skin_path(&state.paths, &id)?)?))
}

#[tauri::command]
pub fn skin_add_file(state: State<'_, AppState>, path: String, name: String, model: SkinModel) -> Result<Skin> {
    add(&state.paths, &std::fs::read(path)?, &name, model)
}

/// Скин уже добавлен на фронтенде (например, перетащили картинку): байты приходят как base64.
#[tauri::command]
pub fn skin_add_data(state: State<'_, AppState>, data: String, name: String, model: SkinModel) -> Result<Skin> {
    let raw = data.strip_prefix("data:image/png;base64,").unwrap_or(&data);
    let bytes = base64::engine::general_purpose::STANDARD.decode(raw).map_err(|_| Error::msg("skin_not_png"))?;
    add(&state.paths, &bytes, &name, model)
}

/// Скин любого игрока по нику — работает без входа в аккаунт.
#[tauri::command]
pub async fn skin_add_player(state: State<'_, AppState>, name: String) -> Result<Skin> {
    add_player(&state, &name).await
}

async fn add_player(state: &AppState, name: &str) -> Result<Skin> {
    let name = name.trim();
    if name.is_empty() || name.len() > 16 || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return Err(Error::msg("skin_player_not_found"));
    }
    let response = state.http.get(format!("{MOJANG_PROFILE_URL}/{name}")).send().await?;
    if matches!(response.status(), reqwest::StatusCode::NOT_FOUND | reqwest::StatusCode::NO_CONTENT) {
        return Err(Error::msg("skin_player_not_found"));
    }
    #[derive(Deserialize)]
    struct Found {
        id: String,
        name: String,
    }
    let found: Found = response.error_for_status()?.json().await?;

    #[derive(Deserialize)]
    struct Property {
        name: String,
        value: String,
    }
    #[derive(Deserialize)]
    struct Session {
        properties: Vec<Property>,
    }
    let session: Session =
        state.http.get(format!("{SESSION_URL}/{}", found.id)).send().await?.error_for_status()?.json().await?;
    let textures = session
        .properties
        .iter()
        .find(|p| p.name == "textures")
        .ok_or_else(|| Error::msg("skin_player_not_found"))?;
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(&textures.value)
        .map_err(|_| Error::msg("skin_player_not_found"))?;
    let value: serde_json::Value = serde_json::from_slice(&decoded)?;
    let skin = &value["textures"]["SKIN"];
    // У игроков со скином по умолчанию текстуры в профиле нет
    let url = skin["url"].as_str().ok_or_else(|| Error::msg("skin_player_default"))?;
    let model = if skin["metadata"]["model"].as_str() == Some("slim") { SkinModel::Slim } else { SkinModel::Classic };
    let bytes = fetch_texture(&state.http, url).await?;
    add(&state.paths, &bytes, &found.name, model)
}

#[tauri::command]
pub fn skin_update(state: State<'_, AppState>, id: String, name: String, model: SkinModel) -> Result<Skin> {
    let mut library = load(&state.paths)?;
    let skin = library.skins.iter_mut().find(|s| s.id == id).ok_or_else(|| Error::msg("skin not found"))?;
    let name = name.trim();
    if !name.is_empty() {
        skin.name = name.chars().take(64).collect();
    }
    skin.model = model;
    let skin = skin.clone();
    save(&state.paths, &library)?;
    Ok(skin)
}

#[tauri::command]
pub fn skin_remove(state: State<'_, AppState>, id: String) -> Result<()> {
    let path = skin_path(&state.paths, &id)?;
    let mut library = load(&state.paths)?;
    library.skins.retain(|s| s.id != id);
    save(&state.paths, &library)?;
    if path.exists() {
        std::fs::remove_file(path)?;
    }
    Ok(())
}

/// Скин офлайн-аккаунта в игре: ставит в сборку CustomSkinLoader и кладёт скин туда,
/// откуда мод берёт локальные скины (`CustomSkinLoader/LocalSkin/skins/<ник>.png`).
/// Сборке без загрузчика модов поставить мод нельзя — тогда ничего не делаем.
pub async fn prepare_offline(state: &AppState, instance_id: &str, nick: &str, skin_id: &str) -> Result<bool> {
    let source = skin_path(&state.paths, skin_id)?;
    if !source.is_file() {
        return Ok(false);
    }
    if !crate::content::ensure_mod(state, instance_id, "customskinloader", "customskinloader").await? {
        return Ok(false);
    }
    let dir = state.paths.game_dir(instance_id)?.join("CustomSkinLoader").join("LocalSkin").join("skins");
    std::fs::create_dir_all(&dir)?;
    std::fs::copy(source, dir.join(format!("{nick}.png")))?;
    Ok(true)
}

/// Сохраняет PNG скина из библиотеки в выбранный файл.
#[tauri::command]
pub fn skin_export(state: State<'_, AppState>, id: String, path: String) -> Result<()> {
    std::fs::copy(skin_path(&state.paths, &id)?, path)?;
    Ok(())
}

/// Текущий профиль Microsoft-аккаунта: надетый скин и доступные плащи.
#[tauri::command]
pub async fn skin_profile(state: State<'_, AppState>, account_id: String) -> Result<Profile> {
    let token = token_for(&state, &account_id).await?;
    profile_response(state.http.get(PROFILE_URL).bearer_auth(&token).send().await?).await
}

/// Картинка скина или плаща с textures.minecraft.net в виде data URL.
#[tauri::command]
pub async fn skin_texture(state: State<'_, AppState>, url: String) -> Result<String> {
    Ok(data_url(&fetch_texture(&state.http, &url).await?))
}

/// Надевает скин из библиотеки на Microsoft-аккаунт.
#[tauri::command]
pub async fn skin_apply(state: State<'_, AppState>, account_id: String, id: String) -> Result<Profile> {
    let library = load(&state.paths)?;
    let skin = library.skins.iter().find(|s| s.id == id).ok_or_else(|| Error::msg("skin not found"))?;
    let bytes = std::fs::read(skin_path(&state.paths, &id)?)?;
    let token = token_for(&state, &account_id).await?;
    let part = reqwest::multipart::Part::bytes(bytes).file_name("skin.png").mime_str("image/png")?;
    let form = reqwest::multipart::Form::new().text("variant", skin.model.variant()).part("file", part);
    let url = format!("{PROFILE_URL}/skins");
    profile_response(state.http.post(url).bearer_auth(&token).multipart(form).send().await?).await
}

/// Включает плащ по id или снимает его (`None`).
#[tauri::command]
pub async fn skin_set_cape(state: State<'_, AppState>, account_id: String, cape_id: Option<String>) -> Result<Profile> {
    let token = token_for(&state, &account_id).await?;
    let url = format!("{PROFILE_URL}/capes/active");
    let request = match cape_id {
        Some(id) => state.http.put(url).json(&serde_json::json!({ "capeId": id })),
        None => state.http.delete(url),
    };
    profile_response(request.bearer_auth(&token).send().await?).await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png(width: u32, height: u32) -> Vec<u8> {
        let mut bytes = b"\x89PNG\r\n\x1a\n\0\0\0\x0dIHDR".to_vec();
        bytes.extend(width.to_be_bytes());
        bytes.extend(height.to_be_bytes());
        bytes.extend([8, 6, 0, 0, 0]);
        bytes
    }

    #[test]
    fn accepts_only_skin_sizes() {
        assert!(check_png(&png(64, 64)).is_ok());
        assert!(check_png(&png(64, 32)).is_ok());
        assert!(check_png(&png(128, 128)).is_err());
        assert!(check_png(b"GIF89a....................").is_err());
    }

    #[test]
    fn library_dedupes_and_survives_removal() {
        let root = std::env::temp_dir().join("shard-skins-test");
        let _ = std::fs::remove_dir_all(&root);
        let paths = Paths::new(root.clone());
        let a = add(&paths, &png(64, 64), "Steve", SkinModel::Classic).unwrap();
        let again = add(&paths, &png(64, 64), "Other name", SkinModel::Slim).unwrap();
        assert_eq!(a.id, again.id);
        let b = add(&paths, &png(64, 32), "Old", SkinModel::Classic).unwrap();
        assert_eq!(load(&paths).unwrap().skins.len(), 2);
        assert!(skin_path(&paths, "../accounts").is_err());
        std::fs::remove_file(skin_path(&paths, &b.id).unwrap()).unwrap();
        let _ = std::fs::remove_dir_all(&root);
    }

    /// Ходит в Mojang: cargo test skin_from_player -- --ignored --nocapture
    #[tokio::test]
    #[ignore]
    async fn skin_from_player() {
        let root = std::env::temp_dir().join("shard-skins-player-test");
        let _ = std::fs::remove_dir_all(&root);
        let state = AppState::new(root.clone());
        let skin = add_player(&state, "jeb_").await.unwrap();
        println!("added {} {:?}", skin.name, skin.model);
        assert!(skin_path(&state.paths, &skin.id).unwrap().exists());
        assert!(matches!(add_player(&state, "no such player here").await, Err(_)));
        let _ = std::fs::remove_dir_all(&root);
    }
}
