use std::path::Path;
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use chrono::Utc;
use open_web_codex_adapter::fake::FakeCodexAdapter;
use open_web_codex_adapter::{CodexAdapter, ThreadStartMode};
use open_web_codex_git_runtime::{GitRuntime, GitRuntimeConfig};
use open_web_codex_platform_store::migrate;
use open_web_codex_run_orchestrator::{
    AgentRunSnapshotInput, AgentRunSource, CancelRunRequest, CreateWorkspaceRequest,
    EnqueueRunRequest, RecoverRunRequest, RemoveWorkspaceRequest, ReplayRunRequest,
    RunExecutionSelection, RunLease, RunOrchestrator, RunOrchestratorError, RunStartPreflight,
    RunStartPreflightError, SupervisorPolicySnapshotInput,
};
use sqlx::postgres::PgPoolOptions;
use sqlx::Row;
use tempfile::TempDir;
use uuid::Uuid;

#[derive(Default)]
struct TestRunStartPreflight {
    calls: AtomicUsize,
}

#[async_trait]
impl RunStartPreflight for TestRunStartPreflight {
    async fn prepare_runtime_start(
        &self,
        _lease: &RunLease,
    ) -> Result<ThreadStartMode, RunStartPreflightError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(ThreadStartMode::Standard)
    }
}

