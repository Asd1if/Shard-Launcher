//! Туннель playit.gg: постоянный адрес сервера без белого IP и настройки роутера.
//!
//! Агент playit (`playitd`, BSD-2) скачивается с GitHub при первом включении и работает
//! фоновым процессом, пока открыт туннель. Один раз агент привязывают к аккаунту playit.gg:
//! лаунчер открывает ссылку, пользователь подтверждает её в браузере, ключ агента сохраняется
//! в `<root>/playit/secret.txt`. Туннель создаётся через API playit на порт сервера.

use std::path::PathBuf;
use std::sync::LazyLock;
use std::time::Duration;

use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::{json, Value};
use tauri::State;
use tokio::process::Child;
use tokio::sync::Mutex;

use crate::error::{Error, Result};
use crate::paths::Paths;
use crate::state::AppState;

const API: &str = "https://api.playit.gg";
const RELEASES: &str = "https://api.github.com/repos/playit-cloud/playit-agent/releases/latest";
const AGENT_ASSET: &str = "playitd-windows-x86_64-signed.exe";

/// Запущенный агент. Один на все туннели: playit сам раздаёт их по портам.
static AGENT: LazyLock<Mutex<Option<Child>>> = LazyLock::new(Default::default);

fn dir(paths: &Paths) -> PathBuf {
    paths.root.join("playit")
}

fn secret_file(paths: &Paths) -> PathBuf {
    dir(paths).join("secret.txt")
}

fn agent_exe(paths: &Paths) -> PathBuf {
    dir(paths).join("playitd.exe")
}

fn secret(paths: &Paths) -> Option<String> {
    let s = std::fs::read_to_string(secret_file(paths)).ok()?;
    let s = s.trim();
    (!s.is_empty() && hex::decode(s).is_ok()).then(|| s.to_owned())
}

/// Ответ API playit: `{"status": "success" | "fail" | "error", "data": …}`.
#[derive(Deserialize)]
#[serde(tag = "status", content = "data", rename_all = "lowercase")]
enum Reply<T> {
    Success(T),
    Fail(Value),
    Error(Value),
}

async fn call<T: DeserializeOwned>(state: &AppState, path: &str, key: Option<&str>, body: Value) -> Result<T> {
    let mut request = state.http.post(format!("{API}{path}")).json(&body);
    if let Some(key) = key {
        request = request.header(reqwest::header::AUTHORIZATION, format!("Agent-Key {key}"));
    }
    match request.send().await?.json::<Reply<T>>().await? {
        Reply::Success(v) => Ok(v),
        Reply::Fail(v) | Reply::Error(v) => Err(Error::msg(format!("tunnel_api:{}", api_reason(&v)))),
    }
}

