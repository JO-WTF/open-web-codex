use std::collections::{BTreeMap, BTreeSet};

use open_web_codex_adapter::{PlatformRuntimeRole, RequiredMcpServer};
use open_web_codex_platform_contracts::{SupervisorPolicySelection, SupervisorPolicySummary};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

use crate::agent::{self, AgentCatalogError};
use crate::seal::{package_content_sha256, validate_artifact_contracts};
use crate::validation::{
    is_safe_capability_segment, is_safe_definition_id, is_safe_runtime_role_name, is_safe_version,
};

const ENTERPRISE_COPILOT_MANIFEST: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../../../capabilities/supervisors/enterprise-supervisor-copilot/1.7.0/manifest.json"
));
const ENTERPRISE_COPILOT_INSTRUCTIONS: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../../../capabilities/supervisors/enterprise-supervisor-copilot/1.7.0/instructions.md"
));
const ENTERPRISE_COPILOT_ARTIFACT_CONTRACTS: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../../../capabilities/supervisors/enterprise-supervisor-copilot/1.7.0/artifact-contracts.json"
));

const MAX_SUPERVISOR_INSTRUCTIONS_BYTES: usize = 16 * 1024;

struct PublishedSupervisorResource {
    manifest: &'static str,
    developer_instructions: &'static str,
    artifact_contracts: &'static str,
}

