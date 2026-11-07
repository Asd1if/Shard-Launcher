//! Снимки сборки: моды, конфиги и версия загрузчика/модпака в одном zip. Делаются
//! вручную или сами перед обновлениями; откат возвращает сборку к снимку.
//! Хранятся в `<сборка>/snapshots/<время>_<причина>.zip`.

use std::io::Read;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::ipc::Channel;
use tauri::State;

use crate::error::{Error, Result};
use crate::instances::{self, Instance};
use crate::paths::Paths;
use crate::state::AppState;

/// Папки игры, которые меняются при обновлениях
const DIRS: [&str; 4] = ["mods", "config", "defaultconfigs", "kubejs"];
/// Автоматических снимков храним столько, ручные не удаляем
const KEEP_AUTO: usize = 5;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub file: String,
    /// manual / update / pack / restore
    pub reason: String,
    pub created_at: String,
    pub size: u64,
    pub mods: u32,
}

/// Прогресс снимка и отката в байтах. `stage`: save — сохраняем снимок, restore — распаковываем.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapProgress {
    pub stage: &'static str,
    pub done: u64,
    pub total: u64,
}

/// Шлёт прогресс не чаще 10 раз в секунду. Без канала — молчит (автоснимки).
pub struct Report {
    channel: Option<Channel<SnapProgress>>,
    stage: &'static str,
    done: u64,
    total: u64,
    sent: Instant,
}

impl Report {
    pub fn new(channel: Option<Channel<SnapProgress>>) -> Self {
        Self { channel, stage: "save", done: 0, total: 0, sent: Instant::now() }
    }

    fn start(&mut self, stage: &'static str, total: u64) {
        (self.stage, self.done, self.total) = (stage, 0, total);
        self.send();
    }

    fn add(&mut self, bytes: u64) {
        self.done = (self.done + bytes).min(self.total);
        if self.sent.elapsed() >= Duration::from_millis(100) || self.done == self.total {
            self.send();
        }
    }

    fn send(&mut self) {
        self.sent = Instant::now();
        if let Some(ch) = &self.channel {
            let _ = ch.send(SnapProgress { stage: self.stage, done: self.done, total: self.total });
        }
    }
}

fn dir(paths: &Paths, id: &str) -> Result<PathBuf> {
    Ok(paths.instance_dir(id)?.join("snapshots"))
}

fn list(paths: &Paths, id: &str) -> Result<Vec<Snapshot>> {
    let Ok(entries) = std::fs::read_dir(dir(paths, id)?) else { return Ok(vec![]) };
    let mut out: Vec<Snapshot> = entries
        .flatten()
        .filter_map(|e| {
            let file = e.file_name().into_string().ok()?;
            let stem = file.strip_suffix(".zip")?;
            let (time, reason) = stem.split_once('_')?;
            let created = chrono::NaiveDateTime::parse_from_str(time, "%Y%m%d-%H%M%S").ok()?;
            let mods = zip::ZipArchive::new(std::fs::File::open(e.path()).ok()?)
                .map(|z| z.file_names().filter(|n| n.starts_with("minecraft/mods/") && n.ends_with(".jar")).count() as u32)
                .unwrap_or(0);
            Some(Snapshot {
                created_at: created.and_local_timezone(chrono::Local).earliest()?.to_rfc3339(),
                reason: reason.to_owned(),
                size: e.metadata().map(|m| m.len()).unwrap_or(0),
                mods,
                file,
            })
        })
        .collect();
    out.sort_by(|a, b| b.file.cmp(&a.file));
    Ok(out)
}

fn checked_file(file: &str) -> Result<&str> {
    if file.contains(['/', '\\']) || !file.ends_with(".zip") || file.starts_with('.') {
        return Err(Error::msg("bad_path"));
    }
    Ok(file)
}

/// Делает снимок. `reason` — латиница, попадает в имя файла.
pub fn make(paths: &Paths, id: &str, reason: &str, report: &mut Report) -> Result<()> {
    let game = paths.game_dir(id)?;
    let target_dir = dir(paths, id)?;
    std::fs::create_dir_all(&target_dir)?;
    let target = target_dir.join(format!("{}_{reason}.zip", chrono::Local::now().format("%Y%m%d-%H%M%S")));
    let sources: Vec<(PathBuf, String)> =
        DIRS.iter().map(|d| (game.join(d), format!("minecraft/{d}"))).filter(|(p, _)| p.is_dir()).collect();
    report.start("save", sources.iter().map(|(p, _)| crate::worlds::dir_size(p)).sum());
    crate::worlds::zip_dirs(&sources, &target, &mut |bytes| report.add(bytes))?;
    // instance.json — отдельным zip-ом рядом не плодим: дописываем в тот же архив
    let instance = std::fs::read(paths.instance_dir(id)?.join("instance.json"))?;
    let mut zip = zip::ZipWriter::new_append(std::fs::OpenOptions::new().read(true).write(true).open(&target)?)?;
    zip.start_file("instance.json", zip::write::SimpleFileOptions::default())?;
    std::io::Write::write_all(&mut zip, &instance)?;
    zip.finish()?;

    if reason != "manual" {
        for old in list(paths, id)?.into_iter().filter(|s| s.reason != "manual").skip(KEEP_AUTO) {
            let _ = std::fs::remove_file(target_dir.join(old.file));
        }
    }
    Ok(())
}

