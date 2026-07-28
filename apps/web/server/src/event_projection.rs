use open_web_codex_platform_contracts::RunEvent;
use serde_json::{json, Map, Value};
use sqlx::PgPool;
use sqlx::Row;
use uuid::Uuid;

const PROJECTION_VERSION: i16 = 1;

#[derive(Debug, PartialEq)]
struct ProjectedEvent {
    event_type: String,
    workspace_id: Option<Uuid>,
    thread_id: String,
    turn_id: Option<String>,
    item_id: Option<String>,
    payload: Value,
    thread_metadata: Option<ProjectedThreadMetadata>,
    artifacts: Vec<ArtifactCandidate>,
    inline_artifact: Option<InlineVisualizationArtifactCandidate>,
}

#[derive(Debug, PartialEq)]
struct ProjectedThreadMetadata {
    parent_thread_id: Option<String>,
    source_kind: Option<String>,
    agent_path: Option<String>,
    agent_nickname: Option<String>,
    agent_role: Option<String>,
    status_type: Option<String>,
    active_flags: Vec<String>,
}

struct EventRunContext {
    run_id: Uuid,
    task_id: Uuid,
    organization_id: Uuid,
    profile_id: Uuid,
    workspace_id: Uuid,
    root_thread_id: String,
    governed_supervisor: bool,
}

#[derive(Debug, PartialEq)]
struct ArtifactCandidate {
    artifact_schema: String,
    display_name: String,
    uri: String,
    mime_type: String,
    expected_size: Option<i64>,
}

struct RegisteredArtifact {
    id: Uuid,
    artifact_schema: String,
    display_name: String,
    mime_type: String,
    expected_size: Option<i64>,
    state: String,
}

#[derive(Debug, PartialEq)]
struct InlineVisualizationArtifactCandidate {
    artifact_ref: String,
    renderer_kind: String,
    renderer_payload: Value,
}

pub struct LiveProjection {
    pub organization_id: Uuid,
    pub payload: Vec<u8>,
    pub pending_artifact_ids: Vec<Uuid>,
}

pub async fn persist_frame(data: &[u8], db: &PgPool) -> Result<Option<LiveProjection>, String> {
    if let Some(projection) = persist_terminal_frame(data, db).await? {
        return Ok(Some(projection));
    }
    let Some(mut event) = project_frame(data)? else {
        return Ok(None);
    };

    let mut transaction = db
        .begin()
        .await
        .map_err(|error| format!("event transaction error: {error}"))?;
    let Some(context) = resolve_event_run_context(&mut transaction, &event).await? else {
        return Ok(None);
    };
    let run_id = context.run_id;
    let organization_id = context.organization_id;
    let is_root_thread = event.thread_id == context.root_thread_id;
    update_runtime_agent_projection(&mut transaction, &context, &event).await?;

    sqlx::query("SAVEPOINT artifact_projection")
        .execute(&mut *transaction)
        .await
        .map_err(|error| format!("Artifact projection savepoint error: {error}"))?;
    let mut pending_artifact_ids = Vec::new();
    let artifact_result = async {
        let registered = register_artifacts(&mut transaction, &context, &event).await?;
        project_registered_artifacts(&mut event.payload, &registered);
        register_inline_visualization_artifact(&mut transaction, &event, run_id, organization_id)
            .await?;
        resolve_inline_artifacts_in_transaction(&mut transaction, run_id, &mut event.payload)
            .await?;
        Ok::<Vec<Uuid>, String>(
            registered
                .into_iter()
                .filter(|artifact| artifact.state == "pending")
                .map(|artifact| artifact.id)
                .collect(),
        )
    }
    .await;
    match artifact_result {
        Ok(ids) => {
            pending_artifact_ids = ids;
            sqlx::query("RELEASE SAVEPOINT artifact_projection")
                .execute(&mut *transaction)
                .await
                .map_err(|error| format!("Artifact projection release error: {error}"))?;
        }
        Err(error) => {
            sqlx::query("ROLLBACK TO SAVEPOINT artifact_projection")
                .execute(&mut *transaction)
                .await
                .map_err(|rollback_error| {
                    format!("Artifact projection rollback error: {rollback_error}")
                })?;
            sqlx::query("RELEASE SAVEPOINT artifact_projection")
                .execute(&mut *transaction)
                .await
                .map_err(|release_error| {
                    format!("Artifact projection release error: {release_error}")
                })?;
            event
                .payload
                .pointer_mut("/data")
                .and_then(Value::as_object_mut)
                .map(|data| {
                    data.remove("artifacts");
                    data.remove("inlineArtifacts");
                });
            tracing::warn!(
                error = %error,
                run_id = %run_id,
                item_id = event.item_id.as_deref().unwrap_or_default(),
                "Artifact projection failed; preserving the Runtime item lifecycle"
            );
        }
    }

    let persisted = sqlx::query(
        "INSERT INTO run_events (
            run_id, event_type, projection_version, thread_id, turn_id, item_id, payload
         ) VALUES ($1, $2, $3, $4, $5, $6, $7)
         RETURNING id, sequence, created_at",
    )
    .bind(run_id)
    .bind(&event.event_type)
    .bind(PROJECTION_VERSION)
    .bind(&event.thread_id)
    .bind(&event.turn_id)
    .bind(&event.item_id)
    .bind(&event.payload)
    .fetch_one(&mut *transaction)
    .await
    .map_err(|error| format!("event insert error: {error}"))?;
    let event_sequence = persisted.get::<i64, _>("sequence");
    let event_created_at = persisted.get::<chrono::DateTime<chrono::Utc>, _>("created_at");
    project_runtime_agent_execution(
        &mut transaction,
        &context,
        &event,
        event_sequence,
        event_created_at,
    )
    .await?;

    if is_root_thread {
        match event.event_type.as_str() {
            "codex.turn.started" => {
                sqlx::query(
                    "UPDATE runs SET active_turn_id = $1, updated_at = now() \
                 WHERE id = $2 AND status = 'running'",
                )
                .bind(&event.turn_id)
                .bind(run_id)
                .execute(&mut *transaction)
                .await
                .map_err(|error| format!("active Turn projection error: {error}"))?;
            }
            "codex.turn.completed" => {
                if context.governed_supervisor
                    && event
                        .payload
                        .pointer("/data/turn/status")
                        .and_then(Value::as_str)
                        == Some("completed")
                {
                    sqlx::query(
                        "WITH completed_run AS (
                            UPDATE runs
                            SET status = 'completed', active_turn_id = NULL, lease_owner = NULL,
                                lease_token = NULL, lease_expires_at = NULL, updated_at = now()
                            WHERE id = $1 AND status = 'running' AND active_turn_id = $2
                            RETURNING task_id
                         )
                         UPDATE tasks SET status = 'completed', updated_at = now()
                         WHERE id IN (SELECT task_id FROM completed_run)
                           AND status NOT IN ('cancelled', 'archived')",
                    )
                    .bind(run_id)
                    .bind(&event.turn_id)
                    .execute(&mut *transaction)
                    .await
                    .map_err(|error| {
                        format!("governed Supervisor completion projection error: {error}")
                    })?;
                } else {
                    sqlx::query(
                        "UPDATE runs SET active_turn_id = NULL, updated_at = now() \
                         WHERE id = $1 AND active_turn_id = $2",
                    )
                    .bind(run_id)
                    .bind(&event.turn_id)
                    .execute(&mut *transaction)
                    .await
                    .map_err(|error| format!("completed Turn projection error: {error}"))?;
                }
            }
            "codex.thread.archived" => {
                sqlx::query(
                    "UPDATE tasks SET status = 'archived', updated_at = now() \
                 WHERE id = (SELECT task_id FROM runs WHERE id = $1)",
                )
                .bind(run_id)
                .execute(&mut *transaction)
                .await
                .map_err(|error| format!("archived Thread projection error: {error}"))?;
            }
            "codex.thread.unarchived" => {
                sqlx::query(
                    "UPDATE tasks SET status = 'pending', updated_at = now() \
                 WHERE id = (SELECT task_id FROM runs WHERE id = $1) AND status = 'archived'",
                )
                .bind(run_id)
                .execute(&mut *transaction)
                .await
                .map_err(|error| format!("unarchived Thread projection error: {error}"))?;
            }
            "codex.thread.name.updated" => {
                if let Some(name) = event
                    .payload
                    .pointer("/data/threadName")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|name| !name.is_empty() && name.len() <= 200)
                {
                    sqlx::query(
                        "UPDATE tasks SET title = $1, updated_at = now() \
                     WHERE id = (SELECT task_id FROM runs WHERE id = $2)",
                    )
                    .bind(name)
                    .bind(run_id)
                    .execute(&mut *transaction)
                    .await
                    .map_err(|error| format!("Thread name projection error: {error}"))?;
                }
            }
            _ => {}
        }
    }

    let terminal_status = match (is_root_thread, event.event_type.as_str()) {
        (true, "codex.thread.completed") => Some("completed"),
        (true, "codex.thread.failed") => Some("failed"),
        _ => None,
    };
    if let Some(status) = terminal_status {
        let task_status = if status == "completed" {
            "completed"
        } else {
            "pending"
        };
        sqlx::query(
            "WITH updated_run AS (
                UPDATE runs SET status = $1, active_turn_id = NULL, lease_owner = NULL,
                                lease_token = NULL, lease_expires_at = NULL, updated_at = now()
                WHERE id = $2 AND status = 'running'
                RETURNING task_id
             )
             UPDATE tasks SET status = $3, updated_at = now()
             WHERE id IN (SELECT task_id FROM updated_run)
               AND status NOT IN ('completed', 'cancelled', 'archived')",
        )
        .bind(status)
        .bind(run_id)
        .bind(task_status)
        .execute(&mut *transaction)
        .await
        .map_err(|error| format!("run lifecycle update error: {error}"))?;
    }

    transaction
        .commit()
        .await
        .map_err(|error| format!("event transaction commit error: {error}"))?;

    let public = RunEvent {
        id: persisted.get("id"),
        sequence: event_sequence,
        run_id,
        event_type: event.event_type,
        projection_version: PROJECTION_VERSION,
        thread_id: Some(event.thread_id),
        turn_id: event.turn_id,
        item_id: event.item_id,
        payload: event.payload,
        created_at: event_created_at,
    };
    let payload = serde_json::to_vec(&json!({
        "type": "run.event",
        "version": 1,
        "event": public,
    }))
    .map_err(|error| format!("live projection encoding error: {error}"))?;
    Ok(Some(LiveProjection {
        organization_id,
        payload,
        pending_artifact_ids,
    }))
}

async fn persist_terminal_frame(
    data: &[u8],
    db: &PgPool,
) -> Result<Option<LiveProjection>, String> {
    let Some(message) = internal_message(data)? else {
        return Ok(None);
    };
    let method = message
        .get("method")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if !matches!(
        method,
        "command/exec/outputDelta" | "platform/terminalExited"
    ) {
        return Ok(None);
    }
    let params = message
        .get("params")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let Some(process_id) = string_field(&params, "processId") else {
        return Ok(None);
    };
    let mut transaction = db
        .begin()
        .await
        .map_err(|error| format!("terminal event transaction error: {error}"))?;
    let session = sqlx::query(
        "SELECT session.terminal_id, session.workspace_id, session.run_id, \
                session.organization_id, run.codex_thread_id \
         FROM terminal_sessions session JOIN runs run ON run.id = session.run_id \
         WHERE session.process_id = $1",
    )
    .bind(&process_id)
    .fetch_optional(&mut *transaction)
    .await
    .map_err(|error| format!("terminal session lookup error: {error}"))?;
    let Some(session) = session else {
        return Ok(None);
    };
    let terminal_id: String = session.get("terminal_id");
    let workspace_id: Uuid = session.get("workspace_id");
    let run_id: Uuid = session.get("run_id");
    let organization_id: Uuid = session.get("organization_id");
    let thread_id: Option<String> = session.get("codex_thread_id");
    let (event_type, payload) =
        if method == "command/exec/outputDelta" {
            let encoded = params
                .get("deltaBase64")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let mut decoded = BASE64
                .decode(encoded)
                .map_err(|_| "terminal output was not valid base64".to_string())?;
            decoded.truncate(256 * 1024);
            (
                "terminal.output",
                json!({
                    "schemaVersion": PROJECTION_VERSION,
                    "workspaceId": workspace_id,
                    "terminalId": terminal_id,
                    "data": String::from_utf8_lossy(&decoded),
                }),
            )
        } else {
            sqlx::query(
            "UPDATE terminal_sessions SET state = CASE WHEN $2 THEN 'failed' ELSE 'closed' END, \
                                          updated_at = now() WHERE process_id = $1",
        )
        .bind(&process_id)
        .bind(params.get("failed").and_then(Value::as_bool).unwrap_or(false))
        .execute(&mut *transaction)
        .await
        .map_err(|error| format!("terminal exit update error: {error}"))?;
            (
                "terminal.exit",
                json!({
                    "schemaVersion": PROJECTION_VERSION,
                    "workspaceId": workspace_id,
                    "terminalId": terminal_id,
                    "exitCode": params.get("exitCode").cloned().unwrap_or(Value::Null),
                }),
            )
        };
    let persisted = sqlx::query(
        "INSERT INTO run_events \
         (run_id, event_type, projection_version, thread_id, payload) \
         VALUES ($1, $2, $3, $4, $5) RETURNING id, sequence, created_at",
    )
    .bind(run_id)
    .bind(event_type)
    .bind(PROJECTION_VERSION)
    .bind(&thread_id)
    .bind(&payload)
    .fetch_one(&mut *transaction)
    .await
    .map_err(|error| format!("terminal event insert error: {error}"))?;
    transaction
        .commit()
        .await
        .map_err(|error| format!("terminal event commit error: {error}"))?;
    let public = RunEvent {
        id: persisted.get("id"),
        sequence: persisted.get("sequence"),
        run_id,
        event_type: event_type.to_string(),
        projection_version: PROJECTION_VERSION,
        thread_id,
        turn_id: None,
        item_id: None,
        payload,
        created_at: persisted.get("created_at"),
    };
    let payload = serde_json::to_vec(&json!({
        "type": "run.event",
        "version": 1,
        "event": public,
    }))
    .map_err(|error| format!("terminal live projection encoding error: {error}"))?;
    Ok(Some(LiveProjection {
        organization_id,
        payload,
        pending_artifact_ids: Vec::new(),
    }))
}

fn project_frame(data: &[u8]) -> Result<Option<ProjectedEvent>, String> {
    let Some(frame) = internal_frame(data)? else {
        return Ok(None);
    };
    let message = frame.message;
    let runtime_method = message
        .get("method")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    let params = message
        .get("params")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let thread_id = string_field(&params, "threadId")
        .or_else(|| string_field(&params, "thread_id"))
        .or_else(|| nested_string_field(&params, "thread", "id"));
    let Some(thread_id) = thread_id else {
        return Ok(None);
    };
    if thread_id.is_empty() || thread_id.len() > 256 {
        return Ok(None);
    }
    let turn_id = string_field(&params, "turnId")
        .or_else(|| string_field(&params, "turn_id"))
        .or_else(|| nested_string_field(&params, "turn", "id"));
    let item = params.get("item").and_then(Value::as_object);
    let item_id = string_field(&params, "itemId")
        .or_else(|| string_field(&params, "item_id"))
        .or_else(|| item.and_then(|item| string_field(item, "id")));

    let (event_type, lifecycle) = classify_method(runtime_method);
    let item_type = item.and_then(|item| string_field(item, "type"));
    let artifacts = item.into_iter().flat_map(artifact_candidates).collect();
    let inline_artifact = item.and_then(project_inline_visualization_artifact);
    let thread_metadata = project_thread_metadata(runtime_method, &params);
    let data = if let Some(item) = item {
        project_item(item)
    } else {
        project_event_data(runtime_method, &params)
    };
    let payload = json!({
        "schemaVersion": PROJECTION_VERSION,
        "threadId": thread_id,
        "turnId": turn_id,
        "itemId": item_id,
        "lifecycle": lifecycle,
        "itemType": item_type,
        "data": data,
    });

    Ok(Some(ProjectedEvent {
        event_type: event_type.to_string(),
        workspace_id: frame.workspace_id,
        thread_id,
        turn_id,
        item_id,
        payload,
        thread_metadata,
        artifacts,
        inline_artifact,
    }))
}

struct InternalFrame {
    workspace_id: Option<Uuid>,
    message: Map<String, Value>,
}

