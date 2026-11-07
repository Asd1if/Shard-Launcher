//! Подготовка ванильной игры к запуску: файлы версии, библиотеки, ассеты, Java и аргументы.

pub mod args;
pub mod assets;
pub mod java;
pub mod libraries;
pub mod version;

use std::collections::HashMap;
use std::path::PathBuf;

use serde::Serialize;

use crate::download::{download_one, Download};
use crate::error::Result;
use crate::paths::Paths;

use version::VersionJson;

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Stage {
    Version,
    Libraries,
    Assets,
    Java,
    /// Установка загрузчика модов (у Forge и NeoForge — с патчингом клиента)
    Loader,
}

pub struct Player {
    pub name: String,
    pub uuid: String,
    pub access_token: String,
    pub user_type: &'static str,
}

pub struct LaunchPlan {
    pub java: PathBuf,
    pub args: Vec<String>,
    pub game_dir: PathBuf,
}

pub fn read_version_json(paths: &Paths, id: &str) -> Result<Option<VersionJson>> {
    let file = paths.version_dir(id).join(format!("{id}.json"));
    match std::fs::read(&file) {
        Ok(bytes) => Ok(Some(serde_json::from_slice(&bytes)?)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.into()),
    }
}

pub async fn prepare(
    client: &reqwest::Client,
    paths: &Paths,
    version: &VersionJson,
    game_dir: PathBuf,
    options: &args::LaunchOptions,
    java_override: Option<PathBuf>,
    player: &Player,
    mut progress: impl FnMut(Stage, usize, usize),
) -> Result<LaunchPlan> {
    tokio::fs::create_dir_all(&game_dir).await?;
    let version_dir = paths.version_dir(&version.id);

    // Клиент
    progress(Stage::Version, 0, 1);
    let client_jar = version_dir.join(format!("{}.jar", version.id));
    let c = &version.downloads.client;
    download_one(
        client,
        &Download { url: c.url.clone(), path: client_jar.clone(), sha1: c.sha1.clone(), size: c.size },
    )
    .await?;
    progress(Stage::Version, 1, 1);

    // Библиотеки и нативные файлы
    let libs = libraries::resolve(&version.libraries, &paths.libraries());
    crate::download::download_all(client, libs.downloads, |d, t| progress(Stage::Libraries, d, t)).await?;
    let natives_dir = version_dir.join("natives");
    let native_jars = libs.native_jars.clone();
    let natives_target = natives_dir.clone();
    tokio::task::spawn_blocking(move || libraries::extract_natives(&native_jars, &natives_target))
        .await
        .map_err(|e| crate::error::Error::msg(e.to_string()))??;

    // Ассеты
    let game_assets =
        assets::ensure(client, &paths.assets(), &game_dir, &version.asset_index, |d, t| {
            progress(Stage::Assets, d, t)
        })
        .await?;

    // Java: своя из настроек сборки или автоматическая от Mojang
    let java = match java_override {
        Some(path) => {
            let info = java::probe(&path).await?;
            let required = version.java_version.as_ref().map_or(8, |j| j.major_version);
            if info.major < required {
                return Err(crate::error::Error::msg(format!("java_too_old:{}:{required}", info.major)));
            }
            path
        }
        None => {
            let component = version
                .java_version
                .as_ref()
                .map(|j| j.component.as_str())
                .unwrap_or(java::LEGACY_COMPONENT);
            java::ensure(client, &paths.runtimes(), component, |d, t| progress(Stage::Java, d, t)).await?
        }
    };

    let separator = if cfg!(windows) { ";" } else { ":" };
    let mut classpath: Vec<String> = libs.classpath.iter().map(|p| p.to_string_lossy().into_owned()).collect();
    classpath.push(client_jar.to_string_lossy().into_owned());

    let s = |p: &PathBuf| p.to_string_lossy().into_owned();
    let vars: HashMap<&'static str, String> = HashMap::from([
        ("auth_player_name", player.name.clone()),
        ("auth_uuid", player.uuid.clone()),
        ("auth_access_token", player.access_token.clone()),
        ("auth_session", player.access_token.clone()),
        ("auth_xuid", "0".to_owned()),
        ("clientid", String::new()),
        ("user_type", player.user_type.to_owned()),
        ("user_properties", "{}".to_owned()),
        ("version_name", version.id.clone()),
        ("version_type", version.kind.clone()),
        ("game_directory", s(&game_dir)),
        ("assets_root", s(&paths.assets())),
        ("game_assets", s(&game_assets)),
        ("assets_index_name", version.asset_index.id.clone()),
        ("natives_directory", s(&natives_dir)),
        ("library_directory", s(&paths.libraries())),
        ("classpath_separator", separator.to_owned()),
        ("classpath", classpath.join(separator)),
        ("launcher_name", "Shard".to_owned()),
        ("launcher_version", env!("CARGO_PKG_VERSION").to_owned()),
    ]);

    let args = args::build(&args::ArgsInput { version, vars, options });
    Ok(LaunchPlan { java, args, game_dir })
}
