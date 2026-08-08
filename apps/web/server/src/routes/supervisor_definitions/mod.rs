use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use open_web_codex_platform_contracts::error::PlatformError;
use open_web_codex_platform_contracts::{
    PublishSupervisorDraftRequest, SupervisorDefinitionSummary, SupervisorDraftRequest,
    SupervisorDraftUpdateRequest, SupervisorReleaseSummary, SupervisorValidationResult,
};
use open_web_codex_platform_store::AppState;
use open_web_codex_supervisor_catalog::supervisor;
use sqlx::Row;
use uuid::Uuid;

use crate::supervisor_policy;
use crate::{agent_catalog, middleware::auth::AuthenticatedUser};

mod resolution;
mod store;

#[cfg(test)]
use resolution::release_spec_from_draft;
use resolution::{
    release_spec_from_draft_with_policy, release_spec_from_draft_with_version, validate_draft,
    validate_draft_storage_shape,
};
use store::{
    draft_content_sha256, load_definition, load_definitions, load_draft_row, lock_definition,
    parse_draft, record_audit,
};

pub(crate) type ApiError = (StatusCode, Json<PlatformError>);
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
    let content_sha256 = draft_content_sha256(&draft);
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
         (id, organization_id, definition_id, version, draft_spec, content_sha256, created_by) \
         VALUES ($1, $2, $3, $4, $5, $6, $7)",
    )
    .bind(revision_id)
    .bind(auth.organization_id)
    .bind(definition_id)
    .bind(Option::<String>::None)
    .bind(draft_spec)
    .bind(content_sha256)
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
    Json(request): Json<SupervisorDraftUpdateRequest>,
) -> ApiResult<SupervisorDefinitionSummary> {
    let draft = request.draft;
    if request.expected_revision < 1 {
        return Err(bad_request("expected_revision must be positive"));
    }
    validate_draft_storage_shape(&draft)?;
    let draft_spec = serde_json::to_value(&draft).map_err(|_| internal_error())?;
    let content_sha256 = draft_content_sha256(&draft);
    let mut transaction = state.db.begin().await.map_err(database_error)?;
    let definition = lock_definition(&mut transaction, auth.organization_id, definition_id).await?;
    require_manage(&auth, definition.get("owner_user_id"))?;
    if definition.get::<String, _>("policy_id") != draft.policy_id {
        return Err(bad_request(
            "A Supervisor Definition policy id is immutable",
        ));
    }
    let draft_revision = sqlx::query(
        "SELECT id, revision_number FROM supervisor_revisions \
         WHERE organization_id = $1 AND definition_id = $2 AND state = 'draft' \
         FOR UPDATE",
    )
    .bind(auth.organization_id)
    .bind(definition_id)
    .fetch_optional(&mut *transaction)
    .await
    .map_err(database_error)?;
    if let Some(row) = draft_revision {
        let current_revision: i64 = row.get("revision_number");
        if current_revision != request.expected_revision {
            return Err((
                StatusCode::CONFLICT,
                Json(PlatformError::bad_request(
                    "Supervisor draft revision is stale; reload before saving",
                )),
            ));
        }
        sqlx::query(
            "UPDATE supervisor_revisions \
             SET draft_spec = $1, content_sha256 = $2, revision_number = revision_number + 1, \
                 updated_at = now() \
             WHERE id = $3 AND organization_id = $4 AND state = 'draft'",
        )
        .bind(draft_spec)
        .bind(&content_sha256)
        .bind(row.get::<Uuid, _>("id"))
        .bind(auth.organization_id)
        .execute(&mut *transaction)
        .await
        .map_err(database_conflict)?;
    } else {
        sqlx::query(
            "INSERT INTO supervisor_revisions \
             (id, organization_id, definition_id, version, draft_spec, content_sha256, created_by) \
             VALUES ($1, $2, $3, $4, $5, $6, $7)",
        )
        .bind(Uuid::now_v7())
        .bind(auth.organization_id)
        .bind(definition_id)
        .bind(Option::<String>::None)
        .bind(draft_spec)
        .bind(content_sha256)
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
    Ok(Json(
        validate_draft(&state.db, auth.organization_id, &draft).await,
    ))
}

pub async fn publish(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(definition_id): Path<Uuid>,
    Json(request): Json<PublishSupervisorDraftRequest>,
) -> ApiResult<SupervisorReleaseSummary> {
    if request.expected_revision < 1 {
        return Err(bad_request("expected_revision must be positive"));
    }
    let mut transaction = state.db.begin().await.map_err(database_error)?;
    let row = sqlx::query(
        "SELECT definition.owner_user_id, definition.policy_id, revision.id AS revision_id, \
                revision.revision_number, revision.draft_spec \
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
    if row.get::<i64, _>("revision_number") != request.expected_revision {
        return Err((
            StatusCode::CONFLICT,
            Json(PlatformError::bad_request(
                "Supervisor draft revision is stale; reload before publishing",
            )),
        ));
    }
    let draft = parse_draft(row.get("draft_spec"))?;
    if row.get::<String, _>("policy_id") != draft.policy_id {
        return Err(internal_error());
    }
    let available_agents = agent_catalog::list_resolved(&state.db, auth.organization_id)
        .await
        .map_err(|_| internal_error())?;
    let instruction_policy =
        crate::supervisor_instruction_policy::resolve(&state.db, &draft.instruction_policy)
            .await
            .map_err(|_| {
                (
                    StatusCode::UNPROCESSABLE_ENTITY,
                    Json(PlatformError::bad_request(
                        "The selected platform instruction policy is unavailable",
                    )),
                )
            })?;
    // Serialize release allocation for one policy even when separate Definitions
    // are published concurrently.
    sqlx::query("SELECT pg_advisory_xact_lock(hashtext($1))")
        .bind(&draft.policy_id)
        .execute(&mut *transaction)
        .await
        .map_err(database_error)?;
    let release_version =
        next_release_version(&mut transaction, auth.organization_id, &draft.policy_id).await?;
    let release_spec = release_spec_from_draft_with_version(
        &draft,
        &available_agents,
        &instruction_policy,
        &release_version,
    )
    .map_err(|issue| {
        (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(PlatformError::bad_request(issue.message)),
        )
    })?;
    let package = supervisor::validate_release_with_agents_and_policy(
        release_spec.clone(),
        &available_agents,
        &instruction_policy,
    )
    .map_err(|error| {
        (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(PlatformError::bad_request(error.to_string())),
        )
    })?;
    let release_id = Uuid::now_v7();
    let revision_id: Uuid = row.get("revision_id");
    let release_spec_value = serde_json::to_value(&release_spec).map_err(|_| internal_error())?;
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
    for reference in &release_spec.agents {
        let Some(agent_release_id) = reference.release_id else {
            continue;
        };
        let definition = available_agents
            .iter()
            .find(|definition| definition.release_id == Some(agent_release_id))
            .ok_or_else(internal_error)?;
        sqlx::query(
            "INSERT INTO supervisor_release_agent_dependencies \
             (organization_id, supervisor_release_id, agent_release_id, \
              agent_definition_id, agent_version, agent_content_sha256) \
             VALUES ($1, $2, $3, $4, $5, $6)",
        )
        .bind(auth.organization_id)
        .bind(release_id)
        .bind(agent_release_id)
        .bind(&reference.definition_id)
        .bind(&reference.version)
        .bind(&definition.content_sha256)
        .execute(&mut *transaction)
        .await
        .map_err(database_conflict)?;
    }
    sqlx::query(
        "UPDATE supervisor_revisions \
         SET state = 'published', version = $1, published_at = $2, updated_at = $2 \
         WHERE id = $3 AND organization_id = $4 AND state = 'draft'",
    )
    .bind(&package.version)
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

async fn next_release_version(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    organization_id: Uuid,
    policy_id: &str,
) -> Result<String, ApiError> {
    let versions = sqlx::query_scalar::<_, String>(
        "SELECT version FROM supervisor_releases \
         WHERE organization_id = $1 AND policy_id = $2",
    )
    .bind(organization_id)
    .bind(policy_id)
    .fetch_all(&mut **transaction)
    .await
    .map_err(database_error)?;
    if versions.is_empty() {
        return Ok("1.0.0".to_string());
    }
    let mut latest = (1_u64, 0_u64, 0_u64);
    for version in versions {
        let mut parts = version.split('.');
        let Some(major) = parts.next().and_then(|part| part.parse::<u64>().ok()) else {
            return Err(internal_error());
        };
        let Some(minor) = parts.next().and_then(|part| part.parse::<u64>().ok()) else {
            return Err(internal_error());
        };
        let Some(patch) = parts.next().and_then(|part| part.parse::<u64>().ok()) else {
            return Err(internal_error());
        };
        if parts.next().is_some() {
            return Err(internal_error());
        }
        latest = latest.max((major, minor, patch));
    }
    Ok(format!("{}.{}.{}", latest.0, latest.1, latest.2 + 1))
}

/// Idempotently publish a trusted, server-authored Supervisor draft through
/// the same semantic compiler and immutable persistence path as Web authoring.
pub(crate) async fn publish_or_reuse_trusted(
    state: &AppState,
    auth: &AuthenticatedUser,
    draft: SupervisorDraftRequest,
) -> Result<SupervisorReleaseSummary, ApiError> {
    validate_draft_storage_shape(&draft)?;
    let available_agents = agent_catalog::list_resolved(&state.db, auth.organization_id)
        .await
        .map_err(|_| internal_error())?;
    let instruction_policy =
        crate::supervisor_instruction_policy::resolve(&state.db, &draft.instruction_policy)
            .await
            .map_err(|_| {
                (
                    StatusCode::UNPROCESSABLE_ENTITY,
                    Json(PlatformError::bad_request(
                        "The selected platform instruction policy is unavailable",
                    )),
                )
            })?;
    let release_spec =
        release_spec_from_draft_with_policy(&draft, &available_agents, &instruction_policy)
            .map_err(|issue| {
                (
                    StatusCode::UNPROCESSABLE_ENTITY,
                    Json(PlatformError::bad_request(issue.message)),
                )
            })?;
    let expected = supervisor::validate_release_with_agents_and_policy(
        release_spec,
        &available_agents,
        &instruction_policy,
    )
    .map_err(|error| {
        (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(PlatformError::bad_request(error.to_string())),
        )
    })?;

    if let Some(row) = sqlx::query(
        "SELECT id, policy_id, version, display_name, description, content_sha256, published_at \
         FROM supervisor_releases \
         WHERE organization_id = $1 AND policy_id = $2 AND content_sha256 = $3 \
         ORDER BY published_at DESC LIMIT 1",
    )
    .bind(auth.organization_id)
    .bind(&draft.policy_id)
    .bind(&expected.content_sha256)
    .fetch_optional(&state.db)
    .await
    .map_err(database_error)?
    {
        let release_id: Uuid = row.get("id");
        let resolved =
            supervisor_policy::resolve_release(&state.db, auth.organization_id, release_id)
                .await
                .map_err(|_| internal_error())?;
        if resolved.snapshot.content_sha256 != expected.content_sha256 {
            return Err((
                StatusCode::CONFLICT,
                Json(PlatformError::bad_request(
                    "The tutorial Supervisor identity is bound to different immutable content",
                )),
            ));
        }
        return Ok(SupervisorReleaseSummary {
            id: release_id,
            policy_id: row.get("policy_id"),
            version: row.get("version"),
            display_name: row.get("display_name"),
            description: row.get("description"),
            content_sha256: row.get("content_sha256"),
            published_at: row.get("published_at"),
        });
    }

    let resources = load_definitions(&state.db, auth.organization_id).await?;
    let resource_id = if let Some(existing) = resources
        .into_iter()
        .find(|resource| resource.policy_id == draft.policy_id)
    {
        if existing.draft.as_ref() != Some(&draft) || existing.owner_user_id != auth.user_id {
            return Err((
                StatusCode::CONFLICT,
                Json(PlatformError::bad_request(
                    "The tutorial Supervisor Definition id is already in use",
                )),
            ));
        }
        existing.id
    } else {
        create(State(state.clone()), auth.clone(), Json(draft.clone()))
            .await?
            .0
            .id
    };
    let expected_revision = sqlx::query_scalar::<_, i64>(
        "SELECT revision_number FROM supervisor_revisions \
         WHERE organization_id = $1 AND definition_id = $2 AND state = 'draft'",
    )
    .bind(auth.organization_id)
    .bind(resource_id)
    .fetch_one(&state.db)
    .await
    .map_err(database_error)?;
    publish(
        State(state.clone()),
        auth.clone(),
        Path(resource_id),
        Json(PublishSupervisorDraftRequest { expected_revision }),
    )
    .await
    .map(|Json(release)| release)
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
