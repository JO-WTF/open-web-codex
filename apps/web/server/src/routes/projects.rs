use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use open_web_codex_platform_contracts::error::PlatformError;
use open_web_codex_platform_contracts::{CreateProjectRequest, Project};
use open_web_codex_platform_store::AppState;
use sqlx::Row;
use uuid::Uuid;

use crate::access::{default_organization_for_user, ensure_organization_member, ensure_project_access};
use crate::middleware::auth::AuthenticatedUser;

type ApiResult<T> = Result<Json<T>, (StatusCode, Json<PlatformError>)>;

/// GET /api/projects
pub async fn list_projects(
    auth: AuthenticatedUser,
    State(state): State<AppState>,
) -> ApiResult<Vec<Project>> {
    let rows = sqlx::query(
        "SELECT p.id, p.organization_id, p.name, p.git_url, p.default_branch, p.created_at, p.updated_at
         FROM projects p
         JOIN memberships m
           ON m.organization_id = p.organization_id
          AND m.user_id = $1
         ORDER BY p.created_at DESC",
    )
    .bind(auth.user_id)
    .fetch_all(&state.db)
    .await
    .map_err(|e| {
        let msg = format!("db error: {e}");
        (StatusCode::INTERNAL_SERVER_ERROR, Json(PlatformError::internal(msg)))
    })?;

    Ok(Json(rows.iter().map(map_project_row).collect()))
}

/// POST /api/projects
pub async fn create_project(
    auth: AuthenticatedUser,
    State(state): State<AppState>,
    Json(req): Json<CreateProjectRequest>,
) -> ApiResult<Project> {
    if req.name.trim().is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(PlatformError::bad_request("name must not be empty")),
        ));
    }
    if req.git_url.trim().is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(PlatformError::bad_request("git_url must not be empty")),
        ));
    }
    crate::git_workspace::validate_git_url(&req.git_url).map_err(|error| {
        (
            StatusCode::BAD_REQUEST,
            Json(PlatformError::bad_request(error)),
        )
    })?;

    let organization_id = match req.organization_id {
        Some(organization_id) => organization_id,
        None => default_organization_for_user(&state.db, auth.user_id).await?,
    };
    ensure_organization_member(&state.db, auth.user_id, organization_id).await?;

    let branch = req.default_branch.unwrap_or_else(|| "main".to_string());
    let row = sqlx::query(
        "INSERT INTO projects (organization_id, name, git_url, default_branch)
         VALUES ($1, $2, $3, $4)
         RETURNING id, organization_id, name, git_url, default_branch, created_at, updated_at",
    )
    .bind(organization_id)
    .bind(&req.name)
    .bind(&req.git_url)
    .bind(&branch)
    .fetch_one(&state.db)
    .await
    .map_err(|e| {
        (StatusCode::INTERNAL_SERVER_ERROR, Json(PlatformError::internal(format!("{e}"))))
    })?;

    Ok(Json(map_project_row(&row)))
}

/// GET /api/projects/:id
pub async fn get_project(
    auth: AuthenticatedUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> ApiResult<Project> {
    ensure_project_access(&state.db, auth.user_id, id).await?;

    let row = sqlx::query(
        "SELECT id, organization_id, name, git_url, default_branch, created_at, updated_at
         FROM projects WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| {
        (StatusCode::INTERNAL_SERVER_ERROR, Json(PlatformError::internal(format!("{e}"))))
    })?
    .ok_or_else(|| {
        (StatusCode::NOT_FOUND, Json(PlatformError::not_found(format!("project {id} not found"))))
    })?;

    Ok(Json(map_project_row(&row)))
}

/// DELETE /api/projects/:id
pub async fn delete_project(
    auth: AuthenticatedUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> ApiResult<serde_json::Value> {
    ensure_project_access(&state.db, auth.user_id, id).await?;

    let deleted = sqlx::query("DELETE FROM projects WHERE id = $1")
        .bind(id)
        .execute(&state.db)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(PlatformError::internal(format!("{e}"))),
            )
        })?;

    if deleted.rows_affected() == 0 {
        return Err((
            StatusCode::NOT_FOUND,
            Json(PlatformError::not_found(format!("project {id} not found"))),
        ));
    }

    Ok(Json(serde_json::json!({ "deleted": true, "id": id })))
}

fn map_project_row(row: &sqlx::postgres::PgRow) -> Project {
    Project {
        id: row.get("id"),
        organization_id: row.get("organization_id"),
        name: row.get("name"),
        git_url: row.get("git_url"),
        default_branch: row.get("default_branch"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    }
}
