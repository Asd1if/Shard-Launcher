//! Инструменты MCP-сервера. Ответы — короткий текст по строке на запись, а не JSON целиком:
//! так модели не приходится разбирать сотни полей, и ответ влезает даже в маленький контекст.
//!
//! Сборку можно указать id или названием (без учёта регистра, достаточно уникальной части).
//! Долгие операции (установка модпака, смена его версии) идут в фоне и возвращают id задачи.

use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};

use serde_json::{Value, json};
use tauri::{AppHandle, Emitter, Manager};

use crate::content::{self, ContentKind, SearchArgs, Source};
use crate::instances::{Instance, ModLoader};
use crate::state::AppState;

type ToolResult = Result<String, String>;

/// Описания инструментов для `tools/list`.
pub fn definitions() -> Vec<Value> {
    let instance = json!({ "type": "string", "description": "Instance ID or name (a unique part of the name is enough)" });
    let kind = json!({ "type": "string", "enum": ["mod", "resourcepack", "shader", "datapack"] });
    let source = json!({ "type": "string", "enum": ["modrinth", "curseforge"], "description": "Default: modrinth" });
    let limit = json!({ "type": "integer", "minimum": 1, "maximum": 50, "description": "Max lines to return (default 10)" });
    let loader = json!({ "type": "string", "enum": ["fabric", "quilt", "forge", "neoforge"] });
    let object = |properties: Value, required: &[&str]| json!({ "type": "object", "properties": properties, "required": required, "additionalProperties": false });
    vec![
        json!({ "name": "list_instances", "description": "List instances, most recently played first: id, name, version, loader, mod count, last played, running.", "inputSchema": object(json!({}), &[]) }),
        json!({ "name": "get_instance", "description": "Details of one instance: version, loader, memory, modpack it came from.", "inputSchema": object(json!({ "instance": instance }), &["instance"]) }),
        json!({ "name": "launch_instance", "description": "Start Minecraft. Without `instance`, launches the most recently played instance. Returns once the game has started.", "inputSchema": object(json!({ "instance": instance }), &[]) }),
        json!({ "name": "stop_instance", "description": "Stop a running instance.", "inputSchema": object(json!({ "instance": instance }), &["instance"]) }),
        json!({ "name": "create_instance", "description": "Create an empty instance. mc_version may be 'latest' or 'latest-snapshot'. Loader version defaults to the recommended one.", "inputSchema": object(json!({ "name": { "type": "string" }, "mc_version": { "type": "string" }, "loader": loader, "loader_version": { "type": "string" } }), &["name", "mc_version"]) }),
        json!({ "name": "list_minecraft_versions", "description": "Minecraft versions, newest first.", "inputSchema": object(json!({ "type": { "type": "string", "enum": ["release", "snapshot", "all"], "description": "Default: release" }, "limit": limit }), &[]) }),
        json!({ "name": "list_loader_versions", "description": "Mod loader versions available for a Minecraft version.", "inputSchema": object(json!({ "loader": loader, "mc_version": { "type": "string" }, "limit": limit }), &["loader", "mc_version"]) }),
        json!({ "name": "search_modpacks", "description": "Search modpacks on Modrinth or CurseForge. Returns project IDs for list_modpack_versions and install_modpack.", "inputSchema": object(json!({ "query": { "type": "string" }, "source": source, "mc_version": { "type": "string" }, "loader": loader, "sort": { "type": "string", "enum": ["relevance", "downloads", "updated", "newest"] }, "limit": limit }), &[]) }),
        json!({ "name": "list_modpack_versions", "description": "Versions of a modpack, newest first: version ID, name, Minecraft versions, loaders, date.", "inputSchema": object(json!({ "project_id": { "type": "string" }, "source": source, "mc_version": { "type": "string", "description": "Only versions for this Minecraft version" }, "limit": limit }), &["project_id"]) }),
        json!({ "name": "install_modpack", "description": "Download a modpack as a new instance (runs in the background, returns a task ID for get_task). Without version_id installs the latest version.", "inputSchema": object(json!({ "project_id": { "type": "string" }, "source": source, "version_id": { "type": "string" } }), &["project_id"]) }),
        json!({ "name": "change_modpack_version", "description": "Update or roll back an instance installed from a modpack to another version of that modpack (background task). Worlds and settings are kept.", "inputSchema": object(json!({ "instance": instance, "version_id": { "type": "string" } }), &["instance", "version_id"]) }),
        json!({ "name": "get_task", "description": "Status of background tasks started by install_modpack or change_modpack_version. Without task_id lists all.", "inputSchema": object(json!({ "task_id": { "type": "string" } }), &[]) }),
        json!({ "name": "list_content", "description": "Installed mods, resource packs, shaders or data packs of an instance: file, title, version, enabled.", "inputSchema": object(json!({ "instance": instance, "kind": kind }), &["instance"]) }),
        json!({ "name": "check_content_updates", "description": "Installed content that has a newer version.", "inputSchema": object(json!({ "instance": instance }), &["instance"]) }),
        json!({ "name": "search_content", "description": "Search mods, resource packs, shaders or data packs. With `instance`, filters by its Minecraft version and loader.", "inputSchema": object(json!({ "query": { "type": "string" }, "kind": kind, "instance": instance, "source": source, "mc_version": { "type": "string" }, "sort": { "type": "string", "enum": ["relevance", "downloads", "updated", "newest"] }, "limit": limit }), &["kind"]) }),
        json!({ "name": "install_content", "description": "Install the latest compatible version of a project (with required dependencies) into an instance.", "inputSchema": object(json!({ "instance": instance, "project_id": { "type": "string" }, "kind": kind, "source": source }), &["instance", "project_id", "kind"]) }),
        json!({ "name": "toggle_content", "description": "Enable or disable installed content (file name from list_content).", "inputSchema": object(json!({ "instance": instance, "kind": kind, "file": { "type": "string" }, "enabled": { "type": "boolean" } }), &["instance", "kind", "file", "enabled"]) }),
        json!({ "name": "remove_content", "description": "Permanently delete installed content (file name from list_content).", "inputSchema": object(json!({ "instance": instance, "kind": kind, "file": { "type": "string" } }), &["instance", "kind", "file"]) }),
        json!({ "name": "list_worlds", "description": "Worlds saved in an instance.", "inputSchema": object(json!({ "instance": instance }), &["instance"]) }),
    ]
}

