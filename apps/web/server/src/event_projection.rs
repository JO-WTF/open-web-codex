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
    reply_artifacts: Vec<ReplyArtifactCandidate>,
    inline_artifact: Option<InlineVisualizationArtifactCandidate>,
}

#[derive(Debug, PartialEq)]
struct ReplyArtifactCandidate {
    uri: String,
    mime_type: Option<String>,
    expected_size: Option<i64>,
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

    sqlx::query("SAVEPOINT artifact_projection")
        .execute(&mut *transaction)
        .await
        .map_err(|error| format!("Artifact projection savepoint error: {error}"))?;
    let artifact_result = async {
        register_reply_artifacts(&mut transaction, &event, run_id, organization_id).await?;
        register_inline_visualization_artifact(&mut transaction, &event, run_id, organization_id)
            .await?;
        resolve_inline_artifacts_in_transaction(&mut transaction, run_id, &mut event.payload).await
    }
    .await;
    match artifact_result {
        Ok(()) => {
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
                .map(|data| data.remove("inlineArtifacts"));
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
        "SELECT session.terminal_id, session.browser_workspace_id, session.run_id, \
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
    let browser_workspace_id: Uuid = session.get("browser_workspace_id");
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
                    "workspaceId": browser_workspace_id,
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
                    "workspaceId": browser_workspace_id,
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
    }))
}

fn project_frame(data: &[u8]) -> Result<Option<ProjectedEvent>, String> {
    let Some(message) = internal_message(data)? else {
        return Ok(None);
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
    let reply_artifacts = item
        .into_iter()
        .flat_map(reply_artifact_candidates)
        .collect();
    let inline_artifact = item.and_then(project_inline_visualization_artifact);
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
        reply_artifacts,
        inline_artifact,
    }))
}

