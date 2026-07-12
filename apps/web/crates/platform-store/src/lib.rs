pub mod migrate;

use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;

/// Open a connection pool to PostgreSQL.
pub async fn connect(database_url: &str, max_connections: u32) -> Result<PgPool, sqlx::Error> {
    PgPoolOptions::new()
        .max_connections(max_connections)
        .connect(database_url)
        .await
}

/// Platform application state shared across handlers.
#[derive(Clone)]
pub struct AppState {
    pub db: PgPool,
    pub started_at: std::time::Instant,
}

impl AppState {
    pub fn new(db: PgPool) -> Self {
        Self { db, started_at: std::time::Instant::now() }
    }
}
