//! Java от Mojang: тот же рантайм, что ставит официальный лаунчер, под нужную версию игры.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::download::{download_all, download_one, Download};
use crate::error::{Error, Result};

const RUNTIMES_URL: &str =
    "https://launchermeta.mojang.com/v1/products/java-runtime/2ec0cc96c44e5a76b9c8b7c39df7210883d12871/all.json";

/// Версии без javaVersion в JSON (старше 1.17) запускаются на Java 8.
pub const LEGACY_COMPONENT: &str = "jre-legacy";

#[derive(Debug, Deserialize)]
struct RuntimeEntry {
    manifest: ManifestRef,
    version: RuntimeVersion,
}

#[derive(Debug, Deserialize)]
struct ManifestRef {
    url: String,
    sha1: String,
    size: u64,
}

#[derive(Debug, Deserialize)]
struct RuntimeVersion {
    name: String,
}

#[derive(Debug, Deserialize)]
struct RuntimeManifest {
    files: HashMap<String, RuntimeFile>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
enum RuntimeFile {
    File {
        downloads: RawDownloads,
        #[serde(default)]
        executable: bool,
    },
    Directory,
    Link {
        #[cfg_attr(not(unix), allow(dead_code))]
        target: String,
    },
}

#[derive(Debug, Deserialize)]
struct RawDownloads {
    raw: ManifestRef,
}

fn platform() -> &'static str {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("windows", "x86_64") => "windows-x64",
        ("windows", "aarch64") => "windows-arm64",
        ("windows", _) => "windows-x86",
        ("macos", "aarch64") => "mac-os-arm64",
        ("macos", _) => "mac-os",
        ("linux", "x86") => "linux-i386",
        _ => "linux",
    }
}

fn java_executable(runtime_dir: &Path) -> PathBuf {
    if cfg!(target_os = "windows") {
        // javaw — без лишнего консольного окна
        runtime_dir.join("bin").join("javaw.exe")
    } else if cfg!(target_os = "macos") {
        runtime_dir.join("jre.bundle/Contents/Home/bin/java")
    } else {
        runtime_dir.join("bin").join("java")
    }
}

/// Проверяет, что нужная Java установлена, и при необходимости скачивает её. Возвращает путь к java.
pub async fn ensure(
    client: &reqwest::Client,
    runtimes_dir: &Path,
    component: &str,
    progress: impl FnMut(usize, usize),
) -> Result<PathBuf> {
    let runtime_dir = runtimes_dir.join(component);
    let java = java_executable(&runtime_dir);
    let marker = runtime_dir.join(".shard-installed");
    // Уже стоит — не ходим в сеть, чтобы игра запускалась и без интернета
    if marker.exists() && java.exists() {
        return Ok(java);
    }

    let all: HashMap<String, HashMap<String, Vec<RuntimeEntry>>> =
        client.get(RUNTIMES_URL).send().await?.error_for_status()?.json().await?;
    let entry = all
        .get(platform())
        .and_then(|p| p.get(component))
        .and_then(|list| list.first())
        .ok_or_else(|| Error::msg(format!("Mojang has no Java '{component}' for {}", platform())))?;

    let manifest_path = runtimes_dir.join(format!("{component}.json"));
    download_one(
        client,
        &Download {
            url: entry.manifest.url.clone(),
            path: manifest_path.clone(),
            sha1: Some(entry.manifest.sha1.clone()),
            size: Some(entry.manifest.size),
        },
    )
    .await?;
    let manifest: RuntimeManifest = serde_json::from_slice(&tokio::fs::read(&manifest_path).await?)?;

    let mut downloads = vec![];
    let mut executables = vec![];
    for (rel, file) in &manifest.files {
        let path = runtime_dir.join(rel);
        match file {
            RuntimeFile::File { downloads: d, executable } => {
                downloads.push(Download {
                    url: d.raw.url.clone(),
                    path: path.clone(),
                    sha1: Some(d.raw.sha1.clone()),
                    size: Some(d.raw.size),
                });
                if *executable {
                    executables.push(path);
                }
            }
            RuntimeFile::Directory => tokio::fs::create_dir_all(&path).await?,
            RuntimeFile::Link { .. } => {}
        }
    }
    download_all(client, downloads, progress).await?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        for path in &executables {
            tokio::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).await?;
        }
        for (rel, file) in &manifest.files {
            if let RuntimeFile::Link { target } = file {
                let link = runtime_dir.join(rel);
                let _ = tokio::fs::remove_file(&link).await;
                tokio::fs::symlink(target, &link).await?;
            }
        }
    }
    #[cfg(not(unix))]
    let _ = executables;

    if !java.exists() {
        return Err(Error::msg(format!("java not found after install: {}", java.display())));
    }
    tokio::fs::write(&marker, &entry.version.name).await?;
    Ok(java)
}

