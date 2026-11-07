//! Forge и NeoForge: качаем официальный установщик и повторяем его клиентскую установку —
//! распаковка библиотек, затем «процессоры» (java-программы, которые патчат клиент).

use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Stdio;

use serde::Deserialize;
use serde_json::Value;
use sha1::{Digest, Sha1};

use super::{is_prerelease, version_key, LoaderVersion};
use crate::download::{download_all, download_one, Download};
use crate::error::{Error, Result};
use crate::game::version::{maven_path, Library, VersionJson};
use crate::game::{java, libraries, Stage};
use crate::instances::ModLoader;
use crate::state::AppState;

const FORGE_MAVEN: &str = "https://maven.minecraftforge.net";
const NEOFORGE_MAVEN: &str = "https://maven.neoforged.net/releases";
const FORGE_PROMOTIONS: &str = "https://files.minecraftforge.net/net/minecraftforge/forge/promotions_slim.json";

struct Coords {
    maven: &'static str,
    group: &'static str,
    artifact: &'static str,
}

fn coords(loader: ModLoader) -> Coords {
    match loader {
        ModLoader::Neoforge => Coords { maven: NEOFORGE_MAVEN, group: "net.neoforged", artifact: "neoforge" },
        _ => Coords { maven: FORGE_MAVEN, group: "net.minecraftforge", artifact: "forge" },
    }
}

/// Ссылка на установщик Forge или NeoForge — им же ставится сервер (`--installServer`).
pub fn installer_url(loader: ModLoader, mc: &str, version: &str) -> String {
    let c = coords(loader);
    let full = maven_version(loader, mc, version);
    let rel = maven_path(&format!("{}:{}:{full}:installer", c.group, c.artifact)).expect("valid coords");
    format!("{}/{rel}", c.maven)
}

/// Полная версия в Maven: у Forge перед версией стоит версия Minecraft, у NeoForge — нет.
fn maven_version(loader: ModLoader, mc: &str, version: &str) -> String {
    match loader {
        ModLoader::Forge => format!("{mc}-{version}"),
        _ => version.to_owned(),
    }
}

/// Версия Minecraft по версии NeoForge: 21.1.77 → 1.21.1, 20.4.x → 1.20.4, 21.0.x → 1.21,
/// а с годовой нумерацией 26.1.0.12 → 26.1 и 26.1.2.5 → 26.1.2.
pub fn neoforge_mc(version: &str) -> Option<String> {
    let core = version.split('-').next()?;
    match core.split('.').collect::<Vec<_>>().as_slice() {
        [a, b, c, _build] => Some(if *c == "0" { format!("{a}.{b}") } else { format!("{a}.{b}.{c}") }),
        [a, b, _build] => Some(if *b == "0" { format!("1.{a}") } else { format!("1.{a}.{b}") }),
        _ => None,
    }
}

fn maven_versions(xml: &str) -> Vec<String> {
    xml.split("<version>")
        .skip(1)
        .filter_map(|s| s.split_once("</version>"))
        .map(|(v, _)| v.trim().to_owned())
        .collect()
}

pub async fn versions(http: &reqwest::Client, loader: ModLoader, mc: &str) -> Result<Vec<LoaderVersion>> {
    let c = coords(loader);
    let url = format!("{}/{}/{}/maven-metadata.xml", c.maven, c.group.replace('.', "/"), c.artifact);
    let xml = http.get(url).send().await?.error_for_status()?.text().await?;

    let mut list: Vec<LoaderVersion> = match loader {
        ModLoader::Forge => {
            let promotions: Value = match http.get(FORGE_PROMOTIONS).send().await {
                Ok(r) => r.json().await.unwrap_or(Value::Null),
                Err(_) => Value::Null,
            };
            let recommended = promotions["promos"][format!("{mc}-recommended")].as_str().map(str::to_owned);
            let prefix = format!("{mc}-");
            maven_versions(&xml)
                .into_iter()
                .filter_map(|v| v.strip_prefix(&prefix).map(str::to_owned))
                .map(|v| LoaderVersion {
                    recommended: recommended.as_deref() == Some(v.as_str()),
                    stable: true,
                    version: v,
                })
                .collect()
        }
        _ => maven_versions(&xml)
            .into_iter()
            .filter(|v| neoforge_mc(v).as_deref() == Some(mc))
            .map(|v| LoaderVersion { stable: !is_prerelease(&v), recommended: false, version: v })
            .collect(),
    };
    list.sort_by(|a, b| version_key(&b.version).cmp(&version_key(&a.version)));
    Ok(list)
}

