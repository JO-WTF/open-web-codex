use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Extension, Json,
};
use open_web_codex_adapter::{
    AuthorizedWorkspace, CodexAdapter, ReviewTarget as AdapterReviewTarget,
};
use open_web_codex_platform_contracts::error::{ErrorKind, PlatformError};
use open_web_codex_platform_contracts::{
    InterruptRunRequest, ReviewTarget as PlatformReviewTarget, Run, RunFailureCode, RunReadiness,
    RunReadinessRequest, RunReadinessStatus, StartReviewRequest, StartRunRequest, StartRunResponse,
    SteerRunRequest, TaskAnalysisReadinessRequest,
};
use open_web_codex_platform_store::AppState;
use open_web_codex_provider_service::secured::{AuthorizedProviderOperations, ProviderActor};
use open_web_codex_run_orchestrator::{
    AgentRunSnapshotInput, AgentRunSource, CancelRunRequest, EnqueueRunRequest, ReplayRunRequest,
    RunExecutionSelection, RunOrchestrator, RunOrchestratorError, RunRecord,
};
use open_web_codex_secret_store::PostgresSecretStore;
use sqlx::Row;
use uuid::Uuid;

use crate::middleware::auth::{require_runtime_profile, AuthenticatedUser};
use crate::routes::RuntimeProfileBinding;
use crate::run_readiness;

type ApiResult<T> = Result<Json<T>, (StatusCode, Json<PlatformError>)>;

/// Evaluate the exact browser-selected execution without creating a Task,
/// Run, or Codex Thread.
pub async fn readiness(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(workspace_id): Path<Uuid>,
    Extension(orchestrator): Extension<Arc<RunOrchestrator>>,
    Extension(adapter): Extension<Arc<dyn CodexAdapter>>,
    Extension(providers): Extension<Arc<dyn AuthorizedProviderOperations>>,
    Extension(git): Extension<Arc<open_web_codex_git_runtime::GitRuntime>>,
    Extension(configuration_secrets): Extension<Arc<PostgresSecretStore>>,
    Extension(profile): Extension<RuntimeProfileBinding>,
    Json(request): Json<RunReadinessRequest>,
) -> ApiResult<RunReadiness> {
    require_runtime_profile(&state.db, &auth, &profile.runtime_key).await?;
    let workspace = readiness_workspace(&state, &auth, workspace_id).await?;
    let workspace_id = Uuid::parse_str(&workspace.id).expect("Workspace id was created from UUID");
    let request = resolve_readiness_execution(&orchestrator, &auth, request).await?;
    let catalog = providers
        .list(ProviderActor {
            user_id: auth.user_id,
            organization_id: auth.organization_id,
        })
        .await
        .map_err(|_| {
            (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(PlatformError::internal(
                    "Provider catalog is temporarily unavailable",
                )),
            )
        })?;
    let runtime_healthy = adapter.health().await.is_ok_and(|health| health.ok);
    let browser_map_configured =
        super::configuration::browser_map_configured(&state, &configuration_secrets).await?;
    let evaluated = run_readiness::evaluate(
        &state.db,
        &git,
        &profile,
        auth.organization_id,
        workspace_id,
        &request,
        &catalog,
        runtime_healthy,
        browser_map_configured,
    )
    .await;
    Ok(Json(evaluated.readiness))
}

