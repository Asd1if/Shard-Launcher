//! Аргументы JVM и игры с подстановкой ${плейсхолдеров}.

use std::collections::HashMap;

use super::version::{rules_allow, Argument, Features, VersionJson};

/// Пользовательские параметры запуска из настроек сборки.
#[derive(Debug, Clone, Default)]
pub struct LaunchOptions {
    pub memory_mb: u32,
    pub min_memory_mb: Option<u32>,
    pub extra_jvm: Vec<String>,
    pub extra_game: Vec<String>,
    pub resolution: Option<(u32, u32)>,
    pub fullscreen: bool,
    /// Сразу подключиться к серверу: `host` или `host:port`
    pub server: Option<String>,
}

pub struct ArgsInput<'a> {
    pub version: &'a VersionJson,
    pub vars: HashMap<&'static str, String>,
    pub options: &'a LaunchOptions,
}

/// Делит строку на аргументы по пробелам; "двойные" и 'одинарные' кавычки группируют.
/// Обратный слэш не экранирует — иначе ломались бы пути Windows.
pub fn split_args(input: &str) -> Vec<String> {
    let mut out = vec![];
    let mut current = String::new();
    let mut quote: Option<char> = None;
    let mut has_token = false;
    for ch in input.chars() {
        match quote {
            Some(q) if ch == q => quote = None,
            Some(_) => current.push(ch),
            None if ch == '"' || ch == '\'' => {
                quote = Some(ch);
                has_token = true;
            }
            None if ch.is_whitespace() => {
                if has_token || !current.is_empty() {
                    out.push(std::mem::take(&mut current));
                    has_token = false;
                }
            }
            None => current.push(ch),
        }
    }
    if has_token || !current.is_empty() {
        out.push(current);
    }
    out
}

fn substitute(arg: &str, vars: &HashMap<&'static str, String>) -> String {
    let mut out = arg.to_owned();
    for (key, value) in vars {
        out = out.replace(&format!("${{{key}}}"), value);
    }
    out
}

fn expand(args: &[Argument], features: Features) -> Vec<String> {
    let mut out = vec![];
    for arg in args {
        match arg {
            Argument::Plain(s) => out.push(s.clone()),
            Argument::Ruled { rules, value } => {
                if rules_allow(rules, features) {
                    out.extend(value.clone().into_vec());
                }
            }
        }
    }
    out
}

/// Подключение к серверу при запуске. С 1.20 работает Quick Play (`--quickPlayMultiplayer`),
/// раньше — `--server` и `--port`.
fn server_args(v: &VersionJson, server: &str) -> Vec<String> {
    let quick_play = v.arguments.as_ref().is_some_and(|a| format!("{:?}", a.game).contains("quickPlayMultiplayer"));
    if quick_play {
        return vec!["--quickPlayMultiplayer".into(), server.to_owned()];
    }
    let (host, port) = match server.rsplit_once(':') {
        Some((h, p)) if p.parse::<u16>().is_ok() => (h, p),
        _ => (server, "25565"),
    };
    vec!["--server".into(), host.to_owned(), "--port".into(), port.to_owned()]
}

/// Полная командная строка после пути к java.
pub fn build(input: &ArgsInput) -> Vec<String> {
    let o = input.options;
    let features = Features { custom_resolution: o.resolution.is_some() };
    let v = input.version;

    let mut vars = input.vars.clone();
    if let Some((w, h)) = o.resolution {
        vars.insert("resolution_width", w.to_string());
        vars.insert("resolution_height", h.to_string());
    }

    let mut args = vec![
        format!("-Xmx{}M", o.memory_mb),
        format!("-Xms{}M", o.min_memory_mb.unwrap_or(512).min(o.memory_mb)),
        // Вывод в UTF-8, а не в кодировке консоли Windows (Java 18+, старые версии флаг игнорируют)
        "-Dstdout.encoding=UTF-8".to_owned(),
        "-Dstderr.encoding=UTF-8".to_owned(),
    ];

    let (jvm, mut game) = match &v.arguments {
        Some(a) => (expand(&a.jvm, features), expand(&a.game, features)),
        // До 1.13 JVM-аргументов в JSON нет — задаём стандартные сами
        None => {
            let mut game: Vec<String> = v
                .minecraft_arguments
                .as_deref()
                .unwrap_or_default()
                .split_whitespace()
                .map(str::to_owned)
                .collect();
            if let Some((w, h)) = o.resolution {
                game.extend(["--width".into(), w.to_string(), "--height".into(), h.to_string()]);
            }
            (
                vec![
                    "-Djava.library.path=${natives_directory}".to_owned(),
                    "-cp".to_owned(),
                    "${classpath}".to_owned(),
                ],
                game,
            )
        }
    };
    if o.fullscreen {
        game.push("--fullscreen".to_owned());
    }
    if let Some(server) = &o.server {
        game.extend(server_args(v, server));
    }

    // Пользовательские JVM-аргументы идут после стандартной памяти, поэтому их -Xmx/-Xms побеждают
    args.extend(o.extra_jvm.iter().cloned());
    args.extend(jvm);
    args.push(v.main_class.clone());
    args.extend(game);
    args.extend(o.extra_game.iter().cloned());
    args.iter().map(|a| substitute(a, &vars)).collect()
}

#[cfg(test)]
mod tests {
    use super::split_args;

    #[test]
    fn splits_with_quotes() {
        assert_eq!(
            split_args(r#"-XX:+UseG1GC  "-Dpath=C:\My Games\x" '' -Da=b"#),
            vec!["-XX:+UseG1GC", r"-Dpath=C:\My Games\x", "", "-Da=b"]
        );
        assert!(split_args("   ").is_empty());
    }
}