const PUBLISHED_SUPERVISOR_RESOURCES: [PublishedSupervisorResource; 1] =
    [PublishedSupervisorResource {
        manifest: ENTERPRISE_COPILOT_MANIFEST,
        developer_instructions: ENTERPRISE_COPILOT_INSTRUCTIONS,
        artifact_contracts: ENTERPRISE_COPILOT_ARTIFACT_CONTRACTS,
    }];

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct SupervisorPackageManifest {
    schema_version: String,
    policy_id: String,
    version: String,
    display_name: String,
    description: String,
    responsibilities: Vec<String>,
    instructions_file: String,
    artifact_contracts_file: String,
    agents: Vec<SupervisorAgentReference>,
    runtime_requirements: Vec<RuntimeCapabilityRequirement>,
    execution: SupervisorExecutionContract,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SupervisorAgentReference {
    pub definition_id: String,
    pub version: String,
    pub runtime_role: String,
    pub spawn_limit: u32,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeCapabilityRequirement {
    pub capability_id: String,
    pub version: String,
    pub required_limits: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct SupervisorExecutionContract {
    max_active_child_agents: u32,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct ArtifactContractSet {
    schema_version: String,
    contracts: Vec<ArtifactContract>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ArtifactContract {
    pub artifact_type: String,
    pub producer_agent: String,
    pub consumer_agents: Vec<String>,
    pub handoff: String,
    pub required: bool,
}

/// Validated authoring contract stored by the platform for an immutable
/// Supervisor Release. It references published Agents and Runtime capabilities;
/// it never contains executable Tool implementations or credentials.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SupervisorReleaseSpec {
    pub policy_id: String,
    pub version: String,
    pub display_name: String,
    pub description: String,
    pub responsibilities: Vec<String>,
    pub developer_instructions: String,
    pub agents: Vec<SupervisorAgentReference>,
    pub runtime_requirements: Vec<RuntimeCapabilityRequirement>,
    pub artifact_contracts: Vec<ArtifactContract>,
    pub max_active_child_agents: u32,
}

#[derive(Debug, Clone)]
pub struct ResolvedSupervisorPackage {
    pub policy_id: String,
    pub version: String,
    pub display_name: String,
    pub description: String,
    pub responsibilities: Vec<String>,
    pub developer_instructions: String,
    pub content_sha256: String,
    pub required_runtime_roles: Vec<PlatformRuntimeRole>,
    pub role_spawn_limits: BTreeMap<String, u32>,
    pub required_mcp_servers: Vec<RequiredMcpServer>,
    pub runtime_requirements: Vec<RuntimeCapabilityRequirement>,
    pub artifact_contracts: Vec<ArtifactContract>,
    pub max_active_child_agents: u32,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum SupervisorCatalogError {
    #[error("Supervisor Package was not found")]
    NotFound,
    #[error("Supervisor Package is invalid: {0}")]
    Invalid(&'static str),
}

pub fn list_published() -> Result<Vec<SupervisorPolicySummary>, SupervisorCatalogError> {
    PUBLISHED_SUPERVISOR_RESOURCES
        .iter()
        .map(|resource| {
            let package = parse_resource(resource)?;
            Ok(SupervisorPolicySummary {
                policy_id: package.policy_id,
                version: package.version,
                display_name: package.display_name,
                description: package.description,
            })
        })
        .collect()
}

pub fn governed_runtime_requirements() -> Vec<RuntimeCapabilityRequirement> {
    vec![RuntimeCapabilityRequirement {
        capability_id: "agents.multi_agent".to_string(),
        version: "1.0.0".to_string(),
        required_limits: [
            ("exactRoleAllowlist".to_string(), Value::Bool(true)),
            ("exactRoleInstanceLimits".to_string(), Value::Bool(true)),
        ]
        .into_iter()
        .collect(),
    }]
}

pub fn resolve(
    selection: &SupervisorPolicySelection,
) -> Result<ResolvedSupervisorPackage, SupervisorCatalogError> {
    let resource = PUBLISHED_SUPERVISOR_RESOURCES
        .iter()
        .find(|resource| {
            serde_json::from_str::<SupervisorPackageManifest>(resource.manifest).is_ok_and(
                |manifest| {
                    manifest.policy_id == selection.policy_id
                        && manifest.version == selection.version
                },
            )
        })
        .ok_or(SupervisorCatalogError::NotFound)?;
    parse_resource(resource)
}

fn parse_resource(
    resource: &PublishedSupervisorResource,
) -> Result<ResolvedSupervisorPackage, SupervisorCatalogError> {
    let manifest = serde_json::from_str::<SupervisorPackageManifest>(resource.manifest)
        .map_err(|_| SupervisorCatalogError::Invalid("manifest JSON is invalid"))?;
    validate_manifest(&manifest)?;
    let developer_instructions = resource.developer_instructions.trim();
    if developer_instructions.is_empty()
        || developer_instructions.len() > MAX_SUPERVISOR_INSTRUCTIONS_BYTES
    {
        return Err(SupervisorCatalogError::Invalid(
            "developer instructions are invalid",
        ));
    }
    let artifact_contracts =
        serde_json::from_str::<ArtifactContractSet>(resource.artifact_contracts)
            .map_err(|_| SupervisorCatalogError::Invalid("Artifact contracts JSON is invalid"))?;
    if artifact_contracts.schema_version != "artifact-contract-set.v1" {
        return Err(SupervisorCatalogError::Invalid(
            "Artifact contract set version is invalid",
        ));
    }
    validate_release(SupervisorReleaseSpec {
        policy_id: manifest.policy_id,
        version: manifest.version,
        display_name: manifest.display_name,
        description: manifest.description,
        responsibilities: manifest.responsibilities,
        developer_instructions: developer_instructions.to_string(),
        agents: manifest.agents,
        runtime_requirements: manifest.runtime_requirements,
        artifact_contracts: artifact_contracts.contracts,
        max_active_child_agents: manifest.execution.max_active_child_agents,
    })
}

/// Validate and resolve a database-authored immutable Release through the same
/// code-published Agent and Runtime contracts used by built-in packages.
pub fn validate_release(
    spec: SupervisorReleaseSpec,
) -> Result<ResolvedSupervisorPackage, SupervisorCatalogError> {
    validate_release_fields(&spec)?;
    let mut roles = Vec::with_capacity(spec.agents.len());
    let mut role_spawn_limits = BTreeMap::new();
    let mut contracts = BTreeMap::new();
    for reference in &spec.agents {
        let role = agent::resolve_runtime_role(
            &reference.definition_id,
            &reference.version,
            &reference.runtime_role,
        )
        .map_err(map_agent_error)?;
        let contract = agent::contract(
            &reference.definition_id,
            &reference.version,
            &reference.runtime_role,
        )
        .map_err(map_agent_error)?;
        role_spawn_limits.insert(reference.runtime_role.clone(), reference.spawn_limit);
        contracts.insert(
            format!("{}@{}", reference.definition_id, reference.version),
            contract,
        );
        roles.push(role);
    }
    let artifact_contracts = ArtifactContractSet {
        schema_version: "artifact-contract-set.v1".to_string(),
        contracts: spec.artifact_contracts.clone(),
    };
    validate_artifact_contracts(&artifact_contracts.contracts, &contracts)?;
    let required_mcp_servers = agent::required_mcp_servers(&roles).map_err(map_agent_error)?;
    let content_sha256 = package_content_sha256(
        &spec,
        &roles,
        &required_mcp_servers,
        &artifact_contracts.contracts,
    );
    Ok(ResolvedSupervisorPackage {
        policy_id: spec.policy_id,
        version: spec.version,
        display_name: spec.display_name,
        description: spec.description,
        responsibilities: spec.responsibilities,
        developer_instructions: spec.developer_instructions,
        content_sha256,
        required_runtime_roles: roles,
        role_spawn_limits,
        required_mcp_servers,
        runtime_requirements: spec.runtime_requirements,
        artifact_contracts: artifact_contracts.contracts,
        max_active_child_agents: spec.max_active_child_agents,
    })
}

fn validate_manifest(manifest: &SupervisorPackageManifest) -> Result<(), SupervisorCatalogError> {
    if manifest.schema_version != "supervisor-package.v1"
        || manifest.instructions_file != "instructions.md"
        || manifest.artifact_contracts_file != "artifact-contracts.json"
        || !is_safe_definition_id(&manifest.policy_id)
        || !is_safe_version(&manifest.version)
        || manifest.display_name.trim().is_empty()
        || manifest.display_name.len() > 256
        || manifest.description.trim().is_empty()
        || manifest.description.len() > 512
        || manifest.responsibilities.is_empty()
        || manifest.responsibilities.len() > 32
        || manifest
            .responsibilities
            .iter()
            .any(|value| value.trim().is_empty() || value.len() > 512)
        || manifest.agents.is_empty()
        || manifest.agents.len() > 16
        || !(1..=16).contains(&manifest.execution.max_active_child_agents)
        || manifest.runtime_requirements.is_empty()
        || manifest.runtime_requirements.len() > 16
    {
        return Err(SupervisorCatalogError::Invalid(
            "manifest fields are invalid",
        ));
    }
    let mut agent_refs = BTreeSet::new();
    let mut runtime_roles = BTreeSet::new();
    for reference in &manifest.agents {
        if !is_safe_definition_id(&reference.definition_id)
            || !is_safe_version(&reference.version)
            || !is_safe_runtime_role_name(&reference.runtime_role)
            || !(1..=16).contains(&reference.spawn_limit)
            || !agent_refs.insert(format!("{}@{}", reference.definition_id, reference.version))
            || !runtime_roles.insert(reference.runtime_role.as_str())
        {
            return Err(SupervisorCatalogError::Invalid(
                "Agent references are invalid",
            ));
        }
    }
    let mut runtime_capabilities = BTreeSet::new();
    for requirement in &manifest.runtime_requirements {
        if !is_safe_runtime_capability_id(&requirement.capability_id)
            || !is_safe_version(&requirement.version)
            || requirement.required_limits.is_empty()
            || requirement
                .required_limits
                .iter()
                .any(|(key, value)| !is_safe_limit_name(key) || !value.is_boolean())
            || !runtime_capabilities.insert(requirement.capability_id.as_str())
        {
            return Err(SupervisorCatalogError::Invalid(
                "Runtime requirements are invalid",
            ));
        }
    }
    Ok(())
}

fn validate_release_fields(spec: &SupervisorReleaseSpec) -> Result<(), SupervisorCatalogError> {
    if !is_safe_definition_id(&spec.policy_id)
        || !is_safe_version(&spec.version)
        || spec.display_name.trim().is_empty()
        || spec.display_name.len() > 256
        || spec.description.trim().is_empty()
        || spec.description.len() > 512
        || spec.responsibilities.is_empty()
        || spec.responsibilities.len() > 32
        || spec
            .responsibilities
            .iter()
            .any(|value| value.trim().is_empty() || value.len() > 512)
        || spec.developer_instructions.trim().is_empty()
        || spec.developer_instructions.len() > MAX_SUPERVISOR_INSTRUCTIONS_BYTES
        || spec.agents.is_empty()
        || spec.agents.len() > 16
        || !(1..=16).contains(&spec.max_active_child_agents)
        || spec.runtime_requirements.is_empty()
        || spec.runtime_requirements.len() > 16
    {
        return Err(SupervisorCatalogError::Invalid(
            "Release fields are invalid",
        ));
    }
    let manifest = SupervisorPackageManifest {
        schema_version: "supervisor-package.v1".to_string(),
        policy_id: spec.policy_id.clone(),
        version: spec.version.clone(),
        display_name: spec.display_name.clone(),
        description: spec.description.clone(),
        responsibilities: spec.responsibilities.clone(),
        instructions_file: "instructions.md".to_string(),
        artifact_contracts_file: "artifact-contracts.json".to_string(),
        agents: spec.agents.clone(),
        runtime_requirements: spec.runtime_requirements.clone(),
        execution: SupervisorExecutionContract {
            max_active_child_agents: spec.max_active_child_agents,
        },
    };
    validate_manifest(&manifest)
}

fn map_agent_error(error: AgentCatalogError) -> SupervisorCatalogError {
    match error {
        AgentCatalogError::NotFound => {
            SupervisorCatalogError::Invalid("required Agent Definition is not published")
        }
        AgentCatalogError::Invalid => {
            SupervisorCatalogError::Invalid("published Agent Definition is invalid")
        }
    }
}

fn is_safe_runtime_capability_id(value: &str) -> bool {
    value.split('.').count().gt(&1)
        && value.split('.').all(is_safe_capability_segment)
        && value.len() <= 128
}

fn is_safe_limit_name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_complete_published_package() {
        let summaries = list_published().unwrap();
        assert_eq!(summaries.len(), 1);
        let package = resolve(&SupervisorPolicySelection {
            policy_id: summaries[0].policy_id.clone(),
            version: summaries[0].version.clone(),
        })
        .unwrap();
        assert_eq!(package.required_runtime_roles.len(), 2);
        assert_eq!(package.artifact_contracts.len(), 3);
        assert_eq!(package.max_active_child_agents, 2);
        assert_eq!(package.content_sha256.len(), 64);
        assert_eq!(
            package.role_spawn_limits,
            [
                ("data_agent".to_string(), 1),
                ("network_planning_agent".to_string(), 1)
            ]
            .into_iter()
            .collect()
        );
    }

    #[test]
    fn rejects_unknown_package_without_fallback() {
        assert_eq!(
            resolve(&SupervisorPolicySelection {
                policy_id: "enterprise-supervisor-copilot".to_string(),
                version: "1.6.0".to_string(),
            })
            .unwrap_err(),
            SupervisorCatalogError::NotFound
        );
    }
}
