use open_web_codex_platform_contracts::SupervisorPolicySelection;
use open_web_codex_run_orchestrator::SupervisorPolicyLease;
use uuid::Uuid;

use super::{resolve_bound_policy, SupervisorRuntimePreflightError};

#[test]
fn rejects_a_leased_snapshot_that_no_longer_matches_the_published_policy() {
    let policy = crate::supervisor_policy::resolve(&SupervisorPolicySelection {
        policy_id: "enterprise-supervisor-copilot".to_string(),
        version: "1.7.0".to_string(),
    })
    .expect("published policy");
    let mut lease = SupervisorPolicyLease {
        binding_id: Uuid::nil(),
        policy_id: policy.snapshot.policy_id.clone(),
        version: policy.snapshot.version.clone(),
        content_sha256: policy.snapshot.content_sha256.clone(),
        developer_instructions: policy.snapshot.developer_instructions.clone(),
    };
    assert!(resolve_bound_policy(&lease).is_ok());

    lease.content_sha256 = "0".repeat(64);
    assert_eq!(
        resolve_bound_policy(&lease).unwrap_err(),
        SupervisorRuntimePreflightError::PolicySnapshotMismatch
    );
}
