use std::collections::BTreeMap;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use open_web_codex_platform_contracts::error::PlatformError;
use open_web_codex_platform_contracts::{
    SupervisorDefinitionSummary, SupervisorDraftRequest, SupervisorReleaseSummary,
    SupervisorValidationIssue, SupervisorValidationResult,
};
use open_web_codex_platform_store::AppState;
use open_web_codex_supervisor_catalog::{
    agent,
    supervisor::{self, ArtifactContract, SupervisorAgentReference, SupervisorReleaseSpec},
};
use sqlx::Row;
use uuid::Uuid;

use crate::middleware::auth::AuthenticatedUser;
use crate::supervisor_policy;

mod store;

use store::{
    load_definition, load_definitions, load_draft_row, lock_definition, parse_draft, record_audit,
};

type ApiError = (StatusCode, Json<PlatformError>);
type ApiResult<T> = Result<Json<T>, ApiError>;

pub async fn list(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
) -> ApiResult<Vec<SupervisorDefinitionSummary>> {
    load_definitions(&state.db, auth.organization_id)
        .await
        .map(Json)
}

pub async fn create(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Json(draft): Json<SupervisorDraftRequest>,
) -> ApiResult<SupervisorDefinitionSummary> {
    validate_draft_storage_shape(&draft)?;
    if supervisor_policy::is_reserved_builtin_policy_id(&draft.policy_id) {
        return Err(bad_request(
            "The policy id is reserved by a built-in Supervisor Package",
        ));
    }
    let definition_id = Uuid::now_v7();
    let revision_id = Uuid::now_v7();
    let draft_spec = serde_json::to_value(&draft).map_err(|_| internal_error())?;
    let mut transaction = state.db.begin().await.map_err(database_error)?;
    sqlx::query(
        "INSERT INTO supervisor_definitions \
         (id, organization_id, owner_user_id, policy_id, display_name, description) \
         VALUES ($1, $2, $3, $4, $5, $6)",
    )
    .bind(definition_id)
    .bind(auth.organization_id)
    .bind(auth.user_id)
    .bind(&draft.policy_id)
    .bind(&draft.display_name)
    .bind(&draft.description)
    .execute(&mut *transaction)
    .await
    .map_err(database_conflict)?;
    sqlx::query(
        "INSERT INTO supervisor_revisions \
         (id, organization_id, definition_id, version, draft_spec, created_by) \
         VALUES ($1, $2, $3, $4, $5, $6)",
    )
    .bind(revision_id)
    .bind(auth.organization_id)
    .bind(definition_id)
    .bind(&draft.version)
    .bind(draft_spec)
    .bind(auth.user_id)
    .execute(&mut *transaction)
    .await
    .map_err(database_conflict)?;
    record_audit(
        &mut transaction,
        &auth,
        "supervisor.definition.created",
        definition_id,
    )
    .await?;
    transaction.commit().await.map_err(database_error)?;
    load_definition(&state.db, auth.organization_id, definition_id)
        .await
        .map(Json)
}

