//! Durable, Profile-scoped approval orchestration.
//!
//! Runtime request ids and complete app-server payloads remain server-side.
//! Browser DTOs expose only the platform approval id, optimistic version and a
//! small authorized projection needed to make a decision.

use open_web_codex_platform_contracts::{
    ApprovalDecision, ApprovalRequestSource, ApprovalSummary, DecideApprovalRequest,
    McpFormFieldSchema, McpFormFieldSummary, McpFormOptionSummary, McpFormRequestSource,
    McpFormResponseAction, PendingApprovalState, PendingMcpFormSummary, PendingUserInputSummary,
    RespondMcpFormRequest, RespondUserInputRequest, UserInputOptionSummary,
    UserInputQuestionSummary, UserInputRequestSource,
};
use serde_json::{json, Value};
use sqlx::{PgPool, Row};
use std::collections::{BTreeMap, BTreeSet};
use thiserror::Error;
use uuid::Uuid;

const COMMAND_APPROVAL: &str = "item/commandExecution/requestApproval";
const FILE_APPROVAL: &str = "item/fileChange/requestApproval";
const MCP_ELICITATION_REQUEST: &str = "mcpServer/elicitation/request";
const PERMISSIONS_APPROVAL: &str = "item/permissions/requestApproval";
const USER_INPUT_REQUEST: &str = "item/tool/requestUserInput";
const MAX_MCP_FORM_FIELDS: usize = 32;
const MAX_MCP_FORM_OPTIONS: usize = 64;
const MAX_MCP_FORM_JSON_BYTES: usize = 64 * 1024;
const MAX_MCP_FORM_NAME_BYTES: usize = 128;
const MAX_MCP_FORM_TITLE_BYTES: usize = 160;
const MAX_MCP_FORM_DESCRIPTION_BYTES: usize = 1_000;
const MAX_MCP_FORM_VALUE_BYTES: usize = 8 * 1024;

#[derive(Clone, Copy, Debug)]
pub struct ApprovalActor {
    pub user_id: Uuid,
    pub organization_id: Uuid,
}

/// Server-only pending approval row; payload is never serialized as a browser DTO.
#[derive(Debug, Clone)]
pub struct PendingApprovalRecord {
    pub id: Uuid,
    pub run_id: Uuid,
    pub source: ApprovalRequestSource,
    pub thread_id: String,
    pub turn_id: Option<String>,
    pub item_id: Option<String>,
    pub request_type: String,
    pub request_payload: Value,
    pub state: PendingApprovalState,
    pub version: i64,
    pub created_at: sqlx::types::chrono::DateTime<sqlx::types::chrono::Utc>,
}

#[derive(Debug)]
pub struct ApprovalDispatch {
    pub approval_id: Uuid,
    pub runtime_instance_id: Uuid,
    pub runtime_request_id: Value,
    pub response: Value,
    pub audit_metadata: Value,
    pub dispatch_version: i64,
    terminal_state: &'static str,
    decision: &'static str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResolvedApproval {
    pub approval_id: Uuid,
    pub thread_id: String,
    pub turn_id: Option<String>,
    pub item_id: Option<String>,
    pub request_type: String,
    pub request_mode: Option<String>,
    pub outcome: ApprovalOutcome,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ApprovalOutcome {
    Accepted,
    Declined,
    Answered,
    Cancelled,
}

impl ApprovalOutcome {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Accepted => "accepted",
            Self::Declined => "declined",
            Self::Answered => "answered",
            Self::Cancelled => "cancelled",
        }
    }
}

#[derive(Debug, Error)]
pub enum ApprovalServiceError {
    #[error("Approval was not found")]
    NotFound,
    #[error("Approval was already decided or its version changed")]
    Conflict,
    #[error("Approval request is invalid")]
    Invalid,
    #[error("Approval database operation failed: {0}")]
    Database(#[from] sqlx::Error),
}

#[derive(Clone)]
pub struct ApprovalService {
    db: PgPool,
    runtime_key: String,
}

impl ApprovalService {
    pub fn new(db: PgPool, runtime_key: impl Into<String>) -> Self {
        Self {
            db,
            runtime_key: runtime_key.into(),
        }
    }

    /// Persist an approval server request before its projection is broadcast.
    pub async fn capture_event_frame(
        &self,
        frame: &[u8],
    ) -> Result<Option<Uuid>, ApprovalServiceError> {
        let payload = frame
            .strip_prefix(b"data: ")
            .and_then(|value| value.strip_suffix(b"\n\n"))
            .ok_or(ApprovalServiceError::Invalid)?;
        let envelope: Value =
            serde_json::from_slice(payload).map_err(|_| ApprovalServiceError::Invalid)?;
        let runtime_instance_id = envelope
            .pointer("/params/runtime_instance_id")
            .and_then(Value::as_str)
            .and_then(|value| Uuid::parse_str(value).ok())
            .ok_or(ApprovalServiceError::Invalid)?;
        let message = envelope
            .pointer("/params/message")
            .ok_or(ApprovalServiceError::Invalid)?;
        self.capture_message(runtime_instance_id, message).await
    }

    pub async fn capture_message(
        &self,
        runtime_instance_id: Uuid,
        message: &Value,
    ) -> Result<Option<Uuid>, ApprovalServiceError> {
        let Some(method) = message.get("method").and_then(Value::as_str) else {
            return Ok(None);
        };
        if !matches!(
            method,
            COMMAND_APPROVAL
                | FILE_APPROVAL
                | MCP_ELICITATION_REQUEST
                | PERMISSIONS_APPROVAL
                | USER_INPUT_REQUEST
        ) {
            return Ok(None);
        }
        let request_id = message.get("id").ok_or(ApprovalServiceError::Invalid)?;
        if !(request_id.is_u64() || request_id.is_i64() || request_id.is_string()) {
            return Err(ApprovalServiceError::Invalid);
        }
        let params = message
            .get("params")
            .cloned()
            .ok_or(ApprovalServiceError::Invalid)?;
        let thread_id = params
            .get("threadId")
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
            .ok_or(ApprovalServiceError::Invalid)?
            .to_string();
        let runtime_request_id =
            serde_json::to_string(request_id).map_err(|_| ApprovalServiceError::Invalid)?;
        let stored_payload = approval_payload(method, &params);

        let row = sqlx::query(
            "INSERT INTO approvals \
             (run_id, request_type, request_payload, organization_id, profile_id, thread_id, \
              runtime_instance_id, runtime_request_id, state, version) \
             SELECT r.id, $1, $2, p.organization_id, p.id, $3, $4, $5, 'pending', 0 \
             FROM profiles p \
             JOIN runs r ON r.requested_profile_id = p.id \
             LEFT JOIN runtime_agent_projections agent \
               ON agent.root_run_id = r.id AND agent.profile_id = p.id \
              AND agent.thread_id = $3 \
             WHERE p.runtime_key = $6 AND p.status = 'active' \
               AND (r.codex_thread_id = $3 OR agent.thread_id IS NOT NULL) \
             ORDER BY r.created_at DESC LIMIT 1 \
             ON CONFLICT (profile_id, runtime_instance_id, runtime_request_id) \
             WHERE runtime_instance_id IS NOT NULL AND runtime_request_id IS NOT NULL \
             DO NOTHING RETURNING id",
        )
        .bind(method)
        .bind(stored_payload)
        .bind(&thread_id)
        .bind(runtime_instance_id)
        .bind(&runtime_request_id)
        .bind(&self.runtime_key)
        .fetch_optional(&self.db)
        .await?;
        if let Some(row) = row {
            return Ok(Some(row.get("id")));
        }
        let existing = sqlx::query(
            "SELECT a.id FROM approvals a JOIN profiles p ON p.id = a.profile_id \
             WHERE p.runtime_key = $1 AND a.runtime_instance_id = $2 \
               AND a.runtime_request_id = $3",
        )
        .bind(&self.runtime_key)
        .bind(runtime_instance_id)
        .bind(runtime_request_id)
        .fetch_optional(&self.db)
        .await?;
        existing
            .map(|row| Some(row.get("id")))
            .ok_or(ApprovalServiceError::NotFound)
    }

    pub async fn list_pending(
        &self,
        actor: ApprovalActor,
        runtime_instance_id: Uuid,
    ) -> Result<Vec<ApprovalSummary>, ApprovalServiceError> {
        self.cancel_stale_runtime_requests(runtime_instance_id)
            .await?;
        let rows = sqlx::query(
            "SELECT a.id, a.run_id, a.thread_id, a.request_type, a.request_payload, \
                    a.state, a.version, a.created_at, a.decided_at \
             FROM approvals a JOIN profiles p ON p.id = a.profile_id \
             WHERE a.organization_id = $1 AND p.owner_user_id = $2 \
               AND p.runtime_key = $3 AND a.state IN ('pending', 'dispatching', 'delivery_unknown') \
               AND a.request_type <> $5 \
               AND NOT (a.request_type = $6 AND a.request_payload->>'mode' = 'form') \
               AND a.runtime_instance_id = $4 \
             ORDER BY a.created_at, a.id",
        )
        .bind(actor.organization_id)
        .bind(actor.user_id)
        .bind(&self.runtime_key)
        .bind(runtime_instance_id)
        .bind(USER_INPUT_REQUEST)
        .bind(MCP_ELICITATION_REQUEST)
        .fetch_all(&self.db)
        .await?;
        Ok(rows.iter().map(summary_from_row).collect())
    }

