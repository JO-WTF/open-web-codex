use sqlx::{Postgres, Row, Transaction};
use uuid::Uuid;

use crate::{RunOrchestratorError, SupervisorPolicySnapshotInput};

const MAX_POLICY_ID_BYTES: usize = 128;
const MAX_POLICY_VERSION_BYTES: usize = 64;
const MAX_POLICY_NAME_BYTES: usize = 160;
const MAX_DEVELOPER_INSTRUCTIONS_BYTES: usize = 16 * 1024;

pub(crate) async fn ensure_run_policy_binding(
    transaction: &mut Transaction<'_, Postgres>,
    organization_id: Uuid,
    profile_id: Uuid,
    task_id: Uuid,
    run_id: Uuid,
    requested: Option<&SupervisorPolicySnapshotInput>,
    inherited_snapshot_id: Option<Uuid>,
) -> Result<(), RunOrchestratorError> {
    if requested.is_some() && inherited_snapshot_id.is_some() {
        return Err(RunOrchestratorError::Invalid(
            "forked Runs inherit their source Supervisor Policy".to_string(),
        ));
    }

    let desired_snapshot_id = if let Some(requested) = requested {
        Some(ensure_policy_snapshot(transaction, organization_id, requested).await?)
    } else if let Some(snapshot_id) = inherited_snapshot_id {
        let exists = sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS( \
                 SELECT 1 FROM supervisor_policy_snapshots \
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
        "SELECT snapshot_id FROM supervisor_policy_bindings WHERE run_id = $1",
    )
    .bind(run_id)
    .fetch_optional(&mut **transaction)
    .await?;

    match (existing_snapshot_id, desired_snapshot_id) {
        (None, None) => Ok(()),
        (Some(existing), Some(desired)) if existing == desired => Ok(()),
        (Some(_), None) | (None, Some(_)) | (Some(_), Some(_)) => {
            if existing_snapshot_id.is_none() {
                if let Some(snapshot_id) = desired_snapshot_id {
                    sqlx::query(
                        "INSERT INTO supervisor_policy_bindings \
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
                    return Ok(());
                }
            }
            Err(RunOrchestratorError::Conflict(
                "idempotency key was already used with a different Supervisor Policy".to_string(),
            ))
        }
    }
}

async fn ensure_policy_snapshot(
    transaction: &mut Transaction<'_, Postgres>,
    organization_id: Uuid,
    requested: &SupervisorPolicySnapshotInput,
) -> Result<Uuid, RunOrchestratorError> {
    validate_snapshot(requested)?;
    let inserted = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO supervisor_policy_snapshots \
         (organization_id, policy_id, version, display_name, developer_instructions, \
          content_sha256, source, release_id) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8) \
         ON CONFLICT (organization_id, policy_id, version) DO NOTHING \
         RETURNING id",
    )
    .bind(organization_id)
    .bind(&requested.policy_id)
    .bind(&requested.version)
    .bind(&requested.display_name)
    .bind(&requested.developer_instructions)
    .bind(&requested.content_sha256)
    .bind(requested.source.as_str())
    .bind(requested.release_id)
    .fetch_optional(&mut **transaction)
    .await?;
    if let Some(snapshot_id) = inserted {
        return Ok(snapshot_id);
    }

    let existing = sqlx::query(
        "SELECT id, display_name, developer_instructions, content_sha256, source, release_id \
         FROM supervisor_policy_snapshots \
         WHERE organization_id = $1 AND policy_id = $2 AND version = $3",
    )
    .bind(organization_id)
    .bind(&requested.policy_id)
    .bind(&requested.version)
    .fetch_one(&mut **transaction)
    .await?;
    if existing.get::<String, _>("display_name") != requested.display_name
        || existing.get::<String, _>("developer_instructions") != requested.developer_instructions
        || existing.get::<String, _>("content_sha256") != requested.content_sha256
        || existing.get::<String, _>("source") != requested.source.as_str()
        || existing.get::<Option<Uuid>, _>("release_id") != requested.release_id
    {
        return Err(RunOrchestratorError::Conflict(format!(
            "Supervisor Policy '{}@{}' already has different immutable content",
            requested.policy_id, requested.version
        )));
    }
    Ok(existing.get("id"))
}

fn validate_snapshot(snapshot: &SupervisorPolicySnapshotInput) -> Result<(), RunOrchestratorError> {
    if !valid_bounded_text(&snapshot.policy_id, MAX_POLICY_ID_BYTES)
        || !valid_identifier(&snapshot.policy_id)
    {
        return Err(RunOrchestratorError::Invalid(
            "Supervisor Policy id is invalid".to_string(),
        ));
    }
    if !valid_bounded_text(&snapshot.version, MAX_POLICY_VERSION_BYTES)
        || !valid_identifier(&snapshot.version)
    {
        return Err(RunOrchestratorError::Invalid(
            "Supervisor Policy version is invalid".to_string(),
        ));
    }
    if !valid_bounded_text(&snapshot.display_name, MAX_POLICY_NAME_BYTES) {
        return Err(RunOrchestratorError::Invalid(
            "Supervisor Policy display name is invalid".to_string(),
        ));
    }
    if !valid_bounded_text(
        &snapshot.developer_instructions,
        MAX_DEVELOPER_INSTRUCTIONS_BYTES,
    ) {
        return Err(RunOrchestratorError::Invalid(
            "Supervisor Policy developer instructions are invalid".to_string(),
        ));
    }
    if snapshot.content_sha256.len() != 64
        || !snapshot
            .content_sha256
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        return Err(RunOrchestratorError::Invalid(
            "Supervisor Policy digest is invalid".to_string(),
        ));
    }
    if (snapshot.source == crate::SupervisorPolicySource::Repository
        && snapshot.release_id.is_some())
        || (snapshot.source == crate::SupervisorPolicySource::UserRelease
            && snapshot.release_id.is_none())
    {
        return Err(RunOrchestratorError::Invalid(
            "Supervisor Policy source is invalid".to_string(),
        ));
    }
    Ok(())
}

fn valid_bounded_text(value: &str, max_bytes: usize) -> bool {
    !value.trim().is_empty() && value.len() <= max_bytes
}

fn valid_identifier(value: &str) -> bool {
    value
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
}
