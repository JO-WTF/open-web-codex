use std::sync::Arc;

use axum::{
    body::Body,
    extract::{Path, State},
    http::{header, HeaderName, HeaderValue, Response, StatusCode},
    Json,
};
use open_web_codex_git_runtime::GitRuntime;
use open_web_codex_platform_contracts::{
    error::PlatformError, ArtifactFailureSummary, ArtifactState, ArtifactSummary, RunEvent,
};
use open_web_codex_platform_store::{AppState, LiveEvent};
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Row};
use tokio::sync::broadcast::Sender;
use uuid::Uuid;

use crate::event_projection::LiveProjection;
use crate::final_artifacts::{artifact_delivery_projection, validate_materialized_bundle};
use crate::middleware::auth::AuthenticatedUser;

type ApiError = (StatusCode, Json<PlatformError>);
type ApiResult<T> = Result<Json<T>, ApiError>;

// Artifact content is persisted server-side before it is presented as durable.
// This is a process-safety bound, not a business schema limit.
const MAX_ARTIFACT_BYTES: usize = 100 * 1024 * 1024;

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
                artifact.failure_code,
                artifact.created_at, artifact.updated_at,
                provenance.producer_run_id, provenance.producer_thread_id,
                provenance.producer_turn_id, provenance.producer_item_id,
                projection.agent_role AS producer_agent_role
         FROM artifact_task_grants artifact_grant
         JOIN tasks task ON task.id = artifact_grant.task_id
           AND task.organization_id = artifact_grant.organization_id
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
         ORDER BY artifact.created_at, artifact.id",
    )
    .bind(task_id)
    .bind(auth.organization_id)
    .fetch_all(&state.db)
    .await
    .map_err(database_error)?;

    let summaries = rows
        .iter()
        .map(artifact_summary)
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Json(summaries))
}

pub async fn get(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(artifact_id): Path<Uuid>,
) -> ApiResult<ArtifactSummary> {
    let row = authorized_artifact_row(&state.db, auth.organization_id, artifact_id)
        .await?
        .ok_or_else(not_found)?;
    Ok(Json(artifact_summary(&row)?))
}

pub async fn read_content(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(artifact_id): Path<Uuid>,
) -> Result<Response<Body>, ApiError> {
    let content = authorized_ready_content(&state.db, auth.organization_id, artifact_id).await?;
    artifact_content_response(artifact_id, content, false)
}

pub async fn download(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(artifact_id): Path<Uuid>,
) -> Result<Response<Body>, ApiError> {
    let content = authorized_ready_content(&state.db, auth.organization_id, artifact_id).await?;
    artifact_content_response(artifact_id, content, true)
}

fn artifact_content_response(
    artifact_id: Uuid,
    content: AuthorizedArtifactContent,
    download: bool,
) -> Result<Response<Body>, ApiError> {
    let content_type = match content.mime_type.as_str() {
        "application/json" => HeaderValue::from_static("application/json"),
        "application/geo+json" => HeaderValue::from_static("application/geo+json"),
        _ => return Err(bad_gateway("Artifact content type is unsupported")),
    };
    let content_length = HeaderValue::from_str(&content.bytes.len().to_string()).map_err(|_| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(PlatformError::internal(
                "Artifact download response could not be created",
            )),
        )
    })?;
    let mut response = Response::new(Body::from(content.bytes));
    let headers = response.headers_mut();
    headers.insert(header::CONTENT_TYPE, content_type);
    headers.insert(header::CONTENT_LENGTH, content_length);
    if download {
        let content_disposition = HeaderValue::from_str(&format!(
            "attachment; filename=\"artifact-{artifact_id}.json\""
        ))
        .map_err(|_| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(PlatformError::internal(
                    "Artifact download filename could not be created",
                )),
            )
        })?;
        headers.insert(header::CONTENT_DISPOSITION, content_disposition);
    }
    headers.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, no-store"),
    );
    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    headers.insert(
        HeaderName::from_static("cross-origin-resource-policy"),
        HeaderValue::from_static("same-origin"),
    );
    Ok(response)
}

struct AuthorizedArtifactContent {
    mime_type: String,
    bytes: Vec<u8>,
}

