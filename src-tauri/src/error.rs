use serde::{Serialize, Serializer};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("network error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("file error: {0}")]
    Io(#[from] std::io::Error),
    #[error("bad JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("archive error: {0}")]
    Zip(#[from] zip::result::ZipError),
    #[error("{0}")]
    Msg(String),
}

impl Error {
    pub fn msg(text: impl Into<String>) -> Self {
        Error::Msg(text.into())
    }
}

// Tauri-команды отдают ошибку во фронтенд строкой.
impl Serialize for Error {
    fn serialize<S: Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

pub type Result<T> = std::result::Result<T, Error>;
