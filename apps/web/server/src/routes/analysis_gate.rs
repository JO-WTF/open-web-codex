//! Internal, non-browser authorization for Network/Visualization analysis tools.
//!
//! MCP children receive the process-local key through the Profile Host
//! environment.  A request is useful only when it proves both possession of
//! that key and an active, task-owned execution snapshot.

use axum::{extract::State, http::HeaderMap, Json};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use hmac::{Hmac, Mac};
use open_web_codex_platform_store::AppState;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::Row;
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

type HmacSha256 = Hmac<Sha256>;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalysisGateRequest {
    pub execution_snapshot_id: Uuid,
    pub tool_name: String,
    pub planning_dataset_ref: String,
    pub binding_fingerprint: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalysisGateResponse {
    pub authorized: bool,
    pub execution_snapshot_id: Uuid,
}

pub async fn authorize(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Result<Json<AnalysisGateResponse>, axum::http::StatusCode> {
    let timestamp = headers
        .get("x-open-web-codex-analysis-timestamp")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<i64>().ok())
        .ok_or(axum::http::StatusCode::UNAUTHORIZED)?;
    let signature = headers
        .get("x-open-web-codex-analysis-signature")
        .and_then(|value| value.to_str().ok())
        .ok_or(axum::http::StatusCode::UNAUTHORIZED)?;
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| axum::http::StatusCode::UNAUTHORIZED)?
        .as_secs() as i64;
    if (now - timestamp).unsigned_abs() > 60 || state.analysis_gate_key.is_empty() {
        return Err(axum::http::StatusCode::UNAUTHORIZED);
    }
    let body_hash = hex::encode(Sha256::digest(&body));
    let path = "/api/internal/analysis-gate/v1/authorize";
    let message = format!("{timestamp}\nPOST\n{path}\n{body_hash}");
    let expected = URL_SAFE_NO_PAD.encode({
        let mut mac = HmacSha256::new_from_slice(state.analysis_gate_key.as_slice())
            .map_err(|_| axum::http::StatusCode::UNAUTHORIZED)?;
        mac.update(message.as_bytes());
        mac.finalize().into_bytes()
    });
    if expected != signature {
        return Err(axum::http::StatusCode::UNAUTHORIZED);
    }
    let request: AnalysisGateRequest =
        serde_json::from_slice(&body).map_err(|_| axum::http::StatusCode::BAD_REQUEST)?;
    const ALLOWED_TOOLS: &[&str] = &[
        "prepare_network_snapshot_from_planning_dataset",
        "register_route_matrix",
        "evaluate_current_coverage",
        "solve_facility_location",
        "evaluate_network_scenario",
        "evaluate_financial_case",
        "compare_network_scenarios",
        "prepare_network_comparison_map",
        "prepare_network_map_render",
        "prepare_network_planning_report",
    ];
    if !ALLOWED_TOOLS.contains(&request.tool_name.as_str())
        || request.binding_fingerprint.len() != 64
        || !request
            .binding_fingerprint
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
    {
        return Err(axum::http::StatusCode::FORBIDDEN);
    }
    // The snapshot alone is not sufficient authorization.  A previously
    // started snapshot must still point at the Task's current binding; input
    // changes revoke that binding and therefore revoke every old analysis
    // call.  Joining the authoritative session/binding rows also prevents a
    // stale snapshot from being used after a newer binding is selected.
    let row = sqlx::query(
        "SELECT snapshot.state, snapshot.readiness_fingerprint,
                snapshot.input_snapshot, snapshot.binding_id,
                binding.fingerprint AS binding_fingerprint,
                intake.current_binding_id
         FROM task_analysis_execution_snapshots snapshot
         JOIN task_dataset_bindings binding
           ON binding.organization_id = snapshot.organization_id
          AND binding.id = snapshot.binding_id
          AND binding.task_id = snapshot.task_id
         JOIN data_intake_sessions intake
           ON intake.organization_id = binding.organization_id
          AND intake.id = binding.intake_session_id
          AND intake.task_id = binding.task_id
          AND intake.current_binding_id = binding.id
         WHERE snapshot.id = $1",
    )
    .bind(request.execution_snapshot_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|_| axum::http::StatusCode::SERVICE_UNAVAILABLE)?
    .ok_or(axum::http::StatusCode::FORBIDDEN)?;
    if row.get::<String, _>("state") != "started" {
        return Err(axum::http::StatusCode::FORBIDDEN);
    }
    if row.get::<String, _>("binding_fingerprint") != request.binding_fingerprint {
        return Err(axum::http::StatusCode::FORBIDDEN);
    }
    let snapshot: serde_json::Value = row.get("input_snapshot");
    if snapshot
        .get("bindingFingerprint")
        .and_then(serde_json::Value::as_str)
        != Some(request.binding_fingerprint.as_str())
    {
        return Err(axum::http::StatusCode::FORBIDDEN);
    }
    let dataset_matches = snapshot
        .get("datasetArtifactId")
        .and_then(serde_json::Value::as_str)
        .is_some_and(|value| value == request.planning_dataset_ref)
        || snapshot
            .get("planningDatasetRef")
            .and_then(serde_json::Value::as_str)
            .is_some_and(|value| value == request.planning_dataset_ref);
    if !dataset_matches {
        return Err(axum::http::StatusCode::FORBIDDEN);
    }
    Ok(Json(AnalysisGateResponse {
        authorized: true,
        execution_snapshot_id: request.execution_snapshot_id,
    }))
}
