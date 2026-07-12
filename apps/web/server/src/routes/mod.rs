pub mod codex_proxy;
pub mod health;

use axum::{Extension, Router};
use open_web_codex_platform_store::AppState;

use crate::routes::codex_proxy::DaemonProxy;

/// Assemble all platform API routes.
pub fn router(daemon_proxy: DaemonProxy) -> Router<AppState> {
    Router::new()
        .route("/health", axum::routing::get(health::health_check))
        .route("/rpc", axum::routing::post(codex_proxy::rpc_proxy))
        .route("/events", axum::routing::get(codex_proxy::events_proxy))
        .layer(Extension(daemon_proxy))
}