/// Evaluate task-scoped Analysis Readiness. Unlike Thread readiness this
/// endpoint can require the immutable intake/release binding for the exact
/// Task, so a missing dataset is an explicit blocked result.
pub async fn analysis_readiness(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(task_id): Path<Uuid>,
    Extension(orchestrator): Extension<Arc<RunOrchestrator>>,
    Extension(adapter): Extension<Arc<dyn CodexAdapter>>,
    Extension(providers): Extension<Arc<dyn AuthorizedProviderOperations>>,
    Extension(git): Extension<Arc<open_web_codex_git_runtime::GitRuntime>>,
    Extension(configuration_secrets): Extension<Arc<PostgresSecretStore>>,
    Extension(profile): Extension<RuntimeProfileBinding>,
    Json(mut request): Json<TaskAnalysisReadinessRequest>,
) -> ApiResult<RunReadiness> {
    require_runtime_profile(&state.db, &auth, &profile.runtime_key).await?;
    let task_exists: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM tasks WHERE id = $1 AND organization_id = $2)",
    )
    .bind(task_id)
    .bind(auth.organization_id)
    .fetch_one(&state.db)
    .await
    .map_err(database_error)?;
    if !task_exists {
        return Err((
            StatusCode::NOT_FOUND,
            Json(PlatformError::not_found("Task was not found")),
        ));
    }
    let workspace = readiness_workspace(&state, &auth, request.workspace_id).await?;
    let workspace_id = Uuid::parse_str(&workspace.id).expect("Workspace id was created from UUID");
    request.execution.purpose = open_web_codex_platform_contracts::RunStartPurpose::Analysis;
    request.execution.task_id = Some(task_id);
    let execution = resolve_readiness_execution(&orchestrator, &auth, request.execution).await?;
    let catalog = providers
        .list(ProviderActor {
            user_id: auth.user_id,
            organization_id: auth.organization_id,
        })
        .await
        .map_err(|_| {
            (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(PlatformError::internal(
                    "Provider catalog is temporarily unavailable",
                )),
            )
        })?;
    let runtime_healthy = adapter.health().await.is_ok_and(|health| health.ok);
    let browser_map_configured =
        super::configuration::browser_map_configured(&state, &configuration_secrets).await?;
    let evaluated = run_readiness::evaluate(
        &state.db,
        &git,
        &profile,
        auth.organization_id,
        workspace_id,
        &execution,
        &catalog,
        runtime_healthy,
        browser_map_configured,
    )
    .await;
    Ok(Json(evaluated.readiness))
}

