use chrono::Utc;
use sqlx::Row;
use uuid::Uuid;

use crate::agent_run::ensure_run_agent_binding;
use crate::supervisor_policy::ensure_run_policy_binding;
use crate::{
    chrono_ttl, validate_idempotency_key, AgentRunLease, CancelRunRequest, EnqueueRunRequest,
    RecoverRunRequest, ReplayRunRequest, RunExecutionSelection, RunLease, RunOrchestrator,
    RunOrchestratorError, RunRecord, SupervisorPolicyLease,
};

impl RunOrchestrator {
    /// Resolve the exact immutable execution identity inherited by a fork.
    ///
    /// The worker still revalidates and binds the source snapshot
    /// transactionally at enqueue time. This read-only projection lets the
    /// readiness endpoint evaluate the same Agent/Supervisor requirements
    /// instead of incorrectly treating every fork as a Standard Run.
    pub async fn resolve_fork_execution(
        &self,
        organization_id: uuid::Uuid,
        actor_id: uuid::Uuid,
        source_run_id: uuid::Uuid,
        source_thread_id: &str,
    ) -> Result<RunExecutionSelection, RunOrchestratorError> {
        if source_thread_id.trim().is_empty() || source_thread_id.len() > 256 {
            return Err(RunOrchestratorError::Invalid(
                "fork source Thread id is invalid".to_string(),
            ));
        }
        let row = sqlx::query(
            "SELECT policy.policy_id, policy.version AS policy_version, \
                    agent.definition_id AS agent_definition_id, \
                    agent.version AS agent_version, agent.release_id AS agent_release_id \
             FROM runs source \
             JOIN workspaces workspace ON workspace.id = source.workspace_id \
             JOIN workspace_grants workspace_grant ON workspace_grant.workspace_id = workspace.id \
               AND workspace_grant.organization_id = workspace.organization_id \
               AND workspace_grant.user_id = $2 \
               AND workspace_grant.profile_id = workspace.profile_id \
             LEFT JOIN supervisor_policy_bindings policy_binding \
               ON policy_binding.run_id = source.id \
             LEFT JOIN supervisor_policy_snapshots policy \
               ON policy.id = policy_binding.snapshot_id \
             LEFT JOIN agent_run_bindings agent_binding ON agent_binding.run_id = source.id \
             LEFT JOIN agent_run_snapshots agent ON agent.id = agent_binding.snapshot_id \
             WHERE source.id = $1 AND source.organization_id = $3 \
               AND source.requested_by = $2 AND source.codex_thread_id = $4 \
               AND workspace.state IN ('ready', 'retained')",
        )
        .bind(source_run_id)
        .bind(actor_id)
        .bind(organization_id)
        .bind(source_thread_id)
        .fetch_optional(&self.db)
        .await?
        .ok_or(RunOrchestratorError::NotFound)?;
        let policy_id = row.get::<Option<String>, _>("policy_id");
        let agent_definition_id = row.get::<Option<String>, _>("agent_definition_id");
        match (policy_id, agent_definition_id) {
            (Some(_), Some(_)) => Err(RunOrchestratorError::Conflict(
                "source Run has conflicting execution policy bindings".to_string(),
            )),
            (Some(policy_id), None) => Ok(RunExecutionSelection::Supervisor {
                policy_id,
                version: row
                    .get::<Option<String>, _>("policy_version")
                    .ok_or_else(|| {
                        RunOrchestratorError::Conflict(
                            "source Run has an incomplete Supervisor binding".to_string(),
                        )
                    })?,
            }),
            (None, Some(definition_id)) => Ok(RunExecutionSelection::Agent {
                definition_id,
                version: row
                    .get::<Option<String>, _>("agent_version")
                    .ok_or_else(|| {
                        RunOrchestratorError::Conflict(
                            "source Run has an incomplete Agent binding".to_string(),
                        )
                    })?,
                release_id: row.get("agent_release_id"),
            }),
            (None, None) => Ok(RunExecutionSelection::Standard),
        }
    }

