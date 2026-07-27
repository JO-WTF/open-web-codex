//! Explicit, Policy-scoped materialization of published platform Agent
//! Definitions into a Profile's Codex Runtime Role configuration.
//!
//! This is deliberately distinct from `runtime_agent_projections`: those rows
//! observe child Threads after Codex creates them, while this module prepares
//! the Profile configuration that makes a published Runtime Role discoverable.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, OnceLock};

use async_trait::async_trait;
use open_web_codex_adapter::{
    AuthorizedWorkspace, CodexAdapter, PlatformRuntimeRole, ProfileMutation, ProfileQuery,
    ThreadStartMode,
};
use open_web_codex_platform_contracts::SupervisorPolicySelection;
use open_web_codex_platform_store::AppState;
use open_web_codex_run_orchestrator::{
    RunLease, RunStartPreflight, RunStartPreflightError, SupervisorPolicyLease,
};
use serde_json::Value;
use sha2::{Digest, Sha256};
use sqlx::Row;
use thiserror::Error;
use tokio::sync::Mutex;
use uuid::Uuid;

use crate::routes::RuntimeProfileBinding;
use crate::supervisor_policy;

const MIN_PLATFORM_AGENT_THREADS: u32 = 2;
const MIN_PLATFORM_AGENT_DEPTH: u32 = 1;
const MAX_PLATFORM_AGENT_THREADS: u32 = 12;
const MAX_PLATFORM_AGENT_DEPTH: u32 = 4;

#[derive(Debug, Error, PartialEq, Eq)]
pub(crate) enum RuntimeRoleProjectionError {
    #[error("published Platform Runtime Role projection is invalid")]
    Invalid,
    #[error("Codex Profile configuration could not be read")]
    ConfigRead,
    #[error("the Runtime Profile binding is unavailable")]
    ProfileUnavailable,
    #[error("the existing Runtime Role '{0}' is not platform-managed")]
    UnmanagedRoleConflict(String),
    #[error("the existing platform Runtime Role '{0}' belongs to a different published version")]
    VersionConflict(String),
    #[error("the Codex Profile Agent limits are outside supported bounds")]
    InvalidAgentLimits,
    #[error("Codex rejected the platform Runtime Role projection")]
    RuntimeWrite,
    #[error("Codex did not expose the projected Runtime Roles after reload")]
    Verification,
    #[error("the platform could not record the Runtime Role projection")]
    Persistence,
    #[error("the immutable Supervisor Policy snapshot no longer matches the published policy")]
    PolicySnapshotMismatch,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ProjectionRecord {
    definition_id: String,
    definition_version: String,
    runtime_role: String,
    config_file: String,
    content_sha256: String,
    verified: bool,
}

/// Exact limits carried into an official governed Runtime start request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct RuntimeRoleProjectionPlan {
    pub(crate) min_threads: u32,
    pub(crate) min_depth: u32,
}

/// The only production start gate for a platform-managed Supervisor Policy.
///
/// It executes after a worker has claimed a Run lease and immediately before
/// either root creation or an inherited fork can ask Codex to create a Thread.
#[derive(Clone)]
pub(crate) struct SupervisorRuntimeRolePreflight {
    state: AppState,
    adapter: Arc<dyn CodexAdapter>,
    profile: RuntimeProfileBinding,
}

impl SupervisorRuntimeRolePreflight {
    pub(crate) fn new(
        state: AppState,
        adapter: Arc<dyn CodexAdapter>,
        profile: RuntimeProfileBinding,
    ) -> Self {
        Self {
            state,
            adapter,
            profile,
        }
    }
}

