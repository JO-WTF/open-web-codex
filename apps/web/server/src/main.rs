mod config;
mod event_projection;
mod git_workspace;
mod middleware;
mod run_lifecycle;
mod routes;

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use axum::Router;
use clap::Parser;
use open_web_codex_profile_host::ensure_profile_home;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;
use tracing_subscriber::EnvFilter;

use open_web_codex_adapter::{fake::FakeCodexAdapter, real::RealCodexAdapter, CodexAdapter};
use open_web_codex_platform_store::AppState;

#[derive(Parser, Debug)]
#[command(
    name = "open-web-codex-server",
    about = "open-web-codex platform server"
)]
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
    #[arg(
        long,
        env = "CODEX_DAEMON_URL",
        default_value = "http://127.0.0.1:4733"
    )]
    daemon_url: String,
    /// Profile home directory to provision before Codex interactions.
    ///
    /// When set, the platform server creates a missing directory and exports a
    /// canonical `CODEX_HOME` for child processes. Codex itself rejects a
    /// configured but missing home.
    #[arg(long, env = "CODEX_HOME")]
    codex_home: Option<PathBuf>,
    /// Expose legacy `/api/rpc` and `/api/events` Codex proxy routes.
    ///
    /// Disabled by default. Local migration may opt in with
    /// `CODEX_ALLOW_LEGACY_PROXY=1`.
    #[arg(long, env = "CODEX_ALLOW_LEGACY_PROXY", default_value_t = false, action = clap::ArgAction::SetTrue)]
    legacy_codex_proxy: bool,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let cli = Cli::parse();

    if let Some(codex_home) = cli.codex_home.as_ref() {
        let canonical = ensure_profile_home(codex_home).map_err(|error| {
            anyhow::anyhow!(
                "failed to provision CODEX_HOME {}: {error}",
                codex_home.display()
            )
        })?;
        std::env::set_var("CODEX_HOME", &canonical);
        tracing::info!(
            codex_home = %canonical.display(),
            "provisioned profile home for platform server"
        );
    }

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
    // Bridge adapter events into durable projections and the live event bus.
    let event_bus = state.event_bus.clone();
    {
        let event_bus = event_bus.clone();
        let adapter = adapter.clone();
        let projection_db = state.db.clone();
        tokio::spawn(async move {
            let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<Vec<u8>>();

            // Subscribe to adapter events
            let adapter_clone = adapter.clone();
            let sub = tokio::spawn(async move {
                if let Err(e) = adapter_clone.subscribe_events(tx).await {
                    tracing::warn!("adapter event subscription ended: {e}");
                }
            });

            // Persist a safe, versioned projection before exposing the event
            // to connected browsers. The database cursor is therefore always
            // available when a client reconnects after seeing an event.
            while let Some(data) = rx.recv().await {
                if let Err(error) = event_projection::persist_frame(&data, &projection_db).await {
                    tracing::warn!("event projection failed: {error}");
                    continue;
                }
                if event_bus.send(data).is_err() {
                    tracing::debug!("event bus: no active receivers, dropping event");
                }
            }

            let _ = sub.await;
            tracing::info!("event bridge task exiting");
        });
    }

    if cli.legacy_codex_proxy {
        tracing::warn!(
            "legacy Codex RPC/SSE proxy is enabled; do not use this as a multi-user production boundary"
        );
    } else {
        tracing::info!("legacy Codex RPC/SSE proxy routes are disabled");
    }

    let app = Router::new()
        .nest("/api", routes::router(adapter, cli.legacy_codex_proxy))
        .layer(TraceLayer::new_for_http())
        .layer(cors_layer(cli.legacy_codex_proxy))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(cli.bind).await?;
    tracing::info!("listening on {}", cli.bind);

    axum::serve(listener, app).await?;

    Ok(())
}

fn cors_layer(legacy_codex_proxy: bool) -> CorsLayer {
    if legacy_codex_proxy {
        CorsLayer::permissive()
    } else {
        CorsLayer::new()
    }
}
