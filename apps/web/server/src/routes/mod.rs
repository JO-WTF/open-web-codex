pub mod projects;
pub mod tasks;
pub mod runs;
pub mod approvals;
pub mod bootstrap;
pub mod codex_catalog;
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
        .route(
            "/tasks/{id}/active-run",
            axum::routing::get(tasks::get_active_run),
        )
        .route("/approvals", axum::routing::get(approvals::list_approvals))
        .route(
            "/approvals/{id}/decision",
            axum::routing::post(approvals::decide_approval),
        )
        .route(
            "/approvals/{id}/respond",
            axum::routing::post(approvals::respond_approval),
        )
        .route("/runs", axum::routing::get(runs::list_runs))
        .route("/runs/{id}", axum::routing::get(runs::get_run))
        .route("/runs/{id}/cancel", axum::routing::post(runs::cancel_run))
        .route("/runs/{id}/interrupt", axum::routing::post(runs::interrupt_run))
        .route("/runs/{id}/steer", axum::routing::post(runs::steer_run))
        .route("/runs/{id}/files", axum::routing::get(runs::list_run_files))
        .route(
            "/runs/{id}/files/content",
            axum::routing::get(runs::read_run_file),
        )
        .route(
            "/runs/{id}/git-status",
            axum::routing::get(runs::get_run_git_status),
        )
        .route(
            "/organizations",
            axum::routing::get(organizations::list_organizations)
                .post(organizations::create_organization),
        )
        .route(
            "/organizations/{id}",
            axum::routing::get(organizations::get_organization),
        )
        .route(
            "/organizations/{id}/members",
            axum::routing::get(organizations::list_members)
                .post(organizations::add_member),
        )
        .route("/health", axum::routing::get(health::health_check))
        .route(
            "/projects",
            axum::routing::get(projects::list_projects).post(projects::create_project),
        )
        .route(
            "/projects/{id}",
            axum::routing::get(projects::get_project).delete(projects::delete_project),
        )
        .route(
            "/tasks",
            axum::routing::get(tasks::list_tasks).post(tasks::create_task),
        )
        .route("/tasks/{id}", axum::routing::get(tasks::get_task))
        .route("/tasks/{id}/messages", axum::routing::post(tasks::send_message))
        .route(
            "/tasks/{id}/thread-settings",
            axum::routing::patch(tasks::update_thread_settings),
        )
        .route("/tasks/{id}/events", axum::routing::get(tasks::list_task_events))
        .route(
            "/codex/model-providers",
            axum::routing::get(codex_catalog::list_model_providers),
        )
        .route("/codex/models", axum::routing::get(codex_catalog::list_models))
        .route(
            "/codex/model-providers/write",
            axum::routing::post(codex_catalog::write_model_provider),
        )
        .layer(Extension(adapter))
}
