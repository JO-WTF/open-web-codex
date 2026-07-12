pub mod projects;
pub mod tasks;
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
        .route("/projects", axum::routing::get(projects::list_projects).post(projects::create_project))
        .route("/projects/:id", axum::routing::get(projects::get_project))
        .route("/tasks", axum::routing::get(tasks::list_tasks).post(tasks::create_task))
        .route("/tasks/:id", axum::routing::get(tasks::get_task))
        .route("/rpc", axum::routing::post(codex_proxy::rpc_handler))
        .route("/events", axum::routing::get(codex_proxy::events_handler))
        .layer(Extension(adapter))
}