/// Queue a Run against an existing authorized Workspace. The worker owns only
/// Run scheduling and Runtime delivery; Workspace provisioning is independent.
pub async fn start_run(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(task_id): Path<Uuid>,
    Extension(orchestrator): Extension<Arc<RunOrchestrator>>,
    Extension(adapter): Extension<Arc<dyn CodexAdapter>>,
    Extension(providers): Extension<Arc<dyn AuthorizedProviderOperations>>,
    Extension(git): Extension<Arc<open_web_codex_git_runtime::GitRuntime>>,
    Extension(configuration_secrets): Extension<Arc<PostgresSecretStore>>,
    Extension(profile): Extension<RuntimeProfileBinding>,
    Json(req): Json<StartRunRequest>,
) -> ApiResult<StartRunResponse> {
    require_runtime_profile(&state.db, &auth, &profile.runtime_key).await?;
    if req.purpose == open_web_codex_platform_contracts::RunStartPurpose::Analysis {
        return Err((
            StatusCode::CONFLICT,
            Json(PlatformError::bad_request(
                "analysis_start_required: analysis Turns must be created through /api/tasks/:task_id/analysis-start",
            )),
        ));
    }
    if req.supervisor_policy.is_some() && req.agent.is_some() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(PlatformError::bad_request(
                "Select either one root Agent or one Supervisor Policy",
            )),
        ));
    }
    let workspace = readiness_workspace(&state, &auth, req.workspace_id).await?;
    let workspace_id = Uuid::parse_str(&workspace.id).expect("Workspace id was created from UUID");
    let replay_execution = if req.fork_thread_id.is_some() || req.fork_source_run_id.is_some() {
        RunExecutionSelection::Inherited
    } else if let Some(selection) = req.supervisor_policy.as_ref() {
        RunExecutionSelection::Supervisor {
            policy_id: selection.policy_id.clone(),
            version: selection.version.clone(),
        }
    } else if let Some(selection) = req.agent.as_ref() {
        RunExecutionSelection::Agent {
            definition_id: selection.definition_id.clone(),
            version: selection.version.clone(),
            release_id: selection.release_id,
        }
    } else {
        RunExecutionSelection::Standard
    };
    if let Some(run) = orchestrator
        .replay_run(ReplayRunRequest {
            organization_id: auth.organization_id,
            actor_id: auth.user_id,
            task_id,
            idempotency_key: req.idempotency_key.clone(),
            workspace_id,
            fork_thread_id: req.fork_thread_id.clone(),
            fork_source_run_id: req.fork_source_run_id,
            execution: replay_execution,
        })
        .await
        .map_err(orchestrator_error)?
    {
        return Ok(Json(StartRunResponse {
            run: run_from_record(run),
        }));
    }
    let task = sqlx::query(
        "SELECT model_provider, model FROM tasks WHERE id = $1 AND organization_id = $2",
    )
    .bind(task_id)
    .bind(auth.organization_id)
    .fetch_optional(&state.db)
    .await
    .map_err(database_error)?
    .ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(PlatformError::not_found("Task was not found")),
        )
    })?;
    let readiness_request = RunReadinessRequest {
        model_provider: task
            .get::<Option<String>, _>("model_provider")
            .unwrap_or_default(),
        model: task.get::<Option<String>, _>("model").unwrap_or_default(),
        supervisor_policy: req.supervisor_policy.clone(),
        agent: req.agent.clone(),
        fork_thread_id: req.fork_thread_id.clone(),
        fork_source_run_id: req.fork_source_run_id,
        purpose: req.purpose,
        task_id: Some(task_id),
    };
    let readiness_request =
        resolve_readiness_execution(&orchestrator, &auth, readiness_request).await?;
    let catalog = providers
        .list(ProviderActor {
            user_id: auth.user_id,
            organization_id: auth.organization_id,
        })
        .await
        .map_err(|_| {
            (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(PlatformError::internal(
                    "Provider catalog is temporarily unavailable",
                )),
            )
        })?;
    let runtime_healthy = adapter.health().await.is_ok_and(|health| health.ok);
    let browser_map_configured =
        super::configuration::browser_map_configured(&state, &configuration_secrets).await?;
    let evaluated = run_readiness::evaluate(
        &state.db,
        &git,
        &profile,
        auth.organization_id,
        workspace_id,
        &readiness_request,
        &catalog,
        runtime_healthy,
        browser_map_configured,
    )
    .await;
    if req.readiness_fingerprint != evaluated.readiness.evaluation_fingerprint {
        return Err(readiness_changed());
    }
    if evaluated.readiness.status == RunReadinessStatus::Blocked {
        return Err(run_not_ready());
    }
    let forked = req.fork_thread_id.is_some();
    let supervisor_policy = if forked {
        None
    } else {
        evaluated.supervisor_policy.map(|policy| policy.snapshot)
    };
    let agent = if forked {
        None
    } else if let Some(resolved) = evaluated.agent {
        Some(AgentRunSnapshotInput {
            definition_id: resolved.definition_id,
            version: resolved.version,
            display_name: resolved.display_name,
            content_sha256: resolved.content_sha256,
            source: if resolved.release_id.is_some() {
                AgentRunSource::UserRelease
            } else {
                AgentRunSource::Repository
            },
            release_id: resolved.release_id,
        })
    } else {
        None
    };
    let run = orchestrator
        .enqueue_run(EnqueueRunRequest {
            organization_id: auth.organization_id,
            actor_id: auth.user_id,
            task_id,
            idempotency_key: req.idempotency_key,
            workspace_id,
            fork_thread_id: req.fork_thread_id,
            fork_source_run_id: req.fork_source_run_id,
            supervisor_policy,
            agent,
        })
        .await
        .map_err(orchestrator_error)?;
    Ok(Json(StartRunResponse {
        run: run_from_record(run),
    }))
}

