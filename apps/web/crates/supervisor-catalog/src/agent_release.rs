use std::collections::BTreeSet;

use open_web_codex_adapter::PlatformRuntimeRole;
use open_web_codex_platform_contracts::{
    AgentCapabilityTemplateSelection, AgentCapabilityTemplateSource, AgentDatasetReleaseBinding,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

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
    pub dataset_releases: Vec<AgentDatasetReleaseBinding>,
}

pub fn validate_user_release(
    spec: AgentReleaseSpec,
) -> Result<ResolvedAgentDefinition, AgentCatalogError> {
    if spec.capability_template.source != AgentCapabilityTemplateSource::RepositoryAgent
        || spec.capability_template.release_id.is_some()
    {
        return Err(AgentCatalogError::Invalid);
    }
    let template = resolve_builtin(
        &spec.capability_template.definition_id,
        &spec.capability_template.version,
    )?;
    compile_agent_release_against_template(spec, &template)
}

pub fn compile_agent_release_against_template(
    spec: AgentReleaseSpec,
    template: &ResolvedAgentDefinition,
) -> Result<ResolvedAgentDefinition, AgentCatalogError> {
    let runtime_role_name = user_runtime_role_name(&spec.definition_id, &spec.version)?;
    validate_user_release_fields(&spec, &runtime_role_name)?;
    if template.capability_template.as_ref() != Some(&spec.capability_template) {
        return Err(AgentCatalogError::Invalid);
    }
    let mut workspace_ids = spec
        .dataset_releases
        .iter()
        .map(|release| release.workspace_id)
        .collect::<BTreeSet<_>>();
    if let Some(workspace_id) = template.capability_workspace_id {
        workspace_ids.insert(workspace_id);
    }
    if workspace_ids.len() > 1 {
        return Err(AgentCatalogError::Invalid);
    }
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
    let runtime_instructions = runtime_developer_instructions(&spec);
    let config_toml = runtime_role_template(
        &runtime_instructions,
        &hex::encode(Sha256::digest(runtime_instructions.as_bytes())),
        &template.required_mcp_servers,
    )?;
    let runtime_role = PlatformRuntimeRole {
        definition_id: spec.definition_id.clone(),
        version: spec.version.clone(),
        name: runtime_role_name,
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
        capability_workspace_id: template.capability_workspace_id,
        definition_id: spec.definition_id,
        version: spec.version,
        display_name: spec.display_name,
        description: spec.description,
        responsibilities: spec.responsibilities,
        developer_instructions: spec.developer_instructions,
        runtime_developer_instructions: runtime_instructions,
        input_artifact_types: spec.input_artifact_types,
        output_artifact_types: spec.output_artifact_types,
        required_capabilities: template.required_capabilities.clone(),
        capability_template: Some(spec.capability_template),
        dataset_releases: spec.dataset_releases,
        capability_template_sha256: template.capability_template_sha256.clone(),
        runtime_role,
        required_mcp_servers: template.required_mcp_servers.clone(),
        content_sha256,
    })
}

pub fn user_runtime_role_name(
    definition_id: &str,
    version: &str,
) -> Result<String, AgentCatalogError> {
    if !is_safe_definition_id(definition_id) || !is_safe_version(version) {
        return Err(AgentCatalogError::Invalid);
    }
    let mut digest = Sha256::new();
    digest.update(b"agent-runtime-role.v1");
    digest.update((definition_id.len() as u64).to_be_bytes());
    digest.update(definition_id.as_bytes());
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
        || spec.dataset_releases.len() > 16
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
    let mut dataset_release_ids = BTreeSet::new();
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
    if spec.dataset_releases.iter().any(|release| {
        !dataset_release_ids.insert(release.release_id)
            || !is_safe_definition_id(&release.dataset_id)
            || !is_safe_version(&release.version)
            || release.display_name.trim().is_empty()
            || release.display_name.len() > 160
            || release.content_sha256.len() != 64
            || !release
                .content_sha256
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (byte.is_ascii_lowercase() && byte <= b'f'))
    }) {
        return Err(AgentCatalogError::Invalid);
    }
    Ok(())
}

fn runtime_developer_instructions(spec: &AgentReleaseSpec) -> String {
    let mut instructions = spec.developer_instructions.trim().to_string();
    if spec.dataset_releases.is_empty() {
        return instructions;
    }
    instructions.push_str(
        "\n\n# Platform-authorized Dataset Releases\n\
         Use only the exact releases below. Pass the Workspace ID, release ID, logical Dataset \
         ID/version, and content SHA-256 to compatible Tools; do not search the Workspace for \
         other data and never pass a host path.\n",
    );
    for release in &spec.dataset_releases {
        instructions.push_str(&format!(
            "- workspace_id: {}; release_id: {}; dataset: {}@{}; content_sha256: {}\n",
            release.workspace_id,
            release.release_id,
            release.dataset_id,
            release.version,
            release.content_sha256
        ));
    }
    instructions.trim().to_string()
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
        template.capability_template_sha256.as_bytes(),
    ] {
        digest.update((field.len() as u64).to_be_bytes());
        digest.update(field);
    }
    hex::encode(digest.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;
    use open_web_codex_platform_contracts::AgentDatasetReleaseBinding;
    use uuid::Uuid;

    #[test]
    fn dataset_binding_compiles_into_runtime_instructions() {
        let template = resolve_builtin("enterprise-data-agent", "5.0.0").unwrap();
        let workspace_id = Uuid::parse_str("0198d5b5-7d0f-7a62-8d9a-f6472dbfab11").unwrap();
        let release_id = Uuid::parse_str("0198d5b5-7d0f-7a62-8d9a-f6472dbfab12").unwrap();
        let mut spec = template.authoring_spec();
        spec.definition_id = "indonesia-data-agent".to_string();
        spec.version = "1.0.0".to_string();
        spec.developer_instructions =
            "Inspect only the exact platform-authorized Indonesia Dataset Release.".to_string();
        spec.dataset_releases = vec![AgentDatasetReleaseBinding {
            release_id,
            workspace_id,
            dataset_id: "indonesia-network".to_string(),
            version: "1.0.0".to_string(),
            display_name: "Indonesia Network".to_string(),
            content_sha256: "a".repeat(64),
        }];

        let runtime_role_name = user_runtime_role_name(&spec.definition_id, &spec.version).unwrap();
        validate_user_release_fields(&spec, &runtime_role_name)
            .expect("Dataset Release fields should be valid");
        let instructions = runtime_developer_instructions(&spec);
        runtime_role_template(
            &instructions,
            &hex::encode(Sha256::digest(instructions.as_bytes())),
            &template.required_mcp_servers,
        )
        .expect("Dataset Release instructions should compile as Runtime TOML");
    }
}
