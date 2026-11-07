//! Формат version JSON от Mojang и правила (rules) для ОС и функций.

use std::collections::HashMap;

use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VersionJson {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub main_class: String,
    /// Новый формат аргументов (1.13+)
    pub arguments: Option<Arguments>,
    /// Старый формат: одна строка игровых аргументов
    pub minecraft_arguments: Option<String>,
    pub asset_index: AssetIndexRef,
    pub downloads: VersionDownloads,
    pub libraries: Vec<Library>,
    pub java_version: Option<JavaVersion>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Arguments {
    #[serde(default)]
    pub game: Vec<Argument>,
    #[serde(default)]
    pub jvm: Vec<Argument>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum Argument {
    Plain(String),
    Ruled { rules: Vec<Rule>, value: OneOrMany },
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum OneOrMany {
    One(String),
    Many(Vec<String>),
}

impl OneOrMany {
    pub fn into_vec(self) -> Vec<String> {
        match self {
            OneOrMany::One(s) => vec![s],
            OneOrMany::Many(v) => v,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct Rule {
    pub action: RuleAction,
    pub os: Option<OsRule>,
    pub features: Option<HashMap<String, bool>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RuleAction {
    Allow,
    Disallow,
}

#[derive(Debug, Clone, Deserialize)]
pub struct OsRule {
    pub name: Option<String>,
    pub arch: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetIndexRef {
    pub id: String,
    pub sha1: String,
    pub size: u64,
    pub url: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct VersionDownloads {
    pub client: Artifact,
    /// У самых старых версий серверного jar нет
    pub server: Option<Artifact>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Artifact {
    pub path: Option<String>,
    pub sha1: Option<String>,
    pub size: Option<u64>,
    pub url: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Library {
    pub name: String,
    pub downloads: Option<LibraryDownloads>,
    pub rules: Option<Vec<Rule>>,
    /// Старый формат нативных библиотек: {"windows": "natives-windows-${arch}"}
    pub natives: Option<HashMap<String, String>>,
    /// Maven-репозиторий для библиотек без downloads
    pub url: Option<String>,
    /// Fabric и Quilt кладут хеш и размер прямо в библиотеку
    pub sha1: Option<String>,
    pub size: Option<u64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LibraryDownloads {
    pub artifact: Option<Artifact>,
    pub classifiers: Option<HashMap<String, Artifact>>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JavaVersion {
    pub component: String,
    pub major_version: u32,
}

/// Включённые «функции» лаунчера, от которых зависят некоторые аргументы.
#[derive(Debug, Default, Clone, Copy)]
pub struct Features {
    pub custom_resolution: bool,
}

impl Features {
    fn get(&self, name: &str) -> bool {
        match name {
            "has_custom_resolution" => self.custom_resolution,
            // демо-режим, quick play и прочее мы не включаем
            _ => false,
        }
    }
}

pub fn os_name() -> &'static str {
    if cfg!(target_os = "windows") {
        "windows"
    } else if cfg!(target_os = "macos") {
        "osx"
    } else {
        "linux"
    }
}

impl Rule {
    fn matches(&self, features: Features) -> bool {
        if let Some(os) = &self.os {
            if os.name.as_deref().is_some_and(|name| name != os_name()) {
                return false;
            }
            // "x86" в правилах означает 32-битную систему
            if os.arch.as_deref() == Some("x86") && !cfg!(target_arch = "x86") {
                return false;
            }
        }
        if let Some(required) = &self.features {
            if required.iter().any(|(name, value)| features.get(name) != *value) {
                return false;
            }
        }
        true
    }
}

/// Без правил — разрешено. С правилами — решает последнее подходящее.
pub fn rules_allow(rules: &[Rule], features: Features) -> bool {
    if rules.is_empty() {
        return true;
    }
    let mut allowed = false;
    for rule in rules {
        if rule.matches(features) {
            allowed = rule.action == RuleAction::Allow;
        }
    }
    allowed
}

/// "group:artifact:version[:classifier][@ext]" → "group/path/artifact/version/artifact-version[-classifier].ext"
pub fn maven_path(name: &str) -> Option<String> {
    let (name, ext) = name.split_once('@').unwrap_or((name, "jar"));
    let mut parts = name.split(':');
    let group = parts.next()?;
    let artifact = parts.next()?;
    let version = parts.next()?;
    let classifier = parts.next();
    let file = match classifier {
        Some(c) => format!("{artifact}-{version}-{c}.{ext}"),
        None => format!("{artifact}-{version}.{ext}"),
    };
    Some(format!("{}/{artifact}/{version}/{file}", group.replace('.', "/")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maven_paths() {
        assert_eq!(
            maven_path("org.lwjgl:lwjgl:3.3.3:natives-windows").unwrap(),
            "org/lwjgl/lwjgl/3.3.3/lwjgl-3.3.3-natives-windows.jar"
        );
        assert_eq!(
            maven_path("de.oceanlabs.mcp:mcp_config:1.20.1:mappings@txt").unwrap(),
            "de/oceanlabs/mcp/mcp_config/1.20.1/mcp_config-1.20.1-mappings.txt"
        );
    }

    #[test]
    fn rules() {
        let only_osx = vec![Rule {
            action: RuleAction::Allow,
            os: Some(OsRule { name: Some("osx".into()), arch: None }),
            features: None,
        }];
        assert_eq!(rules_allow(&only_osx, Features::default()), os_name() == "osx");
    }
}
