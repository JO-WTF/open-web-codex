use std::sync::Arc;

use axum::{extract::Path, http::StatusCode, Extension, Json};
use open_web_codex_platform_contracts::error::{ErrorKind, PlatformError, ProviderCatalogFailure};
use open_web_codex_platform_contracts::{
    ProviderCatalog, UpdateProviderModelRequest, UpsertProviderRequest,
};
use open_web_codex_provider_service::secured::{
    AuthorizedProviderError, AuthorizedProviderOperations, ProviderActor,
};
use open_web_codex_provider_service::ProviderServiceError;

use crate::middleware::auth::AuthenticatedUser;

type ApiResult<T> = Result<Json<T>, (StatusCode, Json<PlatformError>)>;

/// GET /api/providers — return a credential-safe Provider catalog.
pub async fn list_providers(
    auth: AuthenticatedUser,
    Extension(providers): Extension<Arc<dyn AuthorizedProviderOperations>>,
) -> ApiResult<ProviderCatalog> {
    providers
        .list(provider_actor(&auth))
        .await
        .map(Json)
        .map_err(provider_error)
}

/// PUT /api/providers/:id — create or update a custom Provider.
pub async fn upsert_provider(
    auth: AuthenticatedUser,
    Path(id): Path<String>,
    Extension(providers): Extension<Arc<dyn AuthorizedProviderOperations>>,
    Json(request): Json<UpsertProviderRequest>,
) -> ApiResult<ProviderCatalog> {
    providers
        .upsert(provider_actor(&auth), &id, request)
        .await
        .map(Json)
        .map_err(provider_error)
}

/// POST /api/providers/:id/select — select a configured Provider.
pub async fn select_provider(
    auth: AuthenticatedUser,
    Path(id): Path<String>,
    Extension(providers): Extension<Arc<dyn AuthorizedProviderOperations>>,
) -> ApiResult<ProviderCatalog> {
    providers
        .select(provider_actor(&auth), &id)
        .await
        .map(Json)
        .map_err(provider_error)
}

/// POST /api/providers/:provider_id/models/:model_id/select — update the
/// Profile Runtime default. Materialized Threads keep their own settings.
pub async fn select_provider_model(
    auth: AuthenticatedUser,
    Path((provider_id, model_id)): Path<(String, String)>,
    Extension(providers): Extension<Arc<dyn AuthorizedProviderOperations>>,
) -> ApiResult<ProviderCatalog> {
    providers
        .select_model(provider_actor(&auth), &provider_id, &model_id)
        .await
        .map(Json)
        .map_err(provider_error)
}

/// DELETE /api/providers/:id — remove a non-current custom Provider.
pub async fn delete_provider(
    auth: AuthenticatedUser,
    Path(id): Path<String>,
    Extension(providers): Extension<Arc<dyn AuthorizedProviderOperations>>,
) -> ApiResult<ProviderCatalog> {
    providers
        .delete(provider_actor(&auth), &id)
        .await
        .map(Json)
        .map_err(provider_error)
}

/// POST /api/providers/:id/models/refresh — refresh the Provider-scoped catalog.
pub async fn refresh_provider_models(
    auth: AuthenticatedUser,
    Path(id): Path<String>,
    Extension(providers): Extension<Arc<dyn AuthorizedProviderOperations>>,
) -> ApiResult<ProviderCatalog> {
    providers
        .refresh_models(provider_actor(&auth), &id)
        .await
        .map(Json)
        .map_err(provider_error)
}

/// PATCH /api/providers/:provider_id/models/:model_id — update platform-editable
/// model metadata without exposing Codex configuration paths.
pub async fn update_provider_model(
    auth: AuthenticatedUser,
    Path((provider_id, model_id)): Path<(String, String)>,
    Extension(providers): Extension<Arc<dyn AuthorizedProviderOperations>>,
    Json(request): Json<UpdateProviderModelRequest>,
) -> ApiResult<ProviderCatalog> {
    providers
        .update_model(provider_actor(&auth), &provider_id, &model_id, request)
        .await
        .map(Json)
        .map_err(provider_error)
}

