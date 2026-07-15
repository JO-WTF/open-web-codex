use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Extension, Json,
};
use open_web_codex_adapter::CodexAdapter;
use open_web_codex_platform_contracts::error::PlatformError;
use open_web_codex_platform_contracts::{
    CreateTaskRequest, ActiveRunResponse, ListTaskEventsParams, RunEvent, SendMessageRequest,
    SendMessageResponse, Task, ThreadSettingsUpdateRequest, ThreadSettingsUpdateResponse,
};
use open_web_codex_platform_store::AppState;
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::Row;
use std::sync::Arc;
use uuid::Uuid;

use crate::access::{ensure_project_access, ensure_task_access};
use crate::codex_workspace::resolve_workspace_id;
use crate::middleware::auth::AuthenticatedUser;

type ApiResult<T> = Result<Json<T>, (StatusCode, Json<PlatformError>)>;

#[derive(Deserialize)]
pub struct ListTasksParams {
    pub project_id: Uuid,
}

/// GET /api/tasks?project_id=...
pub async fn list_tasks(
    auth: AuthenticatedUser,
    State(state): State<AppState>,
    Query(params): Query<ListTasksParams>,
) -> ApiResult<Vec<Task>> {
    ensure_project_access(&state.db, auth.user_id, params.project_id).await?;

    let rows = sqlx::query(
        "SELECT id, project_id, title, status, created_at, updated_at \
         FROM tasks WHERE project_id = $1 ORDER BY created_at DESC",
    )
    .bind(params.project_id)
    .fetch_all(&state.db)
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(PlatformError::internal(format!("{e}"))),
        )
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
    auth: AuthenticatedUser,
    State(state): State<AppState>,
    Json(req): Json<CreateTaskRequest>,
) -> ApiResult<Task> {
    if req.title.trim().is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(PlatformError::bad_request("title must not be empty")),
        ));
    }

    ensure_project_access(&state.db, auth.user_id, req.project_id).await?;

    let row = sqlx::query(
        "INSERT INTO tasks (project_id, title) VALUES ($1, $2) \
         RETURNING id, project_id, title, status, created_at, updated_at",
    )
    .bind(req.project_id)
    .bind(&req.title)
    .fetch_one(&state.db)
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(PlatformError::internal(format!("{e}"))),
        )
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
    auth: AuthenticatedUser,
    State(state): State<AppState>,
    Path(task_id): Path<Uuid>,
    Query(params): Query<ListTaskEventsParams>,
) -> ApiResult<Vec<RunEvent>> {
    ensure_task_access(&state.db, auth.user_id, task_id).await?;

    let limit = params.limit.unwrap_or(50).min(200);
    let query = match params.after_sequence {
        Some(after) => sqlx::query(
            "SELECT e.id, e.sequence, e.run_id, e.event_type, e.projection_version, \
                    e.thread_id, e.turn_id, e.item_id, e.payload, e.created_at \
             FROM run_events e \
             JOIN runs r ON r.id = e.run_id \
             WHERE r.task_id = $1 AND e.sequence > $2 \
             ORDER BY e.sequence ASC LIMIT $3",
        )
        .bind(task_id)
        .bind(after)
        .bind(limit),
        None => sqlx::query(
            "SELECT * FROM ( \
                 SELECT e.id, e.sequence, e.run_id, e.event_type, e.projection_version, \
                        e.thread_id, e.turn_id, e.item_id, e.payload, e.created_at \
                 FROM run_events e \
                 JOIN runs r ON r.id = e.run_id \
                 WHERE r.task_id = $1 \
                 ORDER BY e.sequence DESC LIMIT $2 \
             ) recent ORDER BY sequence ASC",
        )
        .bind(task_id)
        .bind(limit),
    };

    let rows = query.fetch_all(&state.db).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(PlatformError::internal(format!("{e}"))),
        )
    })?;

    let events: Vec<RunEvent> = rows
        .iter()
        .map(|row| {
            let payload: serde_json::Value = row.get("payload");
            RunEvent {
                id: row.get("id"),
                sequence: row.get("sequence"),
                run_id: row.get("run_id"),
                event_type: row.get("event_type"),
                projection_version: row.get("projection_version"),
                thread_id: row.get("thread_id"),
                turn_id: row.get("turn_id"),
                item_id: row.get("item_id"),
                payload,
                created_at: row.get("created_at"),
            }
        })
        .collect();

    Ok(Json(events))
}

/// POST /api/tasks/:id/messages — send a user message to the task's active thread.
pub async fn send_message(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
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

    ensure_task_access(&state.db, auth.user_id, task_id).await?;

    // Find the active run for this task (latest non-terminal run)
    let active_run = sqlx::query(
        "SELECT id, codex_thread_id FROM runs \
         WHERE task_id = $1 AND status IN ('pending', 'queued', 'provisioning', 'running', 'waiting_approval') \
         ORDER BY created_at DESC LIMIT 1",
    )
    .bind(task_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(PlatformError::internal(format!("{e}"))),
        )
    })?
    .ok_or_else(|| {
        (
            StatusCode::BAD_REQUEST,
            Json(PlatformError::bad_request(
                "no active run for this task; start a run first",
            )),
        )
    })?;

    let thread_id: Option<String> = active_run.get("codex_thread_id");
    let thread_id = thread_id.ok_or_else(|| {
        (
            StatusCode::BAD_REQUEST,
            Json(PlatformError::bad_request(
                "active run has no thread yet; try again shortly",
            )),
        )
    })?;

    // Send message via adapter
    let mut rpc_params = json!({
        "threadId": &thread_id,
        "text": req.text,
    });
    if let Some(model) = req.model.as_ref().filter(|value| !value.trim().is_empty()) {
        rpc_params["model"] = json!(model);
    }
    if let Some(effort) = req.effort.as_ref().filter(|value| !value.trim().is_empty()) {
        rpc_params["effort"] = json!(effort);
    }
    if let Some(access_mode) = req.access_mode.as_ref().filter(|value| !value.trim().is_empty()) {
        rpc_params["accessMode"] = json!(access_mode);
    }

    let rpc_result = adapter
        .rpc("send_user_message", rpc_params)
        .await
        .map_err(|e| {
            (
                StatusCode::BAD_GATEWAY,
                Json(PlatformError::internal(format!("adapter error: {e}"))),
            )
        })?;

    let status = rpc_result
        .get("status")
        .and_then(|v| v.as_str())
        .unwrap_or("sent")
        .to_string();

    Ok(Json(SendMessageResponse { status, thread_id }))
}