/// Что известно о конкретной установке Java.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JavaInfo {
    pub path: String,
    pub version: String,
    pub major: u32,
    pub arch: String,
    pub vendor: String,
    /// Java, скачанная самим лаунчером
    pub managed: bool,
}

/// "1.8.0_402" → 8, "17.0.10" → 17, "21" → 21
fn major_of(version: &str) -> u32 {
    let mut parts = version.split(['.', '_', '-', '+']);
    let first: u32 = parts.next().and_then(|p| p.parse().ok()).unwrap_or(0);
    if first == 1 {
        parts.next().and_then(|p| p.parse().ok()).unwrap_or(0)
    } else {
        first
    }
}

/// Версия, архитектура и вендор Java. Сначала из файла `release` в папке Java — так не нужно
/// запускать скрытые процессы (антивирусы такое не любят); иначе — `java -XshowSettings`.
pub async fn probe(path: &Path) -> Result<JavaInfo> {
    if let Some(info) = from_release_file(path) {
        return Ok(info);
    }
    probe_process(path).await
}

/// Читает `<java home>/release`: строки вида `JAVA_VERSION="17.0.8"`.
fn from_release_file(path: &Path) -> Option<JavaInfo> {
    let text = std::fs::read_to_string(path.parent()?.parent()?.join("release")).ok()?;
    let prop = |name: &str| {
        text.lines()
            .find_map(|l| l.strip_prefix(name)?.strip_prefix('=').map(|v| v.trim().trim_matches('"').to_owned()))
            .unwrap_or_default()
    };
    let version = prop("JAVA_VERSION");
    if version.is_empty() {
        return None;
    }
    let arch = match prop("OS_ARCH").as_str() {
        "x86_64" => "amd64".to_owned(),
        other => other.to_owned(),
    };
    Some(JavaInfo {
        path: path.to_string_lossy().into_owned(),
        major: major_of(&version),
        version,
        arch,
        vendor: prop("IMPLEMENTOR"),
        managed: false,
    })
}

async fn probe_process(path: &Path) -> Result<JavaInfo> {
    // javaw ничего не пишет в консоль — для проверки берём соседний java
    let console = path.with_file_name(if cfg!(windows) { "java.exe" } else { "java" });
    let exe = if console.exists() { console } else { path.to_path_buf() };

    let mut command = tokio::process::Command::new(&exe);
    command.args(["-XshowSettings:properties", "-version"]);
    #[cfg(windows)]
    command.creation_flags(0x0800_0000);
    let output = tokio::time::timeout(std::time::Duration::from_secs(10), command.output())
        .await
        .map_err(|_| Error::msg("java_probe_timeout"))?
        .map_err(|_| Error::msg("java_not_runnable"))?;

    let text = String::from_utf8_lossy(&output.stderr);
    let prop = |name: &str| {
        text.lines()
            .find_map(|l| l.trim().strip_prefix(name)?.trim_start().strip_prefix('=').map(|v| v.trim().to_owned()))
            .unwrap_or_default()
    };
    let version = prop("java.version");
    if version.is_empty() {
        return Err(Error::msg("java_not_runnable"));
    }
    Ok(JavaInfo {
        path: path.to_string_lossy().into_owned(),
        major: major_of(&version),
        version,
        arch: prop("os.arch"),
        vendor: prop("java.vendor"),
        managed: false,
    })
}