async fn authorized_ready_content(
    db: &PgPool,
    organization_id: Uuid,
    artifact_id: Uuid,
) -> Result<AuthorizedArtifactContent, ApiError> {
    let row = sqlx::query(
        "SELECT artifact.mime_type, artifact.state, artifact.content
         FROM artifacts artifact
         WHERE artifact.id = $1
           AND artifact.organization_id = $2
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
    .bind(organization_id)
    .fetch_optional(db)
    .await
    .map_err(database_error)?
    .ok_or_else(not_found)?;

    let persisted_state: String = row.get("state");
    let state = ArtifactState::from_persisted(&persisted_state)
        .ok_or_else(|| database_projection_error("Artifact state is invalid"))?;
    if !state.is_ready() {
        return Err((
            StatusCode::CONFLICT,
            Json(PlatformError::conflict("Artifact content is not ready")),
        ));
    }
    let mime_type: String = row.get("mime_type");
    if !supported_json_mime(&mime_type) {
        return Err(bad_gateway("Artifact content type is unsupported"));
    }
    let bytes: Vec<u8> = row.get("content");
    if bytes.len() > MAX_ARTIFACT_BYTES {
        return Err(payload_too_large());
    }
    Ok(AuthorizedArtifactContent { mime_type, bytes })
}

pub(crate) async fn recover_and_materialize_pending(
    db: PgPool,
    git: Arc<GitRuntime>,
    event_bus: Sender<LiveEvent>,
) {
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
         WHERE state = 'pending'
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
    materialize_artifacts(db, git, ids, event_bus).await;
}

pub(crate) async fn materialize_artifacts(
    db: PgPool,
    git: Arc<GitRuntime>,
    artifact_ids: Vec<Uuid>,
    event_bus: Sender<LiveEvent>,
) {
    for artifact_id in artifact_ids {
        match materialize_artifact(&db, git.as_ref(), artifact_id).await {
            Ok(projections) => {
                for projection in projections {
                    if event_bus
                        .send(LiveEvent {
                            organization_id: projection.organization_id,
                            payload: projection.payload,
                        })
                        .is_err()
                    {
                        tracing::debug!("event bus: no active receivers for Artifact change");
                    }
                }
            }
            Err(error) => {
                tracing::warn!(%error, %artifact_id, "Artifact materialization failed");
            }
        }
    }
}

async fn materialize_artifact(
    db: &PgPool,
    git: &GitRuntime,
    artifact_id: Uuid,
) -> Result<Vec<LiveProjection>, String> {
    let row = sqlx::query(
        "WITH claimed AS (
             UPDATE artifacts
             SET state = 'materializing', updated_at = now()
             WHERE id = $1 AND state = 'pending'
             RETURNING id, organization_id, workspace_id, source_relative_path,
                       artifact_schema, mime_type, expected_size
         )
         SELECT claimed.organization_id, claimed.workspace_id,
                claimed.source_relative_path, claimed.artifact_schema, claimed.mime_type,
                claimed.expected_size
         FROM claimed",
    )
    .bind(artifact_id)
    .fetch_optional(db)
    .await
    .map_err(|error| format!("Artifact claim failed: {error}"))?;
    let Some(row) = row else {
        return Ok(Vec::new());
    };

    let expected_size: i64 = row.get("expected_size");
    if usize::try_from(expected_size)
        .map(|size| size > MAX_ARTIFACT_BYTES)
        .unwrap_or(true)
    {
        return fail_materialization(db, artifact_id, "size_limit").await;
    }

    let workspace_id: Uuid = row.get("workspace_id");
    let relative_path: String = row.get("source_relative_path");
    let bytes = match git.download_file(workspace_id, &relative_path).await {
        Ok(download) => download.bytes,
        Err(error) => {
            tracing::warn!(%artifact_id, %error, "Workspace Artifact read failed");
            return fail_materialization(db, artifact_id, "workspace_read_failed").await;
        }
    };
    let declared_schema: String = row.get("artifact_schema");
    if let Err(code) = validate_downloaded_artifact(&declared_schema, expected_size, &bytes) {
        return fail_materialization(db, artifact_id, code).await;
    }

    let digest = hex::encode(Sha256::digest(&bytes));
    finish_materialization(
        db,
        artifact_id,
        MaterializationOutcome::Ready {
            bytes: &bytes,
            digest: &digest,
        },
    )
    .await
}

async fn fail_materialization(
    db: &PgPool,
    artifact_id: Uuid,
    failure_code: &str,
) -> Result<Vec<LiveProjection>, String> {
    finish_materialization(
        db,
        artifact_id,
        MaterializationOutcome::Failed(failure_code),
    )
    .await
}

enum MaterializationOutcome<'a> {
    Ready { bytes: &'a [u8], digest: &'a str },
    Failed(&'a str),
}

