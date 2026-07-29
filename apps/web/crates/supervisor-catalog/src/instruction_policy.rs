use open_web_codex_platform_contracts::{
    SupervisorInstructionPolicyDetail, SupervisorInstructionPolicyOrigin,
    SupervisorInstructionPolicySelection, SupervisorInstructionPolicySummary,
};
use serde::Deserialize;
use sha2::{Digest, Sha256};

use crate::supervisor::SupervisorCatalogError;
use crate::validation::{is_safe_definition_id, is_safe_version};

const PLATFORM_SUPERVISOR_BEHAVIOR: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../../../capabilities/supervisor-instruction-policies/platform-supervisor-behavior/1.1.0/instructions.md"
));
const PLATFORM_SUPERVISOR_BEHAVIOR_MANIFEST: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../../../capabilities/supervisor-instruction-policies/platform-supervisor-behavior/1.1.0/manifest.json"
));

struct PublishedInstructionPolicyResource {
    manifest: &'static str,
    platform_instructions: &'static str,
}

const PUBLISHED_INSTRUCTION_POLICIES: [PublishedInstructionPolicyResource; 1] =
    [PublishedInstructionPolicyResource {
        manifest: PLATFORM_SUPERVISOR_BEHAVIOR_MANIFEST,
        platform_instructions: PLATFORM_SUPERVISOR_BEHAVIOR,
    }];

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct InstructionPolicyManifest {
    schema_version: String,
    policy_id: String,
    version: String,
    display_name: String,
    description: String,
    instructions_file: String,
}

pub fn list_published() -> Result<Vec<SupervisorInstructionPolicySummary>, SupervisorCatalogError> {
    PUBLISHED_INSTRUCTION_POLICIES
        .iter()
        .map(resolve_resource)
        .map(|result| result.map(|detail| summary(&detail)))
        .collect()
}

pub fn resolve(
    selection: &SupervisorInstructionPolicySelection,
) -> Result<SupervisorInstructionPolicyDetail, SupervisorCatalogError> {
    let resource = PUBLISHED_INSTRUCTION_POLICIES
        .iter()
        .find(|resource| {
            serde_json::from_str::<InstructionPolicyManifest>(resource.manifest).is_ok_and(
                |manifest| {
                    manifest.policy_id == selection.policy_id
                        && manifest.version == selection.version
                },
            )
        })
        .ok_or_else(|| {
            SupervisorCatalogError::InstructionPolicyNotPublished(format!(
                "{}@{}",
                selection.policy_id, selection.version
            ))
        })?;
    resolve_resource(resource)
}

pub fn summary(detail: &SupervisorInstructionPolicyDetail) -> SupervisorInstructionPolicySummary {
    SupervisorInstructionPolicySummary {
        release_id: detail.release_id,
        policy_id: detail.policy_id.clone(),
        version: detail.version.clone(),
        display_name: detail.display_name.clone(),
        description: detail.description.clone(),
        source: detail.source,
        content_sha256: detail.content_sha256.clone(),
    }
}

pub fn validate_platform_release(
    policy_id: String,
    version: String,
    display_name: String,
    description: String,
    platform_instructions: String,
) -> Result<SupervisorInstructionPolicyDetail, SupervisorCatalogError> {
    resolve_fields(
        None,
        SupervisorInstructionPolicyOrigin::PlatformRelease,
        policy_id,
        version,
        display_name,
        description,
        platform_instructions,
    )
}

fn resolve_resource(
    resource: &PublishedInstructionPolicyResource,
) -> Result<SupervisorInstructionPolicyDetail, SupervisorCatalogError> {
    let manifest =
        serde_json::from_str::<InstructionPolicyManifest>(resource.manifest).map_err(|_| {
            SupervisorCatalogError::Invalid("Supervisor instruction policy manifest is invalid")
        })?;
    if manifest.schema_version != "supervisor-instruction-policy.v1"
        || manifest.instructions_file != "instructions.md"
    {
        return Err(SupervisorCatalogError::Invalid(
            "Supervisor instruction policy manifest is invalid",
        ));
    }
    resolve_fields(
        None,
        SupervisorInstructionPolicyOrigin::Repository,
        manifest.policy_id,
        manifest.version,
        manifest.display_name,
        manifest.description,
        resource.platform_instructions.to_string(),
    )
}

fn resolve_fields(
    release_id: Option<uuid::Uuid>,
    source: SupervisorInstructionPolicyOrigin,
    policy_id: String,
    version: String,
    display_name: String,
    description: String,
    platform_instructions: String,
) -> Result<SupervisorInstructionPolicyDetail, SupervisorCatalogError> {
    let platform_instructions = platform_instructions.trim();
    if !is_safe_definition_id(&policy_id)
        || !is_safe_version(&version)
        || display_name.trim().is_empty()
        || display_name.len() > 256
        || description.trim().is_empty()
        || description.len() > 512
        || platform_instructions.is_empty()
        || platform_instructions.len() > 16 * 1024
    {
        return Err(SupervisorCatalogError::Invalid(
            "Supervisor instruction policy is invalid",
        ));
    }
    Ok(SupervisorInstructionPolicyDetail {
        release_id,
        policy_id,
        version,
        display_name,
        description,
        source,
        platform_instructions: platform_instructions.to_string(),
        content_sha256: hex::encode(Sha256::digest(platform_instructions.as_bytes())),
    })
}
