pub mod projects;
pub mod tasks;
pub mod codex_proxy;
pub mod runs;
pub mod bootstrap;
pub mod me;
pub mod organizations;
pub mod sessions;
pub mod health;

use std::sync::Arc;

use axum::{Extension, Router};
use open_web_codex_adapter::CodexAdapter;
use open_web_codex_platform_store::AppState;

/// Assemble all platform API routes.
pub fn router(adapter: Arc<dyn CodexAdapter>) -> Router<AppState> {
    Router::new()
        .route("/bootstrap", axum::routing::post(bootstrap::bootstrap))
        .route("/sessions", axum::routing::post(sessions::create_session))
        .route("/me", axum::routing::get(me::me))
        .route("/tasks/{id}/runs", axum::routing::post(runs::start_run))
        .route("/runs", axum::routing::get(runs::list_runs))
        .route("/runs/{id}", axum::routing::get(runs::get_run))
        .route("/runs/{id}/cancel", axum::routing::post(runs::cancel_run))
        .route("/organizations", axum::routing::get(organizations::list_organizations).post(organizations::create_organization))
        .route("/organizations/{id}", axum::routing::get(organizations::get_organization))
        .route("/organizations/{id}/members", axum::routing::get(organizations::list_members).post(organizations::add_member))
        .route("/health", axum::routing::get(health::health_check))
        .route("/projects", axum::routing::get(projects::list_projects).post(projects::create_project))
        .route("/projects/{id}", axum::routing::get(projects::get_project))
        .route("/tasks", axum::routing::get(tasks::list_tasks).post(tasks::create_task))
        .route("/tasks/{id}", axum::routing::get(tasks::get_task))
        .route("/tasks/{id}/messages", axum::routing::post(tasks::send_message))
        .route("/tasks/{id}/events", axum::routing::get(tasks::list_task_events))
        .route("/rpc", axum::routing::post(codex_proxy::rpc_handler))
        .route("/events", axum::routing::get(codex_proxy::events_handler))
        .layer(Extension(adapter))
}