#[async_trait]
impl RunStartPreflight for SupervisorRuntimeRolePreflight {
    async fn prepare_runtime_start(
        &self,
        lease: &RunLease,
    ) -> Result<ThreadStartMode, RunStartPreflightError> {
        let Some(bound_policy) = lease.supervisor_policy.as_ref() else {
            return Ok(ThreadStartMode::Standard);
        };

        let policy = resolve_bound_policy(bound_policy).map_err(|error| {
            tracing::warn!(run_id = %lease.run_id, error = %error, "governed Run policy preflight rejected");
            RunStartPreflightError::Rejected(
                "the selected governed Runtime policy is unavailable".to_string(),
            )
        })?;
        let capabilities = self.profile.capabilities.get().await.ok_or_else(|| {
            tracing::warn!(run_id = %lease.run_id, "governed Run has no Runtime capability manifest");
            RunStartPreflightError::Rejected(
                "the selected governed Runtime policy is unavailable".to_string(),
            )
        })?;
        supervisor_policy::require_runtime_manifest(&capabilities.manifest).map_err(|error| {
            tracing::warn!(run_id = %lease.run_id, error = %error, "governed Run capability preflight rejected");
            RunStartPreflightError::Rejected(
                "the selected governed Runtime policy is unavailable".to_string(),
            )
        })?;

        let workspace = AuthorizedWorkspace {
            id: lease.workspace_id.to_string(),
            root: lease.workspace_root.clone(),
        };
        let plan = ensure_platform_runtime_roles(
            &self.state,
            self.adapter.as_ref(),
            lease.organization_id,
            lease.profile_id,
            &self.profile.runtime_key,
            &workspace,
            &policy.required_runtime_roles,
        )
        .await
        .map_err(|error| {
            tracing::warn!(run_id = %lease.run_id, error = %error, "governed Run Role projection preflight rejected");
            RunStartPreflightError::Rejected(
                "the selected governed Runtime policy is unavailable".to_string(),
            )
        })?;

        Ok(ThreadStartMode::GovernedSupervisor {
            developer_instructions: policy.snapshot.developer_instructions,
            roles: policy.required_runtime_roles,
            min_threads: plan.min_threads,
            min_depth: plan.min_depth,
        })
    }
}

fn resolve_bound_policy(
    bound_policy: &SupervisorPolicyLease,
) -> Result<supervisor_policy::ResolvedSupervisorPolicy, RuntimeRoleProjectionError> {
    let policy = supervisor_policy::resolve(&SupervisorPolicySelection {
        policy_id: bound_policy.policy_id.clone(),
        version: bound_policy.version.clone(),
    })
    .map_err(|_| RuntimeRoleProjectionError::PolicySnapshotMismatch)?;
    if policy.snapshot.content_sha256 != bound_policy.content_sha256
        || policy.snapshot.developer_instructions != bound_policy.developer_instructions
    {
        return Err(RuntimeRoleProjectionError::PolicySnapshotMismatch);
    }
    Ok(policy)
}

/// Ensure the exact published roles needed by a bound Supervisor Policy are
/// materialized in the target Profile immediately before Runtime execution.
///
/// Role material comes exclusively from code-published Definitions. No browser
/// request, model message, Plugin, or Skill can choose a file name or
/// instruction body here. Every existing Role is re-verified against the
/// Profile Host and workspace-effective Runtime configuration before mutation;
/// a later official Thread request carries the same governed values again.
pub(crate) async fn ensure_platform_runtime_roles(
    state: &AppState,
    adapter: &dyn CodexAdapter,
    organization_id: Uuid,
    profile_id: Uuid,
    runtime_key: &str,
    workspace: &AuthorizedWorkspace,
    roles: &[PlatformRuntimeRole],
) -> Result<RuntimeRoleProjectionPlan, RuntimeRoleProjectionError> {
    validate_role_set(roles)?;
    let _guard = projection_lock().lock().await;

    require_profile(&state.db, organization_id, profile_id, runtime_key).await?;
    let current = read_runtime_config(adapter).await?;
    let existing = load_projection_records(&state.db, profile_id).await?;
    validate_existing_projection(&current, roles, &existing)?;
    let (max_threads, max_depth) = required_agent_limits(&current)?;

    // A role already present in the Profile must be exact before we permit a
    // retry to write any pending role. This prevents a verified record from
    // silently repairing a manually changed definition.
    let configured_roles = configured_recorded_roles(&current, roles, &existing);
    if !configured_roles.is_empty() {
        adapter
            .verify_platform_runtime_roles(
                workspace,
                &configured_roles,
                max_threads,
                max_depth,
            )
            .await
            .map_err(|error| {
                tracing::error!(error = %error, "existing platform Runtime Role verification failed");
                RuntimeRoleProjectionError::Verification
            })?;
    }

    // Persist the exact intent before a Profile Host mutation. If the Runtime
    // write, reload, or final database update fails, the pending row makes a
    // later retry deterministic without accepting a manually-created Role.
    reserve_projection_records(&state.db, organization_id, profile_id, roles).await?;

    adapter
        .mutate_profile(ProfileMutation::EnsurePlatformRuntimeRoles {
            roles: roles.to_vec(),
            max_threads,
            max_depth,
        })
        .await
        .map_err(|error| {
            tracing::error!(error = %error, "platform Runtime Role projection failed");
            RuntimeRoleProjectionError::RuntimeWrite
        })?;

    adapter
        .verify_platform_runtime_roles(workspace, roles, max_threads, max_depth)
        .await
        .map_err(|error| {
            tracing::error!(error = %error, "projected platform Runtime Role verification failed");
            RuntimeRoleProjectionError::Verification
        })?;
    mark_projection_records_verified(&state.db, organization_id, profile_id, roles).await?;
    Ok(RuntimeRoleProjectionPlan {
        min_threads: max_threads,
        min_depth: max_depth,
    })
}

