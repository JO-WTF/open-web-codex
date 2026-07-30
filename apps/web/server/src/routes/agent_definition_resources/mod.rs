use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use open_web_codex_platform_contracts::error::PlatformError;
use open_web_codex_platform_contracts::{
    AgentCapabilityTemplateSource, AgentDatasetReleaseBinding, AgentDefinitionDraftRequest,
    AgentDefinitionReleaseSummary, AgentDefinitionResourceSummary, AgentDefinitionValidationIssue,
    AgentDefinitionValidationResult,
};
use open_web_codex_platform_store::AppState;
use open_web_codex_supervisor_catalog::agent::{self, AgentReleaseSpec, ResolvedAgentDefinition};
use sqlx::Row;
use uuid::Uuid;

use crate::agent_catalog;
use crate::middleware::auth::AuthenticatedUser;

mod store;

use store::{
    load_definition, load_definitions, load_draft_row, lock_definition, parse_draft, record_audit,
};

pub(crate) type ApiError = (StatusCode, Json<PlatformError>);
type ApiResult<T> = Result<Json<T>, ApiError>;

pub async fn list(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
) -> ApiResult<Vec<AgentDefinitionResourceSummary>> {
    load_definitions(&state.db, auth.organization_id)
        .await
        .map(Json)
}

