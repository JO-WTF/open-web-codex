//! Generic Catalog and Installation API used by Tool, Skill, Agent, Supervisor
//! and Copilot Studio. The API owns Draft revisions and Release identity; it
//! never accepts Runtime role names, MCP commands or host paths from a browser.

use std::collections::BTreeMap;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Extension, Json,
};
use open_web_codex_capability_catalog::{compile_draft, next_patch_version};
use open_web_codex_git_runtime::GitRuntime;
use open_web_codex_platform_contracts::error::PlatformError;
use open_web_codex_platform_contracts::{
    CapabilityDraftDetail, CapabilityDraftSummary, CapabilityInstallationSummary,
    CapabilityReadinessSummary, CapabilityReleaseSummary, CapabilityValidationResult,
    CatalogDependency, CatalogDraftContent, CatalogResourceKind, CreateCapabilityDraftRequest,
    InstallCapabilityReleaseRequest, PublishCapabilityDraftRequest, SaveCapabilityDraftRequest,
};
use open_web_codex_platform_store::AppState;
use serde_json::json;
use sqlx::Row;
use uuid::Uuid;

use crate::middleware::auth::AuthenticatedUser;
use crate::routes::workspaces::authorized_workspace;

type ApiError = (StatusCode, Json<PlatformError>);
type ApiResult<T> = Result<Json<T>, ApiError>;

pub async fn list_drafts(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
) -> ApiResult<Vec<CapabilityDraftSummary>> {
    let rows = sqlx::query(
        "SELECT id, kind, resource_id, display_name, description, revision,
                content_sha256, validation_state, updated_at
         FROM capability_catalog_drafts
         WHERE organization_id = $1
         ORDER BY updated_at DESC, id",
    )
    .bind(auth.organization_id)
    .fetch_all(&state.db)
    .await
    .map_err(database_error)?;
    rows.into_iter()
        .map(draft_summary)
        .collect::<Result<Vec<_>, _>>()
        .map(Json)
}

pub async fn create(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Json(request): Json<CreateCapabilityDraftRequest>,
) -> ApiResult<CapabilityDraftDetail> {
    validate_metadata(&request.display_name, &request.description)?;
    let compiled = compile_draft(request.kind, &request.resource_id, request.content)
        .map_err(|error| bad_request(error.to_string()))?;
    let content = serde_json::to_value(&compiled.content).map_err(|_| internal_error())?;
    let id = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO capability_catalog_drafts
         (id, organization_id, owner_user_id, kind, resource_id, display_name,
          description, content, content_sha256, validation_state)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, 'valid')",
    )
    .bind(id)
    .bind(auth.organization_id)
    .bind(auth.user_id)
    .bind(kind_name(request.kind))
    .bind(&request.resource_id)
    .bind(request.display_name.trim())
    .bind(request.description.trim())
    .bind(content)
    .bind(&compiled.content_sha256)
    .execute(&state.db)
    .await
    .map_err(database_conflict)?;
    load_draft(&state.db, auth.organization_id, id)
        .await
        .map(Json)
}

pub async fn get(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(id): Path<Uuid>,
) -> ApiResult<CapabilityDraftDetail> {
    load_draft(&state.db, auth.organization_id, id)
        .await
        .map(Json)
}

