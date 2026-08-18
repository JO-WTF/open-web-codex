use std::sync::Arc;

use axum::{extract::Path, http::StatusCode, Extension, Json};
use open_web_codex_adapter::CodexAdapter;
use open_web_codex_approval_service::{
    bounded_identifier, project_permission_capabilities, safe_maps_credential_url, ApprovalActor,
    ApprovalService, ApprovalServiceError, PendingApprovalRecord,
};
use open_web_codex_platform_contracts::error::PlatformError;
use open_web_codex_platform_contracts::{
    ApprovalRequestSource, ApprovalSummary, DecideApprovalRequest, PendingApprovalFileChangeAction,
    PendingApprovalSubject, PendingApprovalSummary, PendingApprovalUnavailableReason,
    PendingMcpFormSummary, PendingUserInputSummary, RespondMcpFormRequest, RespondUserInputRequest,
};
use serde_json::Value;
use uuid::Uuid;

use crate::event_projection::{bounded_sanitized_text, project_agent_item_descriptor};
use crate::middleware::auth::AuthenticatedUser;

type ApiError = (StatusCode, Json<PlatformError>);

pub async fn list_pending(
    auth: AuthenticatedUser,
    Extension(approvals): Extension<Arc<ApprovalService>>,
    Extension(adapter): Extension<Arc<dyn CodexAdapter>>,
) -> Result<Json<Vec<ApprovalSummary>>, ApiError> {
    let runtime_instance_id = adapter.runtime_instance_id().await;
    approvals
        .list_pending(actor(&auth), runtime_instance_id)
        .await
        .map(Json)
        .map_err(approval_error)
}

pub async fn list_run_approval_requests(
    auth: AuthenticatedUser,
    Path(run_id): Path<Uuid>,
    Extension(approvals): Extension<Arc<ApprovalService>>,
    Extension(adapter): Extension<Arc<dyn CodexAdapter>>,
) -> Result<Json<Vec<PendingApprovalSummary>>, ApiError> {
    let records = approvals
        .list_pending_for_run(actor(&auth), adapter.runtime_instance_id().await, run_id)
        .await
        .map_err(approval_list_error)?;
    let mut summaries = Vec::with_capacity(records.len());
    for record in records {
        let approval_id = record.id;
        match project_pending_approval(record) {
            Ok(summary) => summaries.push(summary),
            Err(reason) => {
                tracing::warn!(approval_id = %approval_id, reason, "approval projection unavailable");
                return Err((
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(PlatformError::internal(
                        "Approval request projection unavailable",
                    )),
                ));
            }
        }
    }
    Ok(Json(summaries))
}

fn project_pending_approval(
    record: PendingApprovalRecord,
) -> Result<PendingApprovalSummary, &'static str> {
    let thread_id = bounded_provenance(&record.thread_id, 256).ok_or("invalid thread identity")?;
    let source = project_source(record.source);
    let subject = project_pending_subject(&record.request_type, &record.request_payload)?;
    Ok(PendingApprovalSummary {
        id: record.id,
        run_id: record.run_id,
        source,
        thread_id,
        turn_id: record
            .turn_id
            .as_deref()
            .and_then(|value| bounded_provenance(value, 256)),
        item_id: record
            .item_id
            .as_deref()
            .and_then(|value| bounded_provenance(value, 256)),
        subject,
        state: record.state,
        attempted_decision: record.attempted_decision,
        version: record.version,
        created_at: record.created_at,
    })
}

fn project_source(source: ApprovalRequestSource) -> ApprovalRequestSource {
    match source {
        ApprovalRequestSource::Root => ApprovalRequestSource::Root,
        ApprovalRequestSource::Agent {
            execution_id,
            display_title,
        } => ApprovalRequestSource::Agent {
            execution_id,
            display_title: bounded_sanitized_text(
                &Value::String(display_title),
                "displayTitle",
                80,
            )
            .unwrap_or_else(|| "Agent".to_string()),
        },
    }
}

