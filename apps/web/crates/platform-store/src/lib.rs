pub mod migrate;

use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;
use tokio::sync::broadcast;

/// Open a connection pool to PostgreSQL.
pub async fn connect(database_url: &str, max_connections: u32) -> Result<PgPool, sqlx::Error> {
    PgPoolOptions::new()
        .max_connections(max_connections)
        .connect(database_url)
        .await
}

/// Event bus capacity for the adapter event broadcast channel.
pub const EVENT_BUS_CAPACITY: usize = 1024;

/// Platform application state shared across handlers.
#[derive(Clone)]
pub struct AppState {
    pub db: PgPool,
    pub event_bus: broadcast::Sender<Vec<u8>>,
    pub started_at: std::time::Instant,
}

impl AppState {
    pub fn new(db: PgPool) -> Self {
        let (event_bus, _) = broadcast::channel(EVENT_BUS_CAPACITY);
        Self { db, event_bus, started_at: std::time::Instant::now() }
    }
}
