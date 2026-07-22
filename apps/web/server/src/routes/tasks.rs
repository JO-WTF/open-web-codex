use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Extension, Json,
};
use open_web_codex_adapter::{AuthorizedWorkspace, CodexAdapter, TurnOptions};
use open_web_codex_platform_contracts::error::PlatformError;
use open_web_codex_platform_contracts::{
    CreateTaskRequest, ListTaskEventsParams, RunEvent, SendMessageRequest, SendMessageResponse,
    Task,
};
use open_web_codex_platform_store::AppState;
use serde::Deserialize;
use sqlx::Row;
use std::sync::Arc;
use uuid::Uuid;

use crate::middleware::auth::{require_runtime_profile, AuthenticatedUser};
use crate::routes::RuntimeProfileBinding;

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
    let rows = sqlx::query(
        "SELECT id, project_id, title, status, created_at, updated_at \
         FROM tasks WHERE project_id = $1 AND organization_id = $2 ORDER BY created_at DESC",
    )
    .bind(params.project_id)
    .bind(auth.organization_id)
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

    let row = sqlx::query(
        "INSERT INTO tasks (organization_id, project_id, created_by, title) \
         SELECT organization_id, id, $4, $2 FROM projects WHERE id = $1 AND organization_id = $3 \
         RETURNING id, project_id, title, status, created_at, updated_at",
    )
    .bind(req.project_id)
    .bind(&req.title)
    .bind(auth.organization_id)
    .bind(auth.user_id)
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
            Json(PlatformError::not_found("project not found")),
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
    let limit = params.limit.unwrap_or(50).min(200);
    let query = match params.after_sequence {
        Some(after) => sqlx::query(
            "SELECT e.id, e.sequence, e.run_id, e.event_type, e.projection_version, \
                    e.thread_id, e.turn_id, e.item_id, e.payload, e.created_at \
             FROM run_events e \
             JOIN runs r ON r.id = e.run_id \
             WHERE r.task_id = $1 AND r.organization_id = $2 AND e.sequence > $3 \
             ORDER BY e.sequence ASC LIMIT $4",
        )
        .bind(task_id)
        .bind(auth.organization_id)
        .bind(after)
        .bind(limit),
        None => sqlx::query(
            "SELECT * FROM ( \
                 SELECT e.id, e.sequence, e.run_id, e.event_type, e.projection_version, \
                        e.thread_id, e.turn_id, e.item_id, e.payload, e.created_at \
                 FROM run_events e \
                 JOIN runs r ON r.id = e.run_id \
                 WHERE r.task_id = $1 AND r.organization_id = $2 \
                 ORDER BY e.sequence DESC LIMIT $3 \
             ) recent ORDER BY sequence ASC",
        )
        .bind(task_id)
        .bind(auth.organization_id)
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
    Extension(profile): Extension<RuntimeProfileBinding>,
    Json(req): Json<SendMessageRequest>,
) -> ApiResult<SendMessageResponse> {
    if req.text.trim().is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(PlatformError::bad_request("message text must not be empty")),
        ));
    }
    require_runtime_profile(&state.db, &auth, &profile.runtime_key).await?;

    // Resolve the server-owned workspace; the browser never supplies a path.
    let active_run = sqlx::query(
        "SELECT r.id, r.codex_thread_id, r.workspace_id, w.root_path \
         FROM runs r LEFT JOIN workspaces w ON w.id = r.workspace_id \
         WHERE r.task_id = $1 AND r.organization_id = $2 \
           AND r.requested_by = $3 AND r.status = 'running' \
         ORDER BY created_at DESC LIMIT 1",
    )
    .bind(task_id)
    .bind(auth.organization_id)
    .bind(auth.user_id)
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
                "no active owned Run for this Task; start a Run first",
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
    let workspace_id: Option<Uuid> = active_run.get("workspace_id");
    let root_path: Option<String> = active_run.get("root_path");
    let workspace = match (workspace_id, root_path) {
        (Some(workspace_id), Some(root)) => AuthorizedWorkspace {
            id: workspace_id.to_string(),
            root: root.into(),
        },
        _ => {
            return Err((
                StatusCode::CONFLICT,
                Json(PlatformError::bad_request(
                    "active Run workspace is not ready",
                )),
            ));
        }
    };

    let result = adapter
        .send_user_message(
            &workspace,
            &thread_id,
            &req.text,
            &TurnOptions {
                model: req.model,
                effort: req.effort,
                service_tier: req.service_tier,
                access_mode: req.access_mode,
                images: req.images,
                collaboration_mode: req.collaboration_mode,
            },
        )
        .await
        .map_err(|_| {
            (
                StatusCode::BAD_GATEWAY,
                Json(PlatformError::internal(
                    "Codex Runtime failed to start the Turn",
                )),
            )
        })?;

    if let Some(turn_id) = result.get("turnId").and_then(serde_json::Value::as_str) {
        if let Err(error) = sqlx::query(
            "UPDATE runs SET active_turn_id = $1, updated_at = now() \
             WHERE id = $2 AND organization_id = $3 AND status = 'running'",
        )
        .bind(turn_id)
        .bind(active_run.get::<Uuid, _>("id"))
        .bind(auth.organization_id)
        .execute(&state.db)
        .await
        {
            tracing::warn!(%error, "active Turn delivery succeeded but projection update failed");
        }
    }

    let status = result
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
    let row = sqlx::query(
        "SELECT id, project_id, title, status, created_at, updated_at \
         FROM tasks WHERE id = $1 AND organization_id = $2",
    )
    .bind(id)
    .bind(auth.organization_id)
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
