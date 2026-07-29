use sqlx::{Postgres, Row, Transaction};
use uuid::Uuid;

use crate::{AgentRunSnapshotInput, AgentRunSource, RunOrchestratorError};

pub(crate) async fn ensure_run_agent_binding(
    transaction: &mut Transaction<'_, Postgres>,
    organization_id: Uuid,
    profile_id: Uuid,
    task_id: Uuid,
    run_id: Uuid,
    requested: Option<&AgentRunSnapshotInput>,
    inherited_snapshot_id: Option<Uuid>,
) -> Result<(), RunOrchestratorError> {
    if requested.is_some() && inherited_snapshot_id.is_some() {
        return Err(RunOrchestratorError::Invalid(
            "forked Runs inherit their source Agent".to_string(),
        ));
    }
    let desired_snapshot_id = if let Some(requested) = requested {
        Some(ensure_agent_snapshot(transaction, organization_id, requested).await?)
    } else if let Some(snapshot_id) = inherited_snapshot_id {
        let exists = sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS ( \
                 SELECT 1 FROM agent_run_snapshots \
                 WHERE id = $1 AND organization_id = $2 \
             )",
        )
        .bind(snapshot_id)
        .bind(organization_id)
        .fetch_one(&mut **transaction)
        .await?;
        if !exists {
            return Err(RunOrchestratorError::NotFound);
        }
        Some(snapshot_id)
    } else {
        None
    };
    let existing_snapshot_id = sqlx::query_scalar::<_, Uuid>(
        "SELECT snapshot_id FROM agent_run_bindings WHERE run_id = $1",
    )
    .bind(run_id)
    .fetch_optional(&mut **transaction)
    .await?;
    match (existing_snapshot_id, desired_snapshot_id) {
        (None, None) => Ok(()),
        (Some(existing), Some(desired)) if existing == desired => Ok(()),
        (None, Some(snapshot_id)) => {
            sqlx::query(
                "INSERT INTO agent_run_bindings \
                 (organization_id, profile_id, task_id, run_id, snapshot_id) \
                 VALUES ($1, $2, $3, $4, $5)",
            )
            .bind(organization_id)
            .bind(profile_id)
            .bind(task_id)
            .bind(run_id)
            .bind(snapshot_id)
            .execute(&mut **transaction)
            .await?;
            Ok(())
        }
        _ => Err(RunOrchestratorError::Conflict(
            "idempotency key was already used with a different root Agent".to_string(),
        )),
    }
}

async fn ensure_agent_snapshot(
    transaction: &mut Transaction<'_, Postgres>,
    organization_id: Uuid,
    requested: &AgentRunSnapshotInput,
) -> Result<Uuid, RunOrchestratorError> {
    validate_snapshot(requested)?;
    let inserted = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO agent_run_snapshots \
         (organization_id, definition_id, version, display_name, content_sha256, source, \
          release_id) \
         VALUES ($1, $2, $3, $4, $5, $6, $7) \
         ON CONFLICT (organization_id, definition_id, version) DO NOTHING \
         RETURNING id",
    )
    .bind(organization_id)
    .bind(&requested.definition_id)
    .bind(&requested.version)
    .bind(&requested.display_name)
    .bind(&requested.content_sha256)
    .bind(requested.source.as_str())
    .bind(requested.release_id)
    .fetch_optional(&mut **transaction)
    .await?;
    if let Some(snapshot_id) = inserted {
        return Ok(snapshot_id);
    }
    let existing = sqlx::query(
        "SELECT id, display_name, content_sha256, source, release_id \
         FROM agent_run_snapshots \
         WHERE organization_id = $1 AND definition_id = $2 AND version = $3",
    )
    .bind(organization_id)
    .bind(&requested.definition_id)
    .bind(&requested.version)
    .fetch_one(&mut **transaction)
    .await?;
    if existing.get::<String, _>("display_name") != requested.display_name
        || existing.get::<String, _>("content_sha256") != requested.content_sha256
        || existing.get::<String, _>("source") != requested.source.as_str()
        || existing.get::<Option<Uuid>, _>("release_id") != requested.release_id
    {
        return Err(RunOrchestratorError::Conflict(format!(
            "Agent '{}@{}' already has different immutable content",
            requested.definition_id, requested.version
        )));
    }
    Ok(existing.get("id"))
}

fn validate_snapshot(snapshot: &AgentRunSnapshotInput) -> Result<(), RunOrchestratorError> {
    if !valid_identifier(&snapshot.definition_id, false, 96)
        || !valid_identifier(&snapshot.version, true, 64)
        || snapshot.display_name.trim().is_empty()
        || snapshot.display_name.len() > 160
        || snapshot.content_sha256.len() != 64
        || !snapshot
            .content_sha256
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (byte.is_ascii_lowercase() && byte <= b'f'))
        || matches!(snapshot.source, AgentRunSource::Repository) != snapshot.release_id.is_none()
    {
        return Err(RunOrchestratorError::Invalid(
            "root Agent snapshot is invalid".to_string(),
        ));
    }
    Ok(())
}

fn valid_identifier(value: &str, allow_period: bool, maximum: usize) -> bool {
    value.len() >= 2
        && value.len() <= maximum
        && !value.contains("..")
        && value
            .as_bytes()
            .first()
            .is_some_and(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
        && value
            .as_bytes()
            .last()
            .is_some_and(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase()
                || byte.is_ascii_digit()
                || matches!(byte, b'-' | b'_')
                || (allow_period && byte == b'.')
        })
}
