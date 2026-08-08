//! Signed Work State mutation bridge for Runtime domain Agents.
//!
//! Root coordination never receives this capability.  A domain MCP server
//! must present a signed request tied to the active Run; the platform derives
//! the actor and task/workspace scope from that Run before calling the shared
//! WorkStateService.

use axum::{extract::State, http::HeaderMap, Json};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use hmac::{Hmac, Mac};
use open_web_codex_platform_contracts::{WorkOperationStatus, WorkStateMutation, WorkResourceReference};
use open_web_codex_platform_store::AppState;
use open_web_codex_work_state_service::{
    BeginWorkOperationRequest, ApplyWorkMutationRequest, WorkStateActor, WorkStateService,
    WorkStateServiceError,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use sqlx::Row;
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

type HmacSha256 = Hmac<Sha256>;

const REQUEST_TTL_SECONDS: i64 = 60;
const SIGNATURE_HEADER: &str = "x-open-web-codex-work-state-signature";
const TIMESTAMP_HEADER: &str = "x-open-web-codex-work-state-timestamp";
const REQUEST_PATH: &str = "/api/internal/work-state/v1/mutate";
const READ_REQUEST_PATH: &str = "/api/internal/work-state/v1/read";

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NamedResourceReference {
    pub name: String,
    pub resource: WorkResourceReference,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "action")]
