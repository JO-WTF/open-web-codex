use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use open_web_codex_platform_contracts::error::PlatformError;
use open_web_codex_platform_contracts::{AgentDefinitionDetail, AgentDefinitionSummary};
use open_web_codex_platform_store::AppState;

use crate::agent_catalog;
use crate::middleware::auth::AuthenticatedUser;

type ApiResult<T> = Result<Json<T>, (StatusCode, Json<PlatformError>)>;

pub async fn list_published(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
) -> ApiResult<Vec<AgentDefinitionSummary>> {
    agent_catalog::list_published(&state.db, auth.organization_id)
        .await
        .map(Json)
        .map_err(|_| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(PlatformError::internal(
                    "published Agent Definitions are invalid",
                )),
            )
        })
}

pub async fn get_published(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path((definition_id, version)): Path<(String, String)>,
) -> ApiResult<AgentDefinitionDetail> {
    agent_catalog::get_published(&state.db, auth.organization_id, &definition_id, &version)
        .await
        .map(Json)
        .map_err(|error| match error {
            agent_catalog::AgentCatalogError::NotFound => (
                StatusCode::NOT_FOUND,
                Json(PlatformError::not_found("Agent Definition was not found")),
            ),
            agent_catalog::AgentCatalogError::Invalid
            | agent_catalog::AgentCatalogError::Database => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(PlatformError::internal(
                    "published Agent Definition is invalid",
                )),
            ),
        })
}