    /// Return an already accepted Run for the same idempotent request.
    ///
    /// This lookup deliberately runs before volatile readiness evaluation in
    /// the HTTP owner. Once a Run has been accepted, a lost response must be
    /// replayable even if Provider, Runtime, or Workspace dependency health
    /// changes afterward.
    pub async fn replay_run(
        &self,
        request: ReplayRunRequest,
    ) -> Result<Option<RunRecord>, RunOrchestratorError> {
        validate_idempotency_key(&request.idempotency_key)?;
        match (
            request.fork_thread_id.as_deref(),
            request.fork_source_run_id,
            &request.execution,
        ) {
            (Some(thread_id), Some(_), RunExecutionSelection::Inherited)
                if !thread_id.trim().is_empty() && thread_id.len() <= 256 => {}
            (None, None, RunExecutionSelection::Inherited) => {
                return Err(RunOrchestratorError::Invalid(
                    "inherited execution requires an exact fork source".to_string(),
                ));
            }
            (None, None, _) => {}
            (Some(_), Some(_), _) => {
                return Err(RunOrchestratorError::Invalid(
                    "forked Runs inherit their source execution policy".to_string(),
                ));
            }
            _ => {
                return Err(RunOrchestratorError::Invalid(
                    "fork source Thread and Run must be provided together".to_string(),
                ));
            }
        }

        let row = sqlx::query(
            "SELECT run.id, run.task_id, run.status, run.failure_code, run.codex_thread_id, \
                    run.active_turn_id, run.workspace_id, run.attempt, run.created_at, \
                    run.updated_at, run.fork_thread_id, run.fork_source_run_id, \
                    policy.policy_id, policy.version AS policy_version, \
                    agent.definition_id AS agent_definition_id, \
                    agent.version AS agent_version, agent.release_id AS agent_release_id \
             FROM runs run \
             LEFT JOIN supervisor_policy_bindings policy_binding \
               ON policy_binding.run_id = run.id \
             LEFT JOIN supervisor_policy_snapshots policy \
               ON policy.id = policy_binding.snapshot_id \
             LEFT JOIN agent_run_bindings agent_binding ON agent_binding.run_id = run.id \
             LEFT JOIN agent_run_snapshots agent ON agent.id = agent_binding.snapshot_id \
             WHERE run.organization_id = $1 AND run.requested_by = $2 \
               AND run.idempotency_key = $3",
        )
        .bind(request.organization_id)
        .bind(request.actor_id)
        .bind(&request.idempotency_key)
        .fetch_optional(&self.db)
        .await?;
        let Some(row) = row else {
            return Ok(None);
        };

        if row.get::<Uuid, _>("task_id") != request.task_id
            || row.get::<Option<Uuid>, _>("workspace_id") != Some(request.workspace_id)
            || row.get::<Option<String>, _>("fork_thread_id") != request.fork_thread_id
            || row.get::<Option<Uuid>, _>("fork_source_run_id") != request.fork_source_run_id
        {
            return Err(RunOrchestratorError::Conflict(
                "idempotency key was already used for another Run request".to_string(),
            ));
        }

        let policy_id = row.get::<Option<String>, _>("policy_id");
        let agent_definition_id = row.get::<Option<String>, _>("agent_definition_id");
        if policy_id.is_some() && agent_definition_id.is_some() {
            return Err(RunOrchestratorError::Conflict(
                "accepted Run has conflicting execution policy bindings".to_string(),
            ));
        }
        if request.execution != RunExecutionSelection::Inherited {
            let accepted = if let Some(policy_id) = policy_id {
                RunExecutionSelection::Supervisor {
                    policy_id,
                    version: row
                        .get::<Option<String>, _>("policy_version")
                        .ok_or_else(|| {
                            RunOrchestratorError::Conflict(
                                "accepted Run has an incomplete Supervisor binding".to_string(),
                            )
                        })?,
                }
            } else if let Some(definition_id) = agent_definition_id {
                RunExecutionSelection::Agent {
                    definition_id,
                    version: row
                        .get::<Option<String>, _>("agent_version")
                        .ok_or_else(|| {
                            RunOrchestratorError::Conflict(
                                "accepted Run has an incomplete Agent binding".to_string(),
                            )
                        })?,
                    release_id: row.get("agent_release_id"),
                }
            } else {
                RunExecutionSelection::Standard
            };
            if accepted != request.execution {
                return Err(RunOrchestratorError::Conflict(
                    "idempotency key was already used with a different execution selection"
                        .to_string(),
                ));
            }
        }
        Ok(Some(run_record(&row)))
    }