/// Подсказки клиенту: только чтение, необратимость, выход в интернет.
pub fn annotations(name: &str) -> Value {
    let read_only = name.starts_with("list_") || name.starts_with("get_") || name.starts_with("search_") || name == "check_content_updates";
    json!({
        "readOnlyHint": read_only,
        "destructiveHint": name == "remove_content",
        "idempotentHint": read_only || name == "toggle_content",
        "openWorldHint": name.contains("modpack") || name.ends_with("_content") || name.contains("versions"),
    })
}

pub async fn call(app: &AppHandle, name: &str, args: &Value) -> Option<ToolResult> {
    let state = app.state::<AppState>();
    let state: &AppState = &state;
    Some(match name {
        "list_instances" => list_instances(state).await,
        "get_instance" => get_instance(state, args),
        "launch_instance" => launch(app, args).await,
        "stop_instance" => stop(app, args).await,
        "create_instance" => create_instance(app, args).await,
        "list_minecraft_versions" => minecraft_versions(state, args).await,
        "list_loader_versions" => loader_versions(state, args).await,
        "search_modpacks" => search_modpacks(app, args).await,
        "list_modpack_versions" => modpack_versions(app, args).await,
        "install_modpack" => install_modpack(app, args),
        "change_modpack_version" => change_modpack_version(app, args),
        "get_task" => get_task(args),
        "list_content" => list_content(app, args).await,
        "check_content_updates" => content_updates(app, args).await,
        "search_content" => search_content(app, args).await,
        "install_content" => install_content(app, args).await,
        "toggle_content" => toggle_content(app, args),
        "remove_content" => remove_content(app, args),
        "list_worlds" => list_worlds(app, args).await,
        _ => return None,
    })
}

// ---------- аргументы ----------

fn text(args: &Value, name: &str) -> Option<String> {
    args.get(name).and_then(Value::as_str).map(str::trim).filter(|s| !s.is_empty()).map(str::to_owned)
}

fn required(args: &Value, name: &str) -> Result<String, String> {
    text(args, name).ok_or_else(|| format!("Missing argument: {name}"))
}

