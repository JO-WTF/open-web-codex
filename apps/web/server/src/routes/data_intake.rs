//! Durable, Workspace-scoped data intake.
//!
//! This module deliberately owns transport, authorization, revisions and
//! safe source-asset storage only. Domain profiling/normalization can enrich
//! the persisted candidates through the same contract; the platform never
//! guesses a Dataset Release from an empty dependency list.

use axum::{
    extract::{Multipart, Path, State},
    http::{HeaderMap, StatusCode},
    Extension, Json,
};
use open_web_codex_adapter::{AuthorizedWorkspace, CodexAdapter, TurnOptions};
use open_web_codex_git_runtime::GitRuntime;
use open_web_codex_platform_contracts::error::{ErrorKind, PlatformError};
use open_web_codex_platform_contracts::{
    AnalysisStartRequest, AnalysisStartResponse, DataGapKind, DataIntakeGap,
    DataIntakeInputRequest, DataIntakeParameterAnswer, DataIntakeResponseRequest,
    DataIntakeSessionSummary, DataIntakeStatus, DataMappingCandidate, DataRequirementContract,
    DataRequirementParameter, PublishWorkspaceDatasetRequest, SourceAssetSummary,
    WorkspaceDataDraftSummary, WorkspaceDatasetUploadFile,
};
use open_web_codex_platform_store::AppState;
use serde::de::DeserializeOwned;
use serde_json::Value;
use sha2::{Digest, Sha256};
use sqlx::Row;
use std::collections::HashSet;
use std::sync::Arc;
use uuid::Uuid;

use crate::middleware::auth::{require_runtime_profile, AuthenticatedUser};
use crate::routes::RuntimeProfileBinding;

use super::workspaces::authorized_workspace;

type ApiError = (StatusCode, Json<PlatformError>);
type ApiResult<T> = Result<Json<T>, ApiError>;

const MAX_FILES: usize = 20;
const MAX_FILE_BYTES: usize = 100 * 1024 * 1024;
const MAX_TOTAL_FILE_BYTES: usize = 250 * 1024 * 1024;
const MAX_JSON_DEPTH: usize = 64;
const MAX_JSON_NODES: usize = 250_000;
const MAX_XLSX_ENTRIES: u32 = 2_048;
const MAX_XLSX_EXPANDED_BYTES: u64 = 512 * 1024 * 1024;

#[derive(Debug)]
struct UploadedAsset {
    asset_id: Uuid,
    file_name: String,
    media_type: String,
    bytes: Vec<u8>,
    content_sha256: String,
}

pub async fn create_draft(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(workspace_id): Path<Uuid>,
    headers: HeaderMap,
    Extension(git): Extension<std::sync::Arc<GitRuntime>>,
    mut multipart: Multipart,
) -> ApiResult<WorkspaceDataDraftSummary> {
    let workspace_id = authorized_workspace(&state, &auth, workspace_id, true).await?;
    if headers.contains_key("draft-id") {
        return Err(bad_request(
            "Draft-Id is no longer accepted; uploads belong to the selected Workspace",
        ));
    }
    let idempotency_key = headers
        .get("idempotency-key")
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| (8..=128).contains(&value.len()))
        .ok_or_else(|| bad_request("Idempotency-Key must contain 8-128 characters"))?
        .to_string();

    let mut assets = Vec::new();
    let mut total_bytes = 0usize;
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|_| bad_request("The upload could not be read"))?
    {
        if assets.len() >= MAX_FILES {
            return Err(bad_request("A data draft may contain at most 20 files"));
        }
        let file_name = field
            .file_name()
            .map(str::to_string)
            .ok_or_else(|| bad_request("Every data draft part must have a file name"))?;
        validate_file_name(&file_name)?;
        let bytes = field
            .bytes()
            .await
            .map_err(|_| bad_request("The upload could not be read"))?;
        if bytes.is_empty() || bytes.len() > MAX_FILE_BYTES {
            return Err(bad_request(format!(
                "{file_name} exceeds the 100 MiB per-file limit"
            )));
        }
        total_bytes = total_bytes
            .checked_add(bytes.len())
            .ok_or_else(|| bad_request("The data draft is too large"))?;
        if total_bytes > MAX_TOTAL_FILE_BYTES {
            return Err(bad_request("A data draft may contain at most 250 MiB"));
        }
        let media_type = media_type_for(&file_name)?;
        validate_content(&file_name, &bytes)?;
        let content_sha256 = hex::encode(Sha256::digest(&bytes));
        assets.push(UploadedAsset {
            asset_id: Uuid::now_v7(),
            file_name,
            media_type,
            bytes: bytes.to_vec(),
            content_sha256,
        });
    }
    if assets.is_empty() {
        return Err(bad_request(
            "A data draft must contain at least one .xlsx, .csv or .json file",
        ));
    }
    let mut batch_keys = HashSet::new();
    for asset in &assets {
        if !batch_keys.insert((asset.file_name.clone(), asset.content_sha256.clone())) {
            return Err(conflict(format!(
                "The data draft contains duplicate file content for {}",
                asset.file_name
            )));
        }
    }

    if let Some(existing) = sqlx::query(
        "SELECT id FROM workspace_data_drafts \
         WHERE organization_id = $1 AND workspace_id = $2 AND idempotency_key = $3",
    )
    .bind(auth.organization_id)
    .bind(workspace_id)
    .bind(&idempotency_key)
    .fetch_optional(&state.db)
    .await
    .map_err(database_error)?
    {
        let draft_id: Uuid = existing.get("id");
        let stored = load_assets(&state, &auth, workspace_id, draft_id).await?;
        if !same_assets(&stored, &assets) {
            return Err(conflict(
                "The idempotency key is already bound to different data draft content",
            ));
        }
        return Ok(Json(
            load_draft(&state, &auth, workspace_id, draft_id).await?,
        ));
    }

    let draft_id = Uuid::now_v7();
    let mut relative_paths = Vec::with_capacity(assets.len());
    let mut storage_paths = Vec::with_capacity(assets.len());
    let mut existing_asset_ids = HashSet::new();
    for asset in &assets {
        if let Some(existing) = sqlx::query(
            "SELECT id, relative_path FROM workspace_data_source_assets
             WHERE organization_id = $1 AND workspace_id = $2
               AND content_sha256 = $3 AND normalized_file_name = $4",
        )
        .bind(auth.organization_id)
        .bind(workspace_id)
        .bind(&asset.content_sha256)
        .bind(normalize_file_name(&asset.file_name))
        .fetch_optional(&state.db)
        .await
        .map_err(database_error)?
        {
            let existing_id: Uuid = existing.get("id");
            let existing_path: String = existing.get("relative_path");
            existing_asset_ids.insert(existing_id);
            storage_paths.push((asset.asset_id, existing_id, existing_path));
            continue;
        }
        let relative_path = match git
            .write_source_asset(workspace_id, asset.asset_id, &asset.file_name, &asset.bytes)
            .await
        {
            Ok(path) => path,
            Err(error) => {
                cleanup_source_assets(git.as_ref(), workspace_id, &assets, &relative_paths).await;
                return Err(conflict(format!("SourceAsset write failed: {error}")));
            }
        };
        relative_paths.push((asset.asset_id, relative_path.clone()));
        storage_paths.push((asset.asset_id, asset.asset_id, relative_path));
    }
    let mut transaction = match state.db.begin().await {
        Ok(transaction) => transaction,
        Err(error) => {
            cleanup_source_assets(git.as_ref(), workspace_id, &assets, &relative_paths).await;
            return Err(database_error(error));
        }
    };
    // Serialize the idempotency decision with the Workspace revision update.
    // The preflight lookup above is only a fast path; without this locked
    // lookup two concurrent uploads using the same key could both pass the
    // preflight and turn a legitimate retry into a database error.
    if let Some(existing) = sqlx::query(
        "SELECT id FROM workspace_data_drafts
         WHERE organization_id = $1 AND workspace_id = $2 AND idempotency_key = $3
         FOR UPDATE",
    )
    .bind(auth.organization_id)
    .bind(workspace_id)
    .bind(&idempotency_key)
    .fetch_optional(&mut *transaction)
    .await
    .map_err(database_error)?
    {
        let draft_id: Uuid = existing.get("id");
        transaction.rollback().await.map_err(database_error)?;
        cleanup_source_assets(git.as_ref(), workspace_id, &assets, &relative_paths).await;
        let stored = load_assets(&state, &auth, workspace_id, draft_id).await?;
        if !same_assets(&stored, &assets) {
            return Err(conflict(
                "idempotency_conflict: the upload key is already bound to different data draft content",
            ));
        }
        return Ok(Json(
            load_draft(&state, &auth, workspace_id, draft_id).await?,
        ));
    }
    let database_result: Result<(), ApiError> = async {
        sqlx::query(
        "UPDATE workspaces SET source_revision = source_revision + 1, updated_at = now() \
         WHERE organization_id = $1 AND id = $2",
    )
    .bind(auth.organization_id)
    .bind(workspace_id)
    .execute(&mut *transaction)
    .await
        .map_err(database_error)?;
        sqlx::query(
        "INSERT INTO workspace_data_drafts \
         (id, organization_id, workspace_id, owner_user_id, revision, state, idempotency_key) \
         VALUES ($1, $2, $3, $4, (
             SELECT source_revision FROM workspaces WHERE organization_id = $2 AND id = $3
         ), 'active', $5)",
        )
    .bind(draft_id)
    .bind(auth.organization_id)
    .bind(workspace_id)
    .bind(auth.user_id)
    .bind(&idempotency_key)
    .execute(&mut *transaction)
        .await
            .map_err(database_error)?;
        for asset in &assets {
            let stored_asset_id = storage_paths
                .iter()
                .find(|(uploaded_id, _, _)| *uploaded_id == asset.asset_id)
                .map(|(_, stored_id, _)| *stored_id)
                .ok_or_else(|| internal_error("source asset path was not recorded"))?;
            if !existing_asset_ids.contains(&stored_asset_id) {
                let relative_path = storage_paths
                    .iter()
                    .find(|(uploaded_id, _, _)| *uploaded_id == asset.asset_id)
                    .map(|(_, _, path)| path)
                    .ok_or_else(|| internal_error("source asset path was not recorded"))?;
                let inserted = sqlx::query(
                "INSERT INTO workspace_data_source_assets \
                 (id, organization_id, workspace_id, owner_user_id, file_name, normalized_file_name, media_type, byte_size, content_sha256, relative_path) \
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
                 ON CONFLICT (organization_id, workspace_id, content_sha256, normalized_file_name)
                 DO NOTHING RETURNING id",
                )
            .bind(stored_asset_id)
            .bind(auth.organization_id)
            .bind(workspace_id)
            .bind(auth.user_id)
            .bind(&asset.file_name)
            .bind(normalize_file_name(&asset.file_name))
            .bind(&asset.media_type)
            .bind(asset.bytes.len() as i64)
            .bind(&asset.content_sha256)
            .bind(relative_path)
            .fetch_optional(&mut *transaction)
            .await
                .map_err(database_error)?;
                if inserted.is_none() {
                    let existing = sqlx::query(
                        "SELECT id, relative_path FROM workspace_data_source_assets
                         WHERE organization_id = $1 AND workspace_id = $2
                           AND content_sha256 = $3 AND normalized_file_name = $4",
                    )
                    .bind(auth.organization_id)
                    .bind(workspace_id)
                    .bind(&asset.content_sha256)
                    .bind(normalize_file_name(&asset.file_name))
                    .fetch_one(&mut *transaction)
                    .await
                    .map_err(database_error)?;
                    let existing_id: Uuid = existing.get("id");
                    let existing_path: String = existing.get("relative_path");
                    existing_asset_ids.insert(existing_id);
                    if let Some(position) = relative_paths
                        .iter()
                        .position(|(uploaded_id, _)| *uploaded_id == asset.asset_id)
                    {
                        relative_paths.remove(position);
                        let _ = git
                            .remove_source_asset(workspace_id, asset.asset_id, &asset.file_name)
                            .await;
                    }
                    if let Some(path) = storage_paths
                        .iter_mut()
                        .find(|(uploaded_id, _, _)| *uploaded_id == asset.asset_id)
                    {
                        *path = (asset.asset_id, existing_id, existing_path);
                    }
                }
            }
            sqlx::query(
            "INSERT INTO workspace_data_draft_assets (organization_id, draft_id, asset_id) \
             VALUES ($1, $2, $3)",
            )
        .bind(auth.organization_id)
        .bind(draft_id)
        .bind(stored_asset_id)
        .execute(&mut *transaction)
        .await
            .map_err(database_error)?;
        }
        Ok(())
    }
    .await;
    if let Err(error) = database_result {
        let _ = transaction.rollback().await;
        cleanup_source_assets(git.as_ref(), workspace_id, &assets, &relative_paths).await;
        return Err(error);
    }
    if let Err(error) = transaction.commit().await {
        cleanup_source_assets(git.as_ref(), workspace_id, &assets, &relative_paths).await;
        return Err(database_error(error));
    }

    Ok(Json(
        load_draft(&state, &auth, workspace_id, draft_id).await?,
    ))
}