    pub async fn list_pending_for_run(
        &self,
        actor: ApprovalActor,
        runtime_instance_id: Uuid,
        run_id: Uuid,
    ) -> Result<Vec<PendingApprovalRecord>, ApprovalServiceError> {
        let authorized = sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS(SELECT 1 FROM runs r \
             JOIN profiles p ON p.id = r.requested_profile_id \
             WHERE r.id = $1 AND r.organization_id = $2 AND p.organization_id = $2 \
               AND p.owner_user_id = $3 AND p.runtime_key = $4)",
        )
        .bind(run_id)
        .bind(actor.organization_id)
        .bind(actor.user_id)
        .bind(&self.runtime_key)
        .fetch_one(&self.db)
        .await?;
        if !authorized {
            return Err(ApprovalServiceError::NotFound);
        }

        self.cancel_stale_runtime_requests(runtime_instance_id)
            .await?;
        let rows = sqlx::query(
            "SELECT a.id, a.run_id, a.thread_id, a.request_type, a.request_payload, \
                    a.state, a.version, a.created_at \
             FROM approvals a \
             JOIN profiles p ON p.id = a.profile_id \
             JOIN runs r ON r.id = a.run_id AND r.requested_profile_id = p.id \
             WHERE a.run_id = $1 AND r.id = $1 AND a.organization_id = $2 \
               AND r.organization_id = $2 AND p.organization_id = $2 \
               AND p.owner_user_id = $3 AND p.runtime_key = $4 \
               AND a.runtime_instance_id = $5 \
               AND a.state IN ('pending', 'dispatching', 'delivery_unknown') \
               AND (a.request_type IN ($6, $7, $8) \
                    OR (a.request_type = $9 AND a.request_payload->>'mode' = 'url')) \
             ORDER BY a.created_at, a.id",
        )
        .bind(run_id)
        .bind(actor.organization_id)
        .bind(actor.user_id)
        .bind(&self.runtime_key)
        .bind(runtime_instance_id)
        .bind(COMMAND_APPROVAL)
        .bind(FILE_APPROVAL)
        .bind(PERMISSIONS_APPROVAL)
        .bind(MCP_ELICITATION_REQUEST)
        .fetch_all(&self.db)
        .await?;

        let mut records = Vec::with_capacity(rows.len());
        for row in rows {
            let payload: Value = row.get("request_payload");
            let thread_id = row
                .get::<Option<String>, _>("thread_id")
                .filter(|value| !value.trim().is_empty())
                .ok_or(ApprovalServiceError::Invalid)?;
            let turn_id = payload
                .get("turnId")
                .and_then(Value::as_str)
                .filter(|value| !value.trim().is_empty())
                .map(str::to_string);
            let item_id = payload
                .get("itemId")
                .and_then(Value::as_str)
                .filter(|value| !value.trim().is_empty())
                .map(str::to_string);
            let source = self
                .resolve_approval_source(actor, run_id, &thread_id, turn_id.as_deref())
                .await?
                .unwrap_or_else(|| ApprovalRequestSource::Agent {
                    execution_id: None,
                    display_title: "Agent".to_string(),
                });
            let state = pending_approval_state(row.get("state"))?;
            records.push(PendingApprovalRecord {
                id: row.get("id"),
                run_id: row.get("run_id"),
                source,
                thread_id,
                turn_id,
                item_id,
                request_type: row.get("request_type"),
                request_payload: payload,
                state,
                version: row.get("version"),
                created_at: row.get("created_at"),
            });
        }
        Ok(records)
    }

    pub async fn list_pending_user_inputs(
        &self,
        actor: ApprovalActor,
        runtime_instance_id: Uuid,
        run_id: Option<Uuid>,
    ) -> Result<Vec<PendingUserInputSummary>, ApprovalServiceError> {
        self.cancel_stale_runtime_requests(runtime_instance_id)
            .await?;
        let rows = sqlx::query(
            "SELECT a.id, a.run_id, a.thread_id, a.request_payload, a.state, a.version, \
                    a.created_at \
             FROM approvals a JOIN profiles p ON p.id = a.profile_id \
             WHERE a.organization_id = $1 AND p.owner_user_id = $2 \
               AND p.runtime_key = $3 AND a.request_type = $4 \
               AND a.state IN ('pending', 'dispatching', 'delivery_unknown') \
               AND a.runtime_instance_id = $5 \
               AND ($6::uuid IS NULL OR a.run_id = $6) \
             ORDER BY a.created_at, a.id",
        )
        .bind(actor.organization_id)
        .bind(actor.user_id)
        .bind(&self.runtime_key)
        .bind(USER_INPUT_REQUEST)
        .bind(runtime_instance_id)
        .bind(run_id)
        .fetch_all(&self.db)
        .await?;

        let mut summaries = Vec::with_capacity(rows.len());
        for row in rows {
            let payload: Value = row.get("request_payload");
            let Some(source) = self
                .resolve_user_input_source(
                    actor,
                    row.get("run_id"),
                    row.get("thread_id"),
                    payload.get("turnId").and_then(Value::as_str),
                )
                .await?
            else {
                continue;
            };
            summaries.push(PendingUserInputSummary {
                id: row.get("id"),
                run_id: row.get("run_id"),
                source,
                questions: parse_user_input_questions(&payload)?,
                state: row.get("state"),
                version: row.get("version"),
                auto_resolution_ms: payload.get("autoResolutionMs").and_then(Value::as_i64),
                created_at: row.get("created_at"),
            });
        }
        Ok(summaries)
    }

    pub async fn list_pending_mcp_forms(
        &self,
        actor: ApprovalActor,
        runtime_instance_id: Uuid,
        run_id: Uuid,
    ) -> Result<Vec<PendingMcpFormSummary>, ApprovalServiceError> {
        self.cancel_stale_runtime_requests(runtime_instance_id)
            .await?;
        let authorized = sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS(SELECT 1 FROM runs r \
             JOIN profiles p ON p.id = r.requested_profile_id \
             WHERE r.id = $1 AND r.organization_id = $2 AND p.organization_id = $2 \
               AND p.owner_user_id = $3 AND p.runtime_key = $4)",
        )
        .bind(run_id)
        .bind(actor.organization_id)
        .bind(actor.user_id)
        .bind(&self.runtime_key)
        .fetch_one(&self.db)
        .await?;
        if !authorized {
            return Err(ApprovalServiceError::NotFound);
        }
        let rows = sqlx::query(
            "SELECT a.id, a.run_id, a.thread_id, a.request_payload, a.state, a.version, \
                    a.created_at \
             FROM approvals a \
             JOIN profiles p ON p.id = a.profile_id \
             JOIN runs r ON r.id = a.run_id AND r.requested_profile_id = p.id \
             WHERE a.run_id = $1 AND a.organization_id = $2 AND r.organization_id = $2 \
               AND p.owner_user_id = $3 AND p.runtime_key = $4 \
               AND a.request_type = $5 AND a.request_payload->>'mode' = 'form' \
               AND a.state IN ('pending', 'dispatching', 'delivery_unknown') \
               AND a.runtime_instance_id = $6 \
             ORDER BY a.created_at, a.id",
        )
        .bind(run_id)
        .bind(actor.organization_id)
        .bind(actor.user_id)
        .bind(&self.runtime_key)
        .bind(MCP_ELICITATION_REQUEST)
        .bind(runtime_instance_id)
        .fetch_all(&self.db)
        .await?;

