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
use open_web_codex_platform_contracts::{
    Run, RunFileContentResponse, RunFileListResponse, RunGitStatusResponse, StartRunResponse,
    TurnControlRequest,
};
use open_web_codex_platform_store::AppState;
use open_web_codex_profile_host::{provision_profile, ProfileLock};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use sqlx::Row;
use uuid::Uuid;

use crate::access::{ensure_run_access, ensure_task_access};
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

struct ProfileSession {
    db: sqlx::PgPool,
    profile_id: Uuid,
    home: PathBuf,
    _lock: ProfileLock,
}

impl ProfileSession {
    async fn release(self) -> Result<(), (StatusCode, Json<PlatformError>)> {
        sqlx::query("DELETE FROM profile_processes WHERE profile_id = $1")
            .bind(self.profile_id)
            .execute(&self.db)
            .await
            .map_err(db_error)?;
        Ok(())
    }
}

struct IdempotencyGuard {
    db: sqlx::PgPool,
    lock_key: i64,
    active: bool,
}

impl IdempotencyGuard {
    async fn acquire(db: &sqlx::PgPool, key_hash: &str, route: &str) -> Result<Option<Self>, sqlx::Error> {
        let lock_key = stable_lock_key(key_hash, route);
        let acquired = sqlx::query_scalar::<_, bool>("SELECT pg_try_advisory_lock($1)")
            .bind(lock_key)
            .fetch_one(db)
            .await?;
        if !acquired {
            return Ok(None);
        }
        Ok(Some(Self {
            db: db.clone(),
            lock_key,
            active: true,
        }))
    }
}

impl Drop for IdempotencyGuard {
    fn drop(&mut self) {
        if !self.active {
            return;
        }
        let db = self.db.clone();
        let lock_key = self.lock_key;
        self.active = false;
        tokio::spawn(async move {
            let _ = sqlx::query("SELECT pg_advisory_unlock($1)")
                .bind(lock_key)
                .execute(&db)
                .await;
        });
    }
}

