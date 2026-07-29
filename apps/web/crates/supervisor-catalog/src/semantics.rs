use std::collections::BTreeMap;

use open_web_codex_adapter::{PlatformRuntimeRole, RequiredMcpServer};
use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::agent::ResolvedAgentDefinition;
use crate::supervisor::{
    ArtifactContract, ResolvedSupervisorPackage, RuntimeCapabilityRequirement,
    SupervisorAgentReference,
};

/// Source-independent, executable meaning of one resolved Agent Definition.
///
/// Publication identity and governance metadata are deliberately excluded.
/// Runtime Role content, MCP requirements, instructions and Artifact contracts
/// are included because a difference in any of them changes execution.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AgentExecutionSemantics {
    pub definition_id: String,
    pub version: String,
    pub developer_instructions: String,
    pub input_artifact_types: Vec<String>,
    pub output_artifact_types: Vec<String>,
    pub required_capabilities: Vec<String>,
    pub runtime_role: RuntimeRoleSemantics,
    pub required_mcp_servers: Vec<McpRequirementSemantics>,
}

/// Source-independent, executable meaning of one resolved Supervisor.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SupervisorExecutionSemantics {
    pub policy_id: String,
    pub version: String,
    pub developer_instructions: String,
    pub agents: Vec<SupervisorAgentReference>,
    pub runtime_roles: Vec<RuntimeRoleSemantics>,
    pub role_spawn_limits: BTreeMap<String, u32>,
    pub required_mcp_servers: Vec<McpRequirementSemantics>,
    pub runtime_requirements: Vec<RuntimeCapabilityRequirement>,
    pub artifact_contracts: Vec<ArtifactContract>,
    pub max_active_child_agents: u32,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeRoleSemantics {
    pub definition_id: String,
    pub version: String,
    pub name: String,
    pub description: String,
    pub config_file: String,
    pub config_toml: String,
    pub content_sha256: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct McpRequirementSemantics {
    pub name: String,
    pub tools: Vec<String>,
    pub capability_roots: Vec<McpCapabilityRootSemantics>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct McpCapabilityRootSemantics {
    pub capability_root_id: String,
    pub mcp_server_names: Vec<String>,
}

impl ResolvedAgentDefinition {
    pub fn execution_semantics(&self) -> AgentExecutionSemantics {
        AgentExecutionSemantics {
            definition_id: self.definition_id.clone(),
            version: self.version.clone(),
            developer_instructions: self.developer_instructions.clone(),
            input_artifact_types: self.input_artifact_types.clone(),
            output_artifact_types: self.output_artifact_types.clone(),
            required_capabilities: self.required_capabilities.clone(),
            runtime_role: (&self.runtime_role).into(),
            required_mcp_servers: self.required_mcp_servers.iter().map(Into::into).collect(),
        }
    }

    pub fn execution_semantics_sha256(&self) -> String {
        semantics_sha256(&self.execution_semantics())
    }
}

impl ResolvedSupervisorPackage {
    pub fn execution_semantics(&self) -> SupervisorExecutionSemantics {
        SupervisorExecutionSemantics {
            policy_id: self.policy_id.clone(),
            version: self.version.clone(),
            developer_instructions: self.developer_instructions.clone(),
            agents: self.agents.clone(),
            runtime_roles: self.required_runtime_roles.iter().map(Into::into).collect(),
            role_spawn_limits: self.role_spawn_limits.clone(),
            required_mcp_servers: self.required_mcp_servers.iter().map(Into::into).collect(),
            runtime_requirements: self.runtime_requirements.clone(),
            artifact_contracts: self.artifact_contracts.clone(),
            max_active_child_agents: self.max_active_child_agents,
        }
    }

    pub fn execution_semantics_sha256(&self) -> String {
        semantics_sha256(&self.execution_semantics())
    }
}

impl From<&PlatformRuntimeRole> for RuntimeRoleSemantics {
    fn from(role: &PlatformRuntimeRole) -> Self {
        Self {
            definition_id: role.definition_id.clone(),
            version: role.version.clone(),
            name: role.name.clone(),
            description: role.description.clone(),
            config_file: role.config_file.clone(),
            config_toml: role.config_toml.clone(),
            content_sha256: role.content_sha256.clone(),
        }
    }
}

impl From<&RequiredMcpServer> for McpRequirementSemantics {
    fn from(server: &RequiredMcpServer) -> Self {
        Self {
            name: server.name.clone(),
            tools: server.tools.clone(),
            capability_roots: server
                .capability_roots
                .iter()
                .map(|root| McpCapabilityRootSemantics {
                    capability_root_id: root.capability_root_id.clone(),
                    mcp_server_names: root.mcp_server_names.clone(),
                })
                .collect(),
        }
    }
}

fn semantics_sha256<T: Serialize>(semantics: &T) -> String {
    let encoded = serde_json::to_vec(semantics).expect("validated execution semantics serialize");
    hex::encode(Sha256::digest(encoded))
}
