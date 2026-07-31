use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use open_web_codex_adapter::{AuthorizedWorkspace, CodexAdapter};
use open_web_codex_platform_contracts::{error::PlatformError, ArtifactSummary};
use open_web_codex_platform_store::AppState;
use serde_json::Value;
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Row};
use uuid::Uuid;

use crate::event_projection::reconcile_materialized_intake_artifact;
use crate::middleware::auth::AuthenticatedUser;

type ApiError = (StatusCode, Json<PlatformError>);
type ApiResult<T> = Result<Json<T>, ApiError>;

// Artifact content is persisted server-side before it is presented as durable.
// This is a process-safety bound, not a business schema limit.
const MAX_ARTIFACT_BYTES: usize = 128 * 1024 * 1024;

pub async fn list_for_task(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(task_id): Path<Uuid>,
) -> ApiResult<Vec<ArtifactSummary>> {
    let task_exists = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(
            SELECT 1 FROM tasks WHERE id = $1 AND organization_id = $2
         )",
    )
    .bind(task_id)
    .bind(auth.organization_id)
    .fetch_one(&state.db)
    .await
    .map_err(database_error)?;
    if !task_exists {
        return Err(not_found());
    }

    let rows = sqlx::query(
        "SELECT artifact.id, artifact_grant.task_id, artifact.artifact_schema,
                artifact.display_name, artifact.mime_type, artifact.expected_size,
                artifact.byte_size, artifact.content_sha256, artifact.state,
                artifact.created_at, artifact.updated_at,
                provenance.producer_run_id, provenance.producer_thread_id,
                provenance.producer_turn_id, provenance.producer_item_id,
                projection.agent_role AS producer_agent_role
         FROM artifact_task_grants artifact_grant
         JOIN artifacts artifact ON artifact.id = artifact_grant.artifact_id
           AND artifact.organization_id = artifact_grant.organization_id
         JOIN LATERAL (
             SELECT producer_run_id, producer_thread_id, producer_turn_id,
                    producer_item_id
             FROM artifact_provenance
             WHERE artifact_id = artifact.id
               AND organization_id = artifact.organization_id
               AND producer_task_id = artifact_grant.task_id
             ORDER BY created_at DESC, producer_run_id DESC, producer_thread_id,
                      producer_turn_id, producer_item_id
             LIMIT 1
         ) provenance ON true
         LEFT JOIN runtime_agent_projections projection
           ON projection.organization_id = artifact.organization_id
          AND projection.root_run_id = provenance.producer_run_id
          AND projection.thread_id = provenance.producer_thread_id
         WHERE artifact_grant.task_id = $1
           AND artifact_grant.organization_id = $2
           AND artifact_grant.permission = 'read'
           AND artifact.retention_state = 'active'
         ORDER BY artifact.created_at, artifact.id",
    )
    .bind(task_id)
    .bind(auth.organization_id)
    .fetch_all(&state.db)
    .await
    .map_err(database_error)?;

    Ok(Json(rows.iter().map(artifact_summary).collect()))
}

pub async fn get(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(artifact_id): Path<Uuid>,
) -> ApiResult<ArtifactSummary> {
    let row = authorized_artifact_row(&state.db, auth.organization_id, artifact_id)
        .await?
        .ok_or_else(not_found)?;
    Ok(Json(artifact_summary(&row)))
}

pub async fn read_content(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(artifact_id): Path<Uuid>,
) -> ApiResult<Value> {
    let row = sqlx::query(
        "SELECT artifact.mime_type, artifact.state, artifact.content
         FROM artifacts artifact
         WHERE artifact.id = $1
           AND artifact.organization_id = $2
           AND artifact.retention_state = 'active'
           AND EXISTS (
               SELECT 1
               FROM artifact_task_grants artifact_grant
               JOIN tasks task ON task.id = artifact_grant.task_id
                 AND task.organization_id = artifact_grant.organization_id
               WHERE artifact_grant.artifact_id = artifact.id
                 AND artifact_grant.organization_id = $2
                 AND artifact_grant.permission = 'read'
           )",
    )
    .bind(artifact_id)
    .bind(auth.organization_id)
    .fetch_optional(&state.db)
    .await
    .map_err(database_error)?
    .ok_or_else(not_found)?;

    let state: String = row.get("state");
    if state != "ready" {
        return Err((
            StatusCode::CONFLICT,
            Json(PlatformError::bad_request(format!(
                "Artifact content is not ready (state: {state})"
            ))),
        ));
    }
    let mime_type: String = row.get("mime_type");
    if !supported_json_mime(&mime_type) {
        return Err(bad_gateway("Artifact content type is unsupported"));
    }
    let content: Vec<u8> = row.get("content");
    if content.len() > MAX_ARTIFACT_BYTES {
        return Err(payload_too_large());
    }
    let value = serde_json::from_slice(&content)
        .map_err(|_| bad_gateway("Artifact did not contain valid JSON"))?;
    Ok(Json(value))
}