fn internal_frame(data: &[u8]) -> Result<Option<InternalFrame>, String> {
    let text = std::str::from_utf8(data).map_err(|error| format!("invalid utf8: {error}"))?;
    let json_text = text
        .lines()
        .filter_map(|line| line.strip_prefix("data:"))
        .map(str::trim_start)
        .collect::<Vec<_>>()
        .join("\n");
    let json_text = if json_text.is_empty() {
        let raw = text.trim();
        if !raw.starts_with('{') && !raw.starts_with('[') {
            return Ok(None);
        }
        raw
    } else {
        json_text.trim()
    };
    if json_text.is_empty() {
        return Ok(None);
    }

    let value: Value =
        serde_json::from_str(json_text).map_err(|error| format!("invalid json: {error}"))?;
    if value.get("method").and_then(Value::as_str) != Some("app-server-event") {
        return Ok(None);
    }
    let workspace_id = value
        .pointer("/params/workspace_id")
        .or_else(|| value.pointer("/params/workspaceId"))
        .and_then(Value::as_str)
        .and_then(|value| Uuid::parse_str(value).ok());
    let message = match value.pointer("/params/message").and_then(Value::as_object) {
        Some(message) => message.clone(),
        None => return Ok(None),
    };
    Ok(Some(InternalFrame {
        workspace_id,
        message,
    }))
}

fn internal_message(data: &[u8]) -> Result<Option<Map<String, Value>>, String> {
    Ok(internal_frame(data)?.map(|frame| frame.message))
}

fn classify_method(method: &str) -> (&'static str, &'static str) {
    match method {
        "platform/approvalRequested" => ("platform.approval.requested", "requested"),
        "serverRequest/resolved" => ("platform.approval.resolved", "resolved"),
        "item/started" => ("codex.item.started", "started"),
        "item/completed" => ("codex.item.completed", "completed"),
        "turn/started" => ("codex.turn.started", "started"),
        "turn/completed" => ("codex.turn.completed", "completed"),
        "thread/started" => ("codex.thread.started", "started"),
        "thread/status/changed" => ("codex.thread.status.changed", "updated"),
        "thread/archived" => ("codex.thread.archived", "archived"),
        "thread/unarchived" => ("codex.thread.unarchived", "unarchived"),
        "thread/name/updated" => ("codex.thread.name.updated", "updated"),
        "thread/tokenUsage/updated" => ("codex.thread.token_usage.updated", "updated"),
        "thread/completed" => ("codex.thread.completed", "completed"),
        "thread/failed" => ("codex.thread.failed", "failed"),
        method
            if method.starts_with("item/")
                && (method.ends_with("/delta") || method.ends_with("Delta")) =>
        {
            ("codex.item.delta", "delta")
        }
        _ => ("codex.unknown", "unknown"),
    }
}

fn project_thread_metadata(
    method: &str,
    params: &Map<String, Value>,
) -> Option<ProjectedThreadMetadata> {
    let thread = params.get("thread").and_then(Value::as_object);
    let source = thread.and_then(|thread| thread.get("source"));
    let subagent = source.and_then(Value::as_object).and_then(|source| {
        source
            .get("subAgent")
            .or_else(|| source.get("sub_agent"))
            .or_else(|| source.get("subagent"))
    });
    let thread_spawn = subagent
        .and_then(Value::as_object)
        .and_then(|subagent| {
            subagent
                .get("thread_spawn")
                .or_else(|| subagent.get("threadSpawn"))
        })
        .and_then(Value::as_object);
    let parent_thread_id = thread
        .and_then(|thread| {
            bounded_object_string(thread, "parentThreadId", 256)
                .or_else(|| bounded_object_string(thread, "parent_thread_id", 256))
        })
        .or_else(|| {
            thread_spawn.and_then(|spawn| {
                bounded_object_string(spawn, "parentThreadId", 256)
                    .or_else(|| bounded_object_string(spawn, "parent_thread_id", 256))
            })
        });
    let source_kind = if thread_spawn.is_some() {
        Some("thread_spawn".to_string())
    } else {
        subagent
            .and_then(Value::as_str)
            .and_then(|value| bounded_text(value, 64))
            .or_else(|| parent_thread_id.as_ref().map(|_| "subagent".to_string()))
    };
    let agent_path = thread_spawn.and_then(|spawn| {
        bounded_object_string(spawn, "agentPath", 512)
            .or_else(|| bounded_object_string(spawn, "agent_path", 512))
    });
    let agent_nickname = thread_spawn.and_then(|spawn| {
        bounded_object_string(spawn, "agentNickname", 128)
            .or_else(|| bounded_object_string(spawn, "agent_nickname", 128))
    });
    let agent_role = thread_spawn.and_then(|spawn| {
        bounded_object_string(spawn, "agentRole", 128)
            .or_else(|| bounded_object_string(spawn, "agent_role", 128))
            .or_else(|| bounded_object_string(spawn, "agentType", 128))
            .or_else(|| bounded_object_string(spawn, "agent_type", 128))
    });
    let status = params.get("status").and_then(Value::as_object).or_else(|| {
        thread
            .and_then(|thread| thread.get("status"))
            .and_then(Value::as_object)
    });
    let status_type = status.and_then(|status| bounded_object_string(status, "type", 64));
    let active_flags = status
        .and_then(|status| {
            status
                .get("activeFlags")
                .or_else(|| status.get("active_flags"))
        })
        .and_then(Value::as_array)
        .map(|flags| {
            flags
                .iter()
                .filter_map(Value::as_str)
                .filter_map(|flag| bounded_text(flag, 64))
                .take(32)
                .collect()
        })
        .unwrap_or_default();

    if thread.is_none()
        && method != "thread/status/changed"
        && status_type.is_none()
        && parent_thread_id.is_none()
    {
        return None;
    }
    Some(ProjectedThreadMetadata {
        parent_thread_id,
        source_kind,
        agent_path,
        agent_nickname,
        agent_role,
        status_type,
        active_flags,
    })
}

fn bounded_object_string(values: &Map<String, Value>, key: &str, max_len: usize) -> Option<String> {
    values
        .get(key)
        .and_then(Value::as_str)
        .and_then(|value| bounded_text(value, max_len))
}

fn bounded_text(value: &str, max_len: usize) -> Option<String> {
    let value = value.trim();
    (!value.is_empty() && value.len() <= max_len).then(|| value.to_string())
}

async fn resolve_event_run_context(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    event: &ProjectedEvent,
) -> Result<Option<EventRunContext>, String> {
    if let Some(context) =
        lookup_known_thread_context(transaction, &event.thread_id, event.workspace_id).await?
    {
        if event.thread_id == context.root_thread_id {
            ensure_root_agent_projection(transaction, &context).await?;
        }
        return Ok(Some(context));
    }
    let Some(metadata) = event.thread_metadata.as_ref() else {
        return Ok(None);
    };
    let Some(parent_thread_id) = metadata.parent_thread_id.as_deref() else {
        return Ok(None);
    };
    let Some(parent) =
        lookup_known_thread_context(transaction, parent_thread_id, event.workspace_id).await?
    else {
        return Ok(None);
    };
    let source_kind = metadata
        .source_kind
        .as_deref()
        .filter(|source| *source != "root")
        .unwrap_or("subagent");
    let row = sqlx::query(
        "INSERT INTO runtime_agent_projections (
            organization_id, profile_id, workspace_id, root_run_id, thread_id,
            parent_thread_id, source_kind, agent_path, agent_nickname, agent_role,
            status_type, active_flags
         ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
         ON CONFLICT (profile_id, thread_id) DO UPDATE SET
            parent_thread_id = COALESCE(runtime_agent_projections.parent_thread_id,
                                        EXCLUDED.parent_thread_id),
            source_kind = CASE
                WHEN runtime_agent_projections.source_kind = 'root'
                    THEN runtime_agent_projections.source_kind
                ELSE EXCLUDED.source_kind
            END,
            agent_path = COALESCE(EXCLUDED.agent_path,
                                  runtime_agent_projections.agent_path),
            agent_nickname = COALESCE(EXCLUDED.agent_nickname,
                                      runtime_agent_projections.agent_nickname),
            agent_role = COALESCE(EXCLUDED.agent_role,
                                  runtime_agent_projections.agent_role),
            status_type = COALESCE(EXCLUDED.status_type,
                                   runtime_agent_projections.status_type),
            active_flags = CASE
                WHEN EXCLUDED.status_type IS NULL
                    THEN runtime_agent_projections.active_flags
                ELSE EXCLUDED.active_flags
            END,
            last_observed_at = now()
         WHERE runtime_agent_projections.root_run_id = EXCLUDED.root_run_id
           AND runtime_agent_projections.workspace_id = EXCLUDED.workspace_id
           AND runtime_agent_projections.parent_thread_id IS NOT DISTINCT FROM
               EXCLUDED.parent_thread_id
         RETURNING root_run_id",
    )
    .bind(parent.organization_id)
    .bind(parent.profile_id)
    .bind(parent.workspace_id)
    .bind(parent.run_id)
    .bind(&event.thread_id)
    .bind(parent_thread_id)
    .bind(source_kind)
    .bind(&metadata.agent_path)
    .bind(&metadata.agent_nickname)
    .bind(&metadata.agent_role)
    .bind(&metadata.status_type)
    .bind(&metadata.active_flags)
    .fetch_optional(&mut **transaction)
    .await
    .map_err(|error| format!("child Thread projection error: {error}"))?;
    if row.is_none() {
        return Err("child Thread is already associated with another Runtime tree".to_string());
    }
    Ok(Some(parent))
}

async fn ensure_root_agent_projection(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    context: &EventRunContext,
) -> Result<(), String> {
    let row = sqlx::query(
        "INSERT INTO runtime_agent_projections (
            organization_id, profile_id, workspace_id, root_run_id, thread_id, source_kind
         ) VALUES ($1, $2, $3, $4, $5, 'root')
         ON CONFLICT (profile_id, thread_id) DO UPDATE
           SET last_observed_at = now()
           WHERE runtime_agent_projections.root_run_id = EXCLUDED.root_run_id
             AND runtime_agent_projections.workspace_id = EXCLUDED.workspace_id
             AND runtime_agent_projections.source_kind = 'root'
         RETURNING root_run_id",
    )
    .bind(context.organization_id)
    .bind(context.profile_id)
    .bind(context.workspace_id)
    .bind(context.run_id)
    .bind(&context.root_thread_id)
    .fetch_optional(&mut **transaction)
    .await
    .map_err(|error| format!("root Thread projection error: {error}"))?;
    if row.is_none() {
        return Err("root Thread is already associated with another Runtime tree".to_string());
    }
    Ok(())
}

async fn lookup_known_thread_context(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    thread_id: &str,
    workspace_id: Option<Uuid>,
) -> Result<Option<EventRunContext>, String> {
    let root = sqlx::query(
        "SELECT run.id AS run_id, run.task_id, run.organization_id,
                run.requested_profile_id AS profile_id,
                run.workspace_id, run.codex_thread_id AS root_thread_id,
                EXISTS (
                    SELECT 1 FROM supervisor_policy_bindings binding
                    WHERE binding.run_id = run.id AND binding.state = 'bound'
                ) AS governed_supervisor
         FROM runs run
         WHERE run.codex_thread_id = $1
           AND run.requested_profile_id IS NOT NULL
           AND run.workspace_id IS NOT NULL
           AND ($2::uuid IS NULL OR run.workspace_id = $2)
         ORDER BY run.created_at DESC
         LIMIT 1",
    )
    .bind(thread_id)
    .bind(workspace_id)
    .fetch_optional(&mut **transaction)
    .await
    .map_err(|error| format!("root Thread run lookup error: {error}"))?;
    if let Some(root) = root {
        return Ok(Some(event_run_context(&root)));
    }

    let projected = sqlx::query(
        "SELECT projection.root_run_id AS run_id, run.task_id, projection.organization_id,
                projection.profile_id, projection.workspace_id,
                run.codex_thread_id AS root_thread_id,
                EXISTS (
                    SELECT 1 FROM supervisor_policy_bindings binding
                    WHERE binding.run_id = run.id AND binding.state = 'bound'
                ) AS governed_supervisor
         FROM runtime_agent_projections projection
         JOIN runs run ON run.id = projection.root_run_id
           AND run.organization_id = projection.organization_id
         WHERE projection.thread_id = $1
           AND ($2::uuid IS NULL OR projection.workspace_id = $2)
         ORDER BY projection.first_observed_at DESC
         LIMIT 1",
    )
    .bind(thread_id)
    .bind(workspace_id)
    .fetch_optional(&mut **transaction)
    .await
    .map_err(|error| format!("projected Thread run lookup error: {error}"))?;
    Ok(projected.as_ref().map(event_run_context))
}

fn event_run_context(row: &sqlx::postgres::PgRow) -> EventRunContext {
    EventRunContext {
        run_id: row.get("run_id"),
        task_id: row.get("task_id"),
        organization_id: row.get("organization_id"),
        profile_id: row.get("profile_id"),
        workspace_id: row.get("workspace_id"),
        root_thread_id: row.get("root_thread_id"),
        governed_supervisor: row.get("governed_supervisor"),
    }
}

async fn update_runtime_agent_projection(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    context: &EventRunContext,
    event: &ProjectedEvent,
) -> Result<(), String> {
    let metadata = event.thread_metadata.as_ref();
    let status_type = match event.event_type.as_str() {
        "codex.thread.completed" => Some("completed"),
        "codex.thread.failed" => Some("failed"),
        _ => metadata.and_then(|metadata| metadata.status_type.as_deref()),
    };
    let active_flags = metadata
        .map(|metadata| metadata.active_flags.as_slice())
        .unwrap_or_default();
    let updated = sqlx::query(
        "UPDATE runtime_agent_projections SET
            parent_thread_id = COALESCE(parent_thread_id, $1),
            source_kind = CASE
                WHEN source_kind = 'root' THEN source_kind
                ELSE COALESCE($2, source_kind)
            END,
            agent_path = COALESCE($3, agent_path),
            agent_nickname = COALESCE($4, agent_nickname),
            agent_role = COALESCE($5, agent_role),
            status_type = COALESCE($6, status_type),
            active_flags = CASE WHEN $6::text IS NULL THEN active_flags ELSE $7 END,
            last_observed_at = now()
         WHERE profile_id = $8 AND thread_id = $9 AND root_run_id = $10
           AND workspace_id = $11",
    )
    .bind(metadata.and_then(|metadata| metadata.parent_thread_id.as_deref()))
    .bind(metadata.and_then(|metadata| metadata.source_kind.as_deref()))
    .bind(metadata.and_then(|metadata| metadata.agent_path.as_deref()))
    .bind(metadata.and_then(|metadata| metadata.agent_nickname.as_deref()))
    .bind(metadata.and_then(|metadata| metadata.agent_role.as_deref()))
    .bind(status_type)
    .bind(active_flags)
    .bind(context.profile_id)
    .bind(&event.thread_id)
    .bind(context.run_id)
    .bind(context.workspace_id)
    .execute(&mut **transaction)
    .await
    .map_err(|error| format!("Runtime agent projection update error: {error}"))?
    .rows_affected();
    if updated != 1 {
        return Err("Runtime agent projection changed during event delivery".to_string());
    }
    Ok(())
}

async fn project_runtime_agent_execution(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    context: &EventRunContext,
    event: &ProjectedEvent,
    sequence: i64,
    observed_at: chrono::DateTime<chrono::Utc>,
) -> Result<(), String> {
    if event.thread_id == context.root_thread_id {
        return project_supervisor_assignment(transaction, context, event, sequence, observed_at)
            .await;
    }

    let Some(turn_id) = event.turn_id.as_deref() else {
        return project_turnless_agent_terminal(transaction, context, event, sequence, observed_at)
            .await;
    };
    ensure_agent_execution(
        transaction,
        context,
        &event.thread_id,
        turn_id,
        sequence,
        observed_at,
    )
    .await?;
    update_agent_execution(transaction, context, event, turn_id, sequence, observed_at).await
}