fn provider_error(error: AuthorizedProviderError) -> (StatusCode, Json<PlatformError>) {
    match error {
        AuthorizedProviderError::Forbidden => (
            StatusCode::NOT_FOUND,
            Json(PlatformError::not_found("Provider Profile was not found")),
        ),
        AuthorizedProviderError::Provider(error) => provider_service_error(error),
        AuthorizedProviderError::Registry(_)
        | AuthorizedProviderError::Secret(_)
        | AuthorizedProviderError::Database(_) => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(PlatformError::internal(
                "Provider service is temporarily unavailable",
            )),
        ),
    }
}

fn provider_actor(auth: &AuthenticatedUser) -> ProviderActor {
    ProviderActor {
        user_id: auth.user_id,
        organization_id: auth.organization_id,
    }
}

fn provider_service_error(error: ProviderServiceError) -> (StatusCode, Json<PlatformError>) {
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
        ProviderServiceError::ProviderCatalogFailure(failure) => {
            provider_catalog_failure_error(failure)
        }
        ProviderServiceError::Runtime(_) => (
            StatusCode::BAD_GATEWAY,
            PlatformError {
                kind: ErrorKind::CodexRejected,
                message: "Codex rejected the Provider operation".to_string(),
                request_id: None,
                retry_after_ms: None,
                provider_catalog_failure: None,
            },
        ),
        ProviderServiceError::InvalidResponse(_) => (
            StatusCode::BAD_GATEWAY,
            PlatformError {
                kind: ErrorKind::CodexRejected,
                message: "Codex returned an invalid Provider response".to_string(),
                request_id: None,
                retry_after_ms: None,
                provider_catalog_failure: None,
            },
        ),
    };
    (status, Json(error))
}