pub async fn save(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(id): Path<Uuid>,
    Json(request): Json<SaveCapabilityDraftRequest>,
) -> ApiResult<CapabilityDraftDetail> {
    let row = sqlx::query(
        "SELECT owner_user_id, kind, resource_id FROM capability_catalog_drafts
         WHERE id = $1 AND organization_id = $2",
    )
    .bind(id)
    .bind(auth.organization_id)
    .fetch_optional(&state.db)
    .await
    .map_err(database_error)?
    .ok_or_else(|| not_found("Capability Draft was not found"))?;
    require_manage(&auth, row.get("owner_user_id"))?;
    validate_metadata(&request.display_name, &request.description)?;
    if request.expected_revision < 1 {
        return Err(bad_request("expected_revision must be positive"));
    }
    let kind = parse_kind(row.get("kind"))?;
    let resource_id: String = row.get("resource_id");
    let compiled = compile_draft(kind, &resource_id, request.content)
        .map_err(|error| bad_request(error.to_string()))?;
    let content = serde_json::to_value(&compiled.content).map_err(|_| internal_error())?;
    let updated = sqlx::query(
        "UPDATE capability_catalog_drafts
         SET display_name = $1, description = $2, content = $3,
             content_sha256 = $4, validation_state = 'valid',
             revision = revision + 1, updated_at = now()
         WHERE id = $5 AND organization_id = $6 AND revision = $7
         RETURNING id",
    )
    .bind(request.display_name.trim())
    .bind(request.description.trim())
    .bind(content)
    .bind(&compiled.content_sha256)
    .bind(id)
    .bind(auth.organization_id)
    .bind(request.expected_revision)
    .fetch_optional(&state.db)
    .await
    .map_err(database_error)?;
    if updated.is_none() {
        return Err(conflict("Capability Draft changed; reload before saving"));
    }
    load_draft(&state.db, auth.organization_id, id)
        .await
        .map(Json)
}

pub async fn validate(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(id): Path<Uuid>,
) -> ApiResult<CapabilityValidationResult> {
    let row = sqlx::query(
        "SELECT owner_user_id, kind, resource_id, content FROM capability_catalog_drafts
         WHERE id = $1 AND organization_id = $2",
    )
    .bind(id)
    .bind(auth.organization_id)
    .fetch_optional(&state.db)
    .await
    .map_err(database_error)?
    .ok_or_else(|| not_found("Capability Draft was not found"))?;
    require_manage(&auth, row.get("owner_user_id"))?;
    let kind = parse_kind(row.get("kind"))?;
    let resource_id: String = row.get("resource_id");
    let content: CatalogDraftContent =
        serde_json::from_value(row.get("content")).map_err(|_| internal_error())?;
    match compile_draft(kind, &resource_id, content) {
        Ok(compiled) => {
            sqlx::query(
                "UPDATE capability_catalog_drafts SET validation_state = 'valid', updated_at = now()
                 WHERE id = $1 AND organization_id = $2",
            )
            .bind(id)
            .bind(auth.organization_id)
            .execute(&state.db)
            .await
            .map_err(database_error)?;
            Ok(Json(CapabilityValidationResult {
                valid: true,
                issues: Vec::new(),
                content_sha256: Some(compiled.content_sha256),
                execution_semantics_sha256: Some(compiled.execution_semantics_sha256),
            }))
        }
        Err(error) => {
            sqlx::query(
                "UPDATE capability_catalog_drafts SET validation_state = 'invalid', updated_at = now()
                 WHERE id = $1 AND organization_id = $2",
            )
            .bind(id)
            .bind(auth.organization_id)
            .execute(&state.db)
            .await
            .map_err(database_error)?;
            Ok(Json(CapabilityValidationResult {
                valid: false,
                issues: vec![error.to_string()],
                content_sha256: None,
                execution_semantics_sha256: None,
            }))
        }
    }
}