async fn project_supervisor_assignment(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    context: &EventRunContext,
    event: &ProjectedEvent,
    sequence: i64,
    observed_at: chrono::DateTime<chrono::Utc>,
) -> Result<(), String> {
    if event.event_type != "codex.item.completed"
        || !matches!(
            event.payload.get("itemType").and_then(Value::as_str),
            Some("collabAgentToolCall" | "collabToolCall")
        )
    {
        return Ok(());
    }
    let data = event.payload.get("data").unwrap_or(&Value::Null);
    let tool = data
        .get("tool")
        .and_then(Value::as_str)
        .map(normalize_agent_tool)
        .unwrap_or_default();
    if !matches!(tool.as_str(), "spawnagent" | "sendinput") {
        return Ok(());
    }
    let Some(task) = data
        .get("prompt")
        .and_then(Value::as_str)
        .and_then(|value| truncated_projection_text(value, 1_000))
    else {
        return Ok(());
    };
    let mut receivers = data
        .get("receiverThreadIds")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty() && value.len() <= 256)
        .map(str::to_string)
        .collect::<Vec<_>>();
    receivers.sort();
    receivers.dedup();

    for receiver in receivers {
        lock_agent_execution(transaction, context.run_id, &receiver).await?;
        let attached = sqlx::query_scalar::<_, Uuid>(
            "UPDATE runtime_agent_execution_projections
             SET assignment_sequence = $1, assignment_item_id = $2, task = $3,
                 first_observed_sequence = LEAST(first_observed_sequence, $1),
                 last_observed_sequence = GREATEST(last_observed_sequence, $1),
                 updated_at = now()
             WHERE id = (
                 SELECT id
                 FROM runtime_agent_execution_projections
                 WHERE root_run_id = $4 AND agent_thread_id = $5
                   AND turn_id IS NOT NULL AND assignment_sequence IS NULL
                 ORDER BY ordinal DESC
                 LIMIT 1
             )
             RETURNING id",
        )
        .bind(sequence)
        .bind(&event.item_id)
        .bind(&task)
        .bind(context.run_id)
        .bind(&receiver)
        .fetch_optional(&mut **transaction)
        .await
        .map_err(|error| format!("Agent task assignment attach error: {error}"))?;
        if attached.is_some() || tool != "spawnagent" {
            continue;
        }

        let ordinal = next_agent_execution_ordinal(transaction, context.run_id, &receiver).await?;
        sqlx::query(
            "INSERT INTO runtime_agent_execution_projections (
                organization_id, profile_id, workspace_id, root_run_id,
                agent_thread_id, ordinal, assignment_sequence, assignment_item_id,
                task, status, current_behavior, first_observed_sequence,
                last_observed_sequence, created_at, updated_at
             ) VALUES (
                $1, $2, $3, $4, $5, $6, $7, $8,
                $9, 'pending', 'Waiting to start', $7, $7, $10, $10
             )
             ON CONFLICT (root_run_id, agent_thread_id, assignment_item_id)
             DO UPDATE SET
                task = EXCLUDED.task,
                assignment_sequence = LEAST(
                    runtime_agent_execution_projections.assignment_sequence,
                    EXCLUDED.assignment_sequence
                ),
                first_observed_sequence = LEAST(
                    runtime_agent_execution_projections.first_observed_sequence,
                    EXCLUDED.first_observed_sequence
                ),
                last_observed_sequence = GREATEST(
                    runtime_agent_execution_projections.last_observed_sequence,
                    EXCLUDED.last_observed_sequence
                ),
                updated_at = now()",
        )
        .bind(context.organization_id)
        .bind(context.profile_id)
        .bind(context.workspace_id)
        .bind(context.run_id)
        .bind(&receiver)
        .bind(ordinal)
        .bind(sequence)
        .bind(&event.item_id)
        .bind(&task)
        .bind(observed_at)
        .execute(&mut **transaction)
        .await
        .map_err(|error| format!("Pending Agent task projection error: {error}"))?;
    }
    Ok(())
}

async fn ensure_agent_execution(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    context: &EventRunContext,
    thread_id: &str,
    turn_id: &str,
    sequence: i64,
    observed_at: chrono::DateTime<chrono::Utc>,
) -> Result<(), String> {
    lock_agent_execution(transaction, context.run_id, thread_id).await?;
    let existing = sqlx::query_scalar::<_, Uuid>(
        "SELECT id FROM runtime_agent_execution_projections
         WHERE root_run_id = $1 AND agent_thread_id = $2 AND turn_id = $3",
    )
    .bind(context.run_id)
    .bind(thread_id)
    .bind(turn_id)
    .fetch_optional(&mut **transaction)
    .await
    .map_err(|error| format!("Agent task lookup error: {error}"))?;
    if existing.is_some() {
        return Ok(());
    }

    let bound = sqlx::query_scalar::<_, Uuid>(
        "UPDATE runtime_agent_execution_projections
         SET turn_id = $1, status = 'running', current_behavior = 'Started working',
             started_at = COALESCE(started_at, $2),
             last_observed_sequence = GREATEST(last_observed_sequence, $3),
             updated_at = now()
         WHERE id = (
             SELECT id
             FROM runtime_agent_execution_projections
             WHERE root_run_id = $4 AND agent_thread_id = $5
               AND turn_id IS NULL AND status = 'pending'
             ORDER BY ordinal DESC
             LIMIT 1
         )
         RETURNING id",
    )
    .bind(turn_id)
    .bind(observed_at)
    .bind(sequence)
    .bind(context.run_id)
    .bind(thread_id)
    .fetch_optional(&mut **transaction)
    .await
    .map_err(|error| format!("Agent task Turn binding error: {error}"))?;
    if bound.is_some() {
        return Ok(());
    }

    let last_consumed = sqlx::query_scalar::<_, Option<i64>>(
        "SELECT MAX(COALESCE(assignment_sequence, first_observed_sequence))
         FROM runtime_agent_execution_projections
         WHERE root_run_id = $1 AND agent_thread_id = $2",
    )
    .bind(context.run_id)
    .bind(thread_id)
    .fetch_one(&mut **transaction)
    .await
    .map_err(|error| format!("Agent task sequence lookup error: {error}"))?
    .unwrap_or(0);
    let assignment = sqlx::query(
        "SELECT sequence, item_id, payload->'data'->>'prompt' AS task
         FROM run_events
         WHERE run_id = $1
           AND event_type = 'codex.item.completed'
           AND payload->>'itemType' IN ('collabAgentToolCall', 'collabToolCall')
           AND jsonb_typeof(payload->'data'->'receiverThreadIds') = 'array'
           AND (payload->'data'->'receiverThreadIds') ? $2
           AND sequence > $3
           AND NULLIF(BTRIM(payload->'data'->>'prompt'), '') IS NOT NULL
         ORDER BY sequence DESC
         LIMIT 1",
    )
    .bind(context.run_id)
    .bind(thread_id)
    .bind(last_consumed)
    .fetch_optional(&mut **transaction)
    .await
    .map_err(|error| format!("Agent task prompt lookup error: {error}"))?;
    let assignment_sequence = assignment.as_ref().map(|row| row.get::<i64, _>("sequence"));
    let assignment_item_id = assignment
        .as_ref()
        .and_then(|row| row.get::<Option<String>, _>("item_id"));
    let task = assignment
        .as_ref()
        .and_then(|row| row.get::<Option<String>, _>("task"))
        .and_then(|value| truncated_projection_text(&value, 1_000));
    let first_observed_sequence = assignment_sequence.unwrap_or(sequence);
    let ordinal = next_agent_execution_ordinal(transaction, context.run_id, thread_id).await?;

    sqlx::query(
        "INSERT INTO runtime_agent_execution_projections (
            organization_id, profile_id, workspace_id, root_run_id,
            agent_thread_id, turn_id, ordinal, assignment_sequence,
            assignment_item_id, task, status, current_behavior,
            first_observed_sequence, last_observed_sequence, started_at,
            created_at, updated_at
         ) VALUES (
            $1, $2, $3, $4, $5, $6, $7, $8,
            $9, $10, 'running', 'Started working',
            $11, $12, $13, $13, $13
         )
         ON CONFLICT (root_run_id, agent_thread_id, turn_id) DO NOTHING",
    )
    .bind(context.organization_id)
    .bind(context.profile_id)
    .bind(context.workspace_id)
    .bind(context.run_id)
    .bind(thread_id)
    .bind(turn_id)
    .bind(ordinal)
    .bind(assignment_sequence)
    .bind(assignment_item_id)
    .bind(task)
    .bind(first_observed_sequence)
    .bind(sequence)
    .bind(observed_at)
    .execute(&mut **transaction)
    .await
    .map_err(|error| format!("Agent task Turn projection error: {error}"))?;
    Ok(())
}

async fn update_agent_execution(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    context: &EventRunContext,
    event: &ProjectedEvent,
    turn_id: &str,
    sequence: i64,
    observed_at: chrono::DateTime<chrono::Utc>,
) -> Result<(), String> {
    let observation = agent_execution_observation(event);
    let Some(observation) = observation else {
        return Ok(());
    };
    let terminal = matches!(
        observation.status,
        Some("completed" | "failed" | "interrupted")
    );
    sqlx::query(
        "UPDATE runtime_agent_execution_projections
         SET status = COALESCE($1, status),
             current_behavior = COALESCE($2, current_behavior),
             latest_progress = COALESCE($3, latest_progress),
             last_observed_sequence = GREATEST(last_observed_sequence, $4),
             completed_at = CASE WHEN $5 THEN COALESCE(completed_at, $6) ELSE completed_at END,
             updated_at = now()
         WHERE root_run_id = $7 AND agent_thread_id = $8 AND turn_id = $9
           AND status NOT IN ('completed', 'failed', 'interrupted')",
    )
    .bind(observation.status)
    .bind(observation.behavior)
    .bind(observation.progress)
    .bind(sequence)
    .bind(terminal)
    .bind(observed_at)
    .bind(context.run_id)
    .bind(&event.thread_id)
    .bind(turn_id)
    .execute(&mut **transaction)
    .await
    .map_err(|error| format!("Agent task activity projection error: {error}"))?;
    Ok(())
}

async fn project_turnless_agent_terminal(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    context: &EventRunContext,
    event: &ProjectedEvent,
    sequence: i64,
    observed_at: chrono::DateTime<chrono::Utc>,
) -> Result<(), String> {
    let (status, behavior) = match event.event_type.as_str() {
        "codex.thread.completed" => ("completed", "Completed the assigned work"),
        "codex.thread.failed" => ("failed", "Agent execution failed"),
        _ => return Ok(()),
    };
    lock_agent_execution(transaction, context.run_id, &event.thread_id).await?;
    sqlx::query(
        "UPDATE runtime_agent_execution_projections
         SET status = $1, current_behavior = $2,
             last_observed_sequence = GREATEST(last_observed_sequence, $3),
             completed_at = COALESCE(completed_at, $4), updated_at = now()
         WHERE id = (
             SELECT id
             FROM runtime_agent_execution_projections
             WHERE root_run_id = $5 AND agent_thread_id = $6
               AND status NOT IN ('completed', 'failed', 'interrupted')
             ORDER BY ordinal DESC
             LIMIT 1
         )",
    )
    .bind(status)
    .bind(behavior)
    .bind(sequence)
    .bind(observed_at)
    .bind(context.run_id)
    .bind(&event.thread_id)
    .execute(&mut **transaction)
    .await
    .map_err(|error| format!("Agent task terminal projection error: {error}"))?;
    Ok(())
}

struct AgentExecutionObservation {
    status: Option<&'static str>,
    behavior: Option<String>,
    progress: Option<String>,
}

fn agent_execution_observation(event: &ProjectedEvent) -> Option<AgentExecutionObservation> {
    match event.event_type.as_str() {
        "codex.turn.started" => Some(AgentExecutionObservation {
            status: Some("running"),
            behavior: Some("Started working".to_string()),
            progress: None,
        }),
        "codex.turn.completed" => {
            let status = projected_turn_terminal_status(&event.payload);
            let behavior = match status {
                "failed" => "Agent execution failed",
                "interrupted" => "Agent execution interrupted",
                _ => "Finished this work cycle",
            };
            Some(AgentExecutionObservation {
                status: Some(status),
                behavior: Some(behavior.to_string()),
                progress: None,
            })
        }
        "platform.approval.requested" => Some(AgentExecutionObservation {
            status: Some("waiting"),
            behavior: Some("Waiting for approval".to_string()),
            progress: None,
        }),
        "platform.approval.resolved" => Some(AgentExecutionObservation {
            status: Some("running"),
            behavior: Some("Approval resolved; continuing work".to_string()),
            progress: None,
        }),
        "codex.item.started" | "codex.item.completed" => project_agent_item_observation(event),
        _ => None,
    }
}

fn project_agent_item_observation(event: &ProjectedEvent) -> Option<AgentExecutionObservation> {
    let item_type = event
        .payload
        .get("itemType")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let data = event.payload.get("data").unwrap_or(&Value::Null);
    let completed = event.event_type == "codex.item.completed";
    if item_type == "agentMessage" && completed {
        let phase = data.get("phase").and_then(Value::as_str)?;
        let progress = data
            .get("text")
            .and_then(Value::as_str)
            .and_then(|value| truncated_projection_text(value, 1_000));
        let behavior = match phase {
            "commentary" => "Reported progress",
            "final_answer" => "Returned results to the Supervisor",
            _ => return None,
        };
        return Some(AgentExecutionObservation {
            status: None,
            behavior: Some(behavior.to_string()),
            progress,
        });
    }

    let failed = completed
        && (data.get("error").is_some_and(|value| !value.is_null())
            || data.get("success").and_then(Value::as_bool) == Some(false)
            || matches!(
                data.get("status").and_then(Value::as_str),
                Some("failed" | "error")
            ));
    let subject = match item_type {
        "mcpToolCall" => {
            let server = data
                .get("server")
                .and_then(Value::as_str)
                .map(display_agent_identifier);
            let tool = data
                .get("tool")
                .and_then(Value::as_str)
                .map(display_agent_identifier);
            match (server, tool) {
                (Some(server), Some(tool)) => format!("{server} · {tool}"),
                (Some(server), None) => server,
                (None, Some(tool)) => tool,
                (None, None) => "an enterprise tool".to_string(),
            }
        }
        "dynamicToolCall" => data
            .get("tool")
            .and_then(Value::as_str)
            .map(display_agent_identifier)
            .unwrap_or_else(|| "a Runtime tool".to_string()),
        "commandExecution" => "a workspace command".to_string(),
        "webSearch" => "web research".to_string(),
        "imageView" => "image inspection".to_string(),
        "imageGeneration" => "image generation".to_string(),
        _ => return None,
    };
    let verb = if failed {
        "Could not complete"
    } else if completed {
        "Completed"
    } else {
        "Using"
    };
    Some(AgentExecutionObservation {
        status: None,
        behavior: Some(format!("{verb} {subject}")),
        progress: None,
    })
}

fn projected_turn_terminal_status(payload: &Value) -> &'static str {
    let status = payload
        .pointer("/data/status")
        .and_then(Value::as_str)
        .or_else(|| payload.pointer("/data/status/type").and_then(Value::as_str))
        .or_else(|| payload.pointer("/data/turn/status").and_then(Value::as_str))
        .or_else(|| {
            payload
                .pointer("/data/turn/status/type")
                .and_then(Value::as_str)
        })
        .unwrap_or_default()
        .to_ascii_lowercase();
    if status.contains("fail") || status.contains("error") {
        "failed"
    } else if status.contains("interrupt") || status.contains("cancel") {
        "interrupted"
    } else {
        "completed"
    }
}

async fn lock_agent_execution(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    run_id: Uuid,
    thread_id: &str,
) -> Result<(), String> {
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
        .bind(format!("{run_id}:{thread_id}"))
        .execute(&mut **transaction)
        .await
        .map_err(|error| format!("Agent task projection lock error: {error}"))?;
    Ok(())
}

async fn next_agent_execution_ordinal(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    run_id: Uuid,
    thread_id: &str,
) -> Result<i32, String> {
    sqlx::query_scalar::<_, i32>(
        "SELECT COALESCE(MAX(ordinal), 0) + 1
         FROM runtime_agent_execution_projections
         WHERE root_run_id = $1 AND agent_thread_id = $2",
    )
    .bind(run_id)
    .bind(thread_id)
    .fetch_one(&mut **transaction)
    .await
    .map_err(|error| format!("Agent task ordinal error: {error}"))
}