fn projection_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

async fn read_runtime_config(
    adapter: &dyn CodexAdapter,
) -> Result<Value, RuntimeRoleProjectionError> {
    adapter
        .query_profile(ProfileQuery::Config)
        .await
        .map_err(|error| {
            tracing::error!(error = %error, "could not read Codex Profile configuration");
            RuntimeRoleProjectionError::ConfigRead
        })
}

async fn require_profile(
    db: &sqlx::PgPool,
    organization_id: Uuid,
    profile_id: Uuid,
    runtime_key: &str,
) -> Result<(), RuntimeRoleProjectionError> {
    sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(             SELECT 1 FROM profiles             WHERE id = $1 AND organization_id = $2 AND runtime_key = $3 AND status = 'active'         )",
    )
    .bind(profile_id)
    .bind(organization_id)
    .bind(runtime_key)
    .fetch_one(db)
    .await
    .map_err(|error| {
        tracing::error!(error = %error, "could not resolve Runtime Profile for role projection");
        RuntimeRoleProjectionError::Persistence
    })?
    .then_some(())
    .ok_or(RuntimeRoleProjectionError::ProfileUnavailable)
}

async fn load_projection_records(
    db: &sqlx::PgPool,
    profile_id: Uuid,
) -> Result<BTreeMap<String, ProjectionRecord>, RuntimeRoleProjectionError> {
    let rows = sqlx::query(
        "SELECT definition_id, definition_version, runtime_role, config_file, content_sha256 \
         , verified_at FROM profile_runtime_role_projections WHERE profile_id = $1",
    )
    .bind(profile_id)
    .fetch_all(db)
    .await
    .map_err(|error| {
        tracing::error!(error = %error, "could not load Runtime Role projections");
        RuntimeRoleProjectionError::Persistence
    })?;
    Ok(rows
        .into_iter()
        .map(|row| {
            let record = ProjectionRecord {
                definition_id: row.get("definition_id"),
                definition_version: row.get("definition_version"),
                runtime_role: row.get("runtime_role"),
                config_file: row.get("config_file"),
                content_sha256: row.get("content_sha256"),
                verified: row
                    .get::<Option<chrono::DateTime<chrono::Utc>>, _>("verified_at")
                    .is_some(),
            };
            (record.runtime_role.clone(), record)
        })
        .collect())
}