pub async fn publish(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(id): Path<Uuid>,
    Json(request): Json<PublishCapabilityDraftRequest>,
) -> ApiResult<CapabilityReleaseSummary> {
    if request.expected_revision < 1 {
        return Err(bad_request("expected_revision must be positive"));
    }
    let mut transaction = state.db.begin().await.map_err(database_error)?;
    let row = sqlx::query(
        "SELECT draft.owner_user_id, draft.kind, draft.resource_id, draft.display_name,
                draft.description, draft.revision, draft.content
         FROM capability_catalog_drafts draft
         WHERE draft.id = $1 AND draft.organization_id = $2
         FOR UPDATE",
    )
    .bind(id)
    .bind(auth.organization_id)
    .fetch_optional(&mut *transaction)
    .await
    .map_err(database_error)?
    .ok_or_else(|| not_found("Capability Draft was not found"))?;
    require_manage(&auth, row.get("owner_user_id"))?;
    let revision: i64 = row.get("revision");
    if revision != request.expected_revision {
        return Err(conflict(
            "Capability Draft changed; reload before publishing",
        ));
    }
    let kind = parse_kind(row.get("kind"))?;
    let resource_id: String = row.get("resource_id");
    let content: CatalogDraftContent =
        serde_json::from_value(row.get("content")).map_err(|_| internal_error())?;
    let compiled = compile_draft(kind, &resource_id, content)
        .map_err(|error| unprocessable(error.to_string()))?;
    let dependencies = compiled.content.dependencies.clone();
    let resolved_dependencies =
        resolve_dependencies(&mut transaction, auth.organization_id, dependencies).await?;
    let mut normalized_content = compiled.content;
    normalized_content.dependencies = resolved_dependencies;
    let compiled = compile_draft(kind, &resource_id, normalized_content)
        .map_err(|error| unprocessable(error.to_string()))?;
    // Serialize the lock after the dependency identities are resolved, so two
    // concurrent publishers cannot allocate the same patch version.
    sqlx::query("SELECT pg_advisory_xact_lock(hashtext($1))")
        .bind(format!(
            "catalog:{}:{}:{}",
            auth.organization_id,
            kind_name(kind),
            resource_id
        ))
        .execute(&mut *transaction)
        .await
        .map_err(database_error)?;
    let version_rows = sqlx::query(
        "SELECT release_version FROM capability_catalog_releases
         WHERE organization_id = $1 AND kind = $2 AND resource_id = $3
         ORDER BY published_at DESC",
    )
    .bind(auth.organization_id)
    .bind(kind_name(kind))
    .bind(&resource_id)
    .fetch_all(&mut *transaction)
    .await
    .map_err(database_error)?;
    let versions = version_rows
        .into_iter()
        .map(|row| row.get::<String, _>("release_version"))
        .collect::<Vec<_>>();
    let release_version = next_patch_version(&versions);
    let release_id = Uuid::now_v7();
    let published_at = chrono::Utc::now();
    let release_content = serde_json::to_value(&compiled.content).map_err(|_| internal_error())?;
    sqlx::query(
        "INSERT INTO capability_catalog_releases
         (id, organization_id, draft_id, kind, resource_id, release_version,
          display_name, description, content, content_sha256,
          execution_semantics_sha256, published_by, published_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)",
    )
    .bind(release_id)
    .bind(auth.organization_id)
    .bind(id)
    .bind(kind_name(kind))
    .bind(&resource_id)
    .bind(&release_version)
    .bind(row.get::<String, _>("display_name"))
    .bind(row.get::<String, _>("description"))
    .bind(release_content)
    .bind(&compiled.content_sha256)
    .bind(&compiled.execution_semantics_sha256)
    .bind(auth.user_id)
    .bind(published_at)
    .execute(&mut *transaction)
    .await
    .map_err(database_conflict)?;
    for dependency in &compiled.content.dependencies {
        let dependency_release_id = dependency.release_id.ok_or_else(|| internal_error())?;
        sqlx::query(
            "INSERT INTO capability_catalog_release_dependencies
             (release_id, dependency_kind, dependency_resource_id,
              dependency_release_id, dependency_release_version)
             VALUES ($1, $2, $3, $4, $5)",
        )
        .bind(release_id)
        .bind(kind_name(dependency.kind))
        .bind(&dependency.resource_id)
        .bind(dependency_release_id)
        .bind(&dependency.release_version)
        .execute(&mut *transaction)
        .await
        .map_err(database_conflict)?;
    }
    sqlx::query(
        "UPDATE capability_catalog_drafts SET validation_state = 'valid', updated_at = now()
         WHERE id = $1 AND organization_id = $2",
    )
    .bind(id)
    .bind(auth.organization_id)
    .execute(&mut *transaction)
    .await
    .map_err(database_error)?;
    transaction.commit().await.map_err(database_error)?;
    Ok(Json(open_web_codex_capability_catalog::release_summary(
        release_id,
        kind,
        resource_id,
        release_version,
        row.get("display_name"),
        row.get("description"),
        compiled.content_sha256,
        compiled.execution_semantics_sha256,
        published_at,
    )))
}