/// POST /api/tasks/:id/runs — queue and start a run with orchestration.
pub async fn start_run(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(task_id): Path<Uuid>,
    headers: HeaderMap,
    Extension(adapter): Extension<Arc<dyn CodexAdapter>>,
) -> ApiResult<StartRunResponse> {
    ensure_task_access(&state.db, auth.user_id, task_id).await?;

    let route = idempotency_route(task_id);
    let idempotency_key = idempotency_key(&headers);
    let key_hash = idempotency_key.as_deref().map(hash_idempotency_key);

    let _idempotency_guard = if let Some(key_hash) = key_hash.as_deref() {
        match IdempotencyGuard::acquire(&state.db, key_hash, &route).await {
            Ok(Some(guard)) => Some(guard),
            Ok(None) => {
                if let Some(cached) =
                    wait_for_idempotent_response(&state, key_hash, &route, 30).await?
                {
                    return Ok(cached);
                }
                return Err((
                    StatusCode::CONFLICT,
                    Json(PlatformError::bad_request(
                        "another identical start_run request is still in progress",
                    )),
                ));
            }
            Err(error) => return Err(db_error(error)),
        }
    } else {
        None
    };

    if let Some(key_hash) = key_hash.as_deref() {
        if let Some(cached) = lookup_idempotent_response(&state, key_hash, &route).await? {
            return Ok(cached);
        }
    }

    ensure_thread_capability(&adapter).await?;

    let existing_active = sqlx::query_scalar::<_, Uuid>(
        "SELECT id FROM runs \
         WHERE task_id = $1 AND status NOT IN ('completed', 'cancelled', 'failed') \
         ORDER BY created_at DESC LIMIT 1",
    )
    .bind(task_id)
    .fetch_optional(&state.db)
    .await
    .map_err(db_error)?;
    if let Some(existing_id) = existing_active {
        return Err((
            StatusCode::CONFLICT,
            Json(PlatformError::bad_request(format!(
                "task {task_id} already has an active run ({existing_id})"
            ))),
        ));
    }

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

    let profile_session = provision_user_profile(&state, &auth, &data_root).await?;

    let workspace_key = match git_workspace::provision_run_workspace(
        &data_root,
        run_id,
        &task.git_url,
        &task.default_branch,
    ) {
        Ok(workspace_key) => workspace_key,
        Err(error) => {
            return fail_run_and_release(
                &state,
                run_id,
                profile_session,
                (
                    StatusCode::BAD_REQUEST,
                    Json(PlatformError::bad_request(format!(
                        "git workspace provisioning failed: {error}"
                    ))),
                ),
            )
            .await;
        }
    };

    if let Err(error) = sqlx::query(
        "INSERT INTO run_workspaces (run_id, state, workspace_key) VALUES ($1, 'ready', $2)",
    )
    .bind(run_id)
    .bind(&workspace_key)
    .execute(&state.db)
    .await
    {
        return fail_run_and_release(&state, run_id, profile_session, db_error(error)).await;
    }

    let workspaces = match adapter.rpc("list_workspaces", json!({})).await {
        Ok(workspaces) => workspaces,
        Err(error) => {
            let _ = transition_run_sync(&state.db, run_id, "provisioning", "failed").await;
            let _ = profile_session.release().await;
            return Err(adapter_error(error));
        }
    };
    let ws_id = match workspaces
        .as_array()
        .and_then(|items| items.first())
        .and_then(|item| item.get("id"))
        .and_then(Value::as_str)
    {
        Some(ws_id) => ws_id.to_string(),
        None => {
            return fail_run_and_release(&state, run_id, profile_session, adapter_error(
                open_web_codex_adapter::AdapterError::Internal("no workspace available".into()),
            ))
            .await;
        }
    };

    let result = match adapter
        .rpc("start_thread", json!({ "workspaceId": ws_id }))
        .await
    {
        Ok(result) => result,
        Err(error) => {
            return fail_run_and_release(&state, run_id, profile_session, adapter_error(error)).await;
        }
    };

    let thread_id = match result
        .get("threadId")
        .or_else(|| result.pointer("/thread/id"))
        .and_then(Value::as_str)
    {
        Some(thread_id) => thread_id.to_string(),
        None => {
            return fail_run_and_release(
                &state,
                run_id,
                profile_session,
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(PlatformError::internal("no threadId in response".to_string())),
                ),
            )
            .await;
        }
    };

    let run = match sqlx::query(
        "UPDATE runs SET status = 'running', codex_thread_id = $1, updated_at = now() \
         WHERE id = $2 AND status = 'provisioning' \
         RETURNING id, task_id, status, codex_thread_id, created_at, updated_at",
    )
    .bind(&thread_id)
    .bind(run_id)
    .fetch_one(&state.db)
    .await
    {
        Ok(run) => run,
        Err(error) => {
            return fail_run_and_release(&state, run_id, profile_session, db_error(error)).await;
        }
    };

    profile_session.release().await?;

    let response = StartRunResponse {
        run: map_run_row(&run),
    };
    if let Some(key_hash) = key_hash.as_deref() {
        store_idempotent_response(&state, key_hash, &route, StatusCode::OK, &response).await?;
    }
    Ok(Json(response))
}

async fn fail_run_and_release(
    state: &AppState,
    run_id: Uuid,
    profile_session: ProfileSession,
    error: (StatusCode, Json<PlatformError>),
) -> ApiResult<StartRunResponse> {
    let _ = transition_run_sync(&state.db, run_id, "provisioning", "failed").await;
    let _ = profile_session.release().await;
    Err(error)
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
    data_root: &std::path::Path,
) -> Result<ProfileSession, (StatusCode, Json<PlatformError>)> {
    let (_layout, lock, home) = provision_profile(data_root, &auth.user_id.to_string()).map_err(|error| {
        if error.kind() == std::io::ErrorKind::AlreadyExists {
            return (
                StatusCode::CONFLICT,
                Json(PlatformError::bad_request(
                    "profile host is already active for this user",
                )),
            );
        }
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(PlatformError::internal(format!("profile provision failed: {error}"))),
        )
    })?;

    let profile_id: Uuid = sqlx::query_scalar(
        "INSERT INTO profiles (user_id, home_path) VALUES ($1, $2) \
         ON CONFLICT (user_id) DO UPDATE SET home_path = EXCLUDED.home_path, updated_at = now() \
         RETURNING id",
    )
    .bind(auth.user_id)
    .bind(home.to_string_lossy().to_string())
    .fetch_one(&state.db)
    .await
    .map_err(db_error)?;

    let lock_token = Uuid::now_v7().to_string();
    let inserted = sqlx::query(
        "INSERT INTO profile_processes (profile_id, lock_token) VALUES ($1, $2) \
         ON CONFLICT (profile_id) DO NOTHING",
    )
    .bind(profile_id)
    .bind(&lock_token)
    .execute(&state.db)
    .await
    .map_err(db_error)?;

    if inserted.rows_affected() == 0 {
        return Err((
            StatusCode::CONFLICT,
            Json(PlatformError::bad_request(
                "profile host is already running for this user",
            )),
        ));
    }

    Ok(ProfileSession {
        db: state.db.clone(),
        profile_id,
        home,
        _lock: lock,
    })
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