fn parsed<T: serde::de::DeserializeOwned>(args: &Value, name: &str) -> Result<Option<T>, String> {
    match args.get(name).filter(|v| !v.is_null()) {
        None => Ok(None),
        Some(v) => serde_json::from_value(v.clone()).map(Some).map_err(|_| format!("Invalid {name}: {v}")),
    }
}

fn limit(args: &Value) -> usize {
    args.get("limit").and_then(Value::as_u64).map_or(10, |n| n.clamp(1, 50) as usize)
}

fn source(args: &Value) -> Result<Source, String> {
    Ok(parsed(args, "source")?.unwrap_or_default())
}

fn kind(args: &Value) -> Result<ContentKind, String> {
    parsed(args, "kind")?.ok_or_else(|| "Missing argument: kind".to_owned())
}

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

// ---------- форматирование ----------

fn loader_label(i: &Instance) -> String {
    match (i.loader, &i.loader_version) {
        (None, _) => "vanilla".into(),
        (Some(l), Some(v)) => format!("{} {v}", loader_str(l)),
        (Some(l), None) => loader_str(l).into(),
    }
}

fn loader_str(l: ModLoader) -> &'static str {
    match l {
        ModLoader::Fabric => "fabric",
        ModLoader::Quilt => "quilt",
        ModLoader::Forge => "forge",
        ModLoader::Neoforge => "neoforge",
    }
}

/// «2026-11-20T18:03:11.123+00:00» → «2026-11-20 18:03».
fn short_date(date: &str) -> String {
    let date = date.replace('T', " ");
    date.chars().take(16).collect()
}

fn short(text: &str, max: usize) -> String {
    let text = text.trim().replace('\n', " ");
    if text.chars().count() <= max { text } else { format!("{}…", text.chars().take(max - 1).collect::<String>().trim_end()) }
}

fn count(n: u64) -> String {
    match n {
        0..1_000 => n.to_string(),
        1_000..1_000_000 => format!("{:.1}k", n as f64 / 1e3),
        _ => format!("{:.1}M", n as f64 / 1e6),
    }
}

fn kind_str(k: ContentKind) -> &'static str {
    match k {
        ContentKind::Mod => "mod",
        ContentKind::Resourcepack => "resourcepack",
        ContentKind::Shader => "shader",
        ContentKind::Datapack => "datapack",
    }
}

fn source_str(s: Source) -> &'static str {
    match s {
        Source::Modrinth => "modrinth",
        Source::Curseforge => "curseforge",
    }
}

// ---------- сборки ----------

fn recent_first(list: &mut [Instance]) {
    list.sort_by(|a, b| b.last_played.as_ref().unwrap_or(&b.created_at).cmp(a.last_played.as_ref().unwrap_or(&a.created_at)));
}

/// Сборка по id или названию: точное совпадение, потом единственное вхождение.
fn find(state: &AppState, query: &str) -> Result<Instance, String> {
    let list = crate::instances::list(&state.paths).map_err(err)?;
    let q = query.trim().to_lowercase();
    if let Some(i) = list.iter().find(|i| i.id == query.trim() || i.name.to_lowercase() == q) {
        return Ok(i.clone());
    }
    let matches: Vec<&Instance> = list.iter().filter(|i| i.name.to_lowercase().contains(&q)).collect();
    match matches.as_slice() {
        [one] => Ok((*one).clone()),
        [] => Err(format!("No instance matches \"{query}\". Use list_instances.")),
        many => Err(format!("\"{query}\" matches several instances: {}", many.iter().map(|i| format!("{} ({})", i.name, i.id)).collect::<Vec<_>>().join(", "))),
    }
}

fn instance_arg(state: &AppState, args: &Value) -> Result<Instance, String> {
    find(state, &required(args, "instance")?)
}

async fn list_instances(state: &AppState) -> ToolResult {
    let mut list = crate::instances::list(&state.paths).map_err(err)?;
    if list.is_empty() {
        return Ok("No instances yet. Use create_instance or install_modpack.".into());
    }
    recent_first(&mut list);
    let running = state.running.lock().await;
    let mut out = format!("{} instances (id | name | minecraft | loader | mods | last played):\n", list.len());
    for i in &list {
        let played = i.last_played.as_deref().map_or("never".into(), short_date);
        let flag = if running.contains_key(&i.id) { " | RUNNING" } else { "" };
        out += &format!("{} | {} | {} | {} | {} | {played}{flag}\n", i.id, i.name, i.mc_version, loader_label(i), i.mod_count);
    }
    Ok(out)
}

