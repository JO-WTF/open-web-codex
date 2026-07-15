use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    Extension, Json,
};
use chrono::{Duration, Utc};
use open_web_codex_adapter::CodexAdapter;
use open_web_codex_codex_contracts::{
    negotiate_capability_manifest, CapabilityManifest, NegotiationPolicy,
};
use open_web_codex_platform_contracts::error::PlatformError;
use open_web_codex_platform_contracts::{Run, StartRunResponse};
use open_web_codex_platform_store::AppState;
use open_web_codex_profile_host::{ensure_profile_home, provision_profile};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use sqlx::Row;
use uuid::Uuid;

use crate::git_workspace;
use crate::middleware::auth::AuthenticatedUser;
use crate::run_lifecycle;

type ApiResult<T> = Result<Json<T>, (StatusCode, Json<PlatformError>)>;

const MANIFEST_FIXTURE: &str =
    include_str!("../../../contracts/codex/fixtures/capability-manifest.v1.json");

struct TaskContext {
    git_url: String,
    default_branch: String,
}

/// POST /api/tasks/:id/runs — queue and start a run with orchestration.
pub async fn start_run(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(task_id): Path<Uuid>,
    headers: HeaderMap,
    Extension(adapter): Extension<Arc<dyn CodexAdapter>>,
) -> ApiResult<StartRunResponse> {
    if let Some(cached) = lookup_idempotent_response(&state, &headers, "POST /api/tasks/:id/runs").await? {
        return Ok(cached);
    }

    ensure_thread_capability(&adapter).await?;

    let task = load_task_context(&state, task_id).await?;
    let data_root = data_root_from_env();

    let run = sqlx::query(
        "INSERT INTO runs (task_id, status) VALUES ($1, 'queued') \
         RETURNING id, task_id, status, codex_thread_id, created_at, updated_at",
    )
    .bind(task_id)
    .fetch_one(&state.db)
    .await
    .map_err(db_error)?;

    let run_id: Uuid = run.get("id");
    transition_run(&state, run_id, "queued", "provisioning").await?;

    let profile_home = provision_user_profile(&state, &auth, &data_root).await?;

    if let Ok(workspace_key) = git_workspace::provision_run_workspace(
        &data_root,
        run_id,
        &task.git_url,
        &task.default_branch,
    ) {
        sqlx::query(
            "INSERT INTO run_workspaces (run_id, state, workspace_key) VALUES ($1, 'ready', $2)",
        )
        .bind(run_id)
        .bind(&workspace_key)
        .execute(&state.db)
        .await
        .map_err(db_error)?;
    }

    let workspaces = adapter.rpc("list_workspaces", json!({})).await.map_err(adapter_error)?;
    let ws_id = workspaces
        .as_array()
        .and_then(|items| items.first())
        .and_then(|item| item.get("id"))
        .and_then(Value::as_str)
        .ok_or_else(|| adapter_error(open_web_codex_adapter::AdapterError::Internal(
            "no workspace available".into(),
        )))?
        .to_string();

    let result = adapter
        .rpc("start_thread", json!({ "workspaceId": ws_id }))
        .await
        .map_err(|error| {
            let _ = transition_run_sync(&state.db, run_id, "provisioning", "failed");
            adapter_error(error)
        })?;

    let thread_id = result
        .get("threadId")
        .or_else(|| result.pointer("/thread/id"))
        .and_then(Value::as_str)
        .ok_or_else(|| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(PlatformError::internal("no threadId in response".to_string())),
            )
        })?
        .to_string();

    let run = sqlx::query(
        "UPDATE runs SET status = 'running', codex_thread_id = $1, updated_at = now() \
         WHERE id = $2 \
         RETURNING id, task_id, status, codex_thread_id, created_at, updated_at",
    )
    .bind(&thread_id)
    .bind(run_id)
    .fetch_one(&state.db)
    .await
    .map_err(db_error)?;

    let _profile_home = profile_home;

    let response = StartRunResponse {
        run: map_run_row(&run),
    };
    store_idempotent_response(
        &state,
        &headers,
        "POST /api/tasks/:id/runs",
        StatusCode::OK,
        &response,
    )
    .await?;
    Ok(Json(response))
}

