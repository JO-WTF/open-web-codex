//! Signed read-only bridge for the platform coordination MCP.
//!
//! The Profile Host is the only Runtime process that receives this endpoint's
//! key.  Requests are still scoped to a persisted Run and can only select one
//! of the fixed read queries below.  This keeps coordination observable without
//! turning the MCP into a second scheduler or a browser-facing protocol proxy.

use axum::{extract::State, http::HeaderMap, Json};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use hmac::{Hmac, Mac};
use open_web_codex_platform_contracts::CollaborationStatusSummary;
use open_web_codex_platform_store::AppState;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

type HmacSha256 = Hmac<Sha256>;

const REQUEST_TTL_SECONDS: i64 = 60;
const SIGNATURE_HEADER: &str = "x-open-web-codex-coordination-signature";
const TIMESTAMP_HEADER: &str = "x-open-web-codex-coordination-timestamp";
const REQUEST_PATH: &str = "/api/internal/coordination/v1/query";

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CoordinationQueryRequest {
    pub run_id: Uuid,
    pub query: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CoordinationQueryResponse {
    schema_version: &'static str,
    run_id: Uuid,
    query: String,
    data: Value,
}

pub async fn query(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Result<Json<CoordinationQueryResponse>, axum::http::StatusCode> {
    verify_signature(&state, &headers, &body)?;
    let request: CoordinationQueryRequest =
        serde_json::from_slice(&body).map_err(|_| axum::http::StatusCode::BAD_REQUEST)?;
    let query = normalize_query(&request.query)?;
    let organization_id: Uuid =
        sqlx::query_scalar("SELECT organization_id FROM runs WHERE id = $1")
            .bind(request.run_id)
            .fetch_optional(&state.db)
            .await
            .map_err(|_| axum::http::StatusCode::SERVICE_UNAVAILABLE)?
            .ok_or(axum::http::StatusCode::FORBIDDEN)?;
    let status = crate::routes::collaboration::load_for_organization(
        &state.db,
        organization_id,
        request.run_id,
    )
    .await
    .map_err(|error| error.0)?;
    let data = project_query(&status, query);
    Ok(Json(CoordinationQueryResponse {
        schema_version: "platform-coordination-result.v1",
        run_id: request.run_id,
        query: query.to_string(),
        data,
    }))
}

fn verify_signature(
    state: &AppState,
    headers: &HeaderMap,
    body: &[u8],
) -> Result<(), axum::http::StatusCode> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| axum::http::StatusCode::UNAUTHORIZED)?
        .as_secs() as i64;
    verify_signature_at(
        state.coordination_gate_key.as_slice(),
        headers,
        body,
        now,
    )
}

fn verify_signature_at(
    key: &[u8],
    headers: &HeaderMap,
    body: &[u8],
    now: i64,
) -> Result<(), axum::http::StatusCode> {
    let timestamp = headers
        .get(TIMESTAMP_HEADER)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<i64>().ok())
        .ok_or(axum::http::StatusCode::UNAUTHORIZED)?;
    let signature = headers
        .get(SIGNATURE_HEADER)
        .and_then(|value| value.to_str().ok())
        .ok_or(axum::http::StatusCode::UNAUTHORIZED)?;
    if (now - timestamp).unsigned_abs() > REQUEST_TTL_SECONDS as u64
        || key.is_empty()
    {
        return Err(axum::http::StatusCode::UNAUTHORIZED);
    }
    let body_hash = hex::encode(Sha256::digest(body));
    let message = format!("{timestamp}\nPOST\n{REQUEST_PATH}\n{body_hash}");
    let mut mac = HmacSha256::new_from_slice(key)
        .map_err(|_| axum::http::StatusCode::UNAUTHORIZED)?;
    mac.update(message.as_bytes());
    let expected = URL_SAFE_NO_PAD.encode(mac.finalize().into_bytes());
    if !constant_time_equal(expected.as_bytes(), signature.as_bytes()) {
        return Err(axum::http::StatusCode::UNAUTHORIZED);
    }
    Ok(())
}

fn constant_time_equal(left: &[u8], right: &[u8]) -> bool {
    left.len() == right.len()
        && left
            .iter()
            .zip(right)
            .fold(0u8, |difference, (a, b)| difference | (a ^ b))
            == 0
}

fn normalize_query(value: &str) -> Result<&str, axum::http::StatusCode> {
    match value {
        "status" | "executions" | "work_state" | "blocking_inputs" | "deliverables" => Ok(value),
        _ => Err(axum::http::StatusCode::BAD_REQUEST),
    }
}

fn project_query(status: &CollaborationStatusSummary, query: &str) -> Value {
    match query {
        "status" => serde_json::to_value(status).unwrap_or_else(|_| json!({})),
        "executions" => json!({ "executions": status.executions }),
        "work_state" => json!({ "workState": status.work_state }),
        "blocking_inputs" => json!({
            "blockingInputs": status.work_state
                .as_ref()
                .map(|state| &state.blocking_inputs)
                .cloned()
                .unwrap_or_default()
        }),
        "deliverables" => json!({ "deliverables": status.deliverables }),
        _ => json!({}),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    #[test]
    fn accepts_only_the_fixed_read_queries() {
        assert_eq!(normalize_query("status"), Ok("status"));
        assert_eq!(normalize_query("deliverables"), Ok("deliverables"));
        assert_eq!(
            normalize_query("spawn_agent"),
            Err(axum::http::StatusCode::BAD_REQUEST)
        );
    }

    #[test]
    fn verifies_the_signed_request_without_exposing_the_key() {
        let key = b"test coordination key";
        let body = br#"{"runId":"00000000-0000-0000-0000-000000000001","query":"status"}"#;
        let timestamp = 1_754_600_000_i64;
        let body_hash = hex::encode(Sha256::digest(body));
        let message = format!(
            "{timestamp}\nPOST\n{REQUEST_PATH}\n{body_hash}"
        );
        let mut mac = HmacSha256::new_from_slice(key).unwrap();
        mac.update(message.as_bytes());
        let signature = URL_SAFE_NO_PAD.encode(mac.finalize().into_bytes());
        let mut headers = HeaderMap::new();
        headers.insert(TIMESTAMP_HEADER, HeaderValue::from(timestamp));
        headers.insert(SIGNATURE_HEADER, HeaderValue::from_str(&signature).unwrap());
        assert!(verify_signature_at(key, &headers, body, timestamp).is_ok());
        assert_eq!(
            verify_signature_at(key, &headers, body, timestamp + REQUEST_TTL_SECONDS + 1),
            Err(axum::http::StatusCode::UNAUTHORIZED)
        );
    }

    #[test]
    fn constant_time_compare_rejects_length_and_content_mismatches() {
        assert!(constant_time_equal(b"abc", b"abc"));
        assert!(!constant_time_equal(b"abc", b"abd"));
        assert!(!constant_time_equal(b"abc", b"ab"));
    }
}
