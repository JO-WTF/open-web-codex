use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Extension, Json,
};
use open_web_codex_adapter::{AuthorizedWorkspace, CodexAdapter, TurnOptions};
use open_web_codex_platform_contracts::error::PlatformError;
use open_web_codex_platform_contracts::{
    CreateTaskRequest, ExplicitResourceSelection, ListTaskEventsParams, ResourceReferenceSummary,
    RunEvent, SendMessageRequest, SendMessageResponse, Task, ThreadModelSettingsUpdateResponse,
    UpdateThreadModelSettingsRequest,
};
use open_web_codex_platform_store::AppState;
use open_web_codex_run_orchestrator::{
    ContinueThreadRunRequest, RecoverRunRequest, RunOrchestrator, RunRecord,
    StartedThreadTurnRequest,
};
use serde::Deserialize;
use sqlx::Row;
use std::collections::HashSet;
use std::sync::Arc;
use uuid::Uuid;

use crate::copilot_installation::{CopilotInstallationError, CopilotInstallationService};
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
        "SELECT id, project_id, workspace_id, title, status, copilot_package_id, created_at, updated_at \
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
            workspace_id: row.get("workspace_id"),
            title: row.get("title"),
            status: row.get("status"),
            copilot_package_id: row.get("copilot_package_id"),
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
    Extension(profile): Extension<RuntimeProfileBinding>,
    Extension(copilots): Extension<Arc<CopilotInstallationService>>,
    Json(req): Json<CreateTaskRequest>,
) -> ApiResult<Task> {
    if req.title.trim().is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(PlatformError::bad_request("title must not be empty")),
        ));
    }
    require_runtime_profile(&state.db, &auth, &profile.runtime_key).await?;
    let copilot_selection = copilots
        .task_selection(req.copilot_package_id.as_deref())
        .await
        .map_err(copilot_selection_error)?;

    let row = sqlx::query(
        "INSERT INTO tasks \
         (organization_id, project_id, workspace_id, created_by, title, copilot_package_id) \
         SELECT project.organization_id, project.id, workspace.id, $4, $2, $5 \
         FROM projects project \
         JOIN profiles runtime_profile ON runtime_profile.organization_id = project.organization_id \
           AND runtime_profile.owner_user_id = $4 AND runtime_profile.runtime_key = $6 \
           AND runtime_profile.status = 'active' \
         JOIN workspaces workspace ON workspace.id = $3 \
           AND workspace.organization_id = project.organization_id \
           AND workspace.project_id = project.id \
           AND workspace.profile_id = runtime_profile.id \
           AND workspace.state IN ('ready', 'retained') \
         JOIN workspace_grants workspace_grant \
           ON workspace_grant.workspace_id = workspace.id \
          AND workspace_grant.organization_id = workspace.organization_id \
          AND workspace_grant.user_id = $4 \
          AND workspace_grant.profile_id = runtime_profile.id \
          AND workspace_grant.role IN ('owner', 'write') \
         WHERE project.id = $1 AND project.organization_id = $7 \
         RETURNING id, project_id, workspace_id, title, status, copilot_package_id, created_at, updated_at",
    )
    .bind(req.project_id)
    .bind(&req.title)
    .bind(req.workspace_id)
    .bind(auth.user_id)
    .bind(copilot_selection.as_ref().map(|value| value.package_id.as_str()))
    .bind(&profile.runtime_key)
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
            Json(PlatformError::not_found(
                "project or authorized Workspace was not found",
            )),
        )
    })?;

    Ok(Json(Task {
        id: row.get("id"),
        project_id: row.get("project_id"),
        workspace_id: row.get("workspace_id"),
        title: row.get("title"),
        status: row.get("status"),
        copilot_package_id: row.get("copilot_package_id"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    }))
}

