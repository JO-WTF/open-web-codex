pub mod error;
pub mod event;
pub mod idempotency;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Platform-level health response.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HealthResponse {
    pub ok: bool,
    pub version: String,
    pub started_at: DateTime<Utc>,
    pub uptime_seconds: u64,
}

/// Generic pagination cursor.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Cursor {
    pub before: Option<Uuid>,
    pub after: Option<Uuid>,
    pub limit: u32,
}

impl Default for Cursor {
    fn default() -> Self {
        Self {
            before: None,
            after: None,
            limit: 50,
        }
    }
}

/// Platform event projection cursor (opaque to clients).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventCursor(pub String);

// ── Project ──────────────────────────────────────────────────────────

/// Database/API project representation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Project {
    pub id: Uuid,
    pub name: String,
    pub git_url: String,
    pub default_branch: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Request body to create a project.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateProjectRequest {
    pub name: String,
    pub git_url: String,
    #[serde(default)]
    pub default_branch: Option<String>,
}

// ── Task ──────────────────────────────────────────────────────────────

/// Database/API task representation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: Uuid,
    pub project_id: Uuid,
    pub title: String,
    pub status: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Request body to create a task.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateTaskRequest {
    pub project_id: Uuid,
    pub title: String,
}

// ── Auth ────────────────────────────────────────────────────────────

/// Public user representation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    pub id: Uuid,
    pub name: String,
    pub email: String,
    pub role: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Request body for one-time bootstrap.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BootstrapRequest {
    pub name: String,
    pub email: String,
    pub password: String,
}

/// Response from bootstrap.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BootstrapResponse {
    pub organization: Organization,
    pub user: User,
    pub session_token: String,
}

/// Request body for login.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoginRequest {
    pub email: String,
    pub password: String,
}

/// Response from login.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoginResponse {
    pub user: User,
    pub session_token: String,
}

/// Response from /api/me.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeResponse {
    pub id: Uuid,
    pub name: String,
    pub email: String,
    pub role: String,
}

// ── Organization ────────────────────────────────────────────────────

/// Database/API organization representation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Organization {
    pub id: Uuid,
    pub name: String,
    pub slug: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Request to create an organization.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateOrganizationRequest {
    pub name: String,
    pub slug: Option<String>,
}

/// Organization membership record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Membership {
    pub id: Uuid,
    pub organization_id: Uuid,
    pub user_id: Uuid,
    pub role: String,
    pub created_at: DateTime<Utc>,
}

/// Member info returned in member list.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemberInfo {
    pub user_id: Uuid,
    pub name: String,
    pub email: String,
    pub role: String,
}

/// Request to add a member.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddMemberRequest {
    pub email: String,
    pub role: Option<String>,
}

// ── Run ────────────────────────────────────────────────────────────

/// Database/API run representation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Run {
    pub id: Uuid,
    pub task_id: Uuid,
    pub status: String,
    pub codex_thread_id: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Request to start a run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StartRunRequest {}

/// Response from starting a run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StartRunResponse {
    pub run: Run,
}

// ── Approvals ─────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Approval {
    pub id: Uuid,
    pub run_id: Uuid,
    pub request_type: String,
    pub request_payload: serde_json::Value,
    pub status: String,
    pub codex_request_id: Option<String>,
    pub workspace_id: Option<String>,
    pub thread_id: Option<String>,
    pub decision: Option<String>,
    pub decided_by: Option<Uuid>,
    pub decided_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalDecisionRequest {
    pub decision: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalDecisionResponse {
    pub approval: Approval,
}

// ── Messages ──────────────────────────────────────────────────────

/// Request to send a user message to a task's active thread.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SendMessageRequest {
    pub text: String,
}

/// Response from sending a message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SendMessageResponse {
    pub status: String,
    pub thread_id: String,
}

// ── Events ────────────────────────────────────────────────────────

/// A persisted run event returned by the task events endpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunEvent {
    pub id: Uuid,
    pub sequence: i64,
    pub run_id: Uuid,
    pub event_type: String,
    pub projection_version: i16,
    pub thread_id: Option<String>,
    pub turn_id: Option<String>,
    pub item_id: Option<String>,
    pub payload: serde_json::Value,
    pub created_at: DateTime<Utc>,
}

/// Query parameters for listing task events.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListTaskEventsParams {
    pub limit: Option<i64>,
    pub after_sequence: Option<i64>,
}
