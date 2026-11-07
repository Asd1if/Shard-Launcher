//! Библиотеки из version JSON: что качать, что класть в classpath, откуда брать нативные файлы.

use std::io::Read;
use std::path::{Path, PathBuf};

use crate::download::Download;
use crate::error::Result;

use super::version::{maven_path, os_name, rules_allow, Features, Library};

const MOJANG_LIBRARIES: &str = "https://libraries.minecraft.net/";

#[derive(Debug, Default)]
pub struct ResolvedLibraries {
    pub downloads: Vec<Download>,
    pub classpath: Vec<PathBuf>,
    /// Jar-файлы, из которых нужно распаковать .dll/.so/.dylib
    pub native_jars: Vec<PathBuf>,
}

pub fn resolve(libraries: &[Library], libraries_dir: &Path) -> ResolvedLibraries {
    let mut out = ResolvedLibraries::default();
    let arch = if cfg!(target_pointer_width = "64") { "64" } else { "32" };

    for lib in libraries {
        if !rules_allow(lib.rules.as_deref().unwrap_or_default(), Features::default()) {
            continue;
        }

        let artifact = lib.downloads.as_ref().and_then(|d| d.artifact.as_ref());
        if let Some(artifact) = artifact {
            let Some(rel) = artifact.path.clone().or_else(|| maven_path(&lib.name)) else { continue };
            let path = libraries_dir.join(rel);
            // Пустая ссылка у Forge — файл берётся из установщика или создаётся его процессорами
            if !artifact.url.is_empty() {
                out.downloads.push(Download {
                    url: artifact.url.clone(),
                    path: path.clone(),
                    sha1: artifact.sha1.clone(),
                    size: artifact.size,
                });
            }
            // Новый формат (1.19+): нативные библиотеки — обычные артефакты с классификатором natives-*
            if lib.name.contains(":natives-") {
                out.native_jars.push(path.clone());
            }
            if !out.classpath.contains(&path) {
                out.classpath.push(path);
            }
        } else if lib.downloads.is_none() {
            // Библиотека из Maven-репозитория без явной ссылки; без url — из репозитория Mojang
            if let Some(rel) = maven_path(&lib.name) {
                let base = lib.url.as_deref().unwrap_or(MOJANG_LIBRARIES);
                let base = base.replace("http://files.minecraftforge.net/maven", "https://maven.minecraftforge.net");
                let path = libraries_dir.join(&rel);
                out.downloads.push(Download {
                    url: format!("{}/{rel}", base.trim_end_matches('/')),
                    path: path.clone(),
                    sha1: lib.sha1.clone(),
                    size: lib.size,
                });
                if !out.classpath.contains(&path) {
                    out.classpath.push(path);
                }
            }
        }

        // Старый формат: нативные файлы в отдельном классификаторе, в classpath не идут
        let classifier = lib
            .natives
            .as_ref()
            .and_then(|n| n.get(os_name()))
            .map(|key| key.replace("${arch}", arch));
        if let Some(key) = classifier {
            let native = lib
                .downloads
                .as_ref()
                .and_then(|d| d.classifiers.as_ref())
                .and_then(|c| c.get(&key));
            if let Some(native) = native {
                let rel = native
                    .path
                    .clone()
                    .or_else(|| maven_path(&format!("{}:{key}", lib.name)))
                    .unwrap_or_default();
                let path = libraries_dir.join(rel);
                out.downloads.push(Download {
                    url: native.url.clone(),
                    path: path.clone(),
                    sha1: native.sha1.clone(),
                    size: native.size,
                });
                out.native_jars.push(path);
            }
        }
    }
    out
}

/// Распаковывает нативные файлы в одну папку (её получает -Djava.library.path).
pub fn extract_natives(jars: &[PathBuf], target: &Path) -> Result<()> {
    std::fs::create_dir_all(target)?;
    for jar in jars {
        let mut archive = zip::ZipArchive::new(std::fs::File::open(jar)?)?;
        for i in 0..archive.len() {
            let mut entry = archive.by_index(i)?;
            let name = entry.name().to_owned();
            if entry.is_dir() || name.starts_with("META-INF/") {
                continue;
            }
            let is_native = [".dll", ".so", ".dylib", ".jnilib"].iter().any(|ext| name.ends_with(ext));
            if !is_native {
                continue;
            }
            let Some(file_name) = Path::new(&name).file_name() else { continue };
            let dest = target.join(file_name);
            if dest.metadata().is_ok_and(|m| m.len() == entry.size()) {
                continue;
            }
            let mut bytes = Vec::with_capacity((entry.size() as usize).min(1 << 24));
            entry.read_to_end(&mut bytes)?;
            std::fs::write(dest, bytes)?;
        }
    }
    Ok(())
}