/// PUT /api/tasks/:id/model-selection — update a materialized Thread's model
/// without permitting a cross-Provider mutation.
pub async fn update_model_selection(
    auth: AuthenticatedUser,
    State(state): State<AppState>,
    Path(task_id): Path<Uuid>,
    Extension(adapter): Extension<Arc<dyn CodexAdapter>>,
    Extension(profile): Extension<RuntimeProfileBinding>,
    Json(request): Json<UpdateThreadModelSettingsRequest>,
) -> ApiResult<ThreadModelSettingsUpdateResponse> {
    let (provider_id, model_id) =
        normalize_thread_model_settings(request.provider_id, request.model_id)?;
    require_runtime_profile(&state.db, &auth, &profile.runtime_key).await?;
    let run = sqlx::query(
        "SELECT r.codex_thread_id, r.workspace_id, w.root_path \
         FROM runs r JOIN tasks t ON t.id = r.task_id \
           AND t.organization_id = r.organization_id \
           AND t.workspace_id = r.workspace_id \
         JOIN workspaces w ON w.id = r.workspace_id \
           AND w.organization_id = r.organization_id \
           AND w.state IN ('ready', 'retained') \
         JOIN workspace_grants workspace_grant ON workspace_grant.workspace_id = w.id \
           AND workspace_grant.organization_id = w.organization_id \
           AND workspace_grant.user_id = $3 AND workspace_grant.profile_id = w.profile_id \
           AND workspace_grant.role IN ('owner', 'write') \
         WHERE r.task_id = $1 AND r.organization_id = $2 AND r.requested_by = $3 \
           AND r.codex_thread_id IS NOT NULL \
           AND r.status IN ('running', 'recovery_pending', 'completed') \
         ORDER BY CASE WHEN r.status IN ('running', 'recovery_pending') THEN 0 ELSE 1 END, \
                  r.created_at DESC LIMIT 1",
    )
    .bind(task_id)
    .bind(auth.organization_id)
    .bind(auth.user_id)
    .fetch_optional(&state.db)
    .await
    .map_err(database_error)?
    .ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(PlatformError::not_found(
                "materialized Task Thread was not found",
            )),
        )
    })?;
    let thread_id: String = run.get("codex_thread_id");
    let workspace = AuthorizedWorkspace {
        id: run.get::<Uuid, _>("workspace_id").to_string(),
        root: run.get::<String, _>("root_path").into(),
    };
    update_materialized_thread_model(&*adapter, &workspace, &thread_id, provider_id, model_id)
        .await
        .map(Json)
        .map_err(thread_settings_runtime_error)
}

async fn update_materialized_thread_model(
    adapter: &dyn CodexAdapter,
    workspace: &AuthorizedWorkspace,
    thread_id: &str,
    provider_id: String,
    model_id: String,
) -> Result<ThreadModelSettingsUpdateResponse, open_web_codex_adapter::AdapterError> {
    let before = adapter
        .read_thread_model_settings(workspace, thread_id)
        .await?;
    if before.model_provider != provider_id {
        return Ok(ThreadModelSettingsUpdateResponse::RequiresNewThread {
            provider_id,
            model_id,
        });
    }
    adapter
        .update_thread_model(workspace, thread_id, &model_id)
        .await?;
    let after = adapter
        .read_thread_model_settings(workspace, thread_id)
        .await?;
    if after.model_provider != before.model_provider || after.model != model_id {
        return Err(open_web_codex_adapter::AdapterError::Rpc(
            "Runtime did not confirm the Thread model update".to_string(),
        ));
    }
    Ok(ThreadModelSettingsUpdateResponse::Updated {
        provider_id: after.model_provider,
        model_id: after.model,
    })
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
            crate::event_projection::project_public_run_event(RunEvent {
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
            })
        })
        .collect();

    Ok(Json(events))
}