/// GET /api/runs?task_id=... — list runs for a task.
pub async fn list_runs(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, Uuid>>,
) -> ApiResult<Vec<Run>> {
    let task_id = params.get("task_id").copied();
    let rows = if let Some(task_id) = task_id {
        sqlx::query(
            "SELECT id, task_id, status, failure_code, codex_thread_id, active_turn_id, workspace_id, \
                    attempt, created_at, updated_at FROM runs \
             WHERE task_id = $1 AND organization_id = $2 ORDER BY created_at DESC",
        )
        .bind(task_id)
        .bind(auth.organization_id)
        .fetch_all(&state.db)
        .await
        .map_err(database_error)?
    } else {
        sqlx::query(
            "SELECT id, task_id, status, failure_code, codex_thread_id, active_turn_id, workspace_id, \
                    attempt, created_at, updated_at FROM runs \
             WHERE organization_id = $1 ORDER BY created_at DESC",
        )
        .bind(auth.organization_id)
        .fetch_all(&state.db)
        .await
        .map_err(database_error)?
    };
    Ok(Json(rows.iter().map(run_from_row).collect()))
}

/// GET /api/runs/:id — get a single run.
pub async fn get_run(
    auth: AuthenticatedUser,
    Path(id): Path<Uuid>,
    Extension(orchestrator): Extension<Arc<RunOrchestrator>>,
) -> ApiResult<Run> {
    let run = orchestrator
        .get_run(auth.organization_id, id)
        .await
        .map_err(orchestrator_error)?;
    Ok(Json(run_from_record(run)))
}

/// Cancel a Run and interrupt its projected active Turn when one exists.
pub async fn cancel_run(
    auth: AuthenticatedUser,
    Path(id): Path<Uuid>,
    Extension(orchestrator): Extension<Arc<RunOrchestrator>>,
) -> ApiResult<Run> {
    let run = orchestrator
        .cancel_run(CancelRunRequest {
            organization_id: auth.organization_id,
            actor_id: auth.user_id,
            allow_organization_admin: matches!(auth.organization_role.as_str(), "owner" | "admin"),
            run_id: id,
        })
        .await
        .map_err(orchestrator_error)?;
    Ok(Json(run_from_record(run)))
}

/// Interrupt the active Turn without cancelling its durable Run.
pub async fn interrupt_run(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(id): Path<Uuid>,
    Extension(adapter): Extension<Arc<dyn CodexAdapter>>,
    Json(request): Json<InterruptRunRequest>,
) -> ApiResult<serde_json::Value> {
    let context = authorized_turn_context(&state, &auth, id).await?;
    if context.turn_id != request.turn_id {
        return Err((
            StatusCode::CONFLICT,
            Json(PlatformError::bad_request(
                "active Turn changed; reload before interrupting",
            )),
        ));
    }
    adapter
        .interrupt_turn(&context.workspace, &context.thread_id, &request.turn_id)
        .await
        .map_err(adapter_control_error)?;
    sqlx::query(
        "UPDATE runs SET active_turn_id = NULL, updated_at = now() \
         WHERE id = $1 AND organization_id = $2 AND active_turn_id = $3",
    )
    .bind(id)
    .bind(auth.organization_id)
    .bind(&request.turn_id)
    .execute(&state.db)
    .await
    .map_err(database_error)?;
    Ok(Json(serde_json::json!({ "status": "interrupted" })))
}

