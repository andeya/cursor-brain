//! HTTP layer: routes (chat, models, health, agent), unified error body, request-id middleware.
//! Delegates completion to service; uses config, session store, and cursor for agent subprocess.

use crate::config::{default_session_file_path, Config};
use crate::cursor::{cursor_agent_version, list_models_via_agent, run_agent_subcommand};
use crate::openai::{
    build_completion_response, sse_chunk, sse_chunk_reasoning, sse_done, ChatCompletionRequest,
};
use crate::service::{CompletionError, CompletionInput, CompletionService};
use crate::session::{PersistentSessionStore, SessionStore};
use axum::{
    body::Body,
    extract::{Path, State},
    http::{HeaderMap, HeaderValue, Request},
    middleware::Next,
    response::Response,
    routing::{get, post},
    Json, Router,
};
use bytes::Bytes;
use std::num::NonZeroUsize;
use std::sync::Arc;
use std::time::Instant;

/// Service version (from Cargo or constant).
pub const CURSOR_BRAIN_VERSION: &str = env!("CARGO_PKG_VERSION", "cursor_brain version");

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Config>,
    pub completion_service: Arc<CompletionService>,
    pub metrics: Arc<crate::metrics::Metrics>,
}

#[derive(serde::Serialize)]
pub struct ErrorBody {
    error: ErrorDetail,
}

#[derive(serde::Serialize)]
pub struct ErrorDetail {
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "type")]
    type_: Option<String>,
}

pub fn err_response(
    status: axum::http::StatusCode,
    message: &str,
    code: Option<&str>,
    type_: Option<&str>,
) -> (axum::http::StatusCode, Json<ErrorBody>) {
    (
        status,
        Json(ErrorBody {
            error: ErrorDetail {
                message: message.to_string(),
                code: code.map(String::from),
                type_: type_.map(String::from),
            },
        }),
    )
}

fn completion_error_to_http(e: CompletionError) -> (axum::http::StatusCode, Json<ErrorBody>) {
    match e {
        CompletionError::CursorNotFound => err_response(
            axum::http::StatusCode::SERVICE_UNAVAILABLE,
            "cursor-agent not found. Set cursor_path in ~/.cursor-brain/config.json or ensure Cursor is installed.",
            Some("cursor_not_found"),
            Some("service_unavailable"),
        ),
        CompletionError::InvalidRequest(msg) => err_response(
            axum::http::StatusCode::BAD_REQUEST,
            &msg,
            Some("invalid_request"),
            Some("invalid_request_error"),
        ),
        CompletionError::NoContent => err_response(
            axum::http::StatusCode::SERVICE_UNAVAILABLE,
            "cursor-agent returned no content. Consider increasing request_timeout_sec.",
            Some("no_response"),
            Some("service_unavailable"),
        ),
        CompletionError::SpawnFailed(io) => err_response(
            axum::http::StatusCode::SERVICE_UNAVAILABLE,
            &format!("Failed to start cursor-agent: {}", io),
            Some("spawn_failed"),
            Some("service_unavailable"),
        ),
        CompletionError::JoinFailed(msg) => err_response(
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            &msg,
            None,
            Some("internal_error"),
        ),
    }
}

async fn chat_completions(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<ChatCompletionRequest>,
) -> Result<Response, (axum::http::StatusCode, Json<ErrorBody>)> {
    let default_model = state.config.default_model.as_deref().unwrap_or("auto");
    let input = CompletionInput::from_request(
        &body,
        &headers,
        &state.config.session_header_name,
        default_model,
    )
    .map_err(completion_error_to_http)?;

    if input.stream {
        let (id, model_owned, mut rx) = state
            .completion_service
            .complete_stream(input)
            .await
            .map_err(completion_error_to_http)?;
        let stream = async_stream::stream! {
            while let Some(delta) = rx.recv().await {
                match delta {
                    crate::cursor::StreamDelta::Content(s) => {
                        let chunk = sse_chunk(&id, &model_owned, Some(&s), None);
                        yield Ok::<_, std::convert::Infallible>(Bytes::from(chunk));
                    }
                    crate::cursor::StreamDelta::ReasoningContent(s) => {
                        let chunk = sse_chunk_reasoning(&id, &model_owned, &s);
                        yield Ok::<_, std::convert::Infallible>(Bytes::from(chunk));
                    }
                    crate::cursor::StreamDelta::Done { finish_reason } => {
                        let chunk = sse_chunk(&id, &model_owned, None, Some(&finish_reason));
                        yield Ok(Bytes::from(chunk));
                        yield Ok(Bytes::from(sse_done()));
                        break;
                    }
                }
            }
        };
        return Ok(Response::builder()
            .status(axum::http::StatusCode::OK)
            .header("Content-Type", "text/event-stream")
            .header("Cache-Control", "no-cache")
            .header("Connection", "keep-alive")
            .body(Body::from_stream(stream))
            .unwrap());
    }

    let (out, model_owned, id) = state
        .completion_service
        .complete(input)
        .await
        .map_err(completion_error_to_http)?;
    let resp = build_completion_response(&id, &model_owned, &out, &state.config.forward_thinking);
    Ok(Response::builder()
        .status(axum::http::StatusCode::OK)
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_string(&resp).unwrap_or_default()))
        .unwrap())
}