pub async fn save_draft(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(definition_id): Path<Uuid>,
    Json(draft): Json<SupervisorDraftRequest>,
) -> ApiResult<SupervisorDefinitionSummary> {
    validate_draft_storage_shape(&draft)?;
    let draft_spec = serde_json::to_value(&draft).map_err(|_| internal_error())?;
    let mut transaction = state.db.begin().await.map_err(database_error)?;
    let definition = lock_definition(&mut transaction, auth.organization_id, definition_id).await?;
    require_manage(&auth, definition.get("owner_user_id"))?;
    if definition.get::<String, _>("policy_id") != draft.policy_id {
        return Err(bad_request(
            "A Supervisor Definition policy id is immutable",
        ));
    }
    let draft_revision = sqlx::query(
        "SELECT id FROM supervisor_revisions \
         WHERE organization_id = $1 AND definition_id = $2 AND state = 'draft' \
         FOR UPDATE",
    )
    .bind(auth.organization_id)
    .bind(definition_id)
    .fetch_optional(&mut *transaction)
    .await
    .map_err(database_error)?;
    if let Some(row) = draft_revision {
        sqlx::query(
            "UPDATE supervisor_revisions \
             SET version = $1, draft_spec = $2, updated_at = now() \
             WHERE id = $3 AND organization_id = $4 AND state = 'draft'",
        )
        .bind(&draft.version)
        .bind(draft_spec)
        .bind(row.get::<Uuid, _>("id"))
        .bind(auth.organization_id)
        .execute(&mut *transaction)
        .await
        .map_err(database_conflict)?;
    } else {
        sqlx::query(
            "INSERT INTO supervisor_revisions \
             (id, organization_id, definition_id, version, draft_spec, created_by) \
             VALUES ($1, $2, $3, $4, $5, $6)",
        )
        .bind(Uuid::now_v7())
        .bind(auth.organization_id)
        .bind(definition_id)
        .bind(&draft.version)
        .bind(draft_spec)
        .bind(auth.user_id)
        .execute(&mut *transaction)
        .await
        .map_err(database_conflict)?;
    }
    sqlx::query(
        "UPDATE supervisor_definitions \
         SET display_name = $1, description = $2, updated_at = now() \
         WHERE id = $3 AND organization_id = $4",
    )
    .bind(&draft.display_name)
    .bind(&draft.description)
    .bind(definition_id)
    .bind(auth.organization_id)
    .execute(&mut *transaction)
    .await
    .map_err(database_error)?;
    record_audit(
        &mut transaction,
        &auth,
        "supervisor.draft.saved",
        definition_id,
    )
    .await?;
    transaction.commit().await.map_err(database_error)?;
    load_definition(&state.db, auth.organization_id, definition_id)
        .await
        .map(Json)
}

pub async fn validate(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(definition_id): Path<Uuid>,
) -> ApiResult<SupervisorValidationResult> {
    let row = load_draft_row(&state.db, auth.organization_id, definition_id).await?;
    require_manage(&auth, row.get("owner_user_id"))?;
    let draft = parse_draft(row.get("draft_spec"))?;
    Ok(Json(validate_draft(&draft)))
}

pub async fn publish(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(definition_id): Path<Uuid>,
) -> ApiResult<SupervisorReleaseSummary> {
    let mut transaction = state.db.begin().await.map_err(database_error)?;
    let row = sqlx::query(
        "SELECT definition.owner_user_id, definition.policy_id, revision.id AS revision_id, \
                revision.draft_spec \
         FROM supervisor_definitions definition \
         JOIN supervisor_revisions revision ON revision.definition_id = definition.id \
           AND revision.organization_id = definition.organization_id AND revision.state = 'draft' \
         WHERE definition.id = $1 AND definition.organization_id = $2 \
         FOR UPDATE OF definition, revision",
    )
    .bind(definition_id)
    .bind(auth.organization_id)
    .fetch_optional(&mut *transaction)
    .await
    .map_err(database_error)?
    .ok_or_else(|| not_found("Supervisor draft was not found"))?;
    require_manage(&auth, row.get("owner_user_id"))?;
    let draft = parse_draft(row.get("draft_spec"))?;
    if row.get::<String, _>("policy_id") != draft.policy_id {
        return Err(internal_error());
    }
    let package = resolve_draft(&draft).map_err(|issue| {
        (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(PlatformError::bad_request(issue.message)),
        )
    })?;
    let release_id = Uuid::now_v7();
    let revision_id: Uuid = row.get("revision_id");
    let release_spec = release_spec_from_draft(&draft).map_err(|issue| {
        (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(PlatformError::bad_request(issue.message)),
        )
    })?;
    let release_spec_value = serde_json::to_value(release_spec).map_err(|_| internal_error())?;
    let published_at = chrono::Utc::now();
    sqlx::query(
        "INSERT INTO supervisor_releases \
         (id, organization_id, definition_id, revision_id, policy_id, version, \
          display_name, description, release_spec, content_sha256, published_by, published_at) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)",
    )
    .bind(release_id)
    .bind(auth.organization_id)
    .bind(definition_id)
    .bind(revision_id)
    .bind(&package.policy_id)
    .bind(&package.version)
    .bind(&package.display_name)
    .bind(&package.description)
    .bind(release_spec_value)
    .bind(&package.content_sha256)
    .bind(auth.user_id)
    .bind(published_at)
    .execute(&mut *transaction)
    .await
    .map_err(database_conflict)?;
    sqlx::query(
        "UPDATE supervisor_revisions \
         SET state = 'published', published_at = $1, updated_at = $1 \
         WHERE id = $2 AND organization_id = $3 AND state = 'draft'",
    )
    .bind(published_at)
    .bind(revision_id)
    .bind(auth.organization_id)
    .execute(&mut *transaction)
    .await
    .map_err(database_error)?;
    sqlx::query(
        "UPDATE supervisor_definitions SET updated_at = $1 \
         WHERE id = $2 AND organization_id = $3",
    )
    .bind(published_at)
    .bind(definition_id)
    .bind(auth.organization_id)
    .execute(&mut *transaction)
    .await
    .map_err(database_error)?;
    record_audit(
        &mut transaction,
        &auth,
        "supervisor.release.published",
        release_id,
    )
    .await?;
    transaction.commit().await.map_err(database_error)?;
    Ok(Json(SupervisorReleaseSummary {
        id: release_id,
        policy_id: package.policy_id,
        version: package.version,
        display_name: package.display_name,
        description: package.description,
        content_sha256: package.content_sha256,
        published_at,
    }))
}