/// GET /api/tasks/:id/resource-refs — list exact provider-owned Resource
/// references produced in the same authorized Profile + Workspace. This is a
/// selector projection, not a Resource content API.
pub async fn list_task_resource_refs(
    auth: AuthenticatedUser,
    State(state): State<AppState>,
    Extension(profile): Extension<RuntimeProfileBinding>,
    Path(task_id): Path<Uuid>,
) -> ApiResult<Vec<ResourceReferenceSummary>> {
    require_runtime_profile(&state.db, &auth, &profile.runtime_key).await?;
    let rows = sqlx::query(
        "SELECT projection.producer_event_id, projection.ordinal, projection.server,
                projection.resource_uri, projection.resource_schema, projection.display_name,
                projection.producer_tool, projection.created_at
         FROM resource_ref_projections projection
         JOIN tasks task ON task.id = $1
           AND task.organization_id = projection.organization_id
           AND task.workspace_id = projection.workspace_id
         JOIN profiles runtime_profile ON runtime_profile.id = projection.profile_id
           AND runtime_profile.organization_id = projection.organization_id
           AND runtime_profile.owner_user_id = $2
           AND runtime_profile.runtime_key = $3
           AND runtime_profile.status = 'active'
         JOIN workspace_grants workspace_grant
           ON workspace_grant.workspace_id = projection.workspace_id
          AND workspace_grant.organization_id = projection.organization_id
          AND workspace_grant.user_id = $2
          AND workspace_grant.profile_id = projection.profile_id
          AND workspace_grant.role IN ('owner', 'write')
         WHERE projection.organization_id = $4
         ORDER BY projection.created_at DESC, projection.ordinal ASC
         LIMIT 200",
    )
    .bind(task_id)
    .bind(auth.user_id)
    .bind(&profile.runtime_key)
    .bind(auth.organization_id)
    .fetch_all(&state.db)
    .await
    .map_err(database_error)?;
    Ok(Json(
        rows.into_iter()
            .map(|row| ResourceReferenceSummary {
                producer_event_id: row.get("producer_event_id"),
                ordinal: row.get("ordinal"),
                server: row.get("server"),
                uri: row.get("resource_uri"),
                resource_schema: row.get("resource_schema"),
                display_name: row.get("display_name"),
                producer_tool: row.get("producer_tool"),
                created_at: row.get("created_at"),
            })
            .collect(),
    ))
}

