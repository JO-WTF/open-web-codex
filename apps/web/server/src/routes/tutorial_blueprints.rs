use std::collections::{BTreeMap, BTreeSet};
use std::ops::Deref;
use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Extension, Json,
};
use open_web_codex_git_runtime::GitRuntime;
use open_web_codex_platform_contracts::error::PlatformError;
use open_web_codex_platform_contracts::{
    AgentCapabilityTemplateSelection, AgentCapabilityTemplateSource, AgentDefinitionDraftRequest,
    PublishWorkspaceDatasetRequest, ReconcileTutorialBlueprintRequest, SupervisorAgentSelection,
    SupervisorArtifactContractInput, SupervisorDraftRequest, SupervisorPolicySelection,
    TutorialBlueprint, TutorialBlueprintAgentTemplate, TutorialBlueprintDataset,
    TutorialBlueprintInstructionPolicyTemplate, TutorialBlueprintIssue,
    TutorialBlueprintReconcileResponse, TutorialBlueprintReconcileStatus, TutorialBlueprintSummary,
    TutorialBlueprintSupervisorTemplate, WorkspaceDatasetReleaseSummary,
    WorkspaceDatasetUploadFile,
};
use open_web_codex_platform_store::AppState;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::middleware::auth::AuthenticatedUser;
use crate::{agent_catalog, supervisor_policy};

use super::{
    agent_definition_resources, supervisor_definitions,
    workspace_datasets::{self},
    workspaces::authorized_workspace,
};

type ApiError = (StatusCode, Json<PlatformError>);
type ApiResult<T> = Result<Json<T>, ApiError>;

const BLUEPRINT_MANIFEST: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../../capabilities/tutorial-blueprints/indonesia-warehouse-network/1.4.0/manifest.json"
));
const RECOMMENDED_PROMPT: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../../capabilities/tutorial-blueprints/indonesia-warehouse-network/1.4.0/recommended-prompt.md"
));
const DATASET_MANIFEST: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../../tools/supply-chain-network-planner/examples/indonesia-tutorial/releases/1.0.0/dataset-manifest.json"
));
const PROVINCE_BOUNDARIES: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../../tools/supply-chain-network-planner/examples/indonesia-tutorial/releases/1.0.0/province-boundaries.geojson"
));
const CUSTOMERS: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../../tools/supply-chain-network-planner/examples/indonesia-tutorial/releases/1.0.0/customers.csv.gz"
));
const CUSTOMER_ASSIGNMENTS: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../../tools/supply-chain-network-planner/examples/indonesia-tutorial/releases/1.0.0/customer-assignments.csv.gz"
));
const WAREHOUSES: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../../tools/supply-chain-network-planner/examples/indonesia-tutorial/releases/1.0.0/warehouses.csv"
));
const WAREHOUSE_LINKS: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../../tools/supply-chain-network-planner/examples/indonesia-tutorial/releases/1.0.0/warehouse-links.csv"
));
const CANDIDATE_LOCATIONS: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../../tools/supply-chain-network-planner/examples/indonesia-tutorial/releases/1.0.0/candidate-locations.csv"
));
const TRANSPORT_QUOTES: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../../tools/supply-chain-network-planner/examples/indonesia-tutorial/releases/1.0.0/transport-quotes.csv"
));
const PLANNING_POLICY: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../../tools/supply-chain-network-planner/examples/indonesia-tutorial/releases/1.0.0/planning-policy.json"
));
const VALIDATION_REPORT: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../../tools/supply-chain-network-planner/examples/indonesia-tutorial/releases/1.0.0/validation-report.json"
));

#[derive(Clone, Copy)]
struct DatasetBundleFile<'a> {
    logical_name: &'a str,
    bytes: &'a [u8],
}

struct PublishedBlueprintResource {
    manifest: &'static str,
    recommended_prompt: &'static str,
    dataset_files: &'static [DatasetBundleFile<'static>],
}