fn get_instance(state: &AppState, args: &Value) -> ToolResult {
    let i = instance_arg(state, args)?;
    let mut out = format!(
        "{} ({})\nMinecraft {} | {} | {} mods | memory {} MB\nCreated {} | last played {}\n",
        i.name,
        i.id,
        i.mc_version,
        loader_label(&i),
        i.mod_count,
        i.memory_mb,
        short_date(&i.created_at),
        i.last_played.as_deref().map_or("never".into(), short_date),
    );
    if let Some(pack) = &i.pack {
        out += &format!(
            "Modpack: {} project {} version {} ({})\n",
            source_str(pack.source),
            pack.project_id,
            pack.version_name.as_deref().unwrap_or("?"),
            pack.version_id.as_deref().unwrap_or("?"),
        );
    }
    Ok(out)
}

async fn launch(app: &AppHandle, args: &Value) -> ToolResult {
    let state = app.state::<AppState>();
    let instance = match text(args, "instance") {
        Some(q) => find(&state, &q)?,
        None => {
            let mut list = crate::instances::list(&state.paths).map_err(err)?;
            recent_first(&mut list);
            list.into_iter().next().ok_or("No instances to launch.")?
        }
    };
    crate::launch::launch_instance(app.clone(), state, instance.id.clone(), None).await.map_err(err)?;
    Ok(format!("Launched {} ({}, Minecraft {}).", instance.name, instance.id, instance.mc_version))
}

async fn stop(app: &AppHandle, args: &Value) -> ToolResult {
    let state = app.state::<AppState>();
    let instance = instance_arg(&state, args)?;
    crate::launch::stop_instance(state, instance.id.clone()).await.map_err(err)?;
    Ok(format!("Stopping {}.", instance.name))
}

async fn create_instance(app: &AppHandle, args: &Value) -> ToolResult {
    let state = app.state::<AppState>();
    let name = required(args, "name")?;
    let wanted = required(args, "mc_version")?;
    let manifest = crate::versions::manifest(&state, false).await.map_err(err)?;
    let id = match wanted.as_str() {
        "latest" => manifest.latest.release.clone(),
        "latest-snapshot" => manifest.latest.snapshot.clone(),
        v => v.to_owned(),
    };
    let version = manifest.versions.iter().find(|v| v.id == id).ok_or_else(|| format!("Unknown Minecraft version {id}. Use list_minecraft_versions."))?;
    let loader: Option<ModLoader> = parsed(args, "loader")?;
    let loader_version = text(args, "loader_version");
    if let (Some(loader), Some(wanted)) = (loader, &loader_version) {
        let known = crate::loaders::versions(&state, loader, &id).await.map_err(err)?;
        if !known.iter().any(|v| &v.version == wanted) {
            return Err(format!("{} {wanted} is not available for Minecraft {id}. Use list_loader_versions.", loader_str(loader)));
        }
    }
    let new = crate::instances::NewInstance { name, mc_version: id, version_type: version.kind, loader, loader_version };
    let i = crate::instances::instance_create(state, new).map_err(err)?;
    let _ = app.emit("instances-changed", ());
    Ok(format!("Created {} ({}): Minecraft {}, {}. Game files download on first launch.", i.name, i.id, i.mc_version, loader_label(&i)))
}

async fn minecraft_versions(state: &AppState, args: &Value) -> ToolResult {
    let manifest = crate::versions::manifest(state, false).await.map_err(err)?;
    let wanted = text(args, "type").unwrap_or_else(|| "release".into());
    let list: Vec<&str> = manifest
        .versions
        .iter()
        .filter(|v| match wanted.as_str() {
            "all" => true,
            "snapshot" => v.kind == crate::versions::VersionType::Snapshot,
            _ => v.kind == crate::versions::VersionType::Release,
        })
        .take(limit(args))
        .map(|v| v.id.as_str())
        .collect();
    Ok(format!("Latest release {}, latest snapshot {}.\n{}", manifest.latest.release, manifest.latest.snapshot, list.join(", ")))
}

