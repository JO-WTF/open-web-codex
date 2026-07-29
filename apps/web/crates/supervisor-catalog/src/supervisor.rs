use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

use open_web_codex_adapter::{PlatformRuntimeRole, RequiredMcpServer};
use open_web_codex_platform_contracts::{
    SupervisorAgentSelection, SupervisorArtifactContractInput, SupervisorDraftRequest,
    SupervisorInstructionPolicyDetail, SupervisorInstructionPolicySelection,
    SupervisorInstructionPolicySummary, SupervisorPolicyOrigin, SupervisorPolicySelection,
    SupervisorPolicySummary,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;
use uuid::Uuid;

use crate::agent::{self, AgentCatalogError, ResolvedAgentDefinition};
use crate::seal::{package_content_sha256, validate_artifact_contracts};
use crate::validation::{
    is_safe_capability_segment, is_safe_definition_id, is_safe_runtime_role_name, is_safe_version,
};

const ENTERPRISE_COPILOT_MANIFEST: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../../../capabilities/supervisors/enterprise-supervisor-copilot/1.9.0/manifest.json"
));
const ENTERPRISE_COPILOT_CUSTOM_INSTRUCTIONS: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../../../capabilities/supervisors/enterprise-supervisor-copilot/1.9.0/custom-instructions.md"
));
const ENTERPRISE_COPILOT_ARTIFACT_CONTRACTS: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../../../capabilities/supervisors/enterprise-supervisor-copilot/1.9.0/artifact-contracts.json"
));

const MAX_SUPERVISOR_INSTRUCTIONS_BYTES: usize = 16 * 1024;

struct PublishedSupervisorResource {
    manifest: &'static str,
    custom_instructions: &'static str,
    artifact_contracts: &'static str,
}