/// Add a follow-up to the active Turn while enforcing the projected Turn id.
pub async fn steer_run(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(id): Path<Uuid>,
    Extension(adapter): Extension<Arc<dyn CodexAdapter>>,
    Json(request): Json<SteerRunRequest>,
) -> ApiResult<serde_json::Value> {
    if request.text.trim().is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(PlatformError::bad_request("message text must not be empty")),
        ));
    }
    let context = authorized_turn_context(&state, &auth, id).await?;
    if context.turn_id != request.turn_id {
        return Err((
            StatusCode::CONFLICT,
            Json(PlatformError::bad_request(
                "active Turn changed; reload before steering",
            )),
        ));
    }
    let result = adapter
        .steer_turn(
            &context.workspace,
            &context.thread_id,
            &request.turn_id,
            &request.text,
            &request.images,
        )
        .await
        .map_err(adapter_control_error)?;
    Ok(Json(
        serde_json::json!({ "status": "steered", "result": result }),
    ))
}

pub async fn compact_run_thread(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(id): Path<Uuid>,
    Extension(adapter): Extension<Arc<dyn CodexAdapter>>,
) -> ApiResult<serde_json::Value> {
    let context = authorized_thread_context(&state, &auth, id).await?;
    if context.turn_id.is_some() {
        return Err((
            StatusCode::CONFLICT,
            Json(PlatformError::bad_request(
                "interrupt the active Turn before compacting",
            )),
        ));
    }
    adapter
        .compact_thread(&context.workspace, &context.thread_id)
        .await
        .map_err(adapter_control_error)?;
    Ok(Json(serde_json::json!({ "status": "compacting" })))
}

pub async fn start_review(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(id): Path<Uuid>,
    Extension(adapter): Extension<Arc<dyn CodexAdapter>>,
    Json(request): Json<StartReviewRequest>,
) -> ApiResult<serde_json::Value> {
    if request
        .delivery
        .as_deref()
        .is_some_and(|value| value != "inline")
    {
        return Err((
            StatusCode::CONFLICT,
            Json(PlatformError::bad_request(
                "detached reviews require a durable child Run and are not enabled",
            )),
        ));
    }
    let context = authorized_thread_context(&state, &auth, id).await?;
    if context.turn_id.is_some() {
        return Err((
            StatusCode::CONFLICT,
            Json(PlatformError::bad_request(
                "interrupt the active Turn before starting review",
            )),
        ));
    }
    let target = review_target(request.target)?;
    let result = adapter
        .start_review(&context.workspace, &context.thread_id, target)
        .await
        .map_err(adapter_control_error)?;
    if let Some(turn_id) = result
        .pointer("/turn/id")
        .and_then(serde_json::Value::as_str)
    {
        sqlx::query(
            "UPDATE runs SET active_turn_id = $1, updated_at = now() \
             WHERE id = $2 AND organization_id = $3 AND status = 'running'",
        )
        .bind(turn_id)
        .bind(id)
        .bind(auth.organization_id)
        .execute(&state.db)
        .await
        .map_err(database_error)?;
    }
    Ok(Json(result))
}

struct AuthorizedTurnContext {
    workspace: AuthorizedWorkspace,
    thread_id: String,
    turn_id: String,
}

struct AuthorizedThreadContext {
    workspace: AuthorizedWorkspace,
    thread_id: String,
    turn_id: Option<String>,
}

async fn authorized_turn_context(
    state: &AppState,
    auth: &AuthenticatedUser,
    run_id: Uuid,
) -> Result<AuthorizedTurnContext, (StatusCode, Json<PlatformError>)> {
    let context = authorized_thread_context(state, auth, run_id).await?;
    Ok(AuthorizedTurnContext {
        workspace: context.workspace,
        thread_id: context.thread_id,
        turn_id: context.turn_id.ok_or_else(|| {
            (
                StatusCode::CONFLICT,
                Json(PlatformError::bad_request("Run has no active Turn")),
            )
        })?,
    })
}

