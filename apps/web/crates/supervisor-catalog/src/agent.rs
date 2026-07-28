use std::collections::{BTreeMap, BTreeSet};

use open_web_codex_adapter::{PlatformRuntimeRole, RequiredMcpServer};
use open_web_codex_platform_contracts::{
    AgentCapabilityTemplateSelection, AgentDefinitionDetail, AgentDefinitionSource,
    AgentDefinitionSummary,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;
use toml_edit::DocumentMut;
use uuid::Uuid;

pub use crate::agent_release::{user_runtime_role_name, validate_user_release, AgentReleaseSpec};

use crate::validation::{
    is_safe_artifact_type, is_safe_capability_segment, is_safe_definition_id,
    is_safe_runtime_role_name, is_safe_version,
};

const DATA_AGENT: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../../../capabilities/agents/enterprise-data-agent/1.6.0/definition.json"
));
const DATA_AGENT_INSTRUCTIONS: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../../../capabilities/agents/enterprise-data-agent/1.6.0/instructions.md"
));
const NETWORK_PLANNING_AGENT: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../../../capabilities/agents/enterprise-network-planning-agent/1.5.0/definition.json"
));
const NETWORK_PLANNING_AGENT_INSTRUCTIONS: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../../../capabilities/agents/enterprise-network-planning-agent/1.5.0/instructions.md"
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
    required_capabilities: Vec<String>,
    #[serde(default)]
    required_mcp_resource_servers: Vec<String>,
    #[serde(default)]
    required_capability_root_ids: Vec<String>,
    risks: Vec<String>,
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
    pub definition_id: String,
    pub version: String,
    pub display_name: String,
    pub description: String,
    pub responsibilities: Vec<String>,
    pub developer_instructions: String,
    pub input_artifact_types: Vec<String>,
    pub output_artifact_types: Vec<String>,
    pub required_capabilities: Vec<String>,
    pub capability_template: Option<AgentCapabilityTemplateSelection>,
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
    let mut required = BTreeMap::<String, (BTreeSet<String>, BTreeSet<String>)>::new();
    for definition in definitions {
        if definition.required_mcp_servers.is_empty() {
            return Err(AgentCatalogError::Invalid);
        }
        for server in &definition.required_mcp_servers {
            let entry = required.entry(server.name.clone()).or_default();
            entry.0.extend(server.tools.iter().cloned());
            entry.1.extend(server.capability_root_ids.iter().cloned());
        }
    }
    if required.is_empty() {
        return Err(AgentCatalogError::Invalid);
    }
    Ok(required
        .into_iter()
        .map(|(name, (tools, capability_root_ids))| RequiredMcpServer {
            name,
            tools: tools.into_iter().collect(),
            capability_root_ids: capability_root_ids.into_iter().collect(),
        })
        .collect())
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
                    definition_id: self.definition_id.clone(),
                    version: self.version.clone(),
                }
            }),
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
        definition_id: definition.definition_id.clone(),
        version: definition.version.clone(),
        display_name: definition.display_name.clone(),
        description: definition.description.clone(),
        responsibilities: definition.responsibilities.clone(),
        developer_instructions: resource.developer_instructions.trim().to_string(),
        input_artifact_types: definition.input_artifact_types.clone(),
        output_artifact_types: definition.output_artifact_types.clone(),
        required_capabilities: definition.required_capabilities.clone(),
        capability_template: Some(AgentCapabilityTemplateSelection {
            definition_id: definition.capability_template.definition_id.clone(),
            version: definition.capability_template.version.clone(),
        }),
        capability_template_sha256: capability_template_sha256.clone(),
        content_sha256: capability_template_sha256,
        runtime_role,
        required_mcp_servers,
    };
    crate::agent_release::compile_agent_release(template.authoring_spec(), &template)
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
    let definition = serde_json::from_str::<PublishedAgentDefinition>(source)
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
    for values in [
        &definition.responsibilities,
        &definition.output_artifact_types,
        &definition.required_capabilities,
        &definition.risks,
    ] {
        if values.is_empty()
            || values
                .iter()
                .any(|value| value.trim().is_empty() || value.len() > 512)
        {
            return Err(AgentCatalogError::Invalid);
        }
    }
    if definition
        .input_artifact_types
        .iter()
        .chain(definition.output_artifact_types.iter())
        .any(|value| !is_safe_artifact_type(value))
    {
        return Err(AgentCatalogError::Invalid);
    }
    let mut resource_servers = BTreeSet::new();
    if definition.required_mcp_resource_servers.len() > 16
        || definition
            .required_mcp_resource_servers
            .iter()
            .any(|server| {
                !is_safe_capability_segment(server) || !resource_servers.insert(server.as_str())
            })
    {
        return Err(AgentCatalogError::Invalid);
    }
    let mut capability_root_ids = BTreeSet::new();
    if definition.required_capability_root_ids.is_empty()
        || definition.required_capability_root_ids.len() > 16
        || definition
            .required_capability_root_ids
            .iter()
            .any(|root_id| {
                !is_safe_capability_segment(root_id)
                    || !capability_root_ids.insert(root_id.as_str())
            })
    {
        return Err(AgentCatalogError::Invalid);
    }
    Ok(definition)
}

