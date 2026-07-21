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

use crate::middleware::auth::AuthenticatedUser;

type ApiResult<T> = Result<Json<T>, (StatusCode, Json<PlatformError>)>;

/// GET /api/projects
pub async fn list_projects(
    auth: AuthenticatedUser,
    State(state): State<AppState>,
) -> ApiResult<Vec<Project>> {
    let rows = sqlx::query(
        "SELECT id, name, git_url, default_branch, created_at, updated_at \
         FROM projects WHERE organization_id = $1 ORDER BY created_at DESC",
    )
    .bind(auth.organization_id)
    .fetch_all(&state.db)
    .await
    .map_err(internal_database_error)?;

    let projects: Vec<Project> = rows
        .iter()
        .map(|row| Project {
            id: row.get("id"),
            name: row.get("name"),
            git_url: row.get("git_url"),
            default_branch: row.get("default_branch"),
            created_at: row.get("created_at"),
            updated_at: row.get("updated_at"),
        })
        .collect();

    Ok(Json(projects))
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
    if auth.organization_role != "owner" && auth.organization_role != "admin" {
        return Err((
            StatusCode::FORBIDDEN,
            Json(PlatformError::forbidden(
                "only Organization owners and admins can create projects",
            )),
        ));
    }
    if req.git_url.trim().is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(PlatformError::bad_request("git_url must not be empty")),
        ));
    }

    let branch = req.default_branch.unwrap_or_else(|| "main".to_string());
    let row = sqlx::query(
        "INSERT INTO projects (organization_id, name, git_url, default_branch) VALUES ($1, $2, $3, $4) \
         RETURNING id, name, git_url, default_branch, created_at, updated_at",
    )
    .bind(auth.organization_id)
    .bind(&req.name)
    .bind(&req.git_url)
    .bind(&branch)
    .fetch_one(&state.db)
    .await
    .map_err(internal_database_error)?;

    Ok(Json(Project {
        id: row.get("id"),
        name: row.get("name"),
        git_url: row.get("git_url"),
        default_branch: row.get("default_branch"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    }))
}

/// GET /api/projects/:id
pub async fn get_project(
    auth: AuthenticatedUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> ApiResult<Project> {
    let row = sqlx::query(
        "SELECT id, name, git_url, default_branch, created_at, updated_at \
         FROM projects WHERE id = $1 AND organization_id = $2",
    )
    .bind(id)
    .bind(auth.organization_id)
    .fetch_optional(&state.db)
    .await
    .map_err(internal_database_error)?
    .ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(PlatformError::not_found(format!("project {id} not found"))),
        )
    })?;

    Ok(Json(Project {
        id: row.get("id"),
        name: row.get("name"),
        git_url: row.get("git_url"),
        default_branch: row.get("default_branch"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    }))
}

fn internal_database_error(_error: sqlx::Error) -> (StatusCode, Json<PlatformError>) {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(PlatformError::internal("database operation failed")),
    )
}