        let mut summaries = Vec::with_capacity(rows.len());
        for row in rows {
            let payload: Value = row.get("request_payload");
            let source = self
                .resolve_mcp_form_source(actor, run_id, row.get("thread_id"))
                .await?
                .ok_or(ApprovalServiceError::Invalid)?;
            summaries.push(PendingMcpFormSummary {
                id: row.get("id"),
                run_id: row.get("run_id"),
                source,
                server_name: bounded_input_text(payload.get("serverName"), 160)?,
                message: bounded_input_text(payload.get("message"), 2_000)?,
                fields: parse_mcp_form_fields(&payload)?,
                state: row.get("state"),
                version: row.get("version"),
                created_at: row.get("created_at"),
            });
        }
        Ok(summaries)
    }

    async fn resolve_approval_source(
        &self,
        actor: ApprovalActor,
        run_id: Uuid,
        thread_id: &str,
        turn_id: Option<&str>,
    ) -> Result<Option<ApprovalRequestSource>, ApprovalServiceError> {
        let is_root = sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS(SELECT 1 FROM runs r \
             WHERE r.id = $1 AND r.organization_id = $2 AND r.codex_thread_id = $3)",
        )
        .bind(run_id)
        .bind(actor.organization_id)
        .bind(thread_id)
        .fetch_one(&self.db)
        .await?;
        if is_root {
            return Ok(Some(ApprovalRequestSource::Root));
        }

        let row = sqlx::query(
            "SELECT execution.id, execution.display_title \
             FROM runtime_agent_execution_projections execution \
             JOIN runs r ON r.id = execution.root_run_id \
             WHERE execution.root_run_id = $1 AND execution.organization_id = $2 \
               AND r.organization_id = $2 AND execution.agent_thread_id = $3 \
               AND ($4::text IS NULL OR execution.turn_id = $4) \
             ORDER BY (execution.turn_id = $4) DESC, execution.ordinal DESC \
             LIMIT 1",
        )
        .bind(run_id)
        .bind(actor.organization_id)
        .bind(thread_id)
        .bind(turn_id)
        .fetch_optional(&self.db)
        .await?;
        if let Some(row) = row {
            return Ok(Some(ApprovalRequestSource::Agent {
                execution_id: Some(row.get("id")),
                display_title: row.get("display_title"),
            }));
        }

        // Runtime agent projections can be persisted before the execution
        // projection during event races. Keep the approval recoverable with a
        // bounded, non-authoritative display fallback until the execution row
        // arrives; the optional execution id makes that uncertainty explicit.
        let projection = sqlx::query(
            "SELECT agent.agent_nickname, agent.agent_role \
             FROM runtime_agent_projections agent \
             JOIN runs r ON r.id = agent.root_run_id \
             WHERE agent.root_run_id = $1 AND agent.organization_id = $2 \
               AND r.organization_id = $2 AND agent.thread_id = $3 \
             LIMIT 1",
        )
        .bind(run_id)
        .bind(actor.organization_id)
        .bind(thread_id)
        .fetch_optional(&self.db)
        .await?;
        let display_title = projection
            .and_then(|row| {
                row.get::<Option<String>, _>("agent_nickname")
                    .or_else(|| row.get::<Option<String>, _>("agent_role"))
            })
            .unwrap_or_else(|| "Agent".to_string());
        Ok(Some(ApprovalRequestSource::Agent {
            execution_id: None,
            display_title,
        }))
    }

    async fn resolve_user_input_source(
        &self,
        actor: ApprovalActor,
        run_id: Uuid,
        thread_id: &str,
        turn_id: Option<&str>,
    ) -> Result<Option<UserInputRequestSource>, ApprovalServiceError> {
        Ok(
            match self
                .resolve_approval_source(actor, run_id, thread_id, turn_id)
                .await?
            {
                Some(ApprovalRequestSource::Root) => Some(UserInputRequestSource::Root),
                Some(ApprovalRequestSource::Agent {
                    execution_id: Some(execution_id),
                    display_title,
                }) => Some(UserInputRequestSource::Agent {
                    execution_id,
                    display_title,
                }),
                Some(ApprovalRequestSource::Agent {
                    execution_id: None, ..
                })
                | None => None,
            },
        )
    }

    async fn resolve_mcp_form_source(
        &self,
        actor: ApprovalActor,
        run_id: Uuid,
        thread_id: &str,
    ) -> Result<Option<McpFormRequestSource>, ApprovalServiceError> {
        let is_root = sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS(SELECT 1 FROM runs r \
             WHERE r.id = $1 AND r.organization_id = $2 AND r.codex_thread_id = $3)",
        )
        .bind(run_id)
        .bind(actor.organization_id)
        .bind(thread_id)
        .fetch_one(&self.db)
        .await?;
        if is_root {
            return Ok(Some(McpFormRequestSource::Root));
        }

        let row = sqlx::query(
            "SELECT execution.id, execution.display_title \
             FROM runtime_agent_execution_projections execution \
             JOIN runs r ON r.id = execution.root_run_id \
             WHERE execution.root_run_id = $1 AND execution.organization_id = $2 \
               AND r.organization_id = $2 AND execution.agent_thread_id = $3 \
             ORDER BY execution.ordinal DESC LIMIT 1",
        )
        .bind(run_id)
        .bind(actor.organization_id)
        .bind(thread_id)
        .fetch_optional(&self.db)
        .await?;
        Ok(row.map(|row| McpFormRequestSource::Agent {
            execution_id: row.get("id"),
            display_title: row.get("display_title"),
        }))
    }

    /// Resolve a Runtime request id to its browser-safe platform identity.
    ///
    /// Runtime resolution can race the HTTP response path or can happen
    /// without a browser decision (for example after interruption or Runtime
    /// auto-resolution). In either case, no active approval may remain visible
    /// after the Runtime says the request is resolved. Existing decisions are
    /// retained and determine the terminal state; an undecided request becomes
    /// cancelled.
    pub async fn resolve_runtime_request(
        &self,
        runtime_instance_id: Uuid,
        thread_id: &str,
        runtime_request_id: &Value,
    ) -> Result<Option<ResolvedApproval>, ApprovalServiceError> {
        if thread_id.trim().is_empty() {
            return Err(ApprovalServiceError::Invalid);
        }
        if !(runtime_request_id.is_u64()
            || runtime_request_id.is_i64()
            || runtime_request_id.is_string())
        {
            return Err(ApprovalServiceError::Invalid);
        }
        let runtime_request_id =
            serde_json::to_string(runtime_request_id).map_err(|_| ApprovalServiceError::Invalid)?;
        let mut transaction = self.db.begin().await?;
        let row = sqlx::query(
            "SELECT a.id, a.thread_id, a.request_type, a.request_payload, a.state, a.decision \
             FROM approvals a JOIN profiles p ON p.id = a.profile_id \
             WHERE p.runtime_key = $1 AND a.runtime_instance_id = $2 \
               AND a.thread_id = $3 AND a.runtime_request_id = $4 \
             FOR UPDATE OF a",
        )
        .bind(&self.runtime_key)
        .bind(runtime_instance_id)
        .bind(thread_id)
        .bind(&runtime_request_id)
        .fetch_optional(&mut *transaction)
        .await?;
        let Some(row) = row else {
            transaction.commit().await?;
            return Ok(None);
        };

        let approval_id: Uuid = row.get("id");
        let thread_id: Option<String> = row.get("thread_id");
        let thread_id = thread_id
            .filter(|value| !value.is_empty())
            .ok_or(ApprovalServiceError::Invalid)?;
        let request_payload: Value = row.get("request_payload");
        let request_type: String = row.get("request_type");
        let turn_id = request_payload
            .get("turnId")
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
            .map(str::to_string);
        let item_id = request_payload
            .get("itemId")
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
            .map(str::to_string);
        let state: String = row.get("state");
        let decision: Option<String> = row.get("decision");
        let resolved_state =
            if let Some(terminal_state) = resolved_terminal_state(&state, decision.as_deref()) {
                sqlx::query(
                    "UPDATE approvals SET state = $1, decided_at = COALESCE(decided_at, now()), \
                 version = version + 1 WHERE id = $2",
                )
                .bind(terminal_state)
                .bind(approval_id)
                .execute(&mut *transaction)
                .await?;
                terminal_state
            } else {
                state.as_str()
            };
        let outcome = approval_outcome(resolved_state).ok_or(ApprovalServiceError::Invalid)?;
        transaction.commit().await?;

        Ok(Some(ResolvedApproval {
            approval_id,
            thread_id,
            turn_id,
            item_id,
            request_type,
            request_mode: request_payload
                .get("mode")
                .and_then(Value::as_str)
                .map(str::to_string),
            outcome,
        }))
    }

    pub async fn begin_decision(
        &self,
        actor: ApprovalActor,
        approval_id: Uuid,
        runtime_instance_id: Uuid,
        request: DecideApprovalRequest,
    ) -> Result<ApprovalDispatch, ApprovalServiceError> {
        let mut transaction = self.db.begin().await?;
        let row = sqlx::query(
            "SELECT a.runtime_instance_id, a.runtime_request_id, a.request_type, \
                    a.request_payload, a.state, a.version, a.decision \
             FROM approvals a JOIN profiles p ON p.id = a.profile_id \
             WHERE a.id = $1 AND a.organization_id = $2 AND p.owner_user_id = $3 \
               AND p.runtime_key = $4 FOR UPDATE",
        )
        .bind(approval_id)
        .bind(actor.organization_id)
        .bind(actor.user_id)
        .bind(&self.runtime_key)
        .fetch_optional(&mut *transaction)
        .await?
        .ok_or(ApprovalServiceError::NotFound)?;
        let state: String = row.get("state");
        let version: i64 = row.get("version");
        let stored_runtime_instance_id: Option<Uuid> = row.get("runtime_instance_id");
        if stored_runtime_instance_id != Some(runtime_instance_id) || version != request.version {
            return Err(ApprovalServiceError::Conflict);
        }
        let request_type: String = row.get("request_type");
        let payload: Value = row.get("request_payload");
        let (response, terminal_state, decision) =
            approval_response(&request_type, &payload, request.decision)?;
        let previous_decision: Option<String> = row.get("decision");
        if state != "pending"
            && !(state == "delivery_unknown" && previous_decision.as_deref() == Some(decision))
        {
            return Err(ApprovalServiceError::Conflict);
        }
        let dispatch_version = version + 1;
        sqlx::query(
            "UPDATE approvals SET state = 'dispatching', decision = $1, decided_by = $2, \
                    decided_at = now(), version = $3 WHERE id = $4",
        )
        .bind(decision)
        .bind(actor.user_id)
        .bind(dispatch_version)
        .bind(approval_id)
        .execute(&mut *transaction)
        .await?;
        transaction.commit().await?;

        let runtime_request_id: String = row.get("runtime_request_id");
        Ok(ApprovalDispatch {
            approval_id,
            runtime_instance_id,
            runtime_request_id: serde_json::from_str(&runtime_request_id)
                .map_err(|_| ApprovalServiceError::Invalid)?,
            response,
            audit_metadata: json!({ "decision": decision }),
            dispatch_version,
            terminal_state,
            decision,
        })
    }

    pub async fn begin_user_input_response(
        &self,
        actor: ApprovalActor,
        approval_id: Uuid,
        runtime_instance_id: Uuid,
        request: RespondUserInputRequest,
    ) -> Result<ApprovalDispatch, ApprovalServiceError> {
        let mut transaction = self.db.begin().await?;
        let row = sqlx::query(
            "SELECT a.runtime_instance_id, a.runtime_request_id, a.request_type, \
                    a.request_payload, a.state, a.version, a.decision \
             FROM approvals a JOIN profiles p ON p.id = a.profile_id \
             WHERE a.id = $1 AND a.organization_id = $2 AND p.owner_user_id = $3 \
               AND p.runtime_key = $4 FOR UPDATE",
        )
        .bind(approval_id)
        .bind(actor.organization_id)
        .bind(actor.user_id)
        .bind(&self.runtime_key)
        .fetch_optional(&mut *transaction)
        .await?
        .ok_or(ApprovalServiceError::NotFound)?;
        let state: String = row.get("state");
        let version: i64 = row.get("version");
        let request_type: String = row.get("request_type");
        let payload: Value = row.get("request_payload");
        let stored_runtime_instance_id: Option<Uuid> = row.get("runtime_instance_id");
        let previous_decision: Option<String> = row.get("decision");
        if stored_runtime_instance_id != Some(runtime_instance_id)
            || version != request.version
            || (state != "pending"
                && !(state == "delivery_unknown"
                    && previous_decision.as_deref() == Some("answered")))
        {
            return Err(ApprovalServiceError::Conflict);
        }
        if request_type != USER_INPUT_REQUEST {
            return Err(ApprovalServiceError::Invalid);
        }
        validate_user_input_answers(&payload, &request)?;
        let questions = parse_user_input_questions(&payload)?;
        let audit_metadata = redact_user_input_decision(&questions, &request.answers);
        let dispatch_version = version + 1;
        sqlx::query(
            "UPDATE approvals SET state = 'dispatching', decision = 'answered', decided_by = $1, \
                    decided_at = now(), version = $2 WHERE id = $3",
        )
        .bind(actor.user_id)
        .bind(dispatch_version)
        .bind(approval_id)
        .execute(&mut *transaction)
        .await?;
        transaction.commit().await?;
        let runtime_request_id: String = row.get("runtime_request_id");
        Ok(ApprovalDispatch {
            approval_id,
            runtime_instance_id,
            runtime_request_id: serde_json::from_str(&runtime_request_id)
                .map_err(|_| ApprovalServiceError::Invalid)?,
            response: json!({ "answers": request.answers }),
            audit_metadata,
            dispatch_version,
            terminal_state: "answered",
            decision: "answered",
        })
    }

    pub async fn begin_mcp_form_response(
        &self,
        actor: ApprovalActor,
        approval_id: Uuid,
        runtime_instance_id: Uuid,
        request: RespondMcpFormRequest,
    ) -> Result<ApprovalDispatch, ApprovalServiceError> {
        let mut transaction = self.db.begin().await?;
        let row = sqlx::query(
            "SELECT a.runtime_instance_id, a.runtime_request_id, a.request_type, \
                    a.request_payload, a.state, a.version, a.decision \
             FROM approvals a \
             JOIN profiles p ON p.id = a.profile_id \
             JOIN runs r ON r.id = a.run_id AND r.requested_profile_id = p.id \
             WHERE a.id = $1 AND a.organization_id = $2 AND r.organization_id = $2 \
               AND p.owner_user_id = $3 AND p.runtime_key = $4 \
             FOR UPDATE OF a",
        )
        .bind(approval_id)
        .bind(actor.organization_id)
        .bind(actor.user_id)
        .bind(&self.runtime_key)
        .fetch_optional(&mut *transaction)
        .await?
        .ok_or(ApprovalServiceError::NotFound)?;
        let state: String = row.get("state");
        let version: i64 = row.get("version");
        let stored_runtime_instance_id: Option<Uuid> = row.get("runtime_instance_id");
        let previous_decision: Option<String> = row.get("decision");
        let decision = match request.action {
            McpFormResponseAction::Accept => "accept",
            McpFormResponseAction::Decline => "decline",
            McpFormResponseAction::Cancel => "cancel",
        };
        if stored_runtime_instance_id != Some(runtime_instance_id)
            || version != request.version
            || (state != "pending"
                && !(state == "delivery_unknown" && previous_decision.as_deref() == Some(decision)))
        {
            return Err(ApprovalServiceError::Conflict);
        }
        let request_type: String = row.get("request_type");
        let payload: Value = row.get("request_payload");
        if request_type != MCP_ELICITATION_REQUEST
            || payload.get("mode").and_then(Value::as_str) != Some("form")
        {
            return Err(ApprovalServiceError::Invalid);
        }
        let fields = parse_mcp_form_fields(&payload)?;
        let content = validate_mcp_form_response(&fields, &request)?;
        let (terminal_state, response) = match request.action {
            McpFormResponseAction::Accept => (
                "approved",
                json!({ "action": "accept", "content": content, "_meta": null }),
            ),
            McpFormResponseAction::Decline => (
                "rejected",
                json!({ "action": "decline", "content": null, "_meta": null }),
            ),
            McpFormResponseAction::Cancel => (
                "cancelled",
                json!({ "action": "cancel", "content": null, "_meta": null }),
            ),
        };
        let dispatch_version = version + 1;
        sqlx::query(
            "UPDATE approvals SET state = 'dispatching', decision = $1, decided_by = $2, \
                    decided_at = now(), version = $3 WHERE id = $4",
        )
        .bind(decision)
        .bind(actor.user_id)
        .bind(dispatch_version)
        .bind(approval_id)
        .execute(&mut *transaction)
        .await?;
        transaction.commit().await?;

        let runtime_request_id: String = row.get("runtime_request_id");
        Ok(ApprovalDispatch {
            approval_id,
            runtime_instance_id,
            runtime_request_id: serde_json::from_str(&runtime_request_id)
                .map_err(|_| ApprovalServiceError::Invalid)?,
            response,
            audit_metadata: mcp_form_audit_metadata(request.action, &fields),
            dispatch_version,
            terminal_state,
            decision,
        })
    }

    pub async fn complete_decision(
        &self,
        actor: ApprovalActor,
        dispatch: &ApprovalDispatch,
    ) -> Result<(), ApprovalServiceError> {
        let mut transaction = self.db.begin().await?;
        let updated = sqlx::query(
            "UPDATE approvals SET state = $1, version = version + 1 \
             WHERE id = $2 AND organization_id = $3 AND state = 'dispatching' AND version = $4",
        )
        .bind(dispatch.terminal_state)
        .bind(dispatch.approval_id)
        .bind(actor.organization_id)
        .bind(dispatch.dispatch_version)
        .execute(&mut *transaction)
        .await?;
        if updated.rows_affected() != 1 {
            let already_completed: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM approvals WHERE id = $1 \
                 AND organization_id = $2 AND state = $3 AND decision = $4)",
            )
            .bind(dispatch.approval_id)
            .bind(actor.organization_id)
            .bind(dispatch.terminal_state)
            .bind(dispatch.decision)
            .fetch_one(&mut *transaction)
            .await?;
            if !already_completed {
                return Err(ApprovalServiceError::Conflict);
            }
        }
        insert_audit(&mut transaction, actor, dispatch, "success").await?;
        transaction.commit().await?;
        Ok(())
    }

    pub async fn mark_delivery_unknown(
        &self,
        actor: ApprovalActor,
        dispatch: &ApprovalDispatch,
    ) -> Result<(), ApprovalServiceError> {
        let mut transaction = self.db.begin().await?;
        sqlx::query(
            "UPDATE approvals SET state = 'delivery_unknown', version = version + 1 \
             WHERE id = $1 AND organization_id = $2 AND state = 'dispatching' AND version = $3",
        )
        .bind(dispatch.approval_id)
        .bind(actor.organization_id)
        .bind(dispatch.dispatch_version)
        .execute(&mut *transaction)
        .await?;
        insert_audit(&mut transaction, actor, dispatch, "delivery_unknown").await?;
        transaction.commit().await?;
        Ok(())
    }

    pub async fn cancel_stale_runtime_requests(
        &self,
        runtime_instance_id: Uuid,
    ) -> Result<u64, ApprovalServiceError> {
        let result = sqlx::query(
            "UPDATE approvals a SET state = 'cancelled', \
                    decided_at = COALESCE(a.decided_at, now()), version = a.version + 1 \
             FROM profiles p WHERE p.id = a.profile_id AND p.runtime_key = $1 \
               AND a.state IN ('pending', 'dispatching', 'delivery_unknown') \
               AND a.runtime_instance_id IS DISTINCT FROM $2",
        )
        .bind(&self.runtime_key)
        .bind(runtime_instance_id)
        .execute(&self.db)
        .await?;
        Ok(result.rows_affected())
    }
}