pub(crate) async fn recover_and_materialize_pending(db: PgPool, adapter: Arc<dyn CodexAdapter>) {
    if let Err(error) =
        sqlx::query("UPDATE artifacts SET state = 'pending' WHERE state = 'materializing'")
            .execute(&db)
            .await
    {
        tracing::warn!(%error, "Artifact materialization recovery failed");
        return;
    }
    let ids = match sqlx::query_scalar::<_, Uuid>(
        "SELECT id FROM artifacts
         WHERE state = 'pending' AND retention_state = 'active'
         ORDER BY created_at, id",
    )
    .fetch_all(&db)
    .await
    {
        Ok(ids) => ids,
        Err(error) => {
            tracing::warn!(%error, "pending Artifact lookup failed");
            return;
        }
    };
    materialize_artifacts(db, adapter, ids).await;
}

pub(crate) async fn materialize_artifacts(
    db: PgPool,
    adapter: Arc<dyn CodexAdapter>,
    artifact_ids: Vec<Uuid>,
) {
    for artifact_id in artifact_ids {
        if let Err(error) = materialize_artifact(&db, adapter.as_ref(), artifact_id).await {
            tracing::warn!(%error, %artifact_id, "Artifact materialization failed");
        }
    }
}

async fn materialize_artifact(
    db: &PgPool,
    adapter: &dyn CodexAdapter,
    artifact_id: Uuid,
) -> Result<(), String> {
    let row = sqlx::query(
        "WITH claimed AS (
             UPDATE artifacts
             SET state = 'materializing', updated_at = now()
             WHERE id = $1 AND state = 'pending' AND retention_state = 'active'
             RETURNING id, source_server, source_uri, mime_type, expected_size
         )
         SELECT claimed.source_server, claimed.source_uri, claimed.mime_type,
                claimed.expected_size, provenance.producer_thread_id,
                workspace.id AS workspace_id, workspace.root_path
         FROM claimed
         JOIN LATERAL (
             SELECT producer_run_id, producer_thread_id
             FROM artifact_provenance
             WHERE artifact_id = claimed.id
             ORDER BY created_at, producer_run_id, producer_thread_id
             LIMIT 1
         ) provenance ON true
         JOIN runs run ON run.id = provenance.producer_run_id
         JOIN workspaces workspace ON workspace.id = run.workspace_id
         WHERE workspace.state IN ('ready', 'retained')",
    )
    .bind(artifact_id)
    .fetch_optional(db)
    .await
    .map_err(|error| format!("Artifact claim failed: {error}"))?;
    let Some(row) = row else {
        return Ok(());
    };

    let expected_size: Option<i64> = row.get("expected_size");
    if expected_size.is_some_and(|size| {
        usize::try_from(size)
            .map(|size| size > MAX_ARTIFACT_BYTES)
            .unwrap_or(true)
    }) {
        mark_failed(db, artifact_id, "size_limit").await?;
        return Err("Artifact exceeds the materialization size limit".to_string());
    }

    let source_server: String = row.get("source_server");
    let source_uri: String = row.get("source_uri");
    let declared_mime: String = row.get("mime_type");
    let workspace = AuthorizedWorkspace {
        id: row.get::<Uuid, _>("workspace_id").to_string(),
        root: row.get::<String, _>("root_path").into(),
    };
    let thread_id: String = row.get("producer_thread_id");
    let response = match adapter
        .read_mcp_resource(&workspace, &thread_id, &source_server, &source_uri)
        .await
    {
        Ok(response) => response,
        Err(error) => {
            mark_failed(db, artifact_id, "runtime_read_failed").await?;
            return Err(format!("Runtime MCP Resource read failed: {error}"));
        }
    };
    let (bytes, response_mime) = match resource_bytes(&response, &source_uri) {
        Ok(value) => value,
        Err(error) => {
            mark_failed(db, artifact_id, "invalid_resource").await?;
            return Err(error);
        }
    };
    if bytes.len() > MAX_ARTIFACT_BYTES {
        mark_failed(db, artifact_id, "size_limit").await?;
        return Err("Artifact exceeds the materialization size limit".to_string());
    }
    if let Some(response_mime) = response_mime.as_deref() {
        if response_mime != declared_mime {
            mark_failed(db, artifact_id, "mime_mismatch").await?;
            return Err("MCP Resource content type changed during materialization".to_string());
        }
    }
    if supported_json_mime(&declared_mime) && serde_json::from_slice::<Value>(&bytes).is_err() {
        mark_failed(db, artifact_id, "invalid_json").await?;
        return Err("MCP Resource did not contain valid JSON".to_string());
    }

    let digest = hex::encode(Sha256::digest(&bytes));
    let updated = sqlx::query(
        "UPDATE artifacts
         SET content = $1, byte_size = $2, content_sha256 = $3, state = 'ready',
             failure_code = NULL, updated_at = now()
         WHERE id = $4 AND state = 'materializing'",
    )
    .bind(&bytes)
    .bind(i64::try_from(bytes.len()).map_err(|_| "Artifact size overflow".to_string())?)
    .bind(digest)
    .bind(artifact_id)
    .execute(db)
    .await
    .map_err(|error| format!("Artifact persistence failed: {error}"))?;
    if updated.rows_affected() != 1 {
        return Err("Artifact state changed during materialization".to_string());
    }
    reconcile_materialized_intake_artifact(db, artifact_id).await?;
    Ok(())
}