const INDONESIA_DATASET_FILES: [DatasetBundleFile<'static>; 10] = [
    DatasetBundleFile {
        logical_name: "dataset-manifest.json",
        bytes: DATASET_MANIFEST,
    },
    DatasetBundleFile {
        logical_name: "province-boundaries.geojson",
        bytes: PROVINCE_BOUNDARIES,
    },
    DatasetBundleFile {
        logical_name: "customers.csv.gz",
        bytes: CUSTOMERS,
    },
    DatasetBundleFile {
        logical_name: "customer-assignments.csv.gz",
        bytes: CUSTOMER_ASSIGNMENTS,
    },
    DatasetBundleFile {
        logical_name: "warehouses.csv",
        bytes: WAREHOUSES,
    },
    DatasetBundleFile {
        logical_name: "warehouse-links.csv",
        bytes: WAREHOUSE_LINKS,
    },
    DatasetBundleFile {
        logical_name: "candidate-locations.csv",
        bytes: CANDIDATE_LOCATIONS,
    },
    DatasetBundleFile {
        logical_name: "transport-quotes.csv",
        bytes: TRANSPORT_QUOTES,
    },
    DatasetBundleFile {
        logical_name: "planning-policy.json",
        bytes: PLANNING_POLICY,
    },
    DatasetBundleFile {
        logical_name: "validation-report.json",
        bytes: VALIDATION_REPORT,
    },
];

const PUBLISHED_BLUEPRINT_RESOURCES: [PublishedBlueprintResource; 1] =
    [PublishedBlueprintResource {
        manifest: BLUEPRINT_MANIFEST,
        recommended_prompt: RECOMMENDED_PROMPT,
        dataset_files: &INDONESIA_DATASET_FILES,
    }];

struct LoadedBlueprint {
    definition: PublishedBlueprint,
    resource: &'static PublishedBlueprintResource,
}

impl Deref for LoadedBlueprint {
    type Target = PublishedBlueprint;

    fn deref(&self) -> &Self::Target {
        &self.definition
    }
}

