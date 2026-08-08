//! Root read-only coordination projection.
//!
//! Codex remains the owner of Threads, Turns and scheduling. This endpoint
//! only joins durable Platform projections so a Supervisor or Web client can
//! inspect progress without asking a child Agent to report its own status.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use open_web_codex_platform_contracts::error::PlatformError;
use open_web_codex_platform_contracts::{
    CollaborationStatusSummary, CoordinationExecutionSummary, RuntimeAgentExecutionStatus,
};
use open_web_codex_platform_store::AppState;
use open_web_codex_work_state_service::WorkStateService;
use sqlx::Row;
use uuid::Uuid;

use crate::middleware::auth::AuthenticatedUser;

type ApiError = (StatusCode, Json<PlatformError>);
type ApiResult<T> = Result<Json<T>, ApiError>;

pub async fn status(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(run_id): Path<Uuid>,
) -> ApiResult<CollaborationStatusSummary> {
    load_for_organization(&state.db, auth.organization_id, run_id)
        .await
        .map(Json)
}

pub(crate) async fn load_for_organization(
    db: &sqlx::PgPool,
    organization_id: Uuid,
    run_id: Uuid,
) -> Result<CollaborationStatusSummary, ApiError> {
    let task_id: Uuid =
        sqlx::query_scalar("SELECT task_id FROM runs WHERE id = $1 AND organization_id = $2")
            .bind(run_id)
            .bind(organization_id)
            .fetch_optional(db)
            .await
            .map_err(database_error)?
            .ok_or_else(|| not_found("Run was not found"))?;

    let state_id: Option<Uuid> = sqlx::query_scalar(
        "SELECT id FROM work_states WHERE organization_id = $1 AND task_id = $2
         ORDER BY updated_at DESC, id DESC LIMIT 1",
    )
    .bind(organization_id)
    .bind(task_id)
    .fetch_optional(db)
    .await
    .map_err(database_error)?;
    let work_state = match state_id {
        Some(id) => Some(
            WorkStateService::new(db.clone())
                .get_summary(organization_id, id)
                .await
                .map_err(|_| internal("work state projection could not be loaded"))?,
        ),
        None => None,
    };

    let rows = sqlx::query(
        "SELECT id, status, current_behavior, latest_progress, display_title,
                result_summary, wait_cycle_count, waiting_approval_id,
                started_at, completed_at, updated_at
         FROM runtime_agent_execution_projections
         WHERE root_run_id = $1 AND organization_id = $2
         ORDER BY first_observed_sequence, id
         LIMIT 500",
    )
    .bind(run_id)
    .bind(organization_id)
    .fetch_all(db)
    .await
    .map_err(database_error)?;
    let executions = rows
        .into_iter()
        .map(|row| CoordinationExecutionSummary {
            id: row.get("id"),
            display_title: row.get("display_title"),
            status: execution_status(row.get("status")),
            current_behavior: row.get("current_behavior"),
            latest_progress: row.get("latest_progress"),
            result_summary: row.get("result_summary"),
            wait_cycle_count: row.get("wait_cycle_count"),
            waiting_for_input: row.get::<Option<Uuid>, _>("waiting_approval_id").is_some(),
            started_at: row.get("started_at"),
            completed_at: row.get("completed_at"),
            updated_at: row.get("updated_at"),
        })
        .collect::<Vec<_>>();
    let open_user_input_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM approvals
         WHERE organization_id = $1 AND run_id = $2
           AND request_type = 'item/tool/requestUserInput'
           AND state IN ('pending', 'dispatching')",
    )
    .bind(organization_id)
    .bind(run_id)
    .fetch_one(db)
    .await
    .map_err(database_error)?;
    let updated_at = work_state
        .as_ref()
        .map(|summary| summary.updated_at)
        .into_iter()
        .chain(executions.iter().map(|execution| execution.updated_at))
        .max()
        .unwrap_or_else(chrono::Utc::now);
    let deliverables = work_state
        .as_ref()
        .map(|summary| summary.deliverables.clone())
        .unwrap_or_default();
    Ok(CollaborationStatusSummary {
        run_id,
        task_id,
        work_state,
        executions,
        open_user_input_count,
        deliverables,
        updated_at,
    })
}

fn execution_status(value: String) -> RuntimeAgentExecutionStatus {
    match value.as_str() {
        "pending" => RuntimeAgentExecutionStatus::Pending,
        "running" => RuntimeAgentExecutionStatus::Running,
        "waiting" => RuntimeAgentExecutionStatus::Waiting,
        "waiting_for_input" => RuntimeAgentExecutionStatus::WaitingForInput,
        "completed" => RuntimeAgentExecutionStatus::Completed,
        "failed" => RuntimeAgentExecutionStatus::Failed,
        "rejected" => RuntimeAgentExecutionStatus::Rejected,
        "cancelled" => RuntimeAgentExecutionStatus::Cancelled,
        "timeout" => RuntimeAgentExecutionStatus::Timeout,
        "interrupted" => RuntimeAgentExecutionStatus::Interrupted,
        _ => RuntimeAgentExecutionStatus::Failed,
    }
}

fn not_found(message: &str) -> ApiError {
    (
        StatusCode::NOT_FOUND,
        Json(PlatformError::not_found(message)),
    )
}

fn internal(message: &str) -> ApiError {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(PlatformError::internal(message)),
    )
}

fn database_error(_: sqlx::Error) -> ApiError {
    internal("collaboration projection could not be loaded")
}
