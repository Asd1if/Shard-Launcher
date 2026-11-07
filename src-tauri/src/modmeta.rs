//! Что мод о себе пишет в jar: id, название, какие моды ему нужны и какие он несёт внутри.
//! Fabric — `fabric.mod.json`, Quilt — `quilt.mod.json`, Forge и NeoForge — `mods.toml`.

use std::collections::BTreeSet;
use std::io::{Read, Seek};
use std::path::Path;

use serde_json::Value;

#[derive(Debug, Default, Clone)]
pub struct ModMeta {
    pub id: String,
    pub name: String,
    /// Обязательные зависимости (id модов)
    pub depends: Vec<String>,
    /// Что мод даёт: свой id, `provides` и моды, вложенные в jar
    pub provides: BTreeSet<String>,
}

/// Не моды, а сама игра, загрузчик и Java — их ставить не нужно.
pub const PLATFORM: &[&str] = &[
    "minecraft",
    "java",
    "fabricloader",
    "fabric-loader",
    "quilt_loader",
    "quilt_base",
    "forge",
    "neoforge",
    "fml",
    "javafml",
];

pub fn read(jar: &Path) -> Option<ModMeta> {
    let file = std::fs::File::open(jar).ok()?;
    read_zip(file, 0)
}

fn read_zip<R: Read + Seek>(reader: R, depth: u8) -> Option<ModMeta> {
    let mut zip = zip::ZipArchive::new(reader).ok()?;
    let mut text = |name: &str| -> Option<String> {
        let mut entry = zip.by_name(name).ok()?;
        let mut out = String::new();
        entry.read_to_string(&mut out).ok()?;
        Some(out)
    };
    let mut meta = if let Some(json) = text("fabric.mod.json").and_then(|t| parse_json(&t)) {
        fabric(&json)
    } else if let Some(json) = text("quilt.mod.json").and_then(|t| parse_json(&t)) {
        quilt(&json)
    } else if let Some(toml) = text("META-INF/neoforge.mods.toml").or_else(|| text("META-INF/mods.toml")) {
        forge(&toml)?
    } else {
        return None;
    };
    // Вложенные моды (jar-in-jar): у Fabric API так лежат все его модули
    if depth == 0 {
        let nested: Vec<String> = (0..zip.len())
            .filter_map(|i| zip.by_index(i).ok().map(|e| e.name().to_owned()))
            .filter(|n| n.starts_with("META-INF/jars/") && n.ends_with(".jar"))
            .collect();
        for name in nested {
            let Ok(mut entry) = zip.by_name(&name) else { continue };
            let mut bytes = Vec::with_capacity(entry.size() as usize);
            if entry.read_to_end(&mut bytes).is_err() {
                continue;
            }
            if let Some(inner) = read_zip(std::io::Cursor::new(bytes), 1) {
                meta.provides.extend(inner.provides);
            }
        }
    }
    meta.provides.insert(meta.id.clone());
    Some(meta)
}

/// В fabric.mod.json встречаются переводы строк прямо в строках — serde их не прощает.
fn parse_json(text: &str) -> Option<Value> {
    serde_json::from_str(text).ok().or_else(|| serde_json::from_str(&text.replace(['\n', '\r', '\t'], " ")).ok())
}

fn fabric(json: &Value) -> ModMeta {
    let id = json["id"].as_str().unwrap_or_default().to_owned();
    ModMeta {
        name: json["name"].as_str().unwrap_or(&id).to_owned(),
        depends: json["depends"].as_object().map(|o| o.keys().cloned().collect()).unwrap_or_default(),
        provides: json["provides"].as_array().into_iter().flatten().filter_map(|v| v.as_str().map(str::to_owned)).collect(),
        id,
    }
}

