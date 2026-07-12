use axum::{
    body::Body,
    http::StatusCode,
    response::{IntoResponse, Response},
    Extension, Json,
};
use futures_util::StreamExt;
use serde_json::Value;

/// Configuration for proxying to the existing Tauri daemon.
#[derive(Clone)]
pub struct DaemonProxy {
    pub base_url: String,
    pub client: reqwest::Client,
}

/// POST /api/rpc — proxy JSON-RPC calls to the Tauri daemon.
pub async fn rpc_proxy(
    Extension(daemon): Extension<DaemonProxy>,
    Json(body): Json<Value>,
) -> Response {
    match daemon
        .client
        .post(format!("{}/api/rpc", daemon.base_url))
        .json(&body)
        .send()
        .await
    {
        Ok(resp) => {
            let status = resp.status();
            let content_type = resp
                .headers()
                .get("content-type")
                .and_then(|v| v.to_str().ok())
                .unwrap_or("application/json")
                .to_owned();
            let body = resp.bytes().await.unwrap_or_default();

            let mut response = Response::new(Body::from(body));
            *response.status_mut() = status;
            response
                .headers_mut()
                .insert("content-type", content_type.parse().unwrap());
            response
        }
        Err(e) => {
            tracing::warn!("RPC proxy error: {e}");
            (
                StatusCode::BAD_GATEWAY,
                Json(serde_json::json!({
                    "error": { "message": format!("daemon unreachable: {e}") }
                })),
            )
                .into_response()
        }
    }
}

/// GET /api/events — proxy SSE event stream from the Tauri daemon.
pub async fn events_proxy(
    Extension(daemon): Extension<DaemonProxy>,
) -> Response {
    match daemon
        .client
        .get(format!("{}/api/events", daemon.base_url))
        .send()
        .await
    {
        Ok(resp) => {
            let stream = resp.bytes_stream().map(|result| {
                result
                    .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)
            });
            let body = Body::from_stream(stream);
            Response::builder()
                .status(StatusCode::OK)
                .header("content-type", "text/event-stream")
                .header("cache-control", "no-cache")
                .header("connection", "keep-alive")
                .body(body)
                .unwrap()
        }
        Err(e) => {
            tracing::warn!("Events proxy error: {e}");
            (StatusCode::BAD_GATEWAY, "daemon unreachable").into_response()
        }
    }
}
