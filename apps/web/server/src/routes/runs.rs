use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Extension, Json,
};
use open_web_codex_platform_contracts::error::PlatformError;
use open_web_codex_platform_contracts::{Run, StartRunRequest, StartRunResponse};
use open_web_codex_platform_store::AppState;
use open_web_codex_run_orchestrator::{
    CancelRunRequest, EnqueueRunRequest, RunOrchestrator, RunOrchestratorError, RunRecord,
};
use sqlx::Row;
use uuid::Uuid;

use crate::middleware::auth::{require_runtime_profile, AuthenticatedUser};
use crate::routes::RuntimeProfileBinding;

type ApiResult<T> = Result<Json<T>, (StatusCode, Json<PlatformError>)>;

/// Queue a Run. A worker owns all Git and Runtime side effects after this
/// transaction, so retries are safe when the caller reuses its idempotency key.
pub async fn start_run(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(task_id): Path<Uuid>,
    Extension(orchestrator): Extension<Arc<RunOrchestrator>>,
    Extension(profile): Extension<RuntimeProfileBinding>,
    Json(req): Json<StartRunRequest>,
) -> ApiResult<StartRunResponse> {
    require_runtime_profile(&state.db, &auth, &profile.runtime_key).await?;
    let run = orchestrator
        .enqueue_run(EnqueueRunRequest {
            organization_id: auth.organization_id,
            actor_id: auth.user_id,
            task_id,
            idempotency_key: req.idempotency_key,
            git_ref: req.git_ref,
        })
        .await
        .map_err(orchestrator_error)?;
    Ok(Json(StartRunResponse {
        run: run_from_record(run),
    }))
}

/// GET /api/runs?task_id=... — list runs for a task.
pub async fn list_runs(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, Uuid>>,
) -> ApiResult<Vec<Run>> {
    let task_id = params.get("task_id").copied();
    let rows = if let Some(task_id) = task_id {
        sqlx::query(
            "SELECT id, task_id, status, codex_thread_id, active_turn_id, workspace_id, \
                    source_ref, attempt, created_at, updated_at FROM runs \
             WHERE task_id = $1 AND organization_id = $2 ORDER BY created_at DESC",
        )
        .bind(task_id)
        .bind(auth.organization_id)
        .fetch_all(&state.db)
        .await
        .map_err(database_error)?
    } else {
        sqlx::query(
            "SELECT id, task_id, status, codex_thread_id, active_turn_id, workspace_id, \
                    source_ref, attempt, created_at, updated_at FROM runs \
             WHERE organization_id = $1 ORDER BY created_at DESC",
        )
        .bind(auth.organization_id)
        .fetch_all(&state.db)
        .await
        .map_err(database_error)?
    };
    Ok(Json(rows.iter().map(run_from_row).collect()))
}

/// GET /api/runs/:id — get a single run.
pub async fn get_run(
    auth: AuthenticatedUser,
    Path(id): Path<Uuid>,
    Extension(orchestrator): Extension<Arc<RunOrchestrator>>,
) -> ApiResult<Run> {
    let run = orchestrator
        .get_run(auth.organization_id, id)
        .await
        .map_err(orchestrator_error)?;
    Ok(Json(run_from_record(run)))
}

/// Cancel a Run and interrupt its projected active Turn when one exists.
pub async fn cancel_run(
    auth: AuthenticatedUser,
    Path(id): Path<Uuid>,
    Extension(orchestrator): Extension<Arc<RunOrchestrator>>,
) -> ApiResult<Run> {
    let run = orchestrator
        .cancel_run(CancelRunRequest {
            organization_id: auth.organization_id,
            actor_id: auth.user_id,
            allow_organization_admin: matches!(auth.organization_role.as_str(), "owner" | "admin"),
            run_id: id,
        })
        .await
        .map_err(orchestrator_error)?;
    Ok(Json(run_from_record(run)))
}

fn run_from_record(run: RunRecord) -> Run {
    Run {
        id: run.id,
        task_id: run.task_id,
        status: run.status,
        codex_thread_id: run.codex_thread_id,
        active_turn_id: run.active_turn_id,
        workspace_id: run.workspace_id,
        source_ref: run.source_ref,
        attempt: run.attempt,
        created_at: run.created_at,
        updated_at: run.updated_at,
    }
}

fn run_from_row(row: &sqlx::postgres::PgRow) -> Run {
    Run {
        id: row.get("id"),
        task_id: row.get("task_id"),
        status: row.get("status"),
        codex_thread_id: row.get("codex_thread_id"),
        active_turn_id: row.get("active_turn_id"),
        workspace_id: row.get("workspace_id"),
        source_ref: row.get("source_ref"),
        attempt: row.get("attempt"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    }
}

pub(crate) fn orchestrator_error(error: RunOrchestratorError) -> (StatusCode, Json<PlatformError>) {
    let (status, platform) = match error {
        RunOrchestratorError::Invalid(message) => {
            (StatusCode::BAD_REQUEST, PlatformError::bad_request(message))
        }
        RunOrchestratorError::NotFound => (
            StatusCode::NOT_FOUND,
            PlatformError::not_found("Run resource was not found"),
        ),
        RunOrchestratorError::Conflict(message) => {
            (StatusCode::CONFLICT, PlatformError::bad_request(message))
        }
        RunOrchestratorError::Adapter(_) => (
            StatusCode::BAD_GATEWAY,
            PlatformError::internal("Codex Runtime operation failed"),
        ),
        RunOrchestratorError::LeaseLost => (
            StatusCode::CONFLICT,
            PlatformError::bad_request("Run ownership changed; reload its current state"),
        ),
        RunOrchestratorError::Database(_) | RunOrchestratorError::Git(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            PlatformError::internal("Run operation failed"),
        ),
    };
    (status, Json(platform))
}

fn database_error(_error: sqlx::Error) -> (StatusCode, Json<PlatformError>) {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(PlatformError::internal("database operation failed")),
    )
}