impl LoadedBlueprint {
    fn dataset_bytes(&self, logical_name: &str) -> Option<&'static [u8]> {
        self.resource
            .dataset_files
            .iter()
            .find(|file| file.logical_name == logical_name)
            .map(|file| file.bytes)
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PublishedBlueprint {
    schema_version: String,
    blueprint_id: String,
    revision: String,
    display_name: String,
    description: String,
    estimated_minutes: u32,
    dataset: PublishedDataset,
    agent_templates: Vec<PublishedAgentTemplate>,
    supervisor_template: PublishedSupervisorTemplate,
    instruction_policy_template: PublishedInstructionPolicyTemplate,
    required_mcp_servers: Vec<String>,
    expected_artifact_types: Vec<String>,
    recommended_prompt_file: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PublishedDataset {
    dataset_id: String,
    version: String,
    display_name: String,
    description: String,
    source_content_sha256: String,
    files: Vec<PublishedDatasetFile>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PublishedDatasetFile {
    logical_name: String,
    role: String,
    media_type: String,
    bytes: u64,
    sha256: String,
}

#[derive(Debug, Deserialize)]
struct SourceDatasetManifest {
    schema_version: String,
    dataset_id: String,
    version: String,
    content_sha256: String,
    files: Vec<SourceDatasetFile>,
}

#[derive(Debug, Deserialize)]
struct SourceDatasetFile {
    path: String,
    role: String,
    bytes: u64,
    sha256: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PublishedAgentTemplate {
    definition_id: String,
    version: String,
    content_sha256: String,
    uses_blueprint_dataset: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PublishedSupervisorTemplate {
    policy_id: String,
    version: String,
    content_sha256: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PublishedInstructionPolicyTemplate {
    policy_id: String,
    version: String,
    content_sha256: String,
}

pub async fn list(_auth: AuthenticatedUser) -> ApiResult<Vec<TutorialBlueprintSummary>> {
    Ok(Json(
        load_blueprints()?
            .iter()
            .map(|blueprint| summary(blueprint))
            .collect(),
    ))
}

pub async fn get(
    _auth: AuthenticatedUser,
    Path((blueprint_id, revision)): Path<(String, String)>,
) -> ApiResult<TutorialBlueprint> {
    let blueprint = selected_blueprint(&blueprint_id, &revision)?;
    Ok(Json(detail(&blueprint)))
}

pub async fn reconcile(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path((workspace_id, blueprint_id, revision)): Path<(Uuid, String, String)>,
    Extension(git): Extension<Arc<GitRuntime>>,
    Json(request): Json<ReconcileTutorialBlueprintRequest>,
) -> ApiResult<TutorialBlueprintReconcileResponse> {
    if request.idempotency_key.len() < 8
        || request.idempotency_key.len() > 128
        || request.idempotency_key.chars().any(char::is_control)
    {
        return Err(bad_request(
            "Tutorial installation idempotency key is invalid",
        ));
    }
    let workspace_id = authorized_workspace(&state, &auth, workspace_id, true).await?;
    let blueprint = selected_blueprint(&blueprint_id, &revision)?;
    let public = detail(&blueprint);
    let mut result = TutorialBlueprintReconcileResponse {
        status: TutorialBlueprintReconcileStatus::Partial,
        blueprint_id: public.blueprint_id.clone(),
        revision: public.revision.clone(),
        workspace_id,
        dataset_release: None,
        agent_releases: Vec::new(),
        supervisor_release: None,
        supervisor_policy: None,
        recommended_prompt: public.recommended_prompt.clone(),
        expected_artifact_types: public.expected_artifact_types.clone(),
        issues: Vec::new(),
    };

    let dataset = match publish_dataset(
        &state,
        &auth,
        &git,
        workspace_id,
        &blueprint,
        &request.idempotency_key,
    )
    .await
    {
        Ok(release) => release,
        Err(error) => {
            result.issues.push(issue("dataset_release_failed", &error));
            return Ok(Json(result));
        }
    };
    result.dataset_release = Some(dataset.clone());

    let suffix = workspace_id.simple().to_string()[..12].to_string();
    let blueprint_scope = &hex::encode(Sha256::digest(blueprint.blueprint_id.as_bytes()))[..8];
    let identities = blueprint
        .agent_templates
        .iter()
        .map(|template| {
            (
                format!("{}@{}", template.definition_id, template.version),
                format!(
                    "tutorial-{blueprint_scope}-{}-{suffix}",
                    template.definition_id
                ),
            )
        })
        .collect::<BTreeMap<_, _>>();

    for template in &blueprint.agent_templates {
        let template_detail = match agent_catalog::get_published(
            &state.db,
            auth.organization_id,
            &template.definition_id,
            &template.version,
        )
        .await
        {
            Ok(detail) if detail.content_sha256 == template.content_sha256 => detail,
            Err(_) => {
                result.issues.push(TutorialBlueprintIssue {
                    code: "agent_template_unavailable".to_string(),
                    message: "A reviewed tutorial Agent template is unavailable.".to_string(),
                });
                return Ok(Json(result));
            }
            Ok(_) => {
                result.issues.push(TutorialBlueprintIssue {
                    code: "agent_template_changed".to_string(),
                    message: "A reviewed tutorial Agent template no longer matches the Blueprint."
                        .to_string(),
                });
                return Ok(Json(result));
            }
        };
        let definition_id = identities
            .get(&format!("{}@{}", template.definition_id, template.version))
            .expect("blueprint identity map is complete")
            .clone();
        let draft = AgentDefinitionDraftRequest {
            definition_id,
            version: blueprint.revision.clone(),
            display_name: format!("{} Tutorial", template_detail.display_name),
            description: template_detail.description,
            responsibilities: template_detail.responsibilities,
            developer_instructions: template_detail.developer_instructions,
            input_artifact_types: template_detail.input_artifact_types,
            output_artifact_types: template_detail.output_artifact_types,
            capability_template: AgentCapabilityTemplateSelection {
                source: AgentCapabilityTemplateSource::RepositoryAgent,
                definition_id: template.definition_id.clone(),
                version: template.version.clone(),
                release_id: None,
            },
            dataset_release_ids: if template.uses_blueprint_dataset {
                vec![dataset.id]
            } else {
                Vec::new()
            },
        };
        match agent_definition_resources::publish_or_reuse_trusted(&state, &auth, draft).await {
            Ok(release) => result.agent_releases.push(release),
            Err(error) => {
                result.issues.push(issue("agent_release_failed", &error));
                return Ok(Json(result));
            }
        }
    }

    let template_selection = SupervisorPolicySelection {
        policy_id: blueprint.supervisor_template.policy_id.clone(),
        version: blueprint.supervisor_template.version.clone(),
    };
    let supervisor_template = match supervisor_policy::resolve_builtin(&template_selection) {
        Ok(template) => template,
        Err(_) => {
            result.issues.push(TutorialBlueprintIssue {
                code: "supervisor_template_unavailable".to_string(),
                message: "The reviewed tutorial Supervisor template is unavailable.".to_string(),
            });
            return Ok(Json(result));
        }
    };
    if supervisor_template.snapshot.content_sha256 != blueprint.supervisor_template.content_sha256
        || supervisor_template.detail.instruction_policy.policy_id
            != blueprint.instruction_policy_template.policy_id
        || supervisor_template.detail.instruction_policy.version
            != blueprint.instruction_policy_template.version
        || supervisor_template.detail.instruction_policy.content_sha256
            != blueprint.instruction_policy_template.content_sha256
    {
        result.issues.push(TutorialBlueprintIssue {
            code: "supervisor_template_changed".to_string(),
            message: "The reviewed tutorial Supervisor template no longer matches the Blueprint."
                .to_string(),
        });
        return Ok(Json(result));
    }
    let release_by_definition = result
        .agent_releases
        .iter()
        .map(|release| (release.definition_id.clone(), release))
        .collect::<BTreeMap<_, _>>();
    let agents = blueprint
        .agent_templates
        .iter()
        .map(|template| {
            let custom_id = identities
                .get(&format!("{}@{}", template.definition_id, template.version))
                .expect("blueprint identity map is complete");
            let release = release_by_definition
                .get(custom_id)
                .expect("every tutorial Agent was published");
            SupervisorAgentSelection {
                definition_id: custom_id.clone(),
                version: blueprint.revision.clone(),
                release_id: Some(release.id),
                spawn_limit: 1,
            }
        })
        .collect::<Vec<_>>();
    let remap = |identity: &str| -> Result<String, ApiError> {
        if identity == "supervisor" {
            return Ok(identity.to_string());
        }
        identities
            .get(identity)
            .map(|definition_id| format!("{definition_id}@{}", blueprint.revision))
            .ok_or_else(|| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(PlatformError::internal(
                        "Tutorial Supervisor Artifact contract is invalid",
                    )),
                )
            })
    };
    let artifact_contracts = supervisor_template
        .detail
        .artifact_contracts
        .iter()
        .map(|contract| {
            Ok(SupervisorArtifactContractInput {
                artifact_type: contract.artifact_type.clone(),
                producer_agent: remap(&contract.producer_agent)?,
                consumer_agents: contract
                    .consumer_agents
                    .iter()
                    .map(|consumer| remap(consumer))
                    .collect::<Result<Vec<_>, _>>()?,
                required: contract.required,
            })
        })
        .collect::<Result<Vec<_>, ApiError>>()?;
    let mut custom_instructions = supervisor_template.detail.custom_instructions.clone();
    for (template_identity, custom_id) in &identities {
        custom_instructions = custom_instructions.replace(
            template_identity,
            &format!("{custom_id}@{}", blueprint.revision),
        );
    }
    let supervisor_id = format!("tutorial-{}-supervisor-{suffix}", blueprint.blueprint_id);
    let supervisor_draft = SupervisorDraftRequest {
        policy_id: supervisor_id,
        version: blueprint.revision.clone(),
        display_name: format!("{} Supervisor", blueprint.display_name),
        description: supervisor_template.detail.description,
        responsibilities: supervisor_template.detail.responsibilities,
        instruction_policy:
            open_web_codex_platform_contracts::SupervisorInstructionPolicySelection {
                policy_id: supervisor_template.detail.instruction_policy.policy_id,
                version: supervisor_template.detail.instruction_policy.version,
            },
        custom_instructions,
        agents,
        artifact_contracts,
        max_active_child_agents: supervisor_template.detail.max_active_child_agents,
    };
    match supervisor_definitions::publish_or_reuse_trusted(&state, &auth, supervisor_draft).await {
        Ok(release) => {
            result.supervisor_policy = Some(SupervisorPolicySelection {
                policy_id: release.policy_id.clone(),
                version: release.version.clone(),
            });
            result.supervisor_release = Some(release);
            result.status = TutorialBlueprintReconcileStatus::Installed;
        }
        Err(error) => result
            .issues
            .push(issue("supervisor_release_failed", &error)),
    }
    Ok(Json(result))
}

async fn publish_dataset(
    state: &AppState,
    auth: &AuthenticatedUser,
    git: &GitRuntime,
    workspace_id: Uuid,
    blueprint: &LoadedBlueprint,
    request_key: &str,
) -> Result<WorkspaceDatasetReleaseSummary, ApiError> {
    let mut digest = Sha256::new();
    digest.update(auth.organization_id.as_bytes());
    digest.update(workspace_id.as_bytes());
    digest.update(blueprint.blueprint_id.as_bytes());
    digest.update(blueprint.revision.as_bytes());
    digest.update(request_key.as_bytes());
    let idempotency_key = format!("tutorial-{}", &hex::encode(digest.finalize())[..32]);
    let descriptors = blueprint
        .dataset
        .files
        .iter()
        .enumerate()
        .map(|(index, file)| WorkspaceDatasetUploadFile {
            field_id: format!("file-{}", index + 1),
            logical_name: file.logical_name.clone(),
            role: file.role.clone(),
            media_type: file.media_type.clone(),
        })
        .collect::<Vec<_>>();
    let files = descriptors
        .iter()
        .map(|descriptor| {
            blueprint
                .dataset_bytes(&descriptor.logical_name)
                .map(|bytes| (descriptor.clone(), bytes))
                .ok_or_else(|| {
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(PlatformError::internal(
                            "Tutorial Dataset bundle is incomplete",
                        )),
                    )
                })
        })
        .collect::<Result<Vec<_>, _>>()?;
    workspace_datasets::publish_trusted_bundle(
        state,
        auth,
        workspace_id,
        git,
        PublishWorkspaceDatasetRequest {
            idempotency_key,
            dataset_id: blueprint.dataset.dataset_id.clone(),
            version: blueprint.dataset.version.clone(),
            display_name: blueprint.dataset.display_name.clone(),
            description: blueprint.dataset.description.clone(),
            files: descriptors,
        },
        files,
    )
    .await
}

fn selected_blueprint(blueprint_id: &str, revision: &str) -> Result<LoadedBlueprint, ApiError> {
    load_blueprints()?
        .into_iter()
        .find(|blueprint| blueprint.blueprint_id == blueprint_id && blueprint.revision == revision)
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(PlatformError::not_found("Tutorial Blueprint was not found")),
            )
        })
}

