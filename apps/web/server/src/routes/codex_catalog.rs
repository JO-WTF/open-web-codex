use std::sync::Arc;

use axum::{
    extract::Query,
    http::StatusCode,
    Extension, Json,
};
use open_web_codex_adapter::CodexAdapter;
use open_web_codex_platform_contracts::error::PlatformError;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::codex_workspace::resolve_workspace_id;
use crate::middleware::auth::AuthenticatedUser;

type ApiResult<T> = Result<Json<T>, (StatusCode, Json<PlatformError>)>;

fn adapter_error(error: open_web_codex_adapter::AdapterError) -> (StatusCode, Json<PlatformError>) {
    (
        StatusCode::BAD_GATEWAY,
        Json(PlatformError::internal(format!("adapter error: {error}"))),
    )
}

/// GET /api/codex/model-providers
pub async fn list_model_providers(
    _auth: AuthenticatedUser,
    Extension(adapter): Extension<Arc<dyn CodexAdapter>>,
) -> ApiResult<Value> {
    let workspace_id = resolve_workspace_id(&adapter)
        .await
        .map_err(adapter_error)?;
    let response = adapter
        .rpc(
            "model_provider_list",
            json!({ "workspaceId": workspace_id }),
        )
        .await
        .map_err(adapter_error)?;
    Ok(Json(unwrap_adapter_result(response)))
}

/// GET /api/codex/models
pub async fn list_models(
    _auth: AuthenticatedUser,
    Extension(adapter): Extension<Arc<dyn CodexAdapter>>,
    Query(params): Query<ListModelsParams>,
) -> ApiResult<Value> {
    let workspace_id = resolve_workspace_id(&adapter)
        .await
        .map_err(adapter_error)?;
    let mut rpc_params = json!({ "workspaceId": workspace_id });
    if params.force_refresh == Some(true) {
        rpc_params["forceRefresh"] = json!(true);
    }
    let response = adapter
        .rpc("model_list", rpc_params)
        .await
        .map_err(adapter_error)?;
    Ok(Json(unwrap_adapter_result(response)))
}

#[derive(Deserialize)]
pub struct ListModelsParams {
    pub force_refresh: Option<bool>,
}

/// POST /api/codex/model-providers/write
pub async fn write_model_provider(
    _auth: AuthenticatedUser,
    Extension(adapter): Extension<Arc<dyn CodexAdapter>>,
    Json(input): Json<Value>,
) -> ApiResult<Value> {
    if !input.is_object() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(PlatformError::bad_request("provider mutation must be an object")),
        ));
    }
    let workspace_id = resolve_workspace_id(&adapter)
        .await
        .map_err(adapter_error)?;
    let response = adapter
        .rpc(
            "model_provider_write",
            json!({ "workspaceId": workspace_id, "input": input }),
        )
        .await
        .map_err(adapter_error)?;
    Ok(Json(unwrap_adapter_result(response)))
}

fn unwrap_adapter_result(response: Value) -> Value {
    response.get("result").cloned().unwrap_or(response)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unwrap_adapter_result_prefers_result_field() {
        let wrapped = json!({ "result": { "currentProviderId": "openai" } });
        assert_eq!(
            unwrap_adapter_result(wrapped),
            json!({ "currentProviderId": "openai" })
        );
    }
}