const LIST_MODELS_TIMEOUT_SECS: u64 = 15;

async fn list_models(State(state): State<AppState>) -> Json<serde_json::Value> {
    let config = state.config.clone();
    let metrics = state.metrics.clone();
    let ids: Vec<String> = match config.resolve_cursor_path() {
        Some(cursor_path) => {
            let list = tokio::time::timeout(
                std::time::Duration::from_secs(LIST_MODELS_TIMEOUT_SECS),
                tokio::task::spawn_blocking(move || list_models_via_agent(&cursor_path)),
            )
            .await;
            let list = match &list {
                Ok(Ok(l)) => {
                    metrics.inc_cursor_ok();
                    l.clone()
                }
                Ok(Err(_)) => {
                    metrics.inc_cursor_fail();
                    Vec::new()
                }
                Err(_) => {
                    metrics.inc_cursor_timeout();
                    Vec::new()
                }
            };
            if list.is_empty() {
                crate::config::DEFAULT_MODELS_LIST
                    .iter()
                    .map(|s| (*s).to_string())
                    .collect()
            } else {
                list
            }
        }
        None => {
            state.metrics.inc_cursor_fail();
            crate::config::DEFAULT_MODELS_LIST
                .iter()
                .map(|s| (*s).to_string())
                .collect()
        }
    };
    let data: Vec<serde_json::Value> = ids
        .iter()
        .map(|id| {
            serde_json::json!({
                "id": id,
                "object": "model",
                "created": 0
            })
        })
        .collect();
    Json(serde_json::json!({
        "object": "list",
        "data": data
    }))
}

