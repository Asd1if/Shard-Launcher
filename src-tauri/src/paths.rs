//! Раскладка файлов лаунчера. Общие библиотеки, ассеты, версии и Java лежат один раз,
//! у каждой сборки — своя папка игры.
//!
//! <root>/
//!   accounts.json
//!   instances/<имя>/instance.json   ← папка называется как сборка, id — внутри instance.json
//!   instances/<имя>/minecraft/       ← папка игры (saves, mods, options.txt …)
//!   versions/<mc>/<mc>.json, <mc>.jar, natives/
//!   libraries/  assets/  runtimes/<component>/

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

use crate::error::{Error, Result};

#[derive(Clone)]
pub struct Paths {
    pub root: PathBuf,
    /// id сборки → имя её папки в instances
    folders: Arc<RwLock<HashMap<String, String>>>,
}

impl Paths {
    pub fn new(root: PathBuf) -> Self {
        Self { root, folders: Arc::default() }
    }

    pub fn accounts_file(&self) -> PathBuf {
        self.root.join("accounts.json")
    }

    pub fn instances(&self) -> PathBuf {
        self.root.join("instances")
    }

    pub fn instance_dir(&self, id: &str) -> Result<PathBuf> {
        // id приходит из фронтенда — не даём выйти за пределы папки сборок
        if id.is_empty() || !id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
            return Err(Error::msg(format!("invalid instance id: {id}")));
        }
        let cached = self.folders.read().expect("lock").get(id).cloned();
        if let Some(folder) = cached.filter(|f| self.instances().join(f).is_dir()) {
            return Ok(self.instances().join(folder));
        }
        self.scan_instances();
        let folder = self.folders.read().expect("lock").get(id).cloned();
        // Старые сборки и сборки без папки — по id
        Ok(self.instances().join(folder.as_deref().unwrap_or(id)))
    }

    /// Папка для новой сборки: по её имени, «Имя (2)» — если такая уже есть.
    pub fn new_instance_dir(&self, id: &str, name: &str) -> PathBuf {
        let folder = self.free_folder(name, None);
        self.folders.write().expect("lock").insert(id.to_owned(), folder.clone());
        self.instances().join(folder)
    }

    /// Переименовывает папку сборки под новое имя. Если файлы заняты (например, их открыл
    /// проводник), папка остаётся прежней — сборка от этого не ломается.
    pub fn rename_instance_dir(&self, id: &str, name: &str) {
        let Ok(current) = self.instance_dir(id) else { return };
        let Some(old) = current.file_name().map(|f| f.to_string_lossy().into_owned()) else { return };
        let folder = self.free_folder(name, Some(&old));
        if folder != old && std::fs::rename(&current, self.instances().join(&folder)).is_ok() {
            self.folders.write().expect("lock").insert(id.to_owned(), folder);
        }
    }

    /// Раньше папки назывались по id — переименовываем их по именам сборок.
    pub fn migrate_instance_folders(&self) {
        let Ok(entries) = std::fs::read_dir(self.instances()) else { return };
        for entry in entries.flatten() {
            let folder = entry.file_name().to_string_lossy().into_owned();
            let Some((id, name)) = read_identity(&entry.path()) else { continue };
            if folder == id {
                self.folders.write().expect("lock").insert(id.clone(), folder);
                self.rename_instance_dir(&id, &name);
            }
        }
    }

    fn scan_instances(&self) {
        let Ok(entries) = std::fs::read_dir(self.instances()) else { return };
        let mut map = self.folders.write().expect("lock");
        map.clear();
        for entry in entries.flatten() {
            if let Some((id, _)) = read_identity(&entry.path()) {
                map.insert(id, entry.file_name().to_string_lossy().into_owned());
            }
        }
    }

    /// Свободное имя папки; `keep` — текущая папка этой же сборки, она не считается занятой.
    fn free_folder(&self, name: &str, keep: Option<&str>) -> String {
        let base = folder_name(name);
        let free = |f: &str| keep.is_some_and(|k| k.eq_ignore_ascii_case(f)) || !self.instances().join(f).exists();
        if free(&base) {
            return base;
        }
        (2..).map(|n| format!("{base} ({n})")).find(|f| free(f)).expect("infinite range")
    }

    pub fn game_dir(&self, id: &str) -> Result<PathBuf> {
        Ok(self.instance_dir(id)?.join("minecraft"))
    }

    pub fn version_dir(&self, version: &str) -> PathBuf {
        self.root.join("versions").join(version)
    }

    pub fn libraries(&self) -> PathBuf {
        self.root.join("libraries")
    }

    pub fn assets(&self) -> PathBuf {
        self.root.join("assets")
    }

    pub fn runtimes(&self) -> PathBuf {
        self.root.join("runtimes")
    }
}

/// id и имя сборки из её instance.json.
fn read_identity(dir: &Path) -> Option<(String, String)> {
    let json: serde_json::Value = serde_json::from_slice(&std::fs::read(dir.join("instance.json")).ok()?).ok()?;
    Some((json["id"].as_str()?.to_owned(), json["name"].as_str().unwrap_or_default().to_owned()))
}

/// Имя сборки → допустимое имя папки Windows: без запрещённых символов, точек и пробелов
/// на конце и зарезервированных имён вроде CON.
pub fn folder_name(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| if c.is_control() || r#"<>:"/\|?*"#.contains(c) { '_' } else { c })
        .take(60)
        .collect();
    let cleaned = cleaned.trim().trim_end_matches(['.', ' ']).to_owned();
    if cleaned.is_empty() {
        return "Instance".into();
    }
    let stem = cleaned.split('.').next().unwrap_or_default().to_ascii_uppercase();
    let reserved = ["CON", "PRN", "AUX", "NUL"].contains(&stem.as_str())
        || (stem.len() == 4 && (stem.starts_with("COM") || stem.starts_with("LPT")) && stem.as_bytes()[3].is_ascii_digit());
    if reserved { format!("{cleaned}_") } else { cleaned }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn makes_safe_folder_names() {
        assert_eq!(folder_name("Мой мир: 1.21"), "Мой мир_ 1.21");
        assert_eq!(folder_name("  ...  "), "Instance");
        assert_eq!(folder_name("con"), "con_");
        assert_eq!(folder_name("pack."), "pack");
        assert_eq!(folder_name(r"a/b\c"), "a_b_c");
    }

    #[test]
    fn names_and_renames_instance_folders() {
        let root = std::env::temp_dir().join(format!("shard-paths-{}", uuid::Uuid::new_v4()));
        let paths = Paths::new(root.clone());
        let write = |dir: &Path, id: &str, name: &str| {
            std::fs::create_dir_all(dir).unwrap();
            std::fs::write(dir.join("instance.json"), format!(r#"{{"id":"{id}","name":"{name}"}}"#)).unwrap();
        };
        // Старая папка по id переименовывается при запуске
        write(&paths.instances().join("old-id"), "old-id", "Survival");
        paths.migrate_instance_folders();
        assert!(paths.instances().join("Survival").is_dir());
        assert_eq!(paths.instance_dir("old-id").unwrap(), paths.instances().join("Survival"));

        // Новая сборка с тем же именем получает «(2)»
        let dir = paths.new_instance_dir("new-id", "Survival");
        assert_eq!(dir, paths.instances().join("Survival (2)"));
        write(&dir, "new-id", "Survival");

        // Переименование, и id находится даже без кеша
        paths.rename_instance_dir("new-id", "Creative");
        assert_eq!(Paths::new(root.clone()).instance_dir("new-id").unwrap(), paths.instances().join("Creative"));
        std::fs::remove_dir_all(root).unwrap();
    }
}
