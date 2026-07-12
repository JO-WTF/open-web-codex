pub mod error;
pub mod event;
pub mod idempotency;

use uuid::Uuid;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

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
        Self { before: None, after: None, limit: 50 }
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