fn pending_approval_state(value: String) -> Result<PendingApprovalState, ApprovalServiceError> {
    match value.as_str() {
        "pending" => Ok(PendingApprovalState::Pending),
        "dispatching" => Ok(PendingApprovalState::Dispatching),
        "delivery_unknown" => Ok(PendingApprovalState::DeliveryUnknown),
        _ => Err(ApprovalServiceError::Invalid),
    }
}

fn resolved_terminal_state(state: &str, decision: Option<&str>) -> Option<&'static str> {
    if !matches!(state, "pending" | "dispatching" | "delivery_unknown") {
        return None;
    }
    Some(match decision {
        Some("rejected" | "decline") => "rejected",
        Some("answered") => "answered",
        Some("cancel") => "cancelled",
        Some(_) => "approved",
        None => "cancelled",
    })
}

fn approval_outcome(state: &str) -> Option<ApprovalOutcome> {
    match state {
        "approved" => Some(ApprovalOutcome::Accepted),
        "rejected" => Some(ApprovalOutcome::Declined),
        "answered" => Some(ApprovalOutcome::Answered),
        "cancelled" => Some(ApprovalOutcome::Cancelled),
        _ => None,
    }
}

fn approval_payload(request_type: &str, params: &Value) -> Value {
    let mut payload = serde_json::Map::new();
    for key in ["threadId", "turnId", "itemId", "reason", "startedAtMs"] {
        if let Some(value) = params.get(key) {
            payload.insert(key.to_string(), value.clone());
        }
    }
    if request_type == COMMAND_APPROVAL {
        if let Some(actions) = params.get("commandActions").and_then(Value::as_array) {
            let actions = actions
                .iter()
                .take(16)
                .filter_map(|action| {
                    let action = action.as_object()?;
                    let action_type = action
                        .get("type")
                        .and_then(Value::as_str)
                        .map(|value| value.chars().take(64).collect::<String>())?;
                    let mut projected = serde_json::Map::new();
                    projected.insert("type".to_string(), Value::String(action_type));
                    if let Some(path) = action.get("path").and_then(Value::as_str) {
                        projected.insert(
                            "path".to_string(),
                            Value::String(path.chars().take(512).collect()),
                        );
                    }
                    Some(Value::Object(projected))
                })
                .collect::<Vec<_>>();
            if !actions.is_empty() {
                payload.insert("commandActions".to_string(), Value::Array(actions));
            }
        }
    }
    if request_type == PERMISSIONS_APPROVAL {
        if let Some(permissions) = params.get("permissions") {
            payload.insert("permissions".to_string(), permissions.clone());
        }
    }
    if request_type == MCP_ELICITATION_REQUEST {
        for key in ["serverName", "mode", "message", "requestedSchema", "url"] {
            if let Some(value) = params.get(key) {
                payload.insert(key.to_string(), value.clone());
            }
        }
    }
    if request_type == USER_INPUT_REQUEST {
        for key in ["questions", "autoResolutionMs"] {
            if let Some(value) = params.get(key) {
                payload.insert(key.to_string(), value.clone());
            }
        }
    }
    Value::Object(payload)
}

