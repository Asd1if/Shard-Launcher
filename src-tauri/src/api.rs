use axum::{
    Json, Router,
    extract::State,
    http::{
        HeaderValue, StatusCode, Uri,
        header::{AUTHORIZATION, ORIGIN},
    },
    middleware,
    response::{IntoResponse, Response},
    routing::post,
};
use serde::Deserialize;
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
struct RpcRequest {
    jsonrpc: String,
    #[serde(default)]
    id: Option<Value>,
    method: String,
    #[serde(default)]
    params: Value,
}

pub const PORT: u16 = 8542;

/// Браузерные запросы с чужих сайтов (DNS rebinding) отсекаем по Origin; обычные MCP-клиенты
/// его не шлют.
fn origin_allowed(origin: Option<&HeaderValue>) -> bool {
    origin.map_or(true, |origin| {
        let origin = origin.to_str().unwrap_or_default();
        ["http://127.0.0.1", "http://localhost", "http://[::1]"].iter().any(|host| origin == format!("{host}:{PORT}"))
    })
}

fn query_token(uri: &Uri) -> Option<&str> {
    if uri.path() != "/mcp" {
        return None;
    }

    let mut tokens = uri.query()?.split('&').filter_map(|part| {
        let (key, value) = part.split_once('=')?;
        (key == "token").then_some(value)
    });
    let token = tokens.next()?;
    (!token.is_empty() && tokens.next().is_none()).then_some(token)
}

fn request_is_authorized(uri: &Uri, bearer_token: Option<&str>, expected_token: &str) -> bool {
    bearer_token.is_some_and(|provided| provided == expected_token)
        || query_token(uri).is_some_and(|provided| provided == expected_token)
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
    let bearer_token = req
        .headers()
        .get(AUTHORIZATION)
        .and_then(|header| header.to_str().ok())
        .and_then(|header| header.strip_prefix("Bearer "));
    let authorized = request_is_authorized(req.uri(), bearer_token, &token);

    if authorized {
        Ok(next.run(req).await)
    } else {
        Err(StatusCode::UNAUTHORIZED)
    }
}

/// Версии протокола MCP, которые понимает сервер; первая — самая новая.
const PROTOCOL_VERSIONS: [&str; 4] = ["2025-11-25", "2025-06-18", "2025-03-26", "2024-11-05"];

/// Клиенту отвечаем его же версией, если знаем её, иначе — своей самой новой (так велит спецификация).
fn negotiate_version(requested: Option<&str>) -> &'static str {
    requested.and_then(|v| PROTOCOL_VERSIONS.iter().find(|&&known| known == v).copied()).unwrap_or(PROTOCOL_VERSIONS[0])
}

const INSTRUCTIONS: &str = "Shard Launcher manages Minecraft: Java Edition instances on this computer. \
Answers are short plain-text lines with fields separated by ' | '. Instances can be referred to by ID or name. \
launch_instance without arguments starts the most recently played instance. \
To install a modpack: search_modpacks -> optionally list_modpack_versions -> install_modpack -> poll get_task. \
To add a mod: search_content (with instance) -> install_content. Launching opens the game on the user's screen.";

fn rpc_result(id: Value, result: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": result })
}

fn rpc_error(id: Option<Value>, code: i32, message: &str) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": { "code": code, "message": message }
    })
}

fn tools() -> Value {
    let list: Vec<Value> = crate::mcp_tools::definitions()
        .into_iter()
        .map(|mut tool| {
            tool["annotations"] = crate::mcp_tools::annotations(tool["name"].as_str().unwrap_or_default());
            tool
        })
        .collect();
    json!({ "tools": list })
}