/// POST /api/tasks/:id/messages — send a user message to the task's active thread.
pub async fn send_message(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(task_id): Path<Uuid>,
    Extension(adapter): Extension<Arc<dyn CodexAdapter>>,
    Extension(orchestrator): Extension<Arc<RunOrchestrator>>,
    Extension(profile): Extension<RuntimeProfileBinding>,
    Json(req): Json<SendMessageRequest>,
) -> ApiResult<SendMessageResponse> {
    if req.text.trim().is_empty() && req.images.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(PlatformError::bad_request(
                "message text or image is required",
            )),
        ));
    }
    let client_user_message_id = normalize_client_user_message_id(&req.client_user_message_id)?;
    require_runtime_profile(&state.db, &auth, &profile.runtime_key).await?;

    // Resolve the server-owned workspace; the browser never supplies a path.
    let active_run = sqlx::query(
        "SELECT r.id, r.status, r.codex_thread_id, r.workspace_id, r.continued_from_run_id, \
                w.profile_id, w.root_path, \
                t.title, t.copilot_package_id \
         FROM runs r JOIN tasks t ON t.id = r.task_id \
           AND t.organization_id = r.organization_id \
           AND t.workspace_id = r.workspace_id \
         JOIN workspaces w ON w.id = r.workspace_id \
           AND w.organization_id = r.organization_id \
           AND w.state IN ('ready', 'retained') \
         JOIN workspace_grants workspace_grant ON workspace_grant.workspace_id = w.id \
           AND workspace_grant.organization_id = w.organization_id \
           AND workspace_grant.user_id = r.requested_by AND workspace_grant.profile_id = w.profile_id \
           AND workspace_grant.role IN ('owner', 'write') \
         WHERE r.task_id = $1 AND r.organization_id = $2 \
           AND r.requested_by = $3 \
           AND r.codex_thread_id IS NOT NULL \
           AND r.status IN ('running', 'recovery_pending', 'completed', 'failed', 'cancelled') \
         ORDER BY CASE WHEN r.status IN ('running', 'recovery_pending') THEN 0 ELSE 1 END, \
                  r.created_at DESC LIMIT 1",
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
    let workspace_id: Uuid = active_run.get("workspace_id");
    let profile_id: Uuid = active_run.get("profile_id");
    let workspace = AuthorizedWorkspace {
        id: workspace_id.to_string(),
        root: active_run.get::<String, _>("root_path").into(),
    };
    if active_run.get::<String, _>("status") == "recovery_pending" {
        orchestrator
            .recover_run(RecoverRunRequest {
                organization_id: auth.organization_id,
                actor_id: auth.user_id,
                allow_organization_admin: matches!(
                    auth.organization_role.as_str(),
                    "owner" | "admin"
                ),
                run_id: active_run.get("id"),
            })
            .await
            .map_err(super::runs::orchestrator_error)?;
    }
    let active_status: String = active_run.get("status");
    let followup_source_run_id: Option<Uuid> =
        if matches!(active_status.as_str(), "completed" | "failed" | "cancelled") {
            Some(active_run.get("id"))
        } else {
            active_run.get("continued_from_run_id")
        };
    let (run_id, is_followup) = if let Some(source_run_id) = followup_source_run_id {
        let continued = orchestrator
            .continue_thread_run(ContinueThreadRunRequest {
                organization_id: auth.organization_id,
                actor_id: auth.user_id,
                task_id,
                workspace_id,
                source_run_id,
                client_user_message_id: client_user_message_id.clone(),
            })
            .await
            .map_err(super::runs::orchestrator_error)?;
        if !continued.created {
            return replay_followup_message(
                continued.run,
                continued.observed_turn_id,
                &thread_id,
                client_user_message_id,
            );
        }
        (continued.run.id, true)
    } else {
        (active_run.get("id"), false)
    };
    let message_text = selected_map_card_turn_text(
        &state,
        auth.organization_id,
        active_run.get("id"),
        req.map_card_ref.as_deref(),
        &req.text,
    )
    .await?;
    let message_text = selected_resource_turn_text(
        &state,
        auth.organization_id,
        profile_id,
        workspace_id,
        &req.selected_resources,
        &message_text,
    )
    .await?;
    let suggested_thread_name =
        suggested_thread_name(active_run.get::<String, _>("title").as_str(), &req.text);

    let result = match adapter
        .send_user_message(
            &workspace,
            &thread_id,
            &message_text,
            &TurnOptions {
                client_user_message_id: Some(client_user_message_id.clone()),
                effort: req.effort,
                service_tier: req.service_tier,
                access_mode: req.access_mode,
                images: req.images,
                collaboration_mode: req.collaboration_mode,
                copilot_package_id: active_run.get("copilot_package_id"),
            },
        )
        .await
    {
        Ok(result) => result,
        Err(error) => {
            if is_followup {
                if let Err(mark_error) = orchestrator
                    .fail_accepted_followup(auth.organization_id, run_id)
                    .await
                {
                    tracing::warn!(%mark_error, %run_id, "follow-up turn start failure could not be recorded");
                }
            }
            tracing::warn!(
                task_id = %task_id,
                thread_id = %thread_id,
                error = %error,
                "Codex Runtime rejected turn start"
            );
            return Err((
                StatusCode::BAD_GATEWAY,
                Json(PlatformError::internal(
                    "Codex Runtime failed to start the Turn",
                )),
            ));
        }
    };

    let turn_id = result
        .get("turnId")
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| {
            (
                StatusCode::BAD_GATEWAY,
                Json(PlatformError::internal(
                    "Codex Runtime started a Turn without returning its id",
                )),
            )
        })?
        .to_string();
    let started_turn = StartedThreadTurnRequest {
        organization_id: auth.organization_id,
        run_id,
        thread_id: thread_id.clone(),
        turn_id: turn_id.clone(),
    };
    if is_followup {
        orchestrator
            .start_accepted_followup_turn(started_turn)
            .await
            .map_err(super::runs::orchestrator_error)?;
    } else {
        orchestrator
            .record_active_turn(started_turn)
            .await
            .map_err(super::runs::orchestrator_error)?;
    }
    let status = result
        .get("status")
        .and_then(|v| v.as_str())
        .unwrap_or("sent")
        .to_string();
    let thread_name = if let Some(name) = suggested_thread_name {
        match sqlx::query(
            "UPDATE tasks SET title = $1, updated_at = now() \
             WHERE id = $2 AND organization_id = $3 AND title IN ('Thread', 'New Agent')",
        )
        .bind(&name)
        .bind(task_id)
        .bind(auth.organization_id)
        .execute(&state.db)
        .await
        {
            Ok(result) if result.rows_affected() > 0 => Some(name),
            Ok(_) => None,
            Err(error) => {
                tracing::warn!(%error, %task_id, "Turn started but Thread title projection failed");
                None
            }
        }
    } else {
        None
    };

    Ok(Json(SendMessageResponse {
        status,
        thread_id,
        turn_id,
        client_user_message_id,
        thread_name,
    }))
}

