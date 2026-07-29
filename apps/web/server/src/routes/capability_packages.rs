use axum::{extract::State, http::StatusCode, Json};
use open_web_codex_platform_contracts::error::PlatformError;
use open_web_codex_platform_contracts::CapabilityPackageSummary;
use open_web_codex_platform_store::AppState;
use sqlx::Row;

use crate::middleware::auth::AuthenticatedUser;

type ApiResult<T> = Result<Json<T>, (StatusCode, Json<PlatformError>)>;

pub async fn list(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
) -> ApiResult<Vec<CapabilityPackageSummary>> {
    let mut packages = open_web_codex_supervisor_catalog::capability_package::list_published()
        .map_err(|_| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(PlatformError::internal(
                    "capability package catalog is invalid",
                )),
            )
        })?;
    let is_admin = matches!(auth.organization_role.as_str(), "owner" | "admin");
    let rows = sqlx::query(
        "SELECT release.id, release.workspace_id, release.package_id, release.version, \
                release.display_name, release.description, release.capability_root_id, \
                release.server_name, release.tool_names, release.capabilities, \
                release.input_artifact_types, release.output_artifact_types, \
                release.content_sha256 \
         FROM workspace_capability_package_releases release \
         WHERE release.organization_id = $1 AND release.state = 'published' \
           AND ($3 OR EXISTS ( \
               SELECT 1 FROM workspace_grants workspace_grant \
               WHERE workspace_grant.workspace_id = release.workspace_id \
                 AND workspace_grant.organization_id = release.organization_id \
                 AND workspace_grant.user_id = $2 \
           )) \
         ORDER BY release.display_name, release.package_id, release.version",
    )
    .bind(auth.organization_id)
    .bind(auth.user_id)
    .bind(is_admin)
    .fetch_all(&state.db)
    .await
    .map_err(|_| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(PlatformError::internal(
                "capability package catalog could not be loaded",
            )),
        )
    })?;
    for row in rows {
        let tool_names =
            serde_json::from_value::<Vec<String>>(row.get("tool_names")).map_err(|_| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(PlatformError::internal(
                        "capability package catalog is invalid",
                    )),
                )
            })?;
        let capabilities =
            serde_json::from_value::<Vec<String>>(row.get("capabilities")).map_err(|_| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(PlatformError::internal(
                        "capability package catalog is invalid",
                    )),
                )
            })?;
        let input_artifact_types = serde_json::from_value::<Vec<String>>(
            row.get("input_artifact_types"),
        )
        .map_err(|_| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(PlatformError::internal(
                    "capability package catalog is invalid",
                )),
            )
        })?;
        let output_artifact_types = serde_json::from_value::<Vec<String>>(
            row.get("output_artifact_types"),
        )
        .map_err(|_| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(PlatformError::internal(
                    "capability package catalog is invalid",
                )),
            )
        })?;
        packages.push(CapabilityPackageSummary {
            release_id: Some(row.get("id")),
            workspace_id: Some(row.get("workspace_id")),
            package_id: row.get("package_id"),
            version: row.get("version"),
            display_name: row.get("display_name"),
            description: row.get("description"),
            capability_root_id: row.get("capability_root_id"),
            capabilities,
            mcp_server_names: vec![row.get("server_name")],
            tool_names,
            input_artifact_types,
            output_artifact_types,
            includes_skills: true,
            source: "workspace_release".to_string(),
            content_sha256: row.get("content_sha256"),
        });
    }
    packages.sort_by(|left, right| {
        left.display_name
            .cmp(&right.display_name)
            .then_with(|| left.package_id.cmp(&right.package_id))
            .then_with(|| left.version.cmp(&right.version))
    });
    Ok(Json(packages))
}
