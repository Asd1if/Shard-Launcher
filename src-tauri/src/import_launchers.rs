//! Импорт сборок из других лаунчеров: Prism Launcher, MultiMC, CurseForge App, Modrinth App,
//! официальный лаунчер и TLauncher (папка .minecraft). Берём версию игры и загрузчик, а моды,
//! миры, конфиги и настройки копируем в новую сборку Shard.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::State;

use crate::error::{Error, Result};
use crate::instances::{self, Instance, InstanceSettings, ModLoader};
use crate::state::AppState;
use crate::versions::VersionType;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Found {
    /// prism, multimc, curseforge, modrinth, minecraft
    pub launcher: String,
    pub name: String,
    /// Папка игры (где mods, saves, config)
    pub path: String,
    pub mc_version: String,
    pub loader: Option<ModLoader>,
    pub loader_version: Option<String>,
    pub mods: u32,
}

/// Что не копируем: это скачает сам Shard или это мусор.
const SKIP: [&str; 14] = [
    "versions", "libraries", "assets", "logs", "crash-reports", "natives", ".fabric", ".mixin.out", "webcache", "cache", "backups",
    "runtime", "launcher_profiles.json", "TlauncherProfiles.json",
];

fn count_mods(game: &Path) -> u32 {
    std::fs::read_dir(game.join("mods"))
        .map(|e| e.flatten().filter(|e| e.file_name().to_string_lossy().ends_with(".jar")).count() as u32)
        .unwrap_or(0)
}

fn read_json(path: &Path) -> Option<Value> {
    serde_json::from_slice(&std::fs::read(path).ok()?).ok()
}

// ---------- Prism / MultiMC ----------

fn mmc_instance(dir: &Path, launcher: &str) -> Option<Found> {
    let pack = read_json(&dir.join("mmc-pack.json"))?;
    let cfg = std::fs::read_to_string(dir.join("instance.cfg")).unwrap_or_default();
    let name = cfg.lines().find_map(|l| l.strip_prefix("name=")).map(str::to_owned).unwrap_or_else(|| dir.file_name().unwrap_or_default().to_string_lossy().into_owned());
    let mut mc = None;
    let mut loader = None;
    let mut loader_version = None;
    for c in pack.get("components")?.as_array()? {
        let uid = c.get("uid").and_then(Value::as_str).unwrap_or_default();
        let version = c.get("version").and_then(Value::as_str).map(str::to_owned);
        let kind = match uid {
            "net.minecraft" => {
                mc = version;
                continue;
            }
            "net.fabricmc.fabric-loader" => ModLoader::Fabric,
            "org.quiltmc.quilt-loader" => ModLoader::Quilt,
            "net.minecraftforge" => ModLoader::Forge,
            "net.neoforged" => ModLoader::Neoforge,
            _ => continue,
        };
        loader = Some(kind);
        loader_version = version;
    }
    let game = [".minecraft", "minecraft"].iter().map(|d| dir.join(d)).find(|p| p.is_dir())?;
    Some(Found {
        launcher: launcher.to_owned(),
        name,
        mods: count_mods(&game),
        path: game.to_string_lossy().into_owned(),
        mc_version: mc?,
        loader,
        loader_version,
    })
}

// ---------- CurseForge App ----------

fn curseforge_instance(dir: &Path) -> Option<Found> {
    let info = read_json(&dir.join("minecraftinstance.json"))?;
    let mc = info.get("gameVersion")?.as_str()?.to_owned();
    let base = info.get("baseModLoader").and_then(|b| b.get("name")).and_then(Value::as_str).unwrap_or_default();
    // forge-47.2.0, neoforge-20.4.80, fabric-0.15.7-1.20.1, quilt-0.24.0-1.20.1
    let (loader, loader_version) = match base.split_once('-') {
        Some((kind, rest)) => {
            let loader = match kind {
                "forge" => Some(ModLoader::Forge),
                "neoforge" => Some(ModLoader::Neoforge),
                "fabric" => Some(ModLoader::Fabric),
                "quilt" => Some(ModLoader::Quilt),
                _ => None,
            };
            let version = match loader {
                Some(ModLoader::Fabric | ModLoader::Quilt) => rest.strip_suffix(&format!("-{mc}")).unwrap_or(rest),
                _ => rest,
            };
            (loader, loader.map(|_| version.to_owned()))
        }
        None => (None, None),
    };
    Some(Found {
        launcher: "curseforge".to_owned(),
        name: info.get("name").and_then(Value::as_str).map_or_else(|| dir.file_name().unwrap_or_default().to_string_lossy().into_owned(), str::to_owned),
        mods: count_mods(dir),
        path: dir.to_string_lossy().into_owned(),
        mc_version: mc,
        loader,
        loader_version,
    })
}

