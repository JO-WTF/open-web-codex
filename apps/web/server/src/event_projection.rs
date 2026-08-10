use open_web_codex_platform_contracts::{RunEvent, RuntimeAgentActivitySubject};
use serde_json::{json, Map, Value};
use sqlx::PgPool;
use sqlx::Row;
use uuid::Uuid;

use crate::final_artifacts::{
    artifact_delivery_failure, artifact_delivery_projection, final_artifact_candidate,
    FinalArtifactCandidate,
};
use crate::inline_maps::{self, InlineMapCandidate};

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
    artifacts: Vec<FinalArtifactCandidate>,
    inline_map: Option<InlineMapCandidate>,
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
}

struct RegisteredArtifact {
    id: Uuid,
    artifact_schema: String,
    display_name: String,
    mime_type: String,
    expected_size: Option<i64>,
    byte_size: Option<i64>,
    state: String,
    failure_code: Option<String>,
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
        project_registered_artifacts(&mut event.payload, &registered)?;
        if let (Some(candidate), Some(turn_id), Some(item_id)) = (
            event.inline_map.as_ref(),
            event.turn_id.as_deref(),
            event.item_id.as_deref(),
        ) {
            inline_maps::register(
                &mut transaction,
                context.organization_id,
                context.run_id,
                &event.thread_id,
                turn_id,
                item_id,
                candidate,
            )
            .await?;
        }
        inline_maps::resolve_in_transaction(&mut transaction, run_id, &mut event.payload).await?;
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
            mark_artifact_delivery_failure(&mut event.payload, "artifact_projection_failed");
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
    project_provider_call_metric(&mut transaction, &context, &event, event_sequence).await?;
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

/// Persist only provider-reported, bounded usage metadata. This function is
/// intentionally a no-op when the Runtime did not emit token usage; a missing
/// field is different from a reported zero and remains observable as NULL.
async fn project_provider_call_metric(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    context: &EventRunContext,
    event: &ProjectedEvent,
    event_sequence: i64,
) -> Result<(), String> {
    if event.event_type != "codex.thread.token_usage.updated" {
        return Ok(());
    }
    let usage = event
        .payload
        .pointer("/data/tokenUsage/last")
        .or_else(|| event.payload.pointer("/data/tokenUsage/total"));
    let Some(usage) = usage else {
        return Ok(());
    };
    let task = sqlx::query(
        "SELECT COALESCE(model_provider, 'unreported') AS provider_id,
                COALESCE(model, 'unreported') AS model_id
         FROM tasks WHERE id = $1 AND organization_id = $2",
    )
    .bind(context.task_id)
    .bind(context.organization_id)
    .fetch_optional(&mut **transaction)
    .await
    .map_err(|error| format!("provider metric task lookup error: {error}"))?;
    let Some(task) = task else {
        return Ok(());
    };
    let input_tokens = token_number(usage, &["inputTokens", "input_tokens"]);
    let cached_input_tokens = token_number(
        usage,
        &[
            "cachedInputTokens",
            "cached_input_tokens",
            "cacheReadInputTokens",
        ],
    );
    let output_tokens = token_number(usage, &["outputTokens", "output_tokens"]);
    let tool_schema_tokens = token_number(usage, &["toolSchemaTokens", "tool_schema_tokens"]);
    let compaction_count = token_number(
        event.payload.pointer("/data").unwrap_or(&Value::Null),
        &["compactionCount", "compaction_count"],
    )
    .unwrap_or(0)
    .clamp(0, i64::from(i32::MAX)) as i32;
    let latency_ms = elapsed_ms(event.payload.pointer("/data").unwrap_or(&Value::Null));
    sqlx::query(
        "INSERT INTO provider_call_metrics
         (organization_id, profile_id, run_id, provider_id, model_id,
          input_tokens, cached_input_tokens, output_tokens, tool_schema_tokens,
          latency_ms, first_token_ms, compaction_count, terminal_status,
          source_event_sequence)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12,
                 'observed', $13)
         ON CONFLICT (run_id, source_event_sequence) DO NOTHING",
    )
    .bind(context.organization_id)
    .bind(context.profile_id)
    .bind(context.run_id)
    .bind(task.get::<String, _>("provider_id"))
    .bind(task.get::<String, _>("model_id"))
    .bind(input_tokens)
    .bind(cached_input_tokens)
    .bind(output_tokens)
    .bind(tool_schema_tokens)
    .bind(latency_ms)
    .bind(first_token_ms(
        event.payload.pointer("/data").unwrap_or(&Value::Null),
    ))
    .bind(compaction_count)
    .bind(event_sequence)
    .execute(&mut **transaction)
    .await
    .map_err(|error| format!("provider metric projection error: {error}"))?;
    Ok(())
}

fn token_number(value: &Value, keys: &[&str]) -> Option<i64> {
    keys.iter()
        .find_map(|key| value.get(*key))
        .and_then(|value| {
            value
                .as_i64()
                .or_else(|| value.as_u64().and_then(|value| i64::try_from(value).ok()))
        })
}

fn elapsed_ms(value: &Value) -> Option<i64> {
    let started = token_number(value, &["startedAtMs", "started_at_ms"])?;
    let completed = token_number(value, &["completedAtMs", "completed_at_ms"])?;
    (completed >= started).then_some(completed - started)
}

fn first_token_ms(value: &Value) -> Option<i64> {
    token_number(value, &["firstTokenMs", "first_token_ms"])
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
    let (artifacts, artifact_delivery_error, final_delivery_seen) = match item
        .map(final_artifact_candidate)
    {
        Some(Ok(Some(artifact))) if event_type == "codex.item.completed" => {
            (vec![artifact], None, true)
        }
        Some(Err(code)) if event_type == "codex.item.completed" => (Vec::new(), Some(code), true),
        _ => (Vec::new(), None, false),
    };
    let inline_map = if event_type == "codex.item.completed" {
        item.and_then(inline_maps::candidate)
    } else {
        None
    };
    let thread_metadata = project_thread_metadata(runtime_method, &params);
    let data = if let Some(item) = item {
        project_item(item)
    } else {
        project_event_data(runtime_method, &params)
    };
    let mut payload = json!({
        "schemaVersion": PROJECTION_VERSION,
        "threadId": thread_id,
        "turnId": turn_id,
        "itemId": item_id,
        "lifecycle": lifecycle,
        "itemType": item_type,
        "data": data,
    });
    if final_delivery_seen {
        payload
            .pointer_mut("/data/result/structuredContent")
            .and_then(Value::as_object_mut)
            .map(|structured| structured.remove("artifact"));
    }
    if let Some(code) = artifact_delivery_error {
        mark_artifact_delivery_failure(&mut payload, &code);
    }

    Ok(Some(ProjectedEvent {
        event_type: event_type.to_string(),
        workspace_id: frame.workspace_id,
        thread_id,
        turn_id,
        item_id,
        payload,
        thread_metadata,
        artifacts,
        inline_map,
    }))
}

fn mark_artifact_delivery_failure(payload: &mut Value, code: &str) {
    if let Some(data) = payload.pointer_mut("/data").and_then(Value::as_object_mut) {
        data.remove("artifacts");
        data.insert(
            "artifactDelivery".to_string(),
            json!({
                "state": "failed",
                "failure": artifact_delivery_failure(code),
            }),
        );
    }
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
                run.workspace_id, run.codex_thread_id AS root_thread_id
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
                run.codex_thread_id AS root_thread_id
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
    if let Some(projected) = projected {
        return Ok(Some(event_run_context(&projected)));
    }

    let execution = sqlx::query(
        "SELECT execution.root_run_id AS run_id, run.task_id, execution.organization_id,
                execution.profile_id, execution.workspace_id,
                run.codex_thread_id AS root_thread_id
         FROM runtime_agent_execution_projections execution
         JOIN runs run ON run.id = execution.root_run_id
           AND run.organization_id = execution.organization_id
         WHERE execution.agent_thread_id = $1
           AND ($2::uuid IS NULL OR execution.workspace_id = $2)
         ORDER BY execution.created_at DESC
         LIMIT 1",
    )
    .bind(thread_id)
    .bind(workspace_id)
    .fetch_optional(&mut **transaction)
    .await
    .map_err(|error| format!("projected agent execution run lookup error: {error}"))?;
    Ok(execution.as_ref().map(event_run_context))
}

