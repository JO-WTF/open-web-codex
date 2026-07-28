use std::collections::BTreeMap;

use open_web_codex_platform_contracts::{
    AgentDefinitionDraftRequest, AgentDefinitionReleaseSummary, AgentDefinitionResourceSummary,
};
use sqlx::{PgPool, Postgres, Row, Transaction};
use uuid::Uuid;

use crate::middleware::auth::AuthenticatedUser;

use super::{database_error, internal_error, not_found, ApiError};

pub(super) async fn load_definitions(
    db: &PgPool,
    organization_id: Uuid,
) -> Result<Vec<AgentDefinitionResourceSummary>, ApiError> {
    let definitions = sqlx::query(
        "SELECT id, definition_id, display_name, description, owner_user_id, created_at, updated_at \
         FROM agent_definitions WHERE organization_id = $1 \
         ORDER BY updated_at DESC, id",
    )
    .bind(organization_id)
    .fetch_all(db)
    .await
    .map_err(database_error)?;
    let drafts = sqlx::query(
        "SELECT definition_id, draft_spec FROM agent_definition_revisions \
         WHERE organization_id = $1 AND state = 'draft'",
    )
    .bind(organization_id)
    .fetch_all(db)
    .await
    .map_err(database_error)?
    .into_iter()
    .map(|row| {
        Ok((
            row.get::<Uuid, _>("definition_id"),
            parse_draft(row.get("draft_spec"))?,
        ))
    })
    .collect::<Result<BTreeMap<_, _>, ApiError>>()?;
    let mut releases = BTreeMap::<Uuid, Vec<AgentDefinitionReleaseSummary>>::new();
    for row in sqlx::query(
        "SELECT id, definition_id, catalog_id, version, display_name, description, \
                content_sha256, published_at \
         FROM agent_definition_releases WHERE organization_id = $1 \
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
            AgentDefinitionResourceSummary {
                id,
                definition_id: row.get("definition_id"),
                display_name: row.get("display_name"),
                description: row.get("description"),
                owner_user_id: row.get("owner_user_id"),
                draft: drafts.get(&id).cloned(),
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
) -> Result<AgentDefinitionResourceSummary, ApiError> {
    load_definitions(db, organization_id)
        .await?
        .into_iter()
        .find(|definition| definition.id == definition_id)
        .ok_or_else(|| not_found("Agent Definition was not found"))
}

pub(super) async fn load_draft_row(
    db: &PgPool,
    organization_id: Uuid,
    definition_id: Uuid,
) -> Result<sqlx::postgres::PgRow, ApiError> {
    sqlx::query(
        "SELECT definition.owner_user_id, definition.id AS definition_resource_id, \
                revision.draft_spec \
         FROM agent_definitions definition \
         JOIN agent_definition_revisions revision ON revision.definition_id = definition.id \
           AND revision.organization_id = definition.organization_id AND revision.state = 'draft' \
         WHERE definition.id = $1 AND definition.organization_id = $2",
    )
    .bind(definition_id)
    .bind(organization_id)
    .fetch_optional(db)
    .await
    .map_err(database_error)?
    .ok_or_else(|| not_found("Agent Definition draft was not found"))
}

pub(super) async fn lock_definition(
    transaction: &mut Transaction<'_, Postgres>,
    organization_id: Uuid,
    definition_id: Uuid,
) -> Result<sqlx::postgres::PgRow, ApiError> {
    sqlx::query(
        "SELECT owner_user_id, definition_id FROM agent_definitions \
         WHERE id = $1 AND organization_id = $2 FOR UPDATE",
    )
    .bind(definition_id)
    .bind(organization_id)
    .fetch_optional(&mut **transaction)
    .await
    .map_err(database_error)?
    .ok_or_else(|| not_found("Agent Definition was not found"))
}

pub(super) fn parse_draft(
    value: serde_json::Value,
) -> Result<AgentDefinitionDraftRequest, ApiError> {
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
         VALUES ($1, $2, $3, 'agent_definition', $4)",
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

fn release_summary(row: &sqlx::postgres::PgRow) -> AgentDefinitionReleaseSummary {
    AgentDefinitionReleaseSummary {
        id: row.get("id"),
        definition_id: row.get("catalog_id"),
        version: row.get("version"),
        display_name: row.get("display_name"),
        description: row.get("description"),
        content_sha256: row.get("content_sha256"),
        published_at: row.get("published_at"),
    }
}
