//! Разбор вылета игры: по отчёту о краше, логу и выводу процесса ищем понятную причину.

use std::path::{Path, PathBuf};
use std::time::SystemTime;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CrashKind {
    /// Не хватает мода-зависимости или её версия не подходит
    MissingDependency,
    /// Мод несовместим с другим модом или с версией игры/загрузчика
    Incompatible,
    /// Один мод установлен дважды
    Duplicate,
    /// Мод не смог встроиться в код игры (Mixin)
    Mixin,
    /// Игре не хватило памяти
    Memory,
    /// Мод или игра требуют другую Java
    Java,
    /// Видеодрайвер не поддерживает нужный OpenGL
    Graphics,
    /// Падение самой Java-машины (часто драйверы или нативные библиотеки модов)
    Native,
    /// Падение, в котором отчёт называет подозрительные моды
    ModCrash,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CrashReport {
    pub kind: CrashKind,
    /// Моды, на которые указывает причина
    pub mods: Vec<String>,
    /// Ключевые строки из лога, как есть
    pub details: Vec<String>,
    /// Путь к отчёту о краше, если игра его создала
    pub file: Option<String>,
    /// Нужная версия Java для ошибки Java
    pub java_major: Option<u32>,
}

/// Самый свежий файл в папке, созданный после запуска игры.
fn newest_since(dir: &Path, since: SystemTime, filter: impl Fn(&str) -> bool) -> Option<PathBuf> {
    std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .filter(|e| filter(&e.file_name().to_string_lossy()))
        .filter_map(|e| Some((e.metadata().ok()?.modified().ok()?, e.path())))
        .filter(|(time, _)| *time >= since)
        .max_by_key(|(time, _)| *time)
        .map(|(_, path)| path)
}

fn read_tail(path: &Path, max: usize) -> String {
    let Ok(bytes) = std::fs::read(path) else { return String::new() };
    let start = bytes.len().saturating_sub(max);
    String::from_utf8_lossy(&bytes[start..]).into_owned()
}

/// Ищет причину вылета. `output` — последние строки вывода процесса.
pub fn analyze(game_dir: &Path, since: SystemTime, output: &[String]) -> CrashReport {
    let report = newest_since(&game_dir.join("crash-reports"), since, |n| n.ends_with(".txt"));
    // hs_err_pid*.log — отчёт самой Java-машины о падении
    let jvm = newest_since(game_dir, since, |n| n.starts_with("hs_err_pid") && n.ends_with(".log"));

    let mut text = String::new();
    if let Some(path) = &report {
        text.push_str(&read_tail(path, 256 * 1024));
        text.push('\n');
    }
    let log = game_dir.join("logs").join("latest.log");
    if std::fs::metadata(&log).and_then(|m| m.modified()).is_ok_and(|t| t >= since) {
        text.push_str(&read_tail(&log, 512 * 1024));
        text.push('\n');
    }
    text.push_str(&output.join("\n"));

    let mut result = classify(&text);
    if result.kind == CrashKind::Unknown && jvm.is_some() {
        result.kind = CrashKind::Native;
        if let Some(path) = &jvm {
            let head = read_tail(path, 64 * 1024);
            result.details = head.lines().filter(|l| l.starts_with("# ")).take(8).map(str::to_owned).collect();
        }
    }
    result.file = report.or(jvm).map(|p| p.to_string_lossy().into_owned());
    result
}

fn report(kind: CrashKind, mods: Vec<String>, details: Vec<String>) -> CrashReport {
    let mut unique: Vec<String> = vec![];
    for m in mods {
        if !m.is_empty() && !unique.contains(&m) {
            unique.push(m);
        }
    }
    CrashReport { kind, mods: unique, details: details.into_iter().take(12).collect(), file: None, java_major: None }
}

/// Строки, содержащие любое из слов, без повторов.
fn lines_with<'a>(text: &'a str, needles: &[&str]) -> Vec<&'a str> {
    let mut out: Vec<&str> = vec![];
    for line in text.lines().map(str::trim) {
        if needles.iter().any(|n| line.contains(n)) && !out.contains(&line) {
            out.push(line);
        }
    }
    out
}

/// Текст в одинарных кавычках после `marker`: `Mod ID: 'sodium'` → sodium.
fn quoted_after(line: &str, marker: &str) -> Option<String> {
    let rest = &line[line.find(marker)? + marker.len()..];
    let rest = rest.trim_start().strip_prefix('\'')?;
    Some(rest[..rest.find('\'')?].to_owned())
}