fn validate_role_set(roles: &[PlatformRuntimeRole]) -> Result<(), RuntimeRoleProjectionError> {
    if roles.is_empty() {
        return Err(RuntimeRoleProjectionError::Invalid);
    }
    let mut names = BTreeSet::new();
    let mut definition_versions = BTreeSet::new();
    for role in roles {
        let expected_config_file = format!(
            "platform-agents/{}/{}.toml",
            role.definition_id, role.version
        );
        let expected_content_sha256 = hex::encode(Sha256::digest(role.config_toml.as_bytes()));
        if role.definition_id.is_empty()
            || role.definition_id.len() > 256
            || role.version.is_empty()
            || role.version.len() > 128
            || role.name.is_empty()
            || role.name.len() > 64
            || !role
                .name
                .chars()
                .all(|value| value.is_ascii_lowercase() || value.is_ascii_digit() || value == '_')
            || role.description.trim().is_empty()
            || role.description.len() > 512
            || role.config_file != expected_config_file
            || role.config_toml.trim().is_empty()
            || role.config_toml.len() > 64 * 1024
            || role.content_sha256.len() != 64
            || !role
                .content_sha256
                .chars()
                .all(|value| value.is_ascii_hexdigit())
            || role.content_sha256 != expected_content_sha256
            || !names.insert(role.name.as_str())
            || !definition_versions.insert((role.definition_id.as_str(), role.version.as_str()))
        {
            return Err(RuntimeRoleProjectionError::Invalid);
        }
    }
    Ok(())
}

fn validate_existing_projection(
    config: &Value,
    roles: &[PlatformRuntimeRole],
    records: &BTreeMap<String, ProjectionRecord>,
) -> Result<(), RuntimeRoleProjectionError> {
    let agents = config.pointer("/config/agents").and_then(Value::as_object);
    for role in roles {
        let matching_record = records.get(&role.name);
        if let Some(record) = matching_record {
            if !record_matches(record, role) {
                return Err(RuntimeRoleProjectionError::VersionConflict(
                    role.name.clone(),
                ));
            }
        }
        let configured = agents.and_then(|agents| agents.get(&role.name));
        if configured.is_some() && matching_record.is_none() {
            return Err(RuntimeRoleProjectionError::UnmanagedRoleConflict(
                role.name.clone(),
            ));
        }
        if matching_record.is_some_and(|record| record.verified) && configured.is_none() {
            return Err(RuntimeRoleProjectionError::Verification);
        }
    }
    Ok(())
}

/// Select existing role declarations that must pass the adapter's exact Host
/// file, SHA-256, workspace-origin and effective-config verification before a
/// projection retry may write anything.
fn configured_recorded_roles(
    config: &Value,
    roles: &[PlatformRuntimeRole],
    records: &BTreeMap<String, ProjectionRecord>,
) -> Vec<PlatformRuntimeRole> {
    let agents = config.pointer("/config/agents").and_then(Value::as_object);
    roles
        .iter()
        .filter(|role| {
            records.contains_key(&role.name)
                && agents.and_then(|agents| agents.get(&role.name)).is_some()
        })
        .cloned()
        .collect()
}

fn required_agent_limits(config: &Value) -> Result<(u32, u32), RuntimeRoleProjectionError> {
    let agents = config.pointer("/config/agents").and_then(Value::as_object);
    let current_threads = agents
        .and_then(|agents| agents.get("max_concurrent_threads_per_session"))
        .and_then(Value::as_u64)
        .map(|value| {
            u32::try_from(value).map_err(|_| RuntimeRoleProjectionError::InvalidAgentLimits)
        })
        .transpose()?
        .unwrap_or(MIN_PLATFORM_AGENT_THREADS);
    let current_depth = agents
        .and_then(|agents| agents.get("max_depth"))
        .and_then(Value::as_u64)
        .map(|value| {
            u32::try_from(value).map_err(|_| RuntimeRoleProjectionError::InvalidAgentLimits)
        })
        .transpose()?
        .unwrap_or(MIN_PLATFORM_AGENT_DEPTH);
    if current_threads > MAX_PLATFORM_AGENT_THREADS || current_depth > MAX_PLATFORM_AGENT_DEPTH {
        return Err(RuntimeRoleProjectionError::InvalidAgentLimits);
    }
    Ok((
        current_threads.max(MIN_PLATFORM_AGENT_THREADS),
        current_depth.max(MIN_PLATFORM_AGENT_DEPTH),
    ))
}

