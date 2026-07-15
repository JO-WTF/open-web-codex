use serde_json::{json, Map, Value};
use sqlx::PgPool;
use uuid::Uuid;

use crate::routes::approvals::{approval_expires_at, is_approval_method};

const PROJECTION_VERSION: i16 = 1;

#[derive(Debug, PartialEq)]
struct ProjectedEvent {
    event_type: String,
    thread_id: String,
    turn_id: Option<String>,
    item_id: Option<String>,
    payload: Value,
}

pub async fn persist_frame(data: &[u8], db: &PgPool) -> Result<(), String> {
    let Some((event, raw)) = project_frame(data)? else {
        return Ok(());
    };

    let mut transaction = db
        .begin()
        .await
        .map_err(|error| format!("event transaction error: {error}"))?;
    let run_id = sqlx::query_scalar::<_, Uuid>(
        "SELECT id FROM runs WHERE codex_thread_id = $1 ORDER BY created_at DESC LIMIT 1",
    )
    .bind(&event.thread_id)
    .fetch_optional(&mut *transaction)
    .await
    .map_err(|error| format!("event run lookup error: {error}"))?;

    let Some(run_id) = run_id else {
        return Ok(());
    };

    if let Some(approval) = extract_approval_request(&raw, run_id) {
        persist_approval(&mut transaction, approval).await?;
    }

    sqlx::query(
        "INSERT INTO run_events (
            run_id, event_type, projection_version, thread_id, turn_id, item_id, payload
         ) VALUES ($1, $2, $3, $4, $5, $6, $7)",
    )
    .bind(run_id)
    .bind(&event.event_type)
    .bind(PROJECTION_VERSION)
    .bind(&event.thread_id)
    .bind(&event.turn_id)
    .bind(&event.item_id)
    .bind(&event.payload)
    .execute(&mut *transaction)
    .await
    .map_err(|error| format!("event insert error: {error}"))?;

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
                UPDATE runs SET status = $1, updated_at = now()
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
        .map_err(|error| format!("event transaction commit error: {error}"))
}

fn project_frame(data: &[u8]) -> Result<Option<(ProjectedEvent, Value)>, String> {
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

    Ok(Some((
        ProjectedEvent {
            event_type: event_type.to_string(),
            thread_id,
            turn_id,
            item_id,
            payload,
        },
        value,
    )))
}

struct ApprovalInsert {
    run_id: Uuid,
    request_type: String,
    request_payload: Value,
    codex_request_id: Option<String>,
    workspace_id: Option<String>,
    thread_id: Option<String>,
}

fn extract_approval_request(value: &Value, run_id: Uuid) -> Option<ApprovalInsert> {
    if value.get("method").and_then(Value::as_str) != Some("app-server-event") {
        return None;
    }
    let message = value.pointer("/params/message")?.as_object()?;
    let method = message.get("method")?.as_str()?;
    if !is_approval_method(method) {
        return None;
    }
    let params = message
        .get("params")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let thread_id = string_field(&params, "threadId")
        .or_else(|| string_field(&params, "thread_id"));
    let workspace_id = value
        .pointer("/params/workspace_id")
        .or_else(|| value.pointer("/params/workspaceId"))
        .and_then(Value::as_str)
        .map(str::to_string);
    let codex_request_id = message
        .get("id")
        .map(|id| match id {
            Value::Number(number) => number.to_string(),
            Value::String(text) => text.clone(),
            _ => String::new(),
        })
        .filter(|value| !value.is_empty())
        .or_else(|| {
            params
                .get("requestId")
                .or_else(|| params.get("request_id"))
                .map(|id| match id {
                    Value::Number(number) => number.to_string(),
                    Value::String(text) => text.clone(),
                    _ => String::new(),
                })
                .filter(|value| !value.is_empty())
        });

    Some(ApprovalInsert {
        run_id,
        request_type: method.to_string(),
        request_payload: Value::Object(params),
        codex_request_id,
        workspace_id,
        thread_id,
    })
}

async fn persist_approval(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    approval: ApprovalInsert,
) -> Result<(), String> {
    sqlx::query(
        "INSERT INTO approvals (
            run_id, request_type, request_payload, status, codex_request_id,
            workspace_id, thread_id, expires_at
         ) VALUES ($1, $2, $3, 'pending', $4, $5, $6, $7)
         ON CONFLICT (run_id, codex_request_id)
             WHERE status = 'pending' AND codex_request_id IS NOT NULL
         DO NOTHING",
    )
    .bind(approval.run_id)
    .bind(&approval.request_type)
    .bind(&approval.request_payload)
    .bind(&approval.codex_request_id)
    .bind(&approval.workspace_id)
    .bind(&approval.thread_id)
    .bind(approval_expires_at())
    .execute(&mut **transaction)
    .await
    .map_err(|error| format!("approval insert error: {error}"))?;

    sqlx::query(
        "UPDATE runs SET status = 'waiting_approval', updated_at = now()
         WHERE id = $1 AND status = 'running'",
    )
    .bind(approval.run_id)
    .execute(&mut **transaction)
    .await
    .map_err(|error| format!("run waiting_approval update error: {error}"))?;
    Ok(())
}

fn classify_method(method: &str) -> (&'static str, &'static str) {
    match method {
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

        let (event, _) = project_frame(frame).unwrap().unwrap();

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

        let (event, _) = project_frame(frame).unwrap().unwrap();

        assert_eq!(event.event_type, "codex.unknown");
        assert_eq!(event.payload["data"]["sourceType"], "item/futureEvent");
        assert!(event.payload["data"].get("credential").is_none());
        assert!(event.payload["data"].get("payload").is_none());
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
