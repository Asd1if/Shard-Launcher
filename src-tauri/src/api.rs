use axum::{
    Json, Router,
    extract::State,
    http::{
        HeaderValue, StatusCode,
        header::{AUTHORIZATION, ORIGIN},
    },
    middleware,
    response::{IntoResponse, Response},
    routing::{get, post},
};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::{Value, json};
use std::sync::{Arc, Mutex, RwLock};
use tauri::{AppHandle, Manager};
use tokio::sync::oneshot;

#[derive(Default)]
pub struct ApiRuntime {
    shutdown: Mutex<Option<oneshot::Sender<()>>>,
    api_token: Arc<RwLock<String>>,
}

#[derive(Clone)]
pub struct ApiState {
    pub app_handle: AppHandle,
    pub api_token: Arc<RwLock<String>>,
}

#[derive(Deserialize)]
pub struct InstanceRequest {
    pub instance_id: String,
}

#[derive(Serialize)]
pub struct ApiResponse {
    pub success: bool,
    pub message: String,
    pub data: Option<Value>,
}

#[derive(Deserialize)]
struct RpcRequest {
    jsonrpc: String,
    #[serde(default)]
    id: Option<Value>,
    method: String,
    #[serde(default)]
    params: Value,
}

fn required_string(args: &Value, name: &str) -> Result<String, String> {
    args.get(name)
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(str::to_owned)
        .ok_or_else(|| format!("Missing or invalid {name}"))
}

fn optional_string(args: &Value, name: &str) -> Result<Option<String>, String> {
    args.get(name)
        .filter(|value| !value.is_null())
        .map(|value| {
            value
                .as_str()
                .map(str::to_owned)
                .ok_or_else(|| format!("Invalid {name}: expected a string"))
        })
        .transpose()
}

fn optional_value<T: DeserializeOwned>(args: &Value, name: &str) -> Result<Option<T>, String> {
    args.get(name)
        .filter(|value| !value.is_null())
        .cloned()
        .map(serde_json::from_value)
        .transpose()
        .map_err(|error| format!("Invalid {name}: {error}"))
}

fn as_text(value: impl Serialize) -> Result<String, String> {
    serde_json::to_string_pretty(&value).map_err(|error| error.to_string())
}

fn origin_allowed(origin: Option<&HeaderValue>) -> bool {
    origin.map_or(true, |origin| origin == "http://127.0.0.1:8542")
}