    pub async fn enqueue_run(
        &self,
        request: EnqueueRunRequest,
    ) -> Result<RunRecord, RunOrchestratorError> {
        validate_idempotency_key(&request.idempotency_key)?;
        if request.supervisor_policy.is_some() && request.agent.is_some() {
            return Err(RunOrchestratorError::Invalid(
                "a Run may select either one root Agent or one Supervisor Policy".to_string(),
            ));
        }
        let mut transaction = self.db.begin().await?;
        let row = sqlx::query(
            "SELECT task.project_id, profile.id AS profile_id \
             FROM tasks task \
             JOIN profiles profile ON profile.organization_id = task.organization_id \
               AND profile.owner_user_id = $2 AND profile.runtime_key = $4 \
               AND profile.status = 'active' \
             JOIN workspaces workspace ON workspace.id = $5 \
               AND workspace.organization_id = task.organization_id \
               AND workspace.project_id = task.project_id \
               AND workspace.profile_id = profile.id \
               AND workspace.state IN ('ready', 'retained') \
             JOIN workspace_grants workspace_grant ON workspace_grant.workspace_id = workspace.id \
               AND workspace_grant.organization_id = workspace.organization_id \
               AND workspace_grant.user_id = $2 AND workspace_grant.profile_id = profile.id \
               AND workspace_grant.role IN ('owner', 'write') \
             WHERE task.id = $1 AND task.organization_id = $3 \
             FOR KEY SHARE OF workspace",
        )
        .bind(request.task_id)
        .bind(request.actor_id)
        .bind(request.organization_id)
        .bind(&self.runtime_key)
        .bind(request.workspace_id)
        .fetch_optional(&mut *transaction)
        .await?
        .ok_or(RunOrchestratorError::NotFound)?;
        let profile_id: Uuid = row.get("profile_id");

        let (inherited_policy_snapshot_id, inherited_agent_snapshot_id) = if let Some(
            fork_thread_id,
        ) =
            request.fork_thread_id.as_deref()
        {
            if fork_thread_id.trim().is_empty() || fork_thread_id.len() > 256 {
                return Err(RunOrchestratorError::Invalid(
                    "fork source Thread id is invalid".to_string(),
                ));
            }
            let Some(source_run_id) = request.fork_source_run_id else {
                return Err(RunOrchestratorError::Invalid(
                    "forked Runs require their source parent Run".to_string(),
                ));
            };
            if request.supervisor_policy.is_some() || request.agent.is_some() {
                return Err(RunOrchestratorError::Invalid(
                    "forked Runs inherit their source execution policy".to_string(),
                ));
            }
            let source = sqlx::query(
                "SELECT policy_binding.snapshot_id AS policy_snapshot_id, \
                        agent_binding.snapshot_id AS agent_snapshot_id \
                   FROM runs source \
                   JOIN workspaces workspace ON workspace.id = source.workspace_id \
                   JOIN workspace_grants workspace_grant ON workspace_grant.workspace_id = workspace.id \
                     AND workspace_grant.organization_id = workspace.organization_id \
                     AND workspace_grant.user_id = $3 AND workspace_grant.profile_id = workspace.profile_id \
                   LEFT JOIN supervisor_policy_bindings policy_binding \
                     ON policy_binding.run_id = source.id \
                   LEFT JOIN agent_run_bindings agent_binding ON agent_binding.run_id = source.id \
                   WHERE source.id = $1 AND source.organization_id = $2 \
                     AND source.requested_by = $3 AND source.codex_thread_id = $4 \
                     AND workspace.state IN ('ready', 'retained') \
                 ",
            )
            .bind(source_run_id)
            .bind(request.organization_id)
            .bind(request.actor_id)
            .bind(fork_thread_id)
            .fetch_optional(&mut *transaction)
            .await?;
            let source = source.ok_or(RunOrchestratorError::NotFound)?;
            let policy_snapshot_id: Option<Uuid> = source.get("policy_snapshot_id");
            let agent_snapshot_id: Option<Uuid> = source.get("agent_snapshot_id");
            if policy_snapshot_id.is_some() && agent_snapshot_id.is_some() {
                return Err(RunOrchestratorError::Conflict(
                    "source Run has conflicting execution policy bindings".to_string(),
                ));
            }
            (policy_snapshot_id, agent_snapshot_id)
        } else if request.fork_source_run_id.is_some() {
            return Err(RunOrchestratorError::Invalid(
                "fork source Run requires a source Thread id".to_string(),
            ));
        } else {
            (None, None)
        };

        let inserted = sqlx::query(
            "INSERT INTO runs \
             (organization_id, task_id, requested_by, requested_profile_id, workspace_id, \
              idempotency_key, fork_thread_id, fork_source_run_id, status) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, 'pending') \
             ON CONFLICT (organization_id, requested_by, idempotency_key) \
               WHERE requested_by IS NOT NULL AND idempotency_key IS NOT NULL \
             DO NOTHING \
             RETURNING id, task_id, status, failure_code, codex_thread_id, active_turn_id, workspace_id, \
                       attempt, created_at, updated_at",
        )
        .bind(request.organization_id)
        .bind(request.task_id)
        .bind(request.actor_id)
        .bind(profile_id)
        .bind(request.workspace_id)
        .bind(&request.idempotency_key)
        .bind(&request.fork_thread_id)
        .bind(request.fork_source_run_id)
        .fetch_optional(&mut *transaction)
        .await;

        let row = match inserted {
            Ok(Some(row)) => row,
            Ok(None) => {
                let existing = sqlx::query(
                    "SELECT id, task_id, status, failure_code, codex_thread_id, active_turn_id, workspace_id, \
                            attempt, created_at, updated_at \
                     FROM runs WHERE organization_id = $1 AND requested_by = $2 \
                       AND idempotency_key = $3",
                )
                .bind(request.organization_id)
                .bind(request.actor_id)
                .bind(&request.idempotency_key)
                .fetch_one(&mut *transaction)
                .await?;
                if existing.get::<Uuid, _>("task_id") != request.task_id
                    || existing.get::<Option<Uuid>, _>("workspace_id") != Some(request.workspace_id)
                {
                    return Err(RunOrchestratorError::Conflict(
                        "idempotency key was already used for another Task or Workspace"
                            .to_string(),
                    ));
                }
                existing
            }
            Err(sqlx::Error::Database(error)) if error.is_unique_violation() => {
                return Err(RunOrchestratorError::Conflict(
                    "Task already has an active Run".to_string(),
                ));
            }
            Err(error) => return Err(error.into()),
        };
        let run = run_record(&row);
        ensure_run_policy_binding(
            &mut transaction,
            request.organization_id,
            profile_id,
            request.task_id,
            run.id,
            request.supervisor_policy.as_ref(),
            inherited_policy_snapshot_id,
        )
        .await?;
        ensure_run_agent_binding(
            &mut transaction,
            request.organization_id,
            profile_id,
            request.task_id,
            run.id,
            request.agent.as_ref(),
            inherited_agent_snapshot_id,
        )
        .await?;
        transaction.commit().await?;
        Ok(run)
    }