fn parse_user_input_questions(
    payload: &Value,
) -> Result<Vec<UserInputQuestionSummary>, ApprovalServiceError> {
    let questions = payload
        .get("questions")
        .and_then(Value::as_array)
        .ok_or(ApprovalServiceError::Invalid)?;
    if !(1..=3).contains(&questions.len()) {
        return Err(ApprovalServiceError::Invalid);
    }
    questions
        .iter()
        .map(|question| {
            let id = bounded_input_text(question.get("id"), 128)?;
            let header = bounded_optional_input_text(question.get("header"), 80)?;
            let prompt = bounded_input_text(question.get("question"), 1_000)?;
            let options = question
                .get("options")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            if options.len() > 3 {
                return Err(ApprovalServiceError::Invalid);
            }
            let options = options
                .iter()
                .map(|option| {
                    Ok(UserInputOptionSummary {
                        label: bounded_input_text(option.get("label"), 80)?,
                        description: bounded_optional_input_text(option.get("description"), 240)?,
                    })
                })
                .collect::<Result<Vec<_>, ApprovalServiceError>>()?;
            Ok(UserInputQuestionSummary {
                id,
                header,
                question: prompt,
                is_other: question
                    .get("isOther")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
                is_secret: question
                    .get("isSecret")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
                options,
            })
        })
        .collect()
}

fn bounded_input_text(
    value: Option<&Value>,
    max_len: usize,
) -> Result<String, ApprovalServiceError> {
    let value = value
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty() && value.len() <= max_len)
        .ok_or(ApprovalServiceError::Invalid)?;
    Ok(value.to_string())
}

fn bounded_optional_input_text(
    value: Option<&Value>,
    max_len: usize,
) -> Result<String, ApprovalServiceError> {
    let Some(value) = value else {
        return Ok(String::new());
    };
    if value.is_null() {
        return Ok(String::new());
    }
    bounded_input_text(Some(value), max_len)
}

fn validate_user_input_answers(
    payload: &Value,
    request: &RespondUserInputRequest,
) -> Result<(), ApprovalServiceError> {
    let questions = parse_user_input_questions(payload)?;
    let question_ids = questions
        .iter()
        .map(|question| question.id.as_str())
        .collect::<std::collections::BTreeSet<_>>();
    if question_ids.is_empty()
        || request.answers.len() != question_ids.len()
        || !request
            .answers
            .keys()
            .all(|id| question_ids.contains(id.as_str()))
    {
        return Err(ApprovalServiceError::Invalid);
    }
    let mut total_bytes = 0usize;
    for question in &questions {
        let answer = request
            .answers
            .get(&question.id)
            .ok_or(ApprovalServiceError::Invalid)?;
        if answer.answers.len() != 1 {
            return Err(ApprovalServiceError::Invalid);
        }
        let value = answer.answers[0].trim();
        if value.is_empty() || value.contains('\0') || value.len() > 4096 {
            return Err(ApprovalServiceError::Invalid);
        }
        if !question.options.is_empty()
            && !question.is_other
            && !question.options.iter().any(|option| option.label == value)
        {
            return Err(ApprovalServiceError::Invalid);
        }
        total_bytes = total_bytes.saturating_add(value.len());
    }
    if total_bytes > 64 * 1024 {
        return Err(ApprovalServiceError::Invalid);
    }
    Ok(())
}

fn redact_user_input_decision(
    questions: &[UserInputQuestionSummary],
    answers: &std::collections::BTreeMap<
        String,
        open_web_codex_platform_contracts::UserInputAnswer,
    >,
) -> Value {
    let mut redacted = serde_json::Map::new();
    for question in questions {
        let count = answers
            .get(&question.id)
            .map(|answer| answer.answers.len())
            .unwrap_or(0);
        let mut entry = serde_json::Map::new();
        entry.insert("answerCount".to_string(), Value::from(count));
        if question.is_secret {
            entry.insert("redacted".to_string(), Value::Bool(true));
        }
        redacted.insert(question.id.clone(), Value::Object(entry));
    }
    Value::Object(redacted)
}

async fn insert_audit(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    actor: ApprovalActor,
    dispatch: &ApprovalDispatch,
    outcome: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO audit_log \
         (organization_id, actor_id, action, target_type, target_id, metadata, outcome) \
         VALUES ($1, $2, 'approval.decide', 'approval', $3, $4, $5)",
    )
    .bind(actor.organization_id)
    .bind(actor.user_id)
    .bind(dispatch.approval_id)
    .bind(&dispatch.audit_metadata)
    .bind(outcome)
    .execute(&mut **transaction)
    .await?;
    Ok(())
}

fn summary_from_row(row: &sqlx::postgres::PgRow) -> ApprovalSummary {
    let payload: Value = row.get("request_payload");
    ApprovalSummary {
        id: row.get("id"),
        run_id: row.get("run_id"),
        thread_id: row.get("thread_id"),
        request_type: row.get("request_type"),
        item_id: payload
            .get("itemId")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned),
        reason: payload
            .get("reason")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned),
        command: payload
            .get("command")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned),
        state: row.get("state"),
        version: row.get("version"),
        created_at: row.get("created_at"),
        decided_at: row.get("decided_at"),
    }
}

fn parse_mcp_form_fields(
    payload: &Value,
) -> Result<Vec<McpFormFieldSummary>, ApprovalServiceError> {
    let schema = payload
        .get("requestedSchema")
        .and_then(Value::as_object)
        .ok_or(ApprovalServiceError::Invalid)?;
    if serde_json::to_vec(schema)
        .map_err(|_| ApprovalServiceError::Invalid)?
        .len()
        > MAX_MCP_FORM_JSON_BYTES
        || !has_only_keys(schema, &["$schema", "type", "properties", "required"])
        || schema.get("type").and_then(Value::as_str) != Some("object")
    {
        return Err(ApprovalServiceError::Invalid);
    }
    let properties = schema
        .get("properties")
        .and_then(Value::as_object)
        .ok_or(ApprovalServiceError::Invalid)?;
    // Codex uses a form elicitation with an empty object schema for an MCP
    // tool approval. It is still a typed approval request: the browser must
    // offer Accept / Decline / Cancel rather than silently dropping it.
    if properties.len() > MAX_MCP_FORM_FIELDS {
        return Err(ApprovalServiceError::Invalid);
    }
    let required_values: &[Value] = match schema.get("required") {
        Some(value) => value.as_array().ok_or(ApprovalServiceError::Invalid)?,
        None => &[],
    };
    if required_values.len() > properties.len() {
        return Err(ApprovalServiceError::Invalid);
    }
    let mut required = BTreeSet::new();
    for value in required_values {
        let name = bounded_schema_text(Some(value), MAX_MCP_FORM_NAME_BYTES)?;
        if !properties.contains_key(&name) || !required.insert(name) {
            return Err(ApprovalServiceError::Invalid);
        }
    }

    properties
        .iter()
        .map(|(name, property)| {
            let name = bounded_schema_name(name)?;
            let property = property.as_object().ok_or(ApprovalServiceError::Invalid)?;
            let title = optional_schema_text(property.get("title"), MAX_MCP_FORM_TITLE_BYTES)?
                .unwrap_or_else(|| name.clone());
            let description =
                optional_schema_text(property.get("description"), MAX_MCP_FORM_DESCRIPTION_BYTES)?
                    .unwrap_or_default();
            let schema = parse_mcp_form_field_schema(property)?;
            Ok(McpFormFieldSummary {
                required: required.contains(&name),
                name,
                title,
                description,
                schema,
            })
        })
        .collect()
}