pub async fn list_releases(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
) -> ApiResult<Vec<CapabilityReleaseSummary>> {
    let rows = sqlx::query(
        "SELECT id, kind, resource_id, release_version, display_name, description,
                content_sha256, execution_semantics_sha256, published_at
         FROM capability_catalog_releases WHERE organization_id = $1
         ORDER BY published_at DESC, id",
    )
    .bind(auth.organization_id)
    .fetch_all(&state.db)
    .await
    .map_err(database_error)?;
    rows.into_iter()
        .map(release_summary_from_row)
        .collect::<Result<Vec<_>, _>>()
        .map(Json)
}

pub async fn install(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(release_id): Path<Uuid>,
    Extension(git): Extension<std::sync::Arc<GitRuntime>>,
    Json(request): Json<InstallCapabilityReleaseRequest>,
) -> ApiResult<CapabilityInstallationSummary> {
    let workspace_id = authorized_workspace(&state, &auth, request.workspace_id, true).await?;
    let row = sqlx::query(
        "SELECT release.kind, release.resource_id, release.release_version,
                release.content, release.content_sha256, workspace.profile_id
         FROM capability_catalog_releases release
         JOIN workspaces workspace ON workspace.id = $3
           AND workspace.organization_id = release.organization_id
         WHERE release.id = $1 AND release.organization_id = $2",
    )
    .bind(release_id)
    .bind(auth.organization_id)
    .bind(workspace_id)
    .fetch_optional(&state.db)
    .await
    .map_err(database_error)?
    .ok_or_else(|| not_found("Capability Release was not found"))?;
    let kind = parse_kind(row.get("kind"))?;
    let profile_id: Uuid = row.get("profile_id");
    let content: CatalogDraftContent =
        serde_json::from_value(row.get("content")).map_err(|_| internal_error())?;
    let requested_installation_id = Uuid::now_v7();
    let installation_id: Uuid = sqlx::query_scalar(
        "INSERT INTO capability_catalog_installations
         (id, organization_id, release_id, workspace_id, profile_id, state)
         VALUES ($1, $2, $3, $4, $5, 'installing')
         ON CONFLICT (organization_id, release_id, workspace_id)
         DO UPDATE SET state = 'installing', failure_code = NULL, updated_at = now()
         RETURNING id",
    )
    .bind(requested_installation_id)
    .bind(auth.organization_id)
    .bind(release_id)
    .bind(workspace_id)
    .bind(profile_id)
    .fetch_one(&state.db)
    .await
    .map_err(database_conflict)?;
    let result = if matches!(
        kind,
        CatalogResourceKind::ToolPackage | CatalogResourceKind::SkillPackage
    ) {
        let mut files = content
            .files
            .into_iter()
            .map(|file| (file.path, file.content))
            .collect::<BTreeMap<_, _>>();
        files.insert(
            ".open-web-release.json".to_string(),
            serde_json::to_string_pretty(&json!({
                "schemaVersion": "workspace.capability-package-release.v1",
                "workspaceId": workspace_id,
                "releaseId": release_id,
                "packageId": row.get::<String, _>("resource_id"),
                "version": row.get::<String, _>("release_version"),
                "contentSha256": row.get::<String, _>("content_sha256"),
            }))
            .map_err(|_| internal_error())?
                + "\n",
        );
        git.publish_capability_package(
            workspace_id,
            &row.get::<String, _>("resource_id"),
            &row.get::<String, _>("release_version"),
            &files,
        )
        .await
        .map(|_| ())
        .map_err(|_| "capability_install_failed")
    } else {
        Ok(())
    };
    match result {
        Ok(()) => {
            set_installation_state(
                &state.db,
                auth.organization_id,
                installation_id,
                release_id,
                workspace_id,
                "installed",
                Some(row.get("content_sha256")),
                None,
            )
            .await?;
        }
        Err(code) => {
            set_installation_state(
                &state.db,
                auth.organization_id,
                installation_id,
                release_id,
                workspace_id,
                "failed",
                None,
                Some(code),
            )
            .await?;
            return Err(bad_gateway("Capability installation failed"));
        }
    }
    load_installation(&state.db, auth.organization_id, release_id, workspace_id)
        .await
        .map(Json)
}

