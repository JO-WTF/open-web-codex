use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use open_web_codex_platform_contracts::error::PlatformError;
use open_web_codex_platform_contracts::{
    SupervisorPolicyBinding, SupervisorPolicyDetail, SupervisorPolicySelection,
    SupervisorPolicySummary,
};
use open_web_codex_platform_store::AppState;
use sqlx::Row;
use uuid::Uuid;

use crate::middleware::auth::AuthenticatedUser;
use crate::supervisor_policy;

type ApiResult<T> = Result<Json<T>, (StatusCode, Json<PlatformError>)>;

pub async fn list_published(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
) -> ApiResult<Vec<SupervisorPolicySummary>> {
    supervisor_policy::list_published(&state.db, auth.organization_id)
        .await
        .map(Json)
        .map_err(|error| {
            tracing::error!(?error, "failed to build Supervisor catalog");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(PlatformError::internal("Supervisor catalog is invalid")),
            )
        })
}

pub async fn get_published(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path((policy_id, version)): Path<(String, String)>,
) -> ApiResult<SupervisorPolicyDetail> {
    supervisor_policy::resolve(
        &state.db,
        auth.organization_id,
        &SupervisorPolicySelection { policy_id, version },
    )
    .await
    .map(|policy| Json(policy.detail))
    .map_err(|error| match error {
        supervisor_policy::SupervisorPolicyError::NotFound => (
            StatusCode::NOT_FOUND,
            Json(PlatformError::not_found("Supervisor Policy was not found")),
        ),
        supervisor_policy::SupervisorPolicyError::Invalid(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(PlatformError::internal(
                "published Supervisor Policy is invalid",
            )),
        ),
        supervisor_policy::SupervisorPolicyError::Capability(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(PlatformError::internal(
                "Supervisor Policy Runtime requirements are unavailable",
            )),
        ),
        supervisor_policy::SupervisorPolicyError::Database => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(PlatformError::internal(
                "Supervisor Policy database operation failed",
            )),
        ),
    })
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