/// GET /api/workspaces/:workspace_id/source-assets
/// Source assets are Workspace-scoped projections, not Draft-scoped files.
pub async fn list_source_assets(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(workspace_id): Path<Uuid>,
) -> ApiResult<Vec<SourceAssetSummary>> {
    let workspace_id = authorized_workspace(&state, &auth, workspace_id, false).await?;
    let rows = sqlx::query(
        "SELECT id, file_name, media_type, byte_size, content_sha256 \
         FROM workspace_data_source_assets \
         WHERE organization_id = $1 AND workspace_id = $2 \
         ORDER BY created_at, id",
    )
    .bind(auth.organization_id)
    .bind(workspace_id)
    .fetch_all(&state.db)
    .await
    .map_err(database_error)?;
    Ok(Json(
        rows.into_iter()
            .map(|row| SourceAssetSummary {
                asset_id: row.get("id"),
                file_name: row.get("file_name"),
                media_type: row.get("media_type"),
                byte_size: row.get("byte_size"),
                content_sha256: row.get("content_sha256"),
            })
            .collect(),
    ))
}

/// Ensure a Task-scoped evidence projection exists when a Network Supervisor
/// Thread receives its first message. This creates no Agent/Turn and performs
/// no domain inspection; it only gives the artifact projector an authorized
/// owner for the next Network/Data handoff.
pub(crate) async fn ensure_session_for_thread(
    state: &AppState,
    auth: &AuthenticatedUser,
    task_id: Uuid,
    workspace_id: Uuid,
    thread_id: &str,
) -> Result<(), ApiError> {
    let bound_policy = sqlx::query(
        "SELECT snapshot.policy_id, snapshot.version
         FROM supervisor_policy_bindings binding
         JOIN supervisor_policy_snapshots snapshot
           ON snapshot.organization_id = binding.organization_id
          AND snapshot.id = binding.snapshot_id
         WHERE binding.organization_id = $1 AND binding.task_id = $2
           AND binding.thread_id = $3 AND binding.state = 'bound'
         ORDER BY binding.bound_at DESC NULLS LAST, binding.id DESC
         LIMIT 1",
    )
    .bind(auth.organization_id)
    .bind(task_id)
    .bind(thread_id)
    .fetch_optional(&state.db)
    .await
    .map_err(database_error)?;
    let Some(bound_policy) = bound_policy else {
        return Ok(());
    };
    let policy_id: String = bound_policy.get("policy_id");
    let policy_version: String = bound_policy.get("version");
    let policy = crate::supervisor_policy::resolve(
        &state.db,
        auth.organization_id,
        &open_web_codex_platform_contracts::SupervisorPolicySelection {
            policy_id,
            version: policy_version,
        },
    )
    .await
    .map_err(|_| conflict("The bound Supervisor Policy could not be resolved"))?;
    let supports_intake = policy.detail.artifact_contracts.iter().any(|contract| {
        contract.artifact_type == "data_requirement_profile.v1"
            && contract
                .producer_agent
                .starts_with("enterprise-network-planning-agent@")
    });
    if !supports_intake {
        return Ok(());
    }
    let Some(contract_ref) = policy.detail.data_requirement_contracts.first() else {
        // A Supervisor without a declared capability-owned contract can still
        // host an ordinary conversation, but it must not create an Intake
        // projection that invents domain requirements.
        return Ok(());
    };
    let input_revision: i64 = sqlx::query_scalar(
        "SELECT source_revision FROM workspaces
         WHERE organization_id = $1 AND id = $2",
    )
    .bind(auth.organization_id)
    .bind(workspace_id)
    .fetch_one(&state.db)
    .await
    .map_err(database_error)?;
    sqlx::query(
        "INSERT INTO data_intake_sessions (
            id, organization_id, workspace_id, task_id, thread_id,
            contract_id, contract_version, contract_sha256, status,
            input_revision, gap_fingerprint, gaps, mapping_candidates,
            idempotency_key
         ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, 'active', $9, '', '[]', '[]', $10)
         ON CONFLICT (organization_id, task_id, contract_id, contract_version)
         DO UPDATE SET thread_id = EXCLUDED.thread_id, updated_at = now()",
    )
    .bind(Uuid::now_v7())
    .bind(auth.organization_id)
    .bind(workspace_id)
    .bind(task_id)
    .bind(thread_id)
    .bind(&contract_ref.contract_id)
    .bind(&contract_ref.version)
    .bind(&contract_ref.content_sha256)
    .bind(input_revision)
    .bind(format!("thread:{task_id}"))
    .execute(&state.db)
    .await
    .map_err(database_error)?;
    Ok(())
}

pub async fn get(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(task_id): Path<Uuid>,
) -> ApiResult<DataIntakeSessionSummary> {
    let row = sqlx::query(
        "SELECT id, workspace_id FROM data_intake_sessions \
         WHERE organization_id = $1 AND task_id = $2 ORDER BY updated_at DESC, id DESC LIMIT 1",
    )
    .bind(auth.organization_id)
    .bind(task_id)
    .fetch_optional(&state.db)
    .await
    .map_err(database_error)?
    .ok_or_else(|| not_found("Data intake session was not found"))?;
    let workspace_id: Uuid = row.get("workspace_id");
    authorized_workspace(&state, &auth, workspace_id, false).await?;
    Ok(Json(load_session(&state, &auth, row.get("id")).await?))
}