pub async fn readiness(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path((release_id, workspace_id)): Path<(Uuid, Uuid)>,
) -> ApiResult<CapabilityReadinessSummary> {
    authorized_workspace(&state, &auth, workspace_id, false).await?;
    let row = sqlx::query(
        "SELECT id, kind, resource_id, release_version, display_name, description,
                content_sha256, execution_semantics_sha256, published_at
         FROM capability_catalog_releases
         WHERE id = $1 AND organization_id = $2",
    )
    .bind(release_id)
    .bind(auth.organization_id)
    .fetch_optional(&state.db)
    .await
    .map_err(database_error)?
    .ok_or_else(|| not_found("Capability Release was not found"))?;
    let release = release_summary_from_row(row)?;
    let installation =
        load_installation_optional(&state.db, auth.organization_id, release_id, workspace_id)
            .await?;
    let runtime_discovered = installation.as_ref().is_some_and(|installation| {
        matches!(
            installation.state,
            open_web_codex_platform_contracts::InstallationState::Ready
                | open_web_codex_platform_contracts::InstallationState::Discovered
        )
    });
    Ok(Json(CapabilityReadinessSummary {
        release,
        installation,
        runtime_discovered,
        missing_capabilities: (!runtime_discovered)
            .then_some(vec!["runtime_discovery".to_string()])
            .unwrap_or_default(),
    }))
}

async fn load_draft(
    db: &sqlx::PgPool,
    organization_id: Uuid,
    id: Uuid,
) -> Result<CapabilityDraftDetail, ApiError> {
    let row = sqlx::query(
        "SELECT id, kind, resource_id, display_name, description, revision,
                content_sha256, validation_state, updated_at, content
         FROM capability_catalog_drafts WHERE id = $1 AND organization_id = $2",
    )
    .bind(id)
    .bind(organization_id)
    .fetch_optional(db)
    .await
    .map_err(database_error)?
    .ok_or_else(|| not_found("Capability Draft was not found"))?;
    let content = serde_json::from_value(row.get("content")).map_err(|_| internal_error())?;
    Ok(CapabilityDraftDetail {
        summary: draft_summary_from_row(&row)?,
        content,
    })
}

fn draft_summary(row: sqlx::postgres::PgRow) -> Result<CapabilityDraftSummary, ApiError> {
    draft_summary_from_row(&row)
}

fn draft_summary_from_row(row: &sqlx::postgres::PgRow) -> Result<CapabilityDraftSummary, ApiError> {
    Ok(CapabilityDraftSummary {
        id: row.get("id"),
        kind: parse_kind(row.get("kind"))?,
        resource_id: row.get("resource_id"),
        display_name: row.get("display_name"),
        description: row.get("description"),
        metadata: open_web_codex_platform_contracts::DraftMetadata {
            id: row.get("id"),
            revision: row.get("revision"),
            content_sha256: row.get("content_sha256"),
            validation_state: row.get("validation_state"),
            updated_at: row.get("updated_at"),
        },
    })
}

fn release_summary_from_row(
    row: sqlx::postgres::PgRow,
) -> Result<CapabilityReleaseSummary, ApiError> {
    let kind = parse_kind(row.get("kind"))?;
    Ok(open_web_codex_capability_catalog::release_summary(
        row.get("id"),
        kind,
        row.get("resource_id"),
        row.get("release_version"),
        row.get("display_name"),
        row.get("description"),
        row.get("content_sha256"),
        row.get("execution_semantics_sha256"),
        row.get("published_at"),
    ))
}

