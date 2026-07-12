pub mod codex_proxy;
pub mod health;

use std::sync::Arc;

use axum::{Extension, Router};
use open_web_codex_adapter::CodexAdapter;
use open_web_codex_platform_store::AppState;

/// Assemble all platform API routes.
pub fn router(adapter: Arc<dyn CodexAdapter>) -> Router<AppState> {
    Router::new()
        .route("/health", axum::routing::get(health::health_check))
        .route("/rpc", axum::routing::post(codex_proxy::rpc_handler))
        .route("/events", axum::routing::get(codex_proxy::events_handler))
        .layer(Extension(adapter))
}