// ---------- Modrinth App ----------

fn modrinth_instance(dir: &Path) -> Option<Found> {
    // Старые версии приложения хранили profile.json рядом с файлами
    let info = read_json(&dir.join("profile.json"))?;
    let meta = info.get("metadata").unwrap_or(&info);
    let mc = meta.get("game_version")?.as_str()?.to_owned();
    let loader = match meta.get("loader").and_then(Value::as_str) {
        Some("fabric") => Some(ModLoader::Fabric),
        Some("quilt") => Some(ModLoader::Quilt),
        Some("forge") => Some(ModLoader::Forge),
        Some("neoforge") => Some(ModLoader::Neoforge),
        _ => None,
    };
    let loader_version = meta.get("loader_version").and_then(|v| v.get("id").or(Some(v))).and_then(Value::as_str).map(str::to_owned);
    Some(Found {
        launcher: "modrinth".to_owned(),
        name: meta.get("name").and_then(Value::as_str).map_or_else(|| dir.file_name().unwrap_or_default().to_string_lossy().into_owned(), str::to_owned),
        mods: count_mods(dir),
        path: dir.to_string_lossy().into_owned(),
        mc_version: mc,
        loader,
        loader_version: loader.and(loader_version),
    })
}

// ---------- .minecraft (официальный лаунчер, TLauncher) ----------

/// Каждая установленная версия из versions — отдельный вариант импорта с общей папкой игры.
fn dot_minecraft(game: &Path) -> Vec<Found> {
    let Ok(entries) = std::fs::read_dir(game.join("versions")) else { return vec![] };
    let mods = count_mods(game);
    let mut out = vec![];
    for e in entries.flatten() {
        let id = e.file_name().to_string_lossy().into_owned();
        let Some(json) = read_json(&e.path().join(format!("{id}.json"))) else { continue };
        let mc = json.get("inheritsFrom").and_then(Value::as_str).unwrap_or(&id).to_owned();
        let libraries: Vec<&str> = json.get("libraries").and_then(Value::as_array).map(|l| l.iter().filter_map(|x| x.get("name")?.as_str()).collect()).unwrap_or_default();
        let lib = |prefix: &str| libraries.iter().find_map(|n| n.strip_prefix(prefix)).map(|v| v.split(':').next().unwrap_or(v).to_owned());
        let lower = id.to_ascii_lowercase();
        let (loader, version) = if let Some(v) = lib("net.fabricmc:fabric-loader:") {
            (Some(ModLoader::Fabric), Some(v))
        } else if let Some(v) = lib("org.quiltmc:quilt-loader:") {
            (Some(ModLoader::Quilt), Some(v))
        } else if lower.contains("neoforge") {
            (Some(ModLoader::Neoforge), lib("net.neoforged:neoforge:"))
        } else if lower.contains("forge") {
            // 1.20.1-forge-47.2.0
            (Some(ModLoader::Forge), lower.split("forge-").nth(1).map(str::to_owned).or_else(|| lib("net.minecraftforge:forge:").map(|v| v.rsplit('-').next().unwrap_or(&v).to_owned())))
        } else if lower.contains("optifine") {
            continue; // OptiFine — не загрузчик; сам мод можно поставить из каталога
        } else {
            (None, None)
        };
        if !mc.chars().next().is_some_and(|c| c.is_ascii_digit()) {
            continue;
        }
        out.push(Found {
            launcher: "minecraft".to_owned(),
            name: id.clone(),
            path: game.to_string_lossy().into_owned(),
            mc_version: mc,
            loader,
            loader_version: version,
            mods: if loader.is_some() { mods } else { 0 },
        });
    }
    out.sort_by(|a, b| b.name.cmp(&a.name));
    out
}

fn subdirs(dir: &Path) -> Vec<PathBuf> {
    std::fs::read_dir(dir).map(|e| e.flatten().map(|e| e.path()).filter(|p| p.is_dir()).collect()).unwrap_or_default()
}

/// Всё, что удалось распознать в папке: экземпляр, папка экземпляров или .minecraft.
fn scan_dir(dir: &Path) -> Vec<Found> {
    if let Some(f) = mmc_instance(dir, "prism").or_else(|| curseforge_instance(dir)).or_else(|| modrinth_instance(dir)) {
        return vec![f];
    }
    if dir.join("versions").is_dir() {
        return dot_minecraft(dir);
    }
    // Папка с экземплярами
    let inner = if dir.join("instances").is_dir() { dir.join("instances") } else { dir.to_path_buf() };
    subdirs(&inner).iter().filter_map(|d| mmc_instance(d, "prism").or_else(|| curseforge_instance(d)).or_else(|| modrinth_instance(d))).collect()
}

