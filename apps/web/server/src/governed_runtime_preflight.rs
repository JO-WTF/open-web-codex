//! Exact, policy-scoped preparation for governed root Threads.
//!
//! The platform seals the selected Policy and Agent Definitions, then asks the
//! Profile Host to materialize immutable Runtime Role files. Those Roles are
//! carried only in the governed `thread/start` or `thread/fork` request. They
//! are never registered in persistent Profile configuration and therefore do
//! not create a second Runtime configuration state machine in PostgreSQL.

use std::sync::Arc;

use async_trait::async_trait;
use open_web_codex_adapter::{CodexAdapter, ProfileMutation, ThreadStartMode};
use open_web_codex_git_runtime::GitRuntime;
use open_web_codex_platform_contracts::{AgentRunSelection, SupervisorPolicySelection};
use open_web_codex_run_orchestrator::{
    AgentRunLease, AgentRunSource, RunLease, RunStartPreflight, RunStartPreflightError,
    SupervisorPolicyLease, SupervisorPolicySource,
};
use sqlx::PgPool;
use thiserror::Error;

use crate::routes::RuntimeProfileBinding;
use crate::{agent_catalog, run_readiness, supervisor_policy};

#[derive(Debug, Error, PartialEq, Eq)]
enum GovernedRuntimePreflightError {
    #[error("the immutable Supervisor Policy snapshot no longer matches the published policy")]
    PolicySnapshotMismatch,
    #[error("the immutable root Agent snapshot no longer matches the published Agent")]
    AgentSnapshotMismatch,
}

/// The only production start gate for a platform-managed Agent or Supervisor.
///
/// It runs after a worker claims a Run lease and immediately before Codex
/// creates or forks the governed root Thread.
#[derive(Clone)]
pub(crate) struct GovernedRuntimePreflight {
    adapter: Arc<dyn CodexAdapter>,
    profile: RuntimeProfileBinding,
    db: PgPool,
    git: Arc<GitRuntime>,
}

impl GovernedRuntimePreflight {
    pub(crate) fn new(
        adapter: Arc<dyn CodexAdapter>,
        profile: RuntimeProfileBinding,
        db: PgPool,
        git: Arc<GitRuntime>,
    ) -> Self {
        Self {
            adapter,
            profile,
            db,
            git,
        }
    }
}

#[async_trait]
impl RunStartPreflight for GovernedRuntimePreflight {
    async fn prepare_runtime_start(
        &self,
        lease: &RunLease,
    ) -> Result<ThreadStartMode, RunStartPreflightError> {
        if lease.supervisor_policy.is_some() && lease.agent.is_some() {
            return Err(unavailable_governed_runtime());
        }
        if let Some(bound_agent) = lease.agent.as_ref() {
            let agent = resolve_bound_agent(&self.db, lease.organization_id, bound_agent)
                .await
                .map_err(|error| {
                    tracing::warn!(
                        run_id = %lease.run_id,
                        error = %error,
                        "root Agent preflight rejected"
                    );
                    unavailable_governed_runtime()
                })?;
            run_readiness::verify_agent_dependencies(&self.git, lease.workspace_id, &agent)
                .await
                .map_err(|()| {
                    tracing::warn!(
                        run_id = %lease.run_id,
                        "root Agent immutable Workspace dependencies failed final verification"
                    );
                    unavailable_governed_runtime()
                })?;
            return Ok(ThreadStartMode::GovernedAgent {
                developer_instructions: agent.runtime_developer_instructions,
                required_mcp_servers: agent.required_mcp_servers,
            });
        }
        let Some(bound_policy) = lease.supervisor_policy.as_ref() else {
            return Ok(ThreadStartMode::Standard);
        };

        let policy = resolve_bound_policy(&self.db, lease.organization_id, bound_policy)
            .await
            .map_err(|error| {
                tracing::warn!(
                    run_id = %lease.run_id,
                    error = %error,
                    "governed Run policy preflight rejected"
                );
                unavailable_governed_runtime()
            })?;
        let capabilities = self.profile.capabilities.get().await.ok_or_else(|| {
            tracing::warn!(
                run_id = %lease.run_id,
                "governed Run has no Runtime capability manifest"
            );
            unavailable_governed_runtime()
        })?;
        supervisor_policy::require_runtime_manifest(
            &capabilities.manifest,
            &policy.runtime_requirements,
        )
        .map_err(|error| {
            tracing::warn!(
                run_id = %lease.run_id,
                error = %error,
                "governed Run capability preflight rejected"
            );
            unavailable_governed_runtime()
        })?;
        run_readiness::verify_supervisor_dependencies(&self.git, lease.workspace_id, &policy)
            .await
            .map_err(|()| {
                tracing::warn!(
                    run_id = %lease.run_id,
                    "governed Run immutable Workspace dependencies failed final verification"
                );
                unavailable_governed_runtime()
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
                unavailable_governed_runtime()
            })?;

        Ok(ThreadStartMode::GovernedSupervisor {
            developer_instructions: policy.snapshot.developer_instructions,
            roles: policy.required_runtime_roles,
            role_spawn_limits: policy.role_spawn_limits,
            required_mcp_servers: policy.required_mcp_servers,
            max_threads: policy.max_active_child_agents,
        })
    }
}