async fn call_tool(api_state: &ApiState, name: &str, args: Value) -> Result<String, String> {
    use crate::content::{self, ContentKind, SearchArgs, Source};
    use crate::state::AppState;

    match name {
        "get_instances" => {
            let state = api_state.app_handle.state::<AppState>();
            as_text(crate::instances::instances_list(state).map_err(|error| error.to_string())?)
        }
        "get_instance_details" => {
            let id = required_string(&args, "instance_id")?;
            let state = api_state.app_handle.state::<AppState>();
            as_text(crate::instances::load(&state.paths, &id).map_err(|error| error.to_string())?)
        }
        "get_running_instances" => {
            let state = api_state.app_handle.state::<AppState>();
            as_text(
                crate::launch::running_instances(state)
                    .await
                    .map_err(|error| error.to_string())?,
            )
        }
        "get_instance_content" => {
            let id = required_string(&args, "instance_id")?;
            let state = api_state.app_handle.state::<AppState>();
            as_text(
                content::content_list(state, id, Some(false))
                    .await
                    .map_err(|error| error.to_string())?,
            )
        }
        "get_instance_worlds" => {
            let id = required_string(&args, "instance_id")?;
            let state = api_state.app_handle.state::<AppState>();
            as_text(
                crate::worlds::worlds_list(state, id)
                    .await
                    .map_err(|error| error.to_string())?,
            )
        }
        "get_content_updates" => {
            let id = required_string(&args, "instance_id")?;
            let state = api_state.app_handle.state::<AppState>();
            as_text(
                content::content_updates(state, id)
                    .await
                    .map_err(|error| error.to_string())?,
            )
        }
        "launch_instance" | "stop_instance" => {
            let id = required_string(&args, "instance_id")?;
            let state = api_state.app_handle.state::<AppState>();
            if name == "launch_instance" {
                crate::launch::launch_instance(
                    api_state.app_handle.clone(),
                    state,
                    id.clone(),
                    None,
                )
                .await
                .map_err(|error| error.to_string())?;
                Ok(format!("Launch started for instance {id}"))
            } else {
                crate::launch::stop_instance(state, id.clone())
                    .await
                    .map_err(|error| error.to_string())?;
                Ok(format!("Stop requested for instance {id}"))
            }
        }
        "search_content" => {
            let kind: ContentKind = optional_value(&args, "kind")?
                .ok_or_else(|| "Missing or invalid kind".to_owned())?;
            let search = SearchArgs {
                query: optional_string(&args, "query")?.unwrap_or_default(),
                kind,
                instance_id: optional_value(&args, "instance_id")?,
                sort: optional_string(&args, "sort")?.unwrap_or_else(|| "relevance".to_owned()),
                offset: optional_value(&args, "offset")?.unwrap_or(0),
                source: optional_value::<Source>(&args, "source")?.unwrap_or_default(),
                any_version: optional_value(&args, "any_version")?.unwrap_or(false),
                categories: optional_value(&args, "categories")?.unwrap_or_default(),
                game_version: optional_value(&args, "game_version")?,
                loaders: optional_value(&args, "loaders")?.unwrap_or_default(),
                environment: optional_value(&args, "environment")?,
            };
            let state = api_state.app_handle.state::<AppState>();
            as_text(
                content::content_search(state, search)
                    .await
                    .map_err(|error| error.to_string())?,
            )
        }
        "install_content" => {
            let id = required_string(&args, "instance_id")?;
            let project_id = required_string(&args, "project_id")?;
            let kind: ContentKind = optional_value(&args, "kind")?
                .ok_or_else(|| "Missing or invalid kind".to_owned())?;
            let source = optional_value(&args, "source")?;
            let any_version = args.get("any_version").and_then(Value::as_bool);
            let game_version = optional_value(&args, "game_version")?;
            let state = api_state.app_handle.state::<AppState>();
            as_text(
                content::content_install(
                    state,
                    id,
                    project_id,
                    kind,
                    source,
                    any_version,
                    game_version,
                )
                .await
                .map_err(|error| error.to_string())?,
            )
        }
        "toggle_content" => {
            let id = required_string(&args, "instance_id")?;
            let kind: ContentKind = optional_value(&args, "kind")?
                .ok_or_else(|| "Missing or invalid kind".to_owned())?;
            let file = required_string(&args, "file")?;
            let enabled = args
                .get("enabled")
                .and_then(Value::as_bool)
                .ok_or_else(|| "Missing or invalid enabled".to_owned())?;
            let state = api_state.app_handle.state::<AppState>();
            as_text(
                content::content_toggle(state, id, kind, file, enabled)
                    .map_err(|error| error.to_string())?,
            )
        }
        "remove_content" => {
            let id = required_string(&args, "instance_id")?;
            let kind: ContentKind = optional_value(&args, "kind")?
                .ok_or_else(|| "Missing or invalid kind".to_owned())?;
            let file = required_string(&args, "file")?;
            let state = api_state.app_handle.state::<AppState>();
            let message = format!("Removed {file} from instance {id}");
            content::content_remove(state, id, kind, file).map_err(|error| error.to_string())?;
            Ok(message)
        }
        _ => Err("Unknown tool".to_owned()),
    }
}

async fn auth_middleware(
    State(state): State<ApiState>,
    req: axum::extract::Request,
    next: middleware::Next,
) -> Result<Response, StatusCode> {
    if !origin_allowed(req.headers().get(ORIGIN)) {
        return Err(StatusCode::FORBIDDEN);
    }

    let token = state
        .api_token
        .read()
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .clone();
    let authorized = req
        .headers()
        .get(AUTHORIZATION)
        .and_then(|header| header.to_str().ok())
        .and_then(|header| header.strip_prefix("Bearer "))
        .is_some_and(|provided| provided == token);

    if authorized {
        Ok(next.run(req).await)
    } else {
        Err(StatusCode::UNAUTHORIZED)
    }
}

async fn get_instances(State(api_state): State<ApiState>) -> Result<Json<ApiResponse>, StatusCode> {
    let app_state = api_state.app_handle.state::<crate::state::AppState>();
    let instances = crate::instances::instances_list(app_state).map_err(|error| {
        eprintln!("MCP API failed to list instances: {error}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let data = serde_json::to_value(instances).map_err(|error| {
        eprintln!("MCP API failed to serialize instances: {error}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok(Json(ApiResponse {
        success: true,
        message: "Instances retrieved".into(),
        data: Some(data),
    }))
}

async fn launch_instance(
    State(api_state): State<ApiState>,
    Json(req): Json<InstanceRequest>,
) -> (StatusCode, Json<ApiResponse>) {
    let result = crate::launch::launch_instance(
        api_state.app_handle.clone(),
        api_state.app_handle.state::<crate::state::AppState>(),
        req.instance_id.clone(),
        None,
    )
    .await;

    match result {
        Ok(()) => (
            StatusCode::ACCEPTED,
            Json(ApiResponse {
                success: true,
                message: format!("Instance '{}' launch initiated", req.instance_id),
                data: None,
            }),
        ),
        Err(error) => {
            eprintln!("MCP API failed to launch instance: {error}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse {
                    success: false,
                    message: error.to_string(),
                    data: None,
                }),
            )
        }
    }
}