/// Persist a structured answer projected from an Agent artifact.  The
/// platform advances the evidence revision and ends the current waiting
/// projection; a Supervisor can resume the same Thread with a new Turn.
pub async fn respond(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(task_id): Path<Uuid>,
    Extension(adapter): Extension<Arc<dyn CodexAdapter>>,
    Extension(profile): Extension<RuntimeProfileBinding>,
    Json(request): Json<DataIntakeResponseRequest>,
) -> ApiResult<DataIntakeSessionSummary> {
    if !(8..=128).contains(&request.idempotency_key.trim().len()) {
        return Err(bad_request("idempotencyKey must contain 8-128 characters"));
    }
    let intake = load_session_row(&state, &auth, task_id).await?;
    let workspace_id: Uuid = intake.get("workspace_id");
    authorized_workspace(&state, &auth, workspace_id, true).await?;
    let intake_id: Uuid = intake.get("id");
    let current_revision: i64 = intake.get("input_revision");
    if request.expected_session_revision != current_revision {
        return Err(conflict(
            "The intake changed; refresh the pending request before answering",
        ));
    }
    let incoming_response_hash = fingerprint_json(&request.response)?;
    let request_kind = if let Some(existing) = sqlx::query(
        "SELECT intake_session_id, status, kind, response_idempotency_key, response_state, response \
         FROM data_intake_input_requests \
         WHERE organization_id = $1 AND task_id = $2 AND id = $3",
    )
    .bind(auth.organization_id)
    .bind(task_id)
    .bind(request.request_id)
    .fetch_optional(&state.db)
    .await
    .map_err(database_error)?
    {
        if existing.get::<Uuid, _>("intake_session_id") != intake_id {
            return Err(conflict("The input request does not belong to this intake"));
        }
        if existing.get::<String, _>("status") == "answered" {
            if existing
                .get::<Option<String>, _>("response_idempotency_key")
                .as_deref()
                == Some(request.idempotency_key.as_str())
            {
                if fingerprint_json(&existing.get::<Value, _>("response"))?
                    != incoming_response_hash
                {
                    return Err(conflict(
                        "The idempotency key was already used with different response content",
                    ));
                }
                if request
                    .response
                    .get("kind")
                    .and_then(Value::as_str)
                    != Some("confirm_analysis")
                    && existing.get::<String, _>("response_state") == "failed"
                {
                    sqlx::query(
                        "UPDATE data_intake_input_requests SET response_state = 'pending', response_failure_code = NULL
                         WHERE organization_id = $1 AND task_id = $2 AND id = $3 AND response_state = 'failed'",
                    )
                    .bind(auth.organization_id)
                    .bind(task_id)
                    .bind(request.request_id)
                    .execute(&state.db)
                    .await
                    .map_err(database_error)?;
                    resume_intake_response_turn(
                        &state,
                        &auth,
                        &adapter,
                        &profile,
                        task_id,
                        intake_id,
                        request.request_id,
                        &existing.get::<Value, _>("response"),
                    )
                    .await?;
                }
                return Ok(Json(load_session(&state, &auth, intake_id).await?));
            }
            return Err(conflict("The input request was already answered with another idempotency key"));
        }
        existing.get::<String, _>("kind")
    } else {
        return Err(not_found("The input request was not found"));
    };
    let response_kind = request
        .response
        .get("kind")
        .and_then(Value::as_str)
        .unwrap_or(request_kind.as_str());
    if response_kind != request_kind {
        return Err(bad_request(
            "The response kind does not match the pending input request",
        ));
    }
    let response_hash = incoming_response_hash;
    let mut transaction = state.db.begin().await.map_err(database_error)?;
    let updated = sqlx::query(
        "UPDATE data_intake_input_requests SET status = 'answered', response = $1, \
         response_idempotency_key = $2, answered_at = now() WHERE organization_id = $3 AND task_id = $4 AND id = $5 \
         AND status = 'open'",
    )
    .bind(&request.response)
    .bind(&request.idempotency_key)
    .bind(auth.organization_id)
    .bind(task_id)
    .bind(request.request_id)
    .execute(&mut *transaction)
    .await
    .map_err(database_error)?;
    if updated.rows_affected() != 1 {
        return Err(conflict(
            "The input request was already answered or superseded",
        ));
    }
    let response_value = request.response.get("value").unwrap_or(&request.response);
    if matches!(
        request_kind.as_str(),
        "confirm_profile" | "confirm_analysis"
    ) {
        let confirmed = response_value
            .get("decision")
            .and_then(Value::as_str)
            .map(|value| value == "confirmed")
            .or_else(|| response_value.get("confirmed").and_then(Value::as_bool))
            .unwrap_or(false);
        if !confirmed {
            return Err(bad_request(format!(
                "{request_kind} requires an explicit confirmation decision"
            )));
        }
    }
    if request_kind == "confirm_mapping" {
        let confirmed = response_value
            .get("mappings")
            .or_else(|| response_value.get("confirmed"))
            .cloned()
            .unwrap_or_else(|| response_value.clone());
        let confirmed: Vec<DataMappingCandidate> = serde_json::from_value(confirmed)
            .map_err(|_| bad_request("confirm_mapping requires a structured mappings array"))?;
        let stored_candidates: Vec<DataMappingCandidate> =
            decode_json(intake.get("mapping_candidates"))?;
        if intake
            .get::<Option<Value>, _>("mapping_proposal")
            .as_ref()
            .and_then(|proposal| proposal.get("conflicts"))
            .and_then(Value::as_array)
            .is_some_and(|conflicts| !conflicts.is_empty())
        {
            return Err(bad_request(
                "The mapping revision contains unresolved conflicts; ask the Data Agent for a revised mapping before confirming",
            ));
        }
        let candidates = if stored_candidates.is_empty() {
            mapping_candidates_from_artifact(
                &intake
                    .get::<Option<Value>, _>("mapping_proposal")
                    .unwrap_or(Value::Null),
                intake.get::<Option<Value>, _>("source_profile").as_ref(),
            )
        } else {
            stored_candidates
        };
        if confirmed.is_empty()
            || confirmed.len() != candidates.len()
            || confirmed.iter().any(|item| !candidates.contains(item))
        {
            return Err(bad_request(
                "The whole mapping revision must be confirmed; it must contain every published candidate",
            ));
        }
        sqlx::query(
            "UPDATE data_intake_sessions SET mapping_revision = mapping_revision + 1, confirmed_mapping = $1 WHERE organization_id = $2 AND id = $3",
        )
        .bind(serde_json::to_value(confirmed).map_err(internal_serde_error)?)
        .bind(auth.organization_id)
        .bind(intake_id)
        .execute(&mut *transaction)
        .await
        .map_err(database_error)?;
    } else if request_kind == "answer_parameters" {
        let answers = response_value
            .get("answers")
            .cloned()
            .unwrap_or_else(|| response_value.clone());
        let answers: Vec<DataIntakeParameterAnswer> = serde_json::from_value(answers)
            .map_err(|_| bad_request("answer_parameters requires a structured answers array"))?;
        let parameters = intake
            .get::<Option<Value>, _>("requirement_profile")
            .as_ref()
            .map(profile_parameters)
            .unwrap_or_default();
        let answers = validate_answers(&parameters, &answers)?;
        let missing_required = parameters
            .iter()
            .filter(|parameter| {
                parameter.required && !answers.iter().any(|answer| answer.name == parameter.name)
            })
            .map(|parameter| parameter.display_name.as_str())
            .collect::<Vec<_>>();
        if !missing_required.is_empty() {
            return Err(bad_request(format!(
                "All required parameters must be answered: {}",
                missing_required.join(", ")
            )));
        }
        sqlx::query(
            "UPDATE data_intake_sessions SET parameter_answers = $1 WHERE organization_id = $2 AND id = $3",
        )
        .bind(serde_json::to_value(answers).map_err(internal_serde_error)?)
        .bind(auth.organization_id)
        .bind(intake_id)
        .execute(&mut *transaction)
        .await
        .map_err(database_error)?;
    }
    let ready_after_confirmation = request_kind == "confirm_analysis"
        && intake
            .get::<Option<Uuid>, _>("dataset_artifact_id")
            .is_some()
        && intake
            .get::<Option<Uuid>, _>("readiness_artifact_id")
            .is_some()
        && intake
            .get::<Option<String>, _>("profile_confirmation_sha256")
            .is_some()
        && intake
            .get::<Option<String>, _>("mapping_confirmation_sha256")
            .is_some()
        && intake
            .get::<Option<String>, _>("parameter_confirmation_sha256")
            .is_some();
    let status = if ready_after_confirmation {
        "ready"
    } else {
        "active"
    };
    let update_sql = match request_kind.as_str() {
        "confirm_profile" => "UPDATE data_intake_sessions SET input_revision = input_revision + 1, evidence_revision = evidence_revision + 1, status = $1, profile_confirmation_sha256 = $5, failure_code = NULL, failure_summary = NULL, updated_at = now() WHERE organization_id = $2 AND id = $3 AND input_revision = $4",
        "confirm_mapping" => "UPDATE data_intake_sessions SET input_revision = input_revision + 1, evidence_revision = evidence_revision + 1, status = $1, mapping_confirmation_sha256 = $5, failure_code = NULL, failure_summary = NULL, updated_at = now() WHERE organization_id = $2 AND id = $3 AND input_revision = $4",
        "answer_parameters" => "UPDATE data_intake_sessions SET input_revision = input_revision + 1, evidence_revision = evidence_revision + 1, status = $1, parameter_confirmation_sha256 = $5, failure_code = NULL, failure_summary = NULL, updated_at = now() WHERE organization_id = $2 AND id = $3 AND input_revision = $4",
        "confirm_analysis" => "UPDATE data_intake_sessions SET input_revision = input_revision + 1, evidence_revision = evidence_revision + 1, status = $1, readiness_confirmation_sha256 = $5, gaps = '[]', blocked_reasons = '[]', failure_code = NULL, failure_summary = NULL, updated_at = now() WHERE organization_id = $2 AND id = $3 AND input_revision = $4",
        _ => "UPDATE data_intake_sessions SET input_revision = input_revision + 1, evidence_revision = evidence_revision + 1, status = $1, failure_code = NULL, failure_summary = NULL, updated_at = now() WHERE organization_id = $2 AND id = $3 AND input_revision = $4",
    };
    let mut update = sqlx::query(update_sql)
        .bind(status)
        .bind(auth.organization_id)
        .bind(intake_id)
        .bind(current_revision);
    if request_kind != "provide_data" {
        update = update.bind(response_hash);
    }
    let updated = update
        .execute(&mut *transaction)
        .await
        .map_err(database_error)?;
    if updated.rows_affected() != 1 {
        return Err(conflict(
            "The intake changed while the response was being recorded",
        ));
    }
    transaction.commit().await.map_err(database_error)?;
    sqlx::query(
        "UPDATE task_analysis_execution_snapshots
         SET state = 'revoked', failure_code = 'input_changed', updated_at = now()
         WHERE organization_id = $1 AND task_id = $2
           AND state IN ('created', 'starting', 'started')",
    )
    .bind(auth.organization_id)
    .bind(task_id)
    .execute(&state.db)
    .await
    .map_err(database_error)?;
    recompute_intake_fingerprint(&state, auth.organization_id, intake_id).await?;
    if request_kind != "confirm_analysis" {
        resume_intake_response_turn(
            &state,
            &auth,
            &adapter,
            &profile,
            task_id,
            intake_id,
            request.request_id,
            &request.response,
        )
        .await?;
    }
    Ok(Json(load_session(&state, &auth, intake_id).await?))
}

async fn resume_intake_response_turn(
    state: &AppState,
    auth: &AuthenticatedUser,
    adapter: &Arc<dyn CodexAdapter>,
    profile: &RuntimeProfileBinding,
    task_id: Uuid,
    intake_id: Uuid,
    request_id: Uuid,
    response: &Value,
) -> Result<(), ApiError> {
    require_runtime_profile(&state.db, auth, &profile.runtime_key).await?;
    let claimed = sqlx::query(
        "UPDATE data_intake_input_requests SET response_state = 'starting', response_failure_code = NULL
         WHERE organization_id = $1 AND task_id = $2 AND intake_session_id = $3 AND id = $4
           AND status = 'answered' AND response_state = 'pending'
         RETURNING id",
    )
    .bind(auth.organization_id)
    .bind(task_id)
    .bind(intake_id)
    .bind(request_id)
    .fetch_optional(&state.db)
    .await
    .map_err(database_error)?;
    if claimed.is_none() {
        return Ok(());
    }
    let run = sqlx::query(
        "SELECT run.id, run.codex_thread_id, workspace.id AS workspace_id, workspace.root_path
         FROM runs run
         JOIN workspaces workspace ON workspace.id = run.workspace_id
          AND workspace.organization_id = run.organization_id
          AND workspace.state IN ('ready', 'retained')
         WHERE run.organization_id = $1 AND run.task_id = $2
           AND run.codex_thread_id IS NOT NULL
           AND run.status IN ('running', 'recovery_pending', 'completed')
         ORDER BY CASE WHEN run.status IN ('running', 'recovery_pending') THEN 0 ELSE 1 END,
                  run.created_at DESC LIMIT 1",
    )
    .bind(auth.organization_id)
    .bind(task_id)
    .fetch_optional(&state.db)
    .await
    .map_err(database_error)?
    .ok_or_else(|| {
        conflict("The original Thread Run is unavailable; resume the Thread before answering")
    })?;
    let thread_id: String = run.get("codex_thread_id");
    let workspace = AuthorizedWorkspace {
        id: run.get::<Uuid, _>("workspace_id").to_string(),
        root: run.get::<String, _>("root_path").into(),
    };
    let summary = serde_json::to_string(response)
        .unwrap_or_else(|_| "{}".to_string())
        .chars()
        .take(4_000)
        .collect::<String>();
    let prompt = format!(
        concat!(
            "A user response was recorded for the current data-intake request. ",
            "Continue the same Network Supervisor task and use the updated Workspace evidence. ",
            "input_request_id={} structured_response={}"
        ),
        request_id, summary
    );
    let result = match adapter
        .send_user_message(&workspace, &thread_id, &prompt, &TurnOptions::default())
        .await
    {
        Ok(result) => result,
        Err(error) => {
            tracing::warn!(%task_id, %request_id, error = %error, "Intake response Turn failed to start");
            sqlx::query(
                "UPDATE data_intake_input_requests SET response_state = 'failed', response_failure_code = 'runtime_turn_start_failed' WHERE organization_id = $1 AND id = $2",
            )
            .bind(auth.organization_id)
            .bind(request_id)
            .execute(&state.db)
            .await
            .map_err(database_error)?;
            return Err(runtime_unavailable(
                "Codex Runtime failed to resume the data-intake Thread",
            ));
        }
    };
    let Some(turn_id) = result
        .get("turnId")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
    else {
        sqlx::query(
            "UPDATE data_intake_input_requests
             SET response_state = 'failed', response_failure_code = 'runtime_turn_start_failed'
             WHERE organization_id = $1 AND id = $2 AND response_state = 'starting'",
        )
        .bind(auth.organization_id)
        .bind(request_id)
        .execute(&state.db)
        .await
        .map_err(database_error)?;
        return Err(runtime_unavailable(
            "Codex Runtime resumed intake without a Turn id",
        ));
    };
    sqlx::query(
        "UPDATE data_intake_input_requests SET response_state = 'started', response_turn_id = $1
         WHERE organization_id = $2 AND id = $3",
    )
    .bind(turn_id)
    .bind(auth.organization_id)
    .bind(request_id)
    .execute(&state.db)
    .await
    .map_err(database_error)?;
    sqlx::query(
        "WITH updated_run AS (
             UPDATE runs SET status = 'running', active_turn_id = $1, lease_owner = NULL,
                 lease_token = NULL, lease_expires_at = NULL, updated_at = now()
             WHERE id = $2 AND organization_id = $3 AND status IN ('running', 'completed')
             RETURNING task_id
         )
         UPDATE tasks SET status = 'running', updated_at = now()
         WHERE id IN (SELECT task_id FROM updated_run) AND organization_id = $3
           AND status NOT IN ('cancelled', 'archived', 'failed')",
    )
    .bind(turn_id)
    .bind(run.get::<Uuid, _>("id"))
    .bind(auth.organization_id)
    .execute(&state.db)
    .await
    .map_err(database_error)?;
    Ok(())
}

