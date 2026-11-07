//! Вход через Microsoft: Device Code Flow → Xbox Live → XSTS → Minecraft.
//!
//! Пользователь видит короткий код, открывает microsoft.com/link, входит и вводит код.
//! Refresh-токен Microsoft и токен Minecraft хранятся в системном хранилище
//! (Диспетчер учётных данных Windows), а не в файлах лаунчера.

use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::json;
use tauri::State;
use tokio::sync::oneshot;

use crate::error::{Error, Result};
use crate::state::AppState;

const DEVICE_CODE_URL: &str = "https://login.microsoftonline.com/consumers/oauth2/v2.0/devicecode";
const TOKEN_URL: &str = "https://login.microsoftonline.com/consumers/oauth2/v2.0/token";
const XBL_URL: &str = "https://user.auth.xboxlive.com/user/authenticate";
const XSTS_URL: &str = "https://xsts.auth.xboxlive.com/xsts/authorize";
const MC_LOGIN_URL: &str = "https://api.minecraftservices.com/authentication/login_with_xbox";
const MC_PROFILE_URL: &str = "https://api.minecraftservices.com/minecraft/profile";
const SCOPE: &str = "XboxLive.signin offline_access";
pub(crate) const KEYRING_SERVICE: &str = "app.shard.launcher";

/// Ошибки входа отдаются фронтенду кодом `auth:<код>`, текст подбирается там по языку интерфейса.
fn auth_error(code: &str) -> Error {
    Error::msg(format!("auth:{code}"))
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceCode {
    pub user_code: String,
    pub verification_uri: String,
    pub expires_in: u64,
}

#[derive(Debug, Deserialize)]
struct DeviceCodeResponse {
    device_code: String,
    user_code: String,
    verification_uri: String,
    expires_in: u64,
    interval: u64,
}

#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: String,
    refresh_token: String,
}