fn event_run_context(row: &sqlx::postgres::PgRow) -> EventRunContext {
    EventRunContext {
        run_id: row.get("run_id"),
        task_id: row.get("task_id"),
        organization_id: row.get("organization_id"),
        profile_id: row.get("profile_id"),
        workspace_id: row.get("workspace_id"),
        root_thread_id: row.get("root_thread_id"),
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
    if updated == 0 {
        let execution_exists: bool = sqlx::query_scalar(
            "SELECT EXISTS (
                SELECT 1 FROM runtime_agent_execution_projections execution
                WHERE execution.root_run_id = $1
                  AND execution.profile_id = $2
                  AND execution.workspace_id = $3
                  AND execution.agent_thread_id = $4
             )",
        )
        .bind(context.run_id)
        .bind(context.profile_id)
        .bind(context.workspace_id)
        .bind(&event.thread_id)
        .fetch_one(&mut **transaction)
        .await
        .map_err(|error| format!("Runtime agent execution lookup error: {error}"))?;
        if execution_exists {
            return Ok(());
        }
    }
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
        .and_then(|value| bounded_runtime_text(value, 1_000))
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
                task, display_title, status, current_behavior, first_observed_sequence,
                last_observed_sequence, created_at, updated_at
             ) VALUES (
                $1, $2, $3, $4, $5, $6, $7, $8,
                $9, $10, 'pending', 'Waiting to start', $7, $7, $11, $11
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
        .bind(build_execution_title(None, Some(&task)))
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
        .and_then(|value| bounded_runtime_text(&value, 1_000));
    let first_observed_sequence = assignment_sequence.unwrap_or(sequence);
    let ordinal = next_agent_execution_ordinal(transaction, context.run_id, thread_id).await?;
    let display_title = task
        .as_deref()
        .map(|value| build_execution_title(None, Some(value)))
        .unwrap_or_else(|| build_execution_title(None, None));

    sqlx::query(
        "INSERT INTO runtime_agent_execution_projections (
            organization_id, profile_id, workspace_id, root_run_id,
            agent_thread_id, turn_id, ordinal, assignment_sequence,
            assignment_item_id, task, display_title, status, current_behavior,
            first_observed_sequence, last_observed_sequence, started_at,
            created_at, updated_at
         ) VALUES (
            $1, $2, $3, $4, $5, $6, $7, $8,
            $9, $10, $11, 'running', 'Started working',
            $12, $13, $14, $14, $14
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
    .bind(display_title)
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
        Some("completed" | "failed" | "rejected" | "cancelled" | "timeout" | "interrupted")
    );
    let waiting_for_input = observation.status == Some("waiting_for_input");
    sqlx::query(
        "UPDATE runtime_agent_execution_projections
         SET status = COALESCE($1, status),
             current_behavior = COALESCE($2, current_behavior),
             latest_progress = COALESCE($3, latest_progress),
             last_observed_sequence = GREATEST(last_observed_sequence, $4),
             completed_at = CASE WHEN $5 THEN COALESCE(completed_at, $6) ELSE completed_at END,
             terminal_sequence = CASE WHEN $5 THEN COALESCE(terminal_sequence, $4) ELSE terminal_sequence END,
             waiting_approval_id = CASE
                 WHEN $7 THEN $8
                 WHEN $9 THEN NULL
                 ELSE waiting_approval_id
             END,
             wait_started_at = CASE
                 WHEN $7 THEN COALESCE(wait_started_at, $6)
                 WHEN $9 OR $5 THEN NULL
                 ELSE wait_started_at
             END,
             wait_cycle_count = CASE WHEN $10 THEN wait_cycle_count + 1 ELSE wait_cycle_count END,
             result_summary = COALESCE($11, result_summary),
             updated_at = now()
         WHERE root_run_id = $12 AND agent_thread_id = $13 AND turn_id = $14
           AND terminal_sequence IS NULL",
    )
    .bind(observation.status)
    .bind(observation.behavior)
    .bind(observation.progress)
    .bind(sequence)
    .bind(terminal)
    .bind(observed_at)
    .bind(waiting_for_input)
    .bind(observation.approval_id)
    .bind(observation.clear_waiting)
    .bind(observation.increment_wait_cycle)
    .bind(observation.result_summary)
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
             completed_at = COALESCE(completed_at, $4),
             terminal_sequence = COALESCE(terminal_sequence, $3),
             updated_at = now()
         WHERE id = (
             SELECT id
             FROM runtime_agent_execution_projections
             WHERE root_run_id = $5 AND agent_thread_id = $6
               AND terminal_sequence IS NULL
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
    approval_id: Option<Uuid>,
    clear_waiting: bool,
    increment_wait_cycle: bool,
    result_summary: Option<String>,
}

fn agent_execution_observation(event: &ProjectedEvent) -> Option<AgentExecutionObservation> {
    match event.event_type.as_str() {
        "codex.turn.started" => Some(AgentExecutionObservation {
            status: Some("running"),
            behavior: Some("Started working".to_string()),
            progress: None,
            approval_id: None,
            clear_waiting: true,
            increment_wait_cycle: false,
            result_summary: None,
        }),
        "codex.turn.completed" => {
            let outcome = projected_turn_terminal_outcome(&event.payload);
            Some(AgentExecutionObservation {
                status: Some(outcome.execution_status()),
                behavior: Some(outcome.execution_behavior().to_string()),
                progress: None,
                approval_id: None,
                clear_waiting: true,
                increment_wait_cycle: false,
                result_summary: None,
            })
        }
        "platform.approval.requested" => {
            let data = event.payload.get("data").unwrap_or(&Value::Null);
            let request_method = data.get("requestMethod").and_then(Value::as_str);
            let approval_id = data
                .get("approvalId")
                .and_then(Value::as_str)
                .and_then(|value| Uuid::parse_str(value).ok());
            let is_user_input = request_method == Some("item/tool/requestUserInput");
            Some(AgentExecutionObservation {
                status: Some(if is_user_input {
                    "waiting_for_input"
                } else {
                    "waiting"
                }),
                behavior: Some(if is_user_input {
                    "Waiting for your input".to_string()
                } else {
                    "Waiting for approval".to_string()
                }),
                progress: None,
                approval_id,
                clear_waiting: false,
                increment_wait_cycle: false,
                result_summary: None,
            })
        }
        "platform.approval.resolved" => Some(AgentExecutionObservation {
            status: Some("running"),
            behavior: Some("Input or approval resolved; continuing work".to_string()),
            progress: None,
            approval_id: None,
            clear_waiting: true,
            increment_wait_cycle: false,
            result_summary: None,
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
    if matches!(item_type, "collabAgentToolCall" | "collabToolCall")
        && data
            .get("tool")
            .and_then(Value::as_str)
            .is_some_and(|tool| normalize_agent_tool(tool) == "wait")
    {
        return Some(AgentExecutionObservation {
            status: Some(if completed { "running" } else { "waiting" }),
            behavior: Some(if completed {
                "Wait cycle finished".to_string()
            } else {
                "Waiting for Agent updates".to_string()
            }),
            progress: None,
            approval_id: None,
            clear_waiting: completed,
            increment_wait_cycle: !completed,
            result_summary: None,
        });
    }
    if item_type == "agentMessage" && completed {
        let phase = data.get("phase").and_then(Value::as_str)?;
        let progress = data
            .get("text")
            .and_then(Value::as_str)
            .and_then(|value| bounded_runtime_text(value, 1_000));
        let behavior = match phase {
            "commentary" => progress
                .as_deref()
                .and_then(|value| bounded_runtime_text(value, 500))
                .unwrap_or_else(|| "Reported progress".to_string()),
            "final_answer" => "Returned results to the Supervisor".to_string(),
            _ => return None,
        };
        return Some(AgentExecutionObservation {
            status: (phase == "final_answer").then_some("completed"),
            behavior: Some(behavior.to_string()),
            progress,
            approval_id: None,
            clear_waiting: phase == "final_answer",
            increment_wait_cycle: false,
            result_summary: (phase == "final_answer")
                .then(|| data.get("text").and_then(Value::as_str))
                .flatten()
                .and_then(|value| bounded_runtime_text(value, 1_000)),
        });
    }

    let descriptor = project_agent_item_descriptor(item_type, data, completed)?;
    let verb = if descriptor.failed {
        "Could not complete"
    } else if completed {
        "Completed"
    } else {
        "Using"
    };
    Some(AgentExecutionObservation {
        status: None,
        behavior: Some(format!("{verb} {}", descriptor.label)),
        progress: None,
        approval_id: None,
        clear_waiting: false,
        increment_wait_cycle: false,
        result_summary: None,
    })
}

#[derive(Debug, Clone)]
pub(crate) struct AgentItemDescriptor {
    pub(crate) subject: RuntimeAgentActivitySubject,
    pub(crate) label: String,
    pub(crate) detail: Option<String>,
    pub(crate) failed: bool,
}

/// Project only the safe, bounded descriptor shared by live Agent execution
/// updates and the rebuildable activity endpoint. Raw item arguments/results
/// never enter this descriptor.
pub(crate) fn project_agent_item_descriptor(
    item_type: &str,
    data: &Value,
    completed: bool,
) -> Option<AgentItemDescriptor> {
    let failed = completed
        && (data.get("error").is_some_and(|value| !value.is_null())
            || data.get("success").and_then(Value::as_bool) == Some(false)
            || matches!(
                data.get("status").and_then(Value::as_str),
                Some("failed" | "error")
            ));
    let subject = match item_type {
        "mcpToolCall" => RuntimeAgentActivitySubject::McpTool {
            server: data
                .get("server")
                .and_then(Value::as_str)
                .and_then(display_identifier),
            tool: data
                .get("tool")
                .and_then(Value::as_str)
                .and_then(display_identifier),
        },
        "dynamicToolCall" => RuntimeAgentActivitySubject::RuntimeTool {
            namespace: data
                .get("namespace")
                .and_then(Value::as_str)
                .and_then(display_identifier),
            tool: data
                .get("tool")
                .and_then(Value::as_str)
                .and_then(display_identifier),
        },
        "commandExecution" => command_subject(data),
        "webSearch" => RuntimeAgentActivitySubject::WebSearch,
        "imageView" => RuntimeAgentActivitySubject::ImageView,
        "imageGeneration" => RuntimeAgentActivitySubject::ImageGeneration,
        _ => return None,
    };
    let label = subject_label(&subject);
    let detail = subject_detail(&subject);
    Some(AgentItemDescriptor {
        subject,
        label,
        detail,
        failed,
    })
}

fn command_subject(data: &Value) -> RuntimeAgentActivitySubject {
    let Some(actions) = data.get("commandActions").and_then(Value::as_array) else {
        return RuntimeAgentActivitySubject::WorkspaceAction {
            action: "execute".to_string(),
            path: None,
        };
    };

    for action in actions.iter().take(16) {
        let Some(action) = action.as_object() else {
            continue;
        };
        let Some(action_type) = action.get("type").and_then(Value::as_str) else {
            continue;
        };
        let action_type = match action_type {
            "read" | "listFiles" | "search" => action_type,
            _ => continue,
        };
        let path = action
            .get("path")
            .and_then(Value::as_str)
            .and_then(safe_workspace_relative_path);
        return RuntimeAgentActivitySubject::WorkspaceAction {
            action: action_type.to_string(),
            path,
        };
    }

    RuntimeAgentActivitySubject::WorkspaceAction {
        action: "execute".to_string(),
        path: None,
    }
}

fn subject_label(subject: &RuntimeAgentActivitySubject) -> String {
    let label = match subject {
        RuntimeAgentActivitySubject::McpTool { server, tool } => match (server, tool) {
            (Some(server), Some(tool)) => {
                format!(
                    "{} · {}",
                    humanize_identifier(server),
                    humanize_identifier(tool)
                )
            }
            (Some(server), None) => humanize_identifier(server),
            (None, Some(tool)) => humanize_identifier(tool),
            (None, None) => "an enterprise tool".to_string(),
        },
        RuntimeAgentActivitySubject::RuntimeTool { namespace, tool } => match (namespace, tool) {
            (Some(namespace), Some(tool)) => format!(
                "{} · {}",
                humanize_identifier(namespace),
                humanize_identifier(tool)
            ),
            (Some(namespace), None) => humanize_identifier(namespace),
            (None, Some(tool)) => humanize_identifier(tool),
            (None, None) => "a Runtime tool".to_string(),
        },
        RuntimeAgentActivitySubject::WorkspaceAction { action, path } => {
            if let Some(path) = path {
                format!("workspace action · {action} · {path}")
            } else {
                format!("workspace action · {action}")
            }
        }
        RuntimeAgentActivitySubject::WebSearch => "web research".to_string(),
        RuntimeAgentActivitySubject::ImageView => "image inspection".to_string(),
        RuntimeAgentActivitySubject::ImageGeneration => "image generation".to_string(),
    };
    bounded_activity_title(&label)
}

fn subject_detail(subject: &RuntimeAgentActivitySubject) -> Option<String> {
    match subject {
        RuntimeAgentActivitySubject::WorkspaceAction { action, path } => {
            let detail = path
                .as_deref()
                .map(|path| format!("{action} · {path}"))
                .unwrap_or_else(|| action.clone());
            bounded_runtime_text(&detail, 256)
        }
        _ => None,
    }
}

fn safe_workspace_relative_path(value: &str) -> Option<String> {
    let value = value.trim();
    if value.is_empty()
        || value.contains("://")
        || value.starts_with('/')
        || value.starts_with('\\')
        || value.as_bytes().get(1) == Some(&b':')
        || value.chars().any(char::is_control)
        || value.split(['/', '\\']).any(|segment| segment == "..")
        || is_sensitive_key(value)
    {
        return None;
    }
    Some(value.chars().take(240).collect())
}

fn display_identifier(value: &str) -> Option<String> {
    let value = value.trim();
    if value.is_empty()
        || value.contains("://")
        || value.starts_with('/')
        || value.starts_with('\\')
        || value.as_bytes().get(1) == Some(&b':')
        || value.chars().any(char::is_control)
        || is_sensitive_key(value)
    {
        return None;
    }
    Some(value.chars().take(128).collect())
}

fn humanize_identifier(value: &str) -> String {
    value
        .replace(['_', '-'], " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn bounded_activity_title(value: &str) -> String {
    bounded_runtime_text(value, 240).unwrap_or_else(|| "Runtime activity".to_string())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProjectedTurnTerminalOutcome {
    Completed,
    Failed,
    Rejected,
    Cancelled,
    Timeout,
    Interrupted,
}

impl ProjectedTurnTerminalOutcome {
    pub(crate) fn execution_status(self) -> &'static str {
        match self {
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Rejected => "rejected",
            Self::Cancelled => "cancelled",
            Self::Timeout => "timeout",
            Self::Interrupted => "interrupted",
        }
    }

    pub(crate) fn execution_behavior(self) -> &'static str {
        match self {
            Self::Completed => "Finished this work cycle",
            Self::Failed => "Agent execution failed",
            Self::Rejected => "Agent execution was rejected",
            Self::Cancelled => "Agent execution was cancelled",
            Self::Timeout => "Agent execution timed out",
            Self::Interrupted => "Agent execution interrupted",
        }
    }
}

/// Resolve the official Turn terminal status from the already-normalized
/// payload. Unknown, missing, or overlong status values are conservatively
/// treated as the Runtime's normal completed outcome; no free-text matching is
/// used here.
pub(crate) fn projected_turn_terminal_outcome(payload: &Value) -> ProjectedTurnTerminalOutcome {
    let status = [
        payload.pointer("/data/status").and_then(Value::as_str),
        payload.pointer("/data/status/type").and_then(Value::as_str),
        payload.pointer("/data/turn/status").and_then(Value::as_str),
        payload
            .pointer("/data/turn/status/type")
            .and_then(Value::as_str),
    ]
    .into_iter()
    .flatten()
    .map(str::trim)
    .find(|value| !value.is_empty() && value.len() <= 64);

    let Some(status) = status else {
        return ProjectedTurnTerminalOutcome::Completed;
    };
    if status.eq_ignore_ascii_case("completed") {
        ProjectedTurnTerminalOutcome::Completed
    } else if status.eq_ignore_ascii_case("failed") {
        ProjectedTurnTerminalOutcome::Failed
    } else if status.eq_ignore_ascii_case("rejected") {
        ProjectedTurnTerminalOutcome::Rejected
    } else if status.eq_ignore_ascii_case("cancelled") || status.eq_ignore_ascii_case("canceled") {
        ProjectedTurnTerminalOutcome::Cancelled
    } else if status.eq_ignore_ascii_case("timeout") {
        ProjectedTurnTerminalOutcome::Timeout
    } else if status.eq_ignore_ascii_case("interrupted") {
        ProjectedTurnTerminalOutcome::Interrupted
    } else {
        ProjectedTurnTerminalOutcome::Completed
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

fn build_execution_title(agent_label: Option<&str>, task: Option<&str>) -> String {
    let label = agent_label
        .and_then(|value| bounded_runtime_text(value, 40))
        .unwrap_or_else(|| "Agent".to_string());
    let summary = task
        .and_then(|value| bounded_runtime_text(value, 70))
        .unwrap_or_else(|| "Assigned task".to_string());
    format!("{label} · {summary}").chars().take(80).collect()
}

pub(crate) fn bounded_runtime_text(value: &str, max_chars: usize) -> Option<String> {
    let normalized = redact_browser_text(value)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    let bounded = normalized.chars().take(max_chars).collect::<String>();
    (!bounded.is_empty()).then_some(bounded)
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
        "willRetry",
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
            if matches!(
                key,
                "error" | "turnError" | "turn_error" | "runtimeError" | "runtime_error"
            ) {
                if value.is_null() {
                    continue;
                }
                data.insert(key.to_string(), project_public_runtime_error(value));
            } else if matches!(key, "additionalDetails" | "additional_details") {
                continue;
            } else if method == "turn/completed" && key == "turn" {
                data.insert(key.to_string(), project_public_runtime_turn(value));
            } else {
                data.insert(key.to_string(), sanitize_value(value, key));
            }
        }
    }
    Value::Object(data)
}

fn project_public_runtime_turn(value: &Value) -> Value {
    let mut projected = sanitize_value(value, "turn");
    let Some(turn) = projected.as_object_mut() else {
        return projected;
    };
    if let Some(error) = turn.get("error").filter(|value| !value.is_null()) {
        turn.insert("error".to_string(), project_public_runtime_error(error));
    } else {
        turn.remove("error");
    }
    turn.remove("additionalDetails");
    projected
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
    content.retain(|item| item.get("type").and_then(Value::as_str) != Some("resource"));
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
    let mut registered = Vec::new();
    for artifact in &event.artifacts {
        let expected_size = artifact.byte_size;
        let existing_for_item = sqlx::query(
            "SELECT artifact.id, artifact.profile_id, artifact.workspace_id,
                    artifact.artifact_schema, artifact.display_name, artifact.mime_type,
                    artifact.source_relative_path, artifact.expected_size, artifact.state,
                    artifact.byte_size, artifact.failure_code,
                    provenance.producer_task_id
             FROM artifact_provenance provenance
             JOIN artifacts artifact ON artifact.id = provenance.artifact_id
               AND artifact.organization_id = provenance.organization_id
             WHERE provenance.organization_id = $1
               AND provenance.producer_run_id = $2
               AND provenance.producer_thread_id = $3
               AND provenance.producer_turn_id = $4
               AND provenance.producer_item_id = $5",
        )
        .bind(context.organization_id)
        .bind(context.run_id)
        .bind(&event.thread_id)
        .bind(turn_id)
        .bind(item_id)
        .fetch_optional(&mut **transaction)
        .await
        .map_err(|error| format!("Artifact provenance lookup error: {error}"))?;
        let (artifact_id, state, byte_size, failure_code) =
            if let Some(existing) = existing_for_item {
                if existing.get::<Uuid, _>("profile_id") != context.profile_id
                    || existing.get::<Uuid, _>("workspace_id") != context.workspace_id
                    || existing.get::<Uuid, _>("producer_task_id") != context.task_id
                    || existing.get::<String, _>("artifact_schema") != artifact.schema
                    || existing.get::<String, _>("display_name") != artifact.display_name
                    || existing.get::<String, _>("mime_type") != artifact.mime_type
                    || existing.get::<String, _>("source_relative_path")
                        != artifact.workspace_relative_path
                    || existing.get::<i64, _>("expected_size") != expected_size
                {
                    return Err(
                        "Artifact producer Item was replayed with different metadata".to_string(),
                    );
                }
                (
                    existing.get::<Uuid, _>("id"),
                    existing.get::<String, _>("state"),
                    existing.get::<Option<i64>, _>("byte_size"),
                    existing.get::<Option<String>, _>("failure_code"),
                )
            } else {
                let inserted = sqlx::query(
                    "INSERT INTO artifacts (
                organization_id, profile_id, workspace_id, artifact_schema,
                display_name, mime_type, source_relative_path, expected_size
             ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
             ON CONFLICT (organization_id, workspace_id, source_relative_path)
             DO NOTHING
             RETURNING id, state, byte_size, failure_code",
                )
                .bind(context.organization_id)
                .bind(context.profile_id)
                .bind(context.workspace_id)
                .bind(&artifact.schema)
                .bind(&artifact.display_name)
                .bind(&artifact.mime_type)
                .bind(&artifact.workspace_relative_path)
                .bind(expected_size)
                .fetch_optional(&mut **transaction)
                .await
                .map_err(|error| format!("Artifact registration error: {error}"))?;
                if let Some(inserted) = inserted {
                    (
                        inserted.get::<Uuid, _>("id"),
                        inserted.get::<String, _>("state"),
                        inserted.get::<Option<i64>, _>("byte_size"),
                        inserted.get::<Option<String>, _>("failure_code"),
                    )
                } else {
                    let existing = sqlx::query(
                        "SELECT id, profile_id, artifact_schema, display_name, mime_type,
                            expected_size, byte_size, state, failure_code
                     FROM artifacts
                     WHERE organization_id = $1 AND workspace_id = $2
                       AND source_relative_path = $3",
                    )
                    .bind(context.organization_id)
                    .bind(context.workspace_id)
                    .bind(&artifact.workspace_relative_path)
                    .fetch_optional(&mut **transaction)
                    .await
                    .map_err(|error| format!("Artifact conflict lookup error: {error}"))?
                    .ok_or_else(|| "Artifact conflict could not be resolved".to_string())?;
                    if existing.get::<Uuid, _>("profile_id") != context.profile_id
                        || existing.get::<String, _>("artifact_schema") != artifact.schema
                        || existing.get::<String, _>("display_name") != artifact.display_name
                        || existing.get::<String, _>("mime_type") != artifact.mime_type
                        || existing.get::<i64, _>("expected_size") != expected_size
                    {
                        return Err("Workspace Artifact path was reused with different metadata"
                            .to_string());
                    }
                    (
                        existing.get::<Uuid, _>("id"),
                        existing.get::<String, _>("state"),
                        existing.get::<Option<i64>, _>("byte_size"),
                        existing.get::<Option<String>, _>("failure_code"),
                    )
                }
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
        let provenance_inserted = sqlx::query(
            "INSERT INTO artifact_provenance (
                artifact_id, organization_id, producer_task_id, producer_run_id,
                producer_thread_id, producer_turn_id, producer_item_id
             ) VALUES ($1, $2, $3, $4, $5, $6, $7)
             ON CONFLICT DO NOTHING",
        )
        .bind(artifact_id)
        .bind(context.organization_id)
        .bind(context.task_id)
        .bind(context.run_id)
        .bind(&event.thread_id)
        .bind(turn_id)
        .bind(item_id)
        .execute(&mut **transaction)
        .await
        .map_err(|error| format!("Artifact provenance error: {error}"))?;
        if provenance_inserted.rows_affected() == 0 {
            let existing_artifact_id = sqlx::query_scalar::<_, Uuid>(
                "SELECT artifact_id FROM artifact_provenance
                 WHERE organization_id = $1 AND producer_run_id = $2
                   AND producer_thread_id = $3 AND producer_turn_id = $4
                   AND producer_item_id = $5 AND producer_task_id = $6",
            )
            .bind(context.organization_id)
            .bind(context.run_id)
            .bind(&event.thread_id)
            .bind(turn_id)
            .bind(item_id)
            .bind(context.task_id)
            .fetch_optional(&mut **transaction)
            .await
            .map_err(|error| format!("Artifact provenance conflict lookup error: {error}"))?;
            if existing_artifact_id != Some(artifact_id) {
                return Err("Artifact producer Item provenance conflict".to_string());
            }
        }

        registered.push(RegisteredArtifact {
            id: artifact_id,
            artifact_schema: artifact.schema.clone(),
            display_name: artifact.display_name.clone(),
            mime_type: artifact.mime_type.clone(),
            expected_size: Some(expected_size),
            byte_size,
            state,
            failure_code,
        });
    }
    Ok(registered)
}

fn project_registered_artifacts(
    payload: &mut Value,
    artifacts: &[RegisteredArtifact],
) -> Result<(), String> {
    if artifacts.is_empty() {
        return Ok(());
    }
    let projected = artifacts
        .iter()
        .map(|artifact| {
            let expected_size = artifact
                .expected_size
                .ok_or_else(|| "Artifact expected size is missing".to_string())?;
            artifact_delivery_projection(
                artifact.id,
                &artifact.artifact_schema,
                &artifact.display_name,
                &artifact.mime_type,
                expected_size,
                artifact.byte_size,
                &artifact.state,
                artifact.failure_code.as_deref(),
            )
        })
        .collect::<Result<Vec<_>, _>>()?;
    if let Some(data) = payload.pointer_mut("/data").and_then(Value::as_object_mut) {
        if let Some(structured) = data
            .get_mut("result")
            .and_then(Value::as_object_mut)
            .and_then(|result| result.get_mut("structuredContent"))
            .and_then(Value::as_object_mut)
        {
            structured.remove("artifact");
        }
        data.insert("artifacts".to_string(), Value::Array(projected));
    }
    Ok(())
}

pub(crate) fn sanitize_value(value: &Value, key: &str) -> Value {
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

/// Project Runtime errors from structured status/code fields; free-form text
/// is server-only because it may contain provider or credential data.
pub(crate) fn project_public_runtime_error(value: &Value) -> Value {
    let object = value.as_object();
    let info = object.and_then(|object| object.get("codexErrorInfo"));
    let kind = classify_public_runtime_error(info);
    let (code, message, recoverable) = kind.descriptor();
    let mut projected = Map::new();
    projected.insert("code".to_string(), Value::String(code.to_string()));
    projected.insert("message".to_string(), Value::String(message.to_string()));
    projected.insert("recoverable".to_string(), Value::Bool(recoverable));
    if let Some(info) = info.and_then(project_public_codex_error_info) {
        projected.insert("codexErrorInfo".to_string(), info);
    }
    Value::Object(projected)
}
pub(crate) fn project_public_run_event(mut event: RunEvent) -> RunEvent {
    event.payload = project_public_event_payload(&event.payload, &event.event_type);
    event
}
fn project_public_event_payload(value: &Value, event_type: &str) -> Value {
    let Some(root) = value.as_object() else {
        return value.clone();
    };
    let Some(data) = root.get("data").and_then(Value::as_object) else {
        return value.clone();
    };
    let source_type = data.get("sourceType").and_then(Value::as_str);
    let mut projected = value.clone();
    let Some(projected_root) = projected.as_object_mut() else {
        return value.clone();
    };
    let Some(projected_data) = projected_root
        .get_mut("data")
        .and_then(Value::as_object_mut)
    else {
        return value.clone();
    };

    if event_type == "codex.turn.completed" {
        replace_public_runtime_error(projected_data, "error");
        if let Some(turn) = projected_data
            .get_mut("turn")
            .and_then(Value::as_object_mut)
        {
            replace_public_runtime_error(turn, "error");
        }
    } else if source_type == Some("error") {
        replace_public_runtime_error(projected_data, "error");
    }
    projected
}

fn replace_public_runtime_error(object: &mut Map<String, Value>, key: &str) {
    let Some(value) = object.get(key).filter(|value| !value.is_null()) else {
        return;
    };
    object.insert(key.to_string(), project_public_runtime_error(value));
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PublicRuntimeErrorKind {
    Auth,
    RateLimit,
    Timeout,
    Network,
    Upstream,
    Interrupted,
    ContextWindow,
    UsageLimit,
    Overloaded,
    BadRequest,
    Policy,
    Sandbox,
    Internal,
    Unknown,
}

impl PublicRuntimeErrorKind {
    fn descriptor(self) -> (&'static str, &'static str, bool) {
        match self {
            Self::Auth => ("auth", "Provider authentication failed.", false),
            Self::RateLimit => ("rate_limit", "The provider rate limit was reached.", true),
            Self::Timeout => ("timeout", "The provider request timed out.", true),
            Self::Network => ("network", "The provider could not be reached.", true),
            Self::Upstream => ("upstream", "The upstream provider returned an error.", true),
            Self::Interrupted => ("interrupted", "The connection was interrupted.", true),
            Self::ContextWindow => ("context_window", "The context window was exceeded.", false),
            Self::UsageLimit => ("usage_limit", "The usage limit was reached.", false),
            Self::Overloaded => ("overloaded", "The provider is currently overloaded.", true),
            Self::BadRequest => ("bad_request", "The Runtime rejected the request.", false),
            Self::Policy => ("policy", "The request was blocked by policy.", false),
            Self::Sandbox => ("sandbox", "Workspace execution failed.", false),
            Self::Internal => (
                "internal",
                "The Runtime encountered an internal error.",
                false,
            ),
            Self::Unknown => ("unknown", "The Runtime reported an error.", false),
        }
    }
}

fn classify_public_runtime_error(info: Option<&Value>) -> PublicRuntimeErrorKind {
    let Some(key) = info.and_then(codex_error_info_key) else {
        return PublicRuntimeErrorKind::Unknown;
    };
    classify_codex_error_info(key, info.and_then(codex_error_info_status))
        .unwrap_or(PublicRuntimeErrorKind::Unknown)
}

fn classify_codex_error_info(key: &str, status: Option<u16>) -> Option<PublicRuntimeErrorKind> {
    Some(match key {
        "unauthorized" => PublicRuntimeErrorKind::Auth,
        "contextWindowExceeded" => PublicRuntimeErrorKind::ContextWindow,
        "sessionBudgetExceeded" | "usageLimitExceeded" => PublicRuntimeErrorKind::UsageLimit,
        "serverOverloaded" => PublicRuntimeErrorKind::Overloaded,
        "cyberPolicy" => PublicRuntimeErrorKind::Policy,
        "badRequest" => PublicRuntimeErrorKind::BadRequest,
        "sandboxError" => PublicRuntimeErrorKind::Sandbox,
        "threadRollbackFailed" => PublicRuntimeErrorKind::Internal,
        "activeTurnNotSteerable" => PublicRuntimeErrorKind::Interrupted,
        "responseStreamDisconnected" => match classify_http_status(status) {
            Some(PublicRuntimeErrorKind::Auth) => PublicRuntimeErrorKind::Auth,
            Some(PublicRuntimeErrorKind::RateLimit) => PublicRuntimeErrorKind::RateLimit,
            Some(PublicRuntimeErrorKind::Timeout) => PublicRuntimeErrorKind::Timeout,
            Some(PublicRuntimeErrorKind::Upstream) => PublicRuntimeErrorKind::Upstream,
            _ => PublicRuntimeErrorKind::Interrupted,
        },
        "responseStreamConnectionFailed" | "httpConnectionFailed" => {
            classify_http_status(status).unwrap_or(PublicRuntimeErrorKind::Network)
        }
        "responseTooManyFailedAttempts" => {
            classify_http_status(status).unwrap_or(PublicRuntimeErrorKind::Network)
        }
        "internalServerError" => PublicRuntimeErrorKind::Internal,
        "other" => return None,
        _ => return None,
    })
}

fn classify_http_status(status: Option<u16>) -> Option<PublicRuntimeErrorKind> {
    Some(match status? {
        401 | 403 => PublicRuntimeErrorKind::Auth,
        408 | 504 => PublicRuntimeErrorKind::Timeout,
        429 => PublicRuntimeErrorKind::RateLimit,
        500..=599 => PublicRuntimeErrorKind::Upstream,
        _ => return None,
    })
}

fn codex_error_info_status(value: &Value) -> Option<u16> {
    let object = value.as_object()?;
    let key = codex_error_info_key(value)?;
    object
        .get(key)
        .and_then(Value::as_object)
        .and_then(|detail| detail.get("httpStatusCode"))
        .and_then(value_status)
}

fn value_status(value: &Value) -> Option<u16> {
    value.as_u64().and_then(|value| u16::try_from(value).ok())
}

fn codex_error_info_key(value: &Value) -> Option<&str> {
    if let Some(key) = value.as_str() {
        return classify_codex_error_info(key, None).map(|_| key);
    }
    value.as_object().and_then(|object| {
        object
            .keys()
            .find_map(|key| classify_codex_error_info(key, None).map(|_| key.as_str()))
    })
}

fn project_public_codex_error_info(value: &Value) -> Option<Value> {
    let key = codex_error_info_key(value)?;
    if let Some(string) = value.as_str() {
        return Some(Value::String(string.to_string()));
    }
    let Some(object) = value.as_object() else {
        return Some(Value::String(key.to_string()));
    };
    let nested = object.get(key).and_then(Value::as_object);
    let mut detail = Map::new();
    if let Some(nested) = nested {
        if let Some(status) = nested.get("httpStatusCode").and_then(value_status) {
            detail.insert("httpStatusCode".to_string(), Value::Number(status.into()));
        }
        if let Some(turn_kind) = nested
            .get("turnKind")
            .and_then(Value::as_str)
            .filter(|value| matches!(*value, "review" | "compact"))
        {
            detail.insert("turnKind".to_string(), Value::String(turn_kind.to_string()));
        }
    }
    Some(Value::Object(Map::from_iter([(
        key.to_string(),
        Value::Object(detail),
    )])))
}

pub(crate) fn bounded_sanitized_text(value: &Value, key: &str, max_len: usize) -> Option<String> {
    let sanitized = sanitize_value(value, key);
    let text = sanitized.as_str()?.trim();
    if text.is_empty() || text.len() > max_len || text.chars().any(char::is_control) {
        return None;
    }
    Some(text.to_string())
}

fn redact_browser_text(value: &str) -> String {
    redact_local_paths(&redact_internal_resource_uris(value))
}

/// Return whether the existing browser text sanitizer would disclose a host
/// path or internal Resource URI. Artifact materialization reuses this
/// predicate and rejects the bytes instead of rewriting them.
pub(crate) fn browser_text_contains_unsafe(value: &str) -> bool {
    if redact_browser_text(value) != value {
        return true;
    }
    let bytes = value.as_bytes();
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
        if bytes.get(scheme_end).copied() != Some(b':') {
            cursor += 1;
            continue;
        }
        let scheme = value[cursor..scheme_end].to_ascii_lowercase();
        let separator_len = if bytes.get(scheme_end + 1..scheme_end + 3) == Some(b"//") {
            1
        } else {
            0
        };
        let next = scheme_end + 1 + separator_len * 2;
        if next < bytes.len()
            && !bytes[next].is_ascii_whitespace()
            && (!matches!(scheme.as_str(), "http" | "https") || separator_len == 0)
        {
            return true;
        }
        cursor = scheme_end + 1;
    }
    false
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
            && (cursor == 0
                || is_local_path_boundary(bytes[cursor - 1])
                || !bytes[cursor - 1].is_ascii())
            && bytes.get(cursor + 1).is_some_and(|byte| {
                !byte.is_ascii_whitespace() && !matches!(byte, b'/' | b'>' | b')' | b']' | b'}')
            });
        let windows_path = bytes.get(cursor..cursor + 3).is_some_and(|candidate| {
            candidate[0].is_ascii_alphabetic()
                && candidate[1] == b':'
                && matches!(candidate[2], b'/' | b'\\')
        }) && (cursor == 0
            || is_local_path_boundary(bytes[cursor - 1])
            || !bytes[cursor - 1].is_ascii());
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

pub(crate) fn is_sensitive_key(key: &str) -> bool {
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
        "config",
        "configuration",
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
    fn projects_only_exact_final_tools_as_workspace_artifact_deliveries() {
        let item = json!({
            "method": "app-server-event",
            "params": {"message": {"method": "item/completed", "params": {
                "threadId": "thread-one",
                "turnId": "turn-one",
                "item": {
                    "id": "item-final",
                    "type": "mcpToolCall",
                    "server": "supply_chain",
                    "tool": "publish_network_planning_report",
                    "status": "completed",
                    "error": null,
                    "result": {"content": [], "structuredContent": {
                        "summary": "Created report.",
                        "artifact": {
                            "schema": "network_planning_report_bundle.v1",
                            "displayName": "Warehouse network planning report",
                            "mimeType": "application/json",
                            "workspaceRelativePath": "outputs/network-report.json",
                            "byteSize": 128
                        }
                    }}
                }
            }}}
        });
        let frame = format!("data: {item}\n\n");
        let projected = project_frame(frame.as_bytes()).unwrap().unwrap();
        assert_eq!(projected.artifacts.len(), 1);
        assert_eq!(
            projected.artifacts[0].workspace_relative_path,
            "outputs/network-report.json"
        );
        assert!(projected
            .payload
            .pointer("/data/result/structuredContent/artifact")
            .is_none());
        assert!(!projected
            .payload
            .to_string()
            .contains("outputs/network-report.json"));

        let item = json!({
            "method": "app-server-event",
            "params": {"message": {"method": "item/completed", "params": {
                "threadId": "thread-one",
                "turnId": "turn-one",
                "item": {
                    "id": "item-intermediate",
                    "type": "mcpToolCall",
                    "server": "supply_chain",
                    "tool": "compare_network_scenarios",
                    "result": {
                        "content": [{
                            "type": "resource_link",
                            "name": "comparison",
                            "title": "network_comparison.v2",
                            "uri": "supply-chain://resources/comparison",
                            "mimeType": "application/json",
                            "size": 128
                        }],
                        "structuredContent": {
                            "summary": "Compared.",
                            "resource_ref": {
                                "type": "mcp_resource",
                                "server": "supply_chain",
                                "uri": "supply-chain://resources/comparison",
                                "resource_schema": "network_comparison.v2"
                            }
                        }
                    }
                }
            }}}
        });
        let frame = format!("data: {item}\n\n");
        let projected = project_frame(frame.as_bytes()).unwrap().unwrap();
        assert!(projected.artifacts.is_empty());
        assert!(projected.payload.pointer("/data/artifacts").is_none());

        let mut invalid =
            serde_json::from_str::<Value>(frame.trim_start_matches("data: ").trim()).unwrap();
        invalid["params"]["message"]["params"]["item"]["server"] = json!("supply_chain");
        invalid["params"]["message"]["params"]["item"]["tool"] =
            json!("publish_network_planning_report");
        invalid["params"]["message"]["params"]["item"]["result"]["structuredContent"] =
            json!({"summary":"bad","artifact":{"schema":"wrong.v1"}});
        let frame = format!("data: {invalid}\n\n");
        let projected = project_frame(frame.as_bytes()).unwrap().unwrap();
        assert!(projected.artifacts.is_empty());
        assert_eq!(
            projected
                .payload
                .pointer("/data/artifactDelivery/state")
                .and_then(Value::as_str),
            Some("failed")
        );
        assert!(projected
            .payload
            .pointer("/data/result/structuredContent/artifact")
            .is_none());
    }

    #[test]
    fn artifact_projection_failure_is_bounded_and_removes_artifact_fields() {
        let mut payload = json!({
            "data": {
                "artifacts": [{"artifactId": "internal"}],
                "artifactDelivery": {"failureCode": "provider/<credential-fragment>"}
            }
        });
        mark_artifact_delivery_failure(&mut payload, "artifact_projection_failed");
        assert!(payload.pointer("/data/artifacts").is_none());
        assert!(payload
            .pointer("/data/artifactDelivery/failureCode")
            .is_none());
        assert_eq!(
            payload.pointer("/data/artifactDelivery/state"),
            Some(&json!("failed"))
        );
        assert_eq!(
            payload.pointer("/data/artifactDelivery/failure/code"),
            Some(&json!("artifact_projection_failed"))
        );
        assert!(!payload.to_string().contains("credential-fragment"));
    }

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

    fn agent_commentary_event(text: &str) -> ProjectedEvent {
        ProjectedEvent {
            event_type: "codex.item.completed".to_string(),
            workspace_id: None,
            thread_id: "child-thread".to_string(),
            turn_id: Some("turn-1".to_string()),
            item_id: Some("item-1".to_string()),
            payload: json!({
                "itemType": "agentMessage",
                "data": {
                    "phase": "commentary",
                    "text": text
                }
            }),
            thread_metadata: None,
            artifacts: Vec::new(),
            inline_map: None,
        }
    }

    #[test]
    fn projects_agent_commentary_as_current_behavior() {
        let first =
            agent_execution_observation(&agent_commentary_event("正在检查城市和仓库数据。"))
                .unwrap();
        let second =
            agent_execution_observation(&agent_commentary_event("已完成线路报价完整性检查。"))
                .unwrap();

        assert_eq!(first.behavior.as_deref(), Some("正在检查城市和仓库数据。"));
        assert_eq!(first.progress.as_deref(), Some("正在检查城市和仓库数据。"));
        assert_eq!(
            second.behavior.as_deref(),
            Some("已完成线路报价完整性检查。")
        );
        assert_ne!(first.behavior, second.behavior);
    }

    #[test]
    fn bounds_runtime_text_with_path_and_resource_redaction() {
        let value = bounded_runtime_text(
            "  C:\\Users\\example\\workspace\\secret.json\n\tsupply-chain://resources/task  ",
            240,
        )
        .expect("bounded runtime text");
        assert!(value.contains("[workspace-path]/secret.json"));
        assert!(value.contains("[internal-resource-uri]"));
        assert!(!value.contains("C:\\Users\\example"));
        assert!(!value.contains("supply-chain://"));
        assert!(!value.contains("  "));

        let title = build_execution_title(
            None,
            Some("Task: /Users/example/workspaces/child supply-chain://resources/task"),
        );
        assert!(title.contains("[workspace-path]/child"));
        assert!(title.contains("[internal-resource-uri]"));
        assert!(!title.contains("/Users/example"));
        assert!(!title.contains("supply-chain://"));

        let localized = bounded_runtime_text("工作区根目录：/Users/example/workspaces/child", 240)
            .expect("localized bounded runtime text");
        assert_eq!(localized, "工作区根目录：[workspace-path]/child");
    }

    #[test]
    fn maps_turn_terminal_outcomes_for_execution_without_text_heuristics() {
        let cases = [
            (
                json!({"data": {"status": "completed"}}),
                ProjectedTurnTerminalOutcome::Completed,
            ),
            (
                json!({"data": {"status": "FAILED"}}),
                ProjectedTurnTerminalOutcome::Failed,
            ),
            (
                json!({"data": {"status": "rejected"}}),
                ProjectedTurnTerminalOutcome::Rejected,
            ),
            (
                json!({"data": {"status": "cancelled"}}),
                ProjectedTurnTerminalOutcome::Cancelled,
            ),
            (
                json!({"data": {"status": "canceled"}}),
                ProjectedTurnTerminalOutcome::Cancelled,
            ),
            (
                json!({"data": {"status": {"type": "timeout"}}}),
                ProjectedTurnTerminalOutcome::Timeout,
            ),
            (
                json!({"data": {"turn": {"status": {"type": "interrupted"}}}}),
                ProjectedTurnTerminalOutcome::Interrupted,
            ),
            (
                json!({"data": {"status": "failed because of a provider error"}}),
                ProjectedTurnTerminalOutcome::Completed,
            ),
            (json!({"data": {}}), ProjectedTurnTerminalOutcome::Completed),
        ];

        for (payload, expected) in cases {
            let event = ProjectedEvent {
                event_type: "codex.turn.completed".to_string(),
                workspace_id: None,
                thread_id: "child-thread".to_string(),
                turn_id: Some("turn-1".to_string()),
                item_id: None,
                payload,
                thread_metadata: None,
                artifacts: Vec::new(),
                inline_map: None,
            };

            assert_eq!(projected_turn_terminal_outcome(&event.payload), expected);
            let observation = agent_execution_observation(&event).unwrap();
            assert_eq!(observation.status, Some(expected.execution_status()));
            assert_eq!(
                observation.behavior.as_deref(),
                Some(expected.execution_behavior())
            );
        }
    }

    #[test]
    fn uses_the_same_safe_item_descriptor_for_live_execution_observation() {
        let event = ProjectedEvent {
            event_type: "codex.item.completed".to_string(),
            workspace_id: None,
            thread_id: "child-thread".to_string(),
            turn_id: Some("turn-1".to_string()),
            item_id: Some("item-1".to_string()),
            payload: json!({
                "itemType": "commandExecution",
                "data": {
                    "status": "completed",
                    "command": "/private/profile/secret.csv",
                    "aggregatedOutput": "https://example.com/private",
                    "commandActions": [{
                        "type": "read",
                        "path": "src/routes/runtime_agents.rs"
                    }]
                }
            }),
            thread_metadata: None,
            artifacts: Vec::new(),
            inline_map: None,
        };

        let descriptor =
            project_agent_item_descriptor("commandExecution", &event.payload["data"], true)
                .unwrap();
        let observation = agent_execution_observation(&event).unwrap();

        assert_eq!(
            observation.behavior.as_deref(),
            Some("Completed workspace action · read · src/routes/runtime_agents.rs")
        );
        let expected = format!("Completed {}", descriptor.label);
        assert_eq!(observation.behavior.as_deref(), Some(expected.as_str()));
        assert!(!observation
            .behavior
            .as_deref()
            .unwrap_or_default()
            .contains("workspace command"));
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
    fn preserves_mcp_tool_error_semantics_with_existing_sanitizer() {
        let frame = br#"data: {"method":"app-server-event","params":{"message":{"method":"item/completed","params":{"threadId":"thread-1","item":{"id":"item-1","type":"mcpToolCall","error":{"message":"MCP failed at /private/profile/secret.json","credential":"<credential-fragment>"}}}}}}

"#;
        let event = project_frame(frame).unwrap().unwrap();
        assert_eq!(
            event.payload["data"]["error"]["message"],
            "MCP failed at [workspace-path]/secret.json"
        );
        assert_eq!(event.payload["data"]["error"]["credential"], "[redacted]");
        assert!(event.payload["data"]["error"].get("code").is_none());
        assert!(!event.payload.to_string().contains("<credential-fragment>"));
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

        let error = br#"data: {"method":"app-server-event","params":{"message":{"method":"error","params":{"threadId":"thread-1","willRetry":true,"error":{"message":"stream disconnected <credential-fragment>","additionalDetails":"provider body https://provider.invalid/<credential-fragment>","codexErrorInfo":{"responseStreamDisconnected":{"httpStatusCode":null}},"apiKey":"must-not-leak"}}}}}

"#;
        let error = project_frame(error).unwrap().unwrap();
        assert_eq!(error.payload["data"]["sourceType"], "error");
        assert_eq!(
            error.payload["data"]["error"]["message"],
            "The connection was interrupted."
        );
        assert_eq!(error.payload["data"]["error"]["code"], "interrupted");
        assert_eq!(error.payload["data"]["willRetry"], true);
        assert!(!error.payload.to_string().contains("credential-fragment"));
        assert!(!error.payload.to_string().contains("provider.invalid"));

        let completed = br#"data: {"method":"app-server-event","params":{"message":{"method":"turn/completed","params":{"threadId":"thread-1","turn":{"id":"turn-1","error":{"message":"provider body <credential-fragment>","additionalDetails":"https://provider.invalid/<credential-fragment>"}}}}}}

"#;
        let completed = project_frame(completed).unwrap().unwrap();
        assert_eq!(
            completed.payload["data"]["turn"]["error"]["code"],
            "unknown"
        );
        assert!(!completed
            .payload
            .to_string()
            .contains("credential-fragment"));
        assert!(!completed.payload.to_string().contains("provider.invalid"));
    }

    #[test]
    fn projects_structured_runtime_error_categories_without_free_form_text() {
        let cases = [
            (
                json!({
                    "message": "unauthorized <credential-fragment>",
                    "additionalDetails": "provider response body",
                    "codexErrorInfo": "unauthorized"
                }),
                "auth",
                false,
            ),
            (
                json!({
                    "message": "429 <credential-fragment>",
                    "codexErrorInfo": {"httpConnectionFailed": {"httpStatusCode": 429}}
                }),
                "rate_limit",
                true,
            ),
            (
                json!({
                    "message": "upstream <credential-fragment>",
                    "codexErrorInfo": {"httpConnectionFailed": {"httpStatusCode": 503}}
                }),
                "upstream",
                true,
            ),
            (
                json!({
                    "message": "disconnected <credential-fragment>",
                    "codexErrorInfo": {"responseStreamDisconnected": {"httpStatusCode": 504}}
                }),
                "timeout",
                true,
            ),
            (
                json!({
                    "message": "connection failed <credential-fragment>",
                    "codexErrorInfo": "responseStreamConnectionFailed"
                }),
                "network",
                true,
            ),
            (
                json!({
                    "message": "context <credential-fragment>",
                    "codexErrorInfo": "contextWindowExceeded"
                }),
                "context_window",
                false,
            ),
            (
                json!({"message": "unknown <credential-fragment>"}),
                "unknown",
                false,
            ),
        ];

        for (input, code, recoverable) in cases {
            let projected = project_public_runtime_error(&input);
            assert_eq!(projected["code"], code);
            assert_eq!(projected["recoverable"], recoverable);
            assert!(!projected.to_string().contains("credential-fragment"));
            assert!(!projected.to_string().contains("provider response body"));
            assert!(projected.get("additionalDetails").is_none());
        }
    }

    #[test]
    fn reprojects_old_persisted_runtime_errors_at_the_read_boundary() {
        let canary = "<credential-fragment>";
        let event = RunEvent {
            id: Uuid::now_v7(),
            sequence: 3,
            run_id: Uuid::now_v7(),
            event_type: "codex.turn.completed".to_string(),
            projection_version: 1,
            thread_id: Some("thread-1".to_string()),
            turn_id: Some("turn-1".to_string()),
            item_id: None,
            payload: json!({
                "data": {
                    "error": {
                        "message": format!("provider body {canary}"),
                        "additionalDetails": format!("https://provider.invalid/{canary}"),
                        "codexErrorInfo": {"responseStreamDisconnected": {"httpStatusCode": 504}}
                    },
                    "turn": {"error": {"message": canary}}
                }
            }),
            created_at: chrono::Utc::now(),
        };

        let projected = project_public_run_event(event);
        let encoded = projected.payload.to_string();
        assert!(!encoded.contains(canary));
        assert!(!encoded.contains("provider.invalid"));
        assert_eq!(projected.payload["data"]["error"]["code"], "timeout");
        assert_eq!(
            projected.payload["data"]["turn"]["error"]["code"],
            "unknown"
        );

        let tool_data = project_item(
            json!({
                "type": "mcpToolCall",
                "error": {
                    "message": "MCP failed at /private/profile/secret.json",
                    "credential": canary
                }
            })
            .as_object()
            .unwrap(),
        );
        let tool_event = RunEvent {
            id: Uuid::now_v7(),
            sequence: 4,
            run_id: projected.run_id,
            event_type: "codex.item.completed".to_string(),
            projection_version: 1,
            thread_id: Some("thread-1".to_string()),
            turn_id: Some("turn-1".to_string()),
            item_id: Some("item-1".to_string()),
            payload: json!({
                "itemType": "mcpToolCall",
                "data": {
                    "result": {"error": "result-level business error"},
                    "error": tool_data["error"].clone()
                }
            }),
            created_at: chrono::Utc::now(),
        };
        let projected_tool = project_public_run_event(tool_event);
        assert_eq!(
            projected_tool.payload["data"]["result"]["error"],
            "result-level business error"
        );
        assert_eq!(
            projected_tool.payload["data"]["error"]["message"],
            "MCP failed at [workspace-path]/secret.json"
        );
        assert_eq!(
            projected_tool.payload["data"]["error"]["credential"],
            "[redacted]"
        );
        assert!(projected_tool.payload["data"]["error"]
            .get("code")
            .is_none());
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