/// Lock the confirmed planning artifact into an immutable Dataset Release and
/// resume the same Runtime Thread with one analysis Turn. Every identity is
/// idempotent so retries cannot create duplicate Releases, bindings or Turns.
pub async fn analysis_start(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(task_id): Path<Uuid>,
    Extension(adapter): Extension<Arc<dyn CodexAdapter>>,
    Extension(profile): Extension<RuntimeProfileBinding>,
    Extension(git): Extension<Arc<GitRuntime>>,
    Json(request): Json<AnalysisStartRequest>,
) -> ApiResult<AnalysisStartResponse> {
    if request.readiness_fingerprint.len() != 64
        || !request
            .readiness_fingerprint
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
    {
        return Err(bad_request(
            "readinessFingerprint must be a SHA-256 hex digest",
        ));
    }
    if !(8..=128).contains(&request.idempotency_key.trim().len()) {
        return Err(bad_request("idempotencyKey must contain 8-128 characters"));
    }
    let intake = load_session_row(&state, &auth, task_id).await?;
    let workspace_id: Uuid = intake.get("workspace_id");
    authorized_workspace(&state, &auth, workspace_id, true).await?;
    require_runtime_profile(&state.db, &auth, &profile.runtime_key).await?;
    let current_revision: i64 = intake.get("input_revision");
    if current_revision != request.expected_session_revision {
        return Err(conflict("The intake changed; refresh the final checklist"));
    }
    let confirmation = sqlx::query(
        "SELECT kind, status FROM data_intake_input_requests WHERE organization_id = $1 AND task_id = $2 AND id = $3 AND intake_session_id = $4",
    )
    .bind(auth.organization_id)
    .bind(task_id)
    .bind(request.request_id)
    .bind(intake.get::<Uuid, _>("id"))
    .fetch_optional(&state.db)
    .await
    .map_err(database_error)?
    .ok_or_else(|| not_found("The final checklist request was not found"))?;
    if confirmation.get::<String, _>("kind") != "confirm_analysis"
        || confirmation.get::<String, _>("status") != "answered"
    {
        return Err(conflict(
            "The final checklist must be confirmed before analysis starts",
        ));
    }
    let status: String = intake.get("status");
    if status != "ready" {
        return Err(conflict(
            "analysis_not_ready: Analysis requires a confirmed planning dataset, parameters and final checklist.",
        ));
    }
    let evidence_fingerprint: String = intake.get("evidence_fingerprint");
    if evidence_fingerprint.len() == 64
        && !evidence_fingerprint.eq_ignore_ascii_case(&request.readiness_fingerprint)
    {
        return Err(conflict(
            "readiness_fingerprint_stale: refresh the final checklist",
        ));
    }
    // Serialize every analysis start for this Task before any externally
    // visible side effect (Release publication, Binding creation or Runtime
    // Turn delivery).  Without this claim, two concurrent clicks can both
    // publish a Release and race on the snapshot uniqueness constraints,
    // turning an otherwise idempotent retry into a database error.
    let mut claim_transaction = state.db.begin().await.map_err(database_error)?;
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
        .bind(format!("analysis-start:{task_id}"))
        .execute(&mut *claim_transaction)
        .await
        .map_err(database_error)?;
    // A client retry may reuse the same idempotency key, while a second client
    // may retry the same confirmed snapshot with a fresh key.  Reject only a
    // key that is being reused for different readiness; the readiness
    // fingerprint remains the stronger deduplication identity.
    if let Some(existing) = sqlx::query(
        "SELECT readiness_fingerprint, state
         FROM task_analysis_execution_snapshots
         WHERE organization_id = $1 AND task_id = $2 AND idempotency_key = $3",
    )
    .bind(auth.organization_id)
    .bind(task_id)
    .bind(&request.idempotency_key)
    .fetch_optional(&state.db)
    .await
    .map_err(database_error)?
    {
        if existing.get::<String, _>("readiness_fingerprint") != request.readiness_fingerprint {
            return Err(conflict(
                "idempotency_conflict: this idempotency key was already used for another analysis snapshot",
            ));
        }
        if matches!(
            existing.get::<String, _>("state").as_str(),
            "starting" | "started"
        ) {
            if let Some(snapshot) = sqlx::query(
                "SELECT id, binding_id, state
                 FROM task_analysis_execution_snapshots
                 WHERE organization_id = $1 AND task_id = $2
                   AND idempotency_key = $3",
            )
            .bind(auth.organization_id)
            .bind(task_id)
            .bind(&request.idempotency_key)
            .fetch_optional(&state.db)
            .await
            .map_err(database_error)?
            {
                return Ok(Json(AnalysisStartResponse {
                    execution_snapshot_id: snapshot.get("id"),
                    task_dataset_binding_id: snapshot.get("binding_id"),
                    readiness_fingerprint: request.readiness_fingerprint,
                    state: snapshot.get("state"),
                }));
            }
        }
    }
    let dataset_artifact_id: Uuid = intake
        .get::<Option<Uuid>, _>("dataset_artifact_id")
        .ok_or_else(|| {
            conflict(
                "dataset_artifact_required: the Data Agent has not published planning-dataset.v2",
            )
        })?;
    let readiness_artifact_id: Uuid = intake
        .get::<Option<Uuid>, _>("readiness_artifact_id")
        .ok_or_else(|| conflict("readiness_review_required: the Network Agent has not published the final checklist"))?;
    let readiness_artifact = sqlx::query(
        "SELECT artifact.artifact_schema, artifact.state, artifact.content
         FROM artifacts artifact
         JOIN artifact_task_grants artifact_grant
           ON artifact_grant.organization_id = artifact.organization_id
          AND artifact_grant.artifact_id = artifact.id
          AND artifact_grant.task_id = $1
          AND artifact_grant.permission = 'read'
         WHERE artifact.organization_id = $2
           AND artifact.id = $3
           AND artifact.retention_state = 'active'",
    )
    .bind(task_id)
    .bind(auth.organization_id)
    .bind(readiness_artifact_id)
    .fetch_optional(&state.db)
    .await
    .map_err(database_error)?
    .ok_or_else(|| {
        conflict(
            "readiness_review_not_authorized: the final checklist is not available for this Task",
        )
    })?;
    if readiness_artifact.get::<String, _>("artifact_schema") != "analysis_readiness_review.v1"
        || readiness_artifact.get::<String, _>("state") != "ready"
    {
        return Err(conflict(
            "readiness_review_pending: the final checklist is not ready",
        ));
    }
    let readiness_bytes: Vec<u8> = readiness_artifact
        .get::<Option<Vec<u8>>, _>("content")
        .ok_or_else(|| {
            conflict("readiness_review_content_unavailable: retry after Checklist materialization")
        })?;
    if readiness_bytes.len() > 128 * 1024 {
        return Err(conflict(
            "readiness_review_invalid: the final checklist envelope exceeds the platform limit",
        ));
    }
    let readiness_value: Value = serde_json::from_slice(&readiness_bytes)
        .map_err(|_| conflict("readiness_review_invalid: the final checklist is not valid JSON"))?;
    if readiness_value.get("schemaVersion").and_then(Value::as_str)
        != Some("analysis_readiness_review.v1")
        || readiness_value.get("ready").and_then(Value::as_bool) != Some(true)
        || readiness_value
            .get("inputRequest")
            .and_then(|request| request.get("kind"))
            .and_then(Value::as_str)
            != Some("confirm_analysis")
    {
        return Err(conflict(
            "readiness_review_invalid: the final checklist is not an explicit ready review",
        ));
    }
    let artifact = sqlx::query(
        "SELECT artifact.artifact_schema, artifact.state, artifact.content, artifact.content_sha256, \
                artifact.source_server, artifact.source_uri \
         FROM artifacts artifact \
         JOIN artifact_task_grants artifact_grant ON artifact_grant.organization_id = artifact.organization_id \
          AND artifact_grant.artifact_id = artifact.id AND artifact_grant.task_id = $1 AND artifact_grant.permission = 'read' \
         WHERE artifact.organization_id = $2 AND artifact.id = $3 AND artifact.retention_state = 'active'",
    )
    .bind(task_id)
    .bind(auth.organization_id)
    .bind(dataset_artifact_id)
    .fetch_optional(&state.db)
    .await
    .map_err(database_error)?
    .ok_or_else(|| not_found("The planning dataset Artifact is not authorized for this Task"))?;
    if artifact.get::<String, _>("artifact_schema") != "planning-dataset.v2"
        || artifact.get::<String, _>("state") != "ready"
    {
        return Err(conflict(
            "planning_dataset_pending: the normalized dataset is not ready",
        ));
    }
    let planning_dataset_server = artifact
        .get::<Option<String>, _>("source_server")
        .ok_or_else(|| {
            conflict("planning_dataset_resource_unavailable: the Data Agent Resource identity is missing")
        })?;
    let planning_dataset_uri =
        artifact
            .get::<Option<String>, _>("source_uri")
            .ok_or_else(|| {
                conflict(
                    "planning_dataset_resource_unavailable: the Data Agent Resource URI is missing",
                )
            })?;
    if planning_dataset_server != "supply_chain_data"
        || !planning_dataset_uri.starts_with("supply-chain-data://resources/")
        || planning_dataset_uri.ends_with('/')
    {
        return Err(conflict(
            "planning_dataset_resource_unavailable: the normalized dataset is not a trusted Data Agent Resource",
        ));
    }
    let planning_dataset_resource_name = planning_dataset_uri
        .rsplit('/')
        .next()
        .filter(|name| !name.is_empty())
        .ok_or_else(|| {
            conflict(
                "planning_dataset_resource_unavailable: the Data Agent Resource name is missing",
            )
        })?
        .to_string();
    let dataset_bytes: Vec<u8> =
        artifact
            .get::<Option<Vec<u8>>, _>("content")
            .ok_or_else(|| {
                conflict(
                    "planning_dataset_content_unavailable: retry after Artifact materialization",
                )
            })?;
    let dataset_value: Value = serde_json::from_slice(&dataset_bytes).map_err(|_| {
        conflict("planning_dataset_invalid: the planning dataset is not valid JSON")
    })?;
    if dataset_value.get("schema_version").and_then(Value::as_str) != Some("planning-dataset.v2")
        || dataset_value
            .get("normalization_status")
            .and_then(Value::as_str)
            != Some("ready")
    {
        return Err(conflict(
            "planning_dataset_invalid: the Data Agent did not publish a ready planning-dataset.v2",
        ));
    }
    if dataset_value
        .pointer("/data_quality/errors")
        .and_then(Value::as_array)
        .is_some_and(|errors| !errors.is_empty())
    {
        return Err(conflict(
            "planning_dataset_quality_blocked: the normalized dataset contains blocking quality errors",
        ));
    }
    let dataset_sha256 = artifact
        .get::<Option<String>, _>("content_sha256")
        .unwrap_or_else(|| hex::encode(Sha256::digest(&dataset_bytes)));
    let mut selected_source_asset_ids = HashSet::new();
    if let Ok(confirmed) = decode_json::<Vec<DataMappingCandidate>>(intake.get("confirmed_mapping"))
    {
        selected_source_asset_ids.extend(
            confirmed
                .into_iter()
                .filter_map(|candidate| candidate.source_asset_id),
        );
    }
    if selected_source_asset_ids.is_empty() {
        if let Ok(Some(profile)) =
            serde_json::from_value::<Option<Value>>(intake.get("source_profile"))
        {
            if let Some(sources) = profile.get("sources").and_then(Value::as_array) {
                selected_source_asset_ids.extend(sources.iter().filter_map(|source| {
                    source
                        .get("source_asset_id")
                        .and_then(Value::as_str)
                        .and_then(|value| Uuid::parse_str(value).ok())
                }));
            }
        }
    }
    let source_rows = sqlx::query(
        "SELECT id, file_name, media_type, byte_size, content_sha256 FROM workspace_data_source_assets \
         WHERE organization_id = $1 AND workspace_id = $2 ORDER BY created_at, id",
    )
    .bind(auth.organization_id)
    .bind(workspace_id)
    .fetch_all(&state.db)
    .await
    .map_err(database_error)?;
    let source_snapshot = source_rows
        .iter()
        .filter(|row| {
            if selected_source_asset_ids.is_empty() {
                return true;
            }
            selected_source_asset_ids.contains(&row.get::<Uuid, _>("id"))
        })
        .map(|row| {
            serde_json::json!({
                "id": row.get::<Uuid, _>("id"),
                "fileName": row.get::<String, _>("file_name"),
                "mediaType": row.get::<String, _>("media_type"),
                "byteSize": row.get::<i64, _>("byte_size"),
                "sha256": row.get::<String, _>("content_sha256"),
            })
        })
        .collect::<Vec<_>>();
    let source_snapshot_sha256 = fingerprint_json(&source_snapshot)?;
    let requirement_profile_sha256 = artifact_hash(
        &state,
        auth.organization_id,
        intake.get("requirement_artifact_id"),
    )
    .await?;
    let mapping_sha256 = hash_json_requests(&state, &auth, task_id, "confirm_mapping").await?;
    let parameter_sha256 = hash_json_requests(&state, &auth, task_id, "answer_parameters").await?;
    let requirement_profile_sha256 = requirement_profile_sha256.unwrap_or_else(|| fingerprint_json(&serde_json::json!({"contract": intake.get::<String, _>("contract_id"), "version": intake.get::<String, _>("contract_version")})).unwrap_or_default());
    let mapping_sha256 = mapping_sha256
        .unwrap_or_else(|| fingerprint_json(&serde_json::json!([])).unwrap_or_default());
    let parameter_sha256 = parameter_sha256
        .unwrap_or_else(|| fingerprint_json(&serde_json::json!([])).unwrap_or_default());
    let binding_fingerprint = fingerprint_json(&serde_json::json!({
        "contract": [intake.get::<String, _>("contract_id"), intake.get::<String, _>("contract_version")],
        "requirement": requirement_profile_sha256,
        "source": source_snapshot_sha256,
        "mapping": mapping_sha256,
        "parameters": parameter_sha256,
        "dataset": dataset_sha256,
        "readiness": request.readiness_fingerprint,
    }))?;
    let release_request = PublishWorkspaceDatasetRequest {
        idempotency_key: format!("task-analysis:{task_id}:{}", binding_fingerprint),
        dataset_id: format!("planning-task-{}", task_id.simple()),
        version: format!("release-{}", &binding_fingerprint[..16]),
        display_name: "Planning dataset release".to_string(),
        description: "Immutable normalized planning dataset for the confirmed analysis snapshot."
            .to_string(),
        files: vec![WorkspaceDatasetUploadFile {
            field_id: "file-0".to_string(),
            logical_name: "planning-dataset.json".to_string(),
            role: "planning-dataset".to_string(),
            media_type: "application/json".to_string(),
        }],
    };
    let release = super::workspace_datasets::publish_trusted_bytes_bundle(
        &state,
        &auth,
        workspace_id,
        &git,
        release_request,
        vec![(
            WorkspaceDatasetUploadFile {
                field_id: "file-0".to_string(),
                logical_name: "planning-dataset.json".to_string(),
                role: "planning-dataset".to_string(),
                media_type: "application/json".to_string(),
            },
            dataset_bytes,
        )],
    )
    .await?;
    let thread = sqlx::query(
        "SELECT r.id, r.codex_thread_id, r.status, w.root_path FROM runs r JOIN workspaces w ON w.id = r.workspace_id \
         WHERE r.organization_id = $1 AND r.task_id = $2 AND r.workspace_id = $3 AND r.codex_thread_id IS NOT NULL \
           AND r.status IN ('running', 'recovery_pending', 'completed') ORDER BY r.created_at DESC LIMIT 1",
    )
    .bind(auth.organization_id)
    .bind(task_id)
    .bind(workspace_id)
    .fetch_optional(&state.db)
    .await
    .map_err(database_error)?
    .ok_or_else(|| conflict("The original Thread Run is unavailable; resume the Thread before analysis"))?;
    let thread_id: String = thread.get("codex_thread_id");
    let run_id: Uuid = thread.get("id");
    let authorized_workspace = AuthorizedWorkspace {
        id: workspace_id.to_string(),
        root: thread.get::<String, _>("root_path").into(),
    };
    let binding_id = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO task_dataset_bindings (id, organization_id, task_id, workspace_id, intake_session_id, dataset_release_id, requirement_profile_sha256, source_snapshot_sha256, contract_id, contract_version, contract_sha256, mapping_sha256, parameter_sha256, dataset_sha256, snapshot, fingerprint, supersedes_binding_id) \
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16, (SELECT current_binding_id FROM data_intake_sessions WHERE id=$5)) \
         ON CONFLICT (organization_id, task_id, fingerprint) DO NOTHING",
    )
    .bind(binding_id).bind(auth.organization_id).bind(task_id).bind(workspace_id).bind(intake.get::<Uuid, _>("id"))
    .bind(release.id).bind(&requirement_profile_sha256).bind(&source_snapshot_sha256)
    .bind(intake.get::<String, _>("contract_id")).bind(intake.get::<String, _>("contract_version"))
    .bind(intake.get::<String, _>("contract_sha256")).bind(&mapping_sha256).bind(&parameter_sha256).bind(&dataset_sha256)
    .bind(serde_json::json!({"datasetArtifactId": dataset_artifact_id, "readinessArtifactId": readiness_artifact_id, "sourceAssets": source_snapshot}))
    .bind(&binding_fingerprint).execute(&state.db).await.map_err(database_error)?;
    let binding_id: Uuid = sqlx::query_scalar("SELECT id FROM task_dataset_bindings WHERE organization_id=$1 AND task_id=$2 AND fingerprint=$3")
        .bind(auth.organization_id).bind(task_id).bind(&binding_fingerprint).fetch_one(&state.db).await.map_err(database_error)?;
    sqlx::query("UPDATE data_intake_sessions SET normalized_release_id=$1, current_binding_id=$2, updated_at=now() WHERE organization_id=$3 AND id=$4")
        .bind(release.id).bind(binding_id).bind(auth.organization_id).bind(intake.get::<Uuid, _>("id")).execute(&state.db).await.map_err(database_error)?;
    let snapshot_id = if let Some(existing) = sqlx::query(
        "SELECT id, state, turn_id, binding_id FROM task_analysis_execution_snapshots
         WHERE organization_id=$1 AND task_id=$2 AND readiness_fingerprint=$3",
    )
    .bind(auth.organization_id)
    .bind(task_id)
    .bind(&request.readiness_fingerprint)
    .fetch_optional(&state.db)
    .await
    .map_err(database_error)?
    {
        let existing_id: Uuid = existing.get("id");
        let existing_state: String = existing.get("state");
        if matches!(existing_state.as_str(), "started" | "starting") {
            return Ok(Json(AnalysisStartResponse {
                execution_snapshot_id: existing_id,
                task_dataset_binding_id: existing.get("binding_id"),
                readiness_fingerprint: request.readiness_fingerprint,
                state: existing_state,
            }));
        }
        let reset = sqlx::query(
            "UPDATE task_analysis_execution_snapshots
             SET state='created', turn_id=NULL, failure_code=NULL,
                 idempotency_key=$3, binding_id=$4, updated_at=now()
             WHERE organization_id=$1 AND id=$2 AND state = 'failed'",
        )
        .bind(auth.organization_id)
        .bind(existing_id)
        .bind(&request.idempotency_key)
        .bind(binding_id)
        .execute(&mut *claim_transaction)
        .await
        .map_err(database_error)?;
        if reset.rows_affected() == 0 && existing_state != "created" {
            return Err(conflict("The analysis snapshot is no longer retryable"));
        }
        existing_id
    } else {
        let snapshot_id = Uuid::now_v7();
        sqlx::query("INSERT INTO task_analysis_execution_snapshots (id, organization_id, task_id, thread_id, run_id, binding_id, readiness_fingerprint, input_snapshot, idempotency_key, state) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,'created')")
            .bind(snapshot_id).bind(auth.organization_id).bind(task_id).bind(&thread_id).bind(run_id).bind(binding_id).bind(&request.readiness_fingerprint)
            .bind(serde_json::json!({
                "bindingFingerprint": binding_fingerprint,
                "datasetReleaseId": release.id,
                "datasetArtifactId": dataset_artifact_id,
                "planningDatasetRef": planning_dataset_resource_name,
                "planningDatasetUri": planning_dataset_uri,
                "readinessArtifactId": readiness_artifact_id
            }))
            .bind(&request.idempotency_key).execute(&mut *claim_transaction).await.map_err(database_error)?;
        snapshot_id
    };
    let claimed = sqlx::query(
        "UPDATE task_analysis_execution_snapshots
         SET state = 'starting', updated_at = now()
         WHERE organization_id = $1 AND id = $2 AND state = 'created'",
    )
    .bind(auth.organization_id)
    .bind(snapshot_id)
    .execute(&mut *claim_transaction)
    .await
    .map_err(database_error)?;
    if claimed.rows_affected() != 1 {
        let state_value: String = sqlx::query_scalar(
            "SELECT state FROM task_analysis_execution_snapshots WHERE organization_id = $1 AND id = $2",
        )
        .bind(auth.organization_id)
        .bind(snapshot_id)
        .fetch_one(&state.db)
        .await
        .map_err(database_error)?;
        return Ok(Json(AnalysisStartResponse {
            execution_snapshot_id: snapshot_id,
            task_dataset_binding_id: binding_id,
            readiness_fingerprint: request.readiness_fingerprint,
            state: state_value,
        }));
    }
    // Publish the authorization state before the Runtime can execute the new
    // Turn.  MCP tools may be invoked immediately after the Turn is accepted;
    // leaving this row in `starting` would create a deterministic race where
    // the first analysis tool is rejected.
    sqlx::query(
        "UPDATE task_analysis_execution_snapshots
         SET state = 'started', updated_at = now()
         WHERE organization_id = $1 AND id = $2 AND state = 'starting'",
    )
    .bind(auth.organization_id)
    .bind(snapshot_id)
    .execute(&mut *claim_transaction)
    .await
    .map_err(database_error)?;
    claim_transaction.commit().await.map_err(database_error)?;
    let analysis_prompt = format!(
        concat!(
            "The confirmed planning dataset and final readiness checklist are locked. ",
            "Start the requested Network Planning analysis in this Thread using ",
            "the authorized planning-dataset.v2 resource and immutable snapshot. ",
            "planning_dataset_ref={} planning_dataset_uri={} ",
            "planning_dataset_artifact_id={} dataset_release_id={} ",
            "task_dataset_binding_id={} execution_snapshot_id={} binding_fingerprint={} readiness_fingerprint={}"
        ),
        planning_dataset_resource_name,
        planning_dataset_uri,
        dataset_artifact_id,
        release.id,
        binding_id,
        snapshot_id,
        binding_fingerprint,
        request.readiness_fingerprint,
    );
    let result = match adapter
        .send_user_message(
            &authorized_workspace,
            &thread_id,
            &analysis_prompt,
            &TurnOptions::default(),
        )
        .await
    {
        Ok(result) => result,
        Err(_) => {
            let _ = sqlx::query("UPDATE task_analysis_execution_snapshots SET state='failed', failure_code='runtime_turn_start_failed', updated_at=now() WHERE organization_id=$1 AND id=$2")
                .bind(auth.organization_id)
                .bind(snapshot_id)
                .execute(&state.db)
                .await;
            return Err(runtime_unavailable(
                "Codex Runtime failed to start the analysis Turn",
            ));
        }
    };
    let Some(turn_id) = result.get("turnId").and_then(Value::as_str) else {
        let _ = sqlx::query(
            "UPDATE task_analysis_execution_snapshots
             SET state='failed', failure_code='runtime_turn_start_failed', updated_at=now()
             WHERE organization_id=$1 AND id=$2 AND state='started'",
        )
        .bind(auth.organization_id)
        .bind(snapshot_id)
        .execute(&state.db)
        .await;
        return Err(runtime_unavailable(
            "Codex Runtime started analysis without a Turn id",
        ));
    };
    sqlx::query("UPDATE task_analysis_execution_snapshots SET turn_id=$1, state='started', updated_at=now() WHERE organization_id=$2 AND id=$3")
        .bind(turn_id)
        .bind(auth.organization_id)
        .bind(snapshot_id)
        .execute(&state.db)
        .await
        .map_err(database_error)?;
    sqlx::query(
        "UPDATE runs SET status = 'running', active_turn_id = $1, lease_owner = NULL, lease_token = NULL, lease_expires_at = NULL, updated_at = now() WHERE organization_id = $2 AND id = $3 AND status IN ('running', 'completed')",
    )
    .bind(turn_id)
    .bind(auth.organization_id)
    .bind(run_id)
    .execute(&state.db)
    .await
    .map_err(database_error)?;
    sqlx::query(
        "UPDATE tasks SET status = 'running', updated_at = now() WHERE organization_id = $1 AND id = $2 AND status NOT IN ('cancelled', 'archived', 'failed')",
    )
    .bind(auth.organization_id)
    .bind(task_id)
    .execute(&state.db)
    .await
    .map_err(database_error)?;
    Ok(Json(AnalysisStartResponse {
        execution_snapshot_id: snapshot_id,
        task_dataset_binding_id: binding_id,
        readiness_fingerprint: request.readiness_fingerprint,
        state: "started".to_string(),
    }))
}