async fn authorized_thread_context(
    state: &AppState,
    auth: &AuthenticatedUser,
    run_id: Uuid,
) -> Result<AuthorizedThreadContext, (StatusCode, Json<PlatformError>)> {
    let row = sqlx::query(
        "SELECT run.codex_thread_id, run.active_turn_id, run.workspace_id, \
                run.requested_by, workspace.root_path \
         FROM runs run \
         JOIN workspaces workspace ON workspace.id = run.workspace_id \
           AND workspace.organization_id = run.organization_id \
         JOIN workspace_grants workspace_grant ON workspace_grant.workspace_id = workspace.id \
           AND workspace_grant.organization_id = workspace.organization_id \
           AND workspace_grant.user_id = run.requested_by \
           AND workspace_grant.profile_id = workspace.profile_id \
           AND workspace_grant.role IN ('owner', 'write') \
         WHERE run.id = $1 AND run.organization_id = $2 \
           AND run.status = 'running' AND workspace.state IN ('ready', 'retained')",
    )
    .bind(run_id)
    .bind(auth.organization_id)
    .fetch_optional(&state.db)
    .await
    .map_err(database_error)?
    .ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(PlatformError::not_found("active Run was not found")),
        )
    })?;
    let requested_by: Option<Uuid> = row.get("requested_by");
    if requested_by != Some(auth.user_id)
        && !matches!(auth.organization_role.as_str(), "owner" | "admin")
    {
        return Err((
            StatusCode::NOT_FOUND,
            Json(PlatformError::not_found("active Run was not found")),
        ));
    }
    let workspace_id: Option<Uuid> = row.get("workspace_id");
    let root_path: String = row.get("root_path");
    let thread_id: Option<String> = row.get("codex_thread_id");
    let turn_id: Option<String> = row.get("active_turn_id");
    Ok(AuthorizedThreadContext {
        workspace: AuthorizedWorkspace {
            id: workspace_id
                .ok_or_else(|| {
                    (
                        StatusCode::CONFLICT,
                        Json(PlatformError::bad_request(
                            "the selected Workspace is not ready",
                        )),
                    )
                })?
                .to_string(),
            root: root_path.into(),
        },
        thread_id: thread_id.ok_or_else(|| {
            (
                StatusCode::CONFLICT,
                Json(PlatformError::bad_request("Run Thread is not ready")),
            )
        })?,
        turn_id,
    })
}

fn review_target(
    target: PlatformReviewTarget,
) -> Result<AdapterReviewTarget, (StatusCode, Json<PlatformError>)> {
    let invalid = || {
        (
            StatusCode::BAD_REQUEST,
            Json(PlatformError::bad_request("review target is invalid")),
        )
    };
    Ok(match target {
        PlatformReviewTarget::UncommittedChanges => AdapterReviewTarget::UncommittedChanges,
        PlatformReviewTarget::BaseBranch { branch } => {
            let branch = branch.trim();
            if branch.is_empty() || branch.len() > 255 || branch.starts_with('-') {
                return Err(invalid());
            }
            AdapterReviewTarget::BaseBranch {
                branch: branch.to_string(),
            }
        }
        PlatformReviewTarget::Commit { sha, title } => {
            let sha = sha.trim();
            if !(7..=64).contains(&sha.len()) || !sha.bytes().all(|byte| byte.is_ascii_hexdigit()) {
                return Err(invalid());
            }
            if title.as_ref().is_some_and(|value| value.len() > 500) {
                return Err(invalid());
            }
            AdapterReviewTarget::Commit {
                sha: sha.to_string(),
                title,
            }
        }
        PlatformReviewTarget::Custom { instructions } => {
            let instructions = instructions.trim();
            if instructions.is_empty() || instructions.len() > 10_000 {
                return Err(invalid());
            }
            AdapterReviewTarget::Custom {
                instructions: instructions.to_string(),
            }
        }
    })
}

fn adapter_control_error(
    _error: open_web_codex_adapter::AdapterError,
) -> (StatusCode, Json<PlatformError>) {
    (
        StatusCode::BAD_GATEWAY,
        Json(PlatformError::internal("Codex Turn control failed")),
    )
}

