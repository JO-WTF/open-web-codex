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

    let execution_rows = sqlx::query(
        "SELECT DISTINCT ON (agent_thread_id)
                agent_thread_id, task, latest_progress, status
         FROM runtime_agent_execution_projections
         WHERE root_run_id = $1 AND organization_id = $2
         ORDER BY agent_thread_id, ordinal DESC",
    )
    .bind(run_id)
    .bind(auth.organization_id)
    .fetch_all(&state.db)
    .await
    .map_err(database_error)?;
    let projection_wait_context = execution_rows
        .into_iter()
        .map(|row| {
            let task = row
                .get::<Option<String>, _>("task")
                .as_deref()
                .and_then(wait_task_summary);
            let latest_progress = row
                .get::<Option<String>, _>("latest_progress")
                .as_deref()
                .and_then(wait_progress_summary);
            let status = row.get::<String, _>("status");
            (
                row.get::<String, _>("agent_thread_id"),
                WaitTaskContext {
                    task,
                    latest_progress,
                    active: !matches!(status.as_str(), "completed" | "failed" | "interrupted"),
                },
            )
        })
        .collect::<HashMap<_, _>>();

    let mut assignments = HashMap::<String, RuntimeAgentActivity>::new();
    let mut wait_context = HashMap::<String, WaitTaskContext>::new();
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
        update_wait_context(&event, &mut wait_context);
        for activity in
            project_activities_with_wait_context(event, &wait_context, &projection_wait_context)
        {
            if activity.kind == RuntimeAgentActivityKind::Assignment && activity.turn_id.is_none() {
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
    project_activities_with_wait_context(event, &HashMap::new(), &HashMap::new())
}

#[derive(Debug, Clone, Default)]
struct WaitTaskContext {
    task: Option<String>,
    latest_progress: Option<String>,
    active: bool,
}

fn project_activities_with_wait_context(
    event: ActivityEvent,
    wait_context: &HashMap<String, WaitTaskContext>,
    projection_wait_context: &HashMap<String, WaitTaskContext>,
) -> Vec<RuntimeAgentActivity> {
    let data = event.payload.get("data").unwrap_or(&Value::Null);
    let item_type = event
        .payload
        .get("itemType")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let completed = event.event_type == "codex.item.completed";
    let is_collaboration_item = matches!(item_type, "collabAgentToolCall" | "collabToolCall");
    let collaboration_tool = is_collaboration_item
        .then(|| {
            data.get("tool")
                .and_then(Value::as_str)
                .map(normalize_tool_name)
                .unwrap_or_default()
        })
        .unwrap_or_default();

    if is_collaboration_item && matches!(collaboration_tool.as_str(), "wait" | "waitagent") {
        let (status, title, detail) =
            project_wait_activity(data, completed, wait_context, projection_wait_context);
        return vec![activity(
            &event,
            event.thread_id.clone(),
            RuntimeAgentActivityKind::Waiting,
            status,
            &title,
            Some(detail),
        )];
    }

    if is_collaboration_item && !completed {
        let prompt = data.get("prompt").and_then(Value::as_str);
        let detail = prompt.and_then(bounded_detail);
        let title = match (
            collaboration_tool.as_str(),
            prompt.and_then(brief_description),
        ) {
            ("spawnagent", Some(summary)) => format!("Starting: {summary}"),
            ("sendinput" | "sendmessage" | "followuptask", Some(summary)) => {
                format!("Sending instructions: {summary}")
            }
            ("spawnagent", None) => "Starting Agent task".to_string(),
            ("sendinput" | "sendmessage" | "followuptask", None) => {
                "Sending Agent instructions".to_string()
            }
            _ => return Vec::new(),
        };
        let kind = if collaboration_tool == "spawnagent" {
            RuntimeAgentActivityKind::Assignment
        } else {
            RuntimeAgentActivityKind::Guidance
        };
        return vec![activity(
            &event,
            event.thread_id.clone(),
            kind,
            RuntimeAgentActivityStatus::Running,
            &title,
            detail,
        )];
    }

    if is_collaboration_item && completed {
        if !matches!(
            collaboration_tool.as_str(),
            "spawnagent" | "sendinput" | "sendmessage" | "followuptask"
        ) {
            return Vec::new();
        }
        let Some(prompt) = data.get("prompt").and_then(Value::as_str) else {
            return Vec::new();
        };
        let detail = bounded_detail(prompt);
        let (kind, status, title) = match collaboration_tool.as_str() {
            "spawnagent" => (
                RuntimeAgentActivityKind::Assignment,
                RuntimeAgentActivityStatus::Pending,
                brief_description(prompt)
                    .map(|summary| format!("Assigned: {summary}"))
                    .unwrap_or_else(|| "Task assigned".to_string()),
            ),
            "sendinput" | "sendmessage" | "followuptask" => (
                RuntimeAgentActivityKind::Guidance,
                RuntimeAgentActivityStatus::Running,
                brief_description(prompt)
                    .map(|summary| format!("Instructions: {summary}"))
                    .unwrap_or_else(|| "Supervisor sent instructions".to_string()),
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
                    &title,
                    detail.clone(),
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
        let detail = data
            .get("text")
            .and_then(Value::as_str)
            .and_then(bounded_detail);
        let (kind, status, title) = match phase {
            "commentary" => (
                RuntimeAgentActivityKind::Reporting,
                RuntimeAgentActivityStatus::Running,
                data.get("text")
                    .and_then(Value::as_str)
                    .and_then(brief_description)
                    .map(|summary| format!("Progress: {summary}"))
                    .unwrap_or_else(|| "Reported progress".to_string()),
            ),
            "final_answer" => (
                RuntimeAgentActivityKind::Completed,
                RuntimeAgentActivityStatus::Completed,
                data.get("text")
                    .and_then(Value::as_str)
                    .and_then(brief_description)
                    .map(|summary| format!("Completed: {summary}"))
                    .unwrap_or_else(|| "Agent task completed".to_string()),
            ),
            _ => return Vec::new(),
        };
        return vec![activity(
            &event,
            event.thread_id.clone(),
            kind,
            status,
            &title,
            detail,
        )];
    }

    let projected = match event.event_type.as_str() {
        "codex.turn.started" => Some((
            RuntimeAgentActivityKind::TurnStarted,
            RuntimeAgentActivityStatus::Running,
            task_event_title(
                "Started",
                "Started working",
                &event.thread_id,
                wait_context,
                projection_wait_context,
            ),
        )),
        "codex.turn.completed" => Some((
            RuntimeAgentActivityKind::TurnCompleted,
            RuntimeAgentActivityStatus::Completed,
            task_event_title(
                "Completed",
                "Finished this work cycle",
                &event.thread_id,
                wait_context,
                projection_wait_context,
            ),
        )),
        "codex.thread.completed" => Some((
            RuntimeAgentActivityKind::Completed,
            RuntimeAgentActivityStatus::Completed,
            task_event_title(
                "Completed",
                "Completed the assigned work",
                &event.thread_id,
                wait_context,
                projection_wait_context,
            ),
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

fn task_event_title(
    prefix: &str,
    fallback: &str,
    thread_id: &str,
    wait_context: &HashMap<String, WaitTaskContext>,
    projection_wait_context: &HashMap<String, WaitTaskContext>,
) -> String {
    let task = wait_context
        .get(thread_id)
        .and_then(|entry| entry.task.as_deref())
        .or_else(|| {
            projection_wait_context
                .get(thread_id)
                .and_then(|entry| entry.task.as_deref())
        });
    task.and_then(brief_description)
        .map(|summary| format!("{prefix}: {summary}"))
        .unwrap_or_else(|| fallback.to_string())
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

fn project_wait_activity(
    data: &Value,
    completed: bool,
    wait_context: &HashMap<String, WaitTaskContext>,
    projection_wait_context: &HashMap<String, WaitTaskContext>,
) -> (RuntimeAgentActivityStatus, String, String) {
    let receiver_count = string_list(data.get("receiverThreadIds")).len();
    let task_detail = describe_wait_tasks(data, completed, wait_context, projection_wait_context);
    if !completed {
        let title = if receiver_count == 0 {
            "Waiting for Agent updates".to_string()
        } else if receiver_count == 1 {
            "Waiting for 1 Agent".to_string()
        } else {
            format!("Waiting for {receiver_count} Agents")
        };
        let detail = if let Some(task_detail) = task_detail {
            task_detail
        } else if receiver_count == 0 {
            "The Supervisor is waiting for any Agent update or new input. This is a bounded wait, not a stopped task.".to_string()
        } else {
            format!(
                "The Supervisor is waiting for {receiver_count} assigned {} to finish or report a terminal state.",
                if receiver_count == 1 {
                    "Agent"
                } else {
                    "Agents"
                }
            )
        };
        return (RuntimeAgentActivityStatus::Waiting, title, detail);
    }

    let failed = matches!(
        data.get("status").and_then(Value::as_str),
        Some("failed" | "error")
    );
    if failed {
        return (
            RuntimeAgentActivityStatus::Failed,
            "Agent wait failed".to_string(),
            task_detail.unwrap_or_else(|| {
                "The bounded wait ended with an error. The Supervisor task is still observable through subsequent activity.".to_string()
            }),
        );
    }

    let summary = summarize_wait_states(data.get("agentsStates"));
    (
        RuntimeAgentActivityStatus::Completed,
        "Wait cycle finished".to_string(),
        match (task_detail, summary) {
            (Some(task_detail), Some(summary)) => format!("{task_detail} {summary}"),
            (Some(task_detail), None) => task_detail,
            (None, Some(summary)) => summary,
            (None, None) => "This bounded wait cycle ended. The Supervisor is processing any available update and may start another wait cycle.".to_string(),
        },
    )
}

fn update_wait_context(event: &ActivityEvent, wait_context: &mut HashMap<String, WaitTaskContext>) {
    if event.event_type == "codex.item.completed"
        && matches!(
            event.payload.get("itemType").and_then(Value::as_str),
            Some("collabAgentToolCall" | "collabToolCall")
        )
    {
        let data = event.payload.get("data").unwrap_or(&Value::Null);
        let tool = data
            .get("tool")
            .and_then(Value::as_str)
            .map(normalize_tool_name)
            .unwrap_or_default();
        if matches!(
            tool.as_str(),
            "spawnagent" | "sendinput" | "sendmessage" | "followuptask"
        ) {
            let task = data
                .get("prompt")
                .and_then(Value::as_str)
                .and_then(wait_task_summary);
            for thread_id in string_list(data.get("receiverThreadIds")) {
                let entry = wait_context.entry(thread_id).or_default();
                if task.is_some() {
                    entry.task = task.clone();
                }
                entry.active = true;
            }
        }
    }

    if event.event_type == "codex.item.completed"
        && event.payload.get("itemType").and_then(Value::as_str) == Some("agentMessage")
    {
        let data = event.payload.get("data").unwrap_or(&Value::Null);
        if data.get("phase").and_then(Value::as_str) == Some("commentary") {
            if let Some(progress) = data
                .get("text")
                .and_then(Value::as_str)
                .and_then(wait_progress_summary)
            {
                wait_context
                    .entry(event.thread_id.clone())
                    .or_default()
                    .latest_progress = Some(progress);
            }
        }
    }

    if matches!(
        event.event_type.as_str(),
        "codex.thread.completed" | "codex.thread.failed"
    ) {
        if let Some(entry) = wait_context.get_mut(&event.thread_id) {
            entry.active = false;
        }
    }
}

fn describe_wait_tasks(
    data: &Value,
    completed: bool,
    wait_context: &HashMap<String, WaitTaskContext>,
    projection_wait_context: &HashMap<String, WaitTaskContext>,
) -> Option<String> {
    let requested_ids = string_list(data.get("receiverThreadIds"));
    let mut thread_ids = if requested_ids.is_empty() {
        let mut ids = Vec::new();
        for context in [wait_context, projection_wait_context] {
            ids.extend(
                context
                    .iter()
                    .filter(|(_, entry)| completed || entry.active)
                    .filter(|(_, entry)| entry.task.is_some())
                    .map(|(thread_id, _)| thread_id.clone()),
            );
        }
        ids.sort();
        ids.dedup();
        ids
    } else {
        requested_ids
    };
    thread_ids.truncate(4);

    let mut descriptions = Vec::new();
    for thread_id in thread_ids {
        let entry = wait_context
            .get(&thread_id)
            .filter(|entry| entry.task.is_some())
            .or_else(|| {
                projection_wait_context
                    .get(&thread_id)
                    .filter(|entry| entry.task.is_some())
            });
        let Some(entry) = entry else {
            continue;
        };
        let Some(task) = entry.task.as_deref() else {
            continue;
        };
        let description = match entry.latest_progress.as_deref() {
            Some(progress) => format!("{task} (latest progress: {progress})"),
            None => task.to_string(),
        };
        descriptions.push(description);
    }
    if descriptions.is_empty() {
        return None;
    }

    let prefix = if descriptions.len() == 1 {
        "Waiting for Agent update: "
    } else {
        "Waiting for Agent updates: "
    };
    bounded_detail(&format!("{prefix}{}", descriptions.join("; ")))
}

fn wait_task_summary(value: &str) -> Option<String> {
    summarize_wait_text(value, 240)
}

fn wait_progress_summary(value: &str) -> Option<String> {
    summarize_wait_text(value, 240)
}

fn summarize_wait_text(value: &str, max_chars: usize) -> Option<String> {
    let normalized = value.split_whitespace().collect::<Vec<_>>().join(" ");
    let normalized = normalized
        .strip_prefix("Task:")
        .or_else(|| normalized.strip_prefix("任务："))
        .unwrap_or(&normalized)
        .trim();
    if normalized.is_empty() {
        return None;
    }
    let sentence_end = normalized
        .char_indices()
        .find(|(_, character)| matches!(*character, '.' | '。' | '!' | '！' | '?' | '？'))
        .map(|(index, character)| index + character.len_utf8());
    let summary = sentence_end
        .map(|end| &normalized[..end])
        .unwrap_or(normalized);
    let summary = summary.chars().take(max_chars).collect::<String>();
    (!summary.is_empty()).then_some(summary)
}

fn brief_description(value: &str) -> Option<String> {
    summarize_wait_text(value, 120)
}

fn summarize_wait_states(value: Option<&Value>) -> Option<String> {
    let states = value?.as_object()?;
    if states.is_empty() {
        return None;
    }

    let mut pending = 0;
    let mut running = 0;
    let mut completed = 0;
    let mut interrupted = 0;
    let mut failed = 0;
    let mut stopped = 0;
    let mut unknown = 0;
    for state in states.values() {
        let status = state
            .get("status")
            .and_then(Value::as_str)
            .map(normalize_tool_name)
            .unwrap_or_default();
        match status.as_str() {
            "pendinginit" | "pending" => pending += 1,
            "running" => running += 1,
            "completed" => completed += 1,
            "interrupted" => interrupted += 1,
            "errored" | "error" | "failed" | "notfound" => failed += 1,
            "shutdown" => stopped += 1,
            _ => unknown += 1,
        }
    }

    let mut parts = Vec::new();
    for (count, label) in [
        (completed, "completed"),
        (running, "still running"),
        (pending, "starting"),
        (interrupted, "interrupted"),
        (failed, "failed or unavailable"),
        (stopped, "shut down"),
        (unknown, "status unknown"),
    ] {
        if count > 0 {
            parts.push(format!("{count} {label}"));
        }
    }
    (!parts.is_empty()).then(|| format!("Agent status after this wait: {}.", parts.join(", ")))
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
    fn shows_a_brief_task_description_when_agent_start_is_projected() {
        let activities = project_activities(event(
            "codex.item.started",
            "root-thread",
            json!({
                "itemType": "collabAgentToolCall",
                "data": {
                    "tool": "spawn_agent",
                    "prompt": "Task: Prepare the Indonesia demo data. Include cities and routes.",
                    "receiverThreadIds": []
                }
            }),
        ));

        assert_eq!(activities.len(), 1);
        assert_eq!(activities[0].kind, RuntimeAgentActivityKind::Assignment);
        assert_eq!(activities[0].status, RuntimeAgentActivityStatus::Running);
        assert_eq!(
            activities[0].title,
            "Starting: Prepare the Indonesia demo data."
        );
        assert_eq!(
            activities[0].detail.as_deref(),
            Some("Task: Prepare the Indonesia demo data. Include cities and routes.")
        );
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
            activities[0].title,
            "Progress: Validated capacity and demand inputs."
        );
        assert_eq!(
            activities[0].detail.as_deref(),
            Some("Validated capacity and demand inputs.")
        );
    }

    #[test]
    fn projects_final_agent_report_as_a_completed_event_with_brief_title() {
        let activities = project_activities(event(
            "codex.item.completed",
            "child-thread",
            json!({
                "itemType": "agentMessage",
                "data": {
                    "phase": "final_answer",
                    "text": "Planning dataset is normalized and ready for network analysis."
                }
            }),
        ));

        assert_eq!(activities.len(), 1);
        assert_eq!(activities[0].kind, RuntimeAgentActivityKind::Completed);
        assert_eq!(activities[0].status, RuntimeAgentActivityStatus::Completed);
        assert_eq!(
            activities[0].title,
            "Completed: Planning dataset is normalized and ready for network analysis."
        );
        assert_eq!(
            activities[0].detail.as_deref(),
            Some("Planning dataset is normalized and ready for network analysis.")
        );
    }

    #[test]
    fn gives_agent_turn_start_and_completion_brief_titles() {
        let mut wait_context = HashMap::new();
        wait_context.insert(
            "child-thread".to_string(),
            WaitTaskContext {
                task: Some("Validate the planning dataset.".to_string()),
                latest_progress: None,
                active: true,
            },
        );

        let started = project_activities_with_wait_context(
            event("codex.turn.started", "child-thread", Value::Null),
            &wait_context,
            &HashMap::new(),
        );
        let completed = project_activities_with_wait_context(
            event("codex.turn.completed", "child-thread", Value::Null),
            &wait_context,
            &HashMap::new(),
        );

        assert_eq!(started[0].title, "Started: Validate the planning dataset.");
        assert_eq!(
            completed[0].title,
            "Completed: Validate the planning dataset."
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

    #[test]
    fn explains_v2_wait_cycles_without_inventing_target_agents() {
        let started = project_activities(event(
            "codex.item.started",
            "root-thread",
            json!({
                "itemType": "collabAgentToolCall",
                "data": {
                    "tool": "wait",
                    "status": "inProgress",
                    "receiverThreadIds": [],
                    "agentsStates": {}
                }
            }),
        ));
        let completed = project_activities(event(
            "codex.item.completed",
            "root-thread",
            json!({
                "itemType": "collabAgentToolCall",
                "data": {
                    "tool": "wait",
                    "status": "completed",
                    "receiverThreadIds": [],
                    "agentsStates": {}
                }
            }),
        ));

        assert_eq!(started.len(), 1);
        assert_eq!(started[0].kind, RuntimeAgentActivityKind::Waiting);
        assert_eq!(started[0].status, RuntimeAgentActivityStatus::Waiting);
        assert_eq!(started[0].title, "Waiting for Agent updates");
        assert!(started[0]
            .detail
            .as_deref()
            .unwrap()
            .contains("bounded wait"));

        assert_eq!(completed.len(), 1);
        assert_eq!(completed[0].status, RuntimeAgentActivityStatus::Completed);
        assert_eq!(completed[0].title, "Wait cycle finished");
        assert!(completed[0]
            .detail
            .as_deref()
            .unwrap()
            .contains("may start another"));
    }

    #[test]
    fn describes_wait_cycle_with_the_current_agent_task_and_progress() {
        let mut wait_context = HashMap::new();
        wait_context.insert(
            "child-thread".to_string(),
            WaitTaskContext {
                task: Some("Generate the Indonesia demo data.".to_string()),
                latest_progress: Some("Writing city and warehouse files.".to_string()),
                active: true,
            },
        );

        let activities = project_activities_with_wait_context(
            event(
                "codex.item.started",
                "root-thread",
                json!({
                    "itemType": "collabAgentToolCall",
                    "data": {
                        "tool": "wait",
                        "status": "inProgress",
                        "receiverThreadIds": [],
                        "agentsStates": {}
                    }
                }),
            ),
            &wait_context,
            &HashMap::new(),
        );

        assert_eq!(activities.len(), 1);
        assert_eq!(
            activities[0].detail.as_deref(),
            Some(
                "Waiting for Agent update: Generate the Indonesia demo data. (latest progress: Writing city and warehouse files.)"
            )
        );
    }

    #[test]
    fn derives_wait_description_from_persisted_assignment_and_progress_events() {
        let mut wait_context = HashMap::new();
        update_wait_context(
            &event(
                "codex.item.completed",
                "root-thread",
                json!({
                    "itemType": "collabAgentToolCall",
                    "data": {
                        "tool": "spawn_agent",
                        "prompt": "Task: Prepare the warehouse network demo. Include cities and routes.",
                        "receiverThreadIds": ["child-thread"]
                    }
                }),
            ),
            &mut wait_context,
        );
        update_wait_context(
            &event(
                "codex.item.completed",
                "child-thread",
                json!({
                    "itemType": "agentMessage",
                    "data": {
                        "phase": "commentary",
                        "text": "Checking the city and warehouse inputs."
                    }
                }),
            ),
            &mut wait_context,
        );

        let activities = project_activities_with_wait_context(
            event(
                "codex.item.started",
                "root-thread",
                json!({
                    "itemType": "collabAgentToolCall",
                    "data": {
                        "tool": "wait",
                        "status": "inProgress",
                        "receiverThreadIds": [],
                        "agentsStates": {}
                    }
                }),
            ),
            &wait_context,
            &HashMap::new(),
        );

        assert_eq!(activities.len(), 1);
        let detail = activities[0].detail.as_deref().unwrap();
        assert!(detail.contains("Prepare the warehouse network demo."));
        assert!(detail.contains("Checking the city and warehouse inputs."));
        assert!(detail.chars().count() <= 1_000);
    }

    #[test]
    fn summarizes_wait_statuses_without_exposing_thread_ids_or_messages() {
        let activities = project_activities(event(
            "codex.item.completed",
            "root-thread",
            json!({
                "itemType": "collabAgentToolCall",
                "data": {
                    "tool": "wait_agent",
                    "status": "completed",
                    "receiverThreadIds": ["secret-data-thread", "secret-network-thread"],
                    "agentsStates": {
                        "secret-data-thread": {
                            "status": "completed",
                            "message": "private model output"
                        },
                        "secret-network-thread": {"status": "running"}
                    }
                }
            }),
        ));

        assert_eq!(activities.len(), 1);
        assert_eq!(
            activities[0].detail.as_deref(),
            Some("Agent status after this wait: 1 completed, 1 still running.")
        );
        let serialized = serde_json::to_string(&activities).unwrap();
        assert!(!serialized.contains("secret-data-thread"));
        assert!(!serialized.contains("private model output"));
    }
}
