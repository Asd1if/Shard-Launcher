//! Игроки и права на сервере: операторы, белый список, баны, кик.
//!
//! Пока сервер запущен, всё делается командами консоли — сервер сам перепишет свои файлы.
//! Когда остановлен — правим ops.json, whitelist.json и banned-players.json напрямую.

use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tauri::State;

use crate::error::{Error, Result};
use crate::server::{self, server_dir};
use crate::state::AppState;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AccessList {
    Ops,
    Whitelist,
    Banned,
}

impl AccessList {
    fn file(self) -> &'static str {
        match self {
            AccessList::Ops => "ops.json",
            AccessList::Whitelist => "whitelist.json",
            AccessList::Banned => "banned-players.json",
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Player {
    name: String,
    uuid: String,
    /// Причина бана
    reason: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Access {
    online: Vec<String>,
    ops: Vec<Player>,
    whitelist: Vec<Player>,
    banned: Vec<Player>,
    whitelist_on: bool,
    online_mode: bool,
}

fn read_list(dir: &Path, list: AccessList) -> Vec<Value> {
    std::fs::read(dir.join(list.file())).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
}

fn players(dir: &Path, list: AccessList) -> Vec<Player> {
    let mut out: Vec<Player> = read_list(dir, list)
        .into_iter()
        .filter_map(|v| {
            Some(Player {
                name: v.get("name")?.as_str()?.to_owned(),
                uuid: v.get("uuid").and_then(Value::as_str).unwrap_or_default().to_owned(),
                reason: v.get("reason").and_then(Value::as_str).map(str::to_owned),
            })
        })
        .collect();
    out.sort_by_key(|p| p.name.to_lowercase());
    out
}

fn prop(dir: &Path, key: &str) -> Option<String> {
    server::read_properties(dir).into_iter().find(|(k, _)| k == key).map(|(_, v)| v)
}

#[tauri::command]
pub async fn server_access(state: State<'_, AppState>, id: String) -> Result<Access> {
    let dir = server_dir(&state, &id)?;
    Ok(Access {
        online: server::online_players(&id).await,
        ops: players(&dir, AccessList::Ops),
        whitelist: players(&dir, AccessList::Whitelist),
        banned: players(&dir, AccessList::Banned),
        whitelist_on: prop(&dir, "white-list").as_deref() == Some("true"),
        online_mode: prop(&dir, "online-mode").as_deref() != Some("false"),
    })
}

/// Ник Minecraft: 3–16 латинских букв, цифр и «_». Заодно защищает консоль от лишних команд.
fn checked_nick(name: &str) -> Result<&str> {
    let name = name.trim();
    if (3..=16).contains(&name.len()) && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        Ok(name)
    } else {
        Err(Error::msg("bad_nick"))
    }
}

fn dashed(hex: &str) -> String {
    if hex.len() != 32 {
        return hex.to_owned();
    }
    format!("{}-{}-{}-{}-{}", &hex[..8], &hex[8..12], &hex[12..16], &hex[16..20], &hex[20..])
}

/// UUID игрока: на лицензионном сервере — с Mojang, иначе тот, что сервер даст в offline-mode.
async fn uuid_for(state: &AppState, name: &str, online_mode: bool) -> Result<(String, String)> {
    if !online_mode {
        return Ok((name.to_owned(), dashed(&crate::accounts::offline_uuid(name))));
    }
    #[derive(Deserialize)]
    struct Profile {
        id: String,
        name: String,
    }
    let response = state.http.get(format!("https://api.mojang.com/users/profiles/minecraft/{name}")).send().await?;
    if response.status() == reqwest::StatusCode::NOT_FOUND || response.status() == reqwest::StatusCode::NO_CONTENT {
        return Err(Error::msg("player_not_found"));
    }
    let profile: Profile = response.error_for_status()?.json().await?;
    Ok((profile.name, dashed(&profile.id)))
}

#[tauri::command]
pub async fn server_access_change(
    state: State<'_, AppState>,
    id: String,
    list: AccessList,
    name: String,
    add: bool,
    reason: Option<String>,
) -> Result<()> {
    let dir = server_dir(&state, &id)?;
    let name = checked_nick(&name)?.to_owned();
    // Причина — одна строка без переводов: она уходит в консоль
    let reason = reason.map(|r| r.replace(['\n', '\r'], " ").trim().to_owned()).filter(|r| !r.is_empty());

    if server::is_running(&id).await {
        let command = match (list, add) {
            (AccessList::Ops, true) => format!("op {name}"),
            (AccessList::Ops, false) => format!("deop {name}"),
            (AccessList::Whitelist, true) => format!("whitelist add {name}"),
            (AccessList::Whitelist, false) => format!("whitelist remove {name}"),
            (AccessList::Banned, true) => match &reason {
                Some(r) => format!("ban {name} {r}"),
                None => format!("ban {name}"),
            },
            (AccessList::Banned, false) => format!("pardon {name}"),
        };
        return server::server_command(id, command).await;
    }

    let mut entries = read_list(&dir, list);
    let same = |v: &Value| v.get("name").and_then(Value::as_str).is_some_and(|n| n.eq_ignore_ascii_case(&name));
    if add {
        if entries.iter().any(same) {
            return Ok(());
        }
        let online_mode = prop(&dir, "online-mode").as_deref() != Some("false");
        let (name, uuid) = uuid_for(&state, &name, online_mode).await?;
        entries.push(match list {
            AccessList::Ops => json!({ "uuid": uuid, "name": name, "level": 4, "bypassesPlayerLimit": false }),
            AccessList::Whitelist => json!({ "uuid": uuid, "name": name }),
            AccessList::Banned => json!({
                "uuid": uuid,
                "name": name,
                "created": chrono::Local::now().format("%Y-%m-%d %H:%M:%S %z").to_string(),
                "source": "Shard",
                "expires": "forever",
                "reason": reason.unwrap_or_else(|| "Banned by an operator.".to_owned()),
            }),
        });
    } else {
        entries.retain(|v| !same(v));
    }
    std::fs::write(dir.join(list.file()), serde_json::to_vec_pretty(&entries)?)?;
    Ok(())
}

#[tauri::command]
pub async fn server_kick(id: String, name: String, reason: Option<String>) -> Result<()> {
    let name = checked_nick(&name)?.to_owned();
    let reason = reason.map(|r| r.replace(['\n', '\r'], " ").trim().to_owned()).filter(|r| !r.is_empty());
    server::server_command(id, match reason {
        Some(r) => format!("kick {name} {r}"),
        None => format!("kick {name}"),
    })
    .await
}
