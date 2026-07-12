mod config;
mod routes;

use std::net::SocketAddr;

use axum::Router;
use clap::Parser;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;
use tracing_subscriber::EnvFilter;

use routes::codex_proxy::DaemonProxy;
use open_web_codex_platform_store::AppState;

#[derive(Parser, Debug)]
#[command(name = "open-web-codex-server", about = "open-web-codex platform server")]
struct Cli {
    /// Address to bind the HTTP server.
    #[arg(long, default_value = "127.0.0.1:4800")]
    bind: SocketAddr,

    /// PostgreSQL connection string.
    #[arg(long, env = "DATABASE_URL")]
    database_url: Option<String>,

    /// Maximum PostgreSQL connections.
    #[arg(long, env = "DATABASE_MAX_CONNECTIONS", default_value_t = 10)]
    database_max_connections: u32,

    /// Run database migrations on startup.
    #[arg(long, default_value_t = true)]
    migrate: bool,
    /// URL of the existing Tauri daemon for /api/rpc and /api/events proxying.
    #[arg(long, env = "CODEX_DAEMON_URL", default_value = "http://127.0.0.1:4733")]
    daemon_url: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let cli = Cli::parse();

    tracing::info!(bind = %cli.bind, "starting open-web-codex server");

    // Connect to PostgreSQL
    let pool = open_web_codex_platform_store::connect(
        &cli.database_url.clone().unwrap_or_else(|| {
            let user = std::env::var("USER").unwrap_or_else(|_| "postgres".to_string());
            format!("postgres://{user}@localhost:5432/open_web_codex")
        }),
        cli.database_max_connections,
    )
    .await?;
    tracing::info!("connected to PostgreSQL");

    // Run migrations
    if cli.migrate {
        open_web_codex_platform_store::migrate::run(&pool).await?;
        tracing::info!("database migrations complete");
    }

    let state = AppState::new(pool);
    let daemon_proxy = DaemonProxy {
        base_url: cli.daemon_url.clone(),
        client: reqwest::Client::new(),
    };
    tracing::info!(daemon_url = %cli.daemon_url, "proxying to codex daemon");

    let app = Router::new()
        .nest("/api", routes::router(daemon_proxy))
        .layer(TraceLayer::new_for_http())
        .layer(CorsLayer::permissive())
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(cli.bind).await?;
    tracing::info!("listening on {}", cli.bind);

    axum::serve(listener, app).await?;

    Ok(())
}
