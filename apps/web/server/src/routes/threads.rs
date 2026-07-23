use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Extension, Json,
};
use open_web_codex_adapter::{AuthorizedWorkspace, CodexAdapter};
use open_web_codex_platform_contracts::error::PlatformError;
use open_web_codex_platform_contracts::{
    SetThreadNameRequest, ThreadHistory, ThreadHistoryError, ThreadHistoryResponse,
    ThreadHistoryStatus, ThreadHistoryTurn,
};
use open_web_codex_platform_store::AppState;
use serde_json::json;
use sqlx::Row;
use uuid::Uuid;

use crate::middleware::auth::AuthenticatedUser;

type ApiError = (StatusCode, Json<PlatformError>);
type ApiResult<T> = Result<Json<T>, ApiError>;

pub async fn read(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(run_id): Path<Uuid>,
    Extension(adapter): Extension<Arc<dyn CodexAdapter>>,
) -> ApiResult<ThreadHistoryResponse> {
    let context = authorized_thread(&state, &auth, run_id).await?;
    let value = adapter
        .read_thread(&context.workspace, &context.thread_id)
        .await
        .map_err(runtime_error)?;
    let thread = value
        .get("thread")
        .ok_or_else(|| bad_gateway("Runtime thread/read omitted thread"))?;
    Ok(Json(ThreadHistoryResponse {
        thread: project_thread(thread, &context.thread_id)?,
    }))
}

pub async fn list_turns(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(run_id): Path<Uuid>,
    Extension(adapter): Extension<Arc<dyn CodexAdapter>>,
) -> ApiResult<Vec<ThreadHistoryTurn>> {
    let context = authorized_thread(&state, &auth, run_id).await?;
    let turns = adapter
        .list_thread_turns(&context.workspace, &context.thread_id)
        .await
        .map_err(runtime_error)?;
    turns
        .iter()
        .map(project_turn)
        .collect::<Result<Vec<_>, _>>()
        .map(Json)
}

pub async fn archive(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(run_id): Path<Uuid>,
    Extension(adapter): Extension<Arc<dyn CodexAdapter>>,
) -> ApiResult<serde_json::Value> {
    let context = authorized_thread(&state, &auth, run_id).await?;
    adapter
        .archive_thread(&context.workspace, &context.thread_id)
        .await
        .map_err(runtime_error)?;
    let mut transaction = state.db.begin().await.map_err(database_error)?;
    sqlx::query("UPDATE tasks SET status = 'archived', updated_at = now() WHERE id = $1")
        .bind(context.task_id)
        .execute(&mut *transaction)
        .await
        .map_err(database_error)?;
    audit(
        &mut transaction,
        &auth,
        context.workspace_id,
        run_id,
        "thread.archive",
    )
    .await?;
    transaction.commit().await.map_err(database_error)?;
    Ok(Json(json!({ "status": "archived" })))
}

pub async fn set_name(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(run_id): Path<Uuid>,
    Extension(adapter): Extension<Arc<dyn CodexAdapter>>,
    Json(request): Json<SetThreadNameRequest>,
) -> ApiResult<serde_json::Value> {
    let name = request.name.trim();
    if name.is_empty()
        || name.len() > 200
        || name
            .chars()
            .any(|character| matches!(character, '\0' | '\n' | '\r'))
    {
        return Err(bad_request("Thread name is invalid"));
    }
    let context = authorized_thread(&state, &auth, run_id).await?;
    adapter
        .set_thread_name(&context.workspace, &context.thread_id, name)
        .await
        .map_err(runtime_error)?;
    let mut transaction = state.db.begin().await.map_err(database_error)?;
    sqlx::query("UPDATE tasks SET title = $1, updated_at = now() WHERE id = $2")
        .bind(name)
        .bind(context.task_id)
        .execute(&mut *transaction)
        .await
        .map_err(database_error)?;
    audit(
        &mut transaction,
        &auth,
        context.workspace_id,
        run_id,
        "thread.name_set",
    )
    .await?;
    transaction.commit().await.map_err(database_error)?;
    Ok(Json(json!({ "status": "renamed", "name": name })))
}

