use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Extension, Json,
};
use open_web_codex_platform_contracts::error::PlatformError;
use open_web_codex_platform_contracts::{CreateTaskRequest, ListTaskEventsParams, RunEvent, SendMessageRequest, SendMessageResponse, Task};
use open_web_codex_platform_store::AppState;
use open_web_codex_adapter::CodexAdapter;
use serde::Deserialize;
use serde_json::json;
use sqlx::Row;
use std::sync::Arc;
use uuid::Uuid;

use crate::middleware::auth::AuthenticatedUser;

type ApiResult<T> = Result<Json<T>, (StatusCode, Json<PlatformError>)>;

#[derive(Deserialize)]
pub struct ListTasksParams {
    pub project_id: Uuid,
}

/// GET /api/tasks?project_id=...
pub async fn list_tasks(
    _auth: AuthenticatedUser,
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
    _auth: AuthenticatedUser,
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

/// GET /api/tasks/:id/events — list persisted run events for a task.
pub async fn list_task_events(
    _auth: AuthenticatedUser,
    State(state): State<AppState>,
    Path(task_id): Path<Uuid>,
    Query(params): Query<ListTaskEventsParams>,
) -> ApiResult<Vec<RunEvent>> {
    let limit = params.limit.unwrap_or(50).min(200);
    let query = match params.after_id {
        Some(after) => sqlx::query(
            "SELECT e.id, e.run_id, e.event_type, e.payload, e.created_at \
             FROM run_events e \
             JOIN runs r ON r.id = e.run_id \
             WHERE r.task_id = $1 AND e.created_at < (SELECT created_at FROM run_events WHERE id = $2) \
             ORDER BY e.created_at DESC LIMIT $3",
        )
        .bind(task_id)
        .bind(after)
        .bind(limit),
        None => sqlx::query(
            "SELECT e.id, e.run_id, e.event_type, e.payload, e.created_at \
             FROM run_events e \
             JOIN runs r ON r.id = e.run_id \
             WHERE r.task_id = $1 \
             ORDER BY e.created_at DESC LIMIT $2",
        )
        .bind(task_id)
        .bind(limit),
    };

    let rows = query
        .fetch_all(&state.db)
        .await
        .map_err(|e| {
            (StatusCode::INTERNAL_SERVER_ERROR, Json(PlatformError::internal(format!("{e}"))))
        })?;

    let events: Vec<RunEvent> = rows.iter().map(|row| {
        let payload: serde_json::Value = row.get("payload");
        RunEvent {
            id: row.get("id"),
            run_id: row.get("run_id"),
            event_type: row.get("event_type"),
            payload,
            created_at: row.get("created_at"),
        }
    }).collect();

    Ok(Json(events))
}

/// POST /api/tasks/:id/messages — send a user message to the task's active thread.
pub async fn send_message(
    State(state): State<AppState>,
    _auth: AuthenticatedUser,
    Path(task_id): Path<Uuid>,
    Extension(adapter): Extension<Arc<dyn CodexAdapter>>,
    Json(req): Json<SendMessageRequest>,
) -> ApiResult<SendMessageResponse> {
    if req.text.trim().is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(PlatformError::bad_request("message text must not be empty")),
        ));
    }

    // Find the active run for this task (latest running or pending run)
    let active_run = sqlx::query(
        "SELECT id, codex_thread_id FROM runs \
         WHERE task_id = $1 AND status IN ('pending', 'running') \
         ORDER BY created_at DESC LIMIT 1",
    )
    .bind(task_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| {
        (StatusCode::INTERNAL_SERVER_ERROR, Json(PlatformError::internal(format!("{e}"))))
    })?
    .ok_or_else(|| {
        (StatusCode::BAD_REQUEST, Json(PlatformError::bad_request(
            "no active run for this task; start a run first"
        )))
    })?;

    let thread_id: Option<String> = active_run.get("codex_thread_id");
    let thread_id = thread_id.ok_or_else(|| {
        (StatusCode::BAD_REQUEST, Json(PlatformError::bad_request(
            "active run has no thread yet; try again shortly"
        )))
    })?;

    // Send message via adapter
    let rpc_result = adapter
        .rpc("send_user_message", json!({
            "threadId": &thread_id,
            "text": req.text,
        }))
        .await
        .map_err(|e| {
            (StatusCode::BAD_GATEWAY, Json(PlatformError::internal(format!("adapter error: {e}"))))
        })?;

    let status = rpc_result
        .get("status")
        .and_then(|v| v.as_str())
        .unwrap_or("sent")
        .to_string();

    Ok(Json(SendMessageResponse {
        status,
        thread_id,
    }))
}

/// GET /api/tasks/:id
pub async fn get_task(
    _auth: AuthenticatedUser,
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
