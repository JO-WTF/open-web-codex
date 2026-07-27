use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use open_web_codex_platform_contracts::error::PlatformError;
use open_web_codex_platform_contracts::{
    RuntimeAgentActivity, RuntimeAgentActivityKind, RuntimeAgentActivityStatus,
    RuntimeAgentExecution, RuntimeAgentProjection,
};
use open_web_codex_platform_store::AppState;
use serde_json::Value;
use sqlx::Row;
use std::collections::HashMap;
use uuid::Uuid;

use crate::middleware::auth::AuthenticatedUser;

type ApiResult<T> = Result<Json<T>, (StatusCode, Json<PlatformError>)>;
const MAX_AGENT_EXECUTIONS: i64 = 500;

/// Return the safe, rebuildable Runtime Thread tree associated with one Run.
///
/// Codex owns these Threads. The projection is deliberately read-only and
/// cannot be used as an alternative Agent lifecycle API.
pub async fn list_for_run(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(run_id): Path<Uuid>,
) -> ApiResult<Vec<RuntimeAgentProjection>> {
    ensure_run_access(&state, auth.organization_id, run_id).await?;

    let rows = sqlx::query(
        "SELECT root_run_id, thread_id, parent_thread_id, source_kind, agent_path,
                agent_nickname, agent_role, status_type, active_flags,
                first_observed_at, last_observed_at
         FROM runtime_agent_projections
         WHERE root_run_id = $1 AND organization_id = $2
         ORDER BY (parent_thread_id IS NOT NULL), first_observed_at, thread_id",
    )
    .bind(run_id)
    .bind(auth.organization_id)
    .fetch_all(&state.db)
    .await
    .map_err(database_error)?;

    Ok(Json(
        rows.into_iter()
            .map(|row| RuntimeAgentProjection {
                run_id: row.get("root_run_id"),
                thread_id: row.get("thread_id"),
                parent_thread_id: row.get("parent_thread_id"),
                source_kind: row.get("source_kind"),
                agent_path: row.get("agent_path"),
                agent_nickname: row.get("agent_nickname"),
                agent_role: row.get("agent_role"),
                status_type: row.get("status_type"),
                active_flags: row.get("active_flags"),
                is_root: row.get::<Option<String>, _>("parent_thread_id").is_none(),
                first_observed_at: row.get("first_observed_at"),
                last_observed_at: row.get("last_observed_at"),
            })
            .collect(),
    ))
}

/// Return a bounded, browser-safe activity stream for the Runtime Agent tree.
///
/// The stream is reconstructed from durable Run events and is therefore a
/// projection, not an Agent scheduler or a second owner of Runtime state.
pub async fn list_activities_for_run(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(run_id): Path<Uuid>,
) -> ApiResult<Vec<RuntimeAgentActivity>> {
    ensure_run_access(&state, auth.organization_id, run_id).await?;

    let rows = sqlx::query(
        "WITH selected_events AS (
             (
                 SELECT sequence, thread_id, turn_id, item_id, event_type, payload, created_at
                 FROM run_events
                 WHERE run_id = $1
                   AND thread_id IS NOT NULL
                   AND (
                       event_type IN (
                           'codex.turn.started', 'codex.turn.completed',
                           'codex.thread.completed', 'codex.thread.failed',
                           'platform.approval.requested', 'platform.approval.resolved'
                       )
                       OR (
                           event_type IN ('codex.item.started', 'codex.item.completed')
                           AND payload->>'itemType' IN (
                               'mcpToolCall', 'dynamicToolCall', 'commandExecution',
                               'collabAgentToolCall', 'collabToolCall',
                               'webSearch', 'imageView', 'imageGeneration',
                               'agentMessage'
                           )
                       )
                   )
                 ORDER BY sequence DESC
                 LIMIT 400
             )
             UNION
             (
                 SELECT sequence, thread_id, turn_id, item_id, event_type, payload, created_at
                 FROM run_events
                 WHERE run_id = $1
                   AND thread_id IS NOT NULL
                   AND event_type = 'codex.item.completed'
                   AND payload->>'itemType' IN ('collabAgentToolCall', 'collabToolCall')
                 ORDER BY sequence DESC
                 LIMIT 100
             )
         )
         SELECT sequence, thread_id, turn_id, item_id, event_type, payload, created_at
         FROM selected_events
         ORDER BY sequence",
    )
    .bind(run_id)
    .fetch_all(&state.db)
    .await
    .map_err(database_error)?;

    let mut assignments = HashMap::<String, RuntimeAgentActivity>::new();
    let mut activities = Vec::new();
    for row in rows {
        let event = ActivityEvent {
            run_id,
            sequence: row.get("sequence"),
            thread_id: row.get("thread_id"),
            turn_id: row.get("turn_id"),
            item_id: row.get("item_id"),
            event_type: row.get("event_type"),
            payload: row.get("payload"),
            created_at: row.get("created_at"),
        };
        for activity in project_activities(event) {
            if activity.kind == RuntimeAgentActivityKind::Assignment {
                assignments.insert(activity.thread_id.clone(), activity);
            } else {
                activities.push(activity);
            }
        }
    }
    if activities.len() > 400 {
        activities.drain(..activities.len() - 400);
    }
    let mut assignments = assignments.into_values().collect::<Vec<_>>();
    assignments.sort_by_key(|activity| activity.sequence);
    if assignments.len() > 100 {
        assignments.drain(..assignments.len() - 100);
    }
    activities.extend(assignments);
    activities.sort_by_key(|activity| activity.sequence);
    Ok(Json(activities))
}