async fn stop_instance(
    State(api_state): State<ApiState>,
    Json(req): Json<InstanceRequest>,
) -> (StatusCode, Json<ApiResponse>) {
    let result = crate::launch::stop_instance(
        api_state.app_handle.state::<crate::state::AppState>(),
        req.instance_id,
    )
    .await;

    match result {
        Ok(()) => (
            StatusCode::OK,
            Json(ApiResponse {
                success: true,
                message: "Instance stopped".into(),
                data: None,
            }),
        ),
        Err(error) => {
            eprintln!("MCP API failed to stop instance: {error}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse {
                    success: false,
                    message: error.to_string(),
                    data: None,
                }),
            )
        }
    }
}

fn rpc_result(id: Value, result: Value) -> Response {
    Json(json!({ "jsonrpc": "2.0", "id": id, "result": result })).into_response()
}

fn rpc_error(id: Option<Value>, code: i32, message: &str) -> Response {
    Json(json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": { "code": code, "message": message }
    }))
    .into_response()
}

async fn mcp_endpoint(
    State(api_state): State<ApiState>,
    Json(request): Json<RpcRequest>,
) -> Response {
    if request.jsonrpc != "2.0" {
        return rpc_error(request.id, -32600, "Invalid JSON-RPC version");
    }

    let Some(id) = request.id.clone() else {
        return StatusCode::ACCEPTED.into_response();
    };

    match request.method.as_str() {
        "initialize" => {
            let version = request
                .params
                .get("protocolVersion")
                .and_then(Value::as_str)
                .unwrap_or("2025-03-26");
            rpc_result(
                id,
                json!({
                    "protocolVersion": version,
                    "capabilities": { "tools": {} },
                    "serverInfo": { "name": "shard-launcher", "version": env!("CARGO_PKG_VERSION") }
                }),
            )
        }
        "notifications/initialized" => StatusCode::ACCEPTED.into_response(),
        "ping" => rpc_result(id, json!({})),
        "tools/list" => rpc_result(
            id,
            json!({
                "tools": [
                    { "name": "get_instances", "description": "List all downloaded Minecraft instances.", "inputSchema": { "type": "object", "properties": {}, "additionalProperties": false } },
                    { "name": "get_instance_details", "description": "Get details for one instance.", "inputSchema": { "type": "object", "properties": { "instance_id": { "type": "string" } }, "required": ["instance_id"], "additionalProperties": false } },
                    { "name": "get_running_instances", "description": "List currently running Minecraft instances.", "inputSchema": { "type": "object", "properties": {}, "additionalProperties": false } },
                    { "name": "get_instance_content", "description": "List installed mods, resource packs, shaders, and data packs.", "inputSchema": { "type": "object", "properties": { "instance_id": { "type": "string" } }, "required": ["instance_id"], "additionalProperties": false } },
                    { "name": "get_instance_worlds", "description": "List worlds saved in an instance.", "inputSchema": { "type": "object", "properties": { "instance_id": { "type": "string" } }, "required": ["instance_id"], "additionalProperties": false } },
                    { "name": "get_content_updates", "description": "Check for available updates to installed content.", "inputSchema": { "type": "object", "properties": { "instance_id": { "type": "string" } }, "required": ["instance_id"], "additionalProperties": false } },
                    { "name": "launch_instance", "description": "Launch a Minecraft instance by its ID.", "inputSchema": { "type": "object", "properties": { "instance_id": { "type": "string" } }, "required": ["instance_id"], "additionalProperties": false } },
                    { "name": "stop_instance", "description": "Stop a running Minecraft instance by its ID.", "inputSchema": { "type": "object", "properties": { "instance_id": { "type": "string" } }, "required": ["instance_id"], "additionalProperties": false } },
                    { "name": "search_content", "description": "Search Modrinth or CurseForge for mods, resource packs, shaders, or data packs.", "inputSchema": { "type": "object", "properties": { "query": { "type": "string" }, "kind": { "type": "string", "enum": ["mod", "resourcepack", "shader", "datapack"] }, "instance_id": { "type": "string" }, "source": { "type": "string", "enum": ["modrinth", "curseforge"] }, "any_version": { "type": "boolean" }, "game_version": { "type": "string" }, "sort": { "type": "string" }, "offset": { "type": "integer" }, "categories": { "type": "array", "items": { "type": "string" } }, "loaders": { "type": "array", "items": { "type": "string" } }, "environment": { "type": "string", "enum": ["client", "server", "both"] } }, "required": ["kind"], "additionalProperties": false } },
                    { "name": "install_content", "description": "Install the latest compatible project version into an instance.", "inputSchema": { "type": "object", "properties": { "instance_id": { "type": "string" }, "project_id": { "type": "string" }, "kind": { "type": "string", "enum": ["mod", "resourcepack", "shader", "datapack"] }, "source": { "type": "string", "enum": ["modrinth", "curseforge"] }, "any_version": { "type": "boolean" }, "game_version": { "type": "string" } }, "required": ["instance_id", "project_id", "kind"], "additionalProperties": false } },
                    { "name": "toggle_content", "description": "Enable or disable installed content in an instance.", "inputSchema": { "type": "object", "properties": { "instance_id": { "type": "string" }, "kind": { "type": "string", "enum": ["mod", "resourcepack", "shader", "datapack"] }, "file": { "type": "string" }, "enabled": { "type": "boolean" } }, "required": ["instance_id", "kind", "file", "enabled"], "additionalProperties": false } },
                    { "name": "remove_content", "description": "Permanently remove installed content from an instance.", "inputSchema": { "type": "object", "properties": { "instance_id": { "type": "string" }, "kind": { "type": "string", "enum": ["mod", "resourcepack", "shader", "datapack"] }, "file": { "type": "string" } }, "required": ["instance_id", "kind", "file"], "additionalProperties": false } }
                ]
            }),
        ),
        "tools/call" => {
            let Some(name) = request.params.get("name").and_then(Value::as_str) else {
                return rpc_error(Some(id), -32602, "Missing tool name");
            };
            let args = request
                .params
                .get("arguments")
                .cloned()
                .unwrap_or_else(|| json!({}));
            let result = call_tool(&api_state, name, args).await;
            if matches!(&result, Err(error) if error == "Unknown tool") {
                return rpc_error(Some(id), -32602, "Unknown tool");
            }
            let is_error = result.is_err();
            let text = result.unwrap_or_else(|error| error);
            rpc_result(
                id,
                json!({
                    "content": [{ "type": "text", "text": text }],
                    "isError": is_error
                }),
            )
        }
        _ => rpc_error(Some(id), -32601, "Method not found"),
    }
}

