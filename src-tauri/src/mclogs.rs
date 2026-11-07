//! Публикация логов на mclo.gs — ссылкой удобно поделиться в чате или с автором мода.

use serde::{Deserialize, Serialize};
use tauri::State;

use crate::error::{Error, Result};
use crate::state::AppState;

/// mclo.gs принимает до 10 МБ и 25 000 строк — шлём хвост, там самое важное.
const MAX_LINES: usize = 25_000;
const MAX_BYTES: usize = 10 * 1024 * 1024 - 1024;

#[derive(Deserialize)]
struct Response {
    success: bool,
    url: Option<String>,
    error: Option<String>,
}

#[derive(Serialize)]
pub struct Uploaded {
    pub url: String,
}

/// Убирает то, что не должно попасть в интернет: токены входа.
fn redact(text: &str) -> String {
    text.lines()
        .map(|line| {
            let mut out = line.to_owned();
            for marker in ["--accessToken ", "accessToken=", "accessToken: ", "--session "] {
                if let Some(i) = out.find(marker) {
                    let start = i + marker.len();
                    let end = out[start..].find(char::is_whitespace).map_or(out.len(), |e| start + e);
                    out.replace_range(start..end, "[скрыто]");
                }
            }
            // JWT-токены Microsoft и Minecraft начинаются с eyJ
            out.split(' ')
                .map(|w| if w.starts_with("eyJ") && w.len() > 60 { "[скрыто]" } else { w })
                .collect::<Vec<_>>()
                .join(" ")
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn tail(text: &str) -> &str {
    let mut start = text.len().saturating_sub(MAX_BYTES);
    while !text.is_char_boundary(start) {
        start += 1;
    }
    let text = &text[start..];
    match text.match_indices('\n').rev().nth(MAX_LINES - 1) {
        Some((i, _)) => &text[i + 1..],
        None => text,
    }
}

/// `content` — текст лога; без него берётся `logs/latest.log` сборки.
#[tauri::command]
pub async fn logs_upload(state: State<'_, AppState>, content: Option<String>, instance_id: Option<String>) -> Result<Uploaded> {
    let text = match (content, instance_id) {
        (Some(text), _) if !text.trim().is_empty() => text,
        (_, Some(id)) => {
            let path = state.paths.game_dir(&id)?.join("logs").join("latest.log");
            String::from_utf8_lossy(&std::fs::read(path).map_err(|_| Error::msg("logs_empty"))?).into_owned()
        }
        _ => return Err(Error::msg("logs_empty")),
    };
    let text = redact(tail(&text));
    if text.trim().is_empty() {
        return Err(Error::msg("logs_empty"));
    }
    let response: Response = state
        .http
        .post("https://api.mclo.gs/1/log")
        .form(&[("content", text)])
        .send()
        .await?
        .json()
        .await?;
    match (response.success, response.url) {
        (true, Some(url)) => Ok(Uploaded { url }),
        _ => Err(Error::msg(format!("mclogs_failed: {}", response.error.unwrap_or_default()))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hides_tokens_and_keeps_the_tail() {
        let log = "Setting user: Steve\n--accessToken eyJhbGciOiJIUzI1NiJ9.abc --version 1.21\ntoken eyJ".to_owned() + &"x".repeat(80);
        let clean = redact(&log);
        assert!(clean.contains("Setting user: Steve"));
        assert!(clean.contains("--accessToken [скрыто] --version 1.21"));
        assert!(!clean.contains("eyJ"));

        let many: String = (0..MAX_LINES + 10).map(|i| format!("line {i}\n")).collect();
        let t = tail(&many);
        assert!(t.starts_with("line 11\n") || t.starts_with("line 10\n"));
    }
}