fn replay_followup_message(
    run: RunRecord,
    observed_turn_id: Option<String>,
    expected_thread_id: &str,
    client_user_message_id: String,
) -> ApiResult<SendMessageResponse> {
    if run.codex_thread_id.as_deref() != Some(expected_thread_id) {
        return Err((
            StatusCode::CONFLICT,
            Json(PlatformError::bad_request(
                "follow-up Run is bound to another Thread",
            )),
        ));
    }
    let status = match run.status.as_str() {
        "running" => "sent",
        "completed" => "completed",
        "provisioning" => {
            return Err((
                StatusCode::CONFLICT,
                Json(PlatformError::bad_request(
                    "follow-up is accepted but its Runtime Turn is not observed yet",
                )),
            ));
        }
        "failed" if run.failure_code.as_deref() == Some("turn_start_failed") => {
            return Err((
                StatusCode::BAD_GATEWAY,
                Json(PlatformError::internal(
                    "Codex Runtime failed to start the prior follow-up Turn",
                )),
            ));
        }
        _ => {
            return Err((
                StatusCode::CONFLICT,
                Json(PlatformError::bad_request(
                    "follow-up Run is no longer active",
                )),
            ));
        }
    };
    let turn_id = observed_turn_id.ok_or_else(|| {
        (
            StatusCode::CONFLICT,
            Json(PlatformError::bad_request(
                "follow-up is accepted but its Runtime Turn is not observed yet",
            )),
        )
    })?;
    Ok(Json(SendMessageResponse {
        status: status.to_string(),
        thread_id: expected_thread_id.to_string(),
        turn_id,
        client_user_message_id,
        thread_name: None,
    }))
}

async fn selected_map_card_turn_text(
    state: &AppState,
    organization_id: Uuid,
    run_id: Uuid,
    card_ref: Option<&str>,
    text: &str,
) -> Result<String, (StatusCode, Json<PlatformError>)> {
    let Some(card_ref) = card_ref else {
        return Ok(text.to_string());
    };
    let map_spec_ref =
        crate::inline_map_cards::map_spec_ref(&state.db, organization_id, run_id, card_ref)
            .await
            .map_err(database_error)?
            .ok_or_else(|| {
                (
                    StatusCode::BAD_REQUEST,
                    Json(PlatformError::bad_request(
                        "selected map card is unavailable in this Task",
                    )),
                )
            })?;
    let encoded = serde_json::to_string(&map_spec_ref).map_err(|_| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(PlatformError::internal(
                "selected map reference could not be encoded",
            )),
        )
    })?;
    Ok(format!(
        "{text}\n\nUser explicitly selected this exact map presentation Resource for revision: {encoded}"
    ))
}

async fn selected_resource_turn_text(
    state: &AppState,
    organization_id: Uuid,
    profile_id: Uuid,
    workspace_id: Uuid,
    selections: &[ExplicitResourceSelection],
    text: &str,
) -> Result<String, (StatusCode, Json<PlatformError>)> {
    if selections.is_empty() {
        return Ok(text.to_string());
    }
    if selections.len() > 8 {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(PlatformError::bad_request("too many selected Resources")),
        ));
    }
    let mut unique = HashSet::new();
    let mut requested = Vec::with_capacity(selections.len());
    for selection in selections {
        if selection.ordinal < 0
            || selection.server.is_empty()
            || selection.server.len() > 128
            || selection.uri.is_empty()
            || selection.uri.len() > 2048
            || selection.resource_schema.is_empty()
            || selection.resource_schema.len() > 128
        {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(PlatformError::bad_request(
                    "selected Resource reference is invalid",
                )),
            ));
        }
        let key = (
            selection.producer_event_id,
            selection.ordinal,
            selection.server.clone(),
            selection.uri.clone(),
            selection.resource_schema.clone(),
        );
        if !unique.insert(key.clone()) {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(PlatformError::bad_request(
                    "selected Resource reference is duplicated",
                )),
            ));
        }
        requested.push(key);
    }
    let refs = crate::resource_ref_projections::exact_authorized_refs(
        &state.db,
        organization_id,
        profile_id,
        workspace_id,
        &requested,
    )
    .await
    .map_err(database_error)?;
    if refs.len() != requested.len() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(PlatformError::bad_request(
                "selected Resource is unavailable in this Profile and Workspace",
            )),
        ));
    }
    let encoded = serde_json::to_string(&refs).map_err(|_| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(PlatformError::internal(
                "selected Resource references could not be encoded",
            )),
        )
    })?;
    Ok(format!(
        "{text}\n\nUser explicitly selected these exact MCP Resources for this Turn: {encoded}"
    ))
}