const PUBLISHED_SUPERVISOR_RESOURCES: [PublishedSupervisorResource; 1] =
    [PublishedSupervisorResource {
        manifest: ENTERPRISE_COPILOT_MANIFEST,
        custom_instructions: ENTERPRISE_COPILOT_CUSTOM_INSTRUCTIONS,
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
    instruction_policy: SupervisorInstructionPolicySelection,
    custom_instructions_file: String,
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
    #[serde(default)]
    pub release_id: Option<Uuid>,
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
    pub instruction_policy: SupervisorInstructionPolicySelection,
    pub instruction_policy_sha256: String,
    pub custom_instructions: String,
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
    pub instruction_policy: SupervisorInstructionPolicySummary,
    pub platform_instructions: String,
    pub custom_instructions: String,
    pub developer_instructions: String,
    pub agents: Vec<SupervisorAgentReference>,
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
    #[error("Agent Definition '{0}' is not published")]
    AgentNotPublished(String),
    #[error("Supervisor instruction policy '{0}' is not published")]
    InstructionPolicyNotPublished(String),
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
                source: SupervisorPolicyOrigin::Repository,
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
    parse_resource_sources(
        resource.manifest,
        resource.custom_instructions,
        resource.artifact_contracts,
    )
}

fn parse_resource_sources(
    manifest_source: &str,
    custom_instructions_source: &str,
    artifact_contracts_source: &str,
) -> Result<ResolvedSupervisorPackage, SupervisorCatalogError> {
    let manifest = serde_json::from_str::<SupervisorPackageManifest>(manifest_source)
        .map_err(|_| SupervisorCatalogError::Invalid("manifest JSON is invalid"))?;
    validate_manifest(&manifest)?;
    let custom_instructions = custom_instructions_source.trim();
    if custom_instructions.is_empty()
        || custom_instructions.len() > MAX_SUPERVISOR_INSTRUCTIONS_BYTES
    {
        return Err(SupervisorCatalogError::Invalid(
            "custom instructions are invalid",
        ));
    }
    let artifact_contracts = serde_json::from_str::<ArtifactContractSet>(artifact_contracts_source)
        .map_err(|_| SupervisorCatalogError::Invalid("Artifact contracts JSON is invalid"))?;
    if artifact_contracts.schema_version != "artifact-contract-set.v1" {
        return Err(SupervisorCatalogError::Invalid(
            "Artifact contract set version is invalid",
        ));
    }
    if artifact_contracts
        .contracts
        .iter()
        .any(|contract| contract.handoff != "durable-resource-reference")
    {
        return Err(SupervisorCatalogError::Invalid(
            "Artifact handoff must match the platform contract",
        ));
    }
    let declared_agents = manifest.agents.clone();
    let declared_runtime_requirements = manifest.runtime_requirements.clone();
    let draft = SupervisorDraftRequest {
        policy_id: manifest.policy_id,
        version: manifest.version,
        display_name: manifest.display_name,
        description: manifest.description,
        responsibilities: manifest.responsibilities,
        instruction_policy: manifest.instruction_policy,
        custom_instructions: custom_instructions.to_string(),
        agents: declared_agents
            .iter()
            .map(|reference| SupervisorAgentSelection {
                definition_id: reference.definition_id.clone(),
                version: reference.version.clone(),
                release_id: reference.release_id,
                spawn_limit: reference.spawn_limit,
            })
            .collect(),
        artifact_contracts: artifact_contracts
            .contracts
            .iter()
            .map(|contract| SupervisorArtifactContractInput {
                artifact_type: contract.artifact_type.clone(),
                producer_agent: contract.producer_agent.clone(),
                consumer_agents: contract.consumer_agents.clone(),
                required: contract.required,
            })
            .collect(),
        max_active_child_agents: manifest.execution.max_active_child_agents,
    };
    let available_agents = agent::list_resolved_builtins().map_err(map_agent_error)?;
    let package = resolve_authoring_spec(draft, &available_agents)?;
    if declared_runtime_requirements != package.runtime_requirements
        || declared_agents != package.agents
    {
        return Err(SupervisorCatalogError::Invalid(
            "repository package derived Runtime fields do not match the canonical compiler",
        ));
    }
    Ok(package)
}

/// Validate and resolve a database-authored immutable Release through the same
/// code-published Agent and Runtime contracts used by built-in packages.
pub fn validate_release(
    spec: SupervisorReleaseSpec,
) -> Result<ResolvedSupervisorPackage, SupervisorCatalogError> {
    let available_agents = agent::list_resolved_builtins().map_err(map_agent_error)?;
    validate_release_with_agents(spec, &available_agents)
}

/// Compile the one browser-safe authoring contract into the same immutable
/// execution package used by repository definitions. Runtime Role names,
/// capability requirements and Artifact handoff encoding are always derived.
pub fn resolve_authoring_spec(
    draft: SupervisorDraftRequest,
    available_agents: &[ResolvedAgentDefinition],
) -> Result<ResolvedSupervisorPackage, SupervisorCatalogError> {
    let instruction_policy = crate::instruction_policy::resolve(&draft.instruction_policy)?;
    resolve_authoring_spec_with_policy(draft, available_agents, &instruction_policy)
}

pub fn resolve_authoring_spec_with_policy(
    draft: SupervisorDraftRequest,
    available_agents: &[ResolvedAgentDefinition],
    instruction_policy: &SupervisorInstructionPolicyDetail,
) -> Result<ResolvedSupervisorPackage, SupervisorCatalogError> {
    let spec =
        release_spec_from_authoring_with_policy(draft, available_agents, instruction_policy)?;
    validate_release_with_agents_and_policy(spec, available_agents, instruction_policy)
}

pub fn release_spec_from_authoring(
    draft: SupervisorDraftRequest,
    available_agents: &[ResolvedAgentDefinition],
) -> Result<SupervisorReleaseSpec, SupervisorCatalogError> {
    let instruction_policy = crate::instruction_policy::resolve(&draft.instruction_policy)?;
    release_spec_from_authoring_with_policy(draft, available_agents, &instruction_policy)
}

pub fn release_spec_from_authoring_with_policy(
    draft: SupervisorDraftRequest,
    available_agents: &[ResolvedAgentDefinition],
    instruction_policy: &SupervisorInstructionPolicyDetail,
) -> Result<SupervisorReleaseSpec, SupervisorCatalogError> {
    if instruction_policy.policy_id != draft.instruction_policy.policy_id
        || instruction_policy.version != draft.instruction_policy.version
    {
        return Err(SupervisorCatalogError::Invalid(
            "Supervisor instruction policy identity does not match the draft",
        ));
    }
    let agents = draft
        .agents
        .iter()
        .map(|selection| {
            let identity = format!("{}@{}", selection.definition_id, selection.version);
            let definition = available_agents
                .iter()
                .find(|definition| {
                    definition.definition_id == selection.definition_id
                        && definition.version == selection.version
                        && definition.release_id == selection.release_id
                })
                .ok_or(SupervisorCatalogError::AgentNotPublished(identity))?;
            Ok(SupervisorAgentReference {
                definition_id: selection.definition_id.clone(),
                version: selection.version.clone(),
                release_id: selection.release_id,
                runtime_role: definition.runtime_role.name.clone(),
                spawn_limit: selection.spawn_limit,
            })
        })
        .collect::<Result<Vec<_>, SupervisorCatalogError>>()?;
    let artifact_contracts = draft
        .artifact_contracts
        .into_iter()
        .map(|contract| ArtifactContract {
            artifact_type: contract.artifact_type,
            producer_agent: contract.producer_agent,
            consumer_agents: contract.consumer_agents,
            handoff: "durable-resource-reference".to_string(),
            required: contract.required,
        })
        .collect();
    Ok(SupervisorReleaseSpec {
        policy_id: draft.policy_id,
        version: draft.version,
        display_name: draft.display_name,
        description: draft.description,
        responsibilities: draft.responsibilities,
        instruction_policy: draft.instruction_policy,
        instruction_policy_sha256: instruction_policy.content_sha256.clone(),
        custom_instructions: draft.custom_instructions,
        agents,
        runtime_requirements: governed_runtime_requirements(),
        artifact_contracts,
        max_active_child_agents: draft.max_active_child_agents,
    })
}

pub fn validate_release_with_agents(
    spec: SupervisorReleaseSpec,
    available_agents: &[ResolvedAgentDefinition],
) -> Result<ResolvedSupervisorPackage, SupervisorCatalogError> {
    let instruction_policy = crate::instruction_policy::resolve(&spec.instruction_policy)?;
    validate_release_with_agents_and_policy(spec, available_agents, &instruction_policy)
}

pub fn validate_release_with_agents_and_policy(
    spec: SupervisorReleaseSpec,
    available_agents: &[ResolvedAgentDefinition],
    instruction_policy: &SupervisorInstructionPolicyDetail,
) -> Result<ResolvedSupervisorPackage, SupervisorCatalogError> {
    validate_release_fields(&spec)?;
    if instruction_policy.policy_id != spec.instruction_policy.policy_id
        || instruction_policy.version != spec.instruction_policy.version
    {
        return Err(SupervisorCatalogError::Invalid(
            "Supervisor instruction policy identity does not match the Release",
        ));
    }
    if instruction_policy.content_sha256 != spec.instruction_policy_sha256 {
        return Err(SupervisorCatalogError::Invalid(
            "Supervisor instruction policy content does not match the Release",
        ));
    }
    let mut roles = Vec::with_capacity(spec.agents.len());
    let mut resolved_agents = Vec::with_capacity(spec.agents.len());
    let mut role_spawn_limits = BTreeMap::new();
    let mut contracts = BTreeMap::new();
    for reference in &spec.agents {
        let definition = available_agents
            .iter()
            .find(|definition| {
                definition.definition_id == reference.definition_id
                    && definition.version == reference.version
                    && definition.release_id == reference.release_id
                    && definition.runtime_role.name == reference.runtime_role
            })
            .ok_or_else(|| {
                SupervisorCatalogError::AgentNotPublished(format!(
                    "{}@{}",
                    reference.definition_id, reference.version
                ))
            })?;
        let role = definition.runtime_role.clone();
        let contract = definition.contract();
        role_spawn_limits.insert(reference.runtime_role.clone(), reference.spawn_limit);
        contracts.insert(
            format!("{}@{}", reference.definition_id, reference.version),
            contract,
        );
        roles.push(role);
        resolved_agents.push(definition.clone());
    }
    let artifact_contracts = ArtifactContractSet {
        schema_version: "artifact-contract-set.v1".to_string(),
        contracts: spec.artifact_contracts.clone(),
    };
    validate_artifact_contracts(&artifact_contracts.contracts, &contracts)?;
    let required_mcp_servers =
        agent::merge_required_mcp_servers(&resolved_agents).map_err(map_agent_error)?;
    let developer_instructions = compile_developer_instructions(
        instruction_policy,
        &spec.custom_instructions,
        &spec.agents,
        &artifact_contracts.contracts,
        spec.max_active_child_agents,
    )?;
    let content_sha256 = package_content_sha256(
        &spec,
        &roles,
        &required_mcp_servers,
        &artifact_contracts.contracts,
        &resolved_agents,
    );
    Ok(ResolvedSupervisorPackage {
        policy_id: spec.policy_id,
        version: spec.version,
        display_name: spec.display_name,
        description: spec.description,
        responsibilities: spec.responsibilities,
        instruction_policy: crate::instruction_policy::summary(instruction_policy),
        platform_instructions: instruction_policy.platform_instructions.clone(),
        custom_instructions: spec.custom_instructions,
        developer_instructions,
        agents: spec.agents,
        content_sha256,
        required_runtime_roles: roles,
        role_spawn_limits,
        required_mcp_servers,
        runtime_requirements: spec.runtime_requirements,
        artifact_contracts: artifact_contracts.contracts,
        max_active_child_agents: spec.max_active_child_agents,
    })
}

fn compile_developer_instructions(
    policy: &SupervisorInstructionPolicyDetail,
    custom_instructions: &str,
    agents: &[SupervisorAgentReference],
    artifact_contracts: &[ArtifactContract],
    max_active_child_agents: u32,
) -> Result<String, SupervisorCatalogError> {
    let mut compiled = format!(
        "# Platform Supervisor behavior contract\n\
         Policy: {}@{}\n\n{}\n\n\
         # Resolved execution contract\n\
         Maximum active child Agents: {}\n\n\
         ## Runtime Roles\n",
        policy.policy_id,
        policy.version,
        policy.platform_instructions.trim(),
        max_active_child_agents,
    );
    for agent in agents {
        writeln!(
            compiled,
            "- {}@{} => `agent_type: \"{}\"`; spawn limit {}",
            agent.definition_id, agent.version, agent.runtime_role, agent.spawn_limit
        )
        .expect("writing to String cannot fail");
    }
    compiled.push_str(
        "\n## Agent slot lifecycle\n\
         A child Agent remains active and occupies the Runtime concurrency limit after its turn \
         reaches a terminal state until the Supervisor calls `close_agent`. After collecting the \
         child's result and confirming every required durable Artifact is ready, close that child \
         before spawning another Agent whenever the maximum active-child limit would otherwise be \
         exceeded. Never close a child with an active turn or before its required Artifact handoff \
         is durable.\n",
    );
    compiled.push_str("\n## Artifact handoffs\n");
    for contract in artifact_contracts {
        writeln!(
            compiled,
            "- `{}`: {} -> {}; {}; handoff `{}`",
            contract.artifact_type,
            contract.producer_agent,
            contract.consumer_agents.join(", "),
            if contract.required {
                "required"
            } else {
                "optional"
            },
            contract.handoff,
        )
        .expect("writing to String cannot fail");
    }
    compiled.push_str("\n# Custom Supervisor instructions\n");
    compiled.push_str(custom_instructions.trim());
    if compiled.len() > MAX_SUPERVISOR_INSTRUCTIONS_BYTES {
        return Err(SupervisorCatalogError::Invalid(
            "compiled Supervisor instructions exceed the Runtime limit",
        ));
    }
    Ok(compiled)
}

fn validate_manifest(manifest: &SupervisorPackageManifest) -> Result<(), SupervisorCatalogError> {
    if manifest.schema_version != "supervisor-package.v1"
        || manifest.custom_instructions_file != "custom-instructions.md"
        || !is_safe_definition_id(&manifest.instruction_policy.policy_id)
        || !is_safe_version(&manifest.instruction_policy.version)
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
        || !is_safe_definition_id(&spec.instruction_policy.policy_id)
        || !is_safe_version(&spec.instruction_policy.version)
        || spec.instruction_policy_sha256.len() != 64
        || !spec
            .instruction_policy_sha256
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (byte.is_ascii_lowercase() && byte <= b'f'))
        || spec.custom_instructions.trim().is_empty()
        || spec.custom_instructions.len() > MAX_SUPERVISOR_INSTRUCTIONS_BYTES
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
        instruction_policy: spec.instruction_policy.clone(),
        custom_instructions_file: "custom-instructions.md".to_string(),
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
        assert_eq!(summaries[0].source, SupervisorPolicyOrigin::Repository);
        let package = resolve(&SupervisorPolicySelection {
            policy_id: summaries[0].policy_id.clone(),
            version: summaries[0].version.clone(),
        })
        .unwrap();
        assert_eq!(package.required_runtime_roles.len(), 2);
        assert_eq!(package.version, "1.9.0");
        assert_eq!(package.agents.len(), 2);
        assert_eq!(package.artifact_contracts.len(), 3);
        assert_eq!(package.max_active_child_agents, 2);
        assert_eq!(package.content_sha256.len(), 64);
        assert!(package
            .platform_instructions
            .contains("You are the root Supervisor"));
        assert!(package
            .custom_instructions
            .contains("warehouse-network case"));
        assert!(package
            .developer_instructions
            .contains("# Resolved execution contract"));
        assert!(package
            .developer_instructions
            .contains("A child Agent remains active and occupies the Runtime concurrency limit"));
        assert!(package
            .developer_instructions
            .contains("calls `close_agent`"));
        assert!(package
            .developer_instructions
            .contains("# Custom Supervisor instructions"));
        assert_eq!(
            package.role_spawn_limits,
            [
                ("agent_7e81fe6ff16d257b64a209abc623833c".to_string(), 1),
                ("agent_cc4182517eeeaeb65ac5b50da67da6ba".to_string(), 1)
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

    #[test]
    fn repository_and_web_supervisor_sources_compile_to_identical_execution_semantics() {
        let repository = resolve(&SupervisorPolicySelection {
            policy_id: "enterprise-supervisor-copilot".to_string(),
            version: "1.9.0".to_string(),
        })
        .unwrap();
        let draft = SupervisorDraftRequest {
            policy_id: repository.policy_id.clone(),
            version: repository.version.clone(),
            display_name: repository.display_name.clone(),
            description: repository.description.clone(),
            responsibilities: repository.responsibilities.clone(),
            instruction_policy: SupervisorInstructionPolicySelection {
                policy_id: repository.instruction_policy.policy_id.clone(),
                version: repository.instruction_policy.version.clone(),
            },
            custom_instructions: repository.custom_instructions.clone(),
            agents: repository
                .agents
                .iter()
                .map(|agent| SupervisorAgentSelection {
                    definition_id: agent.definition_id.clone(),
                    version: agent.version.clone(),
                    release_id: agent.release_id,
                    spawn_limit: agent.spawn_limit,
                })
                .collect(),
            artifact_contracts: repository
                .artifact_contracts
                .iter()
                .map(|contract| SupervisorArtifactContractInput {
                    artifact_type: contract.artifact_type.clone(),
                    producer_agent: contract.producer_agent.clone(),
                    consumer_agents: contract.consumer_agents.clone(),
                    required: contract.required,
                })
                .collect(),
            max_active_child_agents: repository.max_active_child_agents,
        };
        let available_agents = agent::list_resolved_builtins().unwrap();
        let web = resolve_authoring_spec(draft.clone(), &available_agents).unwrap();

        assert_eq!(repository.execution_semantics(), web.execution_semantics());
        assert_eq!(
            repository.execution_semantics_sha256(),
            web.execution_semantics_sha256()
        );

        let mut changed_draft = draft;
        changed_draft.max_active_child_agents = 1;
        let changed = resolve_authoring_spec(changed_draft, &available_agents).unwrap();
        assert_ne!(
            repository.execution_semantics_sha256(),
            changed.execution_semantics_sha256()
        );

        let mut changed_instructions = repository.custom_instructions.clone();
        changed_instructions.push_str("\nAdd a final evidence appendix.");
        let changed = resolve_authoring_spec(
            SupervisorDraftRequest {
                policy_id: repository.policy_id.clone(),
                version: repository.version.clone(),
                display_name: repository.display_name.clone(),
                description: repository.description.clone(),
                responsibilities: repository.responsibilities.clone(),
                instruction_policy: SupervisorInstructionPolicySelection {
                    policy_id: repository.instruction_policy.policy_id.clone(),
                    version: repository.instruction_policy.version.clone(),
                },
                custom_instructions: changed_instructions,
                agents: repository
                    .agents
                    .iter()
                    .map(|agent| SupervisorAgentSelection {
                        definition_id: agent.definition_id.clone(),
                        version: agent.version.clone(),
                        release_id: agent.release_id,
                        spawn_limit: agent.spawn_limit,
                    })
                    .collect(),
                artifact_contracts: repository
                    .artifact_contracts
                    .iter()
                    .map(|contract| SupervisorArtifactContractInput {
                        artifact_type: contract.artifact_type.clone(),
                        producer_agent: contract.producer_agent.clone(),
                        consumer_agents: contract.consumer_agents.clone(),
                        required: contract.required,
                    })
                    .collect(),
                max_active_child_agents: repository.max_active_child_agents,
            },
            &available_agents,
        )
        .unwrap();
        assert_ne!(
            repository.execution_semantics_sha256(),
            changed.execution_semantics_sha256()
        );
    }

    #[test]
    fn repository_derived_fields_cannot_drift_from_the_canonical_compiler() {
        let manifest = ENTERPRISE_COPILOT_MANIFEST.replacen(
            "\"runtimeRole\": \"agent_7e81fe6ff16d257b64a209abc623833c\"",
            "\"runtimeRole\": \"drifted_data_agent\"",
            1,
        );
        assert_eq!(
            parse_resource_sources(
                &manifest,
                ENTERPRISE_COPILOT_CUSTOM_INSTRUCTIONS,
                ENTERPRISE_COPILOT_ARTIFACT_CONTRACTS,
            )
            .unwrap_err(),
            SupervisorCatalogError::Invalid(
                "repository package derived Runtime fields do not match the canonical compiler"
            )
        );
    }
}