fn record_matches(record: &ProjectionRecord, role: &PlatformRuntimeRole) -> bool {
    record.definition_id == role.definition_id
        && record.definition_version == role.version
        && record.runtime_role == role.name
        && record.config_file == role.config_file
        && record.content_sha256 == role.content_sha256
}

async fn reserve_projection_records(
    db: &sqlx::PgPool,
    organization_id: Uuid,
    profile_id: Uuid,
    roles: &[PlatformRuntimeRole],
) -> Result<(), RuntimeRoleProjectionError> {
    let mut transaction = db.begin().await.map_err(|error| {
        tracing::error!(error = %error, "could not begin Runtime Role projection reservation");
        RuntimeRoleProjectionError::Persistence
    })?;

    for role in roles {
        let existing = sqlx::query(
            "SELECT organization_id, definition_id, definition_version, runtime_role, config_file, content_sha256 \
             FROM profile_runtime_role_projections \
             WHERE profile_id = $1 AND runtime_role = $2 FOR UPDATE",
        )
        .bind(profile_id)
        .bind(&role.name)
        .fetch_optional(&mut *transaction)
        .await
        .map_err(|error| {
            tracing::error!(error = %error, "could not inspect Runtime Role projection reservation");
            RuntimeRoleProjectionError::Persistence
        })?;

        if let Some(existing) = existing {
            let existing_organization_id: Uuid = existing.get("organization_id");
            let record = ProjectionRecord {
                definition_id: existing.get("definition_id"),
                definition_version: existing.get("definition_version"),
                runtime_role: existing.get("runtime_role"),
                config_file: existing.get("config_file"),
                content_sha256: existing.get("content_sha256"),
                verified: false,
            };
            if existing_organization_id != organization_id || !record_matches(&record, role) {
                return Err(RuntimeRoleProjectionError::VersionConflict(
                    role.name.clone(),
                ));
            }
            continue;
        }

        let definition_collision = sqlx::query_scalar::<_, String>(
            "SELECT runtime_role FROM profile_runtime_role_projections \
             WHERE profile_id = $1 AND definition_id = $2 AND definition_version = $3 FOR UPDATE",
        )
        .bind(profile_id)
        .bind(&role.definition_id)
        .bind(&role.version)
        .fetch_optional(&mut *transaction)
        .await
        .map_err(|error| {
            tracing::error!(error = %error, "could not inspect Runtime Role definition reservation");
            RuntimeRoleProjectionError::Persistence
        })?;
        if definition_collision.is_some() {
            return Err(RuntimeRoleProjectionError::VersionConflict(
                role.name.clone(),
            ));
        }

        let inserted = sqlx::query(
            "INSERT INTO profile_runtime_role_projections \
             (organization_id, profile_id, definition_id, definition_version, runtime_role, config_file, content_sha256, verified_at) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, NULL) \
             ON CONFLICT DO NOTHING",
        )
        .bind(organization_id)
        .bind(profile_id)
        .bind(&role.definition_id)
        .bind(&role.version)
        .bind(&role.name)
        .bind(&role.config_file)
        .bind(&role.content_sha256)
        .execute(&mut *transaction)
        .await
        .map_err(|error| {
            tracing::error!(error = %error, "could not reserve Runtime Role projection");
            RuntimeRoleProjectionError::Persistence
        })?;
        if inserted.rows_affected() != 1 {
            // A second server instance may have reserved the same immutable
            // published role while this transaction was in flight. Treat the
            // exact same reservation as idempotent, but never accept a
            // different role/version under either uniqueness constraint.
            let concurrent = sqlx::query(
                "SELECT organization_id, definition_id, definition_version, runtime_role, config_file, content_sha256                  FROM profile_runtime_role_projections                  WHERE profile_id = $1 AND runtime_role = $2 FOR UPDATE",
            )
            .bind(profile_id)
            .bind(&role.name)
            .fetch_optional(&mut *transaction)
            .await
            .map_err(|error| {
                tracing::error!(error = %error, "could not re-read concurrent Runtime Role reservation");
                RuntimeRoleProjectionError::Persistence
            })?;
            let Some(concurrent) = concurrent else {
                return Err(RuntimeRoleProjectionError::VersionConflict(
                    role.name.clone(),
                ));
            };
            let concurrent_organization_id: Uuid = concurrent.get("organization_id");
            let concurrent_record = ProjectionRecord {
                definition_id: concurrent.get("definition_id"),
                definition_version: concurrent.get("definition_version"),
                runtime_role: concurrent.get("runtime_role"),
                config_file: concurrent.get("config_file"),
                content_sha256: concurrent.get("content_sha256"),
                verified: false,
            };
            if concurrent_organization_id != organization_id
                || !record_matches(&concurrent_record, role)
            {
                return Err(RuntimeRoleProjectionError::VersionConflict(
                    role.name.clone(),
                ));
            }
        }
    }

    transaction.commit().await.map_err(|error| {
        tracing::error!(error = %error, "could not commit Runtime Role projection reservation");
        RuntimeRoleProjectionError::Persistence
    })
}