async fn loader_versions(state: &AppState, args: &Value) -> ToolResult {
    let loader: ModLoader = parsed(args, "loader")?.ok_or("Missing argument: loader")?;
    let mc = required(args, "mc_version")?;
    let list = crate::loaders::versions(state, loader, &mc).await.map_err(err)?;
    if list.is_empty() {
        return Ok(format!("{} has no versions for Minecraft {mc}.", loader_str(loader)));
    }
    let lines: Vec<String> = list
        .iter()
        .take(limit(args))
        .map(|v| format!("{}{}", v.version, if v.recommended { " (recommended)" } else if v.stable { "" } else { " (beta)" }))
        .collect();
    Ok(format!("{} for Minecraft {mc}, newest first:\n{}", loader_str(loader), lines.join("\n")))
}

// ---------- модпаки ----------

async fn search_modpacks(app: &AppHandle, args: &Value) -> ToolResult {
    let search = crate::modpack::PackSearchArgs {
        query: text(args, "query").unwrap_or_default(),
        sort: text(args, "sort").unwrap_or_else(|| "relevance".into()),
        offset: 0,
        source: source(args)?,
        categories: vec![],
        game_version: text(args, "mc_version"),
        loaders: text(args, "loader").into_iter().collect(),
    };
    let found = crate::modpack::modpack_search(app.state(), search).await.map_err(err)?;
    if found.hits.is_empty() {
        return Ok("Nothing found.".into());
    }
    let mut out = format!("{} modpacks found (project_id | title | downloads | minecraft | description):\n", found.total_hits);
    for h in found.hits.iter().take(limit(args)) {
        let versions = h.game_versions.last().map(|v| format!("up to {v}")).unwrap_or_default();
        out += &format!("{} | {} | {} | {versions} | {}\n", h.project_id, h.title, count(h.downloads), short(&h.description, 90));
    }
    Ok(out)
}

async fn modpack_versions(app: &AppHandle, args: &Value) -> ToolResult {
    let project = required(args, "project_id")?;
    let list = crate::modpack::modpack_versions(app.state(), source(args)?, project.clone()).await.map_err(err)?;
    let mc = text(args, "mc_version");
    let lines: Vec<String> = list
        .iter()
        .filter(|v| mc.as_ref().is_none_or(|mc| v.game_versions.contains(mc)))
        .take(limit(args))
        .map(|v| {
            let manual = if v.manual { " | manual download only" } else { "" };
            format!("{} | {} | {} | {} | {} | {}{manual}", v.id, v.name, v.game_versions.join(","), v.loaders.join(","), &v.date.chars().take(10).collect::<String>(), v.kind)
        })
        .collect();
    if lines.is_empty() {
        return Ok("No matching versions.".into());
    }
    Ok(format!("{} versions of {project} (version_id | name | minecraft | loaders | date | type):\n{}", list.len(), lines.join("\n")))
}

fn install_modpack(app: &AppHandle, args: &Value) -> ToolResult {
    let project = required(args, "project_id")?;
    let source = source(args)?;
    let version_id = text(args, "version_id");
    let import = match source {
        Source::Modrinth => crate::modpack::ImportArgs { url: Some(format!("https://modrinth.com/modpack/{project}")), version_id, ..Default::default() },
        Source::Curseforge => crate::modpack::ImportArgs {
            curseforge_id: Some(project.parse().map_err(|_| "CurseForge project_id must be a number")?),
            version_id,
            ..Default::default()
        },
    };
    let app2 = app.clone();
    Ok(spawn_task(app, format!("install modpack {project}"), async move {
        let result = crate::modpack::modpack_import(app2.state(), import, silent()).await.map_err(err)?;
        Ok(imported(&result))
    }))
}

fn change_modpack_version(app: &AppHandle, args: &Value) -> ToolResult {
    let instance = instance_arg(&app.state::<AppState>(), args)?;
    if instance.pack.is_none() {
        return Err(format!("{} was not installed from a modpack.", instance.name));
    }
    let version_id = required(args, "version_id")?;
    let app2 = app.clone();
    let id = instance.id.clone();
    Ok(spawn_task(app, format!("change {} to version {version_id}", instance.name), async move {
        let result = crate::modpack::modpack_change_version(app2.state(), id, version_id, silent()).await.map_err(err)?;
        Ok(imported(&result))
    }))
}

