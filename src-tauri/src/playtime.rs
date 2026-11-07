//! Время в игре: сколько наиграно в каждой сборке, всего и по дням. Хранится в
//! `<сборка>/playtime.json`, дописывается после выхода из игры.

use std::collections::{BTreeMap, HashMap};

use serde::{Deserialize, Serialize};
use tauri::State;

use crate::error::Result;
use crate::paths::Paths;
use crate::state::AppState;

const FILE: &str = "playtime.json";

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct PlayTime {
    pub total_secs: u64,
    pub sessions: u32,
    /// «2026-11-07» → секунды
    pub days: BTreeMap<String, u64>,
}

fn load(paths: &Paths, id: &str) -> PlayTime {
    paths
        .instance_dir(id)
        .ok()
        .and_then(|d| std::fs::read(d.join(FILE)).ok())
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

/// Записывает сессию: время делится по дням, если игра шла через полночь.
pub fn record(paths: &Paths, id: &str, started: chrono::DateTime<chrono::Local>, ended: chrono::DateTime<chrono::Local>) -> Result<()> {
    let secs = (ended - started).num_seconds();
    // Меньше 10 секунд — игра не запустилась толком
    if secs < 10 {
        return Ok(());
    }
    let mut time = load(paths, id);
    time.total_secs += secs as u64;
    time.sessions += 1;
    let mut from = started;
    while from < ended {
        let midnight = (from.date_naive() + chrono::Days::new(1)).and_hms_opt(0, 0, 0).and_then(|m| m.and_local_timezone(chrono::Local).earliest()).unwrap_or(ended);
        let to = midnight.min(ended);
        *time.days.entry(from.format("%Y-%m-%d").to_string()).or_default() += (to - from).num_seconds().max(0) as u64;
        from = to;
    }
    // Храним последний год
    while time.days.len() > 366 {
        let first = time.days.keys().next().cloned().expect("not empty");
        time.days.remove(&first);
    }
    std::fs::write(paths.instance_dir(id)?.join(FILE), serde_json::to_vec_pretty(&time)?)?;
    Ok(())
}

#[tauri::command]
pub async fn playtime_get(state: State<'_, AppState>, id: String) -> Result<PlayTime> {
    Ok(load(&state.paths, &id))
}

/// Всего наиграно по каждой сборке — для списка сборок.
#[tauri::command]
pub async fn playtime_all(state: State<'_, AppState>) -> Result<HashMap<String, u64>> {
    Ok(crate::instances::list(&state.paths)
        .unwrap_or_default()
        .into_iter()
        .map(|i| (load(&state.paths, &i.id).total_secs, i.id))
        .filter(|(secs, _)| *secs > 0)
        .map(|(secs, id)| (id, secs))
        .collect())
}