fn load_blueprints() -> Result<Vec<LoadedBlueprint>, ApiError> {
    let mut identities = BTreeSet::new();
    let mut loaded = Vec::with_capacity(PUBLISHED_BLUEPRINT_RESOURCES.len());
    for resource in &PUBLISHED_BLUEPRINT_RESOURCES {
        let blueprint = load_blueprint_resource(resource)?;
        if !identities.insert((blueprint.blueprint_id.clone(), blueprint.revision.clone())) {
            return Err(invalid_blueprint());
        }
        loaded.push(blueprint);
    }
    Ok(loaded)
}

fn load_blueprint_resource(
    resource: &'static PublishedBlueprintResource,
) -> Result<LoadedBlueprint, ApiError> {
    let blueprint = serde_json::from_str::<PublishedBlueprint>(resource.manifest)
        .map_err(|_| invalid_blueprint())?;
    if blueprint.schema_version != "tutorial-blueprint.v2"
        || blueprint.recommended_prompt_file != "recommended-prompt.md"
        || resource.recommended_prompt.trim().is_empty()
    {
        return Err(invalid_blueprint());
    }
    validate_dataset_bundle(&blueprint, resource.dataset_files)?;
    validate_blueprint_contracts(&blueprint)?;
    Ok(LoadedBlueprint {
        definition: blueprint,
        resource,
    })
}

