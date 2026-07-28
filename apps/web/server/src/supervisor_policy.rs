use open_web_codex_adapter::{PlatformRuntimeRole, RequiredMcpServer};
use open_web_codex_codex_contracts::{CapabilityDeclaration, CapabilityManifest, CapabilityStatus};
use open_web_codex_platform_contracts::{SupervisorPolicySelection, SupervisorPolicySummary};
use open_web_codex_run_orchestrator::SupervisorPolicySnapshotInput;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use thiserror::Error;

use crate::agent_definition;

const ENTERPRISE_COPILOT_POLICY_ID: &str = "enterprise-supervisor-copilot";
const ENTERPRISE_COPILOT_POLICY_VERSION: &str = "1.7.0";
const ENTERPRISE_COPILOT_POLICY_NAME: &str = "Enterprise Supervisor Copilot";
const ENTERPRISE_COPILOT_POLICY_DESCRIPTION: &str =
    "Coordinates governed data analysis and warehouse-network planning agents.";
const REQUIRED_RUNTIME_CAPABILITY_VERSION: &str = "1.0.0";
const REQUIRED_RUNTIME_CAPABILITIES: [(&str, &str); 1] = [("agents.multi_agent", "multi-agent")];
const ENTERPRISE_COPILOT_POLICY_INSTRUCTIONS: &str =
    include_str!("../resources/supervisor-policies/enterprise-supervisor-copilot-v1.7.md");
const ENTERPRISE_COPILOT_RUNTIME_ROLE_REFS: [(&str, &str, &str); 2] = [
    ("enterprise-data-agent", "1.6.0", "data_agent"),
    (
        "enterprise-network-planning-agent",
        "1.5.0",
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
    pub role_spawn_limits: BTreeMap<String, u32>,
    pub required_mcp_servers: Vec<RequiredMcpServer>,
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
    let required_runtime_roles = resolve_runtime_roles()?;
    let role_spawn_limits = required_runtime_roles
        .iter()
        .map(|role| (role.name.clone(), 1))
        .collect::<BTreeMap<_, _>>();
    let required_mcp_servers = agent_definition::required_mcp_servers(&required_runtime_roles)
        .map_err(|_| SupervisorPolicyError::Invalid("Agent Definitions are invalid"))?;
    Ok(ResolvedSupervisorPolicy {
        snapshot: SupervisorPolicySnapshotInput {
            policy_id: selection.policy_id.clone(),
            version: selection.version.clone(),
            display_name: ENTERPRISE_COPILOT_POLICY_NAME.to_string(),
            developer_instructions: developer_instructions.to_string(),
            content_sha256: policy_content_sha256(
                developer_instructions,
                &required_runtime_roles,
                &role_spawn_limits,
                &required_mcp_servers,
            ),
        },
        required_runtime_roles,
        role_spawn_limits,
        required_mcp_servers,
    })
}

pub(crate) fn resolve_for_new_run(
    selection: &SupervisorPolicySelection,
) -> Result<ResolvedSupervisorPolicy, SupervisorPolicyError> {
    resolve(selection)
}

/// Seal the complete executable Policy contract, not only the Supervisor prompt.
fn policy_content_sha256(
    developer_instructions: &str,
    runtime_roles: &[PlatformRuntimeRole],
    role_spawn_limits: &BTreeMap<String, u32>,
    required_mcp_servers: &[RequiredMcpServer],
) -> String {
    let mut digest = Sha256::new();
    update_digest_field(&mut digest, b"enterprise-supervisor-policy.v9");
    update_digest_field(&mut digest, b"selected-capability-roots-exact");
    update_digest_field(&mut digest, b"agent-role-allowlist-exact");
    update_digest_field(&mut digest, b"child-agent-delegation-disabled");
    update_digest_field(
        &mut digest,
        b"child-agent-multi-agent-v2-disabled-explicitly",
    );
    update_digest_field(&mut digest, b"ordinary-apps-disabled");
    update_digest_field(&mut digest, b"ordinary-plugins-disabled");
    update_digest_field(&mut digest, developer_instructions.as_bytes());
    for role in runtime_roles {
        for field in [
            role.definition_id.as_bytes(),
            role.version.as_bytes(),
            role.name.as_bytes(),
            role.content_sha256.as_bytes(),
        ] {
            update_digest_field(&mut digest, field);
        }
    }
    for (role, limit) in role_spawn_limits {
        update_digest_field(&mut digest, role.as_bytes());
        update_digest_field(&mut digest, &limit.to_be_bytes());
    }
    for server in required_mcp_servers {
        update_digest_field(&mut digest, server.name.as_bytes());
        for capability_root_id in &server.capability_root_ids {
            update_digest_field(&mut digest, capability_root_id.as_bytes());
        }
        for tool in &server.tools {
            update_digest_field(&mut digest, tool.as_bytes());
        }
    }
    hex::encode(digest.finalize())
}

fn update_digest_field(digest: &mut Sha256, value: &[u8]) {
    digest.update((value.len() as u64).to_be_bytes());
    digest.update(value);
}

fn resolve_runtime_roles() -> Result<Vec<PlatformRuntimeRole>, SupervisorPolicyError> {
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
    let exact_role_allowlist = capability
        .limits
        .get("exactRoleAllowlist")
        .and_then(Value::as_bool)
        == Some(true);
    if capability.id == "agents.multi_agent" && !exact_role_allowlist {
        return Err(SupervisorPolicyError::Capability(
            "Codex multi-agent exact role allowlist is unavailable".to_string(),
        ));
    }
    let exact_role_instance_limits = capability
        .limits
        .get("exactRoleInstanceLimits")
        .and_then(Value::as_bool)
        == Some(true);
    if capability.id == "agents.multi_agent" && !exact_role_instance_limits {
        return Err(SupervisorPolicyError::Capability(
            "Codex multi-agent exact role instance limits are unavailable".to_string(),
        ));
    }
    Ok(())
}

#[cfg(test)]
#[path = "supervisor_policy_tests.rs"]
mod tests;