async fn wait_for_idempotent_response(
    state: &AppState,
    key_hash: &str,
    route: &str,
    attempts: u32,
) -> Result<Option<Json<StartRunResponse>>, (StatusCode, Json<PlatformError>)> {
    for _ in 0..attempts {
        if let Some(cached) = lookup_idempotent_response(state, key_hash, route).await? {
            return Ok(Some(cached));
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    Ok(None)
}

async fn lookup_idempotent_response(
    state: &AppState,
    key_hash: &str,
    route: &str,
) -> Result<Option<Json<StartRunResponse>>, (StatusCode, Json<PlatformError>)> {
    let row = sqlx::query(
        "SELECT response_status, response_body FROM idempotency_keys \
         WHERE key_hash = $1 AND route = $2 AND expires_at > now()",
    )
    .bind(key_hash)
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
    key_hash: &str,
    route: &str,
    status: StatusCode,
    response: &StartRunResponse,
) -> Result<(), (StatusCode, Json<PlatformError>)> {
    let expires_at = Utc::now() + Duration::hours(24);
    sqlx::query(
        "INSERT INTO idempotency_keys (key_hash, route, response_status, response_body, expires_at) \
         VALUES ($1, $2, $3, $4, $5) \
         ON CONFLICT (key_hash) DO UPDATE \
         SET route = EXCLUDED.route, \
             response_status = EXCLUDED.response_status, \
             response_body = EXCLUDED.response_body, \
             expires_at = EXCLUDED.expires_at",
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

fn idempotency_route(task_id: Uuid) -> String {
    format!("POST /api/tasks/{task_id}/runs")
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

fn stable_lock_key(key_hash: &str, route: &str) -> i64 {
    let mut hasher = Sha256::new();
    hasher.update(key_hash.as_bytes());
    hasher.update(route.as_bytes());
    let digest = hasher.finalize();
    i64::from_be_bytes(digest[..8].try_into().expect("8 byte slice"))
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
    auth: AuthenticatedUser,
    axum::extract::Query(params): axum::extract::Query<HashMap<String, Uuid>>,
) -> ApiResult<Vec<Run>> {
    let task_id = params.get("task_id").copied();

    let rows = if let Some(task_id) = task_id {
        ensure_task_access(&state.db, auth.user_id, task_id).await?;
        sqlx::query(
            "SELECT id, task_id, status, codex_thread_id, created_at, updated_at \
             FROM runs WHERE task_id = $1 ORDER BY created_at DESC",
        )
        .bind(task_id)
        .fetch_all(&state.db)
        .await
        .map_err(db_error)?
    } else {
        sqlx::query(
            "SELECT r.id, r.task_id, r.status, r.codex_thread_id, r.created_at, r.updated_at
             FROM runs r
             JOIN tasks t ON t.id = r.task_id
             JOIN projects p ON p.id = t.project_id
             JOIN memberships m
               ON m.organization_id = p.organization_id
              AND m.user_id = $1
             ORDER BY r.created_at DESC",
        )
        .bind(auth.user_id)
        .fetch_all(&state.db)
        .await
        .map_err(db_error)?
    };

    Ok(Json(rows.iter().map(map_run_row).collect()))
}

/// GET /api/runs/:id — get a single run.
pub async fn get_run(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Run> {
    ensure_run_access(&state.db, auth.user_id, id).await?;

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
    auth: AuthenticatedUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Run> {
    ensure_run_access(&state.db, auth.user_id, id).await?;

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

/// POST /api/runs/:id/interrupt — interrupt the active turn.
pub async fn interrupt_run(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(id): Path<Uuid>,
    Extension(adapter): Extension<Arc<dyn CodexAdapter>>,
    Json(body): Json<TurnControlRequest>,
) -> ApiResult<serde_json::Value> {
    ensure_run_access(&state.db, auth.user_id, id).await?;
    let thread_id = load_run_thread_id(&state.db, id).await?;

    adapter
        .rpc(
            "turn_interrupt",
            json!({
                "threadId": thread_id,
                "turnId": body.turn_id,
            }),
        )
        .await
        .map_err(adapter_error)?;

    Ok(Json(json!({ "status": "interrupted" })))
}

/// POST /api/runs/:id/steer — steer the active turn.
pub async fn steer_run(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(id): Path<Uuid>,
    Extension(adapter): Extension<Arc<dyn CodexAdapter>>,
    Json(body): Json<TurnControlRequest>,
) -> ApiResult<serde_json::Value> {
    ensure_run_access(&state.db, auth.user_id, id).await?;
    let thread_id = load_run_thread_id(&state.db, id).await?;
    let text = body.text.unwrap_or_default();
    if text.trim().is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(PlatformError::bad_request("steer text must not be empty")),
        ));
    }

    adapter
        .rpc(
            "turn_steer",
            json!({
                "threadId": thread_id,
                "turnId": body.turn_id,
                "text": text,
            }),
        )
        .await
        .map_err(adapter_error)?;

    Ok(Json(json!({ "status": "steered" })))
}

#[derive(serde::Deserialize)]
pub struct RunFileQuery {
    pub path: String,
}

/// GET /api/runs/:id/files — list workspace files for a run.
pub async fn list_run_files(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(id): Path<Uuid>,
) -> ApiResult<RunFileListResponse> {
    ensure_run_access(&state.db, auth.user_id, id).await?;
    let workspace_key = crate::run_workspace_api::load_run_workspace_key(&state.db, id)
        .await
        .map_err(workspace_error)?;
    let root = crate::run_workspace_api::workspace_root(
        &crate::run_workspace_api::data_root_from_env(),
        &workspace_key,
    )
    .map_err(workspace_error)?;
    let files = crate::run_workspace_api::list_workspace_files(&root, 2_000)
        .map_err(workspace_error)?;
    Ok(Json(RunFileListResponse { files }))
}

/// GET /api/runs/:id/files/content?path=... — read a workspace file.
pub async fn read_run_file(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(id): Path<Uuid>,
    axum::extract::Query(query): axum::extract::Query<RunFileQuery>,
) -> ApiResult<RunFileContentResponse> {
    ensure_run_access(&state.db, auth.user_id, id).await?;
    let workspace_key = crate::run_workspace_api::load_run_workspace_key(&state.db, id)
        .await
        .map_err(workspace_error)?;
    let root = crate::run_workspace_api::workspace_root(
        &crate::run_workspace_api::data_root_from_env(),
        &workspace_key,
    )
    .map_err(workspace_error)?;
    let file_path = crate::run_workspace_api::resolve_workspace_file(&root, &query.path)
        .map_err(workspace_error)?;
    let (content, truncated) =
        crate::run_workspace_api::read_workspace_file_limited(&file_path, 512 * 1024)
            .map_err(workspace_error)?;
    Ok(Json(RunFileContentResponse {
        path: query.path,
        content,
        truncated,
    }))
}

/// GET /api/runs/:id/git-status — git status for the run workspace.
pub async fn get_run_git_status(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(id): Path<Uuid>,
) -> ApiResult<RunGitStatusResponse> {
    ensure_run_access(&state.db, auth.user_id, id).await?;
    let workspace_key = crate::run_workspace_api::load_run_workspace_key(&state.db, id)
        .await
        .map_err(workspace_error)?;
    let root = crate::run_workspace_api::workspace_root(
        &crate::run_workspace_api::data_root_from_env(),
        &workspace_key,
    )
    .map_err(workspace_error)?;
    let files = crate::run_workspace_api::git_status(&root).map_err(workspace_error)?;
    Ok(Json(RunGitStatusResponse { files }))
}

async fn load_run_thread_id(
    db: &sqlx::PgPool,
    run_id: Uuid,
) -> Result<String, (StatusCode, Json<PlatformError>)> {
    let thread_id = sqlx::query_scalar::<_, Option<String>>(
        "SELECT codex_thread_id FROM runs WHERE id = $1",
    )
    .bind(run_id)
    .fetch_one(db)
    .await
    .map_err(db_error)?;
    thread_id.ok_or_else(|| {
        (
            StatusCode::BAD_REQUEST,
            Json(PlatformError::bad_request(
                "run has no Codex thread yet; try again shortly",
            )),
        )
    })
}

fn workspace_error(error: String) -> (StatusCode, Json<PlatformError>) {
    (
        StatusCode::BAD_REQUEST,
        Json(PlatformError::bad_request(error)),
    )
}
