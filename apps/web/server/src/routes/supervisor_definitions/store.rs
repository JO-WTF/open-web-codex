use std::collections::BTreeMap;

use open_web_codex_platform_contracts::{
    SupervisorDefinitionSummary, SupervisorDraftRequest, SupervisorDraftSummary,
    SupervisorReleaseSummary,
};
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Postgres, Row, Transaction};
use uuid::Uuid;

use crate::middleware::auth::AuthenticatedUser;

use super::{database_error, internal_error, not_found, ApiError};

pub(super) async fn load_definitions(
    db: &PgPool,
    organization_id: Uuid,
) -> Result<Vec<SupervisorDefinitionSummary>, ApiError> {
    let definitions = sqlx::query(
        "SELECT id, policy_id, display_name, description, owner_user_id, created_at, updated_at \
         FROM supervisor_definitions WHERE organization_id = $1 \
         ORDER BY updated_at DESC, id",
    )
    .bind(organization_id)
    .fetch_all(db)
    .await
    .map_err(database_error)?;
    let drafts = sqlx::query(
        "SELECT definition_id, draft_spec, revision_number, content_sha256, updated_at \
                FROM supervisor_revisions \
         WHERE organization_id = $1 AND state = 'draft'",
    )
    .bind(organization_id)
    .fetch_all(db)
    .await
    .map_err(database_error)?
    .into_iter()
    .map(|row| {
        let draft = parse_draft(row.get("draft_spec"))?;
        let content_sha256 = row
            .get::<Option<String>, _>("content_sha256")
            .unwrap_or_else(|| draft_content_sha256(&draft));
        Ok((
            row.get::<Uuid, _>("definition_id"),
            (
                draft,
                SupervisorDraftSummary {
                    revision: row.get("revision_number"),
                    content_sha256,
                    validation_state: "unvalidated".to_string(),
                    updated_at: row.get("updated_at"),
                },
            ),
        ))
    })
    .collect::<Result<BTreeMap<_, _>, ApiError>>()?;
    let mut releases = BTreeMap::<Uuid, Vec<SupervisorReleaseSummary>>::new();
    for row in sqlx::query(
        "SELECT id, definition_id, policy_id, version, display_name, description, \
                content_sha256, published_at \
         FROM supervisor_releases WHERE organization_id = $1 \
         ORDER BY published_at DESC, id",
    )
    .bind(organization_id)
    .fetch_all(db)
    .await
    .map_err(database_error)?
    {
        releases
            .entry(row.get("definition_id"))
            .or_default()
            .push(release_summary(&row));
    }
    Ok(definitions
        .into_iter()
        .map(|row| {
            let id = row.get("id");
            SupervisorDefinitionSummary {
                id,
                policy_id: row.get("policy_id"),
                display_name: row.get("display_name"),
                description: row.get("description"),
                owner_user_id: row.get("owner_user_id"),
                draft: drafts.get(&id).map(|entry| entry.0.clone()),
                draft_metadata: drafts.get(&id).map(|entry| entry.1.clone()),
                releases: releases.remove(&id).unwrap_or_default(),
                created_at: row.get("created_at"),
                updated_at: row.get("updated_at"),
            }
        })
        .collect())
}

pub(super) async fn load_definition(
    db: &PgPool,
    organization_id: Uuid,
    definition_id: Uuid,
) -> Result<SupervisorDefinitionSummary, ApiError> {
    load_definitions(db, organization_id)
        .await?
        .into_iter()
        .find(|definition| definition.id == definition_id)
        .ok_or_else(|| not_found("Supervisor Definition was not found"))
}

pub(super) async fn load_draft_row(
    db: &PgPool,
    organization_id: Uuid,
    definition_id: Uuid,
) -> Result<sqlx::postgres::PgRow, ApiError> {
    sqlx::query(
        "SELECT definition.owner_user_id, revision.draft_spec \
         FROM supervisor_definitions definition \
         JOIN supervisor_revisions revision ON revision.definition_id = definition.id \
           AND revision.organization_id = definition.organization_id AND revision.state = 'draft' \
         WHERE definition.id = $1 AND definition.organization_id = $2",
    )
    .bind(definition_id)
    .bind(organization_id)
    .fetch_optional(db)
    .await
    .map_err(database_error)?
    .ok_or_else(|| not_found("Supervisor draft was not found"))
}

pub(super) async fn lock_definition(
    transaction: &mut Transaction<'_, Postgres>,
    organization_id: Uuid,
    definition_id: Uuid,
) -> Result<sqlx::postgres::PgRow, ApiError> {
    sqlx::query(
        "SELECT owner_user_id, policy_id FROM supervisor_definitions \
         WHERE id = $1 AND organization_id = $2 FOR UPDATE",
    )
    .bind(definition_id)
    .bind(organization_id)
    .fetch_optional(&mut **transaction)
    .await
    .map_err(database_error)?
    .ok_or_else(|| not_found("Supervisor Definition was not found"))
}

pub(super) fn parse_draft(value: serde_json::Value) -> Result<SupervisorDraftRequest, ApiError> {
    serde_json::from_value(value).map_err(|_| internal_error())
}

pub(super) async fn record_audit(
    transaction: &mut Transaction<'_, Postgres>,
    auth: &AuthenticatedUser,
    action: &str,
    target_id: Uuid,
) -> Result<(), ApiError> {
    sqlx::query(
        "INSERT INTO audit_log (organization_id, actor_id, action, target_type, target_id) \
         VALUES ($1, $2, $3, 'supervisor', $4)",
    )
    .bind(auth.organization_id)
    .bind(auth.user_id)
    .bind(action)
    .bind(target_id)
    .execute(&mut **transaction)
    .await
    .map_err(database_error)?;
    Ok(())
}

fn release_summary(row: &sqlx::postgres::PgRow) -> SupervisorReleaseSummary {
    SupervisorReleaseSummary {
        id: row.get("id"),
        policy_id: row.get("policy_id"),
        version: row.get("version"),
        display_name: row.get("display_name"),
        description: row.get("description"),
        content_sha256: row.get("content_sha256"),
        published_at: row.get("published_at"),
    }
}

pub(super) fn draft_content_sha256(draft: &SupervisorDraftRequest) -> String {
    let bytes = serde_json::to_vec(draft).expect("Supervisor Draft serializes");
    hex::encode(Sha256::digest(bytes))
}