fn classify(text: &str) -> CrashReport {
    // --- Fabric / Quilt: не хватает зависимостей или несовместимые моды ---
    if text.contains("Incompatible mods found!") || text.contains("Mod resolution failed") || text.contains("ModResolutionException") {
        let details: Vec<String> = text
            .lines()
            .map(str::trim)
            .filter(|l| l.starts_with("- ") || l.starts_with("\t- ") || l.contains("requires") || l.contains("is incompatible"))
            .filter(|l| l.len() < 400)
            .map(str::to_owned)
            .collect();
        // «Mod 'Sodium' (sodium) 0.5.8 requires version 0.4 or later of mod 'Fabric API' (fabric-api), which is missing!»
        let missing = details.iter().any(|l| l.contains("which is missing") || l.contains("Install "));
        let mods = details.iter().filter_map(|l| fabric_mod_name(l)).collect();
        let kind = if missing { CrashKind::MissingDependency } else { CrashKind::Incompatible };
        return report(kind, mods, dedupe(details));
    }

    // --- Forge / NeoForge: отсутствующие зависимости ---
    if text.contains("Missing or unsupported mandatory dependencies") {
        let lines = lines_with(text, &["Mod ID: '", "Mod ID:"]);
        let mods = lines.iter().filter_map(|l| quoted_after(l, "Mod ID:")).collect();
        return report(CrashKind::MissingDependency, mods, lines.into_iter().map(str::to_owned).collect());
    }

    // --- Дубликаты ---
    if text.contains("DuplicateModsFoundException") || text.contains("Found duplicate mods") || text.contains("Duplicate mods found") {
        let lines = lines_with(text, &["Mod ID:", "is loaded from", "duplicate", "Duplicate"]);
        let mods = lines.iter().filter_map(|l| quoted_after(l, "Mod ID:")).collect();
        return report(CrashKind::Duplicate, mods, lines.into_iter().map(str::to_owned).collect());
    }

    // --- Не та Java ---
    if text.contains("UnsupportedClassVersionError") || text.contains("compiled by a more recent version of the Java Runtime") {
        let lines = lines_with(text, &["UnsupportedClassVersionError", "class file version"]);
        let mut result = report(CrashKind::Java, vec![], lines.iter().map(|l| l.to_string()).collect());
        // class file version 65.0 → Java 21 (номер класса минус 44)
        result.java_major = lines
            .iter()
            .find_map(|l| l.split("class file version ").nth(1))
            .and_then(|v| v.split('.').next()?.parse::<u32>().ok())
            .map(|v| v.saturating_sub(44));
        return result;
    }
    if text.contains("requires Java") || text.contains("Java version mismatch") {
        let lines = lines_with(text, &["requires Java", "Java version"]);
        return report(CrashKind::Java, vec![], lines.into_iter().map(str::to_owned).collect());
    }

    // --- Память ---
    if text.contains("java.lang.OutOfMemoryError") || text.contains("Could not reserve enough space for") || text.contains("There is insufficient memory") {
        let lines = lines_with(text, &["OutOfMemoryError", "Could not reserve", "insufficient memory"]);
        return report(CrashKind::Memory, vec![], lines.into_iter().map(str::to_owned).collect());
    }

    // --- Графика ---
    if ["Pixel format not accelerated", "GLFW error 65542", "GLFW error 65543", "WGL: The driver does not appear to support OpenGL", "No OpenGL context", "Couldn't set pixel format"]
        .iter()
        .any(|n| text.contains(n))
    {
        let lines = lines_with(text, &["GLFW error", "OpenGL", "Pixel format"]);
        return report(CrashKind::Graphics, vec![], lines.into_iter().map(str::to_owned).collect());
    }

    // --- Mixin ---
    if text.contains("MixinApplyError") || text.contains("Mixin apply failed") || text.contains("InvalidInjectionException") || text.contains("MixinTransformerError") {
        let lines = lines_with(text, &["Mixin apply", "from mod ", "InvalidInjectionException", "MixinApplyError"]);
        // «Mixin apply for mod sodium failed sodium.mixins.json:…» / «… from mod sodium»
        let mods = lines
            .iter()
            .filter_map(|l| {
                let rest = l.split("for mod ").nth(1).or_else(|| l.split("from mod ").nth(1))?;
                Some(rest.split(|c: char| c.is_whitespace() || c == ']' || c == ',').next()?.to_owned())
            })
            .collect();
        return report(CrashKind::Mixin, mods, lines.into_iter().map(str::to_owned).collect());
    }

    // --- Отчёт о краше называет моды ---
    let suspects = suspected_mods(text);
    let description = lines_with(text, &["Description:"]).into_iter().next().map(str::to_owned);
    let exception = first_exception(text);
    let details: Vec<String> = description.into_iter().chain(exception).collect();
    if !suspects.is_empty() {
        return report(CrashKind::ModCrash, suspects, details);
    }
    report(CrashKind::Unknown, vec![], details)
}

fn dedupe(lines: Vec<String>) -> Vec<String> {
    let mut out: Vec<String> = vec![];
    for l in lines {
        if !out.contains(&l) {
            out.push(l);
        }
    }
    out
}