async fn mark_projection_records_verified(
    db: &sqlx::PgPool,
    organization_id: Uuid,
    profile_id: Uuid,
    roles: &[PlatformRuntimeRole],
) -> Result<(), RuntimeRoleProjectionError> {
    let mut transaction = db.begin().await.map_err(|error| {
        tracing::error!(error = %error, "could not begin Runtime Role projection verification");
        RuntimeRoleProjectionError::Persistence
    })?;

    for role in roles {
        let updated = sqlx::query(
            "UPDATE profile_runtime_role_projections \
             SET verified_at = now() \
             WHERE organization_id = $1 AND profile_id = $2 \
               AND definition_id = $3 AND definition_version = $4 \
               AND runtime_role = $5 AND config_file = $6 AND content_sha256 = $7",
        )
        .bind(organization_id)
        .bind(profile_id)
        .bind(&role.definition_id)
        .bind(&role.version)
        .bind(&role.name)
        .bind(&role.config_file)
        .bind(&role.content_sha256)
        .execute(&mut *transaction)
        .await
        .map_err(|error| {
            tracing::error!(error = %error, "could not mark Runtime Role projection verified");
            RuntimeRoleProjectionError::Persistence
        })?;
        if updated.rows_affected() != 1 {
            return Err(RuntimeRoleProjectionError::VersionConflict(
                role.name.clone(),
            ));
        }
    }

    transaction.commit().await.map_err(|error| {
        tracing::error!(error = %error, "could not commit Runtime Role projection verification");
        RuntimeRoleProjectionError::Persistence
    })
}

#[cfg(test)]
mod tests {
    use super::{
        required_agent_limits, resolve_bound_policy, validate_existing_projection,
        validate_role_set, ProjectionRecord, RuntimeRoleProjectionError,
    };
    use open_web_codex_adapter::PlatformRuntimeRole;
    use open_web_codex_platform_contracts::SupervisorPolicySelection;
    use open_web_codex_run_orchestrator::SupervisorPolicyLease;
    use serde_json::json;
    use sha2::{Digest, Sha256};
    use std::collections::BTreeMap;
    use uuid::Uuid;

    fn role() -> PlatformRuntimeRole {
        PlatformRuntimeRole {
            definition_id: "enterprise-data-agent".to_string(),
            version: "1.0.0".to_string(),
            name: "data_agent".to_string(),
            description: "Prepares a bounded planning dataset.".to_string(),
            config_file: "platform-agents/enterprise-data-agent/1.0.0.toml".to_string(),
            config_toml: "developer_instructions = 'Prepare data.'\n".to_string(),
            content_sha256: hex::encode(Sha256::digest(
                b"developer_instructions = 'Prepare data.'\n",
            )),
        }
    }