async fn resolve_dependencies(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    organization_id: Uuid,
    dependencies: Vec<CatalogDependency>,
) -> Result<Vec<CatalogDependency>, ApiError> {
    let mut resolved = Vec::with_capacity(dependencies.len());
    for dependency in dependencies {
        let row = sqlx::query(
            "SELECT id, release_version FROM capability_catalog_releases
             WHERE organization_id = $1 AND kind = $2 AND resource_id = $3
               AND (($4::uuid IS NOT NULL AND id = $4)
                    OR ($4::uuid IS NULL AND release_version = $5))",
        )
        .bind(organization_id)
        .bind(kind_name(dependency.kind))
        .bind(&dependency.resource_id)
        .bind(dependency.release_id)
        .bind(dependency.release_version.as_deref())
        .fetch_optional(&mut **transaction)
        .await
        .map_err(database_error)?
        .ok_or_else(|| bad_request("Capability dependency does not resolve to an exact Release"))?;
        resolved.push(CatalogDependency {
            kind: dependency.kind,
            resource_id: dependency.resource_id,
            release_id: Some(row.get("id")),
            release_version: Some(row.get("release_version")),
        });
    }
    Ok(resolved)
}

async fn load_installation(
    db: &sqlx::PgPool,
    organization_id: Uuid,
    release_id: Uuid,
    workspace_id: Uuid,
) -> Result<CapabilityInstallationSummary, ApiError> {
    load_installation_optional(db, organization_id, release_id, workspace_id)
        .await?
        .ok_or_else(|| internal_error())
}

async fn load_installation_optional(
    db: &sqlx::PgPool,
    organization_id: Uuid,
    release_id: Uuid,
    workspace_id: Uuid,
) -> Result<Option<CapabilityInstallationSummary>, ApiError> {
    let row = sqlx::query(
        "SELECT id, release_id, workspace_id, profile_id, state,
                observed_content_sha256, failure_code, updated_at
         FROM capability_catalog_installations
         WHERE organization_id = $1 AND release_id = $2 AND workspace_id = $3",
    )
    .bind(organization_id)
    .bind(release_id)
    .bind(workspace_id)
    .fetch_optional(db)
    .await
    .map_err(database_error)?;
    row.map(installation_from_row).transpose()
}

async fn set_installation_state(
    db: &sqlx::PgPool,
    organization_id: Uuid,
    installation_id: Uuid,
    release_id: Uuid,
    workspace_id: Uuid,
    state: &str,
    content_sha256: Option<String>,
    failure_code: Option<&str>,
) -> Result<(), ApiError> {
    sqlx::query(
        "UPDATE capability_catalog_installations
         SET state = $1, observed_content_sha256 = $2, failure_code = $3, updated_at = now()
         WHERE id = $4 AND organization_id = $5 AND release_id = $6 AND workspace_id = $7",
    )
    .bind(state)
    .bind(&content_sha256)
    .bind(failure_code)
    .bind(installation_id)
    .bind(organization_id)
    .bind(release_id)
    .bind(workspace_id)
    .execute(db)
    .await
    .map_err(database_error)?;
    sqlx::query(
        "INSERT INTO capability_catalog_installation_events
         (installation_id, state, content_sha256, failure_code)
         VALUES ($1, $2, $3, $4)",
    )
    .bind(installation_id)
    .bind(state)
    .bind(content_sha256)
    .bind(failure_code)
    .execute(db)
    .await
    .map_err(database_error)?;
    Ok(())
}

fn installation_from_row(
    row: sqlx::postgres::PgRow,
) -> Result<CapabilityInstallationSummary, ApiError> {
    Ok(CapabilityInstallationSummary {
        id: row.get("id"),
        release_id: row.get("release_id"),
        workspace_id: row.get("workspace_id"),
        profile_id: row.get("profile_id"),
        state: parse_installation_state(row.get("state"))?,
        observed_content_sha256: row.get("observed_content_sha256"),
        failure_code: row.get("failure_code"),
        updated_at: row.get("updated_at"),
    })
}

