mod config;
mod middleware;
mod routes;
mod supervisor;

use std::sync::Arc;
use std::net::SocketAddr;

use axum::Router;
use clap::Parser;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;
use tracing_subscriber::EnvFilter;

use open_web_codex_adapter::{CodexAdapter, fake::FakeCodexAdapter, real::RealCodexAdapter};
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
    /// Codex adapter mode: "fake" (in-memory) or "real" (proxy to daemon).
    #[arg(long, env = "CODEX_MODE", default_value = "real")]
    codex_mode: String,
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
    let adapter: Arc<dyn CodexAdapter> = match cli.codex_mode.as_str() {
        "fake" => {
            tracing::info!("starting in fake codex mode");
            Arc::new(FakeCodexAdapter::new().with_demo_workspace().await)
        }
        "real" => {
            tracing::info!(daemon_url = %cli.daemon_url, "proxying to codex daemon");
            Arc::new(RealCodexAdapter::new(&cli.daemon_url))
        }
        other => anyhow::bail!("unknown --codex-mode '{other}'; expected 'fake' or 'real'"),
    };

    // ── Event Bus ───────────────────────────────────────────────────
    // Bridge adapter events into a broadcast channel so that both
    // the SSE endpoint and the RunSupervisor can consume from one stream.
    let event_bus = state.event_bus.clone();
    {
        let event_bus = event_bus.clone();
        let adapter = adapter.clone();
        tokio::spawn(async move {
            let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<Vec<u8>>();

            // Subscribe to adapter events
            let adapter_clone = adapter.clone();
            let sub = tokio::spawn(async move {
                if let Err(e) = adapter_clone.subscribe_events(tx).await {
                    tracing::warn!("adapter event subscription ended: {e}");
                }
            });

            // Relay to broadcast channel
            while let Some(data) = rx.recv().await {
                if event_bus.send(data).is_err() {
                    tracing::debug!("event bus: no active receivers, dropping event");
                }
            }

            let _ = sub.await;
            tracing::info!("event bridge task exiting");
        });
    }

    // ── Run Supervisor ──────────────────────────────────────────────
    // Background listener that auto-transitions run statuses from
    // adapter thread lifecycle events.
    supervisor::start(event_bus, state.db.clone());

    let app = Router::new()
        .nest("/api", routes::router(adapter))
        .layer(TraceLayer::new_for_http())
        .layer(CorsLayer::permissive())
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(cli.bind).await?;
    tracing::info!("listening on {}", cli.bind);

    axum::serve(listener, app).await?;

    Ok(())
}
