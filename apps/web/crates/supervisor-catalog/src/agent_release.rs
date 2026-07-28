use std::collections::BTreeSet;

use open_web_codex_adapter::PlatformRuntimeRole;
use open_web_codex_platform_contracts::AgentCapabilityTemplateSelection;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::agent::{
    resolve_builtin, runtime_role_template, AgentCatalogError, ResolvedAgentDefinition,
};
use crate::validation::{
    is_safe_artifact_type, is_safe_definition_id, is_safe_runtime_role_name, is_safe_version,
};

const MAX_RUNTIME_ROLE_INSTRUCTIONS_BYTES: usize = 16 * 1024;

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AgentReleaseSpec {
    pub definition_id: String,
    pub version: String,
    pub display_name: String,
    pub description: String,
    pub responsibilities: Vec<String>,
    pub developer_instructions: String,
    pub input_artifact_types: Vec<String>,
    pub output_artifact_types: Vec<String>,
    pub capability_template: AgentCapabilityTemplateSelection,
}

pub fn validate_user_release(
    spec: AgentReleaseSpec,
    runtime_role_name: &str,
) -> Result<ResolvedAgentDefinition, AgentCatalogError> {
    validate_user_release_fields(&spec, runtime_role_name)?;
    let template = resolve_builtin(
        &spec.capability_template.definition_id,
        &spec.capability_template.version,
    )?;
    let template_inputs = template
        .input_artifact_types
        .iter()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    let template_outputs = template
        .output_artifact_types
        .iter()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    if spec
        .input_artifact_types
        .iter()
        .any(|artifact| !template_inputs.contains(artifact.as_str()))
        || spec
            .output_artifact_types
            .iter()
            .any(|artifact| !template_outputs.contains(artifact.as_str()))
    {
        return Err(AgentCatalogError::Invalid);
    }
    let config_toml = runtime_role_template(
        &spec.developer_instructions,
        &hex::encode(Sha256::digest(
            spec.developer_instructions.trim().as_bytes(),
        )),
        &template.required_mcp_servers,
    )?;
    let runtime_role = PlatformRuntimeRole {
        definition_id: spec.definition_id.clone(),
        version: spec.version.clone(),
        name: runtime_role_name.to_string(),
        description: spec.description.clone(),
        config_file: format!(
            "platform-agents/{}/{}.toml",
            spec.definition_id, spec.version
        ),
        content_sha256: hex::encode(Sha256::digest(config_toml.as_bytes())),
        config_toml,
    };
    let content_sha256 = user_release_content_sha256(&spec, &runtime_role, &template);
    Ok(ResolvedAgentDefinition {
        release_id: None,
        definition_id: spec.definition_id,
        version: spec.version,
        display_name: spec.display_name,
        description: spec.description,
        responsibilities: spec.responsibilities,
        input_artifact_types: spec.input_artifact_types,
        output_artifact_types: spec.output_artifact_types,
        required_capabilities: template.required_capabilities,
        capability_template: Some(spec.capability_template),
        runtime_role,
        required_mcp_servers: template.required_mcp_servers,
        content_sha256,
    })
}

pub fn user_runtime_role_name(
    definition_resource_id: Uuid,
    version: &str,
) -> Result<String, AgentCatalogError> {
    if !is_safe_version(version) {
        return Err(AgentCatalogError::Invalid);
    }
    let mut digest = Sha256::new();
    digest.update(b"agent-runtime-role.v1");
    digest.update(definition_resource_id.as_bytes());
    digest.update((version.len() as u64).to_be_bytes());
    digest.update(version.as_bytes());
    Ok(format!("agent_{}", &hex::encode(digest.finalize())[..32]))
}

fn validate_user_release_fields(
    spec: &AgentReleaseSpec,
    runtime_role_name: &str,
) -> Result<(), AgentCatalogError> {
    if !is_safe_definition_id(&spec.definition_id)
        || !is_safe_version(&spec.version)
        || !is_safe_runtime_role_name(runtime_role_name)
        || !runtime_role_name.starts_with("agent_")
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
        || spec.developer_instructions.len() > MAX_RUNTIME_ROLE_INSTRUCTIONS_BYTES
        || spec.developer_instructions.contains("'''")
        || spec.input_artifact_types.len() > 32
        || spec.output_artifact_types.is_empty()
        || spec.output_artifact_types.len() > 32
        || spec
            .input_artifact_types
            .iter()
            .chain(spec.output_artifact_types.iter())
            .any(|value| !is_safe_artifact_type(value))
        || !is_safe_definition_id(&spec.capability_template.definition_id)
        || !is_safe_version(&spec.capability_template.version)
    {
        return Err(AgentCatalogError::Invalid);
    }
    let mut inputs = BTreeSet::new();
    let mut outputs = BTreeSet::new();
    if spec
        .input_artifact_types
        .iter()
        .any(|value| !inputs.insert(value))
        || spec
            .output_artifact_types
            .iter()
            .any(|value| !outputs.insert(value))
    {
        return Err(AgentCatalogError::Invalid);
    }
    Ok(())
}

fn user_release_content_sha256(
    spec: &AgentReleaseSpec,
    runtime_role: &PlatformRuntimeRole,
    template: &ResolvedAgentDefinition,
) -> String {
    let mut digest = Sha256::new();
    let serialized_spec = serde_json::to_string(spec).expect("validated Agent Release serializes");
    for field in [
        b"agent-release.v1".as_slice(),
        serialized_spec.as_bytes(),
        runtime_role.name.as_bytes(),
        runtime_role.content_sha256.as_bytes(),
        template.definition_id.as_bytes(),
        template.version.as_bytes(),
        template.content_sha256.as_bytes(),
    ] {
        digest.update((field.len() as u64).to_be_bytes());
        digest.update(field);
    }
    hex::encode(digest.finalize())
}
