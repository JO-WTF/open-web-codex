use open_web_codex_platform_contracts::{
    AgentCapabilityTemplateSelection, AgentCapabilityTemplateSource, AgentDefinitionDetail,
    AgentDefinitionSource, AgentDefinitionSummary, AgentRunSelection,
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

pub(crate) async fn resolve_run_selection(
    db: &PgPool,
    organization_id: Uuid,
    selection: &AgentRunSelection,
) -> Result<ResolvedAgentDefinition, AgentCatalogError> {
    match selection.release_id {
        None => agent::resolve_builtin(&selection.definition_id, &selection.version)
            .map_err(|_| AgentCatalogError::NotFound),
        Some(release_id) => {
            let row = sqlx::query(
                "SELECT id, definition_id, catalog_id, version, release_spec, runtime_role, \
                        content_sha256 \
                 FROM agent_definition_releases \
                 WHERE organization_id = $1 AND id = $2",
            )
            .bind(organization_id)
            .bind(release_id)
            .fetch_optional(db)
            .await
            .map_err(|_| AgentCatalogError::Database)?
            .ok_or(AgentCatalogError::NotFound)?;
            if row.get::<String, _>("catalog_id") != selection.definition_id
                || row.get::<String, _>("version") != selection.version
            {
                return Err(AgentCatalogError::Invalid);
            }
            resolve_release_row(db, organization_id, &row).await
        }
    }
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
    let rows = sqlx::query(
        "SELECT id, definition_id, catalog_id, version, release_spec, runtime_role, content_sha256 \
         FROM agent_definition_releases \
         WHERE organization_id = $1 ORDER BY published_at DESC, catalog_id, version",
    )
    .bind(organization_id)
    .fetch_all(db)
    .await
    .map_err(|_| AgentCatalogError::Database)?;
    let mut resolved = Vec::with_capacity(rows.len());
    for row in &rows {
        resolved.push(resolve_release_row(db, organization_id, row).await?);
    }
    Ok(resolved)
}

async fn resolve_release_row(
    db: &PgPool,
    organization_id: Uuid,
    row: &sqlx::postgres::PgRow,
) -> Result<ResolvedAgentDefinition, AgentCatalogError> {
    let release_id: Uuid = row.get("id");
    let spec = serde_json::from_value::<AgentReleaseSpec>(row.get("release_spec"))
        .map_err(|_| invalid_release(row, "release_spec"))?;
    if spec.definition_id != row.get::<String, _>("catalog_id")
        || spec.version != row.get::<String, _>("version")
    {
        return Err(invalid_release(row, "release_identity"));
    }
    let expected_runtime_role = agent::user_runtime_role_name(&spec.definition_id, &spec.version)
        .map_err(|_| invalid_release(row, "runtime_role_name"))?;
    if expected_runtime_role != row.get::<String, _>("runtime_role") {
        return Err(invalid_release(row, "runtime_role"));
    }
    let template = resolve_capability_template(db, organization_id, &spec.capability_template)
        .await
        .map_err(|error| release_error(row, "capability_template", error))?;
    verify_capability_dependency(
        db,
        organization_id,
        release_id,
        &spec.capability_template,
        &template,
    )
    .await
    .map_err(|error| release_error(row, "capability_dependency", error))?;
    verify_dataset_dependencies(db, organization_id, release_id, &spec.dataset_releases)
        .await
        .map_err(|error| release_error(row, "dataset_dependencies", error))?;
    let resolved = agent::compile_agent_release_against_template(spec, &template)
        .map_err(|_| invalid_release(row, "compile"))?
        .with_release_id(release_id);
    if resolved.content_sha256 != row.get::<String, _>("content_sha256") {
        return Err(invalid_release(row, "content_sha256"));
    }
    Ok(resolved)
}

fn invalid_release(
    row: &sqlx::postgres::PgRow,
    validation_stage: &'static str,
) -> AgentCatalogError {
    release_error(row, validation_stage, AgentCatalogError::Invalid)
}

fn release_error(
    row: &sqlx::postgres::PgRow,
    validation_stage: &'static str,
    error: AgentCatalogError,
) -> AgentCatalogError {
    tracing::warn!(
        release_id = %row.get::<Uuid, _>("id"),
        definition_id = %row.get::<String, _>("catalog_id"),
        version = %row.get::<String, _>("version"),
        validation_stage,
        %error,
        "published Agent Definition failed validation"
    );
    error
}

async fn verify_dataset_dependencies(
    db: &PgPool,
    organization_id: Uuid,
    agent_release_id: Uuid,
    expected: &[open_web_codex_platform_contracts::AgentDatasetReleaseBinding],
) -> Result<(), AgentCatalogError> {
    let rows = sqlx::query(
        "SELECT dependency.dataset_release_id, dependency.workspace_id, \
                dependency.dataset_id, dependency.dataset_version, \
                dependency.dataset_content_sha256, release.state, \
                release.workspace_id AS release_workspace_id, \
                release.dataset_id AS release_dataset_id, \
                release.version AS release_version, \
                release.display_name AS release_display_name, \
                release.content_sha256 AS release_content_sha256 \
         FROM agent_release_dataset_dependencies dependency \
         JOIN workspace_dataset_releases release \
           ON release.organization_id = dependency.organization_id \
          AND release.id = dependency.dataset_release_id \
         WHERE dependency.organization_id = $1 AND dependency.agent_release_id = $2",
    )
    .bind(organization_id)
    .bind(agent_release_id)
    .fetch_all(db)
    .await
    .map_err(|_| AgentCatalogError::Database)?;
    if rows.len() != expected.len()
        || expected.iter().any(|binding| {
            !rows.iter().any(|row| {
                row.get::<Uuid, _>("dataset_release_id") == binding.release_id
                    && row.get::<Uuid, _>("workspace_id") == binding.workspace_id
                    && row.get::<String, _>("dataset_id") == binding.dataset_id
                    && row.get::<String, _>("dataset_version") == binding.version
                    && row.get::<String, _>("dataset_content_sha256") == binding.content_sha256
                    && row.get::<String, _>("state") == "published"
                    && row.get::<Uuid, _>("release_workspace_id") == binding.workspace_id
                    && row.get::<String, _>("release_dataset_id") == binding.dataset_id
                    && row.get::<String, _>("release_version") == binding.version
                    && row.get::<String, _>("release_display_name") == binding.display_name
                    && row.get::<String, _>("release_content_sha256") == binding.content_sha256
            })
        })
    {
        return Err(AgentCatalogError::Invalid);
    }
    Ok(())
}

async fn verify_capability_dependency(
    db: &PgPool,
    organization_id: Uuid,
    agent_release_id: Uuid,
    selection: &AgentCapabilityTemplateSelection,
    template: &ResolvedAgentDefinition,
) -> Result<(), AgentCatalogError> {
    let dependency = sqlx::query(
        "SELECT package_release_id, workspace_id, package_id, package_version, \
                package_content_sha256 \
         FROM agent_release_capability_package_dependencies \
         WHERE organization_id = $1 AND agent_release_id = $2",
    )
    .bind(organization_id)
    .bind(agent_release_id)
    .fetch_optional(db)
    .await
    .map_err(|_| AgentCatalogError::Database)?;
    match selection.source {
        AgentCapabilityTemplateSource::RepositoryAgent if dependency.is_none() => Ok(()),
        AgentCapabilityTemplateSource::WorkspacePackageRelease => {
            let dependency = dependency.ok_or(AgentCatalogError::Invalid)?;
            if Some(dependency.get::<Uuid, _>("package_release_id")) != selection.release_id
                || Some(dependency.get::<Uuid, _>("workspace_id"))
                    != template.capability_workspace_id
                || dependency.get::<String, _>("package_id") != selection.definition_id
                || dependency.get::<String, _>("package_version") != selection.version
                || dependency.get::<String, _>("package_content_sha256")
                    != template.capability_template_sha256
            {
                return Err(AgentCatalogError::Invalid);
            }
            Ok(())
        }
        _ => Err(AgentCatalogError::Invalid),
    }
}

pub(crate) async fn resolve_capability_template(
    db: &PgPool,
    organization_id: Uuid,
    selection: &AgentCapabilityTemplateSelection,
) -> Result<ResolvedAgentDefinition, AgentCatalogError> {
    match selection.source {
        AgentCapabilityTemplateSource::RepositoryAgent => {
            if selection.release_id.is_some() {
                return Err(AgentCatalogError::Invalid);
            }
            agent::resolve_builtin(&selection.definition_id, &selection.version)
                .map_err(|_| AgentCatalogError::Invalid)
        }
        AgentCapabilityTemplateSource::WorkspacePackageRelease => {
            let release_id = selection.release_id.ok_or(AgentCatalogError::Invalid)?;
            let row = sqlx::query(
                "SELECT workspace_id, package_id, version, display_name, description, capability_root_id, \
                        server_name, tool_names, input_artifact_types, output_artifact_types, \
                        content_sha256 \
                 FROM workspace_capability_package_releases \
                 WHERE id = $1 AND organization_id = $2 AND state = 'published'",
            )
            .bind(release_id)
            .bind(organization_id)
            .fetch_optional(db)
            .await
            .map_err(|_| AgentCatalogError::Database)?
            .ok_or(AgentCatalogError::NotFound)?;
            let package_id: String = row.get("package_id");
            let version: String = row.get("version");
            if package_id != selection.definition_id || version != selection.version {
                return Err(AgentCatalogError::Invalid);
            }
            let tool_names = serde_json::from_value::<Vec<String>>(row.get("tool_names"))
                .map_err(|_| AgentCatalogError::Invalid)?;
            let input_artifact_types =
                serde_json::from_value::<Vec<String>>(row.get("input_artifact_types"))
                    .map_err(|_| AgentCatalogError::Invalid)?;
            let output_artifact_types =
                serde_json::from_value::<Vec<String>>(row.get("output_artifact_types"))
                    .map_err(|_| AgentCatalogError::Invalid)?;
            agent::workspace_capability_template(
                release_id,
                row.get("workspace_id"),
                package_id,
                version,
                row.get("display_name"),
                row.get("description"),
                row.get("capability_root_id"),
                row.get("server_name"),
                tool_names,
                input_artifact_types,
                output_artifact_types,
                row.get("content_sha256"),
            )
            .map_err(|_| AgentCatalogError::Invalid)
        }
    }
}
