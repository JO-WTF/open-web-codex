mod event_projection;
mod middleware;
mod routes;
#[cfg(test)]
mod security_integration;

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use axum::Router;
use clap::Parser;
use open_web_codex_approval_service::ApprovalService;
use open_web_codex_git_runtime::{GitRuntime, GitRuntimeConfig};
use open_web_codex_profile_host::{ProfileHost, ProfileHostConfig};
use open_web_codex_profile_registry::ProfileRegistry;
use open_web_codex_provider_service::secured::{
    AuthorizedProviderOperations, InMemoryAuthorizedProviderService, SecuredProviderService,
};
use open_web_codex_run_orchestrator::RunOrchestrator;
use open_web_codex_secret_store::{MasterKey, PostgresSecretStore, SecretCipher};
use sqlx::Row;
use tower_http::services::{ServeDir, ServeFile};
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
    /// Private root for server-owned repository mirrors and Run workspaces.
    #[arg(
        long,
        env = "OPEN_WEB_CODEX_RUNNER_ROOT",
        default_value = ".open-web-codex/runner"
    )]
    runner_root: PathBuf,
    /// Permit local filesystem Git sources. Intended only for isolated tests.
    #[arg(long, env = "OPEN_WEB_CODEX_ALLOW_LOCAL_GIT_SOURCES", default_value_t = false, action = clap::ArgAction::SetTrue)]
    allow_local_git_sources: bool,
    /// Built browser assets served by this process. API-only mode is used
    /// when the directory does not exist.
    #[arg(long, env = "OPEN_WEB_CODEX_WEB_DIST", default_value = "dist")]
    web_dist: PathBuf,
    /// Stable Profile identity for the transitional single-Profile flow.
    #[arg(long, env = "CODEX_PROFILE_ID", default_value = "default-profile")]
    profile_id: String,
    /// Stable workspace identity for event and Thread routing.
    #[arg(long, env = "CODEX_WORKSPACE_ID", default_value = "default-workspace")]
    workspace_id: String,
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
    let mut git_config = GitRuntimeConfig::new(cli.runner_root.clone());
    if cli.allow_local_git_sources {
        tracing::warn!("local filesystem Git sources are enabled");
        git_config = git_config.with_local_sources();
    }
    let git = Arc::new(GitRuntime::new(git_config)?);
    let profile_binding = routes::RuntimeProfileBinding {
        runtime_key: cli.profile_id.clone(),
        name: cli.profile_id.clone(),
        codex_home: cli.codex_home.clone().map(Arc::new),
        capabilities: routes::RuntimeCapabilityState::default(),
    };
    let (adapter, providers): (Arc<dyn CodexAdapter>, Arc<dyn AuthorizedProviderOperations>) =
        match cli.codex_mode.as_str() {
            "fake" => {
                tracing::info!("starting in fake codex mode");
                (
                    Arc::new(FakeCodexAdapter::new().with_demo_workspace().await),
                    Arc::new(InMemoryAuthorizedProviderService::default()),
                )
            }
            "real" => {
                let codex_home = cli.codex_home.clone().ok_or_else(|| {
                    anyhow::anyhow!("--codex-home / CODEX_HOME is required in real Codex mode")
                })?;
                let workspace_root = git.workspace_root().to_path_buf();
                tracing::info!(
                    profile_id = %cli.profile_id,
                    workspace_id = %cli.workspace_id,
                    workspace_root = %workspace_root.display(),
                    "starting native Codex Profile Host"
                );
                ensure_transitional_profile_binding(
                    &state.db,
                    &profile_binding.runtime_key,
                    &profile_binding.name,
                )
                .await?;
                let master_key = std::env::var("OPEN_WEB_CODEX_MASTER_KEY").map_err(|_| {
                    anyhow::anyhow!("OPEN_WEB_CODEX_MASTER_KEY is required in real Codex mode")
                })?;
                let key_version = std::env::var("OPEN_WEB_CODEX_MASTER_KEY_VERSION")
                    .unwrap_or_else(|_| "v1".to_string());
                let cipher = SecretCipher::new(MasterKey::from_base64(&master_key)?, key_version)?;
                let secret_store = PostgresSecretStore::new(state.db.clone(), cipher);
                let registry = ProfileRegistry::new();
                let providers = SecuredProviderService::new(
                    state.db.clone(),
                    profile_binding.runtime_key.clone(),
                    registry.clone(),
                    secret_store,
                );
                let secret_environment = providers.startup_secret_environment().await?;
                let host_config =
                    ProfileHostConfig::new(cli.profile_id.clone(), codex_home, workspace_root)
                        .with_codex_bin(cli.codex_bin.clone());
                let workspace_root = host_config.workspace_root.clone();
                let host = registry
                    .register_with_secret_environment(host_config, secret_environment)
                    .await?;
                let capabilities = profile_capability_record(&host).await?;
                profile_binding.capabilities.set(capabilities.clone()).await;
                persist_profile_capabilities(
                    &state.db,
                    &profile_binding.runtime_key,
                    &capabilities,
                )
                .await?;
                let real =
                    RealCodexAdapter::from_host(host, cli.workspace_id.clone(), workspace_root)?;
                (Arc::new(real), Arc::new(providers))
            }
            other => anyhow::bail!("unknown --codex-mode '{other}'; expected 'fake' or 'real'"),
        };
    let approvals = Arc::new(ApprovalService::new(
        state.db.clone(),
        profile_binding.runtime_key.clone(),
    ));
    let orchestrator = Arc::new(RunOrchestrator::new(
        state.db.clone(),
        git.clone(),
        adapter.clone(),
        profile_binding.runtime_key.clone(),
        format!("server-{}", uuid::Uuid::now_v7()),
        std::time::Duration::from_secs(30),
    )?);
    let (runner_shutdown, runner_shutdown_rx) = tokio::sync::watch::channel(false);
    let runner_task = tokio::spawn(orchestrator.clone().run_worker(runner_shutdown_rx));

    // ── Event Bus ───────────────────────────────────────────────────
    // Bridge adapter events into durable projections and the live event bus.
    let event_bus = state.event_bus.clone();
    {
        let event_bus = event_bus.clone();
        let adapter = adapter.clone();
        let approvals = approvals.clone();
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
                let captured = match approvals.capture_event_frame(&data).await {
                    Ok(captured) => captured,
                    Err(error) => {
                        tracing::warn!("app-server request was not persisted: {error}");
                        continue;
                    }
                };
                let public_data = match captured {
                    Some(approval_id) => match public_approval_frame(&data, approval_id) {
                        Ok(frame) => frame,
                        Err(error) => {
                            tracing::warn!("approval projection failed: {error}");
                            continue;
                        }
                    },
                    None => data,
                };
                match event_projection::persist_frame(&public_data, &projection_db).await {
                    Ok(Some(projected)) => {
                        let live = open_web_codex_platform_store::LiveEvent {
                            organization_id: projected.organization_id,
                            payload: projected.payload,
                        };
                        if event_bus.send(live).is_err() {
                            tracing::debug!("event bus: no active receivers, dropping event");
                        }
                    }
                    Ok(None) => {}
                    Err(error) => tracing::warn!("event projection failed: {error}"),
                }
            }

            let _ = sub.await;
            tracing::info!("event bridge task exiting");
        });
    }

    let mut app = Router::new().nest(
        "/api",
        routes::router(
            adapter,
            providers,
            approvals,
            git,
            orchestrator,
            profile_binding,
        ),
    );
    if cli.web_dist.is_dir() {
        let index = cli.web_dist.join("index.html");
        app = app.fallback_service(ServeDir::new(&cli.web_dist).fallback(ServeFile::new(index)));
        tracing::info!(web_dist = %cli.web_dist.display(), "serving browser application");
    } else {
        tracing::warn!(web_dist = %cli.web_dist.display(), "browser assets not found; serving API only");
    }
    let app = app.layer(TraceLayer::new_for_http()).with_state(state);

    let listener = tokio::net::TcpListener::bind(cli.bind).await?;
    tracing::info!("listening on {}", cli.bind);

    let serve_result = axum::serve(listener, app).await;
    let _ = runner_shutdown.send(true);
    let _ = runner_task.await;
    serve_result?;

    Ok(())
}

