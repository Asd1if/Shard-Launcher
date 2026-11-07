//! Перенос настроек игры (options.txt) из одной сборки в другие: управление, звук и
//! графика по отдельности. Остальные строки в целевой сборке не трогаем.

use serde::Deserialize;
use tauri::State;

use crate::error::{Error, Result};
use crate::state::AppState;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Part {
    Controls,
    Sound,
    Video,
}

/// Какая группа у строки options.txt.
fn part_of(key: &str) -> Part {
    if key.starts_with("key_") || matches!(key, "mouseSensitivity" | "invertYMouse" | "autoJump" | "toggleCrouch" | "toggleSprint" | "rawMouseInput" | "touchscreen" | "mouseWheelSensitivity" | "discrete_mouse_scroll") {
        Part::Controls
    } else if key.starts_with("soundCategory_") || matches!(key, "soundDevice" | "showSubtitles" | "directionalAudio") {
        Part::Sound
    } else {
        Part::Video
    }
}

fn parse(text: &str) -> Vec<(String, String)> {
    text.lines().filter_map(|l| l.split_once(':')).map(|(k, v)| (k.to_owned(), v.to_owned())).collect()
}

/// Заменяет в `target` значения выбранных групп на значения из `source`.
fn merge(source: &str, target: &str, parts: &[Part]) -> String {
    // version — формат файла, его оставляем целевым, чтобы игра не сбросила настройки
    let take = |k: &str| k != "version" && parts.contains(&part_of(k));
    let from: Vec<(String, String)> = parse(source).into_iter().filter(|(k, _)| take(k)).collect();
    let mut out: Vec<(String, String)> = parse(target);
    for (k, v) in from {
        match out.iter_mut().find(|(key, _)| *key == k) {
            Some(entry) => entry.1 = v,
            None => out.push((k, v)),
        }
    }
    out.into_iter().map(|(k, v)| format!("{k}:{v}\n")).collect()
}

#[tauri::command]
pub fn options_copy(state: State<'_, AppState>, from: String, to: Vec<String>, parts: Vec<Part>) -> Result<u32> {
    if parts.is_empty() {
        return Ok(0);
    }
    let source = std::fs::read_to_string(state.paths.game_dir(&from)?.join("options.txt")).map_err(|_| Error::msg("options_missing"))?;
    let mut done = 0;
    for id in to.iter().filter(|id| **id != from) {
        let path = state.paths.game_dir(id)?.join("options.txt");
        let target = std::fs::read_to_string(&path).unwrap_or_default();
        std::fs::create_dir_all(path.parent().expect("has parent"))?;
        std::fs::write(&path, merge(&source, &target, &parts))?;
        done += 1;
    }
    Ok(done)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merges_only_chosen_parts() {
        let source = "version:3955\nkey_key.jump:key.keyboard.space\nsoundCategory_master:0.5\nrenderDistance:16\n";
        let target = "version:3700\nkey_key.jump:key.keyboard.j\nsoundCategory_master:1.0\nrenderDistance:8\nlang:ru_ru\n";
        let out = merge(source, target, &[Part::Controls, Part::Sound]);
        assert!(out.contains("version:3700"));
        assert!(out.contains("key_key.jump:key.keyboard.space"));
        assert!(out.contains("soundCategory_master:0.5"));
        assert!(out.contains("renderDistance:8"));
        assert!(out.contains("lang:ru_ru"));
    }
}
