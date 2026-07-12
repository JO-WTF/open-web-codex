use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Extension, Json,
};
use open_web_codex_adapter::CodexAdapter;
use open_web_codex_platform_contracts::error::PlatformError;
use open_web_codex_platform_contracts::{Run, StartRunResponse};
use open_web_codex_platform_store::AppState;
use serde_json::json;
use sqlx::Row;
use uuid::Uuid;

use crate::middleware::auth::AuthenticatedUser;

type ApiResult<T> = Result<Json<T>, (StatusCode, Json<PlatformError>)>;

/// POST /api/tasks/:id/runs — start a new run, creating a Codex thread.
pub async fn start_run(
    State(state): State<AppState>,
    _auth: AuthenticatedUser,
    Path(task_id): Path<Uuid>,
    Extension(adapter): Extension<Arc<dyn CodexAdapter>>,
) -> ApiResult<StartRunResponse> {
    // Verify task exists and user has access
    let task = sqlx::query(
        "SELECT t.id, t.project_id, p.name as project_name \
         FROM tasks t JOIN projects p ON p.id = t.project_id \
         WHERE t.id = $1",
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

    let (_, _project_id) = match task {
        Some(r) => (r.get::<Uuid, _>("id"), r.get::<Uuid, _>("project_id")),
        None => {
            return Err((
                StatusCode::NOT_FOUND,
                Json(PlatformError::not_found("task not found")),
            ))
        }
    };

    // Create run with pending status
    let run = sqlx::query(
        "INSERT INTO runs (task_id, status) VALUES ($1, 'pending') \
         RETURNING id, task_id, status, codex_thread_id, created_at, updated_at",
    )
    .bind(task_id)
    .fetch_one(&state.db)
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(PlatformError::internal(format!("{e}"))),
        )
    })?;

    let run_id: Uuid = run.get("id");

    // Get first workspace from adapter
    let workspaces = adapter
        .rpc("list_workspaces", json!({}))
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(PlatformError::internal(format!("adapter error: {e}"))),
            )
        })?;

    let ws_id = workspaces[0]["id"]
        .as_str()
        .ok_or_else(|| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(PlatformError::internal("no workspace available".to_string())),
            )
        })?
        .to_string();

    // Start thread via adapter
    let result = adapter
        .rpc("start_thread", json!({ "workspaceId": ws_id }))
        .await
        .map_err(|e| {
            // Update run as failed
            let _ = sqlx::query("UPDATE runs SET status = 'failed' WHERE id = $1")
                .bind(run_id)
                .execute(&state.db);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(PlatformError::internal(format!("adapter error: {e}"))),
            )
        })?;

    let thread_id = result["threadId"]
        .as_str()
        .ok_or_else(|| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(PlatformError::internal("no threadId in response".to_string())),
            )
        })?
        .to_string();

    // Update run with thread_id and running status
    let run = sqlx::query(
        "UPDATE runs SET status = 'running', codex_thread_id = $1, updated_at = now() \
         WHERE id = $2 \
         RETURNING id, task_id, status, codex_thread_id, created_at, updated_at",
    )
    .bind(&thread_id)
    .bind(run_id)
    .fetch_one(&state.db)
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(PlatformError::internal(format!("{e}"))),
        )
    })?;

    Ok(Json(StartRunResponse {
        run: Run {
            id: run.get("id"),
            task_id: run.get("task_id"),
            status: run.get("status"),
            codex_thread_id: run.get("codex_thread_id"),
            created_at: run.get("created_at"),
            updated_at: run.get("updated_at"),
        },
    }))
}

/// GET /api/runs?task_id=... — list runs for a task.
pub async fn list_runs(
    State(state): State<AppState>,
    _auth: AuthenticatedUser,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, Uuid>>,
) -> ApiResult<Vec<Run>> {
    let task_id = params.get("task_id").copied();

    let rows = if let Some(tid) = task_id {
        sqlx::query(
            "SELECT id, task_id, status, codex_thread_id, created_at, updated_at \
             FROM runs WHERE task_id = $1 ORDER BY created_at DESC",
        )
        .bind(tid)
        .fetch_all(&state.db)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(PlatformError::internal(format!("{e}"))),
            )
        })?
    } else {
        sqlx::query(
            "SELECT id, task_id, status, codex_thread_id, created_at, updated_at \
             FROM runs ORDER BY created_at DESC",
        )
        .fetch_all(&state.db)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(PlatformError::internal(format!("{e}"))),
            )
        })?
    };

    let runs: Vec<Run> = rows
        .iter()
        .map(|r| Run {
            id: r.get("id"),
            task_id: r.get("task_id"),
            status: r.get("status"),
            codex_thread_id: r.get("codex_thread_id"),
            created_at: r.get("created_at"),
            updated_at: r.get("updated_at"),
        })
        .collect();

    Ok(Json(runs))
}

/// GET /api/runs/:id — get a single run.
pub async fn get_run(
    State(state): State<AppState>,
    _auth: AuthenticatedUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Run> {
    let row = sqlx::query(
        "SELECT id, task_id, status, codex_thread_id, created_at, updated_at \
         FROM runs WHERE id = $1",
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
            Json(PlatformError::not_found("run not found")),
        )
    })?;

    Ok(Json(Run {
        id: row.get("id"),
        task_id: row.get("task_id"),
        status: row.get("status"),
        codex_thread_id: row.get("codex_thread_id"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    }))
}

/// POST /api/runs/:id/cancel — cancel a running run.
pub async fn cancel_run(
    State(state): State<AppState>,
    _auth: AuthenticatedUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Run> {
    let row = sqlx::query(
        "UPDATE runs SET status = 'cancelled', updated_at = now() \
         WHERE id = $1 AND status NOT IN ('completed', 'cancelled', 'failed') \
         RETURNING id, task_id, status, codex_thread_id, created_at, updated_at",
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
            StatusCode::CONFLICT,
            Json(PlatformError::bad_request(
                "run not found or already in terminal state",
            )),
        )
    })?;

    Ok(Json(Run {
        id: row.get("id"),
        task_id: row.get("task_id"),
        status: row.get("status"),
        codex_thread_id: row.get("codex_thread_id"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    }))
}