fn imported(result: &crate::modpack::ImportResult) -> String {
    let i = &result.instance;
    let mut out = format!("Ready: {} ({}), Minecraft {}, {}, {} mods.", i.name, i.id, i.mc_version, loader_label(i), i.mod_count);
    if !result.manual.is_empty() {
        out += &format!(" {} files must be downloaded manually in Shard (CurseForge blocks third-party downloads).", result.manual.len());
    }
    out
}

/// Канал прогресса, который никуда не шлёт: у MCP-клиента нет полосы загрузки.
fn silent<T: tauri::ipc::IpcResponse>() -> tauri::ipc::Channel<T> {
    tauri::ipc::Channel::new(|_| Ok(()))
}

// ---------- фоновые задачи ----------

enum TaskState {
    Running,
    Done(String),
    Failed(String),
}

struct Task {
    label: String,
    started: std::time::Instant,
    state: TaskState,
}

static TASKS: LazyLock<Mutex<HashMap<String, Task>>> = LazyLock::new(Default::default);

fn spawn_task(app: &AppHandle, label: String, work: impl Future<Output = Result<String, String>> + Send + 'static) -> String {
    let id = hex::encode(&uuid::Uuid::new_v4().as_bytes()[..4]);
    TASKS.lock().expect("lock").insert(id.clone(), Task { label: label.clone(), started: std::time::Instant::now(), state: TaskState::Running });
    let app = app.clone();
    let task_id = id.clone();
    tauri::async_runtime::spawn(async move {
        let result = work.await;
        let _ = app.emit("instances-changed", ());
        if let Some(task) = TASKS.lock().expect("lock").get_mut(&task_id) {
            task.state = match result {
                Ok(message) => TaskState::Done(message),
                Err(error) => TaskState::Failed(error),
            };
        }
    });
    format!("Started task {id}: {label}. Check progress with get_task.")
}

fn get_task(args: &Value) -> ToolResult {
    let tasks = TASKS.lock().expect("lock");
    let line = |id: &str, t: &Task| {
        let secs = t.started.elapsed().as_secs();
        match &t.state {
            TaskState::Running => format!("{id} | {} | running for {secs}s", t.label),
            TaskState::Done(m) => format!("{id} | {} | done: {m}", t.label),
            TaskState::Failed(e) => format!("{id} | {} | failed: {e}", t.label),
        }
    };
    match text(args, "task_id") {
        Some(id) => tasks.get(&id).map(|t| line(&id, t)).ok_or_else(|| format!("No task {id}.")),
        None if tasks.is_empty() => Ok("No tasks.".into()),
        None => Ok(tasks.iter().map(|(id, t)| line(id, t)).collect::<Vec<_>>().join("\n")),
    }
}

// ---------- контент ----------

async fn list_content(app: &AppHandle, args: &Value) -> ToolResult {
    let instance = instance_arg(&app.state::<AppState>(), args)?;
    let only: Option<ContentKind> = parsed(args, "kind")?;
    let list = content::content_list(app.state(), instance.id.clone(), Some(false)).await.map_err(err)?;
    let lines: Vec<String> = list
        .iter()
        .filter(|c| only.is_none_or(|k| c.kind == k))
        .map(|c| {
            let (title, version) = c.meta.as_ref().map_or(("", ""), |m| (m.title.as_str(), m.version_number.as_str()));
            let off = if c.enabled { "" } else { " | disabled" };
            format!("{} | {} | {title} {version}{off}", kind_str(c.kind), c.file)
        })
        .collect();
    if lines.is_empty() {
        return Ok(format!("{} has no installed content of that kind.", instance.name));
    }
    Ok(format!("{} items in {} (kind | file | title version):\n{}", lines.len(), instance.name, lines.join("\n")))
}

async fn content_updates(app: &AppHandle, args: &Value) -> ToolResult {
    let instance = instance_arg(&app.state::<AppState>(), args)?;
    let list = content::content_updates(app.state(), instance.id.clone()).await.map_err(err)?;
    if list.is_empty() {
        return Ok(format!("Everything in {} is up to date.", instance.name));
    }
    let lines: Vec<String> = list.iter().map(|u| format!("{} | {} → {} | project {}", kind_str(u.kind), u.file, u.version_number, u.project_id)).collect();
    Ok(format!("{} updates available:\n{}", list.len(), lines.join("\n")))
}