struct ThreadContext {
    task_id: Uuid,
    workspace_id: Uuid,
    thread_id: String,
    workspace: AuthorizedWorkspace,
}

async fn authorized_thread(
    state: &AppState,
    auth: &AuthenticatedUser,
    run_id: Uuid,
) -> Result<ThreadContext, ApiError> {
    let row = sqlx::query(
        "SELECT r.task_id, r.workspace_id, r.codex_thread_id, r.requested_by, \
                w.root_path, w.state \
         FROM runs r JOIN workspaces w ON w.id = r.workspace_id \
         WHERE r.id = $1 AND r.organization_id = $2 AND w.organization_id = $2",
    )
    .bind(run_id)
    .bind(auth.organization_id)
    .fetch_optional(&state.db)
    .await
    .map_err(database_error)?
    .ok_or_else(not_found)?;
    let requested_by: Option<Uuid> = row.get("requested_by");
    if row.get::<String, _>("state") == "retired"
        || (requested_by != Some(auth.user_id)
            && !matches!(auth.organization_role.as_str(), "owner" | "admin"))
    {
        return Err(not_found());
    }
    let workspace_id: Uuid = row.get("workspace_id");
    let thread_id: Option<String> = row.get("codex_thread_id");
    let thread_id = thread_id.ok_or_else(|| bad_request("Run does not have a Codex Thread"))?;
    Ok(ThreadContext {
        task_id: row.get("task_id"),
        workspace_id,
        thread_id,
        workspace: AuthorizedWorkspace {
            id: workspace_id.to_string(),
            root: row.get::<String, _>("root_path").into(),
        },
    })
}

fn project_thread(value: &serde_json::Value, expected_id: &str) -> Result<ThreadHistory, ApiError> {
    let object = value
        .as_object()
        .ok_or_else(|| bad_gateway("Runtime Thread was invalid"))?;
    let id = object
        .get("id")
        .and_then(serde_json::Value::as_str)
        .filter(|id| *id == expected_id)
        .ok_or_else(|| bad_gateway("Runtime returned the wrong Thread"))?;
    let turns = object
        .get("turns")
        .and_then(serde_json::Value::as_array)
        .map(|turns| {
            turns
                .iter()
                .map(project_turn)
                .collect::<Result<Vec<_>, _>>()
        })
        .transpose()?
        .unwrap_or_default();
    Ok(ThreadHistory {
        id: id.to_string(),
        name: object
            .get("name")
            .and_then(serde_json::Value::as_str)
            .map(str::to_string),
        preview: object
            .get("preview")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .to_string(),
        created_at: object
            .get("createdAt")
            .and_then(serde_json::Value::as_i64)
            .unwrap_or_default(),
        updated_at: object
            .get("updatedAt")
            .and_then(serde_json::Value::as_i64)
            .unwrap_or_default(),
        status: project_thread_status(object.get("status")),
        turns,
    })
}

fn project_thread_status(value: Option<&serde_json::Value>) -> ThreadHistoryStatus {
    let object = value.and_then(serde_json::Value::as_object);
    ThreadHistoryStatus {
        r#type: object
            .and_then(|value| value.get("type"))
            .and_then(serde_json::Value::as_str)
            .unwrap_or("notLoaded")
            .to_string(),
        active_flags: object
            .and_then(|value| value.get("activeFlags"))
            .and_then(serde_json::Value::as_array)
            .map(|flags| {
                flags
                    .iter()
                    .filter_map(serde_json::Value::as_str)
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default(),
    }
}

fn project_turn(value: &serde_json::Value) -> Result<ThreadHistoryTurn, ApiError> {
    let object = value
        .as_object()
        .ok_or_else(|| bad_gateway("Runtime Turn was invalid"))?;
    let id = object
        .get("id")
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| bad_gateway("Runtime Turn omitted id"))?;
    let items = object
        .get("items")
        .and_then(serde_json::Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| {
                    let source = item.as_object()?;
                    let mut projected = crate::event_projection::project_item(source);
                    if let (Some(id), Some(target)) = (
                        source.get("id").and_then(serde_json::Value::as_str),
                        projected.as_object_mut(),
                    ) {
                        target.insert("id".to_string(), serde_json::Value::String(id.to_string()));
                    }
                    Some(projected)
                })
                .collect()
        })
        .unwrap_or_default();
    let error = object
        .get("error")
        .and_then(serde_json::Value::as_object)
        .map(|error| ThreadHistoryError {
            message: error
                .get("message")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("Turn failed")
                .to_string(),
            additional_details: error
                .get("additionalDetails")
                .and_then(serde_json::Value::as_str)
                .map(str::to_string),
        });
    Ok(ThreadHistoryTurn {
        id: id.to_string(),
        status: object
            .get("status")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("completed")
            .to_string(),
        items,
        error,
        started_at: object.get("startedAt").and_then(serde_json::Value::as_i64),
        completed_at: object
            .get("completedAt")
            .and_then(serde_json::Value::as_i64),
        duration_ms: object.get("durationMs").and_then(serde_json::Value::as_i64),
    })
}