async fn load_draft(
    state: &AppState,
    auth: &AuthenticatedUser,
    workspace_id: Uuid,
    draft_id: Uuid,
) -> Result<WorkspaceDataDraftSummary, ApiError> {
    let row = sqlx::query(
        "SELECT id, revision FROM workspace_data_drafts WHERE id = $1 AND organization_id = $2 AND workspace_id = $3",
    )
    .bind(draft_id)
    .bind(auth.organization_id)
    .bind(workspace_id)
    .fetch_optional(&state.db)
    .await
    .map_err(database_error)?
    .ok_or_else(|| not_found("Data draft was not found"))?;
    Ok(WorkspaceDataDraftSummary {
        draft_id,
        workspace_id,
        revision: row.get("revision"),
        assets: load_assets(state, auth, workspace_id, draft_id).await?,
    })
}

async fn load_assets(
    state: &AppState,
    auth: &AuthenticatedUser,
    workspace_id: Uuid,
    draft_id: Uuid,
) -> Result<Vec<SourceAssetSummary>, ApiError> {
    Ok(load_asset_rows(state, auth, workspace_id, draft_id)
        .await?
        .into_iter()
        .map(|row| SourceAssetSummary {
            asset_id: row.get("id"),
            file_name: row.get("file_name"),
            media_type: row.get("media_type"),
            byte_size: row.get("byte_size"),
            content_sha256: row.get("content_sha256"),
        })
        .collect())
}

