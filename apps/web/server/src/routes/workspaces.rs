use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Extension, Json,
};
use open_web_codex_git_runtime::{CommitAuthor, GitRuntime, GitRuntimeError};
use open_web_codex_platform_contracts::error::PlatformError;
use open_web_codex_platform_contracts::{
    CommitWorkspaceRequest, CommitWorkspaceResponse, RunWorkspaceStatus, WorkspaceFileChange,
};
use open_web_codex_platform_store::AppState;
use serde_json::json;
use sqlx::Row;
use uuid::Uuid;

use crate::middleware::auth::AuthenticatedUser;

type ApiResult<T> = Result<Json<T>, (StatusCode, Json<PlatformError>)>;

pub async fn status(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(run_id): Path<Uuid>,
    Extension(git): Extension<Arc<GitRuntime>>,
) -> ApiResult<RunWorkspaceStatus> {
    let workspace_id = authorized_workspace(&state, &auth, run_id, false).await?;
    let status = git.status(workspace_id).await.map_err(git_error)?;
    Ok(Json(RunWorkspaceStatus {
        workspace_id,
        branch: status.branch,
        head_commit: status.head_commit,
        changes: status
            .changes
            .into_iter()
            .map(|change| WorkspaceFileChange {
                path: change.path,
                status: change.status,
                additions: change.additions,
                deletions: change.deletions,
                binary: change.binary,
                size_bytes: change.size_bytes,
                large: change.large,
            })
            .collect(),
    }))
}

pub async fn commit(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(run_id): Path<Uuid>,
    Extension(git): Extension<Arc<GitRuntime>>,
    Json(request): Json<CommitWorkspaceRequest>,
) -> ApiResult<CommitWorkspaceResponse> {
    let workspace_id = authorized_workspace(&state, &auth, run_id, true).await?;
    let author = CommitAuthor {
        name: auth.name.clone(),
        email: auth.email.clone(),
    };
    let commit = git
        .commit_selected(
            workspace_id,
            &request.selected_paths,
            &request.message,
            &author,
        )
        .await
        .map_err(git_error)?;
    let mut transaction = state.db.begin().await.map_err(database_error)?;
    sqlx::query(
        "UPDATE workspaces SET head_commit = $1, updated_at = now() \
         WHERE id = $2 AND organization_id = $3",
    )
    .bind(&commit)
    .bind(workspace_id)
    .bind(auth.organization_id)
    .execute(&mut *transaction)
    .await
    .map_err(database_error)?;
    sqlx::query(
        "INSERT INTO audit_log \
         (organization_id, actor_id, action, target_type, target_id, metadata, outcome) \
         VALUES ($1, $2, 'workspace.commit', 'workspace', $3, $4, 'success')",
    )
    .bind(auth.organization_id)
    .bind(auth.user_id)
    .bind(workspace_id)
    .bind(json!({
        "runId": run_id,
        "commit": commit,
        "selectedPathCount": request.selected_paths.len(),
    }))
    .execute(&mut *transaction)
    .await
    .map_err(database_error)?;
    transaction.commit().await.map_err(database_error)?;
    Ok(Json(CommitWorkspaceResponse {
        workspace_id,
        commit,
    }))
}

async fn authorized_workspace(
    state: &AppState,
    auth: &AuthenticatedUser,
    run_id: Uuid,
    require_owner: bool,
) -> Result<Uuid, (StatusCode, Json<PlatformError>)> {
    let row = sqlx::query(
        "SELECT r.workspace_id, r.requested_by, w.state \
         FROM runs r JOIN workspaces w ON w.id = r.workspace_id \
         WHERE r.id = $1 AND r.organization_id = $2 AND w.organization_id = $2",
    )
    .bind(run_id)
    .bind(auth.organization_id)
    .fetch_optional(&state.db)
    .await
    .map_err(database_error)?
    .ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(PlatformError::not_found("Run workspace was not found")),
        )
    })?;
    if row.get::<String, _>("state") == "retired" {
        return Err((
            StatusCode::CONFLICT,
            Json(PlatformError::bad_request("Run workspace has been retired")),
        ));
    }
    let requested_by: Option<Uuid> = row.get("requested_by");
    let can_manage = requested_by == Some(auth.user_id)
        || matches!(auth.organization_role.as_str(), "owner" | "admin");
    if require_owner && !can_manage {
        return Err((
            StatusCode::NOT_FOUND,
            Json(PlatformError::not_found("Run workspace was not found")),
        ));
    }
    Ok(row.get("workspace_id"))
}

fn git_error(error: GitRuntimeError) -> (StatusCode, Json<PlatformError>) {
    match error {
        GitRuntimeError::InvalidSource(_)
        | GitRuntimeError::InvalidRef(_)
        | GitRuntimeError::UnsafePath(_)
        | GitRuntimeError::Conflict(_)
        | GitRuntimeError::NoChanges => (
            StatusCode::CONFLICT,
            Json(PlatformError::bad_request(
                "Git workspace request was rejected",
            )),
        ),
        GitRuntimeError::Git { .. } | GitRuntimeError::Io { .. } => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(PlatformError::internal("Git workspace operation failed")),
        ),
    }
}

fn database_error(_error: sqlx::Error) -> (StatusCode, Json<PlatformError>) {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(PlatformError::internal("database operation failed")),
    )
}
