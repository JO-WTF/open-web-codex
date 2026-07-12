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