/// Один JSON-RPC запрос. None — это уведомление, на него не отвечают.
async fn handle_rpc(api_state: &ApiState, message: Value) -> Option<Value> {
    let request: RpcRequest = match serde_json::from_value(message) {
        Ok(request) => request,
        Err(_) => return Some(rpc_error(None, -32600, "Invalid request")),
    };
    if request.jsonrpc != "2.0" {
        return Some(rpc_error(request.id, -32600, "Invalid JSON-RPC version"));
    }
    // Уведомления (notifications/initialized, notifications/cancelled …) без id
    let id = request.id?;

    Some(match request.method.as_str() {
        "initialize" => {
            let requested = request.params.get("protocolVersion").and_then(Value::as_str);
            rpc_result(
                id,
                json!({
                    "protocolVersion": negotiate_version(requested),
                    "capabilities": { "tools": { "listChanged": false } },
                    "serverInfo": { "name": "shard-launcher", "title": "Shard Launcher", "version": env!("CARGO_PKG_VERSION") },
                    "instructions": INSTRUCTIONS
                }),
            )
        }
        "ping" => rpc_result(id, json!({})),
        "tools/list" => rpc_result(id, tools()),
        "tools/call" => {
            let Some(name) = request.params.get("name").and_then(Value::as_str) else {
                return Some(rpc_error(Some(id), -32602, "Missing tool name"));
            };
            let args = request
                .params
                .get("arguments")
                .cloned()
                .unwrap_or_else(|| json!({}));
            let Some(result) = crate::mcp_tools::call(&api_state.app_handle, name, &args).await else {
                return Some(rpc_error(Some(id), -32602, &format!("Unknown tool: {name}")));
            };
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
        // Списков ресурсов и промптов у нас нет — отвечаем пусто, а не ошибкой: так спокойнее клиентам
        "resources/list" => rpc_result(id, json!({ "resources": [] })),
        "resources/templates/list" => rpc_result(id, json!({ "resourceTemplates": [] })),
        "prompts/list" => rpc_result(id, json!({ "prompts": [] })),
        _ => rpc_error(Some(id), -32601, "Method not found"),
    })
}

/// MCP Streamable HTTP: тело — один JSON-RPC запрос или пакет (массив). На уведомления и
/// ответы клиента — 202 без тела, на запросы — JSON.
async fn mcp_endpoint(State(api_state): State<ApiState>, body: axum::body::Bytes) -> Response {
    let Ok(message) = serde_json::from_slice::<Value>(&body) else {
        return Json(rpc_error(None, -32700, "Parse error")).into_response();
    };
    let reply = match message {
        Value::Array(batch) if batch.is_empty() => Some(rpc_error(None, -32600, "Empty batch")),
        Value::Array(batch) => {
            let mut replies = vec![];
            for message in batch {
                replies.extend(handle_rpc(&api_state, message).await);
            }
            (!replies.is_empty()).then_some(Value::Array(replies))
        }
        message => handle_rpc(&api_state, message).await,
    };
    match reply {
        Some(reply) => Json(reply).into_response(),
        None => StatusCode::ACCEPTED.into_response(),
    }
}

/// Поток уведомлений от сервера (GET) не поддерживаем — спецификация велит ответить 405.
async fn method_not_allowed() -> StatusCode {
    StatusCode::METHOD_NOT_ALLOWED
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

    let listener = std::net::TcpListener::bind(("127.0.0.1", PORT))
        .map_err(|error| crate::error::Error::msg(format!("Failed to bind MCP API: {error}")))?;
    listener.set_nonblocking(true)?;
    let state = ApiState {
        app_handle: app_handle.clone(),
        api_token: runtime.api_token.clone(),
    };
    let app = Router::new()
        // Streamable HTTP: запросы идут POST-ом; поток сервер→клиент (GET) и сессии не нужны
        .route("/mcp", post(mcp_endpoint).get(method_not_allowed).delete(method_not_allowed))
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
        eprintln!("Shard Launcher MCP server listening on http://127.0.0.1:{PORT}/mcp");
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

    #[test]
    fn negotiates_protocol_version() {
        assert_eq!(negotiate_version(Some("2025-03-26")), "2025-03-26");
        assert_eq!(negotiate_version(Some("2024-11-05")), "2024-11-05");
        assert_eq!(negotiate_version(Some("1999-01-01")), PROTOCOL_VERSIONS[0]);
        assert_eq!(negotiate_version(None), PROTOCOL_VERSIONS[0]);
    }

    #[test]
    fn lists_annotated_tools() {
        let tools = tools();
        let list = tools["tools"].as_array().unwrap();
        assert!(list.iter().all(|t| t["annotations"].is_object()));
        assert!(list.iter().any(|t| t["name"] == "install_modpack"));
    }

    #[test]
    fn accepts_local_origins_on_our_port() {
        assert!(origin_allowed(Some(&HeaderValue::from_static("http://localhost:8542"))));
        assert!(!origin_allowed(Some(&HeaderValue::from_static("http://localhost:3000"))));
    }

    #[test]
    fn query_token_is_only_accepted_for_mcp_endpoint() {
        let valid: Uri = "/mcp?token=secret".parse().unwrap();
        let api_route: Uri = "/api/instances?token=secret".parse().unwrap();
        let duplicate: Uri = "/mcp?token=secret&token=other".parse().unwrap();
        let empty: Uri = "/mcp?token=".parse().unwrap();

        assert_eq!(query_token(&valid), Some("secret"));
        assert_eq!(query_token(&api_route), None);
        assert_eq!(query_token(&duplicate), None);
        assert_eq!(query_token(&empty), None);
        assert!(request_is_authorized(&valid, None, "secret"));
        assert!(!request_is_authorized(&valid, None, "wrong"));
        assert!(!request_is_authorized(&api_route, None, "secret"));
        assert!(request_is_authorized(&api_route, Some("secret"), "secret"));
    }
}
