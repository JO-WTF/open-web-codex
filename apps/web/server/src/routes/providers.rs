use std::sync::Arc;

use axum::{extract::Path, http::StatusCode, Extension, Json};
use open_web_codex_platform_contracts::error::{ErrorKind, PlatformError};
use open_web_codex_platform_contracts::{
    ProviderCatalog, UpdateProviderModelRequest, UpsertProviderRequest,
};
use open_web_codex_provider_service::{ProviderOperations, ProviderServiceError};

use crate::middleware::auth::AuthenticatedUser;

type ApiResult<T> = Result<Json<T>, (StatusCode, Json<PlatformError>)>;

/// GET /api/providers — return a credential-safe Provider catalog.
pub async fn list_providers(
    _auth: AuthenticatedUser,
    Extension(providers): Extension<Arc<dyn ProviderOperations>>,
) -> ApiResult<ProviderCatalog> {
    providers.list().await.map(Json).map_err(provider_error)
}

/// PUT /api/providers/:id — create or update a custom Provider.
pub async fn upsert_provider(
    _auth: AuthenticatedUser,
    Path(id): Path<String>,
    Extension(providers): Extension<Arc<dyn ProviderOperations>>,
    Json(request): Json<UpsertProviderRequest>,
) -> ApiResult<ProviderCatalog> {
    providers
        .upsert(&id, request)
        .await
        .map(Json)
        .map_err(provider_error)
}

/// POST /api/providers/:id/select — select a configured Provider.
pub async fn select_provider(
    _auth: AuthenticatedUser,
    Path(id): Path<String>,
    Extension(providers): Extension<Arc<dyn ProviderOperations>>,
) -> ApiResult<ProviderCatalog> {
    providers
        .select(&id)
        .await
        .map(Json)
        .map_err(provider_error)
}

/// DELETE /api/providers/:id — remove a non-current custom Provider.
pub async fn delete_provider(
    _auth: AuthenticatedUser,
    Path(id): Path<String>,
    Extension(providers): Extension<Arc<dyn ProviderOperations>>,
) -> ApiResult<ProviderCatalog> {
    providers
        .delete(&id)
        .await
        .map(Json)
        .map_err(provider_error)
}

/// POST /api/providers/:id/models/refresh — refresh the Provider-scoped catalog.
pub async fn refresh_provider_models(
    _auth: AuthenticatedUser,
    Path(id): Path<String>,
    Extension(providers): Extension<Arc<dyn ProviderOperations>>,
) -> ApiResult<ProviderCatalog> {
    providers
        .refresh_models(&id)
        .await
        .map(Json)
        .map_err(provider_error)
}

/// PATCH /api/providers/:provider_id/models/:model_id — update platform-editable
/// model metadata without exposing Codex configuration paths.
pub async fn update_provider_model(
    _auth: AuthenticatedUser,
    Path((provider_id, model_id)): Path<(String, String)>,
    Extension(providers): Extension<Arc<dyn ProviderOperations>>,
    Json(request): Json<UpdateProviderModelRequest>,
) -> ApiResult<ProviderCatalog> {
    providers
        .update_model(&provider_id, &model_id, request)
        .await
        .map(Json)
        .map_err(provider_error)
}

fn provider_error(error: ProviderServiceError) -> (StatusCode, Json<PlatformError>) {
    let (status, error) = match error {
        ProviderServiceError::InvalidInput(message) => {
            (StatusCode::BAD_REQUEST, PlatformError::bad_request(message))
        }
        ProviderServiceError::NotFound(message) => {
            (StatusCode::NOT_FOUND, PlatformError::not_found(message))
        }
        ProviderServiceError::Forbidden(message) => {
            (StatusCode::FORBIDDEN, PlatformError::forbidden(message))
        }
        ProviderServiceError::Runtime(_) => (
            StatusCode::BAD_GATEWAY,
            PlatformError {
                kind: ErrorKind::CodexRejected,
                message: "Codex rejected the Provider operation".to_string(),
                request_id: None,
                retry_after_ms: None,
            },
        ),
        ProviderServiceError::InvalidResponse(_) => (
            StatusCode::BAD_GATEWAY,
            PlatformError {
                kind: ErrorKind::CodexRejected,
                message: "Codex returned an invalid Provider response".to_string(),
                request_id: None,
                retry_after_ms: None,
            },
        ),
    };
    (status, Json(error))
}

#[cfg(test)]
mod tests {
    use super::provider_error;
    use axum::http::StatusCode;
    use open_web_codex_platform_contracts::error::ErrorKind;
    use open_web_codex_provider_service::ProviderServiceError;

    #[test]
    fn runtime_failures_do_not_expose_runtime_details() {
        let secret = "secret-value-must-not-leak";
        let (status, error) = provider_error(ProviderServiceError::Runtime(secret.to_string()));

        assert_eq!(status, StatusCode::BAD_GATEWAY);
        assert_eq!(error.kind, ErrorKind::CodexRejected);
        assert!(!error.message.contains(secret));
    }
}