pub async fn create(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Json(draft): Json<AgentDefinitionDraftRequest>,
) -> ApiResult<AgentDefinitionResourceSummary> {
    validate_draft_storage_shape(&draft)?;
    resolve_authorized_dependencies(&state.db, &auth, &draft).await?;
    if agent_catalog::is_reserved_builtin_definition_id(&draft.definition_id) {
        return Err(bad_request(
            "The definition id is reserved by a built-in Agent Definition",
        ));
    }
    let definition_id = Uuid::now_v7();
    let revision_id = Uuid::now_v7();
    let draft_spec = serde_json::to_value(&draft).map_err(|_| internal_error())?;
    let mut transaction = state.db.begin().await.map_err(database_error)?;
    sqlx::query(
        "INSERT INTO agent_definitions \
         (id, organization_id, owner_user_id, definition_id, display_name, description) \
         VALUES ($1, $2, $3, $4, $5, $6)",
    )
    .bind(definition_id)
    .bind(auth.organization_id)
    .bind(auth.user_id)
    .bind(&draft.definition_id)
    .bind(&draft.display_name)
    .bind(&draft.description)
    .execute(&mut *transaction)
    .await
    .map_err(database_conflict)?;
    sqlx::query(
        "INSERT INTO agent_definition_revisions \
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
        "agent_definition.created",
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
    Json(draft): Json<AgentDefinitionDraftRequest>,
) -> ApiResult<AgentDefinitionResourceSummary> {
    validate_draft_storage_shape(&draft)?;
    resolve_authorized_dependencies(&state.db, &auth, &draft).await?;
    let draft_spec = serde_json::to_value(&draft).map_err(|_| internal_error())?;
    let mut transaction = state.db.begin().await.map_err(database_error)?;
    let definition = lock_definition(&mut transaction, auth.organization_id, definition_id).await?;
    require_manage(&auth, definition.get("owner_user_id"))?;
    if definition.get::<String, _>("definition_id") != draft.definition_id {
        return Err(bad_request("An Agent Definition id is immutable"));
    }
    let revision = sqlx::query(
        "SELECT id FROM agent_definition_revisions \
         WHERE organization_id = $1 AND definition_id = $2 AND state = 'draft' FOR UPDATE",
    )
    .bind(auth.organization_id)
    .bind(definition_id)
    .fetch_optional(&mut *transaction)
    .await
    .map_err(database_error)?;
    if let Some(row) = revision {
        sqlx::query(
            "UPDATE agent_definition_revisions \
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
            "INSERT INTO agent_definition_revisions \
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
        "UPDATE agent_definitions \
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
        "agent_definition.draft.saved",
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
) -> ApiResult<AgentDefinitionValidationResult> {
    let row = load_draft_row(&state.db, auth.organization_id, definition_id).await?;
    require_manage(&auth, row.get("owner_user_id"))?;
    let draft = parse_draft(row.get("draft_spec"))?;
    let dependencies = resolve_authorized_dependencies(&state.db, &auth, &draft).await?;
    Ok(Json(validate_draft(
        &draft,
        &dependencies.template,
        dependencies.dataset_releases,
    )))
}

pub async fn publish(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(definition_id): Path<Uuid>,
) -> ApiResult<AgentDefinitionReleaseSummary> {
    let mut transaction = state.db.begin().await.map_err(database_error)?;
    let row = sqlx::query(
        "SELECT definition.owner_user_id, definition.definition_id AS catalog_id, \
                revision.id AS revision_id, revision.draft_spec \
         FROM agent_definitions definition \
         JOIN agent_definition_revisions revision ON revision.definition_id = definition.id \
           AND revision.organization_id = definition.organization_id AND revision.state = 'draft' \
         WHERE definition.id = $1 AND definition.organization_id = $2 \
         FOR UPDATE OF definition, revision",
    )
    .bind(definition_id)
    .bind(auth.organization_id)
    .fetch_optional(&mut *transaction)
    .await
    .map_err(database_error)?
    .ok_or_else(|| not_found("Agent Definition draft was not found"))?;
    require_manage(&auth, row.get("owner_user_id"))?;
    let draft = parse_draft(row.get("draft_spec"))?;
    if row.get::<String, _>("catalog_id") != draft.definition_id {
        return Err(internal_error());
    }
    let dependencies = resolve_authorized_dependencies(&state.db, &auth, &draft).await?;
    let template = dependencies.template;
    let spec = release_spec_from_draft(&draft, dependencies.dataset_releases);
    let runtime_role = agent::user_runtime_role_name(&draft.definition_id, &draft.version)
        .map_err(catalog_error)?;
    let resolved = agent::compile_agent_release_against_template(spec.clone(), &template)
        .map_err(catalog_error)?;
    let release_id = Uuid::now_v7();
    let revision_id: Uuid = row.get("revision_id");
    let release_spec = serde_json::to_value(&spec).map_err(|_| internal_error())?;
    let published_at = chrono::Utc::now();
    sqlx::query(
        "INSERT INTO agent_definition_releases \
         (id, organization_id, definition_id, revision_id, catalog_id, version, display_name, \
          description, release_spec, runtime_role, content_sha256, published_by, published_at) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)",
    )
    .bind(release_id)
    .bind(auth.organization_id)
    .bind(definition_id)
    .bind(revision_id)
    .bind(&resolved.definition_id)
    .bind(&resolved.version)
    .bind(&resolved.display_name)
    .bind(&resolved.description)
    .bind(release_spec)
    .bind(&runtime_role)
    .bind(&resolved.content_sha256)
    .bind(auth.user_id)
    .bind(published_at)
    .execute(&mut *transaction)
    .await
    .map_err(database_conflict)?;
    if spec.capability_template.source == AgentCapabilityTemplateSource::WorkspacePackageRelease {
        let package_release_id = spec
            .capability_template
            .release_id
            .ok_or_else(internal_error)?;
        let workspace_id = template
            .capability_workspace_id
            .ok_or_else(internal_error)?;
        sqlx::query(
            "INSERT INTO agent_release_capability_package_dependencies \
             (organization_id, agent_release_id, package_release_id, workspace_id, package_id, \
              package_version, package_content_sha256) \
             VALUES ($1, $2, $3, $4, $5, $6, $7)",
        )
        .bind(auth.organization_id)
        .bind(release_id)
        .bind(package_release_id)
        .bind(workspace_id)
        .bind(&spec.capability_template.definition_id)
        .bind(&spec.capability_template.version)
        .bind(&template.capability_template_sha256)
        .execute(&mut *transaction)
        .await
        .map_err(database_conflict)?;
    }
    for dataset in &spec.dataset_releases {
        sqlx::query(
            "INSERT INTO agent_release_dataset_dependencies \
             (organization_id, agent_release_id, dataset_release_id, workspace_id, dataset_id, \
              dataset_version, dataset_content_sha256) \
             VALUES ($1, $2, $3, $4, $5, $6, $7)",
        )
        .bind(auth.organization_id)
        .bind(release_id)
        .bind(dataset.release_id)
        .bind(dataset.workspace_id)
        .bind(&dataset.dataset_id)
        .bind(&dataset.version)
        .bind(&dataset.content_sha256)
        .execute(&mut *transaction)
        .await
        .map_err(database_conflict)?;
    }
    sqlx::query(
        "UPDATE agent_definition_revisions \
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
        "UPDATE agent_definitions SET updated_at = $1 \
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
        "agent_definition.release.published",
        release_id,
    )
    .await?;
    transaction.commit().await.map_err(database_error)?;
    Ok(Json(AgentDefinitionReleaseSummary {
        id: release_id,
        definition_id: resolved.definition_id,
        version: resolved.version,
        display_name: resolved.display_name,
        description: resolved.description,
        content_sha256: resolved.content_sha256,
        published_at,
    }))
}

/// Idempotently publish a trusted, server-authored Agent draft through the
/// same validation and persistence owner used by the Web authoring routes.
/// Exact identity/content is reused; any collision is explicit.
pub(crate) async fn publish_or_reuse_trusted(
    state: &AppState,
    auth: &AuthenticatedUser,
    draft: AgentDefinitionDraftRequest,
) -> Result<AgentDefinitionReleaseSummary, ApiError> {
    validate_draft_storage_shape(&draft)?;
    let dependencies = resolve_authorized_dependencies(&state.db, auth, &draft).await?;
    let expected = agent::compile_agent_release_against_template(
        release_spec_from_draft(&draft, dependencies.dataset_releases),
        &dependencies.template,
    )
    .map_err(catalog_error)?;

    if let Some(row) = sqlx::query(
        "SELECT id, catalog_id, version, display_name, description, content_sha256, published_at \
         FROM agent_definition_releases \
         WHERE organization_id = $1 AND catalog_id = $2 AND version = $3",
    )
    .bind(auth.organization_id)
    .bind(&draft.definition_id)
    .bind(&draft.version)
    .fetch_optional(&state.db)
    .await
    .map_err(database_error)?
    {
        let release_id: Uuid = row.get("id");
        let resolved = agent_catalog::resolve_run_selection(
            &state.db,
            auth.organization_id,
            &open_web_codex_platform_contracts::AgentRunSelection {
                definition_id: draft.definition_id.clone(),
                version: draft.version.clone(),
                release_id: Some(release_id),
            },
        )
        .await
        .map_err(capability_template_error)?;
        if resolved.content_sha256 != expected.content_sha256 {
            return Err((
                StatusCode::CONFLICT,
                Json(PlatformError::bad_request(
                    "The tutorial Agent identity is bound to different immutable content",
                )),
            ));
        }
        return Ok(AgentDefinitionReleaseSummary {
            id: release_id,
            definition_id: row.get("catalog_id"),
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
        .find(|resource| resource.definition_id == draft.definition_id)
    {
        if existing.draft.as_ref() != Some(&draft) || existing.owner_user_id != auth.user_id {
            return Err((
                StatusCode::CONFLICT,
                Json(PlatformError::bad_request(
                    "The tutorial Agent Definition id is already in use",
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
    publish(State(state.clone()), auth.clone(), Path(resource_id))
        .await
        .map(|Json(release)| release)
}

fn validate_draft(
    draft: &AgentDefinitionDraftRequest,
    template: &ResolvedAgentDefinition,
    dataset_releases: Vec<AgentDatasetReleaseBinding>,
) -> AgentDefinitionValidationResult {
    let result = agent::compile_agent_release_against_template(
        release_spec_from_draft(draft, dataset_releases),
        template,
    );
    match result {
        Ok(resolved) => AgentDefinitionValidationResult {
            valid: true,
            execution_semantics_sha256: Some(resolved.execution_semantics_sha256()),
            content_sha256: Some(resolved.content_sha256),
            issues: Vec::new(),
        },
        Err(_) => AgentDefinitionValidationResult {
            valid: false,
            content_sha256: None,
            execution_semantics_sha256: None,
            issues: vec![validation_issue(
                "invalid_agent_definition",
                "Agent Definition does not match the selected capability template",
            )],
        },
    }
}

struct ResolvedDraftDependencies {
    template: ResolvedAgentDefinition,
    dataset_releases: Vec<AgentDatasetReleaseBinding>,
}

async fn resolve_authorized_dependencies(
    db: &sqlx::PgPool,
    auth: &AuthenticatedUser,
    draft: &AgentDefinitionDraftRequest,
) -> Result<ResolvedDraftDependencies, ApiError> {
    let template = resolve_authorized_capability_template(db, auth, draft).await?;
    let dataset_releases =
        resolve_authorized_dataset_releases(db, auth, &draft.dataset_release_ids).await?;
    let mut required_workspace_id = template.capability_workspace_id;
    for release in &dataset_releases {
        if required_workspace_id.is_some_and(|workspace_id| workspace_id != release.workspace_id) {
            return Err(bad_request(
                "An Agent's capability package and Dataset Releases must belong to one Workspace",
            ));
        }
        required_workspace_id = Some(release.workspace_id);
    }
    Ok(ResolvedDraftDependencies {
        template,
        dataset_releases,
    })
}

async fn resolve_authorized_capability_template(
    db: &sqlx::PgPool,
    auth: &AuthenticatedUser,
    draft: &AgentDefinitionDraftRequest,
) -> Result<ResolvedAgentDefinition, ApiError> {
    let template = agent_catalog::resolve_capability_template(
        db,
        auth.organization_id,
        &draft.capability_template,
    )
    .await
    .map_err(capability_template_error)?;
    if let Some(workspace_id) = template.capability_workspace_id {
        if !matches!(auth.organization_role.as_str(), "owner" | "admin") {
            let authorized = sqlx::query_scalar::<_, bool>(
                "SELECT EXISTS ( \
                     SELECT 1 FROM workspace_grants \
                     WHERE organization_id = $1 AND workspace_id = $2 AND user_id = $3 \
                 )",
            )
            .bind(auth.organization_id)
            .bind(workspace_id)
            .bind(auth.user_id)
            .fetch_one(db)
            .await
            .map_err(database_error)?;
            if !authorized {
                return Err((
                    StatusCode::FORBIDDEN,
                    Json(PlatformError::forbidden(
                        "The selected capability package belongs to an unauthorized Workspace",
                    )),
                ));
            }
        }
    }
    Ok(template)
}

async fn resolve_authorized_dataset_releases(
    db: &sqlx::PgPool,
    auth: &AuthenticatedUser,
    release_ids: &[Uuid],
) -> Result<Vec<AgentDatasetReleaseBinding>, ApiError> {
    if release_ids.is_empty() {
        return Ok(Vec::new());
    }
    let is_admin = matches!(auth.organization_role.as_str(), "owner" | "admin");
    let rows = sqlx::query(
        "SELECT release.id, release.workspace_id, release.dataset_id, release.version, \
                release.display_name, release.content_sha256 \
         FROM workspace_dataset_releases release \
         WHERE release.organization_id = $1 AND release.state = 'published' \
           AND release.id = ANY($2) \
           AND ($4 OR EXISTS ( \
               SELECT 1 FROM workspace_grants workspace_grant \
               WHERE workspace_grant.organization_id = release.organization_id \
                 AND workspace_grant.workspace_id = release.workspace_id \
                 AND workspace_grant.user_id = $3 \
           ))",
    )
    .bind(auth.organization_id)
    .bind(release_ids)
    .bind(auth.user_id)
    .bind(is_admin)
    .fetch_all(db)
    .await
    .map_err(database_error)?;
    let by_id = rows
        .into_iter()
        .map(|row| {
            let binding = AgentDatasetReleaseBinding {
                release_id: row.get("id"),
                workspace_id: row.get("workspace_id"),
                dataset_id: row.get("dataset_id"),
                version: row.get("version"),
                display_name: row.get("display_name"),
                content_sha256: row.get("content_sha256"),
            };
            (binding.release_id, binding)
        })
        .collect::<std::collections::HashMap<_, _>>();
    if by_id.len() != release_ids.len() {
        return Err((
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(PlatformError::bad_request(
                "One or more selected Dataset Releases are unavailable",
            )),
        ));
    }
    release_ids
        .iter()
        .map(|release_id| {
            by_id
                .get(release_id)
                .cloned()
                .ok_or_else(|| internal_error())
        })
        .collect()
}

fn release_spec_from_draft(
    draft: &AgentDefinitionDraftRequest,
    dataset_releases: Vec<AgentDatasetReleaseBinding>,
) -> AgentReleaseSpec {
    AgentReleaseSpec {
        definition_id: draft.definition_id.clone(),
        version: draft.version.clone(),
        display_name: draft.display_name.clone(),
        description: draft.description.clone(),
        responsibilities: draft.responsibilities.clone(),
        developer_instructions: draft.developer_instructions.clone(),
        input_artifact_types: draft.input_artifact_types.clone(),
        output_artifact_types: draft.output_artifact_types.clone(),
        capability_template: draft.capability_template.clone(),
        dataset_releases,
    }
}

fn validate_draft_storage_shape(draft: &AgentDefinitionDraftRequest) -> Result<(), ApiError> {
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
    let valid_artifact = |value: &str| {
        value.len() <= 128
            && value.contains('.')
            && value.bytes().all(|byte| {
                byte.is_ascii_lowercase()
                    || byte.is_ascii_digit()
                    || matches!(byte, b'-' | b'_' | b'.')
            })
    };
    if !safe_identifier(&draft.definition_id, false, 96)
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
        || draft.input_artifact_types.len() > 32
        || draft.output_artifact_types.len() > 32
        || draft.dataset_release_ids.len() > 16
        || draft
            .input_artifact_types
            .iter()
            .chain(draft.output_artifact_types.iter())
            .any(|value| !valid_artifact(value))
        || !safe_identifier(&draft.capability_template.definition_id, false, 96)
        || !safe_identifier(&draft.capability_template.version, true, 64)
        || matches!(
            draft.capability_template.source,
            AgentCapabilityTemplateSource::RepositoryAgent
        ) != draft.capability_template.release_id.is_none()
        || {
            let mut release_ids = std::collections::HashSet::new();
            draft
                .dataset_release_ids
                .iter()
                .any(|release_id| !release_ids.insert(*release_id))
        }
    {
        return Err(bad_request("Agent Definition draft fields are invalid"));
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
                "Agent Definition is owned by another user",
            )),
        ))
    }
}

fn validation_issue(code: &str, message: &str) -> AgentDefinitionValidationIssue {
    AgentDefinitionValidationIssue {
        code: code.to_string(),
        message: message.to_string(),
    }
}

fn catalog_error(_error: agent::AgentCatalogError) -> ApiError {
    (
        StatusCode::UNPROCESSABLE_ENTITY,
        Json(PlatformError::bad_request(
            "Agent Definition does not match a published capability template",
        )),
    )
}

fn capability_template_error(error: agent_catalog::AgentCatalogError) -> ApiError {
    match error {
        agent_catalog::AgentCatalogError::Database => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(PlatformError::internal(
                "Agent capability package lookup failed",
            )),
        ),
        agent_catalog::AgentCatalogError::NotFound => (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(PlatformError::bad_request(
                "The selected capability package release was not found",
            )),
        ),
        agent_catalog::AgentCatalogError::Invalid => {
            catalog_error(agent::AgentCatalogError::Invalid)
        }
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
            "Agent Definition storage is invalid",
        )),
    )
}

fn database_error(_error: sqlx::Error) -> ApiError {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(PlatformError::internal(
            "Agent Definition database operation failed",
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
                "Agent Definition id or version already exists",
            )),
        )
    } else {
        database_error(error)
    }
}

#[cfg(test)]
mod tests;