fn public_approval_frame(frame: &[u8], approval_id: uuid::Uuid) -> anyhow::Result<Vec<u8>> {
    let payload = frame
        .strip_prefix(b"data: ")
        .and_then(|value| value.strip_suffix(b"\n\n"))
        .ok_or_else(|| anyhow::anyhow!("invalid app-server event frame"))?;
    let envelope: serde_json::Value = serde_json::from_slice(payload)?;
    let workspace_id = envelope.pointer("/params/workspace_id").cloned();
    let message = envelope
        .pointer("/params/message")
        .and_then(serde_json::Value::as_object)
        .ok_or_else(|| anyhow::anyhow!("approval omitted message"))?;
    let request_method = message
        .get("method")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("approval omitted method"))?;
    let params = message
        .get("params")
        .and_then(serde_json::Value::as_object)
        .ok_or_else(|| anyhow::anyhow!("approval omitted params"))?;
    let thread_id = params
        .get("threadId")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("approval omitted threadId"))?;
    let mut request_params = serde_json::Map::new();
    for key in ["threadId", "turnId", "itemId", "reason", "startedAtMs"] {
        if let Some(value) = params.get(key) {
            request_params.insert(key.to_string(), value.clone());
        }
    }
    if request_method == "item/commandExecution/requestApproval" {
        if let Some(command) = params.get("command") {
            request_params.insert("command".to_string(), command.clone());
        }
    }
    if request_method == "item/tool/requestUserInput" {
        if let Some(questions) = params.get("questions") {
            request_params.insert("questions".to_string(), questions.clone());
        }
        if let Some(timeout) = params.get("autoResolutionMs") {
            request_params.insert("autoResolutionMs".to_string(), timeout.clone());
        }
    }
    let public = serde_json::json!({
        "method": "app-server-event",
        "params": {
            "workspace_id": workspace_id,
            "message": {
                "method": "platform/approvalRequested",
                "params": {
                    "approvalId": approval_id,
                    "threadId": thread_id,
                    "requestMethod": request_method,
                    "requestParams": request_params,
                }
            }
        }
    });
    let mut projected = b"data: ".to_vec();
    serde_json::to_writer(&mut projected, &public)?;
    projected.extend_from_slice(b"\n\n");
    Ok(projected)
}