async fn get_model_by_id(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, (axum::http::StatusCode, Json<ErrorBody>)> {
    let config = state.config.clone();
    let ids: Vec<String> = match config.resolve_cursor_path() {
        Some(cursor_path) => {
            let list = tokio::time::timeout(
                std::time::Duration::from_secs(LIST_MODELS_TIMEOUT_SECS),
                tokio::task::spawn_blocking(move || list_models_via_agent(&cursor_path)),
            )
            .await
            .ok()
            .and_then(|r| r.ok())
            .unwrap_or_default();
            if list.is_empty() {
                crate::config::DEFAULT_MODELS_LIST
                    .iter()
                    .map(|s| (*s).to_string())
                    .collect()
            } else {
                list
            }
        }
        None => crate::config::DEFAULT_MODELS_LIST
            .iter()
            .map(|s| (*s).to_string())
            .collect(),
    };
    if ids.contains(&id) {
        Ok(Json(serde_json::json!({
            "id": id,
            "object": "model",
            "created": 0
        })))
    } else {
        Err(err_response(
            axum::http::StatusCode::NOT_FOUND,
            "The model requested was not found.",
            Some("model_not_found"),
            Some("invalid_request_error"),
        ))
    }
}

async fn health(State(state): State<AppState>) -> Json<serde_json::Value> {
    let cursor_ok = state.config.resolve_cursor_path().is_some();
    let cursor_agent_version: Option<String> = state
        .config
        .resolve_cursor_path()
        .and_then(|p| cursor_agent_version(&p));
    Json(serde_json::json!({
        "status": if cursor_ok { "ok" } else { "degraded" },
        "cursor": cursor_ok,
        "port": state.config.port,
        "session_storage": "file",
        "cursor_agent_version": cursor_agent_version.unwrap_or_else(|| "unknown".to_string()),
        "cursor_brain_version": CURSOR_BRAIN_VERSION
    }))
}

async fn not_found() -> (axum::http::StatusCode, Json<ErrorBody>) {
    err_response(
        axum::http::StatusCode::NOT_FOUND,
        "The requested resource was not found.",
        Some("not_found"),
        Some("invalid_request_error"),
    )
}

/// GET /v1/version — cursor-agent --version as JSON.
async fn version(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, (axum::http::StatusCode, Json<ErrorBody>)> {
    let path = state.config.resolve_cursor_path().ok_or_else(|| {
        err_response(
            axum::http::StatusCode::SERVICE_UNAVAILABLE,
            "cursor-agent not available.",
            Some("cursor_not_found"),
            Some("service_unavailable"),
        )
    })?;
    let ver = tokio::task::spawn_blocking(move || cursor_agent_version(&path))
        .await
        .ok()
        .flatten()
        .unwrap_or_else(|| "unknown".to_string());
    Ok(Json(serde_json::json!({ "cursor_agent_version": ver })))
}

/// POST /v1/embeddings — 501 Not Implemented.
async fn embeddings_501() -> (axum::http::StatusCode, Json<ErrorBody>) {
    err_response(
        axum::http::StatusCode::NOT_IMPLEMENTED,
        "Embeddings are not supported. cursor-agent does not provide this capability.",
        Some("not_implemented"),
        Some("api_error"),
    )
}

/// POST /v1/completions — 501 Not Implemented (legacy endpoint).
async fn completions_501() -> (axum::http::StatusCode, Json<ErrorBody>) {
    err_response(
        axum::http::StatusCode::NOT_IMPLEMENTED,
        "Legacy completions endpoint is not supported. Use POST /v1/chat/completions instead.",
        Some("not_implemented"),
        Some("api_error"),
    )
}

const AGENT_SUBCOMMAND_TIMEOUT_SECS: u64 = 15;

async fn agent_subcommand_json(
    state: AppState,
    subcommand: &str,
    args: &[&str],
) -> Result<Json<serde_json::Value>, (axum::http::StatusCode, Json<ErrorBody>)> {
    let path = state.config.resolve_cursor_path().ok_or_else(|| {
        err_response(
            axum::http::StatusCode::SERVICE_UNAVAILABLE,
            "cursor-agent not available.",
            Some("cursor_not_found"),
            Some("service_unavailable"),
        )
    })?;
    let sub = subcommand.to_string();
    let args_vec: Vec<String> = args.iter().map(|s| (*s).to_string()).collect();
    let (stdout, stderr) = tokio::time::timeout(
        std::time::Duration::from_secs(AGENT_SUBCOMMAND_TIMEOUT_SECS),
        tokio::task::spawn_blocking(move || {
            run_agent_subcommand(
                &path,
                &sub,
                &args_vec.iter().map(String::as_str).collect::<Vec<_>>(),
            )
        }),
    )
    .await
    .map_err(|_| {
        err_response(
            axum::http::StatusCode::GATEWAY_TIMEOUT,
            "cursor-agent subcommand timed out.",
            Some("timeout"),
            Some("server_error"),
        )
    })?
    .map_err(|_| {
        err_response(
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            "cursor-agent subcommand failed.",
            Some("internal_error"),
            Some("server_error"),
        )
    })?;
    let body = if stdout.is_empty() && !stderr.is_empty() {
        serde_json::json!({ "raw": stderr })
    } else {
        serde_json::json!({ "raw": stdout })
    };
    Ok(Json(body))
}

async fn agent_about(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, (axum::http::StatusCode, Json<ErrorBody>)> {
    agent_subcommand_json(state, "about", &[]).await
}

async fn agent_status(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, (axum::http::StatusCode, Json<ErrorBody>)> {
    agent_subcommand_json(state, "status", &[]).await
}

async fn agent_sessions(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, (axum::http::StatusCode, Json<ErrorBody>)> {
    agent_subcommand_json(state, "ls", &[]).await
}

async fn agent_create_chat(
    State(state): State<AppState>,
) -> Result<
    (axum::http::StatusCode, Json<serde_json::Value>),
    (axum::http::StatusCode, Json<ErrorBody>),
> {
    let path = state.config.resolve_cursor_path().ok_or_else(|| {
        err_response(
            axum::http::StatusCode::SERVICE_UNAVAILABLE,
            "cursor-agent not available.",
            Some("cursor_not_found"),
            Some("service_unavailable"),
        )
    })?;
    let path_clone = path.clone();
    let (stdout, _) = tokio::time::timeout(
        std::time::Duration::from_secs(AGENT_SUBCOMMAND_TIMEOUT_SECS),
        tokio::task::spawn_blocking(move || run_agent_subcommand(&path_clone, "create-chat", &[])),
    )
    .await
    .map_err(|_| {
        err_response(
            axum::http::StatusCode::GATEWAY_TIMEOUT,
            "cursor-agent create-chat timed out.",
            Some("timeout"),
            Some("server_error"),
        )
    })?
    .map_err(|_| {
        err_response(
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            "cursor-agent create-chat failed.",
            Some("internal_error"),
            Some("server_error"),
        )
    })?;
    let id = stdout.trim().to_string();
    if id.is_empty() {
        return Err(err_response(
            axum::http::StatusCode::SERVICE_UNAVAILABLE,
            "cursor-agent create-chat returned no id.",
            Some("no_response"),
            Some("server_error"),
        ));
    }
    Ok((
        axum::http::StatusCode::CREATED,
        Json(serde_json::json!({ "id": id })),
    ))
}

async fn metrics_handler(State(state): State<AppState>) -> Json<serde_json::Value> {
    let (total, cursor_ok, cursor_fail, cursor_timeout) = state.metrics.snapshot();
    Json(serde_json::json!({
        "requests_total": total,
        "cursor_calls_ok": cursor_ok,
        "cursor_calls_fail": cursor_fail,
        "cursor_calls_timeout": cursor_timeout
    }))
}

async fn request_id_and_log(
    State(state): State<AppState>,
    request: Request<Body>,
    next: Next,
) -> Response {
    state.metrics.inc_requests();
    let request_id = request
        .headers()
        .get("x-request-id")
        .and_then(|v| v.to_str().ok())
        .filter(|s| !s.trim().is_empty())
        .map(String::from)
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    let method = request.method().clone();
    let path = request.uri().path().to_string();
    let start = Instant::now();
    let response = next.run(request).await;
    let status = response.status();
    let elapsed_ms = start.elapsed().as_millis();
    tracing::info!(
        request_id = %request_id,
        method = %method,
        path = %path,
        status = %status,
        elapsed_ms = %elapsed_ms,
        "request"
    );
    let hv = match HeaderValue::try_from(request_id.as_str()) {
        Ok(v) => v,
        Err(_) => {
            tracing::warn!(
                request_id = %request_id,
                "x-request-id header value invalid, omitting"
            );
            return response;
        }
    };
    let (mut parts, body) = response.into_parts();
    parts.headers.insert("x-request-id", hv);
    Response::from_parts(parts, body)
}

pub fn app(config: Arc<Config>) -> Router {
    let cap = NonZeroUsize::new(config.session_cache_max as usize).unwrap_or(NonZeroUsize::MIN);
    let session_store: Arc<dyn SessionStore> = Arc::new(PersistentSessionStore::new(
        default_session_file_path(),
        cap,
    ));
    let completion_service = Arc::new(CompletionService::new(config.clone(), session_store));
    let metrics = Arc::new(crate::metrics::Metrics::default());
    let state = AppState {
        config,
        completion_service,
        metrics: metrics.clone(),
    };
    Router::new()
        .route("/v1/chat/completions", post(chat_completions))
        .route("/v1/models", get(list_models))
        .route("/v1/models/:id", get(get_model_by_id))
        .route("/v1/health", get(health))
        .route("/v1/version", get(version))
        .route("/v1/agent/about", get(agent_about))
        .route("/v1/agent/status", get(agent_status))
        .route("/v1/agent/sessions", get(agent_sessions))
        .route("/v1/agent/chats", post(agent_create_chat))
        .route("/v1/metrics", get(metrics_handler))
        .route("/v1/embeddings", post(embeddings_501))
        .route("/v1/completions", post(completions_501))
        .fallback(not_found)
        .with_state(state.clone())
        .layer(axum::middleware::from_fn_with_state(
            state,
            request_id_and_log,
        ))
}
