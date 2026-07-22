use axum::extract::State;
use axum::{Extension, Json};
use chrono::Utc;
use open_web_codex_adapter::CodexAdapter;
use open_web_codex_platform_contracts::HealthResponse;
use open_web_codex_platform_store::AppState;
use std::sync::Arc;

/// GET /api/health — returns server status and uptime.
pub async fn health_check(
    State(state): State<AppState>,
    Extension(adapter): Extension<Arc<dyn CodexAdapter>>,
) -> Json<HealthResponse> {
    let started_at = Utc::now();
    let uptime = state.started_at.elapsed().as_secs();
    let runtime_ok = adapter.health().await.is_ok_and(|status| status.ok);

    Json(HealthResponse {
        ok: runtime_ok,
        version: env!("CARGO_PKG_VERSION").to_string(),
        started_at,
        uptime_seconds: uptime,
    })
}