pub enum WorkStateGateRequest {
    #[serde(rename = "begin")]
    Begin {
        #[serde(rename = "runId")]
        run_id: Uuid,
        #[serde(rename = "workStateId")]
        work_state_id: Uuid,
        kind: String,
        #[serde(rename = "idempotencyKey")]
        idempotency_key: String,
        #[serde(default, rename = "inputReferences")]
        input_references: Vec<NamedResourceReference>,
    },
    #[serde(rename = "apply")]
    Apply {
        #[serde(rename = "runId")]
        run_id: Uuid,
        #[serde(rename = "workStateId")]
        work_state_id: Uuid,
        #[serde(rename = "operationId")]
        operation_id: Uuid,
        mutation: WorkStateMutation,
    },
    #[serde(rename = "fail")]
    Fail {
        #[serde(rename = "runId")]
        run_id: Uuid,
        #[serde(rename = "workStateId")]
        work_state_id: Uuid,
        #[serde(rename = "operationId")]
        operation_id: Uuid,
        status: WorkOperationStatus,
        #[serde(rename = "failureCode")]
        failure_code: Option<String>,
        summary: Option<String>,
    },
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkStateGateResponse {
    pub schema_version: &'static str,
    pub action: &'static str,
    pub data: Value,
}

pub async fn mutate(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Result<Json<WorkStateGateResponse>, axum::http::StatusCode> {
    verify_signature(&state, &headers, &body)?;
    let request: WorkStateGateRequest =
        serde_json::from_slice(&body).map_err(|_| axum::http::StatusCode::BAD_REQUEST)?;
    let (run_id, work_state_id) = request_scope(&request);
    let actor = load_run_actor(&state, run_id, work_state_id).await?;
    let service = WorkStateService::new(state.db.clone());
    let (action, data) = match request {
        WorkStateGateRequest::Begin {
            work_state_id,
            kind,
            idempotency_key,
            input_references,
            ..
        } => {
            let operation = service
                .begin_operation(
                    &actor,
                    BeginWorkOperationRequest {
                        state_id: work_state_id,
                        kind,
                        idempotency_key,
                        input_references: input_references
                            .into_iter()
                            .map(|reference| (reference.name, reference.resource))
                            .collect(),
                    },
                )
                .await
                .map_err(service_error)?;
            ("begin", serde_json::to_value(operation).map_err(internal_error)?)
        }
        WorkStateGateRequest::Apply {
            work_state_id,
            operation_id,
            mutation,
            ..
        } => {
            let summary = service
                .apply_mutation(
                    &actor,
                    ApplyWorkMutationRequest {
                        state_id: work_state_id,
                        operation_id,
                        mutation,
                    },
                )
                .await
                .map_err(service_error)?;
            ("apply", serde_json::to_value(summary).map_err(internal_error)?)
        }
        WorkStateGateRequest::Fail {
            work_state_id,
            operation_id,
            status,
            failure_code,
            summary,
            ..
        } => {
            let result = service
                .fail_operation(
                    &actor,
                    work_state_id,
                    operation_id,
                    status,
                    failure_code.as_deref(),
                    summary.as_deref(),
                )
                .await
                .map_err(service_error)?;
            ("fail", serde_json::to_value(result).map_err(internal_error)?)
        }
    };
    Ok(Json(WorkStateGateResponse {
        schema_version: "platform-work-state-result.v1",
        action,
        data,
    }))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkStateReadRequest {
    pub run_id: Uuid,
    pub work_state_id: Uuid,
}

/// Return the bounded Work State projection to an authorized Domain Agent.
/// The Runtime receives component references, never the source rows behind them.
pub async fn read(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Result<Json<WorkStateGateResponse>, axum::http::StatusCode> {
    verify_signature_for_path(&state, &headers, &body, READ_REQUEST_PATH)?;
    let request: WorkStateReadRequest =
        serde_json::from_slice(&body).map_err(|_| axum::http::StatusCode::BAD_REQUEST)?;
    let actor = load_run_actor(&state, request.run_id, request.work_state_id).await?;
    let summary = WorkStateService::new(state.db.clone())
        .get_summary(actor.organization_id, request.work_state_id)
        .await
        .map_err(service_error)?;
    let data = serde_json::json!({
        "workStateId": summary.id,
        "revision": summary.revision,
        "state": summary.state,
        "components": summary.components,
        "blockingInputs": summary.blocking_inputs,
        "deliverables": summary.deliverables,
    });
    Ok(Json(WorkStateGateResponse {
        schema_version: "platform-work-state-result.v1",
        action: "read",
        data,
    }))
}

fn request_scope(request: &WorkStateGateRequest) -> (Uuid, Uuid) {
    match request {
        WorkStateGateRequest::Begin {
            run_id,
            work_state_id,
            ..
        }
        | WorkStateGateRequest::Apply {
            run_id,
            work_state_id,
            ..
        }
        | WorkStateGateRequest::Fail {
            run_id,
            work_state_id,
            ..
        } => (*run_id, *work_state_id),
    }
}

async fn load_run_actor(
    state: &AppState,
    run_id: Uuid,
    work_state_id: Uuid,
) -> Result<WorkStateActor, axum::http::StatusCode> {
    let row = sqlx::query(
        "SELECT run.organization_id, run.requested_profile_id, run.requested_by
         FROM runs run
         JOIN work_states state
           ON state.id = $2
          AND state.organization_id = run.organization_id
          AND state.task_id = run.task_id
          AND state.workspace_id = run.workspace_id
         WHERE run.id = $1
           AND run.requested_profile_id IS NOT NULL
           AND run.requested_by IS NOT NULL
           AND run.status IN ('pending', 'provisioning', 'running', 'cancelling', 'recovery_pending')",
    )
    .bind(run_id)
    .bind(work_state_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|_| axum::http::StatusCode::SERVICE_UNAVAILABLE)?
    .ok_or(axum::http::StatusCode::FORBIDDEN)?;
    Ok(WorkStateActor {
        organization_id: row.get("organization_id"),
        profile_id: row.get("requested_profile_id"),
        user_id: row.get("requested_by"),
    })
}

fn verify_signature(
    state: &AppState,
    headers: &HeaderMap,
    body: &[u8],
) -> Result<(), axum::http::StatusCode> {
    verify_signature_for_path(state, headers, body, REQUEST_PATH)
}

fn verify_signature_for_path(
    state: &AppState,
    headers: &HeaderMap,
    body: &[u8],
    request_path: &str,
) -> Result<(), axum::http::StatusCode> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| axum::http::StatusCode::UNAUTHORIZED)?
        .as_secs() as i64;
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
        || state.work_state_gate_key.is_empty()
    {
        return Err(axum::http::StatusCode::UNAUTHORIZED);
    }
    let body_hash = hex::encode(Sha256::digest(body));
    let message = format!("{timestamp}\nPOST\n{request_path}\n{body_hash}");
    let mut mac = HmacSha256::new_from_slice(state.work_state_gate_key.as_slice())
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

fn service_error(error: WorkStateServiceError) -> axum::http::StatusCode {
    match error {
        WorkStateServiceError::RevisionConflict | WorkStateServiceError::OperationConflict => {
            axum::http::StatusCode::CONFLICT
        }
        WorkStateServiceError::NotFound => axum::http::StatusCode::NOT_FOUND,
        WorkStateServiceError::InvalidContract(_) => axum::http::StatusCode::BAD_REQUEST,
        WorkStateServiceError::Database(_) | WorkStateServiceError::InvalidStoredData => {
            axum::http::StatusCode::INTERNAL_SERVER_ERROR
        }
    }
}

fn internal_error(_: serde_json::Error) -> axum::http::StatusCode {
    axum::http::StatusCode::INTERNAL_SERVER_ERROR
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_scope_is_typed_and_does_not_depend_on_agent_names() {
        let run_id = Uuid::now_v7();
        let state_id = Uuid::now_v7();
        let request = WorkStateGateRequest::Begin {
            run_id,
            work_state_id: state_id,
            kind: "normalize".to_string(),
            idempotency_key: "normalize-1".to_string(),
            input_references: Vec::new(),
        };
        assert_eq!(request_scope(&request), (run_id, state_id));
    }

    #[test]
    fn mutation_requests_decode_the_camel_case_mcp_contract() {
        let run_id = Uuid::now_v7();
        let state_id = Uuid::now_v7();
        let request = serde_json::json!({
            "action": "begin",
            "runId": run_id,
            "workStateId": state_id,
            "kind": "data_requirements",
            "idempotencyKey": "assignment-1",
            "inputReferences": []
        });
        let decoded: WorkStateGateRequest = serde_json::from_value(request).unwrap();
        assert_eq!(request_scope(&decoded), (run_id, state_id));
    }

    #[test]
    fn constant_time_compare_rejects_mismatches() {
        assert!(constant_time_equal(b"abc", b"abc"));
        assert!(!constant_time_equal(b"abc", b"abd"));
        assert!(!constant_time_equal(b"abc", b"ab"));
    }
}