#[tauri::command]
pub async fn launcher_scan(path: Option<String>) -> Result<Vec<Found>> {
    tokio::task::spawn_blocking(move || {
        if let Some(p) = path {
            return scan_dir(Path::new(&p));
        }
        let appdata = std::env::var_os("APPDATA").map(PathBuf::from);
        let home = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME")).map(PathBuf::from);
        let mut out = vec![];
        if let Some(a) = &appdata {
            for d in subdirs(&a.join("PrismLauncher").join("instances")) {
                out.extend(mmc_instance(&d, "prism"));
            }
            for d in subdirs(&a.join("MultiMC").join("instances")) {
                out.extend(mmc_instance(&d, "multimc"));
            }
            for d in subdirs(&a.join("com.modrinth.theseus").join("profiles")).into_iter().chain(subdirs(&a.join("ModrinthApp").join("profiles"))) {
                out.extend(modrinth_instance(&d));
            }
            out.extend(dot_minecraft(&a.join(".minecraft")));
        }
        if let Some(h) = &home {
            for d in subdirs(&h.join("curseforge").join("minecraft").join("Instances")) {
                out.extend(curseforge_instance(&d));
            }
        }
        out
    })
    .await
    .map_err(|e| Error::msg(e.to_string()))
}

fn copy_game(from: &Path, to: &Path, tracker: &mut crate::progress::Tracker, top: bool) -> Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let name = entry.file_name();
        let text = name.to_string_lossy();
        if top && SKIP.contains(&text.as_ref()) {
            continue;
        }
        let target = to.join(&name);
        let kind = entry.file_type()?;
        if kind.is_dir() {
            copy_game(&entry.path(), &target, tracker, false)?;
        } else if kind.is_file() && name != "session.lock" {
            tracker.add(std::fs::copy(entry.path(), target)?);
        }
    }
    Ok(())
}

fn size_without_skipped(dir: &Path) -> u64 {
    std::fs::read_dir(dir)
        .map(|e| {
            e.flatten()
                .filter(|e| !SKIP.contains(&e.file_name().to_string_lossy().as_ref()))
                .map(|e| if e.path().is_dir() { crate::progress::size_of(&e.path()) } else { e.metadata().map_or(0, |m| m.len()) })
                .sum()
        })
        .unwrap_or(0)
}

/// Создаёт сборку Shard и копирует в неё папку игры.
#[tauri::command]
pub async fn launcher_import(state: State<'_, AppState>, found: Found, progress: tauri::ipc::Channel<crate::progress::Progress>) -> Result<Instance> {
    let source = PathBuf::from(&found.path);
    if !source.is_dir() {
        return Err(Error::msg("file_not_found"));
    }
    let name = found.name.trim();
    let instance = Instance {
        id: uuid::Uuid::new_v4().to_string(),
        name: if name.is_empty() { found.mc_version.clone() } else { name.to_owned() },
        mc_version: found.mc_version.clone(),
        version_type: VersionType::Release,
        loader: found.loader,
        loader_version: found.loader.and(found.loader_version.clone()),
        mod_count: 0,
        last_played: None,
        created_at: chrono::Utc::now().to_rfc3339(),
        memory_mb: crate::settings::new_instance_memory(&state.paths, found.loader.is_some()),
        settings: InstanceSettings::default(),
        icon: None,
        group: None,
        favorite: false,
        pack: None,
    };
    state.paths.new_instance_dir(&instance.id, &instance.name);
    instances::save(&state.paths, &instance)?;
    let game = state.paths.game_dir(&instance.id)?;
    // У ванильной версии из .minecraft моды не нужны — их загрузчика нет
    let skip_mods = found.launcher == "minecraft" && found.loader.is_none();
    let result = tokio::task::spawn_blocking(move || -> Result<()> {
        let mut tracker = crate::progress::Tracker::new(progress, size_without_skipped(&source));
        copy_game(&source, &game, &mut tracker, true)?;
        if skip_mods {
            let _ = std::fs::remove_dir_all(game.join("mods"));
        }
        Ok(())
    })
    .await
    .map_err(|e| Error::msg(e.to_string()))?;
    if let Err(e) = result {
        let _ = std::fs::remove_dir_all(state.paths.instance_dir(&instance.id)?);
        return Err(e);
    }
    Ok(instance)
}