fn project_pending_subject(
    request_type: &str,
    payload: &Value,
) -> Result<PendingApprovalSubject, &'static str> {
    let subject = match request_type {
        "item/commandExecution/requestApproval" => {
            let (action, path) = project_agent_item_descriptor("commandExecution", payload, false)
                .and_then(|descriptor| match descriptor.subject {
                    open_web_codex_platform_contracts::RuntimeAgentActivitySubject::WorkspaceAction {
                        action,
                        path,
                    } => Some((action, path)),
                    _ => None,
                })
                .unwrap_or_else(|| ("workspace_command".to_string(), None));
            PendingApprovalSubject::Command { action, path }
        }
        "item/fileChange/requestApproval" => PendingApprovalSubject::FileChange {
            action: PendingApprovalFileChangeAction::Write,
            path: None,
        },
        "item/permissions/requestApproval" => {
            match project_permission_capabilities(payload.get("permissions")) {
                Ok(capabilities) => PendingApprovalSubject::Permissions { capabilities },
                Err(reason) => PendingApprovalSubject::Unavailable { reason },
            }
        }
        "mcpServer/elicitation/request" => {
            if payload.get("mode").and_then(Value::as_str) == Some("form") {
                return Err("MCP form approval belongs to the dedicated form route");
            }
            let safe_url = payload
                .get("url")
                .and_then(Value::as_str)
                .and_then(safe_maps_credential_url)
                .map(str::to_string);
            if payload.get("mode").and_then(Value::as_str) == Some("url") {
                PendingApprovalSubject::Url {
                    server: bounded_identifier(payload.get("serverName"), 128),
                    available: safe_url.is_some(),
                }
            } else {
                PendingApprovalSubject::Unavailable {
                    reason: PendingApprovalUnavailableReason::UnsupportedRequest,
                }
            }
        }
        _ => return Err("unsupported approval subject"),
    };
    Ok(subject)
}

fn bounded_provenance(value: &str, max_len: usize) -> Option<String> {
    let value = value.trim();
    if value.is_empty()
        || value.len() > max_len
        || value.chars().any(char::is_control)
        || value.contains("://")
        || value.starts_with('/')
        || value.starts_with('\\')
        || value.as_bytes().get(1) == Some(&b':')
    {
        return None;
    }
    Some(value.to_string())
}

pub async fn list_run_user_inputs(
    auth: AuthenticatedUser,
    Path(run_id): Path<Uuid>,
    Extension(approvals): Extension<Arc<ApprovalService>>,
    Extension(adapter): Extension<Arc<dyn CodexAdapter>>,
) -> Result<Json<Vec<PendingUserInputSummary>>, ApiError> {
    let runtime_instance_id = adapter.runtime_instance_id().await;
    approvals
        .list_pending_user_inputs(actor(&auth), runtime_instance_id, Some(run_id))
        .await
        .map(Json)
        .map_err(approval_error)
}

pub async fn list_run_mcp_forms(
    auth: AuthenticatedUser,
    Path(run_id): Path<Uuid>,
    Extension(approvals): Extension<Arc<ApprovalService>>,
    Extension(adapter): Extension<Arc<dyn CodexAdapter>>,
) -> Result<Json<Vec<PendingMcpFormSummary>>, ApiError> {
    let runtime_instance_id = adapter.runtime_instance_id().await;
    approvals
        .list_pending_mcp_forms(actor(&auth), runtime_instance_id, run_id)
        .await
        .map(Json)
        .map_err(approval_error)
}

