use open_web_codex_adapter::{PlatformRuntimeRole, RequiredMcpServer};
use open_web_codex_codex_contracts::{CapabilityDeclaration, CapabilityManifest, CapabilityStatus};
use open_web_codex_platform_contracts::{SupervisorPolicySelection, SupervisorPolicySummary};
use open_web_codex_run_orchestrator::{SupervisorPolicySnapshotInput, SupervisorPolicySource};
use open_web_codex_supervisor_catalog::supervisor::{
    self, ResolvedSupervisorPackage, RuntimeCapabilityRequirement, SupervisorCatalogError,
    SupervisorReleaseSpec,
};
use serde_json::Value;
use sqlx::{PgPool, Row};
use std::collections::BTreeMap;
use thiserror::Error;
use uuid::Uuid;

use crate::agent_catalog;

#[derive(Debug, Error, PartialEq, Eq)]
pub(crate) enum SupervisorPolicyError {
    #[error("Supervisor Policy was not found")]
    NotFound,
    #[error("Supervisor Policy content is invalid: {0}")]
    Invalid(&'static str),
    #[error("required Runtime capabilities are unavailable: {0}")]
    Capability(String),
    #[error("Supervisor Policy database operation failed")]
    Database,
}

#[derive(Debug)]
pub(crate) struct ResolvedSupervisorPolicy {
    pub snapshot: SupervisorPolicySnapshotInput,
    pub required_runtime_roles: Vec<PlatformRuntimeRole>,
    pub role_spawn_limits: BTreeMap<String, u32>,
    pub required_mcp_servers: Vec<RequiredMcpServer>,
    pub runtime_requirements: Vec<RuntimeCapabilityRequirement>,
    pub max_active_child_agents: u32,
}

pub(crate) async fn list_published(
    db: &PgPool,
    organization_id: Uuid,
) -> Result<Vec<SupervisorPolicySummary>, SupervisorPolicyError> {
    let mut published = supervisor::list_published().map_err(map_catalog_error)?;
    let rows = sqlx::query(
        "SELECT policy_id, version, display_name, description \
         FROM supervisor_releases \
         WHERE organization_id = $1 \
         ORDER BY published_at DESC, policy_id, version",
    )
    .bind(organization_id)
    .fetch_all(db)
    .await
    .map_err(|_| SupervisorPolicyError::Database)?;
    published.extend(rows.into_iter().map(|row| SupervisorPolicySummary {
        policy_id: row.get("policy_id"),
        version: row.get("version"),
        display_name: row.get("display_name"),
        description: row.get("description"),
    }));
    Ok(published)
}

pub(crate) async fn resolve(
    db: &PgPool,
    organization_id: Uuid,
    selection: &SupervisorPolicySelection,
) -> Result<ResolvedSupervisorPolicy, SupervisorPolicyError> {
    let builtins = supervisor::list_published().map_err(map_catalog_error)?;
    if builtins
        .iter()
        .any(|policy| policy.policy_id == selection.policy_id)
    {
        let package = supervisor::resolve(selection).map_err(map_catalog_error)?;
        return Ok(from_package(
            package,
            SupervisorPolicySource::Repository,
            None,
        ));
    }
    let row = sqlx::query(
        "SELECT id, release_spec, content_sha256 \
         FROM supervisor_releases \
         WHERE organization_id = $1 AND policy_id = $2 AND version = $3",
    )
    .bind(organization_id)
    .bind(&selection.policy_id)
    .bind(&selection.version)
    .fetch_optional(db)
    .await
    .map_err(|_| SupervisorPolicyError::Database)?
    .ok_or(SupervisorPolicyError::NotFound)?;
    resolve_release_row(db, organization_id, &row).await
}

pub(crate) fn resolve_builtin(
    selection: &SupervisorPolicySelection,
) -> Result<ResolvedSupervisorPolicy, SupervisorPolicyError> {
    let package = supervisor::resolve(selection).map_err(map_catalog_error)?;
    Ok(from_package(
        package,
        SupervisorPolicySource::Repository,
        None,
    ))
}

pub(crate) async fn resolve_release(
    db: &PgPool,
    organization_id: Uuid,
    release_id: Uuid,
) -> Result<ResolvedSupervisorPolicy, SupervisorPolicyError> {
    let row = sqlx::query(
        "SELECT id, release_spec, content_sha256 \
         FROM supervisor_releases WHERE organization_id = $1 AND id = $2",
    )
    .bind(organization_id)
    .bind(release_id)
    .fetch_optional(db)
    .await
    .map_err(|_| SupervisorPolicyError::Database)?
    .ok_or(SupervisorPolicyError::NotFound)?;
    resolve_release_row(db, organization_id, &row).await
}

fn from_package(
    package: ResolvedSupervisorPackage,
    source: SupervisorPolicySource,
    release_id: Option<Uuid>,
) -> ResolvedSupervisorPolicy {
    ResolvedSupervisorPolicy {
        snapshot: SupervisorPolicySnapshotInput {
            policy_id: package.policy_id,
            version: package.version,
            display_name: package.display_name,
            developer_instructions: package.developer_instructions,
            content_sha256: package.content_sha256,
            source,
            release_id,
        },
        required_runtime_roles: package.required_runtime_roles,
        role_spawn_limits: package.role_spawn_limits,
        required_mcp_servers: package.required_mcp_servers,
        runtime_requirements: package.runtime_requirements,
        max_active_child_agents: package.max_active_child_agents,
    }
}

async fn resolve_release_row(
    db: &PgPool,
    organization_id: Uuid,
    row: &sqlx::postgres::PgRow,
) -> Result<ResolvedSupervisorPolicy, SupervisorPolicyError> {
    let release_id: Uuid = row.get("id");
    let spec = serde_json::from_value::<SupervisorReleaseSpec>(row.get("release_spec"))
        .map_err(|_| SupervisorPolicyError::Invalid("Release specification is invalid"))?;
    let available_agents = agent_catalog::list_resolved(db, organization_id)
        .await
        .map_err(|_| SupervisorPolicyError::Invalid("Agent catalog is invalid"))?;
    let package = supervisor::validate_release_with_agents(spec.clone(), &available_agents)
        .map_err(map_catalog_error)?;
    if package.content_sha256 != row.get::<String, _>("content_sha256") {
        return Err(SupervisorPolicyError::Invalid(
            "Release content hash does not match",
        ));
    }
    verify_agent_dependencies(db, organization_id, release_id, &spec, &available_agents).await?;
    Ok(from_package(
        package,
        SupervisorPolicySource::UserRelease,
        Some(release_id),
    ))
}

async fn verify_agent_dependencies(
    db: &PgPool,
    organization_id: Uuid,
    supervisor_release_id: Uuid,
    spec: &SupervisorReleaseSpec,
    available_agents: &[open_web_codex_supervisor_catalog::agent::ResolvedAgentDefinition],
) -> Result<(), SupervisorPolicyError> {
    let rows = sqlx::query(
        "SELECT agent_release_id, agent_definition_id, agent_version, agent_content_sha256 \
         FROM supervisor_release_agent_dependencies \
         WHERE organization_id = $1 AND supervisor_release_id = $2",
    )
    .bind(organization_id)
    .bind(supervisor_release_id)
    .fetch_all(db)
    .await
    .map_err(|_| SupervisorPolicyError::Database)?;
    let expected = spec
        .agents
        .iter()
        .filter_map(|reference| {
            reference.release_id.map(|release_id| {
                let content_sha256 = available_agents
                    .iter()
                    .find(|definition| definition.release_id == Some(release_id))
                    .map(|definition| definition.content_sha256.clone());
                (
                    release_id,
                    reference.definition_id.clone(),
                    reference.version.clone(),
                    content_sha256,
                )
            })
        })
        .collect::<Vec<_>>();
    if rows.len() != expected.len()
        || expected
            .iter()
            .any(|(release_id, definition_id, version, content_sha256)| {
                let Some(content_sha256) = content_sha256 else {
                    return true;
                };
                !rows.iter().any(|row| {
                    row.get::<Uuid, _>("agent_release_id") == *release_id
                        && row.get::<String, _>("agent_definition_id") == *definition_id
                        && row.get::<String, _>("agent_version") == *version
                        && row.get::<String, _>("agent_content_sha256") == *content_sha256
                })
            })
    {
        return Err(SupervisorPolicyError::Invalid(
            "Agent Release dependencies do not match",
        ));
    }
    Ok(())
}

pub(crate) async fn resolve_for_new_run(
    db: &PgPool,
    organization_id: Uuid,
    selection: &SupervisorPolicySelection,
) -> Result<ResolvedSupervisorPolicy, SupervisorPolicyError> {
    resolve(db, organization_id, selection).await
}

pub(crate) fn is_reserved_builtin_policy_id(policy_id: &str) -> bool {
    supervisor::list_published()
        .is_ok_and(|policies| policies.iter().any(|policy| policy.policy_id == policy_id))
}

pub(crate) fn require_runtime_manifest(
    manifest: &Value,
    requirements: &[RuntimeCapabilityRequirement],
) -> Result<(), SupervisorPolicyError> {
    let manifest =
        serde_json::from_value::<CapabilityManifest>(manifest.clone()).map_err(|_| {
            SupervisorPolicyError::Capability(
                "Codex Capability Manifest could not be validated".to_string(),
            )
        })?;
    for requirement in requirements {
        let capability = manifest
            .capabilities
            .iter()
            .find(|capability| capability.id == requirement.capability_id)
            .ok_or_else(|| {
                SupervisorPolicyError::Capability(format!(
                    "Codex did not declare '{}' support",
                    requirement.capability_id
                ))
            })?;
        validate_runtime_capability(capability, requirement)?;
    }
    Ok(())
}

fn validate_runtime_capability(
    capability: &CapabilityDeclaration,
    requirement: &RuntimeCapabilityRequirement,
) -> Result<(), SupervisorPolicyError> {
    if capability.version != requirement.version {
        return Err(SupervisorPolicyError::Capability(format!(
            "Codex '{}' capability version '{}' is unsupported",
            requirement.capability_id, capability.version,
        )));
    }
    let enabled = match &capability.status {
        CapabilityStatus::Supported => true,
        CapabilityStatus::Experimental => capability.experimental,
        CapabilityStatus::Unsupported
        | CapabilityStatus::Degraded
        | CapabilityStatus::Incompatible => false,
    };
    if !enabled {
        return Err(SupervisorPolicyError::Capability(format!(
            "Codex '{}' capability is unavailable",
            requirement.capability_id
        )));
    }
    for (limit, required_value) in &requirement.required_limits {
        if capability.limits.get(limit) != Some(required_value) {
            return Err(SupervisorPolicyError::Capability(format!(
                "Codex '{}' capability does not satisfy required limit '{}'",
                requirement.capability_id, limit
            )));
        }
    }
    Ok(())
}

fn map_catalog_error(error: SupervisorCatalogError) -> SupervisorPolicyError {
    match error {
        SupervisorCatalogError::NotFound => SupervisorPolicyError::NotFound,
        SupervisorCatalogError::Invalid(message) => SupervisorPolicyError::Invalid(message),
    }
}

#[cfg(test)]
#[path = "supervisor_policy_tests.rs"]
mod tests;