    pub async fn cancel_run(
        &self,
        request: CancelRunRequest,
    ) -> Result<RunRecord, RunOrchestratorError> {
        let mut transaction = self.db.begin().await?;
        let row = sqlx::query(
            "SELECT run.id, run.task_id, run.status, run.codex_thread_id, run.active_turn_id, \
                    run.workspace_id, run.attempt, run.created_at, run.updated_at, \
                    run.requested_by, workspace.root_path \
             FROM runs run \
             LEFT JOIN workspaces workspace ON workspace.id = run.workspace_id \
             WHERE run.id = $1 AND run.organization_id = $2 FOR UPDATE OF run",
        )
        .bind(request.run_id)
        .bind(request.organization_id)
        .fetch_optional(&mut *transaction)
        .await?
        .ok_or(RunOrchestratorError::NotFound)?;
        let requested_by: Option<Uuid> = row.get("requested_by");
        if requested_by != Some(request.actor_id) && !request.allow_organization_admin {
            return Err(RunOrchestratorError::NotFound);
        }
        let status: String = row.get("status");
        if matches!(status.as_str(), "completed" | "cancelled" | "failed") {
            return Err(RunOrchestratorError::Conflict(
                "Run is already in a terminal state".to_string(),
            ));
        }
        sqlx::query(
            "UPDATE runs SET status = 'cancelled', active_turn_id = NULL, failure_code = NULL, \
                             lease_owner = NULL, lease_token = NULL, lease_expires_at = NULL, \
                             updated_at = now() WHERE id = $1",
        )
        .bind(request.run_id)
        .execute(&mut *transaction)
        .await?;
        sqlx::query(
            "UPDATE supervisor_policy_bindings \
             SET state = 'cancelled', failure_code = 'run_cancelled', updated_at = now() \
             WHERE run_id = $1 AND state = 'prepared'",
        )
        .bind(request.run_id)
        .execute(&mut *transaction)
        .await?;
        sqlx::query(
            "UPDATE agent_run_bindings \
             SET state = 'cancelled', failure_code = 'run_cancelled', updated_at = now() \
             WHERE run_id = $1 AND state = 'prepared'",
        )
        .bind(request.run_id)
        .execute(&mut *transaction)
        .await?;
        sqlx::query("UPDATE tasks SET status = 'cancelled', updated_at = now() WHERE id = $1")
            .bind(row.get::<Uuid, _>("task_id"))
            .execute(&mut *transaction)
            .await?;
        transaction.commit().await?;

        let thread_id: Option<String> = row.get("codex_thread_id");
        let turn_id: Option<String> = row.get("active_turn_id");
        let workspace_id: Option<Uuid> = row.get("workspace_id");
        let root_path: Option<String> = row.get("root_path");
        if let (Some(thread_id), Some(turn_id), Some(workspace_id), Some(root_path)) =
            (thread_id, turn_id, workspace_id, root_path)
        {
            let workspace = open_web_codex_adapter::AuthorizedWorkspace {
                id: workspace_id.to_string(),
                root: root_path.into(),
            };
            if let Err(error) = self
                .adapter
                .interrupt_turn(&workspace, &thread_id, &turn_id)
                .await
            {
                sqlx::query(
                    "UPDATE runs SET status = 'recovery_pending', failure_code = 'interrupt_failed', \
                                     updated_at = now() WHERE id = $1 AND status = 'cancelled'",
                )
                .bind(request.run_id)
                .execute(&self.db)
                .await?;
                return Err(error.into());
            }
        }
        self.get_run(request.organization_id, request.run_id).await
    }