async fn mark_failed(db: &PgPool, artifact_id: Uuid, failure_code: &str) -> Result<(), String> {
    sqlx::query(
        "UPDATE artifacts
         SET state = 'failed', failure_code = $1, updated_at = now()
         WHERE id = $2 AND state = 'materializing'",
    )
    .bind(failure_code)
    .bind(artifact_id)
    .execute(db)
    .await
    .map_err(|error| format!("Artifact failure persistence failed: {error}"))?;
    Ok(())
}

fn resource_bytes(
    response: &Value,
    expected_uri: &str,
) -> Result<(Vec<u8>, Option<String>), String> {
    let contents = response
        .get("contents")
        .and_then(Value::as_array)
        .ok_or_else(|| "mcpServer/resource/read omitted contents".to_string())?;
    let content = contents
        .iter()
        .find(|content| content.get("uri").and_then(Value::as_str) == Some(expected_uri))
        .ok_or_else(|| "MCP Resource response did not match the requested URI".to_string())?;
    let mime_type = content
        .get("mimeType")
        .and_then(Value::as_str)
        .map(str::to_string);
    if let Some(text) = content.get("text").and_then(Value::as_str) {
        return Ok((text.as_bytes().to_vec(), mime_type));
    }
    if let Some(blob) = content.get("blob").and_then(Value::as_str) {
        return BASE64
            .decode(blob)
            .map(|bytes| (bytes, mime_type))
            .map_err(|_| "MCP Resource blob was not valid base64".to_string());
    }
    Err("MCP Resource content was unsupported".to_string())
}

