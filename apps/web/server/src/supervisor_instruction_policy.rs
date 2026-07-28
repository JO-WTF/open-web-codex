use open_web_codex_platform_contracts::{
    SupervisorInstructionPolicyDetail, SupervisorInstructionPolicyOrigin,
    SupervisorInstructionPolicySelection, SupervisorInstructionPolicySummary,
};
use open_web_codex_supervisor_catalog::{instruction_policy, supervisor::SupervisorCatalogError};
use sqlx::{PgPool, Row};
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub(crate) enum InstructionPolicyError {
    #[error("Supervisor instruction policy was not found")]
    NotFound,
    #[error("Supervisor instruction policy is invalid")]
    Invalid,
    #[error("Supervisor instruction policy database operation failed")]
    Database,
}

pub(crate) async fn list_published(
    db: &PgPool,
) -> Result<Vec<SupervisorInstructionPolicySummary>, InstructionPolicyError> {
    let mut policies = instruction_policy::list_published().map_err(map_catalog_error)?;
    let rows = sqlx::query(
        "SELECT id, policy_id, version, display_name, description, platform_instructions, \
                content_sha256 \
         FROM supervisor_instruction_policy_releases \
         ORDER BY published_at DESC, policy_id, version",
    )
    .fetch_all(db)
    .await
    .map_err(|_| InstructionPolicyError::Database)?;
    for row in rows {
        policies.push(instruction_policy::summary(&resolve_row(&row)?));
    }
    Ok(policies)
}

pub(crate) async fn resolve(
    db: &PgPool,
    selection: &SupervisorInstructionPolicySelection,
) -> Result<SupervisorInstructionPolicyDetail, InstructionPolicyError> {
    let builtins = instruction_policy::list_published().map_err(map_catalog_error)?;
    if builtins.iter().any(|policy| {
        policy.policy_id == selection.policy_id && policy.version == selection.version
    }) {
        return instruction_policy::resolve(selection).map_err(map_catalog_error);
    }
    let row = sqlx::query(
        "SELECT id, policy_id, version, display_name, description, platform_instructions, \
                content_sha256 \
         FROM supervisor_instruction_policy_releases \
         WHERE policy_id = $1 AND version = $2",
    )
    .bind(&selection.policy_id)
    .bind(&selection.version)
    .fetch_optional(db)
    .await
    .map_err(|_| InstructionPolicyError::Database)?
    .ok_or(InstructionPolicyError::NotFound)?;
    resolve_row(&row)
}

pub(crate) fn is_reserved_builtin_policy_version(policy_id: &str, version: &str) -> bool {
    instruction_policy::list_published().is_ok_and(|policies| {
        policies
            .iter()
            .any(|policy| policy.policy_id == policy_id && policy.version == version)
    })
}

pub(crate) fn resolve_row(
    row: &sqlx::postgres::PgRow,
) -> Result<SupervisorInstructionPolicyDetail, InstructionPolicyError> {
    let release_id: Uuid = row.get("id");
    let mut detail = instruction_policy::validate_platform_release(
        row.get("policy_id"),
        row.get("version"),
        row.get("display_name"),
        row.get("description"),
        row.get("platform_instructions"),
    )
    .map_err(map_catalog_error)?;
    if detail.content_sha256 != row.get::<String, _>("content_sha256") {
        return Err(InstructionPolicyError::Invalid);
    }
    detail.release_id = Some(release_id);
    detail.source = SupervisorInstructionPolicyOrigin::PlatformRelease;
    Ok(detail)
}

fn map_catalog_error(error: SupervisorCatalogError) -> InstructionPolicyError {
    match error {
        SupervisorCatalogError::InstructionPolicyNotPublished(_)
        | SupervisorCatalogError::NotFound => InstructionPolicyError::NotFound,
        SupervisorCatalogError::AgentNotPublished(_) | SupervisorCatalogError::Invalid(_) => {
            InstructionPolicyError::Invalid
        }
    }
}
