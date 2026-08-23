use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::{Extension, Json};
use open_web_codex_adapter::{CodexAdapter, ProfileMutation};
use open_web_codex_platform_contracts::error::PlatformError;
use open_web_codex_platform_contracts::SetExperimentalFeatureRequest;
use open_web_codex_platform_store::AppState;
use serde_json::{json, Value};

use crate::middleware::auth::{require_runtime_profile, AuthenticatedUser};
use crate::routes::RuntimeProfileBinding;

type ApiError = (StatusCode, Json<PlatformError>);
type ApiResult<T> = Result<Json<T>, ApiError>;

pub async fn set_experimental_feature(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(name): Path<String>,
    Extension(adapter): Extension<Arc<dyn CodexAdapter>>,
    Extension(profile): Extension<RuntimeProfileBinding>,
    Json(request): Json<SetExperimentalFeatureRequest>,
) -> ApiResult<Value> {
    require_runtime_profile(&state.db, &auth, &profile.runtime_key).await?;
    let name = validate_feature_name(&name)?;
    adapter
        .mutate_profile(ProfileMutation::SetExperimentalFeature {
            name,
            enabled: request.enabled,
        })
        .await
        .map_err(runtime_write_error)?;
    Ok(Json(json!({ "status": "ok" })))
}

fn validate_feature_name(value: &str) -> Result<String, ApiError> {
    let value = value.trim();
    if value.is_empty()
        || value.len() > 64
        || !value
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_'))
    {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(PlatformError::bad_request("Invalid feature name")),
        ));
    }
    Ok(value.to_string())
}

fn runtime_write_error(error: open_web_codex_adapter::AdapterError) -> ApiError {
    tracing::error!(error = %error, "Codex Profile configuration update failed");
    (
        StatusCode::BAD_GATEWAY,
        Json(PlatformError::internal(
            "Codex Profile configuration update failed",
        )),
    )
}