/// Return persisted child Agent task nodes for the Web task stream.
///
/// Rows are a bounded, rebuildable projection of Runtime Turns. The endpoint
/// cannot create, resume or otherwise control an Agent.
pub async fn list_executions_for_run(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(run_id): Path<Uuid>,
) -> ApiResult<Vec<RuntimeAgentExecution>> {
    ensure_run_access(&state, auth.organization_id, run_id).await?;

    let rows = sqlx::query(
        "SELECT id, root_run_id, agent_thread_id, turn_id, ordinal, task, status,
                current_behavior, latest_progress, first_observed_sequence,
                last_observed_sequence, started_at, completed_at, created_at, updated_at
         FROM (
             SELECT id, root_run_id, agent_thread_id, turn_id, ordinal, task, status,
                    current_behavior, latest_progress, first_observed_sequence,
                    last_observed_sequence, started_at, completed_at, created_at, updated_at
             FROM runtime_agent_execution_projections
             WHERE root_run_id = $1 AND organization_id = $2
             ORDER BY first_observed_sequence DESC, id DESC
             LIMIT $3
         ) execution
         ORDER BY first_observed_sequence, id",
    )
    .bind(run_id)
    .bind(auth.organization_id)
    .bind(MAX_AGENT_EXECUTIONS)
    .fetch_all(&state.db)
    .await
    .map_err(database_error)?;

    Ok(Json(
        rows.into_iter()
            .map(|row| RuntimeAgentExecution {
                id: row.get("id"),
                run_id: row.get("root_run_id"),
                thread_id: row.get("agent_thread_id"),
                turn_id: row.get("turn_id"),
                ordinal: row.get("ordinal"),
                task: row.get("task"),
                status: row.get("status"),
                current_behavior: row.get("current_behavior"),
                latest_progress: row.get("latest_progress"),
                first_observed_sequence: row.get("first_observed_sequence"),
                last_observed_sequence: row.get("last_observed_sequence"),
                started_at: row.get("started_at"),
                completed_at: row.get("completed_at"),
                created_at: row.get("created_at"),
                updated_at: row.get("updated_at"),
            })
            .collect(),
    ))
}

struct ActivityEvent {
    run_id: Uuid,
    sequence: i64,
    thread_id: String,
    turn_id: Option<String>,
    item_id: Option<String>,
    event_type: String,
    payload: Value,
    created_at: chrono::DateTime<chrono::Utc>,
}