fn run_from_record(run: RunRecord) -> Run {
    Run {
        id: run.id,
        task_id: run.task_id,
        status: run.status,
        failure_code: run
            .failure_code
            .as_deref()
            .map(RunFailureCode::from_persisted),
        codex_thread_id: run.codex_thread_id,
        active_turn_id: run.active_turn_id,
        workspace_id: run.workspace_id,
        attempt: run.attempt,
        created_at: run.created_at,
        updated_at: run.updated_at,
    }
}

async fn resolve_readiness_execution(
    orchestrator: &RunOrchestrator,
    auth: &AuthenticatedUser,
    mut request: RunReadinessRequest,
) -> Result<RunReadinessRequest, (StatusCode, Json<PlatformError>)> {
    match (
        request.fork_thread_id.as_deref(),
        request.fork_source_run_id,
    ) {
        (None, None) => Ok(request),
        (Some(thread_id), Some(source_run_id)) => {
            if request.supervisor_policy.is_some() || request.agent.is_some() {
                return Err((
                    StatusCode::BAD_REQUEST,
                    Json(PlatformError::bad_request(
                        "Fork readiness inherits its source execution selection",
                    )),
                ));
            }
            match orchestrator
                .resolve_fork_execution(
                    auth.organization_id,
                    auth.user_id,
                    source_run_id,
                    thread_id,
                )
                .await
                .map_err(orchestrator_error)?
            {
                RunExecutionSelection::Standard => {}
                RunExecutionSelection::Supervisor { policy_id, version } => {
                    request.supervisor_policy = Some(
                        open_web_codex_platform_contracts::SupervisorPolicySelection {
                            policy_id,
                            version,
                        },
                    );
                }
                RunExecutionSelection::Agent {
                    definition_id,
                    version,
                    release_id,
                } => {
                    request.agent = Some(open_web_codex_platform_contracts::AgentRunSelection {
                        definition_id,
                        version,
                        release_id,
                    });
                }
                RunExecutionSelection::Inherited => {
                    return Err((
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(PlatformError::internal(
                            "Fork execution identity could not be resolved",
                        )),
                    ));
                }
            }
            Ok(request)
        }
        _ => Err((
            StatusCode::BAD_REQUEST,
            Json(PlatformError::bad_request(
                "Fork source Thread and Run must be provided together",
            )),
        )),
    }
}

fn run_from_row(row: &sqlx::postgres::PgRow) -> Run {
    Run {
        id: row.get("id"),
        task_id: row.get("task_id"),
        status: row.get("status"),
        failure_code: row
            .get::<Option<String>, _>("failure_code")
            .as_deref()
            .map(RunFailureCode::from_persisted),
        codex_thread_id: row.get("codex_thread_id"),
        active_turn_id: row.get("active_turn_id"),
        workspace_id: row.get("workspace_id"),
        attempt: row.get("attempt"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    }
}

pub(crate) fn orchestrator_error(error: RunOrchestratorError) -> (StatusCode, Json<PlatformError>) {
    let (status, platform) = match error {
        RunOrchestratorError::Invalid(message) => {
            (StatusCode::BAD_REQUEST, PlatformError::bad_request(message))
        }
        RunOrchestratorError::NotFound => (
            StatusCode::NOT_FOUND,
            PlatformError::not_found("Run resource was not found"),
        ),
        RunOrchestratorError::Conflict(message) => {
            (StatusCode::CONFLICT, PlatformError::bad_request(message))
        }
        RunOrchestratorError::Adapter(_) => (
            StatusCode::BAD_GATEWAY,
            PlatformError::internal("Codex Runtime operation failed"),
        ),
        RunOrchestratorError::StartPreflight(_) => (
            StatusCode::CONFLICT,
            PlatformError::bad_request("Runtime start requirements are unavailable"),
        ),
        RunOrchestratorError::LeaseLost => (
            StatusCode::CONFLICT,
            PlatformError::bad_request("Run ownership changed; reload its current state"),
        ),
        RunOrchestratorError::Database(_) | RunOrchestratorError::Git(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            PlatformError::internal("Run operation failed"),
        ),
    };
    (status, Json(platform))
}

fn database_error(_error: sqlx::Error) -> (StatusCode, Json<PlatformError>) {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(PlatformError::internal("database operation failed")),
    )
}
fn readiness_changed() -> (StatusCode, Json<PlatformError>) {
    (
        StatusCode::CONFLICT,
        Json(PlatformError {
            kind: ErrorKind::Conflict,
            message: "readiness_changed".to_string(),
            request_id: None,
            retry_after_ms: None,
        }),
    )
}