    pub async fn get_run(
        &self,
        organization_id: Uuid,
        run_id: Uuid,
    ) -> Result<RunRecord, RunOrchestratorError> {
        let row = sqlx::query(
            "SELECT id, task_id, status, failure_code, codex_thread_id, active_turn_id, workspace_id, \
                    attempt, created_at, updated_at \
             FROM runs WHERE id = $1 AND organization_id = $2",
        )
        .bind(run_id)
        .bind(organization_id)
        .fetch_optional(&self.db)
        .await?
        .ok_or(RunOrchestratorError::NotFound)?;
        Ok(run_record(&row))
    }

    pub async fn recover_run(
        &self,
        request: RecoverRunRequest,
    ) -> Result<RunRecord, RunOrchestratorError> {
        let mut transaction = self.db.begin().await?;
        let row = sqlx::query(
            "SELECT run.status, run.requested_by, run.workspace_id, run.codex_thread_id, \
                    workspace.state, profile.runtime_key \
             FROM runs run \
             JOIN workspaces workspace ON workspace.id = run.workspace_id \
               AND workspace.organization_id = run.organization_id \
             JOIN profiles profile ON profile.id = run.requested_profile_id \
               AND profile.id = workspace.profile_id \
             JOIN workspace_grants workspace_grant ON workspace_grant.workspace_id = workspace.id \
               AND workspace_grant.organization_id = workspace.organization_id \
               AND workspace_grant.user_id = run.requested_by AND workspace_grant.profile_id = profile.id \
               AND workspace_grant.role IN ('owner', 'write') \
             WHERE run.id = $1 AND run.organization_id = $2 \
             FOR UPDATE OF run",
        )
        .bind(request.run_id)
        .bind(request.organization_id)
        .fetch_optional(&mut *transaction)
        .await?
        .ok_or(RunOrchestratorError::NotFound)?;
        let requested_by: Option<Uuid> = row.get("requested_by");
        if requested_by != Some(request.actor_id) && !request.allow_organization_admin {
            return Err(RunOrchestratorError::NotFound);
        }
        if row.get::<String, _>("runtime_key") != self.runtime_key
            || !matches!(row.get::<String, _>("state").as_str(), "ready" | "retained")
            || row.get::<Option<Uuid>, _>("workspace_id").is_none()
            || row.get::<Option<String>, _>("codex_thread_id").is_none()
        {
            return Err(RunOrchestratorError::NotFound);
        }
        let status: String = row.get("status");
        if status == "running" {
            transaction.commit().await?;
            return self.get_run(request.organization_id, request.run_id).await;
        }
        if status != "recovery_pending" {
            return Err(RunOrchestratorError::Conflict(format!(
                "Run cannot recover from status '{status}'"
            )));
        }

        let token = Uuid::now_v7().to_string();
        let expires_at = Utc::now() + chrono_ttl(self.lease_ttl)?;
        let task_id: Uuid = sqlx::query_scalar(
            "UPDATE runs SET status = 'running', active_turn_id = NULL, failure_code = NULL, \
                             lease_owner = $1, lease_token = $2, lease_expires_at = $3, \
                             heartbeat_at = now(), updated_at = now() \
             WHERE id = $4 AND status = 'recovery_pending' RETURNING task_id",
        )
        .bind(&self.worker_id)
        .bind(&token)
        .bind(expires_at)
        .bind(request.run_id)
        .fetch_one(&mut *transaction)
        .await?;
        sqlx::query("UPDATE tasks SET status = 'running', updated_at = now() WHERE id = $1")
            .bind(task_id)
            .execute(&mut *transaction)
            .await?;
        transaction.commit().await?;
        self.get_run(request.organization_id, request.run_id).await
    }

