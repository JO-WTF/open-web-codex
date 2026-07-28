use open_web_codex_platform_contracts::{
    SupervisorDraftRequest, SupervisorInstructionPolicyDetail, SupervisorValidationIssue,
    SupervisorValidationResult,
};
use open_web_codex_supervisor_catalog::{
    agent::ResolvedAgentDefinition,
    supervisor::{self, SupervisorReleaseSpec},
};
use sqlx::PgPool;
use uuid::Uuid;

use crate::agent_catalog;
use crate::supervisor_instruction_policy;

use super::{bad_request, ApiError};

pub(super) async fn validate_draft(
    db: &PgPool,
    organization_id: Uuid,
    draft: &SupervisorDraftRequest,
) -> SupervisorValidationResult {
    match resolve_draft(db, organization_id, draft).await {
        Ok(package) => SupervisorValidationResult {
            valid: true,
            execution_semantics_sha256: Some(package.execution_semantics_sha256()),
            content_sha256: Some(package.content_sha256),
            issues: Vec::new(),
        },
        Err(issue) => SupervisorValidationResult {
            valid: false,
            content_sha256: None,
            execution_semantics_sha256: None,
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
    let instruction_policy = supervisor_instruction_policy::resolve(db, &draft.instruction_policy)
        .await
        .map_err(|error| validation_issue("instruction_policy_invalid", error.to_string()))?;
    supervisor::resolve_authoring_spec_with_policy(
        draft.clone(),
        &available_agents,
        &instruction_policy,
    )
    .map_err(catalog_issue)
}

#[cfg(test)]
pub(super) fn release_spec_from_draft(
    draft: &SupervisorDraftRequest,
    published: &[ResolvedAgentDefinition],
) -> Result<SupervisorReleaseSpec, SupervisorValidationIssue> {
    supervisor::release_spec_from_authoring(draft.clone(), published).map_err(catalog_issue)
}

pub(super) fn release_spec_from_draft_with_policy(
    draft: &SupervisorDraftRequest,
    published: &[ResolvedAgentDefinition],
    instruction_policy: &SupervisorInstructionPolicyDetail,
) -> Result<SupervisorReleaseSpec, SupervisorValidationIssue> {
    supervisor::release_spec_from_authoring_with_policy(
        draft.clone(),
        published,
        instruction_policy,
    )
    .map_err(catalog_issue)
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
        || draft.custom_instructions.len() > 16 * 1024
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

fn catalog_issue(error: supervisor::SupervisorCatalogError) -> SupervisorValidationIssue {
    let code = match error {
        supervisor::SupervisorCatalogError::AgentNotPublished(_) => "agent_not_published",
        supervisor::SupervisorCatalogError::InstructionPolicyNotPublished(_) => {
            "instruction_policy_not_published"
        }
        supervisor::SupervisorCatalogError::NotFound
        | supervisor::SupervisorCatalogError::Invalid(_) => "invalid_release",
    };
    validation_issue(code, error.to_string())
}
