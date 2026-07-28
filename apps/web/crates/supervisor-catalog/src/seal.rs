use std::collections::{BTreeMap, BTreeSet};

use open_web_codex_adapter::{PlatformRuntimeRole, RequiredMcpServer};
use sha2::{Digest, Sha256};

use crate::agent;
use crate::supervisor::{ArtifactContract, SupervisorCatalogError, SupervisorReleaseSpec};
use crate::validation::is_safe_artifact_type;

pub(crate) fn validate_artifact_contracts(
    contracts: &[ArtifactContract],
    agents: &BTreeMap<String, agent::AgentContract>,
) -> Result<(), SupervisorCatalogError> {
    if contracts.is_empty() || contracts.len() > 64 {
        return Err(SupervisorCatalogError::Invalid(
            "Artifact contract set is invalid",
        ));
    }
    let mut identities = BTreeSet::new();
    for contract in contracts {
        let producer =
            agents
                .get(&contract.producer_agent)
                .ok_or(SupervisorCatalogError::Invalid(
                    "Artifact producer is not selected",
                ))?;
        if !is_safe_artifact_type(&contract.artifact_type)
            || contract.handoff != "durable-resource-reference"
            || contract.consumer_agents.is_empty()
            || contract.consumer_agents.len() > 16
            || !producer
                .output_artifact_types
                .contains(&contract.artifact_type)
            || !identities.insert(format!(
                "{}:{}",
                contract.artifact_type, contract.producer_agent
            ))
        {
            return Err(SupervisorCatalogError::Invalid(
                "Artifact contract is invalid",
            ));
        }
        for consumer_ref in &contract.consumer_agents {
            if consumer_ref == "supervisor" {
                continue;
            }
            let consumer = agents
                .get(consumer_ref)
                .ok_or(SupervisorCatalogError::Invalid(
                    "Artifact consumer is not selected",
                ))?;
            if !consumer
                .input_artifact_types
                .contains(&contract.artifact_type)
            {
                return Err(SupervisorCatalogError::Invalid(
                    "Artifact consumer does not declare the input contract",
                ));
            }
        }
    }
    Ok(())
}

pub(crate) fn package_content_sha256(
    spec: &SupervisorReleaseSpec,
    runtime_roles: &[PlatformRuntimeRole],
    required_mcp_servers: &[RequiredMcpServer],
    artifact_contracts: &[ArtifactContract],
) -> String {
    let mut digest = Sha256::new();
    update_digest_field(&mut digest, b"supervisor-package.v1");
    update_digest_field(
        &mut digest,
        serde_json::to_string(spec)
            .expect("validated Release serializes")
            .as_bytes(),
    );
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
    for server in required_mcp_servers {
        update_digest_field(&mut digest, server.name.as_bytes());
        for capability_root_id in &server.capability_root_ids {
            update_digest_field(&mut digest, capability_root_id.as_bytes());
        }
        for tool in &server.tools {
            update_digest_field(&mut digest, tool.as_bytes());
        }
    }
    update_digest_field(
        &mut digest,
        serde_json::to_string(artifact_contracts)
            .expect("validated Artifact contracts serialize")
            .as_bytes(),
    );
    hex::encode(digest.finalize())
}

fn update_digest_field(digest: &mut Sha256, value: &[u8]) {
    digest.update((value.len() as u64).to_be_bytes());
    digest.update(value);
}
