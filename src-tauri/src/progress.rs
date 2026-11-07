//! Прогресс долгих файловых операций (перенос данных, копия сборки, бэкап мира) в байтах.

use std::path::Path;
use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::ipc::Channel;

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    pub done: u64,
    pub total: u64,
}

/// Считает байты и шлёт прогресс не чаще 10 раз в секунду.
pub struct Tracker {
    channel: Channel<Progress>,
    done: u64,
    total: u64,
    sent: Instant,
}

impl Tracker {
    pub fn new(channel: Channel<Progress>, total: u64) -> Self {
        let _ = channel.send(Progress { done: 0, total });
        Self { channel, done: 0, total, sent: Instant::now() }
    }

    pub fn add(&mut self, bytes: u64) {
        self.done = (self.done + bytes).min(self.total);
        if self.sent.elapsed() >= Duration::from_millis(100) || self.done == self.total {
            self.sent = Instant::now();
            let _ = self.channel.send(Progress { done: self.done, total: self.total });
        }
    }
}

/// Сколько файлов в папке (для прогресса удаления — там важно число файлов, а не байты).
pub fn count_files(path: &Path) -> u64 {
    let Ok(meta) = std::fs::symlink_metadata(path) else { return 0 };
    if !meta.is_dir() {
        return 1;
    }
    std::fs::read_dir(path).map_or(0, |entries| entries.flatten().map(|e| count_files(&e.path())).sum())
}

/// Удаляет папку по файлу, сообщая прогресс в штуках.
pub fn remove_dir(path: &Path, channel: Channel<Progress>) -> std::io::Result<()> {
    if !path.exists() {
        return Ok(());
    }
    let mut tracker = Tracker::new(channel, count_files(path));
    remove_inner(path, &mut tracker)?;
    // На случай пустых папок и того, что появилось во время удаления
    match std::fs::remove_dir_all(path) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e),
        _ => Ok(()),
    }
}

fn remove_inner(path: &Path, tracker: &mut Tracker) -> std::io::Result<()> {
    for entry in std::fs::read_dir(path)?.flatten() {
        let p = entry.path();
        if entry.file_type()?.is_dir() {
            remove_inner(&p, tracker)?;
            let _ = std::fs::remove_dir(&p);
        } else {
            if let Err(e) = std::fs::remove_file(&p) {
                // Файлы «только для чтения» на Windows удаляются после снятия атрибута
                let mut perms = std::fs::metadata(&p)?.permissions();
                #[allow(clippy::permissions_set_readonly_false)]
                perms.set_readonly(false);
                std::fs::set_permissions(&p, perms).map_err(|_| e)?;
                std::fs::remove_file(&p)?;
            }
            tracker.add(1);
        }
    }
    Ok(())
}

/// Размер файла или папки целиком; недоступное пропускаем.
pub fn size_of(path: &Path) -> u64 {
    let Ok(meta) = std::fs::symlink_metadata(path) else { return 0 };
    if !meta.is_dir() {
        return meta.len();
    }
    std::fs::read_dir(path).map_or(0, |entries| entries.flatten().map(|e| size_of(&e.path())).sum())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn removes_folder_counting_files() {
        let root = std::env::temp_dir().join(format!("shard-rm-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(root.join("a/b")).unwrap();
        for f in ["x.txt", "a/y.txt", "a/b/z.txt"] {
            std::fs::write(root.join(f), "1").unwrap();
        }
        assert_eq!(count_files(&root), 3);
        let last = std::sync::Arc::new(std::sync::Mutex::new((0u64, 0u64)));
        let seen = last.clone();
        let channel = Channel::new(move |body| {
            if let tauri::ipc::InvokeResponseBody::Json(json) = body {
                let v: serde_json::Value = serde_json::from_str(&json).unwrap();
                *seen.lock().unwrap() = (v["done"].as_u64().unwrap(), v["total"].as_u64().unwrap());
            }
            Ok(())
        });
        remove_dir(&root, channel).unwrap();
        assert!(!root.exists());
        assert_eq!(*last.lock().unwrap(), (3, 3));
    }
}
