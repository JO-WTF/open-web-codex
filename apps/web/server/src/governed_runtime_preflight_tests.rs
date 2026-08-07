use open_web_codex_platform_contracts::SupervisorPolicySelection;
use open_web_codex_run_orchestrator::{AgentRunLease, AgentRunSource};
use uuid::Uuid;

use super::{resolve_bound_agent, GovernedRuntimePreflightError};

#[test]
fn retired_repository_supervisor_release_is_not_resolvable() {
    let error = crate::supervisor_policy::resolve_builtin(&SupervisorPolicySelection {
        policy_id: "enterprise-supervisor-copilot".to_string(),
        version: "5.0.1".to_string(),
    })
    .unwrap_err();
    assert_eq!(
        error,
        crate::supervisor_policy::SupervisorPolicyError::NotFound
    );
}

#[tokio::test]
async fn rejects_a_root_agent_snapshot_that_no_longer_matches_the_published_agent() {
    let agent =
        open_web_codex_supervisor_catalog::agent::resolve_builtin("enterprise-data-agent", "5.1.0")
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