fn internal_message(data: &[u8]) -> Result<Option<Map<String, Value>>, String> {
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
        Some(message) => message.clone(),
        None => return Ok(None),
    };
    Ok(Some(message))
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
        || structured
            .keys()
            .any(|key| !matches!(key.as_str(), "type" | "kind" | "artifact" | "embed"))
    {
        return None;
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

fn project_inline_renderer(kind: &str, payload: &Map<String, Value>) -> Option<Value> {
    match kind {
        "map.v2" => project_map_card(payload),
        _ => None,
    }
}

fn project_map_card(card: &Map<String, Value>) -> Option<Value> {
    const CARD_FIELDS: &[&str] = &[
        "title",
        "intent",
        "status",
        "fallback_text",
        "summary",
        "viewport",
        "sources",
        "layers",
        "legend",
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

    let mut projected = Map::new();
    projected.insert("title".to_string(), Value::String(title));
    projected.insert("intent".to_string(), Value::String(intent));
    projected.insert("status".to_string(), Value::String(status));
    for key in ["fallback_text", "summary"] {
        if let Some(value) = optional_string(card, key)? {
            projected.insert(key.to_string(), Value::String(value));
        }
    }
    projected.insert(
        "viewport".to_string(),
        project_map_viewport(card.get("viewport")?)?,
    );
    let sources = project_map_sources(card.get("sources")?)?;
    let source_ids = sources
        .iter()
        .filter_map(|source| source.get("id").and_then(Value::as_str))
        .collect::<std::collections::HashSet<_>>();
    let layers = project_map_layers(card.get("layers")?)?;
    if layers
        .iter()
        .any(|layer| match layer.get("source").and_then(Value::as_str) {
            Some(source) => !source_ids.contains(source),
            None => true,
        })
    {
        return None;
    }
    projected.insert("sources".to_string(), Value::Array(sources));
    projected.insert("layers".to_string(), Value::Array(layers));
    if let Some(legend) = card.get("legend") {
        projected.insert("legend".to_string(), project_map_legend(legend)?);
    }
    Some(Value::Object(projected))
}

fn project_map_viewport(value: &Value) -> Option<Value> {
    let viewport = value.as_object()?;
    let mode = viewport.get("mode")?.as_str()?;
    match mode {
        "fit" => {
            const FIELDS: &[&str] = &["mode", "padding", "max_zoom", "min_zoom"];
            if viewport.keys().any(|key| !FIELDS.contains(&key.as_str())) {
                return None;
            }
            let mut projected =
                Map::from_iter([("mode".to_string(), Value::String("fit".to_string()))]);
            if let Some(padding) = viewport.get("padding") {
                projected.insert("padding".to_string(), project_padding(padding)?);
            }
            for key in ["max_zoom", "min_zoom"] {
                if let Some(zoom) = viewport.get(key) {
                    projected.insert(key.to_string(), json!(bounded_number(zoom, 0.0, 24.0)?));
                }
            }
            Some(Value::Object(projected))
        }
        "camera" => {
            const FIELDS: &[&str] = &["mode", "center", "zoom", "bearing", "pitch"];
            if viewport.keys().any(|key| !FIELDS.contains(&key.as_str())) {
                return None;
            }
            let center = viewport.get("center")?.as_array()?;
            if center.len() != 2 {
                return None;
            }
            let longitude = bounded_number(&center[0], -180.0, 180.0)?;
            let latitude = bounded_number(&center[1], -90.0, 90.0)?;
            let mut projected = Map::from_iter([
                ("mode".to_string(), Value::String("camera".to_string())),
                ("center".to_string(), json!([longitude, latitude])),
                (
                    "zoom".to_string(),
                    json!(bounded_number(viewport.get("zoom")?, 0.0, 24.0)?),
                ),
            ]);
            if let Some(bearing) = viewport.get("bearing") {
                projected.insert(
                    "bearing".to_string(),
                    json!(bounded_number(bearing, -180.0, 180.0)?),
                );
            }
            if let Some(pitch) = viewport.get("pitch") {
                projected.insert(
                    "pitch".to_string(),
                    json!(bounded_number(pitch, 0.0, 85.0)?),
                );
            }
            Some(Value::Object(projected))
        }
        _ => None,
    }
}

fn project_padding(value: &Value) -> Option<Value> {
    if let Some(number) = value.as_f64() {
        return (0.0..=256.0).contains(&number).then(|| json!(number));
    }
    let padding = value.as_object()?;
    const FIELDS: &[&str] = &["top", "right", "bottom", "left"];
    if padding.len() != FIELDS.len() || padding.keys().any(|key| !FIELDS.contains(&key.as_str())) {
        return None;
    }
    Some(json!({
        "top": bounded_number(padding.get("top")?, 0.0, 256.0)?,
        "right": bounded_number(padding.get("right")?, 0.0, 256.0)?,
        "bottom": bounded_number(padding.get("bottom")?, 0.0, 256.0)?,
        "left": bounded_number(padding.get("left")?, 0.0, 256.0)?,
    }))
}

fn project_map_sources(value: &Value) -> Option<Vec<Value>> {
    let sources = value.as_array()?;
    if sources.is_empty() || sources.len() > 64 {
        return None;
    }
    let mut ids = std::collections::HashSet::new();
    sources
        .iter()
        .map(|source| {
            let source = source.as_object()?;
            if source
                .keys()
                .any(|key| !matches!(key.as_str(), "id" | "data"))
            {
                return None;
            }
            let id = nonempty_string(source, "id")?;
            if !valid_card_identifier(&id) || !ids.insert(id.clone()) {
                return None;
            }
            let data = source.get("data")?.as_object()?;
            let data_type = data.get("type")?.as_str()?;
            let format = data.get("format")?.as_str()?;
            if format != "geojson" {
                return None;
            }
            let data = match data_type {
                "mcp_resource" => {
                    if data
                        .keys()
                        .any(|key| !matches!(key.as_str(), "type" | "server" | "uri" | "format"))
                    {
                        return None;
                    }
                    let server = nonempty_string(data, "server")?;
                    if !valid_card_identifier(&server) || server.starts_with("mcp__") {
                        return None;
                    }
                    let uri = nonempty_string(data, "uri")?;
                    if !valid_geojson_resource_uri(&uri) {
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
                        .any(|key| !matches!(key.as_str(), "type" | "geojson" | "format"))
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
            Some(json!({ "id": id, "data": data }))
        })
        .collect()
}

fn project_map_layers(value: &Value) -> Option<Vec<Value>> {
    let layers = value.as_array()?;
    if layers.is_empty() || layers.len() > 128 {
        return None;
    }
    let mut ids = std::collections::HashSet::new();
    layers
        .iter()
        .map(|layer| {
            let layer = layer.as_object()?;
            const FIELDS: &[&str] = &[
                "id",
                "source",
                "geometry",
                "label_property",
                "hover",
                "style",
            ];
            if layer.keys().any(|key| !FIELDS.contains(&key.as_str())) {
                return None;
            }
            let id = nonempty_string(layer, "id")?;
            if !valid_card_identifier(&id) || !ids.insert(id.clone()) {
                return None;
            }
            let source = nonempty_string(layer, "source")?;
            if !valid_card_identifier(&source) {
                return None;
            }
            let geometry = nonempty_string(layer, "geometry")?;
            if !matches!(geometry.as_str(), "point" | "line" | "polygon") {
                return None;
            }
            let mut projected = Map::from_iter([
                ("id".to_string(), Value::String(id)),
                ("source".to_string(), Value::String(source)),
                ("geometry".to_string(), Value::String(geometry.clone())),
            ]);
            if let Some(value) = optional_string(layer, "label_property")? {
                if !valid_map_property_name(&value) {
                    return None;
                }
                projected.insert("label_property".to_string(), Value::String(value));
            }
            if let Some(hover) = layer.get("hover") {
                projected.insert("hover".to_string(), project_layer_hover(hover)?);
            }
            projected.insert(
                "style".to_string(),
                project_layer_style(&geometry, layer.get("style")?)?,
            );
            Some(Value::Object(projected))
        })
        .collect()
}

fn project_layer_style(geometry: &str, value: &Value) -> Option<Value> {
    let style = value.as_object()?;
    let allowed = match geometry {
        "point" => &[
            "color",
            "opacity",
            "radius",
            "size",
            "shape",
            "icon",
            "stroke_color",
            "stroke_width",
            "stroke_opacity",
        ][..],
        "line" => &["color", "opacity", "width", "dash", "cap", "join"][..],
        "polygon" => &[
            "fill_color",
            "fill_opacity",
            "stroke_color",
            "stroke_width",
            "stroke_opacity",
            "stroke_dash",
        ][..],
        _ => return None,
    };
    if style.keys().any(|key| !allowed.contains(&key.as_str())) {
        return None;
    }
    if geometry == "point" {
        let has_icon = style.get("icon").is_some();
        let has_builtin_style = [
            "color",
            "radius",
            "size",
            "shape",
            "stroke_color",
            "stroke_width",
            "stroke_opacity",
        ]
        .iter()
        .any(|key| style.get(*key).is_some());
        if has_icon && has_builtin_style {
            return None;
        }
        if style.get("radius").is_some() && style.get("size").is_some() {
            return None;
        }
        if style.get("radius").is_some()
            && !matches!(
                style.get("shape").and_then(Value::as_str),
                None | Some("circle")
            )
        {
            return None;
        }
    }
    let mut projected = Map::new();
    for key in ["color", "stroke_color", "fill_color"] {
        if let Some(value) = optional_string(style, key)? {
            if !valid_css_color(&value) {
                return None;
            }
            projected.insert(key.to_string(), Value::String(value));
        }
    }
    for (key, minimum, maximum) in [
        ("opacity", 0.0, 1.0),
        ("stroke_opacity", 0.0, 1.0),
        ("fill_opacity", 0.0, 1.0),
        ("radius", 1.0, 64.0),
        ("size", 4.0, 128.0),
        ("width", 0.5, 32.0),
        ("stroke_width", 0.0, 32.0),
    ] {
        if let Some(value) = style.get(key) {
            projected.insert(
                key.to_string(),
                json!(bounded_number(value, minimum, maximum)?),
            );
        }
    }
    for key in ["dash", "stroke_dash"] {
        if let Some(value) = style.get(key) {
            projected.insert(key.to_string(), project_dash(value)?);
        }
    }
    for (key, allowed) in [
        ("cap", &["butt", "round", "square"][..]),
        ("join", &["bevel", "round", "miter"][..]),
        (
            "shape",
            &["circle", "square", "diamond", "triangle", "pin"][..],
        ),
    ] {
        if let Some(value) = style.get(key) {
            let value = value.as_str()?;
            if !allowed.contains(&value) {
                return None;
            }
            projected.insert(key.to_string(), Value::String(value.to_string()));
        }
    }
    if let Some(icon) = style.get("icon") {
        projected.insert("icon".to_string(), project_point_icon(icon)?);
    }
    Some(Value::Object(projected))
}

fn project_point_icon(value: &Value) -> Option<Value> {
    let icon = value.as_object()?;
    const FIELDS: &[&str] = &["url", "scale", "anchor", "rotation", "allow_overlap"];
    if icon.keys().any(|key| !FIELDS.contains(&key.as_str())) {
        return None;
    }
    let url = nonempty_string(icon, "url")?;
    if !valid_map_icon_url(&url) {
        return None;
    }
    let mut projected = Map::from_iter([("url".to_string(), Value::String(url))]);
    if let Some(scale) = icon.get("scale") {
        projected.insert(
            "scale".to_string(),
            json!(bounded_number(scale, 0.05, 8.0)?),
        );
    }
    if let Some(anchor) = icon.get("anchor") {
        let anchor = anchor.as_str()?;
        if !matches!(
            anchor,
            "center"
                | "top"
                | "bottom"
                | "left"
                | "right"
                | "top-left"
                | "top-right"
                | "bottom-left"
                | "bottom-right"
        ) {
            return None;
        }
        projected.insert("anchor".to_string(), Value::String(anchor.to_string()));
    }
    if let Some(rotation) = icon.get("rotation") {
        projected.insert(
            "rotation".to_string(),
            json!(bounded_number(rotation, -360.0, 360.0)?),
        );
    }
    if let Some(allow_overlap) = icon.get("allow_overlap") {
        projected.insert(
            "allow_overlap".to_string(),
            Value::Bool(allow_overlap.as_bool()?),
        );
    }
    Some(Value::Object(projected))
}

fn project_layer_hover(value: &Value) -> Option<Value> {
    let hover = value.as_object()?;
    const FIELDS: &[&str] = &["title_property", "fields"];
    if hover.keys().any(|key| !FIELDS.contains(&key.as_str())) {
        return None;
    }
    let title_property = optional_string(hover, "title_property")?;
    if title_property
        .as_deref()
        .is_some_and(|property| !valid_map_property_name(property))
    {
        return None;
    }
    let fields = match hover.get("fields") {
        Some(fields) => Some(fields.as_array()?),
        None => None,
    };
    let mut seen = std::collections::HashSet::new();
    let projected_fields = match fields {
        Some(fields) if fields.len() <= 16 => fields
            .iter()
            .map(|field| {
                let field = field.as_object()?;
                if field
                    .keys()
                    .any(|key| !matches!(key.as_str(), "property" | "label"))
                {
                    return None;
                }
                let property = nonempty_string(field, "property")?;
                if !valid_map_property_name(&property) || !seen.insert(property.clone()) {
                    return None;
                }
                let mut projected =
                    Map::from_iter([("property".to_string(), Value::String(property))]);
                if let Some(label) = optional_string(field, "label")? {
                    if !valid_map_property_name(&label) {
                        return None;
                    }
                    projected.insert("label".to_string(), Value::String(label));
                }
                Some(Value::Object(projected))
            })
            .collect::<Option<Vec<_>>>()?,
        Some(_) => return None,
        None => Vec::new(),
    };
    if title_property.is_none() && projected_fields.is_empty() {
        return None;
    }
    let mut projected = Map::from_iter([("fields".to_string(), Value::Array(projected_fields))]);
    if let Some(title_property) = title_property {
        projected.insert("title_property".to_string(), Value::String(title_property));
    }
    Some(Value::Object(projected))
}

fn project_dash(value: &Value) -> Option<Value> {
    let values = value.as_array()?;
    if values.is_empty() || values.len() > 8 {
        return None;
    }
    Some(Value::Array(
        values
            .iter()
            .map(|value| bounded_number(value, 0.1, 64.0).map(Value::from))
            .collect::<Option<Vec<_>>>()?,
    ))
}

fn project_map_legend(value: &Value) -> Option<Value> {
    let legend = value.as_object()?;
    if legend
        .keys()
        .any(|key| !matches!(key.as_str(), "title" | "items"))
    {
        return None;
    }
    let items = legend.get("items")?.as_array()?;
    if items.is_empty() || items.len() > 32 {
        return None;
    }
    let items = items
        .iter()
        .map(|item| {
            let item = item.as_object()?;
            if item
                .keys()
                .any(|key| !matches!(key.as_str(), "label" | "color"))
            {
                return None;
            }
            let label = nonempty_string(item, "label")?;
            let color = nonempty_string(item, "color")?;
            valid_css_color(&color).then(|| json!({ "label": label, "color": color }))
        })
        .collect::<Option<Vec<_>>>()?;
    let mut projected = Map::from_iter([("items".to_string(), Value::Array(items))]);
    if let Some(title) = optional_string(legend, "title")? {
        projected.insert("title".to_string(), Value::String(title));
    }
    Some(Value::Object(projected))
}

fn bounded_number(value: &Value, minimum: f64, maximum: f64) -> Option<f64> {
    let value = value.as_f64()?;
    value
        .is_finite()
        .then_some(value)
        .filter(|value| (minimum..=maximum).contains(value))
}

fn valid_css_color(value: &str) -> bool {
    let bytes = value.as_bytes();
    (bytes.len() == 7 && bytes[0] == b'#' && bytes[1..].iter().all(u8::is_ascii_hexdigit))
        || matches!(
            value,
            "red"
                | "orange"
                | "yellow"
                | "green"
                | "blue"
                | "purple"
                | "pink"
                | "gray"
                | "black"
                | "white"
        )
}

fn valid_map_property_name(value: &str) -> bool {
    !value.is_empty() && value.chars().count() <= 128 && !value.chars().any(char::is_control)
}

fn valid_map_icon_url(value: &str) -> bool {
    if value.len() > 2048
        || !value.starts_with("https://")
        || value.chars().any(char::is_whitespace)
    {
        return false;
    }
    let remainder = &value["https://".len()..];
    let authority = remainder.split('/').next().unwrap_or_default();
    let path = value
        .split(|character| character == '?' || character == '#')
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase();
    !authority.is_empty()
        && [".png", ".jpg", ".jpeg", ".webp"]
            .iter()
            .any(|extension| path.ends_with(extension))
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

fn valid_geojson_resource_uri(value: &str) -> bool {
    value
        .strip_prefix("maps-data://geojson/")
        .is_some_and(valid_card_identifier)
}

fn reply_artifact_link(content: &Value) -> Option<(String, Option<String>, Option<i64>)> {
    let content = content.as_object()?;
    if content.get("type")?.as_str()? != "resource_link" {
        return None;
    }
    let uri = content.get("uri")?.as_str()?.trim();
    if !valid_geojson_resource_uri(uri) {
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
    let expected_size = content
        .get("size")
        .and_then(Value::as_u64)
        .and_then(|value| i64::try_from(value).ok());
    Some((
        uri.to_string(),
        mime_type.map(str::to_string),
        expected_size,
    ))
}

fn reply_artifact_candidates(
    item: &Map<String, Value>,
) -> impl Iterator<Item = ReplyArtifactCandidate> + '_ {
    item.get("result")
        .and_then(Value::as_object)
        .and_then(|result| result.get("content"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(reply_artifact_link)
        .map(|(uri, mime_type, expected_size)| ReplyArtifactCandidate {
            uri,
            mime_type,
            expected_size,
        })
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

async fn register_reply_artifacts(
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
    let Some(turn_id) = event.turn_id.as_deref() else {
        return Ok(());
    };
    let Some(item_id) = event.item_id.as_deref() else {
        return Ok(());
    };
    let Some(server) = event
        .payload
        .pointer("/data/server")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return Ok(());
    };
    for artifact in &event.reply_artifacts {
        sqlx::query(
            "INSERT INTO reply_artifacts (
                organization_id, run_id, thread_id, turn_id, producer_item_id,
                source_server, source_uri, mime_type, expected_size
             ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
             ON CONFLICT (run_id, thread_id, source_server, source_uri) DO UPDATE SET
                state = CASE
                    WHEN reply_artifacts.producer_item_id = EXCLUDED.producer_item_id
                    THEN reply_artifacts.state
                    ELSE 'failed'
                END,
                updated_at = now()",
        )
        .bind(organization_id)
        .bind(run_id)
        .bind(&event.thread_id)
        .bind(turn_id)
        .bind(item_id)
        .bind(server)
        .bind(&artifact.uri)
        .bind(&artifact.mime_type)
        .bind(artifact.expected_size)
        .execute(&mut **transaction)
        .await
        .map_err(|error| format!("reply Artifact registration error: {error}"))?;
    }
    Ok(())
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
        "map.v2" => {
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
            "SELECT id, mime_type FROM reply_artifacts
             WHERE run_id = $1 AND thread_id = $2 AND source_server = $3 AND source_uri = $4
               AND producer_item_id <> $5 AND state IN ('pending', 'ready')",
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
                row.get::<Option<String>, _>("mime_type"),
            ),
        );
    }
    replace_map_payload_resource_refs(renderer_payload, run_id, &resolved);
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
    let sources = map_payload.get("sources")?.as_array()?;
    let resource_refs = sources
        .iter()
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
    run_id: Uuid,
    resolved: &std::collections::HashMap<(String, String), (Uuid, Option<String>)>,
) {
    let Some(sources) = map_payload.get_mut("sources").and_then(Value::as_array_mut) else {
        return;
    };
    for source in sources {
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
            "url": format!("/api/runs/{run_id}/artifacts/{artifact_id}"),
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
        Value::String(value) if is_path_key(key) => Value::String(safe_path(value)),
        Value::String(value) if value.starts_with("data:") => {
            Value::String("[embedded-data]".to_string())
        }
        _ => value.clone(),
    }
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
                            "kind": "map.v2",
                            "payload": {
                                "title": "Locations",
                                "intent": "visualization",
                                "status": "ready",
                                "summary": "Two locations",
                                "viewport": {
                                    "mode": "camera",
                                    "center": [-122.08, 37.42],
                                    "zoom": 10
                                },
                                "sources": [{
                                    "id": "locations",
                                    "data": {
                                        "type": "inline",
                                        "format": "geojson",
                                        "geojson": {
                                            "type": "FeatureCollection",
                                            "features": []
                                        }
                                    }
                                }],
                                "layers": [{
                                    "id": "points",
                                    "source": "locations",
                                    "geometry": "point",
                                    "label_property": "label",
                                    "hover": {
                                        "title_property": "label",
                                        "fields": [{
                                            "property": "population",
                                            "label": "Population"
                                        }]
                                    },
                                    "style": {
                                        "color": "#ef4444",
                                        "opacity": 0.8,
                                        "shape": "pin",
                                        "size": 24,
                                        "stroke_color": "#ffffff",
                                        "stroke_width": 2
                                    }
                                }]
                            }
                        }
                    },
                    "embed": {
                        "syntax": "codex-inline-vis.artifact.v1",
                        "code": "::codex-inline-vis{artifact=\"map-7d67b30d\"}"
                    }
                }
            }
        });

        let artifact = project_inline_visualization_artifact(item.as_object().unwrap()).unwrap();
        assert_eq!(artifact.artifact_ref, "map-7d67b30d");
        assert_eq!(artifact.renderer_kind, "map.v2");
        assert_eq!(artifact.renderer_payload["title"], "Locations");
        assert_eq!(
            artifact.renderer_payload["viewport"]["zoom"].as_f64(),
            Some(10.0)
        );
        assert_eq!(
            artifact.renderer_payload["layers"][0]["style"]["shape"],
            "pin"
        );
        assert_eq!(
            artifact.renderer_payload["layers"][0]["hover"]["fields"][0]["property"],
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
    fn projects_only_safe_map_point_icons_and_hover_content() {
        let icon = json!({
            "url": "https://cdn.example.com/marker.webp?version=2",
            "scale": 0.75,
            "anchor": "bottom",
            "rotation": 15,
            "allow_overlap": true
        });
        assert_eq!(
            project_point_icon(&icon).unwrap()["url"],
            "https://cdn.example.com/marker.webp?version=2"
        );
        assert!(project_point_icon(&json!({
            "url": "http://cdn.example.com/marker.png"
        }))
        .is_none());
        assert!(project_point_icon(&json!({
            "url": "https://cdn.example.com/marker.svg"
        }))
        .is_none());

        let hover = project_layer_hover(&json!({
            "title_property": "label",
            "fields": [{
                "property": "population",
                "label": "Population"
            }]
        }))
        .unwrap();
        assert_eq!(hover["title_property"], "label");
        assert_eq!(hover["fields"][0]["property"], "population");
        assert!(project_layer_hover(&json!({
            "fields": [{
                "property": "population"
            }, {
                "property": "population"
            }]
        }))
        .is_none());
    }

    #[test]
    fn rejects_text_legacy_cards_and_mismatched_embed_codes() {
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
                    "kind": "map.v2",
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
                            "kind": "map.v2",
                            "payload": {
                                "title": "Map",
                                "intent": "visualization",
                                "status": "ready",
                                "viewport": { "mode": "fit" },
                                "sources": [{
                                    "id": "data",
                                    "data": {
                                        "type": "inline",
                                        "format": "geojson",
                                        "geojson": {
                                            "type": "FeatureCollection",
                                            "features": []
                                        }
                                    }
                                }],
                                "layers": [{
                                    "id": "points",
                                    "source": "data",
                                    "geometry": "point",
                                    "style": {}
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
                            "kind": "map.v2",
                            "payload": {
                                "title": "Large inline source",
                                "intent": "visualization",
                                "status": "ready",
                                "summary": "x".repeat(32 * 1024),
                                "viewport": { "mode": "fit" },
                                "sources": [{
                                    "id": "data",
                                    "data": {
                                        "type": "inline",
                                        "format": "geojson",
                                        "geojson": {
                                            "type": "FeatureCollection",
                                            "features": []
                                        }
                                    }
                                }],
                                "layers": [{
                                    "id": "points",
                                    "source": "data",
                                    "geometry": "point",
                                    "style": {}
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
        let run_id = Uuid::parse_str("975f1f1c-4b58-47ad-a12c-c32aeae566e7").unwrap();
        let artifact_id = Uuid::parse_str("8e98ff2f-82ee-4cc9-a3e6-2974debf8666").unwrap();
        let resource_uri = "maps-data://geojson/map-data-one";
        let mut map_payload = json!({
            "sources": [{
                "id": "locations",
                "data": {
                    "type": "mcp_resource",
                    "server": "map_utils",
                    "uri": resource_uri,
                    "format": "geojson"
                }
            }]
        });
        let resolved = std::collections::HashMap::from([(
            ("map_utils".to_string(), resource_uri.to_string()),
            (artifact_id, Some("application/geo+json".to_string())),
        )]);

        replace_map_payload_resource_refs(&mut map_payload, run_id, &resolved);

        assert_eq!(
            map_payload["sources"][0]["data"],
            json!({
                "type": "artifact",
                "format": "geojson",
                "artifact_id": artifact_id,
                "mime_type": "application/geo+json",
                "url": format!("/api/runs/{run_id}/artifacts/{artifact_id}")
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
    fn accepts_only_valid_geojson_resource_links() {
        let link = json!({
            "type": "resource_link",
            "name": "map-data-one",
            "title": "Maps GeoJSON",
            "uri": "maps-data://geojson/map-data-one",
            "mimeType": "application/geo+json",
            "size": 128
        });
        let projected = reply_artifact_link(&link).expect("valid link");
        assert_eq!(projected.0, "maps-data://geojson/map-data-one");
        assert_eq!(projected.1.as_deref(), Some("application/geo+json"));
        assert_eq!(projected.2, Some(128));

        let invalid_uri = json!({
            "type": "resource_link",
            "name": "map-data-one",
            "uri": "https://example.com/map-data-one",
            "mimeType": "application/geo+json"
        });
        assert!(reply_artifact_link(&invalid_uri).is_none());

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
        let artifacts = reply_artifact_candidates(item).collect::<Vec<_>>();
        assert_eq!(artifacts.len(), 1);
        assert_eq!(artifacts[0].uri, "maps-data://geojson/map-data-one");

        let public = project_item(item);
        let public_link = public.pointer("/result/content/0").unwrap();
        assert!(public_link.get("uri").is_none());
        assert!(public_link.get("_meta").is_none());
        assert!(public
            .pointer("/result/structuredContent/data_ref")
            .is_none());

        let model_visible_namespace = json!([{
            "id": "locations",
            "data": {
                "type": "mcp_resource",
                "server": "mcp__map_utils",
                "uri": "maps-data://geojson/map-data-one",
                "format": "geojson"
            }
        }]);
        assert!(project_map_sources(&model_visible_namespace).is_none());
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
        assert_eq!(status.event_type, "codex.unknown");
        assert_eq!(
            status.payload["data"]["sourceType"],
            "thread/status/changed"
        );
        assert_eq!(status.payload["data"]["status"]["type"], "active");

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
