use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Extension, Json,
};
use chrono::{Duration, Utc};
use open_web_codex_adapter::CodexAdapter;
use open_web_codex_platform_contracts::error::PlatformError;
use open_web_codex_platform_contracts::{Approval, ApprovalDecisionRequest, ApprovalDecisionResponse};
use open_web_codex_platform_store::AppState;
use serde_json::json;
use sqlx::Row;
use uuid::Uuid;

use crate::middleware::auth::AuthenticatedUser;

type ApiResult<T> = Result<Json<T>, (StatusCode, Json<PlatformError>)>;

/// GET /api/approvals?run_id=...
pub async fn list_approvals(
    State(state): State<AppState>,
    _auth: AuthenticatedUser,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> ApiResult<Vec<Approval>> {
    let run_id = params
        .get("run_id")
        .and_then(|value| Uuid::parse_str(value).ok());

    let rows = if let Some(run_id) = run_id {
        sqlx::query(
            "SELECT id, run_id, request_type, request_payload, status, codex_request_id, \
                    workspace_id, thread_id, decision, decided_by, decided_at, created_at, expires_at \
             FROM approvals WHERE run_id = $1 ORDER BY created_at ASC",
        )
        .bind(run_id)
        .fetch_all(&state.db)
        .await
    } else {
        sqlx::query(
            "SELECT id, run_id, request_type, request_payload, status, codex_request_id, \
                    workspace_id, thread_id, decision, decided_by, decided_at, created_at, expires_at \
             FROM approvals WHERE status = 'pending' ORDER BY created_at ASC",
        )
        .fetch_all(&state.db)
        .await
    }
    .map_err(|error| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(PlatformError::internal(format!("{error}"))),
        )
    })?;

    Ok(Json(rows.iter().map(map_approval_row).collect()))
}

/// POST /api/approvals/:id/decision — CAS decision then respond to Codex.
pub async fn decide_approval(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(approval_id): Path<Uuid>,
    Extension(adapter): Extension<Arc<dyn CodexAdapter>>,
    Json(body): Json<ApprovalDecisionRequest>,
) -> ApiResult<ApprovalDecisionResponse> {
    if !matches!(body.decision.as_str(), "approved" | "rejected") {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(PlatformError::bad_request("decision must be approved or rejected")),
        ));
    }

    let mut transaction = state.db.begin().await.map_err(|error| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(PlatformError::internal(format!("{error}"))),
        )
    })?;

    let row = sqlx::query(
        "UPDATE approvals
         SET status = $1,
             decision = $2,
             decided_by = $3,
             decided_at = now()
         WHERE id = $4
           AND status = 'pending'
           AND (expires_at IS NULL OR expires_at > now())
         RETURNING id, run_id, request_type, request_payload, status, codex_request_id, \
                   workspace_id, thread_id, decision, decided_by, decided_at, created_at, expires_at",
    )
    .bind(if body.decision == "approved" {
        "approved"
    } else {
        "rejected"
    })
    .bind(&body.decision)
    .bind(auth.user_id)
    .bind(approval_id)
    .fetch_optional(&mut *transaction)
    .await
    .map_err(|error| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(PlatformError::internal(format!("{error}"))),
        )
    })?;

    let Some(row) = row else {
        return Err((
            StatusCode::CONFLICT,
            Json(PlatformError::bad_request(
                "approval is not pending or has expired",
            )),
        ));
    };

    let run_id: Uuid = row.get("run_id");
    let workspace_id: Option<String> = row.get("workspace_id");
    let codex_request_id: Option<String> = row.get("codex_request_id");

    sqlx::query(
        "UPDATE runs SET status = 'running', updated_at = now()
         WHERE id = $1 AND status = 'waiting_approval'",
    )
    .bind(run_id)
    .execute(&mut *transaction)
    .await
    .map_err(|error| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(PlatformError::internal(format!("{error}"))),
        )
    })?;

    transaction.commit().await.map_err(|error| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(PlatformError::internal(format!("{error}"))),
        )
    })?;

    if let (Some(workspace_id), Some(request_id)) = (workspace_id, codex_request_id) {
        let result = if body.decision == "approved" {
            json!({ "approved": true })
        } else {
            json!({ "approved": false })
        };
        adapter
            .rpc(
                "respond_to_server_request",
                json!({
                    "workspaceId": workspace_id,
                    "requestId": request_id,
                    "result": result,
                }),
            )
            .await
            .map_err(|error| {
                (
                    StatusCode::BAD_GATEWAY,
                    Json(PlatformError::internal(format!("adapter error: {error}"))),
                )
            })?;
    }

    Ok(Json(ApprovalDecisionResponse {
        approval: map_approval_row(&row),
    }))
}

pub fn map_approval_row(row: &sqlx::postgres::PgRow) -> Approval {
    Approval {
        id: row.get("id"),
        run_id: row.get("run_id"),
        request_type: row.get("request_type"),
        request_payload: row.get("request_payload"),
        status: row.get("status"),
        codex_request_id: row.get("codex_request_id"),
        workspace_id: row.get("workspace_id"),
        thread_id: row.get("thread_id"),
        decision: row.get("decision"),
        decided_by: row.get("decided_by"),
        decided_at: row.get("decided_at"),
        created_at: row.get("created_at"),
        expires_at: row.get("expires_at"),
    }
}

pub fn approval_expires_at() -> chrono::DateTime<Utc> {
    Utc::now() + Duration::hours(24)
}

pub fn is_approval_method(method: &str) -> bool {
    method.ends_with("requestApproval") || method == "item/tool/requestUserInput"
}