fn normalize_agent_tool(value: &str) -> String {
    value
        .chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

fn display_agent_identifier(value: &str) -> String {
    value
        .trim()
        .trim_start_matches("mcp__")
        .replace(['_', '-'], " ")
}

fn truncated_projection_text(value: &str, max_chars: usize) -> Option<String> {
    let value = value.trim();
    if value.is_empty() {
        return None;
    }
    Some(value.chars().take(max_chars).collect())
}

fn project_event_data(method: &str, params: &Map<String, Value>) -> Value {
    let mut data = Map::new();
    data.insert("sourceType".to_string(), Value::String(method.to_string()));
    if method == "serverRequest/resolved" {
        if let Some(request_id) = params
            .get("requestId")
            .and_then(Value::as_str)
            .filter(|value| Uuid::parse_str(value).is_ok())
        {
            data.insert(
                "requestId".to_string(),
                Value::String(request_id.to_string()),
            );
        }
    }
    for key in [
        "approvalId",
        "approvalStatus",
        "requestMethod",
        "requestParams",
        "error",
        "message",
        "additionalDetails",
        "codexErrorInfo",
        "failureReason",
        "name",
        "status",
        "thread",
        "turn",
        "delta",
        "summaryIndex",
        "contentIndex",
        "startedAtMs",
        "completedAtMs",
        "startedAt",
        "started_at",
        "explanation",
        "plan",
        "steps",
        "diff",
        "threadName",
        "tokenUsage",
        "model",
        "reasoningEffort",
        "sandbox",
        "approvalPolicy",
        "goal",
        "rateLimits",
        "command",
        "stdin",
    ] {
        if let Some(value) = params.get(key) {
            data.insert(key.to_string(), sanitize_value(value, key));
        }
    }
    Value::Object(data)
}

pub(crate) fn project_item(item: &Map<String, Value>) -> Value {
    let item_type = string_field(item, "type").unwrap_or_else(|| "unknown".to_string());
    let mut projected = Map::new();
    projected.insert("type".to_string(), Value::String(item_type.clone()));
    if let Some(status) = item.get("status") {
        projected.insert("status".to_string(), sanitize_value(status, "status"));
    }

    let fields: &[&str] = match item_type.as_str() {
        "userMessage" => &["content"],
        "hookPrompt" => &["fragments"],
        "agentMessage" => &["text"],
        "plan" => &["text"],
        "reasoning" => &["summary", "content"],
        "commandExecution" => &[
            "command",
            "aggregatedOutput",
            "exitCode",
            "durationMs",
            "commandActions",
        ],
        "fileChange" => &["changes"],
        "mcpToolCall" => &["server", "tool", "arguments", "result", "error"],
        "dynamicToolCall" => &["namespace", "tool", "arguments", "contentItems", "success"],
        "collabAgentToolCall" | "collabToolCall" => &[
            "tool",
            "prompt",
            "senderThreadId",
            "receiverThreadIds",
            "agentsStates",
        ],
        "subAgentActivity" => &["kind", "agentThreadId", "agentPath"],
        "webSearch" => &["query", "action"],
        "imageView" => &["path"],
        "imageGeneration" => &["revisedPrompt", "result", "savedPath"],
        "sleep" => &["durationMs"],
        "enteredReviewMode" | "exitedReviewMode" => &["review"],
        "contextCompaction" => &[],
        _ => {
            projected.insert(
                "summary".to_string(),
                Value::String("Unsupported runtime item".to_string()),
            );
            &[]
        }
    };
    for key in fields {
        if let Some(value) = item.get(*key) {
            projected.insert((*key).to_string(), sanitize_value(value, key));
        }
    }
    if item_type == "agentMessage" {
        if let Some(phase @ ("commentary" | "final_answer")) =
            item.get("phase").and_then(Value::as_str)
        {
            projected.insert("phase".to_string(), Value::String(phase.to_string()));
        }
    }
    if item_type == "mcpToolCall" {
        if let Some(result) = projected.get_mut("result") {
            redact_mcp_resource_metadata(result);
        }
    }
    Value::Object(projected)
}

fn project_inline_visualization_artifact(
    item: &Map<String, Value>,
) -> Option<InlineVisualizationArtifactCandidate> {
    let structured = item
        .get("result")?
        .as_object()?
        .get("structuredContent")?
        .as_object()?;
    if structured.get("type")?.as_str()? != "open-web-artifact"
        || structured.get("kind")?.as_str()? != "inline-visualization.v1"
        || structured.keys().any(|key| {
            !matches!(
                key.as_str(),
                "type" | "kind" | "artifact" | "embed" | "warnings"
            )
        })
    {
        return None;
    }
    if let Some(warnings) = structured.get("warnings") {
        validate_inline_visualization_warnings(warnings)?;
    }
    let artifact = structured.get("artifact")?.as_object()?;
    if artifact
        .keys()
        .any(|key| !matches!(key.as_str(), "ref" | "renderer"))
    {
        return None;
    }
    let artifact_ref = artifact.get("ref")?.as_str()?.trim();
    if !valid_card_identifier(artifact_ref) {
        return None;
    }
    let renderer = artifact.get("renderer")?.as_object()?;
    if renderer
        .keys()
        .any(|key| !matches!(key.as_str(), "kind" | "payload"))
    {
        return None;
    }
    let renderer_kind = renderer.get("kind")?.as_str()?.trim();
    let renderer_payload =
        project_inline_renderer(renderer_kind, renderer.get("payload")?.as_object()?)?;
    let embed = structured.get("embed")?.as_object()?;
    if embed
        .keys()
        .any(|key| !matches!(key.as_str(), "syntax" | "code"))
        || embed.get("syntax")?.as_str()? != "codex-inline-vis.artifact.v1"
        || embed.get("code")?.as_str()?
            != format!("::codex-inline-vis{{artifact=\"{artifact_ref}\"}}")
    {
        return None;
    }
    Some(InlineVisualizationArtifactCandidate {
        artifact_ref: artifact_ref.to_string(),
        renderer_kind: renderer_kind.to_string(),
        renderer_payload,
    })
}

fn validate_inline_visualization_warnings(value: &Value) -> Option<()> {
    let warnings = value.as_array()?;
    for warning in warnings {
        let warning = warning.as_object()?;
        if warning
            .keys()
            .any(|key| !matches!(key.as_str(), "code" | "path" | "message"))
            || warning.len() < 2
            || warning.len() > 3
        {
            return None;
        }
        if !matches!(
            warning.get("code")?.as_str()?,
            "ignored_extra_input" | "mapbox_style_warning"
        ) {
            return None;
        }
        let path = warning.get("path")?.as_str()?;
        if path.is_empty() || path.chars().count() > 512 || path.chars().any(char::is_control) {
            return None;
        }
        if let Some(message) = warning.get("message") {
            let message = message.as_str()?;
            if message.is_empty()
                || message.chars().count() > 1024
                || message.chars().any(char::is_control)
            {
                return None;
            }
        }
    }
    Some(())
}

fn project_inline_renderer(kind: &str, payload: &Map<String, Value>) -> Option<Value> {
    match kind {
        "map.v3" => project_map_card_v3(payload),
        _ => None,
    }
}

fn project_map_card_v3(card: &Map<String, Value>) -> Option<Value> {
    const CARD_FIELDS: &[&str] = &[
        "title",
        "intent",
        "status",
        "fallback_text",
        "summary",
        "sources",
        "layers",
        "center",
        "zoom",
        "bearing",
        "pitch",
        "extensions",
    ];
    if card.keys().any(|key| !CARD_FIELDS.contains(&key.as_str())) {
        return None;
    }
    let title = nonempty_string(card, "title")?;
    let intent = nonempty_string(card, "intent")?;
    let status = nonempty_string(card, "status")?;
    if !matches!(status.as_str(), "loading" | "ready" | "error") {
        return None;
    }

    let mut projected = Map::from_iter([
        ("title".to_string(), Value::String(title)),
        ("intent".to_string(), Value::String(intent)),
        ("status".to_string(), Value::String(status)),
    ]);
    for key in ["fallback_text", "summary"] {
        if let Some(value) = optional_string(card, key)? {
            projected.insert(key.to_string(), Value::String(value));
        }
    }

    let sources = project_map_v3_sources(card.get("sources")?)?;
    let source_ids = sources
        .keys()
        .map(String::as_str)
        .collect::<std::collections::HashSet<_>>();
    let layers = card.get("layers")?.as_array()?;
    if layers.is_empty() {
        return None;
    }
    let mut layer_ids = std::collections::HashSet::new();
    let mut source_backed_layer_ids = std::collections::HashSet::new();
    for layer in layers {
        let layer = layer.as_object()?;
        if sanitize_value(&Value::Object(layer.clone()), "layer") != Value::Object(layer.clone()) {
            return None;
        }
        let id = nonempty_string(layer, "id")?;
        if !valid_mapbox_identifier(&id) || !layer_ids.insert(id.clone()) {
            return None;
        }
        nonempty_string(layer, "type")?;
        if let Some(source) = layer.get("source") {
            let source = source.as_str()?;
            if !source_ids.contains(source) {
                return None;
            }
            source_backed_layer_ids.insert(id);
        }
    }
    projected.insert("sources".to_string(), Value::Object(sources));
    projected.insert("layers".to_string(), Value::Array(layers.clone()));

    let center = card.get("center");
    let zoom = card.get("zoom");
    if center.is_some() != zoom.is_some() {
        return None;
    }
    if let (Some(center), Some(zoom)) = (center, zoom) {
        let center = center.as_array()?;
        if center.len() != 2 {
            return None;
        }
        projected.insert(
            "center".to_string(),
            json!([
                bounded_number(&center[0], -180.0, 180.0)?,
                bounded_number(&center[1], -90.0, 90.0)?
            ]),
        );
        projected.insert("zoom".to_string(), json!(bounded_number(zoom, 0.0, 24.0)?));
    }
    for (key, minimum, maximum) in [("bearing", -180.0, 180.0), ("pitch", 0.0, 85.0)] {
        if let Some(value) = card.get(key) {
            projected.insert(
                key.to_string(),
                json!(bounded_number(value, minimum, maximum)?),
            );
        }
    }
    if let Some(extensions) = card.get("extensions") {
        projected.insert(
            "extensions".to_string(),
            project_map_v3_extensions(extensions, &layer_ids, &source_backed_layer_ids)?,
        );
    }
    Some(Value::Object(projected))
}

fn project_map_v3_sources(value: &Value) -> Option<Map<String, Value>> {
    let sources = value.as_object()?;
    if sources.is_empty() {
        return None;
    }
    sources
        .iter()
        .map(|(id, source)| {
            if !valid_mapbox_identifier(id) {
                return None;
            }
            let source = source.as_object()?;
            if source.get("type")?.as_str()? != "geojson" {
                return None;
            }
            let mut source_options = source.clone();
            source_options.remove("data");
            if sanitize_value(&Value::Object(source_options.clone()), "source")
                != Value::Object(source_options)
            {
                return None;
            }
            let data = source.get("data")?.as_object()?;
            let data = match data.get("type")?.as_str()? {
                "mcp_resource" => {
                    if data
                        .keys()
                        .any(|key| !matches!(key.as_str(), "type" | "server" | "uri" | "format"))
                        || data.get("format")?.as_str()? != "geojson"
                    {
                        return None;
                    }
                    let server = nonempty_string(data, "server")?;
                    let uri = nonempty_string(data, "uri")?;
                    if !valid_card_identifier(&server)
                        || server.starts_with("mcp__")
                        || !valid_geojson_resource_uri(&uri)
                    {
                        return None;
                    }
                    json!({
                        "type": "mcp_resource",
                        "server": server,
                        "uri": uri,
                        "format": "geojson"
                    })
                }
                "inline" => {
                    if data
                        .keys()
                        .any(|key| !matches!(key.as_str(), "type" | "format" | "geojson"))
                        || data.get("format")?.as_str()? != "geojson"
                    {
                        return None;
                    }
                    let geojson = data.get("geojson")?;
                    if !valid_geojson_root(geojson) {
                        return None;
                    }
                    json!({
                        "type": "inline",
                        "format": "geojson",
                        "geojson": sanitize_value(geojson, "geojson"),
                    })
                }
                _ => return None,
            };
            let mut projected = source.clone();
            projected.insert("data".to_string(), data);
            Some((id.clone(), Value::Object(projected)))
        })
        .collect()
}

fn project_map_v3_extensions(
    value: &Value,
    layer_ids: &std::collections::HashSet<String>,
    source_backed_layer_ids: &std::collections::HashSet<String>,
) -> Option<Value> {
    let extensions = value.as_object()?;
    if extensions
        .keys()
        .any(|key| !matches!(key.as_str(), "hover" | "legend"))
    {
        return None;
    }
    if let Some(hover) = extensions.get("hover") {
        let layers = hover.get("layers")?.as_array()?;
        for layer in layers {
            let layer = layer.as_object()?;
            if layer
                .keys()
                .any(|key| !matches!(key.as_str(), "layer" | "title_property" | "fields"))
            {
                return None;
            }
            let layer_id = nonempty_string(layer, "layer")?;
            if !layer_ids.contains(&layer_id) || !source_backed_layer_ids.contains(&layer_id) {
                return None;
            }
            if let Some(title) = layer.get("title_property") {
                if title.as_str()?.trim().is_empty() {
                    return None;
                }
            }
            let fields = layer.get("fields")?.as_array()?;
            for field in fields {
                if let Some(property) = field.as_str() {
                    if property.trim().is_empty() {
                        return None;
                    }
                } else {
                    let field = field.as_object()?;
                    if field
                        .keys()
                        .any(|key| !matches!(key.as_str(), "property" | "label"))
                        || nonempty_string(field, "property").is_none()
                    {
                        return None;
                    }
                    if field.get("label").is_some() && optional_string(field, "label")?.is_none() {
                        return None;
                    }
                }
            }
        }
    }
    if let Some(legend) = extensions.get("legend") {
        let items = legend.get("items")?.as_array()?;
        if items.is_empty() {
            return None;
        }
        for item in items {
            let item = item.as_object()?;
            if item
                .keys()
                .any(|key| !matches!(key.as_str(), "label" | "color" | "type"))
                || nonempty_string(item, "label").is_none()
                || nonempty_string(item, "color").is_none()
            {
                return None;
            }
            if let Some(kind) = item.get("type") {
                if !matches!(kind.as_str()?, "circle" | "line" | "fill") {
                    return None;
                }
            }
        }
    }
    Some(value.clone())
}

fn bounded_number(value: &Value, minimum: f64, maximum: f64) -> Option<f64> {
    let value = value.as_f64()?;
    value
        .is_finite()
        .then_some(value)
        .filter(|value| (minimum..=maximum).contains(value))
}

fn valid_geojson_root(value: &Value) -> bool {
    let Some(object) = value.as_object() else {
        return false;
    };
    matches!(
        object.get("type").and_then(Value::as_str),
        Some(
            "FeatureCollection"
                | "Feature"
                | "GeometryCollection"
                | "Point"
                | "MultiPoint"
                | "LineString"
                | "MultiLineString"
                | "Polygon"
                | "MultiPolygon"
        )
    )
}

fn valid_card_identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
}

fn valid_mapbox_identifier(value: &str) -> bool {
    !value.trim().is_empty() && !value.chars().any(char::is_control)
}

fn valid_artifact_resource_uri(value: &str) -> bool {
    if value.is_empty() || value.len() > 2048 || value.chars().any(char::is_control) {
        return false;
    }
    let Some((scheme, resource)) = value.split_once("://") else {
        return false;
    };
    !resource.is_empty()
        && !matches!(scheme, "http" | "https" | "file")
        && scheme.bytes().enumerate().all(|(index, byte)| {
            byte.is_ascii_lowercase()
                || (index > 0 && (byte.is_ascii_digit() || matches!(byte, b'+' | b'-' | b'.')))
        })
}

fn valid_geojson_resource_uri(value: &str) -> bool {
    value
        .strip_prefix("maps-data://geojson/")
        .is_some_and(valid_card_identifier)
}

fn artifact_schema(title: Option<&str>, mime_type: &str) -> String {
    title
        .map(str::trim)
        .filter(|value| valid_card_identifier(value) && value.contains('.'))
        .map(str::to_string)
        .unwrap_or_else(|| {
            if mime_type == "application/geo+json" {
                "geojson.v1".to_string()
            } else {
                "mcp-resource.v1".to_string()
            }
        })
}