/// GET /api/tasks/:id
pub async fn get_task(
    auth: AuthenticatedUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> ApiResult<Task> {
    let row = sqlx::query(
        "SELECT id, project_id, workspace_id, title, status, copilot_package_id, created_at, updated_at \
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
        workspace_id: row.get("workspace_id"),
        title: row.get("title"),
        status: row.get("status"),
        copilot_package_id: row.get("copilot_package_id"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    }))
}

fn copilot_selection_error(error: CopilotInstallationError) -> (StatusCode, Json<PlatformError>) {
    match error {
        CopilotInstallationError::InvalidSelection
        | CopilotInstallationError::InvalidSource(_)
        | CopilotInstallationError::NotFound => (
            StatusCode::BAD_REQUEST,
            Json(PlatformError::bad_request(
                "a selected Copilot package is required and must be available",
            )),
        ),
        CopilotInstallationError::Unavailable
        | CopilotInstallationError::Database(_)
        | CopilotInstallationError::Package(_)
        | CopilotInstallationError::StartupFile(_) => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(PlatformError::internal(
                "Copilot selection is temporarily unavailable",
            )),
        ),
    }
}

fn normalize_thread_model_settings(
    provider_id: String,
    model_id: String,
) -> Result<(String, String), (StatusCode, Json<PlatformError>)> {
    let provider_id = provider_id.trim();
    let model_id = model_id.trim();
    let invalid = provider_id.is_empty()
        || model_id.is_empty()
        || provider_id.len() > 200
        || model_id.len() > 300
        || provider_id
            .chars()
            .chain(model_id.chars())
            .any(|character| matches!(character, '\0' | '\n' | '\r'));
    if invalid {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(PlatformError::bad_request(
                "Thread model settings are invalid",
            )),
        ));
    }
    Ok((provider_id.to_string(), model_id.to_string()))
}

fn normalize_client_user_message_id(
    client_user_message_id: &str,
) -> Result<String, (StatusCode, Json<PlatformError>)> {
    const MAX_CLIENT_USER_MESSAGE_ID_BYTES: usize = 128;
    let invalid = client_user_message_id.is_empty()
        || client_user_message_id.len() > MAX_CLIENT_USER_MESSAGE_ID_BYTES
        || client_user_message_id.trim() != client_user_message_id
        || client_user_message_id.chars().any(char::is_control);
    if invalid {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(PlatformError::bad_request(
                "client user message id is invalid",
            )),
        ));
    }
    Ok(client_user_message_id.to_string())
}

fn suggested_thread_name(current_name: &str, message: &str) -> Option<String> {
    if !matches!(current_name.trim(), "Thread" | "New Agent") {
        return None;
    }
    let normalized = message.split_whitespace().collect::<Vec<_>>().join(" ");
    if normalized.is_empty() {
        return None;
    }
    const MAX_CHARS: usize = 80;
    let mut characters = normalized.chars();
    let prefix = characters.by_ref().take(MAX_CHARS - 1).collect::<String>();
    if characters.next().is_some() {
        Some(format!("{prefix}…"))
    } else {
        Some(normalized)
    }
}

fn thread_settings_runtime_error(
    _: open_web_codex_adapter::AdapterError,
) -> (StatusCode, Json<PlatformError>) {
    (
        StatusCode::BAD_GATEWAY,
        Json(PlatformError::internal(
            "Codex Runtime could not read or update Thread settings",
        )),
    )
}

fn database_error(_: sqlx::Error) -> (StatusCode, Json<PlatformError>) {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(PlatformError::internal("Database operation failed")),
    )
}