#[derive(Deserialize)]
struct Processor {
    jar: String,
    #[serde(default)]
    classpath: Vec<String>,
    #[serde(default)]
    args: Vec<String>,
    #[serde(default)]
    outputs: HashMap<String, String>,
    sides: Option<Vec<String>>,
}

/// Что достали из установщика.
struct Unpacked {
    profile: Value,
    version: Value,
}

pub async fn install(
    state: &AppState,
    loader: ModLoader,
    loader_version: &str,
    vanilla: &VersionJson,
    progress: &mut (dyn FnMut(Stage, usize, usize) + Send),
) -> Result<Value> {
    let paths = &state.paths;
    let http = &state.http;
    let mc = vanilla.id.as_str();
    let libraries_dir = paths.libraries();
    let c = coords(loader);
    let full = maven_version(loader, mc, loader_version);

    progress(Stage::Loader, 0, 1);
    let installer_rel = maven_path(&format!("{}:{}:{full}:installer", c.group, c.artifact)).expect("valid coords");
    let installer = libraries_dir.join(&installer_rel);
    download_one(
        http,
        &Download { url: format!("{}/{installer_rel}", c.maven), path: installer.clone(), sha1: None, size: None },
    )
    .await?;

    // Временная папка для файлов данных установщика (бинарные патчи и т. п.)
    let work = paths.root.join("tmp").join(format!("{}-{full}", c.artifact));
    let unpacked = {
        let (installer, libraries_dir, work) = (installer.clone(), libraries_dir.clone(), work.clone());
        tokio::task::spawn_blocking(move || unpack(&installer, &libraries_dir, &work))
            .await
            .map_err(|e| Error::msg(e.to_string()))??
    };

    // Библиотеки и для запуска игры, и для процессоров установщика
    let mut libs: Vec<Library> = vec![];
    for source in [&unpacked.profile["libraries"], &unpacked.version["libraries"]] {
        if let Some(list) = source.as_array() {
            libs.extend(serde_json::from_value::<Vec<Library>>(Value::Array(list.clone()))?);
        }
    }
    let resolved = libraries::resolve(&libs, &libraries_dir);
    download_all(http, resolved.downloads, |d, t| progress(Stage::Libraries, d, t)).await?;

    let processors: Vec<Processor> = match unpacked.profile.get("processors") {
        Some(list) => serde_json::from_value(list.clone())?,
        None => vec![],
    };
    let processors: Vec<Processor> = processors
        .into_iter()
        .filter(|p| p.sides.as_ref().is_none_or(|s| s.iter().any(|side| side == "client")))
        .collect();

    if !processors.is_empty() {
        // Процессорам нужен ванильный клиент и Java той же версии, что и игре
        let client_jar = paths.version_dir(mc).join(format!("{mc}.jar"));
        let cl = &vanilla.downloads.client;
        download_one(
            http,
            &Download { url: cl.url.clone(), path: client_jar.clone(), sha1: cl.sha1.clone(), size: cl.size },
        )
        .await?;
        let component = vanilla.java_version.as_ref().map_or(java::LEGACY_COMPONENT, |j| j.component.as_str());
        let java = java::ensure(http, &paths.runtimes(), component, |d, t| progress(Stage::Java, d, t)).await?;

        let data = data_map(&unpacked.profile, &libraries_dir, &work, &[
            ("SIDE", "client".to_owned()),
            ("MINECRAFT_JAR", path_str(&client_jar)),
            ("MINECRAFT_VERSION", mc.to_owned()),
            ("ROOT", path_str(&work)),
            ("INSTALLER", path_str(&installer)),
            ("LIBRARY_DIR", path_str(&libraries_dir)),
        ]);

        let total = processors.len();
        for (i, processor) in processors.iter().enumerate() {
            progress(Stage::Loader, i, total);
            run_processor(&java, processor, &data, &libraries_dir, &work).await?;
        }
        progress(Stage::Loader, total, total);
    }

    let _ = tokio::fs::remove_dir_all(&work).await;
    Ok(unpacked.version)
}

fn path_str(p: &Path) -> String {
    p.to_string_lossy().into_owned()
}

fn read_entry(archive: &mut zip::ZipArchive<std::fs::File>, name: &str) -> Result<Vec<u8>> {
    let mut entry = archive
        .by_name(name)
        .map_err(|_| Error::msg(format!("loader_install_failed: {name} missing in installer")))?;
    let mut bytes = Vec::with_capacity((entry.size() as usize).min(1 << 24));
    entry.read_to_end(&mut bytes)?;
    Ok(bytes)
}