pub async fn decide(
    auth: AuthenticatedUser,
    Path(id): Path<Uuid>,
    Extension(approvals): Extension<Arc<ApprovalService>>,
    Extension(adapter): Extension<Arc<dyn CodexAdapter>>,
    Json(request): Json<DecideApprovalRequest>,
) -> Result<StatusCode, ApiError> {
    let actor = actor(&auth);
    let runtime_instance_id = adapter.runtime_instance_id().await;
    let dispatch = approvals
        .begin_decision(actor, id, runtime_instance_id, request)
        .await
        .map_err(approval_error)?;
    if adapter
        .respond_to_server_request(
            dispatch.runtime_instance_id,
            dispatch.runtime_request_id.clone(),
            dispatch.response.clone(),
        )
        .await
        .is_err()
    {
        approvals
            .mark_delivery_unknown(actor, &dispatch)
            .await
            .map_err(approval_error)?;
        return Err((
            StatusCode::BAD_GATEWAY,
            Json(PlatformError::internal(
                "Approval delivery status is unknown; inspect before retrying",
            )),
        ));
    }
    approvals
        .complete_decision(actor, &dispatch)
        .await
        .map_err(approval_error)?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn respond_user_input(
    auth: AuthenticatedUser,
    Path(id): Path<Uuid>,
    Extension(approvals): Extension<Arc<ApprovalService>>,
    Extension(adapter): Extension<Arc<dyn CodexAdapter>>,
    Json(request): Json<RespondUserInputRequest>,
) -> Result<StatusCode, ApiError> {
    let actor = actor(&auth);
    let runtime_instance_id = adapter.runtime_instance_id().await;
    let dispatch = approvals
        .begin_user_input_response(actor, id, runtime_instance_id, request)
        .await
        .map_err(approval_error)?;
    if adapter
        .respond_to_server_request(
            dispatch.runtime_instance_id,
            dispatch.runtime_request_id.clone(),
            dispatch.response.clone(),
        )
        .await
        .is_err()
    {
        approvals
            .mark_delivery_unknown(actor, &dispatch)
            .await
            .map_err(approval_error)?;
        return Err((
            StatusCode::BAD_GATEWAY,
            Json(PlatformError::internal(
                "User input delivery status is unknown; inspect before retrying",
            )),
        ));
    }
    approvals
        .complete_decision(actor, &dispatch)
        .await
        .map_err(approval_error)?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn respond_mcp_form(
    auth: AuthenticatedUser,
    Path(id): Path<Uuid>,
    Extension(approvals): Extension<Arc<ApprovalService>>,
    Extension(adapter): Extension<Arc<dyn CodexAdapter>>,
    Json(request): Json<RespondMcpFormRequest>,
) -> Result<StatusCode, ApiError> {
    let actor = actor(&auth);
    let runtime_instance_id = adapter.runtime_instance_id().await;
    let dispatch = approvals
        .begin_mcp_form_response(actor, id, runtime_instance_id, request)
        .await
        .map_err(approval_error)?;
    if adapter
        .respond_to_server_request(
            dispatch.runtime_instance_id,
            dispatch.runtime_request_id.clone(),
            dispatch.response.clone(),
        )
        .await
        .is_err()
    {
        approvals
            .mark_delivery_unknown(actor, &dispatch)
            .await
            .map_err(approval_error)?;
        return Err((
            StatusCode::BAD_GATEWAY,
            Json(PlatformError::internal(
                "MCP form delivery status is unknown; inspect before retrying",
            )),
        ));
    }
    approvals
        .complete_decision(actor, &dispatch)
        .await
        .map_err(approval_error)?;
    Ok(StatusCode::NO_CONTENT)
}

fn actor(auth: &AuthenticatedUser) -> ApprovalActor {
    ApprovalActor {
        user_id: auth.user_id,
        organization_id: auth.organization_id,
    }
}

fn approval_error(error: ApprovalServiceError) -> ApiError {
    match error {
        ApprovalServiceError::NotFound => (
            StatusCode::NOT_FOUND,
            Json(PlatformError::not_found("Approval was not found")),
        ),
        ApprovalServiceError::Conflict => (
            StatusCode::CONFLICT,
            Json(PlatformError::bad_request(
                "Approval was already decided or changed",
            )),
        ),
        ApprovalServiceError::Invalid => (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(PlatformError::bad_request("Approval request is invalid")),
        ),
        ApprovalServiceError::Database(_) => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(PlatformError::internal(
                "Approval service is temporarily unavailable",
            )),
        ),
    }
}

fn approval_list_error(error: ApprovalServiceError) -> ApiError {
    match error {
        ApprovalServiceError::Database(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(PlatformError::internal(
                "Approval requests could not be restored",
            )),
        ),
        other => approval_error(other),
    }
}

#[cfg(test)]
mod tests {
    use super::project_pending_subject;
    use open_web_codex_platform_contracts::{
        PendingApprovalCapability, PendingApprovalFileChangeAction, PendingApprovalSubject,
        PendingApprovalUnavailableReason,
    };
    use serde_json::json;

    #[test]
    fn projects_all_generic_subjects_without_raw_request_fields() {
        let command = project_pending_subject(
            "item/commandExecution/requestApproval",
            &json!({
                "command": "cat /private/server/secret.txt",
                "aggregatedOutput": "password=do-not-show",
                "commandActions": [{"type": "read", "path": "src/lib.rs"}],
                "reason": "embedded secret=do-not-show"
            }),
        )
        .unwrap();
        assert_eq!(
            command,
            PendingApprovalSubject::Command {
                action: "read".to_string(),
                path: Some("src/lib.rs".to_string()),
            }
        );
        let file = project_pending_subject(
            "item/fileChange/requestApproval",
            &json!({
                "grantRoot": "/private/server",
                "reason": "credential=https://example.com"
            }),
        )
        .unwrap();
        assert_eq!(
            file,
            PendingApprovalSubject::FileChange {
                action: PendingApprovalFileChangeAction::Write,
                path: None,
            }
        );
        let permissions = project_pending_subject(
            "item/permissions/requestApproval",
            &json!({
                "cwd": "/private/server",
                "permissions": {
                    "network": {"enabled": true},
                    "fileSystem": {"read": ["/private/read"], "write": ["/private/write"]}
                }
            }),
        )
        .unwrap();
        assert_eq!(
            permissions,
            PendingApprovalSubject::Permissions {
                capabilities: vec![
                    PendingApprovalCapability::Network,
                    PendingApprovalCapability::FilesystemRead,
                    PendingApprovalCapability::FilesystemWrite,
                ],
            }
        );
        let serialized = serde_json::to_string(&command).unwrap();
        assert!(!serialized.contains("/private/server"));
        assert!(!serialized.contains("password=do-not-show"));
        assert!(!serialized.contains("aggregatedOutput"));
    }

    #[test]
    fn keeps_unsafe_url_pending_but_marks_it_unavailable() {
        let subject = project_pending_subject(
            "mcpServer/elicitation/request",
            &json!({
                "mode": "url",
                "url": "https://example.com/credential",
                "serverName": "maps",
                "message": "secret=do-not-show"
            }),
        )
        .unwrap();
        assert_eq!(
            subject,
            PendingApprovalSubject::Url {
                server: Some("maps".to_string()),
                available: false,
            }
        );
        let encoded = serde_json::to_string(&subject).unwrap();
        assert!(!encoded.contains("example.com"));
        assert!(!encoded.contains("/private/server"));
        assert!(!encoded.contains("secret=do-not-show"));
    }

    #[test]
    fn projects_unknown_mcp_modes_and_malformed_permissions_as_typed_unavailable() {
        for payload in [
            json!({"serverName": "mcp", "mode": "future"}),
            json!({"serverName": "mcp"}),
        ] {
            assert_eq!(
                project_pending_subject("mcpServer/elicitation/request", &payload).unwrap(),
                PendingApprovalSubject::Unavailable {
                    reason: PendingApprovalUnavailableReason::UnsupportedRequest
                }
            );
        }
        assert!(
            project_pending_subject("mcpServer/elicitation/request", &json!({"mode": "form"}))
                .is_err()
        );
        assert_eq!(
            project_pending_subject(
                "item/permissions/requestApproval",
                &json!({"permissions": {"network": {"enabled": "yes"}}})
            )
            .unwrap(),
            PendingApprovalSubject::Unavailable {
                reason: PendingApprovalUnavailableReason::InvalidPermissions
            }
        );
    }
}
