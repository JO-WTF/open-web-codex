use sqlx::PgPool;

/// Run all pending SQL migrations from `apps/web/migrations/`.
///
/// This embeds the migration SQL files at compile time via `sqlx::migrate!`.
pub async fn run(pool: &PgPool) -> Result<(), sqlx::migrate::MigrateError> {
    sqlx::migrate!("../../migrations").run(pool).await
}
