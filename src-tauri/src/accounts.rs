//! Аккаунты: оффлайн (только ник) и Microsoft (токены — в системном хранилище, см. auth.rs).

use md5::{Digest, Md5};
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::error::{Error, Result};
use crate::paths::Paths;
use crate::state::AppState;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AccountKind {
    Offline,
    Microsoft,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Account {
    pub id: String,
    pub kind: AccountKind,
    pub name: String,
    /// UUID игрока без дефисов
    pub uuid: String,
    /// Скин офлайн-аккаунта из библиотеки: его показывает мод CustomSkinLoader
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skin: Option<String>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Accounts {
    pub selected: Option<String>,
    pub accounts: Vec<Account>,
}

impl Accounts {
    pub fn current(&self) -> Option<&Account> {
        let id = self.selected.as_deref()?;
        self.accounts.iter().find(|a| a.id == id)
    }
}

pub fn load(paths: &Paths) -> Result<Accounts> {
    match std::fs::read(paths.accounts_file()) {
        Ok(bytes) => Ok(serde_json::from_slice(&bytes)?),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Accounts::default()),
        Err(e) => Err(e.into()),
    }
}

fn save(paths: &Paths, accounts: &Accounts) -> Result<()> {
    std::fs::create_dir_all(&paths.root)?;
    std::fs::write(paths.accounts_file(), serde_json::to_vec_pretty(accounts)?)?;
    Ok(())
}

/// Как в Java: UUID.nameUUIDFromBytes("OfflinePlayer:" + name) — UUID v3 без пространства имён.
/// Такой же UUID получает игрок на сервере в offline-mode.
pub fn offline_uuid(name: &str) -> String {
    let mut bytes: [u8; 16] = Md5::digest(format!("OfflinePlayer:{name}").as_bytes()).into();
    bytes[6] = (bytes[6] & 0x0f) | 0x30;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    hex::encode(bytes)
}

fn valid_nick(name: &str) -> bool {
    (3..=16).contains(&name.len()) && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Добавляет или обновляет Microsoft-аккаунт после входа и делает его текущим.
pub fn upsert_microsoft(paths: &Paths, profile: &crate::auth::McProfile) -> Result<Accounts> {
    let mut accounts = load(paths)?;
    let id = format!("msa-{}", profile.id);
    accounts.accounts.retain(|a| a.id != id);
    accounts.accounts.push(Account {
        id: id.clone(),
        kind: AccountKind::Microsoft,
        name: profile.name.clone(),
        uuid: profile.id.clone(),
        skin: None,
    });
    accounts.selected = Some(id);
    save(paths, &accounts)?;
    Ok(accounts)
}

/// Ник мог смениться на сайте Minecraft — обновляем его при запуске.
pub fn rename(paths: &Paths, id: &str, name: &str) -> Result<()> {
    let mut accounts = load(paths)?;
    if let Some(account) = accounts.accounts.iter_mut().find(|a| a.id == id) {
        if account.name != name {
            account.name = name.to_owned();
            save(paths, &accounts)?;
        }
    }
    Ok(())
}

#[tauri::command]
pub fn accounts_list(state: State<'_, AppState>) -> Result<Accounts> {
    load(&state.paths)
}

#[tauri::command]
pub fn account_add_offline(state: State<'_, AppState>, name: String) -> Result<Accounts> {
    let name = name.trim().to_owned();
    if !valid_nick(&name) {
        return Err(Error::msg("invalid nickname"));
    }
    let mut accounts = load(&state.paths)?;
    let uuid = offline_uuid(&name);
    let id = format!("offline-{uuid}");
    if !accounts.accounts.iter().any(|a| a.id == id) {
        accounts.accounts.push(Account { id: id.clone(), kind: AccountKind::Offline, name, uuid, skin: None });
    }
    accounts.selected = Some(id);
    save(&state.paths, &accounts)?;
    Ok(accounts)
}

/// Скин для офлайн-аккаунта (`None` — снять). В игре его покажет CustomSkinLoader.
#[tauri::command]
pub fn account_set_skin(state: State<'_, AppState>, id: String, skin: Option<String>) -> Result<Accounts> {
    let mut accounts = load(&state.paths)?;
    let account = accounts.accounts.iter_mut().find(|a| a.id == id).ok_or_else(|| Error::msg("account not found"))?;
    if account.kind != AccountKind::Offline {
        return Err(Error::msg("skin_needs_microsoft"));
    }
    account.skin = skin;
    save(&state.paths, &accounts)?;
    Ok(accounts)
}

#[tauri::command]
pub fn account_select(state: State<'_, AppState>, id: String) -> Result<Accounts> {
    let mut accounts = load(&state.paths)?;
    if !accounts.accounts.iter().any(|a| a.id == id) {
        return Err(Error::msg("account not found"));
    }
    accounts.selected = Some(id);
    save(&state.paths, &accounts)?;
    Ok(accounts)
}

#[tauri::command]
pub fn account_remove(state: State<'_, AppState>, id: String) -> Result<Accounts> {
    let mut accounts = load(&state.paths)?;
    if let Some(account) = accounts.accounts.iter().find(|a| a.id == id && a.kind == AccountKind::Microsoft) {
        crate::auth::forget(&account.uuid);
    }
    accounts.accounts.retain(|a| a.id != id);
    if accounts.selected.as_deref() == Some(id.as_str()) {
        accounts.selected = accounts.accounts.first().map(|a| a.id.clone());
    }
    save(&state.paths, &accounts)?;
    Ok(accounts)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn offline_uuid_matches_java() {
        // UUID.nameUUIDFromBytes("OfflinePlayer:Notch".getBytes(UTF_8))
        assert_eq!(offline_uuid("Notch"), "b50ad385829d3141a2167e7d7539ba7f");
    }
}