/// Места, где обычно лежит Java: JAVA_HOME, PATH, стандартные папки вендоров и наши рантаймы.
fn candidates(runtimes_dir: &Path) -> Vec<PathBuf> {
    let exe = if cfg!(windows) { "javaw.exe" } else { "java" };
    let mut out = vec![];
    let mut push_home = |home: PathBuf| out.push(home.join("bin").join(exe));

    if let Ok(home) = std::env::var("JAVA_HOME") {
        push_home(PathBuf::from(home));
    }
    if let Some(path) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&path) {
            // Oracle кладёт в PATH папку-прокладку javapath, которая лишь перенаправляет на настоящую Java
            if dir.ends_with("javapath") {
                continue;
            }
            if dir.join(exe).exists() {
                out.push(dir.join(exe));
            }
        }
    }

    let mut roots: Vec<PathBuf> = vec![];
    if cfg!(windows) {
        for base in [std::env::var("ProgramFiles").ok(), std::env::var("ProgramFiles(x86)").ok()].into_iter().flatten() {
            for vendor in ["Java", "Eclipse Adoptium", "Microsoft", "Zulu", "BellSoft", "Amazon Corretto", "Semeru", "Eclipse Foundation"] {
                roots.push(PathBuf::from(&base).join(vendor));
            }
        }
    } else if cfg!(target_os = "macos") {
        roots.push(PathBuf::from("/Library/Java/JavaVirtualMachines"));
    } else {
        roots.push(PathBuf::from("/usr/lib/jvm"));
    }
    for root in roots {
        let Ok(entries) = std::fs::read_dir(&root) else { continue };
        for entry in entries.flatten() {
            let home = entry.path();
            if cfg!(target_os = "macos") {
                out.push(home.join("Contents/Home/bin/java"));
            } else {
                out.push(home.join("bin").join(exe));
            }
        }
    }

    if let Ok(entries) = std::fs::read_dir(runtimes_dir) {
        for entry in entries.flatten() {
            if entry.path().is_dir() {
                out.push(java_executable(&entry.path()));
            }
        }
    }

    let mut seen = std::collections::HashSet::new();
    out.into_iter()
        .filter(|p| p.exists())
        .filter(|p| seen.insert(std::fs::canonicalize(p).unwrap_or_else(|_| p.clone())))
        .collect()
}

/// Все найденные установки Java, новые версии первыми.
pub async fn detect(runtimes_dir: &Path) -> Vec<JavaInfo> {
    let mut set = tokio::task::JoinSet::new();
    for path in candidates(runtimes_dir) {
        let managed = path.starts_with(runtimes_dir);
        set.spawn(async move { probe(&path).await.map(|info| JavaInfo { managed, ..info }) });
    }
    let mut found = vec![];
    while let Some(result) = set.join_next().await {
        if let Ok(Ok(info)) = result {
            found.push(info);
        }
    }
    found.sort_by(|a, b| b.major.cmp(&a.major).then(a.path.cmp(&b.path)));
    found
}

#[cfg(test)]
mod tests {
    use super::major_of;

    #[test]
    fn majors() {
        assert_eq!(major_of("1.8.0_402"), 8);
        assert_eq!(major_of("17.0.10"), 17);
        assert_eq!(major_of("21"), 21);
        assert_eq!(major_of("25-ea"), 25);
    }
}

#[cfg(test)]
mod detect_tests {
    /// Печатает найденные на этой машине Java; запуск: cargo test detect_local -- --ignored --nocapture
    #[tokio::test]
    #[ignore]
    async fn detect_local() {
        let runtimes = std::path::PathBuf::from(std::env::var("APPDATA").unwrap_or_default()).join("app.shard.launcher/runtimes");
        for java in super::detect(&runtimes).await {
            println!("{:>3} {:<10} {:<8} managed={} {}", java.major, java.version, java.arch, java.managed, java.path);
        }
    }
}