    pub async fn claim_next(&self) -> Result<Option<RunLease>, RunOrchestratorError> {
        let mut transaction = self.db.begin().await?;
        let candidate = sqlx::query(
            "SELECT run.id, run.organization_id, run.requested_by, profile.id AS profile_id, \
                    run.workspace_id, \
                    run.fork_thread_id, run.fork_source_run_id, workspace.root_path, \
                    binding.id AS supervisor_policy_binding_id, \
                    snapshot.policy_id AS supervisor_policy_id, \
                    snapshot.version AS supervisor_policy_version, \
                    snapshot.content_sha256 AS supervisor_policy_content_sha256, \
                    snapshot.developer_instructions AS supervisor_policy_developer_instructions, \
                    snapshot.source AS supervisor_policy_source, \
                    snapshot.release_id AS supervisor_policy_release_id, \
                    agent_binding.id AS agent_run_binding_id, \
                    agent_snapshot.definition_id AS agent_definition_id, \
                    agent_snapshot.version AS agent_version, \
                    agent_snapshot.content_sha256 AS agent_content_sha256, \
                    agent_snapshot.source AS agent_source, \
                    agent_snapshot.release_id AS agent_release_id \
             FROM runs run \
             JOIN tasks task ON task.id = run.task_id \
               AND task.organization_id = run.organization_id \
             JOIN profiles profile ON profile.id = run.requested_profile_id \
               AND profile.runtime_key = $1 AND profile.status = 'active' \
             JOIN workspaces workspace ON workspace.id = run.workspace_id \
               AND workspace.organization_id = run.organization_id \
               AND workspace.project_id = task.project_id \
               AND workspace.profile_id = profile.id \
               AND workspace.state IN ('ready', 'retained') \
             JOIN workspace_grants workspace_grant ON workspace_grant.workspace_id = workspace.id \
               AND workspace_grant.organization_id = workspace.organization_id \
               AND workspace_grant.user_id = run.requested_by AND workspace_grant.profile_id = profile.id \
               AND workspace_grant.role IN ('owner', 'write') \
             LEFT JOIN supervisor_policy_bindings binding ON binding.run_id = run.id \
             LEFT JOIN supervisor_policy_snapshots snapshot ON snapshot.id = binding.snapshot_id \
             LEFT JOIN agent_run_bindings agent_binding ON agent_binding.run_id = run.id \
             LEFT JOIN agent_run_snapshots agent_snapshot \
               ON agent_snapshot.id = agent_binding.snapshot_id \
              AND agent_snapshot.organization_id = agent_binding.organization_id \
             WHERE run.status = 'pending' \
               AND (run.lease_expires_at IS NULL OR run.lease_expires_at < now()) \
               AND (binding.id IS NULL OR binding.state = 'prepared') \
               AND (agent_binding.id IS NULL OR agent_binding.state = 'prepared') \
             ORDER BY run.created_at, run.id \
             FOR UPDATE OF run SKIP LOCKED LIMIT 1",
        )
        .bind(&self.runtime_key)
        .fetch_optional(&mut *transaction)
        .await?;
        let Some(candidate) = candidate else {
            transaction.commit().await?;
            return Ok(None);
        };
        let supervisor_policy = supervisor_policy_lease(&candidate)?;
        let agent = agent_run_lease(&candidate)?;
        if supervisor_policy.is_some() && agent.is_some() {
            return Err(RunOrchestratorError::Conflict(
                "Run has conflicting root execution bindings".to_string(),
            ));
        }
        let run_id: Uuid = candidate.get("id");
        let token = Uuid::now_v7().to_string();
        let expires_at = Utc::now() + chrono_ttl(self.lease_ttl)?;
        sqlx::query(
            "UPDATE runs SET status = 'provisioning', lease_owner = $1, lease_token = $2, \
                             lease_expires_at = $3, heartbeat_at = now(), attempt = attempt + 1, \
                             failure_code = NULL, updated_at = now() \
             WHERE id = $4",
        )
        .bind(&self.worker_id)
        .bind(&token)
        .bind(expires_at)
        .bind(run_id)
        .execute(&mut *transaction)
        .await?;
        transaction.commit().await?;
        Ok(Some(RunLease {
            run_id,
            organization_id: candidate.get("organization_id"),
            actor_id: candidate.get("requested_by"),
            profile_id: candidate.get("profile_id"),
            workspace_id: candidate.get("workspace_id"),
            workspace_root: candidate.get::<String, _>("root_path").into(),
            fork_thread_id: candidate.get("fork_thread_id"),
            fork_source_run_id: candidate.get("fork_source_run_id"),
            supervisor_policy,
            agent,
            token,
        }))
    }