async fn resolve_bound_agent(
    db: &PgPool,
    organization_id: uuid::Uuid,
    bound_agent: &AgentRunLease,
) -> Result<
    open_web_codex_supervisor_catalog::agent::ResolvedAgentDefinition,
    GovernedRuntimePreflightError,
> {
    let selection = AgentRunSelection {
        definition_id: bound_agent.definition_id.clone(),
        version: bound_agent.version.clone(),
        release_id: bound_agent.release_id,
    };
    let valid_source = matches!(
        (bound_agent.source, bound_agent.release_id),
        (AgentRunSource::Repository, None) | (AgentRunSource::UserRelease, Some(_))
    );
    if !valid_source {
        return Err(GovernedRuntimePreflightError::AgentSnapshotMismatch);
    }
    let agent = agent_catalog::resolve_run_selection(db, organization_id, &selection)
        .await
        .map_err(|_| GovernedRuntimePreflightError::AgentSnapshotMismatch)?;
    if agent.content_sha256 != bound_agent.content_sha256
        || agent.definition_id != bound_agent.definition_id
        || agent.version != bound_agent.version
        || agent.release_id != bound_agent.release_id
    {
        return Err(GovernedRuntimePreflightError::AgentSnapshotMismatch);
    }
    Ok(agent)
}

fn unavailable_governed_runtime() -> RunStartPreflightError {
    RunStartPreflightError::Rejected(
        "the selected governed execution definition is unavailable".to_string(),
    )
}

async fn resolve_bound_policy(
    db: &PgPool,
    organization_id: uuid::Uuid,
    bound_policy: &SupervisorPolicyLease,
) -> Result<supervisor_policy::ResolvedSupervisorPolicy, GovernedRuntimePreflightError> {
    let policy = match bound_policy.source {
        SupervisorPolicySource::Repository if bound_policy.release_id.is_none() => {
            supervisor_policy::resolve_builtin(&SupervisorPolicySelection {
                policy_id: bound_policy.policy_id.clone(),
                version: bound_policy.version.clone(),
            })
        }
        SupervisorPolicySource::UserRelease => {
            let release_id = bound_policy
                .release_id
                .ok_or(GovernedRuntimePreflightError::PolicySnapshotMismatch)?;
            supervisor_policy::resolve_release(db, organization_id, release_id).await
        }
        SupervisorPolicySource::Draft => {
            let definition_id = bound_policy
                .draft_definition_id
                .ok_or(GovernedRuntimePreflightError::PolicySnapshotMismatch)?;
            supervisor_policy::resolve_draft_for_new_run(db, organization_id, definition_id).await
        }
        _ => Err(supervisor_policy::SupervisorPolicyError::Invalid(
            "snapshot source is invalid",
        )),
    }
    .map_err(|_| GovernedRuntimePreflightError::PolicySnapshotMismatch)?;
    match bound_policy.source {
        SupervisorPolicySource::Draft => {
            if policy.snapshot.draft_revision != bound_policy.draft_revision {
                return Err(GovernedRuntimePreflightError::PolicySnapshotMismatch);
            }
        }
        SupervisorPolicySource::Repository | SupervisorPolicySource::UserRelease => {
            if policy.snapshot.content_sha256 != bound_policy.content_sha256
                || policy.snapshot.developer_instructions != bound_policy.developer_instructions
            {
                return Err(GovernedRuntimePreflightError::PolicySnapshotMismatch);
            }
        }
    }
    Ok(policy)
}

#[cfg(test)]
#[path = "governed_runtime_preflight_tests.rs"]
mod tests;
