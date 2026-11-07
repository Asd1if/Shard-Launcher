//! Список версий Minecraft из манифеста Mojang (релизы, снапшоты, бета и альфа).

use serde::{Deserialize, Serialize};
use tauri::State;

use crate::error::Result;
use crate::state::AppState;

const MANIFEST_URL: &str = "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VersionType {
    Release,
    Snapshot,
    OldBeta,
    OldAlpha,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Version {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: VersionType,
    pub url: String,
    pub sha1: String,
    pub release_time: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Latest {
    pub release: String,
    pub snapshot: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    pub latest: Latest,
    pub versions: Vec<Version>,
}

/// Манифест кешируется на время работы лаунчера.
pub async fn manifest(state: &AppState, refresh: bool) -> Result<Manifest> {
    let mut cached = state.manifest.lock().await;
    if let (Some(manifest), false) = (cached.as_ref(), refresh) {
        return Ok(manifest.clone());
    }
    let manifest: Manifest = state.http.get(MANIFEST_URL).send().await?.error_for_status()?.json().await?;
    *cached = Some(manifest.clone());
    Ok(manifest)
}

#[tauri::command]
pub async fn minecraft_versions(state: State<'_, AppState>, refresh: Option<bool>) -> Result<Manifest> {
    manifest(&state, refresh.unwrap_or(false)).await
}