    fn projected_config(role: &PlatformRuntimeRole) -> serde_json::Value {
        json!({
            "config": {
                "features": { "multi_agent": true },
                "agents": {
                    "enabled": true,
                    "max_concurrent_threads_per_session": 2,
                    "max_depth": 1,
                    role.name.clone(): {
                        "description": role.description.clone(),
                        "config_file": role.config_file.clone(),
                    }
                }
            }
        })
    }
    fn projection_record(role: &PlatformRuntimeRole, verified: bool) -> ProjectionRecord {
        ProjectionRecord {
            definition_id: role.definition_id.clone(),
            definition_version: role.version.clone(),
            runtime_role: role.name.clone(),
            config_file: role.config_file.clone(),
            content_sha256: role.content_sha256.clone(),
            verified,
        }
    }

    fn records_for(
        role: &PlatformRuntimeRole,
        verified: bool,
    ) -> BTreeMap<String, ProjectionRecord> {
        BTreeMap::from([(role.name.clone(), projection_record(role, verified))])
    }

    #[test]
    fn rejects_non_platform_role_material() {
        let mut invalid = role();
        invalid.config_file = "agents/data_agent.toml".to_string();
        assert_eq!(
            validate_role_set(&[invalid]),
            Err(RuntimeRoleProjectionError::Invalid)
        );
    }

    #[test]
    fn rejects_a_leased_snapshot_that_no_longer_matches_the_published_policy() {
        let policy = crate::supervisor_policy::resolve(&SupervisorPolicySelection {
            policy_id: "enterprise-supervisor-copilot".to_string(),
            version: "1.0.0".to_string(),
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
        assert!(matches!(
            resolve_bound_policy(&lease),
            Err(RuntimeRoleProjectionError::PolicySnapshotMismatch)
        ));
    }

    #[test]
    fn existing_same_name_role_requires_a_matching_projection_record() {
        let role = role();
        let config = projected_config(&role);
        assert_eq!(
            validate_existing_projection(&config, &[role], &BTreeMap::new()),
            Err(RuntimeRoleProjectionError::UnmanagedRoleConflict(
                "data_agent".to_string()
            ))
        );
    }

    #[test]
    fn rejects_version_drift_in_a_managed_role() {
        let role = role();
        let mut records = BTreeMap::new();
        records.insert(
            role.name.clone(),
            ProjectionRecord {
                definition_id: role.definition_id.clone(),
                definition_version: "0.9.0".to_string(),
                runtime_role: role.name.clone(),
                config_file: role.config_file.clone(),
                content_sha256: role.content_sha256.clone(),
                verified: true,
            },
        );
        assert_eq!(
            validate_existing_projection(&json!({ "config": {} }), &[role], &records),
            Err(RuntimeRoleProjectionError::VersionConflict(
                "data_agent".to_string()
            ))
        );
    }

    #[test]
    fn pending_projection_retries_when_role_is_absent_or_exact() {
        let role = role();
        let records = records_for(&role, false);
        validate_existing_projection(
            &json!({ "config": { "agents": {} } }),
            std::slice::from_ref(&role),
            &records,
        )
        .unwrap();
        validate_existing_projection(&projected_config(&role), &[role], &records).unwrap();
    }

    #[test]
    fn verified_projection_rejects_removed_runtime_role() {
        let role = role();
        let records = records_for(&role, true);
        assert_eq!(
            validate_existing_projection(
                &json!({ "config": { "agents": {} } }),
                std::slice::from_ref(&role),
                &records,
            ),
            Err(RuntimeRoleProjectionError::Verification)
        );
    }

    #[test]
    fn preserves_higher_existing_limits_but_raises_insufficient_ones() {
        assert_eq!(
            required_agent_limits(&json!({
                "config": { "agents": {
                    "max_concurrent_threads_per_session": 1,
                    "max_depth": 0
                }}
            }))
            .unwrap(),
            (2, 1)
        );
        assert_eq!(
            required_agent_limits(&json!({
                "config": { "agents": {
                    "max_concurrent_threads_per_session": 6,
                    "max_depth": 3
                }}
            }))
            .unwrap(),
            (6, 3)
        );
    }
}
