use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

use open_web_codex_adapter::{CapabilityRootMcpInventory, PlatformRuntimeRole, RequiredMcpServer};
use open_web_codex_platform_contracts::{
    AgentCapabilityTemplateSource, AgentDatasetReleaseBinding, DataRequirementContractReference,
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

const INDONESIA_NETWORK_PLANNING_DRAFT_MANIFEST: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../../../capabilities/supervisors/enterprise-supervisor-copilot/6.0.0/manifest.json"
));
const INDONESIA_NETWORK_PLANNING_DRAFT_CUSTOM_INSTRUCTIONS: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../../../capabilities/supervisors/enterprise-supervisor-copilot/6.0.0/custom-instructions.md"
));
const INDONESIA_NETWORK_PLANNING_DRAFT_ARTIFACT_CONTRACTS: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../../../capabilities/supervisors/enterprise-supervisor-copilot/6.0.0/artifact-contracts.json"
));
const MAX_SUPERVISOR_INSTRUCTIONS_BYTES: usize = 16 * 1024;

struct PublishedSupervisorResource {
    manifest: &'static str,
    custom_instructions: &'static str,
    artifact_contracts: &'static str,
}

const PUBLISHED_SUPERVISOR_RESOURCES: [PublishedSupervisorResource; 1] =
    [PublishedSupervisorResource {
        manifest: INDONESIA_NETWORK_PLANNING_DRAFT_MANIFEST,
        custom_instructions: INDONESIA_NETWORK_PLANNING_DRAFT_CUSTOM_INSTRUCTIONS,
        artifact_contracts: INDONESIA_NETWORK_PLANNING_DRAFT_ARTIFACT_CONTRACTS,
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
    #[serde(default)]
    data_requirement_contracts: Vec<DataRequirementContractReference>,
    #[serde(default)]
    coordination_capabilities: Vec<String>,
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
    #[serde(default)]
    pub data_requirement_contracts: Vec<DataRequirementContractReference>,
    #[serde(default)]
    pub coordination_capabilities: Vec<String>,
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
    pub coordination_mcp_servers: Vec<RequiredMcpServer>,
    pub coordination_capabilities: Vec<String>,
    pub required_workspace_id: Option<Uuid>,
    pub workspace_capability_packages: Vec<WorkspaceCapabilityPackageRequirement>,
    pub dataset_releases: Vec<AgentDatasetReleaseBinding>,
    pub runtime_requirements: Vec<RuntimeCapabilityRequirement>,
    pub artifact_contracts: Vec<ArtifactContract>,
    pub data_requirement_contracts: Vec<DataRequirementContractReference>,
    pub max_active_child_agents: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceCapabilityPackageRequirement {
    pub release_id: Uuid,
    pub workspace_id: Uuid,
    pub package_id: String,
    pub version: String,
    pub content_sha256: String,
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
                draft_id: None,
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
    let instruction_policy = crate::instruction_policy::resolve(&manifest.instruction_policy)?;
    let draft = SupervisorDraftRequest {
        policy_id: manifest.policy_id,
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
        data_requirement_contracts: manifest.data_requirement_contracts,
        coordination_capabilities: manifest.coordination_capabilities,
        max_active_child_agents: manifest.execution.max_active_child_agents,
    };
    let available_agents = agent::list_resolved_builtins().map_err(map_agent_error)?;
    let package = resolve_authoring_spec_with_version(
        draft,
        &available_agents,
        &instruction_policy,
        &manifest.version,
    )?;
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
    let spec = release_spec_from_authoring_with_version(
        draft,
        available_agents,
        instruction_policy,
        "draft",
    )?;
    validate_release_with_agents_and_policy(spec, available_agents, instruction_policy)
}

pub fn resolve_authoring_spec_with_version(
    draft: SupervisorDraftRequest,
    available_agents: &[ResolvedAgentDefinition],
    instruction_policy: &SupervisorInstructionPolicyDetail,
    version: &str,
) -> Result<ResolvedSupervisorPackage, SupervisorCatalogError> {
    let spec = release_spec_from_authoring_with_version(
        draft,
        available_agents,
        instruction_policy,
        version,
    )?;
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
    release_spec_from_authoring_with_version(draft, available_agents, instruction_policy, "draft")
}

pub fn release_spec_from_authoring_with_version(
    draft: SupervisorDraftRequest,
    available_agents: &[ResolvedAgentDefinition],
    instruction_policy: &SupervisorInstructionPolicyDetail,
    version: &str,
) -> Result<SupervisorReleaseSpec, SupervisorCatalogError> {
    if !is_safe_version(version) {
        return Err(SupervisorCatalogError::Invalid(
            "Supervisor Release version is invalid",
        ));
    }
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
        version: version.to_string(),
        display_name: draft.display_name,
        description: draft.description,
        responsibilities: draft.responsibilities,
        instruction_policy: draft.instruction_policy,
        instruction_policy_sha256: instruction_policy.content_sha256.clone(),
        custom_instructions: draft.custom_instructions,
        agents,
        runtime_requirements: governed_runtime_requirements(),
        artifact_contracts,
        data_requirement_contracts: draft.data_requirement_contracts,
        coordination_capabilities: draft.coordination_capabilities,
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
    let mut capability_workspace_ids = BTreeSet::new();
    let mut workspace_capability_packages = BTreeMap::new();
    let mut dataset_releases = BTreeMap::new();
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
        if let Some(workspace_id) = definition.required_workspace_id() {
            capability_workspace_ids.insert(workspace_id);
        }
        if let Some(selection) = &definition.capability_template {
            if selection.source == AgentCapabilityTemplateSource::WorkspacePackageRelease {
                let requirement = WorkspaceCapabilityPackageRequirement {
                    release_id: selection.release_id.ok_or(SupervisorCatalogError::Invalid(
                        "Workspace capability package Release is missing",
                    ))?,
                    workspace_id: definition.capability_workspace_id.ok_or(
                        SupervisorCatalogError::Invalid(
                            "Workspace capability package ownership is missing",
                        ),
                    )?,
                    package_id: selection.definition_id.clone(),
                    version: selection.version.clone(),
                    content_sha256: definition.capability_template_sha256.clone(),
                };
                if workspace_capability_packages
                    .insert(requirement.release_id, requirement.clone())
                    .is_some_and(|existing| existing != requirement)
                {
                    return Err(SupervisorCatalogError::Invalid(
                        "Workspace capability package dependency is inconsistent",
                    ));
                }
            }
        }
        for release in &definition.dataset_releases {
            if dataset_releases
                .insert(release.release_id, release.clone())
                .is_some_and(|existing| existing != *release)
            {
                return Err(SupervisorCatalogError::Invalid(
                    "Dataset Release dependency is inconsistent",
                ));
            }
        }
        role_spawn_limits.insert(reference.runtime_role.clone(), reference.spawn_limit);
        contracts.insert(
            format!("{}@{}", reference.definition_id, reference.version),
            contract,
        );
        roles.push(role);
        resolved_agents.push(definition.clone());
    }
    if capability_workspace_ids.len() > 1 {
        return Err(SupervisorCatalogError::Invalid(
            "Workspace capability packages must belong to one Workspace",
        ));
    }
    let artifact_contracts = ArtifactContractSet {
        schema_version: "artifact-contract-set.v1".to_string(),
        contracts: spec.artifact_contracts.clone(),
    };
    validate_artifact_contracts(&artifact_contracts.contracts, &contracts)?;
    let required_mcp_servers =
        agent::merge_required_mcp_servers(&resolved_agents).map_err(map_agent_error)?;
    let coordination_mcp_servers =
        select_coordination_mcp_servers(&required_mcp_servers, &spec.coordination_capabilities)?;
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
        coordination_mcp_servers,
        coordination_capabilities: spec.coordination_capabilities,
        required_workspace_id: capability_workspace_ids.into_iter().next(),
        workspace_capability_packages: workspace_capability_packages.into_values().collect(),
        dataset_releases: dataset_releases.into_values().collect(),
        runtime_requirements: spec.runtime_requirements,
        artifact_contracts: artifact_contracts.contracts,
        data_requirement_contracts: spec.data_requirement_contracts,
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
         When spawning a specialized Runtime Role with an explicit `agent_type`, use \
         `fork_turns: \"none\"` or a bounded positive turn count; a full-history fork inherits \
         the parent Agent type and rejects the explicit specialized Role. `wait_agent` timeouts \
         must be at least 10000 milliseconds; use 30000 milliseconds for ordinary waits.\n\n\
         Multi-Agent V2 does not expose `close_agent`. A child Agent remains resident and occupies \
         the configured child limit after its turn reaches a terminal state. Treat the maximum \
         active-child value as the resident budget for the whole task. Do not call \
         `interrupt_agent` on a terminal child to release a slot; interruption does not provide \
         that lifecycle transition. Before spawning, reserve enough remaining slots for every \
         additional Role that the current evidence path may require. If the resident budget cannot \
         support the required evidence path, report the execution-budget gap explicitly.\n",
    );
    compiled.push_str(
        "\n## Artifact handoffs\n\
         A `durable-resource-reference` is one exact handoff tuple: Artifact schema, \
         `resource_name`, and the complete structured `data_ref` object returned by the producer \
         Tool. A schema name, business summary, copied metrics, guessed URI, or rewritten \
         `mcp_resource://` string is not a reference.\n\
         Every producer Tool result is submitted as one bounded evidence batch. The producer's \
         terminal message must end with an internal `HANDOFF_BATCH` containing the Artifact schema \
         (the exact `data_ref.resource_schema` value), exact `resource_name`, and complete \
         structured `data_ref`; copy those values verbatim into the next assignment. Never \
         summarize, reformat, reconstruct, or omit them.\n\
         Do not use `followup_task` to recover a handoff from a terminal producer. If the terminal \
         message has no complete `HANDOFF_BATCH`, end the current batch with a typed missing-evidence \
         result and report the business prerequisite. Do not ask the producer to rediscover files, \
         list Resources, or guess a URI. A consumer that reports `MISSING_ARTIFACT_HANDOFF` may only \
         receive the original tuple when that tuple is already available from the same batch.\n",
    );
    compiled.push_str(
        "\n## On-demand coordination\n\
         Start from the user's country, question, and current evidence gap. Ask Network Planning \
         to publish the smallest data requirement for this question. Invoke Data Preparation only \
         when a source must be discovered, inspected, mapped, normalized, or geographically \
         validated; it must not decide network objectives or perform network analysis.\n\
         Do not require one global artifact or a fixed phase sequence. Run independent work in \
         parallel, wait for the relevant readiness Resource before analysis, and request only the \
         missing business parameter from the Agent that owns it. Reuse an unchanged Resource \
         reference instead of asking an Agent to rescan or regenerate it.\n",
    );
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
    let mut coordination_capabilities = BTreeSet::new();
    if manifest.coordination_capabilities.len() > 16
        || manifest.coordination_capabilities.iter().any(|capability| {
            !is_safe_mcp_capability(capability)
                || !coordination_capabilities.insert(capability.as_str())
        })
    {
        return Err(SupervisorCatalogError::Invalid(
            "Supervisor coordination capabilities are invalid",
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

fn select_coordination_mcp_servers(
    available: &[RequiredMcpServer],
    capabilities: &[String],
) -> Result<Vec<RequiredMcpServer>, SupervisorCatalogError> {
    let platform_coordination = platform_coordination_mcp_server();
    let mut selected = BTreeMap::<String, BTreeSet<String>>::new();
    for capability in capabilities {
        let (server_name, tool_name) =
            capability
                .split_once('.')
                .ok_or(SupervisorCatalogError::Invalid(
                    "Supervisor coordination capability is invalid",
                ))?;
        let server = if server_name == platform_coordination.name {
            &platform_coordination
        } else {
            available
                .iter()
                .find(|server| server.name == server_name)
                .ok_or(SupervisorCatalogError::Invalid(
                    "Supervisor coordination MCP Server is unavailable",
                ))?
        };
        if !server.tools.iter().any(|tool| tool == tool_name) {
            return Err(SupervisorCatalogError::Invalid(
                "Supervisor coordination MCP Tool is unavailable",
            ));
        }
        selected
            .entry(server_name.to_string())
            .or_default()
            .insert(tool_name.to_string());
    }
    selected
        .into_iter()
        .map(|(server_name, tools)| {
            let server = if server_name == platform_coordination.name {
                &platform_coordination
            } else {
                available
                    .iter()
                    .find(|server| server.name == server_name)
                    .ok_or(SupervisorCatalogError::Invalid(
                        "Supervisor coordination MCP Server is unavailable",
                    ))?
            };
            Ok(RequiredMcpServer {
                name: server_name,
                tools: tools.into_iter().collect(),
                capability_roots: server.capability_roots.clone(),
            })
        })
        .collect()
}

fn platform_coordination_mcp_server() -> RequiredMcpServer {
    RequiredMcpServer {
        name: "platform_coordination".to_string(),
        tools: vec![
            "get_collaboration_status".to_string(),
            "list_agent_executions".to_string(),
            "get_work_state_summary".to_string(),
            "list_blocking_inputs".to_string(),
            "list_deliverables".to_string(),
        ],
        capability_roots: vec![CapabilityRootMcpInventory {
            capability_root_id: "local-platform-coordination-mcp".to_string(),
            mcp_server_names: vec!["platform_coordination".to_string()],
        }],
    }
}

fn is_safe_mcp_capability(value: &str) -> bool {
    let mut segments = value.split('.');
    matches!(
        (segments.next(), segments.next(), segments.next()),
        (Some(server), Some(tool), None)
            if is_safe_capability_segment(server) && is_safe_capability_segment(tool)
    )
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
        data_requirement_contracts: spec.data_requirement_contracts.clone(),
        coordination_capabilities: spec.coordination_capabilities.clone(),
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
    fn repository_catalog_publishes_the_current_supervisor_package() {
        let summaries = list_published().unwrap();
        assert_eq!(summaries.len(), 1);
        assert_eq!(
            resolve(&SupervisorPolicySelection {
                policy_id: "enterprise-supervisor-copilot".to_string(),
                version: "6.0.0".to_string(),
            })
            .unwrap()
            .version,
            "6.0.0"
        );
    }

    #[test]
    fn the_previous_release_is_not_a_new_runtime_contract() {
        assert_eq!(
            resolve(&SupervisorPolicySelection {
                policy_id: "enterprise-supervisor-copilot".to_string(),
                version: "5.0.0".to_string(),
            })
            .unwrap_err(),
            SupervisorCatalogError::NotFound
        );
    }

    #[test]
    fn rejects_unknown_package_without_fallback() {
        assert_eq!(
            resolve(&SupervisorPolicySelection {
                policy_id: "enterprise-supervisor-copilot".to_string(),
                version: "1.9.0".to_string(),
            })
            .unwrap_err(),
            SupervisorCatalogError::NotFound
        );
    }

    #[test]
    fn retired_project_supervisor_versions_are_not_runtime_contracts() {
        assert_eq!(
            resolve(&SupervisorPolicySelection {
                policy_id: "enterprise-supervisor-copilot".to_string(),
                version: "3.10.0".to_string(),
            })
            .unwrap_err(),
            SupervisorCatalogError::NotFound
        );
    }

    #[test]
    fn repository_and_web_supervisor_sources_compile_to_identical_execution_semantics() {
        let repository = parse_resource_sources(
            INDONESIA_NETWORK_PLANNING_DRAFT_MANIFEST,
            INDONESIA_NETWORK_PLANNING_DRAFT_CUSTOM_INSTRUCTIONS,
            INDONESIA_NETWORK_PLANNING_DRAFT_ARTIFACT_CONTRACTS,
        )
        .unwrap();
        let draft = SupervisorDraftRequest {
            policy_id: repository.policy_id.clone(),
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
            data_requirement_contracts: repository.data_requirement_contracts.clone(),
            coordination_capabilities: repository
                .coordination_mcp_servers
                .iter()
                .flat_map(|server| {
                    server
                        .tools
                        .iter()
                        .map(|tool| format!("{}.{}", server.name, tool))
                })
                .collect(),
            max_active_child_agents: repository.max_active_child_agents,
        };
        let available_agents = agent::list_resolved_builtins().unwrap();
        let web = resolve_authoring_spec_with_version(
            draft.clone(),
            &available_agents,
            &crate::instruction_policy::resolve(&SupervisorInstructionPolicySelection {
                policy_id: repository.instruction_policy.policy_id.clone(),
                version: repository.instruction_policy.version.clone(),
            })
            .unwrap(),
            &repository.version,
        )
        .unwrap();

        for required in [
            "## Artifact handoffs",
            "Every producer Tool result is submitted as one bounded evidence batch",
            "Do not use `followup_task` to recover a handoff from a terminal producer",
            "## On-demand coordination",
            "Ask Network Planning to publish the smallest data requirement",
            "Do not require one global artifact or a fixed phase sequence",
        ] {
            assert!(
                web.developer_instructions.contains(required),
                "missing compiled instruction: {required}"
            );
        }

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
                data_requirement_contracts: repository.data_requirement_contracts.clone(),
                coordination_capabilities: repository
                    .coordination_mcp_servers
                    .iter()
                    .flat_map(|server| {
                        server
                            .tools
                            .iter()
                            .map(|tool| format!("{}.{}", server.name, tool))
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
        let manifest = INDONESIA_NETWORK_PLANNING_DRAFT_MANIFEST.replacen(
            "\"runtimeRole\": \"agent_4214e235da7aa1d791f4b2951170397b\"",
            "\"runtimeRole\": \"drifted_data_agent\"",
            1,
        );
        assert_eq!(
            parse_resource_sources(
                &manifest,
                INDONESIA_NETWORK_PLANNING_DRAFT_CUSTOM_INSTRUCTIONS,
                INDONESIA_NETWORK_PLANNING_DRAFT_ARTIFACT_CONTRACTS,
            )
            .unwrap_err(),
            SupervisorCatalogError::Invalid(
                "repository package derived Runtime fields do not match the canonical compiler"
            )
        );
    }

    #[test]
    fn current_supervisor_package_requires_a_business_facing_evidence_summary() {
        let instructions = INDONESIA_NETWORK_PLANNING_DRAFT_CUSTOM_INSTRUCTIONS;
        for required in [
            "Network Agent",
            "Data Agent",
            "Work State",
            "platform_coordination",
            "当前覆盖",
            "request_user_input",
        ] {
            assert!(
                instructions.contains(required),
                "missing instruction: {required}"
            );
        }
        assert!(!instructions.contains("relayed authorization"));
    }
}
