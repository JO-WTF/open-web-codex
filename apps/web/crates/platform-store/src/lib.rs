pub mod configuration;
pub mod migrate;

use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;
use std::sync::Arc;
use tokio::sync::broadcast;
use uuid::Uuid;

/// Open a connection pool to PostgreSQL.
pub async fn connect(database_url: &str, max_connections: u32) -> Result<PgPool, sqlx::Error> {
    PgPoolOptions::new()
        .max_connections(max_connections)
        .connect(database_url)
        .await
}

/// Event bus capacity for the adapter event broadcast channel.
pub const EVENT_BUS_CAPACITY: usize = 1024;

/// Internal live delivery unit. Tenant identity is retained for authorization
/// filtering but is not serialized into the browser event payload.
#[derive(Debug, Clone)]
pub struct LiveEvent {
    pub organization_id: Uuid,
    pub payload: Vec<u8>,
}

/// Platform application state shared across handlers.
#[derive(Clone)]
pub struct AppState {
    pub db: PgPool,
    pub event_bus: broadcast::Sender<LiveEvent>,
    pub started_at: std::time::Instant,
    pub started_at_utc: chrono::DateTime<chrono::Utc>,
    /// Set only after the server startup schema assertion succeeds.
    pub schema_current: bool,
    /// Process-local secret used only by the internal analysis authorization
    /// endpoint and inherited by the Profile Host's MCP children.
    pub analysis_gate_key: Arc<Vec<u8>>,
    /// Process-local secret used only by the read-only coordination MCP.
    pub coordination_gate_key: Arc<Vec<u8>>,
    /// Process-local secret used only by the Profile Host Work State writer.
    pub work_state_gate_key: Arc<Vec<u8>>,
}

impl AppState {
    pub fn new(db: PgPool) -> Self {
        let (event_bus, _) = broadcast::channel(EVENT_BUS_CAPACITY);
        Self {
            db,
            event_bus,
            started_at: std::time::Instant::now(),
            started_at_utc: chrono::Utc::now(),
            schema_current: true,
            analysis_gate_key: Arc::new(Vec::new()),
            coordination_gate_key: Arc::new(Vec::new()),
            work_state_gate_key: Arc::new(Vec::new()),
        }
    }

    pub fn with_schema_current(mut self, schema_current: bool) -> Self {
        self.schema_current = schema_current;
        self
    }

    pub fn with_analysis_gate_key(mut self, key: Vec<u8>) -> Self {
        self.analysis_gate_key = Arc::new(key);
        self
    }

    pub fn with_coordination_gate_key(mut self, key: Vec<u8>) -> Self {
        self.coordination_gate_key = Arc::new(key);
        self
    }

    pub fn with_work_state_gate_key(mut self, key: Vec<u8>) -> Self {
        self.work_state_gate_key = Arc::new(key);
        self
    }
}