fn artifact_link(content: &Value) -> Option<ArtifactCandidate> {
    let content = content.as_object()?;
    if content.get("type")?.as_str()? != "resource_link" {
        return None;
    }
    let uri = content.get("uri")?.as_str()?.trim();
    if !valid_artifact_resource_uri(uri) {
        return None;
    }
    let mime_type = content
        .get("mimeType")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty());
    if !matches!(mime_type, Some("application/geo+json" | "application/json")) {
        return None;
    }
    let mime_type = mime_type.expect("supported MIME type").to_string();
    let title = content.get("title").and_then(Value::as_str);
    let display_name = title
        .or_else(|| content.get("name").and_then(Value::as_str))
        .and_then(|value| bounded_text(value, 160))
        .unwrap_or_else(|| "MCP Resource".to_string());
    let expected_size = content
        .get("size")
        .and_then(Value::as_u64)
        .and_then(|value| i64::try_from(value).ok());
    Some(ArtifactCandidate {
        artifact_schema: artifact_schema(title, &mime_type),
        display_name,
        uri: uri.to_string(),
        mime_type,
        expected_size,
    })
}

fn artifact_candidates(item: &Map<String, Value>) -> impl Iterator<Item = ArtifactCandidate> + '_ {
    item.get("result")
        .and_then(Value::as_object)
        .and_then(|result| result.get("content"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(artifact_link)
}

fn redact_mcp_resource_metadata(result: &mut Value) {
    if let Some(structured) = result
        .get_mut("structuredContent")
        .and_then(Value::as_object_mut)
    {
        if structured.get("type").and_then(Value::as_str) == Some("open-web-artifact")
            && structured.get("kind").and_then(Value::as_str) == Some("inline-visualization.v1")
        {
            let artifact_ref = structured
                .get("artifact")
                .and_then(Value::as_object)
                .and_then(|artifact| artifact.get("ref"))
                .and_then(Value::as_str)
                .map(str::to_string);
            let renderer_kind = structured
                .get("artifact")
                .and_then(Value::as_object)
                .and_then(|artifact| artifact.get("renderer"))
                .and_then(Value::as_object)
                .and_then(|renderer| renderer.get("kind"))
                .and_then(Value::as_str)
                .map(str::to_string);
            let embed = structured.get("embed").cloned();
            *structured = Map::from_iter([
                (
                    "type".to_string(),
                    Value::String("open-web-artifact".to_string()),
                ),
                (
                    "kind".to_string(),
                    Value::String("inline-visualization.v1".to_string()),
                ),
                (
                    "artifact".to_string(),
                    json!({
                        "ref": artifact_ref,
                        "renderer": { "kind": renderer_kind },
                    }),
                ),
                ("embed".to_string(), embed.unwrap_or(Value::Null)),
            ]);
        }
        if structured
            .get("data_ref")
            .and_then(Value::as_object)
            .and_then(|data_ref| data_ref.get("type"))
            .and_then(Value::as_str)
            == Some("mcp_resource")
        {
            structured.remove("data_ref");
        }
    }
    let Some(content) = result
        .as_object_mut()
        .and_then(|result| result.get_mut("content"))
        .and_then(Value::as_array_mut)
    else {
        return;
    };
    for item in content {
        let Some(item) = item.as_object_mut() else {
            continue;
        };
        if item.get("type").and_then(Value::as_str) == Some("resource_link") {
            item.remove("uri");
            item.remove("_meta");
        }
    }
}

async fn register_artifacts(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    context: &EventRunContext,
    event: &ProjectedEvent,
) -> Result<Vec<RegisteredArtifact>, String> {
    if event.event_type != "codex.item.completed"
        || event.payload.pointer("/itemType").and_then(Value::as_str) != Some("mcpToolCall")
    {
        return Ok(Vec::new());
    }
    let Some(turn_id) = event.turn_id.as_deref() else {
        return Ok(Vec::new());
    };
    let Some(item_id) = event.item_id.as_deref() else {
        return Ok(Vec::new());
    };
    let Some(server) = event
        .payload
        .pointer("/data/server")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return Ok(Vec::new());
    };
    if bounded_text(server, 256).is_none() {
        return Err("MCP server identity is invalid".to_string());
    }

    let mut registered = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for artifact in &event.artifacts {
        if !seen.insert(artifact.uri.as_str()) {
            continue;
        }
        let inserted = sqlx::query(
            "INSERT INTO artifacts (
                organization_id, profile_id, artifact_schema, display_name, mime_type,
                expected_size, source_server, source_uri
             ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
             ON CONFLICT (organization_id, profile_id, source_server, source_uri)
             DO NOTHING
             RETURNING id, state",
        )
        .bind(context.organization_id)
        .bind(context.profile_id)
        .bind(&artifact.artifact_schema)
        .bind(&artifact.display_name)
        .bind(&artifact.mime_type)
        .bind(artifact.expected_size)
        .bind(server)
        .bind(&artifact.uri)
        .fetch_optional(&mut **transaction)
        .await
        .map_err(|error| format!("Artifact registration error: {error}"))?;
        let (artifact_id, state) = if let Some(inserted) = inserted {
            (
                inserted.get::<Uuid, _>("id"),
                inserted.get::<String, _>("state"),
            )
        } else {
            let existing = sqlx::query(
                "SELECT id, artifact_schema, display_name, mime_type, expected_size, state
                 FROM artifacts
                 WHERE organization_id = $1 AND profile_id = $2
                   AND source_server = $3 AND source_uri = $4",
            )
            .bind(context.organization_id)
            .bind(context.profile_id)
            .bind(server)
            .bind(&artifact.uri)
            .fetch_optional(&mut **transaction)
            .await
            .map_err(|error| format!("Artifact conflict lookup error: {error}"))?
            .ok_or_else(|| "Artifact conflict could not be resolved".to_string())?;
            if existing.get::<String, _>("artifact_schema") != artifact.artifact_schema
                || existing.get::<String, _>("display_name") != artifact.display_name
                || existing.get::<String, _>("mime_type") != artifact.mime_type
                || existing.get::<Option<i64>, _>("expected_size") != artifact.expected_size
            {
                return Err(
                    "MCP Resource identity was reused with different immutable metadata"
                        .to_string(),
                );
            }
            (
                existing.get::<Uuid, _>("id"),
                existing.get::<String, _>("state"),
            )
        };

        sqlx::query(
            "INSERT INTO artifact_task_grants (
                artifact_id, organization_id, task_id, permission
             ) VALUES ($1, $2, $3, 'read')
             ON CONFLICT (artifact_id, task_id) DO NOTHING",
        )
        .bind(artifact_id)
        .bind(context.organization_id)
        .bind(context.task_id)
        .execute(&mut **transaction)
        .await
        .map_err(|error| format!("Artifact Task grant error: {error}"))?;
        sqlx::query(
            "INSERT INTO artifact_provenance (
                artifact_id, organization_id, producer_run_id, producer_thread_id,
                producer_turn_id, producer_item_id
             ) VALUES ($1, $2, $3, $4, $5, $6)
             ON CONFLICT DO NOTHING",
        )
        .bind(artifact_id)
        .bind(context.organization_id)
        .bind(context.run_id)
        .bind(&event.thread_id)
        .bind(turn_id)
        .bind(item_id)
        .execute(&mut **transaction)
        .await
        .map_err(|error| format!("Artifact provenance error: {error}"))?;

        registered.push(RegisteredArtifact {
            id: artifact_id,
            artifact_schema: artifact.artifact_schema.clone(),
            display_name: artifact.display_name.clone(),
            mime_type: artifact.mime_type.clone(),
            expected_size: artifact.expected_size,
            state,
        });
    }
    Ok(registered)
}

fn project_registered_artifacts(payload: &mut Value, artifacts: &[RegisteredArtifact]) {
    if artifacts.is_empty() {
        return;
    }
    let projected = artifacts
        .iter()
        .map(|artifact| {
            json!({
                "artifactId": artifact.id,
                "schema": artifact.artifact_schema,
                "displayName": artifact.display_name,
                "mimeType": artifact.mime_type,
                "expectedSize": artifact.expected_size,
                "state": artifact.state,
                "url": format!("/api/artifacts/{}/content", artifact.id),
            })
        })
        .collect();
    if let Some(data) = payload.pointer_mut("/data").and_then(Value::as_object_mut) {
        data.insert("artifacts".to_string(), Value::Array(projected));
    }
}

async fn register_inline_visualization_artifact(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    event: &ProjectedEvent,
    run_id: Uuid,
    organization_id: Uuid,
) -> Result<(), String> {
    if event.event_type != "codex.item.completed"
        || event.payload.pointer("/itemType").and_then(Value::as_str) != Some("mcpToolCall")
    {
        return Ok(());
    }
    let Some(candidate) = event.inline_artifact.as_ref() else {
        return Ok(());
    };
    let Some(turn_id) = event.turn_id.as_deref() else {
        return Ok(());
    };
    let Some(item_id) = event.item_id.as_deref() else {
        return Ok(());
    };
    let mut renderer_payload = candidate.renderer_payload.clone();
    resolve_inline_renderer_resources(
        transaction,
        run_id,
        &event.thread_id,
        item_id,
        &candidate.renderer_kind,
        &mut renderer_payload,
    )
    .await?;

    let inserted = sqlx::query(
        "INSERT INTO inline_visualization_artifacts (
            organization_id, run_id, thread_id, producer_turn_id, producer_item_id,
            artifact_ref, renderer_kind, renderer_payload
         ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
         ON CONFLICT (run_id, artifact_ref) DO NOTHING",
    )
    .bind(organization_id)
    .bind(run_id)
    .bind(&event.thread_id)
    .bind(turn_id)
    .bind(item_id)
    .bind(&candidate.artifact_ref)
    .bind(&candidate.renderer_kind)
    .bind(&renderer_payload)
    .execute(&mut **transaction)
    .await
    .map_err(|error| format!("inline visualization Artifact registration error: {error}"))?;
    if inserted.rows_affected() == 1 {
        return Ok(());
    }

    let existing = sqlx::query(
        "SELECT producer_item_id, renderer_kind, renderer_payload
         FROM inline_visualization_artifacts
         WHERE run_id = $1 AND artifact_ref = $2",
    )
    .bind(run_id)
    .bind(&candidate.artifact_ref)
    .fetch_optional(&mut **transaction)
    .await
    .map_err(|error| format!("inline visualization Artifact conflict lookup error: {error}"))?;
    let Some(existing) = existing else {
        return Err("inline visualization Artifact conflict could not be resolved".to_string());
    };
    let identical = existing.get::<String, _>("producer_item_id") == item_id
        && existing.get::<String, _>("renderer_kind") == candidate.renderer_kind
        && existing.get::<Value, _>("renderer_payload") == renderer_payload;
    if identical {
        Ok(())
    } else {
        Err(format!(
            "inline visualization Artifact ref {} was already registered",
            candidate.artifact_ref
        ))
    }
}

async fn resolve_inline_renderer_resources(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    run_id: Uuid,
    thread_id: &str,
    producer_item_id: &str,
    renderer_kind: &str,
    renderer_payload: &mut Value,
) -> Result<(), String> {
    match renderer_kind {
        "map.v3" => {
            resolve_map_resource_refs_in_transaction(
                transaction,
                run_id,
                thread_id,
                producer_item_id,
                renderer_payload,
            )
            .await
        }
        unsupported => Err(format!(
            "inline visualization renderer {unsupported} is unsupported"
        )),
    }
}

async fn resolve_map_resource_refs_in_transaction(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    run_id: Uuid,
    thread_id: &str,
    producer_item_id: &str,
    renderer_payload: &mut Value,
) -> Result<(), String> {
    let Some(resource_refs) = map_payload_resource_refs(renderer_payload) else {
        return Ok(());
    };
    let mut resolved = std::collections::HashMap::new();
    for (server, uri) in resource_refs {
        let row = sqlx::query(
            "SELECT artifact.id, artifact.mime_type
             FROM artifacts artifact
             JOIN artifact_provenance provenance
               ON provenance.artifact_id = artifact.id
              AND provenance.organization_id = artifact.organization_id
             WHERE provenance.producer_run_id = $1
               AND provenance.producer_thread_id = $2
               AND artifact.source_server = $3
               AND artifact.source_uri = $4
               AND provenance.producer_item_id <> $5
               AND artifact.state IN ('pending', 'materializing', 'ready')
               AND artifact.retention_state = 'active'",
        )
        .bind(run_id)
        .bind(thread_id)
        .bind(&server)
        .bind(&uri)
        .bind(producer_item_id)
        .fetch_optional(&mut **transaction)
        .await
        .map_err(|error| format!("map Resource Artifact resolution error: {error}"))?;
        let Some(row) = row else {
            return Err(format!(
                "map renderer references unavailable Resource {server} {uri}"
            ));
        };
        resolved.insert(
            (server, uri),
            (
                row.get::<Uuid, _>("id"),
                Some(row.get::<String, _>("mime_type")),
            ),
        );
    }
    replace_map_payload_resource_refs(renderer_payload, &resolved);
    Ok(())
}

async fn resolve_inline_artifacts_in_transaction(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    run_id: Uuid,
    payload: &mut Value,
) -> Result<(), String> {
    if payload.pointer("/itemType").and_then(Value::as_str) != Some("agentMessage") {
        return Ok(());
    }
    let Some(thread_id) = payload.get("threadId").and_then(Value::as_str) else {
        return Ok(());
    };
    let Some(text) = payload.pointer("/data/text").and_then(Value::as_str) else {
        return Ok(());
    };
    let refs = inline_artifact_refs(text);
    if refs.is_empty() {
        return Ok(());
    }
    let mut artifacts = Vec::new();
    for artifact_ref in refs {
        let row = sqlx::query(
            "SELECT renderer_kind, renderer_payload
             FROM inline_visualization_artifacts
             WHERE run_id = $1
               AND thread_id = $2
               AND artifact_ref = $3
               AND state = 'ready'",
        )
        .bind(run_id)
        .bind(thread_id)
        .bind(&artifact_ref)
        .fetch_optional(&mut **transaction)
        .await
        .map_err(|error| format!("inline visualization Artifact resolution error: {error}"))?;
        if let Some(row) = row {
            artifacts.push(json!({
                "ref": artifact_ref,
                "renderer": {
                    "kind": row.get::<String, _>("renderer_kind"),
                    "payload": row.get::<Value, _>("renderer_payload"),
                }
            }));
        }
    }
    if !artifacts.is_empty() {
        payload
            .pointer_mut("/data")
            .and_then(Value::as_object_mut)
            .expect("projected Agent Message data must be an object")
            .insert("inlineArtifacts".to_string(), Value::Array(artifacts));
    }
    Ok(())
}

pub(crate) async fn resolve_inline_artifacts(
    db: &PgPool,
    run_id: Uuid,
    text: &str,
) -> Result<Vec<Value>, sqlx::Error> {
    let refs = inline_artifact_refs(text);
    let mut artifacts = Vec::new();
    for artifact_ref in refs {
        let row = sqlx::query(
            "SELECT artifact.renderer_kind, artifact.renderer_payload
             FROM inline_visualization_artifacts artifact
             JOIN run_events producer
               ON producer.run_id = artifact.run_id
              AND producer.item_id = artifact.producer_item_id
              AND producer.event_type = 'codex.item.completed'
             WHERE artifact.run_id = $1
               AND artifact.thread_id = (
                   SELECT codex_thread_id FROM runs WHERE id = $1
               )
               AND artifact.artifact_ref = $2
               AND artifact.state = 'ready'
               AND producer.thread_id = artifact.thread_id
               AND producer.turn_id = artifact.producer_turn_id",
        )
        .bind(run_id)
        .bind(&artifact_ref)
        .fetch_optional(db)
        .await?;
        if let Some(row) = row {
            artifacts.push(json!({
                "ref": artifact_ref,
                "renderer": {
                    "kind": row.get::<String, _>("renderer_kind"),
                    "payload": row.get::<Value, _>("renderer_payload"),
                }
            }));
        }
    }
    Ok(artifacts)
}