/// `Mod 'Sodium' (sodium) 0.5.8 requires … of mod 'Fabric API' (fabric-api)` → последний мод в строке.
fn fabric_mod_name(line: &str) -> Option<String> {
    let mut name = None;
    let mut rest = line;
    while let Some(i) = rest.find("mod '") {
        rest = &rest[i + 5..];
        let end = rest.find('\'')?;
        name = Some(rest[..end].to_owned());
        rest = &rest[end..];
    }
    name
}

/// Fabric: `Suspected Mods: Sodium (sodium), …`; Forge: блок `Suspected Mod(s):` со строками ниже.
fn suspected_mods(text: &str) -> Vec<String> {
    let mut out = vec![];
    let mut lines = text.lines().map(str::trim).peekable();
    while let Some(line) = lines.next() {
        let Some(rest) = line.strip_prefix("Suspected Mods:").or_else(|| line.strip_prefix("Suspected Mod:")) else {
            if line.starts_with("Suspected Mod(s):") {
                while let Some(next) = lines.peek() {
                    if next.is_empty() || next.starts_with("Stacktrace") {
                        break;
                    }
                    // «Sodium (sodium), Version: 0.5.8»
                    let name = next.split(" (").next().unwrap_or(next).trim_start_matches("- ").to_owned();
                    out.push(name);
                    lines.next();
                }
            }
            continue;
        };
        let rest = rest.trim();
        if rest.is_empty() || rest.starts_with("NONE") || rest.starts_with("Unknown") {
            continue;
        }
        for part in rest.split(", ") {
            let name = part.split(" (").next().unwrap_or(part).trim();
            if !name.is_empty() && name != "Minecraft" {
                out.push(name.to_owned());
            }
        }
    }
    out.retain(|m| !matches!(m.as_str(), "Minecraft" | "Fabric Loader" | "Forge" | "NeoForge" | "Java"));
    out.truncate(5);
    out
}

/// Первая строка вида `java.lang.SomeException: текст` — главная ошибка.
fn first_exception(text: &str) -> Option<String> {
    text.lines()
        .map(str::trim)
        .find(|l| {
            let head = l.split(':').next().unwrap_or("");
            (head.ends_with("Exception") || head.ends_with("Error")) && head.contains('.') && !head.contains(' ')
        })
        .map(|l| l.chars().take(300).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fabric_missing_dependency() {
        let log = "[main/ERROR]: Incompatible mods found!\n\
            net.fabricmc.loader.impl.FormattedException: Some of your mods are incompatible with the game or each other!\n\
            A potential solution has been determined, this may resolve your problem:\n\
            \t - Install fabric-api, version 0.92.0 or later.\n\
            More details:\n\
            \t - Mod 'Sodium' (sodium) 0.5.8 requires version 0.92.0 or later of mod 'Fabric API' (fabric-api), which is missing!";
        let r = classify(log);
        assert_eq!(r.kind, CrashKind::MissingDependency);
        assert_eq!(r.mods, vec!["Fabric API"]);
    }

    #[test]
    fn forge_missing_dependency() {
        let log = "Missing or unsupported mandatory dependencies:\n\
            \tMod ID: 'geckolib', Requested by: 'mowziesmobs', Expected range: '[4.4,)', Actual version: '[MISSING]'";
        let r = classify(log);
        assert_eq!(r.kind, CrashKind::MissingDependency);
        assert_eq!(r.mods, vec!["geckolib"]);
    }

    #[test]
    fn wrong_java() {
        let log = "Exception in thread \"main\" java.lang.UnsupportedClassVersionError: net/minecraft/client/main/Main has been compiled by a more recent version of the Java Runtime (class file version 65.0), this version of the Java Runtime only recognizes class file versions up to 52.0";
        let r = classify(log);
        assert_eq!(r.kind, CrashKind::Java);
        assert_eq!(r.java_major, Some(21));
    }

    #[test]
    fn out_of_memory() {
        assert_eq!(classify("java.lang.OutOfMemoryError: Java heap space").kind, CrashKind::Memory);
    }

    #[test]
    fn suspected_mods_from_fabric_report() {
        let log = "---- Minecraft Crash Report ----\nDescription: Ticking entity\n\njava.lang.NullPointerException: Cannot invoke\n\
            A detailed walkthrough of the error\nSuspected Mods: Create (create), Flywheel (flywheel)";
        let r = classify(log);
        assert_eq!(r.kind, CrashKind::ModCrash);
        assert_eq!(r.mods, vec!["Create", "Flywheel"]);
        assert_eq!(r.details[0], "Description: Ticking entity");
    }

    #[test]
    fn mixin_failure() {
        let log = "org.spongepowered.asm.mixin.transformer.throwables.MixinTransformerError: An unexpected critical error was encountered\n\
            Caused by: org.spongepowered.asm.mixin.injection.throwables.InvalidInjectionException: Critical injection failure from mod lithium";
        let r = classify(log);
        assert_eq!(r.kind, CrashKind::Mixin);
        assert_eq!(r.mods, vec!["lithium"]);
    }
}
