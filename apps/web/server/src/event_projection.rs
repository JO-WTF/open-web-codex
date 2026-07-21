use open_web_codex_platform_contracts::RunEvent;
use serde_json::{json, Map, Value};
use sqlx::PgPool;
use sqlx::Row;
use uuid::Uuid;

const PROJECTION_VERSION: i16 = 1;

#[derive(Debug, PartialEq)]
struct ProjectedEvent {
    event_type: String,
    thread_id: String,
    turn_id: Option<String>,
    item_id: Option<String>,
    payload: Value,
}

pub struct LiveProjection {
    pub organization_id: Uuid,
    pub payload: Vec<u8>,
}

pub async fn persist_frame(data: &[u8], db: &PgPool) -> Result<Option<LiveProjection>, String> {
    let Some(event) = project_frame(data)? else {
        return Ok(None);
    };

    let mut transaction = db
        .begin()
        .await
        .map_err(|error| format!("event transaction error: {error}"))?;
    let run = sqlx::query(
        "SELECT id, organization_id FROM runs \
         WHERE codex_thread_id = $1 ORDER BY created_at DESC LIMIT 1",
    )
    .bind(&event.thread_id)
    .fetch_optional(&mut *transaction)
    .await
    .map_err(|error| format!("event run lookup error: {error}"))?;

    let Some(run) = run else {
        return Ok(None);
    };
    let run_id: Uuid = run.get("id");
    let organization_id: Uuid = run.get("organization_id");

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
        _ => {}
    }

    let terminal_status = match event.event_type.as_str() {
        "codex.thread.completed" => Some("completed"),
        "codex.thread.failed" => Some("failed"),
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
                RETURNING task_id, workspace_id
             ), updated_workspace AS (
                UPDATE workspaces SET state = 'ready', updated_at = now()
                WHERE id IN (SELECT workspace_id FROM updated_run)
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
        sequence: persisted.get("sequence"),
        run_id,
        event_type: event.event_type,
        projection_version: PROJECTION_VERSION,
        thread_id: Some(event.thread_id),
        turn_id: event.turn_id,
        item_id: event.item_id,
        payload: event.payload,
        created_at: persisted.get("created_at"),
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
    }))
}

fn project_frame(data: &[u8]) -> Result<Option<ProjectedEvent>, String> {
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
    let message = match value.pointer("/params/message").and_then(Value::as_object) {
        Some(message) => message,
        None => return Ok(None),
    };
    let runtime_method = message
        .get("method")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    let params = message
        .get("params")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let thread_id =
        string_field(&params, "threadId").or_else(|| string_field(&params, "thread_id"));
    let Some(thread_id) = thread_id else {
        return Ok(None);
    };
    let turn_id = string_field(&params, "turnId")
        .or_else(|| string_field(&params, "turn_id"))
        .or_else(|| nested_string_field(&params, "turn", "id"));
    let item = params.get("item").and_then(Value::as_object);
    let item_id = string_field(&params, "itemId")
        .or_else(|| string_field(&params, "item_id"))
        .or_else(|| item.and_then(|item| string_field(item, "id")));

    let (event_type, lifecycle) = classify_method(runtime_method);
    let item_type = item.and_then(|item| string_field(item, "type"));
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
        thread_id,
        turn_id,
        item_id,
        payload,
    }))
}

fn classify_method(method: &str) -> (&'static str, &'static str) {
    match method {
        "platform/approvalRequested" => ("platform.approval.requested", "requested"),
        "item/started" => ("codex.item.started", "started"),
        "item/completed" => ("codex.item.completed", "completed"),
        "turn/started" => ("codex.turn.started", "started"),
        "turn/completed" => ("codex.turn.completed", "completed"),
        "thread/started" => ("codex.thread.started", "started"),
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

fn project_event_data(method: &str, params: &Map<String, Value>) -> Value {
    let mut data = Map::new();
    data.insert("sourceType".to_string(), Value::String(method.to_string()));
    for key in [
        "approvalId",
        "delta",
        "summaryIndex",
        "contentIndex",
        "startedAtMs",
        "completedAtMs",
    ] {
        if let Some(value) = params.get(key) {
            data.insert(key.to_string(), sanitize_value(value, key));
        }
    }
    Value::Object(data)
}

fn project_item(item: &Map<String, Value>) -> Value {
    let item_type = string_field(item, "type").unwrap_or_else(|| "unknown".to_string());
    let mut projected = Map::new();
    projected.insert("type".to_string(), Value::String(item_type.clone()));
    if let Some(status) = item.get("status") {
        projected.insert("status".to_string(), sanitize_value(status, "status"));
    }

    let fields: &[&str] = match item_type.as_str() {
        "userMessage" => &["content"],
        "hookPrompt" => &["fragments"],
        "agentMessage" => &["text", "phase"],
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
    Value::Object(projected)
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
        Value::String(value) if is_path_key(key) => Value::String(safe_path(value)),
        Value::String(value) if value.starts_with("data:") => {
            Value::String("[embedded-data]".to_string())
        }
        _ => value.clone(),
    }
}

fn is_sensitive_key(key: &str) -> bool {
    let normalized = key.to_ascii_lowercase().replace(['_', '-'], "");
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
    fn projects_platform_approvals_without_runtime_request_ids() {
        let frame = br#"data: {"method":"app-server-event","params":{"message":{"method":"platform/approvalRequested","params":{"approvalId":"018f-id","threadId":"thread-1"}}}}

"#;
        let event = project_frame(frame).unwrap().unwrap();
        assert_eq!(event.event_type, "platform.approval.requested");
        assert_eq!(event.payload["data"]["approvalId"], "018f-id");
        assert!(!event.payload.to_string().contains("requestId"));
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
}
