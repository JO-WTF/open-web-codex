use std::collections::{BTreeMap, BTreeSet};

use open_web_codex_adapter::{CapabilityRootMcpInventory, PlatformRuntimeRole, RequiredMcpServer};
use open_web_codex_platform_contracts::{
    AgentCapabilityTemplateSelection, AgentCapabilityTemplateSource, AgentDatasetReleaseBinding,
    AgentDefinitionDetail, AgentDefinitionSource, AgentDefinitionSummary,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;
use toml_edit::DocumentMut;
use uuid::Uuid;

pub use crate::agent_release::{
    compile_agent_release_against_template, user_runtime_role_name, validate_user_release,
    AgentReleaseSpec,
};

use crate::validation::{
    is_safe_artifact_type, is_safe_capability_segment, is_safe_definition_id,
    is_safe_runtime_role_name, is_safe_version,
};

const DATA_AGENT: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../../../capabilities/agents/enterprise-data-agent/6.0.0/definition.json"
));
const DATA_AGENT_INSTRUCTIONS: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../../../capabilities/agents/enterprise-data-agent/6.0.0/instructions.md"
));
const NETWORK_PLANNING_AGENT: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../../../capabilities/agents/enterprise-network-planning-agent/6.0.0/definition.json"
));
const NETWORK_PLANNING_AGENT_INSTRUCTIONS: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../../../capabilities/agents/enterprise-network-planning-agent/6.0.0/instructions.md"
));

const MAX_RUNTIME_ROLE_INSTRUCTIONS_BYTES: usize = 16 * 1024;

struct PublishedAgentResource {
    definition: &'static str,
    developer_instructions: &'static str,
}

const PUBLISHED_AGENT_RESOURCES: [PublishedAgentResource; 2] = [
    PublishedAgentResource {
        definition: DATA_AGENT,
        developer_instructions: DATA_AGENT_INSTRUCTIONS,
    },
    PublishedAgentResource {
        definition: NETWORK_PLANNING_AGENT,
        developer_instructions: NETWORK_PLANNING_AGENT_INSTRUCTIONS,
    },
];