fn validate_draft(draft: &SupervisorDraftRequest) -> SupervisorValidationResult {
    match resolve_draft(draft) {
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

fn resolve_draft(
    draft: &SupervisorDraftRequest,
) -> Result<supervisor::ResolvedSupervisorPackage, SupervisorValidationIssue> {
    supervisor::validate_release(release_spec_from_draft(draft)?)
        .map_err(|error| validation_issue("invalid_release", error.to_string()))
}

fn release_spec_from_draft(
    draft: &SupervisorDraftRequest,
) -> Result<SupervisorReleaseSpec, SupervisorValidationIssue> {
    let published = agent::list_published()
        .map_err(|error| validation_issue("agent_catalog_invalid", error.to_string()))?;
    let by_identity = published
        .into_iter()
        .map(|definition| {
            (
                format!("{}@{}", definition.definition_id, definition.version),
                definition,
            )
        })
        .collect::<BTreeMap<_, _>>();
    let agents = draft
        .agents
        .iter()
        .map(|selection| {
            let identity = format!("{}@{}", selection.definition_id, selection.version);
            let definition = by_identity.get(&identity).ok_or_else(|| {
                validation_issue(
                    "agent_not_published",
                    format!("Agent Definition '{identity}' is not published"),
                )
            })?;
            Ok(SupervisorAgentReference {
                definition_id: selection.definition_id.clone(),
                version: selection.version.clone(),
                runtime_role: definition.runtime_role.clone(),
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

fn validate_draft_storage_shape(draft: &SupervisorDraftRequest) -> Result<(), ApiError> {
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

fn require_manage(auth: &AuthenticatedUser, owner_user_id: Uuid) -> Result<(), ApiError> {
    if owner_user_id == auth.user_id || matches!(auth.organization_role.as_str(), "owner" | "admin")
    {
        Ok(())
    } else {
        Err((
            StatusCode::FORBIDDEN,
            Json(PlatformError::forbidden(
                "Supervisor Definition is owned by another user",
            )),
        ))
    }
}

fn validation_issue(code: &str, message: String) -> SupervisorValidationIssue {
    SupervisorValidationIssue {
        code: code.to_string(),
        message,
    }
}

fn bad_request(message: &str) -> ApiError {
    (
        StatusCode::BAD_REQUEST,
        Json(PlatformError::bad_request(message)),
    )
}

fn not_found(message: &str) -> ApiError {
    (
        StatusCode::NOT_FOUND,
        Json(PlatformError::not_found(message)),
    )
}

fn internal_error() -> ApiError {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(PlatformError::internal(
            "Supervisor Definition storage is invalid",
        )),
    )
}

fn database_error(_error: sqlx::Error) -> ApiError {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(PlatformError::internal(
            "Supervisor Definition database operation failed",
        )),
    )
}

fn database_conflict(error: sqlx::Error) -> ApiError {
    if error
        .as_database_error()
        .and_then(|error| error.code())
        .as_deref()
        == Some("23505")
    {
        (
            StatusCode::CONFLICT,
            Json(PlatformError::bad_request(
                "Supervisor id or version already exists",
            )),
        )
    } else {
        database_error(error)
    }
}

#[cfg(test)]
mod tests;
