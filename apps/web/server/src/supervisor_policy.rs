use open_web_codex_adapter::PlatformRuntimeRole;
use open_web_codex_codex_contracts::{CapabilityDeclaration, CapabilityManifest, CapabilityStatus};
use open_web_codex_platform_contracts::{SupervisorPolicySelection, SupervisorPolicySummary};
use open_web_codex_run_orchestrator::SupervisorPolicySnapshotInput;
use serde_json::Value;
use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::agent_definition;

const ENTERPRISE_COPILOT_POLICY_ID: &str = "enterprise-supervisor-copilot";
const ENTERPRISE_COPILOT_POLICY_VERSION: &str = "1.0.0";
const ENTERPRISE_COPILOT_POLICY_NAME: &str = "Enterprise Supervisor Copilot";
const ENTERPRISE_COPILOT_POLICY_DESCRIPTION: &str =
    "Coordinates Data and Network Planning agents through governed Artifacts.";
const REQUIRED_RUNTIME_CAPABILITY_VERSION: &str = "1.0.0";
const REQUIRED_RUNTIME_CAPABILITIES: [(&str, &str); 2] = [
    ("agents.multi_agent", "multi-agent"),
    (
        "agents.multi_agent_v1_backend_override",
        "multi-agent V1 backend override",
    ),
];
const ENTERPRISE_COPILOT_POLICY_INSTRUCTIONS: &str =
    include_str!("../resources/supervisor-policies/enterprise-supervisor-copilot-v1.md");
const ENTERPRISE_COPILOT_RUNTIME_ROLE_REFS: [(&str, &str, &str); 2] = [
    ("enterprise-data-agent", "1.0.0", "data_agent"),
    (
        "enterprise-network-planning-agent",
        "1.0.0",
        "network_planning_agent",
    ),
];

#[derive(Debug, Error, PartialEq, Eq)]
pub(crate) enum SupervisorPolicyError {
    #[error("Supervisor Policy was not found")]
    NotFound,
    #[error("Supervisor Policy content is invalid: {0}")]
    Invalid(&'static str),
    #[error("required Runtime capabilities are unavailable: {0}")]
    Capability(String),
}

#[derive(Debug)]
pub(crate) struct ResolvedSupervisorPolicy {
    pub snapshot: SupervisorPolicySnapshotInput,
    pub required_runtime_roles: Vec<PlatformRuntimeRole>,
}

pub(crate) fn list_published() -> Vec<SupervisorPolicySummary> {
    vec![SupervisorPolicySummary {
        policy_id: ENTERPRISE_COPILOT_POLICY_ID.to_string(),
        version: ENTERPRISE_COPILOT_POLICY_VERSION.to_string(),
        display_name: ENTERPRISE_COPILOT_POLICY_NAME.to_string(),
        description: ENTERPRISE_COPILOT_POLICY_DESCRIPTION.to_string(),
    }]
}

pub(crate) fn resolve(
    selection: &SupervisorPolicySelection,
) -> Result<ResolvedSupervisorPolicy, SupervisorPolicyError> {
    if selection.policy_id != ENTERPRISE_COPILOT_POLICY_ID
        || selection.version != ENTERPRISE_COPILOT_POLICY_VERSION
    {
        return Err(SupervisorPolicyError::NotFound);
    }
    let developer_instructions = ENTERPRISE_COPILOT_POLICY_INSTRUCTIONS.trim();
    if developer_instructions.is_empty() {
        return Err(SupervisorPolicyError::Invalid(
            "developer instructions are empty",
        ));
    }
    if developer_instructions.len() > 16 * 1024 {
        return Err(SupervisorPolicyError::Invalid(
            "developer instructions exceed 16384 bytes",
        ));
    }
    let required_runtime_roles = resolve_enterprise_runtime_roles()?;
    Ok(ResolvedSupervisorPolicy {
        snapshot: SupervisorPolicySnapshotInput {
            policy_id: selection.policy_id.clone(),
            version: selection.version.clone(),
            display_name: ENTERPRISE_COPILOT_POLICY_NAME.to_string(),
            developer_instructions: developer_instructions.to_string(),
            content_sha256: hex::encode(Sha256::digest(developer_instructions.as_bytes())),
        },
        required_runtime_roles,
    })
}

fn resolve_enterprise_runtime_roles() -> Result<Vec<PlatformRuntimeRole>, SupervisorPolicyError> {
    let published = agent_definition::platform_runtime_roles()
        .map_err(|_| SupervisorPolicyError::Invalid("Agent Definitions are invalid"))?;
    ENTERPRISE_COPILOT_RUNTIME_ROLE_REFS
        .into_iter()
        .map(|(definition_id, version, name)| {
            published
                .iter()
                .find(|role| {
                    role.definition_id == definition_id
                        && role.version == version
                        && role.name == name
                })
                .cloned()
                .ok_or(SupervisorPolicyError::Invalid(
                    "required Agent Definition is not published",
                ))
        })
        .collect()
}
pub(crate) fn require_runtime_manifest(manifest: &Value) -> Result<(), SupervisorPolicyError> {
    validate_runtime_manifest(manifest)
}

fn validate_runtime_manifest(value: &Value) -> Result<(), SupervisorPolicyError> {
    let manifest = serde_json::from_value::<CapabilityManifest>(value.clone()).map_err(|_| {
        SupervisorPolicyError::Capability(
            "Codex Capability Manifest could not be validated".to_string(),
        )
    })?;
    for (id, label) in REQUIRED_RUNTIME_CAPABILITIES {
        let capability = manifest
            .capabilities
            .iter()
            .find(|capability| capability.id == id)
            .ok_or_else(|| {
                SupervisorPolicyError::Capability(format!("Codex did not declare {label} support"))
            })?;
        validate_required_runtime_capability(capability, label)?;
    }
    Ok(())
}

fn validate_required_runtime_capability(
    capability: &CapabilityDeclaration,
    label: &str,
) -> Result<(), SupervisorPolicyError> {
    if capability.version != REQUIRED_RUNTIME_CAPABILITY_VERSION {
        return Err(SupervisorPolicyError::Capability(format!(
            "Codex {label} capability version '{}' is unsupported",
            capability.version,
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
            "Codex {label} capability is unavailable"
        )));
    }
    Ok(())
}

#[cfg(test)]
#[path = "supervisor_policy_tests.rs"]
mod tests;