async fn search_content(app: &AppHandle, args: &Value) -> ToolResult {
    let instance = match text(args, "instance") {
        Some(q) => Some(find(&app.state::<AppState>(), &q)?),
        None => None,
    };
    let search = SearchArgs {
        query: text(args, "query").unwrap_or_default(),
        kind: kind(args)?,
        instance_id: instance.as_ref().map(|i| i.id.clone()),
        sort: text(args, "sort").unwrap_or_else(|| "relevance".into()),
        offset: 0,
        source: source(args)?,
        any_version: false,
        categories: vec![],
        game_version: text(args, "mc_version"),
        loaders: vec![],
        environment: None,
    };
    let found = content::content_search(app.state(), search).await.map_err(err)?;
    if found.hits.is_empty() {
        return Ok("Nothing found for this version and loader.".into());
    }
    let mut out = format!("{} found (project_id | title | downloads | description):\n", found.total_hits);
    for h in found.hits.iter().take(limit(args)) {
        out += &format!("{} | {} | {} | {}\n", h.project_id, h.title, count(h.downloads), short(&h.description, 90));
    }
    Ok(out)
}

async fn install_content(app: &AppHandle, args: &Value) -> ToolResult {
    let instance = instance_arg(&app.state::<AppState>(), args)?;
    let project = required(args, "project_id")?;
    let result = content::content_install(app.state(), instance.id.clone(), project, kind(args)?, Some(source(args)?), None, None).await.map_err(err)?;
    let installed: Vec<String> = result
        .installed
        .iter()
        .map(|i| format!("{} {}{}", i.title, i.version_number, if i.dependency { " (dependency)" } else { "" }))
        .collect();
    let mut out = format!("Installed into {}: {}.", instance.name, installed.join(", "));
    if !result.manual.is_empty() {
        out += &format!(" {} files must be downloaded manually in Shard.", result.manual.len());
    }
    Ok(out)
}

fn toggle_content(app: &AppHandle, args: &Value) -> ToolResult {
    let instance = instance_arg(&app.state::<AppState>(), args)?;
    let file = required(args, "file")?;
    let enabled = args.get("enabled").and_then(Value::as_bool).ok_or("Missing argument: enabled")?;
    content::content_toggle(app.state(), instance.id, kind(args)?, file.clone(), enabled).map_err(err)?;
    Ok(format!("{} {file}.", if enabled { "Enabled" } else { "Disabled" }))
}

fn remove_content(app: &AppHandle, args: &Value) -> ToolResult {
    let instance = instance_arg(&app.state::<AppState>(), args)?;
    let file = required(args, "file")?;
    content::content_remove(app.state(), instance.id, kind(args)?, file.clone()).map_err(err)?;
    Ok(format!("Removed {file} from {}.", instance.name))
}

async fn list_worlds(app: &AppHandle, args: &Value) -> ToolResult {
    let instance = instance_arg(&app.state::<AppState>(), args)?;
    let worlds = crate::worlds::worlds_list(app.state(), instance.id.clone()).await.map_err(err)?;
    if worlds.is_empty() {
        return Ok(format!("{} has no worlds yet.", instance.name));
    }
    let lines: Vec<String> = worlds
        .iter()
        .map(|w| format!("{} | {:.1} MB | last played {} | {} backups", w.folder, w.size as f64 / 1_048_576.0, w.last_played.as_deref().map_or("?".into(), short_date), w.backups))
        .collect();
    Ok(format!("{} worlds in {}:\n{}", worlds.len(), instance.name, lines.join("\n")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_compactly() {
        assert_eq!(short_date("2026-11-20T18:03:11.123+00:00"), "2026-11-20 18:03");
        assert_eq!(count(999), "999");
        assert_eq!(count(12_345), "12.3k");
        assert_eq!(count(2_500_000), "2.5M");
        assert_eq!(short("a very long description", 8), "a very…");
        assert_eq!(short("short", 8), "short");
    }

    #[test]
    fn every_tool_has_a_schema() {
        let tools = definitions();
        assert_eq!(tools.len(), 19);
        for t in &tools {
            assert!(t["name"].is_string() && t["description"].is_string());
            assert_eq!(t["inputSchema"]["type"], "object");
        }
        assert_eq!(annotations("remove_content")["destructiveHint"], true);
        assert_eq!(annotations("list_instances")["readOnlyHint"], true);
        assert_eq!(annotations("install_modpack")["readOnlyHint"], false);
    }
}