fn write_file(path: &Path, bytes: &[u8]) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, bytes)?;
    Ok(())
}

fn unpack(installer: &Path, libraries_dir: &Path, work: &Path) -> Result<Unpacked> {
    let mut archive = zip::ZipArchive::new(std::fs::File::open(installer)?)?;
    let profile: Value = serde_json::from_slice(&read_entry(&mut archive, "install_profile.json")?)?;

    // Старый формат (Forge до 1.12): профиль внутри install_profile, библиотека Forge лежит в корне
    if let Some(version) = profile.get("versionInfo") {
        let install = &profile["install"];
        if let (Some(file), Some(coords)) = (install["filePath"].as_str(), install["path"].as_str()) {
            let target = libraries_dir.join(maven_path(coords).ok_or_else(|| Error::msg("bad forge coords"))?);
            write_file(&target, &read_entry(&mut archive, file)?)?;
        }
        let mut version = version.clone();
        // Эта библиотека уже распакована, качать её из Maven не нужно (там лежит другой файл)
        if let Some(libs) = version["libraries"].as_array_mut() {
            for lib in libs.iter_mut() {
                if lib["name"].as_str() == install["path"].as_str() {
                    lib["downloads"] = serde_json::json!({ "artifact": { "url": "" } });
                }
            }
        }
        return Ok(Unpacked { profile: Value::Null, version });
    }

    let json_name = profile["json"].as_str().unwrap_or("/version.json").trim_start_matches('/').to_owned();
    let version: Value = serde_json::from_slice(&read_entry(&mut archive, &json_name)?)?;

    // Всё из maven/ — готовые библиотеки, которых нет в публичных репозиториях
    let data_files: Vec<String> = profile["data"]
        .as_object()
        .map(|d| {
            d.values()
                .filter_map(|v| v["client"].as_str())
                .filter(|v| v.starts_with('/'))
                .map(|v| v.trim_start_matches('/').to_owned())
                .collect()
        })
        .unwrap_or_default();
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i)?;
        if entry.is_dir() {
            continue;
        }
        let name = entry.name().to_owned();
        // Пути из архива — только внутрь своих папок
        let target = if let Some(rel) = name.strip_prefix("maven/") {
            crate::modpack::safe_join(libraries_dir, rel)?
        } else if data_files.contains(&name) {
            crate::modpack::safe_join(work, &name)?
        } else {
            continue;
        };
        if target.metadata().is_ok_and(|m| m.len() == entry.size()) {
            continue;
        }
        let mut bytes = Vec::with_capacity((entry.size() as usize).min(1 << 24));
        entry.read_to_end(&mut bytes)?;
        write_file(&target, &bytes)?;
    }
    Ok(Unpacked { profile, version })
}

/// Значения из блока data установщика для клиента: [maven-координаты], 'литерал' или /файл из установщика.
fn data_map(profile: &Value, libraries_dir: &Path, work: &Path, builtins: &[(&str, String)]) -> HashMap<String, String> {
    let mut out: HashMap<String, String> = builtins.iter().map(|(k, v)| ((*k).to_owned(), v.clone())).collect();
    if let Some(data) = profile["data"].as_object() {
        for (key, value) in data {
            let Some(raw) = value["client"].as_str() else { continue };
            let resolved = if let Some(coords) = raw.strip_prefix('[').and_then(|r| r.strip_suffix(']')) {
                maven_path(coords).map_or_else(|| raw.to_owned(), |rel| path_str(&libraries_dir.join(rel)))
            } else if let Some(literal) = raw.strip_prefix('\'').and_then(|r| r.strip_suffix('\'')) {
                literal.to_owned()
            } else if raw.starts_with('/') {
                path_str(&work.join(raw.trim_start_matches('/')))
            } else {
                raw.to_owned()
            };
            out.insert(key.clone(), resolved);
        }
    }
    out
}

