use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use open_web_codex_platform_contracts::error::PlatformError;
use open_web_codex_platform_contracts::RuntimeAgentProjection;
use open_web_codex_platform_store::AppState;
use sqlx::Row;
use uuid::Uuid;

use crate::middleware::auth::AuthenticatedUser;

type ApiResult<T> = Result<Json<T>, (StatusCode, Json<PlatformError>)>;

/// Return the safe, rebuildable Runtime Thread tree associated with one Run.
///
/// Codex owns these Threads. The projection is deliberately read-only and
/// cannot be used as an alternative Agent lifecycle API.
pub async fn list_for_run(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(run_id): Path<Uuid>,
) -> ApiResult<Vec<RuntimeAgentProjection>> {
    let run_exists = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(
            SELECT 1 FROM runs WHERE id = $1 AND organization_id = $2
         )",
    )
    .bind(run_id)
    .bind(auth.organization_id)
    .fetch_one(&state.db)
    .await
    .map_err(database_error)?;
    if !run_exists {
        return Err((
            StatusCode::NOT_FOUND,
            Json(PlatformError::not_found("Run was not found")),
        ));
    }

    let rows = sqlx::query(
        "SELECT root_run_id, thread_id, parent_thread_id, source_kind, agent_path,
                agent_nickname, agent_role, status_type, active_flags,
                first_observed_at, last_observed_at
         FROM runtime_agent_projections
         WHERE root_run_id = $1 AND organization_id = $2
         ORDER BY (parent_thread_id IS NOT NULL), first_observed_at, thread_id",
    )
    .bind(run_id)
    .bind(auth.organization_id)
    .fetch_all(&state.db)
    .await
    .map_err(database_error)?;

    Ok(Json(
        rows.into_iter()
            .map(|row| RuntimeAgentProjection {
                run_id: row.get("root_run_id"),
                thread_id: row.get("thread_id"),
                parent_thread_id: row.get("parent_thread_id"),
                source_kind: row.get("source_kind"),
                agent_path: row.get("agent_path"),
                agent_nickname: row.get("agent_nickname"),
                agent_role: row.get("agent_role"),
                status_type: row.get("status_type"),
                active_flags: row.get("active_flags"),
                is_root: row.get::<Option<String>, _>("parent_thread_id").is_none(),
                first_observed_at: row.get("first_observed_at"),
                last_observed_at: row.get("last_observed_at"),
            })
            .collect(),
    ))
}

fn database_error(_error: sqlx::Error) -> (StatusCode, Json<PlatformError>) {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(PlatformError::internal("database operation failed")),
    )
}
