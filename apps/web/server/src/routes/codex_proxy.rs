use std::convert::Infallible;
use std::sync::Arc;

use axum::{
    body::Body,
    http::StatusCode,
    response::{IntoResponse, Response},
    Extension, Json,
};
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
    Extension(adapter): Extension<Arc<dyn CodexAdapter>>,
) -> Response {
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<Vec<u8>>();

    // Subscribe in background — the adapter pushes SSE frames into tx.
    let adapter_clone = adapter.clone();
    tokio::spawn(async move {
        if let Err(e) = adapter_clone.subscribe_events(tx).await {
            tracing::warn!("events subscription ended: {e}");
        }
    });

    // Convert the mpsc receiver into an axum body stream.
    let stream = futures_util::stream::unfold(rx, |mut rx| async move {
        rx.recv().await.map(|data| (Ok::<_, Infallible>(data), rx))
    });

    Response::builder()
        .status(StatusCode::OK)
        .header("content-type", "text/event-stream")
        .header("cache-control", "no-cache")
        .header("connection", "keep-alive")
        .body(Body::from_stream(stream))
        .unwrap()
}
