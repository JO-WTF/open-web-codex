use open_web_codex_platform_contracts::{
    SupervisorDraftRequest, SupervisorValidationIssue, SupervisorValidationResult,
};
use open_web_codex_supervisor_catalog::{
    agent::ResolvedAgentDefinition,
    supervisor::{self, ArtifactContract, SupervisorAgentReference, SupervisorReleaseSpec},
};
use sqlx::PgPool;
use uuid::Uuid;

use crate::agent_catalog;

use super::{bad_request, ApiError};

pub(super) async fn validate_draft(
    db: &PgPool,
    organization_id: Uuid,
    draft: &SupervisorDraftRequest,
) -> SupervisorValidationResult {
    match resolve_draft(db, organization_id, draft).await {
        Ok(package) => SupervisorValidationResult {
            valid: true,
            content_sha256: Some(package.content_sha256),
            issues: Vec::new(),
        },
        Err(issue) => SupervisorValidationResult {
            valid: false,
            content_sha256: None,
            issues: vec![issue],
        },
    }
}

async fn resolve_draft(
    db: &PgPool,
    organization_id: Uuid,
    draft: &SupervisorDraftRequest,
) -> Result<supervisor::ResolvedSupervisorPackage, SupervisorValidationIssue> {
    let available_agents = agent_catalog::list_resolved(db, organization_id)
        .await
        .map_err(|error| validation_issue("agent_catalog_invalid", error.to_string()))?;
    supervisor::validate_release_with_agents(
        release_spec_from_draft(draft, &available_agents)?,
        &available_agents,
    )
    .map_err(|error| validation_issue("invalid_release", error.to_string()))
}

pub(super) fn release_spec_from_draft(
    draft: &SupervisorDraftRequest,
    published: &[ResolvedAgentDefinition],
) -> Result<SupervisorReleaseSpec, SupervisorValidationIssue> {
    let agents = draft
        .agents
        .iter()
        .map(|selection| {
            let identity = format!("{}@{}", selection.definition_id, selection.version);
            let definition = published
                .iter()
                .find(|definition| {
                    definition.definition_id == selection.definition_id
                        && definition.version == selection.version
                        && definition.release_id == selection.release_id
                })
                .ok_or_else(|| {
                    validation_issue(
                        "agent_not_published",
                        format!("Agent Definition '{identity}' is not published"),
                    )
                })?;
            Ok(SupervisorAgentReference {
                definition_id: selection.definition_id.clone(),
                version: selection.version.clone(),
                release_id: selection.release_id,
                runtime_role: definition.runtime_role.name.clone(),
                spawn_limit: selection.spawn_limit,
            })
        })
        .collect::<Result<Vec<_>, SupervisorValidationIssue>>()?;
    let artifact_contracts = draft
        .artifact_contracts
        .iter()
        .map(|contract| ArtifactContract {
            artifact_type: contract.artifact_type.clone(),
            producer_agent: contract.producer_agent.clone(),
            consumer_agents: contract.consumer_agents.clone(),
            handoff: "durable-resource-reference".to_string(),
            required: contract.required,
        })
        .collect();
    Ok(SupervisorReleaseSpec {
        policy_id: draft.policy_id.clone(),
        version: draft.version.clone(),
        display_name: draft.display_name.clone(),
        description: draft.description.clone(),
        responsibilities: draft.responsibilities.clone(),
        developer_instructions: draft.developer_instructions.clone(),
        agents,
        runtime_requirements: supervisor::governed_runtime_requirements(),
        artifact_contracts,
        max_active_child_agents: draft.max_active_child_agents,
    })
}

pub(super) fn validate_draft_storage_shape(draft: &SupervisorDraftRequest) -> Result<(), ApiError> {
    let safe_identifier = |value: &str, allow_period: bool, max: usize| {
        value.len() >= 2
            && value.len() <= max
            && !value.contains("..")
            && value
                .as_bytes()
                .first()
                .is_some_and(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
            && value
                .as_bytes()
                .last()
                .is_some_and(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
            && value.bytes().all(|byte| {
                byte.is_ascii_lowercase()
                    || byte.is_ascii_digit()
                    || matches!(byte, b'-' | b'_')
                    || (allow_period && byte == b'.')
            })
    };
    if !safe_identifier(&draft.policy_id, false, 96)
        || !safe_identifier(&draft.version, true, 64)
        || draft.display_name.trim().is_empty()
        || draft.display_name.len() > 160
        || draft.description.trim().is_empty()
        || draft.description.len() > 512
        || draft.developer_instructions.len() > 16 * 1024
        || draft.responsibilities.len() > 32
        || draft
            .responsibilities
            .iter()
            .any(|value| value.trim().is_empty() || value.len() > 512)
        || draft.agents.len() > 16
        || draft.agents.iter().any(|agent| {
            !safe_identifier(&agent.definition_id, false, 96)
                || !safe_identifier(&agent.version, true, 64)
                || agent.spawn_limit > 16
        })
        || draft.artifact_contracts.len() > 64
        || draft.artifact_contracts.iter().any(|contract| {
            contract.artifact_type.trim().is_empty()
                || contract.artifact_type.len() > 128
                || contract.producer_agent.trim().is_empty()
                || contract.producer_agent.len() > 192
                || contract.consumer_agents.len() > 16
                || contract
                    .consumer_agents
                    .iter()
                    .any(|consumer| consumer.trim().is_empty() || consumer.len() > 192)
        })
        || draft.max_active_child_agents > 16
    {
        return Err(bad_request("Supervisor draft fields are invalid"));
    }
    Ok(())
}

fn validation_issue(code: &str, message: String) -> SupervisorValidationIssue {
    SupervisorValidationIssue {
        code: code.to_string(),
        message,
    }
}
