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
    _auth: AuthenticatedUser,
    State(state): State<AppState>,
) -> ApiResult<Vec<Project>> {
    let rows = sqlx::query(
        "SELECT id, name, git_url, default_branch, created_at, updated_at \
         FROM projects ORDER BY created_at DESC",
    )
    .fetch_all(&state.db)
    .await
    .map_err(|e| {
        let msg = format!("db error: {e}");
        (StatusCode::INTERNAL_SERVER_ERROR, Json(PlatformError::internal(msg)))
    })?;

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
    _auth: AuthenticatedUser,
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

    let branch = req.default_branch.unwrap_or_else(|| "main".to_string());
    let row = sqlx::query(
        "INSERT INTO projects (name, git_url, default_branch) VALUES ($1, $2, $3) \
         RETURNING id, name, git_url, default_branch, created_at, updated_at",
    )
    .bind(&req.name)
    .bind(&req.git_url)
    .bind(&branch)
    .fetch_one(&state.db)
    .await
    .map_err(|e| {
        (StatusCode::INTERNAL_SERVER_ERROR, Json(PlatformError::internal(format!("{e}"))))
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

/// GET /api/projects/:id
pub async fn get_project(
    _auth: AuthenticatedUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> ApiResult<Project> {
    let row = sqlx::query(
        "SELECT id, name, git_url, default_branch, created_at, updated_at \
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

    Ok(Json(Project {
        id: row.get("id"),
        name: row.get("name"),
        git_url: row.get("git_url"),
        default_branch: row.get("default_branch"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    }))
}
