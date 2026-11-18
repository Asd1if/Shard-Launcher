use axum::{
    Json, Router,
    extract::State,
    http::{StatusCode, header::AUTHORIZATION},
    middleware,
    routing::{get, post},
};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

#[derive(Clone)]
pub struct ApiState {
    pub app_handle: AppHandle,
    pub api_token: String,
}

#[derive(Deserialize)]
pub struct InstanceRequest {
    pub instance_id: String,
}

#[derive(Serialize)]
pub struct ApiResponse {
    pub success: bool,
    pub message: String,
    pub data: Option<serde_json::Value>,
}

async fn auth_middleware(
    State(state): State<ApiState>,
    req: axum::extract::Request,
    next: middleware::Next,
) -> Result<axum::response::Response, StatusCode> {
    let authorized = req
        .headers()
        .get(AUTHORIZATION)
        .and_then(|header| header.to_str().ok())
        .and_then(|header| header.strip_prefix("Bearer "))
        .is_some_and(|token| token == state.api_token);

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

pub fn start_api_server(app_handle: AppHandle, token: String) {
    let state = ApiState {
        app_handle,
        api_token: token,
    };
    let app = Router::new()
        .route("/api/instances", get(get_instances))
        .route("/api/launch", post(launch_instance))
        .route("/api/stop", post(stop_instance))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            auth_middleware,
        ))
        .with_state(state);

    tauri::async_runtime::spawn(async move {
        let listener = match tokio::net::TcpListener::bind("127.0.0.1:8542").await {
            Ok(listener) => listener,
            Err(error) => {
                eprintln!("Failed to bind MCP API server: {error}");
                return;
            }
        };
        eprintln!("Shard Launcher MCP API listening on 127.0.0.1:8542");
        if let Err(error) = axum::serve(listener, app).await {
            eprintln!("MCP API server stopped: {error}");
        }
    });
}