fn validate_dataset_bundle(
    blueprint: &PublishedBlueprint,
    bundle: &[DatasetBundleFile<'_>],
) -> Result<(), ApiError> {
    if blueprint.dataset.files.is_empty() || blueprint.dataset.files.len() != bundle.len() {
        return Err(invalid_blueprint());
    }

    let mut embedded = BTreeMap::new();
    for file in bundle {
        if embedded.insert(file.logical_name, file.bytes).is_some() {
            return Err(invalid_blueprint());
        }
    }

    let mut declared = BTreeMap::new();
    for file in &blueprint.dataset.files {
        let safe_name = !file.logical_name.is_empty()
            && file.logical_name.len() <= 128
            && file.logical_name != "."
            && file.logical_name != ".."
            && !file.logical_name.contains('/')
            && !file.logical_name.contains('\\')
            && !file.logical_name.chars().any(char::is_control);
        let valid_digest = file.sha256.len() == 64
            && file
                .sha256
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte));
        if !safe_name
            || file.role.is_empty()
            || file.media_type.is_empty()
            || !valid_digest
            || declared.insert(file.logical_name.as_str(), file).is_some()
        {
            return Err(invalid_blueprint());
        }
        let bytes = embedded
            .get(file.logical_name.as_str())
            .ok_or_else(invalid_blueprint)?;
        if bytes.len() as u64 != file.bytes || hex::encode(Sha256::digest(bytes)) != file.sha256 {
            return Err(invalid_blueprint());
        }
    }
    if embedded.keys().copied().collect::<BTreeSet<_>>()
        != declared.keys().copied().collect::<BTreeSet<_>>()
    {
        return Err(invalid_blueprint());
    }

    let dataset_manifest = embedded
        .get("dataset-manifest.json")
        .ok_or_else(invalid_blueprint)?;
    let source = serde_json::from_slice::<SourceDatasetManifest>(dataset_manifest)
        .map_err(|_| invalid_blueprint())?;
    if source.schema_version != "workspace_dataset_release.v1"
        || source.dataset_id != blueprint.dataset.dataset_id
        || source.version != blueprint.dataset.version
        || source.content_sha256 != blueprint.dataset.source_content_sha256
    {
        return Err(invalid_blueprint());
    }

    let source_files = source
        .files
        .iter()
        .map(|file| (file.path.as_str(), file))
        .collect::<BTreeMap<_, _>>();
    if source_files.len() + 1 != declared.len()
        || source_files
            .iter()
            .any(|(name, source_file)| match declared.get(name) {
                Some(declared_file) => {
                    declared_file.role != source_file.role
                        || declared_file.bytes != source_file.bytes
                        || declared_file.sha256 != source_file.sha256
                }
                None => true,
            })
        || declared
            .keys()
            .any(|name| *name != "dataset-manifest.json" && !source_files.contains_key(name))
    {
        return Err(invalid_blueprint());
    }
    Ok(())
}

