use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use open_web_codex_platform_contracts::error::PlatformError;
use open_web_codex_platform_contracts::{CreateTaskRequest, Task};
use open_web_codex_platform_store::AppState;
use serde::Deserialize;
use sqlx::Row;
use uuid::Uuid;

type ApiResult<T> = Result<Json<T>, (StatusCode, Json<PlatformError>)>;

#[derive(Deserialize)]
pub struct ListTasksParams {
    pub project_id: Uuid,
}

/// GET /api/tasks?project_id=...
pub async fn list_tasks(
    State(state): State<AppState>,
    Query(params): Query<ListTasksParams>,
) -> ApiResult<Vec<Task>> {
    let rows = sqlx::query(
        "SELECT id, project_id, title, status, created_at, updated_at \
         FROM tasks WHERE project_id = $1 ORDER BY created_at DESC",
    )
    .bind(params.project_id)
    .fetch_all(&state.db)
    .await
    .map_err(|e| {
        (StatusCode::INTERNAL_SERVER_ERROR, Json(PlatformError::internal(format!("{e}"))))
    })?;

    let tasks: Vec<Task> = rows
        .iter()
        .map(|row| Task {
            id: row.get("id"),
            project_id: row.get("project_id"),
            title: row.get("title"),
            status: row.get("status"),
            created_at: row.get("created_at"),
            updated_at: row.get("updated_at"),
        })
        .collect();

    Ok(Json(tasks))
}

/// POST /api/tasks
pub async fn create_task(
    State(state): State<AppState>,
    Json(req): Json<CreateTaskRequest>,
) -> ApiResult<Task> {
    if req.title.trim().is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(PlatformError::bad_request("title must not be empty")),
        ));
    }

    let row = sqlx::query(
        "INSERT INTO tasks (project_id, title) VALUES ($1, $2) \
         RETURNING id, project_id, title, status, created_at, updated_at",
    )
    .bind(req.project_id)
    .bind(&req.title)
    .fetch_one(&state.db)
    .await
    .map_err(|e| {
        (StatusCode::INTERNAL_SERVER_ERROR, Json(PlatformError::internal(format!("{e}"))))
    })?;

    Ok(Json(Task {
        id: row.get("id"),
        project_id: row.get("project_id"),
        title: row.get("title"),
        status: row.get("status"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    }))
}

/// GET /api/tasks/:id
pub async fn get_task(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> ApiResult<Task> {
    let row = sqlx::query(
        "SELECT id, project_id, title, status, created_at, updated_at \
         FROM tasks WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| {
        (StatusCode::INTERNAL_SERVER_ERROR, Json(PlatformError::internal(format!("{e}"))))
    })?
    .ok_or_else(|| {
        (StatusCode::NOT_FOUND, Json(PlatformError::not_found(format!("task {id} not found"))))
    })?;

    Ok(Json(Task {
        id: row.get("id"),
        project_id: row.get("project_id"),
        title: row.get("title"),
        status: row.get("status"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    }))
}