const LEGACY_AGENT_RESOURCES: [PublishedAgentResource; 0] = [];

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct PublishedAgentDefinition {
    definition_id: String,
    version: String,
    display_name: String,
    description: String,
    runtime_role: String,
    runtime_profile: PublishedRuntimeProfileReference,
    responsibilities: Vec<String>,
    input_artifact_types: Vec<String>,
    output_artifact_types: Vec<String>,
    capability_template: PublishedCapabilityTemplateReference,
    #[serde(skip)]
    required_capabilities: Vec<String>,
    required_mcp_servers: Vec<PublishedRequiredMcpServer>,
    risks: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct PublishedRequiredMcpServer {
    name: String,
    tools: Vec<String>,
    resource_read: bool,
    capability_root_ids: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct PublishedRuntimeProfileReference {
    content_sha256: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct PublishedCapabilityTemplateReference {
    definition_id: String,
    version: String,
}

#[derive(Debug, Clone)]
pub struct AgentContract {
    pub input_artifact_types: BTreeSet<String>,
    pub output_artifact_types: BTreeSet<String>,
}

#[derive(Debug, Clone)]
pub struct ResolvedAgentDefinition {
    pub release_id: Option<Uuid>,
    pub capability_workspace_id: Option<Uuid>,
    pub definition_id: String,
    pub version: String,
    pub display_name: String,
    pub description: String,
    pub responsibilities: Vec<String>,
    pub developer_instructions: String,
    pub runtime_developer_instructions: String,
    pub input_artifact_types: Vec<String>,
    pub output_artifact_types: Vec<String>,
    pub required_capabilities: Vec<String>,
    pub capability_template: Option<AgentCapabilityTemplateSelection>,
    pub dataset_releases: Vec<AgentDatasetReleaseBinding>,
    pub capability_template_sha256: String,
    pub runtime_role: PlatformRuntimeRole,
    pub required_mcp_servers: Vec<RequiredMcpServer>,
    pub content_sha256: String,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum AgentCatalogError {
    #[error("published Agent Definition is invalid")]
    Invalid,
    #[error("published Agent Definition was not found")]
    NotFound,
}

pub fn list_published() -> Result<Vec<AgentDefinitionSummary>, AgentCatalogError> {
    PUBLISHED_AGENT_RESOURCES
        .iter()
        .map(parse_published_definition)
        .map(|definition| definition.map(Into::into))
        .collect()
}

pub fn list_resolved_builtins() -> Result<Vec<ResolvedAgentDefinition>, AgentCatalogError> {
    PUBLISHED_AGENT_RESOURCES
        .iter()
        .map(resolved_from_resource)
        .collect()
}

pub fn resolve_builtin(
    definition_id: &str,
    version: &str,
) -> Result<ResolvedAgentDefinition, AgentCatalogError> {
    PUBLISHED_AGENT_RESOURCES
        .iter()
        .chain(LEGACY_AGENT_RESOURCES.iter())
        .find(|resource| {
            parse_published_definition(resource).is_ok_and(|definition| {
                definition.definition_id == definition_id && definition.version == version
            })
        })
        .ok_or(AgentCatalogError::NotFound)
        .and_then(resolved_from_resource)
}

/// Exact immutable Runtime Role specifications for all code-published Agents.
pub fn platform_runtime_roles() -> Result<Vec<PlatformRuntimeRole>, AgentCatalogError> {
    PUBLISHED_AGENT_RESOURCES
        .iter()
        .map(runtime_role_from_resource)
        .collect()
}

pub fn merge_required_mcp_servers(
    definitions: &[ResolvedAgentDefinition],
) -> Result<Vec<RequiredMcpServer>, AgentCatalogError> {
    let mut required = BTreeMap::<String, (BTreeSet<String>, BTreeMap<String, Vec<String>>)>::new();
    for definition in definitions {
        if definition.required_mcp_servers.is_empty() {
            return Err(AgentCatalogError::Invalid);
        }
        for server in &definition.required_mcp_servers {
            let entry = required.entry(server.name.clone()).or_default();
            entry.0.extend(server.tools.iter().cloned());
            for root in &server.capability_roots {
                if entry
                    .1
                    .insert(
                        root.capability_root_id.clone(),
                        root.mcp_server_names.clone(),
                    )
                    .is_some_and(|existing| existing != root.mcp_server_names)
                {
                    return Err(AgentCatalogError::Invalid);
                }
            }
        }
    }
    if required.is_empty() {
        return Err(AgentCatalogError::Invalid);
    }
    Ok(required
        .into_iter()
        .map(|(name, (tools, capability_roots))| RequiredMcpServer {
            name,
            tools: tools.into_iter().collect(),
            capability_roots: capability_roots
                .into_iter()
                .map(
                    |(capability_root_id, mcp_server_names)| CapabilityRootMcpInventory {
                        capability_root_id,
                        mcp_server_names,
                    },
                )
                .collect(),
        })
        .collect())
}

pub fn workspace_capability_template(
    release_id: Uuid,
    workspace_id: Uuid,
    package_id: String,
    version: String,
    display_name: String,
    description: String,
    capability_root_id: String,
    server_name: String,
    tool_names: Vec<String>,
    input_artifact_types: Vec<String>,
    output_artifact_types: Vec<String>,
    content_sha256: String,
) -> Result<ResolvedAgentDefinition, AgentCatalogError> {
    if !is_safe_definition_id(&package_id)
        || !is_safe_version(&version)
        || display_name.trim().is_empty()
        || description.trim().is_empty()
        || !is_safe_capability_segment(&capability_root_id)
        || !is_safe_capability_segment(&server_name)
        || tool_names.is_empty()
        || tool_names.len() > 16
        || tool_names
            .iter()
            .any(|tool| !is_safe_capability_segment(tool))
        || input_artifact_types.len() > 32
        || output_artifact_types.is_empty()
        || output_artifact_types.len() > 32
        || input_artifact_types
            .iter()
            .chain(output_artifact_types.iter())
            .any(|artifact| !is_safe_artifact_type(artifact))
        || content_sha256.len() != 64
        || !content_sha256
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (byte.is_ascii_lowercase() && byte <= b'f'))
    {
        return Err(AgentCatalogError::Invalid);
    }
    let mut unique_tools = BTreeSet::new();
    if tool_names
        .iter()
        .any(|tool| !unique_tools.insert(tool.as_str()))
    {
        return Err(AgentCatalogError::Invalid);
    }
    let selection = AgentCapabilityTemplateSelection {
        source: AgentCapabilityTemplateSource::WorkspacePackageRelease,
        definition_id: package_id.clone(),
        version: version.clone(),
        release_id: Some(release_id),
    };
    let required_capabilities = tool_names
        .iter()
        .map(|tool| format!("{server_name}.{tool}"))
        .collect::<Vec<_>>();
    let required_mcp_servers = vec![RequiredMcpServer {
        name: server_name.clone(),
        tools: tool_names,
        capability_roots: vec![CapabilityRootMcpInventory {
            capability_root_id,
            mcp_server_names: vec![server_name],
        }],
    }];
    let runtime_role_name = user_runtime_role_name(&package_id, &version)?;
    let runtime_role = PlatformRuntimeRole {
        definition_id: package_id.clone(),
        version: version.clone(),
        name: runtime_role_name,
        description: description.clone(),
        config_file: String::new(),
        config_toml: String::new(),
        content_sha256: content_sha256.clone(),
    };
    Ok(ResolvedAgentDefinition {
        release_id: None,
        capability_workspace_id: Some(workspace_id),
        definition_id: package_id,
        version,
        display_name,
        description,
        responsibilities: vec!["Use only the selected Workspace capability package.".to_string()],
        developer_instructions:
            "This is an authoring boundary; the published Agent supplies its own instructions."
                .to_string(),
        runtime_developer_instructions:
            "This is an authoring boundary; the published Agent supplies its own instructions."
                .to_string(),
        input_artifact_types,
        output_artifact_types,
        required_capabilities,
        capability_template: Some(selection),
        dataset_releases: Vec::new(),
        capability_template_sha256: content_sha256.clone(),
        runtime_role,
        required_mcp_servers,
        content_sha256,
    })
}

/// Browser Profile Agent CRUD must not manage platform-owned role names.
pub fn is_platform_runtime_role(name: &str) -> bool {
    PUBLISHED_AGENT_RESOURCES
        .iter()
        .filter_map(|resource| parse_published_definition(resource).ok())
        .any(|definition| definition.runtime_role == name)
}

impl ResolvedAgentDefinition {
    pub fn with_release_id(mut self, release_id: Uuid) -> Self {
        self.release_id = Some(release_id);
        self
    }

    pub fn contract(&self) -> AgentContract {
        AgentContract {
            input_artifact_types: self.input_artifact_types.iter().cloned().collect(),
            output_artifact_types: self.output_artifact_types.iter().cloned().collect(),
        }
    }

    pub fn required_workspace_id(&self) -> Option<Uuid> {
        self.capability_workspace_id.or_else(|| {
            self.dataset_releases
                .first()
                .map(|release| release.workspace_id)
        })
    }

    /// Return the one authoring contract accepted by Web publication.
    /// Repository definitions use their own exact version as the reviewed
    /// capability template, allowing both source adapters to be compiled and
    /// compared at the execution boundary.
    pub fn authoring_spec(&self) -> AgentReleaseSpec {
        AgentReleaseSpec {
            definition_id: self.definition_id.clone(),
            version: self.version.clone(),
            display_name: self.display_name.clone(),
            description: self.description.clone(),
            responsibilities: self.responsibilities.clone(),
            developer_instructions: self.developer_instructions.clone(),
            input_artifact_types: self.input_artifact_types.clone(),
            output_artifact_types: self.output_artifact_types.clone(),
            capability_template: self.capability_template.clone().unwrap_or_else(|| {
                AgentCapabilityTemplateSelection {
                    source: AgentCapabilityTemplateSource::RepositoryAgent,
                    definition_id: self.definition_id.clone(),
                    version: self.version.clone(),
                    release_id: None,
                }
            }),
            dataset_releases: self.dataset_releases.clone(),
        }
    }

    pub fn summary(&self, source: AgentDefinitionSource) -> AgentDefinitionSummary {
        AgentDefinitionSummary {
            source,
            release_id: self.release_id,
            definition_id: self.definition_id.clone(),
            version: self.version.clone(),
            display_name: self.display_name.clone(),
            description: self.description.clone(),
            responsibilities: self.responsibilities.clone(),
            input_artifact_types: self.input_artifact_types.clone(),
            output_artifact_types: self.output_artifact_types.clone(),
            required_capabilities: self.required_capabilities.clone(),
            capability_template: self.capability_template.clone(),
            dataset_releases: self.dataset_releases.clone(),
            required_workspace_id: self.required_workspace_id(),
        }
    }

    pub fn detail(&self, source: AgentDefinitionSource) -> AgentDefinitionDetail {
        AgentDefinitionDetail {
            source,
            release_id: self.release_id,
            definition_id: self.definition_id.clone(),
            version: self.version.clone(),
            display_name: self.display_name.clone(),
            description: self.description.clone(),
            responsibilities: self.responsibilities.clone(),
            developer_instructions: self.developer_instructions.clone(),
            input_artifact_types: self.input_artifact_types.clone(),
            output_artifact_types: self.output_artifact_types.clone(),
            required_capabilities: self.required_capabilities.clone(),
            capability_template: self.capability_template.clone(),
            dataset_releases: self.dataset_releases.clone(),
            required_workspace_id: self.required_workspace_id(),
            content_sha256: self.content_sha256.clone(),
            execution_semantics_sha256: self.execution_semantics_sha256(),
        }
    }
}

fn resolved_from_resource(
    resource: &PublishedAgentResource,
) -> Result<ResolvedAgentDefinition, AgentCatalogError> {
    let definition = parse_published_definition(resource)?;
    let runtime_role = runtime_role_from_resource(resource)?;
    let required_mcp_servers = mcp_requirements_from_definition(&definition)?;
    let capability_template_sha256 =
        published_definition_content_sha256(&definition, &runtime_role);
    let template = ResolvedAgentDefinition {
        release_id: None,
        capability_workspace_id: None,
        definition_id: definition.definition_id.clone(),
        version: definition.version.clone(),
        display_name: definition.display_name.clone(),
        description: definition.description.clone(),
        responsibilities: definition.responsibilities.clone(),
        developer_instructions: resource.developer_instructions.trim().to_string(),
        runtime_developer_instructions: resource.developer_instructions.trim().to_string(),
        input_artifact_types: definition.input_artifact_types.clone(),
        output_artifact_types: definition.output_artifact_types.clone(),
        required_capabilities: definition.required_capabilities.clone(),
        capability_template: Some(AgentCapabilityTemplateSelection {
            source: AgentCapabilityTemplateSource::RepositoryAgent,
            definition_id: definition.capability_template.definition_id.clone(),
            version: definition.capability_template.version.clone(),
            release_id: None,
        }),
        dataset_releases: Vec::new(),
        capability_template_sha256: capability_template_sha256.clone(),
        content_sha256: capability_template_sha256,
        runtime_role,
        required_mcp_servers,
    };
    crate::agent_release::compile_agent_release_against_template(
        template.authoring_spec(),
        &template,
    )
}

fn published_definition_content_sha256(
    definition: &PublishedAgentDefinition,
    runtime_role: &PlatformRuntimeRole,
) -> String {
    let mut digest = Sha256::new();
    let definition_json =
        serde_json::to_vec(definition).expect("validated Agent Definition serializes");
    for field in [
        b"published-agent-definition.v1".as_slice(),
        definition_json.as_slice(),
        runtime_role.content_sha256.as_bytes(),
    ] {
        digest.update((field.len() as u64).to_be_bytes());
        digest.update(field);
    }
    hex::encode(digest.finalize())
}

fn runtime_role_from_resource(
    resource: &PublishedAgentResource,
) -> Result<PlatformRuntimeRole, AgentCatalogError> {
    let definition = parse_published_definition(resource)?;
    let required_mcp_servers = mcp_requirements_from_definition(&definition)?;
    let config_toml = runtime_role_template(
        resource.developer_instructions,
        &definition.runtime_profile.content_sha256,
        &required_mcp_servers,
    )?;
    Ok(PlatformRuntimeRole {
        definition_id: definition.definition_id.clone(),
        version: definition.version.clone(),
        name: definition.runtime_role,
        description: definition.description,
        config_file: format!(
            "platform-agents/{}/{}.toml",
            definition.definition_id, definition.version
        ),
        content_sha256: hex::encode(Sha256::digest(config_toml.as_bytes())),
        config_toml,
    })
}

fn parse_published_definition(
    resource: &PublishedAgentResource,
) -> Result<PublishedAgentDefinition, AgentCatalogError> {
    let definition = parse_definition(resource.definition)?;
    if definition.runtime_role
        != user_runtime_role_name(&definition.definition_id, &definition.version)?
    {
        return Err(AgentCatalogError::Invalid);
    }
    Ok(definition)
}

fn parse_definition(source: &str) -> Result<PublishedAgentDefinition, AgentCatalogError> {
    let mut definition = serde_json::from_str::<PublishedAgentDefinition>(source)
        .map_err(|_| AgentCatalogError::Invalid)?;
    for value in [
        &definition.definition_id,
        &definition.version,
        &definition.display_name,
        &definition.description,
        &definition.runtime_role,
    ] {
        if value.trim().is_empty() || value.len() > 256 {
            return Err(AgentCatalogError::Invalid);
        }
    }
    if !is_safe_definition_id(&definition.definition_id)
        || !is_safe_version(&definition.version)
        || !is_safe_runtime_role_name(&definition.runtime_role)
        || definition.capability_template.definition_id != definition.definition_id
        || definition.capability_template.version != definition.version
        || definition.runtime_profile.content_sha256.len() != 64
        || !definition
            .runtime_profile
            .content_sha256
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (byte.is_ascii_lowercase() && byte <= b'f'))
        || definition.description != definition.description.trim()
        || definition.description.len() > 512
    {
        return Err(AgentCatalogError::Invalid);
    }
    for values in [&definition.responsibilities, &definition.risks] {
        if values.is_empty()
            || values
                .iter()
                .any(|value| value.trim().is_empty() || value.len() > 512)
        {
            return Err(AgentCatalogError::Invalid);
        }
    }
    if definition
        .output_artifact_types
        .iter()
        .any(|value| value.trim().is_empty() || value.len() > 512)
    {
        return Err(AgentCatalogError::Invalid);
    }
    if definition
        .input_artifact_types
        .iter()
        .chain(definition.output_artifact_types.iter())
        .any(|value| !is_safe_artifact_type(value))
    {
        return Err(AgentCatalogError::Invalid);
    }
    definition.required_capabilities =
        compile_published_mcp_requirements(&definition.required_mcp_servers)?.0;
    Ok(definition)
}

fn mcp_requirements_from_definition(
    definition: &PublishedAgentDefinition,
) -> Result<Vec<RequiredMcpServer>, AgentCatalogError> {
    Ok(compile_published_mcp_requirements(&definition.required_mcp_servers)?.1)
}

fn compile_published_mcp_requirements(
    declarations: &[PublishedRequiredMcpServer],
) -> Result<(Vec<String>, Vec<RequiredMcpServer>), AgentCatalogError> {
    if declarations.is_empty() || declarations.len() > 16 {
        return Err(AgentCatalogError::Invalid);
    }
    let mut server_names = BTreeSet::new();
    let mut capabilities = BTreeSet::new();
    let mut required = Vec::with_capacity(declarations.len());
    let mut reads_resources = false;
    for declaration in declarations {
        let mut tools = BTreeSet::new();
        let mut capability_root_ids = BTreeSet::new();
        if !is_safe_capability_segment(&declaration.name)
            || !server_names.insert(declaration.name.as_str())
            || declaration.tools.len() > 64
            || declaration
                .tools
                .iter()
                .any(|tool| !is_safe_capability_segment(tool) || !tools.insert(tool.as_str()))
            || declaration.capability_root_ids.is_empty()
            || declaration.capability_root_ids.len() > 16
            || declaration.capability_root_ids.iter().any(|root_id| {
                !is_safe_capability_segment(root_id)
                    || !capability_root_ids.insert(root_id.as_str())
            })
            || (declaration.tools.is_empty() && !declaration.resource_read)
        {
            return Err(AgentCatalogError::Invalid);
        }
        reads_resources |= declaration.resource_read;
        capabilities.extend(
            declaration
                .tools
                .iter()
                .map(|tool| format!("{}.{}", declaration.name, tool)),
        );
        let capability_roots = resolve_capability_root_inventories(
            &declaration.name,
            &declaration.capability_root_ids,
        )?;
        required.push(RequiredMcpServer {
            name: declaration.name.clone(),
            tools: tools.into_iter().map(str::to_string).collect(),
            capability_roots,
        });
    }
    if reads_resources {
        capabilities.insert("mcpServer/resource/read".to_string());
    }
    required.sort_by(|left, right| left.name.cmp(&right.name));
    Ok((capabilities.into_iter().collect(), required))
}

fn resolve_capability_root_inventories(
    required_server_name: &str,
    capability_root_ids: &[String],
) -> Result<Vec<CapabilityRootMcpInventory>, AgentCatalogError> {
    let packages =
        crate::capability_package::list_published().map_err(|_| AgentCatalogError::Invalid)?;
    capability_root_ids
        .iter()
        .map(|capability_root_id| {
            let mut matches = packages
                .iter()
                .filter(|package| package.capability_root_id == *capability_root_id);
            let package = matches.next().ok_or(AgentCatalogError::Invalid)?;
            if matches.next().is_some()
                || !package
                    .mcp_server_names
                    .iter()
                    .any(|server_name| server_name == required_server_name)
            {
                return Err(AgentCatalogError::Invalid);
            }
            Ok(CapabilityRootMcpInventory {
                capability_root_id: capability_root_id.clone(),
                mcp_server_names: package.mcp_server_names.clone(),
            })
        })
        .collect()
}

pub(crate) fn runtime_role_template(
    developer_instructions: &str,
    expected_instructions_sha256: &str,
    required_mcp_servers: &[RequiredMcpServer],
) -> Result<String, AgentCatalogError> {
    let developer_instructions = developer_instructions.trim();
    if developer_instructions.is_empty()
        || developer_instructions.len() > MAX_RUNTIME_ROLE_INSTRUCTIONS_BYTES
        || hex::encode(Sha256::digest(developer_instructions.as_bytes()))
            != expected_instructions_sha256
        || developer_instructions.contains("'''")
        || required_mcp_servers.is_empty()
        || required_mcp_servers.iter().any(|server| {
            !is_safe_capability_segment(&server.name)
                || server.capability_roots.is_empty()
                || server
                    .capability_roots
                    .iter()
                    .any(|root| !is_safe_capability_segment(&root.capability_root_id))
                || server
                    .tools
                    .iter()
                    .any(|tool| !is_safe_capability_segment(tool))
        })
    {
        return Err(AgentCatalogError::Invalid);
    }
    let capability_roots = compile_runtime_capability_root_inventories(required_mcp_servers)?;
    let mut allowed_tools = BTreeMap::<(String, String), Vec<String>>::new();
    for server in required_mcp_servers {
        for root in &server.capability_roots {
            if allowed_tools
                .insert(
                    (root.capability_root_id.clone(), server.name.clone()),
                    server.tools.clone(),
                )
                .is_some()
            {
                return Err(AgentCatalogError::Invalid);
            }
        }
    }
    let mut source = format!(
        "developer_instructions = '''\n{developer_instructions}\n'''\n\
         \n[agents]\nenabled = false\n\
         \n[skills]\ninclude_instructions = false\n\
         \n[features]\napps = false\nmulti_agent_v2 = false\nplugins = false\nshell_tool = false\n"
    );
    for (capability_root_id, server_names) in capability_roots {
        source.push_str(&format!(
            "\n[plugins.{capability_root_id}]\nenabled = true\n"
        ));
        for server_name in server_names {
            source.push_str(&format!(
                "\n[plugins.{capability_root_id}.mcp_servers.{server_name}]\n"
            ));
            if let Some(tools) =
                allowed_tools.get(&(capability_root_id.clone(), server_name.clone()))
            {
                let enabled_tools = tools
                    .iter()
                    .map(|tool| format!("\"{tool}\""))
                    .collect::<Vec<_>>()
                    .join(", ");
                source.push_str(&format!(
                    "enabled = true\nenabled_tools = [{enabled_tools}]\n"
                ));
            } else {
                source.push_str("enabled = false\n");
            }
        }
    }
    let document = source
        .parse::<DocumentMut>()
        .map_err(|_| AgentCatalogError::Invalid)?;
    if document.as_table().len() != 5 {
        return Err(AgentCatalogError::Invalid);
    }
    Ok(source)
}

fn compile_runtime_capability_root_inventories(
    required_mcp_servers: &[RequiredMcpServer],
) -> Result<BTreeMap<String, Vec<String>>, AgentCatalogError> {
    let mut inventories = BTreeMap::<String, Vec<String>>::new();
    for required in required_mcp_servers {
        let mut root_ids = BTreeSet::new();
        for root in &required.capability_roots {
            let server_names = root
                .mcp_server_names
                .iter()
                .cloned()
                .collect::<BTreeSet<_>>();
            if !root_ids.insert(root.capability_root_id.as_str())
                || server_names.is_empty()
                || server_names.len() != root.mcp_server_names.len()
                || !server_names.contains(&required.name)
                || server_names
                    .iter()
                    .any(|server_name| !is_safe_capability_segment(server_name))
            {
                return Err(AgentCatalogError::Invalid);
            }
            let server_names = server_names.into_iter().collect::<Vec<_>>();
            if inventories
                .insert(root.capability_root_id.clone(), server_names.clone())
                .is_some_and(|existing| existing != server_names)
            {
                return Err(AgentCatalogError::Invalid);
            }
        }
    }
    Ok(inventories)
}

impl From<PublishedAgentDefinition> for AgentDefinitionSummary {
    fn from(value: PublishedAgentDefinition) -> Self {
        Self {
            source: AgentDefinitionSource::Repository,
            release_id: None,
            definition_id: value.definition_id,
            version: value.version,
            display_name: value.display_name,
            description: value.description,
            responsibilities: value.responsibilities,
            input_artifact_types: value.input_artifact_types,
            output_artifact_types: value.output_artifact_types,
            required_capabilities: value.required_capabilities,
            capability_template: Some(AgentCapabilityTemplateSelection {
                source: AgentCapabilityTemplateSource::RepositoryAgent,
                definition_id: value.capability_template.definition_id,
                version: value.capability_template.version,
                release_id: None,
            }),
            dataset_releases: Vec::new(),
            required_workspace_id: None,
        }
    }
}

#[cfg(test)]
#[path = "agent_tests.rs"]
mod tests;
