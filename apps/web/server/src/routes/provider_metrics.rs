use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use open_web_codex_platform_contracts::error::PlatformError;
use open_web_codex_platform_contracts::ProviderCallMetric;
use open_web_codex_platform_store::AppState;
use sqlx::Row;
use uuid::Uuid;

use crate::middleware::auth::AuthenticatedUser;

type ApiError = (StatusCode, Json<PlatformError>);
type ApiResult<T> = Result<Json<T>, ApiError>;

pub async fn list_for_run(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(run_id): Path<Uuid>,
) -> ApiResult<Vec<ProviderCallMetric>> {
    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM runs WHERE id = $1 AND organization_id = $2)",
    )
    .bind(run_id)
    .bind(auth.organization_id)
    .fetch_one(&state.db)
    .await
    .map_err(database_error)?;
    if !exists {
        return Err(not_found("Run was not found"));
    }
    let rows = sqlx::query(
        "SELECT id, run_id, provider_id, model_id, input_tokens,
                cached_input_tokens, output_tokens, tool_schema_tokens,
                latency_ms, first_token_ms, compaction_count, terminal_status,
                stable_prefix_sha256, tool_inventory_sha256, skill_set_sha256,
                runtime_role_sha256, created_at
         FROM provider_call_metrics
         WHERE organization_id = $1 AND run_id = $2
         ORDER BY created_at, id LIMIT 500",
    )
    .bind(auth.organization_id)
    .bind(run_id)
    .fetch_all(&state.db)
    .await
    .map_err(database_error)?;
    Ok(Json(
        rows.into_iter()
            .map(|row| ProviderCallMetric {
                id: row.get("id"),
                run_id: row.get("run_id"),
                provider_id: row.get("provider_id"),
                model_id: row.get("model_id"),
                input_tokens: row.get("input_tokens"),
                cached_input_tokens: row.get("cached_input_tokens"),
                output_tokens: row.get("output_tokens"),
                tool_schema_tokens: row.get("tool_schema_tokens"),
                latency_ms: row.get("latency_ms"),
                first_token_ms: row.get("first_token_ms"),
                compaction_count: row.get("compaction_count"),
                terminal_status: row.get("terminal_status"),
                stable_prefix_sha256: row.get("stable_prefix_sha256"),
                tool_inventory_sha256: row.get("tool_inventory_sha256"),
                skill_set_sha256: row.get("skill_set_sha256"),
                runtime_role_sha256: row.get("runtime_role_sha256"),
                created_at: row.get("created_at"),
            })
            .collect(),
    ))
}

fn not_found(message: &str) -> ApiError {
    (
        StatusCode::NOT_FOUND,
        Json(PlatformError::not_found(message)),
    )
}

fn database_error(_: sqlx::Error) -> ApiError {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(PlatformError::internal(
            "provider metrics could not be loaded",
        )),
    )
}
