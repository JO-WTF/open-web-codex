use std::sync::Arc;

use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    Extension, Json,
};
use open_web_codex_platform_store::AppState;
use serde_json::Value;

use open_web_codex_adapter::CodexAdapter;

/// POST /api/rpc — dispatch JSON-RPC calls through the CodexAdapter.
pub async fn rpc_handler(
    Extension(adapter): Extension<Arc<dyn CodexAdapter>>,
    Json(body): Json<Value>,
) -> Response {
    let method = body["method"]
        .as_str()
        .unwrap_or("")
        .to_string();
    let params = body.get("params").cloned().unwrap_or(Value::Null);

    match adapter.rpc(&method, params).await {
        Ok(result) => {
            Json(serde_json::json!({ "result": result })).into_response()
        }
        Err(e) => {
            tracing::warn!("RPC '{method}' error: {e}");
            (
                StatusCode::BAD_GATEWAY,
                Json(serde_json::json!({
                    "error": { "message": e.to_string() }
                })),
            )
                .into_response()
        }
    }
}

/// GET /api/events — SSE event stream from the CodexAdapter.
pub async fn events_handler(
    State(state): State<AppState>,
    Extension(_adapter): Extension<Arc<dyn CodexAdapter>>,
) -> Response {
    let rx = state.event_bus.subscribe();

    // Convert broadcast receiver into an SSE stream using unfold
    let stream = futures_util::stream::unfold(rx, |mut rx| async move {
        loop {
            match rx.recv().await {
                Ok(data) => return Some((Ok::<_, std::convert::Infallible>(data), rx)),
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    tracing::warn!(n, "SSE client lagged");
                    continue;
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                    tracing::debug!("SSE event bus closed");
                    return None;
                }
            }
        }
    });

    Response::builder()
        .status(StatusCode::OK)
        .header("content-type", "text/event-stream")
        .header("cache-control", "no-cache")
        .header("connection", "keep-alive")
            .body(axum::body::Body::from_stream(stream))
        .unwrap()
}