async fn finish_materialization(
    db: &PgPool,
    artifact_id: Uuid,
    outcome: MaterializationOutcome<'_>,
) -> Result<Vec<LiveProjection>, String> {
    let mut transaction = db
        .begin()
        .await
        .map_err(|error| format!("Artifact change transaction failed: {error}"))?;
    let updated = match outcome {
        MaterializationOutcome::Ready { bytes, digest } => sqlx::query(
            "UPDATE artifacts
             SET content = $1, byte_size = $2, content_sha256 = $3, state = 'ready',
                 failure_code = NULL, updated_at = now()
             WHERE id = $4 AND state = 'materializing'",
        )
        .bind(bytes)
        .bind(i64::try_from(bytes.len()).map_err(|_| "Artifact size overflow".to_string())?)
        .bind(digest)
        .bind(artifact_id)
        .execute(&mut *transaction)
        .await
        .map_err(|error| format!("Artifact persistence failed: {error}"))?,
        MaterializationOutcome::Failed(failure_code) => sqlx::query(
            "UPDATE artifacts
             SET state = 'failed', failure_code = $1, updated_at = now()
             WHERE id = $2 AND state = 'materializing'",
        )
        .bind(failure_code)
        .bind(artifact_id)
        .execute(&mut *transaction)
        .await
        .map_err(|error| format!("Artifact failure persistence failed: {error}"))?,
    };
    if updated.rows_affected() != 1 {
        transaction
            .commit()
            .await
            .map_err(|error| format!("Artifact unchanged state commit failed: {error}"))?;
        return Ok(Vec::new());
    }
    let row = sqlx::query(
        "SELECT artifact.organization_id, artifact.artifact_schema,
                artifact.display_name, artifact.mime_type, artifact.expected_size,
                artifact.byte_size, artifact.state, artifact.failure_code
         FROM artifacts artifact
         WHERE artifact.id = $1 AND artifact.state IN ('ready', 'failed')",
    )
    .bind(artifact_id)
    .fetch_optional(&mut *transaction)
    .await
    .map_err(|error| format!("Artifact change lookup failed: {error}"))?;
    let Some(row) = row else {
        return Err("Artifact terminal state could not be read".to_string());
    };
    let organization_id: Uuid = row.get("organization_id");
    let artifact = artifact_delivery_projection(
        artifact_id,
        &row.get::<String, _>("artifact_schema"),
        &row.get::<String, _>("display_name"),
        &row.get::<String, _>("mime_type"),
        row.get::<i64, _>("expected_size"),
        row.get::<Option<i64>, _>("byte_size"),
        &row.get::<String, _>("state"),
        row.get::<Option<String>, _>("failure_code").as_deref(),
    )?;
    let provenance_rows = sqlx::query(
        "SELECT DISTINCT provenance.producer_run_id, provenance.producer_thread_id,
                provenance.producer_turn_id, provenance.producer_item_id
         FROM artifact_task_grants artifact_grant
         JOIN tasks task
           ON task.id = artifact_grant.task_id
          AND task.organization_id = artifact_grant.organization_id
         JOIN artifact_provenance provenance
           ON provenance.artifact_id = artifact_grant.artifact_id
          AND provenance.organization_id = artifact_grant.organization_id
          AND provenance.producer_task_id = artifact_grant.task_id
         WHERE artifact_grant.artifact_id = $1
           AND artifact_grant.organization_id = $2
           AND artifact_grant.permission = 'read'
         ORDER BY provenance.producer_run_id, provenance.producer_thread_id,
                  provenance.producer_turn_id, provenance.producer_item_id",
    )
    .bind(artifact_id)
    .bind(organization_id)
    .fetch_all(&mut *transaction)
    .await
    .map_err(|error| format!("Artifact provenance scope lookup failed: {error}"))?;
    if provenance_rows.is_empty() {
        return Err("Artifact terminal state lost its authorized provenance".to_string());
    }
    let mut projections = Vec::with_capacity(provenance_rows.len());
    for provenance in provenance_rows {
        let run_id: Uuid = provenance.get("producer_run_id");
        let thread_id: String = provenance.get("producer_thread_id");
        let turn_id: String = provenance.get("producer_turn_id");
        let item_id: String = provenance.get("producer_item_id");
        let payload = serde_json::json!({
            "schemaVersion": 1,
            "itemType": "platformArtifactChanged",
            "data": {
                "sourceType": "platform/artifact/changed",
                "artifact": artifact.clone(),
            }
        });
        let persisted = sqlx::query(
            "INSERT INTO run_events (
                 run_id, event_type, projection_version, thread_id, turn_id, item_id, payload
             ) VALUES ($1, 'platform.artifact.changed', 1, $2, $3, $4, $5)
             RETURNING id, sequence, created_at",
        )
        .bind(run_id)
        .bind(&thread_id)
        .bind(&turn_id)
        .bind(&item_id)
        .bind(&payload)
        .fetch_one(&mut *transaction)
        .await
        .map_err(|error| format!("Artifact change event insert failed: {error}"))?;
        let event = RunEvent {
            id: persisted.get("id"),
            sequence: persisted.get("sequence"),
            run_id,
            event_type: "platform.artifact.changed".to_string(),
            projection_version: 1,
            thread_id: Some(thread_id),
            turn_id: Some(turn_id),
            item_id: Some(item_id),
            payload,
            created_at: persisted.get("created_at"),
        };
        let payload = serde_json::to_vec(&serde_json::json!({
            "type": "run.event",
            "version": 1,
            "event": event,
        }))
        .map_err(|error| format!("Artifact change encoding failed: {error}"))?;
        projections.push(LiveProjection {
            organization_id,
            payload,
            pending_artifact_ids: Vec::new(),
        });
    }
    transaction
        .commit()
        .await
        .map_err(|error| format!("Artifact change commit failed: {error}"))?;
    Ok(projections)
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
                artifact.failure_code,
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
             JOIN tasks task ON task.id = artifact_grant.task_id
              AND task.organization_id = artifact_grant.organization_id
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
           AND artifact.organization_id = $2",
    )
    .bind(artifact_id)
    .bind(organization_id)
    .fetch_optional(db)
    .await
    .map_err(database_error)
}

fn artifact_summary(row: &sqlx::postgres::PgRow) -> Result<ArtifactSummary, ApiError> {
    let id: Uuid = row.get("id");
    let persisted_state: String = row.get("state");
    let state = ArtifactState::from_persisted(&persisted_state)
        .ok_or_else(|| database_projection_error("Artifact state is invalid"))?;
    let failure = if matches!(state, ArtifactState::Failed) {
        let failure_code: String = row.get("failure_code");
        Some(ArtifactFailureSummary::from_persisted(&failure_code))
    } else {
        None
    };
    let (content_url, download_url) = if state.is_ready() {
        (
            Some(format!("/api/artifacts/{id}/content")),
            Some(format!("/api/artifacts/{id}/download")),
        )
    } else {
        (None, None)
    };
    Ok(ArtifactSummary {
        id,
        task_id: row.get("task_id"),
        artifact_schema: row.get("artifact_schema"),
        display_name: row.get("display_name"),
        mime_type: row.get("mime_type"),
        expected_size: row.get("expected_size"),
        byte_size: row.get("byte_size"),
        content_sha256: row.get("content_sha256"),
        state,
        failure,
        content_url,
        download_url,
        producer_run_id: row.get("producer_run_id"),
        producer_thread_id: row.get("producer_thread_id"),
        producer_turn_id: row.get("producer_turn_id"),
        producer_item_id: row.get("producer_item_id"),
        producer_agent_role: row.get("producer_agent_role"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    })
}

fn supported_json_mime(value: &str) -> bool {
    matches!(value, "application/json" | "application/geo+json")
}

fn validate_downloaded_artifact(
    declared_schema: &str,
    expected_size: i64,
    bytes: &[u8],
) -> Result<(), &'static str> {
    if bytes.len() > MAX_ARTIFACT_BYTES {
        return Err("size_limit");
    }
    if i64::try_from(bytes.len()).ok() != Some(expected_size) {
        return Err("size_mismatch");
    }
    validate_materialized_bundle(declared_schema, bytes)
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

fn database_projection_error(message: &str) -> ApiError {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(PlatformError::internal(message)),
    )
}