fn project_activities(event: ActivityEvent) -> Vec<RuntimeAgentActivity> {
    let data = event.payload.get("data").unwrap_or(&Value::Null);
    let item_type = event
        .payload
        .get("itemType")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let completed = event.event_type == "codex.item.completed";

    if matches!(item_type, "collabAgentToolCall" | "collabToolCall") && completed {
        let tool = data.get("tool").and_then(Value::as_str).unwrap_or_default();
        let normalized_tool = normalize_tool_name(tool);
        if !matches!(
            normalized_tool.as_str(),
            "spawnagent" | "sendinput" | "sendmessage" | "followuptask"
        ) {
            return Vec::new();
        }
        let Some(detail) = data
            .get("prompt")
            .and_then(Value::as_str)
            .and_then(bounded_detail)
        else {
            return Vec::new();
        };
        let (kind, status, title) = match normalized_tool.as_str() {
            "spawnagent" => (
                RuntimeAgentActivityKind::Assignment,
                RuntimeAgentActivityStatus::Pending,
                "Task assigned",
            ),
            "sendinput" | "sendmessage" | "followuptask" => (
                RuntimeAgentActivityKind::Guidance,
                RuntimeAgentActivityStatus::Running,
                "Supervisor sent instructions",
            ),
            _ => unreachable!("collaboration tool name was checked above"),
        };
        return string_list(data.get("receiverThreadIds"))
            .into_iter()
            .map(|thread_id| {
                let mut projected = activity(
                    &event,
                    thread_id,
                    kind.clone(),
                    status.clone(),
                    title,
                    Some(detail.clone()),
                );
                // The collaboration item belongs to the sender's Turn. The
                // receiver's task node is bound only when its own next Turn
                // starts, otherwise a queued follow-up could steal the
                // previous task's remaining activity.
                projected.turn_id = None;
                projected
            })
            .collect();
    }

    if item_type == "agentMessage" && completed {
        let Some(phase) = data.get("phase").and_then(Value::as_str) else {
            return Vec::new();
        };
        let (status, title) = match phase {
            "commentary" => (RuntimeAgentActivityStatus::Running, "Reported progress"),
            "final_answer" => (
                RuntimeAgentActivityStatus::Completed,
                "Returned results to the Supervisor",
            ),
            _ => return Vec::new(),
        };
        return vec![activity(
            &event,
            event.thread_id.clone(),
            RuntimeAgentActivityKind::Reporting,
            status,
            title,
            data.get("text")
                .and_then(Value::as_str)
                .and_then(bounded_detail),
        )];
    }

    let projected = match event.event_type.as_str() {
        "codex.turn.started" => Some((
            RuntimeAgentActivityKind::TurnStarted,
            RuntimeAgentActivityStatus::Running,
            "Started working".to_string(),
        )),
        "codex.turn.completed" => Some((
            RuntimeAgentActivityKind::TurnCompleted,
            RuntimeAgentActivityStatus::Completed,
            "Finished this work cycle".to_string(),
        )),
        "codex.thread.completed" => Some((
            RuntimeAgentActivityKind::Completed,
            RuntimeAgentActivityStatus::Completed,
            "Completed the assigned work".to_string(),
        )),
        "codex.thread.failed" => Some((
            RuntimeAgentActivityKind::Failed,
            RuntimeAgentActivityStatus::Failed,
            "Agent execution failed".to_string(),
        )),
        "platform.approval.requested" => Some((
            RuntimeAgentActivityKind::Waiting,
            RuntimeAgentActivityStatus::Waiting,
            "Waiting for approval".to_string(),
        )),
        "platform.approval.resolved" => Some((
            RuntimeAgentActivityKind::TurnStarted,
            RuntimeAgentActivityStatus::Running,
            "Approval resolved; continuing work".to_string(),
        )),
        "codex.item.started" | "codex.item.completed" => {
            project_item_activity(item_type, data, completed)
        }
        _ => None,
    };
    projected
        .map(|(kind, status, title)| {
            vec![activity(
                &event,
                event.thread_id.clone(),
                kind,
                status,
                &title,
                None,
            )]
        })
        .unwrap_or_default()
}

fn project_item_activity(
    item_type: &str,
    data: &Value,
    completed: bool,
) -> Option<(RuntimeAgentActivityKind, RuntimeAgentActivityStatus, String)> {
    let failed = completed
        && (data.get("error").is_some_and(|value| !value.is_null())
            || data.get("success").and_then(Value::as_bool) == Some(false)
            || matches!(
                data.get("status").and_then(Value::as_str),
                Some("failed" | "error")
            ));
    let (kind, status) = if failed {
        (
            RuntimeAgentActivityKind::ToolFailed,
            RuntimeAgentActivityStatus::Failed,
        )
    } else if completed {
        (
            RuntimeAgentActivityKind::ToolCompleted,
            RuntimeAgentActivityStatus::Completed,
        )
    } else {
        (
            RuntimeAgentActivityKind::ToolStarted,
            RuntimeAgentActivityStatus::Running,
        )
    };
    let verb = if failed {
        "Could not complete"
    } else if completed {
        "Completed"
    } else {
        "Using"
    };
    let subject = match item_type {
        "mcpToolCall" => {
            let server = data
                .get("server")
                .and_then(Value::as_str)
                .map(display_identifier);
            let tool = data
                .get("tool")
                .and_then(Value::as_str)
                .map(display_identifier);
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
            .map(display_identifier)
            .unwrap_or_else(|| "a Runtime tool".to_string()),
        "commandExecution" => "a workspace command".to_string(),
        "webSearch" => "web research".to_string(),
        "imageView" => "image inspection".to_string(),
        "imageGeneration" => "image generation".to_string(),
        _ => return None,
    };
    Some((kind, status, format!("{verb} {subject}")))
}

fn activity(
    event: &ActivityEvent,
    thread_id: String,
    kind: RuntimeAgentActivityKind,
    status: RuntimeAgentActivityStatus,
    title: &str,
    detail: Option<String>,
) -> RuntimeAgentActivity {
    RuntimeAgentActivity {
        run_id: event.run_id,
        sequence: event.sequence,
        thread_id,
        turn_id: event.turn_id.clone(),
        item_id: event.item_id.clone(),
        kind,
        status,
        title: title.to_string(),
        detail,
        created_at: event.created_at,
    }
}

fn string_list(value: Option<&Value>) -> Vec<String> {
    value
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty() && value.len() <= 256)
        .take(32)
        .map(str::to_string)
        .collect()
}