fn provider_catalog_failure_error(failure: ProviderCatalogFailure) -> (StatusCode, PlatformError) {
    let (status, kind, message) = match failure {
        ProviderCatalogFailure::Authentication => (
            StatusCode::UNPROCESSABLE_ENTITY,
            ErrorKind::Unprocessable,
            "Provider authentication failed",
        ),
        ProviderCatalogFailure::NotFound => (
            StatusCode::BAD_GATEWAY,
            ErrorKind::CodexRejected,
            "Provider model catalog was not found",
        ),
        ProviderCatalogFailure::RateLimited => (
            StatusCode::TOO_MANY_REQUESTS,
            ErrorKind::RateLimited,
            "Provider model catalog request was rate limited",
        ),
        ProviderCatalogFailure::Upstream => (
            StatusCode::BAD_GATEWAY,
            ErrorKind::CodexRejected,
            "Provider model catalog upstream failure",
        ),
        ProviderCatalogFailure::Timeout => (
            StatusCode::SERVICE_UNAVAILABLE,
            ErrorKind::CodexUnavailable,
            "Provider model catalog request timed out",
        ),
        ProviderCatalogFailure::Network => (
            StatusCode::SERVICE_UNAVAILABLE,
            ErrorKind::CodexUnavailable,
            "Provider model catalog network failure",
        ),
        ProviderCatalogFailure::InvalidJson => (
            StatusCode::BAD_GATEWAY,
            ErrorKind::CodexRejected,
            "Provider model catalog returned invalid JSON",
        ),
        ProviderCatalogFailure::IncompatibleSchema => (
            StatusCode::BAD_GATEWAY,
            ErrorKind::CodexRejected,
            "Provider model catalog schema is incompatible",
        ),
        ProviderCatalogFailure::EmptyCatalog => (
            StatusCode::BAD_GATEWAY,
            ErrorKind::CodexRejected,
            "Provider model catalog is empty",
        ),
    };
    (
        status,
        PlatformError {
            kind,
            message: message.to_string(),
            request_id: None,
            retry_after_ms: None,
            provider_catalog_failure: Some(failure),
        },
    )
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::{list_providers, provider_error};
    use axum::{http::StatusCode, Extension};
    use open_web_codex_platform_contracts::error::{ErrorKind, ProviderCatalogFailure};
    use open_web_codex_provider_service::secured::{
        AuthorizedProviderOperations, InMemoryAuthorizedProviderService, ProviderActor,
    };
    use open_web_codex_provider_service::{secured::AuthorizedProviderError, ProviderServiceError};
    use uuid::Uuid;

    use crate::middleware::auth::AuthenticatedUser;

    fn authenticated_user(organization_id: Uuid, user_id: Uuid) -> AuthenticatedUser {
        AuthenticatedUser {
            session_id: Uuid::now_v7(),
            user_id,
            name: "Provider test".to_string(),
            username: format!("provider-{user_id}"),
            email: format!("{user_id}@example.invalid"),
            role: "owner".to_string(),
            organization_id,
            organization_role: "owner".to_string(),
        }
    }

    #[tokio::test]
    async fn list_forwards_the_runtime_catalog_without_platform_state() {
        let organization_id = Uuid::now_v7();
        let user_id = Uuid::now_v7();
        let providers = Arc::new(InMemoryAuthorizedProviderService::default());
        let expected = providers
            .list(ProviderActor {
                user_id,
                organization_id,
            })
            .await
            .expect("Runtime catalog");

        let actual = list_providers(
            authenticated_user(organization_id, user_id),
            Extension(providers),
        )
        .await
        .expect("route catalog")
        .0;

        assert_eq!(actual, expected);
    }

    #[test]
    fn runtime_failures_do_not_expose_runtime_details() {
        let secret = "secret-value-must-not-leak";
        let (status, error) = provider_error(AuthorizedProviderError::Provider(
            ProviderServiceError::Runtime(secret.to_string()),
        ));

        assert_eq!(status, StatusCode::BAD_GATEWAY);
        assert_eq!(error.kind, ErrorKind::CodexRejected);
        assert!(!error.message.contains(secret));
        assert!(error.provider_catalog_failure.is_none());
    }

    #[test]
    fn provider_catalog_failures_map_to_bounded_safe_errors() {
        let cases = [
            (
                ProviderCatalogFailure::Authentication,
                StatusCode::UNPROCESSABLE_ENTITY,
                ErrorKind::Unprocessable,
                "Provider authentication failed",
            ),
            (
                ProviderCatalogFailure::NotFound,
                StatusCode::BAD_GATEWAY,
                ErrorKind::CodexRejected,
                "Provider model catalog was not found",
            ),
            (
                ProviderCatalogFailure::RateLimited,
                StatusCode::TOO_MANY_REQUESTS,
                ErrorKind::RateLimited,
                "Provider model catalog request was rate limited",
            ),
            (
                ProviderCatalogFailure::Upstream,
                StatusCode::BAD_GATEWAY,
                ErrorKind::CodexRejected,
                "Provider model catalog upstream failure",
            ),
            (
                ProviderCatalogFailure::Timeout,
                StatusCode::SERVICE_UNAVAILABLE,
                ErrorKind::CodexUnavailable,
                "Provider model catalog request timed out",
            ),
            (
                ProviderCatalogFailure::Network,
                StatusCode::SERVICE_UNAVAILABLE,
                ErrorKind::CodexUnavailable,
                "Provider model catalog network failure",
            ),
            (
                ProviderCatalogFailure::InvalidJson,
                StatusCode::BAD_GATEWAY,
                ErrorKind::CodexRejected,
                "Provider model catalog returned invalid JSON",
            ),
            (
                ProviderCatalogFailure::IncompatibleSchema,
                StatusCode::BAD_GATEWAY,
                ErrorKind::CodexRejected,
                "Provider model catalog schema is incompatible",
            ),
            (
                ProviderCatalogFailure::EmptyCatalog,
                StatusCode::BAD_GATEWAY,
                ErrorKind::CodexRejected,
                "Provider model catalog is empty",
            ),
        ];

        for (failure, expected_status, expected_kind, expected_message) in cases {
            let (status, error) = provider_error(AuthorizedProviderError::Provider(
                ProviderServiceError::ProviderCatalogFailure(failure),
            ));
            assert_eq!(status, expected_status);
            assert_eq!(error.kind, expected_kind);
            assert_eq!(error.message, expected_message);
            assert_eq!(error.provider_catalog_failure, Some(failure));
        }
    }
}