fn validate_blueprint_contracts(blueprint: &PublishedBlueprint) -> Result<(), ApiError> {
    for template in &blueprint.agent_templates {
        let resolved = open_web_codex_supervisor_catalog::agent::resolve_builtin(
            &template.definition_id,
            &template.version,
        )
        .map_err(|_| invalid_blueprint())?;
        if resolved.content_sha256 != template.content_sha256 {
            return Err(invalid_blueprint());
        }
    }
    let supervisor = supervisor_policy::resolve_builtin(&SupervisorPolicySelection {
        policy_id: blueprint.supervisor_template.policy_id.clone(),
        version: blueprint.supervisor_template.version.clone(),
    })
    .map_err(|_| invalid_blueprint())?;
    let supervisor_is_current = open_web_codex_supervisor_catalog::supervisor::list_published()
        .map_err(|_| invalid_blueprint())?
        .iter()
        .any(|summary| {
            summary.policy_id == blueprint.supervisor_template.policy_id
                && summary.version == blueprint.supervisor_template.version
        });
    if !supervisor_is_current
        || supervisor.snapshot.content_sha256 != blueprint.supervisor_template.content_sha256
        || supervisor.detail.instruction_policy.policy_id
            != blueprint.instruction_policy_template.policy_id
        || supervisor.detail.instruction_policy.version
            != blueprint.instruction_policy_template.version
        || supervisor.detail.instruction_policy.content_sha256
            != blueprint.instruction_policy_template.content_sha256
    {
        return Err(invalid_blueprint());
    }
    Ok(())
}