fn git(cwd: &Path, args: &[&str]) {
    let output = Command::new("git")
        .args(["-c", "core.hooksPath=/dev/null"])
        .args(args)
        .current_dir(cwd)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()
        .expect("run Git fixture command");
    assert!(
        output.status.success(),
        "Git fixture failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn source_repository(root: &TempDir) -> String {
    let source = root.path().join("source");
    std::fs::create_dir(&source).unwrap();
    git(&source, &["init", "-b", "main"]);
    std::fs::write(source.join("README.md"), "runner fixture\n").unwrap();
    git(&source, &["add", "README.md"]);
    git(
        &source,
        &[
            "-c",
            "user.name=Runner Fixture",
            "-c",
            "user.email=runner@example.invalid",
            "commit",
            "-m",
            "initial",
        ],
    );
    source.to_string_lossy().to_string()
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL pointing at a disposable PostgreSQL database"]
async fn independent_workspace_is_reused_across_run_lifecycles() {
    let database_url = std::env::var("TEST_DATABASE_URL").expect("TEST_DATABASE_URL");
    let pool = PgPoolOptions::new()
        .max_connections(8)
        .connect(&database_url)
        .await
        .expect("connect disposable PostgreSQL database");
    migrate::run(&pool).await.expect("run migrations");

    let fixture = TempDir::new().unwrap();
    let git_url = source_repository(&fixture);
    let organization_id = Uuid::now_v7();
    let user_id = Uuid::now_v7();
    let profile_id = Uuid::now_v7();
    let project_id = Uuid::now_v7();
    let task_id = Uuid::now_v7();
    let runtime_key = format!("runner-profile-{profile_id}");
    sqlx::query("INSERT INTO organizations (id, name, slug) VALUES ($1, 'Runner', $2)")
        .bind(organization_id)
        .bind(format!("runner-{organization_id}"))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO users (id, username, name, email, password_hash, role) \
         VALUES ($1, $2, 'Runner', $3, 'not-a-password', 'owner')",
    )
    .bind(user_id)
    .bind(format!("runner-{user_id}"))
    .bind(format!("{user_id}@example.invalid"))
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO memberships (organization_id, user_id, role) VALUES ($1, $2, 'owner')",
    )
    .bind(organization_id)
    .bind(user_id)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO profiles (id, organization_id, owner_user_id, runtime_key, name) \
         VALUES ($1, $2, $3, $4, 'Runner Profile')",
    )
    .bind(profile_id)
    .bind(organization_id)
    .bind(user_id)
    .bind(&runtime_key)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO projects (id, organization_id, created_by, name, git_url, default_branch) \
         VALUES ($1, $2, $3, 'Project', $4, 'main')",
    )
    .bind(project_id)
    .bind(organization_id)
    .bind(user_id)
    .bind(&git_url)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO tasks (id, organization_id, project_id, created_by, title) \
         VALUES ($1, $2, $3, $4, 'Task')",
    )
    .bind(task_id)
    .bind(organization_id)
    .bind(project_id)
    .bind(user_id)
    .execute(&pool)
    .await
    .unwrap();

    let git_runtime = Arc::new(
        GitRuntime::new(GitRuntimeConfig::new(fixture.path().join("runner")).with_local_sources())
            .unwrap(),
    );
    let adapter: Arc<dyn CodexAdapter> = Arc::new(FakeCodexAdapter::new());
    let preflight = Arc::new(TestRunStartPreflight::default());
    let first = RunOrchestrator::new(
        pool.clone(),
        git_runtime.clone(),
        adapter.clone(),
        preflight.clone(),
        &runtime_key,
        "worker-a",
        Duration::from_secs(30),
    )
    .unwrap();
    let second = RunOrchestrator::new(
        pool.clone(),
        git_runtime.clone(),
        adapter,
        preflight.clone(),
        &runtime_key,
        "worker-b",
        Duration::from_secs(30),
    )
    .unwrap();
    let workspace = first
        .create_workspace(CreateWorkspaceRequest {
            organization_id,
            actor_id: user_id,
            project_id,
            idempotency_key: "workspace-idempotency-0001".to_string(),
            kind: "main".to_string(),
            name: Some("Project workspace".to_string()),
            source_ref: None,
            parent_workspace_id: None,
            copy_agents_md: false,
        })
        .await
        .unwrap();
    assert_eq!(workspace.state, "ready");
    let request = EnqueueRunRequest {
        organization_id,
        actor_id: user_id,
        task_id,
        idempotency_key: "runner-idempotency-0001".to_string(),
        workspace_id: workspace.id,
        fork_thread_id: None,
        fork_source_run_id: None,
        supervisor_policy: Some(SupervisorPolicySnapshotInput {
            policy_id: "enterprise-supervisor-copilot".to_string(),
            version: "1.0.0".to_string(),
            display_name: "Enterprise Supervisor Copilot".to_string(),
            developer_instructions: "Coordinate the approved agents.".to_string(),
            content_sha256: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
                .to_string(),
            source: open_web_codex_run_orchestrator::SupervisorPolicySource::Repository,
            release_id: None,
            draft_definition_id: None,
            draft_revision: None,
        }),
        agent: None,
    };
    let enqueued = first.enqueue_run(request.clone()).await.unwrap();
    let replayed_before_readiness = first
        .replay_run(ReplayRunRequest {
            organization_id,
            actor_id: user_id,
            task_id,
            idempotency_key: request.idempotency_key.clone(),
            workspace_id: workspace.id,
            fork_thread_id: None,
            fork_source_run_id: None,
            execution: RunExecutionSelection::Supervisor {
                policy_id: "enterprise-supervisor-copilot".to_string(),
                version: "1.0.0".to_string(),
            },
        })
        .await
        .unwrap()
        .expect("accepted idempotent Run");
    assert_eq!(enqueued, replayed_before_readiness);
    let replay_mismatch = first
        .replay_run(ReplayRunRequest {
            organization_id,
            actor_id: user_id,
            task_id,
            idempotency_key: request.idempotency_key.clone(),
            workspace_id: workspace.id,
            fork_thread_id: None,
            fork_source_run_id: None,
            execution: RunExecutionSelection::Standard,
        })
        .await
        .unwrap_err();
    assert!(matches!(replay_mismatch, RunOrchestratorError::Conflict(_)));
    let replayed = first.enqueue_run(request).await.unwrap();
    assert_eq!(enqueued, replayed);
    let conflict = first
        .enqueue_run(EnqueueRunRequest {
            organization_id,
            actor_id: user_id,
            task_id,
            idempotency_key: "runner-idempotency-0002".to_string(),
            workspace_id: workspace.id,
            fork_thread_id: None,
            fork_source_run_id: None,
            supervisor_policy: None,
            agent: None,
        })
        .await
        .unwrap_err();
    assert!(matches!(conflict, RunOrchestratorError::Conflict(_)));

    let (claimed_first, claimed_second) = tokio::join!(first.claim_next(), second.claim_next());
    let claimed_first = claimed_first.unwrap();
    let claimed_second = claimed_second.unwrap();
    assert_ne!(claimed_first.is_some(), claimed_second.is_some());
    let (owner, lease) = if let Some(lease) = claimed_first {
        (&first, lease)
    } else {
        (&second, claimed_second.unwrap())
    };
    let policy = lease.supervisor_policy.as_ref().expect("leased policy");
    assert_eq!(policy.policy_id, "enterprise-supervisor-copilot");
    assert_eq!(policy.version, "1.0.0");
    assert_eq!(
        policy.developer_instructions,
        "Coordinate the approved agents."
    );
    assert_eq!(policy.content_sha256, "a".repeat(64));
    owner.execute_lease(&lease).await.unwrap();
    assert_eq!(preflight.calls.load(Ordering::SeqCst), 1);
    let running_supervisor = owner.get_run(organization_id, enqueued.id).await.unwrap();
    let inherited = first
        .resolve_fork_execution(
            organization_id,
            user_id,
            enqueued.id,
            running_supervisor
                .codex_thread_id
                .as_deref()
                .expect("bound source Thread"),
        )
        .await
        .unwrap();
    assert_eq!(
        inherited,
        RunExecutionSelection::Supervisor {
            policy_id: "enterprise-supervisor-copilot".to_string(),
            version: "1.0.0".to_string(),
        }
    );

    let row = sqlx::query(
        "SELECT r.status, r.codex_thread_id, r.workspace_id, w.root_path, w.state \
         FROM runs r JOIN workspaces w ON w.id = r.workspace_id WHERE r.id = $1",
    )
    .bind(enqueued.id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(row.get::<String, _>("status"), "running");
    assert!(row.get::<Option<String>, _>("codex_thread_id").is_some());
    assert_eq!(row.get::<String, _>("state"), "ready");
    let workspace_id: Uuid = row.get("workspace_id");
    assert_eq!(workspace_id, workspace.id);
    let root_path: String = row.get("root_path");
    assert_eq!(
        Path::new(&root_path),
        git_runtime.workspace_path(workspace_id)
    );
    assert!(Path::new(&root_path).is_dir());
    let policy_binding = sqlx::query(
        "SELECT binding.state, binding.thread_id, snapshot.policy_id, snapshot.version \
         FROM supervisor_policy_bindings binding \
         JOIN supervisor_policy_snapshots snapshot ON snapshot.id = binding.snapshot_id \
         WHERE binding.run_id = $1",
    )
    .bind(enqueued.id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(policy_binding.get::<String, _>("state"), "bound");
    assert_eq!(
        policy_binding.get::<Option<String>, _>("thread_id"),
        row.get::<Option<String>, _>("codex_thread_id")
    );
    assert_eq!(
        policy_binding.get::<String, _>("policy_id"),
        "enterprise-supervisor-copilot"
    );
    assert_eq!(policy_binding.get::<String, _>("version"), "1.0.0");
    let root_projection = sqlx::query(
        "SELECT root_run_id, thread_id, source_kind, parent_thread_id \
         FROM runtime_agent_projections WHERE root_run_id = $1",
    )
    .bind(enqueued.id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(root_projection.get::<Uuid, _>("root_run_id"), enqueued.id);
    assert_eq!(
        root_projection.get::<String, _>("thread_id"),
        row.get::<String, _>("codex_thread_id")
    );
    assert_eq!(root_projection.get::<String, _>("source_kind"), "root");
    assert!(root_projection
        .get::<Option<String>, _>("parent_thread_id")
        .is_none());

    sqlx::query("UPDATE runs SET active_turn_id = 'turn-to-cancel' WHERE id = $1")
        .bind(enqueued.id)
        .execute(&pool)
        .await
        .unwrap();
    let cancelled = owner
        .cancel_run(CancelRunRequest {
            organization_id,
            actor_id: user_id,
            allow_organization_admin: false,
            run_id: enqueued.id,
        })
        .await
        .unwrap();
    assert_eq!(cancelled.status, "cancelled");
    assert!(cancelled.active_turn_id.is_none());

    let recovery_run = first
        .enqueue_run(EnqueueRunRequest {
            organization_id,
            actor_id: user_id,
            task_id,
            idempotency_key: "runner-idempotency-0003".to_string(),
            workspace_id: workspace.id,
            fork_thread_id: None,
            fork_source_run_id: None,
            supervisor_policy: None,
            agent: None,
        })
        .await
        .unwrap();
    let recovery_lease = second.claim_next().await.unwrap().unwrap();
    second.execute_lease(&recovery_lease).await.unwrap();
    let recovery_root: String = sqlx::query_scalar(
        "SELECT w.root_path FROM runs r JOIN workspaces w ON w.id = r.workspace_id WHERE r.id = $1",
    )
    .bind(recovery_run.id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(recovery_run.workspace_id, Some(workspace.id));

    sqlx::query("UPDATE runs SET lease_expires_at = now() - interval '1 second' WHERE id = $1")
        .bind(recovery_run.id)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(second.reap_expired().await.unwrap(), 1);
    let status: String = sqlx::query_scalar("SELECT status FROM runs WHERE id = $1")
        .bind(recovery_run.id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(status, "recovery_pending");
    assert!(Path::new(&recovery_root).exists());
    let recovered = owner
        .recover_run(RecoverRunRequest {
            organization_id,
            actor_id: user_id,
            allow_organization_admin: false,
            run_id: recovery_run.id,
        })
        .await
        .unwrap();
    assert_eq!(recovered.status, "running");
    let recovery_owner: Option<String> =
        sqlx::query_scalar("SELECT lease_owner FROM runs WHERE id = $1")
            .bind(recovery_run.id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(recovery_owner.as_deref(), Some("worker-a"));
    sqlx::query(
        "UPDATE runs
            SET active_turn_id = 'stale-turn',
                lease_owner = NULL,
                lease_token = NULL,
                lease_expires_at = NULL,
                updated_at = now() - interval '1 minute'
          WHERE id = $1",
    )
    .bind(recovery_run.id)
    .execute(&pool)
    .await
    .unwrap();
    assert_eq!(
        second
            .reconcile_runs_after_restart(Utc::now())
            .await
            .unwrap(),
        1
    );
    let reconciled = second
        .get_run(organization_id, recovery_run.id)
        .await
        .unwrap();
    assert_eq!(reconciled.status, "recovery_pending");
    assert_eq!(
        reconciled.failure_code.as_deref(),
        Some("runtime_restarted")
    );
    assert!(reconciled.active_turn_id.is_none());
    let cleanup_jobs: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM runner_jobs WHERE state IN ('pending', 'running')",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        cleanup_jobs, 0,
        "Run lease expiry must not schedule Workspace deletion"
    );
    let workspace_state: String = sqlx::query_scalar("SELECT state FROM workspaces WHERE id = $1")
        .bind(workspace.id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(workspace_state, "ready");

    owner
        .cancel_run(CancelRunRequest {
            organization_id,
            actor_id: user_id,
            allow_organization_admin: false,
            run_id: recovery_run.id,
        })
        .await
        .unwrap();
    let agent_run = first
        .enqueue_run(EnqueueRunRequest {
            organization_id,
            actor_id: user_id,
            task_id,
            idempotency_key: "runner-idempotency-agent-0001".to_string(),
            workspace_id: workspace.id,
            fork_thread_id: None,
            fork_source_run_id: None,
            supervisor_policy: None,
            agent: Some(AgentRunSnapshotInput {
                definition_id: "delivery-promise-agent".to_string(),
                version: "1.0.0".to_string(),
                display_name: "Delivery Promise Agent".to_string(),
                content_sha256: "b".repeat(64),
                source: AgentRunSource::Repository,
                release_id: None,
            }),
        })
        .await
        .unwrap();
    let agent_lease = first.claim_next().await.unwrap().expect("Agent Run lease");
    assert!(agent_lease.supervisor_policy.is_none());
    let leased_agent = agent_lease.agent.as_ref().expect("leased root Agent");
    assert_eq!(leased_agent.definition_id, "delivery-promise-agent");
    assert_eq!(leased_agent.version, "1.0.0");
    assert_eq!(leased_agent.content_sha256, "b".repeat(64));
    first.execute_lease(&agent_lease).await.unwrap();
    let agent_binding = sqlx::query(
        "SELECT binding.state, binding.thread_id, snapshot.definition_id, snapshot.version, \
                snapshot.content_sha256 \
         FROM agent_run_bindings binding \
         JOIN agent_run_snapshots snapshot ON snapshot.id = binding.snapshot_id \
         WHERE binding.run_id = $1",
    )
    .bind(agent_run.id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(agent_binding.get::<String, _>("state"), "bound");
    assert!(agent_binding
        .get::<Option<String>, _>("thread_id")
        .is_some());
    assert_eq!(
        agent_binding.get::<String, _>("definition_id"),
        "delivery-promise-agent"
    );
    assert_eq!(agent_binding.get::<String, _>("version"), "1.0.0");
    assert_eq!(
        agent_binding.get::<String, _>("content_sha256"),
        "b".repeat(64)
    );
    first
        .cancel_run(CancelRunRequest {
            organization_id,
            actor_id: user_id,
            allow_organization_admin: false,
            run_id: agent_run.id,
        })
        .await
        .unwrap();
    let draft_definition_id = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO supervisor_definitions \
         (id, organization_id, owner_user_id, policy_id, display_name, description) \
         VALUES ($1, $2, $3, 'draft-supervisor', 'Draft Supervisor', 'Draft test policy')",
    )
    .bind(draft_definition_id)
    .bind(organization_id)
    .bind(user_id)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO supervisor_revisions \
         (organization_id, definition_id, version, draft_spec, created_by) \
         VALUES ($1, $2, '5.0.0', '{}'::jsonb, $3)",
    )
    .bind(organization_id)
    .bind(draft_definition_id)
    .bind(user_id)
    .execute(&pool)
    .await
    .unwrap();
    let draft_run_v1 = first
        .enqueue_run(EnqueueRunRequest {
            organization_id,
            actor_id: user_id,
            task_id,
            idempotency_key: "runner-draft-revision-0001".to_string(),
            workspace_id: workspace.id,
            fork_thread_id: None,
            fork_source_run_id: None,
            supervisor_policy: Some(SupervisorPolicySnapshotInput {
                policy_id: "draft-supervisor".to_string(),
                version: "5.0.0".to_string(),
                display_name: "Draft Supervisor".to_string(),
                developer_instructions: "Draft instructions v1".to_string(),
                content_sha256: "c".repeat(64),
                source: open_web_codex_run_orchestrator::SupervisorPolicySource::Draft,
                release_id: None,
                draft_definition_id: Some(draft_definition_id),
                draft_revision: Some(1),
            }),
            agent: None,
        })
        .await
        .unwrap();
    first
        .cancel_run(CancelRunRequest {
            organization_id,
            actor_id: user_id,
            allow_organization_admin: false,
            run_id: draft_run_v1.id,
        })
        .await
        .unwrap();
    let draft_run_v2 = first
        .enqueue_run(EnqueueRunRequest {
            organization_id,
            actor_id: user_id,
            task_id,
            idempotency_key: "runner-draft-revision-0002".to_string(),
            workspace_id: workspace.id,
            fork_thread_id: None,
            fork_source_run_id: None,
            supervisor_policy: Some(SupervisorPolicySnapshotInput {
                policy_id: "draft-supervisor".to_string(),
                version: "5.0.0".to_string(),
                display_name: "Draft Supervisor".to_string(),
                developer_instructions: "Draft instructions v2".to_string(),
                content_sha256: "c".repeat(64),
                source: open_web_codex_run_orchestrator::SupervisorPolicySource::Draft,
                release_id: None,
                draft_definition_id: Some(draft_definition_id),
                draft_revision: Some(2),
            }),
            agent: None,
        })
        .await
        .unwrap();
    let draft_snapshot_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM supervisor_policy_snapshots \
         WHERE organization_id = $1 AND policy_id = 'draft-supervisor' AND source = 'draft' \
           AND draft_definition_id = $2",
    )
    .bind(organization_id)
    .bind(draft_definition_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(draft_snapshot_count, 2);
    first
        .cancel_run(CancelRunRequest {
            organization_id,
            actor_id: user_id,
            allow_organization_admin: false,
            run_id: draft_run_v2.id,
        })
        .await
        .unwrap();
    std::fs::write(
        git_runtime
            .workspace_path(workspace.id)
            .join("uncommitted-demo.txt"),
        "generated demo data\n",
    )
    .unwrap();
    let removing = owner
        .remove_workspace(RemoveWorkspaceRequest {
            organization_id,
            actor_id: user_id,
            allow_organization_admin: false,
            workspace_id: workspace.id,
        })
        .await
        .unwrap();
    assert_eq!(removing.state, "removing");
    assert!(Path::new(&recovery_root).exists());
    assert!(owner.run_cleanup_once().await.unwrap());
    let removed_state: String = sqlx::query_scalar("SELECT state FROM workspaces WHERE id = $1")
        .bind(workspace.id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(removed_state, "removed");
    assert!(!Path::new(&recovery_root).exists());
}