#[cfg(test)]
mod tests {
    use super::{supported_json_mime, validate_downloaded_artifact};
    use crate::middleware::auth::AuthenticatedUser;
    use axum::http::StatusCode;
    use open_web_codex_git_runtime::{GitRuntime, GitRuntimeConfig};
    use serde_json::{json, Value};
    use sha2::{Digest, Sha256};
    use sqlx::Row;
    use std::path::Path;
    use std::process::Command;
    use std::sync::Arc;
    use tempfile::TempDir;
    use uuid::Uuid;

    fn authenticated_user(organization_id: Uuid, user_id: Uuid) -> AuthenticatedUser {
        AuthenticatedUser {
            session_id: Uuid::now_v7(),
            user_id,
            name: "Artifact test".to_string(),
            username: format!("artifact-{user_id}"),
            email: format!("{user_id}@example.invalid"),
            role: "owner".to_string(),
            organization_id,
            organization_role: "owner".to_string(),
        }
    }

    #[test]
    fn limits_browser_content_to_typed_json_artifacts() {
        assert!(supported_json_mime("application/json"));
        assert!(supported_json_mime("application/geo+json"));
        assert!(!supported_json_mime("text/html"));
    }

    #[test]
    fn rejects_downloaded_artifact_size_and_contract_drift() {
        let valid = include_bytes!(
            "../../../../../tools/supply-chain-network-planner/contracts/fixtures/\
network_planning_report_bundle.v1.json"
        );
        assert_eq!(
            validate_downloaded_artifact(
                "network_planning_report_bundle.v1",
                valid.len() as i64,
                valid,
            ),
            Ok(())
        );
        assert_eq!(
            validate_downloaded_artifact(
                "network_planning_report_bundle.v1",
                valid.len() as i64 + 1,
                valid,
            ),
            Err("size_mismatch")
        );
        let wrong_schema = br#"{"schema_version":"wrong.v1","kind":"network_planning_report"}"#;
        assert_eq!(
            validate_downloaded_artifact(
                "network_planning_report_bundle.v1",
                wrong_schema.len() as i64,
                wrong_schema,
            ),
            Err("artifact_bundle_contract_mismatch")
        );
    }