fn mcp_requirements_from_definition(
    definition: &PublishedAgentDefinition,
) -> Result<Vec<RequiredMcpServer>, AgentCatalogError> {
    let mut required = BTreeMap::<String, (BTreeSet<String>, BTreeSet<String>)>::new();
    for capability in &definition.required_capabilities {
        if capability.starts_with("mcpServer/") {
            continue;
        }
        let (server, tool) = capability
            .split_once('.')
            .filter(|(server, tool)| {
                is_safe_capability_segment(server) && is_safe_capability_segment(tool)
            })
            .ok_or(AgentCatalogError::Invalid)?;
        required
            .entry(server.to_string())
            .or_default()
            .0
            .insert(tool.to_string());
    }
    for server in &definition.required_mcp_resource_servers {
        if !is_safe_capability_segment(server) {
            return Err(AgentCatalogError::Invalid);
        }
        required.entry(server.clone()).or_default();
    }
    if required.is_empty() {
        return Err(AgentCatalogError::Invalid);
    }
    for (_, roots) in required.values_mut() {
        roots.extend(definition.required_capability_root_ids.iter().cloned());
    }
    Ok(required
        .into_iter()
        .map(|(name, (tools, capability_root_ids))| RequiredMcpServer {
            name,
            tools: tools.into_iter().collect(),
            capability_root_ids: capability_root_ids.into_iter().collect(),
        })
        .collect())
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
                || server.capability_root_ids.is_empty()
                || server
                    .capability_root_ids
                    .iter()
                    .any(|root_id| !is_safe_capability_segment(root_id))
                || server
                    .tools
                    .iter()
                    .any(|tool| !is_safe_capability_segment(tool))
        })
    {
        return Err(AgentCatalogError::Invalid);
    }
    let mut source = format!(
        "developer_instructions = '''\n{developer_instructions}\n'''\n\
         \n[agents]\nenabled = false\n\
         \n[features]\napps = false\nmulti_agent_v2 = false\nplugins = false\nshell_tool = false\n"
    );
    let mut plugins = BTreeSet::new();
    for server in required_mcp_servers {
        let enabled_tools = server
            .tools
            .iter()
            .map(|tool| format!("\"{tool}\""))
            .collect::<Vec<_>>()
            .join(", ");
        for capability_root_id in &server.capability_root_ids {
            if plugins.insert(capability_root_id) {
                source.push_str(&format!(
                    "\n[plugins.{capability_root_id}]\nenabled = true\n"
                ));
            }
            source.push_str(&format!(
                "\n[plugins.{capability_root_id}.mcp_servers.{}]\nenabled = true\nenabled_tools = [{enabled_tools}]\n",
                server.name
            ));
        }
    }
    let document = source
        .parse::<DocumentMut>()
        .map_err(|_| AgentCatalogError::Invalid)?;
    if document.as_table().len() != 4 {
        return Err(AgentCatalogError::Invalid);
    }
    Ok(source)
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
                definition_id: value.capability_template.definition_id,
                version: value.capability_template.version,
            }),
        }
    }
}

#[cfg(test)]
#[path = "agent_tests.rs"]
mod tests;
