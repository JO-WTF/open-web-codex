use axum::{http::StatusCode, Json};
use open_web_codex_platform_contracts::error::PlatformError;
use open_web_codex_platform_contracts::AgentDefinitionSummary;
use open_web_codex_supervisor_catalog::agent;

use crate::middleware::auth::AuthenticatedUser;

type ApiResult<T> = Result<Json<T>, (StatusCode, Json<PlatformError>)>;

pub async fn list_published(_auth: AuthenticatedUser) -> ApiResult<Vec<AgentDefinitionSummary>> {
    agent::list_published().map(Json).map_err(|_| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(PlatformError::internal(
                "published Agent Definitions are invalid",
            )),
        )
    })
}