pub fn set_enabled(
    app_handle: &AppHandle,
    enabled: bool,
    token: Option<&str>,
) -> crate::error::Result<()> {
    let runtime = app_handle.state::<ApiRuntime>();
    let mut shutdown = runtime
        .shutdown
        .lock()
        .map_err(|error| crate::error::Error::msg(error.to_string()))?;

    if !enabled {
        if let Some(shutdown) = shutdown.take() {
            let _ = shutdown.send(());
        }
        *runtime
            .api_token
            .write()
            .map_err(|error| crate::error::Error::msg(error.to_string()))? = String::new();
        return Ok(());
    }

    let token = token
        .filter(|token| !token.trim().is_empty())
        .ok_or_else(|| crate::error::Error::msg("MCP API token is missing"))?;
    *runtime
        .api_token
        .write()
        .map_err(|error| crate::error::Error::msg(error.to_string()))? = token.to_owned();

    if shutdown.is_some() {
        return Ok(());
    }

    let listener = std::net::TcpListener::bind("127.0.0.1:8542")
        .map_err(|error| crate::error::Error::msg(format!("Failed to bind MCP API: {error}")))?;
    listener.set_nonblocking(true)?;
    let state = ApiState {
        app_handle: app_handle.clone(),
        api_token: runtime.api_token.clone(),
    };
    let app = Router::new()
        .route("/mcp", post(mcp_endpoint))
        .route("/api/instances", get(get_instances))
        .route("/api/launch", post(launch_instance))
        .route("/api/stop", post(stop_instance))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            auth_middleware,
        ))
        .with_state(state);
    let (shutdown_tx, shutdown_rx) = oneshot::channel();
    *shutdown = Some(shutdown_tx);
    drop(shutdown);

    let app_handle = app_handle.clone();
    tauri::async_runtime::spawn(async move {
        let listener = match tokio::net::TcpListener::from_std(listener) {
            Ok(listener) => listener,
            Err(error) => {
                eprintln!("Failed to start MCP API listener: {error}");
                return;
            }
        };
        eprintln!("Shard Launcher MCP API listening on http://127.0.0.1:8542/mcp");
        if let Err(error) = axum::serve(listener, app)
            .with_graceful_shutdown(async {
                let _ = shutdown_rx.await;
            })
            .await
        {
            eprintln!("MCP API server stopped: {error}");
        }
        if let Ok(mut shutdown) = app_handle.state::<ApiRuntime>().shutdown.lock() {
            *shutdown = None;
        }
    });

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn origin_check_rejects_non_local_origins() {
        assert!(origin_allowed(None));
        assert!(origin_allowed(Some(&HeaderValue::from_static(
            "http://127.0.0.1:8542"
        ))));
        assert!(!origin_allowed(Some(&HeaderValue::from_static(
            "http://attacker.example"
        ))));
    }
}
