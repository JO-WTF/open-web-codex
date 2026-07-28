use axum::{http::StatusCode, Json};
use open_web_codex_platform_contracts::error::PlatformError;
use open_web_codex_platform_contracts::CapabilityPackageSummary;

use crate::middleware::auth::AuthenticatedUser;

type ApiResult<T> = Result<Json<T>, (StatusCode, Json<PlatformError>)>;

pub async fn list(_auth: AuthenticatedUser) -> ApiResult<Vec<CapabilityPackageSummary>> {
    open_web_codex_supervisor_catalog::capability_package::list_published()
        .map(Json)
        .map_err(|_| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(PlatformError::internal(
                    "capability package catalog is invalid",
                )),
            )
        })
}