async fn ensure_thread_capability(adapter: &Arc<dyn CodexAdapter>) -> Result<(), (StatusCode, Json<PlatformError>)> {
    let manifest = match adapter.rpc("initialize", json!({})).await {
        Ok(value) => parse_manifest_value(value)?,
        Err(open_web_codex_adapter::AdapterError::NotImplemented(_)) => {
            serde_json::from_str(MANIFEST_FIXTURE).map_err(|error| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(PlatformError::internal(format!("manifest fixture invalid: {error}"))),
                )
            })?
        }
        Err(error) => return Err(adapter_error(error)),
    };

    let negotiated = negotiate_capability_manifest(
        manifest,
        &NegotiationPolicy {
            client_protocol_version: "1.0.0".to_string(),
            allowed_server_builds: vec![],
            required_capabilities: vec!["thread.lifecycle".to_string()],
            allow_experimental: false,
            allow_experimental_ids: vec![],
        },
    )
    .map_err(|error| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(PlatformError::internal(format!("manifest parse failed: {error}"))),
        )
    })?;

    if negotiated.status != "compatible" {
        return Err((
            StatusCode::CONFLICT,
            Json(PlatformError::bad_request(format!(
                "capability manifest incompatible: {}",
                negotiated.reasons.join(", ")
            ))),
        ));
    }
    Ok(())
}

fn parse_manifest_value(value: Value) -> Result<CapabilityManifest, (StatusCode, Json<PlatformError>)> {
    if let Ok(manifest) = serde_json::from_value::<CapabilityManifest>(value.clone()) {
        return Ok(manifest);
    }
    let manifest_value = value
        .pointer("/result/manifest")
        .or_else(|| value.get("manifest"))
        .cloned()
        .unwrap_or(value);
    serde_json::from_value(manifest_value).map_err(|error| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(PlatformError::internal(format!("manifest decode failed: {error}"))),
        )
    })
}

async fn load_task_context(state: &AppState, task_id: Uuid) -> Result<TaskContext, (StatusCode, Json<PlatformError>)> {
    let row = sqlx::query(
        "SELECT t.id, t.project_id, p.git_url, p.default_branch \
         FROM tasks t JOIN projects p ON p.id = t.project_id \
         WHERE t.id = $1",
    )
    .bind(task_id)
    .fetch_optional(&state.db)
    .await
    .map_err(db_error)?;

    let Some(row) = row else {
        return Err((
            StatusCode::NOT_FOUND,
            Json(PlatformError::not_found("task not found")),
        ));
    };

    Ok(TaskContext {
        git_url: row.get("git_url"),
        default_branch: row.get("default_branch"),
    })
}

async fn provision_user_profile(
    state: &AppState,
    auth: &AuthenticatedUser,
    data_root: &PathBuf,
) -> Result<PathBuf, (StatusCode, Json<PlatformError>)> {
    let (_layout, _lock, home) = provision_profile(data_root, &auth.user_id.to_string()).map_err(|error| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(PlatformError::internal(format!("profile provision failed: {error}"))),
        )
    })?;

    sqlx::query(
        "INSERT INTO profiles (user_id, home_path) VALUES ($1, $2) \
         ON CONFLICT (user_id) DO UPDATE SET home_path = EXCLUDED.home_path, updated_at = now()",
    )
    .bind(auth.user_id)
    .bind(home.to_string_lossy().to_string())
    .execute(&state.db)
    .await
    .map_err(db_error)?;

    if let Ok(canonical) = ensure_profile_home(&home) {
        std::env::set_var("CODEX_HOME", &canonical);
        return Ok(canonical);
    }
    Ok(home)
}

async fn transition_run(
    state: &AppState,
    run_id: Uuid,
    from: &str,
    to: &str,
) -> Result<(), (StatusCode, Json<PlatformError>)> {
    run_lifecycle::assert_transition(from, to).map_err(|error| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(PlatformError::internal(error)),
        )
    })?;
    transition_run_sync(&state.db, run_id, from, to).await
}

async fn transition_run_sync(
    db: &sqlx::PgPool,
    run_id: Uuid,
    from: &str,
    to: &str,
) -> Result<(), (StatusCode, Json<PlatformError>)> {
    let updated = sqlx::query(
        "UPDATE runs SET status = $1, updated_at = now() WHERE id = $2 AND status = $3",
    )
    .bind(to)
    .bind(run_id)
    .bind(from)
    .execute(db)
    .await
    .map_err(db_error)?;

    if updated.rows_affected() == 0 {
        return Err((
            StatusCode::CONFLICT,
            Json(PlatformError::bad_request(format!(
                "run {run_id} is not in state {from}"
            ))),
        ));
    }
    Ok(())
}