async fn readiness_workspace(
    state: &AppState,
    auth: &AuthenticatedUser,
    workspace_id: Uuid,
) -> Result<AuthorizedWorkspace, (StatusCode, Json<PlatformError>)> {
    let workspace_id =
        match super::workspaces::authorized_workspace(state, auth, workspace_id, false).await {
            Ok(workspace_id) => workspace_id,
            Err((status, _))
                if status == StatusCode::NOT_FOUND || status == StatusCode::CONFLICT =>
            {
                return Err((
                    StatusCode::CONFLICT,
                    Json(PlatformError::workspace_unavailable(
                        "workspace_unavailable",
                    )),
                ));
            }
            Err(error) => return Err(error),
        };
    let root = sqlx::query_scalar::<_, String>(
        "SELECT root_path FROM workspaces WHERE id = $1 AND organization_id = $2",
    )
    .bind(workspace_id)
    .bind(auth.organization_id)
    .fetch_optional(&state.db)
    .await
    .map_err(database_error)?
    .ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(PlatformError::not_found("Workspace was not found")),
        )
    })?;
    Ok(AuthorizedWorkspace {
        id: workspace_id.to_string(),
        root: root.into(),
    })
}

fn run_not_ready() -> (StatusCode, Json<PlatformError>) {
    (
        StatusCode::CONFLICT,
        Json(PlatformError {
            kind: ErrorKind::Conflict,
            message: "run_not_ready".to_string(),
            request_id: None,
            retry_after_ms: None,
        }),
    )
}

#[cfg(test)]
mod tests {
    use super::run_from_record;
    use chrono::Utc;
    use open_web_codex_platform_contracts::RunFailureCode;
    use open_web_codex_run_orchestrator::RunRecord;
    use uuid::Uuid;

    #[test]
    fn run_projection_includes_the_safe_failure_code() {
        let projected = run_from_record(RunRecord {
            id: Uuid::now_v7(),
            task_id: Uuid::now_v7(),
            status: "failed".to_string(),
            failure_code: Some("runtime_start_preflight_failed".to_string()),
            codex_thread_id: None,
            active_turn_id: None,
            workspace_id: Some(Uuid::now_v7()),
            attempt: 1,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        });

        assert_eq!(
            projected.failure_code,
            Some(RunFailureCode::RuntimeStartPreflightFailed)
        );
        assert_eq!(
            serde_json::to_value(projected).unwrap()["failure_code"],
            "runtime_start_preflight_failed"
        );
    }

    #[test]
    fn run_projection_bounds_unknown_persisted_failure_codes() {
        let projected = run_from_record(RunRecord {
            id: Uuid::now_v7(),
            task_id: Uuid::now_v7(),
            status: "failed".to_string(),
            failure_code: Some("raw internal failure detail".to_string()),
            codex_thread_id: None,
            active_turn_id: None,
            workspace_id: Some(Uuid::now_v7()),
            attempt: 1,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        });

        assert_eq!(projected.failure_code, Some(RunFailureCode::UnknownFailure));
        assert_eq!(
            serde_json::to_value(projected).unwrap()["failure_code"],
            "unknown_failure"
        );
    }
}
