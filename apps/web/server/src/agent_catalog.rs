use open_web_codex_platform_contracts::{
    AgentDefinitionDetail, AgentDefinitionSource, AgentDefinitionSummary,
};
use open_web_codex_supervisor_catalog::agent::{self, AgentReleaseSpec, ResolvedAgentDefinition};
use sqlx::{PgPool, Row};
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error, PartialEq, Eq)]
pub(crate) enum AgentCatalogError {
    #[error("Agent Definition was not found")]
    NotFound,
    #[error("Agent Definition content is invalid")]
    Invalid,
    #[error("Agent Definition database operation failed")]
    Database,
}

pub(crate) async fn get_published(
    db: &PgPool,
    organization_id: Uuid,
    definition_id: &str,
    version: &str,
) -> Result<AgentDefinitionDetail, AgentCatalogError> {
    let definition = list_resolved(db, organization_id)
        .await?
        .into_iter()
        .find(|definition| {
            definition.definition_id == definition_id && definition.version == version
        })
        .ok_or(AgentCatalogError::NotFound)?;
    let source = if definition.release_id.is_some() {
        AgentDefinitionSource::UserRelease
    } else {
        AgentDefinitionSource::Repository
    };
    Ok(definition.detail(source))
}

pub(crate) async fn list_published(
    db: &PgPool,
    organization_id: Uuid,
) -> Result<Vec<AgentDefinitionSummary>, AgentCatalogError> {
    let mut definitions = agent::list_resolved_builtins()
        .map_err(|_| AgentCatalogError::Invalid)?
        .iter()
        .map(|definition| definition.summary(AgentDefinitionSource::Repository))
        .collect::<Vec<_>>();
    definitions.extend(
        list_user_releases(db, organization_id)
            .await?
            .iter()
            .map(|definition| definition.summary(AgentDefinitionSource::UserRelease)),
    );
    Ok(definitions)
}

pub(crate) async fn list_resolved(
    db: &PgPool,
    organization_id: Uuid,
) -> Result<Vec<ResolvedAgentDefinition>, AgentCatalogError> {
    let mut definitions =
        agent::list_resolved_builtins().map_err(|_| AgentCatalogError::Invalid)?;
    definitions.extend(list_user_releases(db, organization_id).await?);
    Ok(definitions)
}

pub(crate) fn is_reserved_builtin_definition_id(definition_id: &str) -> bool {
    agent::list_resolved_builtins().is_ok_and(|definitions| {
        definitions
            .iter()
            .any(|definition| definition.definition_id == definition_id)
    })
}

async fn list_user_releases(
    db: &PgPool,
    organization_id: Uuid,
) -> Result<Vec<ResolvedAgentDefinition>, AgentCatalogError> {
    sqlx::query(
        "SELECT id, definition_id, catalog_id, version, release_spec, runtime_role, content_sha256 \
         FROM agent_definition_releases \
         WHERE organization_id = $1 ORDER BY published_at DESC, catalog_id, version",
    )
    .bind(organization_id)
    .fetch_all(db)
    .await
    .map_err(|_| AgentCatalogError::Database)?
    .iter()
    .map(resolve_release_row)
    .collect()
}

fn resolve_release_row(
    row: &sqlx::postgres::PgRow,
) -> Result<ResolvedAgentDefinition, AgentCatalogError> {
    let release_id: Uuid = row.get("id");
    let spec = serde_json::from_value::<AgentReleaseSpec>(row.get("release_spec"))
        .map_err(|_| AgentCatalogError::Invalid)?;
    if spec.definition_id != row.get::<String, _>("catalog_id")
        || spec.version != row.get::<String, _>("version")
    {
        return Err(AgentCatalogError::Invalid);
    }
    let expected_runtime_role = agent::user_runtime_role_name(&spec.definition_id, &spec.version)
        .map_err(|_| AgentCatalogError::Invalid)?;
    if expected_runtime_role != row.get::<String, _>("runtime_role") {
        return Err(AgentCatalogError::Invalid);
    }
    let resolved = agent::validate_user_release(spec)
        .map_err(|_| AgentCatalogError::Invalid)?
        .with_release_id(release_id);
    if resolved.content_sha256 != row.get::<String, _>("content_sha256") {
        return Err(AgentCatalogError::Invalid);
    }
    Ok(resolved)
}
