use std::collections::{BTreeMap, BTreeSet};

use open_web_codex_adapter::{PlatformRuntimeRole, RequiredMcpServer};
use open_web_codex_platform_contracts::AgentDefinitionSummary;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use thiserror::Error;
use toml_edit::DocumentMut;

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
    runtime_role_name: &'static str,
}

const PUBLISHED_AGENT_RESOURCES: [PublishedAgentResource; 2] = [
    PublishedAgentResource {
        definition: DATA_AGENT,
        developer_instructions: DATA_AGENT_INSTRUCTIONS,
        runtime_role_name: "data_agent",
    },
    PublishedAgentResource {
        definition: NETWORK_PLANNING_AGENT,
        developer_instructions: NETWORK_PLANNING_AGENT_INSTRUCTIONS,
        runtime_role_name: "network_planning_agent",
    },
];

#[derive(Debug, Clone, Deserialize)]
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
    required_capabilities: Vec<String>,
    #[serde(default)]
    required_mcp_resource_servers: Vec<String>,
    #[serde(default)]
    required_capability_root_ids: Vec<String>,
    risks: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PublishedRuntimeProfileReference {
    content_sha256: String,
}

#[derive(Debug, Clone)]
pub(crate) struct AgentContract {
    pub input_artifact_types: BTreeSet<String>,
    pub output_artifact_types: BTreeSet<String>,
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

/// Exact immutable Runtime Role specifications for all code-published Agents.
pub fn platform_runtime_roles() -> Result<Vec<PlatformRuntimeRole>, AgentCatalogError> {
    PUBLISHED_AGENT_RESOURCES
        .iter()
        .map(runtime_role_from_resource)
        .collect()
}

pub fn resolve_runtime_role(
    definition_id: &str,
    version: &str,
    runtime_role: &str,
) -> Result<PlatformRuntimeRole, AgentCatalogError> {
    let resource = PUBLISHED_AGENT_RESOURCES
        .iter()
        .find(|resource| {
            parse_published_definition(resource).is_ok_and(|definition| {
                definition.definition_id == definition_id
                    && definition.version == version
                    && definition.runtime_role == runtime_role
            })
        })
        .ok_or(AgentCatalogError::NotFound)?;
    runtime_role_from_resource(resource)
}

/// Exact MCP inventory required by selected immutable Agent Definitions.
pub fn required_mcp_servers(
    roles: &[PlatformRuntimeRole],
) -> Result<Vec<RequiredMcpServer>, AgentCatalogError> {
    let mut required = BTreeMap::<String, (BTreeSet<String>, BTreeSet<String>)>::new();
    for role in roles {
        let resource = PUBLISHED_AGENT_RESOURCES
            .iter()
            .find(|resource| {
                parse_published_definition(resource).is_ok_and(|definition| {
                    definition.definition_id == role.definition_id
                        && definition.version == role.version
                        && definition.runtime_role == role.name
                })
            })
            .ok_or(AgentCatalogError::Invalid)?;
        let definition = parse_published_definition(resource)?;
        for server in mcp_requirements_from_definition(&definition)? {
            let entry = required.entry(server.name).or_default();
            entry.0.extend(server.tools);
            entry.1.extend(server.capability_root_ids);
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
        .any(|resource| resource.runtime_role_name == name)
}

pub(crate) fn contract(
    definition_id: &str,
    version: &str,
    runtime_role: &str,
) -> Result<AgentContract, AgentCatalogError> {
    let resource = PUBLISHED_AGENT_RESOURCES
        .iter()
        .find(|resource| {
            parse_published_definition(resource).is_ok_and(|definition| {
                definition.definition_id == definition_id
                    && definition.version == version
                    && definition.runtime_role == runtime_role
            })
        })
        .ok_or(AgentCatalogError::NotFound)?;
    let definition = parse_published_definition(resource)?;
    Ok(AgentContract {
        input_artifact_types: definition.input_artifact_types.into_iter().collect(),
        output_artifact_types: definition.output_artifact_types.into_iter().collect(),
    })
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
    if definition.runtime_role != resource.runtime_role_name {
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

fn runtime_role_template(
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
            definition_id: value.definition_id,
            version: value.version,
            display_name: value.display_name,
            description: value.description,
            runtime_role: value.runtime_role,
            responsibilities: value.responsibilities,
            input_artifact_types: value.input_artifact_types,
            output_artifact_types: value.output_artifact_types,
            required_capabilities: value.required_capabilities,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn publishes_exact_runtime_roles_and_seals_role_content() {
        let definitions = list_published().unwrap();
        assert_eq!(definitions.len(), 2);
        assert_eq!(definitions[0].version, "1.6.0");
        assert_eq!(definitions[1].version, "1.5.0");

        let roles = platform_runtime_roles().unwrap();
        assert_eq!(
            roles
                .iter()
                .map(|role| role.name.as_str())
                .collect::<Vec<_>>(),
            vec!["data_agent", "network_planning_agent"]
        );
        for role in &roles {
            assert_eq!(
                role.content_sha256,
                hex::encode(Sha256::digest(role.config_toml.as_bytes()))
            );
            assert!(role.config_toml.contains("[agents]\nenabled = false"));
            assert!(role.config_toml.contains("shell_tool = false"));
        }
    }

    #[test]
    fn definitions_bind_versions_to_reviewed_runtime_instructions() {
        for (definition, instructions) in [
            (
                parse_definition(DATA_AGENT).unwrap(),
                DATA_AGENT_INSTRUCTIONS,
            ),
            (
                parse_definition(NETWORK_PLANNING_AGENT).unwrap(),
                NETWORK_PLANNING_AGENT_INSTRUCTIONS,
            ),
        ] {
            assert_eq!(
                definition.runtime_profile.content_sha256,
                hex::encode(Sha256::digest(instructions.trim().as_bytes()))
            );
        }
    }

    #[test]
    fn platform_runtime_role_names_are_reserved() {
        assert!(is_platform_runtime_role("data_agent"));
        assert!(is_platform_runtime_role("network_planning_agent"));
        assert!(!is_platform_runtime_role("user_defined_agent"));
    }
}