/// Заменяет {КЛЮЧ} на значения из data; целый аргумент [координаты] — на путь к библиотеке.
fn substitute(arg: &str, data: &HashMap<String, String>, libraries_dir: &Path) -> String {
    if let Some(coords) = arg.strip_prefix('[').and_then(|r| r.strip_suffix(']')) {
        if let Some(rel) = maven_path(coords) {
            return path_str(&libraries_dir.join(rel));
        }
    }
    let mut out = String::with_capacity(arg.len());
    let mut rest = arg;
    while let Some(start) = rest.find('{') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        match after.find('}') {
            Some(end) if data.contains_key(&after[..end]) => {
                out.push_str(&data[&after[..end]]);
                rest = &after[end + 1..];
            }
            _ => {
                out.push('{');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

fn sha1_of(path: &Path) -> Option<String> {
    let bytes = std::fs::read(path).ok()?;
    Some(hex::encode(Sha1::digest(&bytes)))
}

fn main_class(jar: &Path) -> Result<String> {
    let mut archive = zip::ZipArchive::new(std::fs::File::open(jar)?)?;
    let manifest = String::from_utf8_lossy(&read_entry(&mut archive, "META-INF/MANIFEST.MF")?).into_owned();
    manifest
        .lines()
        .find_map(|l| l.strip_prefix("Main-Class:"))
        .map(|c| c.trim().to_owned())
        .ok_or_else(|| Error::msg(format!("loader_install_failed: no Main-Class in {}", jar.display())))
}

async fn run_processor(
    java: &Path,
    processor: &Processor,
    data: &HashMap<String, String>,
    libraries_dir: &Path,
    work: &Path,
) -> Result<()> {
    let lib = |coords: &str| -> Result<PathBuf> {
        let rel = maven_path(coords).ok_or_else(|| Error::msg(format!("bad coords {coords}")))?;
        Ok(libraries_dir.join(rel))
    };

    // Если все выходные файлы уже есть с нужными хешами — процессор уже отработал раньше
    let outputs: Vec<(PathBuf, String)> = processor
        .outputs
        .iter()
        .map(|(file, sha)| {
            (PathBuf::from(substitute(file, data, libraries_dir)), substitute(sha, data, libraries_dir))
        })
        .collect();
    let outputs_ok = |outputs: &[(PathBuf, String)]| {
        outputs.iter().all(|(file, sha)| sha1_of(file).is_some_and(|actual| actual.eq_ignore_ascii_case(sha)))
    };
    if !outputs.is_empty() && outputs_ok(&outputs) {
        return Ok(());
    }

    let jar = lib(&processor.jar)?;
    let main = {
        let jar = jar.clone();
        tokio::task::spawn_blocking(move || main_class(&jar)).await.map_err(|e| Error::msg(e.to_string()))??
    };
    let separator = if cfg!(windows) { ";" } else { ":" };
    let mut classpath = vec![path_str(&jar)];
    for coords in &processor.classpath {
        classpath.push(path_str(&lib(coords)?));
    }
    let args: Vec<String> = processor.args.iter().map(|a| substitute(a, data, libraries_dir)).collect();

    tokio::fs::create_dir_all(work).await?;
    let mut command = tokio::process::Command::new(java);
    command
        .arg("-cp")
        .arg(classpath.join(separator))
        .arg(&main)
        .args(&args)
        .current_dir(work)
        .stdin(Stdio::null());
    #[cfg(windows)]
    if !java.file_name().is_some_and(|n| n.eq_ignore_ascii_case("javaw.exe")) {
        command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    let output = command.output().await?;
    if !output.status.success() {
        let text = format!("{}{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
        let tail: Vec<&str> = text.lines().rev().take(12).collect();
        let tail: Vec<&str> = tail.into_iter().rev().collect();
        return Err(Error::msg(format!(
            "loader_install_failed: {} exited with {}\n{}",
            processor.jar,
            output.status.code().unwrap_or(-1),
            tail.join("\n")
        )));
    }
    if !outputs.is_empty() && !outputs_ok(&outputs) {
        return Err(Error::msg(format!("loader_install_failed: {} produced files with wrong checksums", processor.jar)));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn neoforge_versions_map_to_minecraft() {
        assert_eq!(neoforge_mc("21.1.77").as_deref(), Some("1.21.1"));
        assert_eq!(neoforge_mc("21.0.143").as_deref(), Some("1.21"));
        assert_eq!(neoforge_mc("20.2.3-beta").as_deref(), Some("1.20.2"));
        assert_eq!(neoforge_mc("26.3.0.101").as_deref(), Some("26.3"));
        assert_eq!(neoforge_mc("26.1.2.125").as_deref(), Some("26.1.2"));
    }

    #[test]
    fn substitutes_tokens() {
        let data = HashMap::from([("ROOT".to_owned(), "C:/work".to_owned())]);
        let libs = Path::new("L");
        assert_eq!(substitute("{ROOT}/libraries/", &data, libs), "C:/work/libraries/");
        assert_eq!(substitute("{UNKNOWN}", &data, libs), "{UNKNOWN}");
        assert!(substitute("[a.b:c:1]", &data, libs).ends_with("c-1.jar"));
    }
}