fn inline_artifact_refs(markdown: &str) -> Vec<String> {
    const PREFIX: &str = "::codex-inline-vis{artifact=\"";
    let mut refs = Vec::new();
    let mut fence: Option<(char, usize)> = None;
    for source_line in markdown.lines() {
        let line = source_line.trim_end_matches('\r');
        let leading_spaces = line.bytes().take_while(|byte| *byte == b' ').count();
        let trimmed_start = &line[leading_spaces.min(line.len())..];
        let fence_char = trimmed_start.as_bytes().first().copied();
        if leading_spaces <= 3 && matches!(fence_char, Some(b'`' | b'~')) {
            let marker = fence_char.unwrap() as char;
            let marker_len = trimmed_start
                .chars()
                .take_while(|value| *value == marker)
                .count();
            if marker_len >= 3 {
                match fence {
                    Some((active, minimum)) if active == marker && marker_len >= minimum => {
                        fence = None;
                    }
                    None => fence = Some((marker, marker_len)),
                    _ => {}
                }
                continue;
            }
        }
        if fence.is_some() || leading_spaces >= 4 || line.starts_with('\t') {
            continue;
        }
        let directive = line.trim();
        let Some(value) = directive
            .strip_prefix(PREFIX)
            .and_then(|value| value.strip_suffix("\"}"))
        else {
            continue;
        };
        if valid_card_identifier(value) && !refs.iter().any(|item| item == value) {
            refs.push(value.to_string());
        }
    }
    refs
}

fn map_payload_resource_refs(map_payload: &Value) -> Option<Vec<(String, String)>> {
    let sources = map_payload.get("sources")?.as_object()?;
    let resource_refs = sources
        .values()
        .filter_map(|source| {
            let data = source.get("data")?;
            (data.get("type").and_then(Value::as_str) == Some("mcp_resource"))
                .then(|| {
                    Some((
                        data.get("server")?.as_str()?.to_string(),
                        data.get("uri")?.as_str()?.to_string(),
                    ))
                })
                .flatten()
        })
        .collect::<Vec<_>>();
    (!resource_refs.is_empty()).then_some(resource_refs)
}

fn replace_map_payload_resource_refs(
    map_payload: &mut Value,
    resolved: &std::collections::HashMap<(String, String), (Uuid, Option<String>)>,
) {
    let Some(sources) = map_payload
        .get_mut("sources")
        .and_then(Value::as_object_mut)
    else {
        return;
    };
    for source in sources.values_mut() {
        let Some(data) = source.get_mut("data") else {
            continue;
        };
        if data.get("type").and_then(Value::as_str) != Some("mcp_resource") {
            continue;
        }
        let Some(server) = data.get("server").and_then(Value::as_str) else {
            continue;
        };
        let Some(uri) = data.get("uri").and_then(Value::as_str) else {
            continue;
        };
        let Some((artifact_id, mime_type)) = resolved.get(&(server.to_string(), uri.to_string()))
        else {
            continue;
        };
        *data = json!({
            "type": "artifact",
            "format": "geojson",
            "artifact_id": artifact_id,
            "mime_type": mime_type,
            "url": format!("/api/artifacts/{artifact_id}/content"),
        });
    }
}

fn nonempty_string(values: &Map<String, Value>, key: &str) -> Option<String> {
    values
        .get(key)?
        .as_str()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn optional_string(values: &Map<String, Value>, key: &str) -> Option<Option<String>> {
    match values.get(key) {
        None | Some(Value::Null) => Some(None),
        Some(Value::String(value)) => {
            let value = value.trim();
            (!value.is_empty()).then(|| Some(value.to_string()))
        }
        Some(_) => None,
    }
}

fn sanitize_value(value: &Value, key: &str) -> Value {
    if is_sensitive_key(key) {
        return Value::String("[redacted]".to_string());
    }
    match value {
        Value::Array(values) => Value::Array(
            values
                .iter()
                .map(|value| sanitize_value(value, key))
                .collect(),
        ),
        Value::Object(values) => Value::Object(
            values
                .iter()
                .map(|(entry_key, value)| (entry_key.clone(), sanitize_value(value, entry_key)))
                .collect(),
        ),
        Value::String(value) if is_path_key(key) => {
            Value::String(redact_browser_text(&safe_path(value)))
        }
        Value::String(value) if value.starts_with("data:") => {
            Value::String("[embedded-data]".to_string())
        }
        Value::String(value) => Value::String(redact_browser_text(value)),
        _ => value.clone(),
    }
}

fn redact_browser_text(value: &str) -> String {
    redact_local_paths(&redact_internal_resource_uris(value))
}

/// Remove model-visible, Profile-local MCP Resource identities from browser
/// projections without changing the Runtime history that Agents use for
/// handoff. Public HTTP(S) links remain visible; non-public URI schemes are
/// replaced wherever they occur in prose, JSON snippets, or Tool arguments.
fn redact_internal_resource_uris(value: &str) -> String {
    const REDACTED: &str = "[internal-resource-uri]";

    let bytes = value.as_bytes();
    let mut output = String::with_capacity(value.len());
    let mut copied_until = 0;
    let mut cursor = 0;

    while cursor < bytes.len() {
        if !bytes[cursor].is_ascii_alphabetic()
            || (cursor > 0 && is_uri_scheme_byte(bytes[cursor - 1]))
        {
            cursor += 1;
            continue;
        }

        let mut scheme_end = cursor + 1;
        while scheme_end < bytes.len() && is_uri_scheme_byte(bytes[scheme_end]) {
            scheme_end += 1;
        }
        if bytes.get(scheme_end..scheme_end + 3) != Some(b"://") {
            cursor += 1;
            continue;
        }

        let scheme = value[cursor..scheme_end].to_ascii_lowercase();
        if matches!(scheme.as_str(), "http" | "https") {
            cursor = scheme_end + 3;
            continue;
        }

        let resource_start = scheme_end + 3;
        if resource_start >= bytes.len() || is_uri_terminator(bytes[resource_start]) {
            cursor += 1;
            continue;
        }
        let mut resource_end = resource_start + 1;
        while resource_end < bytes.len() && !is_uri_terminator(bytes[resource_end]) {
            resource_end += 1;
        }

        output.push_str(&value[copied_until..cursor]);
        output.push_str(REDACTED);
        copied_until = resource_end;
        cursor = resource_end;
    }

    if copied_until == 0 {
        return value.to_string();
    }
    output.push_str(&value[copied_until..]);
    output
}

/// Runtime command lines, Tool arguments and textual output can contain local
/// paths even when the field itself is not named `path`. Browser projections
/// must retain useful command/output context without disclosing the host's
/// directory layout. Public URLs are not matched because their path segments
/// do not begin at a shell/path boundary.
fn redact_local_paths(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut output = String::with_capacity(value.len());
    let mut copied_until = 0;
    let mut cursor = 0;

    while cursor < bytes.len() {
        let unix_path = bytes[cursor] == b'/'
            && (cursor == 0 || is_local_path_boundary(bytes[cursor - 1]))
            && bytes.get(cursor + 1).is_some_and(|byte| {
                !byte.is_ascii_whitespace() && !matches!(byte, b'/' | b'>' | b')' | b']' | b'}')
            });
        let windows_path = bytes.get(cursor..cursor + 3).is_some_and(|candidate| {
            candidate[0].is_ascii_alphabetic()
                && candidate[1] == b':'
                && matches!(candidate[2], b'/' | b'\\')
        }) && (cursor == 0 || is_local_path_boundary(bytes[cursor - 1]));
        if !unix_path && !windows_path {
            cursor += 1;
            continue;
        }

        let mut path_end = cursor + if windows_path { 3 } else { 1 };
        while path_end < bytes.len() && !is_local_path_terminator(bytes[path_end]) {
            path_end += 1;
        }
        let path = &value[cursor..path_end];
        output.push_str(&value[copied_until..cursor]);
        output.push_str(&safe_path(path));
        copied_until = path_end;
        cursor = path_end;
    }

    if copied_until == 0 {
        return value.to_string();
    }
    output.push_str(&value[copied_until..]);
    output
}

fn is_local_path_boundary(byte: u8) -> bool {
    byte.is_ascii_whitespace()
        || matches!(
            byte,
            b'"' | b'\'' | b'`' | b'=' | b'(' | b'[' | b'{' | b',' | b';'
        )
}

fn is_local_path_terminator(byte: u8) -> bool {
    byte.is_ascii_whitespace()
        || matches!(
            byte,
            b'"' | b'\'' | b'`' | b'<' | b'>' | b')' | b']' | b'}' | b',' | b';'
        )
}

fn is_uri_scheme_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'+' | b'-' | b'.')
}

fn is_uri_terminator(byte: u8) -> bool {
    byte.is_ascii_whitespace()
        || matches!(
            byte,
            b'"' | b'\'' | b'`' | b'<' | b'>' | b'(' | b')' | b'[' | b']' | b'{' | b'}' | b','
        )
}

fn is_sensitive_key(key: &str) -> bool {
    let normalized = key.to_ascii_lowercase().replace(['_', '-'], "");
    if matches!(
        normalized.as_str(),
        "tokenusage"
            | "inputtokens"
            | "cachedinputtokens"
            | "cachewriteinputtokens"
            | "outputtokens"
            | "reasoningoutputtokens"
            | "totaltokens"
            | "modelcontextwindow"
    ) {
        return false;
    }
    [
        "authorization",
        "cookie",
        "credential",
        "password",
        "secret",
        "token",
        "apikey",
        "stdin",
        "chars",
    ]
    .iter()
    .any(|candidate| normalized.contains(candidate))
}

fn is_path_key(key: &str) -> bool {
    matches!(key, "path" | "cwd" | "savedPath" | "agentPath")
}

fn safe_path(path: &str) -> String {
    let is_absolute =
        path.starts_with('/') || path.starts_with('\\') || path.as_bytes().get(1) == Some(&b':');
    if !is_absolute {
        return path.to_string();
    }
    let name = path
        .rsplit(['/', '\\'])
        .find(|part| !part.is_empty())
        .unwrap_or("path");
    format!("[workspace-path]/{name}")
}

fn string_field(values: &Map<String, Value>, key: &str) -> Option<String> {
    values.get(key).and_then(Value::as_str).map(str::to_string)
}

