use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use open_web_codex_platform_contracts::error::PlatformError;
use open_web_codex_platform_contracts::{SupervisorPolicyBinding, SupervisorPolicySummary};
use open_web_codex_platform_store::AppState;
use sqlx::Row;
use uuid::Uuid;

use crate::middleware::auth::AuthenticatedUser;
use crate::supervisor_policy;

type ApiResult<T> = Result<Json<T>, (StatusCode, Json<PlatformError>)>;

pub async fn list_published(_auth: AuthenticatedUser) -> ApiResult<Vec<SupervisorPolicySummary>> {
    Ok(Json(supervisor_policy::list_published()))
}

pub async fn get_run_binding(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(run_id): Path<Uuid>,
) -> ApiResult<Option<SupervisorPolicyBinding>> {
    let row = sqlx::query(
        "SELECT run.id AS run_id, run.task_id, binding.thread_id, binding.state, \
                binding.created_at, binding.bound_at, snapshot.policy_id, snapshot.version, \
                snapshot.display_name, snapshot.content_sha256 \
         FROM runs run \
         LEFT JOIN supervisor_policy_bindings binding ON binding.run_id = run.id \
           AND binding.organization_id = run.organization_id \
         LEFT JOIN supervisor_policy_snapshots snapshot ON snapshot.id = binding.snapshot_id \
           AND snapshot.organization_id = binding.organization_id \
         WHERE run.id = $1 AND run.organization_id = $2",
    )
    .bind(run_id)
    .bind(auth.organization_id)
    .fetch_optional(&state.db)
    .await
    .map_err(database_error)?
    .ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(PlatformError::not_found("Run was not found")),
        )
    })?;

    let Some(policy_id) = row.get::<Option<String>, _>("policy_id") else {
        return Ok(Json(None));
    };
    Ok(Json(Some(SupervisorPolicyBinding {
        run_id: row.get("run_id"),
        task_id: row.get("task_id"),
        thread_id: row.get("thread_id"),
        policy_id,
        version: row.get("version"),
        display_name: row.get("display_name"),
        content_sha256: row.get("content_sha256"),
        state: row.get("state"),
        created_at: row.get("created_at"),
        bound_at: row.get("bound_at"),
    })))
}

fn database_error(_error: sqlx::Error) -> (StatusCode, Json<PlatformError>) {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(PlatformError::internal("database operation failed")),
    )
}
