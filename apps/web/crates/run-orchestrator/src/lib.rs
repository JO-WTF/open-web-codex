mod agent_run;
mod execution;
mod scheduler;
mod supervisor_policy;
mod workspace;

use async_trait::async_trait;
use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Utc};
use open_web_codex_adapter::{AdapterError, CodexAdapter, ThreadStartMode};
use open_web_codex_git_runtime::{GitRuntime, GitRuntimeError};
use sqlx::PgPool;
use std::path::PathBuf;
use thiserror::Error;
use uuid::Uuid;

/// Server-owned gate evaluated immediately before a leased Run creates or
/// forks a Runtime Thread.
///
/// The orchestrator deliberately has no default implementation: composition
/// must install the policy-specific gate that turns durable Run facts into the
/// exact Runtime start configuration for the current execution.
#[async_trait]
pub trait RunStartPreflight: Send + Sync {
    async fn prepare_runtime_start(
        &self,
        lease: &RunLease,
    ) -> Result<ThreadStartMode, RunStartPreflightError>;
}

#[derive(Debug, Error)]
pub enum RunStartPreflightError {
    #[error("Runtime start preflight rejected the Run: {0}")]
    Rejected(String),
}

#[derive(Debug, Error)]
pub enum RunOrchestratorError {
    #[error("invalid Run request: {0}")]
    Invalid(String),
    #[error("Run resource was not found")]
    NotFound,
    #[error("Run request conflicts with current state: {0}")]
    Conflict(String),
    #[error("Run lease is no longer owned by this worker")]
    LeaseLost,
    #[error("database operation failed: {0}")]
    Database(#[from] sqlx::Error),
    #[error("Git workspace operation failed: {0}")]
    Git(#[from] GitRuntimeError),
    #[error("Codex Runtime operation failed: {0}")]
    Adapter(#[from] AdapterError),
    #[error("Runtime start preflight failed: {0}")]
    StartPreflight(#[from] RunStartPreflightError),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnqueueRunRequest {
    pub organization_id: Uuid,
    pub actor_id: Uuid,
    pub task_id: Uuid,
    pub idempotency_key: String,
    pub workspace_id: Uuid,
    pub fork_thread_id: Option<String>,
    pub fork_source_run_id: Option<Uuid>,
    pub supervisor_policy: Option<SupervisorPolicySnapshotInput>,
    pub agent: Option<AgentRunSnapshotInput>,
}

/// Stable client-selected execution identity used to replay an already
/// accepted Run without re-reading mutable Provider, Runtime, or capability
/// state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RunExecutionSelection {
    Standard,
    Supervisor {
        policy_id: String,
        version: String,
    },
    Agent {
        definition_id: String,
        version: String,
        release_id: Option<Uuid>,
    },
    /// Forked Runs inherit the persisted execution binding of their exact
    /// source Run. They never accept a second direct execution selection.
    Inherited,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplayRunRequest {
    pub organization_id: Uuid,
    pub actor_id: Uuid,
    pub task_id: Uuid,
    pub idempotency_key: String,
    pub workspace_id: Uuid,
    pub fork_thread_id: Option<String>,
    pub fork_source_run_id: Option<Uuid>,
    pub execution: RunExecutionSelection,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentRunSource {
    Repository,
    UserRelease,
}

impl AgentRunSource {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Repository => "repository",
            Self::UserRelease => "user_release",
        }
    }
}

impl std::str::FromStr for AgentRunSource {
    type Err = ();

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "repository" => Ok(Self::Repository),
            "user_release" => Ok(Self::UserRelease),
            _ => Err(()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentRunSnapshotInput {
    pub definition_id: String,
    pub version: String,
    pub display_name: String,
    pub content_sha256: String,
    pub source: AgentRunSource,
    pub release_id: Option<Uuid>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentRunLease {
    pub binding_id: Uuid,
    pub definition_id: String,
    pub version: String,
    pub content_sha256: String,
    pub source: AgentRunSource,
    pub release_id: Option<Uuid>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SupervisorPolicySource {
    Repository,
    UserRelease,
}

impl SupervisorPolicySource {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Repository => "repository",
            Self::UserRelease => "user_release",
        }
    }
}

impl std::str::FromStr for SupervisorPolicySource {
    type Err = ();

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "repository" => Ok(Self::Repository),
            "user_release" => Ok(Self::UserRelease),
            _ => Err(()),
        }
    }
}

/// A server-resolved, immutable Supervisor Policy version.
///
/// The orchestrator persists the exact content before a worker delivers it to
/// Codex. Callers may select an id/version, but must never supply untrusted
/// policy content from a browser request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SupervisorPolicySnapshotInput {
    pub policy_id: String,
    pub version: String,
    pub display_name: String,
    pub developer_instructions: String,
    pub content_sha256: String,
    pub source: SupervisorPolicySource,
    pub release_id: Option<Uuid>,
}

/// Immutable Supervisor Policy facts leased with a Run.
///
/// These facts are read from the persisted binding/snapshot pair while the
/// Run is claimed. The execution-time preflight compares them with the
/// repository-published policy before allowing any Runtime Thread creation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SupervisorPolicyLease {
    pub binding_id: Uuid,
    pub policy_id: String,
    pub version: String,
    pub content_sha256: String,
    pub developer_instructions: String,
    pub source: SupervisorPolicySource,
    pub release_id: Option<Uuid>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunRecord {
    pub id: Uuid,
    pub task_id: Uuid,
    pub status: String,
    pub failure_code: Option<String>,
    pub codex_thread_id: Option<String>,
    pub active_turn_id: Option<String>,
    pub workspace_id: Option<Uuid>,
    pub attempt: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CancelRunRequest {
    pub organization_id: Uuid,
    pub actor_id: Uuid,
    pub allow_organization_admin: bool,
    pub run_id: Uuid,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecoverRunRequest {
    pub organization_id: Uuid,
    pub actor_id: Uuid,
    pub allow_organization_admin: bool,
    pub run_id: Uuid,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreateWorkspaceRequest {
    pub organization_id: Uuid,
    pub actor_id: Uuid,
    pub project_id: Uuid,
    pub idempotency_key: String,
    pub kind: String,
    pub name: Option<String>,
    pub source_ref: Option<String>,
    pub parent_workspace_id: Option<Uuid>,
    pub copy_agents_md: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoveWorkspaceRequest {
    pub organization_id: Uuid,
    pub actor_id: Uuid,
    pub allow_organization_admin: bool,
    pub workspace_id: Uuid,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceRecord {
    pub id: Uuid,
    pub project_id: Uuid,
    pub profile_id: Uuid,
    pub name: String,
    pub kind: String,
    pub state: String,
    pub source_ref: String,
    pub branch_name: Option<String>,
    pub parent_workspace_id: Option<Uuid>,
    pub group_workspace_id: Option<Uuid>,
    pub managed: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct RunLease {
    pub run_id: Uuid,
    pub organization_id: Uuid,
    pub actor_id: Uuid,
    pub profile_id: Uuid,
    pub workspace_id: Uuid,
    pub workspace_root: PathBuf,
    pub fork_thread_id: Option<String>,
    pub fork_source_run_id: Option<Uuid>,
    pub supervisor_policy: Option<SupervisorPolicyLease>,
    pub agent: Option<AgentRunLease>,
    pub token: String,
}

#[derive(Clone)]
pub struct RunOrchestrator {
    pub(crate) db: PgPool,
    pub(crate) git: Arc<GitRuntime>,
    pub(crate) adapter: Arc<dyn CodexAdapter>,
    pub(crate) start_preflight: Arc<dyn RunStartPreflight>,
    pub(crate) runtime_key: String,
    pub(crate) worker_id: String,
    pub(crate) lease_ttl: Duration,
}

impl RunOrchestrator {
    pub fn new(
        db: PgPool,
        git: Arc<GitRuntime>,
        adapter: Arc<dyn CodexAdapter>,
        start_preflight: Arc<dyn RunStartPreflight>,
        runtime_key: impl Into<String>,
        worker_id: impl Into<String>,
        lease_ttl: Duration,
    ) -> Result<Self, RunOrchestratorError> {
        if lease_ttl < Duration::from_secs(5) {
            return Err(RunOrchestratorError::Invalid(
                "lease TTL must be at least five seconds".to_string(),
            ));
        }
        let worker_id = worker_id.into();
        if worker_id.trim().is_empty() || worker_id.len() > 128 {
            return Err(RunOrchestratorError::Invalid(
                "worker id must contain 1 to 128 characters".to_string(),
            ));
        }
        Ok(Self {
            db,
            git,
            adapter,
            start_preflight,
            runtime_key: runtime_key.into(),
            worker_id,
            lease_ttl,
        })
    }

    pub async fn run_worker(self: Arc<Self>, mut shutdown: tokio::sync::watch::Receiver<bool>) {
        let mut interval = tokio::time::interval(Duration::from_secs(2));
        loop {
            tokio::select! {
                _ = interval.tick() => {
                    if let Err(error) = self.heartbeat_owned_runs().await {
                        tracing::warn!(%error, "Runner heartbeat failed");
                    }
                    if let Err(error) = self.reap_expired().await {
                        tracing::warn!(%error, "Runner lease recovery failed");
                    }
                    if let Err(error) = self.run_once().await {
                        tracing::warn!(%error, "Runner execution failed");
                    }
                    if let Err(error) = self.run_cleanup_once().await {
                        tracing::warn!(%error, "Runner cleanup failed");
                    }
                }
                changed = shutdown.changed() => {
                    if changed.is_err() || *shutdown.borrow() {
                        break;
                    }
                }
            }
        }
    }
}

fn validate_idempotency_key(key: &str) -> Result<(), RunOrchestratorError> {
    if !(8..=128).contains(&key.len())
        || !key
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || "-._:".contains(character))
    {
        return Err(RunOrchestratorError::Invalid(
            "idempotency key must contain 8 to 128 safe ASCII characters".to_string(),
        ));
    }
    Ok(())
}

fn chrono_ttl(ttl: Duration) -> Result<chrono::Duration, RunOrchestratorError> {
    chrono::Duration::from_std(ttl)
        .map_err(|_| RunOrchestratorError::Invalid("lease TTL is too large".to_string()))
}
