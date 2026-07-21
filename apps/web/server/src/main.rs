mod event_projection;
mod middleware;
mod routes;

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use axum::Router;
use clap::Parser;
use open_web_codex_profile_host::ProfileHostConfig;
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
    /// Codex adapter mode: "fake" (in-memory) or "real" (native Profile Host).
    #[arg(long, env = "CODEX_MODE", default_value = "real")]
    codex_mode: String,
    /// Persistent Profile home used by the native Codex app-server.
    #[arg(long, env = "CODEX_HOME")]
    codex_home: Option<PathBuf>,
    /// Codex executable used by the native Profile Host.
    #[arg(long, env = "CODEX_BIN", default_value = "codex")]
    codex_bin: PathBuf,
    /// Server-owned workspace root for the transitional single-workspace flow.
    #[arg(long, env = "CODEX_WORKSPACE_ROOT")]
    workspace_root: Option<PathBuf>,
    /// Stable Profile identity for the transitional single-Profile flow.
    #[arg(long, env = "CODEX_PROFILE_ID", default_value = "default-profile")]
    profile_id: String,
    /// Stable workspace identity for event and Thread routing.
    #[arg(long, env = "CODEX_WORKSPACE_ID", default_value = "default-workspace")]
    workspace_id: String,
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
            let codex_home = cli.codex_home.clone().ok_or_else(|| {
                anyhow::anyhow!("--codex-home / CODEX_HOME is required in real Codex mode")
            })?;
            let workspace_root = match cli.workspace_root.clone() {
                Some(path) => path,
                None => std::env::current_dir()?,
            };
            tracing::info!(
                profile_id = %cli.profile_id,
                workspace_id = %cli.workspace_id,
                workspace_root = %workspace_root.display(),
                "starting native Codex Profile Host"
            );
            let host_config =
                ProfileHostConfig::new(cli.profile_id.clone(), codex_home, workspace_root)
                    .with_codex_bin(cli.codex_bin.clone());
            Arc::new(RealCodexAdapter::spawn(host_config, cli.workspace_id.clone()).await?)
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