fn parse_mcp_form_field_schema(
    property: &serde_json::Map<String, Value>,
) -> Result<McpFormFieldSchema, ApprovalServiceError> {
    match property.get("type").and_then(Value::as_str) {
        Some("string") if property.contains_key("enum") || property.contains_key("oneOf") => {
            if !has_only_keys(
                property,
                &[
                    "type",
                    "title",
                    "description",
                    "enum",
                    "enumNames",
                    "oneOf",
                    "default",
                ],
            ) || (property.contains_key("enum") && property.contains_key("oneOf"))
            {
                return Err(ApprovalServiceError::Invalid);
            }
            let options = parse_string_options(property)?;
            let default = optional_schema_text(property.get("default"), MAX_MCP_FORM_VALUE_BYTES)?;
            if default
                .as_ref()
                .is_some_and(|value| !options.iter().any(|option| option.value == *value))
            {
                return Err(ApprovalServiceError::Invalid);
            }
            Ok(McpFormFieldSchema::SingleSelect { options, default })
        }
        Some("string") => {
            if !has_only_keys(
                property,
                &[
                    "type",
                    "title",
                    "description",
                    "minLength",
                    "maxLength",
                    "default",
                ],
            ) {
                return Err(ApprovalServiceError::Invalid);
            }
            let min_length = optional_u32(property.get("minLength"))?;
            let max_length = optional_u32(property.get("maxLength"))?;
            if min_length
                .zip(max_length)
                .is_some_and(|(min, max)| min > max)
            {
                return Err(ApprovalServiceError::Invalid);
            }
            let default = optional_schema_text(property.get("default"), MAX_MCP_FORM_VALUE_BYTES)?;
            if let Some(default) = default.as_ref() {
                validate_string_bounds(default, min_length, max_length)?;
            }
            Ok(McpFormFieldSchema::String {
                default,
                min_length,
                max_length,
            })
        }
        Some("number") => {
            if !has_only_keys(
                property,
                &[
                    "type",
                    "title",
                    "description",
                    "minimum",
                    "maximum",
                    "default",
                ],
            ) {
                return Err(ApprovalServiceError::Invalid);
            }
            let minimum = optional_f64(property.get("minimum"))?;
            let maximum = optional_f64(property.get("maximum"))?;
            let default = optional_f64(property.get("default"))?;
            validate_number_bounds(default, minimum, maximum)?;
            Ok(McpFormFieldSchema::Number {
                default,
                minimum,
                maximum,
            })
        }
        Some("integer") => {
            if !has_only_keys(
                property,
                &[
                    "type",
                    "title",
                    "description",
                    "minimum",
                    "maximum",
                    "default",
                ],
            ) {
                return Err(ApprovalServiceError::Invalid);
            }
            let minimum = optional_i64(property.get("minimum"))?;
            let maximum = optional_i64(property.get("maximum"))?;
            let default = optional_i64(property.get("default"))?;
            validate_integer_bounds(default, minimum, maximum)?;
            Ok(McpFormFieldSchema::Integer {
                default,
                minimum,
                maximum,
            })
        }
        Some("boolean") => {
            if !has_only_keys(property, &["type", "title", "description", "default"]) {
                return Err(ApprovalServiceError::Invalid);
            }
            let default = match property.get("default") {
                Some(Value::Bool(value)) => Some(*value),
                Some(_) => return Err(ApprovalServiceError::Invalid),
                None => None,
            };
            Ok(McpFormFieldSchema::Boolean { default })
        }
        Some("array") => {
            if !has_only_keys(
                property,
                &[
                    "type",
                    "title",
                    "description",
                    "items",
                    "minItems",
                    "maxItems",
                    "default",
                ],
            ) {
                return Err(ApprovalServiceError::Invalid);
            }
            let items = property
                .get("items")
                .and_then(Value::as_object)
                .ok_or(ApprovalServiceError::Invalid)?;
            if items.get("type").and_then(Value::as_str) != Some("string")
                || !has_only_keys(items, &["type", "enum", "enumNames", "anyOf", "oneOf"])
                || ["enum", "anyOf", "oneOf"]
                    .into_iter()
                    .filter(|key| items.contains_key(*key))
                    .count()
                    != 1
                || (items.contains_key("enumNames") && !items.contains_key("enum"))
            {
                return Err(ApprovalServiceError::Invalid);
            }
            let options = parse_string_options(items)?;
            let min_items = optional_u32(property.get("minItems"))?;
            let max_items = optional_u32(property.get("maxItems"))?;
            if min_items.zip(max_items).is_some_and(|(min, max)| min > max)
                || max_items.is_some_and(|max| max as usize > options.len())
            {
                return Err(ApprovalServiceError::Invalid);
            }
            let default = match property.get("default") {
                Some(Value::Array(values)) => values
                    .iter()
                    .map(|value| bounded_schema_text(Some(value), MAX_MCP_FORM_VALUE_BYTES))
                    .collect::<Result<Vec<_>, _>>()
                    .map(Some)?,
                Some(_) => return Err(ApprovalServiceError::Invalid),
                None => None,
            };
            if let Some(default) = default.as_ref() {
                validate_multi_select(default, &options, min_items, max_items)?;
            }
            Ok(McpFormFieldSchema::MultiSelect {
                options,
                default,
                min_items,
                max_items,
            })
        }
        _ => Err(ApprovalServiceError::Invalid),
    }
}

fn parse_string_options(
    schema: &serde_json::Map<String, Value>,
) -> Result<Vec<McpFormOptionSummary>, ApprovalServiceError> {
    let options = if let Some(values) = schema.get("enum") {
        let values = values.as_array().ok_or(ApprovalServiceError::Invalid)?;
        let labels = match schema.get("enumNames") {
            Some(value) => Some(value.as_array().ok_or(ApprovalServiceError::Invalid)?),
            None => None,
        };
        if labels.is_some_and(|labels| labels.len() != values.len()) {
            return Err(ApprovalServiceError::Invalid);
        }
        values
            .iter()
            .enumerate()
            .map(|(index, value)| {
                let value = bounded_schema_text(Some(value), MAX_MCP_FORM_VALUE_BYTES)?;
                let label = labels
                    .and_then(|labels| labels.get(index))
                    .map(|label| bounded_schema_text(Some(label), MAX_MCP_FORM_TITLE_BYTES))
                    .transpose()?
                    .unwrap_or_else(|| value.clone());
                Ok(McpFormOptionSummary { value, label })
            })
            .collect::<Result<Vec<_>, ApprovalServiceError>>()?
    } else {
        let values = schema
            .get("oneOf")
            .or_else(|| schema.get("anyOf"))
            .and_then(Value::as_array)
            .ok_or(ApprovalServiceError::Invalid)?;
        values
            .iter()
            .map(|value| {
                let value = value.as_object().ok_or(ApprovalServiceError::Invalid)?;
                if !has_only_keys(value, &["const", "title"]) {
                    return Err(ApprovalServiceError::Invalid);
                }
                Ok(McpFormOptionSummary {
                    value: bounded_schema_text(value.get("const"), MAX_MCP_FORM_VALUE_BYTES)?,
                    label: bounded_schema_text(value.get("title"), MAX_MCP_FORM_TITLE_BYTES)?,
                })
            })
            .collect::<Result<Vec<_>, ApprovalServiceError>>()?
    };
    if !(1..=MAX_MCP_FORM_OPTIONS).contains(&options.len()) {
        return Err(ApprovalServiceError::Invalid);
    }
    let mut values = BTreeSet::new();
    if !options.iter().all(|option| values.insert(&option.value)) {
        return Err(ApprovalServiceError::Invalid);
    }
    Ok(options)
}

fn validate_mcp_form_response(
    fields: &[McpFormFieldSummary],
    request: &RespondMcpFormRequest,
) -> Result<Value, ApprovalServiceError> {
    if request.action != McpFormResponseAction::Accept {
        if request.content.is_some() {
            return Err(ApprovalServiceError::Invalid);
        }
        return Ok(Value::Null);
    }
    let content = request
        .content
        .as_ref()
        .ok_or(ApprovalServiceError::Invalid)?;
    if serde_json::to_vec(content)
        .map_err(|_| ApprovalServiceError::Invalid)?
        .len()
        > MAX_MCP_FORM_JSON_BYTES
    {
        return Err(ApprovalServiceError::Invalid);
    }
    let by_name = fields
        .iter()
        .map(|field| (field.name.as_str(), field))
        .collect::<BTreeMap<_, _>>();
    if !content
        .keys()
        .all(|name| by_name.contains_key(name.as_str()))
        || fields
            .iter()
            .any(|field| field.required && !content.contains_key(&field.name))
    {
        return Err(ApprovalServiceError::Invalid);
    }
    for (name, value) in content {
        validate_mcp_form_value(&by_name[name.as_str()].schema, value)?;
    }
    serde_json::to_value(content).map_err(|_| ApprovalServiceError::Invalid)
}

fn validate_mcp_form_value(
    schema: &McpFormFieldSchema,
    value: &Value,
) -> Result<(), ApprovalServiceError> {
    match schema {
        McpFormFieldSchema::String {
            min_length,
            max_length,
            ..
        } => validate_string_bounds(
            value.as_str().ok_or(ApprovalServiceError::Invalid)?,
            *min_length,
            *max_length,
        ),
        McpFormFieldSchema::Number {
            minimum, maximum, ..
        } => validate_number_bounds(
            Some(value.as_f64().ok_or(ApprovalServiceError::Invalid)?),
            *minimum,
            *maximum,
        ),
        McpFormFieldSchema::Integer {
            minimum, maximum, ..
        } => validate_integer_bounds(
            Some(value.as_i64().ok_or(ApprovalServiceError::Invalid)?),
            *minimum,
            *maximum,
        ),
        McpFormFieldSchema::Boolean { .. } => value
            .as_bool()
            .map(|_| ())
            .ok_or(ApprovalServiceError::Invalid),
        McpFormFieldSchema::SingleSelect { options, .. } => {
            let value = value.as_str().ok_or(ApprovalServiceError::Invalid)?;
            if options.iter().any(|option| option.value == value) {
                Ok(())
            } else {
                Err(ApprovalServiceError::Invalid)
            }
        }
        McpFormFieldSchema::MultiSelect {
            options,
            min_items,
            max_items,
            ..
        } => {
            let values = value.as_array().ok_or(ApprovalServiceError::Invalid)?;
            let values = values
                .iter()
                .map(|value| {
                    value
                        .as_str()
                        .map(str::to_string)
                        .ok_or(ApprovalServiceError::Invalid)
                })
                .collect::<Result<Vec<_>, _>>()?;
            validate_multi_select(&values, options, *min_items, *max_items)
        }
    }
}

