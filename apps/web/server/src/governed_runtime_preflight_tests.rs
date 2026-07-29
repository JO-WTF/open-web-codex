use open_web_codex_platform_contracts::SupervisorPolicySelection;
use open_web_codex_run_orchestrator::{AgentRunLease, AgentRunSource, SupervisorPolicyLease};
use uuid::Uuid;

use super::{resolve_bound_agent, resolve_bound_policy, GovernedRuntimePreflightError};

#[tokio::test]
async fn rejects_a_leased_snapshot_that_no_longer_matches_the_published_policy() {
    let policy = crate::supervisor_policy::resolve_builtin(&SupervisorPolicySelection {
        policy_id: "enterprise-supervisor-copilot".to_string(),
        version: "3.7.0".to_string(),
    })
    .expect("published policy");
    let mut lease = SupervisorPolicyLease {
        binding_id: Uuid::nil(),
        policy_id: policy.snapshot.policy_id.clone(),
        version: policy.snapshot.version.clone(),
        content_sha256: policy.snapshot.content_sha256.clone(),
        developer_instructions: policy.snapshot.developer_instructions.clone(),
        source: open_web_codex_run_orchestrator::SupervisorPolicySource::Repository,
        release_id: None,
    };
    let db = sqlx::postgres::PgPoolOptions::new()
        .connect_lazy("postgresql://unused:unused@127.0.0.1/unused")
        .unwrap();
    assert!(resolve_bound_policy(&db, Uuid::nil(), &lease).await.is_ok());

    lease.content_sha256 = "0".repeat(64);
    assert_eq!(
        resolve_bound_policy(&db, Uuid::nil(), &lease)
            .await
            .unwrap_err(),
        GovernedRuntimePreflightError::PolicySnapshotMismatch
    );
}

#[tokio::test]
async fn rejects_a_root_agent_snapshot_that_no_longer_matches_the_published_agent() {
    let agent =
        open_web_codex_supervisor_catalog::agent::resolve_builtin("enterprise-data-agent", "3.1.0")
            .expect("published Agent");
    let mut lease = AgentRunLease {
        binding_id: Uuid::nil(),
        definition_id: agent.definition_id.clone(),
        version: agent.version.clone(),
        content_sha256: agent.content_sha256.clone(),
        source: AgentRunSource::Repository,
        release_id: None,
    };
    let db = sqlx::postgres::PgPoolOptions::new()
        .connect_lazy("postgresql://unused:unused@127.0.0.1/unused")
        .unwrap();
    assert!(resolve_bound_agent(&db, Uuid::nil(), &lease).await.is_ok());

    lease.content_sha256 = "0".repeat(64);
    assert_eq!(
        resolve_bound_agent(&db, Uuid::nil(), &lease)
            .await
            .unwrap_err(),
        GovernedRuntimePreflightError::AgentSnapshotMismatch
    );
}
