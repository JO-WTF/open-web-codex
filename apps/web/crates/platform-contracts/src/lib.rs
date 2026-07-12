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