async fn cleanup_source_assets(
    git: &GitRuntime,
    workspace_id: Uuid,
    assets: &[UploadedAsset],
    written: &[(Uuid, String)],
) {
    for (asset_id, _) in written {
        if let Some(asset) = assets.iter().find(|asset| asset.asset_id == *asset_id) {
            let _ = git
                .remove_source_asset(workspace_id, *asset_id, &asset.file_name)
                .await;
        }
    }
}

async fn load_asset_rows(
    state: &AppState,
    auth: &AuthenticatedUser,
    workspace_id: Uuid,
    draft_id: Uuid,
) -> Result<Vec<sqlx::postgres::PgRow>, ApiError> {
    sqlx::query(
        "SELECT asset.id, asset.file_name, asset.media_type, asset.byte_size, \
                asset.content_sha256, asset.relative_path \
         FROM workspace_data_source_assets asset \
         JOIN workspace_data_draft_assets draft_asset \
           ON draft_asset.organization_id = asset.organization_id \
          AND draft_asset.asset_id = asset.id \
         WHERE asset.organization_id = $1 AND asset.workspace_id = $2 \
           AND draft_asset.draft_id = $3 ORDER BY asset.created_at, asset.id",
    )
    .bind(auth.organization_id)
    .bind(workspace_id)
    .bind(draft_id)
    .fetch_all(&state.db)
    .await
    .map_err(database_error)
}

async fn load_session(
    state: &AppState,
    auth: &AuthenticatedUser,
    intake_id: Uuid,
) -> Result<DataIntakeSessionSummary, ApiError> {
    let row = sqlx::query(
        "SELECT intake.id, intake.task_id, intake.workspace_id, intake.contract_id, intake.contract_version, intake.contract_sha256, intake.status, intake.input_revision, \
         mapping_revision, gap_fingerprint, evidence_fingerprint, gaps, mapping_candidates, confirmed_mapping, parameter_answers, \
         requirement_profile, source_profile, mapping_proposal, readiness_review, \
         requirement_artifact_id, source_artifact_id, mapping_artifact_id, readiness_artifact_id, \
         normalized_release_id, attempt_count, failure_code, failure_summary, \
         binding.id AS task_dataset_binding_id \
         FROM data_intake_sessions intake \
         LEFT JOIN task_dataset_bindings binding ON binding.organization_id = intake.organization_id \
           AND binding.id = intake.current_binding_id \
         WHERE intake.id = $1 AND intake.organization_id = $2",
    )
    .bind(intake_id)
    .bind(auth.organization_id)
    .fetch_optional(&state.db)
    .await
    .map_err(database_error)?
    .ok_or_else(|| not_found("Data intake session was not found"))?;
    let requirement_profile: Option<Value> = row.get("requirement_profile");
    let contract = contract_summary(
        &row.get::<String, _>("contract_id"),
        &row.get::<String, _>("contract_version"),
        &row.get::<String, _>("contract_sha256"),
        requirement_profile.as_ref(),
    );
    let mut gaps: Vec<DataIntakeGap> = decode_json(row.get("gaps"))?;
    let stored_candidates: Vec<DataMappingCandidate> = decode_json(row.get("mapping_candidates"))?;
    let source_profile: Option<Value> = match row.get("source_profile") {
        Some(profile) => Some(profile),
        None => {
            artifact_content_for_session(
                state,
                auth,
                row.get("task_id"),
                row.get("source_artifact_id"),
            )
            .await?
        }
    };
    let mapping_proposal: Option<Value> = match row.get("mapping_proposal") {
        Some(proposal) => Some(proposal),
        None => {
            artifact_content_for_session(
                state,
                auth,
                row.get("task_id"),
                row.get("mapping_artifact_id"),
            )
            .await?
        }
    };
    let readiness_review: Option<Value> = match row.get("readiness_review") {
        Some(review) => Some(review),
        None => {
            artifact_content_for_session(
                state,
                auth,
                row.get("task_id"),
                row.get("readiness_artifact_id"),
            )
            .await?
        }
    };
    let candidates = if stored_candidates.is_empty() {
        mapping_proposal
            .as_ref()
            .map(|proposal| mapping_candidates_from_artifact(proposal, source_profile.as_ref()))
            .unwrap_or_default()
    } else {
        stored_candidates
    };
    if let Some(conflicts) = mapping_proposal
        .as_ref()
        .and_then(|proposal| proposal.get("conflicts"))
        .and_then(Value::as_array)
    {
        for conflict in conflicts {
            let target = conflict
                .get("target_entity")
                .and_then(Value::as_str)
                .zip(conflict.get("target_field").and_then(Value::as_str))
                .map(|(entity, field)| format!("{entity}.{field}"))
                .unwrap_or_else(|| "mapping".to_string());
            if !gaps
                .iter()
                .any(|gap| gap.path == target && gap.code == DataGapKind::AmbiguousMapping)
            {
                gaps.push(DataIntakeGap {
                    code: DataGapKind::AmbiguousMapping,
                    path: target,
                    message: "Multiple source fields have similar meaning; the Data Agent must publish a revised mapping before confirmation.".to_string(),
                    required: true,
                });
            }
        }
    }
    let confirmed_mapping: Vec<DataMappingCandidate> = decode_json(row.get("confirmed_mapping"))?;
    let answers: Vec<DataIntakeParameterAnswer> = decode_json(row.get("parameter_answers"))?;
    let request_rows = sqlx::query(
        "SELECT id, task_id, intake_session_id, kind, session_revision, status, \
                artifact_id, response \
         FROM data_intake_input_requests \
         WHERE organization_id = $1 AND task_id = $2 AND intake_session_id = $3 \
           AND status = 'open' ORDER BY created_at, id",
    )
    .bind(auth.organization_id)
    .bind(row.get::<Uuid, _>("task_id"))
    .bind(intake_id)
    .fetch_all(&state.db)
    .await
    .map_err(database_error)?;
    let input_requests = request_rows
        .into_iter()
        .map(|request| DataIntakeInputRequest {
            request_id: request.get("id"),
            task_id: request.get("task_id"),
            intake_id: request.get("intake_session_id"),
            kind: request.get("kind"),
            session_revision: request.get("session_revision"),
            status: request.get("status"),
            prompt: match request.get::<String, _>("kind").as_str() {
                "confirm_profile" => {
                    "Review and confirm the complete planning data profile.".to_string()
                }
                "confirm_mapping" => {
                    "Review and confirm the complete field mapping revision.".to_string()
                }
                "answer_parameters" => {
                    "Answer the required planning parameters with units and sources.".to_string()
                }
                "provide_data" => {
                    "Provide the smallest missing data or parameter evidence.".to_string()
                }
                "confirm_analysis" => {
                    "Review the final analysis checklist and confirm start.".to_string()
                }
                _ => "Provide the requested planning input.".to_string(),
            },
            value: request.get("response"),
        })
        .collect();
    Ok(DataIntakeSessionSummary {
        intake_id,
        task_id: row.get("task_id"),
        workspace_id: row.get("workspace_id"),
        parameters: profile_parameters(requirement_profile.as_ref().unwrap_or(&Value::Null)),
        contract,
        status: status_from_db(row.get("status"))?,
        input_revision: row.get("input_revision"),
        mapping_revision: row.get("mapping_revision"),
        gap_fingerprint: row.get("gap_fingerprint"),
        evidence_fingerprint: row.get("evidence_fingerprint"),
        gaps,
        candidates,
        confirmed_mapping,
        answers,
        attempt_count: row.get("attempt_count"),
        failure_code: row.get("failure_code"),
        failure_summary: row.get("failure_summary"),
        input_requests,
        requirement_profile: artifact_content_for_session(
            state,
            auth,
            row.get("task_id"),
            row.get("requirement_artifact_id"),
        )
        .await?,
        source_profile,
        mapping_proposal,
        readiness_review,
    })
}

fn mapping_candidates_from_artifact(
    proposal: &Value,
    source_profile: Option<&Value>,
) -> Vec<DataMappingCandidate> {
    let names = source_profile
        .and_then(|profile| profile.get("sources"))
        .and_then(Value::as_array)
        .map(|sources| {
            sources
                .iter()
                .filter_map(|source| {
                    Some((
                        source.get("source_ref")?.as_str()?.to_string(),
                        (
                            source.get("display_name")?.as_str()?.to_string(),
                            source
                                .get("source_asset_id")
                                .and_then(Value::as_str)
                                .and_then(|value| Uuid::parse_str(value).ok()),
                        ),
                    ))
                })
                .collect::<std::collections::HashMap<_, _>>()
        })
        .unwrap_or_default();
    proposal
        .get("candidates")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|candidate| {
            let source_ref = candidate.get("source_ref")?.as_str()?.to_string();
            let source_field = candidate.get("source_field")?.as_str()?.to_string();
            let target_entity = candidate.get("target_entity")?.as_str()?.to_string();
            let target_field = candidate.get("target_field")?.as_str()?.to_string();
            let confidence = candidate
                .get("confidence")
                .and_then(Value::as_f64)
                .filter(|value| value.is_finite())
                .map(|value| value as f32)
                .unwrap_or(0.0);
            Some(DataMappingCandidate {
                source_asset_id: candidate
                    .get("source_asset_id")
                    .and_then(Value::as_str)
                    .and_then(|value| Uuid::parse_str(value).ok())
                    .or_else(|| names.get(&source_ref).and_then(|(_, id)| *id)),
                source_ref: Some(source_ref.clone()),
                source_display_name: names.get(&source_ref).map(|(name, _)| name.clone()),
                source_path: names
                    .get(&source_ref)
                    .map(|(name, _)| name.clone())
                    .unwrap_or(source_ref),
                source_field,
                target_entity,
                target_field,
                source_unit: candidate
                    .get("source_unit")
                    .or_else(|| candidate.get("sourceUnit"))
                    .and_then(Value::as_str)
                    .map(str::to_string),
                target_unit: candidate
                    .get("target_unit")
                    .or_else(|| candidate.get("targetUnit"))
                    .and_then(Value::as_str)
                    .map(str::to_string),
                transformation: candidate
                    .get("transformation")
                    .or_else(|| candidate.get("transform"))
                    .and_then(Value::as_str)
                    .map(str::to_string),
                conflict: candidate
                    .get("conflict")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                confidence,
                reason: candidate
                    .get("reason")
                    .and_then(Value::as_str)
                    .unwrap_or("候选映射，必须由用户确认")
                    .to_string(),
                requires_confirmation: candidate
                    .get("requires_confirmation")
                    .and_then(Value::as_bool)
                    .unwrap_or(true),
            })
        })
        .collect()
}