    pub async fn heartbeat(&self, lease: &RunLease) -> Result<(), RunOrchestratorError> {
        let expires_at = Utc::now() + chrono_ttl(self.lease_ttl)?;
        let updated = sqlx::query(
            "UPDATE runs SET heartbeat_at = now(), lease_expires_at = $1, updated_at = now() \
             WHERE id = $2 AND lease_owner = $3 AND lease_token = $4 \
               AND status IN ('provisioning', 'running', 'cancelling')",
        )
        .bind(expires_at)
        .bind(lease.run_id)
        .bind(&self.worker_id)
        .bind(&lease.token)
        .execute(&self.db)
        .await?
        .rows_affected();
        if updated != 1 {
            return Err(RunOrchestratorError::LeaseLost);
        }
        Ok(())
    }

    pub async fn heartbeat_owned_runs(&self) -> Result<u64, RunOrchestratorError> {
        let expires_at = Utc::now() + chrono_ttl(self.lease_ttl)?;
        Ok(sqlx::query(
            "UPDATE runs SET heartbeat_at = now(), lease_expires_at = $1, updated_at = now() \
             WHERE lease_owner = $2 AND lease_token IS NOT NULL \
               AND status IN ('provisioning', 'running', 'cancelling')",
        )
        .bind(expires_at)
        .bind(&self.worker_id)
        .execute(&self.db)
        .await?
        .rows_affected())
    }

