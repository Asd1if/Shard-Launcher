//! Статус в Discord («Играет в …») через локальный канал IPC клиента Discord.
//!
//! Протокол: кадр = опкод (u32 LE) + длина (u32 LE) + JSON. Опкод 0 — рукопожатие, 1 — команда.
//! Нужен Application ID приложения Discord; без него или без запущенного Discord ничего не делаем.

use serde_json::json;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::Mutex;

const CLIENT_ID: Option<&str> = option_env!("SHARD_DISCORD_CLIENT_ID");

#[cfg(windows)]
type Pipe = tokio::net::windows::named_pipe::NamedPipeClient;
#[cfg(unix)]
type Pipe = tokio::net::UnixStream;

static PIPE: Mutex<Option<Pipe>> = Mutex::const_new(None);

pub fn enabled() -> bool {
    CLIENT_ID.is_some_and(|id| !id.trim().is_empty())
}

/// Что показать в статусе.
pub struct Activity {
    pub details: String,
    pub state: String,
    /// Время начала игры, секунды Unix — Discord сам покажет «прошло 12:34»
    pub start: i64,
}

#[cfg(windows)]
async fn open() -> Option<Pipe> {
    use tokio::net::windows::named_pipe::ClientOptions;
    (0..10).find_map(|i| ClientOptions::new().open(format!(r"\\.\pipe\discord-ipc-{i}")).ok())
}

#[cfg(unix)]
async fn open() -> Option<Pipe> {
    let dir = ["XDG_RUNTIME_DIR", "TMPDIR", "TMP", "TEMP"].iter().find_map(|v| std::env::var(v).ok()).unwrap_or("/tmp".into());
    for i in 0..10 {
        if let Ok(s) = tokio::net::UnixStream::connect(format!("{dir}/discord-ipc-{i}")).await {
            return Some(s);
        }
    }
    None
}

async fn send(pipe: &mut Pipe, op: u32, body: &serde_json::Value) -> std::io::Result<()> {
    let data = serde_json::to_vec(body)?;
    let mut frame = Vec::with_capacity(8 + data.len());
    frame.extend(op.to_le_bytes());
    frame.extend((data.len() as u32).to_le_bytes());
    frame.extend(data);
    pipe.write_all(&frame).await
}

/// Ответ Discord на кадр. Читаем его, иначе буфер канала заполнится.
async fn receive(pipe: &mut Pipe) -> std::io::Result<serde_json::Value> {
    let mut header = [0u8; 8];
    pipe.read_exact(&mut header).await?;
    let len = u32::from_le_bytes(header[4..8].try_into().expect("4 bytes")) as usize;
    let mut body = vec![0u8; len.min(1 << 20)];
    pipe.read_exact(&mut body).await?;
    Ok(serde_json::from_slice(&body).unwrap_or_default())
}

async fn connect() -> Option<Pipe> {
    let id = CLIENT_ID?.trim();
    let mut pipe = open().await?;
    send(&mut pipe, 0, &json!({ "v": 1, "client_id": id })).await.ok()?;
    let ready = tokio::time::timeout(std::time::Duration::from_secs(3), receive(&mut pipe)).await.ok()?.ok()?;
    (ready["evt"] == "READY").then_some(pipe)
}

/// Что сейчас должно стоять в статусе: игра важнее лаунчера.
struct Wanted {
    disabled: bool,
    /// Статус открытого лаунчера (текст на языке интерфейса)
    idle: Option<(String, String)>,
    game: Option<Activity>,
    /// Когда открыт лаунчер — от этого времени Discord считает «прошло»
    opened: i64,
}

static WANTED: Mutex<Wanted> = Mutex::const_new(Wanted { disabled: false, idle: None, game: None, opened: 0 });

/// Статус лаунчера: `None` убирает его (игра всё равно покажется).
pub async fn set_idle(idle: Option<(String, String)>) {
    let mut w = WANTED.lock().await;
    if w.opened == 0 {
        w.opened = chrono::Utc::now().timestamp();
    }
    w.idle = idle;
    apply(&w).await;
}

