use std::collections::HashMap;
use std::path::PathBuf;

use tokio::sync::{oneshot, Mutex};

use crate::auth::PendingLogin;
use crate::paths::Paths;
use crate::versions::Manifest;

pub struct AppState {
    pub paths: Paths,
    pub http: reqwest::Client,
    pub manifest: Mutex<Option<Manifest>>,
    /// Сборки, которые сейчас готовятся или запущены. None — идёт подготовка,
    /// Some(sender) — игра запущена, отправка в канал её завершает.
    pub running: Mutex<HashMap<String, Option<oneshot::Sender<()>>>>,
    /// Вход через Microsoft, который ждёт ввода кода
    pub login: Mutex<Option<PendingLogin>>,
    pub login_cancel: Mutex<Option<oneshot::Sender<()>>>,
}

impl AppState {
    pub fn new(root: PathBuf) -> Self {
        let http = reqwest::Client::builder()
            .user_agent(concat!("Shard/", env!("CARGO_PKG_VERSION")))
            .build()
            .expect("http client");
        Self {
            paths: Paths::new(root),
            http,
            manifest: Mutex::new(None),
            running: Mutex::new(HashMap::new()),
            login: Mutex::new(None),
            login_cancel: Mutex::new(None),
        }
    }
}