fn mcp_form_audit_metadata(action: McpFormResponseAction, fields: &[McpFormFieldSummary]) -> Value {
    let action = match action {
        McpFormResponseAction::Accept => "accept",
        McpFormResponseAction::Decline => "decline",
        McpFormResponseAction::Cancel => "cancel",
    };
    let kinds = fields
        .iter()
        .map(|field| match &field.schema {
            McpFormFieldSchema::String { .. } => "string",
            McpFormFieldSchema::Number { .. } => "number",
            McpFormFieldSchema::Integer { .. } => "integer",
            McpFormFieldSchema::Boolean { .. } => "boolean",
            McpFormFieldSchema::SingleSelect { .. } => "singleSelect",
            McpFormFieldSchema::MultiSelect { .. } => "multiSelect",
        })
        .collect::<Vec<_>>();
    json!({ "action": action, "fieldCount": fields.len(), "fieldKinds": kinds })
}

fn has_only_keys(object: &serde_json::Map<String, Value>, allowed: &[&str]) -> bool {
    object.keys().all(|key| allowed.contains(&key.as_str()))
}

fn bounded_schema_text(
    value: Option<&Value>,
    max_bytes: usize,
) -> Result<String, ApprovalServiceError> {
    let value = value
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty() && value.len() <= max_bytes && !value.contains('\0'))
        .ok_or(ApprovalServiceError::Invalid)?;
    Ok(value.to_string())
}

fn bounded_schema_name(value: &str) -> Result<String, ApprovalServiceError> {
    let value = value.trim();
    if value.is_empty() || value.len() > MAX_MCP_FORM_NAME_BYTES || value.contains('\0') {
        return Err(ApprovalServiceError::Invalid);
    }
    Ok(value.to_string())
}

fn optional_schema_text(
    value: Option<&Value>,
    max_bytes: usize,
) -> Result<Option<String>, ApprovalServiceError> {
    value
        .map(|value| bounded_schema_text(Some(value), max_bytes))
        .transpose()
}

fn optional_u32(value: Option<&Value>) -> Result<Option<u32>, ApprovalServiceError> {
    value
        .map(|value| {
            value
                .as_u64()
                .and_then(|value| u32::try_from(value).ok())
                .ok_or(ApprovalServiceError::Invalid)
        })
        .transpose()
}

fn optional_f64(value: Option<&Value>) -> Result<Option<f64>, ApprovalServiceError> {
    value
        .map(|value| {
            value
                .as_f64()
                .filter(|value| value.is_finite())
                .ok_or(ApprovalServiceError::Invalid)
        })
        .transpose()
}

fn optional_i64(value: Option<&Value>) -> Result<Option<i64>, ApprovalServiceError> {
    value
        .map(|value| value.as_i64().ok_or(ApprovalServiceError::Invalid))
        .transpose()
}

fn validate_string_bounds(
    value: &str,
    minimum: Option<u32>,
    maximum: Option<u32>,
) -> Result<(), ApprovalServiceError> {
    if value.len() > MAX_MCP_FORM_VALUE_BYTES || value.contains('\0') {
        return Err(ApprovalServiceError::Invalid);
    }
    let length = value.chars().count() as u64;
    if minimum.is_some_and(|minimum| length < u64::from(minimum))
        || maximum.is_some_and(|maximum| length > u64::from(maximum))
    {
        return Err(ApprovalServiceError::Invalid);
    }
    Ok(())
}

fn validate_number_bounds(
    value: Option<f64>,
    minimum: Option<f64>,
    maximum: Option<f64>,
) -> Result<(), ApprovalServiceError> {
    if minimum.zip(maximum).is_some_and(|(min, max)| min > max)
        || value.is_some_and(|value| {
            !value.is_finite()
                || minimum.is_some_and(|minimum| value < minimum)
                || maximum.is_some_and(|maximum| value > maximum)
        })
    {
        return Err(ApprovalServiceError::Invalid);
    }
    Ok(())
}

fn validate_integer_bounds(
    value: Option<i64>,
    minimum: Option<i64>,
    maximum: Option<i64>,
) -> Result<(), ApprovalServiceError> {
    if minimum.zip(maximum).is_some_and(|(min, max)| min > max)
        || value.is_some_and(|value| {
            minimum.is_some_and(|minimum| value < minimum)
                || maximum.is_some_and(|maximum| value > maximum)
        })
    {
        return Err(ApprovalServiceError::Invalid);
    }
    Ok(())
}

fn validate_multi_select(
    values: &[String],
    options: &[McpFormOptionSummary],
    minimum: Option<u32>,
    maximum: Option<u32>,
) -> Result<(), ApprovalServiceError> {
    let mut unique = BTreeSet::new();
    if !values
        .iter()
        .all(|value| unique.insert(value) && options.iter().any(|option| option.value == *value))
        || minimum.is_some_and(|minimum| values.len() < minimum as usize)
        || maximum.is_some_and(|maximum| values.len() > maximum as usize)
    {
        return Err(ApprovalServiceError::Invalid);
    }
    Ok(())
}

fn approval_response(
    request_type: &str,
    payload: &Value,
    decision: ApprovalDecision,
) -> Result<(Value, &'static str, &'static str), ApprovalServiceError> {
    let (terminal_state, stored_decision) = match decision {
        ApprovalDecision::Accept | ApprovalDecision::AcceptForSession => ("approved", "approved"),
        ApprovalDecision::Decline => ("rejected", "rejected"),
        ApprovalDecision::Cancel => ("cancelled", "rejected"),
    };
    let response = match request_type {
        COMMAND_APPROVAL | FILE_APPROVAL => {
            let decision = match decision {
                ApprovalDecision::Accept => "accept",
                ApprovalDecision::AcceptForSession => "acceptForSession",
                ApprovalDecision::Decline => "decline",
                ApprovalDecision::Cancel => "cancel",
            };
            json!({ "decision": decision })
        }
        PERMISSIONS_APPROVAL => match decision {
            ApprovalDecision::Accept => {
                json!({ "permissions": payload.get("permissions").cloned().unwrap_or_else(|| json!({})), "scope": "turn" })
            }
            ApprovalDecision::AcceptForSession => {
                json!({ "permissions": payload.get("permissions").cloned().unwrap_or_else(|| json!({})), "scope": "session" })
            }
            ApprovalDecision::Decline | ApprovalDecision::Cancel => {
                json!({ "permissions": {}, "scope": "turn" })
            }
        },
        MCP_ELICITATION_REQUEST if payload.get("mode").and_then(Value::as_str) == Some("url") => {
            match decision {
                ApprovalDecision::Accept | ApprovalDecision::AcceptForSession => {
                    json!({ "action": "accept", "content": null, "_meta": null })
                }
                ApprovalDecision::Decline => {
                    json!({ "action": "decline", "content": null, "_meta": null })
                }
                ApprovalDecision::Cancel => {
                    json!({ "action": "cancel", "content": null, "_meta": null })
                }
            }
        }
        _ => return Err(ApprovalServiceError::Invalid),
    };
    Ok((response, terminal_state, stored_decision))
}

#[cfg(test)]
mod tests {
    use super::{
        approval_outcome, approval_payload, approval_response, mcp_form_audit_metadata,
        parse_mcp_form_fields, pending_approval_state, resolved_terminal_state,
        validate_mcp_form_response, validate_user_input_answers, ApprovalOutcome, COMMAND_APPROVAL,
        MCP_ELICITATION_REQUEST, PERMISSIONS_APPROVAL,
    };
    use open_web_codex_platform_contracts::{
        ApprovalDecision, McpFormFieldSchema, McpFormResponseAction, RespondMcpFormRequest,
        RespondUserInputRequest, UserInputAnswer,
    };
    use serde_json::{json, Value};
    use std::collections::BTreeMap;

    #[test]
    fn maps_platform_decisions_to_typed_runtime_responses() {
        let (command, state, _) = approval_response(
            COMMAND_APPROVAL,
            &json!({}),
            ApprovalDecision::AcceptForSession,
        )
        .unwrap();
        assert_eq!(command, json!({ "decision": "acceptForSession" }));
        assert_eq!(state, "approved");

        let (permissions, state, _) = approval_response(
            PERMISSIONS_APPROVAL,
            &json!({ "permissions": { "network": { "enabled": true } } }),
            ApprovalDecision::Accept,
        )
        .unwrap();
        assert_eq!(permissions["scope"], "turn");
        assert_eq!(permissions["permissions"]["network"]["enabled"], true);
        assert_eq!(state, "approved");
    }

    #[test]
    fn stores_only_selected_approval_fields_for_safe_recovery() {
        let payload = approval_payload(
            COMMAND_APPROVAL,
            &json!({
                "threadId": "thread-1",
                "command": "cat /private/server/secret.txt",
                "aggregatedOutput": "password=secret",
                "commandActions": [{
                    "type": "read",
                    "path": "src/lib.rs",
                    "command": "cat /private/server/secret.txt"
                }]
            }),
        );
        assert!(payload.get("command").is_none());
        assert!(payload.get("aggregatedOutput").is_none());
        assert_eq!(payload["commandActions"][0]["type"], "read");
        assert_eq!(payload["commandActions"][0]["path"], "src/lib.rs");
        assert!(payload["commandActions"][0].get("command").is_none());
        assert_eq!(
            pending_approval_state("delivery_unknown".to_string()).unwrap(),
            open_web_codex_platform_contracts::PendingApprovalState::DeliveryUnknown
        );
    }

    #[test]
    fn keeps_url_mcp_elicitation_on_the_generic_approval_contract() {
        let url_request = json!({
            "mode": "url",
            "url": "http://127.0.0.1:43123/one-time-token"
        });
        let (accepted, state, _) = approval_response(
            MCP_ELICITATION_REQUEST,
            &url_request,
            ApprovalDecision::Accept,
        )
        .unwrap();
        assert_eq!(
            accepted,
            json!({ "action": "accept", "content": null, "_meta": null })
        );
        assert_eq!(state, "approved");
    }