async fn artifact_content_for_session(
    state: &AppState,
    auth: &AuthenticatedUser,
    task_id: Uuid,
    artifact_id: Option<Uuid>,
) -> Result<Option<Value>, ApiError> {
    let Some(artifact_id) = artifact_id else {
        return Ok(None);
    };
    let row = sqlx::query(
        "SELECT artifact.state, artifact.content FROM artifacts artifact
         JOIN artifact_task_grants artifact_grant ON artifact_grant.organization_id = artifact.organization_id
          AND artifact_grant.artifact_id = artifact.id AND artifact_grant.task_id = $1 AND artifact_grant.permission = 'read'
         WHERE artifact.organization_id = $2 AND artifact.id = $3
           AND artifact.retention_state = 'active'",
    )
    .bind(task_id)
    .bind(auth.organization_id)
    .bind(artifact_id)
    .fetch_optional(&state.db)
    .await
    .map_err(database_error)?;
    let Some(row) = row else {
        return Ok(None);
    };
    if row.get::<String, _>("state") != "ready" {
        return Ok(None);
    }
    let Some(content) = row.get::<Option<Vec<u8>>, _>("content") else {
        return Ok(None);
    };
    if content.len() > 2 * 1024 * 1024 {
        return Ok(None);
    }
    serde_json::from_slice(&content)
        .map(Some)
        .map_err(|_| internal_error("The Intake Artifact is not valid bounded JSON"))
}

async fn load_session_row<'a>(
    state: &AppState,
    auth: &AuthenticatedUser,
    task_id: Uuid,
) -> Result<sqlx::postgres::PgRow, ApiError> {
    sqlx::query(
        "SELECT id, workspace_id, contract_id, contract_version, contract_sha256, status, input_revision, mapping_revision, mapping_candidates, confirmed_mapping, parameter_answers, mapping_proposal, source_profile, requirement_artifact_id, dataset_artifact_id, readiness_artifact_id, evidence_fingerprint, profile_confirmation_sha256, mapping_confirmation_sha256, parameter_confirmation_sha256 \
         FROM data_intake_sessions WHERE organization_id = $1 AND task_id = $2 \
         ORDER BY updated_at DESC, id DESC LIMIT 1",
    )
    .bind(auth.organization_id)
    .bind(task_id)
    .fetch_optional(&state.db)
    .await
    .map_err(database_error)?
    .ok_or_else(|| not_found("Data intake session was not found"))
}

async fn artifact_hash(
    state: &AppState,
    organization_id: Uuid,
    artifact_id: Option<Uuid>,
) -> Result<Option<String>, ApiError> {
    let Some(artifact_id) = artifact_id else {
        return Ok(None);
    };
    let hash = sqlx::query_scalar::<_, Option<String>>(
        "SELECT content_sha256 FROM artifacts WHERE organization_id = $1 AND id = $2",
    )
    .bind(organization_id)
    .bind(artifact_id)
    .fetch_optional(&state.db)
    .await
    .map_err(database_error)?
    .flatten()
    .unwrap_or_else(|| hex::encode(Sha256::digest(artifact_id.as_bytes())));
    Ok(Some(hash))
}

async fn recompute_intake_fingerprint(
    state: &AppState,
    organization_id: Uuid,
    intake_id: Uuid,
) -> Result<String, ApiError> {
    let row = sqlx::query(
        "SELECT contract_id, contract_version, contract_sha256,
                requirement_artifact_id, source_artifact_id, mapping_artifact_id,
                dataset_artifact_id, readiness_artifact_id,
                profile_confirmation_sha256, mapping_confirmation_sha256,
                parameter_confirmation_sha256, readiness_confirmation_sha256,
                parameter_answers, gaps
         FROM data_intake_sessions
         WHERE organization_id = $1 AND id = $2",
    )
    .bind(organization_id)
    .bind(intake_id)
    .fetch_optional(&state.db)
    .await
    .map_err(database_error)?
    .ok_or_else(|| not_found("Data intake session was not found"))?;
    let artifact_hashes = futures_util::future::try_join_all([
        artifact_hash(state, organization_id, row.get("requirement_artifact_id")),
        artifact_hash(state, organization_id, row.get("source_artifact_id")),
        artifact_hash(state, organization_id, row.get("mapping_artifact_id")),
        artifact_hash(state, organization_id, row.get("dataset_artifact_id")),
        artifact_hash(state, organization_id, row.get("readiness_artifact_id")),
    ])
    .await?;
    let fingerprint = fingerprint_json(&serde_json::json!({
        "contract": [row.get::<String, _>("contract_id"), row.get::<String, _>("contract_version"), row.get::<String, _>("contract_sha256")],
        "artifacts": artifact_hashes,
        "profileConfirmation": row.get::<Option<String>, _>("profile_confirmation_sha256"),
        "mappingConfirmation": row.get::<Option<String>, _>("mapping_confirmation_sha256"),
        "parameterConfirmation": row.get::<Option<String>, _>("parameter_confirmation_sha256"),
        "readinessConfirmation": row.get::<Option<String>, _>("readiness_confirmation_sha256"),
        "parameters": row.get::<Value, _>("parameter_answers"),
    }))?;
    let gaps: Vec<Value> = decode_json(row.get("gaps"))?;
    let ready = row
        .get::<Option<Uuid>, _>("requirement_artifact_id")
        .is_some()
        && row.get::<Option<Uuid>, _>("source_artifact_id").is_some()
        && row.get::<Option<Uuid>, _>("mapping_artifact_id").is_some()
        && row.get::<Option<Uuid>, _>("dataset_artifact_id").is_some()
        && row
            .get::<Option<Uuid>, _>("readiness_artifact_id")
            .is_some()
        && row
            .get::<Option<String>, _>("profile_confirmation_sha256")
            .is_some()
        && row
            .get::<Option<String>, _>("mapping_confirmation_sha256")
            .is_some()
        && row
            .get::<Option<String>, _>("parameter_confirmation_sha256")
            .is_some()
        && row
            .get::<Option<String>, _>("readiness_confirmation_sha256")
            .is_some()
        && gaps.is_empty();
    sqlx::query(
        "UPDATE data_intake_sessions
         SET evidence_fingerprint = $1,
             status = CASE WHEN $2 THEN 'ready' ELSE 'active' END,
             updated_at = now()
         WHERE organization_id = $3 AND id = $4",
    )
    .bind(&fingerprint)
    .bind(ready)
    .bind(organization_id)
    .bind(intake_id)
    .execute(&state.db)
    .await
    .map_err(database_error)?;
    Ok(fingerprint)
}

async fn hash_json_requests(
    state: &AppState,
    auth: &AuthenticatedUser,
    task_id: Uuid,
    kind: &str,
) -> Result<Option<String>, ApiError> {
    let response = sqlx::query_scalar::<_, Option<Value>>(
        "SELECT response FROM data_intake_input_requests WHERE organization_id = $1 AND task_id = $2 AND kind = $3 AND status = 'answered' ORDER BY answered_at DESC NULLS LAST, id DESC LIMIT 1",
    )
    .bind(auth.organization_id)
    .bind(task_id)
    .bind(kind)
    .fetch_optional(&state.db)
    .await
    .map_err(database_error)?
    .flatten();
    response.map(|value| fingerprint_json(&value)).transpose()
}

fn contract_summary(
    contract_id: &str,
    version: &str,
    content_sha256: &str,
    profile: Option<&Value>,
) -> DataRequirementContract {
    let display_name = profile
        .and_then(|value| value.get("title").or_else(|| value.get("displayName")))
        .and_then(Value::as_str)
        .unwrap_or("Planning data requirements")
        .to_string();
    let description = profile
        .and_then(|value| value.get("description").or_else(|| value.get("goal")))
        .and_then(Value::as_str)
        .unwrap_or("Requirements are owned by the bound capability package.")
        .to_string();
    DataRequirementContract {
        contract_id: contract_id.to_string(),
        version: version.to_string(),
        content_sha256: content_sha256.to_string(),
        display_name,
        description,
        // The full domain schema remains in the immutable Profile Artifact;
        // this DTO is only a bounded human-readable reference.
        required_entities: Vec::new(),
        business_parameters: profile_parameters(profile.unwrap_or(&Value::Null)),
    }
}

fn profile_parameters(profile: &Value) -> Vec<DataRequirementParameter> {
    let Some(parameters) = profile
        .get("parameters")
        .or_else(|| profile.get("businessParameters"))
        .and_then(Value::as_array)
    else {
        return Vec::new();
    };
    parameters
        .iter()
        .filter_map(|value| serde_json::from_value::<DataRequirementParameter>(value.clone()).ok())
        .take(64)
        .collect()
}

fn validate_answers(
    parameters: &[DataRequirementParameter],
    answers: &[DataIntakeParameterAnswer],
) -> Result<Vec<DataIntakeParameterAnswer>, ApiError> {
    let mut seen = std::collections::HashSet::new();
    for answer in answers {
        let parameter = parameters
            .iter()
            .find(|parameter| parameter.name == answer.name)
            .ok_or_else(|| {
                bad_request("The submitted parameter is not in the confirmed Profile")
            })?;
        if answer.source.trim().is_empty()
            || answer.value.is_null()
            || !seen.insert(answer.name.clone())
        {
            return Err(bad_request(format!(
                "Parameter {} requires a non-empty source and one value",
                parameter.display_name
            )));
        }
        if answer.unit.as_deref() != parameter.unit.as_deref() {
            return Err(bad_request(format!(
                "Parameter {} must use unit {:?}",
                parameter.display_name, parameter.unit
            )));
        }
        if parameter.data_type == "number"
            && answer.value.as_f64().is_none_or(|value| !value.is_finite())
        {
            return Err(bad_request(format!(
                "Parameter {} must be a finite number",
                parameter.display_name
            )));
        }
        if parameter.data_type == "string"
            && answer
                .value
                .as_str()
                .is_some_and(|value| value.trim().is_empty())
        {
            return Err(bad_request(format!(
                "Parameter {} must not be empty",
                parameter.display_name
            )));
        }
    }
    Ok(answers.to_vec())
}

fn validate_file_name(file_name: &str) -> Result<(), ApiError> {
    if file_name.len() > 256
        || file_name.contains('/')
        || file_name.contains('\\')
        || file_name == "."
        || file_name == ".."
    {
        return Err(bad_request("File names must be simple relative names"));
    }
    if file_name.to_ascii_lowercase().ends_with(".xls") {
        return Err(bad_request(
            "Legacy .xls files are not supported; export as .xlsx, .csv or .json",
        ));
    }
    Ok(())
}

fn normalize_file_name(file_name: &str) -> String {
    file_name.trim().to_ascii_lowercase()
}