fn quilt(json: &Value) -> ModMeta {
    let loader = &json["quilt_loader"];
    let id = loader["id"].as_str().unwrap_or_default().to_owned();
    // depends: ["id", {"id": "x", "optional": true}]
    let depends = loader["depends"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|d| match d {
            Value::String(s) => Some(s.clone()),
            Value::Object(o) if !o.get("optional").and_then(Value::as_bool).unwrap_or(false) => o.get("id")?.as_str().map(str::to_owned),
            _ => None,
        })
        .collect();
    let provides = loader["provides"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|p| p.as_str().or_else(|| p["id"].as_str()).map(str::to_owned))
        .collect();
    ModMeta { name: loader["metadata"]["name"].as_str().unwrap_or(&id).to_owned(), id, depends, provides }
}

fn forge(text: &str) -> Option<ModMeta> {
    let doc: toml::Table = text.parse().ok()?;
    let first = doc.get("mods")?.as_array()?.first()?;
    let id = first.get("modId")?.as_str()?.to_owned();
    let name = first.get("displayName").and_then(|v| v.as_str()).unwrap_or(&id).to_owned();
    let provides = doc["mods"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|m| m.get("modId")?.as_str().map(str::to_owned))
        .collect();
    let mut depends = vec![];
    if let Some(deps) = doc.get("dependencies").and_then(|d| d.as_table()) {
        for list in deps.values().filter_map(|v| v.as_array()) {
            for dep in list {
                // Forge: mandatory=true; NeoForge: type="required"
                let required = dep.get("mandatory").and_then(|v| v.as_bool()).unwrap_or(false)
                    || dep.get("type").and_then(|v| v.as_str()).is_some_and(|t| t.eq_ignore_ascii_case("required"));
                // Зависимость только для сервера к клиенту отношения не имеет
                let side_ok = dep.get("side").and_then(|v| v.as_str()).is_none_or(|s| !s.eq_ignore_ascii_case("server"));
                if let (true, true, Some(dep_id)) = (required, side_ok, dep.get("modId").and_then(|v| v.as_str())) {
                    depends.push(dep_id.to_owned());
                }
            }
        }
    }
    Some(ModMeta { id, name, depends, provides })
}

/// Обязательные зависимости, которых нет ни у одного включённого мода. id → кому нужен.
pub fn missing(mods: &[ModMeta]) -> Vec<(String, Vec<String>)> {
    let provided: BTreeSet<&str> = mods.iter().flat_map(|m| m.provides.iter().map(String::as_str)).collect();
    let mut out: Vec<(String, Vec<String>)> = vec![];
    for m in mods {
        for dep in &m.depends {
            if PLATFORM.contains(&dep.as_str()) || provided.contains(dep.as_str()) {
                continue;
            }
            match out.iter_mut().find(|(id, _)| id == dep) {
                Some((_, by)) => by.push(m.name.clone()),
                None => out.push((dep.clone(), vec![m.name.clone()])),
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_fabric_and_forge_metadata() {
        let f = fabric(&serde_json::json!({
            "id": "sodium-extra", "name": "Sodium Extra",
            "depends": {"minecraft": "1.21", "sodium": "*", "fabric-api": "*"},
            "provides": ["sodiumextra"]
        }));
        assert_eq!(f.id, "sodium-extra");
        assert!(f.depends.contains(&"sodium".to_owned()));
        assert!(f.provides.contains("sodiumextra"));

        let toml = r#"
            [[mods]]
            modId = "mowziesmobs"
            displayName = "Mowzie's Mobs"
            [[dependencies.mowziesmobs]]
            modId = "geckolib"
            mandatory = true
            [[dependencies.mowziesmobs]]
            modId = "jei"
            type = "optional"
            [[dependencies.mowziesmobs]]
            modId = "forge"
            mandatory = true
        "#;
        let m = forge(toml).unwrap();
        assert_eq!(m.name, "Mowzie's Mobs");
        assert_eq!(m.depends, vec!["geckolib", "forge"]);

        let mut sodium = ModMeta { id: "sodium".into(), name: "Sodium".into(), ..Default::default() };
        sodium.provides.insert("sodium".into());
        let mut extra = f;
        extra.provides.insert(extra.id.clone());
        let missing = missing(&[extra, sodium]);
        assert_eq!(missing, vec![("fabric-api".to_owned(), vec!["Sodium Extra".to_owned()])]);
    }
}