    #[test]
    fn parses_and_validates_the_bounded_mcp_form_subset() {
        let payload = json!({
            "mode": "form",
            "requestedSchema": {
                "type": "object",
                "properties": {
                    "name": {
                        "type": "string", "title": "Scenario name",
                        "minLength": 3, "maxLength": 20, "default": "Default plan"
                    },
                    "factor": { "type": "number", "minimum": 1.0, "maximum": 2.0 },
                    "warehouses": { "type": "integer", "minimum": 1, "maximum": 20 },
                    "confirmed": { "type": "boolean", "default": false },
                    "method": {
                        "type": "string",
                        "oneOf": [
                            { "const": "haversine", "title": "Curve distance" },
                            { "const": "navigation", "title": "Navigation" }
                        ],
                        "default": "haversine"
                    },
                    "targets": {
                        "type": "array", "minItems": 1, "maxItems": 2,
                        "items": {
                            "type": "string",
                            "enum": ["6h", "12h", "18h"],
                            "enumNames": ["6 hours", "12 hours", "18 hours"]
                        },
                        "default": ["12h"]
                    }
                },
                "required": ["name", "factor", "warehouses", "confirmed", "method", "targets"]
            }
        });
        let fields = parse_mcp_form_fields(&payload).unwrap();
        assert_eq!(fields.len(), 6);
        assert!(matches!(
            fields[0].schema,
            McpFormFieldSchema::Boolean { .. }
        ));
        assert!(fields.iter().all(|field| field.required));

        let request = RespondMcpFormRequest {
            action: McpFormResponseAction::Accept,
            content: Some(BTreeMap::from([
                ("name".to_string(), json!("Actual plan")),
                ("factor".to_string(), json!(1.35)),
                ("warehouses".to_string(), json!(5)),
                ("confirmed".to_string(), json!(true)),
                ("method".to_string(), json!("navigation")),
                ("targets".to_string(), json!(["6h", "18h"])),
            ])),
            version: 0,
        };
        let content = validate_mcp_form_response(&fields, &request).unwrap();
        assert_eq!(content["name"], "Actual plan");
        assert_eq!(content["method"], "navigation");

        let audit = mcp_form_audit_metadata(request.action, &fields);
        assert_eq!(audit["action"], "accept");
        assert_eq!(audit["fieldCount"], 6);
        assert!(!audit.to_string().contains("Actual plan"));
        assert!(!audit.to_string().contains("navigation"));
    }

    #[test]
    fn accepts_the_zero_field_mcp_tool_approval_form() {
        let fields = parse_mcp_form_fields(&json!({
            "mode": "form",
            "requestedSchema": {
                "type": "object",
                "properties": {}
            }
        }))
        .unwrap();
        assert!(fields.is_empty());

        let request = RespondMcpFormRequest {
            action: McpFormResponseAction::Accept,
            content: Some(BTreeMap::new()),
            version: 0,
        };
        assert_eq!(
            validate_mcp_form_response(&fields, &request).unwrap(),
            json!({})
        );
    }

    #[test]
    fn parses_the_official_titled_multi_select_any_of_shape_only() {
        let fields = parse_mcp_form_fields(&json!({
            "requestedSchema": {
                "type": "object",
                "properties": {
                    "targets": {
                        "type": "array",
                        "items": {
                            "type": "string",
                            "anyOf": [
                                { "const": "6h", "title": "6 hours" },
                                { "const": "12h", "title": "12 hours" }
                            ]
                        }
                    }
                }
            }
        }))
        .unwrap();
        assert!(matches!(
            fields[0].schema,
            McpFormFieldSchema::MultiSelect { .. }
        ));

        for items in [
            json!({
                "type": "string",
                "enum": ["6h"],
                "anyOf": [{ "const": "6h", "title": "6 hours" }]
            }),
            json!({ "type": "string", "enumNames": ["6 hours"] }),
        ] {
            let payload = json!({
                "requestedSchema": {
                    "type": "object",
                    "properties": { "targets": { "type": "array", "items": items } }
                }
            });
            assert!(parse_mcp_form_fields(&payload).is_err());
        }
    }

    #[test]
    fn preserves_missing_optional_form_defaults_as_missing() {
        let fields = parse_mcp_form_fields(&json!({
            "requestedSchema": {
                "type": "object",
                "properties": {
                    "includeExistingWarehouses": { "type": "boolean" },
                    "candidateTiers": {
                        "type": "array",
                        "items": { "type": "string", "enum": ["urban"] }
                    }
                }
            }
        }))
        .unwrap();

        assert!(matches!(
            fields[0].schema,
            McpFormFieldSchema::MultiSelect { default: None, .. }
        ));
        assert!(matches!(
            fields[1].schema,
            McpFormFieldSchema::Boolean { default: None }
        ));
    }

    #[test]
    fn rejects_missing_additional_invalid_bounded_and_unsupported_mcp_form_values() {
        let payload = json!({
            "requestedSchema": {
                "type": "object",
                "properties": {
                    "name": { "type": "string", "minLength": 2, "maxLength": 4 },
                    "count": { "type": "integer", "minimum": 1, "maximum": 3 }
                },
                "required": ["name", "count"]
            }
        });
        let fields = parse_mcp_form_fields(&payload).unwrap();
        for content in [
            json!({ "name": "ok" }),
            json!({ "name": "ok", "count": 2, "extra": true }),
            json!({ "name": "too-long", "count": 2 }),
            json!({ "name": "ok", "count": 4 }),
            json!({ "name": "ok", "count": 1.5 }),
        ] {
            let request = RespondMcpFormRequest {
                action: McpFormResponseAction::Accept,
                content: Some(serde_json::from_value(content).unwrap()),
                version: 0,
            };
            assert!(validate_mcp_form_response(&fields, &request).is_err());
        }

        let unsupported = json!({
            "requestedSchema": {
                "type": "object",
                "properties": { "nested": { "type": "object", "properties": {} } }
            }
        });
        assert!(parse_mcp_form_fields(&unsupported).is_err());
        let unsupported_format = json!({
            "requestedSchema": {
                "type": "object",
                "properties": { "email": { "type": "string", "format": "email" } }
            }
        });
        assert!(parse_mcp_form_fields(&unsupported_format).is_err());
    }

    #[test]
    fn enforces_mcp_form_action_content_contract() {
        let fields = parse_mcp_form_fields(&json!({
            "requestedSchema": {
                "type": "object",
                "properties": { "confirmed": { "type": "boolean" } },
                "required": ["confirmed"]
            }
        }))
        .unwrap();
        let accept_without_content = RespondMcpFormRequest {
            action: McpFormResponseAction::Accept,
            content: None,
            version: 0,
        };
        assert!(validate_mcp_form_response(&fields, &accept_without_content).is_err());
        for action in [
            McpFormResponseAction::Decline,
            McpFormResponseAction::Cancel,
        ] {
            let valid = RespondMcpFormRequest {
                action,
                content: None,
                version: 0,
            };
            assert_eq!(
                validate_mcp_form_response(&fields, &valid).unwrap(),
                Value::Null
            );
            let invalid = RespondMcpFormRequest {
                action,
                content: Some(BTreeMap::from([("confirmed".to_string(), json!(true))])),
                version: 0,
            };
            assert!(validate_mcp_form_response(&fields, &invalid).is_err());
        }
    }

    #[test]
    fn validates_user_input_answers_against_the_persisted_questions() {
        let payload = json!({
            "questions": [
                { "id": "first", "question": "First?" },
                { "id": "second", "question": "Second?" }
            ]
        });
        let request = RespondUserInputRequest {
            answers: BTreeMap::from([
                (
                    "first".to_string(),
                    UserInputAnswer {
                        answers: vec!["yes".to_string()],
                    },
                ),
                (
                    "second".to_string(),
                    UserInputAnswer {
                        answers: vec!["details".to_string()],
                    },
                ),
            ]),
            version: 0,
        };
        assert!(validate_user_input_answers(&payload, &request).is_ok());

        let missing = RespondUserInputRequest {
            answers: BTreeMap::from([(
                "first".to_string(),
                UserInputAnswer {
                    answers: vec!["yes".to_string()],
                },
            )]),
            version: 0,
        };
        assert!(validate_user_input_answers(&payload, &missing).is_err());
    }

    #[test]
    fn runtime_resolution_settles_only_active_approvals_and_preserves_decisions() {
        assert_eq!(resolved_terminal_state("pending", None), Some("cancelled"));
        assert_eq!(
            resolved_terminal_state("dispatching", Some("approved")),
            Some("approved")
        );
        assert_eq!(
            resolved_terminal_state("delivery_unknown", Some("rejected")),
            Some("rejected")
        );
        assert_eq!(
            resolved_terminal_state("delivery_unknown", Some("accept")),
            Some("approved")
        );
        assert_eq!(
            resolved_terminal_state("delivery_unknown", Some("decline")),
            Some("rejected")
        );
        assert_eq!(
            resolved_terminal_state("delivery_unknown", Some("cancel")),
            Some("cancelled")
        );
        assert_eq!(
            resolved_terminal_state("dispatching", Some("answered")),
            Some("answered")
        );
        assert_eq!(resolved_terminal_state("approved", Some("approved")), None);
        assert_eq!(resolved_terminal_state("rejected", Some("rejected")), None);
        assert_eq!(
            approval_outcome("approved"),
            Some(ApprovalOutcome::Accepted)
        );
        assert_eq!(
            approval_outcome("rejected"),
            Some(ApprovalOutcome::Declined)
        );
        assert_eq!(
            approval_outcome("answered"),
            Some(ApprovalOutcome::Answered)
        );
        assert_eq!(
            approval_outcome("cancelled"),
            Some(ApprovalOutcome::Cancelled)
        );
    }
}