async fn authorized_artifact_row(
    db: &PgPool,
    organization_id: Uuid,
    artifact_id: Uuid,
) -> Result<Option<sqlx::postgres::PgRow>, ApiError> {
    sqlx::query(
        "SELECT artifact.id, provenance.task_id, artifact.artifact_schema,
                artifact.display_name, artifact.mime_type, artifact.expected_size,
                artifact.byte_size, artifact.content_sha256, artifact.state,
                artifact.created_at, artifact.updated_at,
                provenance.producer_run_id, provenance.producer_thread_id,
                provenance.producer_turn_id, provenance.producer_item_id,
                projection.agent_role AS producer_agent_role
         FROM artifacts artifact
         JOIN LATERAL (
             SELECT artifact_grant.task_id, provenance.producer_run_id,
                    provenance.producer_thread_id, provenance.producer_turn_id,
                    provenance.producer_item_id
             FROM artifact_task_grants artifact_grant
             JOIN artifact_provenance provenance
               ON provenance.artifact_id = artifact_grant.artifact_id
              AND provenance.organization_id = artifact_grant.organization_id
              AND provenance.producer_task_id = artifact_grant.task_id
             WHERE artifact_grant.artifact_id = artifact.id
               AND artifact_grant.organization_id = artifact.organization_id
               AND artifact_grant.permission = 'read'
             ORDER BY artifact_grant.created_at DESC, artifact_grant.task_id,
                      provenance.created_at DESC, provenance.producer_run_id DESC,
                      provenance.producer_thread_id, provenance.producer_turn_id,
                      provenance.producer_item_id
             LIMIT 1
         ) provenance ON true
         LEFT JOIN runtime_agent_projections projection
           ON projection.organization_id = artifact.organization_id
          AND projection.root_run_id = provenance.producer_run_id
          AND projection.thread_id = provenance.producer_thread_id
         WHERE artifact.id = $1
           AND artifact.organization_id = $2
           AND artifact.retention_state = 'active'",
    )
    .bind(artifact_id)
    .bind(organization_id)
    .fetch_optional(db)
    .await
    .map_err(database_error)
}

fn artifact_summary(row: &sqlx::postgres::PgRow) -> ArtifactSummary {
    ArtifactSummary {
        id: row.get("id"),
        task_id: row.get("task_id"),
        artifact_schema: row.get("artifact_schema"),
        display_name: row.get("display_name"),
        mime_type: row.get("mime_type"),
        expected_size: row.get("expected_size"),
        byte_size: row.get("byte_size"),
        content_sha256: row.get("content_sha256"),
        state: row.get("state"),
        producer_run_id: row.get("producer_run_id"),
        producer_thread_id: row.get("producer_thread_id"),
        producer_turn_id: row.get("producer_turn_id"),
        producer_item_id: row.get("producer_item_id"),
        producer_agent_role: row.get("producer_agent_role"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    }
}

fn supported_json_mime(value: &str) -> bool {
    matches!(value, "application/json" | "application/geo+json")
}

fn not_found() -> ApiError {
    (
        StatusCode::NOT_FOUND,
        Json(PlatformError::not_found("Artifact was not found")),
    )
}

fn payload_too_large() -> ApiError {
    (
        StatusCode::PAYLOAD_TOO_LARGE,
        Json(PlatformError::bad_request(
            "Artifact exceeds the server safety limit",
        )),
    )
}

fn bad_gateway(message: &str) -> ApiError {
    (
        StatusCode::BAD_GATEWAY,
        Json(PlatformError::internal(message)),
    )
}

fn database_error(_error: sqlx::Error) -> ApiError {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(PlatformError::internal("Database operation failed")),
    )
}

#[cfg(test)]
mod tests {
    use super::{resource_bytes, supported_json_mime};
    use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
    use serde_json::json;

    #[test]
    fn reads_text_and_blob_mcp_resource_contents() {
        let (text, mime) = resource_bytes(
            &json!({
                "contents": [{
                    "uri": "supply-chain://resources/one",
                    "mimeType": "application/json",
                    "text": "{\"schema_version\":\"network_snapshot.v1\"}"
                }]
            }),
            "supply-chain://resources/one",
        )
        .expect("text Resource");
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&text).unwrap()["schema_version"],
            "network_snapshot.v1"
        );
        assert_eq!(mime.as_deref(), Some("application/json"));

        let blob = BASE64.encode(br#"{"type":"FeatureCollection","features":[]}"#);
        let (decoded, mime) = resource_bytes(
            &json!({
                "contents": [{
                    "uri": "maps-data://geojson/one",
                    "mimeType": "application/geo+json",
                    "blob": blob
                }]
            }),
            "maps-data://geojson/one",
        )
        .expect("blob Resource");
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&decoded).unwrap()["type"],
            "FeatureCollection"
        );
        assert_eq!(mime.as_deref(), Some("application/geo+json"));
    }

    #[test]
    fn limits_browser_content_to_typed_json_artifacts() {
        assert!(supported_json_mime("application/json"));
        assert!(supported_json_mime("application/geo+json"));
        assert!(!supported_json_mime("text/html"));
    }
}