/// Короткая причина отказа API: `"RequiresVerifiedAccount"` или `{"type": …, "message": …}`.
fn api_reason(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Object(o) => o.get("message").or_else(|| o.get("type")).map_or_else(|| v.to_string(), |m| m.as_str().unwrap_or_default().to_owned()),
        _ => v.to_string(),
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TunnelStatus {
    /// Агент привязан к аккаунту playit.gg
    pub linked: bool,
    pub running: bool,
}

#[tauri::command]
pub async fn tunnel_status(state: State<'_, AppState>) -> Result<TunnelStatus> {
    Ok(TunnelStatus { linked: secret(&state.paths).is_some(), running: agent_running().await })
}

async fn agent_running() -> bool {
    let mut agent = AGENT.lock().await;
    match agent.as_mut().map(|c| c.try_wait()) {
        Some(Ok(None)) => true,
        Some(_) => {
            *agent = None;
            false
        }
        None => false,
    }
}

#[derive(Serialize)]
pub struct LinkStart {
    pub code: String,
    pub url: String,
}

/// Первый шаг привязки: код и ссылка, которую пользователь открывает в браузере.
#[tauri::command]
pub async fn tunnel_link_start(state: State<'_, AppState>) -> Result<LinkStart> {
    let code = hex::encode(&uuid::Uuid::new_v4().as_bytes()[..5]);
    let _: Value = call(&state, "/claim/setup", None, setup_body(&code)).await?;
    Ok(LinkStart { url: format!("https://playit.gg/claim/{code}"), code })
}

fn setup_body(code: &str) -> Value {
    json!({ "code": code, "agent_type": "self-managed", "version": concat!("Shard ", env!("CARGO_PKG_VERSION")) })
}

/// Ждёт подтверждения в браузере (до 10 минут) и сохраняет ключ агента.
#[tauri::command]
pub async fn tunnel_link_finish(state: State<'_, AppState>, code: String) -> Result<()> {
    if hex::decode(&code).is_err() {
        return Err(Error::msg("tunnel_link_expired"));
    }
    // Страница playit считает агент «offline», если он не отмечается постоянно: как и
    // официальный агент, спрашиваем часто и не бросаем ожидание из-за сбоя сети
    let deadline = tokio::time::Instant::now() + Duration::from_secs(600);
    loop {
        if tokio::time::Instant::now() > deadline {
            return Err(Error::msg("tunnel_link_expired"));
        }
        match call::<String>(&state, "/claim/setup", None, setup_body(&code)).await.as_deref() {
            Ok("UserAccepted") => break,
            Ok("UserRejected") => return Err(Error::msg("tunnel_link_rejected")),
            Ok(_) => tokio::time::sleep(Duration::from_millis(250)).await,
            Err(_) => tokio::time::sleep(Duration::from_secs(1)).await,
        }
    }
    #[derive(Deserialize)]
    struct Key {
        secret_key: String,
    }
    // Ключ выдают не сразу после подтверждения
    loop {
        match call::<Key>(&state, "/claim/exchange", None, json!({ "code": code })).await {
            Ok(key) => {
                std::fs::create_dir_all(dir(&state.paths))?;
                std::fs::write(secret_file(&state.paths), key.secret_key.trim())?;
                return Ok(());
            }
            Err(e) if tokio::time::Instant::now() > deadline => return Err(e),
            Err(_) => tokio::time::sleep(Duration::from_secs(1)).await,
        }
    }
}

/// Отвязать агент: остановить туннель и забыть ключ.
#[tauri::command]
pub async fn tunnel_unlink(state: State<'_, AppState>) -> Result<()> {
    close().await;
    let _ = std::fs::remove_file(secret_file(&state.paths));
    Ok(())
}

#[derive(Deserialize)]
struct RunData {
    agent_id: String,
    #[serde(default)]
    tunnels: Vec<AgentTunnel>,
}

#[derive(Deserialize)]
struct AgentTunnel {
    id: String,
    local_port: u16,
    #[serde(default)]
    tunnel_type: Option<String>,
    #[serde(default)]
    assigned_domain: String,
    #[serde(default)]
    custom_domain: Option<String>,
    #[serde(default)]
    disabled: Option<Value>,
}

impl AgentTunnel {
    fn address(&self) -> Option<String> {
        let domain = self.custom_domain.clone().filter(|d| !d.is_empty()).unwrap_or_else(|| self.assigned_domain.clone());
        (!domain.is_empty() && self.disabled.is_none()).then_some(domain)
    }
}

async fn rundata(state: &AppState, key: &str) -> Result<RunData> {
    call(state, "/agents/rundata", Some(key), json!({})).await
}

/// Скачивает агент playit, если его ещё нет.
async fn ensure_agent(state: &AppState) -> Result<PathBuf> {
    let exe = agent_exe(&state.paths);
    if exe.is_file() {
        return Ok(exe);
    }
    #[derive(Deserialize)]
    struct Release {
        assets: Vec<Asset>,
    }
    #[derive(Deserialize)]
    struct Asset {
        name: String,
        browser_download_url: String,
    }
    let release: Release = state.http.get(RELEASES).send().await?.error_for_status()?.json().await?;
    let url = release
        .assets
        .into_iter()
        .find(|a| a.name == AGENT_ASSET)
        .map(|a| a.browser_download_url)
        .ok_or_else(|| Error::msg("tunnel_agent_missing"))?;
    let bytes = state.http.get(url).send().await?.error_for_status()?.bytes().await?;
    std::fs::create_dir_all(dir(&state.paths))?;
    crate::download::write_atomic(&exe, &bytes).await?;
    Ok(exe)
}

async fn spawn_agent(state: &AppState) -> Result<()> {
    if agent_running().await {
        return Ok(());
    }
    let exe = ensure_agent(state).await?;
    let mut command = tokio::process::Command::new(exe);
    command
        .arg("--secret-path")
        .arg(secret_file(&state.paths))
        .arg("--socket-path")
        .arg(format!(r"\\.\pipe\shard-playit-{}", std::process::id()))
        .arg("--log-path")
        .arg(dir(&state.paths).join("playitd.log"))
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true);
    #[cfg(windows)]
    command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    *AGENT.lock().await = Some(command.spawn()?);
    Ok(())
}

/// Открывает туннель на порт и возвращает адрес для друзей (вида `name.joinmc.link`).
pub async fn open(state: &AppState, port: u16) -> Result<String> {
    let key = secret(&state.paths).ok_or_else(|| Error::msg("tunnel_not_linked"))?;
    let data = rundata(state, &key).await?;
    let ours = |t: &AgentTunnel| t.local_port == port && t.tunnel_type.as_deref() == Some("minecraft-java");
    let tunnel_id = match data.tunnels.iter().find(|t| ours(t)) {
        Some(t) => t.id.clone(),
        None => {
            #[derive(Deserialize)]
            struct Created {
                id: String,
            }
            let body = json!({
                "name": format!("Shard {port}"),
                "tunnel_type": "minecraft-java",
                "port_type": "tcp",
                "port_count": 1,
                "origin": { "type": "agent", "data": { "agent_id": data.agent_id, "local_ip": "127.0.0.1", "local_port": port } },
                "enabled": true,
                "alloc": null,
                "firewall_id": null,
                "proxy_protocol": null,
            });
            call::<Created>(state, "/tunnels/create", Some(&key), body).await?.id
        }
    };
    spawn_agent(state).await?;
    // Новому туннелю playit выдаёт адрес не сразу
    for _ in 0..30 {
        if let Some(address) = rundata(state, &key).await?.tunnels.iter().find(|t| t.id == tunnel_id).and_then(AgentTunnel::address) {
            return Ok(address);
        }
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
    Err(Error::msg("tunnel_pending"))
}

/// Останавливает агент — все туннели уходят в офлайн.
pub async fn close() {
    if let Some(mut child) = AGENT.lock().await.take() {
        let _ = child.kill().await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_api_replies() {
        let ok: Reply<String> = serde_json::from_str(r#"{"status":"success","data":"UserAccepted"}"#).unwrap();
        assert!(matches!(ok, Reply::Success(s) if s == "UserAccepted"));
        let fail: Reply<Value> = serde_json::from_str(r#"{"status":"fail","data":"RequiresVerifiedAccount"}"#).unwrap();
        let Reply::Fail(v) = fail else { panic!() };
        assert_eq!(api_reason(&v), "RequiresVerifiedAccount");
        let tunnel: AgentTunnel = serde_json::from_str(
            r#"{"id":"x","local_port":25565,"tunnel_type":"minecraft-java","assigned_domain":"abc.joinmc.link","custom_domain":null,"disabled":null}"#,
        )
        .unwrap();
        assert_eq!(tunnel.address().as_deref(), Some("abc.joinmc.link"));
    }
}