/// Снимок перед обновлением: ошибки не мешают самому обновлению.
pub fn auto(paths: &Paths, id: &str, reason: &str) {
    if let Err(e) = make(paths, id, reason, &mut Report::new(None)) {
        eprintln!("snapshot before {reason} failed for {id}: {e}");
    }
}

#[tauri::command]
pub fn snapshots(state: State<'_, AppState>, id: String) -> Result<Vec<Snapshot>> {
    list(&state.paths, &id)
}

#[tauri::command]
pub async fn snapshot_create(
    state: State<'_, AppState>,
    id: String,
    reason: Option<String>,
    progress: Channel<SnapProgress>,
) -> Result<Vec<Snapshot>> {
    let reason = match reason.as_deref() {
        Some("update") => "update",
        _ => "manual",
    };
    let paths = state.paths.clone();
    let id2 = id.clone();
    tokio::task::spawn_blocking(move || make(&paths, &id2, reason, &mut Report::new(Some(progress)))).await.map_err(|e| Error::msg(e.to_string()))??;
    list(&state.paths, &id)
}

#[tauri::command]
pub fn snapshot_delete(state: State<'_, AppState>, id: String, file: String) -> Result<Vec<Snapshot>> {
    let file = checked_file(&file)?;
    std::fs::remove_file(dir(&state.paths, &id)?.join(file))?;
    list(&state.paths, &id)
}

/// Откат: папки из снимка заменяют текущие, версия загрузчика и модпака — из снимка.
/// Текущее состояние перед этим тоже сохраняется — откат можно отменить.
#[tauri::command]
pub async fn snapshot_restore(state: State<'_, AppState>, id: String, file: String, progress: Channel<SnapProgress>) -> Result<Instance> {
    if state.running.lock().await.contains_key(&id) {
        return Err(Error::msg("instance_running"));
    }
    let file = checked_file(&file)?.to_owned();
    let paths = state.paths.clone();
    tokio::task::spawn_blocking(move || -> Result<Instance> {
        let archive = dir(&paths, &id)?.join(&file);
        if !archive.is_file() {
            return Err(Error::msg("bad_path"));
        }
        let mut report = Report::new(Some(progress));
        make(&paths, &id, "restore", &mut report)?;
        // Сначала распаковываем рядом, и только потом меняем папки: сбой посередине
        // не оставит сборку без модов
        let staging = paths.instance_dir(&id)?.join(".snapshot-restore");
        let _ = std::fs::remove_dir_all(&staging);
        unzip_with(&archive, &staging, &mut report)?;
        let game = paths.game_dir(&id)?;
        for d in DIRS {
            let path = game.join(d);
            if path.is_dir() {
                std::fs::remove_dir_all(&path)?;
            }
            let from = staging.join("minecraft").join(d);
            if from.is_dir() {
                std::fs::rename(&from, &path)?;
            }
        }
        let mut instance = instances::load(&paths, &id)?;
        if let Some(old) = std::fs::read(staging.join("instance.json")).ok().and_then(|b| serde_json::from_slice::<Instance>(&b).ok()) {
            instance.mc_version = old.mc_version;
            instance.version_type = old.version_type;
            instance.loader = old.loader;
            instance.loader_version = old.loader_version;
            instance.pack = old.pack;
        }
        let _ = std::fs::remove_dir_all(&staging);
        crate::content::refresh_mod_count(&paths, &mut instance)?;
        Ok(instance)
    })
    .await
    .map_err(|e| Error::msg(e.to_string()))?
}

/// Распаковка с прогрессом в байтах; пути проверяются, за пределы `dest` не выходим.
fn unzip_with(archive: &std::path::Path, dest: &std::path::Path, report: &mut Report) -> Result<()> {
    let mut zip = zip::ZipArchive::new(std::fs::File::open(archive)?)?;
    let total = (0..zip.len()).filter_map(|i| zip.by_index(i).ok().map(|e| e.size())).sum();
    report.start("restore", total);
    let mut buf = vec![0u8; 256 * 1024];
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i)?;
        let path = crate::modpack::safe_join(dest, entry.name())?;
        if entry.is_dir() {
            std::fs::create_dir_all(&path)?;
            continue;
        }
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut out = std::fs::File::create(&path)?;
        loop {
            let n = entry.read(&mut buf)?;
            if n == 0 {
                break;
            }
            std::io::Write::write_all(&mut out, &buf[..n])?;
            report.add(n as u64);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn makes_and_lists_snapshot() {
        let root = std::env::temp_dir().join(format!("shard-snap-{}", std::process::id()));
        let paths = Paths::new(root.clone());
        let inst = paths.instance_dir("test-1").unwrap();
        std::fs::create_dir_all(inst.join("minecraft/mods")).unwrap();
        std::fs::write(inst.join("minecraft/mods/a.jar"), b"x").unwrap();
        std::fs::write(inst.join("instance.json"), b"{}").unwrap();
        make(&paths, "test-1", "manual", &mut Report::new(None)).unwrap();
        let list = list(&paths, "test-1").unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].mods, 1);
        assert_eq!(list[0].reason, "manual");
        let _ = std::fs::remove_dir_all(root);
    }
}