fn bounded_detail(value: &str) -> Option<String> {
    let value = value.trim();
    if value.is_empty() {
        return None;
    }
    Some(value.chars().take(1_000).collect())
}

fn normalize_tool_name(value: &str) -> String {
    value
        .chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

fn display_identifier(value: &str) -> String {
    value
        .trim()
        .trim_start_matches("mcp__")
        .replace(['_', '-'], " ")
}

async fn ensure_run_access(
    state: &AppState,
    organization_id: Uuid,
    run_id: Uuid,
) -> Result<(), (StatusCode, Json<PlatformError>)> {
    let run_exists = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(
            SELECT 1 FROM runs WHERE id = $1 AND organization_id = $2
         )",
    )
    .bind(run_id)
    .bind(organization_id)
    .fetch_one(&state.db)
    .await
    .map_err(database_error)?;
    if !run_exists {
        return Err((
            StatusCode::NOT_FOUND,
            Json(PlatformError::not_found("Run was not found")),
        ));
    }
    Ok(())
}

fn database_error(_error: sqlx::Error) -> (StatusCode, Json<PlatformError>) {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(PlatformError::internal("database operation failed")),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use serde_json::json;

    fn event(event_type: &str, thread_id: &str, payload: Value) -> ActivityEvent {
        ActivityEvent {
            run_id: Uuid::nil(),
            sequence: 7,
            thread_id: thread_id.to_string(),
            turn_id: Some("turn-1".to_string()),
            item_id: Some("item-1".to_string()),
            event_type: event_type.to_string(),
            payload,
            created_at: Utc::now(),
        }
    }

    #[test]
    fn assigns_prompt_to_the_runtime_receiver_without_exposing_tool_state() {
        let activities = project_activities(event(
            "codex.item.completed",
            "root-thread",
            json!({
                "itemType": "collabAgentToolCall",
                "data": {
                    "tool": "spawn_agent",
                    "prompt": "Analyze network capacity.",
                    "receiverThreadIds": ["child-thread"],
                    "agentsStates": {"child-thread": {"status": "running"}}
                }
            }),
        ));

        assert_eq!(activities.len(), 1);
        assert_eq!(activities[0].thread_id, "child-thread");
        assert_eq!(activities[0].kind, RuntimeAgentActivityKind::Assignment);
        assert_eq!(activities[0].status, RuntimeAgentActivityStatus::Pending);
        assert_eq!(activities[0].turn_id, None);
        assert_eq!(
            activities[0].detail.as_deref(),
            Some("Analyze network capacity.")
        );
        assert!(!serde_json::to_string(&activities)
            .unwrap()
            .contains("agentsStates"));
    }

    #[test]
    fn distinguishes_queued_guidance_from_a_new_task_assignment() {
        let activities = project_activities(event(
            "codex.item.completed",
            "root-thread",
            json!({
                "itemType": "collabAgentToolCall",
                "data": {
                    "tool": "sendInput",
                    "prompt": "Keep the baseline assumptions unchanged.",
                    "receiverThreadIds": ["child-thread"]
                }
            }),
        ));

        assert_eq!(activities.len(), 1);
        assert_eq!(activities[0].kind, RuntimeAgentActivityKind::Guidance);
        assert_eq!(
            activities[0].detail.as_deref(),
            Some("Keep the baseline assumptions unchanged.")
        );
        assert_eq!(activities[0].turn_id, None);
    }

    #[test]
    fn exposes_only_bounded_public_agent_progress() {
        let activities = project_activities(event(
            "codex.item.completed",
            "child-thread",
            json!({
                "itemType": "agentMessage",
                "data": {
                    "phase": "commentary",
                    "text": "Validated capacity and demand inputs."
                }
            }),
        ));

        assert_eq!(activities.len(), 1);
        assert_eq!(activities[0].kind, RuntimeAgentActivityKind::Reporting);
        assert_eq!(
            activities[0].detail.as_deref(),
            Some("Validated capacity and demand inputs.")
        );
    }

    #[test]
    fn projects_only_a_safe_tool_label() {
        let activities = project_activities(event(
            "codex.item.started",
            "child-thread",
            json!({
                "itemType": "mcpToolCall",
                "data": {
                    "server": "network_planner",
                    "tool": "optimize_network",
                    "arguments": {"warehouse_path": "/private/workspace/secret.csv"}
                }
            }),
        ));

        assert_eq!(activities.len(), 1);
        assert_eq!(
            activities[0].title,
            "Using network planner · optimize network"
        );
        let serialized = serde_json::to_string(&activities).unwrap();
        assert!(!serialized.contains("warehouse_path"));
        assert!(!serialized.contains("/private/workspace"));
    }
}