fn summary(blueprint: &PublishedBlueprint) -> TutorialBlueprintSummary {
    TutorialBlueprintSummary {
        blueprint_id: blueprint.blueprint_id.clone(),
        revision: blueprint.revision.clone(),
        display_name: blueprint.display_name.clone(),
        description: blueprint.description.clone(),
        estimated_minutes: blueprint.estimated_minutes,
    }
}

fn detail(blueprint: &LoadedBlueprint) -> TutorialBlueprint {
    let mut digest = Sha256::new();
    for value in [
        blueprint.resource.manifest.as_bytes(),
        blueprint.resource.recommended_prompt.as_bytes(),
    ] {
        digest.update((value.len() as u64).to_be_bytes());
        digest.update(value);
    }
    for declared in &blueprint.dataset.files {
        let value = blueprint
            .dataset_bytes(&declared.logical_name)
            .expect("validated Blueprint bundle is complete");
        digest.update((declared.logical_name.len() as u64).to_be_bytes());
        digest.update(declared.logical_name.as_bytes());
        digest.update((value.len() as u64).to_be_bytes());
        digest.update(value);
    }
    TutorialBlueprint {
        blueprint_id: blueprint.blueprint_id.clone(),
        revision: blueprint.revision.clone(),
        display_name: blueprint.display_name.clone(),
        description: blueprint.description.clone(),
        estimated_minutes: blueprint.estimated_minutes,
        dataset: TutorialBlueprintDataset {
            dataset_id: blueprint.dataset.dataset_id.clone(),
            version: blueprint.dataset.version.clone(),
            display_name: blueprint.dataset.display_name.clone(),
            description: blueprint.dataset.description.clone(),
            file_count: blueprint.dataset.files.len() as u32,
            source_content_sha256: blueprint.dataset.source_content_sha256.clone(),
        },
        agent_templates: blueprint
            .agent_templates
            .iter()
            .map(|agent| TutorialBlueprintAgentTemplate {
                definition_id: agent.definition_id.clone(),
                version: agent.version.clone(),
                content_sha256: agent.content_sha256.clone(),
            })
            .collect(),
        supervisor_template: TutorialBlueprintSupervisorTemplate {
            policy_id: blueprint.supervisor_template.policy_id.clone(),
            version: blueprint.supervisor_template.version.clone(),
            content_sha256: blueprint.supervisor_template.content_sha256.clone(),
        },
        instruction_policy_template: TutorialBlueprintInstructionPolicyTemplate {
            policy_id: blueprint.instruction_policy_template.policy_id.clone(),
            version: blueprint.instruction_policy_template.version.clone(),
            content_sha256: blueprint.instruction_policy_template.content_sha256.clone(),
        },
        required_mcp_servers: blueprint.required_mcp_servers.clone(),
        expected_artifact_types: blueprint.expected_artifact_types.clone(),
        recommended_prompt: blueprint.resource.recommended_prompt.trim().to_string(),
        content_sha256: hex::encode(digest.finalize()),
    }
}

