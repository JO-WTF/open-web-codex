//! Policy-scoped preparation for governed Supervisor Threads.
//!
//! The platform seals the selected Policy and Agent Definitions, then asks the
//! Profile Host to materialize immutable Runtime Role files. Those Roles are
//! carried only in the governed `thread/start` or `thread/fork` request. They
//! are never registered in persistent Profile configuration and therefore do
//! not create a second Runtime configuration state machine in PostgreSQL.

use std::sync::Arc;

use async_trait::async_trait;
use open_web_codex_adapter::{CodexAdapter, ProfileMutation, ThreadStartMode};
use open_web_codex_platform_contracts::SupervisorPolicySelection;
use open_web_codex_run_orchestrator::{
    RunLease, RunStartPreflight, RunStartPreflightError, SupervisorPolicyLease,
};
use thiserror::Error;

use crate::routes::RuntimeProfileBinding;
use crate::supervisor_policy;

const GOVERNED_MAX_AGENT_THREADS: u32 = 2;

#[derive(Debug, Error, PartialEq, Eq)]
enum SupervisorRuntimePreflightError {
    #[error("the immutable Supervisor Policy snapshot no longer matches the published policy")]
    PolicySnapshotMismatch,
}

/// The only production start gate for a platform-managed Supervisor Policy.
///
/// It runs after a worker claims a Run lease and immediately before Codex
/// creates or forks the governed root Thread.
#[derive(Clone)]
pub(crate) struct SupervisorRuntimePreflight {
    adapter: Arc<dyn CodexAdapter>,
    profile: RuntimeProfileBinding,
}

impl SupervisorRuntimePreflight {
    pub(crate) fn new(adapter: Arc<dyn CodexAdapter>, profile: RuntimeProfileBinding) -> Self {
        Self { adapter, profile }
    }
}

#[async_trait]
impl RunStartPreflight for SupervisorRuntimePreflight {
    async fn prepare_runtime_start(
        &self,
        lease: &RunLease,
    ) -> Result<ThreadStartMode, RunStartPreflightError> {
        let Some(bound_policy) = lease.supervisor_policy.as_ref() else {
            return Ok(ThreadStartMode::Standard);
        };

        let policy = resolve_bound_policy(bound_policy).map_err(|error| {
            tracing::warn!(
                run_id = %lease.run_id,
                error = %error,
                "governed Run policy preflight rejected"
            );
            unavailable_policy()
        })?;
        let capabilities = self.profile.capabilities.get().await.ok_or_else(|| {
            tracing::warn!(
                run_id = %lease.run_id,
                "governed Run has no Runtime capability manifest"
            );
            unavailable_policy()
        })?;
        supervisor_policy::require_runtime_manifest(&capabilities.manifest).map_err(|error| {
            tracing::warn!(
                run_id = %lease.run_id,
                error = %error,
                "governed Run capability preflight rejected"
            );
            unavailable_policy()
        })?;

        self.adapter
            .mutate_profile(ProfileMutation::MaterializePlatformRuntimeRoleFiles {
                roles: policy.required_runtime_roles.clone(),
            })
            .await
            .map_err(|error| {
                tracing::error!(
                    run_id = %lease.run_id,
                    error = %error,
                    "governed Runtime Role materialization failed"
                );
                unavailable_policy()
            })?;

        Ok(ThreadStartMode::GovernedSupervisor {
            developer_instructions: policy.snapshot.developer_instructions,
            roles: policy.required_runtime_roles,
            role_spawn_limits: policy.role_spawn_limits,
            required_mcp_servers: policy.required_mcp_servers,
            max_threads: GOVERNED_MAX_AGENT_THREADS,
        })
    }
}

fn unavailable_policy() -> RunStartPreflightError {
    RunStartPreflightError::Rejected(
        "the selected governed Runtime policy is unavailable".to_string(),
    )
}

fn resolve_bound_policy(
    bound_policy: &SupervisorPolicyLease,
) -> Result<supervisor_policy::ResolvedSupervisorPolicy, SupervisorRuntimePreflightError> {
    let policy = supervisor_policy::resolve(&SupervisorPolicySelection {
        policy_id: bound_policy.policy_id.clone(),
        version: bound_policy.version.clone(),
    })
    .map_err(|_| SupervisorRuntimePreflightError::PolicySnapshotMismatch)?;
    if policy.snapshot.content_sha256 != bound_policy.content_sha256
        || policy.snapshot.developer_instructions != bound_policy.developer_instructions
    {
        return Err(SupervisorRuntimePreflightError::PolicySnapshotMismatch);
    }
    Ok(policy)
}

#[cfg(test)]
#[path = "supervisor_runtime_preflight_tests.rs"]
mod tests;