    fn git(cwd: &Path, args: &[&str]) {
        let output = Command::new("git")
            .args(["-c", "core.hooksPath=/dev/null"])
            .args(args)
            .current_dir(cwd)
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .output()
            .expect("run git fixture command");
        assert!(
            output.status.success(),
            "git fixture failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    fn final_item_frame(
        workspace_id: Uuid,
        item_id: &str,
        relative_path: &str,
        byte_size: usize,
    ) -> String {
        format!(
            "data: {}\n\n",
            json!({
                "method": "app-server-event",
                "params": {
                    "workspace_id": workspace_id,
                    "message": {"method": "item/completed", "params": {
                        "threadId": "root-thread",
                        "turnId": "root-turn",
                        "item": {
                            "id": item_id,
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
                                    "workspaceRelativePath": relative_path,
                                    "byteSize": byte_size
                                }
                            }}
                        }
                    }}
                }
            })
        )
    }

    #[tokio::test]
    #[ignore = "requires TEST_DATABASE_URL pointing at a disposable PostgreSQL database"]
    async fn final_workspace_artifact_is_idempotent_materialized_and_recovered() {
        let database_url = std::env::var("TEST_DATABASE_URL").expect("TEST_DATABASE_URL");
        let pool = sqlx::postgres::PgPoolOptions::new()
            .max_connections(4)
            .connect(&database_url)
            .await
            .expect("connect disposable PostgreSQL database");
        open_web_codex_platform_store::migrate::run(&pool)
            .await
            .expect("migrate database");

        let files = TempDir::new().expect("Artifact fixture");
        let source = files.path().join("source");
        std::fs::create_dir(&source).unwrap();
        git(&source, &["init", "-b", "main"]);
        std::fs::write(source.join("README.md"), "fixture\n").unwrap();
        git(&source, &["add", "README.md"]);
        git(
            &source,
            &[
                "-c",
                "user.name=Fixture",
                "-c",
                "user.email=fixture@example.invalid",
                "commit",
                "-m",
                "fixture",
            ],
        );
        let git_runtime = Arc::new(
            GitRuntime::new(
                GitRuntimeConfig::new(files.path().join("runner")).with_local_sources(),
            )
            .unwrap(),
        );

        let organization_id = Uuid::now_v7();
        let user_id = Uuid::now_v7();
        let profile_id = Uuid::now_v7();
        let project_id = Uuid::now_v7();
        let task_id = Uuid::now_v7();
        let secondary_task_id = Uuid::now_v7();
        let ungranted_task_id = Uuid::now_v7();
        let workspace_id = Uuid::now_v7();
        let run_id = Uuid::now_v7();
        let secondary_run_id = Uuid::now_v7();
        let source = git_runtime
            .validate_source(&source.to_string_lossy())
            .unwrap();
        let git_ref = git_runtime.validate_ref("main").unwrap();
        let checkout = git_runtime
            .provision(project_id, workspace_id, &source, &git_ref)
            .await
            .unwrap();
        std::fs::create_dir(checkout.root.join("deliverables")).unwrap();
        let valid = include_bytes!(
            "../../../../../tools/supply-chain-network-planner/contracts/fixtures/\
network_planning_report_bundle.v1.json"
        );
        std::fs::write(checkout.root.join("deliverables/report.json"), valid).unwrap();
        std::fs::write(checkout.root.join("deliverables/restart.json"), valid).unwrap();
        std::fs::write(checkout.root.join("deliverables/size.json"), valid).unwrap();
        let wrong_kind = br#"{"kind":"network_comparison_map","schema_version":"network_planning_report_bundle.v1"}"#;
        std::fs::write(checkout.root.join("deliverables/wrong.json"), wrong_kind).unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(
            checkout.root.join("deliverables/report.json"),
            checkout.root.join("deliverables/symlink.json"),
        )
        .unwrap();

        sqlx::query("INSERT INTO organizations (id, name, slug) VALUES ($1, 'Artifact', $2)")
            .bind(organization_id)
            .bind(format!("artifact-{organization_id}"))
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO users (id, username, name, email, password_hash, role)
             VALUES ($1, $2, 'Artifact', $3, 'test-only', 'owner')",
        )
        .bind(user_id)
        .bind(format!("artifact-{user_id}"))
        .bind(format!("{user_id}@example.invalid"))
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO profiles (id, organization_id, owner_user_id, runtime_key, name)
             VALUES ($1, $2, $3, $4, 'Artifact Profile')",
        )
        .bind(profile_id)
        .bind(organization_id)
        .bind(user_id)
        .bind(format!("artifact-{profile_id}"))
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO projects (id, organization_id, created_by, name, git_url, default_branch)
             VALUES ($1, $2, $3, 'Artifact Project', '/tmp/artifact.git', 'main')",
        )
        .bind(project_id)
        .bind(organization_id)
        .bind(user_id)
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO workspaces (
                id, organization_id, project_id, profile_id, created_by, kind, name,
                root_path, source_ref, state
             ) VALUES ($1, $2, $3, $4, $5, 'main', 'Artifact Workspace',
                       $6, 'main', 'ready')",
        )
        .bind(workspace_id)
        .bind(organization_id)
        .bind(project_id)
        .bind(profile_id)
        .bind(user_id)
        .bind(checkout.root.to_string_lossy().as_ref())
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO tasks (
                id, organization_id, project_id, created_by, workspace_id, title, status
             ) VALUES ($1, $2, $3, $4, $5, 'Artifact Task', 'running')",
        )
        .bind(task_id)
        .bind(organization_id)
        .bind(project_id)
        .bind(user_id)
        .bind(workspace_id)
        .execute(&pool)
        .await
        .unwrap();
        for (id, title) in [
            (secondary_task_id, "Secondary Artifact Task"),
            (ungranted_task_id, "Un granted Artifact Task"),
        ] {
            sqlx::query(
                "INSERT INTO tasks (
                    id, organization_id, project_id, created_by, workspace_id, title, status
                 ) VALUES ($1, $2, $3, $4, $5, $6, 'running')",
            )
            .bind(id)
            .bind(organization_id)
            .bind(project_id)
            .bind(user_id)
            .bind(workspace_id)
            .bind(title)
            .execute(&pool)
            .await
            .unwrap();
        }
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
        sqlx::query(
            "INSERT INTO runs (
                id, organization_id, task_id, requested_by, requested_profile_id,
                workspace_id, status, codex_thread_id
             ) VALUES ($1, $2, $3, $4, $5, $6, 'running', 'secondary-thread')",
        )
        .bind(secondary_run_id)
        .bind(organization_id)
        .bind(secondary_task_id)
        .bind(user_id)
        .bind(profile_id)
        .bind(workspace_id)
        .execute(&pool)
        .await
        .unwrap();

        let intermediate = final_item_frame(
            workspace_id,
            "intermediate-item",
            "deliverables/report.json",
            valid.len(),
        )
        .replace(
            "publish_network_planning_report",
            "compare_network_scenarios",
        );
        let intermediate = crate::event_projection::persist_frame(intermediate.as_bytes(), &pool)
            .await
            .unwrap()
            .unwrap();
        assert!(intermediate.pending_artifact_ids.is_empty());
        let invalid = final_item_frame(
            workspace_id,
            "invalid-item",
            "deliverables/report.json",
            valid.len(),
        )
        .replace(
            "Warehouse network planning report",
            "Unexpected report title",
        );
        let invalid = crate::event_projection::persist_frame(invalid.as_bytes(), &pool)
            .await
            .unwrap()
            .unwrap();
        assert!(invalid.pending_artifact_ids.is_empty());
        assert_eq!(
            serde_json::from_slice::<Value>(&invalid.payload)
                .unwrap()
                .pointer("/event/payload/data/artifactDelivery/state")
                .and_then(Value::as_str),
            Some("failed")
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT count(*) FROM artifacts WHERE organization_id = $1",
            )
            .bind(organization_id)
            .fetch_one(&pool)
            .await
            .unwrap(),
            0
        );

        let frame = final_item_frame(
            workspace_id,
            "final-item",
            "deliverables/report.json",
            valid.len(),
        );
        let first = crate::event_projection::persist_frame(frame.as_bytes(), &pool)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(first.pending_artifact_ids.len(), 1);
        assert!(!String::from_utf8_lossy(&first.payload).contains("deliverables/report.json"));
        let replay = crate::event_projection::persist_frame(frame.as_bytes(), &pool)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(replay.pending_artifact_ids, first.pending_artifact_ids);
        let artifact_id = first.pending_artifact_ids[0];
        let app_state = open_web_codex_platform_store::AppState::new(pool.clone());
        let auth = authenticated_user(organization_id, user_id);
        let pending_list = super::list_for_task(
            axum::extract::State(app_state.clone()),
            auth.clone(),
            axum::extract::Path(task_id),
        )
        .await
        .unwrap();
        assert_eq!(pending_list.0.len(), 1);
        assert_eq!(
            pending_list.0[0].state,
            open_web_codex_platform_contracts::ArtifactState::Pending
        );
        assert!(pending_list.0[0].content_url.is_none());
        assert!(pending_list.0[0].download_url.is_none());
        assert!(pending_list.0[0].failure.is_none());
        let pending_detail = super::get(
            axum::extract::State(app_state.clone()),
            auth.clone(),
            axum::extract::Path(artifact_id),
        )
        .await
        .unwrap();
        assert_eq!(
            pending_detail.0.state,
            open_web_codex_platform_contracts::ArtifactState::Pending
        );
        let pending_content = super::read_content(
            axum::extract::State(app_state.clone()),
            auth.clone(),
            axum::extract::Path(artifact_id),
        )
        .await
        .unwrap_err();
        assert_eq!(pending_content.0, StatusCode::CONFLICT);
        assert_eq!(
            pending_content.1 .0.kind,
            open_web_codex_platform_contracts::error::ErrorKind::Conflict
        );
        let pending_download = super::download(
            axum::extract::State(app_state.clone()),
            auth.clone(),
            axum::extract::Path(artifact_id),
        )
        .await
        .unwrap_err();
        assert_eq!(pending_download.0, StatusCode::CONFLICT);
        assert_eq!(
            pending_download.1 .0.kind,
            open_web_codex_platform_contracts::error::ErrorKind::Conflict
        );
        let wrong_task = super::list_for_task(
            axum::extract::State(app_state.clone()),
            auth.clone(),
            axum::extract::Path(ungranted_task_id),
        )
        .await
        .unwrap();
        assert!(wrong_task.0.is_empty());
        let nonexistent_task = super::list_for_task(
            axum::extract::State(app_state.clone()),
            auth.clone(),
            axum::extract::Path(Uuid::now_v7()),
        )
        .await
        .unwrap_err();
        assert_eq!(nonexistent_task.0, StatusCode::NOT_FOUND);
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT count(*) FROM artifacts WHERE organization_id = $1",
            )
            .bind(organization_id)
            .fetch_one(&pool)
            .await
            .unwrap(),
            1
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT count(*) FROM artifact_provenance WHERE organization_id = $1",
            )
            .bind(organization_id)
            .fetch_one(&pool)
            .await
            .unwrap(),
            1
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT count(*) FROM artifact_task_grants WHERE organization_id = $1",
            )
            .bind(organization_id)
            .fetch_one(&pool)
            .await
            .unwrap(),
            1
        );

        sqlx::query(
            "INSERT INTO artifact_task_grants (
                artifact_id, organization_id, task_id, permission
             ) VALUES ($1, $2, $3, 'read')",
        )
        .bind(artifact_id)
        .bind(organization_id)
        .bind(secondary_task_id)
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO artifact_provenance (
                artifact_id, organization_id, producer_task_id, producer_run_id,
                producer_thread_id, producer_turn_id, producer_item_id
             ) VALUES ($1, $2, $3, $4, 'secondary-thread', 'secondary-turn', 'secondary-item')",
        )
        .bind(artifact_id)
        .bind(organization_id)
        .bind(secondary_task_id)
        .bind(secondary_run_id)
        .execute(&pool)
        .await
        .unwrap();
        let ungranted_list = super::list_for_task(
            axum::extract::State(app_state.clone()),
            auth.clone(),
            axum::extract::Path(ungranted_task_id),
        )
        .await
        .unwrap();
        assert!(ungranted_list.0.is_empty());

        let ready_projections =
            super::materialize_artifact(&pool, git_runtime.as_ref(), artifact_id)
                .await
                .unwrap();
        assert_eq!(ready_projections.len(), 2);
        let ready_payload: Value = serde_json::from_slice(&ready_projections[0].payload).unwrap();
        assert_eq!(
            ready_payload
                .pointer("/event/event_type")
                .and_then(Value::as_str),
            Some("platform.artifact.changed")
        );
        assert_eq!(
            ready_payload
                .pointer("/event/payload/data/artifact/state")
                .and_then(Value::as_str),
            Some("ready")
        );
        assert_eq!(
            ready_payload
                .pointer("/event/payload/data/artifact/byteSize")
                .and_then(Value::as_i64),
            Some(valid.len() as i64)
        );
        assert!(!ready_payload
            .to_string()
            .contains("deliverables/report.json"));
        let ready_run_ids = ready_projections
            .iter()
            .map(|projection| {
                serde_json::from_slice::<Value>(&projection.payload)
                    .unwrap()
                    .pointer("/event/run_id")
                    .and_then(Value::as_str)
                    .expect("Artifact event run identity")
                    .to_string()
            })
            .collect::<Vec<_>>();
        assert!(ready_run_ids.contains(&run_id.to_string()));
        assert!(ready_run_ids.contains(&secondary_run_id.to_string()));
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT count(*) FROM run_events
                 WHERE event_type = 'platform.artifact.changed'
                   AND run_id IN ($1, $2)",
            )
            .bind(run_id)
            .bind(secondary_run_id)
            .fetch_one(&pool)
            .await
            .unwrap(),
            2
        );
        let ready_replay = crate::event_projection::persist_frame(frame.as_bytes(), &pool)
            .await
            .unwrap()
            .unwrap();
        assert!(ready_replay.pending_artifact_ids.is_empty());
        let ready_replay_payload: Value = serde_json::from_slice(&ready_replay.payload).unwrap();
        assert_eq!(
            ready_replay_payload
                .pointer("/event/payload/data/artifacts/0/state")
                .and_then(Value::as_str),
            Some("ready")
        );
        assert_eq!(
            ready_replay_payload
                .pointer("/event/payload/data/artifacts/0/byteSize")
                .and_then(Value::as_i64),
            Some(valid.len() as i64)
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT count(*) FROM run_events
                 WHERE event_type = 'platform.artifact.changed'
                   AND run_id IN ($1, $2)",
            )
            .bind(run_id)
            .bind(secondary_run_id)
            .fetch_one(&pool)
            .await
            .unwrap(),
            2
        );
        let stored = sqlx::query("SELECT state, content FROM artifacts WHERE id = $1")
            .bind(artifact_id)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(stored.get::<String, _>("state"), "ready");
        assert_eq!(stored.get::<Vec<u8>, _>("content"), valid);
        assert!(
            super::authorized_artifact_row(&pool, organization_id, artifact_id)
                .await
                .unwrap()
                .is_some(),
            "ready Artifact must be browser-readable"
        );
        let ready_list = super::list_for_task(
            axum::extract::State(app_state.clone()),
            auth.clone(),
            axum::extract::Path(task_id),
        )
        .await
        .unwrap();
        assert_eq!(ready_list.0.len(), 1);
        assert_eq!(ready_list.0[0].id, artifact_id);
        assert_eq!(
            ready_list.0[0].state,
            open_web_codex_platform_contracts::ArtifactState::Ready
        );
        let expected_content_url = format!("/api/artifacts/{artifact_id}/content");
        let expected_download_url = format!("/api/artifacts/{artifact_id}/download");
        assert_eq!(
            ready_list.0[0].content_url.as_deref(),
            Some(expected_content_url.as_str())
        );
        assert_eq!(
            ready_list.0[0].download_url.as_deref(),
            Some(expected_download_url.as_str())
        );
        assert_eq!(ready_list.0[0].byte_size, Some(valid.len() as i64));
        let expected_digest = hex::encode(Sha256::digest(valid));
        assert_eq!(
            ready_list.0[0].content_sha256.as_deref(),
            Some(expected_digest.as_str())
        );
        let ready_detail = super::get(
            axum::extract::State(app_state.clone()),
            auth.clone(),
            axum::extract::Path(artifact_id),
        )
        .await
        .unwrap();
        assert_eq!(ready_detail.0.id, artifact_id);
        assert_eq!(ready_detail.0.failure, None);
        let download = super::download(
            axum::extract::State(app_state.clone()),
            auth.clone(),
            axum::extract::Path(artifact_id),
        )
        .await
        .unwrap();
        assert_eq!(
            download.headers().get(axum::http::header::CONTENT_TYPE),
            Some(&axum::http::HeaderValue::from_static("application/json"))
        );
        let expected_disposition = format!("attachment; filename=\"artifact-{artifact_id}.json\"");
        assert_eq!(
            download
                .headers()
                .get(axum::http::header::CONTENT_DISPOSITION)
                .and_then(|value| value.to_str().ok()),
            Some(expected_disposition.as_str())
        );
        let content = super::read_content(
            axum::extract::State(app_state.clone()),
            auth.clone(),
            axum::extract::Path(artifact_id),
        )
        .await
        .unwrap();
        assert!(content
            .headers()
            .get(axum::http::header::CONTENT_DISPOSITION)
            .is_none());
        let content_bytes = axum::body::to_bytes(content.into_body(), valid.len() + 1)
            .await
            .unwrap();
        let download_bytes = axum::body::to_bytes(download.into_body(), valid.len() + 1)
            .await
            .unwrap();
        assert_eq!(content_bytes.as_ref(), valid);
        assert_eq!(download_bytes, content_bytes);
        assert_eq!(
            hex::encode(Sha256::digest(&content_bytes)),
            ready_list.0[0].content_sha256.as_deref().unwrap()
        );
        let denied = super::get(
            axum::extract::State(app_state.clone()),
            authenticated_user(Uuid::now_v7(), Uuid::now_v7()),
            axum::extract::Path(artifact_id),
        )
        .await
        .unwrap_err();
        assert_eq!(denied.0, axum::http::StatusCode::NOT_FOUND);
        let denied_content = super::read_content(
            axum::extract::State(app_state.clone()),
            authenticated_user(Uuid::now_v7(), Uuid::now_v7()),
            axum::extract::Path(artifact_id),
        )
        .await
        .unwrap_err();
        assert_eq!(denied_content.0, axum::http::StatusCode::NOT_FOUND);
        let denied_download = super::download(
            axum::extract::State(app_state.clone()),
            authenticated_user(Uuid::now_v7(), Uuid::now_v7()),
            axum::extract::Path(artifact_id),
        )
        .await
        .unwrap_err();
        assert_eq!(denied_download.0, axum::http::StatusCode::NOT_FOUND);
        let cross_org_list = super::list_for_task(
            axum::extract::State(app_state.clone()),
            authenticated_user(Uuid::now_v7(), Uuid::now_v7()),
            axum::extract::Path(ungranted_task_id),
        )
        .await
        .unwrap_err();
        assert_eq!(cross_org_list.0, axum::http::StatusCode::NOT_FOUND);

        for (item_id, path, expected_code) in [
            (
                "missing-item",
                "deliverables/missing.json",
                "workspace_read_failed",
            ),
            (
                "symlink-item",
                "deliverables/symlink.json",
                "workspace_read_failed",
            ),
            ("escape-item", "../escape.json", "workspace_read_failed"),
        ] {
            let projection = crate::event_projection::persist_frame(
                final_item_frame(workspace_id, item_id, path, valid.len()).as_bytes(),
                &pool,
            )
            .await
            .unwrap()
            .unwrap();
            let failed_id = projection.pending_artifact_ids[0];
            let failed = super::materialize_artifact(&pool, git_runtime.as_ref(), failed_id)
                .await
                .unwrap()
                .into_iter()
                .next()
                .expect("failed Artifact emits a projection");
            let failed_payload: Value = serde_json::from_slice(&failed.payload).unwrap();
            assert_eq!(
                failed_payload
                    .pointer("/event/payload/data/artifact/state")
                    .and_then(Value::as_str),
                Some("failed")
            );
            assert_eq!(
                sqlx::query_scalar::<_, String>(
                    "SELECT failure_code FROM artifacts WHERE id = $1",
                )
                .bind(failed_id)
                .fetch_one(&pool)
                .await
                .unwrap(),
                expected_code
            );
            if item_id == "missing-item" {
                sqlx::query(
                    "UPDATE artifacts SET failure_code = 'provider/<credential-fragment>'
                     WHERE id = $1",
                )
                .bind(failed_id)
                .execute(&pool)
                .await
                .unwrap();
                let failed_detail = super::get(
                    axum::extract::State(app_state.clone()),
                    auth.clone(),
                    axum::extract::Path(failed_id),
                )
                .await
                .unwrap();
                assert_eq!(
                    failed_detail.0.failure.as_ref().map(|failure| failure.code),
                    Some(open_web_codex_platform_contracts::ArtifactFailureCode::Unknown)
                );
                assert_eq!(
                    failed_detail
                        .0
                        .failure
                        .as_ref()
                        .map(|failure| failure.message.as_str()),
                    Some("Artifact materialization failed")
                );
                assert!(failed_detail.0.content_url.is_none());
                assert!(failed_detail.0.download_url.is_none());
                assert!(!serde_json::to_string(&failed_detail.0)
                    .unwrap()
                    .contains("credential-fragment"));
                let failed_content = super::read_content(
                    axum::extract::State(app_state.clone()),
                    auth.clone(),
                    axum::extract::Path(failed_id),
                )
                .await
                .unwrap_err();
                assert_eq!(failed_content.0, StatusCode::CONFLICT);
                assert_eq!(
                    failed_content.1 .0.kind,
                    open_web_codex_platform_contracts::error::ErrorKind::Conflict
                );
                let failed_download = super::download(
                    axum::extract::State(app_state.clone()),
                    auth.clone(),
                    axum::extract::Path(failed_id),
                )
                .await
                .unwrap_err();
                assert_eq!(failed_download.0, StatusCode::CONFLICT);
                assert_eq!(
                    failed_download.1 .0.kind,
                    open_web_codex_platform_contracts::error::ErrorKind::Conflict
                );
            }
        }

        for (item_id, path, bytes, declared_size, expected_code) in [
            (
                "size-item",
                "deliverables/size.json",
                valid.as_slice(),
                valid.len() + 1,
                "size_mismatch",
            ),
            (
                "contract-item",
                "deliverables/wrong.json",
                wrong_kind.as_slice(),
                wrong_kind.len(),
                "artifact_bundle_contract_mismatch",
            ),
        ] {
            assert_eq!(std::fs::read(checkout.root.join(path)).unwrap(), bytes);
            let projection = crate::event_projection::persist_frame(
                final_item_frame(workspace_id, item_id, path, declared_size).as_bytes(),
                &pool,
            )
            .await
            .unwrap()
            .unwrap();
            let failed_id = projection.pending_artifact_ids[0];
            super::materialize_artifact(&pool, git_runtime.as_ref(), failed_id)
                .await
                .unwrap();
            assert_eq!(
                sqlx::query_scalar::<_, String>(
                    "SELECT failure_code FROM artifacts WHERE id = $1",
                )
                .bind(failed_id)
                .fetch_one(&pool)
                .await
                .unwrap(),
                expected_code
            );
        }

        let restart = crate::event_projection::persist_frame(
            final_item_frame(
                workspace_id,
                "restart-item",
                "deliverables/restart.json",
                valid.len(),
            )
            .as_bytes(),
            &pool,
        )
        .await
        .unwrap()
        .unwrap();
        let restart_id = restart.pending_artifact_ids[0];
        sqlx::query("UPDATE artifacts SET state = 'materializing' WHERE id = $1")
            .bind(restart_id)
            .execute(&pool)
            .await
            .unwrap();
        let restart_detail = super::get(
            axum::extract::State(app_state.clone()),
            auth.clone(),
            axum::extract::Path(restart_id),
        )
        .await
        .unwrap();
        assert_eq!(
            restart_detail.0.state,
            open_web_codex_platform_contracts::ArtifactState::Materializing
        );
        assert!(restart_detail.0.content_url.is_none());
        let materializing_content = super::read_content(
            axum::extract::State(app_state.clone()),
            auth.clone(),
            axum::extract::Path(restart_id),
        )
        .await
        .unwrap_err();
        assert_eq!(materializing_content.0, StatusCode::CONFLICT);
        assert_eq!(
            materializing_content.1 .0.kind,
            open_web_codex_platform_contracts::error::ErrorKind::Conflict
        );
        let materializing_download = super::download(
            axum::extract::State(app_state.clone()),
            auth.clone(),
            axum::extract::Path(restart_id),
        )
        .await
        .unwrap_err();
        assert_eq!(materializing_download.0, StatusCode::CONFLICT);
        assert_eq!(
            materializing_download.1 .0.kind,
            open_web_codex_platform_contracts::error::ErrorKind::Conflict
        );
        let (events, mut receiver) = tokio::sync::broadcast::channel(4);
        super::recover_and_materialize_pending(pool.clone(), git_runtime, events).await;
        let recovered: Value =
            serde_json::from_slice(&receiver.recv().await.unwrap().payload).unwrap();
        assert_eq!(
            recovered
                .pointer("/event/payload/data/artifact/state")
                .and_then(Value::as_str),
            Some("ready")
        );
        let recovered_detail = super::get(
            axum::extract::State(app_state),
            auth,
            axum::extract::Path(restart_id),
        )
        .await
        .unwrap();
        assert_eq!(
            recovered_detail.0.state,
            open_web_codex_platform_contracts::ArtifactState::Ready
        );
        assert!(recovered_detail.0.content_url.is_some());
    }
}
