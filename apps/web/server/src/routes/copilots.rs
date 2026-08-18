use std::sync::Arc;

use axum::{http::StatusCode, Extension, Json};
use open_web_codex_platform_contracts::error::PlatformError;
use open_web_codex_platform_contracts::{ActivateCopilotRequest, CopilotProfileStatus};
use open_web_codex_platform_store::AppState;

use crate::copilot_installation::{CopilotInstallationError, CopilotInstallationService};
use crate::middleware::auth::{require_runtime_profile, AuthenticatedUser};
use crate::routes::RuntimeProfileBinding;

type ApiResult<T> = Result<Json<T>, (StatusCode, Json<PlatformError>)>;

pub async fn status(
    axum::extract::State(state): axum::extract::State<AppState>,
    auth: AuthenticatedUser,
    Extension(profile): Extension<RuntimeProfileBinding>,
    Extension(copilots): Extension<Arc<CopilotInstallationService>>,
) -> ApiResult<CopilotProfileStatus> {
    require_runtime_profile(&state.db, &auth, &profile.runtime_key).await?;
    copilots.status().await.map(Json).map_err(api_error)
}

pub async fn activate(
    axum::extract::State(state): axum::extract::State<AppState>,
    auth: AuthenticatedUser,
    Extension(profile): Extension<RuntimeProfileBinding>,
    Extension(copilots): Extension<Arc<CopilotInstallationService>>,
    Json(request): Json<ActivateCopilotRequest>,
) -> ApiResult<CopilotProfileStatus> {
    require_runtime_profile(&state.db, &auth, &profile.runtime_key).await?;
    copilots
        .activate(&request.package_id)
        .await
        .map(Json)
        .map_err(api_error)
}

pub async fn deactivate(
    axum::extract::State(state): axum::extract::State<AppState>,
    auth: AuthenticatedUser,
    Extension(profile): Extension<RuntimeProfileBinding>,
    Extension(copilots): Extension<Arc<CopilotInstallationService>>,
    Json(request): Json<ActivateCopilotRequest>,
) -> ApiResult<CopilotProfileStatus> {
    require_runtime_profile(&state.db, &auth, &profile.runtime_key).await?;
    copilots
        .deactivate(&request.package_id)
        .await
        .map(Json)
        .map_err(api_error)
}

fn api_error(error: CopilotInstallationError) -> (StatusCode, Json<PlatformError>) {
    match error {
        CopilotInstallationError::NotFound | CopilotInstallationError::InvalidSource(_) => (
            StatusCode::NOT_FOUND,
            Json(PlatformError::not_found("Copilot package was not found")),
        ),
        CopilotInstallationError::Unavailable => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(PlatformError::internal("Copilot package is unavailable")),
        ),
        CopilotInstallationError::InvalidSelection => (
            StatusCode::BAD_REQUEST,
            Json(PlatformError::bad_request(
                "Copilot package selection is required and must be available",
            )),
        ),
        CopilotInstallationError::Database(_)
        | CopilotInstallationError::Package(_)
        | CopilotInstallationError::StartupFile(_) => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(PlatformError::internal(
                "Copilot installation service is temporarily unavailable",
            )),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_application_source_is_not_exposed_as_a_server_path_error() {
        let (status, Json(error)) = api_error(CopilotInstallationError::InvalidSource(
            "/private/server/path".to_string(),
        ));
        assert_eq!(status, StatusCode::NOT_FOUND);
        let encoded = serde_json::to_string(&error).expect("encode platform error");
        assert!(!encoded.contains("/private/server/path"));
    }
}