fn media_type_for(file_name: &str) -> Result<String, ApiError> {
    let lower = file_name.to_ascii_lowercase();
    if lower.ends_with(".xlsx") {
        Ok("application/vnd.openxmlformats-officedocument.spreadsheetml.sheet".to_string())
    } else if lower.ends_with(".csv") {
        Ok("text/csv".to_string())
    } else if lower.ends_with(".json") {
        Ok("application/json".to_string())
    } else {
        Err(bad_request(
            "Only .xlsx, .csv and .json files are supported",
        ))
    }
}

fn validate_content(file_name: &str, bytes: &[u8]) -> Result<(), ApiError> {
    let lower = file_name.to_ascii_lowercase();
    if lower.ends_with(".json") {
        let value: Value =
            serde_json::from_slice(bytes).map_err(|_| bad_request("JSON input is invalid"))?;
        let mut count = 0;
        validate_json_shape(&value, 0, &mut count)?;
    } else if lower.ends_with(".xlsx") {
        validate_xlsx_archive(bytes)?;
    } else if bytes.iter().filter(|byte| **byte == b'\n').count() > 1_000_000 {
        return Err(bad_request("CSV contains too many rows"));
    }
    Ok(())
}

fn validate_xlsx_archive(bytes: &[u8]) -> Result<(), ApiError> {
    if !bytes.starts_with(b"PK\x03\x04") {
        return Err(bad_request("The XLSX file is invalid"));
    }
    let search_start = bytes.len().saturating_sub(65_557);
    let eocd = bytes[search_start..]
        .windows(4)
        .rposition(|window| window == b"PK\x05\x06")
        .map(|offset| search_start + offset)
        .ok_or_else(|| bad_request("The XLSX central directory is missing"))?;
    if eocd + 22 > bytes.len() {
        return Err(bad_request("The XLSX central directory is truncated"));
    }
    let entries = u16::from_le_bytes([bytes[eocd + 10], bytes[eocd + 11]]) as u32;
    let central_size = u32::from_le_bytes([
        bytes[eocd + 12],
        bytes[eocd + 13],
        bytes[eocd + 14],
        bytes[eocd + 15],
    ]) as usize;
    let central_offset = u32::from_le_bytes([
        bytes[eocd + 16],
        bytes[eocd + 17],
        bytes[eocd + 18],
        bytes[eocd + 19],
    ]) as usize;
    if entries == 0
        || entries > MAX_XLSX_ENTRIES
        || central_offset
            .checked_add(central_size)
            .is_none_or(|end| end > bytes.len())
    {
        return Err(bad_request("The XLSX archive is too large or unsupported"));
    }
    let central_end = central_offset + central_size;
    let mut cursor = central_offset;
    let mut expanded_bytes = 0u64;
    for _ in 0..entries {
        if cursor.checked_add(46).is_none_or(|end| end > central_end)
            || &bytes[cursor..cursor + 4] != b"PK\x01\x02"
        {
            return Err(bad_request("The XLSX central directory is invalid"));
        }
        let compressed = u32::from_le_bytes([
            bytes[cursor + 20],
            bytes[cursor + 21],
            bytes[cursor + 22],
            bytes[cursor + 23],
        ]) as u64;
        let expanded = u32::from_le_bytes([
            bytes[cursor + 24],
            bytes[cursor + 25],
            bytes[cursor + 26],
            bytes[cursor + 27],
        ]) as u64;
        let name_len = u16::from_le_bytes([bytes[cursor + 28], bytes[cursor + 29]]) as usize;
        let extra_len = u16::from_le_bytes([bytes[cursor + 30], bytes[cursor + 31]]) as usize;
        let comment_len = u16::from_le_bytes([bytes[cursor + 32], bytes[cursor + 33]]) as usize;
        let record_end = cursor
            .checked_add(46)
            .and_then(|end| end.checked_add(name_len))
            .and_then(|end| end.checked_add(extra_len))
            .and_then(|end| end.checked_add(comment_len))
            .ok_or_else(|| bad_request("The XLSX central directory is invalid"))?;
        if record_end > central_end {
            return Err(bad_request("The XLSX central directory is truncated"));
        }
        let name = &bytes[cursor + 46..cursor + 46 + name_len];
        if name
            .windows(b"vbaProject.bin".len())
            .any(|part| part.eq_ignore_ascii_case(b"vbaProject.bin"))
            || name
                .windows(b"externalLinks/".len())
                .any(|part| part.eq_ignore_ascii_case(b"externalLinks/"))
        {
            return Err(bad_request(
                "The XLSX file contains macros or external links",
            ));
        }
        expanded_bytes = expanded_bytes
            .checked_add(expanded)
            .ok_or_else(|| bad_request("The XLSX archive is too large"))?;
        if expanded_bytes > MAX_XLSX_EXPANDED_BYTES
            || (compressed == 0 && expanded > 0)
            || (compressed > 0 && expanded > compressed.saturating_mul(100))
        {
            return Err(bad_request(
                "The XLSX archive exceeds decompression safety limits",
            ));
        }
        cursor = record_end;
    }
    if cursor != central_end {
        return Err(bad_request("The XLSX central directory is invalid"));
    }
    Ok(())
}

fn validate_json_shape(value: &Value, depth: usize, count: &mut usize) -> Result<(), ApiError> {
    if depth > MAX_JSON_DEPTH {
        return Err(bad_request("JSON nesting is too deep"));
    }
    *count += 1;
    if *count > MAX_JSON_NODES {
        return Err(bad_request("JSON contains too many values"));
    }
    match value {
        Value::Object(map) => map
            .values()
            .try_for_each(|value| validate_json_shape(value, depth + 1, count)),
        Value::Array(values) => values
            .iter()
            .try_for_each(|value| validate_json_shape(value, depth + 1, count)),
        _ => Ok(()),
    }
}

fn same_assets(stored: &[SourceAssetSummary], uploaded: &[UploadedAsset]) -> bool {
    stored.len() == uploaded.len()
        && uploaded.iter().all(|asset| {
            stored.iter().any(|stored| {
                stored.file_name == asset.file_name && stored.content_sha256 == asset.content_sha256
            })
        })
}

fn fingerprint_json(value: &impl serde::Serialize) -> Result<String, ApiError> {
    Ok(hex::encode(Sha256::digest(
        serde_json::to_vec(value).map_err(internal_serde_error)?,
    )))
}
fn decode_json<T: DeserializeOwned>(value: Value) -> Result<T, ApiError> {
    serde_json::from_value(value).map_err(|_| internal_error("Stored data intake state is invalid"))
}
fn status_from_db(value: String) -> Result<DataIntakeStatus, ApiError> {
    match value.as_str() {
        "active" => Ok(DataIntakeStatus::Active),
        "ready" => Ok(DataIntakeStatus::Ready),
        "failed" => Ok(DataIntakeStatus::Failed),
        "cancelled" => Ok(DataIntakeStatus::Cancelled),
        _ => Err(internal_error("Stored data intake status is invalid")),
    }
}

fn database_error(_error: sqlx::Error) -> ApiError {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(PlatformError::internal("database operation failed")),
    )
}
fn bad_request(message: impl Into<String>) -> ApiError {
    (
        StatusCode::BAD_REQUEST,
        Json(PlatformError::bad_request(message.into())),
    )
}
fn conflict(message: impl Into<String>) -> ApiError {
    (
        StatusCode::CONFLICT,
        Json(PlatformError {
            kind: ErrorKind::Conflict,
            message: message.into(),
            request_id: None,
            retry_after_ms: None,
        }),
    )
}
fn runtime_unavailable(message: impl Into<String>) -> ApiError {
    (
        StatusCode::SERVICE_UNAVAILABLE,
        Json(PlatformError {
            kind: ErrorKind::CodexUnavailable,
            message: message.into(),
            request_id: None,
            retry_after_ms: None,
        }),
    )
}
fn not_found(message: impl Into<String>) -> ApiError {
    (
        StatusCode::NOT_FOUND,
        Json(PlatformError::not_found(message.into())),
    )
}
fn internal_error(message: impl Into<String>) -> ApiError {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(PlatformError::internal(message.into())),
    )
}
fn internal_serde_error(error: serde_json::Error) -> ApiError {
    internal_error(format!("data intake state could not be encoded: {error}"))
}

#[cfg(test)]
mod tests {
    use super::{contract_summary, validate_file_name, validate_json_shape, validate_xlsx_archive};
    use serde_json::json;

    #[test]
    fn contract_summary_keeps_only_capability_reference_without_domain_defaults() {
        let contract = contract_summary(
            "indonesia-warehouse-network",
            "2.0.0",
            &"a".repeat(64),
            None,
        );
        assert_eq!(contract.content_sha256.len(), 64);
        assert!(contract.required_entities.is_empty());
        assert!(contract.business_parameters.is_empty());
    }

    #[test]
    fn upload_names_reject_legacy_formats_and_paths() {
        assert!(validate_file_name("planning.xlsx").is_ok());
        assert!(validate_file_name("planning.xls").is_err());
        assert!(validate_file_name("nested/planning.json").is_err());
    }

    #[test]
    fn json_profile_limits_depth_and_node_count() {
        let value = json!({"orders": [{"origin": "JKT"}]});
        let mut count = 0;
        assert!(validate_json_shape(&value, 0, &mut count).is_ok());
        let mut count = 0;
        assert!(validate_json_shape(&value, super::MAX_JSON_DEPTH + 1, &mut count).is_err());
    }

    #[test]
    fn xlsx_requires_a_bounded_central_directory() {
        assert!(validate_xlsx_archive(b"PK\x03\x04").is_err());
    }

    #[tokio::test]
    #[ignore = "requires TEST_DATABASE_URL pointing at disposable PostgreSQL"]
    async fn response_lookup_reads_the_input_request_table() {
        let database_url = std::env::var("TEST_DATABASE_URL").expect("TEST_DATABASE_URL");
        let pool = sqlx::postgres::PgPoolOptions::new()
            .max_connections(1)
            .connect(&database_url)
            .await
            .expect("connect disposable PostgreSQL database");
        let mut transaction = pool.begin().await.expect("begin transaction");
        sqlx::query(
            "CREATE TEMP TABLE data_intake_input_requests (
                organization_id uuid NOT NULL,
                task_id uuid NOT NULL,
                id uuid NOT NULL,
                intake_session_id uuid NOT NULL,
                kind text NOT NULL,
                evidence_fingerprint text NOT NULL,
                status text NOT NULL,
                response jsonb
            ) ON COMMIT DROP",
        )
        .execute(&mut *transaction)
        .await
        .expect("create temporary input request table");
        let organization_id = uuid::Uuid::now_v7();
        let task_id = uuid::Uuid::now_v7();
        let request_id = uuid::Uuid::now_v7();
        let intake_id = uuid::Uuid::now_v7();
        sqlx::query(
            "INSERT INTO data_intake_input_requests
             (organization_id, task_id, id, intake_session_id, kind, evidence_fingerprint, status, response)
             VALUES ($1, $2, $3, $4, 'confirm_mapping', repeat('a', 64), 'open', '{}'::jsonb)",
        )
        .bind(organization_id)
        .bind(task_id)
        .bind(request_id)
        .bind(intake_id)
        .execute(&mut *transaction)
        .await
        .expect("insert temporary input request");
        let row = sqlx::query(
            "SELECT kind, status, response
             FROM data_intake_input_requests
             WHERE organization_id = $1 AND task_id = $2 AND id = $3
               AND intake_session_id = $4",
        )
        .bind(organization_id)
        .bind(task_id)
        .bind(request_id)
        .bind(intake_id)
        .fetch_one(&mut *transaction)
        .await
        .expect("lookup input request");
        assert_eq!(sqlx::Row::get::<String, _>(&row, "kind"), "confirm_mapping");
        assert_eq!(sqlx::Row::get::<String, _>(&row, "status"), "open");
    }
}