/// GET /api/tasks/:id
pub async fn get_task(
    auth: AuthenticatedUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> ApiResult<Task> {
    ensure_task_access(&state.db, auth.user_id, id).await?;

    let row = sqlx::query(
        "SELECT id, project_id, title, status, created_at, updated_at \
         FROM tasks WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(PlatformError::internal(format!("{e}"))),
        )
    })?
    .ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(PlatformError::not_found(format!("task {id} not found"))),
        )
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

fn map_run_row(row: &sqlx::postgres::PgRow) -> open_web_codex_platform_contracts::Run {
    open_web_codex_platform_contracts::Run {
        id: row.get("id"),
        task_id: row.get("task_id"),
        status: row.get("status"),
        codex_thread_id: row.get("codex_thread_id"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    }
}

/// GET /api/tasks/:id/active-run — latest non-terminal run for a task.
pub async fn get_active_run(
    auth: AuthenticatedUser,
    State(state): State<AppState>,
    Path(task_id): Path<Uuid>,
) -> ApiResult<ActiveRunResponse> {
    ensure_task_access(&state.db, auth.user_id, task_id).await?;

    let row = sqlx::query(
        "SELECT id, task_id, status, codex_thread_id, created_at, updated_at \
         FROM runs \
         WHERE task_id = $1 AND status NOT IN ('completed', 'cancelled', 'failed') \
         ORDER BY created_at DESC LIMIT 1",
    )
    .bind(task_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(PlatformError::internal(format!("{e}"))),
        )
    })?;

    Ok(Json(ActiveRunResponse {
        run: row.as_ref().map(map_run_row),
    }))
}

/// PATCH /api/tasks/:id/thread-settings — update model/effort for the active thread.
pub async fn update_thread_settings(
    State(_state): State<AppState>,
    auth: AuthenticatedUser,
    Path(task_id): Path<Uuid>,
    Extension(adapter): Extension<Arc<dyn CodexAdapter>>,
    Json(req): Json<ThreadSettingsUpdateRequest>,
) -> ApiResult<ThreadSettingsUpdateResponse> {
    if req.model.as_ref().is_none_or(|value| value.trim().is_empty())
        && req.effort.as_ref().is_none_or(|value| value.trim().is_empty())
    {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(PlatformError::bad_request(
                "at least one of model or effort must be provided",
            )),
        ));
    }

    ensure_task_access(&_state.db, auth.user_id, task_id).await?;

    let active_run = sqlx::query(
        "SELECT codex_thread_id FROM runs \
         WHERE task_id = $1 AND status IN ('pending', 'queued', 'provisioning', 'running', 'waiting_approval') \
         ORDER BY created_at DESC LIMIT 1",
    )
    .bind(task_id)
    .fetch_optional(&_state.db)
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(PlatformError::internal(format!("{e}"))),
        )
    })?
    .ok_or_else(|| {
        (
            StatusCode::BAD_REQUEST,
            Json(PlatformError::bad_request(
                "no active run for this task; start a run first",
            )),
        )
    })?;

    let thread_id: Option<String> = active_run.get("codex_thread_id");
    let thread_id = thread_id.ok_or_else(|| {
        (
            StatusCode::BAD_REQUEST,
            Json(PlatformError::bad_request(
                "active run has no thread yet; try again shortly",
            )),
        )
    })?;

    let workspace_id = resolve_workspace_id(&adapter)
        .await
        .map_err(|error| {
            (
                StatusCode::BAD_GATEWAY,
                Json(PlatformError::internal(format!("adapter error: {error}"))),
            )
        })?;

    let mut settings = serde_json::Map::new();
    if let Some(model) = req.model.as_ref().filter(|value| !value.trim().is_empty()) {
        settings.insert("model".to_string(), json!(model));
    }
    if let Some(effort) = req.effort.as_ref().filter(|value| !value.trim().is_empty()) {
        settings.insert("effort".to_string(), json!(effort));
    }

    adapter
        .rpc(
            "thread_settings_update",
            json!({
                "workspaceId": workspace_id,
                "threadId": thread_id,
                "settings": Value::Object(settings),
            }),
        )
        .await
        .map_err(|error| {
            (
                StatusCode::BAD_GATEWAY,
                Json(PlatformError::internal(format!("adapter error: {error}"))),
            )
        })?;

    Ok(Json(ThreadSettingsUpdateResponse {
        thread_id,
        status: "updated".to_string(),
    }))
}