    pub async fn reap_expired(&self) -> Result<u64, RunOrchestratorError> {
        Ok(sqlx::query(
            "UPDATE runs SET status = 'recovery_pending', failure_code = 'lease_expired', \
                             lease_owner = NULL, lease_token = NULL, lease_expires_at = NULL, \
                             updated_at = now() \
             WHERE status IN ('provisioning', 'running', 'cancelling') \
               AND lease_expires_at < now()",
        )
        .execute(&self.db)
        .await?
        .rows_affected())
    }
}

fn supervisor_policy_lease(
    candidate: &sqlx::postgres::PgRow,
) -> Result<Option<SupervisorPolicyLease>, RunOrchestratorError> {
    match (
        candidate.get::<Option<Uuid>, _>("supervisor_policy_binding_id"),
        candidate.get::<Option<String>, _>("supervisor_policy_id"),
        candidate.get::<Option<String>, _>("supervisor_policy_version"),
        candidate.get::<Option<String>, _>("supervisor_policy_content_sha256"),
        candidate.get::<Option<String>, _>("supervisor_policy_developer_instructions"),
        candidate.get::<Option<String>, _>("supervisor_policy_source"),
        candidate.get::<Option<Uuid>, _>("supervisor_policy_release_id"),
    ) {
        (None, None, None, None, None, None, None) => Ok(None),
        (
            Some(binding_id),
            Some(policy_id),
            Some(version),
            Some(content_sha256),
            Some(developer_instructions),
            Some(source),
            release_id,
        ) => {
            let source = source.parse().map_err(|_| {
                RunOrchestratorError::Conflict(
                    "Supervisor Policy binding has an invalid snapshot source".to_string(),
                )
            })?;
            Ok(Some(SupervisorPolicyLease {
                binding_id,
                policy_id,
                version,
                content_sha256,
                developer_instructions,
                source,
                release_id,
            }))
        }
        _ => Err(RunOrchestratorError::Conflict(
            "Supervisor Policy binding is missing immutable snapshot fields".to_string(),
        )),
    }
}

fn agent_run_lease(
    candidate: &sqlx::postgres::PgRow,
) -> Result<Option<AgentRunLease>, RunOrchestratorError> {
    match (
        candidate.get::<Option<Uuid>, _>("agent_run_binding_id"),
        candidate.get::<Option<String>, _>("agent_definition_id"),
        candidate.get::<Option<String>, _>("agent_version"),
        candidate.get::<Option<String>, _>("agent_content_sha256"),
        candidate.get::<Option<String>, _>("agent_source"),
        candidate.get::<Option<Uuid>, _>("agent_release_id"),
    ) {
        (None, None, None, None, None, None) => Ok(None),
        (
            Some(binding_id),
            Some(definition_id),
            Some(version),
            Some(content_sha256),
            Some(source),
            release_id,
        ) => {
            let source = source.parse().map_err(|_| {
                RunOrchestratorError::Conflict(
                    "root Agent binding has an invalid snapshot source".to_string(),
                )
            })?;
            Ok(Some(AgentRunLease {
                binding_id,
                definition_id,
                version,
                content_sha256,
                source,
                release_id,
            }))
        }
        _ => Err(RunOrchestratorError::Conflict(
            "root Agent binding is missing immutable snapshot fields".to_string(),
        )),
    }
}

pub(crate) fn run_record(row: &sqlx::postgres::PgRow) -> RunRecord {
    RunRecord {
        id: row.get("id"),
        task_id: row.get("task_id"),
        status: row.get("status"),
        failure_code: row.get("failure_code"),
        codex_thread_id: row.get("codex_thread_id"),
        active_turn_id: row.get("active_turn_id"),
        workspace_id: row.get("workspace_id"),
        attempt: row.get("attempt"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    }
}
