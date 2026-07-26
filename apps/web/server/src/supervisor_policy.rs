use open_web_codex_adapter::{CodexAdapter, ProfileQuery};
use open_web_codex_codex_contracts::{CapabilityManifest, CapabilityStatus};
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
const ENTERPRISE_COPILOT_POLICY_INSTRUCTIONS: &str =
    include_str!("../resources/supervisor-policies/enterprise-supervisor-copilot-v1.md");

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
    pub required_runtime_roles: Vec<String>,
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
    let required_runtime_roles = agent_definition::enterprise_runtime_roles()
        .map_err(|_| SupervisorPolicyError::Invalid("Agent Definitions are invalid"))?;
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

pub(crate) async fn require_runtime_capabilities(
    adapter: &dyn CodexAdapter,
    manifest: &Value,
    required_runtime_roles: &[String],
) -> Result<(), SupervisorPolicyError> {
    validate_runtime_manifest(manifest)?;
    let config = adapter
        .query_profile(ProfileQuery::Config)
        .await
        .map_err(|_| {
            SupervisorPolicyError::Capability(
                "Codex Profile configuration could not be read".to_string(),
            )
        })?;
    validate_runtime_config(&config, required_runtime_roles)
}

fn validate_runtime_manifest(value: &Value) -> Result<(), SupervisorPolicyError> {
    let manifest = serde_json::from_value::<CapabilityManifest>(value.clone()).map_err(|_| {
        SupervisorPolicyError::Capability(
            "Codex Capability Manifest could not be validated".to_string(),
        )
    })?;
    let capability = manifest
        .capabilities
        .into_iter()
        .find(|capability| capability.id == "agents.multi_agent")
        .ok_or_else(|| {
            SupervisorPolicyError::Capability(
                "Codex did not declare multi-agent support".to_string(),
            )
        })?;
    if capability.version != "1.0.0" {
        return Err(SupervisorPolicyError::Capability(format!(
            "Codex multi-agent capability version '{}' is unsupported",
            capability.version
        )));
    }
    let enabled = match capability.status {
        CapabilityStatus::Supported => true,
        CapabilityStatus::Experimental => capability.experimental,
        CapabilityStatus::Unsupported
        | CapabilityStatus::Degraded
        | CapabilityStatus::Incompatible => false,
    };
    if !enabled {
        return Err(SupervisorPolicyError::Capability(
            "Codex multi-agent capability is unavailable".to_string(),
        ));
    }
    Ok(())
}

fn validate_runtime_config(
    value: &Value,
    required_runtime_roles: &[String],
) -> Result<(), SupervisorPolicyError> {
    let config = value
        .get("config")
        .and_then(Value::as_object)
        .ok_or_else(|| {
            SupervisorPolicyError::Capability(
                "Codex Profile configuration is unavailable".to_string(),
            )
        })?;
    let multi_agent_enabled = config
        .get("features")
        .and_then(Value::as_object)
        .and_then(|features| features.get("multi_agent"))
        .and_then(Value::as_bool)
        .unwrap_or(false);
    if !multi_agent_enabled {
        return Err(SupervisorPolicyError::Capability(
            "Codex multi-agent support is disabled".to_string(),
        ));
    }
    let agents = config
        .get("agents")
        .and_then(Value::as_object)
        .ok_or_else(|| {
            SupervisorPolicyError::Capability(
                "Codex Runtime Role configuration is unavailable".to_string(),
            )
        })?;
    if agents.get("enabled").and_then(Value::as_bool) == Some(false) {
        return Err(SupervisorPolicyError::Capability(
            "Codex multi-agent support is disabled".to_string(),
        ));
    }
    let max_threads = agents
        .get("max_concurrent_threads_per_session")
        .and_then(Value::as_u64)
        .unwrap_or_default();
    let max_depth = agents
        .get("max_depth")
        .and_then(Value::as_u64)
        .unwrap_or_default();
    if max_threads < 2 || max_depth < 1 {
        return Err(SupervisorPolicyError::Capability(
            "Codex Agent limits do not permit the required collaboration".to_string(),
        ));
    }
    let missing = required_runtime_roles
        .iter()
        .filter(|role| {
            agents
                .get(role.as_str())
                .and_then(Value::as_object)
                .and_then(|definition| definition.get("config_file"))
                .and_then(Value::as_str)
                .is_none_or(|config_file| config_file.trim().is_empty())
        })
        .cloned()
        .collect::<Vec<_>>();
    if !missing.is_empty() {
        return Err(SupervisorPolicyError::Capability(format!(
            "required Runtime Roles are not configured: {}",
            missing.join(", ")
        )));
    }
    Ok(())
}

#[cfg(test)]
#[path = "supervisor_policy_tests.rs"]
mod tests;