#[cfg(test)]
mod tests {
    use super::{
        normalize_client_user_message_id, normalize_thread_model_settings, suggested_thread_name,
        update_materialized_thread_model,
    };
    use open_web_codex_adapter::fake::FakeCodexAdapter;
    use open_web_codex_adapter::{AuthorizedWorkspace, CodexAdapter};
    use open_web_codex_platform_contracts::ThreadModelSettingsUpdateResponse;
    use std::path::PathBuf;

    #[test]
    fn thread_model_settings_require_nonempty_bounded_values() {
        assert!(normalize_thread_model_settings(
            "deepseek".to_string(),
            "deepseek-v4-flash".to_string(),
        )
        .is_ok());
        assert!(normalize_thread_model_settings(" ".to_string(), "model".to_string(),).is_err());
        assert!(
            normalize_thread_model_settings("provider".to_string(), "\n".to_string(),).is_err()
        );
    }

    #[test]
    fn client_user_message_id_requires_a_bounded_non_control_identity() {
        assert_eq!(
            normalize_client_user_message_id("client-message-1").unwrap(),
            "client-message-1"
        );
        let oversized = "x".repeat(129);
        for invalid in [
            "",
            " client-message",
            "client-message\n",
            oversized.as_str(),
        ] {
            assert!(normalize_client_user_message_id(invalid).is_err());
        }
    }

    #[tokio::test]
    async fn updates_only_the_model_of_a_same_provider_thread() {
        let adapter = FakeCodexAdapter::new();
        let workspace = AuthorizedWorkspace {
            id: "workspace".to_string(),
            root: PathBuf::from("/tmp"),
        };
        let thread = adapter
            .start_thread(&workspace, None)
            .await
            .expect("start fake Thread");

        let result = update_materialized_thread_model(
            &adapter,
            &workspace,
            &thread.thread_id,
            "mock_provider".to_string(),
            "mock-model-updated".to_string(),
        )
        .await
        .expect("same-provider model update");

        assert_eq!(
            result,
            ThreadModelSettingsUpdateResponse::Updated {
                provider_id: "mock_provider".to_string(),
                model_id: "mock-model-updated".to_string(),
            }
        );
        assert_eq!(adapter.thread_model_update_count().await, 1);
        assert_eq!(
            adapter
                .read_thread_model_settings(&workspace, &thread.thread_id)
                .await
                .expect("read updated fake Thread settings")
                .model,
            "mock-model-updated"
        );
    }

    #[tokio::test]
    async fn cross_provider_model_selection_requires_a_new_thread_without_mutation() {
        let adapter = FakeCodexAdapter::new();
        let workspace = AuthorizedWorkspace {
            id: "workspace".to_string(),
            root: PathBuf::from("/tmp"),
        };
        let thread = adapter
            .start_thread(&workspace, None)
            .await
            .expect("start fake Thread");

        let result = update_materialized_thread_model(
            &adapter,
            &workspace,
            &thread.thread_id,
            "other_provider".to_string(),
            "other-model".to_string(),
        )
        .await
        .expect("cross-provider selection must be explicit");

        assert_eq!(
            result,
            ThreadModelSettingsUpdateResponse::RequiresNewThread {
                provider_id: "other_provider".to_string(),
                model_id: "other-model".to_string(),
            }
        );
        assert_eq!(adapter.thread_model_update_count().await, 0);
        let settings = adapter
            .read_thread_model_settings(&workspace, &thread.thread_id)
            .await
            .expect("read unchanged fake Thread settings");
        assert_eq!(settings.model_provider, "mock_provider");
        assert_eq!(settings.model, "mock-model");
    }

    #[test]
    fn derives_a_readable_title_only_for_placeholder_threads() {
        assert_eq!(
            suggested_thread_name("Thread", "  查询上海\n  的地图  "),
            Some("查询上海 的地图".to_string()),
        );
        assert_eq!(
            suggested_thread_name("New Agent", "Show the route"),
            Some("Show the route".to_string()),
        );
        assert_eq!(suggested_thread_name("Custom title", "ignored"), None);
        assert_eq!(suggested_thread_name("Thread", " \n\t "), None);
    }

    #[test]
    fn truncates_long_titles_on_unicode_character_boundaries() {
        let title = suggested_thread_name("Thread", &"图".repeat(100)).unwrap();
        assert_eq!(title.chars().count(), 80);
        assert!(title.ends_with('…'));
    }
}