fn parse_kind(value: String) -> Result<CatalogResourceKind, ApiError> {
    match value.as_str() {
        "tool_package" => Ok(CatalogResourceKind::ToolPackage),
        "skill_package" => Ok(CatalogResourceKind::SkillPackage),
        "agent_definition" => Ok(CatalogResourceKind::AgentDefinition),
        "supervisor_definition" => Ok(CatalogResourceKind::SupervisorDefinition),
        "copilot_package" => Ok(CatalogResourceKind::CopilotPackage),
        _ => Err(internal_error()),
    }
}

fn kind_name(kind: CatalogResourceKind) -> &'static str {
    match kind {
        CatalogResourceKind::ToolPackage => "tool_package",
        CatalogResourceKind::SkillPackage => "skill_package",
        CatalogResourceKind::AgentDefinition => "agent_definition",
        CatalogResourceKind::SupervisorDefinition => "supervisor_definition",
        CatalogResourceKind::CopilotPackage => "copilot_package",
    }
}

fn parse_installation_state(
    value: String,
) -> Result<open_web_codex_platform_contracts::InstallationState, ApiError> {
    match value.as_str() {
        "authorized" => Ok(open_web_codex_platform_contracts::InstallationState::Authorized),
        "installing" => Ok(open_web_codex_platform_contracts::InstallationState::Installing),
        "installed" => Ok(open_web_codex_platform_contracts::InstallationState::Installed),
        "discovered" => Ok(open_web_codex_platform_contracts::InstallationState::Discovered),
        "ready" => Ok(open_web_codex_platform_contracts::InstallationState::Ready),
        "failed" => Ok(open_web_codex_platform_contracts::InstallationState::Failed),
        "uninstalled" => Ok(open_web_codex_platform_contracts::InstallationState::Uninstalled),
        _ => Err(internal_error()),
    }
}

fn require_manage(auth: &AuthenticatedUser, owner: Uuid) -> Result<(), ApiError> {
    if owner == auth.user_id || matches!(auth.organization_role.as_str(), "owner" | "admin") {
        Ok(())
    } else {
        Err(forbidden("You are not allowed to manage this capability"))
    }
}

fn validate_metadata(display_name: &str, description: &str) -> Result<(), ApiError> {
    let display_name = display_name.trim();
    let description = description.trim();
    if display_name.is_empty() || display_name.chars().count() > 120 {
        return Err(bad_request(
            "display_name must contain 1 to 120 characters",
        ));
    }
    if description.chars().count() > 1000 {
        return Err(bad_request(
            "description must contain at most 1000 characters",
        ));
    }
    Ok(())
}

fn bad_request(message: impl Into<String>) -> ApiError {
    let message = message.into();
    (
        StatusCode::BAD_REQUEST,
        Json(PlatformError::bad_request(&message)),
    )
}

fn unprocessable(message: impl Into<String>) -> ApiError {
    let message = message.into();
    (
        StatusCode::UNPROCESSABLE_ENTITY,
        Json(PlatformError::bad_request(&message)),
    )
}

fn conflict(message: &str) -> ApiError {
    (
        StatusCode::CONFLICT,
        Json(PlatformError::bad_request(message)),
    )
}

fn forbidden(message: &str) -> ApiError {
    (
        StatusCode::FORBIDDEN,
        Json(PlatformError::forbidden(message)),
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
        Json(PlatformError::internal("capability catalog failed")),
    )
}

fn bad_gateway(message: &str) -> ApiError {
    (
        StatusCode::BAD_GATEWAY,
        Json(PlatformError::internal(message)),
    )
}

fn database_error(_: sqlx::Error) -> ApiError {
    internal_error()
}

fn database_conflict(_: sqlx::Error) -> ApiError {
    conflict("Capability catalog identity already exists or is inconsistent")
}