#[derive(Debug, Deserialize)]
struct TokenError {
    error: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct XboxResponse {
    token: String,
    display_claims: DisplayClaims,
}

#[derive(Debug, Deserialize)]
struct DisplayClaims {
    xui: Vec<Xui>,
}

#[derive(Debug, Deserialize)]
struct Xui {
    uhs: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct XstsError {
    x_err: u64,
}

#[derive(Debug, Deserialize)]
struct McLogin {
    access_token: String,
    expires_in: i64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct McProfile {
    pub id: String,
    pub name: String,
}

/// Сохранённые секреты одного Microsoft-аккаунта.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Secrets {
    refresh_token: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CachedMcToken {
    token: String,
    /// Unix-время, после которого токен уже не годится
    expires_at: i64,
}

/// Ожидающий подтверждения вход: код устройства и канал для отмены.
pub struct PendingLogin {
    device_code: String,
    interval: u64,
    expires_at: i64,
    cancel: Option<oneshot::Receiver<()>>,
}

/// Client ID приложения Azure вшивается при сборке, как у Prism Launcher: его берут из
/// `src-tauri/msa-client-id.txt` или переменной SHARD_MSA_CLIENT_ID (см. build.rs).
const CLIENT_ID: Option<&str> = option_env!("SHARD_MSA_CLIENT_ID");

fn client_id() -> Result<String> {
    CLIENT_ID
        .map(str::trim)
        .filter(|id| !id.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| auth_error("no_client_id"))
}

// Windows хранит в одной записи не больше 2560 байт, а токены Microsoft бывают длиннее,
// поэтому секрет режется на части: `kind:uuid` хранит число частей, `kind:uuid#N` — сами части.
const CHUNK: usize = 2000;
const MAX_CHUNKS: usize = 16;

fn keyring_entry(name: &str) -> Result<keyring::Entry> {
    keyring::Entry::new(KEYRING_SERVICE, name).map_err(|e| Error::msg(format!("credential store: {e}")))
}

fn keyring_err(e: keyring::Error) -> Error {
    Error::msg(format!("credential store: {e}"))
}

fn store_secret<T: Serialize>(kind: &str, uuid: &str, value: &T) -> Result<()> {
    forget_kind(kind, uuid);
    let bytes = serde_json::to_vec(value)?;
    let chunks: Vec<&[u8]> = bytes.chunks(CHUNK).collect();
    if chunks.len() > MAX_CHUNKS {
        return Err(Error::msg("credential store: secret too large"));
    }
    for (i, chunk) in chunks.iter().enumerate() {
        keyring_entry(&format!("{kind}:{uuid}#{i}"))?.set_secret(chunk).map_err(keyring_err)?;
    }
    keyring_entry(&format!("{kind}:{uuid}"))?.set_password(&chunks.len().to_string()).map_err(keyring_err)
}

fn read_secret<T: for<'de> Deserialize<'de>>(kind: &str, uuid: &str) -> Result<Option<T>> {
    let count = match keyring_entry(&format!("{kind}:{uuid}"))?.get_password() {
        Ok(text) => text.parse::<usize>().unwrap_or(0).min(MAX_CHUNKS),
        Err(keyring::Error::NoEntry) => return Ok(None),
        Err(e) => return Err(keyring_err(e)),
    };
    let mut bytes = vec![];
    for i in 0..count {
        match keyring_entry(&format!("{kind}:{uuid}#{i}"))?.get_secret() {
            Ok(part) => bytes.extend(part),
            Err(keyring::Error::NoEntry) => return Ok(None),
            Err(e) => return Err(keyring_err(e)),
        }
    }
    // Повреждённый секрет считаем отсутствующим: пользователь просто войдёт заново
    Ok(serde_json::from_slice(&bytes).ok())
}

fn forget_kind(kind: &str, uuid: &str) {
    if let Ok(entry) = keyring_entry(&format!("{kind}:{uuid}")) {
        let _ = entry.delete_credential();
    }
    for i in 0..MAX_CHUNKS {
        if let Ok(entry) = keyring_entry(&format!("{kind}:{uuid}#{i}")) {
            if entry.delete_credential().is_err() {
                break;
            }
        }
    }
}

pub fn forget(uuid: &str) {
    forget_kind("msa", uuid);
    forget_kind("mc", uuid);
}

/// Шаг 1: получить код, который пользователь введёт на сайте Microsoft.
pub async fn start(state: &AppState) -> Result<(DeviceCode, PendingLogin, oneshot::Sender<()>)> {
    let client_id = client_id()?;
    let res = state
        .http
        .post(DEVICE_CODE_URL)
        .form(&[("client_id", client_id.as_str()), ("scope", SCOPE)])
        .send()
        .await?;
    if !res.status().is_success() {
        // Чаще всего — неверный Client ID или приложение не разрешает личные аккаунты Microsoft
        return Err(auth_error("bad_client_id"));
    }
    let code: DeviceCodeResponse = res.json().await?;
    let (cancel_tx, cancel_rx) = oneshot::channel();
    let pending = PendingLogin {
        device_code: code.device_code,
        interval: code.interval.max(1),
        expires_at: chrono::Utc::now().timestamp() + code.expires_in as i64,
        cancel: Some(cancel_rx),
    };
    Ok((
        DeviceCode { user_code: code.user_code, verification_uri: code.verification_uri, expires_in: code.expires_in },
        pending,
        cancel_tx,
    ))
}

/// Шаг 2: ждать, пока пользователь введёт код, и пройти всю цепочку до профиля Minecraft.
pub async fn finish(state: &AppState, mut pending: PendingLogin) -> Result<McProfile> {
    let client_id = client_id()?;
    let mut cancel = pending.cancel.take().expect("cancel receiver");
    let mut interval = pending.interval;

    let tokens = loop {
        tokio::select! {
            _ = tokio::time::sleep(Duration::from_secs(interval)) => {}
            _ = &mut cancel => return Err(auth_error("cancelled")),
        }
        if chrono::Utc::now().timestamp() > pending.expires_at {
            return Err(auth_error("expired"));
        }
        let res = state
            .http
            .post(TOKEN_URL)
            .form(&[
                ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
                ("client_id", client_id.as_str()),
                ("device_code", pending.device_code.as_str()),
            ])
            .send()
            .await?;
        if res.status().is_success() {
            break res.json::<TokenResponse>().await?;
        }
        let err: TokenError = res.json().await.unwrap_or(TokenError { error: "unknown".into() });
        match err.error.as_str() {
            "authorization_pending" => continue,
            "slow_down" => interval += 5,
            "authorization_declined" => return Err(auth_error("declined")),
            "expired_token" => return Err(auth_error("expired")),
            _ => return Err(auth_error("microsoft_failed")),
        }
    };

    let (mc, profile) = minecraft_from_ms(state, &tokens.access_token).await?;
    store_secret("msa", &profile.id, &Secrets { refresh_token: tokens.refresh_token })?;
    store_secret("mc", &profile.id, &mc)?;
    Ok(profile)
}

/// Microsoft access token → Xbox Live → XSTS → токен Minecraft и профиль.
async fn minecraft_from_ms(state: &AppState, ms_token: &str) -> Result<(CachedMcToken, McProfile)> {
    let http = &state.http;
    let xbl: XboxResponse = http
        .post(XBL_URL)
        .json(&json!({
            "Properties": {
                "AuthMethod": "RPS",
                "SiteName": "user.auth.xboxlive.com",
                "RpsTicket": format!("d={ms_token}"),
            },
            "RelyingParty": "http://auth.xboxlive.com",
            "TokenType": "JWT",
        }))
        .send()
        .await?
        .error_for_status()
        .map_err(|_| auth_error("xbox_failed"))?
        .json()
        .await?;

    let res = http
        .post(XSTS_URL)
        .json(&json!({
            "Properties": { "SandboxId": "RETAIL", "UserTokens": [xbl.token] },
            "RelyingParty": "rp://api.minecraftservices.com/",
            "TokenType": "JWT",
        }))
        .send()
        .await?;
    if res.status() == reqwest::StatusCode::UNAUTHORIZED {
        let code = res.json::<XstsError>().await.map(|e| e.x_err).unwrap_or(0);
        return Err(auth_error(match code {
            2148916233 => "no_xbox_profile",
            2148916235 => "xbox_region",
            2148916236 | 2148916237 => "xbox_adult_verification",
            2148916238 => "xbox_child",
            _ => "xbox_failed",
        }));
    }
    let xsts: XboxResponse = res.error_for_status().map_err(|_| auth_error("xbox_failed"))?.json().await?;
    let uhs = xsts.display_claims.xui.first().map(|x| x.uhs.clone()).ok_or_else(|| auth_error("xbox_failed"))?;

    let res = http
        .post(MC_LOGIN_URL)
        .json(&json!({ "identityToken": format!("XBL3.0 x={uhs};{}", xsts.token) }))
        .send()
        .await?;
    if res.status() == reqwest::StatusCode::FORBIDDEN {
        // Новые приложения Azure должны быть одобрены Mojang для доступа к Minecraft API
        return Err(auth_error("app_not_approved"));
    }
    let login: McLogin = res.error_for_status().map_err(|_| auth_error("minecraft_failed"))?.json().await?;

    let res = http.get(MC_PROFILE_URL).bearer_auth(&login.access_token).send().await?;
    if res.status() == reqwest::StatusCode::NOT_FOUND {
        return Err(auth_error("no_minecraft"));
    }
    let profile: McProfile = res.error_for_status().map_err(|_| auth_error("minecraft_failed"))?.json().await?;

    let cached = CachedMcToken {
        token: login.access_token,
        // Запас 5 минут, чтобы токен не истёк прямо во время запуска
        expires_at: chrono::Utc::now().timestamp() + login.expires_in - 300,
    };
    Ok((cached, profile))
}

/// Токен из кеша без обращения к сети, если он действует ещё хотя бы 10 минут. Для быстрого запуска.
pub fn cached_minecraft_token(uuid: &str) -> Option<String> {
    let cached = read_secret::<CachedMcToken>("mc", uuid).ok()??;
    (cached.expires_at > chrono::Utc::now().timestamp() + 600).then_some(cached.token)
}

/// Действующий токен Minecraft для запуска: из кеша или через refresh-токен.
pub async fn minecraft_token(state: &AppState, uuid: &str) -> Result<(String, McProfile)> {
    if let Some(cached) = read_secret::<CachedMcToken>("mc", uuid)? {
        if cached.expires_at > chrono::Utc::now().timestamp() {
            let res = state.http.get(MC_PROFILE_URL).bearer_auth(&cached.token).send().await?;
            if res.status().is_success() {
                return Ok((cached.token, res.json().await?));
            }
        }
    }

    let secrets: Secrets = read_secret("msa", uuid)?.ok_or_else(|| auth_error("signed_out"))?;
    let client_id = client_id()?;
    let res = state
        .http
        .post(TOKEN_URL)
        .form(&[
            ("grant_type", "refresh_token"),
            ("client_id", client_id.as_str()),
            ("refresh_token", secrets.refresh_token.as_str()),
            ("scope", SCOPE),
        ])
        .send()
        .await?;
    if !res.status().is_success() {
        return Err(auth_error("signed_out"));
    }
    let tokens: TokenResponse = res.json().await?;
    let (mc, profile) = minecraft_from_ms(state, &tokens.access_token).await?;
    store_secret("msa", uuid, &Secrets { refresh_token: tokens.refresh_token })?;
    store_secret("mc", uuid, &mc)?;
    Ok((mc.token, profile))
}

#[tauri::command]
pub async fn msa_start(state: State<'_, AppState>) -> Result<DeviceCode> {
    let (code, pending, cancel) = start(&state).await?;
    // Новый вход заменяет незавершённый предыдущий: старый канал отмены выбрасывается, и его ожидание прекращается
    *state.login_cancel.lock().await = Some(cancel);
    *state.login.lock().await = Some(pending);
    Ok(code)
}

#[tauri::command]
pub async fn msa_finish(state: State<'_, AppState>) -> Result<crate::accounts::Accounts> {
    let pending = state.login.lock().await.take().ok_or_else(|| auth_error("cancelled"))?;
    let profile = finish(&state, pending).await?;
    state.login_cancel.lock().await.take();
    crate::accounts::upsert_microsoft(&state.paths, &profile)
}

#[tauri::command]
pub async fn msa_cancel(state: State<'_, AppState>) -> Result<()> {
    state.login.lock().await.take();
    if let Some(cancel) = state.login_cancel.lock().await.take() {
        let _ = cancel.send(());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn credential_store_roundtrip() {
        let uuid = "selftest-00000000";
        store_secret("mc", uuid, &CachedMcToken { token: "x".repeat(5000), expires_at: 1 }).unwrap();
        let back: CachedMcToken = read_secret("mc", uuid).unwrap().unwrap();
        assert_eq!(back.token.len(), 5000);
        forget(uuid);
        assert!(read_secret::<CachedMcToken>("mc", uuid).unwrap().is_none());
    }
}