async fn profile_capability_record(
    host: &ProfileHost,
) -> anyhow::Result<routes::RuntimeCapabilityRecord> {
    let snapshot = host.snapshot().await;
    let manifest = host
        .capability_manifest()
        .await
        .ok_or_else(|| anyhow::anyhow!("initialized Profile omitted its Capability Manifest"))?;
    let server_build = snapshot
        .server_build
        .ok_or_else(|| anyhow::anyhow!("initialized Profile omitted its server build"))?;
    let protocol_version = snapshot
        .protocol_version
        .ok_or_else(|| anyhow::anyhow!("initialized Profile omitted its protocol version"))?;
    Ok(routes::RuntimeCapabilityRecord {
        server_build,
        protocol_version,
        manifest: serde_json::to_value(manifest)?,
    })
}

async fn persist_profile_capabilities(
    db: &sqlx::PgPool,
    runtime_key: &str,
    capabilities: &routes::RuntimeCapabilityRecord,
) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO profile_capabilities \
         (profile_id, server_build, protocol_version, manifest, observed_at) \
         SELECT id, $1, $2, $3, now() FROM profiles WHERE runtime_key = $4 \
         ON CONFLICT (profile_id) DO UPDATE SET server_build = EXCLUDED.server_build, \
         protocol_version = EXCLUDED.protocol_version, manifest = EXCLUDED.manifest, \
         observed_at = now()",
    )
    .bind(&capabilities.server_build)
    .bind(&capabilities.protocol_version)
    .bind(&capabilities.manifest)
    .bind(runtime_key)
    .execute(db)
    .await?;
    Ok(())
}

/// Upgrade the previous single-user deployment without guessing ownership in
/// a multi-owner database. New installations create this row in bootstrap.
async fn ensure_transitional_profile_binding(
    db: &sqlx::PgPool,
    runtime_key: &str,
    name: &str,
) -> anyhow::Result<()> {
    let exists: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM profiles WHERE runtime_key = $1)")
            .bind(runtime_key)
            .fetch_one(db)
            .await?;
    if exists {
        return Ok(());
    }

    let owners = sqlx::query(
        "SELECT m.organization_id, m.user_id FROM memberships m \
         WHERE m.role = 'owner' ORDER BY m.created_at, m.id LIMIT 2",
    )
    .fetch_all(db)
    .await?;
    match owners.as_slice() {
        [] => Ok(()),
        [owner] => {
            sqlx::query(
                "INSERT INTO profiles (organization_id, owner_user_id, runtime_key, name) \
                 VALUES ($1, $2, $3, $4) ON CONFLICT (runtime_key) DO NOTHING",
            )
            .bind(owner.get::<uuid::Uuid, _>("organization_id"))
            .bind(owner.get::<uuid::Uuid, _>("user_id"))
            .bind(runtime_key)
            .bind(name)
            .execute(db)
            .await?;
            Ok(())
        }
        _ => anyhow::bail!(
            "Profile '{runtime_key}' has no ownership binding and multiple owner memberships exist"
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::public_approval_frame;
    use serde_json::Value;
    use uuid::Uuid;

    #[test]
    fn public_approval_events_omit_runtime_request_ids_and_server_paths() {
        let mut raw = br#"data: {"method":"app-server-event","params":{"workspace_id":"workspace-1","message":{"id":77,"method":"item/commandExecution/requestApproval","params":{"threadId":"thread-1","itemId":"item-1","cwd":"/private/server/path","command":"git status"}}}}"#.to_vec();
        raw.extend_from_slice(b"\n\n");
        let approval_id = Uuid::now_v7();
        let projected = public_approval_frame(&raw, approval_id).expect("approval projection");
        let text = String::from_utf8(projected.clone()).unwrap();
        assert!(!text.contains("/private/server/path"));
        assert!(!text.contains("\"id\":77"));

        let payload = projected
            .strip_prefix(b"data: ")
            .and_then(|value| value.strip_suffix(b"\n\n"))
            .unwrap();
        let value: Value = serde_json::from_slice(payload).unwrap();
        assert_eq!(
            value.pointer("/params/message/method").unwrap(),
            "platform/approvalRequested"
        );
        assert_eq!(
            value.pointer("/params/message/params/approvalId").unwrap(),
            &Value::String(approval_id.to_string())
        );
        assert_eq!(
            value
                .pointer("/params/message/params/requestMethod")
                .unwrap(),
            "item/commandExecution/requestApproval"
        );
        assert_eq!(
            value
                .pointer("/params/message/params/requestParams/command")
                .unwrap(),
            "git status"
        );
    }
}