fn issue(code: &str, error: &ApiError) -> TutorialBlueprintIssue {
    let code = if error.0 == StatusCode::CONFLICT {
        code.strip_suffix("_failed")
            .map(|prefix| format!("{prefix}_conflict"))
            .unwrap_or_else(|| code.to_string())
    } else {
        code.to_string()
    };
    TutorialBlueprintIssue {
        code,
        message: error.1 .0.message.clone(),
    }
}

fn invalid_blueprint() -> ApiError {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(PlatformError::internal(
            "Checked-in Tutorial Blueprint is invalid",
        )),
    )
}

fn bad_request(message: &str) -> ApiError {
    (
        StatusCode::BAD_REQUEST,
        Json(PlatformError::bad_request(message)),
    )
}

#[cfg(test)]
mod tests {
    use axum::{http::StatusCode, Json};
    use open_web_codex_platform_contracts::error::{ErrorKind, PlatformError};

    use super::{
        detail, issue, load_blueprints, validate_dataset_bundle, DatasetBundleFile,
        INDONESIA_DATASET_FILES,
    };

    #[test]
    fn checked_in_blueprint_is_complete_and_path_free() {
        let blueprint = load_blueprints()
            .expect("blueprints")
            .into_iter()
            .next()
            .expect("blueprint");
        let public = detail(&blueprint);
        assert_eq!(public.dataset.file_count, 10);
        assert_eq!(public.agent_templates.len(), 3);
        assert_eq!(public.expected_artifact_types.len(), 10);
        let encoded = serde_json::to_string(&public).expect("serialize");
        assert!(!encoded.contains("/Users/"));
        assert!(!encoded.contains("/home/"));
        assert!(!encoded.contains("resource://"));
    }

    #[test]
    fn blueprint_rejects_dataset_byte_or_declared_digest_drift() {
        let mut blueprints = load_blueprints().expect("blueprints");
        let blueprint = blueprints.pop().expect("blueprint");
        let mut corrupted_manifest = INDONESIA_DATASET_FILES[0].bytes.to_vec();
        corrupted_manifest[0] ^= 1;
        let corrupted_bundle = INDONESIA_DATASET_FILES
            .iter()
            .enumerate()
            .map(|(index, file)| DatasetBundleFile {
                logical_name: file.logical_name,
                bytes: if index == 0 {
                    corrupted_manifest.as_slice()
                } else {
                    file.bytes
                },
            })
            .collect::<Vec<_>>();
        assert!(validate_dataset_bundle(&blueprint, &corrupted_bundle).is_err());

        let mut changed = blueprint.definition;
        changed.dataset.files[0].sha256 = "0".repeat(64);
        assert!(validate_dataset_bundle(&changed, &INDONESIA_DATASET_FILES).is_err());
    }

    #[test]
    fn reconcile_reports_identity_conflicts_as_a_typed_partial_issue() {
        let error = (
            StatusCode::CONFLICT,
            Json(PlatformError {
                kind: ErrorKind::Conflict,
                message: "A Release with this identity has different content".to_string(),
                request_id: None,
                retry_after_ms: None,
            }),
        );
        let issue = issue("agent_release_failed", &error);
        assert_eq!(issue.code, "agent_release_conflict");
        assert!(issue.message.contains("different content"));
    }
}