fn nested_string_field(
    values: &Map<String, Value>,
    object_key: &str,
    field_key: &str,
) -> Option<String> {
    values
        .get(object_key)
        .and_then(Value::as_object)
        .and_then(|nested| string_field(nested, field_key))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn projects_completed_items_to_a_versioned_safe_contract() {
        let frame = br#"data: {"method":"app-server-event","params":{"workspace_id":"workspace-1","message":{"method":"item/completed","params":{"threadId":"thread-1","turnId":"turn-1","item":{"id":"item-1","type":"dynamicToolCall","tool":"write_stdin","status":"completed","arguments":{"session_id":7,"chars":"secret"},"contentItems":[{"type":"inputText","text":"done"}]}}}}}

"#;

        let event = project_frame(frame).unwrap().unwrap();

        assert_eq!(event.event_type, "codex.item.completed");
        assert_eq!(event.thread_id, "thread-1");
        assert_eq!(event.turn_id.as_deref(), Some("turn-1"));
        assert_eq!(event.item_id.as_deref(), Some("item-1"));
        assert_eq!(event.payload["schemaVersion"], 1);
        assert_eq!(event.payload["data"]["arguments"]["chars"], "[redacted]");
        assert_eq!(event.payload["data"]["contentItems"][0]["text"], "done");
        assert!(!event.payload.to_string().contains("secret"));
    }

    #[test]
    fn projects_only_supported_agent_message_phases() {
        let commentary = json!({
            "type": "agentMessage",
            "text": "Inspecting the project.",
            "phase": "commentary"
        });
        let final_answer = json!({
            "type": "agentMessage",
            "text": "The project is ready.",
            "phase": "final_answer"
        });
        let unknown = json!({
            "type": "agentMessage",
            "text": "Provider-specific phase.",
            "phase": "analysis"
        });

        assert_eq!(
            project_item(commentary.as_object().unwrap())["phase"],
            "commentary"
        );
        assert_eq!(
            project_item(final_answer.as_object().unwrap())["phase"],
            "final_answer"
        );
        assert!(project_item(unknown.as_object().unwrap())
            .get("phase")
            .is_none());
    }

    #[test]
    fn projects_only_valid_typed_inline_visualization_artifacts() {
        let item = json!({
            "type": "mcpToolCall",
            "status": "completed",
            "result": {
                "content": [{
                    "type": "text",
                    "text": "Visualization ready"
                }],
                "structuredContent": {
                    "type": "open-web-artifact",
                    "kind": "inline-visualization.v1",
                    "artifact": {
                        "ref": "map-7d67b30d",
                        "renderer": {
                            "kind": "map.v3",
                            "payload": {
                                "title": "Locations",
                                "intent": "visualization",
                                "status": "ready",
                                "summary": "Two locations",
                                "center": [-122.08, 37.42],
                                "zoom": 10,
                                "sources": {
                                    "locations": {
                                    "type": "geojson",
                                    "data": {
                                        "type": "inline",
                                        "format": "geojson",
                                        "geojson": {
                                            "type": "FeatureCollection",
                                            "features": []
                                        }
                                    }
                                    }
                                },
                                "layers": [{
                                    "id": "points",
                                    "source": "locations",
                                    "type": "circle",
                                    "filter": ["==", ["get", "index"], 0],
                                    "paint": {
                                        "circle-color": "#ef4444",
                                        "circle-opacity": 0.8
                                    }
                                }],
                                "extensions": {
                                    "hover": {
                                        "layers": [{
                                            "layer": "points",
                                            "title_property": "label",
                                            "fields": [{
                                                "property": "population",
                                                "label": "Population"
                                            }]
                                        }]
                                    }
                                }
                            }
                        }
                    },
                    "embed": {
                        "syntax": "codex-inline-vis.artifact.v1",
                        "code": "::codex-inline-vis{artifact=\"map-7d67b30d\"}"
                    },
                    "warnings": [{
                        "code": "ignored_extra_input",
                        "path": "layers[0].paint.circle-blur"
                    }]
                }
            }
        });

        let artifact = project_inline_visualization_artifact(item.as_object().unwrap()).unwrap();
        assert_eq!(artifact.artifact_ref, "map-7d67b30d");
        assert_eq!(artifact.renderer_kind, "map.v3");
        assert_eq!(artifact.renderer_payload["title"], "Locations");
        assert_eq!(artifact.renderer_payload["zoom"].as_f64(), Some(10.0));
        assert_eq!(
            artifact.renderer_payload["layers"][0]["paint"]["circle-color"],
            "#ef4444"
        );
        assert_eq!(
            artifact.renderer_payload["extensions"]["hover"]["layers"][0]["fields"][0]["property"],
            "population"
        );

        let projected = project_item(item.as_object().unwrap());
        assert!(projected.get("replyCard").is_none());
        assert!(
            projected["result"]["structuredContent"]["artifact"]["renderer"]
                .get("payload")
                .is_none()
        );
        assert_eq!(
            projected["result"]["structuredContent"]["embed"]["code"],
            "::codex-inline-vis{artifact=\"map-7d67b30d\"}"
        );
    }

    #[test]
    fn projects_raw_mapbox_layers_and_open_web_extensions() {
        let color = json!([
            "interpolate",
            ["linear"],
            ["zoom"],
            4,
            "#e11d48",
            12,
            "#2563eb"
        ]);
        let card = json!({
            "title": "Map",
            "intent": "visualization",
            "status": "ready",
            "sources": {
                "data": {
                    "type": "geojson",
                    "lineMetrics": true,
                    "data": {
                        "type": "inline",
                        "format": "geojson",
                        "geojson": {"type": "FeatureCollection", "features": []}
                    }
                }
            },
            "layers": [{
                "id": "route",
                "type": "line",
                "source": "data",
                "minzoom": 3,
                "layout": {"line-cap": "round"},
                "paint": {"line-color": color, "line-width": 4}
            }],
            "extensions": {
                "hover": {
                    "layers": [{
                        "layer": "route",
                        "title_property": "name",
                        "fields": ["distance"]
                    }]
                },
                "legend": {
                    "items": [{"label": "路线", "color": "#2563eb", "type": "line"}]
                }
            }
        });
        let projected = project_map_card_v3(card.as_object().unwrap()).unwrap();
        assert_eq!(projected["layers"][0]["paint"]["line-color"], color);
        assert_eq!(projected["layers"][0]["minzoom"], 3);
        assert_eq!(projected["sources"]["data"]["lineMetrics"], true);
        assert_eq!(
            projected["extensions"]["hover"]["layers"][0]["fields"][0],
            "distance"
        );
    }

    #[test]
    fn rejects_untyped_cards_removed_renderer_versions_and_mismatched_embeds() {
        let text_only = json!({
            "type": "mcpToolCall",
            "result": {
                "content": [{
                    "type": "text",
                    "text": "{\"type\":\"open-web-artifact\"}"
                }]
            }
        });
        let legacy_card = json!({
            "type": "mcpToolCall",
            "result": {
                "structuredContent": {
                    "type": "open-web-card",
                    "kind": "map.removed",
                    "card": {}
                }
            }
        });
        let mismatched_embed = json!({
            "type": "mcpToolCall",
            "result": {
                "structuredContent": {
                    "type": "open-web-artifact",
                    "kind": "inline-visualization.v1",
                    "artifact": {
                        "ref": "map-one",
                        "renderer": {
                            "kind": "map.v3",
                            "payload": {
                                "title": "Map",
                                "intent": "visualization",
                                "status": "ready",
                                "sources": {
                                    "data": {
                                    "type": "geojson",
                                    "data": {
                                        "type": "inline",
                                        "format": "geojson",
                                        "geojson": {
                                            "type": "FeatureCollection",
                                            "features": []
                                        }
                                    }
                                    }
                                },
                                "layers": [{
                                    "id": "points",
                                    "source": "data",
                                    "type": "circle",
                                    "paint": {}
                                }]
                            }
                        }
                    },
                    "embed": {
                        "syntax": "codex-inline-vis.artifact.v1",
                        "code": "::codex-inline-vis{artifact=\"map-two\"}"
                    }
                }
            }
        });

        assert!(project_inline_visualization_artifact(text_only.as_object().unwrap()).is_none());
        assert!(project_inline_visualization_artifact(legacy_card.as_object().unwrap()).is_none());
        assert!(
            project_inline_visualization_artifact(mismatched_embed.as_object().unwrap()).is_none()
        );
    }

    #[test]
    fn does_not_apply_a_map_specific_inline_byte_limit() {
        let item = json!({
            "type": "mcpToolCall",
            "result": {
                "structuredContent": {
                    "type": "open-web-artifact",
                    "kind": "inline-visualization.v1",
                    "artifact": {
                        "ref": "map-large",
                        "renderer": {
                            "kind": "map.v3",
                            "payload": {
                                "title": "Large inline source",
                                "intent": "visualization",
                                "status": "ready",
                                "summary": "x".repeat(32 * 1024),
                                "sources": {
                                    "data": {
                                    "type": "geojson",
                                    "data": {
                                        "type": "inline",
                                        "format": "geojson",
                                        "geojson": {
                                            "type": "FeatureCollection",
                                            "features": []
                                        }
                                    }
                                    }
                                },
                                "layers": [{
                                    "id": "points",
                                    "source": "data",
                                    "type": "circle",
                                    "paint": {}
                                }]
                            }
                        }
                    },
                    "embed": {
                        "syntax": "codex-inline-vis.artifact.v1",
                        "code": "::codex-inline-vis{artifact=\"map-large\"}"
                    }
                }
            }
        });

        assert!(project_inline_visualization_artifact(item.as_object().unwrap()).is_some());
    }

    #[test]
    fn replaces_mcp_resource_refs_with_opaque_authorized_artifact_urls() {
        let artifact_id = Uuid::parse_str("8e98ff2f-82ee-4cc9-a3e6-2974debf8666").unwrap();
        let resource_uri = "maps-data://geojson/map-data-one";
        let mut map_payload = json!({
            "sources": {
                "locations": {
                "type": "geojson",
                "data": {
                    "type": "mcp_resource",
                    "server": "map_utils",
                    "uri": resource_uri,
                    "format": "geojson"
                }
                }
            }
        });
        let resolved = std::collections::HashMap::from([(
            ("map_utils".to_string(), resource_uri.to_string()),
            (artifact_id, Some("application/geo+json".to_string())),
        )]);

        replace_map_payload_resource_refs(&mut map_payload, &resolved);

        assert_eq!(
            map_payload["sources"]["locations"]["data"],
            json!({
                "type": "artifact",
                "format": "geojson",
                "artifact_id": artifact_id,
                "mime_type": "application/geo+json",
                "url": format!("/api/artifacts/{artifact_id}/content")
            })
        );
        assert!(!map_payload.to_string().contains(resource_uri));
    }

    #[test]
    fn extracts_only_standalone_artifact_directives_outside_code_blocks() {
        let markdown = r#"Before
::codex-inline-vis{artifact="map-one"}
```text
::codex-inline-vis{artifact="map-code"}
```
    ::codex-inline-vis{artifact="map-indented"}
::codex-inline-vis{file="chart.html"}
::codex-inline-vis{artifact="map-one"}
::codex-inline-vis{artifact="map-two"}
After"#;
        assert_eq!(
            inline_artifact_refs(markdown),
            vec!["map-one".to_string(), "map-two".to_string()]
        );
    }

    #[test]
    fn accepts_only_typed_local_mcp_resource_links() {
        let link = json!({
            "type": "resource_link",
            "name": "map-data-one",
            "title": "Maps GeoJSON",
            "uri": "maps-data://geojson/map-data-one",
            "mimeType": "application/geo+json",
            "size": 128
        });
        let projected = artifact_link(&link).expect("valid link");
        assert_eq!(projected.uri, "maps-data://geojson/map-data-one");
        assert_eq!(projected.artifact_schema, "geojson.v1");
        assert_eq!(projected.mime_type, "application/geo+json");
        assert_eq!(projected.expected_size, Some(128));

        let planning = artifact_link(&json!({
            "type": "resource_link",
            "name": "planning-dataset.v1-digest",
            "title": "planning-dataset.v1",
            "uri": "supply-chain-data://resources/planning-dataset.v1-digest",
            "mimeType": "application/json",
            "size": 512
        }))
        .expect("planning Resource");
        assert_eq!(planning.artifact_schema, "planning-dataset.v1");

        let invalid_uri = json!({
            "type": "resource_link",
            "name": "map-data-one",
            "uri": "https://example.com/map-data-one",
            "mimeType": "application/geo+json"
        });
        assert!(artifact_link(&invalid_uri).is_none());

        let item = json!({
            "type": "mcpToolCall",
            "server": "map_utils",
            "tool": "batch_geocode",
            "result": {
                "content": [link],
                "structuredContent": {
                    "provider": "mapbox",
                    "summary": "Geocoded one address.",
                    "feature_count": 1,
                    "data_ref": {
                        "type": "mcp_resource",
                        "server": "map_utils",
                        "uri": "maps-data://geojson/map-data-one",
                        "format": "geojson"
                    }
                }
            }
        });
        let item = item.as_object().unwrap();
        let artifacts = artifact_candidates(item).collect::<Vec<_>>();
        assert_eq!(artifacts.len(), 1);
        assert_eq!(artifacts[0].uri, "maps-data://geojson/map-data-one");

        let public = project_item(item);
        let public_link = public.pointer("/result/content/0").unwrap();
        assert!(public_link.get("uri").is_none());
        assert!(public_link.get("_meta").is_none());
        assert!(public
            .pointer("/result/structuredContent/data_ref")
            .is_none());

        let model_visible_namespace = json!({
            "locations": {
                "type": "geojson",
                "data": {
                "type": "mcp_resource",
                "server": "mcp__map_utils",
                "uri": "maps-data://geojson/map-data-one",
                "format": "geojson"
            }
            }
        });
        assert!(project_map_v3_sources(&model_visible_namespace).is_none());
    }

    #[test]
    fn removes_internal_resource_uris_from_browser_text_and_arguments() {
        let item = json!({
            "type": "agentMessage",
            "text": "Use supply-chain-data://resources/dataset-one, then see https://example.com/run."
        });
        let projected = project_item(item.as_object().unwrap());
        assert_eq!(
            projected["text"],
            "Use [internal-resource-uri], then see https://example.com/run."
        );

        let arguments = sanitize_value(
            &json!({
                "server": "supply_chain_data",
                "uri": "supply-chain-data://resources/dataset-one"
            }),
            "arguments",
        );
        assert_eq!(arguments["server"], "supply_chain_data");
        assert_eq!(arguments["uri"], "[internal-resource-uri]");
    }

    #[test]
    fn removes_local_paths_embedded_in_browser_text_and_commands() {
        let projected = project_item(
            json!({
                "type": "commandExecution",
                "command": "/bin/zsh -lc \"sed -n '1,20p' /Users/example/project/skills/demo/SKILL.md\"",
                "aggregatedOutput": "loaded cwd=/private/tmp/profile/resources and https://example.com/run/1",
                "commandActions": [{
                    "type": "read",
                    "path": "/Users/example/project/skills/demo/SKILL.md",
                    "command": "sed -n '1,20p' /Users/example/project/skills/demo/SKILL.md"
                }]
            })
            .as_object()
            .unwrap(),
        );

        let encoded = projected.to_string();
        assert!(!encoded.contains("/Users/example"));
        assert!(!encoded.contains("/private/tmp/profile"));
        assert!(encoded.contains("[workspace-path]/SKILL.md"));
        assert!(encoded.contains("[workspace-path]/resources"));
        assert!(encoded.contains("https://example.com/run/1"));
    }

    #[test]
    fn keeps_unknown_notifications_without_exposing_arbitrary_params() {
        let frame = br#"data: {"method":"app-server-event","params":{"message":{"method":"item/futureEvent","params":{"threadId":"thread-1","turnId":"turn-1","itemId":"item-1","credential":"secret","payload":{"local":"value"}}}}}

"#;

        let event = project_frame(frame).unwrap().unwrap();

        assert_eq!(event.event_type, "codex.unknown");
        assert_eq!(event.payload["data"]["sourceType"], "item/futureEvent");
        assert!(event.payload["data"].get("credential").is_none());
        assert!(event.payload["data"].get("payload").is_none());
    }

    #[test]
    fn keeps_webapp_runtime_state_and_error_details_with_secret_redaction() {
        let status = br#"data: {"method":"app-server-event","params":{"message":{"method":"thread/status/changed","params":{"threadId":"thread-1","status":{"type":"active","activeFlags":["waiting"]}}}}}

"#;
        let status = project_frame(status).unwrap().unwrap();
        assert_eq!(status.event_type, "codex.thread.status.changed");
        assert_eq!(
            status.payload["data"]["sourceType"],
            "thread/status/changed"
        );
        assert_eq!(status.payload["data"]["status"]["type"], "active");
        assert_eq!(
            status
                .thread_metadata
                .as_ref()
                .and_then(|metadata| metadata.status_type.as_deref()),
            Some("active")
        );

        let error = br#"data: {"method":"app-server-event","params":{"message":{"method":"error","params":{"threadId":"thread-1","error":{"message":"stream disconnected","additionalDetails":"retrying sampling request 1/3","apiKey":"must-not-leak"}}}}}

"#;
        let error = project_frame(error).unwrap().unwrap();
        assert_eq!(error.payload["data"]["sourceType"], "error");
        assert_eq!(
            error.payload["data"]["error"]["message"],
            "stream disconnected"
        );
        assert_eq!(error.payload["data"]["error"]["apiKey"], "[redacted]");
        assert!(!error.payload.to_string().contains("must-not-leak"));
    }

    #[test]
    fn projects_platform_approvals_without_runtime_request_ids() {
        let frame = br#"data: {"method":"app-server-event","params":{"message":{"method":"platform/approvalRequested","params":{"approvalId":"018f-id","threadId":"thread-1","turnId":"turn-1","itemId":"item-1"}}}}

"#;
        let event = project_frame(frame).unwrap().unwrap();
        assert_eq!(event.event_type, "platform.approval.requested");
        assert_eq!(event.turn_id.as_deref(), Some("turn-1"));
        assert_eq!(event.item_id.as_deref(), Some("item-1"));
        assert_eq!(event.payload["data"]["approvalId"], "018f-id");
        assert!(!event.payload.to_string().contains("requestId"));
    }

    #[test]
    fn projects_only_platform_ids_for_resolved_approvals() {
        let approval_id = Uuid::now_v7();
        let safe = format!(
            "data: {{\"method\":\"app-server-event\",\"params\":{{\"message\":{{\"method\":\"serverRequest/resolved\",\"params\":{{\"threadId\":\"thread-1\",\"turnId\":\"turn-1\",\"itemId\":\"item-1\",\"requestId\":\"{approval_id}\"}}}}}}}}\n\n"
        );
        let event = project_frame(safe.as_bytes()).unwrap().unwrap();
        assert_eq!(event.event_type, "platform.approval.resolved");
        assert_eq!(event.turn_id.as_deref(), Some("turn-1"));
        assert_eq!(event.item_id.as_deref(), Some("item-1"));
        assert_eq!(event.payload["data"]["requestId"], approval_id.to_string());

        let unsafe_frame = br#"data: {"method":"app-server-event","params":{"message":{"method":"serverRequest/resolved","params":{"threadId":"thread-1","requestId":77}}}}

"#;
        let event = project_frame(unsafe_frame).unwrap().unwrap();
        assert!(event.payload["data"].get("requestId").is_none());
    }

    #[test]
    fn hides_absolute_server_paths_but_keeps_relative_workspace_paths() {
        assert_eq!(
            safe_path("/srv/workspaces/project/src/lib.rs"),
            "[workspace-path]/lib.rs"
        );
        assert_eq!(safe_path("src/lib.rs"), "src/lib.rs");
    }

    #[test]
    fn ignores_sse_keepalive_frames() {
        assert_eq!(project_frame(b": keepalive\n\n").unwrap(), None);
    }

    #[test]
    fn projects_thread_archive_and_name_notifications() {
        let archived = br#"data: {"method":"app-server-event","params":{"message":{"method":"thread/archived","params":{"threadId":"thread-1"}}}}

"#;
        let archived = project_frame(archived).unwrap().unwrap();
        assert_eq!(archived.event_type, "codex.thread.archived");

        let renamed = br#"data: {"method":"app-server-event","params":{"message":{"method":"thread/name/updated","params":{"threadId":"thread-1","threadName":"Durable name"}}}}

"#;
        let renamed = project_frame(renamed).unwrap().unwrap();
        assert_eq!(renamed.event_type, "codex.thread.name.updated");
        assert_eq!(renamed.payload["data"]["threadName"], "Durable name");
    }

    #[test]
    fn projects_runtime_child_thread_identity_from_the_official_thread_shape() {
        let workspace_id = Uuid::now_v7();
        let frame = format!(
            r#"data: {{"method":"app-server-event","params":{{"workspace_id":"{workspace_id}","message":{{"method":"thread/started","params":{{"thread":{{"id":"child-thread","parentThreadId":"root-thread","source":{{"subAgent":{{"thread_spawn":{{"parent_thread_id":"root-thread","depth":1,"agent_path":"/root/network","agent_nickname":"Network","agent_role":"network_planning_agent"}}}}}},"status":{{"type":"idle","activeFlags":[]}}}}}}}}}}}}

"#
        );
        let event = project_frame(frame.as_bytes()).unwrap().unwrap();

        assert_eq!(event.event_type, "codex.thread.started");
        assert_eq!(event.workspace_id, Some(workspace_id));
        assert_eq!(event.thread_id, "child-thread");
        let metadata = event.thread_metadata.unwrap();
        assert_eq!(metadata.parent_thread_id.as_deref(), Some("root-thread"));
        assert_eq!(metadata.source_kind.as_deref(), Some("thread_spawn"));
        assert_eq!(metadata.agent_path.as_deref(), Some("/root/network"));
        assert_eq!(metadata.agent_nickname.as_deref(), Some("Network"));
        assert_eq!(
            metadata.agent_role.as_deref(),
            Some("network_planning_agent")
        );
        assert_eq!(metadata.status_type.as_deref(), Some("idle"));
    }

    #[tokio::test]
    #[ignore = "requires TEST_DATABASE_URL pointing at a disposable PostgreSQL database"]
    async fn child_thread_events_remain_under_the_root_run_without_owning_its_lifecycle() {
        let database_url = std::env::var("TEST_DATABASE_URL").expect("TEST_DATABASE_URL");
        let pool = sqlx::postgres::PgPoolOptions::new()
            .max_connections(4)
            .connect(&database_url)
            .await
            .expect("connect disposable PostgreSQL database");
        open_web_codex_platform_store::migrate::run(&pool)
            .await
            .expect("migrate database");

        let organization_id = Uuid::now_v7();
        let user_id = Uuid::now_v7();
        let profile_id = Uuid::now_v7();
        let project_id = Uuid::now_v7();
        let task_id = Uuid::now_v7();
        let workspace_id = Uuid::now_v7();
        let run_id = Uuid::now_v7();
        sqlx::query("INSERT INTO organizations (id, name, slug) VALUES ($1, 'Projection', $2)")
            .bind(organization_id)
            .bind(format!("projection-{organization_id}"))
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO users (id, username, name, email, password_hash, role)
             VALUES ($1, $2, 'Projection', $3, 'test-only', 'owner')",
        )
        .bind(user_id)
        .bind(format!("projection-{user_id}"))
        .bind(format!("{user_id}@example.invalid"))
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO profiles (id, organization_id, owner_user_id, runtime_key, name)
             VALUES ($1, $2, $3, $4, 'Projection Profile')",
        )
        .bind(profile_id)
        .bind(organization_id)
        .bind(user_id)
        .bind(format!("projection-{profile_id}"))
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO projects (
                id, organization_id, created_by, name, git_url, default_branch
             ) VALUES ($1, $2, $3, 'Projection Project', '/tmp/projection.git', 'main')",
        )
        .bind(project_id)
        .bind(organization_id)
        .bind(user_id)
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO tasks (
                id, organization_id, project_id, created_by, title, status
             ) VALUES ($1, $2, $3, $4, 'Projection Task', 'running')",
        )
        .bind(task_id)
        .bind(organization_id)
        .bind(project_id)
        .bind(user_id)
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO workspaces (
                id, organization_id, project_id, profile_id, created_by, kind, name,
                root_path, source_ref, state
             ) VALUES ($1, $2, $3, $4, $5, 'main', 'Projection Workspace',
                       $6, 'main', 'ready')",
        )
        .bind(workspace_id)
        .bind(organization_id)
        .bind(project_id)
        .bind(profile_id)
        .bind(user_id)
        .bind(format!("/tmp/projection-{workspace_id}"))
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO runs (
                id, organization_id, task_id, requested_by, requested_profile_id,
                workspace_id, status, codex_thread_id
             ) VALUES ($1, $2, $3, $4, $5, $6, 'running', 'root-thread')",
        )
        .bind(run_id)
        .bind(organization_id)
        .bind(task_id)
        .bind(user_id)
        .bind(profile_id)
        .bind(workspace_id)
        .execute(&pool)
        .await
        .unwrap();
        let policy_snapshot_id = sqlx::query_scalar::<_, Uuid>(
            "INSERT INTO supervisor_policy_snapshots (
                organization_id, policy_id, version, display_name,
                developer_instructions, content_sha256
             ) VALUES ($1, 'projection-supervisor', '1.0.0', 'Projection Supervisor',
                       'Coordinate the projected child agents.', $2)
             RETURNING id",
        )
        .bind(organization_id)
        .bind("0".repeat(64))
        .fetch_one(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO supervisor_policy_bindings (
                organization_id, profile_id, task_id, run_id, snapshot_id,
                thread_id, state, bound_at
             ) VALUES ($1, $2, $3, $4, $5, 'root-thread', 'bound', now())",
        )
        .bind(organization_id)
        .bind(profile_id)
        .bind(task_id)
        .bind(run_id)
        .bind(policy_snapshot_id)
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO runtime_agent_projections (
                organization_id, profile_id, workspace_id, root_run_id, thread_id,
                source_kind
             ) VALUES ($1, $2, $3, $4, 'root-thread', 'root')",
        )
        .bind(organization_id)
        .bind(profile_id)
        .bind(workspace_id)
        .bind(run_id)
        .execute(&pool)
        .await
        .unwrap();

        let assignment = format!(
            r#"data: {{"method":"app-server-event","params":{{"workspace_id":"{workspace_id}","message":{{"method":"item/completed","params":{{"threadId":"root-thread","turnId":"root-turn","item":{{"id":"spawn-network","type":"collabAgentToolCall","tool":"spawnAgent","status":"completed","prompt":"Build and validate the network plan.","senderThreadId":"root-thread","receiverThreadIds":["child-thread"],"agentsStates":{{}}}}}}}}}}}}

"#
        );
        let started = format!(
            r#"data: {{"method":"app-server-event","params":{{"workspace_id":"{workspace_id}","message":{{"method":"thread/started","params":{{"thread":{{"id":"child-thread","parentThreadId":"root-thread","source":{{"subAgent":{{"thread_spawn":{{"parent_thread_id":"root-thread","depth":1,"agent_path":"/root/network","agent_nickname":"Network","agent_role":"network_planning_agent"}}}}}},"status":{{"type":"idle","activeFlags":[]}}}}}}}}}}}}

"#
        );
        let child_turn = format!(
            r#"data: {{"method":"app-server-event","params":{{"workspace_id":"{workspace_id}","message":{{"method":"turn/started","params":{{"threadId":"child-thread","turnId":"child-turn","turn":{{"id":"child-turn","status":"inProgress"}}}}}}}}}}

"#
        );
        let data_started = format!(
            r#"data: {{"method":"app-server-event","params":{{"workspace_id":"{workspace_id}","message":{{"method":"thread/started","params":{{"thread":{{"id":"data-thread","parentThreadId":"root-thread","source":{{"subAgent":{{"thread_spawn":{{"parent_thread_id":"root-thread","depth":1,"agent_path":"/root/data","agent_nickname":"Data","agent_role":"data_agent"}}}}}},"status":{{"type":"idle","activeFlags":[]}}}}}}}}}}}}

"#
        );
        let data_turn = format!(
            r#"data: {{"method":"app-server-event","params":{{"workspace_id":"{workspace_id}","message":{{"method":"turn/started","params":{{"threadId":"data-thread","turnId":"data-turn","turn":{{"id":"data-turn","status":"inProgress"}}}}}}}}}}

"#
        );
        let data_assignment = format!(
            r#"data: {{"method":"app-server-event","params":{{"workspace_id":"{workspace_id}","message":{{"method":"item/completed","params":{{"threadId":"root-thread","turnId":"root-turn","item":{{"id":"spawn-data","type":"collabAgentToolCall","tool":"spawnAgent","status":"completed","prompt":"Validate the planning inputs.","senderThreadId":"root-thread","receiverThreadIds":["data-thread"],"agentsStates":{{}}}}}}}}}}}}

"#
        );
        let data_turn_completed = format!(
            r#"data: {{"method":"app-server-event","params":{{"workspace_id":"{workspace_id}","message":{{"method":"turn/completed","params":{{"threadId":"data-thread","turnId":"data-turn","turn":{{"id":"data-turn","status":"completed"}}}}}}}}}}

"#
        );
        let data_completed = format!(
            r#"data: {{"method":"app-server-event","params":{{"workspace_id":"{workspace_id}","message":{{"method":"thread/completed","params":{{"threadId":"data-thread"}}}}}}}}

"#
        );
        let child_artifact = format!(
            r#"data: {{"method":"app-server-event","params":{{"workspace_id":"{workspace_id}","message":{{"method":"item/completed","params":{{"threadId":"child-thread","turnId":"child-turn","item":{{"id":"data-item","type":"mcpToolCall","server":"supply_chain_data","tool":"build_planning_dataset","result":{{"content":[{{"type":"resource_link","name":"planning-dataset.v1-digest","title":"planning-dataset.v1","uri":"supply-chain-data://resources/planning-dataset.v1-digest","mimeType":"application/json","size":512}}],"structuredContent":{{"summary":"ready","data_ref":{{"server":"supply_chain_data","uri":"supply-chain-data://resources/planning-dataset.v1-digest","resource_schema":"planning-dataset.v1"}}}}}}}}}}}}}}}}

"#
        );
        let child_turn_completed = format!(
            r#"data: {{"method":"app-server-event","params":{{"workspace_id":"{workspace_id}","message":{{"method":"turn/completed","params":{{"threadId":"child-thread","turnId":"child-turn","turn":{{"id":"child-turn","status":"completed"}}}}}}}}}}

"#
        );
        let followup = format!(
            r#"data: {{"method":"app-server-event","params":{{"workspace_id":"{workspace_id}","message":{{"method":"item/completed","params":{{"threadId":"root-thread","turnId":"root-turn","item":{{"id":"followup-network","type":"collabAgentToolCall","tool":"sendInput","status":"completed","prompt":"Compare the feasible network scenarios.","senderThreadId":"root-thread","receiverThreadIds":["child-thread"],"agentsStates":{{}}}}}}}}}}}}

"#
        );
        let second_turn = format!(
            r#"data: {{"method":"app-server-event","params":{{"workspace_id":"{workspace_id}","message":{{"method":"turn/started","params":{{"threadId":"child-thread","turnId":"child-turn-2","turn":{{"id":"child-turn-2","status":"inProgress"}}}}}}}}}}

"#
        );
        let second_tool = format!(
            r#"data: {{"method":"app-server-event","params":{{"workspace_id":"{workspace_id}","message":{{"method":"item/started","params":{{"threadId":"child-thread","turnId":"child-turn-2","item":{{"id":"compare-item","type":"mcpToolCall","server":"supply_chain_planner","tool":"compare_network_scenarios","status":"inProgress"}}}}}}}}}}

"#
        );
        let completed = format!(
            r#"data: {{"method":"app-server-event","params":{{"workspace_id":"{workspace_id}","message":{{"method":"thread/completed","params":{{"threadId":"child-thread"}}}}}}}}

"#
        );
        let root_turn_completed = format!(
            r#"data: {{"method":"app-server-event","params":{{"workspace_id":"{workspace_id}","message":{{"method":"turn/completed","params":{{"threadId":"root-thread","turnId":"root-turn","turn":{{"id":"root-turn","status":"completed"}}}}}}}}}}

"#
        );
        assert!(persist_frame(assignment.as_bytes(), &pool)
            .await
            .unwrap()
            .is_some());
        assert!(persist_frame(started.as_bytes(), &pool)
            .await
            .unwrap()
            .is_some());
        assert!(persist_frame(child_turn.as_bytes(), &pool)
            .await
            .unwrap()
            .is_some());
        assert!(persist_frame(data_started.as_bytes(), &pool)
            .await
            .unwrap()
            .is_some());
        assert!(persist_frame(data_turn.as_bytes(), &pool)
            .await
            .unwrap()
            .is_some());
        assert!(persist_frame(data_assignment.as_bytes(), &pool)
            .await
            .unwrap()
            .is_some());
        let artifact_projection = persist_frame(child_artifact.as_bytes(), &pool)
            .await
            .unwrap()
            .expect("Artifact projection");
        assert_eq!(artifact_projection.pending_artifact_ids.len(), 1);
        assert!(persist_frame(data_turn_completed.as_bytes(), &pool)
            .await
            .unwrap()
            .is_some());
        assert!(persist_frame(data_completed.as_bytes(), &pool)
            .await
            .unwrap()
            .is_some());
        assert!(persist_frame(child_turn_completed.as_bytes(), &pool)
            .await
            .unwrap()
            .is_some());
        assert!(persist_frame(followup.as_bytes(), &pool)
            .await
            .unwrap()
            .is_some());
        assert!(persist_frame(second_turn.as_bytes(), &pool)
            .await
            .unwrap()
            .is_some());
        assert!(persist_frame(second_tool.as_bytes(), &pool)
            .await
            .unwrap()
            .is_some());
        assert!(persist_frame(completed.as_bytes(), &pool)
            .await
            .unwrap()
            .is_some());

        let run = sqlx::query("SELECT status, active_turn_id FROM runs WHERE id = $1")
            .bind(run_id)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(run.get::<String, _>("status"), "running");
        assert!(run.get::<Option<String>, _>("active_turn_id").is_none());
        let child = sqlx::query(
            "SELECT root_run_id, parent_thread_id, agent_role, status_type
             FROM runtime_agent_projections
             WHERE profile_id = $1 AND thread_id = 'child-thread'",
        )
        .bind(profile_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(child.get::<Uuid, _>("root_run_id"), run_id);
        assert_eq!(
            child
                .get::<Option<String>, _>("parent_thread_id")
                .as_deref(),
            Some("root-thread")
        );
        assert_eq!(
            child.get::<Option<String>, _>("agent_role").as_deref(),
            Some("network_planning_agent")
        );
        assert_eq!(
            child.get::<Option<String>, _>("status_type").as_deref(),
            Some("completed")
        );
        let event_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM run_events
             WHERE run_id = $1 AND thread_id = 'child-thread'",
        )
        .bind(run_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(event_count, 7);
        let executions = sqlx::query(
            "SELECT turn_id, ordinal, task, status, current_behavior
             FROM runtime_agent_execution_projections
             WHERE root_run_id = $1 AND agent_thread_id = 'child-thread'
             ORDER BY ordinal",
        )
        .bind(run_id)
        .fetch_all(&pool)
        .await
        .unwrap();
        assert_eq!(executions.len(), 2);
        assert_eq!(
            executions[0].get::<Option<String>, _>("turn_id").as_deref(),
            Some("child-turn")
        );
        assert_eq!(executions[0].get::<i32, _>("ordinal"), 1);
        assert_eq!(
            executions[0].get::<String, _>("task"),
            "Build and validate the network plan."
        );
        assert_eq!(executions[0].get::<String, _>("status"), "completed");
        assert_eq!(
            executions[0].get::<String, _>("current_behavior"),
            "Finished this work cycle"
        );
        assert_eq!(
            executions[1].get::<Option<String>, _>("turn_id").as_deref(),
            Some("child-turn-2")
        );
        assert_eq!(executions[1].get::<i32, _>("ordinal"), 2);
        assert_eq!(
            executions[1].get::<String, _>("task"),
            "Compare the feasible network scenarios."
        );
        assert_eq!(executions[1].get::<String, _>("status"), "completed");
        let data_execution = sqlx::query(
            "SELECT turn_id, ordinal, task, status
             FROM runtime_agent_execution_projections
             WHERE root_run_id = $1 AND agent_thread_id = 'data-thread'",
        )
        .bind(run_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(
            data_execution
                .get::<Option<String>, _>("turn_id")
                .as_deref(),
            Some("data-turn")
        );
        assert_eq!(data_execution.get::<i32, _>("ordinal"), 1);
        assert_eq!(
            data_execution.get::<String, _>("task"),
            "Validate the planning inputs."
        );
        assert_eq!(data_execution.get::<String, _>("status"), "completed");
        let artifact = sqlx::query(
            "SELECT artifact.id, artifact.artifact_schema, artifact.state,
                    artifact_grant.task_id, provenance.producer_thread_id
             FROM artifacts artifact
             JOIN artifact_task_grants artifact_grant
               ON artifact_grant.artifact_id = artifact.id
             JOIN artifact_provenance provenance
               ON provenance.artifact_id = artifact.id
             WHERE artifact.organization_id = $1",
        )
        .bind(organization_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(
            artifact.get::<String, _>("artifact_schema"),
            "planning-dataset.v1"
        );
        assert_eq!(artifact.get::<String, _>("state"), "pending");
        assert_eq!(artifact.get::<Uuid, _>("task_id"), task_id);
        assert_eq!(
            artifact.get::<String, _>("producer_thread_id"),
            "child-thread"
        );
        sqlx::query("UPDATE runs SET active_turn_id = 'root-turn' WHERE id = $1")
            .bind(run_id)
            .execute(&pool)
            .await
            .unwrap();
        assert!(persist_frame(root_turn_completed.as_bytes(), &pool)
            .await
            .unwrap()
            .is_some());
        let run = sqlx::query("SELECT status, active_turn_id FROM runs WHERE id = $1")
            .bind(run_id)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(run.get::<String, _>("status"), "completed");
        assert!(run.get::<Option<String>, _>("active_turn_id").is_none());
        let task_status: String = sqlx::query_scalar("SELECT status FROM tasks WHERE id = $1")
            .bind(task_id)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(task_status, "completed");
    }

    #[test]
    fn projects_token_usage_without_treating_counts_as_credentials() {
        let frame = br#"data: {"method":"app-server-event","params":{"message":{"method":"thread/tokenUsage/updated","params":{"threadId":"thread-1","turnId":"turn-1","tokenUsage":{"total":{"totalTokens":150,"inputTokens":100,"cachedInputTokens":25,"outputTokens":50,"reasoningOutputTokens":0},"last":{"totalTokens":150,"inputTokens":100,"cachedInputTokens":25,"outputTokens":50,"reasoningOutputTokens":0},"modelContextWindow":200000}}}}}

"#;
        let event = project_frame(frame).unwrap().unwrap();
        assert_eq!(event.event_type, "codex.thread.token_usage.updated");
        assert_eq!(
            event.payload["data"]["tokenUsage"]["total"]["totalTokens"],
            150
        );
        assert!(!event.payload.to_string().contains("[redacted]"));
    }
}
use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;
