//! Browser-safe access to the platform-owned Work State projection.
//!
//! These routes are intentionally limited to creation and reads. Domain Tools
//! mutate Work State through the server-owned service boundary, not through a
//! browser or a Runtime-controlled Thread message.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use open_web_codex_platform_contracts::error::PlatformError;
use open_web_codex_platform_contracts::{
    CreateWorkStateApiRequest, WorkBlockingInputSummary, WorkDeliverableSummary, WorkStateSummary,
};
use open_web_codex_platform_store::AppState;
use open_web_codex_work_state_service::{
    CreateWorkStateRequest, WorkStateActor, WorkStateService, WorkStateServiceError,
};
use uuid::Uuid;

use crate::middleware::auth::AuthenticatedUser;

type ApiResult<T> = Result<Json<T>, (StatusCode, Json<PlatformError>)>;

pub async fn create(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(task_id): Path<Uuid>,
    Json(request): Json<CreateWorkStateApiRequest>,
) -> ApiResult<WorkStateSummary> {
    let profile_id =
        authorized_task_workspace(&state, &auth, task_id, request.workspace_id).await?;
    let service = WorkStateService::new(state.db.clone());
    service
        .create_state(CreateWorkStateRequest {
            actor: WorkStateActor {
                organization_id: auth.organization_id,
                profile_id,
                user_id: auth.user_id,
            },
            workspace_id: request.workspace_id,
            task_id,
            definition: request.definition,
            idempotency_key: request.idempotency_key,
        })
        .await
        .map(Json)
        .map_err(service_error)
}

pub async fn get(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(state_id): Path<Uuid>,
) -> ApiResult<WorkStateSummary> {
    let service = WorkStateService::new(state.db.clone());
    service
        .get_summary(auth.organization_id, state_id)
        .await
        .map(Json)
        .map_err(service_error)
}

pub async fn list_blocking_inputs(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(state_id): Path<Uuid>,
) -> ApiResult<Vec<WorkBlockingInputSummary>> {
    let service = WorkStateService::new(state.db.clone());
    service
        .list_blocking_inputs(auth.organization_id, state_id)
        .await
        .map(Json)
        .map_err(service_error)
}

pub async fn list_deliverables(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(state_id): Path<Uuid>,
) -> ApiResult<Vec<WorkDeliverableSummary>> {
    let service = WorkStateService::new(state.db.clone());
    service
        .list_deliverables(auth.organization_id, state_id)
        .await
        .map(Json)
        .map_err(service_error)
}

async fn authorized_task_workspace(
    state: &AppState,
    auth: &AuthenticatedUser,
    task_id: Uuid,
    workspace_id: Uuid,
) -> Result<Uuid, (StatusCode, Json<PlatformError>)> {
    let profile_id = sqlx::query_scalar(
        "SELECT workspace.profile_id \
         FROM workspaces workspace \
         JOIN workspace_grants workspace_grant \
           ON workspace_grant.workspace_id = workspace.id \
          AND workspace_grant.organization_id = workspace.organization_id \
          AND workspace_grant.profile_id = workspace.profile_id \
         JOIN tasks task ON task.id = $1 AND task.organization_id = workspace.organization_id \
                           AND task.project_id = workspace.project_id \
         WHERE workspace.id = $2 AND workspace.organization_id = $3 \
           AND workspace_grant.user_id = $4 AND workspace_grant.role IN ('owner', 'write') \
           AND workspace.state IN ('ready', 'retained')",
    )
    .bind(task_id)
    .bind(workspace_id)
    .bind(auth.organization_id)
    .bind(auth.user_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|_| internal("work state authorization lookup failed"))?;
    profile_id.ok_or_else(|| forbidden("task or workspace access is not authorized"))
}

fn service_error(error: WorkStateServiceError) -> (StatusCode, Json<PlatformError>) {
    match error {
        WorkStateServiceError::NotFound => not_found("work state was not found"),
        WorkStateServiceError::RevisionConflict | WorkStateServiceError::OperationConflict => {
            conflict("work state changed; reload before retrying")
        }
        WorkStateServiceError::InvalidContract(_) => bad_request("work state request is invalid"),
        WorkStateServiceError::Database(_) | WorkStateServiceError::InvalidStoredData => {
            internal("work state service failed")
        }
    }
}

fn bad_request(message: &str) -> (StatusCode, Json<PlatformError>) {
    (
        StatusCode::BAD_REQUEST,
        Json(PlatformError::bad_request(message)),
    )
}

fn forbidden(message: &str) -> (StatusCode, Json<PlatformError>) {
    (
        StatusCode::FORBIDDEN,
        Json(PlatformError::forbidden(message)),
    )
}

fn not_found(message: &str) -> (StatusCode, Json<PlatformError>) {
    (
        StatusCode::NOT_FOUND,
        Json(PlatformError::not_found(message)),
    )
}

fn conflict(message: &str) -> (StatusCode, Json<PlatformError>) {
    (
        StatusCode::CONFLICT,
        Json(PlatformError {
            kind: open_web_codex_platform_contracts::error::ErrorKind::Conflict,
            message: message.to_string(),
            request_id: None,
            retry_after_ms: None,
        }),
    )
}

fn internal(message: &str) -> (StatusCode, Json<PlatformError>) {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(PlatformError::internal(message)),
    )
}