async fn audit(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    auth: &AuthenticatedUser,
    workspace_id: Uuid,
    run_id: Uuid,
    action: &str,
) -> Result<(), ApiError> {
    sqlx::query(
        "INSERT INTO audit_log \
         (organization_id, actor_id, action, target_type, target_id, metadata, outcome) \
         VALUES ($1, $2, $3, 'workspace', $4, $5, 'success')",
    )
    .bind(auth.organization_id)
    .bind(auth.user_id)
    .bind(action)
    .bind(workspace_id)
    .bind(json!({ "runId": run_id }))
    .execute(&mut **transaction)
    .await
    .map_err(database_error)?;
    Ok(())
}

fn bad_request(message: &str) -> ApiError {
    (
        StatusCode::BAD_REQUEST,
        Json(PlatformError::bad_request(message)),
    )
}

fn bad_gateway(message: &str) -> ApiError {
    (
        StatusCode::BAD_GATEWAY,
        Json(PlatformError::internal(message)),
    )
}

fn not_found() -> ApiError {
    (
        StatusCode::NOT_FOUND,
        Json(PlatformError::not_found("Run Thread was not found")),
    )
}

fn runtime_error(_: open_web_codex_adapter::AdapterError) -> ApiError {
    (
        StatusCode::BAD_GATEWAY,
        Json(PlatformError::internal("Codex Thread operation failed")),
    )
}

fn database_error(_: sqlx::Error) -> ApiError {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(PlatformError::internal("Database operation failed")),
    )
}

#[cfg(test)]
mod tests {
    use super::{project_thread, project_turn};
    use serde_json::json;

    #[test]
    fn projects_authoritative_history_without_runtime_only_fields() {
        let projected = project_thread(
            &json!({
                "id": "thread-1",
                "name": "Example",
                "preview": "hello",
                "createdAt": 10,
                "updatedAt": 20,
                "status": { "type": "active", "activeFlags": ["waitingOnApproval"] },
                "cwd": "/private/server/workspace",
                "providerApiKey": "secret",
                "turns": [{
                    "id": "turn-1",
                    "status": "completed",
                    "items": [{
                        "id": "item-1",
                        "type": "agentMessage",
                        "text": "done",
                        "cwd": "/private/server/workspace",
                        "apiKey": "secret"
                    }]
                }]
            }),
            "thread-1",
        )
        .expect("valid Thread projection");
        let value = serde_json::to_value(projected).expect("serializable projection");

        assert_eq!(value["status"]["type"], "active");
        assert_eq!(value["turns"][0]["items"][0]["id"], "item-1");
        assert_eq!(value["turns"][0]["items"][0]["text"], "done");
        let encoded = value.to_string();
        assert!(!encoded.contains("/private/server/workspace"));
        assert!(!encoded.contains("secret"));
    }

    #[test]
    fn rejects_turns_without_stable_identity() {
        assert!(project_turn(&json!({ "status": "completed", "items": [] })).is_err());
    }
}
