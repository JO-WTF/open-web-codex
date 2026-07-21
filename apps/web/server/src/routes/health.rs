use axum::extract::State;
use axum::Json;
use chrono::Utc;
use open_web_codex_platform_contracts::HealthResponse;
use open_web_codex_platform_store::AppState;

/// GET /api/health — returns server status and uptime.
pub async fn health_check(State(state): State<AppState>) -> Json<HealthResponse> {
    let started_at = Utc::now();
    let uptime = state.started_at.elapsed().as_secs();

    Json(HealthResponse {
        ok: true,
        version: env!("CARGO_PKG_VERSION").to_string(),
        started_at,
        uptime_seconds: uptime,
    })
}