fn data_root_from_env() -> PathBuf {
    std::env::var("DATA_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|_| std::env::temp_dir().join("open-web-codex-data"))
}

async fn lookup_idempotent_response(
    state: &AppState,
    headers: &HeaderMap,
    route: &str,
) -> Result<Option<Json<StartRunResponse>>, (StatusCode, Json<PlatformError>)> {
    let Some(key) = idempotency_key(headers) else {
        return Ok(None);
    };
    let key_hash = hash_idempotency_key(&key);
    let row = sqlx::query(
        "SELECT response_status, response_body FROM idempotency_keys \
         WHERE key_hash = $1 AND route = $2 AND expires_at > now()",
    )
    .bind(&key_hash)
    .bind(route)
    .fetch_optional(&state.db)
    .await
    .map_err(db_error)?;

    let Some(row) = row else {
        return Ok(None);
    };
    let body: Value = row.get("response_body");
    let response: StartRunResponse = serde_json::from_value(body).map_err(|error| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(PlatformError::internal(format!("stored idempotency body invalid: {error}"))),
        )
    })?;
    Ok(Some(Json(response)))
}

async fn store_idempotent_response(
    state: &AppState,
    headers: &HeaderMap,
    route: &str,
    status: StatusCode,
    response: &StartRunResponse,
) -> Result<(), (StatusCode, Json<PlatformError>)> {
    let Some(key) = idempotency_key(headers) else {
        return Ok(());
    };
    let key_hash = hash_idempotency_key(&key);
    let expires_at = Utc::now() + Duration::hours(24);
    sqlx::query(
        "INSERT INTO idempotency_keys (key_hash, route, response_status, response_body, expires_at) \
         VALUES ($1, $2, $3, $4, $5) \
         ON CONFLICT (key_hash) DO NOTHING",
    )
    .bind(key_hash)
    .bind(route)
    .bind(status.as_u16() as i32)
    .bind(serde_json::to_value(response).map_err(|error| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(PlatformError::internal(format!("{error}"))),
        )
    })?)
    .bind(expires_at)
    .execute(&state.db)
    .await
    .map_err(db_error)?;
    Ok(())
}

fn idempotency_key(headers: &HeaderMap) -> Option<String> {
    headers
        .get("idempotency-key")
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn hash_idempotency_key(key: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(key.as_bytes());
    hex::encode(hasher.finalize())
}

fn map_run_row(row: &sqlx::postgres::PgRow) -> Run {
    Run {
        id: row.get("id"),
        task_id: row.get("task_id"),
        status: row.get("status"),
        codex_thread_id: row.get("codex_thread_id"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    }
}

fn db_error(error: sqlx::Error) -> (StatusCode, Json<PlatformError>) {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(PlatformError::internal(format!("{error}"))),
    )
}

fn adapter_error(error: open_web_codex_adapter::AdapterError) -> (StatusCode, Json<PlatformError>) {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(PlatformError::internal(format!("adapter error: {error}"))),
    )
}

/// GET /api/runs?task_id=... — list runs for a task.
pub async fn list_runs(
    State(state): State<AppState>,
    _auth: AuthenticatedUser,
    axum::extract::Query(params): axum::extract::Query<HashMap<String, Uuid>>,
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
        .map_err(db_error)?
    } else {
        sqlx::query(
            "SELECT id, task_id, status, codex_thread_id, created_at, updated_at \
             FROM runs ORDER BY created_at DESC",
        )
        .fetch_all(&state.db)
        .await
        .map_err(db_error)?
    };

    Ok(Json(rows.iter().map(map_run_row).collect()))
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
    .map_err(db_error)?
    .ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(PlatformError::not_found("run not found")),
        )
    })?;

    Ok(Json(map_run_row(&row)))
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
    .map_err(db_error)?
    .ok_or_else(|| {
        (
            StatusCode::CONFLICT,
            Json(PlatformError::bad_request(
                "run not found or already in terminal state",
            )),
        )
    })?;

    Ok(Json(map_run_row(&row)))
}