/// Статус игры; `None` — игра закрыта, возвращаем статус лаунчера.
pub async fn set_game(game: Option<Activity>) {
    let mut w = WANTED.lock().await;
    w.game = game;
    apply(&w).await;
}

/// Выключатель из настроек.
pub async fn set_disabled(disabled: bool) {
    let mut w = WANTED.lock().await;
    w.disabled = disabled;
    apply(&w).await;
}

/// Discord могли запустить позже лаунчера или перезапустить — раз в 30 секунд
/// переподключаемся, если статус нужен, а связи нет.
pub fn watch() {
    if !enabled() {
        return;
    }
    tauri::async_runtime::spawn(async {
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(30)).await;
            let w = WANTED.lock().await;
            let wanted = !w.disabled && (w.game.is_some() || w.idle.is_some());
            if wanted && PIPE.lock().await.is_none() {
                apply(&w).await;
            }
        }
    });
}

async fn apply(w: &Wanted) {
    let activity = if w.disabled {
        None
    } else if let Some(g) = &w.game {
        Some((g.details.clone(), g.state.clone(), g.start))
    } else {
        w.idle.as_ref().map(|(d, s)| (d.clone(), s.clone(), w.opened))
    };
    send_activity(activity).await;
}

/// Ставит статус или убирает его (None). Ошибки не важны: Discord могли закрыть в любой момент.
async fn send_activity(activity: Option<(String, String, i64)>) {
    if !enabled() {
        return;
    }
    let mut guard = PIPE.lock().await;
    // Без Discord и без статуса подключаться незачем
    if guard.is_none() && activity.is_none() {
        return;
    }
    let activity = activity.map(|(details, state, start)| {
        let mut a = json!({
            "details": details,
            "timestamps": { "start": start },
            "assets": { "large_image": "logo", "large_text": "Shard" },
        });
        // Discord отклоняет пустой state
        if !state.is_empty() {
            a["state"] = state.into();
        }
        a
    });
    let body = json!({
        "cmd": "SET_ACTIVITY",
        "args": { "pid": std::process::id(), "activity": activity },
        "nonce": uuid::Uuid::new_v4().to_string(),
    });
    // Одна повторная попытка: соединение могло оборваться после перезапуска Discord
    for _ in 0..2 {
        if guard.is_none() {
            *guard = connect().await;
        }
        let Some(pipe) = guard.as_mut() else { return };
        let ok = send(pipe, 1, &body).await.is_ok()
            && tokio::time::timeout(std::time::Duration::from_secs(3), receive(pipe)).await.is_ok_and(|r| r.is_ok());
        if ok {
            return;
        }
        *guard = None;
    }
}

/// Статус лаунчера из интерфейса: текст приходит уже переведённым.
#[tauri::command]
pub async fn discord_idle(details: Option<String>, state: Option<String>) -> crate::error::Result<()> {
    set_idle(details.map(|d| (d, state.unwrap_or_default()))).await;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Живая проверка с запущенным Discord: `cargo test --lib discord -- --ignored --nocapture`
    #[tokio::test]
    #[ignore]
    async fn shows_activity_in_running_discord() {
        let mut pipe = connect().await.expect("Discord не запущен или Application ID неверный");
        let start = chrono::Utc::now().timestamp();
        let body = json!({
            "cmd": "SET_ACTIVITY",
            "args": { "pid": std::process::id(), "activity": {
                "details": "Проверка Shard", "state": "Fabric 1.21.1",
                "timestamps": { "start": start },
                "assets": { "large_image": "logo", "large_text": "Shard" },
            }},
            "nonce": "test",
        });
        send(&mut pipe, 1, &body).await.unwrap();
        let reply = receive(&mut pipe).await.unwrap();
        println!("{reply}");
        assert_ne!(reply["evt"], "ERROR", "{reply}");
        tokio::time::sleep(std::time::Duration::from_secs(20)).await;
    }
}
